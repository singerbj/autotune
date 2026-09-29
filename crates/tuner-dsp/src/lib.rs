//! `tuner-dsp`: the offline-testable heart of the voice tuner.
//!
//! Pipeline per block (see `ARCHITECTURE.md` › DSP pipeline):
//! conditioning → McLeod pitch detection → voicing decision → scale snap with
//! hysteresis → retune glide → PSOLA shift → dry/wet mix and soft limiter.
//!
//! Everything is allocated in [`Tuner::new`]; [`Tuner::process`] never
//! allocates, locks or panics, and is deterministic for a given seed.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod filters;
mod mpm;
mod params;
mod psola;
mod rng;
pub mod scale;
pub mod signals;
mod tuner;

pub use mpm::{PitchDetector, PitchEstimate};
pub use params::{Scale, TuningParams, VoiceRange, MAX_SHIFT_SEMITONES};
pub use tuner::{DspMeters, Tuner, TunerConfig};

/// Lowest pitch any preset can track; all buffers are sized for it.
pub const ABS_MIN_PITCH_HZ: f32 = 70.0;
/// Highest fundamental the detector will report.
pub const MAX_PITCH_HZ: f32 = 1000.0;

/// Errors raised while constructing DSP state (never from `process`).
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DspError {
    #[error("unsupported sample rate {0} Hz (expected 8000..=192000)")]
    UnsupportedSampleRate(f32),
}
