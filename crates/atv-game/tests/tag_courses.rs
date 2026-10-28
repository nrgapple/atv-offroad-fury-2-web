//! Every original Tag course (`[tag1]`-`[tag14]`, the venues' `*_TG.RES`)
//! loads from the hosted content package: the ride reads its TG archive
//! over the Freeride archive, starts at the Tag mode TOY's
//! `DefaultStartPosition`, and its `tagball.psg` resolves into drawn
//! scenery with nothing unsupported; on each course's heightfield the
//! boundary throw-in and the ground respawn land the ball back inside
//! (research/GAMES.md "Tag").
//! Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test tag_courses -- --ignored --nocapture
use atv_content::{Content, Store};
use atv_core::games::{GameKind, game_archive, mode_toy};
use atv_core::tag::{BALL_RADIUS, Ball, Bounds, boundary_margin, respawn_search};
use atv_game::venue::{LoadOptions, load_with};
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

const VENUES: [&str; 14] = [
    "env05", "env02", "env08", "env06", "env04", "env01", "env03", "free01", "free04", "free02",
    "free06", "free03", "free05", "env07",
];

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn every_tag_course_loads_its_start_and_ball() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        for venue in VENUES {
            let path = game_archive(GameKind::Tag, venue).unwrap();
            assert!(content.has(&path), "{path} is not in the package");
            // The mode TOY's start, read straight from the TG archive.
            let bytes = content.file(&path).await.unwrap();
            let archive = atv_core::resource::Resource::parse(&bytes).unwrap();
            let name = mode_toy(GameKind::Tag, venue);
            let index = archive
                .members
                .iter()
                .position(|m| m.name.to_ascii_lowercase().ends_with(&name))
                .unwrap_or_else(|| panic!("{path} lacks {name}"));
            let toy = String::from_utf8_lossy(&archive.extract(index).unwrap()).into_owned();
            let start: Vec<f32> = toy
                .lines()
                .find_map(|l| l.trim().strip_prefix("DefaultStartPosition="))
                .unwrap_or_else(|| panic!("{name} has no start"))
                .split(',')
                .map(|v| v.trim().parse().unwrap())
                .collect();
            let options = LoadOptions {
                game_scene: Some((path.clone(), format!("{venue}tg"))),
                ..Default::default()
            };
            let loaded = load_with(&content, venue, false, options)
                .await
                .unwrap_or_else(|e| panic!("{venue}: {e}"));
            assert!(loaded.tag_ball, "{venue}: no tagball");
            for (a, b) in loaded.scene.start_position.iter().zip(&start) {
                assert!(
                    (a - b).abs() < 1e-3,
                    "{venue}: start {:?} vs {start:?}",
                    loaded.scene.start_position
                );
            }
            let ball = loaded
                .scene
                .models
                .iter()
                .position(|m| m.name == "tagball")
                .unwrap_or_else(|| panic!("{venue}: no tagball model"));
            assert!(
                loaded
                    .scenery
                    .draws
                    .iter()
                    .any(|d| d.model == ball && d.gate),
                "{venue}: tagball draws nothing"
            );
            assert!(
                loaded.scenery.unsupported.is_empty(),
                "{venue}: unsupported {:?}",
                loaded.scenery.unsupported
            );
            // The boundary and the ground respawn on this course's
            // heightfield: the start lies inside the 110-cell margin, a
            // ball past the west margin is thrown back inside, and the
            // respawn from the start lands inside (`0020aec0`, `00246ec0`,
            // `00246500`).
            let terrain = &loaded.terrain;
            let bounds = Bounds::from_cells(
                (terrain.width - 1) as u32,
                (terrain.depth - 1) as u32,
                terrain.spacing,
            );
            // Bilinear over the render heightfield (the ride's contact
            // planes need a ride; this is within a fraction of a unit).
            let ground = |x: f32, z: f32| {
                let fx = (x / terrain.spacing).clamp(0., (terrain.width - 2) as f32);
                let fz = (z / terrain.spacing).clamp(0., (terrain.depth - 2) as f32);
                let (ix, iz) = (fx as usize, fz as usize);
                let (tx, tz) = (fx - ix as f32, fz - iz as f32);
                let h = |x: usize, z: usize| terrain.heights[z * terrain.width + x];
                let a = h(ix, iz) * (1. - tx) + h(ix + 1, iz) * tx;
                let b = h(ix, iz + 1) * (1. - tx) + h(ix + 1, iz + 1) * tx;
                a * (1. - tz) + b * tz
            };
            let s = loaded.scene.start_position;
            let margin = boundary_margin(loaded.scene.crowd_present);
            assert!(!bounds.outside(s, margin), "{venue}: start outside");
            let fly = |mut ball: Ball| {
                let mut throws = 0;
                for _ in 0..600 {
                    if bounds.outside(ball.position, margin) {
                        ball = Ball::throw_in(ball.position, bounds.centre());
                        throws += 1;
                    }
                    ball.step(1. / 60., ground, None, false);
                }
                (ball, throws)
            };
            // Past the margin (past the edge itself in a stadium).
            let west = [bounds.cell[0] * (margin * 0.4 - 1.), s[1] + 20., s[2]];
            let (thrown, throws) = fly(Ball {
                position: west,
                velocity: [0.; 3],
            });
            assert!(throws >= 1, "{venue}: no throw-in");
            assert!(
                !bounds.outside(thrown.position, margin),
                "{venue}: throw-in ends at {:?}",
                thrown.position
            );
            let from = [s[0], ground(s[0], s[2]) + BALL_RADIUS, s[2]];
            let cast = |a: [f32; 3], b: [f32; 3]| {
                let steps = 300;
                (1..=steps).find_map(|k| {
                    let t = k as f32 / steps as f32;
                    let p = [a[0] + (b[0] - a[0]) * t, a[1], a[2] + (b[2] - a[2]) * t];
                    (ground(p[0], p[2]) >= p[1]).then_some(p)
                })
            };
            let hit = respawn_search(from, 0., cast);
            let respawn = match hit {
                Some(hit) => Ball::respawn_throw(from, hit, [0.5; 3]),
                None => Ball::respawn_start(s, ground(s[0], s[2])),
            };
            let (landed, _) = fly(respawn);
            assert!(
                !bounds.outside(landed.position, margin),
                "{venue}: respawn ends at {:?}",
                landed.position
            );
            println!(
                "{venue}: crowd {}, respawn {}, throw-in lands {:?}",
                loaded.scene.crowd_present,
                if hit.is_some() { "ray hit" } else { "start" },
                thrown.position.map(|v| v.round())
            );
            println!(
                "{venue}: start {:?}, tagball model {ball}, {} draws",
                loaded.scene.start_position,
                loaded.scenery.draws.len()
            );
        }
    });
}
