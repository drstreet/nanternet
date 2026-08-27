use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Unknown,
    Ok,
    Warn,
    Critical,
}

impl Level {
    pub fn worst<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Self {
        findings
            .into_iter()
            .map(|finding| finding.level)
            .max()
            .unwrap_or(Level::Unknown)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub level: Level,
    pub detail: String,
}

impl Finding {
    pub fn new(code: &str, level: Level, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            level,
            detail: detail.into(),
        }
    }

    pub fn ok(code: &str, detail: impl Into<String>) -> Self {
        Self::new(code, Level::Ok, detail)
    }

    pub fn warn(code: &str, detail: impl Into<String>) -> Self {
        Self::new(code, Level::Warn, detail)
    }

    pub fn critical(code: &str, detail: impl Into<String>) -> Self {
        Self::new(code, Level::Critical, detail)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetStatus {
    pub target_id: String,
    pub level: Level,
    pub checked_at: i64,
    pub duration_ms: u64,
    pub findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<WebsiteReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<DomainReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsReport>,
}

impl TargetStatus {
    pub fn new(target_id: &str) -> Self {
        Self {
            target_id: target_id.to_owned(),
            level: Level::Unknown,
            checked_at: crate::unix_now(),
            duration_ms: 0,
            findings: Vec::new(),
            website: None,
            server: None,
            domain: None,
            dns: None,
        }
    }

    pub fn seal(mut self, findings: Vec<Finding>, started: std::time::Instant) -> Self {
        self.level = Level::worst(&findings);
        self.findings = findings;
        self.duration_ms = started.elapsed().as_millis() as u64;
        self.checked_at = crate::unix_now();
        self
    }

    pub fn codes(&self) -> Vec<String> {
        self.findings.iter().map(|f| f.code.clone()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProbe {
    pub reachable: bool,
    pub status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub content_match: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeResult {
    pub node: String,
    pub country: String,
    pub city: String,
    pub asn: String,
    pub reachable: Option<bool>,
    pub latency_ms: Option<u64>,
    pub status: Option<String>,
    pub message: Option<String>,
    pub address: Option<String>,
}

impl NodeResult {
    pub fn inside_iran(&self) -> bool {
        self.country.eq_ignore_ascii_case("ir")
    }

    pub fn label(&self) -> String {
        if self.city.is_empty() {
            self.country.to_uppercase()
        } else {
            format!("{} ({})", self.city, self.country.to_uppercase())
        }
    }

    pub fn region(&self) -> &'static str {
        if self.inside_iran() {
            "IR"
        } else {
            continent(&self.country)
        }
    }
}

pub fn continent(country: &str) -> &'static str {
    match country.to_ascii_lowercase().as_str() {
        "al" | "at" | "ba" | "be" | "bg" | "by" | "ch" | "cy" | "cz" | "de" | "dk" | "ee"
        | "es" | "fi" | "fr" | "gb" | "gr" | "hr" | "hu" | "ie" | "is" | "it" | "lt" | "lu"
        | "lv" | "md" | "me" | "mk" | "mt" | "nl" | "no" | "pl" | "pt" | "ro" | "rs" | "ru"
        | "se" | "si" | "sk" | "ua" => "EU",
        "ae" | "am" | "az" | "bd" | "bh" | "cn" | "ge" | "hk" | "id" | "il" | "in" | "iq"
        | "ir" | "jo" | "jp" | "kr" | "kw" | "kz" | "lb" | "lk" | "my" | "np" | "om" | "ph"
        | "pk" | "qa" | "sa" | "sg" | "th" | "tr" | "tw" | "uz" | "vn" => "AS",
        "ca" | "cr" | "do" | "gt" | "mx" | "pa" | "pr" | "us" => "NA",
        "ar" | "bo" | "br" | "cl" | "co" | "ec" | "pe" | "py" | "uy" | "ve" => "SA",
        "dz" | "eg" | "et" | "gh" | "ke" | "ma" | "mu" | "ng" | "tn" | "za" => "AF",
        "au" | "nz" => "OC",
        _ => "??",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalProbe {
    pub checked_at: i64,
    pub permanent_link: Option<String>,
    pub nodes: Vec<NodeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteReport {
    pub local: LocalProbe,
    pub external: Option<ExternalProbe>,
    pub external_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsAttempt {
    pub label: String,
    pub resolver: String,
    pub builtin: bool,
    pub answers: Vec<String>,
    pub reserved: bool,
    pub error: Option<String>,
    pub probe: Option<LocalProbe>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsReport {
    pub checked_at: i64,
    pub attempts: Vec<DnsAttempt>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub total: u64,
    pub used: u64,
    pub percent: f64,
}

impl Usage {
    pub fn new(total: u64, used: u64) -> Self {
        Self {
            total,
            used,
            percent: if total == 0 {
                0.0
            } else {
                used as f64 * 100.0 / total as f64
            },
        }
    }

    pub fn from_available(total: u64, available: u64) -> Self {
        Self::new(total, total.saturating_sub(available))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    #[serde(flatten)]
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerState {
    pub name: String,
    pub state: String,
    pub status: String,
    pub health: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerReport {
    pub host_fingerprint: Option<String>,
    pub load_average: Option<f64>,
    pub cores: Option<u32>,
    pub load_per_core: Option<f64>,
    pub uptime_secs: Option<u64>,
    pub memory: Option<Usage>,
    pub disks: Vec<DiskUsage>,
    pub containers: Vec<ContainerState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub not_after: i64,
    pub days_left: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationInfo {
    pub source: String,
    pub expires_at: i64,
    pub days_left: i64,
}

impl RegistrationInfo {
    pub fn aged(self) -> Self {
        Self {
            days_left: (self.expires_at - crate::unix_now()).div_euclid(86_400),
            ..self
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainReport {
    pub certificate: Option<CertificateInfo>,
    pub registration: Option<RegistrationInfo>,
    pub registration_error: Option<String>,
}
