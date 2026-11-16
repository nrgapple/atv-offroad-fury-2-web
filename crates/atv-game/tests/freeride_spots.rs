//! Free Ride spawn spots on the eight Environment venues (issue #710,
//! research/FREERIDE_HUB.md "Spawn spots"): each venue's long Enduro line
//! yields at least `MIN_SPOTS` validated spots covering at least
//! `MIN_CELLS` cells of a 4 × 4 grid over its rideable area, and an ATV
//! dropped on every spot settles there: still riding, dry, on all four
//! wheels, within `MAX_ROLL` feet (an idle ATV rolls a little on the
//! gentle slopes allowed). Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test freeride_spots -- --ignored --nocapture
use atv_content::{Content, Store};
use atv_core::math::Det;
use atv_core::wreck::Phase;
use atv_game::multiplayer::spots::{self, Spots};
use atv_game::ride::Controls;
use std::future::Future;
use std::task::{Context, Poll, Waker};

const MIN_SPOTS: usize = 20;
const MIN_CELLS: usize = 6;
/// Idle steps after placement (1.5 s at 60 Hz).
const SETTLE_STEPS: usize = 90;
const MAX_ROLL: f32 = 10.;

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
fn every_environment_venue_has_spread_valid_spots() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let mut failures = Vec::new();
        for n in 1..=8 {
            let environment = format!("env0{n}");
            let venue = atv_game::venue::load(&content, &environment, false)
                .await
                .unwrap();
            let mut ride = atv_game::ride::load(&content, &venue, 1, 0, |_| {})
                .await
                .unwrap();
            let crowd = venue.scene.crowd_present;
            let found: Spots = spots::free_ride_spots(&content, &environment, &ride, crowd)
                .await
                .unwrap();
            let area = spots::rideable_area(&ride, crowd, spots::INSIDE);
            let cells = found.cells_covered(area, 4);
            println!(
                "{environment}: {} spots of {} candidates, {cells}/16 cells, rejected {:?}",
                found.spots.len(),
                found.tried,
                found.rejected
            );
            if found.spots.len() < MIN_SPOTS || cells < MIN_CELLS {
                failures.push(format!(
                    "{environment}: {} spots, {cells} cells",
                    found.spots.len()
                ));
            }
            let idle = Controls::default();
            let mut worst = 0f32;
            for spot in &found.spots {
                ride.place_beside(spot.origin, spot.yaw, [0.; 3]).unwrap();
                for _ in 0..SETTLE_STEPS {
                    ride.step(&idle, 0.).unwrap();
                }
                let f = ride.frame();
                let moved = (f.origin[0] - spot.origin[0]).hypot_d(f.origin[2] - spot.origin[2]);
                let drop = spot.origin[1] - f.origin[1];
                let wet = ride
                    .water_depth(f.origin[0], f.origin[2])
                    .is_some_and(|d| d > 0.);
                let wheels = f.grounded.iter().filter(|g| **g).count();
                worst = worst.max(moved);
                if f.wreck.phase != Phase::Riding
                    || moved > MAX_ROLL
                    || drop > 6.
                    || wet
                    || wheels < 4
                {
                    failures.push(format!(
                        "{environment} spot {:?}: moved {moved:.1} ft, dropped {drop:.1} ft, \
                         wet {wet}, {wheels} wheels down, {:?}",
                        spot.origin, f.wreck.phase
                    ));
                }
            }
            println!("{environment}: settled within {worst:.1} ft");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}
