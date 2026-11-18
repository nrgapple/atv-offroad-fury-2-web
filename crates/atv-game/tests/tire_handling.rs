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
                eprintln!(
                    "{id} {mode:?}: peak {peak:.2} ft/s, wear {:.5}",
                    ride.tire_wear()
                );
                assert!(peak > 15.);
                assert_eq!(ride.tire_wear() > 0., mode != Handling::Original);
                assert_eq!(ride.assisted(), mode != Handling::Original);
            }
        }
    });
}
