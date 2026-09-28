//! Tiny seeded PCG32 so humanize is deterministic (hard rule: same input,
//! same output).

#[derive(Clone, Debug)]
pub(crate) struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub(crate) fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed ^ 0x853c_49e6_748f_ea9b);
        rng.next_u32();
        rng
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `[-1, 1)`.
    pub(crate) fn bipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Pcg32::new(42);
        let mut b = Pcg32::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        let mut c = Pcg32::new(43);
        assert_ne!(Pcg32::new(42).next_u32(), c.next_u32());
    }

    #[test]
    fn bipolar_range() {
        let mut r = Pcg32::new(7);
        for _ in 0..10_000 {
            let v = r.bipolar();
            assert!((-1.0..1.0).contains(&v));
        }
    }
}
