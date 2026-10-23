//! Uniform randomness where the browser game used `Math.random()`.
//!
//! The web build calls `Math.random()` itself. Desktop builds run a
//! SplitMix64 generator seeded once per thread from the standard library's
//! OS-entropy hash keys. Clock digits are not a substitute: the web clock has
//! millisecond resolution and the desktop clocks microsecond or 100 ns steps,
//! so `subsec_nanos() % n` repeats the same few values.

/// A uniform sample in [0, 1) (`Math.random()`).
pub fn unit() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Math::random()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // 53 random mantissa bits, as V8's Math.random.
        (native::next() >> 11) as f64 * (1. / (1u64 << 53) as f64)
    }
}

/// `Math.floor(Math.random() * n)`: uniform in `0..n` (0 when `n` is 0).
pub fn below(n: usize) -> usize {
    ((unit() * n as f64) as usize).min(n.saturating_sub(1))
}

/// `Math.floor(Math.random() * 2 ** 31)`: a uniform seed in `0..2^31`.
pub fn seed31() -> u32 {
    (unit() * f64::from(0x8000_0000u32)) as u32 & 0x7fff_ffff
}

/// 64 uniform bits (orderings and hashes, no browser counterpart).
pub fn bits() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        let high = (js_sys::Math::random() * 4_294_967_296.) as u64;
        let low = (js_sys::Math::random() * 4_294_967_296.) as u64;
        (high << 32) | low
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::next()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::cell::Cell;
    use std::hash::{BuildHasher, Hasher};

    thread_local! {
        static STATE: Cell<u64> = Cell::new({
            // RandomState draws its keys from the OS entropy source.
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u64(std::process::id().into());
            hasher.finish()
        });
    }

    /// SplitMix64 (Steele, Lea and Flood): every output is equidistributed
    /// over a full 2^64 period.
    pub fn next() -> u64 {
        STATE.with(|state| {
            let s = state.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
            state.set(s);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every bucket of a small range is drawn about equally often (the clock
    /// picks hit only some buckets: `subsec_nanos() % 4` was always 0).
    fn uniform(n: usize, draws: usize) {
        let mut counts = vec![0usize; n];
        for _ in 0..draws {
            counts[below(n)] += 1;
        }
        let expected = draws as f64 / n as f64;
        for (bucket, &count) in counts.iter().enumerate() {
            assert!(
                (count as f64 - expected).abs() < expected * 0.1,
                "bucket {bucket} of {n}: {count} vs {expected}"
            );
        }
    }

    #[test]
    fn small_ranges_are_uniform() {
        uniform(4, 40_000);
        uniform(14, 70_000);
        assert_eq!(below(0), 0);
        assert_eq!(below(1), 0);
    }

    #[test]
    fn unit_and_seeds_stay_in_range() {
        let mut below_half = 0;
        for _ in 0..20_000 {
            let u = unit();
            assert!((0. ..1.).contains(&u));
            let seed = seed31();
            assert!(seed <= 0x7fff_ffff);
            // The weather sample (seed * 2^-31) covers [0, 1), not only [0.5, 1).
            below_half += usize::from(seed < 0x4000_0000);
        }
        assert!((9_000..11_000).contains(&below_half), "{below_half}");
    }
}
