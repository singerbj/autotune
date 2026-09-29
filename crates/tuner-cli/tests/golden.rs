//! Golden-file tests (M1): the committed vocal WAV is tuned to C major and its
//! pitch track compared with the committed reference track.
//!
//! Regenerate references after an intentional DSP change with
//! `UPDATE_GOLDEN=1 cargo test -p tuner-cli --test golden`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use tuner_cli::{
    golden_vocal, pitch_track, read_wav, track_from_csv, track_to_csv, tune, write_wav, WavBits,
};
use tuner_dsp::scale::hz_to_midi;
use tuner_dsp::{Scale, TuningParams, VoiceRange};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn update() -> bool {
    std::env::var_os("UPDATE_GOLDEN").is_some()
}

/// Loads the golden input, regenerating it first when `UPDATE_GOLDEN` is set.
fn golden_input() -> tuner_cli::Audio {
    let path = golden_dir().join("vocal_in.wav");
    if update() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| write_wav(&path, &golden_vocal(48_000), WavBits::Int16).unwrap());
    }
    read_wav(&path).expect("golden input missing; run with UPDATE_GOLDEN=1")
}

const C_MAJOR: u16 = 0b1010_1011_0101;

fn params() -> TuningParams {
    TuningParams {
        key: 0,
        scale: Scale::Major,
        retune_ms: 5.0,
        ..Default::default()
    }
}

#[test]
fn golden_input_is_stable() {
    let stored = golden_input();
    let fresh = golden_vocal(48_000);
    assert_eq!(stored.samples.len(), fresh.samples.len());
    for (a, b) in stored.samples.iter().zip(&fresh.samples) {
        assert!((a - b).abs() < 1.0 / 16_000.0, "generator drifted");
    }
}

#[test]
fn golden_c_major_track_matches_reference() {
    let input = golden_input();
    let out = tune(&input, &params(), 128, 0x5EED).unwrap();
    let track = pitch_track(&out, VoiceRange::Low, 0.8);
    let ref_path = golden_dir().join("vocal_c_major.track.csv");
    if update() {
        std::fs::write(&ref_path, track_to_csv(&track)).unwrap();
    }
    let reference = track_from_csv(&std::fs::read_to_string(&ref_path).unwrap()).unwrap();
    assert_eq!(track.len(), reference.len());

    let mut agree = 0;
    let mut both = 0;
    for (a, b) in track.iter().zip(&reference) {
        if (a.hz > 0.0) == (b.hz > 0.0) {
            agree += 1;
        }
        if a.hz > 0.0 && b.hz > 0.0 {
            both += 1;
            let cents = 1200.0 * (a.hz / b.hz).log2();
            assert!(cents.abs() <= 10.0, "t={} {cents} cents", a.time_s);
        }
    }
    let ratio = agree as f32 / track.len() as f32;
    assert!(ratio >= 0.95, "voicing agreement {ratio}");
    assert!(both > 100);
}

#[test]
fn golden_output_lands_on_c_major_notes() {
    let input = golden_input();
    let out = tune(&input, &params(), 128, 0x5EED).unwrap();
    let track = pitch_track(&out, VoiceRange::Low, 0.9);
    // Skip note onsets (retune glide + voicing crossfade) by checking the
    // middle of each sung note only.
    let notes = [(0.25, 0.5), (0.85, 1.1), (1.45, 1.7), (2.05, 2.3)];
    for (from, to) in notes {
        let mut devs: Vec<f32> = track
            .iter()
            .filter(|f| f.time_s >= from && f.time_s <= to && f.hz > 0.0)
            .map(|f| {
                let m = hz_to_midi(f.hz);
                let n = m.round() as i32;
                assert!(
                    C_MAJOR & (1 << n.rem_euclid(12)) != 0,
                    "t={} note {n} not in C major",
                    f.time_s
                );
                (m - n as f32) * 100.0
            })
            .collect();
        assert!(devs.len() > 10, "note {from}-{to} not voiced");
        devs.sort_by(|a, b| a.total_cmp(b));
        let median = devs[devs.len() / 2];
        // Vibrato is kept by the detector window; the median sits on the note.
        assert!(
            median.abs() <= 5.0,
            "note {from}-{to}: median {median} cents"
        );
    }
}
