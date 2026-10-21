//! Desktop entry point. Content comes from, in order: a hosted package
//! directory (`--content DIR` or FURY_CONTENT), a content pack file
//! (`--pack FILE` or FURY_PACK), the pack compiled in (`embedded-content`) or
//! appended to the executable, or a `content.pack` beside it (macOS:
//! `Contents/Resources`).
//! Nothing is read from Git.
use atv_app::{Options, run};
use atv_content::{Store, pack};
use std::path::PathBuf;

const USAGE: &str = "usage: fury [--content DIR | --pack FILE] [--front PAGE] [--venue ID] [--quad 1-46] [--rider 0|1] [--free] [--race [--laps N] [--cpu 0|3|4] [--difficulty 1|2] [--seed N]] \
[--camera x,y,z,yaw,pitch,focal] [--weather on|off] [--steps N] [--graphics original|enhanced|high] \
[--crash original|crazy|gorn] \
[--script IN.json] [--trace OUT.json] [--present-trace OUT.json [--present-hz 144]] [--capture OUT.png WxH [--ui] [--gpu-frames N]] [--verify-content] [--version] \
[--mute] [--no-music] [--sound original|enhanced] [--audio OUT.wav] \
[--autodrive IN.json [--bench OUT.json [--profile]]]";

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
    let mut venue_given = false;
    let mut front_page: Option<String> = None;
    let mut capture = None;
    let mut capture_ui = false;
    let mut gpu_frames = None;
    let mut verify = false;
    let mut camera = None;
    let mut weather = None;
    let mut steps = None;
    let mut graphics = atv_render::Graphics::default();
    let mut crash = atv_game::crash::CrashMode::Original;
    let mut ride = true;
    let mut selection = atv_game::ride::Selection::default();
    let mut race: Option<atv_game::ride::race::Settings> = None;
    let mut script = None;
    let mut trace = None;
    let (mut present_trace, mut present_hz) = (None, 144.);
    let mut autopilot_script = None;
    let mut audio = None;
    let mut sound = atv_game::audio::ride::SoundMode::Original;
    let mut music = true;
    let mut no_audio = false;
    let mut autodrive = None;
    let mut bench = None;
    let mut profile = false;
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
            "--venue" => {
                venue = args.next().expect("--venue needs an id");
                venue_given = true;
            }
            "--front" => front_page = Some(args.next().expect("--front needs a page")),
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
            "--crash" => {
                crash = args
                    .next()
                    .and_then(|v| atv_game::crash::CrashMode::parse(&v))
                    .expect("--crash needs original, crazy or gorn")
            }
            "--ui" => capture_ui = true,
            "--gpu-frames" => {
                gpu_frames = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .expect("--gpu-frames needs a count"),
                )
            }
            "--capture" => {
                let path = args.next().expect("--capture needs a PNG path");
                let size = args.next().unwrap_or_else(|| "1280x720".into());
                let (w, h) = size.split_once('x').expect("capture size is WIDTHxHEIGHT");
                capture = Some((PathBuf::from(path), w.parse().unwrap(), h.parse().unwrap()));
            }
            "--free" => ride = false,
            "--quad" | "--rider" => {
                let value: u16 = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| fail(&format!("{arg} needs a number")));
                if arg == "--quad" {
                    selection.quad = value;
                } else {
                    selection.rider = value;
                }
            }
            "--race" => race = Some(race.unwrap_or_default()),
            "--laps" | "--cpu" | "--difficulty" | "--seed" => {
                let value: u64 = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| fail(&format!("{arg} needs a number")));
                let settings = race.get_or_insert_with(Default::default);
                match arg.as_str() {
                    "--laps" => settings.laps = value as u32,
                    "--cpu" => settings.cpu = value as usize,
                    "--difficulty" => settings.difficulty = value as u32,
                    _ => settings.seed = value,
                }
            }
            "--script" => {
                let path = args.next().expect("--script needs a JSON file");
                let text = std::fs::read_to_string(&path).expect("read script");
                // Accept the browser tool's script file unchanged.
                script = Some(atv_app::replay::Script::parse(&text).expect("parse script"));
            }
            "--audio" => {
                audio = Some(PathBuf::from(
                    args.next().expect("--audio needs a WAV path"),
                ))
            }
            "--sound" => {
                sound = match args.next().as_deref() {
                    Some("enhanced") => atv_game::audio::ride::SoundMode::Enhanced,
                    Some("original") => atv_game::audio::ride::SoundMode::Original,
                    _ => panic!("--sound needs original or enhanced"),
                }
            }
            "--no-music" => music = false,
            "--autodrive" => {
                let text =
                    std::fs::read_to_string(args.next().expect("--autodrive needs a JSON file"))
                        .expect("read autodrive script");
                autodrive = Some(atv_app::replay::Script::parse(&text).expect("parse autodrive"));
            }
            "--profile" => profile = true,
            "--bench" => bench = Some(PathBuf::from(args.next().expect("--bench needs a path"))),
            "--mute" => no_audio = true,
            "--record-autopilot" => {
                autopilot_script = Some(PathBuf::from(
                    args.next().expect("--record-autopilot needs a path"),
                ))
            }
            "--trace" => trace = Some(PathBuf::from(args.next().expect("--trace needs a path"))),
            "--present-trace" => {
                present_trace = Some(PathBuf::from(
                    args.next().expect("--present-trace needs a path"),
                ))
            }
            "--present-hz" => {
                present_hz = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--present-hz needs a rate")
            }
            other => fail(&format!("unknown argument {other}\n{USAGE}")),
        }
    }
    if !atv_game::launch::environments().contains(&venue) {
        panic!("unknown venue {venue}");
    }
    // Captures render a held frame; they open no audio device.
    no_audio |= capture.is_some();
    if let Some(out) = audio {
        let content = content.unwrap_or_else(|| PathBuf::from("content"));
        // Offline mix of a scripted ride: no window or audio device.
        let script = script.unwrap_or_else(|| atv_app::replay::Script::parse("[]").unwrap());
        atv_app::replay::render_audio(&content, &venue, selection, &script, &out, sound)
            .expect("render audio");
        return;
    }
    if let Some(out) = present_trace {
        // Headless presentation timeline at --present-hz (interpolation parity).
        let content = content.unwrap_or_else(|| PathBuf::from("content"));
        let script = script.unwrap_or_else(|| atv_app::replay::Script::parse("[]").unwrap());
        atv_app::replay::present_trace(
            &content,
            &venue,
            selection,
            race,
            graphics.interpolation,
            &script,
            present_hz,
            &out,
        )
        .expect("presentation trace");
        return;
    }
    if let Some(out) = trace {
        // Headless replay for parity traces: no window or GPU, package directory only.
        let content = content.unwrap_or_else(|| PathBuf::from("content"));
        let script = script.unwrap_or_else(|| atv_app::replay::Script::parse("[]").unwrap());
        atv_app::replay::trace(
            &content,
            &venue,
            selection,
            race,
            &script,
            &out,
            autopilot_script.as_deref(),
        )
        .expect("replay trace");
        return;
    }
    let store = choose_store(content, pack_file).unwrap_or_else(|e| fail(&e));
    if verify {
        return verify_content(store);
    }
    // Without a direct-ride or tooling option the game opens at its title.
    let direct = venue_given
        || camera.is_some()
        || script.is_some()
        || autodrive.is_some()
        || race.is_some()
        || !ride
        || steps.is_some();
    let front = (front_page.is_some() || !direct).then(|| atv_app::FrontStart {
        search: String::new(),
        page: front_page,
    });
    run(Options {
        store,
        venue,
        camera,
        weather,
        steps,
        graphics,
        crash,
        capture,
        capture_ui,
        gpu_frames,
        ride,
        selection,
        race,
        script,
        ui_backdrop: None,
        sound: (!no_audio).then_some((sound, music)),
        autodrive,
        bench,
        profile,
        front,
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
