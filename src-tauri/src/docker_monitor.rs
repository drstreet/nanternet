use crate::status::{ContainerState, Finding};

pub const FORMAT: &str = "{{.Names}}|{{.State}}|{{.Status}}";

pub fn parse(raw: &str) -> Vec<ContainerState> {
    raw.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '|');
            let name = fields.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let state = fields
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            let status = fields.next().unwrap_or_default().trim().to_owned();
            Some(ContainerState {
                name: name.to_owned(),
                health: health_of(&status),
                state,
                status,
            })
        })
        .collect()
}

fn health_of(status: &str) -> Option<String> {
    let lowered = status.to_ascii_lowercase();
    let opening = lowered.find('(')?;
    let closing = lowered[opening..].find(')')? + opening;
    let inside = lowered[opening + 1..closing].trim();
    let value = inside.strip_prefix("health:").unwrap_or(inside).trim();
    match value {
        "healthy" | "unhealthy" | "starting" => Some(value.to_owned()),
        _ => None,
    }
}

pub fn locate<'a>(wanted: &str, containers: &'a [ContainerState]) -> Option<&'a ContainerState> {
    containers
        .iter()
        .find(|container| container.name == wanted)
        .or_else(|| {
            containers
                .iter()
                .find(|container| container.name.contains(wanted))
        })
}

pub fn classify(wanted: &[String], containers: &[ContainerState]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for name in wanted {
        let Some(container) = locate(name, containers) else {
            findings.push(Finding::critical(
                "docker.missing",
                format!("no container matching {name:?} exists on the host"),
            ));
            continue;
        };
        if container.state != "running" {
            findings.push(Finding::critical(
                "docker.notRunning",
                format!(
                    "{} is {} ({})",
                    container.name, container.state, container.status
                ),
            ));
            continue;
        }
        match container.health.as_deref() {
            Some("unhealthy") => findings.push(Finding::critical(
                "docker.unhealthy",
                format!("{} reports an unhealthy healthcheck", container.name),
            )),
            Some("starting") => findings.push(Finding::warn(
                "docker.healthStarting",
                format!("{} is still warming up", container.name),
            )),
            _ => {}
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "nginx|running|Up 3 days\n\
db|running|Up 2 hours (healthy)\n\
cache|running|Up 5 seconds (health: starting)\n\
worker|exited|Exited (137) 4 minutes ago\n\
api|running|Up 1 hour (unhealthy)\n";

    #[test]
    fn parses_state_and_health_separately() {
        let containers = parse(SAMPLE);
        assert_eq!(containers.len(), 5);
        assert_eq!(containers[0].health, None);
        assert_eq!(containers[1].health.as_deref(), Some("healthy"));
        assert_eq!(containers[2].health.as_deref(), Some("starting"));
        assert_eq!(containers[3].state, "exited");
        assert_eq!(containers[4].health.as_deref(), Some("unhealthy"));
    }

    #[test]
    fn exit_code_parentheses_are_not_mistaken_for_health() {
        let containers = parse("worker|exited|Exited (137) 4 minutes ago\n");
        assert_eq!(containers[0].health, None);
    }

    #[test]
    fn names_with_pipes_are_impossible_so_status_text_survives_intact() {
        let containers = parse("db|running|Up 2 hours (healthy)");
        assert_eq!(containers[0].status, "Up 2 hours (healthy)");
    }

    #[test]
    fn compose_suffixes_still_match_a_plain_service_name() {
        let containers = parse("stack-db-1|running|Up 2 hours\n");
        assert_eq!(locate("db", &containers).unwrap().name, "stack-db-1");
    }

    #[test]
    fn exact_names_win_over_substring_matches() {
        let containers = parse("stack-db-1|exited|Exited (1) ago\ndb|running|Up 1 hour\n");
        assert_eq!(locate("db", &containers).unwrap().state, "running");
    }

    #[test]
    fn every_failure_mode_is_reported_once() {
        let wanted = vec![
            "nginx".to_owned(),
            "worker".to_owned(),
            "api".to_owned(),
            "cache".to_owned(),
            "ghost".to_owned(),
        ];
        let codes: Vec<String> = classify(&wanted, &parse(SAMPLE))
            .into_iter()
            .map(|finding| finding.code)
            .collect();
        assert_eq!(
            codes,
            vec![
                "docker.notRunning",
                "docker.unhealthy",
                "docker.healthStarting",
                "docker.missing"
            ]
        );
    }
}
