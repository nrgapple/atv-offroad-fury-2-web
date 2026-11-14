//! fury-loadtest: simulated riders against the multiplayer game server,
//! fury-server (research/GAME_SERVER.md "Load testing").
//! Each rider claims a Free Ride slot or a private room over the HTTP
//! routes, joins over the room WebSocket and sends compact poses at an
//! adaptive cadence while it counts what comes back. The report is JSON.
// A test tool, not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]
mod rider;
mod stats;

use rider::{Plan, Rooms};
use stats::Stats;
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

const USAGE: &str = "usage: fury-loadtest [--url ORIGIN] [--origin ORIGIN] [--riders N] [--rooms N] \
[--duration S] [--ramp S] [--frames auto|on|off] [--clustered F] [--ping-ms MS] [--seed N] [--out FILE]

  --url        server origin (default http://127.0.0.1:8789)
  --origin     page Origin header presented (default: the --url origin)
  --riders     simulated riders (default 8)
  --rooms      private rooms (up to 4 riders each); 0 = Arlington Free Ride (default)
  --duration   measured seconds after ramp-up (default 30)
  --ramp       seconds over which riders arrive (default 5)
  --frames     say signal::FRAMES: auto = when the room claim says frames (default)
  --clustered  fraction of riders riding near each other (default 0.5)
  --ping-ms    ping interval per rider (default 5000)
  --seed       path randomness (default 1)
  --out        write the JSON report here instead of stdout";

/// Command-line settings.
#[derive(Clone, Debug)]
pub struct Args {
    pub url: String,
    pub origin: String,
    pub riders: usize,
    pub rooms: usize,
    pub duration: f64,
    pub ramp: f64,
    pub frames: Frames,
    pub clustered: f64,
    pub ping_ms: u64,
    pub seed: u64,
    pub out: Option<String>,
}

/// When riders say [`atv_net::signal::FRAMES`] in their hello.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frames {
    /// When the room claim says `"frames": true` (as the game does).
    Auto,
    /// Always (a server that does not decode it closes the socket).
    On,
    Off,
}

fn fail(message: &str) -> ! {
    eprintln!("fury-loadtest: {message}\n{USAGE}");
    std::process::exit(2);
}

fn parse(mut args: impl Iterator<Item = String>) -> Args {
    let mut parsed = Args {
        url: "http://127.0.0.1:8789".into(),
        origin: String::new(),
        riders: 8,
        rooms: 0,
        duration: 30.,
        ramp: 5.,
        frames: Frames::Auto,
        clustered: 0.5,
        ping_ms: 5000,
        seed: 1,
        out: None,
    };
    fn number<T: std::str::FromStr>(flag: &str, value: Option<String>) -> T {
        value
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| fail(&format!("{flag} needs a number")))
    }
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--url" => parsed.url = args.next().unwrap_or_else(|| fail("--url needs a URL")),
            "--origin" => {
                parsed.origin = args.next().unwrap_or_else(|| fail("--origin needs a URL"))
            }
            "--riders" => parsed.riders = number(&flag, args.next()),
            "--rooms" => parsed.rooms = number(&flag, args.next()),
            "--duration" => parsed.duration = number(&flag, args.next()),
            "--ramp" => parsed.ramp = number(&flag, args.next()),
            "--clustered" => parsed.clustered = number(&flag, args.next()),
            "--ping-ms" => parsed.ping_ms = number(&flag, args.next()),
            "--seed" => parsed.seed = number(&flag, args.next()),
            "--out" => parsed.out = args.next(),
            "--frames" => {
                parsed.frames = match args.next().as_deref() {
                    Some("auto") => Frames::Auto,
                    Some("on") => Frames::On,
                    Some("off") => Frames::Off,
                    _ => fail("--frames is auto, on or off"),
                }
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => fail(&format!("unknown option {other}")),
        }
    }
    parsed.url = parsed.url.trim_end_matches('/').to_string();
    if !parsed.url.starts_with("http://") && !parsed.url.starts_with("https://") {
        fail("--url is an http(s) origin");
    }
    if parsed.origin.is_empty() {
        parsed.origin = origin(&parsed.url);
    }
    if parsed.riders == 0 || parsed.duration <= 0. || parsed.ramp < 0. || parsed.ping_ms == 0 {
        fail("--riders, --duration and --ping-ms must be positive");
    }
    if parsed.rooms > 0 && parsed.riders > parsed.rooms * atv_net::PRIVATE_CAPACITY {
        fail("a private room holds 4 riders: raise --rooms");
    }
    parsed.clustered = parsed.clustered.clamp(0., 1.);
    parsed
}

/// A URL's scheme and host.
fn origin(url: &str) -> String {
    let start = url.find("://").map_or(0, |i| i + 3);
    match url[start..].find('/') {
        Some(i) => url[..start + i].to_string(),
        None => url.to_string(),
    }
}

#[tokio::main]
async fn main() {
    let args = parse(std::env::args().skip(1));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let stats = Arc::new(Stats::default());
    let rooms = Arc::new(Rooms::new(args.rooms));
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let started = Instant::now();
    let plans = Plan::all(&args);
    let mut riders = Vec::with_capacity(plans.len());
    let spacing = if plans.len() > 1 {
        args.ramp / (plans.len() - 1) as f64
    } else {
        0.
    };
    for (i, plan) in plans.into_iter().enumerate() {
        let (args, stats, rooms, stopped) =
            (args.clone(), stats.clone(), rooms.clone(), stopped.clone());
        let at = started + Duration::from_secs_f64(spacing * i as f64);
        riders.push(tokio::spawn(async move {
            tokio::time::sleep_until(at.into()).await;
            rider::ride(plan, &args, &stats, &rooms, stopped).await;
        }));
    }
    // Measure from the end of the ramp-up.
    tokio::time::sleep_until((started + Duration::from_secs_f64(args.ramp)).into()).await;
    let first = stats.window_start();
    tokio::time::sleep(Duration::from_secs_f64(args.duration)).await;
    let last = stats.window_end();
    let _ = stop.send(true);
    for rider in riders {
        let _ = tokio::time::timeout(Duration::from_secs(5), rider).await;
    }
    let report = stats.report(&args, &first, &last);
    let text = serde_json::to_string_pretty(&report).expect("report");
    match &args.out {
        Some(path) => std::fs::write(path, text + "\n").unwrap_or_else(|e| fail(&e.to_string())),
        None => println!("{text}"),
    }
    if stats.connect_ok.load(Ordering::Relaxed) == 0 {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_to_the_local_server_and_check_rooms() {
        let args = parse(std::iter::empty());
        assert_eq!(args.url, "http://127.0.0.1:8789");
        assert_eq!(args.origin, "http://127.0.0.1:8789");
        let args = parse(
            [
                "--url",
                "https://mp.furyremastered.com/",
                "--origin",
                "https://furyremastered.com",
                "--riders",
                "12",
                "--rooms",
                "3",
                "--frames",
                "on",
            ]
            .map(String::from)
            .into_iter(),
        );
        assert_eq!(args.url, "https://mp.furyremastered.com");
        assert_eq!(args.origin, "https://furyremastered.com");
        assert_eq!((args.riders, args.rooms, args.frames), (12, 3, Frames::On));
        assert_eq!(
            origin("http://127.0.0.1:4187/play.html"),
            "http://127.0.0.1:4187"
        );
    }
}
