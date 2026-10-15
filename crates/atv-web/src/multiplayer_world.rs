//! Authored shared train presentation, peer contacts and safe nearby spawning.
use super::*;
use atv_core::obstacle::{relative_transform, transform_point};
const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn surface(
    mesh: &atv_core::collision::CollisionMesh,
    matrix: &[f32; 16],
    point: [f32; 3],
    depth: f32,
) -> Result<Option<f32>> {
    let rays = atv_core::collision::CollisionRayTree {
        transform: IDENTITY,
        query_capacity: 8,
        nodes: vec![],
        rays: vec![atv_core::collision::CollisionRay {
            start: [0., 0., 0.],
            end: [0., -depth, 0.],
        }],
    };
    let mut world = IDENTITY;
    world[12..15].copy_from_slice(&point);
    let hits = atv_core::obstacle::query_bundle_from_root(
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
pub(super) fn platform(
    sim: &Simulation,
    frame: &atv_core::vehicle::VehicleFrame,
) -> Result<Option<Box<atv_net::Platform>>> {
    if !frame.grounded.iter().any(|v| *v) || !frame.wheel_obstacle_support.iter().any(|v| *v) {
        return Ok(None);
    }
    let Some(train) = &sim.train else {
        return Ok(None);
    };
    let Some(support) = sim.vehicle.as_ref().and_then(|v| v.network_platform()) else {
        return Ok(None);
    };
    for binding in &sim.train_collision {
        let Some(motion) = train.motion_state(binding.car) else {
            continue;
        };
        if motion.current == support.current {
            return Ok(Some(Box::new(atv_net::Platform {
                car: binding.car as u8,
                transform: mapped(Transform::from_column_major(motion.current))?,
            })));
        }
    }
    Ok(None)
}

pub(super) fn rebase(snapshot: &mut Snapshot, sim: &Simulation) -> Result<()> {
    let Some(platform) = &snapshot.platform else {
        return Ok(());
    };
    let Some(motion) = sim
        .train
        .as_ref()
        .and_then(|t| t.motion_state(platform.car as usize))
    else {
        return Ok(());
    };
    let sender = platform.transform.to_column_major();
    let local = relative_transform(&snapshot.atv.to_column_major(), &sender);
    snapshot.atv = mapped(Transform::from_column_major(multiply(
        motion.current,
        local,
    )))?;
    // Detached riders retain their independent world-space pose.
    if snapshot.rider_state != RiderState::Wreck {
        let local = relative_transform(&snapshot.rider.to_column_major(), &sender);
        snapshot.rider = mapped(Transform::from_column_major(multiply(
            motion.current,
            local,
        )))?;
    }
    Ok(())
}
/// Every rider derives the train phase from the relay's wall clock, so the
/// shared train keeps running when any rider (including the host) pauses,
/// leaves or hands over the room. Large resyncs rebuild pose history.
pub fn synchronize_train(sim: &mut Simulation, now: u64, dt: f32) -> Result<bool> {
    let target = CLIENT.with_borrow(|c| {
        if !c.lobby.as_ref().is_some_and(|l| l.started) {
            return None;
        }
        server_now(c, now)
    });
    let Some(server_ms) = target else {
        return Ok(false);
    };
    let attached = sim
        .vehicle
        .as_ref()
        .map(|v| v.frame())
        .map(|f| platform(sim, &f))
        .transpose()?
        .flatten();
    let Some(train) = sim.train.as_mut() else {
        return Ok(false);
    };
    let duration = train.route.duration();
    if !duration.is_finite() || duration <= 0. {
        return Ok(false);
    }
    let target = (server_ms / 1000.).rem_euclid(f64::from(duration)) as f32;
    let error = (target - train.time + duration * 0.5).rem_euclid(duration) - duration * 0.5;
    if error.abs() > 0.25 {
        // Prime the route's heading filter and previous/current velocity pair.
        train.time = (target - 16. * dt).rem_euclid(duration);
        for _ in 0..16 {
            super::super::step_train_scene(sim, dt)?;
        }
        if let Some(platform) = attached {
            let matrix = sim
                .train
                .as_ref()
                .unwrap()
                .motion_state(platform.car as usize)
                .unwrap()
                .current;
            if let Some(vehicle) = sim.vehicle.as_mut() {
                let origin = vehicle.frame().origin;
                let inverse = relative_transform(&IDENTITY, &platform.transform.to_column_major());
                let moved = transform_point(&matrix, transform_point(&inverse, origin));
                vehicle.translate_network(std::array::from_fn(|i| moved[i] - origin[i]));
            }
        }
    } else {
        // Gentle phase correction avoids impulses from clock jitter.
        sim.train.as_mut().unwrap().time = (sim.train.as_ref().unwrap().time
            + (error - dt).clamp(-dt * 0.25, dt * 0.25))
        .rem_euclid(duration);
        super::super::step_train_scene(sim, dt)?;
    }
    Ok(true)
}
pub fn collide(sim: &mut Simulation, now: u64, dt: f32) -> Result<()> {
    CLIENT.with_borrow_mut(|c| {
        let Some(id) = c.peer else { return Ok(()) };
        // Riders appear beside each other after a spawn; let them separate.
        if now < c.ghost_until {
            return Ok(());
        }
        for (&peer, buffer) in &c.buffers {
            let Some(mut pose) = buffer.sample_current(now) else {
                continue;
            };
            if pose.stale {
                continue;
            }
            rebase(&mut pose.snapshot, sim)?;
            if let Some(vehicle) = sim.vehicle.as_mut()
                && vehicle.peer_contact(
                    pose.snapshot.atv.position,
                    pose.snapshot.velocity,
                    pose.snapshot.atv.rotation,
                    dt,
                    if id < peer { -1. } else { 1. },
                )
            {
                c.contacts += 1;
            }
        }
        Ok(())
    })
}
/// Spawning (beside a rider, or on joining) briefly disables rider contacts.
pub fn ghost(now: u64) {
    CLIENT.with_borrow_mut(|c| c.ghost_until = now + 2500);
}
pub fn spawn(sim: &Simulation, id: u8, now: u64) -> Result<Value> {
    ghost(now);
    CLIENT.with_borrow(|c| {
        if c.peer == Some(id)
            || !c
                .lobby
                .as_ref()
                .is_some_and(|l| l.started && l.players.iter().any(|p| p.id == id))
        {
            return Err("That rider has left the lobby.".into());
        }
        let mut pose = c
            .buffers
            .get(&id)
            .and_then(|b| b.sample_current(now))
            .filter(|p| !p.stale)
            .ok_or("Waiting for a fresh position from that rider.")?;
        if pose.snapshot.rider_state != RiderState::Riding {
            return Err("Wait until that rider is safely on the ground.".into());
        }
        rebase(&mut pose.snapshot, sim)?;
        let mut peers = Vec::new();
        for buffer in c.buffers.values() {
            if let Some(mut p) = buffer.sample_current(now).filter(|p| !p.stale) {
                rebase(&mut p.snapshot, sim)?;
                peers.push(p.snapshot.atv.position);
            }
        }
        let m = pose.snapshot.atv.to_column_major();
        let terrain = sim.terrain.as_ref().ok_or("Course is still loading.")?;
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
            let Some(ground) = terrain.contact(origin[0], origin[2])? else {
                continue;
            };
            let mut height = ground.height;
            let ray = [origin[0], pose.snapshot.atv.position[1] + 5., origin[2]];
            if let Some(scene) = sim.vehicle.as_ref().and_then(|v| v.scene_collision()) {
                for object in scene
                    .obstacles()
                    .iter()
                    .chain(scene.dynamic().iter().map(|d| &d.obstacle))
                {
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
            }
            if (height - pose.snapshot.atv.position[1]).abs() > 8. {
                continue;
            }
            origin[1] = height + 2.5;
            if peers.iter().any(|p| {
                p.iter()
                    .zip(origin)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    < 64.
            }) {
                continue;
            }
            // Conservative clearance prevents spawning partly through a wall or
            // car side. Support geometry ends below the new vehicle's chassis.
            if let Some(scene) = sim.vehicle.as_ref().and_then(|v| v.scene_collision()) {
                let blocked = scene
                    .obstacles()
                    .iter()
                    .chain(scene.dynamic().iter().map(|d| &d.obstacle))
                    .any(|object| {
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
            }
            let velocity = pose
                .snapshot
                .platform
                .as_ref()
                .and_then(|p| sim.train.as_ref()?.motion_state(p.car as usize))
                .map_or([0.; 3], |m| m.point_velocity(origin));
            return Ok(json!({"origin":origin,"yaw":m[8].atan2(m[10]),"velocity":velocity}));
        }
        Err("No clear nearby spawn found. Ask the rider to move into an open area.".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebasing_preserves_train_relative_pose_and_detached_rider_world() {
        let route=atv_core::train::Route::from_vue(b"FRAME 0\ntransform \"Body\" 1 0 0 0 1 0 0 0 1 0 0 0\nFRAME 60\ntransform \"Body\" 1 0 0 0 1 0 0 0 1 100 0 0\n",30.).unwrap();
        let definition = atv_core::train::Definition {
            model: "train".into(),
            route: "test".into(),
            fps: 30.,
            lag_distance: 0,
            offset: [0.; 3],
            path_is_level: true,
            drive_on_soultrees: false,
            use_collision: true,
            use_lighting: true,
        };
        let mut train = atv_core::train::Runtime::new(route, vec![definition]).unwrap();
        for _ in 0..10 {
            train.step(1. / 60., |_| Ok(None)).unwrap();
        }
        let before = train.motion_state(0).unwrap().current;
        let mut local = IDENTITY;
        local[12..15].copy_from_slice(&[1., 8., -2.]);
        let atv = Transform::from_column_major(multiply(before, local)).unwrap();
        let mut snapshot = Snapshot {
            atv,
            rider: atv,
            train_time: Some(train.time),
            platform: Some(Box::new(atv_net::Platform {
                car: 0,
                transform: Transform::from_column_major(before).unwrap(),
            })),
            ..Snapshot::default()
        };
        for _ in 0..60 {
            train.step(1. / 60., |_| Ok(None)).unwrap();
        }
        let after = train.motion_state(0).unwrap().current;
        let sim = Simulation {
            train: Some(train),
            ..Simulation::default()
        };
        rebase(&mut snapshot, &sim).unwrap();
        let actual = relative_transform(&snapshot.atv.to_column_major(), &after);
        for (a, b) in actual.iter().zip(local) {
            assert!((a - b).abs() < 0.001);
        }
        snapshot.rider_state = RiderState::Wreck;
        let detached = snapshot.rider;
        rebase(&mut snapshot, &sim).unwrap();
        assert_eq!(snapshot.rider, detached);
    }
    #[test]
    fn spawn_rejects_self_missing_peer_and_stale_pose() {
        CLIENT.with_borrow_mut(|c| {
            *c = Client {
                peer: Some(0),
                lobby: Some(atv_net::Lobby {
                    host: 0,
                    epoch: 1,
                    map: 1,
                    started: true,
                    players: vec![atv_net::Participant {
                        id: 1,
                        name: "Guest".into(),
                        atv_model: 1,
                        rider_model: 0,
                        appearance: [0; 8],
                    }],
                }),
                ..Client::default()
            }
        });
        assert!(spawn(&Simulation::default(), 0, 100).is_err());
        assert!(spawn(&Simulation::default(), 2, 100).is_err());
        CLIENT.with_borrow_mut(|c| {
            let mut b = PoseBuffer::default();
            b.push(
                Snapshot {
                    peer: 1,
                    ..Snapshot::default()
                },
                0,
            )
            .unwrap();
            c.buffers.insert(1, b);
        });
        assert!(
            spawn(&Simulation::default(), 1, 2000)
                .unwrap_err()
                .contains("fresh")
        );
    }
}
