use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager};

use crate::error::Result;
use crate::state::AppState;

pub const SETTINGS_EVENT: &str = "settings://changed";

const SILENCE_SECS: i64 = 3600;

pub fn install(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open IranNANternet", true, None::<&str>)?;
    let silence = MenuItem::with_id(
        app,
        "silence",
        "Silence alerts for 1 hour",
        true,
        None::<&str>,
    )?;
    let resume = MenuItem::with_id(app, "resume", "Resume alerts", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &silence,
            &resume,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("nanternet")
        .tooltip("IranNANternet")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => reveal(app),
            "silence" => spawn_silence(app, Some(crate::unix_now() + SILENCE_SECS)),
            "resume" => spawn_silence(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

pub fn reveal(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn spawn_silence(app: &AppHandle, until: Option<i64>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = silence(&app, until).await {
            eprintln!("could not change the alert snooze: {err}");
        }
    });
}

pub async fn silence(app: &AppHandle, until: Option<i64>) -> Result<()> {
    app.state::<AppState>()
        .mutate(move |store| {
            store.settings.snoozed_until = until;
            Ok(())
        })
        .await?;
    let _ = app.emit(SETTINGS_EVENT, until);
    Ok(())
}
