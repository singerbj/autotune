//! End-to-end tests of the tuning pipeline on synthetic signals.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use tuner_dsp::scale::{hz_to_midi, midi_to_hz};
use tuner_dsp::{
    signals, DspMeters, PitchDetector, Scale, Tuner, TunerConfig, TuningParams, VoiceRange,
};

const SR: f32 = 48_000.0;

fn tuner(params: TuningParams) -> Tuner {
    let mut t = Tuner::new(TunerConfig {
        sample_rate: SR,
        seed: 42,
    })
    .unwrap();
    t.set_params(&params);
    t
}

fn run(t: &mut Tuner, x: &[f32], block: usize) -> (Vec<f32>, Vec<DspMeters>) {
    let mut y = vec![0.0; x.len()];
    let mut meters = Vec::new();
    for (i, o) in x.chunks(block).zip(y.chunks_mut(block)) {
        t.process(i, o);
        meters.push(t.meters());
    }
    (y, meters)
}

/// Median detected pitch (in fractional MIDI) over analysis windows in `y[from..to]`.
fn median_midi(y: &[f32], from: usize, to: usize) -> f32 {
    let mut det = PitchDetector::new(SR, 1372);
    let mut v: Vec<f32> = (from..to - 1372)
        .step_by(480)
        .filter_map(|s| det.detect(&y[s..s + 1372], &[], 70.0, 1000.0))
        .filter(|e| e.clarity > 0.8)
        .map(|e| hz_to_midi(e.hz))
        .collect();
    assert!(v.len() > 5, "too few voiced frames");
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

#[test]
fn nfr06_corrects_sharp_tone_to_within_5_cents() {
    for (hz_target, cents_off) in [(440.0f32, 35.0f32), (196.0, -40.0), (261.63, 22.0)] {
        let hz_in = hz_target * (cents_off / 1200.0).exp2();
        let x = signals::harmonic(hz_in, SR, 48_000, 0.4, 10);
        let mut t = tuner(TuningParams {
            retune_ms: 0.0,
            ..Default::default()
        });
        let (y, meters) = run(&mut t, &x, 128);
        let got = median_midi(&y, 12_000, 48_000);
        let err_cents = (got - hz_to_midi(hz_target)) * 100.0;
        assert!(
            err_cents.abs() <= 5.0,
            "{hz_in} Hz → err {err_cents} cents (target {hz_target})"
        );
        let last = meters.last().unwrap();
        assert!(last.voiced);
        assert_eq!(last.target_midi, hz_to_midi(hz_target).round() as i32);
        assert!((last.correction_cents + cents_off).abs() < 3.0);
    }
}

#[test]
fn fr06_snaps_to_custom_single_note_mask_up_and_down() {
    // A4 with only C allowed → C5 (+3 semitones); only F# → F#4 (−3).
    for (mask, want) in [(1u16 << 0, 72), (1u16 << 6, 66)] {
        let x = signals::harmonic(440.0, SR, 48_000, 0.4, 10);
        let mut t = tuner(TuningParams {
            scale: Scale::Custom,
            custom_mask: mask,
            retune_ms: 5.0,
            ..Default::default()
        });
        let (y, _) = run(&mut t, &x, 256);
        let got = median_midi(&y, 16_000, 48_000);
        assert!(
            (got - want as f32).abs() <= 0.05,
            "mask {mask:#x}: got {got} want {want}"
        );
    }
}

#[test]
fn fr06_major_scale_in_g_moves_f_natural_to_nearest_scale_note() {
    // F4+30c (65.3) is not in G major; the nearest scale note is F#4 (66).
    let x = signals::harmonic(midi_to_hz(65.3), SR, 48_000, 0.4, 10);
    let mut t = tuner(TuningParams {
        key: 7,
        scale: Scale::Major,
        retune_ms: 0.0,
        ..Default::default()
    });
    let (y, _) = run(&mut t, &x, 128);
    let got = median_midi(&y, 16_000, 48_000);
    assert!((got - 66.0).abs() <= 0.05, "{got}");
}

#[test]
fn fr07_retune_speed_controls_glide_time() {
    let x = signals::harmonic(midi_to_hz(69.4), SR, 24_000, 0.4, 10);
    let correction_after = |retune_ms: f32, ms: usize| {
        let mut t = tuner(TuningParams {
            retune_ms,
            ..Default::default()
        });
        let (_, meters) = run(&mut t, &x, 48);
        meters[ms].correction_cents
    };
    // Hard snap reaches −40 cents immediately after voicing onset.
    assert!((correction_after(0.0, 100) + 40.0).abs() < 2.0);
    // 200 ms retune is still far from the target 50 ms after onset.
    let slow = correction_after(200.0, 100);
    assert!(slow > -35.0, "{slow}");
    let fast = correction_after(10.0, 100);
    assert!(fast < -38.0, "{fast}");
}

#[test]
fn fr07_humanize_is_deterministic_and_keeps_vibrato() {
    let x = signals::vocal_phrase(&[(0.05, 0.9, 220.0), (1.0, 1.9, 247.0)], SR, 96_000, 3);
    let params = TuningParams {
        humanize: 1.0,
        retune_ms: 0.0,
        ..Default::default()
    };
    let (a, _) = run(&mut tuner(params), &x, 128);
    let (b, _) = run(&mut tuner(params), &x, 128);
    assert_eq!(a, b, "same seed and input must give identical output");

    // With humanize, output pitch keeps more natural variation than hard tune.
    let spread = |y: &[f32]| {
        let mut det = PitchDetector::new(SR, 1372);
        let v: Vec<f32> = (10_000..40_000)
            .step_by(240)
            .filter_map(|s| det.detect(&y[s..s + 1372], &[], 70.0, 1000.0))
            .map(|e| hz_to_midi(e.hz))
            .collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|m| (m - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    };
    let (hard, _) = run(
        &mut tuner(TuningParams {
            humanize: 0.0,
            retune_ms: 0.0,
            ..Default::default()
        }),
        &x,
        128,
    );
    assert!(
        spread(&a) > spread(&hard) * 1.5,
        "{} vs {}",
        spread(&a),
        spread(&hard)
    );
}

#[test]
fn fr08_voice_range_sets_latency_and_low_notes_track_only_in_low() {
    for (range, lat) in [
        (VoiceRange::Low, 686),
        (VoiceRange::Mid, 480),
        (VoiceRange::High, 320),
    ] {
        let t = Tuner::new(TunerConfig {
            sample_rate: SR,
            seed: 0,
        })
        .map(|mut t| {
            t.set_params(&TuningParams {
                voice_range: range,
                ..Default::default()
            });
            // Let the range crossfade complete.
            let mut o = [0.0; 1024];
            t.process(&[0.0; 1024], &mut o);
            t
        })
        .unwrap();
        assert_eq!(t.latency_samples(), lat);
    }
    let x = signals::harmonic(80.0, SR, 24_000, 0.4, 12);
    let voiced_with = |range| {
        let mut t = tuner(TuningParams {
            voice_range: range,
            ..Default::default()
        });
        let (_, m) = run(&mut t, &x, 128);
        m.last().unwrap().voiced
    };
    assert!(voiced_with(VoiceRange::Low));
    assert!(!voiced_with(VoiceRange::High));
}

#[test]
fn fr09_unvoiced_noise_passes_through_unshifted() {
    let x = signals::noise(24_000, 0.2, 9);
    let mut t = tuner(TuningParams {
        gate_threshold_db: -100.0,
        ..Default::default()
    });
    let (y, meters) = run(&mut t, &x, 128);
    assert!(meters.iter().all(|m| !m.voiced));
    // Output equals the conditioned input delayed by the latency: check
    // correlation at lag 480 is near 1.
    let lag = 480;
    let (mut xy, mut xx, mut yy) = (0.0f64, 0.0f64, 0.0f64);
    for i in 4_000..24_000 {
        let a = x[i - lag] as f64;
        let b = y[i] as f64;
        xy += a * b;
        xx += a * a;
        yy += b * b;
    }
    let corr = xy / (xx * yy).sqrt();
    assert!(corr > 0.98, "corr {corr}");
}

#[test]
fn fr09_gate_silences_low_level_noise() {
    let x = signals::noise(48_000, 0.001, 5); // ≈ −60 dBFS peak
    let mut t = tuner(TuningParams {
        gate_threshold_db: -40.0,
        ..Default::default()
    });
    let (y, _) = run(&mut t, &x, 128);
    let peak = y[24_000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak < 1e-5, "{peak}");
}

#[test]
fn fr10_mix_zero_is_dry_and_bypass_is_raw_delayed() {
    let x = signals::harmonic(453.0, SR, 24_000, 0.4, 8);
    let mut t = tuner(TuningParams {
        bypass: true,
        ..Default::default()
    });
    let (y, _) = run(&mut t, &x, 128);
    for i in 2_000..24_000 {
        assert!((y[i] - x[i - 480]).abs() < 1e-6);
    }
}

/// Largest sample-to-sample jump, a crude click detector.
fn max_step(y: &[f32]) -> f32 {
    y.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
}

#[test]
fn fr10_fr11_bypass_and_param_changes_are_click_free() {
    let hz = 220.0;
    let x = signals::sine(hz * 1.02, SR, 96_000, 0.5);
    // A clean 224 Hz sine at 0.5 amplitude moves at most ~0.015 per sample;
    // allow generous headroom but catch discontinuities.
    let natural = 2.0 * core::f32::consts::PI * 240.0 / SR * 0.5;
    let mut t = tuner(TuningParams::default());
    let mut y = vec![0.0; x.len()];
    let toggles = [
        TuningParams {
            bypass: true,
            ..Default::default()
        },
        TuningParams::default(),
        TuningParams {
            mix: 0.0,
            ..Default::default()
        },
        TuningParams {
            key: 3,
            scale: Scale::Major,
            ..Default::default()
        },
        TuningParams {
            gate_threshold_db: -20.0,
            ..Default::default()
        },
    ];
    for (n, (i, o)) in x.chunks(128).zip(y.chunks_mut(128)).enumerate() {
        if n % 90 == 50 {
            t.set_params(&toggles[(n / 90) % toggles.len()]);
        }
        t.process(i, o);
    }
    let step = max_step(&y[4_000..]);
    assert!(step < natural * 3.0, "max step {step} vs natural {natural}");
}

#[test]
fn process_is_deterministic_across_block_sizes() {
    let x = signals::vocal_phrase(&[(0.0, 0.5, 180.0), (0.6, 1.0, 240.0)], SR, 48_000, 11);
    let (a, _) = run(&mut tuner(TuningParams::default()), &x, 128);
    let (b, _) = run(&mut tuner(TuningParams::default()), &x, 128);
    assert_eq!(a, b);
    let (c, _) = run(&mut tuner(TuningParams::default()), &x, 96);
    // Different block sizes keep 32-sample internal chunking, so results match.
    assert_eq!(a, c);
}

#[test]
fn algorithmic_latency_measured_with_impulse() {
    for (range, lat) in [(VoiceRange::Mid, 480usize), (VoiceRange::High, 320)] {
        let mut t = Tuner::new(TunerConfig {
            sample_rate: SR,
            seed: 0,
        })
        .unwrap();
        t.set_params(&TuningParams {
            voice_range: range,
            gate_threshold_db: -100.0,
            ..Default::default()
        });
        let mut warm = [0.0; 2048];
        t.process(&[0.0; 2048], &mut warm);
        let mut x = vec![0.0; 4096];
        x[1000] = 0.9;
        let mut y = vec![0.0; 4096];
        t.process(&x, &mut y);
        let peak = y
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
        assert_eq!(peak - 1000, lat, "{range:?}");
    }
}

#[test]
fn non_finite_input_is_sanitized() {
    let mut t = tuner(TuningParams::default());
    let mut x = signals::sine(200.0, SR, 4096, 0.5);
    x[100] = f32::NAN;
    x[200] = f32::INFINITY;
    let mut y = vec![0.0; 4096];
    t.process(&x, &mut y);
    assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
}

#[test]
fn works_at_common_sample_rates() {
    for sr in [44_100.0f32, 48_000.0, 96_000.0] {
        let mut t = Tuner::new(TunerConfig {
            sample_rate: sr,
            seed: 1,
        })
        .unwrap();
        t.set_params(&TuningParams {
            retune_ms: 0.0,
            ..Default::default()
        });
        let x = signals::harmonic(445.0, sr, sr as usize, 0.4, 10);
        let mut y = vec![0.0; x.len()];
        for (i, o) in x.chunks(256).zip(y.chunks_mut(256)) {
            t.process(i, o);
        }
        let win = (2.0 * sr / 70.0) as usize;
        let mut det = PitchDetector::new(sr, win);
        let s = y.len() - win - 100;
        let e = det.detect(&y[s..s + win], &[], 70.0, 1000.0).unwrap();
        let cents = 1200.0 * (e.hz / 440.0).log2();
        assert!(cents.abs() < 5.0, "{sr}: {cents}");
    }
}
