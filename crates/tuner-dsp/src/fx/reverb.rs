//! Plate reverb after Dattorro, "Effect Design Part 1" (J. AES 1997): a
//! pre-delay, four input diffusers and a two-branch figure-eight tank with
//! modulated allpasses. Mono in, mono out (the two output tap sets summed).

use super::lines::{allpass, DelayLine, QuadOsc};
use crate::filters::Biquad;

/// Sample rate the published delay lengths are given at.
const REF_SR: f32 = 29_761.0;
/// Size 0 … 1 scales the tank by this range.
const SCALE_MIN: f32 = 0.5;
const SCALE_MAX: f32 = 1.5;
const MAX_PREDELAY_S: f32 = 0.1;
/// Constant added inside the tank so decaying tails never reach denormals.
const DENORMAL_GUARD: f32 = 1e-18;

const IN_DIFFUSERS: [(f32, f32); 4] =
    [(142.0, 0.75), (107.0, 0.75), (379.0, 0.625), (277.0, 0.625)];
const EXCURSION: f32 = 16.0;

/// One half of the tank: modulated allpass → delay → damping → allpass → delay.
#[derive(Debug)]
struct Branch {
    ap1: DelayLine,
    d1: DelayLine,
    ap2: DelayLine,
    d2: DelayLine,
    damp_state: f32,
    lens: [f32; 4],
}

impl Branch {
    fn new(lens: [f32; 4], max_scale: f32) -> Self {
        let line = |l: f32| DelayLine::new((l * max_scale).ceil() as usize + 64);
        Self {
            ap1: line(lens[0]),
            d1: line(lens[1]),
            ap2: line(lens[2]),
            d2: line(lens[3]),
            damp_state: 0.0,
            lens,
        }
    }

    #[inline]
    fn process(&mut self, x: f32, s: &Shape, modulation: f32) -> f32 {
        let a = allpass(&mut self.ap1, x, self.lens[0] * s.scale + modulation, -0.7);
        let d = self.d1.tap_frac(self.lens[1] * s.scale);
        self.d1.push(a);
        self.damp_state = d + (self.damp_state - d) * s.damping;
        let b = allpass(
            &mut self.ap2,
            self.damp_state * s.decay,
            self.lens[2] * s.scale,
            s.diffusion2,
        );
        let e = self.d2.tap_frac(self.lens[3] * s.scale);
        self.d2.push(b);
        e * s.decay
    }

    fn reset(&mut self) {
        self.ap1.reset();
        self.d1.reset();
        self.ap2.reset();
        self.d2.reset();
        self.damp_state = 0.0;
    }
}

/// Derived coefficients for the current size and decay.
#[derive(Clone, Copy, Debug)]
struct Shape {
    /// Delay length multiplier (sample rate × size).
    scale: f32,
    decay: f32,
    diffusion2: f32,
    damping: f32,
}

#[derive(Debug)]
pub(crate) struct Reverb {
    sr: f32,
    predelay: DelayLine,
    predelay_samples: f32,
    hp: Biquad,
    bw_coeff: f32,
    bw_state: f32,
    diffusers: [DelayLine; 4],
    left: Branch,
    right: Branch,
    left_out: f32,
    right_out: f32,
    lfo: QuadOsc,
    shape: Shape,
    excursion: f32,
}

impl Reverb {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let sr_scale = sample_rate / REF_SR;
        let max_scale = sr_scale * SCALE_MAX;
        let mut r = Self {
            sr: sample_rate,
            predelay: DelayLine::new((MAX_PREDELAY_S * sample_rate).ceil() as usize + 2),
            predelay_samples: 1.0,
            hp: Biquad::highpass(sample_rate, 180.0, core::f32::consts::FRAC_1_SQRT_2),
            bw_coeff: (-2.0 * core::f32::consts::PI * 8_000.0 / sample_rate).exp(),
            bw_state: 0.0,
            diffusers: IN_DIFFUSERS
                .map(|(l, _)| DelayLine::new((l * max_scale).ceil() as usize + 4)),
            left: Branch::new([672.0, 4453.0, 1800.0, 3720.0], max_scale),
            right: Branch::new([908.0, 4217.0, 2656.0, 3163.0], max_scale),
            left_out: 0.0,
            right_out: 0.0,
            lfo: QuadOsc::new(0.8, sample_rate, 0.0),
            shape: Shape {
                scale: sr_scale,
                decay: 0.5,
                diffusion2: 0.5,
                damping: (-2.0 * core::f32::consts::PI * 6_000.0 / sample_rate).exp(),
            },
            excursion: EXCURSION * sr_scale,
        };
        r.set(0.6, 0.5, 20.0);
        r
    }

    /// `size` and `decay` in 0 … 1, pre-delay in ms.
    pub(crate) fn set(&mut self, size: f32, decay: f32, predelay_ms: f32) {
        let sr_scale = self.sr / REF_SR;
        self.shape.scale = sr_scale * (SCALE_MIN + (SCALE_MAX - SCALE_MIN) * size.clamp(0.0, 1.0));
        self.shape.decay = 0.25 + 0.7 * decay.clamp(0.0, 1.0);
        self.shape.diffusion2 = (self.shape.decay + 0.15).clamp(0.25, 0.5);
        self.predelay_samples =
            (predelay_ms * 0.001 * self.sr).clamp(1.0, self.predelay.max_delay());
    }

    /// One sample in, one wet sample out.
    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let pre = self.predelay.tap_frac(self.predelay_samples);
        self.predelay.push(x);
        let h = self.hp.process(pre);
        self.bw_state = h + (self.bw_state - h) * self.bw_coeff;
        let s = self.shape;
        let mut d = self.bw_state;
        for (line, &(len, g)) in self.diffusers.iter_mut().zip(IN_DIFFUSERS.iter()) {
            d = allpass(line, d, len * s.scale, g);
        }
        d += DENORMAL_GUARD;
        let (sin, cos) = self.lfo.next();
        let l = self
            .left
            .process(d + self.right_out, &s, self.excursion * sin);
        let r = self
            .right
            .process(d + self.left_out, &s, self.excursion * cos);
        self.left_out = l;
        self.right_out = r;
        self.output_taps(s.scale)
    }

    fn output_taps(&self, k: f32) -> f32 {
        let t = |line: &DelayLine, n: f32| line.tap_frac(n * k);
        let (l, r) = (&self.left, &self.right);
        let out_l = t(&r.d1, 266.0) + t(&r.d1, 2974.0) - t(&r.ap2, 1913.0) + t(&r.d2, 1996.0)
            - t(&l.d1, 1990.0)
            - t(&l.ap2, 187.0)
            - t(&l.d2, 1066.0);
        let out_r = t(&l.d1, 353.0) + t(&l.d1, 3627.0) - t(&l.ap2, 1228.0) + t(&l.d2, 2673.0)
            - t(&r.d1, 2111.0)
            - t(&r.ap2, 335.0)
            - t(&r.d2, 121.0);
        0.3 * (out_l + out_r)
    }

    pub(crate) fn reset(&mut self) {
        self.predelay.reset();
        self.hp.reset();
        self.bw_state = 0.0;
        for d in &mut self.diffusers {
            d.reset();
        }
        self.left.reset();
        self.right.reset();
        self.left_out = 0.0;
        self.right_out = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(y: &[f32]) -> f32 {
        y.iter().map(|v| v * v).sum()
    }

    fn impulse_response(decay: f32, seconds: f32) -> Vec<f32> {
        let sr = 48_000.0;
        let mut r = Reverb::new(sr);
        r.set(0.6, decay, 0.0);
        (0..(seconds * sr) as usize)
            .map(|i| r.process(if i == 0 { 1.0 } else { 0.0 }))
            .collect()
    }

    #[test]
    fn fr25_reverb_tail_rings_then_decays() {
        let y = impulse_response(0.5, 6.0);
        assert!(y.iter().all(|v| v.is_finite()));
        let early = energy(&y[4_800..24_000]);
        let late = energy(&y[240_000..288_000]);
        assert!(early > 1e-3, "no tail: {early}");
        assert!(late < early * 1e-4, "not decaying: {early} → {late}");
    }

    #[test]
    fn fr25_longer_decay_rings_longer() {
        let short = impulse_response(0.1, 2.0);
        let long = impulse_response(0.9, 2.0);
        let tail = |y: &[f32]| energy(&y[48_000..96_000]);
        assert!(tail(&long) > 10.0 * tail(&short));
    }

    #[test]
    fn fr25_predelay_holds_back_the_tail() {
        let sr = 48_000.0;
        let mut r = Reverb::new(sr);
        r.set(0.6, 0.5, 50.0);
        let y: Vec<f32> = (0..9_600)
            .map(|i| r.process(if i == 0 { 1.0 } else { 0.0 }))
            .collect();
        assert!(energy(&y[..2_300]) < 1e-12);
        assert!(energy(&y[2_400..]) > 1e-4);
    }

    #[test]
    fn fr25_reverb_is_stable_under_loud_input() {
        let sr = 48_000.0;
        let mut r = Reverb::new(sr);
        r.set(1.0, 1.0, 0.0);
        let mut peak = 0.0f32;
        for i in 0..(10.0 * sr) as usize {
            let x = if (i / 4_000) % 2 == 0 { 1.0 } else { -1.0 };
            peak = peak.max(r.process(x).abs());
        }
        assert!(peak.is_finite() && peak < 20.0, "{peak}");
    }
}
