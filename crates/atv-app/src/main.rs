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
            other => panic!(
                "unknown argument {other}; use --content DIR --venue ID [--camera x,y,z,yaw,pitch,focal] [--capture OUT.png WxH]"
            ),
        }
    }
    if !atv_game::launch::environments().contains(&venue) {
        panic!("unknown venue {venue}");
    }
    let content = content.unwrap_or_else(|| PathBuf::from("content"));
    run(Options {
        store: Store::Directory(content),
        venue,
        camera,
        capture,
    });
}
