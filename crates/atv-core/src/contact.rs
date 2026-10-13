//! Original static-contact response (00195b40 / 00195890 / 00146250).
//! Moving-body contacts, wreck flags, and the query that creates obstacle contacts
//! are separate systems. This implements the normal riding solver configuration.
use crate::{
    Result,
    dynamics::{Basis, BodyConfig, BodyState, inverse_transform},
    physics_data::PhysicsData,
    wheel::{Vec3, add, cross, dot, scale, sub},
};

#[derive(Debug, Clone, Copy)]
pub struct ContactConfig {
    pub reflection: f32,
    pub angular_impulse_scale: f32,
    pub friction_scale: f32,
    pub friction_torque_scale: f32,
}
impl ContactConfig {
    pub fn from_physics(data: &PhysicsData) -> Result<Self> {
        Ok(Self {
            reflection: data.first_scalar("Collisions", "ObstacleReflection")?,
            angular_impulse_scale: data.first_scalar("Collisions", "DefaultAngularImpulseScale")?,
            friction_scale: data.first_scalar("Collisions", "DefaultFriction")?,
            friction_torque_scale: data.first_scalar("Collisions", "DefaultFrictionTorqueScale")?,
        })
    }
}
/// A contact already produced by the original terrain/obstacle query. The point
/// is the projected surface point, not the penetrating chassis vertex.
#[derive(Debug, Clone, Copy)]
pub struct StaticContact {
    pub point: Vec3,
    pub normal: Vec3,
    pub penetration: f32,
    pub material_grip: f32,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct ContactCorrections {
    /// Presentation-only strongest inward point speed at a resolved contact.
    /// Never participates in native response, wreck classification or recovery.
    pub closing_speed: f32,
    /// Applied after the body integrator, once per substep.
    pub translation: Vec3,
    /// Original body+0x180 impact accumulator, used by later wreck detection.
    pub velocity_delta: Vec3,
}

// The inline VU inverse transform sums Z,X,Y for its third component.
fn reciprocal_sqrt(v: f32) -> f32 {
    // One rounded reciprocal-square-root operation; EE hardware edge cases remain unverified.
    (1.0 / f64::from(v).sqrt()) as f32
}
fn inline_local(b: Basis, v: Vec3) -> Vec3 {
    [
        v[0] * b[0][0] + v[1] * b[0][1] + v[2] * b[0][2],
        v[1] * b[1][1] + v[0] * b[1][0] + v[2] * b[1][2],
        v[2] * b[2][2] + v[0] * b[2][0] + v[1] * b[2][1],
    ]
}
fn inverse_inertia(config: &BodyConfig, v: Vec3) -> Vec3 {
    // Original matrix product, including its zero off-diagonal terms.
    [
        ((1.0 / config.inertia[0]) * v[0] + 0.0 * v[1]) + 0.0 * v[2],
        (0.0 * v[0] + (1.0 / config.inertia[1]) * v[1]) + 0.0 * v[2],
        (0.0 * v[0] + 0.0 * v[1]) + (1.0 / config.inertia[2]) * v[2],
    ]
}
/// Merge positional corrections as in00146250: preserve perpendicular portions
/// and take the larger parallel correction, instead of summing every overlap.
pub fn merge_translation(accumulated: &mut Vec3, incoming: Vec3) {
    let squared = dot(*accumulated, *accumulated);
    let direction = if squared == 0.0 {
        [0.; 3]
    } else {
        scale(*accumulated, reciprocal_sqrt(squared))
    };
    let along = dot(incoming, direction);
    let magnitude = dot(*accumulated, *accumulated).sqrt();
    let extra = (along - magnitude).max(0.0);
    for i in 0..3 {
        accumulated[i] += (incoming[i] - along * direction[i]) + extra * direction[i];
    }
}
fn apply_force(
    body: &mut BodyState,
    config: &BodyConfig,
    force: Vec3,
    point: Vec3,
    torque_scale: f32,
) {
    body.force = add(body.force, force);
    if point != [0.; 3] {
        let b = body.basis();
        let local_force = inline_local(b, force);
        let local_point = inline_local(b, sub(point, body.origin(config)));
        let torque = cross(sub(local_point, config.center_of_mass), local_force);
        body.local_torque = add(body.local_torque, scale(torque, torque_scale));
    }
}
/// Apply one static terrain/obstacle contact. Contact iteration order is retained
/// by the caller because each impact updates the next contact's point velocity.
pub fn resolve_static(
    body: &mut BodyState,
    body_config: &BodyConfig,
    config: ContactConfig,
    contact: StaticContact,
    corrections: &mut ContactCorrections,
) -> Result<()> {
    resolve_static_with_position_scale(body, body_config, config, contact, corrections, 0.3)
}
/// Same native response with the caller's actor-specific penetration correction
/// coefficient (normal ATV0.3; detached rider0.25).
pub fn resolve_static_with_position_scale(
    body: &mut BodyState,
    body_config: &BodyConfig,
    config: ContactConfig,
    contact: StaticContact,
    corrections: &mut ContactCorrections,
    position_scale: f32,
) -> Result<()> {
    let arm = inverse_transform(body.basis(), sub(contact.point, body.center));
    let rotational = cross(body.local_angular_velocity, arm);
    let velocity = add(
        crate::dynamics::transform(body.basis(), rotational),
        body.linear_velocity,
    );
    resolve_with_velocity_and_scale(
        body,
        body_config,
        config,
        contact,
        velocity,
        corrections,
        position_scale,
    )
}
/// Explicit pre-impact point velocity also allows independent instruction-oracle
/// verification without conflating the query/point-velocity routines.
pub fn resolve_static_with_velocity(
    body: &mut BodyState,
    body_config: &BodyConfig,
    config: ContactConfig,
    contact: StaticContact,
    velocity: Vec3,
    corrections: &mut ContactCorrections,
) -> Result<()> {
    resolve_static_with_velocity_and_position_scale(
        body,
        body_config,
        config,
        contact,
        velocity,
        corrections,
        0.3,
    )
}
/// Native response with both the supplied pre-impact point velocity and the
/// caller's correction coefficient (the secondary nonstatic ATV pass uses1.0).
pub fn resolve_static_with_velocity_and_position_scale(
    body: &mut BodyState,
    body_config: &BodyConfig,
    config: ContactConfig,
    contact: StaticContact,
    velocity: Vec3,
    corrections: &mut ContactCorrections,
    position_scale: f32,
) -> Result<()> {
    resolve_with_velocity_and_scale(
        body,
        body_config,
        config,
        contact,
        velocity,
        corrections,
        position_scale,
    )
}
fn resolve_with_velocity_and_scale(
    body: &mut BodyState,
    body_config: &BodyConfig,
    config: ContactConfig,
    contact: StaticContact,
    velocity: Vec3,
    corrections: &mut ContactCorrections,
    position_scale: f32,
) -> Result<()> {
    let StaticContact {
        point,
        normal,
        penetration,
        material_grip,
    } = contact;
    if !point
        .iter()
        .chain(normal.iter())
        .chain(velocity.iter())
        .chain(
            [
                penetration,
                position_scale,
                material_grip,
                config.reflection,
                config.angular_impulse_scale,
                config.friction_scale,
                config.friction_torque_scale,
            ]
            .iter(),
        )
        .all(|x| x.is_finite())
    {
        return Err("nonfinite static contact".into());
    }
    if penetration <= 0.0 {
        return Ok(());
    }
    let basis = body.basis();
    let arm = inline_local(basis, sub(point, body.center));
    let local_normal = inline_local(basis, normal);
    let rotational = inverse_inertia(body_config, cross(arm, local_normal));
    let inverse_mass = 1.0 / body_config.mass;
    let effective_mass =
        1.0 / ((0.0 + 0.0 + inverse_mass) + dot(local_normal, cross(rotational, arm)));
    let normal_velocity = dot(velocity, normal);
    corrections.closing_speed = corrections.closing_speed.max(-normal_velocity);
    if normal_velocity < 0.0 {
        let impulse = scale(
            normal,
            effective_mass * (-(config.reflection + 1.0) * normal_velocity),
        );
        let delta = scale(impulse, inverse_mass);
        body.linear_velocity = add(body.linear_velocity, delta);
        corrections.velocity_delta = add(corrections.velocity_delta, delta);
        let local_impulse = inverse_transform(basis, impulse);
        let angular = scale(
            inverse_inertia(body_config, cross(arm, local_impulse)),
            config.angular_impulse_scale,
        );
        body.local_angular_velocity = add(body.local_angular_velocity, angular);
    }
    let into_surface = dot(body.force, normal);
    if into_surface < 0.0 {
        apply_force(
            body,
            body_config,
            scale(normal, -0.95 * into_surface),
            point,
            0.0,
        );
    }
    let displacement = scale(normal, penetration * effective_mass);
    merge_translation(
        &mut corrections.translation,
        scale(scale(displacement, position_scale), inverse_mass),
    );
    let tangent = sub(velocity, scale(normal, normal_velocity));
    let squared = dot(tangent, tangent);
    let direction = if squared == 0.0 {
        [0.; 3]
    } else {
        scale(tangent, reciprocal_sqrt(squared))
    };
    let speed_squared = dot(tangent, tangent);
    let speed = if speed_squared == 0.0 {
        0.0
    } else {
        1.0 / reciprocal_sqrt(speed_squared)
    };
    let fade = (speed * 0.5).min(1.0);
    let force_magnitude = (body_config.mass * (fade * (material_grip * config.friction_scale)))
        * f32::from_bits(0x4200cccd);
    apply_force(
        body,
        body_config,
        scale(direction.map(|x| -x), force_magnitude),
        point,
        config.friction_torque_scale,
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_overlap_uses_largest_correction() {
        let mut d = [0.; 3];
        merge_translation(&mut d, [0., 2., 0.]);
        merge_translation(&mut d, [0., 1., 0.]);
        assert_eq!(d, [0., 2., 0.]);
        merge_translation(&mut d, [1., 3., 0.]);
        assert_eq!(d, [1., 3., 0.]);
    }
    #[test]
    fn crash_signal_requires_penetration_and_measures_inward_normal_speed() {
        let cfg = BodyConfig::new(10., [20.; 3], [0.; 3]).unwrap();
        let config = ContactConfig {
            reflection: 0.,
            angular_impulse_scale: 0.,
            friction_scale: 0.,
            friction_torque_scale: 0.,
        };
        for (normal, velocity, penetration, expected) in [
            ([0., 1., 0.], [90., -5., 0.], 1., 5.),  // fast soft landing
            ([0., 1., 0.], [90., 5., 0.], 1., 0.),   // leaving a contact
            ([0., 1., 0.], [0., -60., 0.], 0., 0.),  // airborne
            ([0., 1., 0.], [0., -60., 0.], 1., 60.), // hard ground impact
            ([-1., 0., 0.], [60., 0., 0.], 1., 60.), // obstacle impact
        ] {
            let mut body = BodyState::new([0.; 3], [1., 0., 0., 0.], &cfg).unwrap();
            body.linear_velocity = velocity;
            let mut corrections = ContactCorrections::default();
            resolve_static(
                &mut body,
                &cfg,
                config,
                StaticContact {
                    point: [0.; 3],
                    normal,
                    penetration,
                    material_grip: 1.,
                },
                &mut corrections,
            )
            .unwrap();
            assert_eq!(corrections.closing_speed, expected);
        }
    }
    #[test]
    fn static_contact_stops_inward_speed_and_defers_position() {
        let cfg = BodyConfig::new(10., [20.; 3], [0.; 3]).unwrap();
        let mut b = BodyState::new([0.; 3], [1., 0., 0., 0.], &cfg).unwrap();
        b.linear_velocity = [0., -10., 0.];
        b.force = [0., -322., 0.];
        let mut c = ContactCorrections::default();
        resolve_static(
            &mut b,
            &cfg,
            ContactConfig {
                reflection: 0.,
                angular_impulse_scale: 0.,
                friction_scale: 0.,
                friction_torque_scale: 0.,
            },
            StaticContact {
                point: [0.; 3],
                normal: [0., 1., 0.],
                penetration: 1.,
                material_grip: 1.,
            },
            &mut c,
        )
        .unwrap();
        assert_eq!(c.closing_speed, 10.);
        assert_eq!(b.linear_velocity, [0.; 3]);
        assert_eq!(b.center, [0.; 3]);
        assert!((c.translation[1] - 0.3).abs() < 1e-6);
        assert!((b.force[1] + 16.1).abs() < 1e-4);
    }
}
