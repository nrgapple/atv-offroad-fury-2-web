//! Motorcycle feel on the private venue: cornering sideslip, bank, yaw
//! response and stability per bike class, printed and bounded
//! (research/MOTORCYCLE_HANDLING.md).
use atv_content::{Content, Store};
use atv_core::dynamics;
use atv_core::math::Det;
use atv_game::ride::{Controls, Ride};
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

/// What one step looks like to a player watching the bike.
#[derive(Clone, Copy, Default)]
struct Sample {
    speed: f32,
    /// Angle between heading and travel (deg): a planted bike stays near 0.
    slip: f32,
    /// Bank about the heading (deg, + right side low).
    bank: f32,
    yaw_rate: f32,
    grounded: bool,
    riding: bool,
}

fn sample(ride: &Ride) -> Sample {
    let f = ride.frame();
    let b = dynamics::basis(f.orientation);
    let v = f.linear_velocity;
    let heading = [b[2][0], b[2][2]];
    let length = (heading[0] * heading[0] + heading[1] * heading[1])
        .sqrt()
        .max(1e-6);
    let heading = [heading[0] / length, heading[1] / length];
    let along = v[0] * heading[0] + v[2] * heading[1];
    let across = v[0] * heading[1] - v[2] * heading[0];
    let yaw_rate = dynamics::transform(b, f.local_angular_velocity)[1];
    Sample {
        speed: (v[0] * v[0] + v[2] * v[2]).sqrt(),
        slip: if along.abs() > 3. {
            across.atan2_d(along).to_degrees()
        } else {
            0.
        },
        bank: (-b[0][1]).atan2_d(b[1][1]).to_degrees(),
        yaw_rate,
        grounded: f.grounded.iter().any(|g| *g),
        riding: f.wreck.phase == atv_core::wreck::Phase::Riding,
    }
}

struct Summary {
    peak_slip: f32,
    mean_slip: f32,
    peak_bank: f32,
    mean_yaw: f32,
    entry: f32,
    exit: f32,
    wrecks: u32,
    air: u32,
}

/// Reach `target` ft/s on a straight, then hold `steer` for `ticks`.
fn corner(ride: &mut Ride, target: f32, steer: f32, ticks: usize) -> Summary {
    let (start, yaw) = ride.start_pose();
    ride.place_beside(start, yaw, [0.; 3]).unwrap();
    for _ in 0..900 {
        let s = sample(ride);
        if s.speed >= target {
            break;
        }
        ride.step(
            &Controls {
                throttle: 1.,
                ..Default::default()
            },
            0.,
        )
        .unwrap();
    }
    let entry = sample(ride).speed;
    let mut summary = Summary {
        peak_slip: 0.,
        mean_slip: 0.,
        peak_bank: 0.,
        mean_yaw: 0.,
        entry,
        exit: 0.,
        wrecks: 0,
        air: 0,
    };
    let mut was_riding = true;
    let mut count = 0.;
    for _ in 0..ticks {
        let hold = sample(ride).speed < target;
        ride.step(
            &Controls {
                throttle: if hold { 1. } else { 0.3 },
                steer,
                ..Default::default()
            },
            0.,
        )
        .unwrap();
        let s = sample(ride);
        if was_riding && !s.riding {
            summary.wrecks += 1;
            let f = ride.frame();
            eprintln!(
                "  wreck {:?} at {:.0} ft/s, bank {:.0}, slip {:.0}, impact {:.0}, grounded {:?}",
                f.wreck.kind, s.speed, s.bank, s.slip, f.crash_impact_speed, f.grounded
            );
        }
        was_riding = s.riding;
        if !s.grounded {
            summary.air += 1;
        }
        if s.riding && s.grounded {
            summary.peak_slip = summary.peak_slip.max(s.slip.abs());
            summary.mean_slip += s.slip.abs();
            summary.peak_bank = summary.peak_bank.max(s.bank.abs());
            summary.mean_yaw += s.yaw_rate.abs();
            count += 1.;
        }
    }
    summary.mean_slip /= f32::max(count, 1.);
    summary.mean_yaw /= f32::max(count, 1.);
    summary.exit = sample(ride).speed;
    summary
}

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn report() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    let ids: Vec<u16> = std::env::var("BIKE_IDS")
        .ok()
        .map(|v| v.split(',').filter_map(|s| s.parse().ok()).collect())
        .unwrap_or_else(|| vec![29, 33, 27]);
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env01", false)
            .await
            .unwrap();
        let mut failures = 0;
        for id in ids {
            let mut ride = atv_game::ride::load(&content, &venue, id, 0, |_| {})
                .await
                .unwrap();
            for (name, speed, steer) in [
                ("30 ft/s half", 30., 0.5),
                ("30 ft/s full", 30., 1.),
                ("45 ft/s half", 45., 0.5),
                ("45 ft/s full", 45., 1.),
                ("70 ft/s half", 70., 0.5),
                ("70 ft/s full", 70., 1.),
            ] {
                let s = corner(&mut ride, speed, steer, 180);
                // The single-track handling corners where it points: a turn
                // on real terrain is not ridden sideways, laid flat or ended
                // in a wreck. The recovered side-force model measured mean
                // sideslip of 3 to 22 degrees and banks to 81 here.
                eprintln!(
                    "{id} {name}: entry {:.0} exit {:.0} ft/s, slip mean {:.1} peak {:.1} deg, bank peak {:.0} deg, yaw {:.2} rad/s, air {} wrecks {}",
                    s.entry,
                    s.exit,
                    s.mean_slip,
                    s.peak_slip,
                    s.peak_bank,
                    s.mean_yaw,
                    s.air,
                    s.wrecks
                );
                // The single-track handling corners where it points: a turn
                // on real terrain is not ridden sideways or laid flat. The
                // recovered side-force model measured mean sideslip of 3 to
                // 22 degrees and banks to 81 here. Wrecks are printed, not
                // bounded: a bike that holds its speed through the turn can
                // reach the venue's trees.
                failures += u32::from(s.mean_slip >= 9. || s.peak_bank >= 62.);
            }
            // Free run: full throttle with a gentle weave across the venue.
            let (start, yaw) = ride.start_pose();
            ride.place_beside(start, yaw, [0.; 3]).unwrap();
            let (mut wrecks, mut air, mut peak, mut slip) = (0, 0, 0_f32, 0_f32);
            let mut was_riding = true;
            for tick in 0..1800 {
                ride.step(
                    &Controls {
                        throttle: 1.,
                        steer: 0.35 * (tick as f32 / 90.).sin_d(),
                        ..Default::default()
                    },
                    0.,
                )
                .unwrap();
                let s = sample(&ride);
                if was_riding && !s.riding {
                    wrecks += 1;
                }
                was_riding = s.riding;
                air += u32::from(!s.grounded);
                peak = peak.max(s.speed);
                if s.grounded && s.riding {
                    slip = slip.max(s.slip.abs());
                }
            }
            eprintln!(
                "{id} free run 30 s: peak {peak:.0} ft/s, peak slip {slip:.1} deg, air {air} steps, wrecks {wrecks}"
            );
        }
        assert_eq!(failures, 0, "corners outside the bounds above");
    });
}
