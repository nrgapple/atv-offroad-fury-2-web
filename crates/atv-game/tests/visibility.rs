//! Every venue route's static models resolve the original visibility policy
//! (research/VISIBILITY.md): each placed model has its stored LOD table
//! (finest first, last level FLT_MAX), every level loads geometry, later
//! passes of `EnableMultiPassCull` materials are marked for the 130 ft cull,
//! and every zone reference lies in the scene's single zone.
//! Needs the private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test visibility -- --ignored --nocapture
//! `ATV_VISIBILITY_REPORT=<path>` writes the per-route census as JSON.
use atv_content::{Content, Store};
use atv_core::model_lod;
use atv_game::launch::{RaceVariant, environments, is_race_course};
use atv_game::venue::{LoadOptions, load_with};
use std::collections::BTreeMap;
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
fn every_venue_route_resolves_its_visibility_policy() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    // The native follow camera's 85 degree horizontal field of view.
    let focal = model_lod::focal_from_fov(85.);
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        let mut report = Vec::new();
        let mut unresolved = Vec::new();
        let (mut routes, mut placements, mut multi) = (0, 0, 0);
        let mut names: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for environment in environments() {
            let mut loads = vec![(false, RaceVariant::National)];
            if is_race_course(&environment) {
                loads.extend(RaceVariant::ALL.map(|v| (true, v)));
            }
            for (racing, race) in loads {
                let options = LoadOptions {
                    race,
                    ..Default::default()
                };
                let venue = match load_with(&content, &environment, racing, options).await {
                    Ok(venue) => venue,
                    // Not every venue ships every race variant.
                    Err(_) if racing && race != RaceVariant::National => continue,
                    Err(e) => panic!("{environment} racing={racing}: {e}"),
                };
                routes += 1;
                let route = format!("{environment}{}", if racing { race.stem() } else { "" });
                let scenery = &venue.scenery;
                let scene = &venue.scene;
                assert_eq!(scenery.lods.len(), scene.models.len(), "{route}");
                for (name, error) in &scenery.unsupported {
                    unresolved.push(format!("{route}: {name}: {error}"));
                }
                if scene.zones != 1 || scene.portals != 0 {
                    unresolved.push(format!(
                        "{route}: {} zones, {} portals",
                        scene.zones, scene.portals
                    ));
                }
                for outside in &venue.zones.outside {
                    unresolved.push(format!("{route}: {}={}", outside.key, outside.value));
                }
                let mut levels = [0usize; 4];
                let mut route_multi = 0;
                for (index, model) in scene.models.iter().enumerate() {
                    placements += 1;
                    let Some(lod) = &scenery.lods[index] else {
                        continue;
                    };
                    let n = lod.thresholds.len();
                    levels[(n - 1).min(3)] += 1;
                    if n > 1 {
                        route_multi += 1;
                        multi += 1;
                    }
                    let sorted = lod.thresholds.windows(2).all(|w| w[0] < w[1]);
                    if !sorted || lod.thresholds[n - 1] != f32::MAX {
                        unresolved.push(format!(
                            "{route}: {} thresholds {:?}",
                            model.name, lod.thresholds
                        ));
                    }
                    if !lod.radius.is_finite() || lod.radius < 0. {
                        unresolved.push(format!("{route}: {} radius {}", model.name, lod.radius));
                    }
                    // Every level of a drawn model has geometry.
                    let drawn: Vec<u8> = scenery
                        .draws
                        .iter()
                        .filter(|d| d.model == index)
                        .map(|d| d.lod)
                        .collect();
                    if !drawn.is_empty() {
                        for level in 0..n as u8 {
                            if !drawn.contains(&level) {
                                unresolved
                                    .push(format!("{route}: {} level {level} empty", model.name));
                            }
                        }
                    }
                    names.entry(model.name.to_ascii_lowercase()).or_insert_with(|| {
                        serde_json::json!({
                            "thresholds": lod.thresholds.iter().map(|t| if *t == f32::MAX { -1. } else { *t }).collect::<Vec<_>>(),
                            "radius": lod.radius,
                            "switchFeet": lod.switch_distances(focal),
                        })
                    });
                }
                let far_cull = scenery.draws.iter().filter(|d| d.far_cull).count();
                let far_models: std::collections::BTreeSet<_> = scenery
                    .draws
                    .iter()
                    .filter(|d| d.far_cull)
                    .map(|d| scene.models[d.model].name.to_ascii_lowercase())
                    .collect();
                report.push(serde_json::json!({
                    "route": route,
                    "placements": scene.models.len(),
                    "levels": levels,
                    "multiLevel": route_multi,
                    "farCullDraws": far_cull,
                    "farCullModels": far_models,
                    "zones": scene.zones, "portals": scene.portals,
                    "zoneRecords": venue.zones.records,
                    "zonesOutside": venue.zones.outside.len(),
                }));
            }
        }
        println!(
            "{routes} routes, {placements} placements, {multi} multi-level placements, {} model names; unresolved {}",
            names.len(),
            unresolved.len()
        );
        if let Some(path) = std::env::var_os("ATV_VISIBILITY_REPORT") {
            let full = serde_json::json!({"focal": focal, "routes": report, "models": names, "unresolved": unresolved});
            std::fs::write(path, serde_json::to_string_pretty(&full).unwrap()).unwrap();
        }
        assert!(unresolved.is_empty(), "{unresolved:#?}");
        assert!(routes >= 40 + 32, "{routes} routes");
        assert!(multi > 0);
    });
}
