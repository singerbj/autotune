//! Vocal effects after the tuner (FR-25, FR-26): presence/air EQ and a
//! compressor in series ("inserts", heard everywhere), then doubler, echo and
//! plate reverb as parallel sends, each routed to the headphones, the
//! virtual mic, or both. None of them adds latency, and every one is exactly
//! transparent at its default so the tuner's output is unchanged until an
//! effect is turned up. A send at rest on 0 isn't processed at all.

mod dynamics;
mod lines;
mod reverb;
mod time;

use crate::filters::Smoother;
use crate::params::FxParams;

use dynamics::{Compressor, Eq};
use reverb::Reverb;
use time::{Doubler, Echo};

/// One processed sample for each destination.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FxOut {
    /// Every effect, ignoring routes (offline rendering).
    pub(crate) all: f32,
    pub(crate) monitor: f32,
    pub(crate) cable: f32,
}

#[derive(Debug)]
pub(crate) struct FxChain {
    params: FxParams,
    eq: Eq,
    comp: Compressor,
    doubler: Doubler,
    echo: Echo,
    reverb: Reverb,
    doubler_mix: Smoother,
    delay_mix: Smoother,
    reverb_mix: Smoother,
}

impl FxChain {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let params = FxParams::default();
        let mut fx = Self {
            params,
            eq: Eq::new(sample_rate),
            comp: Compressor::new(sample_rate),
            doubler: Doubler::new(sample_rate),
            echo: Echo::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            doubler_mix: Smoother::new(0.0, 0.020, sample_rate),
            delay_mix: Smoother::new(0.0, 0.020, sample_rate),
            reverb_mix: Smoother::new(0.0, 0.020, sample_rate),
        };
        fx.set_params(&params);
        fx
    }

    /// Real-time safe; levels are smoothed.
    pub(crate) fn set_params(&mut self, p: &FxParams) {
        let p = p.sanitized();
        self.eq.set(p.presence_db, p.air_db);
        self.comp.set(p.comp_threshold_db, p.comp_ratio);
        self.doubler.set(p.doubler_detune_cents, p.doubler_delay_ms);
        self.echo.set(
            60.0 / p.delay_bpm * p.delay_division.beats(),
            p.delay_feedback,
        );
        self.reverb
            .set(p.reverb_size, p.reverb_decay, p.reverb_predelay_ms);
        // An idle send isn't processed, so its lines hold stale audio:
        // clear them as it comes back.
        if self.doubler_mix.is_silent() && p.doubler_mix > 0.0 {
            self.doubler.reset();
        }
        if self.delay_mix.is_silent() && p.delay_mix > 0.0 {
            self.echo.reset();
        }
        if self.reverb_mix.is_silent() && p.reverb_mix > 0.0 {
            self.reverb.reset();
        }
        self.doubler_mix.set_target(p.doubler_mix);
        self.delay_mix.set_target(p.delay_mix);
        self.reverb_mix.set_target(p.reverb_mix);
        self.params = p;
    }

    pub(crate) fn reset(&mut self) {
        self.eq.reset();
        self.comp.reset();
        self.doubler.reset();
        self.echo.reset();
        self.reverb.reset();
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> FxOut {
        let v = self.comp.process(self.eq.process(x));
        // A send at rest on 0 is skipped (it adds exactly nothing).
        let d = if self.doubler_mix.is_silent() {
            0.0
        } else {
            self.doubler_mix.next() * self.doubler.process(v)
        };
        let e = if self.delay_mix.is_silent() {
            0.0
        } else {
            self.delay_mix.next() * self.echo.process(v)
        };
        let r = if self.reverb_mix.is_silent() {
            0.0
        } else {
            self.reverb_mix.next() * self.reverb.process(v)
        };
        let p = &self.params;
        let pick = |on: bool, s: f32| if on { s } else { 0.0 };
        FxOut {
            all: v + d + e + r,
            monitor: v
                + pick(p.doubler_route.to_monitor(), d)
                + pick(p.delay_route.to_monitor(), e)
                + pick(p.reverb_route.to_monitor(), r),
            cable: v
                + pick(p.doubler_route.to_cable(), d)
                + pick(p.delay_route.to_cable(), e)
                + pick(p.reverb_route.to_cable(), r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::FxRoute;

    fn run(fx: &mut FxChain, n: usize) -> Vec<FxOut> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let burst = if i < 4_800 { 1.0 } else { 0.0 };
                fx.process(burst * 0.5 * (2.0 * core::f32::consts::PI * 220.0 * t).sin())
            })
            .collect()
    }

    #[test]
    fn fr25_defaults_are_bit_transparent() {
        let mut fx = FxChain::new(48_000.0);
        for i in 0..48_000 {
            let x = ((i * 7919) % 1000) as f32 / 1000.0 - 0.5;
            let y = fx.process(x);
            assert_eq!(y.all.to_bits(), x.to_bits());
            assert_eq!(y.monitor.to_bits(), x.to_bits());
            assert_eq!(y.cable.to_bits(), x.to_bits());
        }
    }

    #[test]
    fn fr25_send_turned_back_on_starts_clean() {
        let mut fx = FxChain::new(48_000.0);
        let on = FxParams {
            delay_mix: 1.0,
            ..Default::default()
        };
        fx.set_params(&on);
        run(&mut fx, 6_000);
        fx.set_params(&FxParams::default());
        // Fade out and go idle with the old burst still in the line.
        for _ in 0..48_000 {
            fx.process(0.0);
        }
        fx.set_params(&on);
        let y: Vec<f32> = (0..24_000).map(|_| fx.process(0.0).all).collect();
        assert!(y.iter().all(|v| v.abs() < 1e-6), "stale echo replayed");
    }

    #[test]
    fn fr26_sends_follow_their_routes() {
        let mut fx = FxChain::new(48_000.0);
        fx.set_params(&FxParams {
            reverb_mix: 0.5,
            reverb_route: FxRoute::Headphones,
            delay_mix: 0.5,
            delay_route: FxRoute::Discord,
            ..Default::default()
        });
        let out = run(&mut fx, 48_000);
        // After the burst only effect tails remain.
        let tail = &out[12_000..];
        let energy = |f: fn(&FxOut) -> f32| tail.iter().map(|o| f(o).powi(2)).sum::<f32>();
        let mon = energy(|o| o.monitor);
        let cab = energy(|o| o.cable);
        let all = energy(|o| o.all);
        assert!(mon > 1e-3 && cab > 1e-3);
        // Monitor gets the reverb only, cable the echo only: different tails.
        let diff: f32 = tail.iter().map(|o| (o.monitor - o.cable).powi(2)).sum();
        assert!(diff > 1e-3);
        assert!(all >= mon.max(cab) * 0.5);
        // Each route alone matches the all-effects output minus the other send.
        for o in tail {
            assert!((o.monitor + o.cable - o.all).abs() < 1e-5);
        }
    }
}
