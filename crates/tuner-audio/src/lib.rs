//! `tuner-audio`: device I/O below the engine.
//!
//! * [`AudioBackend`] hides the OS API. [`WasapiBackend`] (Windows) is the
//!   dependable default; ASIO is an automatic upgrade behind the `asio`
//!   feature. [`MockBackend`] drives tests on any OS.
//! * Each opened stream runs on its own thread which the backend registers
//!   with MMCSS and which only waits on its own device event.
//! * [`resample::AdaptiveResampler`] bridges two device clocks with a PI loop
//!   on ring fill.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod backend;
pub mod mock;
pub mod resample;
mod types;

#[cfg(all(windows, feature = "asio"))]
pub mod asio;
#[cfg(windows)]
pub mod wasapi;

pub use backend::{
    open_with_fallback, AudioBackend, CallbackInfo, CaptureCallback, CaptureRequest,
    RenderCallback, RenderRequest, StreamHandle, StreamState, StreamStatus, ThreadPriority,
};
pub use types::{
    is_vb_cable_name, AudioError, BackendTier, DeviceInfo, Direction, StreamInfo, TierAttempt,
};

/// Re-exported so the engine uses exactly the same ring type.
pub use rtrb;

/// The platform's default backend: WASAPI (with ASIO when compiled in) on
/// Windows, otherwise the mock backend (useful for UI development).
pub fn default_backend() -> std::sync::Arc<dyn AudioBackend> {
    #[cfg(windows)]
    {
        std::sync::Arc::new(wasapi::WasapiBackend::new())
    }
    #[cfg(not(windows))]
    {
        std::sync::Arc::new(mock::MockBackend::with_default_devices())
    }
}
