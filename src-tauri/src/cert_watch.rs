use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use crate::config::DomainSpec;
use crate::error::{Error, Result};
use crate::status::{CertificateInfo, DomainReport, Finding, Level, RegistrationInfo};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const RDAP_GATEWAY: &str = "https://rdap.org/domain";
const IRNIC: &str = "IRNIC";

pub async fn inspect(
    http: &reqwest::Client,
    spec: &DomainSpec,
    known: Option<DomainReport>,
    refresh: bool,
) -> (DomainReport, Vec<Finding>) {
    let mut report = DomainReport::default();
    let mut findings = Vec::new();

    if let Err(err) = validate(&spec.domain) {
        return (
            report,
            vec![Finding::critical("domain.invalid", err.to_string())],
        );
    }

    if spec.check_certificate {
        match certificate(&spec.domain, spec.port).await {
            Ok(info) => {
                findings.push(expiry_finding(
                    "certificate",
                    info.days_left,
                    spec.warn_days,
                    format!("issued by {}", info.issuer),
                ));
                report.certificate = Some(info);
            }
            Err(err) => findings.push(Finding::critical(
                "certificate.unavailable",
                err.to_string(),
            )),
        }
    }

    if spec.check_registration {
        match unpublished_registry(&spec.domain) {
            Some(reason) => findings.push(Finding::ok("domain.registrationUnpublished", reason)),
            None => {
                if refresh {
                    match registration(http, &spec.domain).await {
                        Ok(info) => report.registration = Some(info),
                        Err(err) => report.registration_error = Some(err.to_string()),
                    }
                } else if let Some(carried) = known {
                    report.registration = carried.registration.map(RegistrationInfo::aged);
                    report.registration_error = carried.registration_error;
                }

                if let Some(info) = &report.registration {
                    findings.push(expiry_finding(
                        "domain",
                        info.days_left,
                        spec.warn_days,
                        format!("according to {}", info.source),
                    ));
                } else if let Some(err) = &report.registration_error {
                    findings.push(Finding::warn("domain.lookupFailed", err.clone()));
                }
            }
        }
    }

    if findings.is_empty() {
        findings.push(Finding::ok("domain.healthy", "nothing to check"));
    }

    (report, findings)
}

fn expiry_finding(subject: &str, days_left: i64, warn_days: i64, context: String) -> Finding {
    if days_left <= 0 {
        Finding::critical(
            &format!("{subject}.expired"),
            format!("expired {} day(s) ago, {context}", days_left.abs()),
        )
    } else if days_left <= warn_days {
        Finding::new(
            &format!("{subject}.expiring"),
            if days_left <= warn_days / 3 {
                Level::Critical
            } else {
                Level::Warn
            },
            format!("{days_left} day(s) left, {context}"),
        )
    } else {
        Finding::ok(
            &format!("{subject}.valid"),
            format!("{days_left} day(s) left, {context}"),
        )
    }
}

pub async fn certificate(host: &str, port: u16) -> Result<CertificateInfo> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let name = ServerName::try_from(host.to_owned())
        .map_err(|_| Error::msg(format!("{host} is not a valid TLS server name")))?;

    let tcp = timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| Error::msg(format!("no TCP connection to {host}:{port}")))??;
    let stream = timeout(
        HANDSHAKE_TIMEOUT,
        TlsConnector::from(Arc::new(config)).connect(name, tcp),
    )
    .await
    .map_err(|_| Error::msg(format!("{host} did not finish the TLS handshake")))??;

    let (_, connection) = stream.get_ref();
    let leaf = connection
        .peer_certificates()
        .and_then(|chain| chain.first())
        .ok_or_else(|| Error::msg("the server presented no certificate"))?;

    let (_, parsed) = x509_parser::parse_x509_certificate(leaf.as_ref())
        .map_err(|err| Error::msg(format!("unreadable certificate: {err}")))?;

    let not_after = parsed.validity().not_after.timestamp();
    Ok(CertificateInfo {
        subject: common_name(parsed.subject(), host),
        issuer: common_name(parsed.issuer(), "unknown issuer"),
        not_after,
        days_left: days_until(not_after),
    })
}

pub fn unpublished_registry(domain: &str) -> Option<String> {
    registrable_domain(domain)
        .ends_with(".ir")
        .then(|| format!("{IRNIC} filters expiry dates out of its answers"))
}

pub async fn registration(http: &reqwest::Client, domain: &str) -> Result<RegistrationInfo> {
    let registrable = registrable_domain(domain);
    let expires_at = rdap_expiry(http, &registrable).await?;
    Ok(RegistrationInfo {
        source: "RDAP".to_owned(),
        expires_at,
        days_left: days_until(expires_at),
    })
}

async fn rdap_expiry(http: &reqwest::Client, domain: &str) -> Result<i64> {
    let response = http
        .get(format!("{RDAP_GATEWAY}/{domain}"))
        .header(reqwest::header::ACCEPT, "application/rdap+json")
        .send()
        .await?
        .error_for_status()?
        .json::<RdapDomain>()
        .await?;

    response
        .events
        .iter()
        .find(|event| event.action.eq_ignore_ascii_case("expiration"))
        .and_then(|event| parse_moment(&event.date))
        .ok_or_else(|| Error::msg("the registry did not publish an expiration date"))
}

fn parse_moment(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if let Ok(moment) = OffsetDateTime::parse(raw, &Rfc3339) {
        return Some(moment.unix_timestamp());
    }
    let calendar_day = format_description!("[year]-[month]-[day]");
    Date::parse(raw.split_whitespace().next()?, calendar_day)
        .ok()
        .map(|date| date.midnight().assume_utc().unix_timestamp())
}

fn days_until(moment: i64) -> i64 {
    (moment - crate::unix_now()).div_euclid(86_400)
}

fn common_name(name: &x509_parser::x509::X509Name<'_>, fallback: &str) -> String {
    name.iter_common_name()
        .next()
        .and_then(|entry| entry.as_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

fn validate(domain: &str) -> Result<()> {
    if domain.is_empty() || domain.len() > 253 {
        return Err(Error::msg("a domain name is required"));
    }
    let acceptable = domain
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'));
    if !acceptable || domain.starts_with('.') || domain.ends_with('.') {
        return Err(Error::msg(format!("{domain:?} is not a plain domain name")));
    }
    Ok(())
}

fn registrable_domain(domain: &str) -> String {
    let labels: Vec<&str> = domain.split('.').collect();
    let keep = match labels.as_slice() {
        [.., second, last] if second.len() <= 3 && last.len() <= 3 && labels.len() > 2 => 3,
        _ => 2,
    };
    if labels.len() <= keep {
        return domain.to_owned();
    }
    labels[labels.len() - keep..].join(".")
}

#[derive(Deserialize)]
struct RdapDomain {
    #[serde(default)]
    events: Vec<RdapEvent>,
}

#[derive(Deserialize)]
struct RdapEvent {
    #[serde(rename = "eventAction")]
    action: String,
    #[serde(rename = "eventDate")]
    date: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_dates_are_understood_in_both_shapes() {
        let iso = parse_moment("2026-12-22T00:00:00Z").unwrap();
        let plain = parse_moment("2026-12-22").unwrap();
        assert_eq!(iso, plain);
    }

    #[test]
    fn padded_registry_dates_still_parse() {
        assert!(parse_moment("  2027-01-05  ").is_some());
    }

    #[test]
    fn iranian_domains_are_reported_as_unpublished_rather_than_broken() {
        assert!(unpublished_registry("panel.softmac.ir").is_some());
        assert!(unpublished_registry("example.com").is_none());
    }

    #[test]
    fn multi_label_suffixes_are_kept_together() {
        assert_eq!(registrable_domain("shop.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("www.example.com"), "example.com");
        assert_eq!(registrable_domain("softmac.ir"), "softmac.ir");
        assert_eq!(registrable_domain("panel.softmac.ir"), "softmac.ir");
    }

    #[test]
    fn injection_attempts_are_refused_before_any_socket_opens() {
        assert!(validate("example.com\r\nHELP").is_err());
        assert!(validate("exa mple.com").is_err());
        assert!(validate("example.com").is_ok());
    }

    #[test]
    fn expiry_severity_climbs_as_the_deadline_approaches() {
        assert_eq!(
            expiry_finding("certificate", 40, 14, String::new()).level,
            Level::Ok
        );
        assert_eq!(
            expiry_finding("certificate", 12, 14, String::new()).level,
            Level::Warn
        );
        assert_eq!(
            expiry_finding("certificate", 3, 14, String::new()).level,
            Level::Critical
        );
        assert_eq!(
            expiry_finding("certificate", -1, 14, String::new()).code,
            "certificate.expired"
        );
    }
}
