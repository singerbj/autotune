//! Deterministic synthetic test signals shared by unit tests, benchmarks,
//! golden files and the CLI.

use core::f32::consts::PI;

use crate::rng::Pcg32;

/// Pure sine.
pub fn sine(hz: f32, sample_rate: f32, len: usize, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| amp * (2.0 * PI * hz * i as f32 / sample_rate).sin())
        .collect()
}

/// Band-limited harmonic tone with 1/k amplitudes (a soft sawtooth).
pub fn harmonic(hz: f32, sample_rate: f32, len: usize, amp: f32, harmonics: usize) -> Vec<f32> {
    let nyq = sample_rate * 0.45;
    let mut out = vec![0.0f32; len];
    let mut norm = 0.0;
    for k in 1..=harmonics {
        let fk = hz * k as f32;
        if fk > nyq {
            break;
        }
        norm += 1.0 / k as f32;
        for (i, o) in out.iter_mut().enumerate() {
            let ph = 2.0 * PI * fk * i as f32 / sample_rate;
            *o += ph.sin() / k as f32;
        }
    }
    let g = amp / norm.max(1e-6);
    out.iter_mut().for_each(|o| *o *= g);
    out
}

/// Harmonic tone whose frequency follows `hz_at(t_seconds)`; phase-continuous.
pub fn harmonic_sweep(
    hz_at: impl Fn(f32) -> f32,
    sample_rate: f32,
    len: usize,
    amp: f32,
    harmonics: usize,
) -> Vec<f32> {
    let mut phase = 0.0f64;
    let mut out = Vec::with_capacity(len);
    let norm: f32 = (1..=harmonics).map(|k| 1.0 / k as f32).sum();
    for i in 0..len {
        let f = hz_at(i as f32 / sample_rate);
        phase += f as f64 / sample_rate as f64;
        phase -= phase.floor();
        let mut s = 0.0;
        for k in 1..=harmonics {
            if f * k as f32 > sample_rate * 0.45 {
                break;
            }
            s += (2.0 * PI * (phase as f32) * k as f32).sin() / k as f32;
        }
        out.push(amp * s / norm);
    }
    out
}

/// Seeded white noise in `[-amp, amp)`.
pub fn noise(len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Pcg32::new(seed);
    (0..len).map(|_| amp * rng.bipolar()).collect()
}

/// A vocal-like phrase: glottal pulse train through three formant resonators,
/// with vibrato, glides between notes, and short unvoiced gaps. Frequencies
/// are given as `(start_s, end_s, hz)` note segments.
pub fn vocal_phrase(
    notes: &[(f32, f32, f32)],
    sample_rate: f32,
    len: usize,
    seed: u64,
) -> Vec<f32> {
    let mut rng = Pcg32::new(seed);
    let mut out = vec![0.0f32; len];
    let mut phase = 0.0f32;
    let formants = [(700.0f32, 110.0f32), (1220.0, 120.0), (2600.0, 160.0)];
    let mut res = formants.map(|(f, bw)| Resonator::new(f, bw, sample_rate));
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f32 / sample_rate;
        let seg = notes.iter().find(|(s, e, _)| t >= *s && t < *e);
        let src = match seg {
            Some(&(s, e, hz)) => {
                let vib = 1.0 + 0.006 * (2.0 * PI * 5.5 * t).sin();
                let f = hz * vib;
                phase += f / sample_rate;
                phase -= phase.floor();
                // Rosenberg-ish pulse: raised cosine open phase, closed phase.
                let g = if phase < 0.6 {
                    0.5 * (1.0 - (PI * phase / 0.6).cos())
                } else {
                    (0.5 * PI * (phase - 0.6) / 0.4).cos()
                };
                let fade = ((t - s).min(e - t) / 0.02).min(1.0);
                (g - 0.5) * fade
            }
            None => 0.02 * rng.bipolar(),
        };
        let mut y = 0.0;
        for r in res.iter_mut() {
            y += r.process(src);
        }
        *o = y;
    }
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    out.iter_mut().for_each(|x| *x *= 0.5 / peak);
    out
}

struct Resonator {
    a1: f32,
    a2: f32,
    g: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn new(f: f32, bw: f32, sr: f32) -> Self {
        let r = (-PI * bw / sr).exp();
        let a1 = -2.0 * r * (2.0 * PI * f / sr).cos();
        let a2 = r * r;
        Self {
            a1,
            a2,
            g: 1.0 - r,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.g * x - self.a1 * self.y1 - self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}
