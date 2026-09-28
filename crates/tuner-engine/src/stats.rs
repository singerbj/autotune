//! Lock-free runtime health counters (FR-18).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Histogram bucket width and count: 10 µs buckets up to 20 ms.
const BUCKET_US: u32 = 10;
const BUCKETS: usize = 2000;

/// Callback duration histogram plus xrun/overflow counters, written from
/// audio threads with relaxed atomics only.
#[derive(Debug)]
pub struct CallbackStats {
    hist: Box<[AtomicU32]>,
    max_us: AtomicU32,
    pub(crate) capture_discontinuities: AtomicU64,
    pub(crate) monitor_underruns: AtomicU64,
    pub(crate) cable_underruns: AtomicU64,
    pub(crate) ring_overflows: AtomicU64,
    pub(crate) monitor_fill_us: AtomicU32,
    pub(crate) cable_fill_us: AtomicU32,
    pub(crate) cable_ratio_ppm: AtomicU32,
}

impl Default for CallbackStats {
    fn default() -> Self {
        Self {
            hist: (0..BUCKETS).map(|_| AtomicU32::new(0)).collect(),
            max_us: AtomicU32::new(0),
            capture_discontinuities: AtomicU64::new(0),
            monitor_underruns: AtomicU64::new(0),
            cable_underruns: AtomicU64::new(0),
            ring_overflows: AtomicU64::new(0),
            monitor_fill_us: AtomicU32::new(0),
            cable_fill_us: AtomicU32::new(0),
            cable_ratio_ppm: AtomicU32::new(1_000_000),
        }
    }
}

impl CallbackStats {
    /// Record one callback duration (real-time safe).
    pub fn record(&self, us: u32) {
        let b = ((us / BUCKET_US) as usize).min(BUCKETS - 1);
        self.hist[b].fetch_add(1, Ordering::Relaxed);
        self.max_us.fetch_max(us, Ordering::Relaxed);
    }

    pub fn max_us(&self) -> u32 {
        self.max_us.load(Ordering::Relaxed)
    }

    /// Percentile of recorded durations in µs (upper bucket edge).
    pub fn percentile_us(&self, p: f64) -> u32 {
        let counts: Vec<u32> = self
            .hist
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        let total: u64 = counts.iter().map(|&c| u64::from(c)).sum();
        if total == 0 {
            return 0;
        }
        let want = (total as f64 * p).ceil() as u64;
        let mut acc = 0u64;
        for (i, &c) in counts.iter().enumerate() {
            acc += u64::from(c);
            if acc >= want {
                return (i as u32 + 1) * BUCKET_US;
            }
        }
        BUCKETS as u32 * BUCKET_US
    }

    pub fn callbacks(&self) -> u64 {
        self.hist
            .iter()
            .map(|c| u64::from(c.load(Ordering::Relaxed)))
            .sum()
    }

    pub fn xruns(&self) -> u64 {
        self.capture_discontinuities.load(Ordering::Relaxed)
            + self.monitor_underruns.load(Ordering::Relaxed)
            + self.cable_underruns.load(Ordering::Relaxed)
            + self.ring_overflows.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr18_percentiles() {
        let s = CallbackStats::default();
        for _ in 0..99 {
            s.record(25);
        }
        s.record(900);
        assert_eq!(s.percentile_us(0.5), 30);
        assert_eq!(s.percentile_us(0.99), 30);
        assert_eq!(s.percentile_us(1.0), 910);
        assert_eq!(s.max_us(), 900);
        assert_eq!(s.callbacks(), 100);
    }

    #[test]
    fn huge_durations_clamp_to_last_bucket() {
        let s = CallbackStats::default();
        s.record(u32::MAX);
        assert_eq!(s.percentile_us(0.99), 20_000);
    }
}
