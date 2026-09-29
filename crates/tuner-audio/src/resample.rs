//! Clock bridging between two streams.
//!
//! The producer (capture thread) pushes mono samples into an `rtrb` ring at
//! its own clock; a render thread pulls them through a [`RingReader`]:
//!
//! * [`RingReader::direct`] when both ends share one clock (same USB device,
//!   same rate): keeps the fill near one period and drops the excess.
//! * [`RingReader::resampled`] otherwise: a `rubato` polynomial resampler
//!   whose ratio is steered by a PI loop on ring fill.
//!
//! Everything is allocated at construction; `read` is real-time safe.

use rtrb::Consumer;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Adjustable, Async, FixedAsync, PolynomialDegree, Resampler};

use crate::types::AudioError;

/// PI gains on fill error measured in seconds.
const KP: f64 = 1.0;
const KI: f64 = 0.1;
/// Largest deviation of the resampling ratio from nominal.
const MAX_REL: f64 = 0.01;
/// Output chunk of the resampler, in frames.
const CHUNK: usize = 32;

/// What happened during one `read`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReadReport {
    pub underrun: bool,
    pub dropped: usize,
}

enum Mode {
    Direct,
    Resampled {
        rs: Box<Async<f32>>,
        in_buf: Vec<f32>,
        out_buf: Vec<f32>,
        out_pos: usize,
        out_len: usize,
        integral: f64,
        rel: f64,
    },
}

/// Reads a ring written at another clock. See the module docs.
pub struct RingReader {
    rx: Consumer<f32>,
    mode: Mode,
    in_rate: f64,
    out_rate: f64,
    target: usize,
    max_fill: usize,
    fill_avg: f64,
    primed: bool,
    underruns: u64,
    dropped: u64,
}

impl core::fmt::Debug for RingReader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RingReader")
            .field("in_rate", &self.in_rate)
            .field("out_rate", &self.out_rate)
            .field("target", &self.target)
            .field("resampled", &matches!(self.mode, Mode::Resampled { .. }))
            .finish()
    }
}

impl RingReader {
    /// Same clock: no resampling. `target` is the fill (frames) to hold.
    pub fn direct(rx: Consumer<f32>, rate: u32, target: usize) -> Self {
        let target = target.max(1);
        let cap = rx.buffer().capacity();
        Self {
            rx,
            mode: Mode::Direct,
            in_rate: f64::from(rate),
            out_rate: f64::from(rate),
            target,
            max_fill: (target * 3).max(target + 256).min(cap),
            fill_avg: target as f64,
            primed: false,
            underruns: 0,
            dropped: 0,
        }
    }

    /// Different clocks and/or rates: adaptive resampling.
    pub fn resampled(
        rx: Consumer<f32>,
        in_rate: u32,
        out_rate: u32,
        target: usize,
    ) -> Result<Self, AudioError> {
        let ratio = f64::from(out_rate) / f64::from(in_rate.max(1));
        let rs = Async::<f32>::new_poly(
            ratio,
            1.0 + 2.0 * MAX_REL,
            PolynomialDegree::Cubic,
            CHUNK,
            1,
            FixedAsync::Output,
        )
        .map_err(|e| AudioError::Thread(format!("resampler: {e}")))?;
        let in_max = rs.input_frames_max();
        let out_max = rs.output_frames_max();
        let target = target.max(in_max + 1);
        let cap = rx.buffer().capacity();
        Ok(Self {
            rx,
            mode: Mode::Resampled {
                rs: Box::new(rs),
                in_buf: vec![0.0; in_max],
                out_buf: vec![0.0; out_max],
                out_pos: 0,
                out_len: 0,
                integral: 0.0,
                rel: 1.0,
            },
            in_rate: f64::from(in_rate),
            out_rate: f64::from(out_rate),
            target,
            max_fill: (target * 4).min(cap),
            fill_avg: target as f64,
            primed: false,
            underruns: 0,
            dropped: 0,
        })
    }

    pub fn is_resampling(&self) -> bool {
        matches!(self.mode, Mode::Resampled { .. })
    }

    /// Current ratio relative to nominal (1.0 when direct).
    pub fn relative_ratio(&self) -> f64 {
        match &self.mode {
            Mode::Direct => 1.0,
            Mode::Resampled { rel, .. } => *rel,
        }
    }

    /// Smoothed ring fill in frames of the producer clock.
    pub fn fill(&self) -> f64 {
        self.fill_avg
    }

    pub fn fill_ms(&self) -> f32 {
        (self.fill_avg * 1000.0 / self.in_rate) as f32
    }

    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// True once the writer side has been dropped.
    pub fn is_abandoned(&self) -> bool {
        self.rx.is_abandoned()
    }

    fn pop_into(rx: &mut Consumer<f32>, dst: &mut [f32]) -> usize {
        let n = dst.len().min(rx.slots());
        if n == 0 {
            return 0;
        }
        match rx.read_chunk(n) {
            Ok(chunk) => {
                let (a, b) = chunk.as_slices();
                dst[..a.len()].copy_from_slice(a);
                dst[a.len()..a.len() + b.len()].copy_from_slice(b);
                chunk.commit_all();
                n
            }
            Err(_) => 0,
        }
    }

    fn skip(rx: &mut Consumer<f32>, n: usize) -> usize {
        let n = n.min(rx.slots());
        match rx.read_chunk(n) {
            Ok(chunk) => {
                chunk.commit_all();
                n
            }
            Err(_) => 0,
        }
    }

    /// Fill `out` completely (zeros where no data is available).
    pub fn read(&mut self, out: &mut [f32]) -> ReadReport {
        let mut report = ReadReport::default();
        let avail = self.rx.slots();

        if !self.primed {
            if avail >= self.target {
                self.primed = true;
            } else {
                out.fill(0.0);
                return report;
            }
        }

        let dt = out.len() as f64 / self.out_rate;
        match &mut self.mode {
            Mode::Direct => {
                let got = Self::pop_into(&mut self.rx, out);
                if got < out.len() {
                    out[got..].fill(0.0);
                    report.underrun = true;
                }
                let fill = self.rx.slots();
                if fill > self.max_fill {
                    report.dropped = Self::skip(&mut self.rx, fill - self.target);
                }
            }
            Mode::Resampled {
                rs,
                in_buf,
                out_buf,
                out_pos,
                out_len,
                integral,
                rel,
            } => {
                let mut i = 0;
                while i < out.len() {
                    if *out_pos == *out_len {
                        let need = rs.input_frames_next();
                        if self.rx.slots() < need {
                            out[i..].fill(0.0);
                            report.underrun = true;
                            break;
                        }
                        Self::pop_into(&mut self.rx, &mut in_buf[..need]);
                        let out_frames = out_buf.len();
                        let res =
                            InterleavedSlice::new(&in_buf[..need], 1, need).and_then(|input| {
                                let mut output =
                                    InterleavedSlice::new_mut(&mut out_buf[..], 1, out_frames)?;
                                Ok(rs.process_into_buffer(&input, &mut output, None))
                            });
                        match res {
                            Ok(Ok((_, produced))) => {
                                *out_len = produced;
                                *out_pos = 0;
                            }
                            _ => {
                                out[i..].fill(0.0);
                                report.underrun = true;
                                break;
                            }
                        }
                        if *out_len == 0 {
                            continue;
                        }
                    }
                    let n = (*out_len - *out_pos).min(out.len() - i);
                    out[i..i + n].copy_from_slice(&out_buf[*out_pos..*out_pos + n]);
                    *out_pos += n;
                    i += n;
                }

                let fill = self.rx.slots();
                if fill > self.max_fill {
                    report.dropped = Self::skip(&mut self.rx, fill - self.target);
                }

                // PI loop: too full → consume faster → lower out/in ratio.
                let err_s = (self.fill_avg - self.target as f64) / self.in_rate;
                *integral = (*integral + err_s * dt).clamp(-0.05, 0.05);
                *rel = (1.0 - KP * err_s - KI * *integral).clamp(1.0 - MAX_REL, 1.0 + MAX_REL);
                let _ = rs.set_resample_ratio_relative(*rel, true);
            }
        }

        let fill = self.rx.slots() as f64;
        let a = (-dt / 0.05).exp();
        self.fill_avg = fill + (self.fill_avg - fill) * a;

        if report.underrun {
            self.underruns += 1;
            self.primed = false;
        }
        self.dropped += report.dropped as u64;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtrb::RingBuffer;

    /// Deterministic two-clock simulation: the producer writes `p_block`
    /// frames at `p_rate`, the consumer reads `c_block` frames at `c_rate`.
    fn simulate(
        p_rate: f64,
        c_rate: f64,
        p_block: usize,
        c_block: usize,
        seconds: f64,
        mut reader: RingReader,
        mut tx: rtrb::Producer<f32>,
    ) -> (RingReader, Vec<f32>, u64) {
        let mut t_p = 0.0;
        let mut t_c = 0.0;
        let mut phase = 0.0f64;
        let mut out_all = Vec::new();
        let mut buf = vec![0.0; c_block];
        let mut underruns_after_start = 0;
        while t_p < seconds {
            if t_p <= t_c {
                for _ in 0..p_block {
                    phase += 440.0 / p_rate;
                    let _ = tx.push((2.0 * std::f64::consts::PI * phase).sin() as f32 * 0.5);
                }
                t_p += p_block as f64 / p_rate;
            } else {
                let r = reader.read(&mut buf);
                if r.underrun && t_c > 0.5 {
                    underruns_after_start += 1;
                }
                out_all.extend_from_slice(&buf);
                t_c += c_block as f64 / c_rate;
            }
        }
        (reader, out_all, underruns_after_start)
    }

    #[test]
    fn fr04_resampler_tracks_clock_drift_without_underruns() {
        // Cable clock 300 ppm fast relative to the engine clock.
        let (tx, rx) = RingBuffer::new(16_384);
        let reader = RingReader::resampled(rx, 48_000, 48_000, 960).unwrap();
        let (reader, _, underruns) =
            simulate(48_000.0, 48_000.0 * 1.0003, 144, 480, 120.0, reader, tx);
        assert_eq!(underruns, 0);
        assert_eq!(reader.dropped(), 0);
        assert!(
            (reader.fill() - 960.0).abs() < 300.0,
            "fill {}",
            reader.fill()
        );
        // A fast consumer drains the ring, so more output per input: ratio > 1.
        assert!(reader.relative_ratio() > 1.0, "{}", reader.relative_ratio());
    }

    #[test]
    fn fr04_resampler_converts_rate() {
        let (tx, rx) = RingBuffer::new(16_384);
        let reader = RingReader::resampled(rx, 44_100, 48_000, 882).unwrap();
        let (_, out, underruns) = simulate(44_100.0, 48_000.0, 441, 480, 5.0, reader, tx);
        assert_eq!(underruns, 0);
        // 440 Hz at 44.1k must still be 440 Hz at 48k: count zero crossings.
        let seg = &out[48_000..48_000 * 4];
        let crossings = seg.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let hz = crossings as f64 / 3.0;
        assert!((hz - 440.0).abs() < 2.0, "{hz}");
    }

    #[test]
    fn direct_mode_holds_fill_and_bounds_latency() {
        let (tx, rx) = RingBuffer::new(8_192);
        let reader = RingReader::direct(rx, 48_000, 144);
        let (reader, _, underruns) = simulate(48_000.0, 48_000.0, 144, 144, 10.0, reader, tx);
        assert_eq!(underruns, 0);
        assert!(reader.fill() <= 3.0 * 144.0);
    }

    #[test]
    fn direct_mode_drops_excess_when_producer_runs_fast() {
        let (tx, rx) = RingBuffer::new(8_192);
        let reader = RingReader::direct(rx, 48_000, 144);
        let (reader, _, _) = simulate(48_000.0 * 1.01, 48_000.0, 144, 144, 10.0, reader, tx);
        assert!(reader.dropped() > 0);
        assert!(reader.fill() <= 3.0 * 144.0 + 144.0);
    }

    #[test]
    fn outputs_silence_until_primed() {
        let (mut tx, rx) = RingBuffer::new(1024);
        let mut r = RingReader::direct(rx, 48_000, 256);
        for _ in 0..100 {
            tx.push(1.0).unwrap();
        }
        let mut out = [9.0f32; 64];
        r.read(&mut out);
        assert!(out.iter().all(|&v| v == 0.0));
        for _ in 0..200 {
            tx.push(1.0).unwrap();
        }
        r.read(&mut out);
        assert!(out.iter().all(|&v| v == 1.0));
    }
}
