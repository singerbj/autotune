//! The full per-block pipeline, glued together.

use crate::filters::{
    lin_to_db, one_pole_coeff, soft_limit, Biquad, DcBlocker, NoiseGate, Ramp, Smoother,
};
use crate::fx::FxChain;
use crate::mpm::PitchDetector;
use crate::params::{TuningParams, VoiceRange, MAX_SHIFT_SEMITONES};
use crate::psola::Psola;
use crate::rng::Pcg32;
use crate::scale::{hz_to_midi, midi_to_hz, NoteSnapper};
use crate::{DspError, ABS_MIN_PITCH_HZ, MAX_PITCH_HZ};

/// Internal processing granularity in samples.
const CHUNK: usize = 32;
/// Clarity needed to enter / stay in the voiced state.
const CLARITY_ON: f32 = 0.80;
const CLARITY_OFF: f32 = 0.65;
/// Hard tune (FR-23) accepts shakier detections so the effect stays on
/// through breathy or noisy notes…
const HARD_CLARITY_ON: f32 = 0.70;
const HARD_CLARITY_OFF: f32 = 0.50;
/// …and holds the last note through detection dropouts this long.
const HARD_HOLD_S: f32 = 0.040;
/// Snapper hysteresis in semitones: normal vs hard tune (instant switches).
const SNAP_HYSTERESIS: f32 = 0.2;
/// Crossfade into the tuned signal at a note onset: normal vs hard tune.
const VOICED_FADE_S: f32 = 0.004;
const HARD_VOICED_FADE_S: f32 = 0.001;
/// Input level below which nothing is considered voiced.
const VOICING_FLOOR_DB: f32 = -50.0;
/// Maximum random detune per note at 100 % humanize, in semitones.
const HUMANIZE_DETUNE: f32 = 0.10;

/// Construction parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerConfig {
    pub sample_rate: f32,
    /// Seed for the humanize RNG (determinism rule).
    pub seed: u64,
}

impl Default for TunerConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            seed: 0x5EED,
        }
    }
}

/// Meter snapshot updated by every [`Tuner::process`] call (FR-17).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DspMeters {
    /// Peak input level in dBFS.
    pub input_db: f32,
    /// Detected pitch in Hz, 0 when unvoiced.
    pub detected_hz: f32,
    /// Target MIDI note, −1 when there is none.
    pub target_midi: i32,
    /// Correction currently applied, in cents.
    pub correction_cents: f32,
    pub voiced: bool,
    pub gate_open: bool,
}

/// Real-time voice tuner. Construct off the audio thread, then call
/// [`set_params`](Self::set_params) and [`process`](Self::process) from it.
pub struct Tuner {
    sr: f32,
    params: TuningParams,
    note_mask: u16,
    dc: DcBlocker,
    hp: Biquad,
    gate: NoiseGate,
    detector: PitchDetector,
    psola: Psola,
    range: VoiceRange,
    pending_range: Option<VoiceRange>,
    range_fade: Ramp,
    detect_hop: usize,
    since_detect: usize,
    voiced: bool,
    /// Samples since the last voiced detection while holding (hard tune).
    unvoiced_run: usize,
    hard_hold: usize,
    hist: [f32; 3],
    hist_len: usize,
    midi_det: f32,
    midi_slow: f32,
    slow_coeff: f32,
    period: Option<f32>,
    snapper: NoteSnapper,
    target: Option<i32>,
    human_offset: f32,
    rng: Pcg32,
    seed: u64,
    desired_shift: f32,
    applied_shift: f32,
    voiced_mix: Smoother,
    mix: Smoother,
    bypass: Ramp,
    level_env: f32,
    level_release: f32,
    fx: FxChain,
    meters: DspMeters,
}

impl core::fmt::Debug for Tuner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tuner")
            .field("sample_rate", &self.sr)
            .field("params", &self.params)
            .field("latency", &self.latency_samples())
            .finish_non_exhaustive()
    }
}

impl Tuner {
    /// Allocate all state for the worst case (70 Hz at `sample_rate`).
    pub fn new(config: TunerConfig) -> Result<Self, DspError> {
        let sr = config.sample_rate;
        if !(8_000.0..=192_000.0).contains(&sr) {
            return Err(DspError::UnsupportedSampleRate(sr));
        }
        let max_period = (sr / ABS_MIN_PITCH_HZ).ceil() as usize;
        let params = TuningParams::default();
        let range = params.voice_range;
        let detect_hop = ((sr * 128.0 / 48_000.0).round() as usize).max(CHUNK);
        let mut t = Self {
            sr,
            params,
            note_mask: params.note_mask(),
            dc: DcBlocker::new(sr),
            hp: Biquad::highpass(sr, 70.0, core::f32::consts::FRAC_1_SQRT_2),
            gate: NoiseGate::new(sr),
            detector: PitchDetector::new(sr, 2 * max_period),
            psola: Psola::new(sr, max_period, range.latency_samples(sr)),
            range,
            pending_range: None,
            range_fade: Ramp::new(1.0, 0.005, sr),
            detect_hop,
            since_detect: 0,
            voiced: false,
            unvoiced_run: 0,
            hard_hold: (HARD_HOLD_S * sr) as usize,
            hist: [0.0; 3],
            hist_len: 0,
            midi_det: 0.0,
            midi_slow: 0.0,
            slow_coeff: one_pole_coeff(0.120, sr / detect_hop as f32),
            period: None,
            snapper: NoteSnapper::default(),
            target: None,
            human_offset: 0.0,
            rng: Pcg32::new(config.seed),
            seed: config.seed,
            desired_shift: 0.0,
            applied_shift: 0.0,
            voiced_mix: Smoother::new(0.0, VOICED_FADE_S, sr),
            mix: Smoother::new(params.mix, 0.020, sr),
            bypass: Ramp::new(0.0, 0.010, sr),
            level_env: 0.0,
            level_release: one_pole_coeff(0.300, sr),
            fx: FxChain::new(sr),
            meters: DspMeters {
                target_midi: -1,
                input_db: -200.0,
                ..Default::default()
            },
        };
        t.set_params(&params);
        Ok(t)
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Algorithmic latency in samples for the active voice range.
    pub fn latency_samples(&self) -> usize {
        self.psola.latency()
    }

    pub fn params(&self) -> &TuningParams {
        &self.params
    }

    pub fn meters(&self) -> DspMeters {
        self.meters
    }

    /// Apply new parameters. Real-time safe; changes are smoothed (FR-11).
    pub fn set_params(&mut self, p: &TuningParams) {
        let p = p.sanitized();
        self.gate.set_threshold_db(p.gate_threshold_db);
        self.mix.set_target(p.mix);
        self.bypass.set_target(if p.bypass { 1.0 } else { 0.0 });
        self.note_mask = p.note_mask();
        self.snapper
            .set_hysteresis(if p.hard_tune { 0.0 } else { SNAP_HYSTERESIS });
        self.voiced_mix.set_time(
            if p.hard_tune {
                HARD_VOICED_FADE_S
            } else {
                VOICED_FADE_S
            },
            self.sr,
        );
        self.psola.set_formant((p.formant_semitones / 12.0).exp2());
        self.fx.set_params(&p.fx);
        if p.voice_range != self.range || self.pending_range.is_some() {
            if p.voice_range == self.range {
                self.pending_range = None;
                self.range_fade.set_target(1.0);
            } else {
                self.pending_range = Some(p.voice_range);
                self.range_fade.set_target(0.0);
            }
        }
        self.params = p;
    }

    /// Clear all signal state (keeps parameters). Real-time safe.
    pub fn reset(&mut self) {
        self.dc.reset();
        self.hp.reset();
        self.gate.reset();
        self.psola.reset();
        self.fx.reset();
        self.reset_pitch_state();
        self.rng = Pcg32::new(self.seed);
        self.level_env = 0.0;
        self.voiced_mix.snap(0.0);
        self.since_detect = 0;
    }

    fn reset_pitch_state(&mut self) {
        self.voiced = false;
        self.unvoiced_run = 0;
        self.hist_len = 0;
        self.period = None;
        self.snapper.reset();
        self.target = None;
        self.desired_shift = 0.0;
        self.applied_shift = 0.0;
        self.voiced_mix.set_target(0.0);
    }

    /// Process one block with every effect applied, whatever its route
    /// (offline rendering). `input` and `output` should have equal length;
    /// any extra output samples are zeroed. Never allocates.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let n = input.len().min(output.len());
        output[n..].fill(0.0);
        let mut off = 0;
        while off < n {
            let len = (n - off).min(CHUNK);
            self.process_chunk(&input[off..off + len], &mut output[off..off + len], None);
            off += len;
        }
    }

    /// Process one block into the two live destinations (FR-26): `monitor`
    /// (headphones) and `cable` (virtual mic) each get the effects routed to
    /// them. Lengths as for [`process`](Self::process). Never allocates.
    pub fn process_split(&mut self, input: &[f32], monitor: &mut [f32], cable: &mut [f32]) {
        let n = input.len().min(monitor.len()).min(cable.len());
        monitor[n..].fill(0.0);
        cable[n..].fill(0.0);
        let mut off = 0;
        while off < n {
            let len = (n - off).min(CHUNK);
            self.process_chunk(
                &input[off..off + len],
                &mut monitor[off..off + len],
                Some(&mut cable[off..off + len]),
            );
            off += len;
        }
    }

    fn process_chunk(&mut self, input: &[f32], output: &mut [f32], mut cable: Option<&mut [f32]>) {
        for &x in input {
            let x = if x.is_finite() {
                x.clamp(-4.0, 4.0)
            } else {
                0.0
            };
            let c = self.gate.process(self.hp.process(self.dc.process(x)));
            self.psola.push(x, c);
            let a = x.abs();
            self.level_env = if a > self.level_env {
                a
            } else {
                a + (self.level_env - a) * self.level_release
            };
        }

        self.since_detect += input.len();
        if self.since_detect >= self.detect_hop {
            self.since_detect -= self.detect_hop;
            self.analyze();
        }

        self.psola.update_marks(self.period);
        let ratio = self.glide(input.len());
        self.psola.synthesize(ratio);

        for (i, o) in output.iter_mut().enumerate() {
            let (wet, dry, raw) = self.psola.pop();
            let tuned = dry + self.voiced_mix.next() * (wet - dry);
            let fx = self.fx.process(dry + self.mix.next() * (tuned - dry));
            let bypass = self.bypass.next();
            let fade = self.range_fade.next();
            let finish = |v: f32| {
                let limited = soft_limit(v);
                (limited + bypass * (raw - limited)) * fade
            };
            match cable.as_deref_mut() {
                Some(c) => {
                    *o = finish(fx.monitor);
                    c[i] = finish(fx.cable);
                }
                None => *o = finish(fx.all),
            }
        }

        if let Some(r) = self.pending_range {
            if self.range_fade.value() == 0.0 {
                self.range = r;
                self.pending_range = None;
                self.psola.set_latency(r.latency_samples(self.sr));
                self.reset_pitch_state();
                self.voiced_mix.snap(0.0);
                self.range_fade.set_target(1.0);
            }
        }

        self.meters = DspMeters {
            input_db: lin_to_db(self.level_env),
            detected_hz: if self.voiced {
                midi_to_hz(self.midi_det)
            } else {
                0.0
            },
            target_midi: match (self.voiced, self.target) {
                (true, Some(n)) => n,
                _ => -1,
            },
            correction_cents: if self.voiced {
                self.applied_shift * 100.0
            } else {
                0.0
            },
            voiced: self.voiced,
            gate_open: self.gate.is_open(),
        };
    }

    /// Pitch detection, voicing decision and target selection (stages 2–4).
    fn analyze(&mut self) {
        let min_hz = self.range.min_hz();
        let win = ((2.0 * self.sr / min_hz).ceil() as usize).min(self.detector.max_window());
        let (a, b) = self.psola.recent(win);
        let est = self.detector.detect(a, b, min_hz, MAX_PITCH_HZ);
        let hard = self.params.hard_tune;
        let threshold = match (hard, self.voiced) {
            (false, false) => CLARITY_ON,
            (false, true) => CLARITY_OFF,
            (true, false) => HARD_CLARITY_ON,
            (true, true) => HARD_CLARITY_OFF,
        };
        let loud = lin_to_db(self.level_env) > VOICING_FLOOR_DB && self.gate.is_open();
        let voiced_est = est.filter(|e| loud && e.clarity >= threshold);

        let Some(e) = voiced_est else {
            if self.voiced {
                // Hard tune rides out short dropouts on the last note while
                // there is still sound; silence ends the note at once.
                self.unvoiced_run += self.detect_hop;
                if !(hard && loud && self.unvoiced_run < self.hard_hold) {
                    self.reset_pitch_state();
                }
            }
            return;
        };
        self.unvoiced_run = 0;

        let midi = hz_to_midi(e.hz);
        if self.hist_len < 3 {
            self.hist[self.hist_len] = midi;
            self.hist_len += 1;
        } else {
            self.hist = [self.hist[1], self.hist[2], midi];
        }
        let med = median3(&self.hist[..self.hist_len]);

        if !self.voiced {
            self.voiced = true;
            self.midi_slow = med;
            self.applied_shift = 0.0;
            self.snapper.reset();
            self.target = None;
            self.voiced_mix.set_target(1.0);
        }
        self.midi_det = med;
        self.midi_slow = med + (self.midi_slow - med) * self.slow_coeff;
        let max_period = self.range.latency_samples(self.sr) as f32;
        self.period = Some((self.sr / midi_to_hz(med)).min(max_period));

        let target = self.snapper.snap(med, self.note_mask);
        let h = if hard { 0.0 } else { self.params.humanize };
        if target != self.target {
            self.human_offset = self.rng.bipolar() * h * HUMANIZE_DETUNE;
            self.target = target;
        }
        let basis = med + h * (self.midi_slow - med);
        self.desired_shift = match target {
            Some(n) => {
                let s = n as f32 + self.human_offset - basis;
                if s.abs() <= MAX_SHIFT_SEMITONES {
                    s
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
    }

    /// Retune glide (stage 5); returns the pitch ratio to apply.
    fn glide(&mut self, samples: usize) -> f32 {
        let retune_s = if self.params.hard_tune {
            0.0
        } else {
            self.params.retune_ms / 1000.0
        };
        if retune_s < 0.0005 {
            self.applied_shift = self.desired_shift;
        } else {
            let dt = samples as f32 / self.sr;
            let k = 1.0 - (-dt / retune_s).exp();
            self.applied_shift += (self.desired_shift - self.applied_shift) * k;
        }
        (self.applied_shift / 12.0).exp2()
    }
}

fn median3(v: &[f32]) -> f32 {
    match *v {
        [a] => a,
        [a, b] => 0.5 * (a + b),
        [a, b, c] => a.max(b).min(a.min(b).max(c)),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_three() {
        assert_eq!(median3(&[1.0, 5.0, 3.0]), 3.0);
        assert_eq!(median3(&[5.0, 1.0, 3.0]), 3.0);
        assert_eq!(median3(&[3.0, 1.0, 5.0]), 3.0);
        assert_eq!(median3(&[2.0]), 2.0);
    }

    #[test]
    fn rejects_bad_sample_rate() {
        assert!(Tuner::new(TunerConfig {
            sample_rate: 1000.0,
            seed: 0
        })
        .is_err());
    }
}
