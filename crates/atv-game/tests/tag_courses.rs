//! Every original Tag course (`[tag1]`-`[tag14]`, the venues' `*_TG.RES`)
//! loads from the hosted content package: the ride reads its TG archive
//! over the Freeride archive, starts at the Tag mode TOY's
//! `DefaultStartPosition`, and its `tagball.psg` resolves into drawn
//! scenery with nothing unsupported (research/GAMES.md "Tag").
//! Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test tag_courses -- --ignored --nocapture
use atv_content::{Content, Store};
use atv_core::games::{GameKind, game_archive, mode_toy};
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
            println!(
                "{venue}: start {:?}, tagball model {ball}, {} draws",
                loaded.scene.start_position,
                loaded.scenery.draws.len()
            );
        }
    });
}
