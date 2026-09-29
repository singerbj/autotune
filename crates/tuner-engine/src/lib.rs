//! `tuner-engine`: wires `tuner-audio` streams to the `tuner-dsp` tuner.
//!
//! Threads (Architecture › Process and thread model):
//! * capture + DSP — owns the capture stream and the [`tuner_dsp::Tuner`],
//!   writes the monitor and cable rings;
//! * monitor render — reads the monitor ring (direct or resampled);
//! * cable render — reads the cable ring through the adaptive resampler.
//!
//! Control → engine: continuous parameters as atomics ([`SharedControls`]),
//! one-shot objects (processors, latency probes) through `rtrb` queues.
//! Engine → control: meters via a triple buffer, counters via atomics.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod controls;
mod engine;
mod latency;
mod stats;
mod supervisor;

pub use controls::SharedControls;
pub use engine::{Engine, EngineConfig, EngineHealth, EngineStatus, MeterSnapshot};
pub use latency::{analyze_loopback, chirp, LatencyResult};
pub use stats::CallbackStats;
pub use supervisor::{Supervisor, SupervisorState, SupervisorStatus};

pub use tuner_audio;
pub use tuner_dsp;

/// Engine errors; the Tauri layer maps them to user-facing messages.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Audio(#[from] tuner_audio::AudioError),
    #[error(transparent)]
    Dsp(#[from] tuner_dsp::DspError),
    #[error("the engine is not running")]
    NotRunning,
    #[error("a latency test is already running")]
    Busy,
    #[error("the latency test timed out")]
    LatencyTimeout,
    #[error("no loopback signal detected — hold an earcup against the microphone and try again")]
    NoLoopbackSignal,
    #[error("no usable microphone is connected")]
    NoCaptureDevice,
}
