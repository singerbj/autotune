//! Acoustic loopback latency measurement (FR-16 "Measure").
//!
//! The capture thread plays a short log chirp through the monitor path with
//! the DSP bypassed while recording the microphone; the control plane then
//! cross-correlates the recording with the chirp.

use realfft::num_complex::Complex;
use realfft::RealFftPlanner;

/// Chirp length and recording window.
pub(crate) const CHIRP_S: f32 = 0.1;
pub(crate) const RECORD_S: f32 = 1.0;
const CHIRP_AMP: f32 = 0.3;

/// Hann-faded logarithmic sweep 200 Hz → 8 kHz.
pub fn chirp(sample_rate: u32) -> Vec<f32> {
    let sr = sample_rate as f32;
    let n = (sr * CHIRP_S) as usize;
    let (f0, f1) = (200.0f32, 8_000.0f32.min(sr * 0.45));
    let k = (f1 / f0).ln();
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let phase =
                2.0 * core::f32::consts::PI * f0 * CHIRP_S / k * ((t / CHIRP_S * k).exp() - 1.0);
            let w = 0.5 - 0.5 * (2.0 * core::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
            CHIRP_AMP * w * phase.sin()
        })
        .collect()
}

/// A latency test in flight: moved to the capture thread and back as a Box
/// (allocated and freed on the control plane only).
#[derive(Debug)]
pub(crate) struct LatencyProbe {
    pub chirp: Vec<f32>,
    pub recorded: Vec<f32>,
    pub played: usize,
    pub filled: usize,
}

impl LatencyProbe {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self {
            chirp: chirp(sample_rate),
            recorded: vec![0.0; (sample_rate as f32 * RECORD_S) as usize],
            played: 0,
            filled: 0,
        }
    }

    pub(crate) fn done(&self) -> bool {
        self.filled >= self.recorded.len()
    }

    /// Capture-thread step: record `input`, write the next chirp samples to
    /// `out`. Real-time safe.
    pub(crate) fn step(&mut self, input: &[f32], out: &mut [f32]) {
        let n = input.len().min(out.len());
        let rec = (self.recorded.len() - self.filled).min(n);
        self.recorded[self.filled..self.filled + rec].copy_from_slice(&input[..rec]);
        self.filled += rec;
        for o in out[..n].iter_mut() {
            *o = self.chirp.get(self.played).copied().unwrap_or(0.0);
            self.played += 1;
        }
    }
}

/// Result of a loopback measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LatencyResult {
    /// Hardware + buffering round trip (DSP bypassed), ms.
    pub hardware_ms: f32,
    /// `hardware_ms` plus the DSP's algorithmic latency, ms.
    pub total_ms: f32,
    /// Normalised correlation peak, 0…1.
    pub confidence: f32,
}

/// Find the delay of `chirp` inside `recorded`. Returns `(lag_samples,
/// normalised_peak)`, or `None` when there is no convincing peak.
pub fn analyze_loopback(recorded: &[f32], chirp: &[f32]) -> Option<(usize, f32)> {
    if recorded.len() < chirp.len() || chirp.is_empty() {
        return None;
    }
    let n = (recorded.len() + chirp.len()).next_power_of_two();
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);
    let mut a = vec![0.0f32; n];
    a[..recorded.len()].copy_from_slice(recorded);
    let mut b = vec![0.0f32; n];
    b[..chirp.len()].copy_from_slice(chirp);
    let mut fa = fwd.make_output_vec();
    let mut fb = fwd.make_output_vec();
    fwd.process(&mut a, &mut fa).ok()?;
    fwd.process(&mut b, &mut fb).ok()?;
    for (x, y) in fa.iter_mut().zip(&fb) {
        *x *= y.conj();
    }
    if let Some(f) = fa.first_mut() {
        *f = Complex::new(f.re, 0.0);
    }
    if let Some(l) = fa.last_mut() {
        *l = Complex::new(l.re, 0.0);
    }
    let mut xc = vec![0.0f32; n];
    inv.process(&mut fa, &mut xc).ok()?;

    let max_lag = recorded.len() - chirp.len();
    let (lag, peak) = xc[..=max_lag]
        .iter()
        .enumerate()
        .fold((0, f32::MIN), |b, (i, &v)| if v > b.1 { (i, v) } else { b });
    let peak = peak / n as f32;
    let chirp_energy: f32 = chirp.iter().map(|x| x * x).sum();
    let seg_energy: f32 = recorded[lag..lag + chirp.len()].iter().map(|x| x * x).sum();
    let norm = (chirp_energy * seg_energy).sqrt();
    if norm <= 1e-12 {
        return None;
    }
    let coeff = peak / norm;
    (coeff > 0.2).then_some((lag, coeff.min(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr16_finds_known_delay_in_noise() {
        let sr = 48_000;
        let c = chirp(sr);
        let mut rec = tuner_dsp::signals::noise(48_000, 0.02, 3);
        let delay = 913;
        for (i, &s) in c.iter().enumerate() {
            rec[delay + i] += 0.4 * s;
        }
        let (lag, conf) = analyze_loopback(&rec, &c).unwrap();
        assert_eq!(lag, delay);
        assert!(conf > 0.5, "{conf}");
    }

    #[test]
    fn fr16_rejects_silence_and_noise() {
        let c = chirp(48_000);
        assert!(analyze_loopback(&vec![0.0; 48_000], &c).is_none());
        let noise = tuner_dsp::signals::noise(48_000, 0.3, 8);
        assert!(analyze_loopback(&noise, &c).is_none());
    }

    #[test]
    fn probe_plays_chirp_and_records() {
        let mut p = LatencyProbe::new(1000);
        let input = vec![0.5f32; 300];
        let mut out = vec![0.0f32; 300];
        p.step(&input, &mut out);
        assert_eq!(p.filled, 300);
        assert_eq!(&out[..100], &p.chirp[..]);
        assert!(out[100..].iter().all(|&v| v == 0.0));
        for _ in 0..3 {
            p.step(&input, &mut out);
        }
        assert!(p.done());
    }
}
