//! Every venue route's keyframed TOY actors resolve against the hosted
//! content package: each record's MCF, PSG and MOT load, every motion binds
//! to its model and poses finitely (research/AMBIENT_ACTORS.md).
//! Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test toy_actors -- --ignored --nocapture
//! `ATV_ACTORS_REPORT=<path>` writes the per-route census as JSON.
use atv_content::{Content, Store};
use atv_game::launch::{RaceVariant, environments, is_race_course};
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

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn every_venue_route_resolves_its_keyframed_actors() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let mut report = Vec::new();
        let (mut total, mut unresolved) = (0, 0);
        let mut sounds = std::collections::BTreeMap::new();
        for environment in environments() {
            let mut routes = vec![(false, RaceVariant::National)];
            if is_race_course(&environment) {
                routes.push((true, RaceVariant::National));
            }
            for (racing, race) in routes {
                let options = LoadOptions {
                    race,
                    ..Default::default()
                };
                let venue = load_with(&content, &environment, racing, options)
                    .await
                    .unwrap_or_else(|e| panic!("{environment} racing={racing}: {e}"));
                let Some(actors) = venue.actors else {
                    report.push(serde_json::json!({
                        "environment": environment, "racing": racing, "actors": 0}));
                    continue;
                };
                let mut actors = actors.borrow_mut();
                total += actors.actors.len();
                // Every loop sound attaches to a node of its model and its
                // emitter stays finite.
                assert_eq!(venue.actor_sounds.len(), actors.sounds.len());
                for sound in &actors.sounds {
                    let model = &actors.models[actors.actors[sound.actor].model];
                    assert!(
                        sound.node.is_some(),
                        "{environment}: {} has no node for its loop sound",
                        model.name
                    );
                    *sounds
                        .entry((
                            sound.definition.sample.to_ascii_lowercase(),
                            model.name.clone(),
                        ))
                        .or_insert(0) += 1;
                }
                for (now, before) in actors.sound_emitters() {
                    assert!(now.iter().chain(&before).all(|v| v.is_finite()));
                }
                unresolved += actors.unresolved.len();
                // Every actor poses finitely over its motion and moves.
                let mut moving = 0;
                let mut first = Vec::new();
                for i in 0..actors.actors.len() {
                    let pose = actors.pose(i).expect("pose");
                    assert!(pose.min.iter().chain(&pose.max).all(|v| v.is_finite()));
                    first.push(pose.nodes.clone());
                }
                for _ in 0..90 {
                    actors.step();
                }
                for (i, before) in first.iter().enumerate() {
                    let pose = actors.pose(i).expect("pose");
                    assert!(pose.nodes.iter().flatten().all(|v| v.is_finite()));
                    if pose.nodes != *before {
                        moving += 1;
                    }
                }
                // Posing every actor (the renderer's worst case, nothing culled).
                let started = std::time::Instant::now();
                for _ in 0..120 {
                    actors.step();
                    for i in 0..actors.actors.len() {
                        std::hint::black_box(actors.pose(i));
                    }
                }
                let pose_us = started.elapsed().as_secs_f64() * 1e6 / 120.;
                let models: Vec<_> = actors
                    .models
                    .iter()
                    .map(|m| {
                        let features: Vec<String> = m
                            .chunks
                            .iter()
                            .map(|c| {
                                let p = c.pass();
                                let s = p.pass.as_ref();
                                format!(
                                    "{}{}{}{}{}{}",
                                    if s.is_some_and(|s| s.blend) { "B" } else { "" },
                                    if s.is_some_and(|s| s.additive) {
                                        "A"
                                    } else {
                                        ""
                                    },
                                    if s.is_some_and(|s| s.prelit) { "P" } else { "" },
                                    if s.is_some_and(|s| s.uv_scroll.is_some()) {
                                        "U"
                                    } else {
                                        ""
                                    },
                                    if s.is_some_and(|s| s.texture_animation.is_some()) {
                                        "T"
                                    } else {
                                        ""
                                    },
                                    if p.scene_alpha_cutoff > 0. { "C" } else { "" },
                                )
                            })
                            .collect();
                        serde_json::json!({"name": m.name, "nodes": m.nodes.len(),
                            "chunks": m.chunks.len(), "features": features})
                    })
                    .collect();
                let clips: Vec<_> = actors
                    .clips
                    .iter()
                    .map(|c| {
                        serde_json::json!({"name": c.name, "duration": c.animation.duration,
                            "looping": c.looping})
                    })
                    .collect();
                let mut kinds = std::collections::BTreeMap::new();
                for a in &actors.actors {
                    *kinds.entry(a.definition.mcf.clone()).or_insert(0) += 1;
                }
                println!(
                    "{environment:8} racing={racing:5} actors={:3} moving={moving:3} pose={pose_us:.1}us unresolved={:?} {:?}",
                    actors.actors.len(),
                    actors.unresolved,
                    kinds
                );
                report.push(serde_json::json!({
                    "environment": environment, "racing": racing,
                    "actors": actors.actors.len(), "moving": moving, "pose_us_per_step": pose_us,
                    "unresolved": actors.unresolved, "kinds": kinds,
                    "models": models, "clips": clips,
                }));
            }
        }
        println!("total actors {total}, unresolved {unresolved}, loop sounds {sounds:?}");
        if let Some(path) = std::env::var_os("ATV_ACTORS_REPORT") {
            std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        }
        assert_eq!(unresolved, 0, "every record resolves");
        assert!(total > 0);
    });
}
