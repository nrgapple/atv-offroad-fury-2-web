//! A motorcycle meets Arlington's moving train (tools/test_motorcycle_train.mjs
//! for the browser game): the bike's scene carries the original train bodies
//! and the ride steps them, so a broadside hit wrecks and a drop onto the
//! train lands on it. Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test motorcycle_train -- --ignored
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

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn motorcycle_collides_with_the_moving_train() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env08", false)
            .await
            .unwrap();
        // (local offset on car 0, heading along the car (+1) or across (0), throttle)
        for (name, local, direction, throttle) in [
            ("side-wreck", [17., 7., -20.], 0., 1.),
            ("top-drop", [0., 25., -10.], 1., 0.),
        ] {
            let mut ride = atv_game::ride::load(&content, &venue, 29, 0, |_| {})
                .await
                .unwrap();
            assert!(ride.is_motorcycle());
            let idle = Controls::default();
            for _ in 0..30 {
                ride.step(&idle, 0.).unwrap();
            }
            let m = ride.train_car(0).expect("the Arlington train");
            let origin: [f32; 3] = std::array::from_fn(|i| {
                m[12 + i] + m[i] * local[0] + m[4 + i] * local[1] + m[8 + i] * local[2]
            });
            let forward = if direction != 0. {
                [m[8] * direction, m[10] * direction]
            } else {
                [-m[0], -m[2]]
            };
            ride.place_beside(origin, forward[0].atan2_d(forward[1]))
                .unwrap();
            let controls = Controls {
                throttle,
                ..Default::default()
            };
            let (mut contacts, mut supported, mut wrecked) = (0, 0, false);
            for _ in 0..720 {
                ride.step(&controls, 0.).unwrap();
                let f = ride.motorcycle().unwrap().frame();
                assert!(f.origin.iter().all(|v| v.is_finite()));
                contacts = contacts.max(f.dynamic_contact_count);
                supported += usize::from(f.moving_support);
                wrecked |= f.wreck.phase != Phase::Riding;
            }
            eprintln!(
                "{name}: dynamic contacts {contacts}, moving support {supported} steps, wrecked {wrecked}"
            );
            if name == "side-wreck" {
                assert!(contacts > 0, "side: no chassis contact with the train");
                assert!(wrecked, "side: a broadside train hit must wreck");
            } else {
                assert!(supported > 0, "top: tires must stand on the moving train");
            }
        }
    });
}
