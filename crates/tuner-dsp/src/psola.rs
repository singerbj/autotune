//! Real-time TD-PSOLA pitch shifter (pipeline stage 6).
//!
//! Timeline: input sample `t` and output sample `o` share one clock; output
//! `o` is emitted once input up to `o + L` has arrived, where `L` is one
//! period of the lowest tracked pitch. Pitch marks are placed on the
//! conditioned input at detected-period spacing (snapped to local peaks).
//! Synthesis marks advance by `period / ratio`; each takes the nearest
//! complete analysis grain (two periods, Hann-windowed) and overlap-adds it.
//! The accumulated window sum is divided out so amplitude stays flat for any
//! ratio. All rings are sized at construction; nothing here allocates.

const HANN_LEN: usize = 1024;
const MAX_MARKS: usize = 64;

#[derive(Clone, Copy, Debug, Default)]
struct Mark {
    pos: i64,
    period: f32,
}

#[derive(Debug)]
pub(crate) struct Psola {
    mask: usize,
    raw: Vec<f32>,
    cond: Vec<f32>,
    acc: Vec<f32>,
    wsum: Vec<f32>,
    hann: Vec<f32>,
    write_pos: i64,
    read_pos: i64,
    latency: i64,
    marks: [Mark; MAX_MARKS],
    mark_head: usize,
    mark_count: usize,
    last_mark: i64,
    unvoiced_period: f32,
    synth_pos: f64,
    sample_rate: f32,
}

impl Psola {
    /// `max_period`: period of the lowest pitch any preset can track, in samples.
    pub(crate) fn new(sample_rate: f32, max_period: usize, latency: usize) -> Self {
        let cap = (8 * max_period + 1024).next_power_of_two();
        let hann = (0..=HANN_LEN)
            .map(|k| 0.5 - 0.5 * (2.0 * core::f32::consts::PI * k as f32 / HANN_LEN as f32).cos())
            .collect();
        let mut p = Self {
            mask: cap - 1,
            raw: vec![0.0; cap],
            cond: vec![0.0; cap],
            acc: vec![0.0; cap],
            wsum: vec![0.0; cap],
            hann,
            write_pos: 0,
            read_pos: 0,
            latency: 0,
            marks: [Mark::default(); MAX_MARKS],
            mark_head: 0,
            mark_count: 0,
            last_mark: 0,
            unvoiced_period: 0.0,
            synth_pos: 0.0,
            sample_rate,
        };
        p.set_latency(latency);
        p
    }

    pub(crate) fn latency(&self) -> usize {
        self.latency as usize
    }

    /// Change the algorithmic latency. Clears synthesis state (the caller
    /// fades the output around this).
    pub(crate) fn set_latency(&mut self, latency: usize) {
        self.latency = latency.max(8) as i64;
        self.unvoiced_period = (self.sample_rate * 0.005).min(self.latency as f32).max(4.0);
        self.read_pos = self.write_pos - self.latency;
        self.synth_pos = self.write_pos as f64;
        self.acc.fill(0.0);
        self.wsum.fill(0.0);
        self.mark_count = 0;
        self.last_mark = self.write_pos;
    }

    pub(crate) fn reset(&mut self) {
        self.raw.fill(0.0);
        self.cond.fill(0.0);
        self.write_pos = 0;
        self.set_latency(self.latency as usize);
    }

    #[inline]
    fn idx(&self, t: i64) -> usize {
        (t as usize) & self.mask
    }

    #[inline]
    pub(crate) fn push(&mut self, raw: f32, cond: f32) {
        let i = self.idx(self.write_pos);
        self.raw[i] = raw;
        self.cond[i] = cond;
        self.write_pos += 1;
    }

    /// The most recent `n` conditioned samples as (up to) two slices.
    pub(crate) fn recent(&self, n: usize) -> (&[f32], &[f32]) {
        let cap = self.mask + 1;
        let n = n.min(cap);
        let start = self.idx(self.write_pos - n as i64);
        if start + n <= cap {
            (&self.cond[start..start + n], &[])
        } else {
            (&self.cond[start..], &self.cond[..n - (cap - start)])
        }
    }

    /// Extend analysis marks up to the newest input. `period` is the detected
    /// period in samples when voiced.
    pub(crate) fn update_marks(&mut self, period: Option<f32>) {
        let t = self.write_pos;
        let (p, voiced) = match period {
            Some(p) => (p.clamp(4.0, self.latency as f32), true),
            None => (self.unvoiced_period, false),
        };
        let step = p.round().max(1.0) as i64;
        if self.last_mark < t - 4 * self.latency {
            self.last_mark = t - 2 * step;
        }
        loop {
            let cand = self.last_mark + step;
            let pos = if voiced {
                let r = (p * 0.25) as i64;
                if cand + r >= t {
                    break;
                }
                let mut best = cand;
                let mut best_v = f32::NEG_INFINITY;
                for i in cand - r..=cand + r {
                    let v = self.cond[self.idx(i)];
                    if v > best_v {
                        best_v = v;
                        best = i;
                    }
                }
                best
            } else {
                if cand >= t {
                    break;
                }
                cand
            };
            self.marks[self.mark_head] = Mark { pos, period: p };
            self.mark_head = (self.mark_head + 1) % MAX_MARKS;
            self.mark_count = (self.mark_count + 1).min(MAX_MARKS);
            self.last_mark = pos;
        }
    }

    /// Place synthesis grains up to the newest input with pitch `ratio`.
    pub(crate) fn synthesize(&mut self, ratio: f32) {
        let t = self.write_pos;
        let limit = t as f64;
        let ratio = ratio.clamp(0.25, 4.0);
        while self.synth_pos < limit {
            let ts = self.synth_pos.round() as i64;
            let mut chosen: Option<Mark> = None;
            let mut best_d = i64::MAX;
            for k in 0..self.mark_count {
                let m = self.marks[(self.mark_head + MAX_MARKS - 1 - k) % MAX_MARKS];
                if m.pos + m.period.ceil() as i64 > t {
                    continue; // grain not fully captured yet
                }
                let d = (m.pos - ts).abs();
                if d < best_d {
                    best_d = d;
                    chosen = Some(m);
                } else if m.pos < ts {
                    break; // marks are time-ordered; only getting further
                }
            }
            let step = match chosen {
                Some(m) if best_d <= 3 * self.latency => {
                    self.overlap_add(ts, m);
                    m.period / ratio
                }
                _ => self.unvoiced_period,
            };
            self.synth_pos += f64::from(step.max(2.0));
        }
    }

    fn overlap_add(&mut self, ts: i64, m: Mark) {
        let half = m.period.round().max(2.0) as i64;
        let inv_len = HANN_LEN as f32 / (2 * half) as f32;
        for i in -half..half {
            let x = (i + half) as f32 * inv_len;
            let k = x as usize;
            let frac = x - k as f32;
            let w = self.hann[k] + (self.hann[(k + 1).min(HANN_LEN)] - self.hann[k]) * frac;
            let src = self.cond[self.idx(m.pos + i)];
            let dst = self.idx(ts + i);
            self.acc[dst] += w * src;
            self.wsum[dst] += w;
        }
    }

    /// Emit the next output sample: `(wet, dry_conditioned, dry_raw)`, all
    /// delayed by the algorithmic latency.
    #[inline]
    pub(crate) fn pop(&mut self) -> (f32, f32, f32) {
        let o = self.read_pos;
        self.read_pos += 1;
        let i = self.idx(o);
        let a = self.acc[i];
        let w = self.wsum[i];
        self.acc[i] = 0.0;
        self.wsum[i] = 0.0;
        if o < 0 {
            return (0.0, 0.0, 0.0);
        }
        let wet = a / w.max(0.1);
        (wet, self.cond[i], self.raw[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scale::hz_to_midi, signals, PitchDetector};

    fn run(x: &[f32], sr: f32, period: Option<f32>, ratio: f32) -> Vec<f32> {
        let mut p = Psola::new(sr, 686, 480);
        let mut out = Vec::with_capacity(x.len());
        for chunk in x.chunks(32) {
            for &s in chunk {
                p.push(s, s);
            }
            p.update_marks(period);
            p.synthesize(ratio);
            for _ in chunk {
                out.push(p.pop().0);
            }
        }
        out
    }

    #[test]
    fn ratio_one_reconstructs_delayed_input() {
        let sr = 48_000.0;
        let f = 200.0;
        let x = signals::harmonic(f, sr, 24_000, 0.5, 8);
        let y = run(&x, sr, Some(sr / f), 1.0);
        // Compare steady state against input delayed by the latency, allowing
        // a small alignment offset (grains are taken from the nearest mark).
        let mut best = f32::MAX;
        for lag in 200..760 {
            let err: f32 = (8_000..20_000)
                .map(|i| (y[i] - x[i - lag]).powi(2))
                .sum::<f32>()
                / 12_000.0;
            best = best.min(err);
        }
        assert!(best < 1e-3, "mse {best}");
    }

    #[test]
    fn shifts_pitch_by_ratio() {
        let sr = 48_000.0;
        let f = 220.0;
        let x = signals::harmonic(f, sr, 48_000, 0.5, 8);
        let mut det = PitchDetector::new(sr, 1372);
        for semis in [-5.0f32, -1.0, 0.5, 3.0, 7.0] {
            let ratio = (semis / 12.0).exp2();
            let y = run(&x, sr, Some(sr / f), ratio);
            let e = det
                .detect(&y[30_000..31_372], &[], 70.0, 1000.0)
                .expect("pitch");
            let got = hz_to_midi(e.hz) - hz_to_midi(f);
            assert!((got - semis).abs() < 0.05, "want {semis} got {got}");
        }
    }

    #[test]
    fn impulse_passes_at_latency_when_unvoiced() {
        let sr = 48_000.0;
        let mut p = Psola::new(sr, 686, 480);
        let mut dry = Vec::new();
        for i in 0..2_000 {
            let s = if i == 100 { 1.0 } else { 0.0 };
            p.push(s, s);
            p.update_marks(None);
            p.synthesize(1.0);
            dry.push(p.pop().2);
        }
        let peak = dry
            .iter()
            .enumerate()
            .fold((0, 0.0f32), |b, (i, &v)| if v > b.1 { (i, v) } else { b });
        assert_eq!(peak.0, 100 + 480);
    }
}
