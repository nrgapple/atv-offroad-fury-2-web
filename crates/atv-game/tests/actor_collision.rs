//! `UseCollision` TOY actors collide (research/AMBIENT_ACTORS.md,
//! Collision): Rockbridge's windmill registers its original type-0
//! `anim\windmill.col` in the ride's dynamic scene list at its placement,
//! and a quad driven into the tower makes chassis contact with that body
//! and does not pass through it.
//! Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test actor_collision -- --ignored --nocapture
use atv_content::{Content, Store};
use atv_core::math::Det;
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
fn a_quad_driven_into_the_rockbridge_windmill_hits_its_collision() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env02", false)
            .await
            .unwrap();
        let actors = venue.actors.clone().expect("env02 actors");
        let (centre, bodies) = {
            let actors = actors.borrow();
            let body = actors
                .collision
                .iter()
                .find(|b| {
                    actors.actors[b.actor]
                        .definition
                        .mcf
                        .eq_ignore_ascii_case("windmill.mcf")
                })
                .expect("the windmill's collision body")
                .clone();
            let m = body.world_matrix;
            ([m[12], m[13], m[14]], actors.collision.len())
        };
        assert_eq!(bodies, 1, "env02: only the windmill has UseCollision");
        for (name, approach) in [
            ("from-south", 0f32),
            ("from-west", std::f32::consts::FRAC_PI_2),
        ] {
            let mut ride = atv_game::ride::load(&content, &venue, 1, 0, |_| {})
                .await
                .unwrap();
            let idle = Controls::default();
            for _ in 0..10 {
                ride.step(&idle, 0.).unwrap();
            }
            // 40 units out on the approach bearing (yaw in radians, forward
            // = (sin, cos)), heading at the tower.
            let (s, c) = (approach.sin_d(), approach.cos_d());
            let start = [centre[0] - s * 40., centre[1] + 6., centre[2] - c * 40.];
            ride.place_beside(start, approach, [s * 30., 0., c * 30.])
                .unwrap();
            let throttle = Controls {
                throttle: 1.,
                ..Default::default()
            };
            let (mut contacts, mut closest) = (0usize, f32::MAX);
            for _ in 0..240 {
                ride.step(&throttle, 0.).unwrap();
                let f = ride.frame();
                assert!(f.origin.iter().all(|v| v.is_finite()));
                contacts += usize::from(f.dynamic_contact_count > 0);
                closest = closest.min(
                    ((f.origin[0] - centre[0]).powi(2) + (f.origin[2] - centre[2]).powi(2)).sqrt(),
                );
            }
            eprintln!("{name}: steps in dynamic contact {contacts}, closest {closest:.2}");
            assert!(contacts > 0, "{name}: no contact with the windmill body");
            assert!(closest > 1.0, "{name}: the quad passed through the tower");
        }
    });
}
