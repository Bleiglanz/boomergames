//! Deterministic pseudorandom number generator (`SplitMix64`) with zero OS entropy.

/// Fast, reproducible 64-bit PRNG used for deterministic racks, AI perturbation,
/// and golden replay tests across native and WASM targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Creates a new generator from a 64-bit seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Advances the generator and returns the next pseudorandom `u64`.
    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a uniform `f64` in `[0.0, 1.0)` using the top 53 mantissa bits.
    #[must_use]
    pub fn next_unit_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        #[expect(
            clippy::cast_precision_loss,
            reason = "53-bit integer fits losslessly in IEEE-754 f64 mantissa"
        )]
        let value = (bits as f64) * (1.0 / ((1_u64 << 53) as f64));
        value
    }

    /// Returns a uniform integer in `0..bound`, or `0` if `bound == 0`.
    #[must_use]
    pub fn next_bounded(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let candidate = self.next_u64();
            if candidate >= threshold {
                return candidate % bound;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_is_reproducible() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..128 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn unit_f64_stays_in_unit_interval() {
        let mut rng = SplitMix64::new(123_456);
        for _ in 0..1024 {
            let v = rng.next_unit_f64();
            assert!((0.0..1.0).contains(&v));
        }
    }
}
