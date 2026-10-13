//! National AI direction correction, SCUS_972.11 00252080 / 0024f840.
//! Peer equations and ray attenuation are recovered; scene eligibility/order
//! and the optional aggression branch remain separate integration gates.
use crate::{Result, collision::CollisionRay, scene_collision::SceneCollision, wheel::Vec3};

#[derive(Clone, Copy)]
pub struct Peer {
    pub origin: Vec3,
    pub velocity: Vec3,
}
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct Response {
    pub velocity: Vec3,
    pub peers: usize,
    pub obstacle_hits: usize,
    pub vegetation: usize,
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(a: Vec3, s: f32) -> Vec3 {
    a.map(|n| n * s)
}
// Matches the finite-binary32 reciprocal-square-root model used by the private
// instruction probe. This does not claim exact PS2 VU hardware rounding.
fn normalized(a: Vec3) -> Vec3 {
    let squared = dot(a, a);
    if squared == 0. {
        return scale(a, f32::MAX);
    }
    scale(a, (1. / f64::from(squared).sqrt()) as f32)
}

/// Original ordinary peer loop. The caller excludes the actor itself and
/// supplies other race actors in a stable order, including wrecked riders.
pub fn peers(origin: Vec3, velocity: Vec3, peers: &[Peer], lane_flags: u32) -> Response {
    peers_with_forward(origin, velocity, normalized(velocity), peers, lane_flags)
}
fn peers_with_forward(
    origin: Vec3,
    mut velocity: Vec3,
    forward: Vec3,
    peers: &[Peer],
    lane_flags: u32,
) -> Response {
    let mut encounters = 0;
    for peer in peers {
        let delta = sub(peer.origin, origin);
        let squared = dot(delta, delta);
        let mut normal = normalized(delta);
        let facing = dot(normal, forward);
        if facing > f32::from_bits(0x3f34fdf4) {
            normal = normalized(sub(normal, scale(forward, facing * 0.01)));
        }
        if squared < 144. {
            encounters += 1;
            let closing = dot(sub(velocity, peer.velocity), normal);
            if closing > 0. {
                let projected = scale(normal, closing);
                velocity = sub(velocity, scale(projected, f32::from_bits(0x3f266666)));
            }
            velocity = sub(velocity, scale(normal, 1.));
            if lane_flags & 0x10 != 0 {
                velocity = scale(velocity, 0.8);
            }
        } else if squared <= 10000. {
            encounters += 1;
            let closing = dot(sub(velocity, peer.velocity), normal);
            if closing > 0. {
                let projected = scale(normal, closing);
                let weighted = scale(projected, 100.);
                velocity = sub(velocity, scale(weighted, 1. / (squared + 1.)));
            }
        }
    }
    Response {
        velocity,
        peers: encounters,
        obstacle_hits: 0,
        vegetation: 0,
    }
}

/// Ordinary plant response00252a3c–00252e10. Packed X/Z positions are
/// horizontal obstacles; the plant-specific near/far gains are1 and200.
/// Unlike peer actors, vegetation has no relative moving velocity.
pub fn vegetation(
    origin: Vec3,
    velocity: Vec3,
    forward: Vec3,
    position: [f32; 2],
    lane_flags: u32,
) -> Response {
    let delta = [position[0] - origin[0], 0., position[1] - origin[2]];
    let squared = dot(delta, delta);
    let mut normal = normalized(delta);
    let facing = dot(normal, forward);
    if facing > f32::from_bits(0x3f34fdf4) {
        normal = normalized(sub(normal, scale(forward, facing * 0.01)));
    }
    let mut corrected = velocity;
    let mut encountered = 0;
    if squared < 144. {
        encountered = 1;
        let closing = dot(corrected, normal);
        if closing > 0. {
            corrected = sub(corrected, scale(scale(normal, closing), 1.));
        }
        corrected = sub(corrected, scale(normal, 1.));
        if lane_flags & 0x10 != 0 {
            corrected = scale(corrected, 0.8);
        }
    } else if squared <= 10000. {
        encountered = 1;
        let closing = dot(corrected, normal);
        if closing > 0. {
            corrected = sub(
                corrected,
                scale(scale(scale(normal, closing), 200.), 1. / (squared + 1.)),
            );
        }
    }
    Response {
        velocity: corrected,
        vegetation: encountered,
        ..Response::default()
    }
}

/// Original 100ft ray hit weighting; applied successively to the requested
/// velocity before 0024f840 normalizes the horizontal steering direction.
pub fn attenuate(velocity: Vec3, direction: Vec3, distance: f32) -> Vec3 {
    let weight = ((100. - distance) * 0.01).max(0.);
    let projected = scale(direction, dot(velocity, direction));
    sub(velocity, scale(projected, weight))
}

pub fn direction(
    origin: Vec3,
    velocity: Vec3,
    peers_input: &[Peer],
    lane_flags: u32,
    scene: Option<&SceneCollision>,
) -> Result<Response> {
    let forward = normalized(velocity);
    let mut corrected = velocity;
    let mut hits = 0;
    let mut plants = 0;
    if let Some(scene) = scene {
        let nodes = scene.direct_query(
            crate::scene_partition::Bounds {
                center: origin,
                half: [50.; 3],
            },
            false,
            |_, _| true,
        )?;
        // The native fan uses eight midpoint samples over pi/4, with its
        // origin raised one foot. Rust trig is an integration approximation.
        let directions: [Vec3; 8] = std::array::from_fn(|i| {
            let angle = (((i as f32 - 4.) + 0.5) * std::f32::consts::FRAC_PI_4) / 8.;
            let (sin, cos) = angle.sin_cos();
            normalized([
                forward[2] * sin + forward[0] * cos,
                forward[1],
                forward[2] * cos - forward[0] * sin,
            ])
        });
        let start = [origin[0], origin[1] + 1., origin[2]];
        let rays = directions.map(|d| CollisionRay {
            start,
            end: std::array::from_fn(|i| start[i] + d[i] * 100.),
        });
        for node in nodes {
            let object = match node {
                crate::scene_collision::SceneNode::StaticObstacle(i) => &scene.obstacles()[i],
                crate::scene_collision::SceneNode::DynamicObstacle(i) => {
                    &scene.dynamic()[i].obstacle
                }
                crate::scene_collision::SceneNode::Vegetation(i) => {
                    let plant = &scene.vegetation()[i];
                    let response = vegetation(
                        origin,
                        corrected,
                        forward,
                        [
                            plant.coordinates[0] as f32 * scene.coordinate_unit(),
                            plant.coordinates[2] as f32 * scene.coordinate_unit(),
                        ],
                        lane_flags,
                    );
                    corrected = response.velocity;
                    plants += response.vegetation;
                    continue;
                }
            };
            for hit in crate::wreck::query_static_segments(&rays, std::slice::from_ref(object), 8)?
            {
                let direction = directions[hit.ray_index];
                let distance = dot(sub(hit.world_point, origin), direction);
                corrected = attenuate(corrected, direction, distance);
                hits += 1;
            }
        }
    }
    let mut response = peers_with_forward(origin, corrected, forward, peers_input, lane_flags);
    response.obstacle_hits = hits;
    response.vegetation = plants;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn horizontal_vegetation_ignores_height_and_uses_its_stronger_far_gain() {
        let r = vegetation([0., 100., 0.], [0., 5., 50.], [0., 0., 1.], [3., 8.], 0);
        assert_eq!(r.vegetation, 1);
        assert!(r.velocity[0] < -10.);
        assert_eq!(r.velocity[1], 5.);
        let far = vegetation([0.; 3], [0., 0., 50.], [0., 0., 1.], [0., 100.], 0);
        assert!(
            far.velocity[2]
                < peers(
                    [0.; 3],
                    [0., 0., 50.],
                    &[Peer {
                        origin: [0., 0., 100.],
                        velocity: [0.; 3]
                    }],
                    0
                )
                .velocity[2]
        );
    }
    #[test]
    fn eight_ray_scene_fan_responds_to_a_real_triangle_on_the_right() {
        let wall = crate::wreck::tests::surface([[0., -10., 20.], [0., 10., 20.], [20., 0., 20.]]);
        let scene = SceneCollision::new(
            crate::scene_partition::Config {
                min_x: -128.,
                max_x: 128.,
                min_z: -128.,
                max_z: 128.,
                cell_size: 32.,
            },
            vec![wall],
            None,
            false,
            |_| Ok(0.),
        )
        .unwrap();
        let result = direction([0.; 3], [0., 0., 50.], &[], 0, Some(&scene)).unwrap();
        assert!(result.obstacle_hits > 0);
        assert!(
            result.velocity[0] < 0.,
            "obstacle on right deflects heading left"
        );
        assert!(result.velocity[2] < 50.);
        let away = direction([0.; 3], [0., 0., -50.], &[], 0, Some(&scene)).unwrap();
        assert_eq!(away.obstacle_hits, 0);
        assert_eq!(away.velocity, [0., 0., -50.]);
    }
    #[test]
    fn nearby_peer_velocity_and_source_distance_boundaries() {
        let peer = |z, speed| Peer {
            origin: [0., 0., z],
            velocity: [0., 0., speed],
        };
        let near = peers([0.; 3], [0., 0., 50.], &[peer(8., 0.)], 0);
        assert_eq!(near.peers, 1);
        assert!((near.velocity[2] - 16.5).abs() < 0.0001);
        let parallel = peers([0.; 3], [0., 0., 50.], &[peer(8., 50.)], 0);
        assert!((parallel.velocity[2] - 49.).abs() < 0.0001);
        assert_eq!(peers([0.; 3], [0., 0., 50.], &[peer(100., 0.)], 0).peers, 1);
        assert_eq!(
            peers([0.; 3], [0., 0., 50.], &[peer(100.01, 0.)], 0).peers,
            0
        );
        let left = peers(
            [0.; 3],
            [0., 0., 50.],
            &[Peer {
                origin: [3., 0., 8.],
                velocity: [0.; 3],
            }],
            0,
        );
        assert!(left.velocity[0] < -10.);
    }
    #[test]
    fn source_ray_hit_weight_steers_away_from_obstruction() {
        assert!((attenuate([0., 0., 50.], [0., 0., 1.], 20.)[2] - 10.).abs() < 0.00001);
        assert_eq!(attenuate([0., 0., 50.], [0., 0., 1.], 100.), [0., 0., 50.]);
        let r = attenuate([0., 0., 50.], [0.6, 0., 0.8], 20.);
        assert!(r[0] < 0. && r[2] < 50.);
    }
}
