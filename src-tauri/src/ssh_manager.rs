use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, Disconnect};
use tokio::time::timeout;

use crate::config::{ServerSpec, SshAuth};
use crate::docker_monitor;
use crate::status::{DiskUsage, Finding, Level, ServerReport, Usage};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(25);
const CRITICAL_USAGE: f64 = 97.0;

pub enum ConnectError {
    HostKey { expected: String, observed: String },
    Authentication(String),
    Transport(String),
}

impl ConnectError {
    fn into_finding(self) -> Finding {
        match self {
            ConnectError::HostKey { expected, observed } => Finding::critical(
                "server.hostKeyMismatch",
                format!("expected {expected} but the host presented {observed}"),
            ),
            ConnectError::Authentication(detail) => {
                Finding::critical("server.authenticationFailed", detail)
            }
            ConnectError::Transport(detail) => Finding::critical("server.unreachable", detail),
        }
    }
}

struct Verifier {
    expected: Option<String>,
    observed: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                key.fingerprint(HashAlg::Sha256).to_string()
            }
            PublicKeyOrCertificate::Certificate(certificate) => certificate
                .public_key()
                .fingerprint(HashAlg::Sha256)
                .to_string(),
        };
        let trusted = self
            .expected
            .as_ref()
            .is_none_or(|expected| expected == &fingerprint);
        *self.observed.lock().unwrap() = Some(fingerprint);
        Ok(trusted)
    }
}

pub struct Session {
    handle: Handle<Verifier>,
    pub fingerprint: Option<String>,
    pub learned: bool,
}

impl Session {
    pub async fn open(spec: &ServerSpec, secret: Option<&str>) -> Result<Self, ConnectError> {
        let observed = Arc::new(Mutex::new(None));
        let verifier = Verifier {
            expected: spec.host_fingerprint.clone(),
            observed: observed.clone(),
        };

        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        });

        let connected = timeout(
            CONNECT_TIMEOUT,
            client::connect(config, (spec.host.as_str(), spec.port), verifier),
        )
        .await;

        let fingerprint = observed.lock().unwrap().clone();

        let mut handle = match connected {
            Ok(Ok(handle)) => handle,
            Ok(Err(err)) => {
                if let (Some(expected), Some(observed)) = (&spec.host_fingerprint, &fingerprint) {
                    if expected != observed {
                        return Err(ConnectError::HostKey {
                            expected: expected.clone(),
                            observed: observed.clone(),
                        });
                    }
                }
                return Err(ConnectError::Transport(err.to_string()));
            }
            Err(_) => {
                return Err(ConnectError::Transport(format!(
                    "no SSH banner from {}:{} within {} seconds",
                    spec.host,
                    spec.port,
                    CONNECT_TIMEOUT.as_secs()
                )));
            }
        };

        authenticate(&mut handle, spec, secret).await?;

        Ok(Self {
            handle,
            learned: spec.host_fingerprint.is_none() && fingerprint.is_some(),
            fingerprint,
        })
    }

    pub async fn run(&mut self, command: &str) -> crate::error::Result<String> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let collect = async {
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                    ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                    _ => {}
                }
            }
        };

        timeout(COMMAND_TIMEOUT, collect).await?;

        if stdout.is_empty() && !stderr.is_empty() {
            return Err(crate::error::Error::msg(
                String::from_utf8_lossy(&stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    }

    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }
}

async fn authenticate(
    handle: &mut Handle<Verifier>,
    spec: &ServerSpec,
    secret: Option<&str>,
) -> Result<(), ConnectError> {
    let accepted = match &spec.auth {
        SshAuth::Key { path } => {
            let expanded = expand(path);
            let key = load_secret_key(&expanded, secret).map_err(|err| {
                ConnectError::Authentication(format!("{}: {err}", expanded.display()))
            })?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|err| ConnectError::Transport(err.to_string()))?
                .flatten();
            handle
                .authenticate_publickey(
                    &spec.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(|err| ConnectError::Authentication(err.to_string()))?
        }
        SshAuth::Password => {
            let password = secret.ok_or_else(|| {
                ConnectError::Authentication("no password is stored for this server".to_owned())
            })?;
            handle
                .authenticate_password(&spec.username, password)
                .await
                .map_err(|err| ConnectError::Authentication(err.to_string()))?
        }
    };

    if accepted.success() {
        Ok(())
    } else {
        Err(ConnectError::Authentication(format!(
            "server rejected the credentials for {}",
            spec.username
        )))
    }
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    }
}

pub async fn check(spec: &ServerSpec, secret: Option<&str>) -> (ServerReport, Vec<Finding>) {
    let mut session = match Session::open(spec, secret).await {
        Ok(session) => session,
        Err(err) => return (ServerReport::default(), vec![err.into_finding()]),
    };

    let mut report = ServerReport {
        host_fingerprint: session.fingerprint.clone(),
        ..Default::default()
    };
    let mut findings = Vec::new();

    if session.learned {
        findings.push(Finding::warn(
            "server.hostKeyLearned",
            format!(
                "trusting {} on first use",
                report.host_fingerprint.clone().unwrap_or_default()
            ),
        ));
    }

    let raw = session.run(&probe_script(spec)).await;
    session.close().await;

    let raw = match raw {
        Ok(raw) => raw,
        Err(err) => {
            findings.push(Finding::critical("server.probeFailed", err.to_string()));
            return (report, findings);
        }
    };

    let sections = split_sections(&raw);
    report.load_average = sections.get("load").and_then(|raw| first_number(raw));
    report.cores = sections
        .get("cores")
        .and_then(|raw| first_number(raw))
        .map(|cores| cores.max(1.0) as u32);
    report.uptime_secs = sections
        .get("uptime")
        .and_then(|raw| first_number(raw))
        .map(|seconds| seconds as u64);
    report.memory = sections.get("memory").and_then(|raw| parse_memory(raw));
    report.load_per_core = match (report.load_average, report.cores) {
        (Some(load), Some(cores)) if cores > 0 => Some(load / cores as f64),
        (Some(load), _) => Some(load),
        _ => None,
    };

    let disks = sections
        .get("disk")
        .map(|raw| parse_disks(raw, &spec.mounts))
        .unwrap_or_default();
    for mount in &spec.mounts {
        if !disks.iter().any(|disk| &disk.mount == mount) {
            findings.push(Finding::warn(
                "server.mountMissing",
                format!("{mount} is not mounted on the host"),
            ));
        }
    }
    report.disks = disks;

    if !spec.containers.is_empty() {
        report.containers = sections
            .get("docker")
            .map(|raw| docker_monitor::parse(raw))
            .unwrap_or_default();
        if report.containers.is_empty() {
            findings.push(Finding::critical(
                "docker.unavailable",
                "the docker daemon did not answer over this connection",
            ));
        } else {
            findings.extend(docker_monitor::classify(
                &spec.containers,
                &report.containers,
            ));
        }
    }

    findings.extend(classify(spec, &report));
    if findings.is_empty() {
        findings.push(Finding::ok("server.healthy", summarize(&report)));
    }

    (report, findings)
}

pub fn classify(spec: &ServerSpec, report: &ServerReport) -> Vec<Finding> {
    let mut findings = Vec::new();

    if let Some(load) = report.load_per_core {
        if load > spec.load_limit {
            let level = if load > spec.load_limit * 3.0 {
                Level::Critical
            } else {
                Level::Warn
            };
            findings.push(Finding::new(
                "server.loadHigh",
                level,
                format!(
                    "load {:.2} per core over a limit of {:.2}",
                    load, spec.load_limit
                ),
            ));
        }
    }

    if let Some(memory) = report.memory {
        if memory.percent > spec.memory_limit {
            findings.push(Finding::new(
                "server.memoryHigh",
                escalate(memory.percent),
                format!(
                    "{:.1}% of {} in use",
                    memory.percent,
                    human_bytes(memory.total)
                ),
            ));
        }
    }

    for disk in &report.disks {
        if disk.usage.percent > spec.disk_limit {
            findings.push(Finding::new(
                "server.diskHigh",
                escalate(disk.usage.percent),
                format!(
                    "{} is {:.1}% full, {} free",
                    disk.mount,
                    disk.usage.percent,
                    human_bytes(disk.usage.total.saturating_sub(disk.usage.used))
                ),
            ));
        }
    }

    findings
}

fn escalate(percent: f64) -> Level {
    if percent >= CRITICAL_USAGE {
        Level::Critical
    } else {
        Level::Warn
    }
}

fn summarize(report: &ServerReport) -> String {
    let mut parts = Vec::new();
    if let Some(load) = report.load_per_core {
        parts.push(format!("load {load:.2}/core"));
    }
    if let Some(memory) = report.memory {
        parts.push(format!("memory {:.0}%", memory.percent));
    }
    if let Some(disk) = report.disks.first() {
        parts.push(format!("{} {:.0}%", disk.mount, disk.usage.percent));
    }
    if !report.containers.is_empty() {
        let running = report
            .containers
            .iter()
            .filter(|container| container.state == "running")
            .count();
        parts.push(format!(
            "{running}/{} containers up",
            report.containers.len()
        ));
    }
    if parts.is_empty() {
        "responding to SSH".to_owned()
    } else {
        parts.join(", ")
    }
}

fn probe_script(spec: &ServerSpec) -> String {
    let mut script = String::from(concat!(
        "echo '::load'; cat /proc/loadavg 2>/dev/null\n",
        "echo '::cores'; nproc 2>/dev/null || grep -c '^processor' /proc/cpuinfo 2>/dev/null\n",
        "echo '::uptime'; cat /proc/uptime 2>/dev/null\n",
        "echo '::memory'; cat /proc/meminfo 2>/dev/null\n",
        "echo '::disk'; df -Pk 2>/dev/null\n",
    ));
    if !spec.containers.is_empty() {
        script.push_str("echo '::docker'; docker ps --all --format '");
        script.push_str(docker_monitor::FORMAT);
        script.push_str("' 2>/dev/null\n");
    }
    script.push_str("exit 0\n");
    script
}

fn split_sections(raw: &str) -> HashMap<String, String> {
    let mut sections = HashMap::new();
    let mut current: Option<(String, String)> = None;
    for line in raw.lines() {
        match line.trim().strip_prefix("::") {
            Some(name) => {
                if let Some((key, body)) = current.take() {
                    sections.insert(key, body);
                }
                current = Some((name.to_owned(), String::new()));
            }
            None => {
                if let Some((_, body)) = current.as_mut() {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    if let Some((key, body)) = current {
        sections.insert(key, body);
    }
    sections
}

fn first_number(raw: &str) -> Option<f64> {
    raw.split_whitespace().next()?.parse().ok()
}

fn parse_memory(raw: &str) -> Option<Usage> {
    let mut fields: HashMap<&str, u64> = HashMap::new();
    for line in raw.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if let Some(kilobytes) = value
            .split_whitespace()
            .next()
            .and_then(|number| number.parse::<u64>().ok())
        {
            fields.insert(key.trim(), kilobytes * 1024);
        }
    }

    let total = *fields.get("MemTotal")?;
    let available = match fields.get("MemAvailable") {
        Some(available) => *available,
        None => {
            *fields.get("MemFree")?
                + fields.get("Buffers").copied().unwrap_or(0)
                + fields.get("Cached").copied().unwrap_or(0)
        }
    };
    Some(Usage::from_available(total, available))
}

fn parse_disks(raw: &str, mounts: &[String]) -> Vec<DiskUsage> {
    let mut disks = Vec::new();
    for line in raw.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let mount = fields[5..].join(" ");
        if !mounts.iter().any(|wanted| wanted == &mount) {
            continue;
        }
        let (Ok(used), Ok(available)) = (fields[2].parse::<u64>(), fields[3].parse::<u64>()) else {
            continue;
        };
        disks.push(DiskUsage {
            mount,
            usage: Usage::new((used + available) * 1024, used * 1024),
        });
    }
    disks
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE_OUTPUT: &str = "::load\n\
0.52 0.41 0.38 1/512 90210\n\
::cores\n\
4\n\
::uptime\n\
884512.44 3390451.10\n\
::memory\n\
MemTotal:        8127048 kB\n\
MemFree:          204512 kB\n\
MemAvailable:    1015881 kB\n\
Buffers:           92140 kB\n\
Cached:           980220 kB\n\
::disk\n\
Filesystem     1024-blocks      Used Available Capacity Mounted on\n\
/dev/vda1         51475068  46327560   2500296      95% /\n\
/dev/vdb1        103081248   9200000  88600000      10% /mnt/backup volume\n\
tmpfs              4063524         0   4063524       0% /dev/shm\n";

    #[test]
    fn sections_are_isolated_from_each_other() {
        let sections = split_sections(PROBE_OUTPUT);
        assert_eq!(sections.len(), 5);
        assert!(sections["memory"].contains("MemTotal"));
        assert!(!sections["load"].contains("MemTotal"));
    }

    #[test]
    fn load_is_normalised_per_core() {
        let sections = split_sections(PROBE_OUTPUT);
        let load = first_number(&sections["load"]).unwrap();
        let cores = first_number(&sections["cores"]).unwrap();
        assert!((load / cores - 0.13).abs() < 0.01);
    }

    #[test]
    fn memory_usage_prefers_the_available_figure() {
        let usage = parse_memory(&split_sections(PROBE_OUTPUT)["memory"]).unwrap();
        assert_eq!(usage.total, 8127048 * 1024);
        assert!((usage.percent - 87.5).abs() < 0.2);
    }

    #[test]
    fn memory_falls_back_when_available_is_absent() {
        let usage = parse_memory("MemTotal: 1000 kB\nMemFree: 200 kB\nCached: 300 kB\n").unwrap();
        assert_eq!(usage.used, 500 * 1024);
    }

    #[test]
    fn mount_points_containing_spaces_are_matched_whole() {
        let disks = parse_disks(
            &split_sections(PROBE_OUTPUT)["disk"],
            &["/mnt/backup volume".to_owned()],
        );
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].mount, "/mnt/backup volume");
        assert!((disks[0].usage.percent - 9.4).abs() < 0.2);
    }

    #[test]
    fn unlisted_mounts_are_ignored() {
        let disks = parse_disks(&split_sections(PROBE_OUTPUT)["disk"], &["/".to_owned()]);
        assert_eq!(disks.len(), 1);
        assert!((disks[0].usage.percent - 94.9).abs() < 0.2);
    }

    #[test]
    fn a_nearly_full_disk_escalates_past_a_warning() {
        let spec = ServerSpec {
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth: SshAuth::Password,
            host_fingerprint: None,
            load_limit: 1.5,
            memory_limit: 85.0,
            disk_limit: 85.0,
            mounts: vec!["/".into()],
            containers: Vec::new(),
        };
        let report = ServerReport {
            disks: parse_disks(&split_sections(PROBE_OUTPUT)["disk"], &spec.mounts),
            memory: parse_memory(&split_sections(PROBE_OUTPUT)["memory"]),
            ..Default::default()
        };
        let codes: Vec<String> = classify(&spec, &report)
            .into_iter()
            .map(|finding| finding.code)
            .collect();
        assert_eq!(codes, vec!["server.memoryHigh", "server.diskHigh"]);
    }

    #[test]
    fn the_docker_format_survives_string_assembly() {
        let spec = ServerSpec {
            host: "example.test".into(),
            port: 22,
            username: "root".into(),
            auth: SshAuth::Password,
            host_fingerprint: None,
            load_limit: 1.5,
            memory_limit: 85.0,
            disk_limit: 85.0,
            mounts: vec!["/".into()],
            containers: vec!["nginx".into()],
        };
        assert!(probe_script(&spec).contains("{{.Names}}|{{.State}}|{{.Status}}"));
    }
}
