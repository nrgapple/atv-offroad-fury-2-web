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
        for id in [1, 29, 27] {
            for mode in [Handling::Original, Handling::Sport, Handling::Simulation] {
                let mut ride = atv_game::ride::load(&content, &venue, id, 0, |_| {})
                    .await
                    .unwrap();
                ride.options.handling = mode;
                let mut peak = 0_f32;
                let mut glow = 0_f32;
                let mut hand_error = 0_f32;
                let mut peg_error = 0_f32;
                let mut attached = 0;
                let began = std::time::Instant::now();
                for tick in 0..600 {
                    let controls = Controls {
                        throttle: if (60..450).contains(&tick) { 1. } else { 0. },
                        steer: if (360..510).contains(&tick) {
                            -0.40
                        } else {
                            0.
                        },
                        brake: if tick >= 450 { 0.8 } else { 0. },
                        ..Default::default()
                    };
                    ride.step(&controls, 0.).unwrap();
                    assert!(ride.frame().origin.iter().all(|v| v.is_finite()));
                    if tick % 10 == 0 {
                        let pose = ride.pose().unwrap();
                        glow = glow.max(pose.brake_glow.into_iter().fold(0., f32::max));
                        if let Some(bike) = ride.motorcycle()
                            && let Some(errors) =
                                bike.grip_errors(&pose.rider_nodes, &pose.quad_nodes)
                        {
                            attached += 1;
                            hand_error = errors.into_iter().fold(hand_error, f32::max);
                            let errors = bike
                                .foot_errors(&pose.rider_nodes, &pose.quad_nodes)
                                .unwrap();
                            for (pin, error) in bike.frame().rider_foot_pins.into_iter().zip(errors)
                            {
                                if pin >= 0.999 {
                                    peg_error = peg_error.max(error);
                                }
                            }
                        }
                    }
                    peak = peak.max(ride.frame().speed);
                }
                let step_us = began.elapsed().as_secs_f64() * 1e6 / 600.;
                eprintln!(
                    "{id} {mode:?}: {step_us:.0} us/step, peak {peak:.2} ft/s, wear {:.5}, brake heat {:.5}",
                    ride.tire_wear(),
                    ride.brake_heat()
                );
                eprintln!(
                    "peak rotor emission {glow:.3}, grip/peg errors {hand_error:.3}/{peg_error:.3} ft"
                );

                if ride.is_motorcycle() {
                    assert!(attached > 20);
                    let tolerance = if mode == Handling::Original {
                        0.32
                    } else {
                        0.10
                    };
                    assert!(
                        hand_error < tolerance && peg_error < tolerance,
                        "{id} {mode:?} grip {hand_error} ft, peg {peg_error} ft"
                    );
                }
                assert!(peak > 15.);
                assert_eq!(ride.tire_wear() > 0., mode != Handling::Original);
                // Reach the braking marker on throttle. Keep the entry speed
                // comparable across engines, including the mini bike.
                let (start, yaw) = ride.start_pose();
                // Follow a hard stop with progressively lighter brake pressure.
                // A locked tire puts most of its energy into the dirt.
                let mut glow = 0_f32;
                for pressure in [1., 0.45, 0.25, 0.18] {
                    ride.place_beside(start, yaw, [0.; 3]).unwrap();
                    let gas = Controls {
                        throttle: 0.82,
                        ..Default::default()
                    };
                    for _ in 0..450 {
                        ride.step(&gas, 0.).unwrap();
                        if ride.frame().speed > 30. {
                            break;
                        }
                    }
                    let entry = ride.frame().speed;
                    assert!(entry > 20., "{id} {mode:?} braking entry {entry} ft/s");
                    let brake = Controls {
                        brake: pressure,
                        ..Default::default()
                    };
                    for _ in 0..120 {
                        ride.step(&brake, 0.).unwrap();
                        let rotors = ride.pose().unwrap().brake_glow;
                        glow = glow.max(rotors.into_iter().fold(0., f32::max));
                    }
                    eprintln!(
                        "braking entry {entry:.2} ft/s, heat {:.4}, peak rotor emission {glow:.3}",
                        ride.brake_heat()
                    );
                    if glow > 0. || mode == Handling::Original {
                        break;
                    }
                }
                assert_eq!(glow > 0., mode != Handling::Original);
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
