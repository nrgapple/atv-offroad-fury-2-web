//! Arrivals appear beside a rider already riding (atv-web
//! multiplayer_world.rs `spawn`): six candidate spots around that rider,
//! each on the ground or a nearby surface, clear of other riders and props.
//! Authored browser behaviour, not a recovered PS2 routine.
use crate::Result;
use crate::ride::Ride;
use atv_core::collision::{CollisionMesh, CollisionRay, CollisionRayTree};
use atv_core::math::Det;
use atv_core::obstacle::{query_bundle_from_root, relative_transform, transform_point};
use atv_net::Snapshot;

const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

/// The highest surface of `mesh` under `point`, within `depth` feet.
fn surface(
    mesh: &CollisionMesh,
    matrix: &[f32; 16],
    point: [f32; 3],
    depth: f32,
) -> Result<Option<f32>> {
    let rays = CollisionRayTree {
        transform: IDENTITY,
        query_capacity: 8,
        nodes: vec![],
        rays: vec![CollisionRay {
            start: [0., 0., 0.],
            end: [0., -depth, 0.],
        }],
    };
    let mut world = IDENTITY;
    world[12..15].copy_from_slice(&point);
    let hits = query_bundle_from_root(
        &rays,
        mesh,
        &relative_transform(&world, matrix),
        &relative_transform(matrix, &world),
        0x8000,
    )?;
    Ok(hits
        .into_iter()
        .map(|h| point[1] - h.hit.fraction * depth)
        .max_by(f32::total_cmp))
}

/// A clear spot beside `target` (a fresh, Riding snapshot) and its heading,
/// avoiding the riders at `others`.
pub fn beside(ride: &Ride, target: &Snapshot, others: &[[f32; 3]]) -> Result<([f32; 3], f32)> {
    let m = target.atv.to_column_major();
    let scene = ride.scene_collision();
    let objects = || {
        scene.into_iter().flat_map(|scene| {
            scene
                .obstacles()
                .iter()
                .chain(scene.dynamic().iter().map(|d| &d.obstacle))
        })
    };
    for [side, forward] in [
        [10., 0.],
        [-10., 0.],
        [0., -12.],
        [0., 12.],
        [14., -8.],
        [-14., -8.],
    ] {
        let mut origin = transform_point(&m, [side, 0., forward]);
        if origin[0] < 0. || origin[2] < 0. {
            continue;
        }
        let Some(ground) = ride.ground(origin[0], origin[2]) else {
            continue;
        };
        let mut height = ground;
        let ray = [origin[0], target.atv.position[1] + 5., origin[2]];
        for object in objects() {
            let bounds = object.world_bounds()?;
            if (origin[0] - bounds.center[0]).abs() > bounds.half[0] + 3.
                || (origin[2] - bounds.center[2]).abs() > bounds.half[2] + 3.
            {
                continue;
            }
            if let Some(y) = surface(&object.mesh, &object.world_matrix, ray, 18.)? {
                height = height.max(y);
            }
        }
        if (height - target.atv.position[1]).abs() > 8. {
            continue;
        }
        origin[1] = height + 2.5;
        if others.iter().any(|p| {
            p.iter()
                .zip(origin)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                < 64.
        }) {
            continue;
        }
        // Conservative clearance: never partly through a wall or car side.
        let blocked = objects().any(|object| {
            object.world_bounds().is_ok_and(|b| {
                (origin[0] - b.center[0]).abs() < b.half[0] + 3.
                    && (origin[2] - b.center[2]).abs() < b.half[2] + 3.
                    && b.center[1] + b.half[1] > height + 0.75
                    && b.center[1] - b.half[1] < height + 6.
            })
        });
        if blocked {
            continue;
        }
        return Ok((origin, m[8].atan2_d(m[10])));
    }
    Err("No clear nearby spawn found. Ask the rider to move into an open area.".into())
}
