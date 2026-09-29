//! The backend trait, stream callbacks and stream handles.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::types::{AudioError, BackendTier, DeviceInfo, StreamInfo, TierAttempt};

/// Per-callback facts from the stream thread.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CallbackInfo {
    pub sample_rate: u32,
    /// The device reported a gap before this buffer (an xrun).
    pub discontinuity: bool,
}

/// Receives mono capture buffers on the capture thread.
///
/// Real-time rules apply: no allocation, locks, logging, I/O or blocking.
pub trait CaptureCallback: Send + 'static {
    fn on_capture(&mut self, mono: &[f32], info: &CallbackInfo);
}

/// Fills mono render buffers on a render thread. Same real-time rules.
pub trait RenderCallback: Send + 'static {
    fn on_render(&mut self, mono: &mut [f32], info: &CallbackInfo);
}

impl<F: FnMut(&[f32], &CallbackInfo) + Send + 'static> CaptureCallback for F {
    fn on_capture(&mut self, mono: &[f32], info: &CallbackInfo) {
        self(mono, info)
    }
}

/// MMCSS priority for a stream thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadPriority {
    /// "Pro Audio", critical: capture + DSP and monitor render.
    Critical,
    /// "Pro Audio", high: cable render.
    High,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRequest {
    /// `None` opens the default console capture device.
    pub device_id: Option<String>,
    /// Preferred engine rate; exclusive mode tries it first.
    pub preferred_rate: u32,
    pub allow_exclusive: bool,
    pub allow_asio: bool,
    pub priority: ThreadPriority,
}

impl Default for CaptureRequest {
    fn default() -> Self {
        Self {
            device_id: None,
            preferred_rate: 48_000,
            allow_exclusive: true,
            allow_asio: true,
            priority: ThreadPriority::Critical,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderRequest {
    /// `None` opens the default console render device.
    pub device_id: Option<String>,
    /// Use `IAudioClient3` minimum-period shared mode when available.
    pub low_latency: bool,
    pub priority: ThreadPriority,
}

impl Default for RenderRequest {
    fn default() -> Self {
        Self {
            device_id: None,
            low_latency: true,
            priority: ThreadPriority::Critical,
        }
    }
}

/// An audio API. Implementations must be cheap to share across threads.
pub trait AudioBackend: Send + Sync {
    fn name(&self) -> &'static str;
    /// All active capture and render endpoints (FR-01).
    fn list_devices(&self) -> Result<Vec<DeviceInfo>, AudioError>;
    /// Open a capture stream through the fallback chain (FR-02).
    fn open_capture(
        &self,
        req: &CaptureRequest,
        cb: Box<dyn CaptureCallback>,
    ) -> Result<StreamHandle, AudioError>;
    /// Open a shared-mode render stream (FR-03, FR-04).
    fn open_render(
        &self,
        req: &RenderRequest,
        cb: Box<dyn RenderCallback>,
    ) -> Result<StreamHandle, AudioError>;
    /// Open the control panel of the active ASIO driver, if any.
    fn open_asio_panel(&self) -> Result<(), AudioError> {
        Err(AudioError::TierUnavailable(
            "ASIO support is not compiled in",
        ))
    }
}

/// Lifecycle of a stream thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamState {
    Starting = 0,
    Running = 1,
    Stopped = 2,
    /// `AUDCLNT_E_DEVICE_INVALIDATED` or equivalent: rebuild from control.
    DeviceLost = 3,
    Failed = 4,
}

/// Lock-free status shared between a stream thread and the control plane.
#[derive(Debug, Default)]
pub struct StreamStatus {
    state: AtomicU8,
    xruns: AtomicU64,
    callbacks: AtomicU64,
    last_error: AtomicU64,
}

impl StreamStatus {
    pub fn state(&self) -> StreamState {
        match self.state.load(Ordering::Acquire) {
            0 => StreamState::Starting,
            1 => StreamState::Running,
            2 => StreamState::Stopped,
            3 => StreamState::DeviceLost,
            _ => StreamState::Failed,
        }
    }

    pub fn set_state(&self, s: StreamState) {
        self.state.store(s as u8, Ordering::Release);
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self.state(), StreamState::Starting | StreamState::Running)
    }

    pub fn add_xrun(&self) {
        self.xruns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn xruns(&self) -> u64 {
        self.xruns.load(Ordering::Relaxed)
    }

    pub fn tick(&self) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    /// Record an OS error code (e.g. HRESULT) for diagnostics.
    pub fn set_error_code(&self, code: u32) {
        self.last_error.store(u64::from(code), Ordering::Relaxed);
    }

    pub fn error_code(&self) -> u32 {
        self.last_error.load(Ordering::Relaxed) as u32
    }
}

/// Owns a running stream thread. Dropping it stops and joins the thread.
#[derive(Debug)]
pub struct StreamHandle {
    info: StreamInfo,
    status: Arc<StreamStatus>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StreamHandle {
    pub fn new(
        info: StreamInfo,
        status: Arc<StreamStatus>,
        stop: Arc<AtomicBool>,
        thread: JoinHandle<()>,
    ) -> Self {
        Self {
            info,
            status,
            stop,
            thread: Some(thread),
        }
    }

    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    pub fn status(&self) -> &Arc<StreamStatus> {
        &self.status
    }

    /// Record a tier that was tried (and failed) before this stream opened.
    pub fn prepend_fallback(&mut self, attempt: TierAttempt) {
        self.info.fallbacks.insert(0, attempt);
    }

    /// Stop the stream and wait for its thread to exit.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if self.status.is_healthy() {
            self.status.set_state(StreamState::Stopped);
        }
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Run a fallback chain: try each tier in order, return the first success
/// and the reasons earlier tiers failed. Callers pass only the tiers that are
/// applicable (e.g. no ASIO when the device has no ASIO driver).
pub fn open_with_fallback<T>(
    tiers: &[BackendTier],
    mut try_open: impl FnMut(BackendTier) -> Result<T, AudioError>,
) -> Result<(T, BackendTier, Vec<TierAttempt>), AudioError> {
    let mut attempts = Vec::new();
    for &tier in tiers {
        match try_open(tier) {
            Ok(v) => return Ok((v, tier, attempts)),
            // Device gone: no point trying other tiers.
            Err(e @ (AudioError::DeviceNotFound(_) | AudioError::NoDevice)) => return Err(e),
            Err(e) => attempts.push(TierAttempt {
                tier,
                error: e.to_string(),
            }),
        }
    }
    Err(AudioError::AllTiersFailed(attempts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr02_fallback_stops_at_first_success() {
        let mut tried = vec![];
        let (v, tier, attempts) = open_with_fallback(&BackendTier::CAPTURE_CHAIN, |t| {
            tried.push(t);
            match t {
                BackendTier::Asio => Err(AudioError::TierUnavailable("no driver")),
                BackendTier::WasapiExclusive => Err(AudioError::DeviceInUse),
                _ => Ok(7),
            }
        })
        .unwrap();
        assert_eq!(v, 7);
        assert_eq!(tier, BackendTier::WasapiSharedLowLatency);
        assert_eq!(tried.len(), 3);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[1].tier, BackendTier::WasapiExclusive);
    }

    #[test]
    fn fr02_fallback_reports_all_failures() {
        let r: Result<(u8, _, _), _> = open_with_fallback(&BackendTier::CAPTURE_CHAIN, |_| {
            Err(AudioError::FormatNotSupported)
        });
        match r {
            Err(AudioError::AllTiersFailed(a)) => assert_eq!(a.len(), 4),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fallback_aborts_when_device_missing() {
        let mut n = 0;
        let r: Result<(u8, _, _), _> = open_with_fallback(&BackendTier::CAPTURE_CHAIN, |_| {
            n += 1;
            Err(AudioError::DeviceNotFound("x".into()))
        });
        assert!(matches!(r, Err(AudioError::DeviceNotFound(_))));
        assert_eq!(n, 1);
    }

    #[test]
    fn status_states() {
        let s = StreamStatus::default();
        assert_eq!(s.state(), StreamState::Starting);
        s.set_state(StreamState::DeviceLost);
        assert!(!s.is_healthy());
        s.add_xrun();
        assert_eq!(s.xruns(), 1);
    }
}
