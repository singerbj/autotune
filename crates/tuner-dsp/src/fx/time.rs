//! Time-based sends: a two-voice doubler and a tempo-synced echo.

use super::lines::{DelayLine, QuadOsc};
use crate::filters::{one_pole_coeff, Biquad, Smoother};

/// LFO rate and delay multiple of the two doubler voices; the rates differ
/// so the copies never line up.
const DOUBLER_VOICES: [(f32, f32); 2] = [(0.31, 1.0), (0.43, 1.5)];
const DOUBLER_MAX_S: f32 = 0.070;

#[derive(Debug)]
struct DoublerVoice {
    rate_hz: f32,
    spread: f32,
    lfo: QuadOsc,
    base: Smoother,
    depth: Smoother,
}

/// Two delayed copies whose delay is slowly modulated: a delay changing at
/// rate `dD/dt` shifts pitch by `1 − dD/dt`, so a sine of depth `A` at `f`
/// detunes by up to `2πfA` (that peak is what the cents control sets).
#[derive(Debug)]
pub(crate) struct Doubler {
    sr: f32,
    line: DelayLine,
    voices: [DoublerVoice; 2],
}

impl Doubler {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let mut phase = 0.0;
        let voices = DOUBLER_VOICES.map(|(rate_hz, spread)| {
            // Opposite phases: one copy goes sharp while the other goes flat.
            let v = DoublerVoice {
                rate_hz,
                spread,
                lfo: QuadOsc::new(rate_hz, sample_rate, phase),
                base: Smoother::new(0.0, 0.050, sample_rate),
                depth: Smoother::new(0.0, 0.050, sample_rate),
            };
            phase += 0.5;
            v
        });
        let mut d = Self {
            sr: sample_rate,
            line: DelayLine::new((DOUBLER_MAX_S * sample_rate).ceil() as usize),
            voices,
        };
        d.set(10.0, 18.0);
        for v in &mut d.voices {
            v.base.finish();
            v.depth.finish();
        }
        d
    }

    pub(crate) fn set(&mut self, detune_cents: f32, delay_ms: f32) {
        let ratio = (detune_cents / 1200.0).exp2() - 1.0;
        for v in &mut self.voices {
            let base = delay_ms * 0.001 * self.sr * v.spread;
            let depth = ratio / (2.0 * core::f32::consts::PI * v.rate_hz) * self.sr;
            // Never let the modulated tap reach "now" (≥ 1 ms of delay).
            let depth = depth.min(base - 0.001 * self.sr).max(0.0);
            v.base.set_target(base);
            v.depth.set_target(depth);
        }
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let mut sum = 0.0;
        for v in &mut self.voices {
            let (sin, _) = v.lfo.next();
            let d = v.base.next() + v.depth.next() * sin;
            sum += self.line.tap_frac(d);
        }
        self.line.push(x);
        0.5 * sum
    }

    pub(crate) fn reset(&mut self) {
        self.line.reset();
    }
}

const ECHO_MAX_S: f32 = 1.05;
const ECHO_HP_HZ: f32 = 250.0;
const ECHO_LP_HZ: f32 = 4_500.0;
/// Alternating offset fed into the loop so the filters never hold denormals.
const DENORMAL_GUARD: f32 = 1e-20;

/// Feedback delay with band-limited repeats, so echoes sit behind the voice.
#[derive(Debug)]
pub(crate) struct Echo {
    sr: f32,
    line: DelayLine,
    time: Smoother,
    feedback: f32,
    hp: Biquad,
    lp_coeff: f32,
    lp_state: f32,
    guard: f32,
}

impl Echo {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let mut e = Self {
            sr: sample_rate,
            line: DelayLine::new((ECHO_MAX_S * sample_rate).ceil() as usize),
            // Tempo changes glide like a tape echo instead of clicking.
            time: Smoother::new(0.0, 0.080, sample_rate),
            feedback: 0.0,
            hp: Biquad::highpass(sample_rate, ECHO_HP_HZ, core::f32::consts::FRAC_1_SQRT_2),
            lp_coeff: one_pole_coeff(
                1.0 / (2.0 * core::f32::consts::PI * ECHO_LP_HZ),
                sample_rate,
            ),
            lp_state: 0.0,
            guard: DENORMAL_GUARD,
        };
        e.set(0.25, 0.3);
        e.time.finish();
        e
    }

    pub(crate) fn set(&mut self, seconds: f32, feedback: f32) {
        self.time
            .set_target((seconds * self.sr).clamp(1.0, self.line.max_delay()));
        self.feedback = feedback.clamp(0.0, 0.95);
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = self.line.tap_frac(self.time.next());
        let h = self.hp.process(y);
        self.lp_state = h + (self.lp_state - h) * self.lp_coeff;
        let wet = self.lp_state;
        self.guard = -self.guard;
        self.line.push(x + self.feedback * wet + self.guard);
        wet
    }

    pub(crate) fn reset(&mut self) {
        self.line.reset();
        self.hp.reset();
        self.lp_state = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scale::hz_to_midi, signals, PitchDetector};

    #[test]
    fn fr25_echo_repeats_at_the_note_length() {
        let sr = 48_000.0;
        let mut e = Echo::new(sr);
        // 120 BPM eighth = 0.25 s.
        e.set(0.25, 0.5);
        let n = (sr * 1.0) as usize;
        let y: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let x = if i < 480 {
                    (2.0 * core::f32::consts::PI * 1_000.0 * t).sin()
                } else {
                    0.0
                };
                e.process(x)
            })
            .collect();
        let peak_at = |from: usize, to: usize| {
            y[from..to]
                .iter()
                .enumerate()
                .fold((0, 0.0f32), |b, (i, &v)| {
                    if v.abs() > b.1 {
                        (from + i, v.abs())
                    } else {
                        b
                    }
                })
        };
        let first = peak_at(1_000, 20_000);
        let second = peak_at(20_000, 30_000);
        assert!(
            (first.0 as i64 - 12_000).abs() < 600,
            "first echo at {}",
            first.0
        );
        assert!(
            (second.0 as i64 - 24_000).abs() < 600,
            "second echo at {}",
            second.0
        );
        assert!(second.1 < first.1, "repeats must decay");
        assert!(
            y[..11_000].iter().all(|v| v.abs() < 1e-3),
            "echo before its time"
        );
    }

    #[test]
    fn fr25_doubler_detunes_but_keeps_pitch_centre() {
        let sr = 48_000.0;
        let f = 220.0;
        let x = signals::harmonic(f, sr, 96_000, 0.5, 6);
        let mut d = Doubler::new(sr);
        d.set(20.0, 20.0);
        let y: Vec<f32> = x.iter().map(|&s| d.process(s)).collect();
        assert!(y.iter().all(|v| v.is_finite()));
        let mut det = PitchDetector::new(sr, 1372);
        let mut max_dev = 0.0f32;
        for start in (48_000..90_000).step_by(4_000) {
            if let Some(e) = det.detect(&y[start..start + 1_372], &[], 70.0, 1000.0) {
                max_dev = max_dev.max((hz_to_midi(e.hz) - hz_to_midi(f)).abs());
            }
        }
        // Within the 20-cent spread (plus detector tolerance), never a semitone off.
        assert!(max_dev < 0.35, "{max_dev}");
        let energy: f32 = y[48_000..].iter().map(|v| v * v).sum();
        assert!(energy > 100.0);
    }
}
