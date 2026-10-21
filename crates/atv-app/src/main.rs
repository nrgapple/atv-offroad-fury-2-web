//! Desktop entry point. Content comes from a hosted package directory
//! (`--content DIR` or FURY_CONTENT); nothing is read from Git.
use atv_app::{Options, run};
use atv_content::Store;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut content = std::env::var_os("FURY_CONTENT").map(PathBuf::from);
    let mut venue = "env01".to_string();
    let mut capture = None;
    let mut camera = None;
    let mut ride = true;
    let mut script = None;
    let mut trace = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--content" => content = args.next().map(PathBuf::from),
            "--venue" => venue = args.next().expect("--venue needs an id"),
            "--camera" => {
                camera = Some(
                    atv_app::parse_camera(&args.next().unwrap_or_default())
                        .expect("--camera needs x,y,z,yaw,pitch,focal"),
                )
            }
            "--capture" => {
                let path = args.next().expect("--capture needs a PNG path");
                let size = args.next().unwrap_or_else(|| "1280x720".into());
                let (w, h) = size.split_once('x').expect("capture size is WIDTHxHEIGHT");
                capture = Some((PathBuf::from(path), w.parse().unwrap(), h.parse().unwrap()));
            }
            "--free" => ride = false,
            "--script" => {
                let path = args.next().expect("--script needs a JSON file");
                let text = std::fs::read_to_string(&path).expect("read script");
                // Accept the browser tool's script file unchanged.
                script = Some(atv_app::replay::Script::parse(&text).expect("parse script"));
            }
            "--trace" => trace = Some(PathBuf::from(args.next().expect("--trace needs a path"))),
            other => panic!(
                "unknown argument {other}; use --content DIR --venue ID [--free] [--camera x,y,z,yaw,pitch,focal] [--script IN.json] [--trace OUT.json] [--capture OUT.png WxH]"
            ),
        }
    }
    if !atv_game::launch::environments().contains(&venue) {
        panic!("unknown venue {venue}");
    }
    let content = content.unwrap_or_else(|| PathBuf::from("content"));
    if let Some(out) = trace {
        // Headless replay for parity traces: no window or GPU.
        let script = script.unwrap_or_else(|| atv_app::replay::Script::parse("[]").unwrap());
        atv_app::replay::trace(&content, &venue, &script, &out).expect("replay trace");
        return;
    }
    run(Options {
        store: Store::Directory(content),
        venue,
        camera,
        capture,
        ride,
        script,
    });
}
