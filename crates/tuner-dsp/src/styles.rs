//! Built-in sound styles (FR-27): one click from the plain tuner to a
//! finished pop vocal. A style sets the sound (tuning feel, formant and
//! effects) but keeps everything personal: key, scale, voice range, gate,
//! bypass, echo tempo and effect routes.

use crate::params::{DelayDivision, FxParams, Scale, TuningParams};

/// A built-in style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Style {
    /// Hard-snapped robotic croon with a glossy plate and slap echo.
    ChromeSnap,
    /// Smooth, fast-tuned R&B voice: doubled, with a wide reverb and echo.
    VelvetEcho,
    /// Dark trap vocal: hard tune, heavy compression, bright EQ, short plate.
    NightDrive,
    /// Gentle correction that keeps your vibrato, with a touch of room.
    Natural,
}

impl Style {
    pub const ALL: [Style; 4] = [
        Style::ChromeSnap,
        Style::VelvetEcho,
        Style::NightDrive,
        Style::Natural,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Style::ChromeSnap => "Chrome Snap",
            Style::VelvetEcho => "Velvet Echo",
            Style::NightDrive => "Night Drive",
            Style::Natural => "Natural",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Style::ChromeSnap => "Hard-snapped robotic croon with a glossy plate and slap echo.",
            Style::VelvetEcho => "Smooth, fast tuning with a doubled voice, wide reverb and echo.",
            Style::NightDrive => "Hard tune, heavy compression and a bright, close trap sound.",
            Style::Natural => "Gentle correction that keeps your vibrato, plus a little room.",
        }
    }

    /// Parse a kebab-case id such as `chrome-snap`.
    pub fn from_id(id: &str) -> Option<Style> {
        Style::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Kebab-case id, e.g. for the CLI.
    pub fn id(self) -> &'static str {
        match self {
            Style::ChromeSnap => "chrome-snap",
            Style::VelvetEcho => "velvet-echo",
            Style::NightDrive => "night-drive",
            Style::Natural => "natural",
        }
    }

    /// `base` with this style's sound. Snapping styles move a chromatic
    /// scale to major in the same key, because the jump between scale notes
    /// is what makes the effect audible.
    pub fn apply(self, base: &TuningParams) -> TuningParams {
        let fx = FxParams {
            delay_bpm: base.fx.delay_bpm,
            doubler_route: base.fx.doubler_route,
            delay_route: base.fx.delay_route,
            reverb_route: base.fx.reverb_route,
            ..self.fx()
        };
        let (hard_tune, retune_ms, humanize, formant_semitones) = match self {
            Style::ChromeSnap => (true, 0.0, 0.0, -1.0),
            Style::VelvetEcho => (false, 5.0, 0.1, 0.0),
            Style::NightDrive => (true, 0.0, 0.0, 0.0),
            Style::Natural => (false, 40.0, 0.5, 0.0),
        };
        let scale = if self != Style::Natural && base.scale == Scale::Chromatic {
            Scale::Major
        } else {
            base.scale
        };
        TuningParams {
            scale,
            retune_ms,
            humanize,
            mix: 1.0,
            hard_tune,
            formant_semitones,
            fx,
            ..*base
        }
        .sanitized()
    }

    fn fx(self) -> FxParams {
        let d = FxParams::default();
        match self {
            Style::ChromeSnap => FxParams {
                presence_db: 2.0,
                air_db: 2.0,
                comp_threshold_db: -18.0,
                comp_ratio: 3.0,
                delay_mix: 0.12,
                delay_division: DelayDivision::Eighth,
                delay_feedback: 0.3,
                reverb_mix: 0.2,
                reverb_size: 0.6,
                reverb_decay: 0.55,
                reverb_predelay_ms: 20.0,
                ..d
            },
            Style::VelvetEcho => FxParams {
                presence_db: 1.5,
                air_db: 3.0,
                comp_threshold_db: -20.0,
                comp_ratio: 3.0,
                doubler_mix: 0.5,
                doubler_detune_cents: 10.0,
                doubler_delay_ms: 18.0,
                delay_mix: 0.15,
                delay_division: DelayDivision::Quarter,
                delay_feedback: 0.35,
                reverb_mix: 0.25,
                reverb_size: 0.7,
                reverb_decay: 0.6,
                reverb_predelay_ms: 30.0,
                ..d
            },
            Style::NightDrive => FxParams {
                presence_db: 3.0,
                air_db: 4.0,
                comp_threshold_db: -24.0,
                comp_ratio: 6.0,
                doubler_mix: 0.25,
                doubler_detune_cents: 8.0,
                doubler_delay_ms: 14.0,
                delay_mix: 0.18,
                delay_division: DelayDivision::DottedEighth,
                delay_feedback: 0.4,
                reverb_mix: 0.15,
                reverb_size: 0.35,
                reverb_decay: 0.4,
                reverb_predelay_ms: 10.0,
                ..d
            },
            Style::Natural => FxParams {
                reverb_mix: 0.1,
                reverb_size: 0.5,
                reverb_decay: 0.45,
                reverb_predelay_ms: 15.0,
                ..d
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{FxRoute, VoiceRange};

    fn personal() -> TuningParams {
        TuningParams {
            key: 9,
            scale: Scale::Chromatic,
            voice_range: VoiceRange::Low,
            gate_threshold_db: -42.0,
            bypass: true,
            fx: FxParams {
                delay_bpm: 87.0,
                reverb_route: FxRoute::Headphones,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn fr27_styles_keep_personal_settings() {
        for style in Style::ALL {
            let p = style.apply(&personal());
            assert_eq!(p.key, 9);
            assert_eq!(p.voice_range, VoiceRange::Low);
            assert_eq!(p.gate_threshold_db, -42.0);
            assert!(p.bypass);
            assert_eq!(p.fx.delay_bpm, 87.0);
            assert_eq!(p.fx.reverb_route, FxRoute::Headphones);
            assert_eq!(p, p.sanitized(), "{style:?} out of range");
            assert_eq!(Style::from_id(style.id()), Some(style));
        }
    }

    #[test]
    fn fr27_snapping_styles_pick_a_scale() {
        assert_eq!(Style::ChromeSnap.apply(&personal()).scale, Scale::Major);
        assert_eq!(Style::Natural.apply(&personal()).scale, Scale::Chromatic);
        let minor = TuningParams {
            scale: Scale::NaturalMinor,
            ..personal()
        };
        assert_eq!(Style::NightDrive.apply(&minor).scale, Scale::NaturalMinor);
    }

    #[test]
    fn fr23_hard_styles_snap_instantly() {
        let p = Style::ChromeSnap.apply(&personal());
        assert!(p.hard_tune);
        assert_eq!(p.retune_ms, 0.0);
        assert_eq!(p.humanize, 0.0);
        assert!(p.fx.reverb_mix > 0.0 && p.fx.delay_mix > 0.0);
        assert!(!Style::Natural.apply(&personal()).hard_tune);
    }
}
