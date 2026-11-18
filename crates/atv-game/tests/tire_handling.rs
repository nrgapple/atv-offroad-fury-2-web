//! Drive both vehicle types with the full private venue and model assets.
use atv_content::{Content, Store};
use atv_core::tire_wear::Handling;
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
fn corner_and_brake_on_both_mounts() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env01", false)
            .await
            .unwrap();
        for id in [1, 29] {
            for mode in [Handling::Original, Handling::Sport, Handling::Simulation] {
                let mut ride = atv_game::ride::load(&content, &venue, id, 0, |_| {})
                    .await
                    .unwrap();
                ride.options.handling = mode;
                let mut peak = 0_f32;
                let began = std::time::Instant::now();
                for tick in 0..600 {
                    let controls = Controls {
                        throttle: if (60..450).contains(&tick) { 1. } else { 0. },
                        steer: if tick >= 300 { -0.55 } else { 0. },
                        brake: if tick >= 450 { 0.8 } else { 0. },
                        ..Default::default()
                    };
                    ride.step(&controls, 0.).unwrap();
                    assert!(ride.frame().origin.iter().all(|v| v.is_finite()));
                    peak = peak.max(ride.frame().speed);
                }
                let step_us = began.elapsed().as_secs_f64() * 1e6 / 600.;
                eprintln!(
                    "{id} {mode:?}: {step_us:.0} us/step, peak {peak:.2} ft/s, wear {:.5}, brake heat {:.5}",
                    ride.tire_wear(),
                    ride.brake_heat()
                );
                assert!(peak > 15.);
                assert_eq!(ride.tire_wear() > 0., mode != Handling::Original);
                assert_eq!(ride.brake_heat() > 0., mode != Handling::Original);
                assert_eq!(ride.assisted(), mode != Handling::Original);
                // Drop back onto the start surface with no throttle. Contact
                // must return on both vehicles, with wheel state still finite.
                let (mut drop, yaw) = ride.start_pose();
                drop[1] = ride.ground(drop[0], drop[2]).unwrap() + 26.;
                let wear = ride.tire_wear();
                ride.place_beside(drop, yaw, [0.; 3]).unwrap();
                assert!(ride.tire_wear() >= wear);
                let mut airborne = false;
                let mut landed = false;
                let shock_before = ride.shock_wear();
                for _ in 0..150 {
                    ride.step(&Controls::default(), 0.).unwrap();
                    let frame = ride.frame();
                    assert!(frame.linear_velocity.iter().all(|v| v.is_finite()));
                    let grounded = frame.grounded.iter().any(|g| *g);
                    landed |= airborne && grounded;
                    airborne |= !grounded;
                }
                assert!(landed);
                assert!(ride.shock_wear() >= shock_before);
                eprintln!(
                    "{id} {mode:?}: drop added {:.5} shock wear",
                    ride.shock_wear() - shock_before
                );
            }
        }
    });
}
