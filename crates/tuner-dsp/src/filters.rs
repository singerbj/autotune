//! Conditioning filters, parameter smoothing and the output limiter.

use core::f32::consts::PI;

/// One-pole DC blocker.
#[derive(Clone, Debug)]
pub(crate) struct DcBlocker {
    r: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            r: (1.0 - 2.0 * PI * 10.0 / sample_rate).clamp(0.9, 0.9999),
            x1: 0.0,
            y1: 0.0,
        }
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    pub(crate) fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// RBJ biquad, used as the 70 Hz high-pass.
#[derive(Clone, Debug)]
pub(crate) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub(crate) fn highpass(sample_rate: f32, freq: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Transposed direct form II.
    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub(crate) fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Coefficient for a one-pole smoother with time constant `tau_s`.
#[inline]
pub(crate) fn one_pole_coeff(tau_s: f32, sample_rate: f32) -> f32 {
    if tau_s <= 0.0 {
        0.0
    } else {
        (-1.0 / (tau_s * sample_rate)).exp()
    }
}

/// One-pole parameter smoother (FR-11: ≥10 ms smoothing).
#[derive(Clone, Debug)]
pub(crate) struct Smoother {
    value: f32,
    target: f32,
    coeff: f32,
}

impl Smoother {
    pub(crate) fn new(initial: f32, tau_s: f32, sample_rate: f32) -> Self {
        Self {
            value: initial,
            target: initial,
            coeff: one_pole_coeff(tau_s, sample_rate),
        }
    }

    #[inline]
    pub(crate) fn set_target(&mut self, t: f32) {
        self.target = t;
    }

    #[inline]
    pub(crate) fn next(&mut self) -> f32 {
        self.value = self.target + (self.value - self.target) * self.coeff;
        if (self.value - self.target).abs() < 1e-6 {
            self.value = self.target;
        }
        self.value
    }

    pub(crate) fn snap(&mut self, v: f32) {
        self.value = v;
        self.target = v;
    }
}

/// Linear ramp, used for the bypass crossfade (FR-10: ≥5 ms).
#[derive(Clone, Debug)]
pub(crate) struct Ramp {
    value: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    pub(crate) fn new(initial: f32, duration_s: f32, sample_rate: f32) -> Self {
        Self {
            value: initial,
            target: initial,
            step: 1.0 / (duration_s * sample_rate).max(1.0),
        }
    }

    #[inline]
    pub(crate) fn set_target(&mut self, t: f32) {
        self.target = t;
    }

    #[inline]
    pub(crate) fn next(&mut self) -> f32 {
        if self.value < self.target {
            self.value = (self.value + self.step).min(self.target);
        } else if self.value > self.target {
            self.value = (self.value - self.step).max(self.target);
        }
        self.value
    }

    #[inline]
    pub(crate) fn value(&self) -> f32 {
        self.value
    }
}

/// Noise gate with hysteresis and hold (FR-09).
#[derive(Clone, Debug)]
pub(crate) struct NoiseGate {
    env: f32,
    attack: f32,
    release: f32,
    open: bool,
    hold_samples: u32,
    hold_left: u32,
    gain: Smoother,
    gain_open_coeff: f32,
    gain_close_coeff: f32,
    open_lin: f32,
    close_lin: f32,
}

/// Gate closes this many dB below the open threshold.
const GATE_HYSTERESIS_DB: f32 = 6.0;

impl NoiseGate {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let mut g = Self {
            env: 0.0,
            attack: one_pole_coeff(0.001, sample_rate),
            release: one_pole_coeff(0.050, sample_rate),
            open: false,
            hold_samples: (0.050 * sample_rate) as u32,
            hold_left: 0,
            gain: Smoother::new(0.0, 0.002, sample_rate),
            gain_open_coeff: one_pole_coeff(0.002, sample_rate),
            gain_close_coeff: one_pole_coeff(0.030, sample_rate),
            open_lin: 0.0,
            close_lin: 0.0,
        };
        g.set_threshold_db(-60.0);
        g
    }

    pub(crate) fn set_threshold_db(&mut self, db: f32) {
        if db <= -99.9 {
            self.open_lin = 0.0;
            self.close_lin = 0.0;
        } else {
            self.open_lin = db_to_lin(db);
            self.close_lin = db_to_lin(db - GATE_HYSTERESIS_DB);
        }
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let a = x.abs();
        let c = if a > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = a + (self.env - a) * c;
        if self.open_lin == 0.0 {
            self.open = true;
        } else if self.env >= self.open_lin {
            self.open = true;
            self.hold_left = self.hold_samples;
        } else if self.env < self.close_lin {
            if self.hold_left > 0 {
                self.hold_left -= 1;
            } else {
                self.open = false;
            }
        }
        self.gain.coeff = if self.open {
            self.gain_open_coeff
        } else {
            self.gain_close_coeff
        };
        self.gain.set_target(if self.open { 1.0 } else { 0.0 });
        x * self.gain.next()
    }

    #[inline]
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn reset(&mut self) {
        self.env = 0.0;
        self.open = false;
        self.hold_left = 0;
        self.gain.snap(0.0);
    }
}

#[inline]
pub(crate) fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[inline]
pub(crate) fn lin_to_db(lin: f32) -> f32 {
    if lin <= 1e-10 {
        -200.0
    } else {
        20.0 * lin.log10()
    }
}

/// Stateless soft limiter: linear below 0.8, smoothly saturating to ±1.
#[inline]
pub(crate) fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.8;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        (KNEE + (1.0 - KNEE) * over.tanh()).copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_blocker_removes_offset() {
        let mut dc = DcBlocker::new(48_000.0);
        let mut y = 0.0;
        for _ in 0..48_000 {
            y = dc.process(0.5);
        }
        assert!(y.abs() < 1e-3, "residual {y}");
    }

    #[test]
    fn highpass_attenuates_20hz_passes_440hz() {
        let sr = 48_000.0;
        let rms_after = |f: f32| {
            let mut hp = Biquad::highpass(sr, 70.0, core::f32::consts::FRAC_1_SQRT_2);
            let mut acc = 0.0;
            let n = 48_000;
            for i in 0..n {
                let y = hp.process((2.0 * PI * f * i as f32 / sr).sin());
                if i > n / 2 {
                    acc += y * y;
                }
            }
            (acc / (n / 2) as f32).sqrt() * core::f32::consts::SQRT_2
        };
        assert!(rms_after(20.0) < 0.12);
        assert!(rms_after(440.0) > 0.97);
    }

    #[test]
    fn fr09_gate_closes_on_quiet_input_and_opens_on_loud() {
        let mut g = NoiseGate::new(48_000.0);
        g.set_threshold_db(-40.0);
        for _ in 0..48_000 {
            g.process(0.001);
        }
        assert!(!g.is_open());
        let mut last = 0.0;
        for _ in 0..4_800 {
            last = g.process(0.5);
        }
        assert!(g.is_open());
        assert!((last - 0.5).abs() < 1e-3);
    }

    #[test]
    fn fr09_gate_disabled_at_minus_100() {
        let mut g = NoiseGate::new(48_000.0);
        g.set_threshold_db(-100.0);
        g.process(0.0);
        assert!(g.is_open());
    }

    #[test]
    fn fr11_smoother_takes_at_least_10ms() {
        let sr = 48_000.0;
        let mut s = Smoother::new(0.0, 0.010, sr);
        s.set_target(1.0);
        // After 1 ms we must still be far from the target.
        let mut v = 0.0;
        for _ in 0..48 {
            v = s.next();
        }
        assert!(v < 0.2, "{v}");
    }

    #[test]
    fn fr10_ramp_is_linear_and_bounded() {
        let mut r = Ramp::new(0.0, 0.010, 48_000.0);
        r.set_target(1.0);
        for _ in 0..239 {
            r.next();
        }
        assert!((r.value() - 0.498).abs() < 0.01);
        for _ in 0..1000 {
            r.next();
        }
        assert_eq!(r.value(), 1.0);
    }

    #[test]
    fn limiter_is_bounded_and_transparent_below_knee() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.5), -0.5);
        for x in [1.0f32, 2.0, 10.0, 1e6] {
            assert!(soft_limit(x) <= 1.0);
            assert!(soft_limit(-x) >= -1.0);
        }
    }
}
