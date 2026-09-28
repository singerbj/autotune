//! Typed commands (Architecture › Tauri app layer). Bindings are generated
//! by tauri-specta into `ui/src/bindings.ts`; never edit that file by hand.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tuner_audio::DeviceInfo;
use tuner_dsp::{Scale, TuningParams, VoiceRange};
use tuner_engine::LatencyResult;
use tuner_win::{EndpointSummary, SetupInputs, SetupReport};

use crate::config::{AppConfig, ConfigPatch, Preset};
use crate::diagnostics::{report, Diagnostics};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::EngineStatusEvent;
use crate::state::{lock, state};
use crate::updater::{self, UpdateStatus};

/// M0 round-trip check.
#[tauri::command]
#[specta::specta]
pub fn ping(message: String) -> String {
    format!("pong: {message}")
}

#[derive(Serialize, Debug, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    /// Show the setup wizard (first run, or launched with `--first-run`).
    pub show_wizard: bool,
    pub asio_compiled: bool,
}

#[tauri::command]
#[specta::specta]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    let s = state(&app);
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        show_wizard: s.first_run_arg || !s.config().wizard_completed,
        asio_compiled: cfg!(feature = "asio"),
    }
}

/// Capture and render endpoints with ASIO and VB-Cable flagged (FR-01).
#[tauri::command]
#[specta::specta]
pub fn list_devices(app: AppHandle) -> AppResult<Vec<DeviceInfo>> {
    state(&app).devices()
}

#[tauri::command]
#[specta::specta]
pub fn get_config(app: AppHandle) -> AppConfig {
    state(&app).config()
}

/// Patch persisted settings (FR-20); rebuilds the engine when devices change.
#[tauri::command]
#[specta::specta]
pub fn set_config(app: AppHandle, patch: ConfigPatch) -> AppResult<AppConfig> {
    let s = state(&app);
    let new_hotkey = patch.bypass_hotkey.clone();
    let rebuild = s.update_config(|c| c.apply(patch));
    let c = s.config();
    s.controls.set_monitor(c.monitor_enabled, c.monitor_volume);
    if let Some(h) = new_hotkey {
        crate::register_hotkey(&app, &h)?;
    }
    if rebuild {
        let devices = s.devices().unwrap_or_default();
        let cfg = s.engine_config(&devices);
        let _ = lock(&s.supervisor).set_config(cfg);
        crate::emit_status(&app);
    }
    s.flush_config();
    Ok(c)
}

pub fn start_engine_inner(app: &AppHandle) -> AppResult<EngineStatusEvent> {
    let s = state(app);
    let devices = s.devices()?;
    let cfg = s.engine_config(&devices);
    let result = lock(&s.supervisor).start(cfg);
    if s.config().route_all_apps {
        if let Some(cable_out) = devices.iter().find(|d| d.is_cable_output()) {
            if let Err(e) = s.routing().enable(&cable_out.id) {
                tracing::warn!("re-applying 'use for all apps' failed: {e}");
            }
        }
    }
    crate::emit_status(app);
    result?;
    Ok(s.status_event())
}

/// Open all streams through the fallback chain (FR-02).
#[tauri::command]
#[specta::specta]
pub fn start_engine(app: AppHandle) -> AppResult<EngineStatusEvent> {
    start_engine_inner(&app)
}

#[tauri::command]
#[specta::specta]
pub fn stop_engine(app: AppHandle) -> EngineStatusEvent {
    let s = state(&app);
    lock(&s.supervisor).stop();
    crate::emit_status(&app);
    s.status_event()
}

#[tauri::command]
#[specta::specta]
pub fn get_engine_status(app: AppHandle) -> EngineStatusEvent {
    state(&app).status_event()
}

/// Partial tuning parameter update.
#[derive(Deserialize, Debug, Clone, Default, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ParamsPatch {
    pub key: Option<u8>,
    pub scale: Option<Scale>,
    pub custom_mask: Option<u16>,
    pub retune_ms: Option<f32>,
    pub humanize: Option<f32>,
    pub voice_range: Option<VoiceRange>,
    pub mix: Option<f32>,
    pub gate_threshold_db: Option<f32>,
    pub bypass: Option<bool>,
}

impl ParamsPatch {
    pub fn apply(self, p: &mut TuningParams) {
        if let Some(v) = self.key {
            p.key = v;
        }
        if let Some(v) = self.scale {
            p.scale = v;
        }
        if let Some(v) = self.custom_mask {
            p.custom_mask = v;
        }
        if let Some(v) = self.retune_ms {
            p.retune_ms = v;
        }
        if let Some(v) = self.humanize {
            p.humanize = v;
        }
        if let Some(v) = self.voice_range {
            p.voice_range = v;
        }
        if let Some(v) = self.mix {
            p.mix = v;
        }
        if let Some(v) = self.gate_threshold_db {
            p.gate_threshold_db = v;
        }
        if let Some(v) = self.bypass {
            p.bypass = v;
        }
        *p = p.sanitized();
    }
}

/// Live parameter change (FR-11): atomics to the audio thread, config saved
/// by the control loop.
#[tauri::command]
#[specta::specta]
pub fn set_params(app: AppHandle, patch: ParamsPatch) -> TuningParams {
    let s = state(&app);
    let bypass_changed = patch.bypass;
    let params = s.update_config(|c| {
        patch.apply(&mut c.params);
        c.params
    });
    s.controls.set_params(&params);
    if let Some(b) = bypass_changed {
        crate::tray::sync(&app, b);
    }
    params
}

#[tauri::command]
#[specta::specta]
pub fn set_bypass(app: AppHandle, bypass: bool) -> bool {
    state(&app).set_bypass(&app, bypass);
    bypass
}

/// Acoustic loopback test (FR-16).
#[tauri::command]
#[specta::specta]
pub async fn run_latency_test(app: AppHandle) -> AppResult<LatencyResult> {
    let handle = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        let s = state(&handle);
        let mut sup = lock(&s.supervisor);
        let engine = sup
            .engine_mut()
            .ok_or_else(|| AppError::new(ErrorKind::NotRunning, "Start the tuner first."))?;
        Ok::<_, AppError>(engine.measure_latency(std::time::Duration::from_secs(4))?)
    })
    .await
    .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))??;
    state(&app).update_config(|c| c.measured_latency_ms = Some(r.total_ms));
    Ok(r)
}

/// VB-Cable, conflicts, Discord session, sidetone and Bluetooth (FR-12, FR-13).
#[tauri::command]
#[specta::specta]
pub fn run_setup_check(app: AppHandle) -> AppResult<SetupReport> {
    let s = state(&app);
    let devices = s.devices()?;
    let cfg = s.config();
    let endpoints: Vec<EndpointSummary> = devices
        .iter()
        .map(|d| EndpointSummary {
            id: d.id.clone(),
            name: d.name.clone(),
            is_capture: d.direction == tuner_audio::Direction::Capture,
            is_bluetooth: d.is_bluetooth,
            is_vb_cable: d.is_vb_cable,
        })
        .collect();
    let default_of = |dir| {
        devices
            .iter()
            .find(|d| d.direction == dir && d.is_default)
            .map(|d| d.id.clone())
    };
    let mic = cfg
        .capture_device
        .clone()
        .or_else(|| default_of(tuner_audio::Direction::Capture));
    let hp = cfg
        .monitor_device
        .clone()
        .or_else(|| default_of(tuner_audio::Direction::Render));
    let sessions = |pred: fn(&DeviceInfo) -> bool| {
        devices
            .iter()
            .find(|d| pred(d))
            .and_then(|d| tuner_win::endpoint_sessions(&d.id).ok())
            .unwrap_or_default()
    };
    let cin = sessions(DeviceInfo::is_cable_input);
    let cout = sessions(DeviceInfo::is_cable_output);
    Ok(tuner_win::evaluate_setup(&SetupInputs {
        endpoints: &endpoints,
        selected_mic: mic.as_deref(),
        selected_headphones: hp.as_deref(),
        cable_input_sessions: &cin,
        cable_output_sessions: &cout,
        own_pid: std::process::id(),
        listen_enabled: mic.as_deref().and_then(tuner_win::listen_to_device_enabled),
    }))
}

/// "Use for all apps" (FR-14).
#[tauri::command]
#[specta::specta]
pub fn set_route_all_apps(app: AppHandle, enabled: bool) -> AppResult<bool> {
    let s = state(&app);
    if enabled {
        let devices = s.devices()?;
        let cable = devices
            .iter()
            .find(|d| d.is_cable_output())
            .ok_or_else(|| AppError::new(ErrorKind::Device, "VB-Cable is not installed."))?;
        s.routing().enable(&cable.id)?;
    } else {
        s.routing().disable()?;
    }
    s.update_config(|c| c.route_all_apps = enabled);
    s.flush_config();
    Ok(enabled)
}

#[tauri::command]
#[specta::specta]
pub fn open_asio_panel(app: AppHandle) -> AppResult<()> {
    Ok(state(&app).backend.open_asio_panel()?)
}

/// Diagnostics panel (FR-18).
#[tauri::command]
#[specta::specta]
pub fn get_diagnostics(app: AppHandle) -> Diagnostics {
    let s = state(&app);
    let cfg = s.config();
    let (supervisor, engine, meters) = {
        let mut sup = lock(&s.supervisor);
        let status = sup.status();
        let engine = sup.engine().map(|e| e.status().clone());
        let meters = sup.engine_mut().map(|e| e.meters());
        (status, engine, meters)
    };
    let mut d = Diagnostics {
        app_version: app.package_info().version.to_string(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        audio_backend: s.backend.name().into(),
        supervisor,
        engine,
        meters,
        measured_latency_ms: cfg.measured_latency_ms,
        log_dir: s.log_dir.display().to_string(),
        config_path: s.config.lock().path().display().to_string(),
        report: String::new(),
    };
    d.report = report(&d);
    d
}

/// Mark the wizard done (FR-12).
#[tauri::command]
#[specta::specta]
pub fn complete_wizard(app: AppHandle) -> AppConfig {
    let s = state(&app);
    s.update_config(|c| c.wizard_completed = true);
    s.flush_config();
    s.config()
}

/// Launch at login (FR-19).
#[tauri::command]
#[specta::specta]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> AppResult<bool> {
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
    let s = state(&app);
    s.update_config(|c| c.launch_at_login = enabled);
    s.flush_config();
    Ok(enabled)
}

/// Save the current parameters as a named preset (FR-21).
#[tauri::command]
#[specta::specta]
pub fn save_preset(app: AppHandle, name: String) -> AppResult<Vec<Preset>> {
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::new(
            ErrorKind::Config,
            "Preset names must be 1–64 characters.",
        ));
    }
    let s = state(&app);
    let presets = s.update_config(|c| {
        let params = c.params;
        match c.presets.iter_mut().find(|p| p.name == name) {
            Some(p) => p.params = params,
            None => c.presets.push(Preset { name, params }),
        }
        c.presets.clone()
    });
    s.flush_config();
    Ok(presets)
}

#[tauri::command]
#[specta::specta]
pub fn load_preset(app: AppHandle, name: String) -> AppResult<TuningParams> {
    let s = state(&app);
    let preset = s
        .config()
        .presets
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| AppError::new(ErrorKind::Config, format!("No preset named \"{name}\".")))?;
    let bypass = s.controls.bypass();
    let params = TuningParams {
        bypass,
        ..preset.params
    };
    s.update_config(|c| c.params = params);
    s.controls.set_params(&params);
    Ok(params)
}

#[tauri::command]
#[specta::specta]
pub fn delete_preset(app: AppHandle, name: String) -> Vec<Preset> {
    let s = state(&app);
    let presets = s.update_config(|c| {
        c.presets.retain(|p| p.name != name);
        c.presets.clone()
    });
    s.flush_config();
    presets
}

#[tauri::command]
#[specta::specta]
pub fn get_update_status(app: AppHandle) -> UpdateStatus {
    lock(&state(&app).updater).status.clone()
}

/// Manual "Check for updates" (FR-22).
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(app: AppHandle) -> AppResult<UpdateStatus> {
    updater::check_and_download(&app).await
}

/// Install a downloaded update and restart.
#[tauri::command]
#[specta::specta]
pub fn install_update(app: AppHandle) -> AppResult<()> {
    updater::install(&app)
}

/// Hide to tray (the engine keeps running).
#[tauri::command]
#[specta::specta]
pub fn hide_window(app: AppHandle) {
    crate::hide_main(&app);
}

#[tauri::command]
#[specta::specta]
pub fn quit_app(app: AppHandle) {
    crate::quit(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr11_params_patch_merges_and_sanitizes() {
        let mut p = TuningParams::default();
        ParamsPatch {
            key: Some(14),
            retune_ms: Some(999.0),
            scale: Some(Scale::Major),
            ..Default::default()
        }
        .apply(&mut p);
        assert_eq!(p.key, 2);
        assert_eq!(p.retune_ms, 200.0);
        assert_eq!(p.scale, Scale::Major);
        assert_eq!(p.mix, 1.0, "untouched fields keep their values");
    }

    #[test]
    fn m0_ping_round_trips() {
        assert_eq!(ping("hi".into()), "pong: hi");
    }
}
