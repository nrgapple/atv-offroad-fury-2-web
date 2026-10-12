//! USA v2.00 crash decisions (001e8690) and recovery clock (001e5260).
//! Impact input is the original local collision velocity delta, not body speed.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

/// Eight horizontal20-foot clearance rays from001ea4d0.
pub fn checkpoint_clearance_rays() -> crate::collision::CollisionRayTree {
    use crate::collision::{CollisionRay, CollisionRayTree, RayTreeNode};
    let identity = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let rays = (0..8)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 8.0;
            CollisionRay {
                start: [0., 3., 0.],
                end: [angle.sin() * 20., 3., angle.cos() * 20.],
            }
        })
        .collect();
    let nodes = (0..7)
        .map(|i| RayTreeNode {
            bound_measure: 800.,
            center: [0., 3., 0.],
            half_extents: [20., 0., 20.],
            children: if i < 3 {
                [(i * 2 + 1) as u16, (i * 2 + 2) as u16]
            } else {
                [0x8000 + ((i - 3) * 2) as u16, 0x8001 + ((i - 3) * 2) as u16]
            },
        })
        .collect();
    CollisionRayTree {
        transform: identity,
        query_capacity: 8,
        nodes,
        rays,
    }
}
/// Native ray0 starts ten feet above the candidate and reaches five feet below
/// it. 001ea050 raises the candidate to a higher platform hit; a horizontal hit
/// rejects it. Callers retain the terrain/wheel, water and scene-category gates.
pub fn checkpoint_static_support(
    origin: crate::wheel::Vec3,
    yaw: f32,
    obstacles: &[crate::obstacle::StaticObstacle],
) -> Result<Option<crate::wheel::Vec3>> {
    use crate::collision::CollisionRay;
    let mut rays = vec![CollisionRay {
        start: [0., 10., 0.],
        end: [0., -5., 0.],
    }];
    rays.extend(checkpoint_clearance_rays().rays);
    let (sin, cos) = yaw.sin_cos();
    let world = [
        cos, 0., -sin, 0., 0., 1., 0., 0., sin, 0., cos, 0., origin[0], origin[1], origin[2], 1.,
    ];
    let rays: Vec<_> = rays
        .into_iter()
        .map(|ray| CollisionRay {
            start: crate::obstacle::transform_point(&world, ray.start),
            end: crate::obstacle::transform_point(&world, ray.end),
        })
        .collect();
    let mut support = origin;
    for hit in query_static_segments(&rays, obstacles, usize::MAX)? {
        if hit.ray_index != 0 {
            return Ok(None);
        }
        support[1] = support[1].max(hit.world_point[1]);
    }
    Ok(Some(support))
}

/// Riding rider-height contact flag from 001dd830 after the final substep.
/// The original loop repeats the first static row instead of advancing it.
/// Caller must apply the recovery cooldown and hard-landing branch gates.
pub fn rider_height_contact(
    solver_any_contact: bool,
    first_static_endpoint: Option<crate::wheel::Vec3>,
) -> bool {
    solver_any_contact && first_static_endpoint.is_some_and(|point| point[1] > 4.)
}

/// Rider-height branch follows hard-landing actions and has its own cooldown.
/// Other impact, stunt and angle branches must remain active during cooldown.
pub fn rider_height_allowed(height_contact: bool, cooldown: f32, hard_landing: bool) -> bool {
    height_contact && cooldown <= 0. && !hard_landing
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SegmentContact {
    pub ray_index: usize,
    pub world_end: crate::wheel::Vec3,
    pub world_point: crate::wheel::Vec3,
    pub world_normal: crate::wheel::Vec3,
}

/// 00327810/00121440 geometry gate. Scene candidate selection separately
/// accepts touching spheres; the type-3 dispatcher requires strict overlap.
fn segment_spheres_overlap(a: crate::wheel::Vec3, ar: f32, b: crate::wheel::Vec3, br: f32) -> bool {
    let difference = crate::wheel::sub(a, b);
    crate::wheel::dot(difference, difference) < (ar + br) * (ar + br)
}

///00327360 bounds shared by plant admission and the type-3 geometry wrapper.
pub(crate) fn segment_query_sphere(
    rays: &[crate::collision::CollisionRay],
) -> (crate::wheel::Vec3, f32) {
    if rays.is_empty() {
        return ([0.; 3], 0.);
    }
    let mut low = [f32::MAX; 3];
    let mut high = [-f32::MAX; 3];
    for ray in rays {
        for point in [ray.start, ray.end] {
            for i in 0..3 {
                low[i] = low[i].min(point[i]);
                high[i] = high[i].max(point[i]);
            }
        }
    }
    let center = std::array::from_fn(|i| (high[i] + low[i]) * 0.5);
    let extent = std::array::from_fn(|i| (high[i] - low[i]) * 0.5);
    (center, crate::wheel::dot(extent, extent).sqrt())
}

/// Native type-3 box entry rule (001148c0), including X/Z/Y plane order.
/// Direction is transformed from the normalized world ray; retain its length.
fn segment_enters_box(
    center: crate::wheel::Vec3,
    half: crate::wheel::Vec3,
    start: crate::wheel::Vec3,
    direction: crate::wheel::Vec3,
    length: f32,
) -> bool {
    let delta = crate::wheel::sub(center, start);
    if (0..3).all(|i| delta[i].abs() <= half[i]) {
        return true;
    }
    for (axis, other) in [(0, [1, 2]), (2, [1, 0]), (1, [2, 0])] {
        let plane = if delta[axis] > 0. {
            -start[axis] - half[axis]
        } else {
            half[axis] - start[axis]
        };
        let distance = if direction[axis] == 0. {
            -1.
        } else {
            (center[axis] + plane) / direction[axis]
        };
        if distance >= 0.
            && distance <= length
            && other
                .into_iter()
                .all(|i| ((start[i] + distance * direction[i]) - center[i]).abs() <= half[i])
        {
            return true;
        }
    }
    false
}

/// Runtime type-3 segment array (00326bb0/003272e0), queried separately for
/// each static type-0 object (00121440), then appended in ray-index order
/// (00193060). Each object resets nearest-ray results. Unlike type-2 hulls,
/// these rays have no second tree and never carry prior-object hits forward.
/// Box pruning retains the native prefix mask: rays before the first box
/// entry are disabled, while later rays retain their incoming mask. The
/// optional smooth-normal branch remains unverified (default policy is flat).
pub(crate) fn query_static_segments(
    rays: &[crate::collision::CollisionRay],
    obstacles: &[crate::obstacle::StaticObstacle],
    capacity: usize,
) -> Result<Vec<SegmentContact>> {
    use crate::wheel::{add, dot, scale, sub};
    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    // 00327360 prepares world-space directions once. 00121440 transforms
    // each start and direction independently without renormalizing or changing
    // the world length; recomputing from two local endpoints changes rounding.
    let world: Vec<_> = rays
        .iter()
        .map(|ray| {
            let delta = sub(ray.end, ray.start);
            let length = dot(delta, delta).sqrt();
            let direction = if length > 0. {
                scale(delta, 1. / length)
            } else {
                [0.; 3]
            };
            (ray.start, direction, length)
        })
        .collect();
    if rays.is_empty() || capacity == 0 {
        return Ok(Vec::new());
    }
    let (world_center, radius) = segment_query_sphere(rays);
    let mut contacts = Vec::new();
    for object in obstacles {
        let root = object
            .mesh
            .nodes
            .first()
            .ok_or("missing static segment mesh root")?;
        let center = crate::obstacle::transform_point(&object.world_matrix, root.center);
        let object_radius = dot(root.half_extents, root.half_extents).sqrt();
        if !segment_spheres_overlap(world_center, radius, center, object_radius) {
            continue;
        }
        let to_mesh = crate::obstacle::relative_transform(&IDENTITY, &object.world_matrix);
        let local: Vec<_> = world
            .iter()
            .map(|&(world_start, world_direction, length)| {
                let start = crate::obstacle::transform_point(&to_mesh, world_start);
                let direction = std::array::from_fn(|i| {
                    (to_mesh[i] * world_direction[0] + to_mesh[4 + i] * world_direction[1])
                        + to_mesh[8 + i] * world_direction[2]
                });
                (start, direction, length)
            })
            .collect();
        let mut nearest: Vec<Option<crate::obstacle::RayHit>> = vec![None; rays.len()];
        let mut stack = vec![(0_u16, (0..rays.len()).collect::<Vec<_>>())];
        let mut visits = 0;
        while let Some((reference, active)) = stack.pop() {
            visits += 1;
            if visits > 2_000_000 {
                return Err("static segment tree traversal exceeded bound".into());
            }
            let index = usize::from(reference & 0x7fff);
            if reference & 0x8000 == 0 {
                let node = object
                    .mesh
                    .nodes
                    .get(index)
                    .ok_or("invalid static segment tree child")?;
                let local_center = crate::obstacle::transform_point(&to_mesh, world_center);
                if (0..3).any(|i| {
                    (node.center[i] - local_center[i]).abs() > node.half_extents[i] + radius
                }) {
                    continue;
                }
                // The native loop stops at its first entry, leaving the
                // subsequent ray mask untouched for both child traversals.
                let first_entry = active.iter().position(|&i| {
                    let (start, direction, length) = local[i];
                    segment_enters_box(node.center, node.half_extents, start, direction, length)
                });
                let active = first_entry.map_or_else(Vec::new, |first| active[first..].to_vec());
                if !active.is_empty() {
                    stack.push((node.children[1], active.clone()));
                    stack.push((node.children[0], active));
                }
            } else {
                let face = object
                    .mesh
                    .triangles
                    .get(index)
                    .ok_or("invalid static segment mesh leaf")?;
                if face.invalid_plane_bits.is_some() {
                    continue;
                }
                let vertices = face.indices.map(|i| object.mesh.vertices[usize::from(i)]);
                for i in active {
                    let (start, direction, length) = local[i];
                    if let Some(hit) =
                        crate::obstacle::triangle_hit(start, direction, length, vertices)
                        && nearest[i].is_none_or(|old| hit.fraction < old.fraction)
                    {
                        nearest[i] = Some(hit);
                    }
                }
            }
        }
        for (ray_index, hit) in nearest.into_iter().enumerate() {
            if let Some(hit) = hit {
                if contacts.len() == capacity {
                    return Ok(contacts);
                }
                contacts.push(SegmentContact {
                    ray_index,
                    world_end: add(
                        world[ray_index].0,
                        scale(world[ray_index].1, world[ray_index].2),
                    ),
                    world_point: crate::obstacle::transform_point(&object.world_matrix, hit.point),
                    world_normal: std::array::from_fn(|i| {
                        (object.world_matrix[i] * hit.normal[0]
                            + object.world_matrix[4 + i] * hit.normal[1])
                            + object.world_matrix[8 + i] * hit.normal[2]
                    }),
                });
            }
        }
    }
    Ok(contacts)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum Kind {
    Front = 0,
    Left = 1,
    Right = 2,
    Back = 3,
    Vertical = 4,
}
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub impact_limit: [f32; 3],
    pub ground_angles: [f32; 3],
    pub air_angles: [f32; 3],
}
impl Config {
    /// Profile upgrades are supplied explicitly; zero is the base ATV tuning.
    pub fn from_physics(data: &PhysicsData, stability_upgrade: f32) -> Result<Self> {
        let get = |name| data.first_scalar("Wrecks", name);
        let radians = |name| get(name).map(|v| v * 0.017_453_29);
        if !stability_upgrade.is_finite() || stability_upgrade < 0.0 {
            return Err("invalid stability upgrade".into());
        }
        Ok(Self {
            impact_limit: [
                get("LateralFpsLimit")?,
                get("VerticalFpsLimit")? + get("TweakVerticalFpsLimit")? * stability_upgrade,
                get("LongitudinalFpsLimit")?,
            ],
            ground_angles: [
                radians("PitchUpLimit")?,
                radians("PitchDownLimit")?,
                radians("RollLimit")?,
            ],
            air_angles: [
                radians("AirPitchUpLimit")?,
                radians("AirPitchDownLimit")?,
                radians("AirRollLimit")?,
            ],
        })
    }
    pub fn evaluate(self, input: Input) -> Option<Kind> {
        use Kind::*;
        let [x, y, z] = input.local_impact;
        let [lateral, vertical, longitudinal] = self.impact_limit;
        // Preserve native branch precedence when multiple limits are crossed.
        let impact = if z > longitudinal {
            Some(Back)
        } else if z < -longitudinal {
            Some(Front)
        } else if x > lateral {
            Some(Left)
        } else if x < -lateral {
            Some(Right)
        } else if y > vertical * input.vertical_limit_scale {
            Some(Vertical)
        } else if input.rider_contact {
            Some(if input.forward_speed > 0.0 {
                Back
            } else {
                Front
            })
        } else {
            None
        };
        if input.wheel_contact && !input.stunt_landing_safe {
            return Some(Vertical);
        }
        let [up, down, roll] = if !input.wheel_contact && !input.body_contact {
            self.air_angles
        } else {
            self.ground_angles
        };
        let angle = if input.pitch > up {
            Some(Back)
        } else if input.pitch < -down {
            Some(Front)
        } else if input.roll > roll {
            Some(Left)
        } else if input.roll < -roll {
            Some(Right)
        } else {
            None
        };
        // Native calls the entry routine again if an angle also crosses a limit;
        // that later decision is the last visible selected wreck type.
        angle.or(impact)
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Input {
    pub local_impact: [f32; 3],
    /// 1.25 for original modes0/1/2; 1.0 otherwise. Caller owns game-mode mapping.
    pub vertical_limit_scale: f32,
    pub pitch: f32,
    pub roll: f32,
    pub wheel_contact: bool,
    pub body_contact: bool,
    pub rider_contact: bool,
    pub forward_speed: f32,
    pub stunt_landing_safe: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Phase {
    Riding,
    Falling,
    Recovering,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct State {
    pub phase: Phase,
    pub kind: Option<Kind>,
    pub timer: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            phase: Phase::Riding,
            kind: None,
            timer: 0.0,
        }
    }
}
impl State {
    pub fn begin(&mut self, kind: Kind) {
        self.phase = Phase::Falling;
        self.kind = Some(kind);
        self.timer = 0.0;
    }
    /// Constructor +0x7f8 is1.8s. Recovery waits for the wrecked rider's contact,
    /// not for the ATV wheels. Returns true when the native reset is requested.
    pub fn step(&mut self, dt: f32, rider_grounded: bool) -> bool {
        let delay = f32::from_bits(0x3fe66666);
        match self.phase {
            Phase::Riding => false,
            Phase::Falling => {
                self.timer += dt;
                if self.timer > delay - 1.0 {
                    self.phase = Phase::Recovering;
                    self.timer = 0.0;
                }
                false
            }
            Phase::Recovering => {
                if self.timer != 0.0 || rider_grounded {
                    self.timer += dt;
                }
                self.timer > delay
            }
        }
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn surface(vertices: [[f32; 3]; 3]) -> crate::obstacle::StaticObstacle {
        use crate::{
            collision::{CollisionMesh, RayTreeNode, Triangle},
            wheel::{cross, dot, scale, sub},
        };
        let normal = cross(sub(vertices[1], vertices[0]), sub(vertices[2], vertices[0]));
        let normal = scale(normal, 1. / dot(normal, normal).sqrt());
        let low: [f32; 3] =
            std::array::from_fn(|i| vertices.iter().map(|v| v[i]).fold(f32::MAX, f32::min));
        let high: [f32; 3] =
            std::array::from_fn(|i| vertices.iter().map(|v| v[i]).fold(-f32::MAX, f32::max));
        let half_extents = std::array::from_fn(|i| (high[i] - low[i]) * 0.5);
        crate::obstacle::StaticObstacle {
            mesh: std::rc::Rc::new(CollisionMesh {
                transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
                vertices: vertices.to_vec(),
                vertex_normal_bits: None,
                triangles: vec![Triangle {
                    indices: [0, 1, 2],
                    normal,
                    distance: -dot(normal, vertices[0]),
                    invalid_plane_bits: None,
                }],
                broad_phase_nodes: 1,
                nodes: vec![RayTreeNode {
                    center: std::array::from_fn(|i| (high[i] + low[i]) * 0.5),
                    half_extents,
                    bound_measure: dot(half_extents, half_extents),
                    children: [0x8000, 0x8000],
                }],
                flags: 0,
            }),
            world_matrix: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
        }
    }
    #[test]
    fn recovery_platform_raises_height_but_never_lowers_or_extends_ray() {
        let platform = |y| surface([[-100., y, -100.], [0., y, 100.], [100., y, -100.]]);
        // Above horizontal rays; the native candidate becomes the platform Y.
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[platform(5.)]).unwrap(),
            Some([0., 5., 0.])
        );
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[platform(-2.)]).unwrap(),
            Some([0.; 3])
        );
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[platform(-6.)]).unwrap(),
            Some([0.; 3])
        );
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[platform(11.)]).unwrap(),
            Some([0.; 3])
        );
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[platform(5.), platform(8.)]).unwrap(),
            Some([0., 8., 0.])
        );
    }
    #[test]
    fn recovery_rejects_side_obstruction_with_transformed_scene_placement() {
        let mut wall = surface([[-30., -20., 10.], [0., 20., 10.], [30., -20., 10.]]);
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[wall.clone()]).unwrap(),
            None
        );
        wall.world_matrix[12] = 100.;
        assert_eq!(
            checkpoint_static_support([0.; 3], 0., &[wall.clone()]).unwrap(),
            Some([0.; 3])
        );
        assert_eq!(
            checkpoint_static_support([100., 0., 0.], 1.2, &[wall]).unwrap(),
            None
        );
    }
    #[test]
    fn runtime_segments_reset_per_object_and_append_in_source_ray_order() {
        use crate::collision::CollisionRay;
        let rays = [
            CollisionRay {
                start: [0.; 3],
                end: [0., 0., 6.],
            },
            CollisionRay {
                start: [10., 0., 0.],
                end: [10., 0., 6.],
            },
        ];
        let near = surface([[-2., -2., 2.], [0., 2., 2.], [2., -2., 2.]]);
        let far = surface([[-30., -30., 4.], [0., 30., 4.], [30., -30., 4.]]);
        let hits = query_static_segments(&rays, &[near.clone(), far.clone()], 60).unwrap();
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.ray_index, hit.world_point[2]))
                .collect::<Vec<_>>(),
            [(0, 2.), (0, 4.), (1, 4.)]
        );
        assert_eq!(
            query_static_segments(&rays, &[near, far], 2).unwrap().len(),
            2
        );
    }
    #[test]
    fn segment_geometry_strict_spheres_match_native_boundary_replay() {
        // Actual 00327810 and 00121440 instruction replays with radius-one
        // actors enter the mesh callee only below distance two. The callee was
        // explicitly hooked to no-hit to isolate this wrapper boundary.
        assert!(segment_spheres_overlap(
            [0.; 3],
            1.,
            [f32::from_bits(2_f32.to_bits() - 1), 0., 0.],
            1.
        ));
        assert!(!segment_spheres_overlap([0.; 3], 1., [2., 0., 0.], 1.));
        assert!(!segment_spheres_overlap(
            [0.; 3],
            1.,
            [f32::from_bits(2_f32.to_bits() + 1), 0., 0.],
            1.
        ));
        assert_eq!(
            crate::obstacle::sphere_separation([0.; 3], 1., [2., 0., 0.], 1.),
            0.
        );
        let ray = crate::collision::CollisionRay {
            start: [0., 0., 0.],
            end: [0., 0., 4.],
        };
        let object = surface([[-2., -2., 2.], [0., 2., 2.], [2., -2., 2.]]);
        let hits = query_static_segments(&[ray], &[object], 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].world_point, [0., 0., 2.]);
    }
    #[test]
    fn riding_rider_height_flag_uses_first_row_and_strict_threshold() {
        assert!(!rider_height_contact(false, Some([0., 5., 0.])));
        assert!(!rider_height_contact(true, None));
        assert!(!rider_height_contact(true, Some([0., 4., 0.])));
        assert!(rider_height_contact(
            true,
            Some([0., f32::from_bits(4_f32.to_bits() + 1), 0.])
        ));
    }
    #[test]
    fn rider_height_cooldown_and_hard_landing_do_not_disable_later_angle_or_stunt() {
        assert!(!rider_height_allowed(true, 0.01, false));
        assert!(!rider_height_allowed(true, 0., true));
        assert!(rider_height_allowed(true, -0.01, false));
        let mut event = input();
        event.rider_contact = rider_height_allowed(true, 2., false);
        event.roll = 1.31;
        assert_eq!(config().evaluate(event), Some(Kind::Left));
        event.roll = 0.;
        event.stunt_landing_safe = false;
        assert_eq!(config().evaluate(event), Some(Kind::Vertical));
    }
    #[test]
    fn transformed_oblique_segment_matches_original_instruction_checkpoint() {
        // Synthetic rotated mesh at track-sized coordinates. Captured by
        // executing 00327360→00121440→001148c0, rather than local-endpoint math.
        let mut platform = surface([[-5., 0., -5.], [0., 0., 5.], [5., 0., -5.]]);
        platform.world_matrix = [
            0.60340595, 0., -0.7974342, 0., 0., 1., 0., 0., 0.7974342, 0., 0.60340595, 0.,
            1584.7358, 46.722614, 1885.6891, 1.,
        ];
        let ray = crate::collision::CollisionRay {
            start: [1584.4838, 50.443714, 1882.3279],
            end: [1585.7899, 45.064724, 1885.9165],
        };
        let hits = query_static_segments(&[ray], &[platform], 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].world_point.map(f32::to_bits),
            [1153838183, 1111155701, 1156291055]
        );
        assert_eq!(hits[0].world_normal, [0., 1., 0.]);
    }
    fn config() -> Config {
        Config {
            impact_limit: [19.0, 84.0, 21.0],
            ground_angles: [1.4, 1.4, 1.3],
            air_angles: [1.5, 1.2, 1.4],
        }
    }
    fn input() -> Input {
        Input {
            local_impact: [0.; 3],
            vertical_limit_scale: 1.0,
            pitch: 0.,
            roll: 0.,
            wheel_contact: true,
            body_contact: false,
            rider_contact: false,
            forward_speed: 0.,
            stunt_landing_safe: true,
        }
    }
    #[test]
    fn normal_landing_is_safe_and_held_trick_is_not() {
        let mut i = input();
        assert_eq!(config().evaluate(i), None);
        i.stunt_landing_safe = false;
        assert_eq!(config().evaluate(i), Some(Kind::Vertical));
        i.wheel_contact = false;
        assert_eq!(config().evaluate(i), None);
    }
    #[test]
    fn strict_thresholds_and_native_branch_order() {
        let mut i = input();
        i.local_impact = [19., 84., 21.];
        assert_eq!(config().evaluate(i), None);
        i.local_impact = [20., 100., 22.];
        assert_eq!(config().evaluate(i), Some(Kind::Back));
        i.pitch = -1.41;
        assert_eq!(config().evaluate(i), Some(Kind::Front));
        i.stunt_landing_safe = false;
        assert_eq!(config().evaluate(i), Some(Kind::Vertical));
    }
    #[test]
    fn recovery_waits_for_rider_contact_then_finishes_even_if_contact_changes() {
        let mut s = State::default();
        s.begin(Kind::Front);
        for _ in 0..49 {
            assert!(!s.step(1. / 60., false));
        }
        assert_eq!(s.phase, Phase::Recovering);
        for _ in 0..180 {
            assert!(!s.step(1. / 60., false));
        }
        assert_eq!(s.timer, 0.);
        assert!(!s.step(1. / 60., true));
        for _ in 0..106 {
            assert!(!s.step(1. / 60., false));
        }
        assert!(s.step(2. / 60., false));
    }
}
