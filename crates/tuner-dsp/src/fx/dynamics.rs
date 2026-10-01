//! Insert effects: presence/air EQ and a feed-forward compressor.

use crate::filters::{db_to_lin, lin_to_db, one_pole_coeff, Biquad};

const PRESENCE_HZ: f32 = 3_500.0;
const PRESENCE_Q: f32 = 0.9;
const AIR_HZ: f32 = 10_000.0;

/// Presence peak + air shelf. A band at exactly 0 dB is skipped, so the
/// default is bit-transparent.
#[derive(Debug)]
pub(crate) struct Eq {
    sr: f32,
    presence: Biquad,
    air: Biquad,
    presence_on: bool,
    air_on: bool,
}

impl Eq {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sr: sample_rate,
            presence: Biquad::peaking(sample_rate, PRESENCE_HZ, PRESENCE_Q, 0.0),
            air: Biquad::high_shelf(sample_rate, Self::air_hz(sample_rate), 0.0),
            presence_on: false,
            air_on: false,
        }
    }

    /// Keep the shelf below Nyquist at low sample rates.
    fn air_hz(sample_rate: f32) -> f32 {
        AIR_HZ.min(0.4 * sample_rate)
    }

    pub(crate) fn set(&mut self, presence_db: f32, air_db: f32) {
        let presence_hz = PRESENCE_HZ.min(0.4 * self.sr);
        self.presence.set_coeffs(&Biquad::peaking(
            self.sr,
            presence_hz,
            PRESENCE_Q,
            presence_db,
        ));
        self.air
            .set_coeffs(&Biquad::high_shelf(self.sr, Self::air_hz(self.sr), air_db));
        self.presence_on = presence_db != 0.0;
        self.air_on = air_db != 0.0;
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let mut y = x;
        if self.presence_on {
            y = self.presence.process(y);
        }
        if self.air_on {
            y = self.air.process(y);
        }
        y
    }

    pub(crate) fn reset(&mut self) {
        self.presence.reset();
        self.air.reset();
    }
}

/// Fast peak compressor (3 ms attack, 80 ms release) with automatic makeup
/// gain of half the gain reduction at full scale. Off at a 0 dBFS threshold.
#[derive(Debug)]
pub(crate) struct Compressor {
    env: f32,
    attack: f32,
    release: f32,
    threshold_db: f32,
    slope: f32,
    makeup: f32,
    active: bool,
}

impl Compressor {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            env: 0.0,
            attack: one_pole_coeff(0.003, sample_rate),
            release: one_pole_coeff(0.080, sample_rate),
            threshold_db: 0.0,
            slope: 0.0,
            makeup: 1.0,
            active: false,
        }
    }

    pub(crate) fn set(&mut self, threshold_db: f32, ratio: f32) {
        self.threshold_db = threshold_db;
        self.slope = 1.0 - 1.0 / ratio.max(1.0);
        self.active = threshold_db < -0.05 && self.slope > 1e-3;
        self.makeup = db_to_lin((-threshold_db * self.slope * 0.5).min(12.0));
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        if !self.active {
            return x;
        }
        let a = x.abs();
        let c = if a > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = a + (self.env - a) * c;
        let over = lin_to_db(self.env) - self.threshold_db;
        let gain = if over > 0.0 {
            db_to_lin(-over * self.slope)
        } else {
            1.0
        };
        x * gain * self.makeup
    }

    pub(crate) fn reset(&mut self) {
        self.env = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steady_peak(c: &mut Compressor, amp: f32) -> f32 {
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let y =
                c.process(amp * (2.0 * core::f32::consts::PI * 200.0 * i as f32 / 48_000.0).sin());
            if i > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn fr25_compressor_reduces_dynamic_range() {
        let mut c = Compressor::new(48_000.0);
        c.set(-20.0, 4.0);
        let loud = steady_peak(&mut c, 0.5);
        c.reset();
        let quiet = steady_peak(&mut c, 0.05);
        // 20 dB in → much less out.
        let out_range = 20.0 * (loud / quiet).log10();
        assert!(out_range < 12.0, "{out_range} dB");
        assert!(out_range > 3.0, "{out_range} dB");
    }

    #[test]
    fn fr25_compressor_off_at_zero_threshold() {
        let mut c = Compressor::new(48_000.0);
        c.set(0.0, 8.0);
        assert_eq!(c.process(0.9), 0.9);
    }

    #[test]
    fn fr25_eq_boosts_presence() {
        let sr = 48_000.0;
        let mut eq = Eq::new(sr);
        eq.set(6.0, 0.0);
        let mut acc = 0.0;
        for i in 0..24_000 {
            let y = eq.process((2.0 * core::f32::consts::PI * 3_500.0 * i as f32 / sr).sin());
            if i > 12_000 {
                acc += y * y;
            }
        }
        let gain_db = 10.0 * (acc / 12_000.0 * 2.0).log10();
        assert!((gain_db - 6.0).abs() < 0.3, "{gain_db}");
    }
}
