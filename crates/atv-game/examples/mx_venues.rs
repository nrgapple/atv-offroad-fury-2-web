//! Private-content acceptance: run the actual venue and vehicle loaders for
//! every MX map, retaining no source assets in Git. Host CPU measurements
//! are not browser or physical-phone frame timings.
use atv_content::{Content, Store};
use atv_game::{Result, launch::mx_courses, ride, venue};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let directory = args.next().ok_or("mx_venues CONTENT_DIR [MX_ID ...]")?;
    let filter: Vec<_> = args.collect();
    block_on(async {
        let content = Content::open(Store::Directory(directory.into())).await?;
        let mut count = 0;
        let mut accepted = Vec::new();
        for course in mx_courses::COURSES {
            if !filter.is_empty() && !filter.iter().any(|id| id == course.id) {
                continue;
            }
            let started = std::time::Instant::now();
            let venue = venue::load_mx_candidate(&content, course.id)
                .await
                .map_err(|e| format!("{}: venue: {e}", course.id))?;
            let load_ms = started.elapsed().as_millis();
            if !venue.scenery.unsupported.is_empty() {
                return Err(format!(
                    "{}: scene models failed: {:?}",
                    course.id, venue.scenery.unsupported
                ));
            }
            let [x, _, z] = venue.scene.start_position;
            if !(0. ..=venue.terrain.size_x()).contains(&x)
                || !(0. ..=venue.terrain.size_z()).contains(&z)
            {
                return Err(format!("{}: spawn outside terrain", course.id));
            }
            let mut vehicles = Vec::new();
            for quad in [1, 29] {
                let mut ride = ride::load(&content, &venue, quad, 0, |_| {})
                    .await
                    .map_err(|e| format!("{}: vehicle {quad}: {e}", course.id))?;
                let mut grounded = 0;
                let first = ride.frame().origin;
                let mut max_speed = 0_f32;
                for step in 0..360 {
                    ride.step(
                        &ride::Controls {
                            throttle: if step < 120 { 0. } else { 0.4 },
                            ..Default::default()
                        },
                        0.,
                    )
                    .map_err(|e| format!("{}: vehicle {quad} step {step}: {e}", course.id))?;
                    let frame = ride.frame();
                    if !frame.origin.iter().all(|v| v.is_finite()) || !frame.speed.is_finite() {
                        return Err(format!(
                            "{}: vehicle {quad} nonfinite simulation",
                            course.id
                        ));
                    }
                    grounded += usize::from(frame.grounded.iter().any(|g| *g));
                    max_speed = max_speed.max(frame.speed.abs());
                }
                if grounded == 0 {
                    return Err(format!(
                        "{}: vehicle {quad} never contacted ground",
                        course.id
                    ));
                }
                let last = ride.frame().origin;
                let travelled = (last[0] - first[0]).hypot(last[2] - first[2]);
                if max_speed < 1. || travelled < 1. {
                    return Err(format!(
                        "{}: vehicle {quad} failed to drive from spawn",
                        course.id
                    ));
                }
                vehicles.push(serde_json::json!({"quad": quad, "grounded_steps": grounded,
                    "max_speed": max_speed, "travelled": travelled, "final": last}));
            }
            println!(
                "{}",
                serde_json::json!({
                    "id": course.id, "host_load_ms": load_ms,
                    "grid": [venue.terrain.width, venue.terrain.depth],
                    "layers": venue.terrain.layers.len(), "models": venue.scene.models.len(),
                    "chunks": venue.scenery.chunks.len(), "draws": venue.scenery.draws.len(),
                    "unsupported_models": venue.scenery.unsupported,
                    "skipped_shader_groups": venue.scenery.skipped_groups,
                    "cpu_texture_bytes": venue.terrain.layers.iter().map(|l| l.rgba.len()).sum::<usize>(),
                    "cpu_terrain_mip_bytes": venue.terrain.layer_mips.iter().flatten().map(Vec::len).sum::<usize>(),
                    "cpu_height_bytes": venue.terrain.heights.len() * std::mem::size_of::<f32>(),
                    "vehicles": vehicles, "steps_each": 360,
                })
            );
            let digest = content
                .manifest
                .record(course.archive)
                .and_then(|r| r.sha256.as_deref())
                .ok_or("MX package lacks whole-file digest")?;
            accepted.push(serde_json::json!({
                "id": course.id, "sha256": digest, "atv": true, "motorcycle": true,
                "steps_each": 360, "unsupported_models": 0,
            }));
            count += 1;
        }
        if count == 0 {
            return Err("no MX venues selected".into());
        }
        if let Ok(path) = std::env::var("ATV_MX_ACCEPTANCE") {
            if count != mx_courses::COURSES.len() {
                return Err("a publication acceptance report requires all MX venues".into());
            }
            let adapter = content
                .manifest
                .record("/MX/ADAPTER.RES")
                .and_then(|r| r.sha256.as_deref())
                .ok_or("MX adapter lacks whole-file digest")?;
            let report =
                serde_json::json!({"version": 1, "adapter_sha256": adapter, "maps": accepted});
            let temporary = format!("{path}.tmp");
            std::fs::write(
                &temporary,
                serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        }
        eprintln!("Validated {count} MX venues with ATV and motorcycle starts/contact");
        Ok(())
    })
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::{
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };
    struct Local;
    impl Wake for Local {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Local));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}
