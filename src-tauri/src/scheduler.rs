use std::collections::HashMap;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::config::{Target, TargetSpec};
use crate::error::Result;
use crate::notification_engine::{self, Alert};
use crate::state::AppState;
use crate::status::{Finding, TargetStatus, WebsiteReport};
use crate::{cert_watch, network_checker, secrets, ssh_manager};

pub const STATUS_EVENT: &str = "target://status";

const TICK: Duration = Duration::from_secs(1);

struct Slot {
    next: Instant,
    next_slow: Instant,
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut slots: HashMap<String, Slot> = HashMap::new();
        loop {
            let state = app.state::<AppState>();
            let targets = state.targets().await;
            let ids: Vec<String> = targets.iter().map(|target| target.id.clone()).collect();

            slots.retain(|id, _| ids.contains(id));
            state.retain_known(&ids).await;

            let now = Instant::now();
            for target in targets.into_iter().filter(|target| target.enabled) {
                let slot = slots.entry(target.id.clone()).or_insert(Slot {
                    next: now,
                    next_slow: now,
                });
                if now < slot.next {
                    continue;
                }
                slot.next = now + Duration::from_secs(target.interval_secs);

                let slow = match target.slow_probe_interval() {
                    Some(interval) if now >= slot.next_slow => {
                        slot.next_slow = now + Duration::from_secs(interval);
                        true
                    }
                    _ => false,
                };

                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = visit(handle, target, slow).await {
                        eprintln!("check failed: {err}");
                    }
                });
            }

            tokio::select! {
                _ = tokio::time::sleep(TICK) => {}
                _ = state.wake.notified() => {}
            }
        }
    });
}

pub async fn visit(app: AppHandle, target: Target, slow: bool) -> Result<TargetStatus> {
    let status = examine(&app, &target, slow).await;
    let state = app.state::<AppState>();

    state.record(status.clone()).await;
    let _ = app.emit(STATUS_EVENT, &status);

    let settings = state.settings().await;
    if state.announce(&status, settings.confirmations).await {
        let alert = Alert {
            target: target.name.clone(),
            level: status.level,
            findings: status.findings.clone(),
        };
        notification_engine::dispatch(&app, &settings, &alert).await;
    }

    Ok(status)
}

async fn examine(app: &AppHandle, target: &Target, slow: bool) -> TargetStatus {
    let state = app.state::<AppState>();
    let started = Instant::now();
    let mut status = TargetStatus::new(&target.id);

    match &target.spec {
        TargetSpec::Website(spec) => {
            let local = network_checker::probe_locally(&state.local_http, spec).await;

            let carried = state
                .status_of(&target.id)
                .await
                .and_then(|previous| previous.website);
            let mut external = carried.as_ref().and_then(|report| report.external.clone());
            let mut external_error = carried.and_then(|report| report.external_error);

            if spec.external_probe && slow {
                match network_checker::probe_externally(&state.checkhost, spec).await {
                    Ok(probe) => {
                        external = Some(probe);
                        external_error = None;
                    }
                    Err(err) => external_error = Some(err.to_string()),
                }
            }

            let report = WebsiteReport {
                local,
                external: if spec.external_probe { external } else { None },
                external_error: if spec.external_probe {
                    external_error
                } else {
                    None
                },
            };
            let findings = network_checker::classify(spec, &report);
            status.website = Some(report);
            status.seal(findings, started)
        }
        TargetSpec::Server(spec) => {
            let secret = match secrets::read(&secrets::target_key(&target.id)).await {
                Ok(secret) => secret,
                Err(err) => {
                    return status.seal(
                        vec![Finding::critical(
                            "server.authenticationFailed",
                            err.to_string(),
                        )],
                        started,
                    );
                }
            };

            let (report, findings) = ssh_manager::check(spec, secret.as_deref()).await;

            if spec.host_fingerprint.is_none() {
                if let Some(fingerprint) = report.host_fingerprint.clone() {
                    let id = target.id.clone();
                    let stored = state
                        .mutate(move |store| {
                            if let Some(found) = store
                                .targets
                                .iter_mut()
                                .find(|candidate| candidate.id == id)
                            {
                                if let TargetSpec::Server(server) = &mut found.spec {
                                    server.host_fingerprint = Some(fingerprint);
                                }
                            }
                            Ok(())
                        })
                        .await;
                    if let Err(err) = stored {
                        eprintln!("could not persist the host key: {err}");
                    }
                }
            }

            status.server = Some(report);
            status.seal(findings, started)
        }
        TargetSpec::Domain(spec) => {
            let known = state
                .status_of(&target.id)
                .await
                .and_then(|previous| previous.domain);
            let (report, findings) =
                cert_watch::inspect(&state.external_http, spec, known, slow).await;
            status.domain = Some(report);
            status.seal(findings, started)
        }
    }
}
