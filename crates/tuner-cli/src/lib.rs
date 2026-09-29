//! Library half of `tuner-cli` so golden-file tests can drive the same code
//! paths as the binary.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

use tuner_dsp::{PitchDetector, Tuner, TunerConfig, TuningParams, VoiceRange, MAX_PITCH_HZ};

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("wav: {0}")]
    Wav(#[from] hound::Error),
    #[error("dsp: {0}")]
    Dsp(#[from] tuner_dsp::DspError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

/// Mono audio buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

/// Read any PCM/float WAV and downmix to mono.
pub fn read_wav(path: &Path) -> Result<Audio, CliError> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let ch = usize::from(spec.channels.max(1));
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()?
        }
    };
    let samples = interleaved
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    Ok(Audio {
        sample_rate: spec.sample_rate,
        samples,
    })
}

/// Output sample encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WavBits {
    Int16,
    Float32,
}

pub fn write_wav(path: &Path, audio: &Audio, bits: WavBits) -> Result<(), CliError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: match bits {
            WavBits::Int16 => 16,
            WavBits::Float32 => 32,
        },
        sample_format: match bits {
            WavBits::Int16 => hound::SampleFormat::Int,
            WavBits::Float32 => hound::SampleFormat::Float,
        },
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in &audio.samples {
        match bits {
            WavBits::Int16 => w.write_sample((s.clamp(-1.0, 1.0) * 32767.0).round() as i16)?,
            WavBits::Float32 => w.write_sample(s)?,
        }
    }
    w.finalize()?;
    Ok(())
}

/// Run the tuner over a whole buffer in `block`-sized pieces.
pub fn tune(
    audio: &Audio,
    params: &TuningParams,
    block: usize,
    seed: u64,
) -> Result<Audio, CliError> {
    if block == 0 {
        return Err(CliError::Invalid("block size must be > 0".into()));
    }
    let mut t = Tuner::new(TunerConfig {
        sample_rate: audio.sample_rate as f32,
        seed,
    })?;
    t.set_params(params);
    let mut out = vec![0.0; audio.samples.len()];
    for (i, o) in audio.samples.chunks(block).zip(out.chunks_mut(block)) {
        t.process(i, o);
    }
    Ok(Audio {
        sample_rate: audio.sample_rate,
        samples: out,
    })
}

/// One frame of a pitch track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackFrame {
    pub time_s: f32,
    /// 0 when unvoiced.
    pub hz: f32,
    pub clarity: f32,
}

/// Pitch track with a fixed hop (10 ms), using the same MPM detector as the
/// engine. Frames with clarity below `min_clarity` are reported unvoiced.
pub fn pitch_track(audio: &Audio, range: VoiceRange, min_clarity: f32) -> Vec<TrackFrame> {
    let sr = audio.sample_rate as f32;
    let win = (2.0 * sr / range.min_hz()).ceil() as usize;
    let hop = (sr * 0.01) as usize;
    let mut det = PitchDetector::new(sr, win);
    let mut out = Vec::new();
    let mut start = 0;
    while start + win <= audio.samples.len() {
        let frame = &audio.samples[start..start + win];
        let est = det
            .detect(frame, &[], range.min_hz(), MAX_PITCH_HZ)
            .filter(|e| e.clarity >= min_clarity);
        out.push(TrackFrame {
            time_s: (start + win) as f32 / sr,
            hz: est.map_or(0.0, |e| e.hz),
            clarity: est.map_or(0.0, |e| e.clarity),
        });
        start += hop;
    }
    out
}

pub fn track_to_csv(track: &[TrackFrame]) -> String {
    let mut s = String::from("time_s,hz,clarity\n");
    for f in track {
        s.push_str(&format!("{:.4},{:.3},{:.4}\n", f.time_s, f.hz, f.clarity));
    }
    s
}

pub fn track_from_csv(csv: &str) -> Result<Vec<TrackFrame>, CliError> {
    csv.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Vec<f32> = l
                .split(',')
                .map(|x| x.trim().parse::<f32>())
                .collect::<Result<_, _>>()
                .map_err(|e| CliError::Invalid(format!("bad csv line {l:?}: {e}")))?;
            match v[..] {
                [time_s, hz, clarity] => Ok(TrackFrame {
                    time_s,
                    hz,
                    clarity,
                }),
                _ => Err(CliError::Invalid(format!("bad csv line {l:?}"))),
            }
        })
        .collect()
}

/// The deterministic "vocal" phrase used by golden tests.
pub fn golden_vocal(sample_rate: u32) -> Audio {
    let notes = [
        (0.10, 0.60, 196.0 * 1.012), // G3, 21 cents sharp
        (0.70, 1.20, 240.0),         // A#3 +49 cents → B3 (A#/Bb not in C major)
        (1.30, 1.80, 262.9),         // C4 +8 cents
        (1.90, 2.40, 318.0),         // D#4 +38 cents → E4 (D# not in C major)
    ];
    Audio {
        sample_rate,
        samples: tuner_dsp::signals::vocal_phrase(
            &notes,
            sample_rate as f32,
            (sample_rate as f32 * 2.5) as usize,
            2026,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip() {
        let t = vec![
            TrackFrame {
                time_s: 0.01,
                hz: 220.0,
                clarity: 0.95,
            },
            TrackFrame {
                time_s: 0.02,
                hz: 0.0,
                clarity: 0.0,
            },
        ];
        let back = track_from_csv(&track_to_csv(&t)).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn wav_roundtrip_float_and_int() {
        let dir = std::env::temp_dir().join(format!("tuner-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = Audio {
            sample_rate: 44_100,
            samples: vec![0.0, 0.5, -0.5, 0.25],
        };
        for bits in [WavBits::Float32, WavBits::Int16] {
            let p = dir.join(format!("{bits:?}.wav"));
            write_wav(&p, &a, bits).unwrap();
            let b = read_wav(&p).unwrap();
            assert_eq!(b.sample_rate, 44_100);
            for (x, y) in a.samples.iter().zip(&b.samples) {
                assert!((x - y).abs() < 1e-4);
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn zero_block_is_rejected() {
        let a = Audio {
            sample_rate: 48_000,
            samples: vec![0.0; 10],
        };
        assert!(tune(&a, &TuningParams::default(), 0, 0).is_err());
    }
}
