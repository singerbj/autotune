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
        }
    }
}

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
    fn fr08_voice_range_latencies() {
        assert_eq!(VoiceRange::Low.min_hz(), 70.0);
        assert_eq!(VoiceRange::Mid.min_hz(), 100.0);
        assert_eq!(VoiceRange::High.min_hz(), 150.0);
        assert_eq!(VoiceRange::Mid.latency_samples(48_000.0), 480);
        assert_eq!(VoiceRange::Low.latency_samples(48_000.0), 686);
        assert_eq!(VoiceRange::High.latency_samples(48_000.0), 320);
    }
}
