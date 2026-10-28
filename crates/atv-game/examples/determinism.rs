//! Cross-platform determinism check with original content: replays a script
//! (a Freeride, or a race with `--race LAPS CPU SEED`) headlessly and prints
//! the trace fingerprint. Pure Rust (no graphics, audio or input), so it
//! builds for any target, e.g. Linux in Docker or Windows under Wine:
//!
//!   cargo run --release -p atv-game --example determinism -- CONTENT_DIR \
//!     env01 SCRIPT.json [--quad 29] [--race 1 3 12345] [--out TRACE.json] \
//!     [--events EVENTS.json]   # race presentation per step (not fingerprinted)
//!     [--autopilot]            # the race path driver rides for the player
use atv_content::{Content, Store};
use atv_game::ride::{
    Selection,
    race::Settings,
    script::{Script, fingerprint, trace},
};

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "determinism CONTENT_DIR VENUE SCRIPT.json [--quad N] [--race LAPS CPU SEED] [--out TRACE.json] [--events EVENTS.json] [--autopilot]";
    let content = args.next().expect(usage);
    let venue = args.next().expect(usage);
    let script =
        Script::parse(&std::fs::read_to_string(args.next().expect(usage)).expect("read script"))
            .expect("parse script");
    let (mut race, mut out, mut selection) = (None, None, Selection::default());
    let mut events = None;
    let mut autopilot = false;
    while let Some(arg) = args.next() {
        let mut number = || {
            args.next()
                .and_then(|v| v.parse::<u64>().ok())
                .expect(usage)
        };
        match arg.as_str() {
            "--race" => {
                race = Some(Settings {
                    laps: number() as u32,
                    cpu: number() as usize,
                    seed: number(),
                    ..Settings::default()
                })
            }
            "--out" => out = args.next(),
            "--events" => events = args.next(),
            "--quad" => selection.quad = number() as u16,
            "--autopilot" => autopilot = true,
            _ => panic!("{usage}"),
        }
    }
    let started = std::time::Instant::now();
    let traced = block_on(async {
        let content = Content::open(Store::Directory(content.into())).await?;
        trace(&content, &venue, selection, race, &script, autopilot).await
    })
    .expect("trace");
    let steps = traced.records.len() - 1;
    println!(
        "{}-{} {steps} steps fingerprint {:016x} ({:.3} ms/step incl. load)",
        std::env::consts::OS,
        std::env::consts::ARCH,
        fingerprint(&traced.records),
        started.elapsed().as_secs_f64() * 1000. / steps.max(1) as f64
    );
    if traced.cpu_recovery.iter().any(|&(a, s)| a + s > 0) {
        // Port adaptation counters (atv_core::cpu_recovery), per CPU.
        println!("cpu recovery (assists, skips): {:?}", traced.cpu_recovery);
    }
    if let Some(path) = events {
        std::fs::write(path, serde_json::to_vec(&traced.events).unwrap()).expect("write events");
    }
    if let Some(path) = out {
        std::fs::write(path, serde_json::to_vec(&traced.records).unwrap()).expect("write trace");
    }
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
