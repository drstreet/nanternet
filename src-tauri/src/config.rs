use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

pub const MIN_INTERVAL: u64 = 15;
pub const MIN_EXTERNAL_INTERVAL: u64 = 300;
pub const MAX_PROBE_NODES: usize = 8;
pub const REGISTRATION_INTERVAL: u64 = 43_200;

fn enabled() -> bool {
    true
}

fn default_interval() -> u64 {
    120
}

fn default_external_interval() -> u64 {
    900
}

fn default_iran_nodes() -> usize {
    3
}

fn default_abroad_nodes() -> usize {
    4
}

fn default_ssh_port() -> u16 {
    22
}

fn default_https_port() -> u16 {
    443
}

fn default_load_limit() -> f64 {
    1.5
}

fn default_usage_limit() -> f64 {
    85.0
}

fn default_mounts() -> Vec<String> {
    vec!["/".to_owned()]
}

fn default_warn_days() -> i64 {
    14
}

fn default_language() -> String {
    "en".to_owned()
}

fn default_confirmations() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteSpec {
    pub url: String,
    #[serde(default = "enabled")]
    pub external_probe: bool,
    #[serde(default = "default_external_interval")]
    pub external_interval_secs: u64,
    /// Resolve through several DNS servers and fetch the site through each
    /// answer. Off by default: it costs one request per resolver.
    #[serde(default)]
    pub dns_probe: bool,
    #[serde(default)]
    pub expected_status: Option<u16>,
    #[serde(default)]
    pub expected_body: Option<String>,
    #[serde(default = "default_iran_nodes")]
    pub iran_nodes: usize,
    #[serde(default = "default_abroad_nodes")]
    pub abroad_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum SshAuth {
    #[serde(rename_all = "camelCase")]
    Key {
        path: String,
    },
    Password,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSpec {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    #[serde(default)]
    pub host_fingerprint: Option<String>,
    #[serde(default = "default_load_limit")]
    pub load_limit: f64,
    #[serde(default = "default_usage_limit")]
    pub memory_limit: f64,
    #[serde(default = "default_usage_limit")]
    pub disk_limit: f64,
    #[serde(default = "default_mounts")]
    pub mounts: Vec<String>,
    #[serde(default)]
    pub containers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSpec {
    pub domain: String,
    #[serde(default = "default_https_port")]
    pub port: u16,
    #[serde(default = "enabled")]
    pub check_certificate: bool,
    #[serde(default = "enabled")]
    pub check_registration: bool,
    #[serde(default = "default_warn_days")]
    pub warn_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetSpec {
    Website(WebsiteSpec),
    Server(ServerSpec),
    Domain(DomainSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    pub spec: TargetSpec,
}

impl Target {
    pub fn normalize(&mut self) {
        if self.id.is_empty() {
            self.id = uuid::Uuid::new_v4().to_string();
        }
        self.name = self.name.trim().to_owned();
        self.interval_secs = self.interval_secs.max(MIN_INTERVAL);
        match &mut self.spec {
            TargetSpec::Website(spec) => {
                spec.url = spec.url.trim().to_owned();
                if !spec.url.contains("://") {
                    spec.url = format!("https://{}", spec.url);
                }
                spec.external_interval_secs = spec
                    .external_interval_secs
                    .max(MIN_EXTERNAL_INTERVAL)
                    .max(self.interval_secs);
                spec.iran_nodes = spec.iran_nodes.min(MAX_PROBE_NODES);
                spec.abroad_nodes = spec.abroad_nodes.min(MAX_PROBE_NODES);
                if let Some(body) = &spec.expected_body {
                    if body.trim().is_empty() {
                        spec.expected_body = None;
                    }
                }
            }
            TargetSpec::Server(spec) => {
                spec.host = spec.host.trim().to_owned();
                spec.username = spec.username.trim().to_owned();
                spec.mounts.retain(|mount| !mount.trim().is_empty());
                spec.containers = spec
                    .containers
                    .iter()
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
                if spec.mounts.is_empty() {
                    spec.mounts = default_mounts();
                }
            }
            TargetSpec::Domain(spec) => {
                spec.domain = spec
                    .domain
                    .trim()
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                spec.warn_days = spec.warn_days.clamp(1, 120);
            }
        }
    }

    pub fn slow_probe_interval(&self) -> Option<u64> {
        match &self.spec {
            TargetSpec::Website(spec) if spec.external_probe || spec.dns_probe => {
                Some(spec.external_interval_secs)
            }
            TargetSpec::Domain(spec) if spec.check_registration => Some(REGISTRATION_INTERVAL),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "enabled")]
    pub desktop_notifications: bool,
    #[serde(default)]
    pub discord_webhook: Option<String>,
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
    /// Proxy for outbound alerts only. Telegram and Discord are blocked in
    /// Iran, so without this the app can watch a site but never report on it.
    /// Site probes deliberately ignore it: routing them through a tunnel would
    /// answer the wrong question.
    #[serde(default)]
    pub proxy_url: Option<String>,
    /// Extra DNS resolver addresses on top of the built-in list.
    #[serde(default)]
    pub dns_resolvers: Vec<String>,
    #[serde(default = "default_confirmations")]
    pub confirmations: u32,
    #[serde(default)]
    pub start_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: default_language(),
            desktop_notifications: true,
            discord_webhook: None,
            telegram_chat_id: None,
            proxy_url: None,
            dns_resolvers: Vec::new(),
            confirmations: default_confirmations(),
            start_at_login: false,
        }
    }
}

impl Settings {
    pub fn normalize(&mut self) {
        if self.language != "fa" {
            self.language = "en".to_owned();
        }
        self.confirmations = self.confirmations.clamp(1, 10);
        blank_to_none(&mut self.discord_webhook);
        blank_to_none(&mut self.telegram_chat_id);
        blank_to_none(&mut self.proxy_url);
        self.dns_resolvers = self
            .dns_resolvers
            .iter()
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect();
        self.dns_resolvers.truncate(crate::dns_probe::MAX_RESOLVERS);
    }
}

fn blank_to_none(value: &mut Option<String>) {
    if let Some(inner) = value {
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            *value = None;
        } else if trimmed.len() != inner.len() {
            *value = Some(trimmed.to_owned());
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default)]
    pub settings: Settings,
}

impl Store {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let staging = staging_path(path);
        std::fs::write(&staging, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&staging, path)?;
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&Target> {
        self.targets.iter().find(|target| target.id == id)
    }
}

fn staging_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".staging");
    path.with_file_name(name)
}
