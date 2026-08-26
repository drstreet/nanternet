use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};

use crate::checkhost::CheckHost;
use crate::config::{Settings, Store, Target};
use crate::error::Result;
use crate::status::{Level, TargetStatus};

const USER_AGENT: &str = concat!("IranNANternet/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Default, Clone)]
pub struct Tracked {
    pub pending_level: Level,
    pub streak: u32,
    pub announced_level: Level,
    pub announced_codes: Vec<String>,
}

pub struct AppState {
    path: PathBuf,
    store: Mutex<Store>,
    statuses: Mutex<HashMap<String, TargetStatus>>,
    tracking: Mutex<HashMap<String, Tracked>>,
    pub local_http: reqwest::Client,
    pub external_http: reqwest::Client,
    pub checkhost: CheckHost,
    pub wake: Notify,
}

impl AppState {
    pub fn new(path: PathBuf) -> Result<Self> {
        let store = Store::load(&path)?;

        let local_http = reqwest::Client::builder()
            .no_proxy()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        let external_http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            path,
            store: Mutex::new(store),
            statuses: Mutex::new(HashMap::new()),
            tracking: Mutex::new(HashMap::new()),
            checkhost: CheckHost::new(external_http.clone()),
            local_http,
            external_http,
            wake: Notify::new(),
        })
    }

    pub async fn targets(&self) -> Vec<Target> {
        self.store.lock().await.targets.clone()
    }

    pub async fn target(&self, id: &str) -> Option<Target> {
        self.store.lock().await.find(id).cloned()
    }

    pub async fn settings(&self) -> Settings {
        self.store.lock().await.settings.clone()
    }

    pub async fn mutate<T>(&self, change: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let (outcome, snapshot) = {
            let mut store = self.store.lock().await;
            let outcome = change(&mut store)?;
            (outcome, store.clone())
        };
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || snapshot.save(&path)).await??;
        self.wake.notify_one();
        Ok(outcome)
    }

    pub async fn record(&self, status: TargetStatus) {
        self.statuses
            .lock()
            .await
            .insert(status.target_id.clone(), status);
    }

    pub async fn snapshot(&self) -> Vec<TargetStatus> {
        self.statuses.lock().await.values().cloned().collect()
    }

    pub async fn status_of(&self, id: &str) -> Option<TargetStatus> {
        self.statuses.lock().await.get(id).cloned()
    }

    pub async fn forget(&self, id: &str) {
        self.statuses.lock().await.remove(id);
        self.tracking.lock().await.remove(id);
    }

    pub async fn retain_known(&self, ids: &[String]) {
        let keep = |key: &String| ids.iter().any(|id| id == key);
        self.statuses.lock().await.retain(|key, _| keep(key));
        self.tracking.lock().await.retain(|key, _| keep(key));
    }

    pub async fn announce(&self, status: &TargetStatus, confirmations: u32) -> bool {
        let mut tracking = self.tracking.lock().await;
        let entry = tracking.entry(status.target_id.clone()).or_default();

        if entry.pending_level == status.level {
            entry.streak = entry.streak.saturating_add(1);
        } else {
            entry.pending_level = status.level;
            entry.streak = 1;
        }

        let settled = status.level == Level::Ok || entry.streak >= confirmations.max(1);
        if !settled {
            return false;
        }

        let codes = status.codes();
        if entry.announced_level == status.level && entry.announced_codes == codes {
            return false;
        }

        let first_observation = entry.announced_level == Level::Unknown;
        entry.announced_level = status.level;
        entry.announced_codes = codes;

        !(first_observation && status.level == Level::Ok)
    }
}
