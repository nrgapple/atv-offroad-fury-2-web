//! Measures what the race path driver does on each race course, for the
//! World Hall of Fame house riders and lap floors (research/WORLD_BOARDS.md).
//! Runs full-length races headlessly with original content and appends one
//! JSON line per race: every rider's lap times, best lap and finish time
//! (estimates marked), the CPU roster and, with `--autopilot`, the player's
//! world board trace.
//!
//!   cargo run --release -p atv-game --example cpu_times -- CONTENT_DIR \
//!     OUT.jsonl [--courses env01,sx03] [--difficulty 1|2] [--autopilot] \
//!     [--quad N] [--seed N] [--minutes 45] [--laps N]
//!
//! `--autopilot`: the driver also rides the player, so the CPUs race a
//! rider of their own pace (the race ends when the player finishes, as in
//! the game; riders still out get estimates). Without it the player waits
//! on the grid and every CPU rides the whole race.
use atv_content::{Content, Store};
use atv_core::hall_of_fame::RecordWatch;
use atv_game::launch::{RaceVariant, environments, race_course_id, split_race_course};
use atv_game::ride::{Controls, race::Settings};
use serde_json::{Value, json};
use std::io::Write;

const USAGE: &str = "cpu_times CONTENT_DIR OUT.jsonl [--courses a,b] [--difficulty 1|2] [--autopilot] [--quad N] [--seed N] [--minutes N] [--laps N]";

struct Options {
    difficulty: u32,
    autopilot: bool,
    quad: u16,
    seed: u64,
    minutes: u32,
    /// Laps when not the course's default (vehicle comparisons).
    laps: Option<u32>,
}

async fn measure(content: &Content, course: &str, options: &Options) -> atv_game::Result<Value> {
    let (environment, variant) = split_race_course(course).ok_or("not a race course")?;
    let full = atv_core::world_board::default_laps(course);
    let laps = options.laps.unwrap_or(full);
    let settings = Settings {
        laps,
        cpu: 4,
        difficulty: options.difficulty,
        seed: options.seed,
        intro: false,
        course: variant,
        ..Settings::default()
    };
    let load = atv_game::venue::LoadOptions {
        weather: atv_game::venue::WeatherChoice::Active(false),
        race: variant,
        ..Default::default()
    };
    let venue = atv_game::venue::load_with(content, environment, true, load).await?;
    let mut ride =
        atv_game::ride::load_with(content, &venue, options.quad, 0, Some(settings), |_| {}).await?;
    let state = ride.race_mut().ok_or("no race")?;
    state.autopilot_always = options.autopilot;
    // The record watch switches the world board trace on.
    state.set_records(RecordWatch::new(
        course,
        options.difficulty == 2,
        options.quad,
        laps == full,
        None,
        None,
    ));
    let roster: Vec<Value> = state
        .roster
        .iter()
        .map(|e| json!({"name": e.name, "quad": e.quad}))
        .collect();
    let controls = Controls::default();
    // Each rider's lap marks (clock seconds), the player first.
    let mut marks: Vec<Vec<f32>> = Vec::new();
    let mut steps = 0u64;
    let limit = u64::from(options.minutes) * 60 * 60;
    let done = loop {
        ride.step(&controls, 0.)?;
        steps += 1;
        let Some(frame) = ride.race().and_then(|r| r.frame.as_ref()) else {
            continue;
        };
        let mut order: Vec<&atv_core::race::Standing> = frame.standings.iter().collect();
        order.sort_by_key(|s| (!s.player, s.name.clone()));
        marks.resize(order.len(), Vec::new());
        for (rider, standing) in marks.iter_mut().zip(&order) {
            // A race-over estimate completes the laps without riding them.
            while (rider.len() as u32) < standing.laps && !standing.estimated {
                rider.push(frame.elapsed);
            }
        }
        let cpus_done = order.iter().filter(|s| !s.player).all(|s| s.time.is_some());
        if frame.phase == "finished" || cpus_done {
            break true;
        }
        if steps >= limit {
            break false;
        }
    };
    let state = ride.race().ok_or("no race")?;
    let frame = state.frame.as_ref().ok_or("no frame")?;
    let mut order: Vec<&atv_core::race::Standing> = frame.standings.iter().collect();
    order.sort_by_key(|s| (!s.player, s.name.clone()));
    let riders: Vec<Value> = order
        .iter()
        .zip(&marks)
        .map(|(s, marks)| {
            json!({"name": s.name, "player": s.player, "laps": s.laps, "time": s.time,
                "best": s.best, "estimated": s.estimated, "marks": marks})
        })
        .collect();
    let recovery: Vec<(u32, u32)> = state
        .cpus
        .iter()
        .map(|c| (c.recovery.assists, c.recovery.skips))
        .collect();
    Ok(json!({
        "course": course, "difficulty": options.difficulty, "autopilot": options.autopilot,
        "quad": options.quad, "seed": options.seed, "laps": laps, "steps": steps,
        "elapsed": frame.elapsed, "done": done, "roster": roster, "riders": riders,
        "resets": ride.frame().reset_count, "recovery": recovery,
        "trace": frame.trace.as_deref(),
    }))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let content = args.next().expect(USAGE);
    let out = args.next().expect(USAGE);
    let mut options = Options {
        difficulty: 1,
        autopilot: false,
        quad: 1,
        seed: 7,
        minutes: 45,
        laps: None,
    };
    let mut courses: Vec<String> = environments()
        .iter()
        .flat_map(|id| {
            RaceVariant::ALL
                .into_iter()
                .filter(|v| v.exists(id))
                .map(|v| race_course_id(id, v))
                .collect::<Vec<_>>()
        })
        .collect();
    while let Some(arg) = args.next() {
        let mut value = || args.next().expect(USAGE);
        match arg.as_str() {
            "--courses" => courses = value().split(',').map(String::from).collect(),
            "--difficulty" => options.difficulty = value().parse().expect(USAGE),
            "--autopilot" => options.autopilot = true,
            "--quad" => options.quad = value().parse().expect(USAGE),
            "--seed" => options.seed = value().parse().expect(USAGE),
            "--minutes" => options.minutes = value().parse().expect(USAGE),
            "--laps" => options.laps = Some(value().parse().expect(USAGE)),
            _ => panic!("{USAGE}"),
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out)
        .expect("open output");
    block_on(async {
        let content = Content::open(Store::Directory(content.into()))
            .await
            .expect("content");
        for course in &courses {
            let started = std::time::Instant::now();
            match measure(&content, course, &options).await {
                Ok(run) => {
                    writeln!(file, "{run}").expect("write");
                    eprintln!(
                        "{course} d{} {} steps in {:.1}s",
                        options.difficulty,
                        run["steps"],
                        started.elapsed().as_secs_f32()
                    );
                }
                Err(error) => eprintln!("{course}: {error}"),
            }
        }
    });
}

/// A minimal executor: content reads from a directory never suspend.
fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn raw() -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| raw(), |_| {}, |_| {}, |_| {});
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}
