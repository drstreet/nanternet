use std::time::Instant;

use crate::checkhost::CheckHost;
use crate::config::WebsiteSpec;
use crate::status::{ExternalProbe, Finding, LocalProbe, NodeResult, WebsiteReport};

pub async fn probe_locally(http: &reqwest::Client, spec: &WebsiteSpec) -> LocalProbe {
    let started = Instant::now();
    let response = match http.get(&spec.url).send().await {
        Ok(response) => response,
        Err(err) => {
            return LocalProbe {
                reachable: false,
                status: None,
                latency_ms: Some(started.elapsed().as_millis() as u64),
                error: Some(describe(&err)),
                content_match: None,
            };
        }
    };

    let status = response.status().as_u16();
    let latency_ms = Some(started.elapsed().as_millis() as u64);
    let content_match = match &spec.expected_body {
        Some(needle) => match response.text().await {
            Ok(body) => Some(body.to_lowercase().contains(&needle.to_lowercase())),
            Err(_) => Some(false),
        },
        None => None,
    };

    LocalProbe {
        reachable: true,
        status: Some(status),
        latency_ms,
        error: None,
        content_match,
    }
}

pub async fn probe_externally(
    checkhost: &CheckHost,
    spec: &WebsiteSpec,
) -> crate::error::Result<ExternalProbe> {
    let nodes = checkhost.select(spec.iran_nodes, spec.abroad_nodes).await?;
    let probe = checkhost.http_probe(&spec.url, &nodes).await?;
    Ok(ExternalProbe {
        checked_at: crate::unix_now(),
        permanent_link: probe.permanent_link,
        nodes: probe.nodes,
    })
}

pub fn classify(spec: &WebsiteSpec, report: &WebsiteReport) -> Vec<Finding> {
    let local = &report.local;
    let nodes: &[NodeResult] = report
        .external
        .as_ref()
        .map(|probe| probe.nodes.as_slice())
        .unwrap_or_default();

    let inside = Tally::of(nodes.iter().filter(|node| node.inside_iran()));
    let outside = Tally::of(nodes.iter().filter(|node| !node.inside_iran()));

    if local.reachable && outside.answered > 0 && outside.reachable == 0 {
        return vec![Finding::critical(
            "website.iranAccess",
            format!(
                "reachable from this network but not from {} outside vantage point(s): {}",
                outside.answered,
                outside.failures.join(", ")
            ),
        )];
    }

    if !local.reachable && outside.reachable > 0 {
        return vec![Finding::critical(
            "website.blockedLocally",
            format!(
                "{} of {} outside vantage points succeeded while this network failed: {}",
                outside.reachable,
                outside.answered,
                local.error.clone().unwrap_or_else(|| "no response".into())
            ),
        )];
    }

    if outside.reachable > 0 && inside.answered > 0 && inside.reachable == 0 {
        return vec![Finding::critical(
            "website.blockedInsideIran",
            format!(
                "reachable from abroad but every Iranian vantage point failed: {}",
                inside.failures.join(", ")
            ),
        )];
    }

    if !local.reachable {
        let detail = local
            .error
            .clone()
            .unwrap_or_else(|| "no response from this network".into());
        return vec![Finding::critical(
            "website.down",
            if outside.answered > 0 {
                format!(
                    "{detail}; all {} outside vantage points also failed",
                    outside.answered
                )
            } else {
                detail
            },
        )];
    }

    let mut findings = Vec::new();

    if let Some(expected) = spec.expected_status {
        if local.status != Some(expected) {
            findings.push(Finding::warn(
                "website.unexpectedStatus",
                format!(
                    "expected HTTP {expected}, received {}",
                    local
                        .status
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "nothing".into())
                ),
            ));
        }
    } else if local.status.is_some_and(|code| code >= 400) {
        findings.push(Finding::warn(
            "website.unexpectedStatus",
            format!(
                "server answered with HTTP {}",
                local.status.unwrap_or_default()
            ),
        ));
    }

    if local.content_match == Some(false) {
        findings.push(Finding::warn(
            "website.contentMismatch",
            format!(
                "expected text {:?} was missing from the response body",
                spec.expected_body.clone().unwrap_or_default()
            ),
        ));
    }

    if breached(inside.failed(), spec.iran_alert_threshold) {
        findings.push(Finding::warn(
            "website.ispPartial",
            format!(
                "{} of {} Iranian vantage points failed: {}",
                inside.failed(),
                inside.answered,
                inside.failures.join(", ")
            ),
        ));
    }

    if breached(outside.failed(), spec.abroad_alert_threshold) {
        findings.push(Finding::warn(
            "website.abroadPartial",
            format!(
                "{} of {} outside vantage points failed: {}",
                outside.failed(),
                outside.answered,
                outside.failures.join(", ")
            ),
        ));
    }

    if let Some(error) = &report.external_error {
        findings.push(Finding::warn("website.externalProbeFailed", error.clone()));
    }

    if findings.is_empty() {
        findings.push(Finding::ok(
            "website.healthy",
            match local.latency_ms {
                Some(ms) => format!("HTTP {} in {ms} ms", local.status.unwrap_or_default()),
                None => format!("HTTP {}", local.status.unwrap_or_default()),
            },
        ));
    }

    findings
}

fn breached(failed: usize, threshold: usize) -> bool {
    threshold > 0 && failed >= threshold
}

struct Tally {
    answered: usize,
    reachable: usize,
    failures: Vec<String>,
}

impl Tally {
    fn failed(&self) -> usize {
        self.answered - self.reachable
    }

    fn of<'a>(nodes: impl Iterator<Item = &'a NodeResult>) -> Self {
        let mut tally = Self {
            answered: 0,
            reachable: 0,
            failures: Vec::new(),
        };
        for node in nodes {
            match node.reachable {
                Some(true) => {
                    tally.answered += 1;
                    tally.reachable += 1;
                }
                Some(false) => {
                    tally.answered += 1;
                    tally.failures.push(match &node.message {
                        Some(message) => format!("{} {message}", node.label()),
                        None => node.label(),
                    });
                }
                None => {}
            }
        }
        tally
    }
}

fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "connection timed out".to_owned();
    }
    if error.is_connect() {
        return "connection refused or unreachable".to_owned();
    }
    let mut source: &dyn std::error::Error = error;
    let mut message = error.to_string();
    while let Some(inner) = source.source() {
        message = inner.to_string();
        source = inner;
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::ExternalProbe;

    fn spec() -> WebsiteSpec {
        WebsiteSpec {
            url: "https://example.ir".into(),
            external_probe: true,
            external_interval_secs: 900,
            dns_probe: false,
            expected_status: None,
            expected_body: None,
            iran_nodes: 2,
            abroad_nodes: 2,
            iran_alert_threshold: 1,
            abroad_alert_threshold: 1,
        }
    }

    fn local(reachable: bool) -> LocalProbe {
        LocalProbe {
            reachable,
            status: if reachable { Some(200) } else { None },
            latency_ms: Some(80),
            error: if reachable {
                None
            } else {
                Some("connection timed out".into())
            },
            content_match: None,
        }
    }

    fn node(country: &str, reachable: Option<bool>) -> NodeResult {
        NodeResult {
            node: format!("{country}1.node.check-host.net"),
            country: country.into(),
            city: "Somewhere".into(),
            asn: "AS1".into(),
            reachable,
            latency_ms: None,
            status: None,
            message: None,
            address: None,
        }
    }

    fn report(local: LocalProbe, nodes: Vec<NodeResult>) -> WebsiteReport {
        WebsiteReport {
            local,
            external: Some(ExternalProbe {
                checked_at: 0,
                permanent_link: None,
                nodes,
            }),
            external_error: None,
        }
    }

    #[test]
    fn local_success_with_total_foreign_failure_is_iran_access() {
        let findings = classify(
            &spec(),
            &report(
                local(true),
                vec![node("de", Some(false)), node("us", Some(false))],
            ),
        );
        assert_eq!(findings[0].code, "website.iranAccess");
    }

    #[test]
    fn foreign_success_with_local_failure_is_a_local_block() {
        let findings = classify(&spec(), &report(local(false), vec![node("de", Some(true))]));
        assert_eq!(findings[0].code, "website.blockedLocally");
    }

    #[test]
    fn foreign_success_with_iranian_failure_is_an_inbound_block() {
        let findings = classify(
            &spec(),
            &report(
                local(true),
                vec![node("de", Some(true)), node("ir", Some(false))],
            ),
        );
        assert_eq!(findings[0].code, "website.blockedInsideIran");
    }

    #[test]
    fn everything_failing_is_a_plain_outage() {
        let findings = classify(
            &spec(),
            &report(
                local(false),
                vec![node("de", Some(false)), node("ir", Some(false))],
            ),
        );
        assert_eq!(findings[0].code, "website.down");
    }

    #[test]
    fn one_failing_iranian_network_is_only_a_warning() {
        let findings = classify(
            &spec(),
            &report(
                local(true),
                vec![
                    node("de", Some(true)),
                    node("ir", Some(true)),
                    node("ir", Some(false)),
                ],
            ),
        );
        assert_eq!(findings[0].code, "website.ispPartial");
    }

    #[test]
    fn pending_nodes_never_trigger_an_alert() {
        let findings = classify(
            &spec(),
            &report(local(true), vec![node("de", None), node("ir", None)]),
        );
        assert_eq!(findings[0].code, "website.healthy");
    }

    #[test]
    fn one_failing_foreign_node_stays_quiet_when_the_site_asked_for_two() {
        let mut spec = spec();
        spec.abroad_alert_threshold = 2;
        let quiet = report(
            local(true),
            vec![node("de", Some(true)), node("us", Some(false))],
        );
        assert_eq!(classify(&spec, &quiet)[0].code, "website.healthy");

        let loud = report(
            local(true),
            vec![
                node("de", Some(true)),
                node("us", Some(false)),
                node("br", Some(false)),
            ],
        );
        assert_eq!(classify(&spec, &loud)[0].code, "website.abroadPartial");
    }

    #[test]
    fn a_zero_threshold_silences_the_group_entirely() {
        let mut spec = spec();
        spec.iran_alert_threshold = 0;
        let findings = classify(
            &spec,
            &report(
                local(true),
                vec![
                    node("de", Some(true)),
                    node("ir", Some(true)),
                    node("ir", Some(false)),
                ],
            ),
        );
        assert_eq!(findings[0].code, "website.healthy");
    }
}
