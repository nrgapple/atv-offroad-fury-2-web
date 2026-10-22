//! Settings → Realistic train riding on Arlington's flatbed, the Rust ride's
//! version of tools/test_train_riding.mjs: the quad placed on car 6 through
//! the long left curve at the car's own velocity stays put with the setting
//! on, and slides off in the original mode (world-frame grip and drag).
//! Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test train_riding -- --ignored
use atv_content::{Content, Store};
use atv_core::math::Det;
use atv_core::wreck::Phase;
use atv_game::ride::Controls;
use std::future::Future;
use std::task::{Context, Poll, Waker};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

/// Car-local coordinates of `p` in column-major `m` (the JS test's toLocal).
fn local(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    let d: [f32; 3] = std::array::from_fn(|i| p[i] - m[12 + i]);
    std::array::from_fn(|axis| {
        let c = axis * 4;
        let length = m[c] * m[c] + m[c + 1] * m[c + 1] + m[c + 2] * m[c + 2];
        (d[0] * m[c] + d[1] * m[c + 1] + d[2] * m[c + 2]) / length
    })
}

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn realistic_train_riding_keeps_the_quad_on_the_flatbed() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env08", false)
            .await
            .unwrap();
        const CAR: usize = 6;
        // (coast along the car, braked across the deck)
        for (name, direction, brake) in [("curve-coast", 1., 0.), ("curve-brake-across", 0., 1.)] {
            let mut drift = Vec::new();
            for realistic in [false, true] {
                let mut ride = atv_game::ride::load(&content, &venue, 1, 0, |_| {})
                    .await
                    .unwrap();
                ride.options.realistic_train = realistic;
                let idle = Controls::default();
                for _ in 0..1980 {
                    ride.step(&idle, 0.).unwrap();
                }
                let m = ride.train_car(CAR).unwrap();
                let origin: [f32; 3] = std::array::from_fn(|i| m[12 + i] + m[4 + i] * 8.);
                let velocity = ride.train_point_velocity(CAR, origin).unwrap();
                let forward = if direction != 0. {
                    [m[8], m[10]]
                } else {
                    [-m[0], -m[2]]
                };
                ride.place_beside(origin, forward[0].atan2_d(forward[1]), velocity)
                    .unwrap();
                let controls = Controls {
                    brake,
                    ..Default::default()
                };
                let mut samples = Vec::new();
                for _ in 0..90 {
                    for _ in 0..4 {
                        ride.step(&controls, 0.).unwrap();
                    }
                    let f = ride.frame();
                    samples.push((
                        local(&ride.train_car(CAR).unwrap(), f.origin),
                        f.wreck.phase,
                    ));
                }
                let [x, _, z] = samples[0].0;
                let moved = samples
                    .iter()
                    .map(|(l, _)| ((l[0] - x).powi(2) + (l[2] - z).powi(2)).sqrt())
                    .fold(0f32, f32::max);
                let speed = velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
                eprintln!(
                    "{name} realistic {realistic}: train {speed:.1} ft/s, drift {moved:.1} ft, grounded {:?}",
                    ride.frame().grounded
                );
                if realistic {
                    assert!(speed > 40., "{name}: train is not moving");
                    assert!(
                        samples.iter().all(|(_, p)| *p == Phase::Riding),
                        "{name}: realistic rider wrecked"
                    );
                    assert!(
                        ride.frame().grounded.iter().all(|g| *g),
                        "{name}: realistic quad left the deck"
                    );
                }
                drift.push(moved);
            }
            assert!(
                drift[1] < 2.,
                "{name}: realistic drifted {:.1} ft",
                drift[1]
            );
            assert!(
                drift[0] > 10.,
                "{name}: original drifted only {:.1} ft",
                drift[0]
            );
        }
    });
}

/// An arrival spawned beside a rider on the moving flatbed (Players → Spawn
/// beside, or meeting on join) starts with the car's point velocity at its
/// spot, as atv-web's `spawn` returns it. The clearance check rejects most
/// spots beside a train rider in both engines; at this route phase the first
/// clear spot is on the ground beside the car.
#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn arrival_beside_a_rider_on_the_train_starts_with_the_car_velocity() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env08", false)
            .await
            .unwrap();
        const CAR: usize = 6;
        let mut ride = atv_game::ride::load(&content, &venue, 1, 0, |_| {})
            .await
            .unwrap();
        ride.options.realistic_train = true;
        let idle = Controls::default();
        for _ in 0..300 {
            ride.step(&idle, 0.).unwrap();
        }
        // The rider being joined: parked on the flatbed (realistic riding
        // keeps it there), facing along the car.
        let m = ride.train_car(CAR).unwrap();
        let on_deck: [f32; 3] = std::array::from_fn(|i| m[12 + i] + m[4 + i] * 8.);
        let velocity = ride.train_point_velocity(CAR, on_deck).unwrap();
        ride.place_beside(on_deck, m[8].atan2_d(m[10]), velocity)
            .unwrap();
        for _ in 0..120 {
            ride.step(&idle, 0.).unwrap();
        }
        assert_eq!(ride.train_platform().map(|(car, _)| car), Some(CAR));
        let m = ride.train_car(CAR).unwrap();
        let mut atv = m;
        atv[12..15].copy_from_slice(&ride.frame().origin);
        let target = atv_net::Snapshot {
            atv: atv_net::Transform::from_column_major(atv).unwrap(),
            platform: Some(Box::new(atv_net::Platform {
                car: CAR as u8,
                transform: atv_net::Transform::from_column_major(m).unwrap(),
            })),
            ..Default::default()
        };
        let (origin, yaw, velocity) =
            atv_game::multiplayer::spawn::beside(&ride, &target, &[]).unwrap();
        assert_eq!(velocity, ride.train_point_velocity(CAR, origin).unwrap());
        let speed = velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(speed > 40., "the train is moving ({speed} ft/s)");
        // Without a platform the arrival starts at rest.
        let ground = atv_net::Snapshot {
            platform: None,
            ..target.clone()
        };
        assert_eq!(
            atv_game::multiplayer::spawn::beside(&ride, &ground, &[])
                .unwrap()
                .2,
            [0.; 3]
        );
        ride.place_beside(origin, yaw, velocity).unwrap();
        let placed = ride.frame().linear_velocity;
        eprintln!("arrival at {origin:?}: car {velocity:?}, placed {placed:?}");
        for i in 0..3 {
            assert!(
                (placed[i] - velocity[i]).abs() < 0.5,
                "{placed:?} vs {velocity:?}"
            );
        }
    });
}
