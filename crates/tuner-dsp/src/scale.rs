//! Note math and scale snapping with hysteresis (pipeline stage 4).

/// Frequency in Hz → fractional MIDI note number (A4 = 69 = 440 Hz).
#[inline]
pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// Fractional MIDI note number → frequency in Hz.
#[inline]
pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * ((midi - 69.0) / 12.0).exp2()
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Name of a pitch class (0 = C).
pub fn pitch_class_name(pc: u8) -> &'static str {
    NOTE_NAMES[usize::from(pc % 12)]
}

/// Parse a key name such as `C`, `F#`, `Bb` into a pitch class.
pub fn parse_key(name: &str) -> Option<u8> {
    let mut chars = name.trim().chars();
    let base: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let acc: i32 = match chars.as_str() {
        "" => 0,
        "#" | "s" | "♯" => 1,
        "b" | "♭" => -1,
        _ => return None,
    };
    Some((base + acc).rem_euclid(12) as u8)
}

#[inline]
fn in_mask(note: i32, mask: u16) -> bool {
    mask & (1 << note.rem_euclid(12)) != 0
}

/// Nearest note (integer MIDI) whose pitch class is in `mask`.
pub fn nearest_in_mask(midi: f32, mask: u16) -> Option<i32> {
    let center = midi.round() as i32;
    let mut best: Option<(i32, f32)> = None;
    for d in 0..=12 {
        for n in [center - d, center + d] {
            if in_mask(n, mask) {
                let dist = (midi - n as f32).abs();
                if best.is_none_or(|(_, bd)| dist < bd) {
                    best = Some((n, dist));
                }
            }
        }
        if best.is_some() && d >= 1 {
            // Any note further out is at least d semitones away.
            if let Some((_, bd)) = best {
                if bd <= d as f32 {
                    break;
                }
            }
        }
    }
    best.map(|(n, _)| n)
}

/// Stateful snapper: keeps the current note until another allowed note is
/// closer by more than the hysteresis margin, so a pitch hovering near the
/// midpoint between notes doesn't flip-flop.
#[derive(Clone, Debug)]
pub struct NoteSnapper {
    current: Option<i32>,
    /// Margin in semitones (0.2 = ~20 cents band).
    hysteresis: f32,
}

impl Default for NoteSnapper {
    fn default() -> Self {
        Self::new(0.2)
    }
}

impl NoteSnapper {
    pub fn new(hysteresis: f32) -> Self {
        Self {
            current: None,
            hysteresis,
        }
    }

    pub fn current(&self) -> Option<i32> {
        self.current
    }

    pub fn reset(&mut self) {
        self.current = None;
    }

    pub fn snap(&mut self, midi: f32, mask: u16) -> Option<i32> {
        let cand = nearest_in_mask(midi, mask)?;
        let next = match self.current {
            Some(cur) if cur != cand && in_mask(cur, mask) => {
                let d_cur = (midi - cur as f32).abs();
                let d_cand = (midi - cand as f32).abs();
                if d_cur <= d_cand + self.hysteresis {
                    cur
                } else {
                    cand
                }
            }
            _ => cand,
        };
        self.current = Some(next);
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_roundtrip() {
        assert!((hz_to_midi(440.0) - 69.0).abs() < 1e-5);
        assert!((midi_to_hz(60.0) - 261.6256).abs() < 1e-3);
        for m in [30.0f32, 50.5, 69.0, 90.25] {
            assert!((hz_to_midi(midi_to_hz(m)) - m).abs() < 1e-4);
        }
    }

    #[test]
    fn parse_keys() {
        assert_eq!(parse_key("C"), Some(0));
        assert_eq!(parse_key("f#"), Some(6));
        assert_eq!(parse_key("Bb"), Some(10));
        assert_eq!(parse_key("Cb"), Some(11));
        assert_eq!(parse_key("H"), None);
        assert_eq!(pitch_class_name(9), "A");
    }

    #[test]
    fn fr06_nearest_in_c_major() {
        let c_major = 0b1010_1011_0101;
        assert_eq!(nearest_in_mask(61.2, c_major), Some(62)); // C#+20 → D
        assert_eq!(nearest_in_mask(60.8, c_major), Some(60)); // C#-20 → C
        assert_eq!(nearest_in_mask(66.0, c_major), Some(65)); // F# → F (tie → lower)
        assert_eq!(nearest_in_mask(69.3, 0x0FFF), Some(69));
    }

    #[test]
    fn fr06_single_note_mask_picks_nearest_octave() {
        let only_c = 1;
        assert_eq!(nearest_in_mask(69.0, only_c), Some(72));
        assert_eq!(nearest_in_mask(65.0, only_c), Some(60));
        assert_eq!(nearest_in_mask(69.0, 0), None);
    }

    #[test]
    fn hysteresis_prevents_flip_flop_near_midpoint() {
        let mut s = NoteSnapper::new(0.2);
        assert_eq!(s.snap(60.3, 0x0FFF), Some(60));
        // Midpoint wobble stays on 60.
        for m in [60.45, 60.55, 60.48, 60.58, 60.5] {
            assert_eq!(s.snap(m, 0x0FFF), Some(60));
        }
        // Clearly past the band → switch.
        assert_eq!(s.snap(60.65, 0x0FFF), Some(61));
        assert_eq!(s.snap(60.45, 0x0FFF), Some(61));
        assert_eq!(s.snap(60.35, 0x0FFF), Some(60));
    }

    #[test]
    fn snapper_drops_note_removed_from_mask() {
        let mut s = NoteSnapper::default();
        assert_eq!(s.snap(61.0, 0x0FFF), Some(61));
        let c_major = 0b1010_1011_0101;
        assert_eq!(s.snap(61.0, c_major), Some(60));
    }
}
