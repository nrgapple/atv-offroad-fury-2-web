//! Ground bank component recovered from SLUS_211.04/GAME_REL 004dbd10.
//! The original writes orientation, not a competing roll spring. The contact
//! adapter and host trigonometry do not establish native whole-step parity.
use super::*;
use crate::wheel::cross;
fn unit(v: V) -> V {
    scale(v, 1. / dot(v, v).sqrt().max(1e-6))
}
pub(super) fn angle(b: dynamics::Basis, normal: V) -> f32 {
    let forward = b[2];
    let up = unit(sub(normal, scale(forward, dot(normal, forward))));
    let right = cross(up, forward);
    (-dot(b[1], right)).atan2(dot(b[1], up))
}
pub(super) fn rotate(q: [f32; 4], change: f32) -> [f32; 4] {
    let (s, c) = (change * 0.5).sin_cos();
    let [w, x, y, z] = q;
    let result = [w * c - z * s, x * c + y * s, y * c - x * s, z * c + w * s];
    let norm = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    result.map(|v| v / norm)
}
impl Motorcycle {
    pub(super) fn bank_reference(&self, normal: V, speed: f32) -> V {
        let min = self.value("Roll Control", "MinStandUprightSpeed") * 1.46666;
        let max = self.value("Roll Control", "MaxStandUprightSpeed") * 1.46666;
        let blend = if speed <= 0. || max <= min {
            0.
        } else {
            ((speed - min) / (max - min)).clamp(0., 1.)
        };
        unit(add(scale([0., 1., 0.], 1. - blend), scale(normal, blend)))
    }
    pub(super) fn bank_target(&self, speed: f32, steer: f32) -> f32 {
        let max = self.value("Roll Control", "MaxRollSpeed") * 1.46666;
        let scale = if max > 0. {
            (speed / max).clamp(0., 1.)
        } else {
            0.
        };
        -self.value("Roll Control", "MaxRollCommand").to_radians() * scale * steer
    }
    pub(super) fn apply_bank(&mut self, previous: f32, target: f32, normal: V, dt: f32) {
        let next = previous + (target - previous) * self.value("Roll Control", "RollGain") * dt;
        let actual = angle(self.body.basis(), normal);
        self.body.orientation = rotate(self.body.orientation, next - actual);
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
