//! One implementation of every transcendental the simulation uses, on every
//! target. The browser build's `std` maths already is Rust's `libm` (Wasm has
//! no system maths library); desktop `std` calls the platform libm, whose
//! results differ in the last bit for 1-20% of inputs (sin, cos, tan, atan,
//! atan2, asin, acos, exp, powf, hypot, ...), enough for a race to diverge
//! within seconds. Simulation code calls these instead of the `std` methods
//! (enforced by clippy's `disallowed-methods`), so the browser keeps its
//! results and macOS, Windows and Linux reproduce them bit for bit.
//! Basic arithmetic, `sqrt`, `abs`, `floor`, `powi` and friends are exactly
//! specified and stay on `std`.

/// Deterministic transcendentals for `f32` and `f64`.
pub trait Det: Copy {
    fn sin_d(self) -> Self;
    fn cos_d(self) -> Self;
    fn sin_cos_d(self) -> (Self, Self);
    fn tan_d(self) -> Self;
    fn asin_d(self) -> Self;
    fn acos_d(self) -> Self;
    fn atan_d(self) -> Self;
    fn atan2_d(self, x: Self) -> Self;
    fn exp_d(self) -> Self;
    fn ln_d(self) -> Self;
    fn powf_d(self, n: Self) -> Self;
    fn hypot_d(self, other: Self) -> Self;
}

macro_rules! det {
    ($t:ty, $sin:path, $cos:path, $tan:path, $asin:path, $acos:path, $atan:path, $atan2:path, $exp:path, $ln:path, $pow:path, $hypot:path) => {
        impl Det for $t {
            #[inline]
            fn sin_d(self) -> Self {
                $sin(self)
            }
            #[inline]
            fn cos_d(self) -> Self {
                $cos(self)
            }
            /// `std` sin_cos is a separate libm entry on some platforms; the
            /// browser computes it as sin and cos.
            #[inline]
            fn sin_cos_d(self) -> (Self, Self) {
                ($sin(self), $cos(self))
            }
            #[inline]
            fn tan_d(self) -> Self {
                $tan(self)
            }
            #[inline]
            fn asin_d(self) -> Self {
                $asin(self)
            }
            #[inline]
            fn acos_d(self) -> Self {
                $acos(self)
            }
            #[inline]
            fn atan_d(self) -> Self {
                $atan(self)
            }
            #[inline]
            fn atan2_d(self, x: Self) -> Self {
                $atan2(self, x)
            }
            #[inline]
            fn exp_d(self) -> Self {
                $exp(self)
            }
            #[inline]
            fn ln_d(self) -> Self {
                $ln(self)
            }
            #[inline]
            fn powf_d(self, n: Self) -> Self {
                $pow(self, n)
            }
            #[inline]
            fn hypot_d(self, other: Self) -> Self {
                $hypot(self, other)
            }
        }
    };
}
det!(
    f32,
    libm::sinf,
    libm::cosf,
    libm::tanf,
    libm::asinf,
    libm::acosf,
    libm::atanf,
    libm::atan2f,
    libm::expf,
    libm::logf,
    libm::powf,
    libm::hypotf
);
det!(
    f64,
    libm::sin,
    libm::cos,
    libm::tan,
    libm::asin,
    libm::acos,
    libm::atan,
    libm::atan2,
    libm::exp,
    libm::log,
    libm::pow,
    libm::hypot
);

/// `f32::atan2` as a function value (`atan2(y, x)`).
pub fn atan2f(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bit patterns of a fixed input sweep, identical on every target: the
    /// values below were produced by the browser (Wasm) build, whose `std`
    /// maths is this implementation. A platform that diverges fails here.
    #[test]
    fn results_are_the_same_on_every_target() {
        let mut hash = 0xcbf29ce484222325u64;
        let mut mix = |bits: u64| {
            hash ^= bits;
            hash = hash.wrapping_mul(0x100000001b3);
        };
        let mut seed = 12345u64;
        for _ in 0..20_000 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let a = ((seed >> 40) as f32 / (1u64 << 24) as f32) * 20. - 10.;
            let b = ((seed >> 16 & 0xffffff) as f32 / (1u64 << 24) as f32) * 20. - 10.;
            let u = a / 10.;
            for v in [
                a.sin_d(),
                a.cos_d(),
                a.tan_d(),
                u.asin_d(),
                u.acos_d(),
                a.atan_d(),
                a.atan2_d(b),
                (a / 4.).exp_d(),
                a.abs().ln_d(),
                a.abs().powf_d(b / 4.),
                a.hypot_d(b),
            ] {
                mix(u64::from(v.to_bits()));
            }
            let (x, y) = (f64::from(a), f64::from(b));
            for v in [
                x.sin_d(),
                x.cos_d(),
                x.atan2_d(y),
                (x / 10.).acos_d(),
                x.hypot_d(y),
            ] {
                mix(v.to_bits());
            }
        }
        assert_eq!(
            hash, EXPECTED,
            "deterministic maths differ on this target: {hash:#x}"
        );
    }
    const EXPECTED: u64 = 0x6d214a7aadf0a9d0;
}
