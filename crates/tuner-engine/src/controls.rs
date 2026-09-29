//! Continuous control-plane → engine parameters as atomics.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use tuner_dsp::{Scale, TuningParams, VoiceRange};

#[derive(Debug, Default)]
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Lock-free parameter block shared by the control plane and audio threads.
/// Writers bump `generation` after a full update; the capture thread
/// re-reads only when it changes.
#[derive(Debug)]
pub struct SharedControls {
    key: AtomicU8,
    scale: AtomicU8,
    custom_mask: AtomicU32,
    retune_ms: AtomicF32,
    humanize: AtomicF32,
    voice_range: AtomicU8,
    mix: AtomicF32,
    gate_db: AtomicF32,
    bypass: AtomicBool,
    monitor_enabled: AtomicBool,
    monitor_volume: AtomicF32,
    generation: AtomicU64,
}

impl Default for SharedControls {
    fn default() -> Self {
        let c = Self {
            key: AtomicU8::new(0),
            scale: AtomicU8::new(0),
            custom_mask: AtomicU32::new(0),
            retune_ms: AtomicF32::default(),
            humanize: AtomicF32::default(),
            voice_range: AtomicU8::new(0),
            mix: AtomicF32::default(),
            gate_db: AtomicF32::default(),
            bypass: AtomicBool::new(false),
            monitor_enabled: AtomicBool::new(true),
            monitor_volume: AtomicF32::new(1.0),
            generation: AtomicU64::new(0),
        };
        c.set_params(&TuningParams::default());
        c
    }
}

impl SharedControls {
    pub fn new(params: &TuningParams, monitor_enabled: bool, monitor_volume: f32) -> Self {
        let c = Self::default();
        c.set_params(params);
        c.set_monitor(monitor_enabled, monitor_volume);
        c
    }

    pub fn set_params(&self, p: &TuningParams) {
        let p = p.sanitized();
        self.key.store(p.key, Ordering::Relaxed);
        self.scale.store(
            match p.scale {
                Scale::Chromatic => 0,
                Scale::Major => 1,
                Scale::NaturalMinor => 2,
                Scale::Custom => 3,
            },
            Ordering::Relaxed,
        );
        self.custom_mask
            .store(u32::from(p.custom_mask), Ordering::Relaxed);
        self.retune_ms.store(p.retune_ms);
        self.humanize.store(p.humanize);
        self.voice_range.store(
            match p.voice_range {
                VoiceRange::Low => 0,
                VoiceRange::Mid => 1,
                VoiceRange::High => 2,
            },
            Ordering::Relaxed,
        );
        self.mix.store(p.mix);
        self.gate_db.store(p.gate_threshold_db);
        self.bypass.store(p.bypass, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn params(&self) -> TuningParams {
        TuningParams {
            key: self.key.load(Ordering::Relaxed),
            scale: match self.scale.load(Ordering::Relaxed) {
                1 => Scale::Major,
                2 => Scale::NaturalMinor,
                3 => Scale::Custom,
                _ => Scale::Chromatic,
            },
            custom_mask: self.custom_mask.load(Ordering::Relaxed) as u16,
            retune_ms: self.retune_ms.load(),
            humanize: self.humanize.load(),
            voice_range: match self.voice_range.load(Ordering::Relaxed) {
                0 => VoiceRange::Low,
                2 => VoiceRange::High,
                _ => VoiceRange::Mid,
            },
            mix: self.mix.load(),
            gate_threshold_db: self.gate_db.load(),
            bypass: self.bypass.load(Ordering::Relaxed),
        }
    }

    /// Toggle bypass alone (tray / hotkey path, FR-10). Returns the new state.
    pub fn toggle_bypass(&self) -> bool {
        let new = !self.bypass.fetch_xor(true, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
        new
    }

    pub fn set_bypass(&self, on: bool) {
        self.bypass.store(on, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn bypass(&self) -> bool {
        self.bypass.load(Ordering::Relaxed)
    }

    pub fn set_monitor(&self, enabled: bool, volume: f32) {
        self.monitor_enabled.store(enabled, Ordering::Relaxed);
        self.monitor_volume.store(if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        });
    }

    /// Monitor gain the capture thread applies (0 when disabled).
    pub fn monitor_gain(&self) -> f32 {
        if self.monitor_enabled.load(Ordering::Relaxed) {
            self.monitor_volume.load()
        } else {
            0.0
        }
    }

    pub fn monitor_enabled(&self) -> bool {
        self.monitor_enabled.load(Ordering::Relaxed)
    }

    pub fn monitor_volume(&self) -> f32 {
        self.monitor_volume.load()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_params_and_generation() {
        let c = SharedControls::default();
        let g0 = c.generation();
        let p = TuningParams {
            key: 5,
            scale: Scale::Custom,
            custom_mask: 0b101,
            retune_ms: 42.0,
            humanize: 0.3,
            voice_range: VoiceRange::High,
            mix: 0.7,
            gate_threshold_db: -45.0,
            bypass: true,
        };
        c.set_params(&p);
        assert_eq!(c.params(), p);
        assert!(c.generation() > g0);
    }

    #[test]
    fn fr10_toggle_bypass() {
        let c = SharedControls::default();
        assert!(c.toggle_bypass());
        assert!(c.bypass());
        assert!(!c.toggle_bypass());
    }

    #[test]
    fn fr03_monitor_gain() {
        let c = SharedControls::default();
        c.set_monitor(true, 0.5);
        assert_eq!(c.monitor_gain(), 0.5);
        c.set_monitor(false, 0.5);
        assert_eq!(c.monitor_gain(), 0.0);
        c.set_monitor(true, 7.0);
        assert_eq!(c.monitor_gain(), 1.0);
    }
}
