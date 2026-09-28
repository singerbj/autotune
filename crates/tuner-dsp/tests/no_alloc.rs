//! Hard rule: `process()` never allocates. The soak test runs the equivalent
//! of one hour of audio under `assert_no_alloc` (run with `--ignored`, in
//! release mode, in CI).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use tuner_dsp::{signals, Scale, Tuner, TunerConfig, TuningParams, VoiceRange};

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

const SR: f32 = 48_000.0;

fn phrase() -> Vec<f32> {
    let mut x = signals::vocal_phrase(
        &[(0.0, 0.8, 150.0), (1.0, 1.7, 330.0), (2.0, 2.9, 90.0)],
        SR,
        144_000,
        4,
    );
    x.extend(signals::noise(48_000, 0.1, 2));
    x
}

fn param_cycle(i: usize) -> TuningParams {
    TuningParams {
        key: (i % 12) as u8,
        scale: [
            Scale::Chromatic,
            Scale::Major,
            Scale::NaturalMinor,
            Scale::Custom,
        ][i % 4],
        custom_mask: 0b1001_0001_0001,
        retune_ms: (i % 5) as f32 * 40.0,
        humanize: (i % 3) as f32 * 0.5,
        voice_range: [VoiceRange::Mid, VoiceRange::Low, VoiceRange::High][i % 3],
        mix: 1.0 - (i % 2) as f32 * 0.3,
        gate_threshold_db: -60.0,
        bypass: i % 7 == 6,
    }
}

fn run_seconds(seconds: usize) {
    let x = phrase();
    let mut t = Tuner::new(TunerConfig {
        sample_rate: SR,
        seed: 9,
    })
    .unwrap();
    let mut out = [0.0f32; 128];
    let blocks = seconds * SR as usize / 128;
    let mut pos = 0;
    for b in 0..blocks {
        let p = if b % 1500 == 0 {
            Some(param_cycle(b / 1500))
        } else {
            None
        };
        let block = &x[pos..pos + 128];
        assert_no_alloc(|| {
            if let Some(p) = p {
                t.set_params(&p);
            }
            t.process(block, &mut out);
            let _ = t.meters();
        });
        assert!(out.iter().all(|v| v.is_finite()));
        pos = (pos + 128) % (x.len() - 128);
    }
}

#[test]
fn process_does_not_allocate() {
    run_seconds(20);
}

/// Architecture › Automated tests: "A soak test runs process() offline for the
/// equivalent of 1 hour with assert_no_alloc enabled."
#[test]
#[ignore = "long: run with `cargo test --release -- --ignored soak`"]
fn soak_one_hour_no_alloc() {
    run_seconds(3600);
}
