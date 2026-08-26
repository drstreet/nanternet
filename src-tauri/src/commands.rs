use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::config::{Settings, Target, TargetSpec};
use crate::error::{Error, Result};
use crate::state::AppState;
use crate::status::TargetStatus;
use crate::{notification_engine, scheduler, secrets};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetView {
    #[serde(flatten)]
    target: Target,
    secret_stored: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    settings: Settings,
    telegram_token_stored: bool,
}

#[tauri::command]
pub async fn list_targets(state: State<'_, AppState>) -> Result<Vec<TargetView>> {
    let mut views = Vec::new();
    for target in state.targets().await {
        let secret_stored = secrets::exists(&secrets::target_key(&target.id)).await;
        views.push(TargetView {
            target,
            secret_stored,
        });
    }
    Ok(views)
}

#[tauri::command]
pub async fn list_statuses(state: State<'_, AppState>) -> Result<Vec<TargetStatus>> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn save_target(
    state: State<'_, AppState>,
    mut target: Target,
    secret: Option<String>,
) -> Result<Target> {
    target.normalize();
    if target.name.is_empty() {
        return Err(Error::msg("every target needs a name"));
    }
    describe_endpoint(&target)?;

    let key = secrets::target_key(&target.id);
    match secret.as_deref().map(str::trim) {
        Some("") => secrets::forget(&key).await?,
        Some(secret) => secrets::store(&key, secret).await?,
        None => {}
    }

    let stored = target.clone();
    state
        .mutate(move |store| {
            match store
                .targets
                .iter_mut()
                .find(|candidate| candidate.id == stored.id)
            {
                Some(existing) => *existing = stored,
                None => store.targets.push(stored),
            }
            Ok(())
        })
        .await?;

    Ok(target)
}

#[tauri::command]
pub async fn delete_target(state: State<'_, AppState>, id: String) -> Result<()> {
    let removed = id.clone();
    state
        .mutate(move |store| {
            store.targets.retain(|target| target.id != removed);
            Ok(())
        })
        .await?;
    secrets::forget(&secrets::target_key(&id)).await?;
    state.forget(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn set_target_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<()> {
    state
        .mutate(
            move |store| match store.targets.iter_mut().find(|target| target.id == id) {
                Some(target) => {
                    target.enabled = enabled;
                    Ok(())
                }
                None => Err(Error::msg("that target no longer exists")),
            },
        )
        .await
}

#[tauri::command]
pub async fn check_now(app: AppHandle, id: String) -> Result<TargetStatus> {
    let target = app
        .state::<AppState>()
        .target(&id)
        .await
        .ok_or_else(|| Error::msg("that target no longer exists"))?;
    scheduler::visit(app, target, true).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsView> {
    Ok(SettingsView {
        settings: state.settings().await,
        telegram_token_stored: secrets::exists(secrets::TELEGRAM_TOKEN).await,
    })
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut settings: Settings,
    telegram_token: Option<String>,
) -> Result<Settings> {
    settings.normalize();

    match telegram_token.as_deref().map(str::trim) {
        Some("") => secrets::forget(secrets::TELEGRAM_TOKEN).await?,
        Some(token) => secrets::store(secrets::TELEGRAM_TOKEN, token).await?,
        None => {}
    }

    let launcher = app.autolaunch();
    let already = launcher.is_enabled().unwrap_or(false);
    if settings.start_at_login && !already {
        launcher
            .enable()
            .map_err(|err| Error::msg(format!("could not register the login item: {err}")))?;
    } else if !settings.start_at_login && already {
        launcher
            .disable()
            .map_err(|err| Error::msg(format!("could not remove the login item: {err}")))?;
    }

    let stored = settings.clone();
    state
        .mutate(move |store| {
            store.settings = stored;
            Ok(())
        })
        .await?;

    Ok(settings)
}

#[tauri::command]
pub async fn send_test_notification(app: AppHandle) -> Result<()> {
    let settings = app.state::<AppState>().settings().await;
    let alert = notification_engine::Alert {
        target: "IranNANternet".to_owned(),
        level: crate::status::Level::Ok,
        findings: vec![crate::status::Finding::ok(
            "website.healthy",
            "test alert from the settings screen",
        )],
    };
    notification_engine::dispatch(&app, &settings, &alert).await;
    Ok(())
}

fn describe_endpoint(target: &Target) -> Result<()> {
    match &target.spec {
        TargetSpec::Website(spec) => reqwest::Url::parse(&spec.url)
            .map_err(|_| Error::msg("that website address cannot be parsed"))
            .and_then(|url| match url.host_str() {
                Some(_) => Ok(()),
                None => Err(Error::msg("that website address has no host")),
            }),
        TargetSpec::Server(spec) => {
            if spec.host.is_empty() {
                Err(Error::msg("a server needs a hostname or IP address"))
            } else if spec.username.is_empty() {
                Err(Error::msg("a server needs an SSH username"))
            } else {
                Ok(())
            }
        }
        TargetSpec::Domain(spec) => {
            if spec.domain.contains('.') {
                Ok(())
            } else {
                Err(Error::msg("a domain needs at least one dot"))
            }
        }
    }
}
