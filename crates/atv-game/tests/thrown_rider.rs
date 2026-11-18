//! A motorcycle rider thrown into scenery stops against it (#751): the bike
//! is aimed at trees and props on the private venue and ridden into them,
//! once with the rider meeting scenery and his bike and once against
//! terrain only, as before.
use atv_content::{Content, Store};
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

struct Run {
    wrecked: bool,
    scene: usize,
    bike: usize,
    /// Farthest the pelvis got along the run, past the target's center, in
    /// the first second after the throw.
    beyond: f32,
    pelvis: [f32; 3],
}

/// Rides flat out at `target` from `back` ft away, then lets the rider land.
fn ride_into(ride: &mut Ride, target: [f32; 3], back: f32, angle: f32) -> Option<Run> {
    let (dx, dz) = angle.sin_cos_d();
    let start = [target[0] - dx * back, 0., target[2] - dz * back];
    let ground = ride.ground(start[0], start[2])?;
    ride.place_beside([start[0], ground + 0.5, start[2]], angle, [0.; 3])
        .ok()?;
    let mut run = Run {
        wrecked: false,
        scene: 0,
        bike: 0,
        beyond: f32::MIN,
        pelvis: [0.; 3],
    };
    let mut thrown = 0;
    for tick in 0..420 {
        let riding = ride.frame().wreck.phase == atv_core::wreck::Phase::Riding;
        if riding && tick > 240 {
            break;
        }
        ride.step(
            &Controls {
                throttle: if riding { 1. } else { 0. },
                ..Default::default()
            },
            0.,
        )
        .ok()?;
        if let (Some(bike), Some(w)) = (ride.motorcycle(), ride.detached_rider_world()) {
            if !run.wrecked {
                thrown = tick;
            }
            run.wrecked = true;
            let f = bike.frame();
            run.scene = run.scene.max(f.rider_scene_contacts);
            run.bike = run.bike.max(f.rider_bike_contacts);
            run.pelvis = [w[12], w[13], w[14]];
            // Where his flight takes him in the first second, before he
            // can roll off downhill.
            if tick < thrown + 60 {
                run.beyond = run
                    .beyond
                    .max((w[12] - target[0]) * dx + (w[14] - target[2]) * dz);
            }
        }
    }
    Some(run)
}

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn thrown_rider_stops_at_trees_and_props() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let venue = atv_game::venue::load(&content, "env01", false)
            .await
            .unwrap();
        let mut ride = atv_game::ride::load(&content, &venue, 29, 0, |_| {})
            .await
            .unwrap();
        let (start, _) = ride.start_pose();
        let scene = ride.motorcycle().unwrap().scene_collision().unwrap();
        // Tall, narrow trees and props within reach of the start.
        let mut targets: Vec<(&str, atv_core::scene_partition::Bounds)> = scene
            .vegetation()
            .iter()
            .filter(|v| v.proxy_count > 0)
            .map(|v| ("tree", v.bounds))
            .chain(
                scene
                    .obstacles()
                    .iter()
                    .filter_map(|o| o.world_bounds().ok())
                    .map(|b| ("prop", b)),
            )
            .filter(|(_, b)| b.half[1] > 3. && b.half[0].max(b.half[2]) < 6.)
            .collect();
        let distance = |b: &atv_core::scene_partition::Bounds| {
            let (x, z) = (b.center[0] - start[0], b.center[2] - start[2]);
            (x * x + z * z).sqrt()
        };
        targets.sort_by(|a, b| distance(&a.1).total_cmp(&distance(&b.1)));
        // Each run twice: the rider against scenery and his bike, and
        // against terrain only as before #751.
        let (mut checked, mut stopped) = (0, 0);
        for (kind, bounds) in targets.iter().take(40) {
            for quarter in 0..4 {
                let angle = quarter as f32 * std::f32::consts::FRAC_PI_2;
                let mut run = |on: bool| {
                    ride.motorcycle_mut().unwrap().set_rider_scene(on);
                    ride_into(&mut ride, bounds.center, 45., angle)
                };
                let (Some(with), Some(without)) = (run(true), run(false)) else {
                    continue;
                };
                if !with.wrecked || with.scene == 0 {
                    continue;
                }
                eprintln!(
                    "{kind} at {:.0?} angle {angle}: scene {} bike {}, first-second travel past it {:.1} ft (terrain only {:.1})",
                    bounds.center, with.scene, with.bike, with.beyond, without.beyond
                );
                checked += 1;
                stopped += usize::from(with.beyond < without.beyond - 2.);
                assert!(
                    with.beyond < without.beyond + 1.,
                    "scenery carried the rider farther at the {kind} at {:?}",
                    bounds.center
                );
                break;
            }
            if checked >= 8 {
                break;
            }
        }
        eprintln!("{stopped} of {checked} riders stopped short at scenery");
        assert!(
            checked >= 4,
            "only {checked} runs threw the rider into scenery"
        );
        assert!(
            stopped * 2 >= checked,
            "scenery stopped only {stopped} of {checked} riders"
        );
    });
}

/// `RIDER_SEARCH=1`: scripted runs from the ENV01 start line (as the browser
/// replays them) that throw the rider into scenery, for the browser check.
#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn search_scripted_scenery_crash() {
    if std::env::var_os("RIDER_SEARCH").is_none() {
        return;
    }
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let options = atv_game::venue::LoadOptions {
            weather: atv_game::venue::WeatherChoice::Active(false),
            ..Default::default()
        };
        let venue = atv_game::venue::load_with(&content, "env01", false, options)
            .await
            .unwrap();
        // The pelvis per step after the throw, and the steps touching scenery.
        let ride_script = async |text: &str, on: bool| {
            let script = atv_game::ride::script::Script::parse(text).unwrap();
            let mut ride = atv_game::ride::load_with(&content, &venue, 29, 0, None, |_| {})
                .await
                .unwrap();
            ride.motorcycle_mut().unwrap().set_rider_scene(on);
            let (mut path, mut touching) = (Vec::new(), 0);
            for (step, controls) in script.steps().enumerate() {
                ride.step(controls, 0.).unwrap();
                if let Some(w) = ride.detached_rider_world() {
                    path.push((step + 1, [w[12], w[13], w[14]]));
                    touching +=
                        usize::from(ride.motorcycle().unwrap().frame().rider_scene_contacts > 0);
                }
            }
            (path, touching)
        };
        let only = std::env::var("RIDER_SEARCH")
            .ok()
            .filter(|v| v.starts_with('['));
        let mut scripts = Vec::new();
        for straight in (30..=330).step_by(30) {
            for k in 0..=10 {
                let steer = -1. + 0.2 * k as f32;
                scripts.push(format!(
                    r#"[{{"frames":60,"input":{{}}}},{{"frames":{straight},"input":{{"throttle":1}}}},{{"frames":300,"input":{{"throttle":1,"steer":{steer:.1}}}}}]"#
                ));
            }
        }
        let mut found = Vec::new();
        for text in only.into_iter().chain(scripts) {
            let (with, touching) = ride_script(&text, true).await;
            if touching == 0 {
                continue;
            }
            let (without, _) = ride_script(&text, false).await;
            // How far apart the two riders are one second after the throw.
            let (Some(a), Some(b)) = (with.get(60), without.get(60)) else {
                continue;
            };
            let d = [a.1[0] - b.1[0], a.1[1] - b.1[1], a.1[2] - b.1[2]];
            let apart = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if std::env::var_os("RIDER_PRINT").is_some() {
                for (w, o) in with.iter().zip(&without).step_by(6) {
                    eprintln!("  {} {:.1?} {:.1?}", w.0, w.1, o.1);
                }
            }
            found.push((apart, with[0].0, a.1, touching, text));
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (apart, step, at, touching, text) in found.iter().take(12) {
            eprintln!(
                "thrown at step {step}, {touching} steps on scenery, 1 s later at {at:.1?}, {apart:.1} ft from the terrain-only rider: {text}"
            );
        }
    });
}
