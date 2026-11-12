use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::status::{Level, TargetStatus};

pub const RETENTION_SECS: i64 = 30 * 86_400;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub checked_at: i64,
    pub level: Level,
    pub duration_ms: u64,
}

impl From<&TargetStatus> for Sample {
    fn from(status: &TargetStatus) -> Self {
        Self {
            checked_at: status.checked_at,
            level: status.level,
            duration_ms: status.duration_ms,
        }
    }
}

fn file(dir: &Path, id: &str) -> Option<PathBuf> {
    let safe = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| dir.join(format!("{id}.jsonl")))
}

pub fn append(dir: &Path, id: &str, sample: &Sample) -> Result<()> {
    let Some(path) = file(dir, id) else {
        return Ok(());
    };
    let mut line = serde_json::to_vec(sample)?;
    line.push(b'\n');

    let _guard = lock();
    fs::create_dir_all(dir)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)?;
    Ok(())
}

pub fn read(dir: &Path, id: &str, since: i64) -> Result<Vec<Sample>> {
    let Some(path) = file(dir, id) else {
        return Ok(Vec::new());
    };
    let handle = match fs::File::open(path) {
        Ok(handle) => handle,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    Ok(BufReader::new(handle)
        .lines()
        .map_while(std::result::Result::ok)
        .filter_map(|line| serde_json::from_str::<Sample>(&line).ok())
        .filter(|sample| sample.checked_at >= since)
        .collect())
}

pub fn remove(dir: &Path, id: &str) {
    if let Some(path) = file(dir, id) {
        let _guard = lock();
        let _ = fs::remove_file(path);
    }
}

pub fn prune(dir: &Path, known: &[String], cutoff: i64) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(id) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".jsonl"))
        else {
            continue;
        };
        if !known.iter().any(|known| known == id) {
            remove(dir, id);
            continue;
        }

        let _guard = lock();
        let mut body = Vec::new();
        for sample in read(dir, id, cutoff)? {
            serde_json::to_writer(&mut body, &sample)?;
            body.push(b'\n');
        }
        let staging = path.with_extension("jsonl.tmp");
        fs::write(&staging, &body)?;
        fs::rename(&staging, &path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nanternet-history-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(t: i64, l: Level) -> Sample {
        Sample {
            checked_at: t,
            level: l,
            duration_ms: 10,
        }
    }

    #[test]
    fn appends_reads_and_filters_by_time() {
        let dir = scratch("read");
        append(&dir, "a", &sample(100, Level::Ok)).unwrap();
        append(&dir, "a", &sample(200, Level::Critical)).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(dir.join("a.jsonl"))
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        append(&dir, "a", &sample(300, Level::Warn)).unwrap();

        assert_eq!(
            read(&dir, "a", 150).unwrap(),
            vec![sample(200, Level::Critical), sample(300, Level::Warn)]
        );
        assert!(read(&dir, "missing", 0).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_ids_that_escape_the_directory() {
        let dir = scratch("escape");
        append(&dir, "../evil", &sample(1, Level::Ok)).unwrap();
        assert!(!dir.join("../evil.jsonl").exists());
        assert!(read(&dir, "../evil", 0).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prune_drops_old_samples_and_unknown_targets() {
        let dir = scratch("prune");
        append(&dir, "keep", &sample(100, Level::Ok)).unwrap();
        append(&dir, "keep", &sample(500, Level::Ok)).unwrap();
        append(&dir, "gone", &sample(500, Level::Ok)).unwrap();

        prune(&dir, &["keep".to_owned()], 300).unwrap();

        assert_eq!(read(&dir, "keep", 0).unwrap(), vec![sample(500, Level::Ok)]);
        assert!(!dir.join("gone.jsonl").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
