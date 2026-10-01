//! Building blocks shared by the time-based effects: a fractional delay
//! line, a Schroeder allpass on top of it, and a quadrature oscillator.

/// Ring-buffer delay line. All memory is allocated in [`new`](Self::new).
#[derive(Debug)]
pub(crate) struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl DelayLine {
    /// A line that can delay by up to `max_delay` samples.
    pub(crate) fn new(max_delay: usize) -> Self {
        let cap = (max_delay + 4).next_power_of_two();
        Self {
            buf: vec![0.0; cap],
            mask: cap - 1,
            pos: 0,
        }
    }

    /// Longest usable delay in samples.
    pub(crate) fn max_delay(&self) -> f32 {
        (self.mask - 2) as f32
    }

    /// The input `d` pushes ago (`d ≥ 1`, fractional: linear interpolation).
    /// Call before [`push`](Self::push).
    #[inline]
    pub(crate) fn tap_frac(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, self.max_delay());
        let i = d as usize;
        let frac = d - i as f32;
        let a = self.buf[self.pos.wrapping_sub(i) & self.mask];
        let b = self.buf[self.pos.wrapping_sub(i + 1) & self.mask];
        a + (b - a) * frac
    }

    #[inline]
    pub(crate) fn push(&mut self, x: f32) {
        self.buf[self.pos & self.mask] = x;
        self.pos = self.pos.wrapping_add(1);
    }

    pub(crate) fn reset(&mut self) {
        self.buf.fill(0.0);
    }
}

/// Schroeder allpass `(z^-D − g) / (1 − g·z^-D)` with a fractional delay.
#[inline]
pub(crate) fn allpass(line: &mut DelayLine, x: f32, delay: f32, g: f32) -> f32 {
    let delayed = line.tap_frac(delay);
    let v = x + g * delayed;
    line.push(v);
    delayed - g * v
}

/// Sine/cosine oscillator by complex rotation (no trig per sample).
#[derive(Clone, Debug)]
pub(crate) struct QuadOsc {
    s: f32,
    c: f32,
    sin_d: f32,
    cos_d: f32,
    count: u32,
}

impl QuadOsc {
    pub(crate) fn new(hz: f32, sample_rate: f32, phase: f32) -> Self {
        let w = 2.0 * core::f32::consts::PI * hz / sample_rate;
        let p = 2.0 * core::f32::consts::PI * phase;
        Self {
            s: p.sin(),
            c: p.cos(),
            sin_d: w.sin(),
            cos_d: w.cos(),
            count: 0,
        }
    }

    /// Advance one sample; returns `(sin, cos)`.
    #[inline]
    pub(crate) fn next(&mut self) -> (f32, f32) {
        let s = self.s * self.cos_d + self.c * self.sin_d;
        let c = self.c * self.cos_d - self.s * self.sin_d;
        self.s = s;
        self.c = c;
        self.count += 1;
        if self.count >= 1024 {
            // Keep the amplitude at 1 despite rounding.
            self.count = 0;
            let n = (s * s + c * c).sqrt().recip();
            self.s *= n;
            self.c *= n;
        }
        (self.s, self.c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_line_taps_past_samples() {
        let mut d = DelayLine::new(16);
        for i in 0..10 {
            d.push(i as f32);
        }
        assert_eq!(d.tap_frac(1.0), 9.0);
        assert_eq!(d.tap_frac(4.0), 6.0);
        assert!((d.tap_frac(2.5) - 7.5).abs() < 1e-6);
    }

    #[test]
    fn allpass_keeps_energy() {
        let mut line = DelayLine::new(64);
        let mut e_out = 0.0;
        for i in 0..4_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let y = allpass(&mut line, x, 37.0, 0.6);
            e_out += y * y;
        }
        assert!((e_out - 1.0).abs() < 1e-3, "{e_out}");
    }

    #[test]
    fn oscillator_stays_on_the_unit_circle() {
        let mut o = QuadOsc::new(1.3, 48_000.0, 0.25);
        let mut last = (0.0, 0.0);
        for _ in 0..480_000 {
            last = o.next();
        }
        let r = (last.0 * last.0 + last.1 * last.1).sqrt();
        assert!((r - 1.0).abs() < 1e-3, "{r}");
    }
}
