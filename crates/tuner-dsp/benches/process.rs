//! NFR-03 gate: one 128-sample block must process in < 0.27 ms at 48 kHz.
//! CI parses `target/criterion/process_128/voiced_mid/new/estimates.json`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use tuner_dsp::{signals, Scale, Tuner, TunerConfig, TuningParams, VoiceRange};

fn bench(c: &mut Criterion) {
    let sr = 48_000.0;
    let input = signals::vocal_phrase(&[(0.0, 10.0, 196.0)], sr, 48_000, 1);
    let mut group = c.benchmark_group("process_128");
    for (name, range) in [
        ("voiced_mid", VoiceRange::Mid),
        ("voiced_low", VoiceRange::Low),
    ] {
        let mut tuner = Tuner::new(TunerConfig {
            sample_rate: sr,
            seed: 1,
        })
        .expect("tuner");
        tuner.set_params(&TuningParams {
            scale: Scale::Major,
            voice_range: range,
            retune_ms: 10.0,
            ..Default::default()
        });
        let mut out = [0.0f32; 128];
        // Warm up past the range fade and into the voiced state.
        for block in input.chunks_exact(128).take(100) {
            tuner.process(block, &mut out);
        }
        let mut pos = 0usize;
        group.bench_function(name, |b| {
            b.iter(|| {
                let block = &input[pos..pos + 128];
                pos = (pos + 128) % (input.len() - 128);
                tuner.process(black_box(block), &mut out);
                black_box(&out);
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
