//! Type0 blowing smoke `0019c200` movement terms: per-step emitter
//! displacement drives size/life/rate clamps, path-spread births and the VU
//! macro alignment of the authored velocity. A static emitter has zero
//! displacement and reduces to the previously connected static path.
use serde::Deserialize;

pub type Vec3 = [f32; 3];
#[derive(Clone, Deserialize)]
pub struct Motion {
    pub size_ratio: f32,
    pub life_ratio: f32,
    pub distance_ratio: f32,
    pub speed_ratio: f32,
    pub min_per_frame: f32,
}
impl Default for Motion {
    /// `0019cbb0`/`0019ad90` loader defaults.
    fn default() -> Self {
        Self {
            size_ratio: 0.01,
            life_ratio: 5.,
            distance_ratio: 0.01,
            speed_ratio: 0.,
            min_per_frame: 0.,
        }
    }
}
/// Native c.lt/c.le clamp order: below the minimum selects the minimum,
/// otherwise anything not at or below the maximum selects the maximum.
pub fn clamp(value: f32, low: f32, high: f32) -> f32 {
    if value < low {
        low
    } else if value <= high {
        value
    } else {
        high
    }
}
/// FPU length: adda(x², y²), madd z², sqrt.
pub fn distance(d: Vec3) -> f32 {
    ((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]).sqrt()
}
/// VU `vmul`/`vadday`/`vmaddz`/`vrsqrt`/`vmulq` normalization.
fn normalize(v: Vec3) -> Vec3 {
    let squared = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    let q = if squared == 0. {
        f32::MAX
    } else {
        (1. / f64::from(squared).sqrt()) as f32
    };
    v.map(|x| x * q)
}
/// `vopmula`/`vopmsub` outer product: a × b.
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[(i + 1) % 3] * b[(i + 2) % 3] - b[(i + 1) % 3] * a[(i + 2) % 3])
}
/// Authored X/Y/Z velocity along side/up/travel axes once the step moves
/// more than 0.25 ft; near-vertical travel replaces world up with +Z.
pub fn velocity(base: Vec3, delta: Vec3) -> Vec3 {
    if distance(delta) <= 0.25 {
        return base;
    }
    let n = normalize(delta);
    let axis = if n[0].abs() < 0.015625 && n[2].abs() < 0.015625 {
        [0., 0., 1.]
    } else {
        [0., 1., 0.]
    };
    let side = normalize(cross(axis, n));
    let up = normalize(cross(n, side));
    std::array::from_fn(|i| (base[0] * side[i] + base[1] * up[i]) + base[2] * n[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alignment_keeps_short_steps_and_rotates_travel_basis() {
        assert_eq!(velocity([1., 10., 2.], [0.2, 0., 0.]), [1., 10., 2.]);
        // Level travel along +X: side = up × n = (0,0,-1), up stays +Y.
        let v = velocity([0., 10., 3.], [1., 0., 0.]);
        assert_eq!(v, [3., 10., 0.]);
        let side = velocity([2., 0., 0.], [1., 0., 0.]);
        assert_eq!(side, [0., 0., -2.]);
        // Vertical travel selects the +Z reference axis.
        let vertical = velocity([0., 0., 5.], [0., 1., 0.]);
        assert_eq!(vertical, [0., 5., 0.]);
        assert!(
            velocity([0., 10., 0.], [0., 0.5, 0.])
                .iter()
                .all(|x| x.is_finite())
        );
    }
    #[test]
    fn native_clamp_and_length_order() {
        assert_eq!(clamp(0.5, 1., 6.), 1.);
        assert_eq!(clamp(7., 1., 6.), 6.);
        assert_eq!(clamp(f32::NAN, 1., 6.), 6.);
        assert_eq!(distance([3., 0., 4.]), 5.);
    }
}
