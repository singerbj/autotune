//! Application state shared by commands, the tray, the hotkey and the
//! control loop. Rust owns all state; the UI only renders it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tuner_audio::{AudioBackend, DeviceInfo};
use tuner_engine::{EngineConfig, SharedControls, Supervisor};
use tuner_win::{DefaultEndpointControl, DeviceWatcher, RouteAllApps};

use crate::config::{AppConfig, ConfigStore, SharedConfig};
use crate::error::AppResult;
use crate::events::{BypassEvent, EngineStatusEvent};
use crate::hotkey::Registration;
use crate::updater::UpdaterState;

pub struct AppState {
    pub config: SharedConfig,
    pub supervisor: Mutex<Supervisor>,
    pub controls: Arc<SharedControls>,
    pub backend: Arc<dyn AudioBackend>,
    pub endpoint_control: Box<dyn DefaultEndpointControl>,
    pub updater: Mutex<UpdaterState>,
    /// The registered tuning on/off hotkey.
    pub hotkey: Mutex<Registration>,
    pub watcher: Mutex<Option<DeviceWatcher>>,
    /// Set from the device-notification thread; handled by the control loop.
    pub devices_dirty: Arc<AtomicBool>,
    /// Config changed and should be written by the control loop (debounce).
    pub config_dirty: AtomicBool,
    pub log_dir: PathBuf,
    pub first_run_arg: bool,
}

/// Lock ignoring poisoning (a panicked command must not brick the app).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    /// Tuning starts off (bypassed, headphones silent) whatever was saved;
    /// the hotkey, tray or UI turns it on.
    pub fn new(
        mut config: ConfigStore,
        backend: Arc<dyn AudioBackend>,
        log_dir: PathBuf,
        first_run_arg: bool,
    ) -> Self {
        config.config.params.bypass = true;
        let c = &config.config;
        let controls = Arc::new(SharedControls::new(
            &c.params,
            c.monitor_enabled,
            c.monitor_volume,
        ));
        Self {
            supervisor: Mutex::new(Supervisor::new(backend.clone(), controls.clone())),
            config: SharedConfig::new(config),
            controls,
            backend,
            endpoint_control: tuner_win::default_endpoint_control(),
            updater: Mutex::new(UpdaterState::default()),
            hotkey: Mutex::new(Registration::default()),
            watcher: Mutex::new(None),
            devices_dirty: Arc::new(AtomicBool::new(false)),
            config_dirty: AtomicBool::new(false),
            log_dir,
            first_run_arg,
        }
    }

    pub fn config(&self) -> AppConfig {
        self.config.lock().config.clone()
    }

    /// Mutate the config and schedule a save.
    pub fn update_config<R>(&self, f: impl FnOnce(&mut AppConfig) -> R) -> R {
        let r = f(&mut self.config.lock().config);
        self.config_dirty.store(true, Ordering::Release);
        r
    }

    /// Write the config now if it changed.
    pub fn flush_config(&self) {
        if self.config_dirty.swap(false, Ordering::AcqRel) {
            if let Err(e) = self.config.lock().save() {
                tracing::error!("saving config failed: {e}");
            }
        }
    }

    pub fn devices(&self) -> AppResult<Vec<DeviceInfo>> {
        Ok(self.backend.list_devices()?)
    }

    pub fn routing(&self) -> RouteAllApps<'_> {
        RouteAllApps::new(self.endpoint_control.as_ref(), &self.config)
    }

    /// Engine options derived from the config and the live device list.
    pub fn engine_config(&self, devices: &[DeviceInfo]) -> EngineConfig {
        let c = self.config();
        EngineConfig {
            capture_device: c.capture_device,
            monitor_device: c.monitor_device,
            cable_device: if c.virtual_mic_enabled {
                devices
                    .iter()
                    .find(|d| d.is_cable_input())
                    .map(|d| d.id.clone())
            } else {
                None
            },
            allow_exclusive: c.allow_exclusive,
            allow_asio: c.allow_asio,
            ..EngineConfig::default()
        }
    }

    pub fn status_event(&self) -> EngineStatusEvent {
        let sup = lock(&self.supervisor);
        EngineStatusEvent {
            supervisor: sup.status(),
            engine: sup.engine().map(|e| e.status().clone()),
            bypass: self.controls.bypass(),
        }
    }

    pub fn set_bypass(&self, app: &AppHandle, on: bool) {
        self.controls.set_bypass(on);
        self.update_config(|c| c.params.bypass = on);
        let _ = BypassEvent(on).emit(app);
        crate::tray::sync(app, on);
    }
}

pub fn state(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr10_tuning_and_monitoring_start_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ConfigStore::load(dir.path().join("config.json"));
        store.config.params.bypass = false;
        store.config.monitor_enabled = true;
        let s = AppState::new(
            store,
            Arc::new(tuner_audio::mock::MockBackend::with_default_devices()),
            dir.path().into(),
            false,
        );
        assert!(s.controls.bypass());
        assert!(s.config().params.bypass);
        assert_eq!(s.controls.monitor_gain(), 0.0);
        // The hotkey / tray / UI path turns both on.
        s.controls.set_bypass(false);
        assert!(s.controls.monitor_gain() > 0.0);
    }
}
