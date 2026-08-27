use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::config::WebsiteSpec;
use crate::network_checker;
use crate::status::{DnsAttempt, DnsReport, Finding, LocalProbe};

pub const BUILTIN: &[(&str, &str)] = &[
    ("Shecan", "178.22.122.100"),
    ("403.online", "10.202.10.202"),
    ("Begzar", "185.55.226.26"),
    ("Electro", "78.157.42.100"),
    ("Cloudflare", "1.1.1.1"),
    ("Google", "8.8.8.8"),
];

const QUERY_TIMEOUT: Duration = Duration::from_secs(4);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

const REPLY_LIMIT: usize = 512;

pub const MAX_RESOLVERS: usize = 12;

pub async fn probe(spec: &WebsiteSpec, extra: &[String]) -> DnsReport {
    let host = match reqwest::Url::parse(&spec.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
    {
        Some(host) => host,
        None => return DnsReport::default(),
    };

    let mut roster = Vec::new();
    let mut running = tokio::task::JoinSet::new();
    for (label, address) in resolvers(extra) {
        let Ok(resolver) = address.parse::<IpAddr>() else {
            continue;
        };
        let rank = roster.len();
        roster.push((label.clone(), resolver));
        let spec = spec.clone();
        let host = host.clone();
        running.spawn(async move { (rank, attempt(&spec, &host, label, resolver).await) });
    }

    let mut collected: Vec<Option<DnsAttempt>> = vec![None; roster.len()];
    while let Some(finished) = running.join_next().await {
        if let Ok((rank, attempt)) = finished {
            collected[rank] = Some(attempt);
        }
    }

    let attempts = collected
        .into_iter()
        .zip(roster)
        .map(|(attempt, (label, resolver))| attempt.unwrap_or_else(|| unfinished(label, resolver)))
        .collect();

    DnsReport {
        checked_at: crate::unix_now(),
        attempts,
    }
}

fn is_builtin(resolver: IpAddr) -> bool {
    let address = resolver.to_string();
    BUILTIN.iter().any(|(_, known)| *known == address)
}

fn unfinished(label: String, resolver: IpAddr) -> DnsAttempt {
    DnsAttempt {
        builtin: is_builtin(resolver),
        label,
        resolver: resolver.to_string(),
        answers: Vec::new(),
        reserved: false,
        error: Some("the lookup did not finish".to_owned()),
        probe: None,
    }
}

fn resolvers(extra: &[String]) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = BUILTIN
        .iter()
        .map(|(label, address)| ((*label).to_owned(), (*address).to_owned()))
        .collect();
    for entry in extra {
        let address = entry.trim();
        if address.is_empty() || list.iter().any(|(_, known)| known == address) {
            continue;
        }
        list.push((address.to_owned(), address.to_owned()));
    }
    list.truncate(MAX_RESOLVERS);
    list
}

async fn attempt(spec: &WebsiteSpec, host: &str, label: String, resolver: IpAddr) -> DnsAttempt {
    let builtin = is_builtin(resolver);

    match ask(resolver, host).await {
        Ok(answers) => {
            let probe = match answers.first() {
                Some(first) => fetch_through(spec, host, *first).await,
                None => None,
            };
            DnsAttempt {
                label,
                resolver: resolver.to_string(),
                builtin,
                answers: answers.iter().map(Ipv4Addr::to_string).collect(),
                reserved: answers.iter().copied().all(is_reserved) && !answers.is_empty(),
                error: None,
                probe,
            }
        }
        Err(error) => DnsAttempt {
            label,
            resolver: resolver.to_string(),
            builtin,
            answers: Vec::new(),
            reserved: false,
            error: Some(error),
            probe: None,
        },
    }
}

async fn fetch_through(spec: &WebsiteSpec, host: &str, address: Ipv4Addr) -> Option<LocalProbe> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .user_agent(crate::state::USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .connect_timeout(Duration::from_secs(8))
        .resolve_to_addrs(host, &[SocketAddr::new(IpAddr::V4(address), 0)])
        .build()
        .ok()?;
    Some(network_checker::probe_locally(&client, spec).await)
}

async fn ask(resolver: IpAddr, host: &str) -> Result<Vec<Ipv4Addr>, String> {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let id = u16::from_be_bytes([bytes[0], bytes[1]]);
    let query = encode(id, host).ok_or("that host name is not a valid DNS name")?;

    let bind = if resolver.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind).await.map_err(|err| err.to_string())?;

    socket
        .connect(SocketAddr::new(resolver, 53))
        .await
        .map_err(|err| err.to_string())?;
    socket.send(&query).await.map_err(|err| err.to_string())?;

    let mut buffer = [0u8; REPLY_LIMIT];
    let read = tokio::time::timeout(QUERY_TIMEOUT, socket.recv(&mut buffer))
        .await
        .map_err(|_| "no answer before the timeout".to_owned())?
        .map_err(|err| err.to_string())?;

    decode(&buffer[..read], id).map_err(str::to_owned)
}

fn encode(id: u16, host: &str) -> Option<Vec<u8>> {
    let mut name = Vec::with_capacity(host.len() + 2);
    for label in host.split('.').filter(|label| !label.is_empty()) {
        if label.is_empty() || label.len() > 63 || !label.is_ascii() {
            return None;
        }
        name.push(label.len() as u8);
        name.extend_from_slice(label.as_bytes());
    }
    name.push(0);
    if name.len() < 2 || name.len() > 255 {
        return None;
    }

    let mut message = Vec::with_capacity(name.len() + 16);
    message.extend_from_slice(&id.to_be_bytes());

    message.extend_from_slice(&[0x01, 0x00]);

    message.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
    message.extend_from_slice(&name);

    message.extend_from_slice(&[0, 1, 0, 1]);
    Some(message)
}

fn decode(message: &[u8], id: u16) -> Result<Vec<Ipv4Addr>, &'static str> {
    if message.len() < 12 {
        return Err("the reply was too short to be DNS");
    }
    if u16::from_be_bytes([message[0], message[1]]) != id {
        return Err("the reply did not match the query");
    }
    if message[2] & 0x80 == 0 {
        return Err("the reply was not a DNS answer");
    }
    match message[3] & 0x0F {
        0 => {}
        1 => return Err("the resolver rejected the query as malformed"),
        2 => return Err("the resolver reported a server failure"),
        3 => return Err("no such host (NXDOMAIN)"),
        5 => return Err("the resolver refused the query"),
        _ => return Err("the resolver returned an error"),
    }

    let questions = u16::from_be_bytes([message[4], message[5]]);
    let answers = u16::from_be_bytes([message[6], message[7]]);

    let mut at = 12;
    for _ in 0..questions {
        at = skip_name(message, at).ok_or("the question section is malformed")?;
        at = step(message, at, 4).ok_or("the question section is truncated")?;
    }

    let mut found = Vec::new();
    for _ in 0..answers {
        at = skip_name(message, at).ok_or("an answer record is malformed")?;
        if at + 10 > message.len() {
            return Err("an answer record is truncated");
        }
        let kind = u16::from_be_bytes([message[at], message[at + 1]]);
        let length = u16::from_be_bytes([message[at + 8], message[at + 9]]) as usize;
        at = step(message, at + 10, length).ok_or("an answer record is truncated")?;
        if kind == 1 && length == 4 {
            let data = &message[at - 4..at];
            found.push(Ipv4Addr::new(data[0], data[1], data[2], data[3]));
        }
    }

    if found.is_empty() {
        return Err("the resolver returned no address for this name");
    }
    Ok(found)
}

fn skip_name(message: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let length = *message.get(at)?;
        match length & 0xC0 {
            0xC0 => return step(message, at, 2),
            0x00 if length == 0 => return step(message, at, 1),
            0x00 => at = step(message, at, 1 + length as usize)?,
            _ => return None,
        }
    }
}

fn step(message: &[u8], at: usize, by: usize) -> Option<usize> {
    at.checked_add(by).filter(|end| *end <= message.len())
}

fn is_reserved(address: Ipv4Addr) -> bool {
    address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_unspecified()
        || address.is_broadcast()
        || address.is_documentation()
        || matches!(address.octets(), [100, 64..=127, _, _])
}

pub fn classify(report: &DnsReport, local_reachable: bool) -> Vec<Finding> {
    if report.attempts.is_empty() {
        return Vec::new();
    }

    let answered: Vec<&DnsAttempt> = report
        .attempts
        .iter()
        .filter(|attempt| attempt.error.is_none())
        .collect();

    if answered.is_empty() {
        return vec![Finding::warn(
            "website.dnsUnavailable",
            format!(
                "none of the {} resolvers answered; this network may block outbound port 53",
                report.attempts.len()
            ),
        )];
    }

    let mut findings = Vec::new();

    if answered.iter().any(|attempt| !attempt.reserved) {
        let faked: Vec<&str> = answered
            .iter()
            .filter(|attempt| attempt.reserved)
            .map(|attempt| attempt.label.as_str())
            .collect();
        if !faked.is_empty() {
            findings.push(Finding::critical(
                "website.dnsSinkholed",
                format!(
                    "{} answered with a non-routable address, which is how DNS-level filtering looks: {}",
                    faked.len(),
                    faked.join(", ")
                ),
            ));
        }
    }

    if !local_reachable {
        let working: Vec<&str> = answered
            .iter()
            .filter(|attempt| attempt.probe.as_ref().is_some_and(|probe| probe.reachable))
            .map(|attempt| attempt.label.as_str())
            .collect();
        if !working.is_empty() {
            findings.push(Finding::warn(
                "website.dnsBypass",
                format!(
                    "the site loads when resolved by {}, so switching resolver would restore access",
                    working.join(", ")
                ),
            ));
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> WebsiteSpec {
        WebsiteSpec {
            url: "https://example.ir".into(),
            external_probe: false,
            external_interval_secs: 900,
            dns_probe: true,
            expected_status: None,
            expected_body: None,
            iran_nodes: 0,
            abroad_nodes: 0,
            iran_alert_threshold: 1,
            abroad_alert_threshold: 1,
        }
    }

    fn reply(id: u16, rcode: u8, records: &[(u16, &[u8])]) -> Vec<u8> {
        let question = encode(id, "example.ir").unwrap();
        let mut message = question.clone();

        message[2] = 0x81;
        message[3] = rcode;
        message[6..8].copy_from_slice(&(records.len() as u16).to_be_bytes());
        for (kind, data) in records {
            message.extend_from_slice(&[0xC0, 0x0C]);
            message.extend_from_slice(&kind.to_be_bytes());
            message.extend_from_slice(&[0, 1]);
            message.extend_from_slice(&60u32.to_be_bytes());
            message.extend_from_slice(&(data.len() as u16).to_be_bytes());
            message.extend_from_slice(data);
        }
        message
    }

    fn attempt(label: &str, answers: &[&str], reachable: Option<bool>) -> DnsAttempt {
        DnsAttempt {
            label: label.into(),
            resolver: "1.1.1.1".into(),
            builtin: true,
            answers: answers.iter().map(|a| (*a).to_owned()).collect(),
            reserved: answers.iter().all(|a| is_reserved(a.parse().unwrap()))
                && !answers.is_empty(),
            error: None,
            probe: reachable.map(|reachable| LocalProbe {
                reachable,
                status: Some(200),
                latency_ms: Some(10),
                error: None,
                content_match: None,
            }),
        }
    }

    fn failed(label: &str) -> DnsAttempt {
        DnsAttempt {
            label: label.into(),
            resolver: "1.1.1.1".into(),
            builtin: true,
            answers: Vec::new(),
            reserved: false,
            error: Some("no answer before the timeout".into()),
            probe: None,
        }
    }

    fn report(attempts: Vec<DnsAttempt>) -> DnsReport {
        DnsReport {
            checked_at: 0,
            attempts,
        }
    }

    #[test]
    fn encodes_a_query_the_wire_format_accepts() {
        let query = encode(0xBEEF, "example.ir").unwrap();
        assert_eq!(&query[0..2], &[0xBE, 0xEF]);
        assert_eq!(&query[2..4], &[0x01, 0x00], "recursion desired");
        assert_eq!(&query[4..6], &[0x00, 0x01], "exactly one question");
        assert_eq!(&query[12..20], b"\x07example");
        assert_eq!(&query[20..23], b"\x02ir");
        assert_eq!(query[23], 0, "root label terminates the name");
        assert_eq!(&query[24..], &[0, 1, 0, 1], "QTYPE A, QCLASS IN");
    }

    #[test]
    fn rejects_names_the_protocol_cannot_carry() {
        assert!(encode(1, &"a".repeat(64)).is_none(), "label over 63 bytes");
        assert!(encode(1, "").is_none(), "empty name");
        assert!(
            encode(1, &vec!["ab"; 100].join(".")).is_none(),
            "over 255 bytes"
        );
    }

    #[test]
    fn reads_addresses_out_of_a_reply() {
        let message = reply(0x1234, 0, &[(1, &[93, 184, 216, 34]), (1, &[1, 2, 3, 4])]);
        assert_eq!(
            decode(&message, 0x1234).unwrap(),
            vec![Ipv4Addr::new(93, 184, 216, 34), Ipv4Addr::new(1, 2, 3, 4)]
        );
    }

    #[test]
    fn skips_records_that_are_not_addresses() {
        let message = reply(
            7,
            0,
            &[(5, b"\x03www\x07example\x02ir\x00"), (1, &[10, 0, 0, 1])],
        );
        assert_eq!(
            decode(&message, 7).unwrap(),
            vec![Ipv4Addr::new(10, 0, 0, 1)]
        );
    }

    #[test]
    fn refuses_a_reply_that_answers_a_different_query() {
        let message = reply(0x1111, 0, &[(1, &[1, 1, 1, 1])]);
        assert!(
            decode(&message, 0x2222).is_err(),
            "id mismatch must not pass"
        );
    }

    #[test]
    fn surfaces_resolver_error_codes() {
        assert!(decode(&reply(1, 3, &[]), 1).is_err(), "NXDOMAIN");
        assert!(decode(&reply(1, 2, &[]), 1).is_err(), "SERVFAIL");
        assert!(decode(&reply(1, 5, &[]), 1).is_err(), "REFUSED");
        assert!(
            decode(&reply(1, 0, &[]), 1).is_err(),
            "NOERROR with no address"
        );
    }

    #[test]
    fn survives_a_truncated_or_hostile_reply() {
        let message = reply(1, 0, &[(1, &[8, 8, 8, 8])]);
        for cut in 12..message.len() {
            let _ = decode(&message[..cut], 1);
        }

        let mut hostile = message.clone();
        let tail = hostile.len() - 1;
        hostile[tail] = 0x3F;
        let _ = decode(&hostile, 1);
    }

    #[test]
    fn knows_which_addresses_cannot_serve_a_public_site() {
        assert!(is_reserved("10.10.34.34".parse().unwrap()));
        assert!(is_reserved("0.0.0.0".parse().unwrap()));
        assert!(is_reserved("127.0.0.1".parse().unwrap()));
        assert!(is_reserved("192.168.1.1".parse().unwrap()));
        assert!(is_reserved("100.100.0.1".parse().unwrap()));
        assert!(!is_reserved("93.184.216.34".parse().unwrap()));

        assert!(!is_reserved("178.22.122.100".parse().unwrap()));
    }

    #[test]
    fn calls_out_a_sinkholed_answer() {
        let report = report(vec![
            attempt("Cloudflare", &["93.184.216.34"], Some(true)),
            attempt("ISP", &["10.10.34.34"], Some(false)),
        ]);
        let findings = classify(&report, true);
        assert_eq!(findings[0].code, "website.dnsSinkholed");
        assert!(findings[0].detail.contains("ISP"));
    }

    #[test]
    fn leaves_an_internal_site_alone() {
        let report = report(vec![
            attempt("Cloudflare", &["192.168.1.10"], Some(true)),
            attempt("Shecan", &["192.168.1.10"], Some(true)),
        ]);
        assert!(
            classify(&report, true).is_empty(),
            "no finding for a LAN name"
        );
    }

    #[test]
    fn points_at_a_resolver_that_still_works() {
        let report = report(vec![
            attempt("Shecan", &["93.184.216.34"], Some(true)),
            attempt("Google", &["93.184.216.34"], Some(false)),
        ]);
        let findings = classify(&report, false);
        assert_eq!(findings[0].code, "website.dnsBypass");
        assert!(findings[0].detail.contains("Shecan"));
        assert!(!findings[0].detail.contains("Google"));
    }

    #[test]
    fn stays_quiet_when_a_working_site_resolves_everywhere() {
        let report = report(vec![
            attempt("Shecan", &["93.184.216.34"], Some(true)),
            attempt("Google", &["93.184.216.35"], Some(true)),
        ]);

        assert!(classify(&report, true).is_empty());
    }

    #[test]
    fn blames_the_network_when_no_resolver_answers() {
        let report = report(vec![failed("Shecan"), failed("Google")]);
        let findings = classify(&report, true);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "website.dnsUnavailable");
    }

    #[test]
    fn deduplicates_a_custom_resolver_that_repeats_a_builtin() {
        let list = resolvers(&["8.8.8.8".into(), "9.9.9.9".into(), "  ".into()]);
        assert_eq!(list.iter().filter(|(_, ip)| ip == "8.8.8.8").count(), 1);
        assert!(list.iter().any(|(_, ip)| ip == "9.9.9.9"));
        assert!(list.len() <= MAX_RESOLVERS);
    }

    #[tokio::test]
    #[ignore = "needs outbound UDP/53"]
    async fn a_real_resolver_understands_the_query() {
        let answers = ask("1.1.1.1".parse().unwrap(), "example.com")
            .await
            .expect("cloudflare should resolve example.com");
        assert!(!answers.is_empty());
        assert!(!answers.iter().any(|a| is_reserved(*a)));
        println!("example.com -> {answers:?}");
    }

    #[tokio::test]
    async fn a_bad_url_yields_an_empty_report_instead_of_a_panic() {
        let mut broken = spec();
        broken.url = "not a url".into();
        assert!(probe(&broken, &[]).await.attempts.is_empty());
    }
}
