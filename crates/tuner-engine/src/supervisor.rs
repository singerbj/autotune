//! Device lifecycle (FR-05): rebuild streams from the control plane on
//! device loss, fall back to another device when the chosen one is gone,
//! and switch back when it returns — all within 2 s, without a restart.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tuner_audio::{AudioBackend, AudioError, DeviceInfo, Direction};

use crate::controls::SharedControls;
use crate::engine::{Engine, EngineConfig, EngineHealth};
use crate::EngineError;

/// Delay between failed start attempts.
const RETRY: Duration = Duration::from_millis(400);
/// Coalesce bursts of device notifications.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// While on a fallback device, look for the preferred one this often.
const PREFERRED_POLL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SupervisorState {
    Stopped,
    Running,
    /// Wants to run but no stream could be opened yet.
    Recovering,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SupervisorStatus {
    pub state: SupervisorState,
    pub last_error: Option<String>,
    pub restarts: u32,
    /// True while running on a substitute because the chosen device is gone.
    pub using_fallback_device: bool,
}

/// Owns the engine on behalf of the control plane.
pub struct Supervisor {
    backend: Arc<dyn AudioBackend>,
    controls: Arc<SharedControls>,
    desired: EngineConfig,
    engine: Option<Engine>,
    want_running: bool,
    next_attempt: Option<Instant>,
    last_preferred_check: Instant,
    last_error: Option<String>,
    restarts: u32,
    started_once: bool,
    fallback: bool,
}

impl core::fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Supervisor")
            .field("desired", &self.desired)
            .field("running", &self.engine.is_some())
            .finish_non_exhaustive()
    }
}

fn exists(devices: &[DeviceInfo], id: &Option<String>, dir: Direction) -> bool {
    match id {
        None => true,
        Some(id) => devices.iter().any(|d| &d.id == id && d.direction == dir),
    }
}

/// Default (or first) real device for `dir`, never VB-Cable: falling back
/// to CABLE Output as a microphone would create a feedback loop.
fn substitute(devices: &[DeviceInfo], dir: Direction) -> Option<String> {
    let real = || {
        devices
            .iter()
            .filter(move |d| d.direction == dir && !d.is_vb_cable)
    };
    real()
        .find(|d| d.is_default)
        .or_else(|| real().next())
        .map(|d| d.id.clone())
}

impl Supervisor {
    pub fn new(backend: Arc<dyn AudioBackend>, controls: Arc<SharedControls>) -> Self {
        Self {
            backend,
            controls,
            desired: EngineConfig::default(),
            engine: None,
            want_running: false,
            next_attempt: None,
            last_preferred_check: Instant::now(),
            last_error: None,
            restarts: 0,
            started_once: false,
            fallback: false,
        }
    }

    pub fn backend(&self) -> &Arc<dyn AudioBackend> {
        &self.backend
    }

    pub fn controls(&self) -> &Arc<SharedControls> {
        &self.controls
    }

    pub fn desired(&self) -> &EngineConfig {
        &self.desired
    }

    pub fn engine(&self) -> Option<&Engine> {
        self.engine.as_ref()
    }

    pub fn engine_mut(&mut self) -> Option<&mut Engine> {
        self.engine.as_mut()
    }

    pub fn status(&self) -> SupervisorStatus {
        SupervisorStatus {
            state: match (self.want_running, self.engine.is_some()) {
                (false, _) => SupervisorState::Stopped,
                (true, true) => SupervisorState::Running,
                (true, false) => SupervisorState::Recovering,
            },
            last_error: self.last_error.clone(),
            restarts: self.restarts,
            using_fallback_device: self.fallback,
        }
    }

    /// Start (or restart with a new config). Errors are also kept in
    /// `status().last_error`; the supervisor keeps retrying.
    pub fn start(&mut self, config: EngineConfig) -> Result<(), EngineError> {
        self.desired = config;
        self.want_running = true;
        self.started_once = false;
        if let Some(e) = self.engine.take() {
            e.stop();
        }
        self.try_start()
    }

    pub fn stop(&mut self) {
        self.want_running = false;
        self.next_attempt = None;
        if let Some(e) = self.engine.take() {
            e.stop();
        }
    }

    /// Replace the config; restarts the engine if it is running.
    pub fn set_config(&mut self, config: EngineConfig) -> Result<(), EngineError> {
        if config == self.desired && self.engine.is_some() {
            return Ok(());
        }
        if self.want_running {
            self.start(config)
        } else {
            self.desired = config;
            Ok(())
        }
    }

    /// A device was added/removed or a default changed: re-evaluate soon.
    pub fn on_device_change(&mut self) {
        if self.want_running {
            let at = Instant::now() + DEBOUNCE;
            if self.engine.is_none() || self.fallback {
                self.next_attempt = Some(self.next_attempt.map_or(at, |t| t.min(at)));
            }
            self.last_preferred_check = Instant::now() - PREFERRED_POLL;
        }
    }

    /// Drive recovery. Call regularly (the meter loop runs it at 30 Hz).
    /// Returns true when the engine was (re)started or lost this tick.
    pub fn tick(&mut self) -> bool {
        if !self.want_running {
            return false;
        }
        let now = Instant::now();
        let mut changed = false;
        if let Some(engine) = &self.engine {
            let health = engine.health();
            let preferred_back = self.fallback
                && now.duration_since(self.last_preferred_check) >= PREFERRED_POLL
                && {
                    self.last_preferred_check = now;
                    self.preferred_available()
                };
            if health != EngineHealth::Healthy || preferred_back {
                self.last_error = match health {
                    EngineHealth::Healthy => None,
                    EngineHealth::Failed(m) => Some(m),
                    other => Some(format!("{other:?}")),
                };
                if let Some(e) = self.engine.take() {
                    e.stop();
                }
                self.next_attempt = Some(now);
                changed = true;
            }
        }
        if self.engine.is_none() && self.next_attempt.is_none_or(|t| now >= t) {
            let _ = self.try_start();
            changed = true;
        }
        changed
    }

    fn preferred_available(&self) -> bool {
        match self.backend.list_devices() {
            Ok(d) => {
                exists(&d, &self.desired.capture_device, Direction::Capture)
                    && exists(&d, &self.desired.monitor_device, Direction::Render)
            }
            Err(_) => false,
        }
    }

    fn try_start(&mut self) -> Result<(), EngineError> {
        let devices = self.backend.list_devices().unwrap_or_default();
        let mut cfg = self.desired.clone();
        let mut fallback = false;
        if !exists(&devices, &cfg.capture_device, Direction::Capture) {
            cfg.capture_device = substitute(&devices, Direction::Capture);
            fallback = true;
        }
        if !exists(&devices, &cfg.monitor_device, Direction::Render) {
            cfg.monitor_device = substitute(&devices, Direction::Render);
            fallback = true;
        }
        if !exists(&devices, &cfg.cable_device, Direction::Render) {
            cfg.cable_device = None;
        }
        let result = if fallback && cfg.capture_device.is_none() {
            Err(EngineError::NoCaptureDevice)
        } else {
            Engine::start(self.backend.as_ref(), &cfg, self.controls.clone())
        };
        match result {
            Ok(engine) => {
                if self.started_once {
                    self.restarts += 1;
                }
                self.started_once = true;
                self.engine = Some(engine);
                self.fallback = fallback;
                self.next_attempt = None;
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                self.next_attempt = Some(Instant::now() + RETRY);
                Err(match e {
                    EngineError::Audio(AudioError::DeviceNotFound(_)) => {
                        EngineError::NoCaptureDevice
                    }
                    other => other,
                })
            }
        }
    }
}
