//! `tuner-cli`: offline harness for the tuning DSP (milestone M1).
//!
//! ```text
//! tuner-cli tune  -i in.wav -o out.wav --key A --scale minor --retune-ms 10
//! tuner-cli tune  -i in.wav -o out.wav --key A --scale minor --style chrome-snap
//! tuner-cli track -i out.wav --csv out.csv
//! tuner-cli gen   --kind vocal -o golden.wav
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use tuner_cli::{
    golden_vocal, pitch_track, read_wav, track_to_csv, tune, write_wav, Audio, CliError, WavBits,
};
use tuner_dsp::scale::parse_key;
use tuner_dsp::{signals, FxParams, Scale, Style, TuningParams, VoiceRange};

#[derive(Parser)]
#[command(version, about = "Offline harness for the voice tuner DSP")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum ScaleArg {
    Chromatic,
    Major,
    Minor,
    Custom,
}

#[derive(Clone, Copy, ValueEnum)]
enum RangeArg {
    Low,
    Mid,
    High,
}

impl From<RangeArg> for VoiceRange {
    fn from(r: RangeArg) -> Self {
        match r {
            RangeArg::Low => VoiceRange::Low,
            RangeArg::Mid => VoiceRange::Mid,
            RangeArg::High => VoiceRange::High,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Tone,
    Vocal,
    Noise,
}

#[derive(Subcommand)]
enum Cmd {
    /// Tune a WAV file (FR-06 … FR-11).
    Tune {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Key root, e.g. C, F#, Bb.
        #[arg(long, default_value = "C")]
        key: String,
        #[arg(long, value_enum, default_value_t = ScaleArg::Chromatic)]
        scale: ScaleArg,
        /// Notes for --scale custom, comma separated (e.g. C,E,G).
        #[arg(long, value_delimiter = ',')]
        notes: Vec<String>,
        #[arg(long, default_value_t = 20.0)]
        retune_ms: f32,
        #[arg(long, default_value_t = 0.0)]
        humanize: f32,
        #[arg(long, value_enum, default_value_t = RangeArg::Mid)]
        range: RangeArg,
        #[arg(long, default_value_t = 1.0)]
        mix: f32,
        #[arg(long, default_value_t = -60.0, allow_hyphen_values = true)]
        gate_db: f32,
        /// Hard tune: instant note switches, retune and humanize ignored (FR-23).
        #[arg(long)]
        hard_tune: bool,
        /// Formant shift in semitones, -4 … 4 (FR-24).
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        formant: f32,
        /// Built-in style applied on top of the other flags (FR-27):
        /// chrome-snap, velvet-echo, night-drive or natural.
        #[arg(long)]
        style: Option<String>,
        #[arg(long, default_value_t = 128)]
        block: usize,
        #[arg(long, default_value_t = 0x5EED)]
        seed: u64,
        /// Write 16-bit PCM instead of 32-bit float.
        #[arg(long)]
        pcm16: bool,
    },
    /// Print or save the pitch track of a WAV file.
    Track {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long)]
        csv: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = RangeArg::Low)]
        range: RangeArg,
        #[arg(long, default_value_t = 0.8)]
        min_clarity: f32,
    },
    /// Generate a deterministic test signal.
    Gen {
        #[arg(long, value_enum)]
        kind: Kind,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 440.0)]
        hz: f32,
        #[arg(long, default_value_t = 2.0)]
        seconds: f32,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.cmd {
        Cmd::Tune {
            input,
            output,
            key,
            scale,
            notes,
            retune_ms,
            humanize,
            range,
            mix,
            gate_db,
            hard_tune,
            formant,
            style,
            block,
            seed,
            pcm16,
        } => {
            let key =
                parse_key(&key).ok_or_else(|| CliError::Invalid(format!("bad key {key:?}")))?;
            let mut custom_mask = 0u16;
            for n in &notes {
                let pc =
                    parse_key(n).ok_or_else(|| CliError::Invalid(format!("bad note {n:?}")))?;
                custom_mask |= 1 << pc;
            }
            let params = TuningParams {
                key,
                scale: match scale {
                    ScaleArg::Chromatic => Scale::Chromatic,
                    ScaleArg::Major => Scale::Major,
                    ScaleArg::Minor => Scale::NaturalMinor,
                    ScaleArg::Custom => Scale::Custom,
                },
                custom_mask,
                retune_ms,
                humanize,
                voice_range: range.into(),
                mix,
                gate_threshold_db: gate_db,
                bypass: false,
                hard_tune,
                formant_semitones: formant,
                fx: FxParams::default(),
            };
            let params = match style {
                Some(id) => Style::from_id(&id)
                    .ok_or_else(|| CliError::Invalid(format!("unknown style {id:?}")))?
                    .apply(&params),
                None => params,
            };
            let audio = read_wav(&input)?;
            let started = Instant::now();
            let out = tune(&audio, &params, block, seed)?;
            let secs = audio.samples.len() as f64 / f64::from(audio.sample_rate);
            eprintln!(
                "tuned {secs:.2}s of audio in {:.1} ms ({:.0}x real time)",
                started.elapsed().as_secs_f64() * 1e3,
                secs / started.elapsed().as_secs_f64().max(1e-9)
            );
            write_wav(
                &output,
                &out,
                if pcm16 {
                    WavBits::Int16
                } else {
                    WavBits::Float32
                },
            )
        }
        Cmd::Track {
            input,
            csv,
            range,
            min_clarity,
        } => {
            let audio = read_wav(&input)?;
            let text = track_to_csv(&pitch_track(&audio, range.into(), min_clarity));
            match csv {
                Some(p) => std::fs::write(p, text)?,
                None => print!("{text}"),
            }
            Ok(())
        }
        Cmd::Gen {
            kind,
            output,
            hz,
            seconds,
            sample_rate,
        } => {
            let sr = sample_rate as f32;
            let len = (sr * seconds) as usize;
            let samples = match kind {
                Kind::Tone => signals::harmonic(hz, sr, len, 0.5, 12),
                Kind::Noise => signals::noise(len, 0.3, 1),
                Kind::Vocal => golden_vocal(sample_rate).samples,
            };
            write_wav(
                &output,
                &Audio {
                    sample_rate,
                    samples,
                },
                WavBits::Int16,
            )
        }
    }
}
