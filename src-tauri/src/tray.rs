//! System tray (FR-19) with the tuning on/off toggle (FR-10).

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{lock, state};

pub struct TrayHandles {
    /// Checked while tuning is on (not bypassed).
    tuning: CheckMenuItem<Wry>,
    engine: MenuItem<Wry>,
}

pub struct TrayState(pub Mutex<Option<TrayHandles>>);

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let bypass_on = state(app).controls.bypass();
    let show = MenuItem::with_id(app, "show", "Open TunedUp", true, None::<&str>)?;
    let tuning = CheckMenuItem::with_id(
        app,
        "tuning",
        tuning_label(app),
        true,
        !bypass_on,
        None::<&str>,
    )?;
    let engine = MenuItem::with_id(app, "engine", "Stop tuner", true, None::<&str>)?;
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &tuning,
            &engine,
            &PredefinedMenuItem::separator(app)?,
            &update,
            &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("TunedUp")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "show" => show_main(app),
            "tuning" => {
                let s = state(app);
                let on = !s.controls.bypass();
                s.set_bypass(app, on);
            }
            "engine" => {
                let s = state(app);
                let running =
                    lock(&s.supervisor).status().state != tuner_engine::SupervisorState::Stopped;
                if running {
                    lock(&s.supervisor).stop();
                } else {
                    let _ = crate::commands::start_engine_inner(app);
                }
                crate::emit_status(app);
            }
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if crate::updater::has_pending(&app) {
                        let _ = crate::updater::install(&app);
                    } else {
                        let _ = crate::updater::check_and_download(&app).await;
                    }
                });
            }
            "quit" => crate::quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayState(Mutex::new(Some(TrayHandles { tuning, engine }))));
    Ok(())
}

/// "Tuning on (Ctrl+Alt+B)" with the registered hotkey, if any.
fn tuning_label(app: &AppHandle) -> String {
    match lock(&state(app).hotkey).status.active.as_deref() {
        Some(hotkey) => format!("Tuning on ({})", hotkey.replace("CommandOrControl", "Ctrl")),
        None => "Tuning on".into(),
    }
}

/// Reflect tuning / engine state and the hotkey in the tray menu.
pub fn sync(app: &AppHandle, bypass: bool) {
    let Some(t) = app.try_state::<TrayState>() else {
        return;
    };
    if let Some(h) = lock(&t.0).as_ref() {
        let _ = h.tuning.set_checked(!bypass);
        let _ = h.tuning.set_text(tuning_label(app));
        let running =
            lock(&state(app).supervisor).status().state != tuner_engine::SupervisorState::Stopped;
        let _ = h
            .engine
            .set_text(if running { "Stop tuner" } else { "Start tuner" });
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(if bypass {
            "TunedUp — tuning off"
        } else {
            "TunedUp — tuning on"
        }));
    }
}
