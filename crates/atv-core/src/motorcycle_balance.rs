//! Ground bank geometry recovered from SLUS_211.04/GAME_REL 004dbd10: the bank
//! angle about the forward axis against a reference that blends from vertical
//! to the contact normal between the stand-upright speeds. The once-per-frame
//! orientation write lives in `motorcycle_drive.rs`. Host trigonometry does not
//! establish native whole-step parity.
use super::*;
use crate::math::Det;
use crate::wheel::cross;
fn unit(v: V) -> V {
    scale(v, 1. / dot(v, v).sqrt().max(1e-6))
}
pub(super) fn angle(b: dynamics::Basis, normal: V) -> f32 {
    let forward = b[2];
    let up = unit(sub(normal, scale(forward, dot(normal, forward))));
    let right = cross(up, forward);
    (-dot(b[1], right)).atan2_d(dot(b[1], up))
}
pub(super) fn rotate(q: [f32; 4], change: f32) -> [f32; 4] {
    let (s, c) = (change * 0.5).sin_cos_d();
    let [w, x, y, z] = q;
    let result = [w * c - z * s, x * c + y * s, y * c - x * s, z * c + w * s];
    let norm = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    result.map(|v| v / norm)
}
impl Motorcycle {
    pub(super) fn bank_reference(&self, normal: V, speed: f32) -> V {
        let (min, max) = (self.params.stand_min, self.params.stand_max);
        let blend = if speed <= 0. || max <= min {
            0.
        } else {
            ((speed - min) / (max - min)).clamp(0., 1.)
        };
        unit(add(scale([0., 1., 0.], 1. - blend), scale(normal, blend)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_rotates_into_the_turn_and_preserves_heading_and_pitch() {
        let q = [0.973, 0.13, 0.18, 0.04];
        let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        let q = q.map(|v| v / length);
        let before = dynamics::basis(q);
        for target in [-0.4, 0.4] {
            let after = dynamics::basis(rotate(q, target - angle(before, [0., 1., 0.])));
            assert!((angle(after, [0., 1., 0.]) - target).abs() < 1e-5);
            assert!(sub(before[2], after[2]).iter().all(|v| v.abs() < 1e-5));
        }
    }
}
