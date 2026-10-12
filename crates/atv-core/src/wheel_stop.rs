//! Terrain suspension travel stops: ATV actor +184/+188 in 001e1fd0.
//! This actor clips inward linear velocity and force (00196cf0); it applies no
//! positional correction, angular impulse, reflection, or friction.
use crate::{
    Result,
    dynamics::BodyState,
    obstacle::transform_point,
    terrain::Terrain,
    wheel::{Vec3, add, dot, scale, sub},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}

/// Constructor stop point: wheel+60 + UP*(wheel+1bc-wheel+20c), followed
/// by the local translation of the model's body node (001e1fd0).
pub fn local_point(rest: Vec3, travel: f32, radius: f32, body_translation: Vec3) -> Vec3 {
    let offset = travel - radius;
    add(add(rest, scale([0., 1., 0.], offset)), body_translation)
}

/// Query all four terrain planes once before substeps (00193b40). Actor +188
/// has no obstacle collider: its 00193830 arguments 4 and 5 are both zero.
/// A missing terrain tile leaves the sampled world point unchanged and the
/// normal upright in 0016ac10, giving zero penetration in the first substep.
pub fn sample(terrain: &Terrain, world: &[f32; 16], local_points: [Vec3; 4]) -> Result<[Plane; 4]> {
    if world
        .iter()
        .chain(local_points.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err("nonfinite wheel stop query".into());
    }
    let mut planes = [Plane {
        point: [0.; 3],
        normal: [0., 1., 0.],
    }; 4];
    for (plane, local) in planes.iter_mut().zip(local_points) {
        let world_point = transform_point(world, local);
        *plane = if let Some(hit) = terrain.contact(world_point[0], world_point[2])? {
            Plane {
                point: [world_point[0], hit.height, world_point[2]],
                normal: hit.normal,
            }
        } else {
            Plane {
                point: world_point,
                normal: [0., 1., 0.],
            }
        };
    }
    Ok(planes)
}

/// 00196cf0, exposed separately for original-instruction comparison. The native
/// routine uses center linear velocity, ignoring the contact's angular velocity.
pub fn project_inward(body: &mut BodyState, normal: Vec3, impact_delta: &mut Vec3) {
    let delta = scale(normal, -dot(body.linear_velocity, normal).min(0.));
    *impact_delta = add(*impact_delta, delta);
    body.linear_velocity = add(body.linear_velocity, delta);
    body.force = add(body.force, scale(normal, -dot(body.force, normal).min(0.)));
}

/// 00194930's terrain branch with stop-actor flags (+78=1, +4c/+58/+50=0).
/// Iterate front-left, front-right, rear-left, rear-right before wheel springs.
/// The plane is frozen for the outer frame, the stop point moves every substep.
pub fn resolve(
    body: &mut BodyState,
    plane: Plane,
    world_point: Vec3,
    impact_delta: &mut Vec3,
) -> Result<bool> {
    if plane
        .point
        .iter()
        .chain(plane.normal.iter())
        .chain(world_point.iter())
        .chain(body.linear_velocity.iter())
        .chain(body.force.iter())
        .chain(impact_delta.iter())
        .any(|v| !v.is_finite())
    {
        return Err("nonfinite wheel stop contact".into());
    }
    if dot(sub(plane.point, world_point), plane.normal) <= 0. {
        return Ok(false);
    }
    project_inward(body, plane.normal, impact_delta);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamics::BodyConfig;
    #[test]
    fn stop_preserves_tangent_angular_velocity_and_position() {
        let config = BodyConfig::new(100., [50.; 3], [0.; 3]).unwrap();
        let mut body = BodyState::new([1., 2., 3.], [1., 0., 0., 0.], &config).unwrap();
        body.linear_velocity = [3., -20., 4.];
        body.local_angular_velocity = [1., 2., 3.];
        body.force = [7., -1000., 9.];
        let mut impact = [0.; 3];
        assert!(
            resolve(
                &mut body,
                Plane {
                    point: [0.; 3],
                    normal: [0., 1., 0.]
                },
                [1., -0.1, 3.],
                &mut impact
            )
            .unwrap()
        );
        assert_eq!(body.linear_velocity, [3., 0., 4.]);
        assert_eq!(body.force, [7., 0., 9.]);
        assert_eq!(body.local_angular_velocity, [1., 2., 3.]);
        assert_eq!(body.center, [1., 2., 3.]);
        assert_eq!(impact, [0., 20., 0.]);
    }
    #[test]
    fn boundary_is_strict_and_stop_uses_full_compression() {
        assert_eq!(
            local_point([2., -1., 3.], 1., 1.25, [0., 2., 0.]),
            [2., 0.75, 3.]
        );
        let config = BodyConfig::new(100., [50.; 3], [0.; 3]).unwrap();
        let mut body = BodyState::new([0.; 3], [1., 0., 0., 0.], &config).unwrap();
        body.linear_velocity = [0., -10., 0.];
        let mut impact = [0.; 3];
        assert!(
            !resolve(
                &mut body,
                Plane {
                    point: [0.; 3],
                    normal: [0., 1., 0.]
                },
                [0.; 3],
                &mut impact
            )
            .unwrap()
        );
        assert_eq!(body.linear_velocity, [0., -10., 0.]);
    }
}
