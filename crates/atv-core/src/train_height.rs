//! The procedural actor's original one-leaf type2 scene-height query.
use crate::{
    Result,
    collision::{CollisionRay, CollisionRayTree},
    obstacle::{query_bundle_from_root, relative_transform, sphere_separation, transform_point},
    scene_collision::{SceneCollision, SceneNode},
    scene_partition::Bounds,
    wheel::{Vec3, dot},
};
const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
///0011c840 queries from the preceding model origin, ignoring vegetation and its
///own body.00328990 creates leaf0 directly, without an internal ray-tree node.
///Direct scene order is static provider followed by the original dynamic list.
pub fn height_geometry(
    scene: &SceneCollision,
    own_dynamic: Option<usize>,
    previous_origin: Vec3,
) -> Result<Option<f32>> {
    if !previous_origin.iter().all(|v| v.is_finite()) {
        return Err("nonfinite procedural scene-height origin".into());
    }
    let rays = CollisionRayTree {
        transform: IDENTITY,
        query_capacity: 3,
        nodes: Vec::new(),
        rays: vec![CollisionRay {
            start: [0., 5., 0.],
            end: [0., -5., 0.],
        }],
    };
    let mut world = IDENTITY;
    world[12..15].copy_from_slice(&previous_origin);
    let mut closest = 1.;
    let mut hit = false;
    for node in scene.direct_query(
        Bounds {
            center: previous_origin,
            half: [0., 5., 0.],
        },
        true,
        |_, _| true,
    )? {
        let object = match node {
            SceneNode::StaticObstacle(index) => &scene.obstacles()[index],
            SceneNode::DynamicObstacle(index) if Some(index) != own_dynamic => {
                &scene.dynamic()[index].obstacle
            }
            _ => continue,
        };
        let root = object
            .mesh
            .nodes
            .first()
            .ok_or("missing procedural height mesh root")?;
        let center = transform_point(&object.world_matrix, root.center);
        let radius = dot(root.half_extents, root.half_extents).sqrt();
        if sphere_separation(previous_origin, 5., center, radius) != 0. {
            continue;
        }
        let to_mesh = relative_transform(&world, &object.world_matrix);
        let to_ray = relative_transform(&object.world_matrix, &world);
        for result in query_bundle_from_root(&rays, &object.mesh, &to_mesh, &to_ray, 0x8000)? {
            if result.hit.fraction < closest {
                closest = result.hit.fraction;
                hit = true;
            }
        }
    }
    //0011c840 reconstructs the hit from the retained fraction, rather than
    //taking a transformed triangle intersection's Y coordinate.
    Ok(hit.then(|| previous_origin[1] + ((1. - closest) * 2. * 5. - 5.)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision::{CollisionMesh, RayTreeNode, Triangle},
        obstacle::StaticObstacle,
        scene_collision::DynamicObstacle,
        scene_partition::Config,
        train::MotionState,
    };
    use std::rc::Rc;
    fn platform(y: f32) -> StaticObstacle {
        let mut world = IDENTITY;
        world[12..15].copy_from_slice(&[40., y, 40.]);
        StaticObstacle {
            world_matrix: world,
            mesh: Rc::new(CollisionMesh {
                transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
                vertices: vec![[-6., 0., -6.], [-6., 0., 6.], [6., 0., -6.]],
                vertex_normal_bits: None,
                triangles: vec![Triangle {
                    indices: [0, 1, 2],
                    normal: [0., 1., 0.],
                    distance: 0.,
                    invalid_plane_bits: None,
                }],
                nodes: vec![RayTreeNode {
                    center: [0.; 3],
                    half_extents: [6., 0., 6.],
                    bound_measure: 72.,
                    children: [0x8000; 2],
                }],
                broad_phase_nodes: 1,
                flags: 0,
            }),
        }
    }
    #[test]
    fn nearest_platform_replaces_static_height_and_excludes_own_body() {
        let mut scene = SceneCollision::new(
            Config {
                min_x: 0.,
                max_x: 128.,
                min_z: 0.,
                max_z: 128.,
                cell_size: 32.,
            },
            vec![platform(8.)],
            None,
            false,
            |_| Ok(0.),
        )
        .unwrap();
        let dynamic = [14., 12.].map(|y| {
            let obstacle = platform(y);
            DynamicObstacle {
                motion: MotionState {
                    current: obstacle.world_matrix,
                    previous: obstacle.world_matrix,
                    reciprocal_dt: 60.,
                },
                obstacle,
            }
        });
        scene.install_dynamic(dynamic.into()).unwrap();
        assert_eq!(
            height_geometry(&scene, None, [40., 10., 40.]).unwrap(),
            Some(14.)
        );
        assert_eq!(
            height_geometry(&scene, Some(0), [40., 10., 40.]).unwrap(),
            Some(12.)
        );
        assert_eq!(
            height_geometry(&scene, None, [70., 10., 70.]).unwrap(),
            None
        );
        assert_eq!(
            height_geometry(&scene, None, [40., 30., 40.]).unwrap(),
            None
        );
    }
}
