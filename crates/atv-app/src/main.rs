//! Desktop entry point. Content comes from, in order: a hosted package
//! directory (`--content DIR` or FURY_CONTENT), a content pack file
//! (`--pack FILE` or FURY_PACK), the pack compiled in (`embedded-content`) or
//! appended to the executable, or a `content.pack` beside it (macOS:
//! `Contents/Resources`).
//! Nothing is read from Git.
use atv_app::{Options, run};
use atv_content::{Store, pack};
use std::path::PathBuf;

const USAGE: &str = "usage: fury [--content DIR | --pack FILE] [--venue ID] [--free] \
[--camera x,y,z,yaw,pitch,focal] [--weather on|off] [--steps N] [--graphics original|enhanced|high] \
[--script IN.json] [--trace OUT.json] [--capture OUT.png WxH] [--verify-content] [--version]";

fn fail(message: &str) -> ! {
    eprintln!("fury: {message}");
    std::process::exit(2)
}

/// The first available content source; `Err` explains what to do.
fn choose_store(content: Option<PathBuf>, pack_file: Option<PathBuf>) -> Result<Store, String> {
    if let Some(dir) = content {
        return Ok(Store::Directory(dir));
    }
    if let Some(file) = pack_file {
        return Ok(Store::Pack(pack::Pack::open(&file)?));
    }
    if let Some(embedded) = pack::embedded() {
        return Ok(Store::Pack(embedded?));
    }
    let exe = std::env::current_exe().unwrap_or_default();
    if let Some(attached) = pack::Pack::open_attached(&exe)? {
        return Ok(Store::Pack(attached));
    }
    if let Some(file) = pack::sidecar_candidates(&exe)
        .into_iter()
        .find(|path| path.is_file())
    {
        return Ok(Store::Pack(pack::Pack::open(&file)?));
    }
    Err(
        "no game content found. Use a release build with the content packed in, or pass \
--content DIR (a hosted content package), --pack FILE, or set FURY_CONTENT / FURY_PACK."
            .into(),
    )
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut content = std::env::var_os("FURY_CONTENT").map(PathBuf::from);
    let mut pack_file = std::env::var_os("FURY_PACK").map(PathBuf::from);
    let mut venue = "env01".to_string();
    let mut capture = None;
    let mut verify = false;
    let mut camera = None;
    let mut weather = None;
    let mut steps = None;
    let mut graphics = atv_render::Graphics::default();
    let mut ride = true;
    let mut script = None;
    let mut trace = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--content" => content = args.next().map(PathBuf::from),
            "--pack" => pack_file = args.next().map(PathBuf::from),
            "--verify-content" => verify = true,
            "--version" => {
                println!("fury {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            "--venue" => venue = args.next().expect("--venue needs an id"),
            "--camera" => {
                camera = Some(
                    atv_app::parse_camera(&args.next().unwrap_or_default())
                        .expect("--camera needs x,y,z,yaw,pitch,focal"),
                )
            }
            "--weather" => {
                weather = Some(
                    atv_app::parse_switch(&args.next().unwrap_or_default())
                        .expect("--weather needs on or off"),
                )
            }
            "--steps" => {
                steps = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .expect("--steps needs a count"),
                )
            }
            "--graphics" => graphics = atv_app::graphics_preset(&args.next().unwrap_or_default()),
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
            other => fail(&format!("unknown argument {other}\n{USAGE}")),
        }
    }
    if !atv_game::launch::environments().contains(&venue) {
        panic!("unknown venue {venue}");
    }
    if let Some(out) = trace {
        // Headless replay for parity traces: no window or GPU, package directory only.
        let content = content.unwrap_or_else(|| PathBuf::from("content"));
        let script = script.unwrap_or_else(|| atv_app::replay::Script::parse("[]").unwrap());
        atv_app::replay::trace(&content, &venue, &script, &out).expect("replay trace");
        return;
    }
    let store = choose_store(content, pack_file).unwrap_or_else(|e| fail(&e));
    if verify {
        return verify_content(store);
    }
    run(Options {
        store,
        venue,
        camera,
        weather,
        steps,
        graphics,
        capture,
        ride,
        script,
    });
}

/// Reads every file in the content source, checking each chunk against the
/// manifest, and exits with an error on the first mismatch. No window or GPU.
fn verify_content(store: Store) {
    let started = std::time::Instant::now();
    let result = pollster::block_on(async {
        let content = atv_content::Content::open(store).await?;
        let mut bytes = 0;
        for record in &content.manifest.files {
            bytes += content.file(&record.path).await?.len();
        }
        Ok::<_, String>((content.manifest.files.len(), bytes))
    });
    match result {
        Ok((files, bytes)) => println!(
            "content ok: {files} files, {:.1} MB verified in {:.2}s",
            bytes as f64 / 1e6,
            started.elapsed().as_secs_f64()
        ),
        Err(e) => fail(&format!("content check failed: {e}")),
    }
}
