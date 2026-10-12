//! Recovered rigid-body integration kernels (001469e0 / 0013c010).
//! Collision impulses, rider stabilization, and external corrections are supplied
//! by callers; this module does not replace those original vehicle systems.
use crate::{
    Result,
    wheel::{Vec3, add, cross, dot, scale, sub},
};
use serde::Serialize;

/// The original default integrator is two-step Adams–Bashforth, including its
/// zero-history startup. Preserve operation order instead of using Euler/f64.
pub fn integrate<const N: usize>(
    state: &mut [f32; N],
    previous: &mut [f32; N],
    derivative: [f32; N],
    dt: f32,
) {
    let half_dt = 0.5 * dt;
    for i in 0..N {
        state[i] += half_dt * (3.0 * derivative[i] - previous[i]);
    }
    *previous = derivative;
}

/// Column-major rotation basis.
pub type Basis = [Vec3; 3];
pub fn transform(b: Basis, v: Vec3) -> Vec3 {
    add(add(scale(b[0], v[0]), scale(b[1], v[1])), scale(b[2], v[2]))
}
pub fn inverse_transform(b: Basis, v: Vec3) -> Vec3 {
    [dot(b[0], v), dot(b[1], v), dot(b[2], v)]
}

/// Quaternion component order is W,X,Y,Z in the original body and animation data.
pub fn basis(q: [f32; 4]) -> Basis {
    let [w, x, y, z] = q;
    let yy = y * 2.0;
    let zz = z * 2.0;
    let xx = x * x * 2.0;
    [
        [1.0 - (y * yy + z * zz), x * yy + w * zz, x * zz - w * yy],
        [x * yy - w * zz, 1.0 - (xx + z * zz), y * zz + w * x * 2.0],
        [x * zz + w * yy, y * zz - w * x * 2.0, 1.0 - (xx + y * yy)],
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct BodyConfig {
    pub mass: f32,
    pub inertia: Vec3,
    pub center_of_mass: Vec3,
    pub max_speed_squared: f32,
    pub max_angular_speed_squared: f32,
    pub angular_min: Vec3,
    pub angular_max: Vec3,
    pub translation_axes: [bool; 3],
    pub rotation_axes: [bool; 3],
}
impl BodyConfig {
    pub fn new(mass: f32, inertia: Vec3, center_of_mass: Vec3) -> Result<Self> {
        if !mass.is_finite()
            || mass <= 0.0
            || inertia.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || center_of_mass.iter().any(|v| !v.is_finite())
        {
            return Err("invalid body mass/inertia".into());
        }
        // 00145c00 defaults. Vehicle-specific changes remain explicit setters.
        Ok(Self {
            mass,
            inertia,
            center_of_mass,
            max_speed_squared: f32::from_bits(0x468e6200),
            max_angular_speed_squared: 1875.0,
            angular_min: [-25.0; 3],
            angular_max: [25.0; 3],
            translation_axes: [true; 3],
            rotation_axes: [true; 3],
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct BodyState {
    pub center: Vec3,
    pub orientation: [f32; 4],
    pub linear_velocity: Vec3,
    pub local_angular_velocity: Vec3,
    pub force: Vec3,
    pub local_torque: Vec3,
    previous_acceleration: Vec3,
    previous_velocity: Vec3,
    previous_angular_acceleration: Vec3,
    previous_quaternion_derivative: [f32; 4],
}
impl BodyState {
    pub fn new(origin: Vec3, orientation: [f32; 4], config: &BodyConfig) -> Result<Self> {
        if origin
            .iter()
            .chain(orientation.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite body pose".into());
        }
        let length = orientation.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length <= 0.0 {
            return Err("zero body quaternion".into());
        }
        let orientation = orientation.map(|v| v / length);
        Ok(Self {
            center: add(origin, transform(basis(orientation), config.center_of_mass)),
            orientation,
            linear_velocity: [0.; 3],
            local_angular_velocity: [0.; 3],
            force: [0.; 3],
            local_torque: [0.; 3],
            previous_acceleration: [0.; 3],
            previous_velocity: [0.; 3],
            previous_angular_acceleration: [0.; 3],
            previous_quaternion_derivative: [0.; 4],
        })
    }
    pub fn basis(&self) -> Basis {
        basis(self.orientation)
    }
    pub fn origin(&self, config: &BodyConfig) -> Vec3 {
        sub(self.center, transform(self.basis(), config.center_of_mass))
    }
    pub fn world_angular_velocity(&self) -> Vec3 {
        transform(self.basis(), self.local_angular_velocity)
    }
    pub fn velocity_at(&self, point: Vec3) -> Vec3 {
        self.velocity_at_with_center(point, self.center)
    }
    ///001ed330 uses body+60's cached world center before001469e0 refreshes
    ///it from the Model and a newly shifted local center of mass.
    pub fn velocity_at_with_center(&self, point: Vec3, cached_center: Vec3) -> Vec3 {
        add(
            self.linear_velocity,
            cross(self.world_angular_velocity(), sub(point, cached_center)),
        )
    }
    pub fn apply_world_force(&mut self, force: Vec3, point: Vec3) {
        self.force = add(self.force, force);
        let b = self.basis();
        let arm = inverse_transform(b, sub(point, self.center));
        self.local_torque = add(self.local_torque, cross(arm, inverse_transform(b, force)));
    }
    pub fn apply_local_force(&mut self, config: &BodyConfig, force: Vec3, point: Vec3) {
        self.force = add(self.force, transform(self.basis(), force));
        self.local_torque = add(
            self.local_torque,
            cross(sub(point, config.center_of_mass), force),
        );
    }
    /// Integrates accumulated forces. The vehicle invokes this once per <=1/60s
    /// substep after contact and tire forces; force accumulators are then cleared.
    pub fn step(&mut self, config: &BodyConfig, dt: f32) -> Result<()> {
        if !dt.is_finite()
            || dt < 0.0
            || !self
                .force
                .iter()
                .chain(self.local_torque.iter())
                .all(|x| x.is_finite())
        {
            return Err("invalid body step".into());
        }
        let mut a = scale(self.force, 1.0 / config.mass);
        for (i, v) in a.iter_mut().enumerate() {
            if !config.translation_axes[i] {
                *v = 0.0;
                self.linear_velocity[i] = 0.0;
                self.previous_acceleration[i] = 0.0;
            }
        }
        integrate(
            &mut self.linear_velocity,
            &mut self.previous_acceleration,
            a,
            dt,
        );
        limit_length(&mut self.linear_velocity, config.max_speed_squared);
        integrate(
            &mut self.center,
            &mut self.previous_velocity,
            self.linear_velocity,
            dt,
        );
        let mut alpha = std::array::from_fn(|i| self.local_torque[i] * (1.0 / config.inertia[i]));
        for (i, v) in alpha.iter_mut().enumerate() {
            if !config.rotation_axes[i] {
                *v = 0.0;
                self.local_angular_velocity[i] = 0.0;
                self.previous_angular_acceleration[i] = 0.0;
            }
        }
        integrate(
            &mut self.local_angular_velocity,
            &mut self.previous_angular_acceleration,
            alpha,
            dt,
        );
        limit_length(
            &mut self.local_angular_velocity,
            config.max_angular_speed_squared,
        );
        for i in 0..3 {
            self.local_angular_velocity[i] =
                self.local_angular_velocity[i].clamp(config.angular_min[i], config.angular_max[i]);
        }
        let [w, x, y, z] = self.orientation;
        let [a, b, c] = self.local_angular_velocity;
        let dq = [
            ((z * c + x * a) + y * b) * -0.5,
            ((w * a + y * c) - z * b) * 0.5,
            ((w * b + z * a) - x * c) * 0.5,
            ((w * c + x * b) - y * a) * 0.5,
        ];
        integrate(
            &mut self.orientation,
            &mut self.previous_quaternion_derivative,
            dq,
            dt,
        );
        let q = self.orientation;
        let reciprocal = 1.0 / (((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]) + q[3] * q[3]).sqrt();
        self.orientation = q.map(|x| x * reciprocal);
        self.force = [0.; 3];
        self.local_torque = [0.; 3];
        if self
            .center
            .iter()
            .chain(self.orientation.iter())
            .chain(self.linear_velocity.iter())
            .chain(self.local_angular_velocity.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite rigid-body result".into());
        }
        Ok(())
    }
}
fn limit_length(v: &mut Vec3, max_squared: f32) {
    let square = dot(*v, *v);
    if square > max_squared {
        *v = scale(*v, (max_squared / square).sqrt());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integrator_preserves_previous_derivative_and_original_startup() {
        let mut value = [0.0];
        let mut previous = [0.0];
        integrate(&mut value, &mut previous, [2.0], 0.1);
        assert_eq!(value, [0.3]);
        integrate(&mut value, &mut previous, [2.0], 0.1);
        assert_eq!(value, [0.5]);
        integrate(&mut value, &mut previous, [0.0], 0.1);
        assert_eq!(value, [0.4]);
        assert_eq!(previous, [0.0]);
    }
    #[test]
    fn force_at_center_translates_and_offcenter_force_rotates() {
        let c = BodyConfig::new(10., [2., 3., 4.], [0.; 3]).unwrap();
        let mut b = BodyState::new([0.; 3], [1., 0., 0., 0.], &c).unwrap();
        b.apply_world_force([10., 0., 0.], b.center);
        b.step(&c, 0.01).unwrap();
        assert!(b.center[0] > 0.);
        assert_eq!(b.local_angular_velocity, [0.; 3]);
        b.apply_world_force([0., 0., 10.], add(b.center, [1., 0., 0.]));
        b.step(&c, 0.01).unwrap();
        assert!(b.local_angular_velocity[1] < 0.);
        assert_eq!(b.force, [0.; 3]);
        assert_eq!(b.local_torque, [0.; 3]);
    }
    #[test]
    fn rotated_force_roundtrip_and_center_offset() {
        let q = [0.5, 0.5, 0.5, 0.5];
        let v = [2., 3., 4.];
        assert_eq!(inverse_transform(basis(q), transform(basis(q), v)), v);
        let c = BodyConfig::new(2., [1.; 3], [1., 2., 3.]).unwrap();
        let b = BodyState::new([4., 5., 6.], q, &c).unwrap();
        assert_eq!(b.origin(&c), [4., 5., 6.]);
    }
}
