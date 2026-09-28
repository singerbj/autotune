//! A software backend for tests and non-Windows development.
//!
//! Streams run on ordinary threads paced with `sleep`, so this backend is
//! *not* real-time; it exists to exercise the engine's logic: start/stop,
//! rings, resampling, meters, latency measurement (render → capture
//! loopback with a configurable acoustic delay) and device loss.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::backend::{
    AudioBackend, CallbackInfo, CaptureCallback, CaptureRequest, RenderCallback, RenderRequest,
    StreamHandle, StreamState, StreamStatus,
};
use crate::types::{AudioError, BackendTier, DeviceInfo, Direction, StreamInfo};

/// What the mock microphone "hears".
#[derive(Clone, Debug, PartialEq)]
pub enum MockSignal {
    Silence,
    Sine {
        hz: f32,
        amp: f32,
    },
    /// Whatever any mock render stream played, delayed by `delay_frames`.
    Loopback {
        delay_frames: usize,
    },
}

#[derive(Debug)]
struct Shared {
    removed: HashSet<String>,
    signal: MockSignal,
    loopback: VecDeque<f32>,
    /// Last rendered samples per device id (bounded).
    rendered: HashMap<String, VecDeque<f32>>,
    /// Instant of the first non-zero sample rendered per device.
    first_sound: HashMap<String, Instant>,
    fail_exclusive: bool,
}

/// See module docs.
#[derive(Clone, Debug)]
pub struct MockBackend {
    devices: Arc<Mutex<Vec<DeviceInfo>>>,
    shared: Arc<Mutex<Shared>>,
    /// Frames per callback.
    pub period_frames: u32,
    pub sample_rate: u32,
    pub render_rate: u32,
}

const RENDER_HISTORY: usize = 48_000 * 4;

type Spawned = (
    Arc<StreamStatus>,
    Arc<AtomicBool>,
    std::thread::JoinHandle<()>,
);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl MockBackend {
    pub fn new(devices: Vec<DeviceInfo>) -> Self {
        Self {
            devices: Arc::new(Mutex::new(devices)),
            shared: Arc::new(Mutex::new(Shared {
                removed: HashSet::new(),
                signal: MockSignal::Silence,
                loopback: VecDeque::new(),
                rendered: HashMap::new(),
                first_sound: HashMap::new(),
                fail_exclusive: false,
            })),
            period_frames: 480,
            sample_rate: 48_000,
            render_rate: 48_000,
        }
    }

    /// A headset (mic + headphones sharing a container), speakers, and VB-Cable.
    pub fn with_default_devices() -> Self {
        let d = |id: &str, name: &str, dir, def, cable, container: Option<&str>| DeviceInfo {
            id: id.into(),
            name: name.into(),
            direction: dir,
            is_default: def,
            is_default_communications: def,
            is_vb_cable: cable,
            is_bluetooth: false,
            asio_driver: None,
            container_id: container.map(Into::into),
        };
        Self::new(vec![
            d(
                "mic",
                "Headset Microphone (Mock)",
                Direction::Capture,
                true,
                false,
                Some("headset"),
            ),
            d(
                "hp",
                "Headset Earphone (Mock)",
                Direction::Render,
                true,
                false,
                Some("headset"),
            ),
            d(
                "spk",
                "Speakers (Mock)",
                Direction::Render,
                false,
                false,
                None,
            ),
            d(
                "cable-in",
                "CABLE Input (VB-Audio Virtual Cable)",
                Direction::Render,
                false,
                true,
                None,
            ),
            d(
                "cable-out",
                "CABLE Output (VB-Audio Virtual Cable)",
                Direction::Capture,
                false,
                true,
                None,
            ),
        ])
    }

    pub fn set_signal(&self, s: MockSignal) {
        let mut sh = lock(&self.shared);
        sh.loopback.clear();
        sh.signal = s;
    }

    /// Make exclusive mode fail so the fallback chain is exercised.
    pub fn set_fail_exclusive(&self, fail: bool) {
        lock(&self.shared).fail_exclusive = fail;
    }

    /// Simulate unplugging: running streams on it fail with `DeviceLost`.
    pub fn unplug(&self, id: &str) {
        lock(&self.shared).removed.insert(id.to_string());
    }

    pub fn replug(&self, id: &str) {
        lock(&self.shared).removed.remove(id);
    }

    pub fn rendered(&self, id: &str) -> Vec<f32> {
        lock(&self.shared)
            .rendered
            .get(id)
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn first_sound(&self, id: &str) -> Option<Instant> {
        lock(&self.shared).first_sound.get(id).copied()
    }

    pub fn clear_rendered(&self) {
        let mut sh = lock(&self.shared);
        sh.rendered.clear();
        sh.first_sound.clear();
    }

    fn resolve(&self, id: Option<&str>, dir: Direction) -> Result<DeviceInfo, AudioError> {
        let removed = lock(&self.shared).removed.clone();
        let devices = lock(&self.devices);
        let alive = |d: &&DeviceInfo| d.direction == dir && !removed.contains(&d.id);
        match id {
            Some(id) => devices
                .iter()
                .filter(alive)
                .find(|d| d.id == id)
                .cloned()
                .ok_or_else(|| AudioError::DeviceNotFound(id.to_string())),
            None => devices
                .iter()
                .filter(alive)
                .find(|d| d.is_default)
                .or_else(|| {
                    devices
                        .iter()
                        .find(|d| d.direction == dir && !removed.contains(&d.id))
                })
                .cloned()
                .ok_or(AudioError::NoDevice),
        }
    }

    fn spawn<F>(
        &self,
        name: &str,
        dev: &DeviceInfo,
        rate: u32,
        mut body: F,
    ) -> Result<Spawned, AudioError>
    where
        F: FnMut(&mut [f32], &CallbackInfo) + Send + 'static,
    {
        let status = Arc::new(StreamStatus::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (st, sp, shared) = (status.clone(), stop.clone(), self.shared.clone());
        let id = dev.id.clone();
        let period = self.period_frames as usize;
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut buf = vec![0.0f32; period];
                let dur = Duration::from_secs_f64(period as f64 / f64::from(rate));
                let mut next = Instant::now();
                st.set_state(StreamState::Running);
                let info = CallbackInfo {
                    sample_rate: rate,
                    discontinuity: false,
                };
                while !sp.load(Ordering::Acquire) {
                    if lock(&shared).removed.contains(&id) {
                        st.set_state(StreamState::DeviceLost);
                        return;
                    }
                    body(&mut buf, &info);
                    st.tick();
                    next += dur;
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    } else if now - next > Duration::from_millis(100) {
                        // Far behind (debugger, overloaded CI): resync
                        // instead of bursting. Small lags are caught up so
                        // the average rate stays exact.
                        next = now;
                    }
                }
            })
            .map_err(|e| AudioError::Thread(e.to_string()))?;
        Ok((status, stop, thread))
    }
}

impl AudioBackend for MockBackend {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, AudioError> {
        let removed = lock(&self.shared).removed.clone();
        Ok(lock(&self.devices)
            .iter()
            .filter(|d| !removed.contains(&d.id))
            .cloned()
            .collect())
    }

    fn open_capture(
        &self,
        req: &CaptureRequest,
        mut cb: Box<dyn CaptureCallback>,
    ) -> Result<StreamHandle, AudioError> {
        let dev = self.resolve(req.device_id.as_deref(), Direction::Capture)?;
        let fail_excl = lock(&self.shared).fail_exclusive;
        let mut fallbacks = Vec::new();
        let tier = if req.allow_exclusive && !fail_excl {
            BackendTier::WasapiExclusive
        } else {
            if req.allow_exclusive {
                fallbacks.push(crate::TierAttempt {
                    tier: BackendTier::WasapiExclusive,
                    error: AudioError::DeviceInUse.to_string(),
                });
            }
            BackendTier::WasapiShared
        };
        let rate = self.sample_rate;
        let shared = self.shared.clone();
        let mut phase = 0.0f64;
        // The "air" delay lives on the capture side so it is independent of
        // when the render thread starts.
        let mut air: VecDeque<f32> = VecDeque::new();
        let mut air_primed = false;
        let (status, stop, thread) = self.spawn("mock-capture", &dev, rate, move |buf, info| {
            {
                let mut sh = lock(&shared);
                match sh.signal.clone() {
                    MockSignal::Silence => buf.fill(0.0),
                    MockSignal::Sine { hz, amp } => {
                        for s in buf.iter_mut() {
                            phase += f64::from(hz) / f64::from(rate);
                            *s = amp * (2.0 * std::f64::consts::PI * phase).sin() as f32;
                        }
                    }
                    MockSignal::Loopback { delay_frames } => {
                        if !air_primed {
                            air.extend(std::iter::repeat_n(0.0, delay_frames));
                            air_primed = true;
                        }
                        for s in buf.iter_mut() {
                            air.push_back(sh.loopback.pop_front().unwrap_or(0.0));
                            *s = air.pop_front().unwrap_or(0.0);
                        }
                    }
                }
            }
            cb.on_capture(buf, info);
        })?;
        let info = StreamInfo {
            device_id: dev.id,
            device_name: dev.name,
            tier,
            sample_rate: rate,
            channels: 1,
            period_frames: self.period_frames,
            stream_latency_frames: 0,
            fallbacks,
        };
        Ok(StreamHandle::new(info, status, stop, thread))
    }

    fn open_render(
        &self,
        req: &RenderRequest,
        mut cb: Box<dyn RenderCallback>,
    ) -> Result<StreamHandle, AudioError> {
        let dev = self.resolve(req.device_id.as_deref(), Direction::Render)?;
        let rate = self.render_rate;
        let shared = self.shared.clone();
        let id = dev.id.clone();
        let (status, stop, thread) = self.spawn("mock-render", &dev, rate, move |buf, info| {
            cb.on_render(buf, info);
            let mut sh = lock(&shared);
            if matches!(sh.signal, MockSignal::Loopback { .. }) && !id.starts_with("cable") {
                sh.loopback.extend(buf.iter().copied());
            }
            if buf.iter().any(|v| *v != 0.0) {
                sh.first_sound
                    .entry(id.clone())
                    .or_insert_with(Instant::now);
            }
            let hist = sh.rendered.entry(id.clone()).or_default();
            hist.extend(buf.iter().copied());
            while hist.len() > RENDER_HISTORY {
                hist.pop_front();
            }
        })?;
        let info = StreamInfo {
            device_id: dev.id,
            device_name: dev.name,
            tier: BackendTier::WasapiSharedLowLatency,
            sample_rate: rate,
            channels: 2,
            period_frames: self.period_frames,
            stream_latency_frames: 0,
            fallbacks: vec![],
        };
        Ok(StreamHandle::new(info, status, stop, thread))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tone;
    impl RenderCallback for Tone {
        fn on_render(&mut self, mono: &mut [f32], _: &CallbackInfo) {
            mono.fill(0.25);
        }
    }

    #[test]
    fn lists_devices_and_hides_unplugged() {
        let b = MockBackend::with_default_devices();
        assert_eq!(b.list_devices().unwrap().len(), 5);
        b.unplug("mic");
        assert_eq!(b.list_devices().unwrap().len(), 4);
    }

    #[test]
    fn capture_delivers_audio_and_detects_unplug() {
        let b = MockBackend::with_default_devices();
        b.set_signal(MockSignal::Sine {
            hz: 440.0,
            amp: 0.5,
        });
        let got = Arc::new(Mutex::new(0usize));
        let g = got.clone();
        let h = b
            .open_capture(
                &CaptureRequest::default(),
                Box::new(move |x: &[f32], _: &CallbackInfo| {
                    if x.iter().any(|v| *v != 0.0) {
                        *lock(&g) += 1;
                    }
                }),
            )
            .unwrap();
        assert_eq!(h.info().tier, BackendTier::WasapiExclusive);
        std::thread::sleep(Duration::from_millis(60));
        assert!(*lock(&got) > 0);
        b.unplug("mic");
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(h.status().state(), StreamState::DeviceLost);
    }

    #[test]
    fn fr02_mock_falls_back_when_exclusive_fails() {
        let b = MockBackend::with_default_devices();
        b.set_fail_exclusive(true);
        let h = b
            .open_capture(
                &CaptureRequest::default(),
                Box::new(|_: &[f32], _: &CallbackInfo| {}),
            )
            .unwrap();
        assert_eq!(h.info().tier, BackendTier::WasapiShared);
        assert_eq!(h.info().fallbacks.len(), 1);
    }

    #[test]
    fn render_records_output() {
        let b = MockBackend::with_default_devices();
        let h = b
            .open_render(
                &RenderRequest {
                    device_id: Some("spk".into()),
                    ..Default::default()
                },
                Box::new(Tone),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(40));
        h.stop();
        assert!(b.rendered("spk").iter().all(|v| *v == 0.25));
        assert!(b.first_sound("spk").is_some());
        assert!(matches!(
            b.open_render(
                &RenderRequest {
                    device_id: Some("nope".into()),
                    ..Default::default()
                },
                Box::new(Tone)
            ),
            Err(AudioError::DeviceNotFound(_))
        ));
    }
}
