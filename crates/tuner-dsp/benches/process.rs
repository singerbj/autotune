//! NFR-03 gate: one 128-sample block must process in < 0.27 ms at 48 kHz.
//! CI parses `target/criterion/process_128/voiced_mid/new/estimates.json`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use tuner_dsp::{signals, Scale, Style, Tuner, TunerConfig, TuningParams, VoiceRange};

fn bench(c: &mut Criterion) {
    let sr = 48_000.0;
    let input = signals::vocal_phrase(&[(0.0, 10.0, 196.0)], sr, 48_000, 1);
    let mut group = c.benchmark_group("process_128");
    let plain = |range| TuningParams {
        scale: Scale::Major,
        voice_range: range,
        retune_ms: 10.0,
        ..Default::default()
    };
    // `voiced_mid` is the NFR-03 gate; `styled_mid` runs every effect, a
    // formant shift and the split outputs (FR-23 – FR-27).
    for (name, params, split) in [
        ("voiced_mid", plain(VoiceRange::Mid), false),
        ("voiced_low", plain(VoiceRange::Low), false),
        (
            "styled_mid",
            TuningParams {
                formant_semitones: -1.0,
                fx: tuner_dsp::FxParams {
                    doubler_mix: 0.3,
                    ..Style::ChromeSnap.apply(&plain(VoiceRange::Mid)).fx
                },
                ..Style::ChromeSnap.apply(&plain(VoiceRange::Mid))
            },
            true,
        ),
    ] {
        let mut tuner = Tuner::new(TunerConfig {
            sample_rate: sr,
            seed: 1,
        })
        .expect("tuner");
        tuner.set_params(&params);
        let mut out = [0.0f32; 128];
        let mut cable = [0.0f32; 128];
        // Warm up past the range fade and into the voiced state.
        for block in input.chunks_exact(128).take(100) {
            tuner.process(block, &mut out);
        }
        let mut pos = 0usize;
        group.bench_function(name, |b| {
            b.iter(|| {
                let block = &input[pos..pos + 128];
                pos = (pos + 128) % (input.len() - 128);
                if split {
                    tuner.process_split(black_box(block), &mut out, &mut cable);
                } else {
                    tuner.process(black_box(block), &mut out);
                }
                black_box((&out, &cable));
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
