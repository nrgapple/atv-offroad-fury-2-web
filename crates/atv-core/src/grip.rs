//! Recovered scalar stages of the body grip update at 0x001d8540.
//! The caller supplies original contact state, material data, steering-table
//! sine/cosine, and point velocities after any moving-surface correction.
//! These functions do not substitute a new tire model or collision solver.
use crate::Result;
use crate::wheel::{Vec3, cross};

#[derive(Debug, Clone, Copy)]
pub struct LateralInput {
    pub any_contact: bool,
    pub front_contact: bool,
    pub front_point_velocity: Vec3,
    pub rear_point_velocity: Vec3,
    pub steering_sine: f32,
    pub steering_cosine: f32,
    /// Effective tire friction after the original profile/garage adjustments.
    pub tire_friction: f32,
    pub steering_reduction: f32,
    /// Original material record +4.
    pub material_grip: f32,
    /// Original material record +0x10.
    pub material_powerslide: f32,
    pub throttle: f32,
}

/// Body-X forces at the front and rear virtual contact points.
/// They remain body-X forces after using steering to measure front slip.
pub fn lateral_forces(input: LateralInput) -> Result<[f32; 2]> {
    if !input
        .front_point_velocity
        .iter()
        .chain(input.rear_point_velocity.iter())
        .chain(
            [
                input.steering_sine,
                input.steering_cosine,
                input.tire_friction,
                input.steering_reduction,
                input.material_grip,
                input.material_powerslide,
                input.throttle,
            ]
            .iter(),
        )
        .all(|x| x.is_finite())
    {
        return Err("non-finite lateral grip input".into());
    }
    if !input.any_contact {
        return Ok([0.0; 2]);
    }
    let front_slip = input.steering_cosine * input.front_point_velocity[0]
        - input.steering_sine * input.front_point_velocity[2];
    let gain = f32::from_bits(0xc234_0000);
    let front_gain = if input.front_contact {
        gain
    } else {
        gain * input.steering_reduction
    };
    let rear_gain = if input.throttle != 0.0 {
        gain * input.material_powerslide
    } else {
        gain
    };
    let friction = input.material_grip * input.tire_friction;
    let forces = [
        front_slip * (front_gain * friction),
        input.rear_point_velocity[0] * (rear_gain * friction),
    ];
    if !forces.iter().all(|x| x.is_finite()) {
        return Err("lateral grip overflow".into());
    }
    Ok(forces)
}

/// Locations are offset by ±3 feet along body Z from the center-of-gravity Z.
/// This helper returns body-space totals. For exact accumulation order, callers
/// rotate and add each point force separately as the original routine does.
pub fn lateral_force_and_moment(forces: [f32; 2], center_of_gravity: Vec3) -> Result<(Vec3, Vec3)> {
    if !forces
        .iter()
        .chain(center_of_gravity.iter())
        .all(|x| x.is_finite())
    {
        return Err("non-finite lateral force or center of gravity".into());
    }
    let mut force = [0.0; 3];
    let mut moment = [0.0; 3];
    for (x, distance) in forces.into_iter().zip([3.0, -3.0]) {
        // Keep the authored point construction/subtraction order.
        let point_z = center_of_gravity[2] + distance;
        let lever = [
            -center_of_gravity[0],
            -center_of_gravity[1],
            point_z - center_of_gravity[2],
        ];
        let contribution = [x, 0.0, 0.0];
        let torque = cross(lever, contribution);
        for i in 0..3 {
            force[i] += contribution[i];
            moment[i] += torque[i];
        }
    }
    if !force.iter().chain(moment.iter()).all(|x| x.is_finite()) {
        return Err("lateral moment overflow".into());
    }
    Ok((force, moment))
}

/// The material +8 speed-squared drag stage. Original caller executes this only
/// when any wheel contacts. World velocity determines direction; the separately
/// supplied body speed determines magnitude, as in the recovered routine.
pub fn material_drag(world_velocity: Vec3, body_speed: f32, coefficient: f32) -> Result<Vec3> {
    if !world_velocity
        .iter()
        .chain([body_speed, coefficient].iter())
        .all(|x| x.is_finite())
    {
        return Err("non-finite material drag input".into());
    }
    let length_squared = (world_velocity[0] * world_velocity[0]
        + world_velocity[1] * world_velocity[1])
        + world_velocity[2] * world_velocity[2];
    if length_squared == 0.0 {
        return Ok([0.0; 3]);
    }
    let inverse_length = 1.0 / length_squared.sqrt();
    let magnitude = -coefficient * (body_speed * body_speed);
    let force = world_velocity.map(|v| (v * inverse_length) * magnitude);
    if !force.iter().all(|x| x.is_finite()) {
        return Err("material drag overflow".into());
    }
    Ok(force)
}

/// Gravity cancellation on the contact plane (001d8540's all-contact branch).
/// `normal` is the original aggregated contact normal; `forward` is the world
/// forward basis returned by00165420. Neither is silently renormalized.
#[derive(Debug, Clone, Copy)]
pub struct StictionInput {
    pub all_contacts: bool,
    pub mass: f32,
    pub normal: Vec3,
    pub forward: Vec3,
    pub local_forward_speed: f32,
    pub brake: f32,
    pub stiction_speed: f32,
}
pub fn gravity_stiction(input: StictionInput) -> Result<Vec3> {
    if input
        .normal
        .iter()
        .chain(input.forward.iter())
        .chain(
            [
                input.mass,
                input.local_forward_speed,
                input.brake,
                input.stiction_speed,
            ]
            .iter(),
        )
        .any(|v| !v.is_finite())
    {
        return Err("nonfinite gravity stiction input".into());
    }
    if !input.all_contacts {
        return Ok([0.; 3]);
    }
    let weight = f32::from_bits(0x4200cccd) * input.mass;
    let up_force = [weight * 0.0, weight * 1.0, weight * 0.0];
    let normal_projection = (input.normal[0] * up_force[0] + input.normal[1] * up_force[1])
        + input.normal[2] * up_force[2];
    let tangent_force =
        std::array::from_fn::<_, 3, _>(|i| up_force[i] - normal_projection * input.normal[i]);
    let forward_projection = (input.forward[0] * input.normal[0]
        + input.forward[1] * input.normal[1])
        + input.forward[2] * input.normal[2];
    let tangent_forward =
        std::array::from_fn::<_, 3, _>(|i| input.forward[i] - forward_projection * input.normal[i]);
    let length_squared = (tangent_forward[0] * tangent_forward[0]
        + tangent_forward[1] * tangent_forward[1])
        + tangent_forward[2] * tangent_forward[2];
    let tangent_forward = if length_squared == 0.0 {
        [0.; 3]
    } else {
        let inv = 1.0 / length_squared.sqrt();
        tangent_forward.map(|v| v * inv)
    };
    let projection = (tangent_forward[0] * tangent_force[0]
        + tangent_forward[1] * tangent_force[1])
        + tangent_forward[2] * tangent_force[2];
    let along = tangent_forward.map(|v| projection * v);
    #[allow(clippy::neg_multiply)] // Preserve original MUL.S stage.
    let fade = (1.0 + ((input.local_forward_speed - 0.0) * -1.0) / 20.0).clamp(0.0, 1.0);
    let mut force = std::array::from_fn(|i| 0.0 + fade * (tangent_force[i] - along[i]));
    if input.local_forward_speed.abs() < input.stiction_speed && input.brake != 0.0 {
        for i in 0..3 {
            force[i] += along[i];
        }
    }
    Ok(force)
}

/// The final local-velocity settling stage. Returns None when the original
/// routine would not rewrite velocity, preserving caller transform roundoff.
pub fn settle_local_velocity(mut velocity: Vec3, braking: bool) -> Result<Option<Vec3>> {
    if !velocity.iter().all(|x| x.is_finite()) {
        return Err("non-finite settling velocity".into());
    }
    let scale = f32::from_bits(0x3dcc_cccd);
    let mut changed = false;
    if velocity[0].abs() < scale {
        velocity[0] *= scale;
        changed = true;
    }
    if braking && velocity[2].abs() < 0.5 {
        velocity[2] *= scale;
        changed = true;
    }
    Ok(changed.then_some(velocity))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> LateralInput {
        LateralInput {
            any_contact: true,
            front_contact: true,
            front_point_velocity: [2., 0., 8.],
            rear_point_velocity: [-1., 0., 8.],
            steering_sine: 0.,
            steering_cosine: 1.,
            tire_friction: 2.,
            steering_reduction: 0.25,
            material_grip: 0.5,
            material_powerslide: 0.4,
            throttle: 0.,
        }
    }
    #[test]
    fn recovered_contact_steering_and_powerslide_branches() {
        assert_eq!(lateral_forces(input()).unwrap(), [-90., 45.]);
        let mut i = input();
        i.front_contact = false;
        i.throttle = 1.;
        assert_eq!(lateral_forces(i).unwrap(), [-22.5, 18.]);
        i.steering_sine = 1.;
        i.steering_cosine = 0.;
        assert_eq!(lateral_forces(i).unwrap(), [90., 18.]);
        i.any_contact = false;
        assert_eq!(lateral_forces(i).unwrap(), [0., 0.]);
    }
    #[test]
    fn virtual_points_apply_yaw_and_roll_about_center_of_gravity() {
        let (f, m) = lateral_force_and_moment([-90., 45.], [0., 0.1, 0.]).unwrap();
        assert_eq!(f, [-45., 0., 0.]);
        assert_eq!(m, [0., -405., -4.5]);
    }
    #[test]
    fn slope_cancellation_respects_braking_and_contact_gate() {
        let input = StictionInput {
            all_contacts: true,
            mass: 10.,
            normal: [0., 0.8, 0.6],
            forward: [0., 0., 1.],
            local_forward_speed: 0.,
            brake: 0.,
            stiction_speed: 5.,
        };
        let rolling = gravity_stiction(input).unwrap();
        assert!(rolling.iter().all(|v| v.abs() < 0.0001));
        let braking = gravity_stiction(StictionInput { brake: 1., ..input }).unwrap();
        assert!(braking[1] > 100. && braking[2] < -100.);
        assert_eq!(
            gravity_stiction(StictionInput {
                all_contacts: false,
                ..input
            })
            .unwrap(),
            [0.; 3]
        );
    }
    #[test]
    fn drag_direction_and_strict_settling_thresholds() {
        assert_eq!(material_drag([0., 0., 2.], 3., 2.).unwrap(), [0., 0., -18.]);
        assert_eq!(material_drag([0.; 3], 0., 2.).unwrap(), [0.; 3]);
        assert!(
            settle_local_velocity([0.1, 0., 0.5], true)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            settle_local_velocity([0.05, 2., 0.4], true)
                .unwrap()
                .unwrap(),
            [0.0050000004, 2., 0.040000003]
        );
        assert!(
            lateral_forces(LateralInput {
                throttle: f32::NAN,
                ..input()
            })
            .is_err()
        );
    }
}
