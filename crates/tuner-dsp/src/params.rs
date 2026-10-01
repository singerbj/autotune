//! User-facing tuning parameters (FR-06 … FR-11).

/// Largest correction the shifter will apply, in semitones (risk: PSOLA
/// artifacts on big shifts → cap the shift range).
pub const MAX_SHIFT_SEMITONES: f32 = 7.0;

/// Scale used to pick target notes (FR-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Scale {
    #[default]
    Chromatic,
    Major,
    NaturalMinor,
    /// Uses [`TuningParams::custom_mask`] (absolute pitch classes, bit 0 = C).
    Custom,
}

/// Voice range preset (FR-08): sets the lowest tracked pitch and therefore
/// the fixed DSP latency (≈ one period of the lowest pitch).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum VoiceRange {
    Low,
    #[default]
    Mid,
    High,
}

impl VoiceRange {
    /// Lowest tracked pitch in Hz: 70, 100 or 150 (FR-08).
    pub fn min_hz(self) -> f32 {
        match self {
            VoiceRange::Low => 70.0,
            VoiceRange::Mid => 100.0,
            VoiceRange::High => 150.0,
        }
    }

    /// Algorithmic latency in samples at `sample_rate`.
    pub fn latency_samples(self, sample_rate: f32) -> usize {
        (sample_rate / self.min_hz()).ceil() as usize
    }
}

/// Complete tuning parameter set. Plain `Copy` data so it can be moved to the
/// audio thread without allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TuningParams {
    /// Key root as a pitch class, 0 = C … 11 = B.
    pub key: u8,
    pub scale: Scale,
    /// Bit `n` enables pitch class `n` (C = bit 0). Only used by [`Scale::Custom`].
    pub custom_mask: u16,
    /// Retune time constant, 0 (hard snap) … 200 ms (FR-07).
    pub retune_ms: f32,
    /// 0 … 1; preserves natural pitch movement and adds seeded detune (FR-07).
    pub humanize: f32,
    pub voice_range: VoiceRange,
    /// Dry/wet mix, 0 = dry … 1 = fully tuned (FR-10).
    pub mix: f32,
    /// Noise gate threshold in dBFS; −100 disables the gate (FR-09).
    pub gate_threshold_db: f32,
    /// Click-free bypass (FR-10).
    pub bypass: bool,
    /// Hard tune (FR-23): instant note switches, retune and humanize treated
    /// as 0, and more forgiving voicing so the effect never drops out.
    pub hard_tune: bool,
    /// Formant shift in semitones, −4 … +4 (FR-24). Negative sounds like a
    /// longer throat (deeper), positive like a shorter one (brighter).
    pub formant_semitones: f32,
    /// Vocal effects after the tuner (FR-25, FR-26).
    pub fx: FxParams,
}

/// Which outputs a time-based effect is heard on (FR-26).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FxRoute {
    #[default]
    Both,
    /// Only in your headphones (the monitor).
    Headphones,
    /// Only in the virtual mic (Discord and other apps).
    Discord,
}

impl FxRoute {
    pub fn to_monitor(self) -> bool {
        self != FxRoute::Discord
    }

    pub fn to_cable(self) -> bool {
        self != FxRoute::Headphones
    }
}

/// Echo length as a note value at [`FxParams::delay_bpm`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum DelayDivision {
    Quarter,
    #[default]
    Eighth,
    DottedEighth,
    Sixteenth,
}

impl DelayDivision {
    /// Length in beats (quarter notes).
    pub fn beats(self) -> f32 {
        match self {
            DelayDivision::Quarter => 1.0,
            DelayDivision::Eighth => 0.5,
            DelayDivision::DottedEighth => 0.75,
            DelayDivision::Sixteenth => 0.25,
        }
    }
}

/// Effects chain parameters (FR-25). Every effect is neutral at its default:
/// EQ gains 0 dB, compressor threshold 0 dBFS, all send mixes 0.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxParams {
    /// Presence peak at 3.5 kHz, −6 … +12 dB.
    pub presence_db: f32,
    /// Air shelf from 10 kHz, −6 … +12 dB.
    pub air_db: f32,
    /// Compressor threshold, −40 … 0 dBFS; 0 turns it off.
    pub comp_threshold_db: f32,
    /// Compressor ratio, 1 … 10.
    pub comp_ratio: f32,
    /// Doubler level, 0 … 1.
    pub doubler_mix: f32,
    /// Doubler pitch spread, 0 … 30 cents.
    pub doubler_detune_cents: f32,
    /// Doubler delay, 10 … 40 ms.
    pub doubler_delay_ms: f32,
    pub doubler_route: FxRoute,
    /// Echo level, 0 … 1.
    pub delay_mix: f32,
    /// Song tempo for the echo, 60 … 200 BPM.
    pub delay_bpm: f32,
    pub delay_division: DelayDivision,
    /// Echo feedback, 0 … 0.9.
    pub delay_feedback: f32,
    pub delay_route: FxRoute,
    /// Reverb level, 0 … 1.
    pub reverb_mix: f32,
    /// Plate size, 0 … 1.
    pub reverb_size: f32,
    /// Tail length, 0 … 1 (≈ 0.3 s … 6 s).
    pub reverb_decay: f32,
    /// Gap before the reverb starts, 0 … 100 ms.
    pub reverb_predelay_ms: f32,
    pub reverb_route: FxRoute,
}

impl Default for FxParams {
    fn default() -> Self {
        Self {
            presence_db: 0.0,
            air_db: 0.0,
            comp_threshold_db: 0.0,
            comp_ratio: 4.0,
            doubler_mix: 0.0,
            doubler_detune_cents: 10.0,
            doubler_delay_ms: 18.0,
            doubler_route: FxRoute::Both,
            delay_mix: 0.0,
            delay_bpm: 120.0,
            delay_division: DelayDivision::Eighth,
            delay_feedback: 0.3,
            delay_route: FxRoute::Both,
            reverb_mix: 0.0,
            reverb_size: 0.6,
            reverb_decay: 0.5,
            reverb_predelay_ms: 20.0,
            reverb_route: FxRoute::Both,
        }
    }
}

/// Number of 32-bit words in [`FxParams::to_words`].
pub const FX_WORDS: usize = 18;

impl FxParams {
    /// Clamp every field into its documented range.
    pub fn sanitized(mut self) -> Self {
        let d = Self::default();
        let c = |v: f32, fallback: f32, lo: f32, hi: f32| finite_or(v, fallback).clamp(lo, hi);
        self.presence_db = c(self.presence_db, 0.0, -6.0, 12.0);
        self.air_db = c(self.air_db, 0.0, -6.0, 12.0);
        self.comp_threshold_db = c(self.comp_threshold_db, 0.0, -40.0, 0.0);
        self.comp_ratio = c(self.comp_ratio, d.comp_ratio, 1.0, 10.0);
        self.doubler_mix = c(self.doubler_mix, 0.0, 0.0, 1.0);
        self.doubler_detune_cents = c(self.doubler_detune_cents, d.doubler_detune_cents, 0.0, 30.0);
        self.doubler_delay_ms = c(self.doubler_delay_ms, d.doubler_delay_ms, 10.0, 40.0);
        self.delay_mix = c(self.delay_mix, 0.0, 0.0, 1.0);
        self.delay_bpm = c(self.delay_bpm, d.delay_bpm, 60.0, 200.0);
        self.delay_feedback = c(self.delay_feedback, d.delay_feedback, 0.0, 0.9);
        self.reverb_mix = c(self.reverb_mix, 0.0, 0.0, 1.0);
        self.reverb_size = c(self.reverb_size, d.reverb_size, 0.0, 1.0);
        self.reverb_decay = c(self.reverb_decay, d.reverb_decay, 0.0, 1.0);
        self.reverb_predelay_ms = c(self.reverb_predelay_ms, d.reverb_predelay_ms, 0.0, 100.0);
        self
    }

    /// Flat encoding for lock-free transfer to the audio thread (one atomic
    /// per word). Floats are stored as bits, enums as indices.
    pub fn to_words(&self) -> [u32; FX_WORDS] {
        let route = |r: FxRoute| match r {
            FxRoute::Both => 0,
            FxRoute::Headphones => 1,
            FxRoute::Discord => 2,
        };
        let division = match self.delay_division {
            DelayDivision::Quarter => 0,
            DelayDivision::Eighth => 1,
            DelayDivision::DottedEighth => 2,
            DelayDivision::Sixteenth => 3,
        };
        [
            self.presence_db.to_bits(),
            self.air_db.to_bits(),
            self.comp_threshold_db.to_bits(),
            self.comp_ratio.to_bits(),
            self.doubler_mix.to_bits(),
            self.doubler_detune_cents.to_bits(),
            self.doubler_delay_ms.to_bits(),
            route(self.doubler_route),
            self.delay_mix.to_bits(),
            self.delay_bpm.to_bits(),
            division,
            self.delay_feedback.to_bits(),
            route(self.delay_route),
            self.reverb_mix.to_bits(),
            self.reverb_size.to_bits(),
            self.reverb_decay.to_bits(),
            self.reverb_predelay_ms.to_bits(),
            route(self.reverb_route),
        ]
    }

    /// Inverse of [`to_words`](Self::to_words).
    pub fn from_words(w: &[u32; FX_WORDS]) -> Self {
        let route = |v: u32| match v {
            1 => FxRoute::Headphones,
            2 => FxRoute::Discord,
            _ => FxRoute::Both,
        };
        let f = f32::from_bits;
        Self {
            presence_db: f(w[0]),
            air_db: f(w[1]),
            comp_threshold_db: f(w[2]),
            comp_ratio: f(w[3]),
            doubler_mix: f(w[4]),
            doubler_detune_cents: f(w[5]),
            doubler_delay_ms: f(w[6]),
            doubler_route: route(w[7]),
            delay_mix: f(w[8]),
            delay_bpm: f(w[9]),
            delay_division: match w[10] {
                0 => DelayDivision::Quarter,
                2 => DelayDivision::DottedEighth,
                3 => DelayDivision::Sixteenth,
                _ => DelayDivision::Eighth,
            },
            delay_feedback: f(w[11]),
            delay_route: route(w[12]),
            reverb_mix: f(w[13]),
            reverb_size: f(w[14]),
            reverb_decay: f(w[15]),
            reverb_predelay_ms: f(w[16]),
            reverb_route: route(w[17]),
        }
    }
}

impl Default for TuningParams {
    fn default() -> Self {
        Self {
            key: 0,
            scale: Scale::Chromatic,
            custom_mask: 0x0FFF,
            retune_ms: 20.0,
            humanize: 0.0,
            voice_range: VoiceRange::Mid,
            mix: 1.0,
            gate_threshold_db: -60.0,
            bypass: false,
            hard_tune: false,
            formant_semitones: 0.0,
            fx: FxParams::default(),
        }
    }
}

/// Largest formant shift in semitones (keeps PSOLA grains overlapping).
pub const MAX_FORMANT_SEMITONES: f32 = 4.0;

const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const NATURAL_MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

impl TuningParams {
    /// Clamp every field into its documented range.
    pub fn sanitized(mut self) -> Self {
        self.key %= 12;
        self.custom_mask &= 0x0FFF;
        self.retune_ms = finite_or(self.retune_ms, 20.0).clamp(0.0, 200.0);
        self.humanize = finite_or(self.humanize, 0.0).clamp(0.0, 1.0);
        self.mix = finite_or(self.mix, 1.0).clamp(0.0, 1.0);
        self.gate_threshold_db = finite_or(self.gate_threshold_db, -60.0).clamp(-100.0, 0.0);
        self.formant_semitones = finite_or(self.formant_semitones, 0.0)
            .clamp(-MAX_FORMANT_SEMITONES, MAX_FORMANT_SEMITONES);
        self.fx = self.fx.sanitized();
        self
    }

    /// Absolute pitch-class mask of allowed target notes (bit 0 = C).
    /// An empty custom mask falls back to chromatic so there is always a target.
    pub fn note_mask(&self) -> u16 {
        let key = u32::from(self.key % 12);
        let rotate = |steps: &[u8]| {
            steps
                .iter()
                .fold(0u16, |m, &s| m | 1 << ((u32::from(s) + key) % 12))
        };
        let mask = match self.scale {
            Scale::Chromatic => 0x0FFF,
            Scale::Major => rotate(&MAJOR),
            Scale::NaturalMinor => rotate(&NATURAL_MINOR),
            Scale::Custom => self.custom_mask & 0x0FFF,
        };
        if mask == 0 {
            0x0FFF
        } else {
            mask
        }
    }
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr06_major_scale_masks_rotate_with_key() {
        let mut p = TuningParams {
            scale: Scale::Major,
            ..Default::default()
        };
        // C major: C D E F G A B
        assert_eq!(p.note_mask(), 0b1010_1011_0101);
        p.key = 7; // G major has F#, not F
        let m = p.note_mask();
        assert!(m & (1 << 6) != 0, "F# in G major");
        assert!(m & (1 << 5) == 0, "F not in G major");
        assert_eq!(m.count_ones(), 7);
    }

    #[test]
    fn fr06_natural_minor_a_equals_c_major() {
        let a_minor = TuningParams {
            key: 9,
            scale: Scale::NaturalMinor,
            ..Default::default()
        };
        let c_major = TuningParams {
            scale: Scale::Major,
            ..Default::default()
        };
        assert_eq!(a_minor.note_mask(), c_major.note_mask());
    }

    #[test]
    fn fr06_empty_custom_mask_is_chromatic() {
        let p = TuningParams {
            scale: Scale::Custom,
            custom_mask: 0,
            ..Default::default()
        };
        assert_eq!(p.note_mask(), 0x0FFF);
    }

    #[test]
    fn fr07_sanitize_clamps_ranges() {
        let p = TuningParams {
            key: 14,
            retune_ms: 500.0,
            humanize: -1.0,
            mix: f32::NAN,
            gate_threshold_db: -300.0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(p.key, 2);
        assert_eq!(p.retune_ms, 200.0);
        assert_eq!(p.humanize, 0.0);
        assert_eq!(p.mix, 1.0);
        assert_eq!(p.gate_threshold_db, -100.0);
    }

    #[test]
    fn fr24_fr25_sanitize_clamps_formant_and_fx() {
        let p = TuningParams {
            formant_semitones: 9.0,
            fx: FxParams {
                presence_db: 40.0,
                comp_ratio: 0.0,
                delay_bpm: f32::INFINITY,
                delay_feedback: 1.5,
                reverb_mix: -1.0,
                doubler_delay_ms: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
        .sanitized();
        assert_eq!(p.formant_semitones, MAX_FORMANT_SEMITONES);
        assert_eq!(p.fx.presence_db, 12.0);
        assert_eq!(p.fx.comp_ratio, 1.0);
        assert_eq!(p.fx.delay_bpm, 120.0);
        assert_eq!(p.fx.delay_feedback, 0.9);
        assert_eq!(p.fx.reverb_mix, 0.0);
        assert_eq!(p.fx.doubler_delay_ms, 10.0);
    }

    #[test]
    fn fr25_fx_words_roundtrip() {
        let fx = FxParams {
            presence_db: 3.0,
            air_db: -2.0,
            comp_threshold_db: -18.0,
            comp_ratio: 6.0,
            doubler_mix: 0.4,
            doubler_detune_cents: 12.0,
            doubler_delay_ms: 22.0,
            doubler_route: FxRoute::Headphones,
            delay_mix: 0.2,
            delay_bpm: 93.0,
            delay_division: DelayDivision::DottedEighth,
            delay_feedback: 0.45,
            delay_route: FxRoute::Discord,
            reverb_mix: 0.3,
            reverb_size: 0.8,
            reverb_decay: 0.7,
            reverb_predelay_ms: 35.0,
            reverb_route: FxRoute::Both,
        };
        assert_eq!(FxParams::from_words(&fx.to_words()), fx);
        for d in [
            DelayDivision::Quarter,
            DelayDivision::Eighth,
            DelayDivision::DottedEighth,
            DelayDivision::Sixteenth,
        ] {
            let fx = FxParams {
                delay_division: d,
                ..Default::default()
            };
            assert_eq!(FxParams::from_words(&fx.to_words()), fx);
        }
    }

    #[test]
    fn fr26_routes() {
        assert!(FxRoute::Both.to_monitor() && FxRoute::Both.to_cable());
        assert!(FxRoute::Headphones.to_monitor() && !FxRoute::Headphones.to_cable());
        assert!(!FxRoute::Discord.to_monitor() && FxRoute::Discord.to_cable());
    }

    #[test]
    fn fr08_voice_range_latencies() {
        assert_eq!(VoiceRange::Low.min_hz(), 70.0);
        assert_eq!(VoiceRange::Mid.min_hz(), 100.0);
        assert_eq!(VoiceRange::High.min_hz(), 150.0);
        assert_eq!(VoiceRange::Mid.latency_samples(48_000.0), 480);
        assert_eq!(VoiceRange::Low.latency_samples(48_000.0), 686);
        assert_eq!(VoiceRange::High.latency_samples(48_000.0), 320);
    }
}
