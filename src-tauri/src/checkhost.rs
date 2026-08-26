use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::{Error, Result};
use crate::status::NodeResult;

const ENDPOINT: &str = "https://check-host.net";
const NODE_CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
const POLL_INTERVAL: Duration = Duration::from_millis(1200);
const POLL_BUDGET: Duration = Duration::from_secs(28);

#[derive(Debug, Clone)]
pub struct Node {
    pub host: String,
    pub country: String,
    pub city: String,
    pub asn: String,
}

impl Node {
    fn inside_iran(&self) -> bool {
        self.country.eq_ignore_ascii_case("ir")
    }

    fn pending(&self) -> NodeResult {
        NodeResult {
            node: self.host.clone(),
            country: self.country.clone(),
            city: self.city.clone(),
            asn: self.asn.clone(),
            reachable: None,
            latency_ms: None,
            status: None,
            message: None,
            address: None,
        }
    }
}

pub struct Probe {
    pub permanent_link: Option<String>,
    pub nodes: Vec<NodeResult>,
}

pub struct CheckHost {
    http: reqwest::Client,
    cache: Mutex<Option<(Instant, Arc<Vec<Node>>)>>,
}

impl CheckHost {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            cache: Mutex::new(None),
        }
    }

    pub async fn nodes(&self) -> Result<Arc<Vec<Node>>> {
        let mut cache = self.cache.lock().await;
        if let Some((fetched, nodes)) = cache.as_ref() {
            if fetched.elapsed() < NODE_CACHE_TTL {
                return Ok(nodes.clone());
            }
        }

        let listing: NodeListing = self
            .http
            .get(format!("{ENDPOINT}/nodes/hosts"))
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut nodes: Vec<Node> = listing
            .nodes
            .into_iter()
            .map(|(host, entry)| Node {
                host,
                country: entry.location.first().cloned().unwrap_or_default(),
                city: entry.location.get(2).cloned().unwrap_or_default(),
                asn: entry.asn.unwrap_or_default(),
            })
            .collect();
        nodes.sort_by(|a, b| a.host.cmp(&b.host));

        if nodes.is_empty() {
            return Err(Error::msg("check-host returned an empty node list"));
        }

        let nodes = Arc::new(nodes);
        *cache = Some((Instant::now(), nodes.clone()));
        Ok(nodes)
    }

    pub async fn select(&self, iran: usize, abroad: usize) -> Result<Vec<Node>> {
        let all = self.nodes().await?;
        let mut chosen = Vec::with_capacity(iran + abroad);
        chosen.extend(spread(
            all.iter().filter(|node| node.inside_iran()),
            iran,
            |node| node.asn.clone(),
        ));
        chosen.extend(spread(
            all.iter().filter(|node| !node.inside_iran()),
            abroad,
            |node| node.country.to_ascii_lowercase(),
        ));
        if chosen.is_empty() {
            return Err(Error::msg("no usable check-host nodes were available"));
        }
        Ok(chosen)
    }

    pub async fn http_probe(&self, url: &str, nodes: &[Node]) -> Result<Probe> {
        let mut query: Vec<(&str, &str)> = vec![("host", url)];
        query.extend(nodes.iter().map(|node| ("node", node.host.as_str())));

        let started: StartResponse = self
            .http
            .get(format!("{ENDPOINT}/check-http"))
            .header(reqwest::header::ACCEPT, "application/json")
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(error) = started.error {
            return Err(Error::msg(format!(
                "check-host refused the request: {error}"
            )));
        }
        let request_id = started
            .request_id
            .ok_or_else(|| Error::msg("check-host did not return a request id"))?;

        let mut collected: HashMap<String, NodeResult> = HashMap::new();
        let deadline = Instant::now() + POLL_BUDGET;

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            let payload: HashMap<String, serde_json::Value> = self
                .http
                .get(format!("{ENDPOINT}/check-result/{request_id}"))
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            for node in nodes {
                if collected.contains_key(&node.host) {
                    continue;
                }
                let Some(value) = payload.get(&node.host) else {
                    continue;
                };
                let Some(outcome) = parse_http_result(value) else {
                    continue;
                };
                let mut result = node.pending();
                result.reachable = Some(outcome.reachable);
                result.latency_ms = outcome.latency_ms;
                result.status = outcome.status;
                result.message = outcome.message;
                result.address = outcome.address;
                collected.insert(node.host.clone(), result);
            }

            if collected.len() == nodes.len() || Instant::now() >= deadline {
                break;
            }
        }

        let results = nodes
            .iter()
            .map(|node| {
                collected
                    .remove(&node.host)
                    .unwrap_or_else(|| node.pending())
            })
            .collect();

        Ok(Probe {
            permanent_link: started.permanent_link,
            nodes: results,
        })
    }
}

fn spread<'a, I, K>(candidates: I, limit: usize, key: K) -> Vec<Node>
where
    I: Iterator<Item = &'a Node>,
    K: Fn(&Node) -> String,
{
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut primary = Vec::new();
    let mut spare = Vec::new();
    for node in candidates {
        if seen.insert(key(node)) {
            primary.push(node.clone());
        } else {
            spare.push(node.clone());
        }
    }
    primary.extend(spare);
    primary.truncate(limit);
    primary
}

#[derive(Debug, PartialEq)]
pub struct NodeOutcome {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
    pub status: Option<String>,
    pub address: Option<String>,
}

pub fn parse_http_result(value: &serde_json::Value) -> Option<NodeOutcome> {
    let row = value.as_array()?.first()?.as_array()?;
    if row.is_empty() {
        return None;
    }
    let succeeded = row.first().and_then(serde_json::Value::as_i64) == Some(1);
    let status = row.get(3).and_then(text);
    Some(NodeOutcome {
        reachable: succeeded || status.is_some(),
        latency_ms: row
            .get(1)
            .and_then(serde_json::Value::as_f64)
            .map(|seconds| (seconds * 1000.0).round() as u64),
        message: row.get(2).and_then(text),
        status,
        address: row.get(4).and_then(text),
    })
}

fn text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(inner) if inner.is_empty() => None,
        serde_json::Value::String(inner) => Some(inner.clone()),
        serde_json::Value::Number(inner) => Some(inner.to_string()),
        _ => None,
    }
}

#[derive(Deserialize)]
struct NodeListing {
    nodes: HashMap<String, NodeEntry>,
}

#[derive(Deserialize)]
struct NodeEntry {
    #[serde(default)]
    asn: Option<String>,
    #[serde(default)]
    location: Vec<String>,
}

#[derive(Deserialize)]
struct StartResponse {
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    permanent_link: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Option<NodeOutcome> {
        parse_http_result(&serde_json::from_str(raw).unwrap())
    }

    #[test]
    fn pending_nodes_are_not_treated_as_failures() {
        assert!(parse("null").is_none());
        assert!(parse("[null]").is_none());
        assert!(parse("[[]]").is_none());
    }

    #[test]
    fn successful_download_is_reachable() {
        let outcome = parse(r#"[[1, 0.131124019622803, "OK", "200", "94.242.206.94"]]"#).unwrap();
        assert!(outcome.reachable);
        assert_eq!(outcome.latency_ms, Some(131));
        assert_eq!(outcome.status.as_deref(), Some("200"));
        assert_eq!(outcome.address.as_deref(), Some("94.242.206.94"));
    }

    #[test]
    fn http_error_still_proves_the_host_answered() {
        let outcome = parse(r#"[[0, 0.17, "Not Found", "404", "94.242.206.94"]]"#).unwrap();
        assert!(outcome.reachable);
        assert_eq!(outcome.status.as_deref(), Some("404"));
    }

    #[test]
    fn network_error_without_status_is_unreachable() {
        let outcome = parse(r#"[[0, 0.07, "No such device or address", null, null]]"#).unwrap();
        assert!(!outcome.reachable);
        assert_eq!(
            outcome.message.as_deref(),
            Some("No such device or address")
        );
        assert!(outcome.address.is_none());
    }

    #[test]
    fn numeric_status_codes_are_accepted() {
        let outcome = parse(r#"[[1, 0.2, "OK", 200, "1.1.1.1"]]"#).unwrap();
        assert_eq!(outcome.status.as_deref(), Some("200"));
    }

    #[test]
    fn selection_prefers_distinct_networks() {
        let nodes = [
            node("ir1", "ir", "AS1"),
            node("ir2", "ir", "AS1"),
            node("ir3", "ir", "AS2"),
        ];
        let picked = spread(nodes.iter(), 2, |node| node.asn.clone());
        assert_eq!(picked[0].host, "ir1");
        assert_eq!(picked[1].host, "ir3");
    }

    fn node(host: &str, country: &str, asn: &str) -> Node {
        Node {
            host: host.to_owned(),
            country: country.to_owned(),
            city: String::new(),
            asn: asn.to_owned(),
        }
    }
}
