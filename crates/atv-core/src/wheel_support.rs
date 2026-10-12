//! Outer-frame wheel support queries recovered from 001ebd90.
//! The suspension reprojects each moving wheel against this frozen plane during
//! substeps. Static props use the same finite type-3 ray query as rider contacts.
//! Moving bodies, category masks, native spatial-grid ordering and smooth normals
//! still require scene integration; the supplied objects are eligible static props.
use crate::{
    Result,
    collision::CollisionRay,
    obstacle::{StaticObstacle, transform_point},
    terrain::Terrain,
    wheel::{Vec3, add},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Support {
    pub available: bool,
    pub point: Vec3,
    pub normal: Vec3,
    /// The native prop override leaves the terrain-selected material unchanged.
    pub surface: usize,
    pub obstacle: bool,
    ///001ebb30 stores moving Body support independently of the final geometry.
    ///Later static callbacks do not clear it;001ebd90 clears it next outer query.
    pub motion: Option<crate::train::MotionState>,
    pub mud: bool,
    pub mud_height: f32,
}
impl Default for Support {
    fn default() -> Self {
        Self {
            available: false,
            point: [0.; 3],
            normal: [0., 1., 0.],
            surface: 0,
            obstacle: false,
            motion: None,
            mud: false,
            mud_height: 0.,
        }
    }
}

/// 001ebd90/001ebd30, with the original zero query offset supplied by 001e1fd0.
/// The wheel +0x80 direction is DOWN. The ray extends from travel above the
/// uncompressed wheel center to radius below it, along the body orientation.
pub fn support_ray(world: &[f32; 16], rest: Vec3, travel: f32, radius: f32) -> CollisionRay {
    let center = transform_point(world, rest);
    let rotated_offset = |y: f32| -> Vec3 {
        std::array::from_fn(|i| (world[i] * 0. + world[4 + i] * y) + world[8 + i] * 0.)
    };
    CollisionRay {
        start: add(rotated_offset(travel), center),
        end: add(rotated_offset(-radius), center),
    }
}

/// Sample terrain, then apply each successful ordinary static-object override
/// in caller scene order. Native 00327810 resets nearest-face state per object;
/// it does not select one globally nearest object. A prop hit replaces the plane,
/// retaining the surface byte selected by terrain. This runs once per outer frame.
pub fn sample(
    terrain: &Terrain,
    obstacles: &[StaticObstacle],
    world: &[f32; 16],
    rest: Vec3,
    travel: f32,
    radius: f32,
    previous: Support,
) -> Result<Support> {
    if world.iter().chain(rest.iter()).any(|v| !v.is_finite())
        || !travel.is_finite()
        || !radius.is_finite()
        || travel < 0.
        || radius <= 0.
    {
        return Err("invalid wheel support query".into());
    }
    let center = transform_point(world, rest);
    let mut support = Support {
        available: false,
        obstacle: false,
        motion: None,
        mud: false,
        mud_height: 0.,
        ..previous
    };
    if let Some(hit) = terrain.contact(center[0], center[2])? {
        support.available = true;
        support.point = [center[0], hit.height, center[2]];
        support.normal = hit.normal;
        support.surface = usize::from(hit.surface);
    }
    let ray = support_ray(world, rest, travel, radius);
    for hit in crate::wreck::query_static_segments(&[ray], obstacles, usize::MAX)? {
        support.available = true;
        support.obstacle = true;
        support.point = hit.world_point;
        support.normal = hit.world_normal;
    }
    Ok(support)
}

pub(crate) struct SceneSupport {
    pub support: Support,
    pub vegetation_proxies: usize,
    pub vegetation_callbacks: usize,
    pub dynamic_callbacks: usize,
    pub unsupported_vegetation_proxies: usize,
}

///The supplied quad list retains mixed node order. Each wheel has its own
///0011f9a0 plant-sphere gate and each expanded proxy can replace the support
///plane without changing the terrain-selected material.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_with_scene(
    terrain: &Terrain,
    scene: &crate::scene_collision::SceneCollision,
    query: &crate::scene_collision::CachedQuery,
    skip_vegetation: bool,
    world: &[f32; 16],
    rest: Vec3,
    travel: f32,
    radius: f32,
    previous: Support,
) -> Result<SceneSupport> {
    let mut support = sample(terrain, &[], world, rest, travel, radius, previous)?;
    let ray = support_ray(world, rest, travel, radius);
    let (center, query_radius) = crate::wreck::segment_query_sphere(std::slice::from_ref(&ray));
    let selected = scene.physical_selection(
        query,
        crate::scene_collision::PhysicalTrim::Supplied,
        center,
        query_radius,
        skip_vegetation,
        |_, _| true,
    )?;
    let vegetation_proxies = selected.vegetation_proxies();
    let mut vegetation_callbacks = 0;
    let mut dynamic_callbacks = 0;
    let mut geometry_plane = None;
    for mesh in &selected.meshes {
        let hits = crate::wreck::query_static_segments(
            std::slice::from_ref(&ray),
            std::slice::from_ref(&mesh.obstacle),
            usize::MAX,
        )?;
        if !hits.is_empty() && mesh.proxy_index.is_some() {
            vegetation_callbacks += 1;
        }
        if !hits.is_empty() && !mesh.mud && mesh.motion.is_some() {
            dynamic_callbacks += 1;
            support.motion = mesh.motion;
        }
        for hit in hits {
            if mesh.mud {
                support.mud = true;
                support.mud_height = hit.world_point[1];
            } else {
                geometry_plane = Some((hit.world_point, hit.world_normal));
            }
        }
    }
    //001ebd90 leaves the terrain support intact after any MUD callback,
    //even if a later ordinary object changed the query's geometry result.
    if !support.mud
        && let Some((point, normal)) = geometry_plane
    {
        support.available = true;
        support.obstacle = true;
        support.point = point;
        support.normal = normal;
    }
    Ok(SceneSupport {
        support,
        vegetation_proxies,
        vegetation_callbacks,
        dynamic_callbacks,
        unsupported_vegetation_proxies: selected.unsupported_vegetation_proxies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision::{CollisionMesh, RayTreeNode, Triangle},
        terrain::{TerrainLeaf, TerrainTile},
    };
    use std::rc::Rc;
    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    fn terrain() -> Terrain {
        Terrain {
            spacing: 8.,
            tiles: vec![TerrainTile {
                tag: 0,
                heights: [0; 289],
                attributes: [0x0300_7f00; 289],
            }],
            leaves: vec![TerrainLeaf {
                x: 0,
                z: 0,
                tile: 0,
                height_morph: 1.,
                height_scale: 1.,
                minimum_height: 0.,
                maximum_height: 0.,
            }],
            hierarchy_nodes: 1,
            width_cells: 16,
            depth_cells: 16,
        }
    }
    fn platform(height: f32) -> StaticObstacle {
        let mut world_matrix = IDENTITY;
        world_matrix[12] = 30.;
        world_matrix[13] = height;
        world_matrix[14] = 30.;
        StaticObstacle {
            mesh: Rc::new(CollisionMesh {
                transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
                vertices: vec![[-10., 0., -10.], [0., 0., 10.], [10., 0., -10.]],
                vertex_normal_bits: None,
                triangles: vec![Triangle {
                    indices: [0, 1, 2],
                    normal: [0., 1., 0.],
                    distance: 0.,
                    invalid_plane_bits: None,
                }],
                broad_phase_nodes: 1,
                nodes: vec![RayTreeNode {
                    bound_measure: 100.,
                    center: [0.; 3],
                    half_extents: [10., 1., 10.],
                    children: [0x8000, 0x8000],
                }],
                flags: 0,
            }),
            world_matrix,
        }
    }
    #[test]
    fn mud_hit_preserves_terrain_plane_and_independent_moving_owner() {
        use crate::scene_collision::{DynamicObstacle, SceneCollision};
        use crate::train::MotionState;
        for mud_first in [true, false] {
            let objects = vec![platform(2.5), platform(1.5)];
            let mut scene = SceneCollision::new(
                crate::scene_partition::Config {
                    min_x: 0.,
                    max_x: 128.,
                    min_z: 0.,
                    max_z: 128.,
                    cell_size: 16.,
                },
                objects,
                None,
                false,
                |_| Ok(0.),
            )
            .unwrap();
            scene
                .set_mud_obstacles(vec![mud_first, !mud_first])
                .unwrap();
            let object = platform(2.);
            let mut previous = object.world_matrix;
            previous[12] -= 1.;
            let motion = MotionState {
                current: object.world_matrix,
                previous,
                reciprocal_dt: 1.,
            };
            scene
                .install_dynamic(vec![DynamicObstacle {
                    obstacle: object,
                    motion,
                }])
                .unwrap();
            let query = scene.cached_query([30., 2., 30.], 4., |_, _| true).unwrap();
            let result = sample_with_scene(
                &terrain(),
                &scene,
                &query,
                false,
                &IDENTITY,
                [30., 2., 30.],
                1.,
                1.,
                Support::default(),
            )
            .unwrap();
            assert!(result.support.mud);
            assert_eq!(result.support.mud_height, if mud_first { 2.5 } else { 1.5 });
            assert_eq!(result.support.point, [30., 0., 30.]);
            assert!(!result.support.obstacle);
            assert_eq!(
                result.support.surface,
                usize::from(terrain().contact(30., 30.).unwrap().unwrap().surface)
            );
            assert_eq!(result.support.motion, Some(motion));
            //An outer frame with no MUD callback clears only the MUD state.
            let far = scene.cached_query([80., 2., 80.], 4., |_, _| true).unwrap();
            let result = sample_with_scene(
                &terrain(),
                &scene,
                &far,
                false,
                &IDENTITY,
                [80., 2., 80.],
                1.,
                1.,
                result.support,
            )
            .unwrap();
            assert!(!result.support.mud);
            assert!(result.support.motion.is_none());
        }
    }
    #[test]
    fn finite_body_direction_ray_matches_original_interval() {
        let ray = support_ray(&IDENTITY, [1., 2., 3.], 0.75, 1.);
        assert_eq!(ray.start, [1., 2.75, 3.]);
        assert_eq!(ray.end, [1., 1., 3.]);
        let mut rolled = IDENTITY;
        rolled[..8].copy_from_slice(&[0., 1., 0., 0., -1., 0., 0., 0.]);
        let ray = support_ray(&rolled, [0., 2., 0.], 0.75, 1.);
        assert_eq!(ray.start, [-2.75, 0., 0.]);
        assert_eq!(ray.end, [-1., 0., 0.]);
    }
    #[test]
    fn prop_support_overrides_terrain_without_changing_material() {
        let t = terrain();
        let rest = [30., 2., 30.];
        let base = sample(&t, &[], &IDENTITY, rest, 1., 1., Support::default()).unwrap();
        let prop = sample(&t, &[platform(1.5)], &IDENTITY, rest, 1., 1., base).unwrap();
        assert!(prop.available && prop.obstacle);
        assert_eq!(prop.point, [30., 1.5, 30.]);
        assert_eq!(prop.normal, [0., 1., 0.]);
        assert_eq!(prop.surface, base.surface);
        let below = sample(&t, &[platform(0.5)], &IDENTITY, rest, 1., 1., base).unwrap();
        assert!(!below.obstacle);
        assert_eq!(below.point[1], 0.);
    }
    #[test]
    fn each_object_has_its_own_nearest_face_and_last_hit_supplies_plane() {
        let t = terrain();
        let rest = [30., 2., 30.];
        let support = sample(
            &t,
            &[platform(2.5), platform(1.5)],
            &IDENTITY,
            rest,
            1.,
            1.,
            Support::default(),
        )
        .unwrap();
        assert_eq!(support.point[1], 1.5);
    }
}
