//! Allocation-free McLeod Pitch Method (MPM).
//!
//! Vendored-in-spirit from the `pitch-detection` crate's McLeod detector, but
//! reworked so that every buffer and FFT plan is created up front and
//! [`PitchDetector::detect`] never allocates (see ADR 0002).

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Key maxima at or above `K × highest` are candidates; the first wins.
const MPM_K: f32 = 0.88;
const MAX_KEY_MAXIMA: usize = 64;

/// One pitch estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchEstimate {
    pub hz: f32,
    /// Normalised peak height of the NSDF, 0…1. Higher means more periodic.
    pub clarity: f32,
}

/// McLeod pitch detector with preallocated FFT buffers.
pub struct PitchDetector {
    sample_rate: f32,
    max_window: usize,
    fft_len: usize,
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
    time: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch_fwd: Vec<Complex<f32>>,
    scratch_inv: Vec<Complex<f32>>,
    nsdf: Vec<f32>,
}

impl core::fmt::Debug for PitchDetector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PitchDetector")
            .field("sample_rate", &self.sample_rate)
            .field("max_window", &self.max_window)
            .field("fft_len", &self.fft_len)
            .finish()
    }
}

impl PitchDetector {
    /// `max_window` is the largest analysis window that will ever be passed to
    /// [`detect`](Self::detect).
    pub fn new(sample_rate: f32, max_window: usize) -> Self {
        let fft_len = (2 * max_window).next_power_of_two();
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(fft_len);
        let c2r = planner.plan_fft_inverse(fft_len);
        let scratch_fwd = r2c.make_scratch_vec();
        let scratch_inv = c2r.make_scratch_vec();
        Self {
            sample_rate,
            max_window,
            fft_len,
            time: vec![0.0; fft_len],
            frame: vec![0.0; max_window],
            spectrum: r2c.make_output_vec(),
            scratch_fwd,
            scratch_inv,
            nsdf: vec![0.0; max_window],
            r2c,
            c2r,
        }
    }

    pub fn max_window(&self) -> usize {
        self.max_window
    }

    /// Detect the pitch of `a ++ b` (two slices so ring buffers can be passed
    /// without copying). Returns `None` for silence or when no periodicity is
    /// found between `min_hz` and `max_hz`.
    pub fn detect(
        &mut self,
        a: &[f32],
        b: &[f32],
        min_hz: f32,
        max_hz: f32,
    ) -> Option<PitchEstimate> {
        let w = (a.len() + b.len()).min(self.max_window);
        if w < 8 {
            return None;
        }
        let a_used = a.len().min(w);
        self.frame[..a_used].copy_from_slice(&a[..a_used]);
        self.frame[a_used..w].copy_from_slice(&b[..w - a_used]);
        let frame = &self.frame[..w];

        // Autocorrelation r(τ) via FFT (zero padded to avoid circular wrap).
        self.time[..w].copy_from_slice(frame);
        self.time[w..].fill(0.0);
        self.r2c
            .process_with_scratch(&mut self.time, &mut self.spectrum, &mut self.scratch_fwd)
            .ok()?;
        for c in self.spectrum.iter_mut() {
            *c = Complex::new(c.norm_sqr(), 0.0);
        }
        self.c2r
            .process_with_scratch(&mut self.spectrum, &mut self.time, &mut self.scratch_inv)
            .ok()?;
        let norm = 1.0 / self.fft_len as f32;

        // m(τ) = Σ x_j² + x_{j+τ}², updated incrementally.
        let energy: f32 = frame.iter().map(|x| x * x).sum();
        if energy < 1e-9 {
            return None;
        }
        let tau_max = ((self.sample_rate / min_hz).ceil() as usize + 2).min(w - 2);
        let tau_min = ((self.sample_rate / max_hz).floor() as usize).max(1);
        let mut m = 2.0 * energy;
        for tau in 0..=tau_max {
            if tau > 0 {
                m -= frame[tau - 1] * frame[tau - 1] + frame[w - tau] * frame[w - tau];
            }
            let r = self.time[tau] * norm;
            self.nsdf[tau] = if m > 1e-12 { 2.0 * r / m } else { 0.0 };
        }
        let nsdf = &self.nsdf[..=tau_max];

        // Key maxima: highest point between each positive-going and the next
        // negative-going zero crossing, after the first negative region.
        let mut keys = [(0usize, 0.0f32); MAX_KEY_MAXIMA];
        let mut n_keys = 0;
        let mut tau = 1;
        while tau < tau_max && nsdf[tau] > 0.0 {
            tau += 1;
        }
        while tau < tau_max && nsdf[tau] <= 0.0 {
            tau += 1;
        }
        let mut best: Option<(usize, f32)> = None;
        while tau < tau_max {
            if nsdf[tau] > 0.0 {
                if best.is_none_or(|(_, v)| nsdf[tau] > v) {
                    best = Some((tau, nsdf[tau]));
                }
            } else if let Some(bk) = best.take() {
                if n_keys < MAX_KEY_MAXIMA {
                    keys[n_keys] = bk;
                    n_keys += 1;
                }
            }
            tau += 1;
        }
        // A lobe still open at tau_max counts only if its peak is interior.
        if let Some(bk) = best {
            if bk.0 + 1 < tau_max && n_keys < MAX_KEY_MAXIMA {
                keys[n_keys] = bk;
                n_keys += 1;
            }
        }
        let keys = &keys[..n_keys];
        let highest = keys
            .iter()
            .filter(|k| k.0 >= tau_min)
            .fold(0.0f32, |h, k| h.max(k.1));
        if highest <= 0.0 {
            return None;
        }
        let threshold = MPM_K * highest;
        let &(t, _) = keys.iter().find(|k| k.0 >= tau_min && k.1 >= threshold)?;

        // Parabolic interpolation around the chosen peak.
        let (y0, y1, y2) = (nsdf[t - 1], nsdf[t], nsdf[t + 1]);
        let denom = y0 - 2.0 * y1 + y2;
        let (delta, peak) = if denom.abs() > 1e-12 {
            let d = (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5);
            (d, y1 - 0.25 * (y0 - y2) * d)
        } else {
            (0.0, y1)
        };
        let period = t as f32 + delta;
        let hz = self.sample_rate / period;
        if !(min_hz * 0.97..=max_hz * 1.03).contains(&hz) {
            return None;
        }
        Some(PitchEstimate {
            hz,
            clarity: peak.clamp(0.0, 1.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals;

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn nfr06_detects_sines_within_5_cents() {
        let sr = 48_000.0;
        let mut det = PitchDetector::new(sr, 2048);
        for &f in &[72.0f32, 98.0, 110.0, 196.0, 261.63, 440.0, 659.25, 880.0] {
            let x = signals::sine(f, sr, 2048, 0.5);
            let e = det.detect(&x, &[], 70.0, 1000.0).expect("pitch");
            assert!(cents(e.hz, f).abs() < 5.0, "{f} Hz → {} Hz", e.hz);
            assert!(e.clarity > 0.9);
        }
    }

    #[test]
    fn nfr06_detects_harmonic_tones_without_octave_errors() {
        let sr = 48_000.0;
        let mut det = PitchDetector::new(sr, 1372);
        for &f in &[82.41f32, 110.0, 220.0, 246.94, 392.0] {
            let x = signals::harmonic(f, sr, 1372, 0.5, 12);
            let e = det.detect(&x, &[], 70.0, 1000.0).expect("pitch");
            assert!(cents(e.hz, f).abs() < 5.0, "{f} Hz → {} Hz", e.hz);
        }
    }

    #[test]
    fn split_input_equals_contiguous() {
        let sr = 44_100.0;
        let mut det = PitchDetector::new(sr, 1024);
        let x = signals::harmonic(300.0, sr, 1024, 0.4, 6);
        let a = det.detect(&x, &[], 70.0, 1000.0);
        let b = det.detect(&x[..300], &x[300..], 70.0, 1000.0);
        assert_eq!(a, b);
    }

    #[test]
    fn silence_and_noise_have_no_confident_pitch() {
        let sr = 48_000.0;
        let mut det = PitchDetector::new(sr, 2048);
        assert!(det.detect(&[0.0; 2048], &[], 70.0, 1000.0).is_none());
        let noise = signals::noise(2048, 0.3, 1);
        if let Some(e) = det.detect(&noise, &[], 70.0, 1000.0) {
            assert!(e.clarity < 0.6, "noise clarity {}", e.clarity);
        }
    }
}
