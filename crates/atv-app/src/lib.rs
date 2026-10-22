//! One game application for every build. The same event loop, loader and
//! renderer run natively (Metal/DX12/Vulkan) and in browsers (WebGL2).
mod front;
mod gamepad;
mod multiplayer;
mod net;
mod overlay;
pub mod replay;
mod riding;
mod sound;
mod soundtrack;
#[cfg(target_arch = "wasm32")]
mod text_inputs;
mod wake;
use atv_core::math::Det;
pub use multiplayer::RoomStart;
use replay::RunScript;

use atv_content::{Content, Store};
use atv_game::venue::{self, Venue};
use atv_render::{Camera, Gpu, Graphics, SceneRenderer, View, camera::default_focal_length};
use std::{cell::RefCell, collections::HashSet, rc::Rc, sync::Arc};
use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy, OwnedDisplayHandle},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

pub struct Options {
    pub store: Store,
    pub venue: String,
    /// Fixed `x,y,z,yaw,pitch,focal` view, e.g. copied from the browser build.
    pub camera: Option<[f32; 6]>,
    /// Forced Weather outcome (parity captures); random per load otherwise.
    pub weather: Option<bool>,
    /// World clock steps to run before the first frame, then hold the clocks
    /// (parity captures against a paused JavaScript frame).
    pub steps: Option<u64>,
    pub graphics: Graphics,
    /// `graphics` came from the command line or URL; otherwise rides from
    /// the frontend use the saved Display → Graphics preset.
    pub graphics_set: bool,
    /// Ride Effects -> Crashes (Original by default; Crazy/Gorn are authored).
    pub crash: atv_game::crash::CrashMode,
    /// Crash camera Tight (default) and Chase camera Assisted for rides the
    /// saved settings do not reach (held parity replays): `crashcam=` /
    /// `chase=` or storage on the web, `--crash-camera` / `--chase-camera`.
    pub camera_choice: (bool, bool),
    /// Native parity capture: render once at this size, write PNG and exit.
    #[cfg(not(target_arch = "wasm32"))]
    pub capture: Option<(std::path::PathBuf, u32, u32)>,
    /// Draw the ride UI (toolbar, HUD, minimap, keyboard card) into captures,
    /// with default in-memory settings.
    #[cfg(not(target_arch = "wasm32"))]
    pub capture_ui: bool,
    /// Before the capture, time the scene pass on the GPU over this many
    /// offscreen frames (timestamp queries) and print the median.
    #[cfg(not(target_arch = "wasm32"))]
    pub gpu_frames: Option<u32>,
    /// Ride the player's quad (default); false keeps the free camera.
    pub ride: bool,
    /// The Garage choice: ATV or motorcycle and rider.
    pub selection: atv_game::ride::Selection,
    /// The Garage rider gear and quad paint (ATVs).
    pub appearance: atv_game::ride::gear::Appearance,
    /// Race on the venue's National/Supercross course (with `ride`; a free
    /// camera views the race-course world and its pre-race shots).
    pub race: Option<atv_game::ride::race::Settings>,
    /// Steps replayed at load, then the ride holds (parity captures).
    pub script: Option<replay::Script>,
    /// Test hook: paint this flat colour under the UI instead of the scene's
    /// pixels, for overlay parity captures against the JavaScript DOM.
    pub ui_backdrop: Option<[u8; 3]>,
    /// Sound: None plays nothing (captures); otherwise the mode and music.
    pub sound: Option<(atv_game::audio::ride::SoundMode, bool)>,
    /// A script driving the live loop in real time (CPU baselines).
    pub autodrive: Option<replay::Script>,
    /// Autodrive reports also carry per-pass render CPU (`--profile`, `?profile=1`).
    pub profile: bool,
    /// Native: write the autodrive CPU report here and exit when it ends.
    #[cfg(not(target_arch = "wasm32"))]
    pub bench: Option<std::path::PathBuf>,
    /// Open the engine's frontend (title and menus) instead of riding at
    /// once; the frontend then reads this launch link (launch.js contract).
    pub front: Option<FrontStart>,
    /// Ride in a multiplayer room (host or join) on the relay.
    pub room: Option<RoomStart>,
    /// The multiplayer relay origin for menu rides (Arlington Free Ride);
    /// None: the page origin on the web, furyremastered.com on desktop.
    pub relay: Option<String>,
}

/// How the frontend opens.
#[derive(Clone, Debug, Default)]
pub struct FrontStart {
    /// The page URL query (ride links, `?menu=` shortcuts).
    pub search: String,
    /// Open this page directly (tests and parity captures).
    pub page: Option<String>,
}

struct Ready {
    gpu: Gpu,
    venue: Venue,
    scene: SceneRenderer,
    camera: Camera,
    focal_length: f32,
    /// World clocks held at the requested steps (parity captures).
    hold: bool,
    /// A fixed `--camera` view overrides the chase camera.
    fixed: bool,
    riding: Option<riding::Riding>,
    pending_sound: Option<PendingSound>,
    /// Original speed meter and stunt text resources (web/hud.js `init`).
    hud_assets: Option<atv_ui::hud::assets::OverlayAssets>,
    /// The multiplayer room this ride is in.
    net: Option<multiplayer::Net>,
}

struct PendingSound {
    content: std::rc::Rc<Content>,
    environment: String,
    settings: sound::Settings,
}

enum Message {
    Ready(Box<Ready>),
    /// The original frontend tune decoded (or why not).
    MenuTune(Result<atv_game::audio::mixer::Pcm, String>),
    /// The page's soundtrack player loaded for the frontend Jukebox.
    SoundtrackReady(Result<Box<sound::Soundtrack>, String>),
    SoundReady(Box<sound::Sound>),
    /// The original Jukebox art decoded.
    JukeboxArt(Box<atv_game::jukebox::JukeboxArt>),
    /// A line for the open menu's notice (e.g. the copied ride link).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Notice(String),
    SoundFailed(String),
    Failed(String),
    /// The frontend's GPU, content and venue list are ready.
    FrontReady(Box<Booted>),
    /// Frontend artwork finished loading.
    FrontArt(Box<ArtPiece>),
    /// A new GPU after a failed load from the frontend.
    FrontGpu(Box<Gpu>),
    /// A remote rider's model finished loading.
    PeerModel(Box<PeerModel>),
    /// The Arlington Free Ride rider count for the main menu.
    FreeRideRiders(Option<u32>),
    /// A Host/Join form field's text from the browser's input.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    FrontText(&'static str, String),
    /// A form field took (or gave back) the keyboard.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    FrontFocus(Option<usize>),
    /// The browser's 1 s presence timer (a hidden tab runs no frames).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    PresenceTick,
    /// A line for the frontend's notice (the copied invite link).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    LobbyNotice(String),
    /// A hosted or joined room connected (or failed) while the Free Ride
    /// Lobby page showed: (the room, its course).
    RoomJoined(Box<Result<(multiplayer::Net, String), String>>),
    /// Another rider's engine bank finished loading.
    RemoteEngine(u16, Box<Result<atv_game::audio::remote::Engine, String>>),
}

/// A connected room waiting on the lobby page: (room, course, quad, rider,
/// when the loader takes over).
type Joined = (multiplayer::Net, String, u16, u16, Instant);

type PeerModel = Result<
    atv_game::multiplayer::remote::RemoteModel,
    (atv_game::multiplayer::remote::Key, String),
>;

struct Booted {
    gpu: Gpu,
    content: Rc<Content>,
    boot: atv_ui::front::assets::Boot,
}

enum ArtPiece {
    Photos(Vec<(String, atv_ui::front::pixels::Rgba)>),
    Tips(Vec<Vec<String>>),
    Quads(atv_ui::front::assets::QuadCatalog),
    TopSpeed(u16, f32),
    Shots(String, Vec<atv_ui::front::pixels::Rgba>),
    Vehicle(
        u16,
        atv_ui::front::assets::PreviewModel,
        Option<atv_ui::front::assets::MenuRider>,
    ),
    MenuRider(u16, atv_ui::front::assets::MenuRider),
}

/// Load-time choices kept for rides started from the frontend.
struct Defaults {
    graphics: Graphics,
    graphics_set: bool,
    weather: Option<bool>,
    crash: atv_game::crash::CrashMode,
    sound: Option<(atv_game::audio::ride::SoundMode, bool)>,
    start: FrontStart,
    /// The multiplayer relay origin.
    relay: String,
}

struct App {
    /// Crash camera Tight, Chase camera Assisted (Options::camera_choice).
    camera_choice: (bool, bool),
    /// Screen Wake Lock while riding (web/wake-lock.js).
    wake_lock: wake::ScreenWake,
    /// The Display → Graphics choice the running ride applied.
    graphics_pref: Option<String>,
    proxy: EventLoopProxy<Message>,
    display: OwnedDisplayHandle,
    options: Option<Options>,
    window: Option<Arc<Window>>,
    ready: Option<Ready>,
    keys: HashSet<KeyCode>,
    dragging: bool,
    /// A pad button was down last frame (game.js introPadHeld): only a new
    /// press skips the race intro.
    intro_pad_held: bool,
    cursor: Option<(f64, f64)>,
    last: Instant,
    meter: Meter,
    gamepad: gamepad::Gamepads,
    drag: f32,
    started: Instant,
    /// The UI layer (touch controls, toolbar), once the GPU is ready.
    overlay: Option<overlay::Overlay>,
    /// Keep independent lifecycle causes: becoming visible must not undo a blur.
    focused: bool,
    visible: bool,
    suspended: bool,
    /// No frame is scheduled: the single-player ride is inactive (see `awake`).
    asleep: bool,
    /// Controller buttons held at the last paused-state poll.
    pad_buttons: u32,
    /// The left mouse press belongs to the UI, not the camera drag.
    mouse_ui: bool,
    ui_backdrop: Option<[u8; 3]>,
    ui_diagnostics: bool,
    /// The diagnostics overlay (F3; web/diagnostics.js).
    diagnostics: Diagnostics,
    /// `?ui=0` (parity captures): draw the scene only, as the JavaScript
    /// canvas captures leave out the DOM toolbar, HUD and keyboard card.
    hide_ui: bool,
    sound_unlocked: bool,
    activation: Option<sound::Activation>,
    bench: Bench,
    #[cfg(not(target_arch = "wasm32"))]
    bench_out: Option<std::path::PathBuf>,
    /// The frontend (title and menus), its GPU while no ride runs, and the
    /// content they share with rides.
    front: Option<Rc<RefCell<front::Front>>>,
    /// The original frontend tune (audio.js playMenuMusic).
    menu_music: MenuMusic,
    /// The page's soundtrack player: a ride's sound and the frontend
    /// Jukebox share it (audio.js `this.music`).
    soundtrack: Option<Rc<RefCell<sound::Soundtrack>>>,
    /// The frontend asked for the soundtrack before it was loaded.
    soundtrack_requested: bool,
    /// The Settings → Audio music on/off and volume last given to the
    /// soundtrack from the frontend (applied on change, as rides do).
    soundtrack_prefs: Option<(bool, f32)>,
    gpu: Option<Gpu>,
    content: Option<Rc<Content>>,
    defaults: Option<Defaults>,
    /// Frontend loads already started (top speeds, loader shots).
    requested: HashSet<String>,
    /// When the Free Ride rider count was last requested.
    riders_polled: Option<Instant>,
    /// A room kept live while the frontend's garage or course picker is open
    /// over it (Change ATV and Rider, Select Track).
    held_room: Option<multiplayer::Net>,
    /// The held room's ride, paused behind the frontend: its presence keeps
    /// going out until the ride reloads.
    held_ride: Option<atv_game::ride::Ride>,
    /// The held room the next frontend ride resumes.
    pending_room: Option<multiplayer::Net>,
    /// `pending_room` was just joined (it meets a rider; a held room
    /// reloading a course does not).
    pending_fresh: bool,
    /// The room connecting behind the Free Ride Lobby page: (quad, rider,
    /// the form page a failure returns to). None after Leave.
    joining: Option<(u16, u16, &'static str)>,
    /// A connected room shown on the Free Ride Lobby page (its code and
    /// riders, as multiplayer.js once the lobby state arrives) until the
    /// loader takes over at the instant given.
    joined: Option<Joined>,
    /// Frame-spread loads (see `spawn_frame`).
    frame_tasks: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = Message>>>>,
    /// The Host/Join forms' browser inputs.
    #[cfg(target_arch = "wasm32")]
    text_inputs: text_inputs::TextInputs,
    /// Desktop: the rider count request in flight.
    #[cfg(not(target_arch = "wasm32"))]
    riders_pending: Option<std::sync::mpsc::Receiver<Option<u32>>>,
}

/// Per-frame CPU while an autodrive script plays (vsync wait excluded).
#[derive(Default)]
struct Bench {
    frames: u32,
    cpu: f64,
    sim: f64,
    render: f64,
    audio: f64,
    reported: bool,
    /// Add the renderer's per-pass CPU means to the report.
    profile: bool,
}

#[derive(Default)]
struct Meter {
    frames: u32,
    cpu: f64,
    /// UI build/upload CPU time (included in `cpu`).
    ui: f64,
    since: Option<Instant>,
    triangles: u32,
    draws: u32,
    /// Contact shadow blobs and High shadow-map vehicles in the last frame.
    shadows: (u32, u32),
    /// Room work (socket, poses, remote riders and loads) CPU, included in
    /// `cpu`, and the longest frame interval this second.
    net: f64,
    last: Option<Instant>,
    worst: f64,
    /// Longest single load slice this second (remote riders' uploads are
    /// timed by the room).
    load_max: f64,
}

/// Load-time choices for the venue presentation and the ride.
struct World {
    selection: atv_game::ride::Selection,
    appearance: atv_game::ride::gear::Appearance,
    race: Option<atv_game::ride::race::Settings>,
    weather: Option<bool>,
    steps: Option<u64>,
    graphics: Graphics,
    crash: atv_game::crash::CrashMode,
    ride: bool,
    script: Option<replay::Script>,
    sound: Option<(atv_game::audio::ride::SoundMode, bool)>,
    autodrive: Option<replay::Script>,
}

/// Ride prefetch (`?prefetch=off` turns it off for load-time measurement).
static PREFETCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// A loading progress line (status bar, and the frontend's loader).
type Progress = Box<dyn FnMut(&Gpu, &str)>;

/// The window's GPU device and surface.
async fn open_gpu(window: Arc<Window>, display: OwnedDisplayHandle) -> Result<Gpu, String> {
    let size = window.inner_size();
    let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display));
    // Browsers get WebGL2 so the port runs on every phone the site supports.
    descriptor.backends = if cfg!(target_arch = "wasm32") {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::PRIMARY
    };
    let instance = wgpu::Instance::new(descriptor);
    let surface = instance
        .create_surface(window.clone())
        .map_err(|e| format!("surface: {e}"))?;
    Gpu::new(&instance, surface, size.width, size.height).await
}

async fn prepare(
    gpu: Gpu,
    content: std::rc::Rc<Content>,
    environment: String,
    fixed: Option<[f32; 6]>,
    world: World,
    mut progress: Progress,
) -> Result<Ready, String> {
    let World {
        selection,
        appearance,
        race,
        weather,
        steps,
        graphics,
        crash,
        ride,
        script,
        sound,
        autodrive,
    } = world;
    // Every archive the ride reads downloads in parallel (game.js prefetchRide).
    if ride && PREFETCH.load(std::sync::atomic::Ordering::Relaxed) {
        let (paths, bank) = atv_game::launch::ride_prefetch(
            &environment,
            race.is_some(),
            selection.quad,
            selection.rider,
        );
        content.prefetch(&paths, false, false);
        content.prefetch(&bank, true, true);
    }
    progress(
        &gpu,
        &format!("Reading original {}…", environment.to_uppercase()),
    );
    let weather = match weather {
        Some(active) => venue::WeatherChoice::Active(active),
        // Per-load sample, like the browser's Math.random() * 2^31.
        None => venue::WeatherChoice::Random(
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos() ^ d.as_secs() as u32),
        ),
    };
    let venue = venue::load_with(
        &content,
        &environment,
        race.is_some(),
        venue::LoadOptions {
            weather,
            sky_smooth: graphics.sky_smooth,
        },
    )
    .await?;
    progress(&gpu, "Building the scenery textures…");
    let start = venue.scene.start_position;
    let direction = venue.scene.start_direction;
    let yaw = direction[0].atan2_d(direction[2]);
    let mut camera = Camera {
        x: start[0],
        y: start[1],
        z: start[2],
        yaw,
        pitch: -0.25,
    };
    camera.x -= yaw.sin_d() * 14.;
    camera.z -= yaw.cos_d() * 14.;
    camera.y = ground(&venue, camera.x, camera.z).max(start[1]) + 6.;
    let mut focal_length = default_focal_length();
    if let Some([x, y, z, yaw, pitch, focal]) = fixed {
        camera = Camera {
            x,
            y,
            z,
            yaw,
            pitch,
        };
        focal_length = focal;
    }
    // The browser centres the fine terrain patch on the rider at the start.
    let mut scene =
        SceneRenderer::with_graphics(&gpu, &venue, Some([start[0], start[2]]), graphics);
    if let Some(steps) = steps {
        scene.set_weather_camera(camera);
        scene.advance_steps(steps);
    }
    // speedRushTaps: phones and tablets ((pointer: coarse)) get fewer taps.
    #[cfg(target_arch = "wasm32")]
    scene.set_coarse_pointer(
        web_sys::window()
            .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
            .is_some_and(|m| m.matches()),
    );
    let mut pending_sound = None;
    let riding = if ride {
        let ride = atv_game::ride::load_with(
            &content,
            &venue,
            selection.quad,
            selection.rider,
            race,
            |text| progress(&gpu, text),
        )
        .await?;
        let mut ride = ride;
        ride.dress(&content, appearance).await?;
        scene.set_ride(&gpu, &venue, &ride.quad, &ride.rider)?;
        scene.set_ride_paint(ride.paint());
        if let Some(race) = ride.race_mut() {
            scene.set_race_cpus(&gpu, &venue, &std::mem::take(&mut race.draw))?;
        }
        let mut riding = riding::Riding::new(ride, &venue);
        riding.interpolation = graphics.interpolation;
        riding.wheel_reprojection = graphics.wheel_reprojection;
        riding.dirt_enabled = graphics.dirt;
        // `?autopilot=1` (parity tooling): the race path driver rides the
        // player before the finish too; traces record its controls.
        if url_flag("autopilot=1")
            && let Some(state) = riding.ride.race_mut()
        {
            state.autopilot_always = true;
        }
        // startSimulation's first frame (op 23) primes the wheel effects; a
        // motorcycle draws its own MX particles (loadMotorcycleEffects).
        let effects = if riding.ride.is_motorcycle() {
            Some(atv_game::wheel_effects::load_motorcycle(&content, &riding.ride).await?)
        } else {
            atv_game::wheel_effects::load(&content, &venue, selection.quad).await?
        };
        if let Some(mut effects) = effects {
            effects.set_lingering_dust(usize::from(graphics.dust), &venue);
            effects.set_tire_tracks(
                usize::from(graphics.tire_tracks.0),
                f32::from(graphics.tire_tracks.1),
                &venue,
            );
            effects.reset(&riding.ride)?;
            scene.set_wheel_effects(&gpu, &effects);
            riding.effects = Some(effects);
        }
        if crash != atv_game::crash::CrashMode::Original {
            let mut crashes = atv_game::crash::Crashes::new(crash);
            crashes.reset(&riding.ride)?;
            riding.crashes = Some(crashes);
        }
        if let Some(script) = &script {
            // `?traceAll=1`: every step, for browser-vs-native parity.
            if trace_all() {
                let records = script.run_recording(&mut riding)?;
                publish_trace(&serde_json::Value::from(records).to_string());
            } else {
                script.run(&mut riding)?;
                publish_trace(&replay::record(&riding.ride, riding.ride.steps()).to_string());
            }
            // Hold the replayed state for screenshots.
            riding.paused = true;
            riding.held = true;
            if let Some(gate) = riding.ride.race().and_then(|r| r.frame.as_ref()?.gate) {
                scene.race_step(
                    &gpu,
                    &atv_game::ride::race::StepEvents {
                        gate: Some(gate),
                        ..Default::default()
                    },
                );
            }
            if let Some(effects) = &mut riding.effects {
                scene.accept_wheel_effects(&gpu, effects);
            }
            for origin in riding.step_rest.iter().flatten() {
                scene.observe_rest(*origin);
            }
        }
        if let (Some((mode, music)), None) = (sound, &script) {
            pending_sound = Some(PendingSound {
                content: content.clone(),
                environment: environment.clone(),
                settings: sound::Settings {
                    mode,
                    music,
                    selection,
                    race_samples: riding.ride.race().map(|r| r.samples()).unwrap_or_default(),
                    train_sounds: venue.train_sounds.clone(),
                    // loadWeather: active Rain weather loops rain.vag.
                    rain: venue.weather.as_ref().is_some_and(|w| {
                        w.active
                            && w.definition
                                .as_ref()
                                .is_some_and(|d| d.kind.eq_ignore_ascii_case("rain"))
                    }),
                },
            });
        }
        riding.autodrive = autodrive.map(|script| (script.controls(), 0));
        // `?presentTrace=1` (tools/test_engine_interpolation.mjs): log the
        // delivered states and sampled presentation of the autodrive run.
        if url_flag("presentTrace=1") {
            riding.present_log = Some(Vec::new());
        }
        Some(riding)
    } else {
        None
    };
    let hud_assets = if riding.is_some() {
        match atv_ui::hud::assets::load(&content, &environment, race.is_some()).await {
            Ok(assets) => Some(assets),
            Err(error) => {
                status(&format!("HUD resources unavailable: {error}"));
                None
            }
        }
    } else {
        None
    };
    progress(&gpu, "Static terrain ready");
    Ok(Ready {
        gpu,
        venue,
        scene,
        camera,
        focal_length,
        hold: steps.is_some(),
        fixed: fixed.is_some(),
        riding,
        pending_sound,
        hud_assets,
        net: None,
    })
}

/// Native room captures: run the room (poses, remote models) until
/// `FURY_ROOM_PEERS` other riders (default 1) are drawn, then two more
/// seconds of presentation; at most `FURY_ROOM_WAIT` seconds (default 20).
#[cfg(not(target_arch = "wasm32"))]
fn settle_room(ready: &mut Ready) {
    let (Some(net), Some(riding)) = (&mut ready.net, &mut ready.riding) else {
        return;
    };
    let setting = |name: &str, default: f32| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (wait, peers) = (
        setting("FURY_ROOM_WAIT", 20.),
        setting("FURY_ROOM_PEERS", 1.),
    );
    let started = Instant::now();
    let mut seen: Option<Instant> = None;
    while started.elapsed().as_secs_f32() < wait && net.closed.is_none() {
        let tick = net.tick(Some(&riding.ride), false);
        for key in tick.loads {
            net.model_loaded(pollster::block_on(multiplayer::load_model(
                net.content(),
                key,
            )));
        }
        if let Some((origin, yaw, velocity)) = tick.place {
            let _ = riding.place_beside(origin, yaw, velocity);
        }
        let eye = ready.camera.eye();
        net.present(
            &ready.gpu,
            &mut ready.scene,
            &ready.venue,
            &riding.ride,
            eye,
        );
        if net.drawn as f32 >= peers {
            let since = *seen.get_or_insert_with(Instant::now);
            if since.elapsed().as_secs_f32() > 2. {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    println!("multiplayer {}", net.report());
}

/// Bilinear presentation height, for keeping the free camera above ground.
fn ground(venue: &Venue, x: f32, z: f32) -> f32 {
    let t = &venue.terrain;
    let gx = (x / t.spacing).clamp(0., (t.width - 1) as f32);
    let gz = (z / t.spacing).clamp(0., (t.depth - 1) as f32);
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (x1, z1) = ((x0 + 1).min(t.width - 1), (z0 + 1).min(t.depth - 1));
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let top = t.height(x0, z0) + (t.height(x1, z0) - t.height(x0, z0)) * fx;
    let bottom = t.height(x0, z1) + (t.height(x1, z1) - t.height(x0, z1)) * fx;
    top + (bottom - top) * fz
}

impl App {
    fn step(&mut self) {
        // game.js screenWake.set(!menuOpen): a ride on screen, no menu.
        let riding = self.ready.as_ref().is_some_and(|r| r.riding.is_some())
            && self.overlay.as_ref().is_none_or(|o| o.menu.is_none());
        self.wake_lock.set(riding, self.visible);
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        let Some(ready) = &mut self.ready else {
            self.front_frame();
            return;
        };
        if ready.riding.is_some() {
            self.ride_frame(dt);
            return;
        }
        let held = |k| self.keys.contains(&k);
        let axis = |a, b| (held(a) as i32 - held(b) as i32) as f32;
        let speed = if held(KeyCode::ShiftLeft) || held(KeyCode::ShiftRight) {
            160.
        } else {
            40.
        } * dt;
        let forward =
            axis(KeyCode::KeyW, KeyCode::KeyS) + axis(KeyCode::ArrowUp, KeyCode::ArrowDown);
        let strafe = axis(KeyCode::KeyD, KeyCode::KeyA);
        let turn = axis(KeyCode::ArrowLeft, KeyCode::ArrowRight);
        let rise = axis(KeyCode::KeyE, KeyCode::KeyQ);
        let c = &mut ready.camera;
        c.yaw += turn * 1.5 * dt;
        let axes = c.axes();
        c.x += (axes.forward[0] * forward - axes.right[0] * strafe) * speed;
        c.y += (axes.forward[1] * forward + rise) * speed;
        c.z += (axes.forward[2] * forward - axes.right[2] * strafe) * speed;
        c.y = c.y.max(ground(&ready.venue, c.x, c.z) + 1.5);
        let started = Instant::now();
        let mut acquire = 0.;
        if !ready.hold {
            ready.scene.advance(dt);
        }
        // A race-course load plays its pre-race shots unless a view is fixed.
        if !ready.fixed
            && let Some((camera, focal)) = ready.scene.intro_view(ready.gpu.aspect())
        {
            ready.camera = camera;
            ready.focal_length = focal;
        }
        match ready.scene.render(
            &ready.gpu,
            &View {
                camera: ready.camera,
                focal_length: ready.focal_length,
            },
        ) {
            Ok(stats) => {
                self.meter.triangles = stats.triangles();
                self.meter.draws = stats.draws();
                self.meter.shadows = (stats.contact_casters, stats.shadow_casters);
                acquire = stats.acquire_ms;
            }
            Err(error) => status(&error),
        }
        self.meter.cpu += started.elapsed().as_secs_f64() * 1000. - acquire;
        self.meter_frame(now, "");
    }

    /// One rendered frame of the ride: input, fixed steps, chase view.
    fn ride_frame(&mut self, dt: f32) {
        self.update_diagnostics();
        // Other riders' models and engine banks load a slice per frame.
        let loads = Instant::now();
        self.poll_frame_tasks();
        self.meter.net += loads.elapsed().as_secs_f64() * 1000.;
        let focused = self.effective_focused();
        let suspended = !self.visible || self.suspended;
        let now = Instant::now();
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        let started = Instant::now();
        let ms = (now - self.started).as_secs_f64() * 1000.;
        let active = !riding.paused && !riding.held && focused;
        let pad = self.gamepad.sample();
        if let Some(overlay) = &mut self.overlay {
            overlay.pad_name = self.gamepad.name();
            overlay.sync_menu(riding.paused && !riding.held);
            riding.menu_open = overlay.menu.is_some();
            if riding.menu_open
                && let Some(event) = overlay.menu_pad(pad.as_ref(), Some(&mut riding.bindings))
            {
                self.menu_event(event);
                return;
            }
            if !riding.menu_open
                && let Some(action) =
                    overlay.results_pad(&pad.as_ref().map_or([0.; 17], |p| p.buttons))
            {
                self.act(action);
                return;
            }
        }
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        // game.js: a new pad press skips the pre-race camera (00212d40), like
        // any key, tap or click.
        let pad_pressed = pad
            .as_ref()
            .is_some_and(|p| p.buttons.iter().any(|b| *b > 0.5));
        if pad_pressed && !self.intro_pad_held {
            riding.ride.skip_intro();
        }
        self.intro_pad_held = pad_pressed;
        let names: HashSet<String> = self.keys.iter().map(|k| format!("{k:?}")).collect();
        let keys = |code: &str| names.contains(code);
        let sample = riding::Sample {
            keys: &keys,
            pad: pad.as_ref(),
            now: (now - self.started).as_secs_f64(),
            drag: std::mem::take(&mut self.drag),
        };
        // render(): dt = min(.05, frame time).
        let dt = dt.min(0.05);
        let overlay = &mut self.overlay;
        let mut touch = || overlay.as_mut().map(|o| o.sample(ms, active));
        if let Err(error) = riding.frame(&sample, &mut touch, dt) {
            status(&format!("Simulation stopped: {error}"));
            ready.riding = None;
            return;
        }
        // Rumble.pulse: the frame's impacts on the active controller.
        for pulse in std::mem::take(&mut riding.pulses) {
            if self.gamepad.rumble(pulse) {
                riding.rumble.sent(pulse);
            }
        }
        // A Pause binding this frame opens the menu in this same frame (the
        // ride then sleeps until input).
        if let Some(overlay) = &mut self.overlay {
            overlay.sync_menu(riding.paused && !riding.held);
            riding.menu_open = overlay.menu.is_some();
            // Audio → Sound shows the mode the ride is actually playing.
            if let Some(sound) = &riding.sound {
                let playing = match sound.ride.mode {
                    atv_game::audio::ride::SoundMode::Enhanced => "enhanced",
                    _ => "original",
                };
                if overlay.prefs.sound_mode != playing {
                    overlay.prefs.sound_mode = playing.into();
                }
            }
        }
        // World clocks (scenery animation, water, train) follow the
        // simulation's fixed steps, as the browser's op 50 batch does.
        if !ready.hold {
            for (step, &(position, target)) in riding.step_cameras.iter().enumerate() {
                if let Some(Some(origin)) = riding.step_rest.get(step) {
                    ready.scene.observe_rest(*origin);
                }
                // The race's flash bursts, fireworks and gate precede the
                // step's ambient and flash clocks (atv-web's batch order).
                if let Some(events) = riding.step_race.get(step) {
                    ready.scene.race_step(&ready.gpu, events);
                }
                ready.scene.set_weather_view(position, target);
                ready.scene.advance_steps(1);
            }
        }
        if let Some(effects) = &mut riding.effects {
            ready.scene.accept_wheel_effects(&ready.gpu, effects);
        }
        match riding.present(
            &ready.gpu,
            &mut ready.scene,
            &ready.venue,
            ready.gpu.aspect(),
            dt,
        ) {
            Ok((camera, focal)) if !ready.fixed => {
                ready.camera = camera;
                ready.focal_length = focal;
            }
            Ok(_) => {}
            Err(error) => status(&error),
        }
        // The speed-rush screen pass shares the FOV kick's amount.
        let motion = riding.ride.frame();
        ready.scene.set_speed_rush(
            &ready.gpu,
            riding.rush_amount(),
            dt,
            motion.origin,
            motion.linear_velocity,
        );
        // Alpha Air dark blur follows the director's big-air amount.
        ready.scene.set_air_blur(&ready.gpu, riding.air_blur());
        let room_started = Instant::now();
        let mut loads = Vec::new();
        let mut engine_loads = Vec::new();
        let mut course = None;
        if let Some(net) = &mut ready.net {
            let was_open = net.closed.is_none();
            // Only actual suspension (hidden page, suspended app) sends the
            // paused presence pose; the online pause menu and blur keep
            // neutral-input riding (Riding::online).
            let tick = net.tick(Some(&riding.ride), suspended);
            if let Some((origin, yaw, velocity)) = tick.place
                && let Err(error) = riding.place_beside(origin, yaw, velocity)
            {
                status(&format!("Unable to join beside that rider: {error}"));
            }
            if tick.train_place
                && let Err(error) = riding.place_on_train(TRAIN_PLACE_CAR)
            {
                status(&format!("Train placement: {error}"));
            }
            riding.peers = net.contact_bodies(&riding.ride);
            if let Some(overlay) = &mut self.overlay {
                overlay.lobby = net.menu_lobby();
            }
            riding.server_clock = net.server_clock();
            let eye = ready.camera.eye();
            net.present(
                &ready.gpu,
                &mut ready.scene,
                &ready.venue,
                &riding.ride,
                eye,
            );
            if was_open && let Some(closed) = &net.closed {
                status(&format!("Multiplayer ended: {closed}"));
            }
            if let Some(sound) = &mut riding.sound {
                for quad in sound.remote_frame(&net.riders) {
                    engine_loads.push((quad, sound.load_remote_engine(quad)));
                }
            }
            let content = net.content();
            loads.extend(tick.loads.into_iter().map(|key| (content.clone(), key)));
            course = tick.course;
            let remote = riding
                .sound
                .as_ref()
                .map(|s| (s.remote.voices(), s.remote.starts, s.remote.impact_starts));
            let tags = self.overlay.as_ref().map_or(0, |o| o.tags);
            publish_multiplayer(net, riding.contacts, &riding.ride, remote, tags);
        }
        for (quad, future) in engine_loads {
            self.spawn_frame(future, move |engine| {
                Message::RemoteEngine(quad, Box::new(engine))
            });
        }
        for (content, key) in loads {
            self.spawn_frame(multiplayer::load_model(content, key), |model| {
                Message::PeerModel(Box::new(model))
            });
        }
        if self.ready.as_ref().is_some_and(|r| r.net.is_some()) {
            self.meter.net += room_started.elapsed().as_secs_f64() * 1000.;
        }
        if let Some(environment) = course {
            self.reload_room(environment);
            return;
        }
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        let active = !riding.paused && !riding.held && focused;
        let ui_started = Instant::now();
        self.update_jukebox();
        self.save_soundtrack();
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        if let Some(overlay) = &mut self.overlay {
            if riding.input_cleared {
                overlay.clear_input();
            }
            let ride = &riding.ride;
            let frame = ride.frame();
            let applied = ride.applied();
            overlay.feedback(
                atv_ui::touch::Feedback {
                    grounded: frame.grounded,
                    preload_available: frame.preload_available,
                    preload_armed: frame.preload_armed,
                    preload_active: frame.preload_active,
                    applied_touch: applied.touch_drive.map(|d| (d.generation, d.held)),
                    applied_brake: applied.brake,
                    flip_launch_window: false,
                    stunt_name: None,
                },
                ms,
            );
            let gpu = &ready.gpu;
            // Other riders' tags and map markers project with the scene camera.
            let hud_peers = ready.net.as_ref().map(|net| net.hud_peers());
            let view_projection =
                ready
                    .camera
                    .view_projection(gpu.aspect(), ready.focal_length, 1., 10_000.);
            overlay.prepare(
                &gpu.device,
                &gpu.queue,
                gpu.config.width,
                gpu.config.height,
                ms,
                // A held parity replay keeps its HUD, as the browser renders it.
                active || (riding.held && focused),
                !riding.paused || riding.held,
                riding.paused && !riding.held,
                Some(overlay::HudInput {
                    hud: ride.hud(),
                    rider_world: ride.rider_world(),
                    terrain: &ready.venue.terrain,
                    race: ride
                        .race()
                        .and_then(|r| r.frame.as_ref())
                        .map(|f| (f, ride.committed_points(), frame.origin)),
                    peers: hud_peers
                        .as_deref()
                        .map(|peers| (peers, frame.origin, view_projection)),
                }),
                Some(&mut riding.bindings),
            );
        }
        self.meter.ui += ui_started.elapsed().as_secs_f64() * 1000.;
        if self.ui_diagnostics {
            let mut state = replay::record(&riding.ride, riding.ride.steps());
            state["camera"]["preset"] = riding.ride.camera().0.preset.into();
            state["paused"] = riding.paused.into();
            state["wakeLock"] = self.wake_lock.held().into();
            state["diagnostics"] = serde_json::json!({
                "mode": self.diagnostics.mode.name(), "lines": self.diagnostics.lines});
            state["active"] = active.into();
            state["shadows"] = serde_json::json!({
                "contact": self.meter.shadows.0,
                "map": self.meter.shadows.1,
            });
            let applied = riding.ride.applied();
            state["controls"] = serde_json::json!({
                "steer": applied.steer, "lean": applied.lean,
                "throttle": applied.throttle, "brake": applied.brake,
                "touchHeld": applied.touch_drive.is_some_and(|t| t.held),
                "preload": applied.preload_axis,
            });
            if let Some(overlay) = &self.overlay {
                state["toolbar"] = overlay.diagnostics();
            }
            // The saved settings the ride applies (tools/test_engine_settings.mjs).
            let graphics = ready.scene.graphics();
            let mixer = riding.sound.as_ref().and_then(|s| {
                s.output
                    .mixer
                    .lock()
                    .ok()
                    .map(|m| (m.sfx_volume, m.music_volume))
            });
            state["settings"] = serde_json::json!({
                "speedRushPeak": riding.speed_rush_peak(),
                "crashMode": riding.crashes.as_ref().map(|c| c.mode.name()),
                "flips": riding.ride.options.flips,
                "realisticTrain": riding.ride.options.realistic_train,
                "arcadeFlips": riding.ride.options.arcade_flips,
                "alphaSlowmo": riding.alpha_slowmo,
                "alphaBigAirCamera": riding.alpha_big_air_camera,
                "alphaAirBlur": riding.alpha_air_blur, "airBlur": riding.air_blur(),
                "hypeBlur": riding.hype.blur,
                "alphaMuffle": riding.alpha_muffle,
                "hypeMuffle": riding.hype.muffle, "hypeWind": riding.hype.wind,
                "musicMuffle": riding.sound.as_ref().map_or(0., |s| s.muffle()),
                "musicStream": riding.sound.as_ref().and_then(|s| s.web_state())
                    .map(|(graph, time, paused)| serde_json::json!({
                        "graph": graph, "time": time, "paused": paused})),
                "alphaLandingBurst": riding.alpha_landing_burst,
                "burstLive": riding.burst.live, "thumps": riding.thumps,
                "heavyMud": riding.dirt.heavy(),
                "timeScale": riding.hype.time_scale,
                "interpolation": riding.interpolation,
                "skySmooth": graphics.sky_smooth, "fresnel": graphics.fresnel,
                "contactShadows": graphics.contact_shadows, "anisotropy": graphics.anisotropy,
                "dust": graphics.dust, "tireTracks": graphics.tire_tracks.0,
                "shadowMap": graphics.shadow_map,
                "wheelReprojection": riding.wheel_reprojection,
                "hillOrbit": riding.hill_orbit(), "chaseAssist": riding.chase_assist,
                "crashCameraTight": riding.crash_camera_tight,
                "vibration": {"enabled": riding.vibration, "pulses": riding.rumble.pulses,
                    "impacts": riding.impacts.impacts,
                    "last": riding.rumble.last.map(|p| serde_json::json!({
                        "strength": p.strength, "duration": p.duration_ms}))},
                "wheelLift": riding.wheel_lift.get(),
                "dirtLevel": riding.dirt.level(),
                "dirtApplied": if riding.dirt_enabled { riding.dirt.amount() } else { 0. },
                "sfx": mixer.map(|m| m.0), "music": mixer.map(|m| m.1),
                "soundMode": riding.sound.as_ref().map(|s| format!("{:?}", s.ride.mode)),
            });
            publish_ui_state(&state.to_string());
        }
        let overlay = self.overlay.as_ref().filter(|_| !self.hide_ui);
        let render_started = Instant::now();
        let mut acquire = 0.;
        match ready.scene.render_with(
            &ready.gpu,
            &View {
                camera: ready.camera,
                focal_length: ready.focal_length,
            },
            &mut |encoder, target| {
                if let Some(overlay) = overlay {
                    overlay.draw(encoder, target);
                }
            },
        ) {
            Ok(stats) => {
                self.meter.triangles = stats.triangles();
                self.meter.draws = stats.draws();
                self.meter.shadows = (stats.contact_casters, stats.shadow_casters);
                acquire = stats.acquire_ms;
            }
            Err(error) => status(&error),
        }
        let cpu = started.elapsed().as_secs_f64() * 1000. - acquire;
        self.meter.cpu += cpu;
        self.diagnostics.cpu = cpu;
        if riding.autodrive.is_some() {
            let render = render_started.elapsed().as_secs_f64() * 1000. - acquire;
            let audio = riding.sound.as_ref().map_or(0., |s| s.mix);
            let bench = &mut self.bench;
            if bench.frames == 0 && bench.profile {
                ready.scene.set_profiling(true);
            }
            bench.frames += 1;
            bench.cpu += cpu;
            bench.sim += riding.sim_ms;
            bench.render += render;
            bench.audio += audio;
        } else if riding.autodrive_done && !self.bench.reported {
            self.bench.reported = true;
            if let Some(log) = riding.present_log.take() {
                publish_trace(
                    &serde_json::json!({"interpolation": riding.interpolation, "events": log})
                        .to_string(),
                );
                return;
            }
            let b = &self.bench;
            let n = f64::from(b.frames.max(1));
            let passes = if b.profile {
                let entries: Vec<String> = ready
                    .scene
                    .profile_report()
                    .iter()
                    .map(|(name, ms)| format!(r#""{name}":{ms:.4}"#))
                    .collect();
                let draws: Vec<String> = ready
                    .scene
                    .profile_counts()
                    .iter()
                    .map(|(name, n)| format!(r#""{name}":{n:.1}"#))
                    .collect();
                format!(
                    r#","passes":{{{}}},"draws":{{{}}}"#,
                    entries.join(","),
                    draws.join(",")
                )
            } else {
                String::new()
            };
            let report = format!(
                r#"{{"frames":{},"cpu_ms_per_frame":{:.4},"cpu_ms_per_60hz_step":{:.4},"sim_and_sound_ms":{:.4},"render_ms":{:.4},"mixer_ms":{:.4},"steps":{}{passes}}}"#,
                b.frames,
                b.cpu / n,
                b.cpu / riding.ride.steps().max(1) as f64,
                b.sim / n,
                b.render / n,
                b.audio / n,
                riding.ride.steps()
            );
            publish_trace(&report);
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = &self.bench_out {
                std::fs::write(path, &report).expect("write bench report");
                status(&format!("wrote {}", path.display()));
                std::process::exit(0);
            }
        }
        let ride = &riding.ride;
        let frame = ride.frame();
        let sound = match &riding.sound {
            Some(sound) if sound.output.started.get() => format!(
                " · {:.2}+{:.2} ms audio{}",
                sound.cpu,
                sound.mix,
                sound
                    .music
                    .borrow()
                    .title
                    .as_ref()
                    .map_or(String::new(), |t| format!(" · {t}"))
            ),
            Some(_) => " · press a key for sound".into(),
            None => String::new(),
        };
        let race = ride
            .race()
            .and_then(|r| r.frame.as_ref())
            .map_or(String::new(), |f| match f.phase {
                "intro" => " · race intro (any key skips)".into(),
                "countdown" => format!(" · {}", f.countdown),
                "finished" => format!(
                    " · finished {}/{} · {}",
                    f.position,
                    f.field,
                    f.standings
                        .iter()
                        .map(|s| format!(
                            "{}. {} {}",
                            s.position,
                            s.name,
                            s.time.map_or("--".into(), |t| format!("{t:.2}"))
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                _ => format!(
                    " · LAP {}/{} · PLACE {}/{}{}",
                    f.lap,
                    f.laps,
                    f.position,
                    f.field,
                    if f.wrong_way { " · WRONG WAY" } else { "" }
                ),
            });
        let extra = format!(
            " · {:.0} MPH{race}{sound}{}",
            frame.speed_magnitude * 3600. / 5280.,
            if riding.paused {
                " · paused (Esc)"
            } else {
                ""
            }
        );
        self.meter_frame(now, &extra);
    }

    fn meter_frame(&mut self, now: Instant, extra: &str) {
        let Some(ready) = &self.ready else { return };
        self.meter.frames += 1;
        if let Some(last) = self.meter.last {
            self.meter.worst = self.meter.worst.max((now - last).as_secs_f64() * 1000.);
        }
        self.meter.last = Some(now);
        let room = if ready.net.is_some() {
            format!(
                " · {:.0} ms worst · {:.2} ms room · {:.1} ms load · {:.1} ms upload",
                self.meter.worst,
                self.meter.net / f64::from(self.meter.frames.max(1)),
                self.meter.load_max,
                ready.net.as_ref().map_or(0., |n| n.upload_max)
            )
        } else {
            String::new()
        };
        let since = *self.meter.since.get_or_insert(now);
        let elapsed = (now - since).as_secs_f64();
        if elapsed >= 1. {
            let info = &ready.gpu.adapter_info;
            status(&format!(
                "{} · {:.0} fps · {:.1} ms frame · {:.2} ms CPU ({:.2} UI){room} · {} draws · {} tris · {:?} {}{extra}",
                ready.venue.environment.to_uppercase(),
                self.meter.frames as f64 / elapsed,
                elapsed * 1000. / self.meter.frames as f64,
                self.meter.cpu / self.meter.frames as f64,
                self.meter.ui / self.meter.frames as f64,
                self.meter.draws,
                self.meter.triangles,
                info.backend,
                info.name
            ));
            if let Some(net) = self.ready.as_mut().and_then(|r| r.net.as_mut()) {
                net.upload_max = 0.;
            }
            self.meter = Meter {
                triangles: self.meter.triangles,
                draws: self.meter.draws,
                last: self.meter.last,
                ..Default::default()
            };
        }
    }
}

impl App {
    /// Physical window pixels to CSS pixels.
    fn css(&self, (x, y): (f64, f64)) -> (f32, f32) {
        let scale = self.window.as_ref().map_or(1., |w| w.scale_factor());
        ((x / scale) as f32, (y / scale) as f32)
    }

    /// Whether frames keep being scheduled. Like render() in web/game.js, a
    /// single-player ride schedules no further frame while the window is
    /// unfocused, hidden or suspended, or while paused (the menu is open);
    /// lifecycle and input events wake it. Simulation already stops in those
    /// states, so nothing but redundant redraws is skipped. Parity holds and
    /// autodrive benchmarks keep rendering. A multiplayer ride stays awake.
    fn awake(&self) -> bool {
        let Some(ready) = &self.ready else {
            // The frontend draws on input, and continuously only while its
            // page animates; a ride loading from it presents its own frames.
            return match &self.front {
                Some(front) => {
                    self.gpu.is_some() && self.effective_focused() && front.borrow().animating()
                }
                None => true,
            };
        };
        if let Some(riding) = &ready.riding
            && (riding.held || riding.autodrive.is_some())
        {
            return true;
        }
        // A room keeps its socket, poses and remote riders live: inactive
        // render sleep must not sleep multiplayer.
        if ready.net.as_ref().is_some_and(|n| n.closed.is_none()) {
            return true;
        }
        // The Jukebox animates its disc, progress and spectrum while open.
        let jukebox = self.overlay.as_ref().is_some_and(|o| o.jukebox_open());
        self.effective_focused() && (jukebox || ready.riding.as_ref().is_none_or(|r| !r.paused))
    }

    /// Schedule one frame: it applies the event, then `awake` decides.
    fn wake(&mut self) {
        if self.asleep {
            self.asleep = false;
            // The browser restarts its frame clock on resume (lastTime = 0).
            self.last = Instant::now();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// While asleep and paused in focus: poll controllers so a button can
    /// resume, and keep the soundtrack fed (the pause menu keeps music).
    fn paused_tick(&mut self) {
        let pad = self.gamepad.sample();
        let buttons = pad.as_ref().map_or(0, |pad| {
            pad.buttons
                .iter()
                .enumerate()
                .filter(|(_, v)| **v > 0.5)
                .fold(0u32, |bits, (i, _)| bits | 1 << i)
        });
        let pressed = buttons & !self.pad_buttons != 0;
        self.pad_buttons = buttons;
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            let steps = riding.ride.steps();
            if let Some(sound) = &mut riding.sound {
                sound.maintain();
                sound.report(steps);
            }
        }
        if pressed {
            self.wake();
        }
    }

    fn effective_focused(&self) -> bool {
        self.focused && self.visible && !self.suspended
    }

    /// Apply lifecycle state even when content is still loading. Winit reports
    /// web document visibility as Occluded, separately from canvas focus.
    fn sync_activity(&mut self) {
        let focused = self.effective_focused();
        if !focused {
            self.blur();
        }
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            riding.set_focused(focused);
        }
        self.sync_menu_music();
        self.last = Instant::now();
        self.wake();
    }

    fn blur(&mut self) {
        self.dragging = false;
        self.mouse_ui = false;
        self.drag = 0.;
        self.keys.clear();
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            riding.clear_input();
        }
        if let Some(overlay) = &mut self.overlay {
            overlay.clear_input();
        }
    }

    /// Applies a pause-menu outcome to the ride.
    fn menu_event(&mut self, event: atv_ui::menu::controller::Event) {
        use atv_ui::menu::controller::Event;
        let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) else {
            return;
        };
        match event {
            Event::Resume | Event::Reset => {
                // Leaving the menu ends a Jukebox pause (stopJukebox).
                if let Some(sound) = &mut riding.sound {
                    sound.jukebox(atv_ui::menu::jukebox::JukeboxAction::Leave);
                }
                if event == Event::Reset
                    && let Err(error) = riding.reset()
                {
                    status(&format!("Reset failed: {error}"));
                }
                riding.paused = false;
                riding.menu_open = false;
                riding.clear_input();
                riding.sync_sound();
                if let Some(overlay) = &mut self.overlay {
                    overlay.sync_menu(false);
                    overlay.clear_input();
                }
            }
            Event::Leave(page) => {
                // Quit to Main Menu returns to the engine's own frontend.
                if page == "play.html" && self.quit_to_menu() {
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                if let Some(window) = web_sys::window() {
                    window.location().set_href(page).ok();
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = page;
            }
            Event::Fullscreen => {
                if let Some(window) = &self.window {
                    let on = window.fullscreen().is_none();
                    window
                        .set_fullscreen(on.then_some(winit::window::Fullscreen::Borderless(None)));
                }
            }
            Event::CopyRide => self.copy_ride_link(),
            Event::Spawn(peer) => {
                let Some(ready) = &mut self.ready else { return };
                let (Some(net), Some(riding)) = (&mut ready.net, &mut ready.riding) else {
                    return;
                };
                match net
                    .spawn_beside(peer, &riding.ride)
                    .and_then(|(origin, yaw, velocity)| riding.place_beside(origin, yaw, velocity))
                {
                    Ok(()) => self.menu_event(Event::Resume),
                    Err(error) => {
                        if let Some(menu) = self.overlay.as_mut().and_then(|o| o.menu.as_mut()) {
                            menu.notice = Some(error);
                        }
                    }
                }
            }
            Event::ChangeRide | Event::SelectTrack => {
                let garage = event == Event::ChangeRide;
                let Some(ready) = &mut self.ready else { return };
                if ready.net.is_none() || ready.riding.is_none() {
                    return;
                }
                let (Some(mut net), Some(riding)) = (ready.net.take(), ready.riding.take()) else {
                    return;
                };
                // Change ATV and Rider reloads in place (atv-web op 112's pose).
                if garage {
                    let m = riding.ride.rider_world();
                    net.resume_at = Some((riding.ride.frame().origin, m[8].atan2_d(m[10])));
                }
                let environment = net.environment.clone();
                self.held_room = Some(net);
                if let Some(front) = &self.front {
                    let mut f = front.borrow_mut();
                    f.state.room = Some(if garage { "garage" } else { "tracks" });
                    f.state.hosting = false;
                    if let Some(index) = f.state.tracks.iter().position(|t| t.id == environment) {
                        f.state.track = index;
                    }
                }
                if self.quit_to(Some(if garage { "garage" } else { "tracks" })) {
                    // The ride waits paused behind the frontend, still present.
                    self.held_ride = Some(riding.ride);
                } else if let Some(ready) = &mut self.ready {
                    // No frontend to show: keep riding in the room.
                    ready.net = self.held_room.take();
                    ready.riding = Some(riding);
                }
            }
            Event::LeaveGame => {
                if let Some(net) = self.ready.as_mut().and_then(|r| r.net.as_mut()) {
                    net.leave();
                }
                // Back to the main menu (shared Free Ride rooms return there
                // in the browser too; the Multiplayer page is not ported).
                if self.quit_to_menu() {
                    return;
                }
                #[cfg(target_arch = "wasm32")]
                if let Some(window) = web_sys::window() {
                    window.location().set_href("engine.html").ok();
                }
                if let Some(ready) = &mut self.ready {
                    ready.net = None;
                    if let Some(riding) = &mut ready.riding {
                        riding.online = false;
                        riding.peers.clear();
                        riding.server_clock = None;
                    }
                }
                if let Some(overlay) = &mut self.overlay {
                    overlay.lobby = None;
                }
                status("Left the multiplayer room.");
                self.menu_event(Event::Resume);
            }
            Event::Jukebox(action) => {
                if let Some(sound) = &mut riding.sound {
                    sound.jukebox(action);
                }
                self.save_soundtrack();
                self.wake();
            }
            Event::Changed => {
                if let Some(overlay) = &mut self.overlay {
                    overlay.set_help_keys(&riding.bindings);
                }
                self.apply_preferences();
                let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) else {
                    return;
                };
                // Audio → Sound applies to the running mixer at once.
                let enhanced = self
                    .overlay
                    .as_ref()
                    .is_some_and(|o| o.prefs.sound_mode == "enhanced");
                if let Some(sound) = &mut riding.sound {
                    use atv_game::audio::ride::SoundMode;
                    let mode = if enhanced {
                        SoundMode::Enhanced
                    } else {
                        SoundMode::Original
                    };
                    if sound.ride.mode != mode {
                        sound.ride.mode = mode;
                        if let Ok(mut mixer) = sound.output.mixer.lock() {
                            sound.ride.apply_mode(&mut mixer);
                        }
                    }
                }
            }
        }
    }

    /// music.js `save`: the soundtrack record keeps the play history.
    fn save_soundtrack(&mut self) {
        let Some(music) = &self.soundtrack else {
            return;
        };
        let mut music = music.borrow_mut();
        if !std::mem::take(&mut music.player.dirty) {
            return;
        }
        let front = self.front.clone();
        let mut front = front.as_ref().map(|f| f.borrow_mut());
        let settings = match (&mut self.overlay, front.as_deref_mut()) {
            (Some(overlay), _) if self.ready.is_some() => &mut overlay.settings,
            (_, Some(front)) => &mut front.settings,
            (Some(overlay), None) => &mut overlay.settings,
            (None, None) => return,
        };
        let mut record = settings
            .get("atv-soundtrack-v1")
            .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({"v": 3, "enabled": true, "volume": 0.45}));
        record["v"] = 3.into();
        record["history"] = music.player.history.clone().into();
        record["historyIndex"] = music.player.history_index.into();
        settings.set("atv-soundtrack-v1", &record.to_string());
    }

    /// The open menu's music view, and the Jukebox art on its first visit.
    fn update_jukebox(&mut self) {
        let (Some(ready), Some(overlay)) = (&mut self.ready, &mut self.overlay) else {
            return;
        };
        if overlay.menu.is_none() {
            return;
        }
        // jukebox.js drawBars: the analyser only while a song plays.
        overlay.spectrum = if overlay.jukebox_open() {
            ready
                .riding
                .as_mut()
                .and_then(|r| r.sound.as_mut())
                .and_then(|s| s.spectrum())
                .map(|bytes| bytes.to_vec())
        } else {
            None
        };
        overlay.music = ready
            .riding
            .as_ref()
            .and_then(|r| r.sound.as_ref())
            .map(|s| s.music_view());
        if overlay.jukebox_art_wanted && overlay.jukebox_art.is_none() {
            overlay.jukebox_art_wanted = false;
            if let Some(content) = ready
                .riding
                .as_ref()
                .and_then(|r| r.sound.as_ref())
                .map(|s| s.content())
                && self.requested.insert("jukebox-art".into())
            {
                self.spawn(
                    async move { atv_game::jukebox::load(&content).await },
                    |result| match result {
                        Ok(Some(art)) => Message::JukeboxArt(Box::new(art)),
                        Ok(None) => Message::Notice(String::new()),
                        Err(error) => Message::SoundFailed(format!("Jukebox art: {error}")),
                    },
                );
            }
        }
    }

    /// This ride's link (frontend.js `rideUrl`): this page with the ride's
    /// setup, or the room's invite, and the notice a copy shows.
    fn ride_link(&self) -> Option<(String, &'static str)> {
        let (ready, overlay) = (self.ready.as_ref()?, self.overlay.as_ref()?);
        let riding = ready.riding.as_ref()?;
        let selection = riding.ride.selection();
        let player = overlay
            .settings
            .get(atv_ui::front::controller::SELECTION_KEY)
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["playerName"].as_str().map(String::from))
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| "Rider".into());
        let race = riding.ride.race().map(|r| {
            let s = r.settings();
            (s.laps, s.cpu, s.difficulty, s.aggressive, s.all_out)
        });
        #[cfg(target_arch = "wasm32")]
        let page = web_sys::window()
            .and_then(|w| {
                let l = w.location();
                Some(format!("{}{}", l.origin().ok()?, l.pathname().ok()?))
            })
            .unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        let page = "https://furyremastered.com/engine.html".to_string();
        // In a room the row is Copy invite link (buildRideUrl's invite).
        if let Some(net) = &ready.net {
            return Some((
                net.invite(selection.quad, selection.rider),
                "Invite link copied.",
            ));
        }
        let url = atv_game::launch::ride_url(
            &page,
            &atv_game::launch::RideLink {
                map: &ready.venue.environment,
                quad: selection.quad,
                rider: selection.rider,
                player: &player,
                race,
                room: None,
            },
        );
        Some((url, "Ride link copied."))
    }

    /// After a WebGL context loss engine.js reopens this ride's link
    /// (`fury-resume` in session storage); menus clear it.
    fn remember_ride(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = web_sys::window().and_then(|w| w.session_storage().ok().flatten()) {
            let _ = match self.ride_link() {
                Some((url, _)) => storage.set_item("fury-resume", &url),
                None => storage.remove_item("fury-resume"),
            };
        }
    }

    /// Pause → Copy ride link (frontend.js `rideUrl` + launch.js
    /// `copyRideUrl`): the clipboard, or the link itself on the notice line
    /// when the copy is refused.
    fn copy_ride_link(&mut self) {
        let Some((url, copied_text)) = self.ride_link() else {
            return;
        };
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        let Some(menu) = &mut overlay.menu else {
            return;
        };
        #[cfg(target_arch = "wasm32")]
        {
            let clipboard = web_sys::window().map(|w| w.navigator().clipboard());
            match clipboard {
                Some(clipboard) => {
                    menu.notice = Some(url.clone());
                    let promise = clipboard.write_text(&url);
                    let proxy = self.proxy.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let copied = wasm_bindgen_futures::JsFuture::from(promise).await.is_ok();
                        let _ = proxy.send_event(Message::Notice(if copied {
                            copied_text.into()
                        } else {
                            url
                        }));
                    });
                }
                None => menu.notice = Some(url),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = copied_text;
            menu.notice = Some(url);
        }
        self.wake();
    }

    /// The saved choices game.js applies while riding (graphicsSettings,
    /// crashSettings, audio and soundtrack onChange): Speed rush, crashes,
    /// Unleashed flips, realistic train, effect and music volume, music on or
    /// off, and a changed Display → Graphics preset. Parity replays and
    /// autodrive benchmarks keep the defaults they were recorded with.
    fn apply_preferences(&mut self) {
        let (Some(overlay), Some(ready)) = (&self.overlay, &mut self.ready) else {
            return;
        };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        if riding.held || riding.autodrive.is_some() {
            return;
        }
        let prefs = &overlay.prefs;
        riding.set_speed_rush(&prefs.speed_rush);
        riding.crash_camera_tight = !prefs.crash_camera_original;
        riding.chase_assist = prefs.chase_assist;
        riding.vibration = prefs.vibration;
        riding.ride.options = atv_game::ride::RideOptions {
            flips: prefs.flips,
            realistic_train: prefs.realistic_train,
            arcade_flips: prefs.alpha("arcade-flips"),
        };
        // Settings → Alpha presentation (web/hype.js).
        riding.alpha_slowmo = prefs.alpha("air-slowmo");
        riding.alpha_big_air_camera = prefs.alpha("big-air-camera");
        riding.alpha_air_blur = prefs.alpha("air-blur");
        riding.set_alpha_muffle(prefs.alpha("music-muffle"));
        if riding.alpha_landing_burst && !prefs.alpha("landing-burst") {
            riding.burst.reset();
        }
        riding.alpha_landing_burst = prefs.alpha("landing-burst");
        riding.dirt.set_heavy(prefs.alpha("heavy-mud"));
        // Tire tracks read the (Heavy mud) zones game.js passes.
        if let Some(effects) = &mut riding.effects {
            effects.set_track_zones(*riding.dirt.zones());
        }
        let mode = atv_game::crash::CrashMode::parse(&prefs.crash_mode).unwrap_or_default();
        if riding
            .crashes
            .as_ref()
            .map_or(atv_game::crash::CrashMode::Original, |c| c.mode)
            != mode
        {
            riding.crashes = if mode == atv_game::crash::CrashMode::Original {
                None
            } else {
                let mut crashes = atv_game::crash::Crashes::new(mode);
                match crashes.reset(&riding.ride) {
                    Ok(()) => Some(crashes),
                    Err(error) => {
                        status(&format!("Crashes unavailable: {error}"));
                        None
                    }
                }
            };
        }
        if let Some(sound) = &mut riding.sound {
            sound.set_volumes(prefs.sfx_volume, prefs.music_volume, prefs.music_enabled);
        }
        // A preset changed in the menu applies at once where it can.
        if self.graphics_pref.as_deref() != Some(prefs.graphics.as_str()) {
            self.graphics_pref = Some(prefs.graphics.clone());
            let graphics = graphics_preset(prefs.graphics_resolved());
            riding.interpolation = graphics.interpolation;
            riding.wheel_reprojection = graphics.wheel_reprojection;
            riding.dirt_enabled = graphics.dirt;
            if let Some(effects) = &mut riding.effects
                && !riding.ride.is_motorcycle()
            {
                effects.set_lingering_dust(usize::from(graphics.dust), &ready.venue);
                effects.set_tire_tracks(
                    usize::from(graphics.tire_tracks.0),
                    f32::from(graphics.tire_tracks.1),
                    &ready.venue,
                );
                effects.set_track_zones(*riding.dirt.zones());
            }
            ready.scene.set_graphics(&ready.gpu, &ready.venue, graphics);
        }
    }

    fn act(&mut self, action: overlay::Action) {
        let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) else {
            return;
        };
        match action {
            overlay::Action::Pause => {
                if !riding.held {
                    riding.paused = !riding.paused;
                    riding.clear_input();
                    riding.sync_sound();
                    if let Some(overlay) = &mut self.overlay {
                        overlay.clear_input();
                    }
                }
            }
            overlay::Action::Camera => riding.cycle_requested = true,
            overlay::Action::RaceRetry => self.restart_race(),
            overlay::Action::RaceQuit => {
                self.quit_to(Some("single"));
            }
        }
    }

    /// Race over → Restart: race.js reloads the whole event (a new roster
    /// draw, no pre-race camera). With the engine's frontend the event
    /// reloads through the course loader; a direct ride restarts in place
    /// with the loaded roster.
    fn restart_race(&mut self) {
        let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) else {
            return;
        };
        let Some(settings) = riding.ride.race().map(|r| r.settings()) else {
            return;
        };
        let selection = riding.ride.selection();
        let environment = self
            .ready
            .as_ref()
            .map(|r| r.venue.environment.clone())
            .unwrap_or_default();
        if self.front.is_some() && self.quit_to(None) {
            self.start_ride(
                environment,
                selection.quad,
                selection.rider,
                Some(atv_game::ride::race::Settings {
                    seed: fresh_seed(),
                    intro: false,
                    ..settings
                }),
                None,
            );
            return;
        }
        let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) else {
            return;
        };
        if let Err(error) = riding.ride.restart_race() {
            if let Some(overlay) = &mut self.overlay {
                overlay.results.error = Some(error);
            }
            return;
        }
        riding.clear_input();
        if let Some(overlay) = &mut self.overlay {
            overlay.results = Default::default();
        }
    }

    /// winit touch events (pointer events on the web): toolbar buttons first,
    /// then the riding gestures.
    fn touch(&mut self, touch: winit::event::Touch) {
        use winit::event::TouchPhase;
        // Mobile browsers may grant activation only on a completed touch.
        // Retry there too; cancellation is never an activating gesture.
        if matches!(touch.phase, TouchPhase::Started | TouchPhase::Ended) {
            self.gesture();
        }
        let (x, y) = self.css((touch.location.x, touch.location.y));
        if self.front_pointer(touch.id, touch.phase, x, y) {
            return;
        }
        let now = (Instant::now() - self.started).as_secs_f64() * 1000.;
        let active = self
            .ready
            .as_ref()
            .and_then(|r| r.riding.as_ref())
            .is_some_and(|r| !r.paused && !r.held)
            && self.effective_focused();
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        let id = touch.id;
        if overlay.menu.is_some() {
            let bindings = self
                .ready
                .as_mut()
                .and_then(|r| r.riding.as_mut())
                .map(|r| &mut r.bindings);
            match touch.phase {
                TouchPhase::Started => overlay.menu_press(id, x, y, bindings),
                TouchPhase::Moved => overlay.menu_drag(id, x, y, bindings),
                TouchPhase::Ended => {
                    if let Some(event) = overlay.menu_release(id, x, y, bindings) {
                        self.menu_event(event);
                    }
                }
                TouchPhase::Cancelled => {
                    if let Some(menu) = &mut overlay.menu {
                        menu.cancel_pointer();
                    }
                }
            }
            return;
        }
        match touch.phase {
            TouchPhase::Started => {
                if !overlay.press(id, x, y) {
                    overlay.touch_down(id, x, y, now, active);
                    // A tap off the toolbar skips the race intro (game.js).
                    if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
                        riding.ride.skip_intro();
                    }
                }
            }
            TouchPhase::Moved => {
                if !overlay.drag(id, x, y) {
                    overlay.touch.moved(id, x, y, now);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let cancelled = touch.phase == TouchPhase::Cancelled;
                match overlay.release(id, x, y, cancelled) {
                    Some(Some(action)) => self.act(action),
                    Some(None) => {}
                    None => {
                        overlay.touch.up(id, x, y, cancelled, now);
                    }
                }
            }
        }
    }

    /// The first key, click or touch starts audio (browser autoplay rules).
    fn gesture(&mut self) {
        self.sound_unlocked = true;
        self.wake_lock.gesture();
        if let Some(sound) = self
            .ready
            .as_mut()
            .and_then(|r| r.riding.as_mut())
            .and_then(|r| r.sound.as_mut())
        {
            sound.start();
        } else if let Some(activation) = &self.activation {
            if let Err(error) = activation.activate() {
                status(&format!("Sound unavailable: {error}"));
            }
            // The frontend Jukebox's stream starts inside the gesture.
            if let Some(music) = &self.soundtrack {
                music.borrow_mut().start();
            }
        }
    }

    /// F3: Off -> Performance -> Debug, saved as the Settings row saves it.
    fn cycle_diagnostics(&mut self) {
        let mode = self.diagnostics.mode.next();
        self.set_diagnostics(mode);
        // Every settings holder keeps the choice (natively they rewrite one file).
        let key = atv_ui::diagnostics::KEY;
        let mut saved = false;
        if let Some(overlay) = &mut self.overlay {
            overlay.settings.set(key, mode.name());
            saved = true;
        }
        if let Some(front) = &self.front {
            front.borrow_mut().settings.set(key, mode.name());
            saved = true;
        }
        if !saved {
            atv_ui::settings::Settings::load().set(key, mode.name());
        }
        self.wake();
    }

    fn set_diagnostics(&mut self, mode: atv_ui::diagnostics::Mode) {
        self.diagnostics.mode = mode;
        self.diagnostics.updated = None;
        if let Some(overlay) = &mut self.overlay {
            overlay.prefs.diagnostics = mode.name().into();
        }
        if let Some(front) = &self.front {
            front.borrow_mut().set_diagnostics_pref(mode.name());
        }
    }

    /// One frame of the overlay: statistics, the menu row's choice, and the
    /// lines (rebuilt four times a second, as diagnostics.js `present`).
    fn update_diagnostics(&mut self) {
        use atv_ui::diagnostics::{Mode, number, vector};
        // Settings → Diagnostics in a menu changes the mode too.
        // (ride menu?, value): a change seen on the same menu is the player's.
        let pref = match (&self.ready, &self.overlay, &self.front) {
            (Some(_), Some(overlay), _) => Some((true, overlay.prefs.diagnostics.clone())),
            (None, _, Some(front)) => Some((false, front.borrow().diagnostics_pref().to_string())),
            _ => None,
        };
        if let (Some((ride, value)), Some((was_ride, was))) = (&pref, &self.diagnostics.pref)
            && ride == was_ride
            && value != was
            && let Some(mode) = Mode::parse(value)
        {
            self.diagnostics.mode = mode;
            self.diagnostics.updated = None;
        }
        self.diagnostics.pref = pref;
        let now = self.started.elapsed().as_secs_f64() * 1000.;
        let active = self.effective_focused();
        let mode = self.diagnostics.mode;
        if mode == Mode::Off {
            self.diagnostics.stats = Default::default();
            self.diagnostics.lines.clear();
        } else {
            let cpu = self.diagnostics.cpu;
            self.diagnostics.stats.sample(now, cpu, active);
            if self.diagnostics.updated.is_none_or(|t| now - t >= 250.) {
                self.diagnostics.updated = Some(now);
                let t = self.diagnostics.stats.snapshot();
                let riding = self.ready.as_ref().and_then(|r| r.riding.as_ref());
                let state = match riding {
                    Some(r) if r.paused => "PAUSED",
                    _ if !active => "UNFOCUSED",
                    Some(_) => "LIVE",
                    None => "MENU",
                };
                let mut lines = vec![
                    format!(
                        "{} · {state} · F3",
                        if mode == Mode::Debug {
                            "DEBUG"
                        } else {
                            "PERFORMANCE"
                        }
                    ),
                    format!(
                        "FPS {}  Frame {} ms",
                        number(t.fps, 1),
                        number(t.frame_ms, 1)
                    ),
                    format!(
                        "Frame p95 {} ms  CPU draw {} ms",
                        number(t.p95_ms, 1),
                        number(t.cpu_ms, 1)
                    ),
                ];
                if let Some(r) = riding {
                    lines.push(format!(
                        "Simulation {} ms / {} steps",
                        number(Some(r.sim_ms), 2),
                        r.steps_last_frame
                    ));
                }
                lines.push(format!(
                    "Draws {}  Triangles {}",
                    self.meter.draws, self.meter.triangles
                ));
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    lines.push(format!("Resolution {} × {}", size.width, size.height));
                }
                // The former debug strip (#status): load, error and frame lines.
                // Its " · " fields regroup into short lines so the panel stays
                // narrow, as the browser overlay's own lines are.
                let strip = LAST_STATUS.with_borrow(|s| s.clone());
                let mut line = String::new();
                for field in strip.split(" · ").filter(|f| !f.is_empty()) {
                    if !line.is_empty() && line.chars().count() + field.chars().count() > 44 {
                        lines.push(std::mem::take(&mut line));
                    }
                    if !line.is_empty() {
                        line.push_str(" · ");
                    }
                    line.push_str(field);
                }
                if !line.is_empty() {
                    lines.push(line);
                }
                if let (Mode::Debug, Some(r), Some(ready)) = (mode, riding, &self.ready) {
                    let f = r.ride.frame();
                    let applied = r.ride.applied();
                    let wheels = |v: [bool; 4]| v.map(|g| if g { "1" } else { "0" }).join(" ");
                    lines.extend([
                        format!(
                            "Map {}  Step {}",
                            ready.venue.environment.to_uppercase(),
                            r.ride.steps()
                        ),
                        format!("Position ft {}", vector(&f.origin)),
                        format!("Velocity ft/s {}", vector(&f.linear_velocity)),
                        format!("Contact FL FR RL RR {}", wheels(f.grounded)),
                        format!("Suspension ft {}", vector(&f.wheel_compression)),
                        format!(
                            "Stops {}  Body {}  Props {}",
                            f.suspension_stop_contacts,
                            f.chassis_contact_count,
                            f.obstacle_contact_count
                        ),
                        format!("Wreck {:?}  Resets {}", f.wreck.phase, f.reset_count),
                        format!("Water ft {}", number(Some(f64::from(f.water_height)), 2)),
                        format!(
                            "Input T {} B {} S {} L {}",
                            number(Some(f64::from(applied.throttle)), 2),
                            number(Some(f64::from(applied.brake)), 2),
                            number(Some(f64::from(applied.steer)), 2),
                            number(Some(f64::from(applied.lean)), 2)
                        ),
                        format!(
                            "Speed {} mph",
                            number(Some(f64::from(f.speed_magnitude) * 3600. / 5280.), 1)
                        ),
                    ]);
                }
                self.diagnostics.lines = lines;
            }
        }
        let lines = self.diagnostics.lines.clone();
        if self.ready.is_some() {
            if let Some(overlay) = &mut self.overlay {
                overlay.diagnostics = lines;
            }
        } else if let Some(front) = &self.front {
            front.borrow_mut().diagnostics = lines;
        }
    }

    /// Key releases: a binding capture saves when all keys are up; the
    /// bindings track held edge keys (bindings.js `observeKeys`).
    fn key_released(&mut self, code: KeyCode) {
        if let Some(front) = self.front_input() {
            let event = front.borrow_mut().key_up(&format!("{code:?}"));
            if let Some(event) = event {
                self.front_event(event);
            }
            return;
        }
        let names: HashSet<String> = self.keys.iter().map(|k| format!("{k:?}")).collect();
        let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) else {
            return;
        };
        if let Some(overlay) = self.overlay.as_mut().filter(|o| o.menu.is_some()) {
            if let Some(event) =
                overlay.menu_key_up(&format!("{code:?}"), Some(&mut riding.bindings))
            {
                self.menu_event(event);
            }
            return;
        }
        riding.key_released(&|c: &str| names.contains(c));
    }

    fn key_pressed(&mut self, code: KeyCode) {
        if let Some(front) = self.front_input() {
            let event = front.borrow_mut().key(&format!("{code:?}"));
            if let Some(event) = event {
                self.front_event(event);
            }
            return;
        }
        let bindings = self
            .ready
            .as_mut()
            .and_then(|r| r.riding.as_mut())
            .map(|r| &mut r.bindings);
        if let Some(event) = self
            .overlay
            .as_mut()
            .filter(|o| o.menu.is_some())
            .map(|o| o.menu_key(&format!("{code:?}"), bindings))
        {
            if let Some(event) = event {
                self.menu_event(event);
            }
            return;
        }
        // Race over: Escape and Enter drive the results menu (race.js).
        if let Some((true, action)) = self
            .overlay
            .as_mut()
            .filter(|_| self.ready.as_ref().is_some_and(|r| r.riding.is_some()))
            .map(|o| o.results_key(&format!("{code:?}")))
        {
            if let Some(action) = action {
                self.act(action);
            }
            self.wake();
            return;
        }
        let now = (Instant::now() - self.started).as_secs_f64();
        let names: HashSet<String> = self.keys.iter().map(|k| format!("{k:?}")).collect();
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            riding.key_pressed(&format!("{code:?}"), now, &|c: &str| names.contains(c));
            // Any key skips the pre-race camera (00212d40).
            riding.ride.skip_intro();
            // H stays a gameplay key when the player bound it (usesKey).
            let free = !riding.bindings.uses_key("KeyH");
            let riding = !riding.paused && !riding.held && free;
            if let Some(overlay) = &mut self.overlay {
                overlay.key(&format!("{code:?}"), riding);
            }
        }
    }

    /// Start sound loading after the first presented frame. Network/decode
    /// readiness must not gate the ride, as in the optimized browser host.
    fn load_sound(&mut self) {
        let Some(pending) = self.ready.as_mut().and_then(|r| r.pending_sound.take()) else {
            return;
        };
        let proxy = self.proxy.clone();
        let Some(activation) = self.activation.clone() else {
            return;
        };
        let music = self.soundtrack.clone();
        let future = async move {
            let result = sound::Sound::load(
                pending.content,
                &pending.environment,
                pending.settings,
                activation,
                music,
            )
            .await;
            let _ = proxy.send_event(match result {
                Ok(sound) => Message::SoundReady(Box::new(sound)),
                Err(error) => Message::SoundFailed(error),
            });
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(future);
        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(future);
    }
}

impl ApplicationHandler<Message> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = false;
        // Winit suppresses visibility events while a page is in the back/forward
        // cache. Refresh that independent cause when its pageshow resumes us.
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            self.visible = document.visibility_state() == web_sys::VisibilityState::Visible;
        }
        self.sync_activity();
        let Some(options) = self.options.take() else {
            return;
        };
        self.ui_backdrop = options.ui_backdrop;
        let attributes = Window::default_attributes().with_title("Fury Remastered");
        #[cfg(target_arch = "wasm32")]
        let attributes = {
            use winit::platform::web::WindowAttributesExtWebSys;
            attributes.with_append(true)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let attributes = attributes.with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        self.window = Some(window.clone());
        if let Some(start) = options.front.clone() {
            self.defaults = Some(Defaults {
                relay: options.relay.clone().unwrap_or_else(default_relay),
                graphics: options.graphics,
                graphics_set: options.graphics_set,
                weather: options.weather,
                crash: options.crash,
                sound: options.sound,
                start,
            });
            let display = self.display.clone();
            let store = options.store;
            let future = async move {
                let gpu = open_gpu(window, display).await?;
                let content = Rc::new(Content::open(store).await?);
                let boot = atv_ui::front::assets::boot(&content).await?;
                Ok::<_, String>(Booted { gpu, content, boot })
            };
            #[cfg(not(target_arch = "wasm32"))]
            if let Some((path, width, height)) = options.capture {
                let page = options
                    .front
                    .and_then(|f| f.page)
                    .unwrap_or_else(|| "title".into());
                match pollster::block_on(future) {
                    Ok(booted) => capture_front(booted, &page, &path, width, height),
                    Err(error) => status(&error),
                }
                event_loop.exit();
                return;
            }
            self.spawn(future, |result| match result {
                Ok(booted) => Message::FrontReady(Box::new(booted)),
                Err(error) => Message::Failed(error),
            });
            return;
        }
        let display = self.display.clone();
        let store = options.store;
        let (environment, fixed) = (options.venue, options.camera);
        let room = options.room.clone();
        // Rooms ride ATVs: a motorcycle choice falls back to ATV01.
        let profile = room.as_ref().map(|room| {
            atv_game::multiplayer::Profile::new(
                &room.name,
                options.selection.quad,
                options.selection.rider,
                options.appearance,
            )
        });
        let ride = multiplayer::RoomRide {
            graphics: options.graphics,
            weather: options.weather,
            crash: options.crash,
            sound: options.sound,
            appearance: options.appearance,
        };
        let world = World {
            selection: profile
                .as_ref()
                .map_or(options.selection, |p| atv_game::ride::Selection {
                    quad: p.quad,
                    rider: p.rider,
                }),
            appearance: options.appearance,
            race: options.race,
            weather: options.weather,
            steps: options.steps,
            graphics: options.graphics,
            crash: options.crash,
            ride: options.ride,
            script: options.script,
            sound: options.sound,
            autodrive: options.autodrive,
        };
        let future = async move {
            let gpu = open_gpu(window, display).await?;
            let content = Rc::new(Content::open(store).await?);
            // A room decides the course: the host starts it on the venue, a
            // guest rides whatever the room is riding.
            let (net, environment) = match (room, profile) {
                (Some(room), Some(profile)) => {
                    status("Joining the multiplayer room…");
                    let (net, environment) =
                        multiplayer::Net::join(&room, profile, &environment, content.clone(), ride)
                            .await?;
                    (Some(net), environment)
                }
                _ => (None, environment),
            };
            let mut ready = prepare(
                gpu,
                content,
                environment,
                fixed,
                world,
                Box::new(|_, text| status(text)),
            )
            .await?;
            if let Some(net) = &net {
                let selection = ready
                    .riding
                    .as_ref()
                    .map(|r| r.ride.selection())
                    .unwrap_or_default();
                status(&format!(
                    "Multiplayer room {} ({} riding). Invite: {}",
                    net.code(),
                    net.session.lobby.as_ref().map_or(1, |l| l.players.len()),
                    net.invite(selection.quad, selection.rider)
                ));
            }
            if net.is_some()
                && let Some(riding) = &mut ready.riding
            {
                riding.online = true;
            }
            ready.net = net;
            Ok::<_, String>(ready)
        };
        let proxy = self.proxy.clone();
        let send = move |result: Result<Ready, String>| {
            let _ = proxy.send_event(match result {
                Ok(ready) => Message::Ready(Box::new(ready)),
                Err(error) => Message::Failed(error),
            });
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { send(future.await) });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut result = pollster::block_on(future);
            if let (Ok(ready), Some((path, width, height))) = (&mut result, options.capture) {
                // A room capture first exchanges poses until other riders show.
                settle_room(ready);
                if let Some(riding) = &mut ready.riding {
                    (riding.crash_camera_tight, riding.chase_assist) = options.camera_choice;
                    let aspect = width as f32 / height as f32;
                    // About a second of held presentation, as the browser
                    // renders before its capture: the speed rush settles.
                    for _ in 0..60 {
                        riding
                            .present(&ready.gpu, &mut ready.scene, &ready.venue, aspect, 1. / 60.)
                            .expect("present ride");
                        let motion = riding.ride.frame();
                        ready.scene.set_speed_rush(
                            &ready.gpu,
                            riding.rush_amount(),
                            1. / 60.,
                            motion.origin,
                            motion.linear_velocity,
                        );
                    }
                    let (camera, focal) = riding
                        .present(&ready.gpu, &mut ready.scene, &ready.venue, aspect, 0.)
                        .expect("present ride");
                    if !ready.fixed {
                        ready.camera = camera;
                        ready.focal_length = focal;
                    }
                }
                if let Some(frames) = options.gpu_frames {
                    let view = View {
                        camera: ready.camera,
                        focal_length: ready.focal_length,
                    };
                    match ready
                        .scene
                        .gpu_scene_ms(&ready.gpu, &view, width, height, frames)
                    {
                        Some(ms) => println!(
                            "gpu scene pass {ms:.3} ms (median of {frames} frames, {width}x{height})"
                        ),
                        None => println!("gpu timing unavailable (no timestamp queries)"),
                    }
                }
                capture(ready, &path, width, height, options.capture_ui);
                event_loop.exit();
                return;
            }
            send(result);
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.suspended = true;
        self.sync_activity();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, message: Message) {
        match message {
            Message::SoundReady(mut sound) => {
                if let Some(ready) = &mut self.ready
                    && let Some(riding) = &mut ready.riding
                {
                    if let Err(error) = sound.configure(&ready.venue, &riding.ride) {
                        status(&format!("Sound unavailable: {error}"));
                        return;
                    }
                    // The saved soundtrack history (music.js v3 record), once
                    // per page: the frontend may have started the player.
                    let fresh = self
                        .soundtrack
                        .as_ref()
                        .is_none_or(|s| !Rc::ptr_eq(s, &sound.music));
                    self.soundtrack = Some(sound.music.clone());
                    if fresh
                        && let Some(record) = self
                            .overlay
                            .as_ref()
                            .and_then(|o| o.settings.get("atv-soundtrack-v1"))
                            .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
                    {
                        let history = record["history"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let index = record["historyIndex"].as_i64().unwrap_or(-1) as i32;
                        sound.music.borrow_mut().restore_history(history, index);
                    }
                    // Settings → Audio volumes and the music switch.
                    if let Some(prefs) = self.overlay.as_ref().map(|o| &o.prefs)
                        && !riding.held
                        && riding.autodrive.is_none()
                    {
                        sound.set_volumes(
                            prefs.sfx_volume,
                            prefs.music_volume,
                            prefs.music_enabled,
                        );
                    }
                    // A race already under way holds the soundtrack now.
                    if let Some(music) = riding.ride.race().and_then(|r| r.music_now()) {
                        sound.race(&atv_game::ride::race::StepEvents {
                            music: Some(music),
                            ..Default::default()
                        });
                    }
                    riding.sound = Some(*sound);
                    // A delayed bank can finish after blur/hide. Apply the
                    // live lifecycle before starting the already-unlocked sink.
                    riding.sync_sound();
                    if self.sound_unlocked {
                        riding.sound.as_mut().unwrap().start();
                    }
                }
                // One frame publishes the new sound state even while asleep.
                self.wake();
            }
            Message::SoundFailed(error) => status(&format!("Sound unavailable: {error}")),
            Message::SoundtrackReady(result) => {
                match result {
                    Ok(music) if self.soundtrack.is_none() => {
                        let mut music = *music;
                        // The saved soundtrack history (music.js v3 record).
                        if let Some(record) = self
                            .front
                            .as_ref()
                            .and_then(|f| f.borrow().settings.get("atv-soundtrack-v1"))
                            .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
                        {
                            let history = record["history"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default();
                            let index = record["historyIndex"].as_i64().unwrap_or(-1) as i32;
                            music.restore_history(history, index);
                        }
                        self.soundtrack_prefs = None;
                        self.soundtrack = Some(Rc::new(RefCell::new(music)));
                        self.sync_menu_music();
                    }
                    Ok(_) => {}
                    Err(error) => status(&format!("Soundtrack unavailable: {error}")),
                }
                self.wake();
            }
            Message::MenuTune(result) => {
                match result {
                    Ok(pcm) => self.menu_music.tune = Some(std::sync::Arc::new(pcm)),
                    Err(error) => status(&format!("Menu music: {error}")),
                }
                self.sync_menu_music();
            }
            Message::JukeboxArt(art) => {
                if let (Some(overlay), Some(ready)) = (&mut self.overlay, &self.ready) {
                    overlay.set_jukebox_art(&ready.gpu.device, &ready.gpu.queue, *art);
                } else if let Some(front) = &self.front {
                    front.borrow_mut().jukebox_source = Some(*art);
                }
                self.wake();
            }
            Message::Notice(text) => {
                if let Some(menu) = self.overlay.as_mut().and_then(|o| o.menu.as_mut()) {
                    menu.notice = Some(text);
                }
                self.wake();
            }
            Message::Ready(ready) => {
                if let Some(front) = &self.front {
                    front.borrow_mut().release_gpu();
                }
                self.ready = Some(*ready);
                // The ride takes over the music bus from the menu tune.
                self.sync_menu_music();
                // Rides the saved settings do not reach (direct and held
                // replays) still honour the camera and Vibration settings.
                if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
                    (riding.crash_camera_tight, riding.chase_assist) = self.camera_choice;
                    riding.vibration = saved_setting("atv-vibration").as_deref() != Some("off");
                }
                self.sync_activity();
                // Desktop has no autoplay rule: sound starts with the ride.
                if !cfg!(target_arch = "wasm32") {
                    self.gesture();
                }
                self.last = Instant::now();
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    let ready = self.ready.as_mut().unwrap();
                    ready.gpu.resize(size.width, size.height);
                    if ready.riding.is_some() {
                        let mut overlay = overlay::Overlay::new(
                            &ready.gpu.device,
                            ready.gpu.config.format,
                            overlay::viewport(size.width, size.height, window.scale_factor()),
                        );
                        overlay.backdrop = self.ui_backdrop;
                        overlay.frontend = self.front.is_some();
                        overlay.set_course(&ready.venue);
                        if let Some(assets) = ready.hud_assets.take() {
                            overlay.set_meter(&ready.gpu.device, &ready.gpu.queue, assets);
                        }
                        // Saved bindings and controller tuning (play.html's keys).
                        if let Some(riding) = &mut ready.riding
                            && !riding.held
                            && riding.autodrive.is_none()
                        {
                            riding.bindings =
                                atv_ui::menu::controller::load_bindings(&mut overlay.settings);
                            overlay.set_help_keys(&riding.bindings);
                        }
                        overlay.prefs.graphics_detected = detect_graphics_tier().into();
                        // RideEffectsSettings: Speed rush defaults to Off with
                        // reduced motion.
                        if overlay.settings.get("atv-speed-rush").is_none() && reduced_motion() {
                            overlay.prefs.speed_rush = "off".into();
                        }
                        self.graphics_pref = Some(overlay.prefs.graphics.clone());
                        self.overlay = Some(overlay);
                    }
                    window.request_redraw();
                }
                self.apply_preferences();
                self.remember_ride();
            }
            Message::Failed(error) => {
                status(&error);
                if self.front.is_some() && self.gpu.is_none() && self.ready.is_none() {
                    // The ride did not load: back to the course select on a
                    // new surface (the failed load held the old one).
                    if let Some(window) = self.window.clone() {
                        let display = self.display.clone();
                        self.spawn(open_gpu(window, display), |result| match result {
                            Ok(gpu) => Message::FrontGpu(Box::new(gpu)),
                            Err(error) => Message::Failed(error),
                        });
                    }
                    return;
                }
                // Nothing left to show: keep the error on screen without ?debug=1.
                fatal();
                if !cfg!(target_arch = "wasm32") {
                    event_loop.exit();
                }
            }
            Message::FreeRideRiders(riders) => {
                if let Some(front) = &self.front {
                    let mut f = front.borrow_mut();
                    f.state.freeride_riders = riders;
                }
                self.wake();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Message::PresenceTick => self.presence_tick(),
            Message::RoomJoined(result) => self.room_joined(*result),
            Message::LobbyNotice(text) => {
                if let Some(front) = &self.front {
                    front.borrow_mut().state.notice = Some(text);
                }
                self.wake();
            }
            Message::FrontText(action, text) => {
                if let Some(front) = &self.front {
                    front.borrow_mut().state.set_field(action, &text);
                }
                self.wake();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Message::FrontFocus(index) => {
                if let Some(front) = &self.front {
                    front.borrow_mut().state.focus_field(index);
                }
                self.wake();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Message::RemoteEngine(quad, engine) => match *engine {
                Ok(engine) => {
                    if let Some(sound) = self
                        .ready
                        .as_mut()
                        .and_then(|r| r.riding.as_mut())
                        .and_then(|r| r.sound.as_mut())
                    {
                        sound.remote.add_engine(quad, engine);
                    }
                }
                Err(error) => status(&format!("Remote engine sound unavailable: {error}")),
            },
            Message::PeerModel(model) => {
                if let Some(net) = self.ready.as_mut().and_then(|r| r.net.as_mut()) {
                    net.model_loaded(*model);
                }
            }
            Message::FrontReady(booted) => self.front_ready(*booted),
            Message::FrontArt(piece) => {
                if let Some(front) = &self.front {
                    let mut front = front.borrow_mut();
                    match *piece {
                        ArtPiece::Photos(photos) => front.art.photos.extend(photos),
                        ArtPiece::Tips(tips) => front.set_tips(tips),
                        ArtPiece::Quads(quads) => front.art.quads = Some(quads),
                        ArtPiece::TopSpeed(quad, speed) => {
                            front.art.top_speed.insert(quad, speed);
                        }
                        ArtPiece::Shots(environment, shots) => {
                            front.art.shots = Some((environment, shots))
                        }
                        ArtPiece::Vehicle(quad, model, rider) => {
                            front.add_vehicle(quad, model, rider);
                            let kept: Vec<u16> =
                                front.art.vehicles.iter().map(|(q, ..)| *q).collect();
                            self.requested.retain(|k| {
                                k.strip_prefix("vehicle:")
                                    .and_then(|q| q.parse::<u16>().ok())
                                    .is_none_or(|q| kept.contains(&q) || q == quad)
                            });
                        }
                        ArtPiece::MenuRider(rider, model) => {
                            front.art.menu_riders.insert(rider, model);
                        }
                    }
                }
                self.wake();
            }
            Message::FrontGpu(gpu) => {
                let mut gpu = *gpu;
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    gpu.resize(size.width, size.height);
                }
                self.gpu = Some(gpu);
                if let Some(front) = &self.front {
                    front.borrow_mut().resume("tracks");
                }
                self.wake();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // Input and resizes reach a sleeping ride through one frame.
        if matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::Touch(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            self.wake();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(ready) = &mut self.ready {
                    ready.gpu.resize(size.width, size.height);
                }
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
                if let (Some(front), Some(window)) = (&self.front, &self.window) {
                    front.borrow_mut().set_view(overlay::viewport(
                        size.width,
                        size.height,
                        window.scale_factor(),
                    ));
                }
                if let (Some(overlay), Some(window)) = (&mut self.overlay, &self.window) {
                    overlay.set_view(overlay::viewport(
                        size.width,
                        size.height,
                        window.scale_factor(),
                    ));
                }
            }
            WindowEvent::Touch(touch) => self.touch(touch),
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => -y * 40.,
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        -(p.y / self.window.as_ref().map_or(1., |w| w.scale_factor())) as f32
                    }
                };
                if let Some(front) = self.front_input() {
                    front.borrow_mut().wheel(dy);
                } else if let Some(overlay) = &mut self.overlay {
                    overlay.menu_wheel(dy);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Desktop typing into a focused Host/Join field (the browser
                // build types into its own <input>).
                if event.state == ElementState::Pressed
                    && let Some(text) = &event.text
                    && !text.chars().any(char::is_control)
                    && let Some(front) = self.front_input()
                    && front.borrow_mut().state.type_text(text)
                {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    // F3 belongs to the diagnostics overlay everywhere.
                    if code == KeyCode::F3 {
                        if event.state == ElementState::Pressed && !event.repeat {
                            self.cycle_diagnostics();
                        }
                        return;
                    }
                    match event.state {
                        ElementState::Pressed => {
                            self.gesture();
                            if !event.repeat && self.keys.insert(code) {
                                self.key_pressed(code);
                            }
                        }
                        ElementState::Released => {
                            self.keys.remove(&code);
                            self.key_released(code);
                        }
                    };
                }
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.sync_activity();
            }
            WindowEvent::Occluded(occluded) => {
                self.visible = !occluded;
                self.sync_activity();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.css(self.cursor.unwrap_or_default());
                let phase = if state == ElementState::Pressed {
                    winit::event::TouchPhase::Started
                } else {
                    winit::event::TouchPhase::Ended
                };
                if self.front_pointer(MOUSE, phase, x, y) {
                    return;
                }
                if let Some(overlay) = self.overlay.as_mut().filter(|o| o.menu.is_some()) {
                    self.dragging = false;
                    let bindings = self
                        .ready
                        .as_mut()
                        .and_then(|r| r.riding.as_mut())
                        .map(|r| &mut r.bindings);
                    if state == ElementState::Pressed {
                        overlay.menu_press(overlay::MOUSE, x, y, bindings);
                    } else if let Some(event) = overlay.menu_release(overlay::MOUSE, x, y, bindings)
                    {
                        self.menu_event(event);
                    }
                    return;
                }
                if state == ElementState::Pressed {
                    self.gesture();
                    self.mouse_ui = self
                        .overlay
                        .as_mut()
                        .is_some_and(|o| o.press(overlay::MOUSE, x, y));
                    self.dragging = !self.mouse_ui;
                    if !self.mouse_ui
                        && let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut())
                    {
                        riding.ride.skip_intro();
                    }
                } else {
                    self.dragging = false;
                    if std::mem::take(&mut self.mouse_ui)
                        && let Some(Some(action)) = self
                            .overlay
                            .as_mut()
                            .and_then(|o| o.release(overlay::MOUSE, x, y, false))
                    {
                        self.act(action);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.front_input().is_some() {
                    self.cursor = Some((position.x, position.y));
                    let (x, y) = self.css((position.x, position.y));
                    self.front_pointer(MOUSE, winit::event::TouchPhase::Moved, x, y);
                    return;
                }
                if let (true, Some((x, _)), Some(Some(_))) = (
                    self.dragging,
                    self.cursor,
                    self.ready.as_ref().map(|r| r.riding.as_ref()),
                ) {
                    self.drag += (position.x - x) as f32;
                } else if let (true, Some((x, y)), Some(ready)) =
                    (self.dragging, self.cursor, &mut self.ready)
                {
                    ready.camera.yaw -= ((position.x - x) * 0.005) as f32;
                    ready.camera.pitch =
                        (ready.camera.pitch - ((position.y - y) * 0.005) as f32).clamp(-1.5, 1.5);
                }
                self.cursor = Some((position.x, position.y));
                let (x, y) = self.css((position.x, position.y));
                if let Some(overlay) = &mut self.overlay {
                    if overlay.menu.is_some() {
                        let bindings = self
                            .ready
                            .as_mut()
                            .and_then(|r| r.riding.as_mut())
                            .map(|r| &mut r.bindings);
                        overlay.menu_drag(overlay::MOUSE, x, y, bindings);
                    } else {
                        overlay.hover(x, y);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.step();
                self.load_sound();
                if self.awake() {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                } else {
                    self.asleep = true;
                    self.pad_buttons = u32::MAX;
                }
            }
            _ => {}
        }
    }

    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            if self.asleep {
                self.paused_tick();
            }
            #[cfg(not(target_arch = "wasm32"))]
            self.presence_tick();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(receive) = &self.riders_pending {
            match receive.try_recv() {
                Ok(riders) => {
                    self.riders_pending = None;
                    self.user_event(event_loop, Message::FreeRideRiders(riders));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + std::time::Duration::from_millis(250),
                    ));
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.riders_pending = None,
            }
        }
        self.poll_riders();
        // A hidden or suspended room ride draws no frames: a 1 s timer keeps
        // its presence (the browser's throttled background timers).
        #[cfg(not(target_arch = "wasm32"))]
        if self.presence_due() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_secs(1),
            ));
            return;
        }
        // The main menu refreshes its Free Ride count every 15 s.
        if self.ready.is_none()
            && self
                .front
                .as_ref()
                .is_some_and(|f| f.borrow().state.page == "main")
            && let Some(polled) = self.riders_polled
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                polled + std::time::Duration::from_secs(15),
            ));
            return;
        }
        // Asleep in focus (paused): a 50 ms tick; away: wait for events.
        event_loop.set_control_flow(if self.asleep && self.effective_focused() {
            ControlFlow::WaitUntil(Instant::now() + std::time::Duration::from_millis(50))
        } else {
            ControlFlow::Wait
        });
    }
}

/// The frontend's mouse pointer id (touches use their own ids).
const MOUSE: u64 = overlay::MOUSE;

impl App {
    /// Loads that must not hold up a ride frame (another rider's model or
    /// engine bank): polled a slice at a time from the frame loop, on both
    /// web and desktop; they `yield_frame` between their steps.
    fn spawn_frame<T: 'static>(
        &mut self,
        future: impl std::future::Future<Output = T> + 'static,
        message: impl FnOnce(T) -> Message + 'static,
    ) {
        self.frame_tasks
            .push(Box::pin(async move { message(future.await) }));
    }

    /// Advances frame-spread loads for about 3 ms of this frame, starting
    /// with a different load each frame.
    fn poll_frame_tasks(&mut self) {
        if self.frame_tasks.is_empty() {
            return;
        }
        let started = Instant::now();
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        let mut index = 0;
        while index < self.frame_tasks.len() {
            if index > 0 && started.elapsed().as_secs_f64() > 0.003 {
                break;
            }
            let slice = Instant::now();
            let poll = self.frame_tasks[index].as_mut().poll(&mut cx);
            self.meter.load_max = self
                .meter
                .load_max
                .max(slice.elapsed().as_secs_f64() * 1000.);
            match poll {
                std::task::Poll::Ready(message) => {
                    drop(self.frame_tasks.remove(index));
                    let _ = self.proxy.send_event(message);
                }
                std::task::Poll::Pending => index += 1,
            }
        }
        if !self.frame_tasks.is_empty() {
            self.frame_tasks.rotate_left(1);
        }
    }

    /// Runs `future` (web: on the event loop; desktop: now) and posts its
    /// outcome as a message.
    fn spawn<T: 'static>(
        &self,
        future: impl std::future::Future<Output = T> + 'static,
        message: impl FnOnce(T) -> Message + 'static,
    ) {
        let proxy = self.proxy.clone();
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let _ = proxy.send_event(message(future.await));
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = proxy.send_event(message(pollster::block_on(future)));
        }
    }

    /// audio.js setMusicPreview + playMenuMusic: on the frontend menus, while
    /// the page is focused and Music is on, the original UI tune loops on the
    /// music bus at the music volume; a ride (or losing focus) stops it.
    fn sync_menu_music(&mut self) {
        let Some(output) = self.activation.as_ref().and_then(|a| a.started_output()) else {
            return;
        };
        let front_shown = self.ready.is_none() && self.gpu.is_some();
        let (enabled, volume) = self
            .front
            .as_ref()
            .filter(|_| front_shown)
            .map_or((false, 0.), |f| f.borrow().music());
        // The Jukebox (or an Audio page song) plays the soundtrack instead of
        // the menu tune (setMusicPreview's `originalMenu`).
        let preview = front_shown
            && self
                .front
                .as_ref()
                .is_some_and(|f| f.borrow().wants_soundtrack());
        let focused = self.effective_focused();
        if front_shown {
            self.sync_soundtrack(preview, focused, (enabled, volume));
        }
        let wanted = front_shown && enabled && focused && !preview;
        if wanted
            && self.menu_music.tune.is_none()
            && !self.menu_music.requested
            && let Some(content) = self.content.clone()
        {
            self.menu_music.requested = true;
            let rate = output.rate;
            self.spawn(
                async move {
                    atv_game::audio::menu::load_tune(&content, rate)
                        .await
                        .map_err(|e| e.to_string())
                },
                Message::MenuTune,
            );
        }
        let Ok(mut mixer) = output.mixer.lock() else {
            return;
        };
        match (&self.menu_music.tune, wanted) {
            (Some(tune), true) => {
                mixer.music_volume = volume;
                mixer.music_active = true;
                if !mixer.playing(self.menu_music.voice) {
                    self.menu_music.voice = Some(mixer.play(
                        tune,
                        atv_game::audio::mixer::Looping::Markers,
                        [1., 1.],
                        1.,
                        atv_game::audio::mixer::Bus::Music,
                        false,
                        "menu",
                    ));
                }
                drop(mixer);
                // A page shown again resumes an output the browser suspended.
                output.start();
            }
            _ => {
                if let Some(voice) = self.menu_music.voice.take() {
                    mixer.stop(voice);
                }
                if front_shown {
                    // The decoded soundtrack also plays on the music bus.
                    mixer.music_volume = volume;
                    mixer.music_active = preview && focused;
                }
            }
        }
    }

    /// The frontend's soundtrack preview: load the player on first use, run
    /// it while the Jukebox wants it, pause it under the menus otherwise,
    /// and publish its view to the Jukebox page.
    fn sync_soundtrack(&mut self, preview: bool, focused: bool, prefs: (bool, f32)) {
        if preview && self.soundtrack.is_none() {
            self.load_soundtrack();
        }
        let Some(music) = self.soundtrack.clone() else {
            return;
        };
        let jukebox = self
            .front
            .as_ref()
            .is_some_and(|f| f.borrow().jukebox_open());
        {
            let mut music = music.borrow_mut();
            if self.soundtrack_prefs != Some(prefs) {
                self.soundtrack_prefs = Some(prefs);
                music.set_volume(prefs.1, prefs.0);
            }
            music.set_active(preview);
            if preview {
                music.set_focused(focused);
                music.maintain();
            }
        }
        if let Some(front) = &self.front {
            let mut front = front.borrow_mut();
            let mut music = music.borrow_mut();
            front.music = Some(music.music_view());
            front.spectrum = if jukebox {
                music.spectrum().map(|b| b.to_vec())
            } else {
                None
            };
        }
        // The original art loads on the first visit (jukebox.js loadArt).
        if jukebox
            && self
                .front
                .as_ref()
                .is_some_and(|f| f.borrow().jukebox_source.is_none())
            && let Some(content) = self.content.clone()
            && self.requested.insert("front-jukebox-art".into())
        {
            self.spawn(
                async move { atv_game::jukebox::load(&content).await },
                |result| match result {
                    Ok(Some(art)) => Message::JukeboxArt(Box::new(art)),
                    Ok(None) => Message::Notice(String::new()),
                    Err(error) => Message::SoundFailed(format!("Jukebox art: {error}")),
                },
            );
        }
        self.save_soundtrack();
    }

    /// Reads the soundtrack catalog for the frontend (once per page).
    fn load_soundtrack(&mut self) {
        if self.soundtrack_requested {
            return;
        }
        let (Some(content), Some(activation)) = (self.content.clone(), self.activation.clone())
        else {
            return;
        };
        let music = self.front.as_ref().is_none_or(|f| f.borrow().music().0);
        self.soundtrack_requested = true;
        self.spawn(
            async move {
                sound::Soundtrack::load(content, &activation, music)
                    .await
                    .map(Box::new)
            },
            Message::SoundtrackReady,
        );
    }

    /// The frontend while it owns the screen (no ride, not loading).
    fn front_input(&self) -> Option<Rc<RefCell<front::Front>>> {
        self.front
            .clone()
            .filter(|_| self.ready.is_none() && self.gpu.is_some())
    }

    fn front_ready(&mut self, booted: Booted) {
        let Booted {
            mut gpu,
            content,
            boot,
        } = booted;
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        gpu.resize(size.width, size.height);
        let view = overlay::viewport(size.width, size.height, window.scale_factor());
        let tracks = boot.tracks.clone();
        let front = front::Front::new(
            &gpu.device,
            gpu.config.format,
            view,
            boot.tracks,
            boot.backdrop,
        );
        self.front = Some(Rc::new(RefCell::new(front)));
        self.gpu = Some(gpu);
        self.content = Some(content.clone());
        // Course photos, loader tips and ATV stats load behind the title.
        // Training ships no TP photo pair: play.html shows the approved
        // capture of the venue from the site's dev log images.
        #[cfg(target_arch = "wasm32")]
        if tracks.iter().any(|t| t.id == "training") {
            self.spawn(
                web_image("/devlog-images/training-map-preview.png"),
                |image| {
                    Message::FrontArt(Box::new(ArtPiece::Photos(
                        image
                            .map(|i| vec![("training".to_string(), i)])
                            .unwrap_or_default(),
                    )))
                },
            );
        }
        let c = content.clone();
        self.spawn(
            async move { atv_ui::front::assets::photos(&c, &tracks).await },
            |result| Message::FrontArt(Box::new(ArtPiece::Photos(result.unwrap_or_default()))),
        );
        let c = content.clone();
        self.spawn(
            async move { atv_ui::front::assets::tips(&c).await },
            |result| Message::FrontArt(Box::new(ArtPiece::Tips(result.unwrap_or_default()))),
        );
        if content
            .manifest
            .record("/UI/RESOURCE/QUADINFO.RES")
            .is_some()
        {
            let c = content.clone();
            self.spawn(
                async move { atv_ui::front::assets::quad_catalog(&c).await },
                |result| match result {
                    Ok(quads) => Message::FrontArt(Box::new(ArtPiece::Quads(quads))),
                    Err(error) => Message::SoundFailed(format!("ATV stats: {error}")),
                },
            );
        }
        self.open_launch();
        window.request_redraw();
    }

    /// launch.js `openLaunch`: ride links start their ride, `?menu=`
    /// shortcuts and setup-only links open a menu, anything else the title.
    fn open_launch(&mut self) {
        let Some(front) = self.front.clone() else {
            return;
        };
        let start = self
            .defaults
            .as_ref()
            .map(|d| d.start.clone())
            .unwrap_or_default();
        let mut f = front.borrow_mut();
        let view = f.view;
        if let Some(page) = &start.page {
            let page = page.clone();
            f.state.show(&page, &view);
            return;
        }
        if let Some(page) = atv_game::launch::menu_shortcut(&start.search) {
            // Without the race setup page (gap 1), Single Player.
            f.state
                .show(if page == "race" { "single" } else { page }, &view);
            return;
        }
        let tracks: Vec<(String, String)> = f
            .state
            .tracks
            .iter()
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect();
        let setup = atv_game::launch::parse_launch(&start.search, &tracks);
        if !setup.requested {
            return;
        }
        let mut errors = setup.errors.clone();
        // frontend.js refuses motorcycles online (Free Ride falls back).
        if setup.mode == "multiplayer" && setup.quad_id >= 27 {
            errors.push(
                "Choose an ATV for multiplayer. Motorcycles are available in Single Player.".into(),
            );
        }
        if !errors.is_empty() {
            f.state.show("main", &view);
            status(&format!("Cannot open ride link. {}", errors.join(" ")));
            return;
        }
        f.state.quad_id = setup.quad_id;
        f.state.rider_id = setup.rider_id;
        if let Some(index) = f.state.tracks.iter().position(|t| t.id == setup.map) {
            f.state.track = index;
        }
        // openLaunch: a multiplayer link opens Create or Join Lobby with its
        // code and name (the saved name unless the link names one) and, with
        // play=1, hosts or joins at once; a join reopens an ended room on the
        // link's map.
        if setup.mode == "multiplayer" {
            let create = setup.action == Some("create");
            if setup.player_name != "Rider" || f.state.player_name.is_empty() {
                f.state.player_name = setup.player_name.clone();
            }
            if create {
                f.state.create_code = setup.room.clone();
            } else {
                f.state.room_code = setup.room.clone();
            }
            f.state
                .show(if create { "create-lobby" } else { "join-lobby" }, &view);
            if !setup.play {
                return;
            }
            let name = f.state.player_name.clone();
            {
                let f = &mut *f;
                f.state.save(&mut f.settings);
            }
            drop(f);
            use atv_ui::front::controller::Event;
            self.front_event(if create {
                Event::Host {
                    environment: setup.map,
                    code: (!setup.room.is_empty()).then_some(setup.room),
                    quad: setup.quad_id,
                    rider: setup.rider_id,
                    name,
                }
            } else {
                Event::Join {
                    code: setup.room,
                    environment: setup.map,
                    quad: setup.quad_id,
                    rider: setup.rider_id,
                    name,
                }
            });
            return;
        }
        let race = setup.activity == "race";
        f.state.show(if race { "single" } else { "tracks" }, &view);
        if !setup.play {
            return;
        }
        // A launched link becomes the saved selection, as in the browser game.
        {
            let f = &mut *f;
            f.state.save(&mut f.settings);
        }
        let settings = race.then(|| {
            let defaults = atv_game::ride::race::Settings::default();
            atv_game::ride::race::Settings {
                laps: setup.race.laps,
                cpu: setup.race.cpu as usize,
                difficulty: setup.race.difficulty,
                // The original EATDIRT and BRINGIT cheats (cheats=).
                aggressive: setup.race.aggressive,
                all_out: setup.race.all_out,
                // A new roster draw per load, as the browser draws one.
                seed: fresh_seed(),
                ..defaults
            }
        });
        drop(f);
        self.start_ride(setup.map, setup.quad_id, setup.rider_id, settings, None);
    }

    fn front_event(&mut self, event: atv_ui::front::controller::Event) {
        use atv_ui::front::controller::Event;
        match event {
            Event::Ride {
                environment,
                quad,
                rider,
            } => self.start_ride(environment, quad, rider, None, None),
            Event::FreeRide { quad, rider, name } => {
                let Some(relay) = self.defaults.as_ref().map(|d| d.relay.clone()) else {
                    return;
                };
                // Many riders keep the default name: number it so strangers
                // can tell them apart (frontend.js).
                let name = if name == "Rider" {
                    format!("Rider {}", 1000 + fresh_seed() % 9000)
                } else {
                    name
                };
                let room = RoomStart {
                    relay,
                    code: None,
                    host: false,
                    freeride: true,
                    name,
                    drop_after: None,
                    train_place: None,
                };
                self.start_ride(
                    atv_game::multiplayer::FREERIDE_ENVIRONMENT.into(),
                    quad,
                    rider,
                    None,
                    Some(room),
                );
            }
            Event::RoomRide {
                quad,
                rider,
                environment,
            } => {
                self.held_ride = None;
                let Some(mut net) = self.held_room.take() else {
                    // The room ended meanwhile: back to the main menu.
                    if let Some(front) = &self.front {
                        front.borrow_mut().resume("main");
                    }
                    self.wake();
                    return;
                };
                let same = net
                    .session
                    .profile()
                    .is_some_and(|p| p.quad == quad && p.rider == rider);
                if !same && let Err(error) = net.change_profile(quad, rider) {
                    status(&format!("Change ATV and Rider: {error}"));
                }
                // The host's Ride moves everyone; riders follow from the start.
                let environment = match environment {
                    Some(environment) if net.session.is_host() => {
                        if let Err(error) = net.set_map(&environment) {
                            status(&error);
                        }
                        net.resume_at = None;
                        environment
                    }
                    _ => net
                        .session
                        .environment()
                        .map_or_else(|| net.environment.clone(), String::from),
                };
                if environment != net.environment {
                    net.resume_at = None;
                }
                self.pending_room = Some(net);
                self.start_ride(environment, quad, rider, None, None);
            }
            Event::Host {
                environment,
                code,
                quad,
                rider,
                name,
            } => {
                let Some(relay) = self.defaults.as_ref().map(|d| d.relay.clone()) else {
                    return;
                };
                let room = RoomStart {
                    relay,
                    code,
                    host: true,
                    freeride: false,
                    name,
                    drop_after: None,
                    train_place: None,
                };
                self.start_ride(environment, quad, rider, None, Some(room));
            }
            Event::Join {
                code,
                environment,
                quad,
                rider,
                name,
            } => {
                let Some(relay) = self.defaults.as_ref().map(|d| d.relay.clone()) else {
                    return;
                };
                let room = RoomStart {
                    relay,
                    code: Some(code),
                    host: false,
                    freeride: false,
                    name,
                    drop_after: None,
                    train_place: None,
                };
                self.start_ride(environment, quad, rider, None, Some(room));
            }
            Event::LeaveLobby => {
                // The connect finishes in the background and is left at once.
                self.joining = None;
                if let Some((mut net, ..)) = self.joined.take() {
                    net.leave();
                }
                self.wake();
            }
            Event::CopyInvite => self.copy_lobby_invite(),
            Event::DevLog =>
            {
                #[cfg(target_arch = "wasm32")]
                if let Some(window) = web_sys::window() {
                    window.location().set_href("devlog.html").ok();
                }
            }
            Event::Fullscreen => {
                if let Some(window) = &self.window {
                    let on = window.fullscreen().is_none();
                    window
                        .set_fullscreen(on.then_some(winit::window::Fullscreen::Borderless(None)));
                }
            }
            Event::Settings => {}
            Event::Jukebox => {
                self.sync_menu_music();
                self.wake();
            }
            Event::Music(action) => {
                if let Some(music) = &self.soundtrack {
                    music.borrow_mut().jukebox(action);
                }
                self.save_soundtrack();
                self.sync_menu_music();
                self.wake();
            }
        }
    }

    /// Shows the course loader and loads the ride on the frontend's GPU.
    /// Claims the room and connects with the frontend still live on the
    /// Free Ride Lobby page; [`Message::RoomJoined`] then loads the ride.
    fn join_room(&mut self, environment: String, quad: u16, rider: u16, room: RoomStart) {
        let (Some(front), Some(content)) = (self.front.clone(), self.content.clone()) else {
            return;
        };
        let (world, ride) = self.ride_setup(&front, quad, rider, None);
        let form = if room.host {
            "create-lobby"
        } else {
            "join-lobby"
        };
        {
            let mut f = front.borrow_mut();
            let view = f.view;
            f.state
                .show_lobby(room.code.as_deref().unwrap_or(""), &environment, &view);
        }
        self.joining = Some((quad, rider, form));
        let profile =
            atv_game::multiplayer::Profile::new(&room.name, quad, rider, world.appearance);
        let delay = multiplayer::JOIN_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed);
        let future = async move {
            if delay > 0 {
                net::sleep(delay).await;
            }
            multiplayer::Net::join(&room, profile, &environment, content, ride).await
        };
        self.spawn(future, |result| Message::RoomJoined(Box::new(result)));
        self.wake();
    }

    /// The room connected: load its course (the loader replaces the lobby
    /// page), or leave it when Leave Game came first.
    fn room_joined(&mut self, result: Result<(multiplayer::Net, String), String>) {
        let Some((quad, rider, form)) = self.joining.take() else {
            if let Ok((mut net, _)) = result {
                net.leave();
            }
            return;
        };
        match result {
            Ok((net, environment)) => {
                // The lobby page fills in the room's code and riders; the
                // next front frame draws it, then the loader takes over.
                if let Some(front) = &self.front {
                    let mut f = front.borrow_mut();
                    if let Some(lobby) = &mut f.state.lobby {
                        lobby.code.clone_from(&net.session.code);
                        lobby.status = net.menu_lobby().map(|l| l.status).unwrap_or_default();
                    }
                }
                let hold = multiplayer::JOIN_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed);
                let at = Instant::now() + std::time::Duration::from_millis(u64::from(hold));
                self.joined = Some((net, environment, quad, rider, at));
                self.wake();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(error) => {
                status(&error);
                // multiplayer.js reports a failed host/join on the form.
                if let Some(front) = &self.front {
                    let mut f = front.borrow_mut();
                    f.state.lobby = None;
                    let view = f.view;
                    f.state.show(form, &view);
                    f.state.notice = Some(error);
                }
                self.wake();
            }
        }
    }

    /// Free Ride Lobby → Copy invite link (multiplayer.js `inviteUrl`), once
    /// the room's code is known.
    fn copy_lobby_invite(&mut self) {
        let Some(front) = self.front.clone() else {
            return;
        };
        let mut f = front.borrow_mut();
        let Some(code) = f.state.lobby.as_ref().map(|l| l.code.clone()) else {
            return;
        };
        if code.is_empty() {
            return;
        }
        let Some(relay) = self.defaults.as_ref().map(|d| d.relay.clone()) else {
            return;
        };
        let environment = f
            .state
            .tracks
            .get(f.state.track)
            .map_or("env01".to_string(), |t| t.id.clone());
        let url = multiplayer::invite_url(
            &relay,
            &environment,
            f.state.quad_id,
            f.state.rider_id,
            &code,
        );
        f.state.notice = Some(url.clone());
        #[cfg(target_arch = "wasm32")]
        if let Some(clipboard) = web_sys::window().map(|w| w.navigator().clipboard()) {
            let promise = clipboard.write_text(&url);
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let copied = wasm_bindgen_futures::JsFuture::from(promise).await.is_ok();
                let _ = proxy.send_event(Message::LobbyNotice(if copied {
                    "Invite link copied.".into()
                } else {
                    url
                }));
            });
        }
        drop(f);
        self.wake();
    }

    /// The ride's world setup and the room's view of it, from the saved
    /// settings (Display → Graphics, crash mode, Garage appearance).
    fn ride_setup(
        &self,
        front: &Rc<RefCell<front::Front>>,
        quad: u16,
        rider: u16,
        race: Option<atv_game::ride::race::Settings>,
    ) -> (World, multiplayer::RoomRide) {
        let (graphics, weather, sound, default_crash) = self
            .defaults
            .as_ref()
            .map_or((Graphics::default(), None, None, Default::default()), |d| {
                (d.graphics, d.weather, d.sound, d.crash)
            });
        // Display → Graphics as saved (Auto: graphics.js detectGraphicsTier).
        let graphics = if self.defaults.as_ref().is_some_and(|d| d.graphics_set) {
            graphics
        } else {
            let saved = front.borrow().settings.get("atv-graphics");
            graphics_preset(&resolve_graphics(saved.as_deref()))
        };
        // Audio → Sound as saved (audio.js loadAudioSettings: Enhanced unless
        // Original was chosen); music on or off keeps the launch choice.
        let sound = sound.map(|(_, music)| {
            let saved = front.borrow().settings.get("atv-audio-v1");
            (saved_sound_mode(saved.as_deref()), music)
        });
        // Crash mode follows the saved setting (Settings may have changed it).
        let crash = front
            .borrow()
            .settings
            .get("atv-crash-mode")
            .and_then(|v| atv_game::crash::CrashMode::parse(&v))
            .unwrap_or(default_crash);
        let world = World {
            race,
            weather,
            steps: None,
            graphics,
            crash,
            ride: true,
            script: None,
            sound,
            autodrive: None,
            selection: atv_game::ride::Selection { quad, rider },
            // The saved Garage appearance dresses the ride (game.js
            // rideAppearance = frontend.appearance).
            appearance: front
                .borrow()
                .settings
                .get(atv_ui::front::controller::SELECTION_KEY)
                .and_then(|text| serde_json::from_str(&text).ok())
                .map(|saved| atv_game::ride::gear::saved_appearance(&saved))
                .unwrap_or_default(),
        };
        let ride = multiplayer::RoomRide {
            graphics,
            weather,
            crash,
            sound,
            appearance: world.appearance,
        };
        (world, ride)
    }

    fn start_ride(
        &mut self,
        environment: String,
        quad: u16,
        rider: u16,
        race: Option<atv_game::ride::race::Settings>,
        room: Option<RoomStart>,
    ) {
        let (Some(front), Some(content)) = (self.front.clone(), self.content.clone()) else {
            return;
        };
        // Host/Join: the Free Ride Lobby page while the room connects
        // (multiplayer.js `host`); Free Ride goes straight to the loader.
        if let Some(room) = room.as_ref().filter(|r| !r.freeride)
            && self.pending_room.is_none()
        {
            let room = room.clone();
            self.join_room(environment, quad, rider, room);
            return;
        }
        let Some(gpu) = self.gpu.take() else {
            return;
        };
        #[cfg(target_arch = "wasm32")]
        self.text_inputs.sync(&[], &[], &self.proxy);
        let (world, ride) = self.ride_setup(&front, quad, rider, race);
        {
            let mut f = front.borrow_mut();
            f.begin_loading(&environment);
            paint_front(&gpu, &mut f);
        }
        // The original loader's action shots for this course.
        let key = format!("shots:{environment}");
        if self.requested.insert(key) {
            let (c, env, shot_env) = (content.clone(), environment.clone(), environment.clone());
            self.spawn(
                async move { atv_ui::front::assets::shots(&c, &env).await },
                move |result| {
                    Message::FrontArt(Box::new(ArtPiece::Shots(
                        shot_env,
                        result.unwrap_or_default(),
                    )))
                },
            );
        }
        let painter = front.clone();
        let progress: Progress = Box::new(move |gpu, text| {
            status(text);
            if let Ok(mut f) = painter.try_borrow_mut() {
                f.state.loading_stage(text);
                paint_front(gpu, &mut f);
            }
        });
        let held = self.pending_room.take();
        let fresh = std::mem::take(&mut self.pending_fresh);
        let future = async move {
            // A room ride claims its slot first; the room decides the course.
            // Returning from the garage/course picker resumes the held room.
            let (net, environment) = match (held, room) {
                (Some(mut net), _) => {
                    if !fresh {
                        net.loaded(&environment);
                    }
                    (Some(net), environment)
                }
                (None, room) => match room {
                    Some(room) => {
                        let profile = atv_game::multiplayer::Profile::new(
                            &room.name,
                            quad,
                            rider,
                            ride.appearance,
                        );
                        let (net, environment) = multiplayer::Net::join(
                            &room,
                            profile,
                            &environment,
                            content.clone(),
                            ride,
                        )
                        .await?;
                        (Some(net), environment)
                    }
                    None => (None, environment),
                },
            };
            let mut ready = prepare(gpu, content, environment, None, world, progress).await?;
            if net.is_some()
                && let Some(riding) = &mut ready.riding
            {
                riding.online = true;
            }
            ready.net = net;
            Ok::<_, String>(ready)
        };
        self.spawn(future, |result| match result {
            Ok(ready) => Message::Ready(Box::new(ready)),
            Err(error) => Message::Failed(error),
        });
    }

    /// A room ride whose window is hidden or suspended runs no frames.
    fn presence_due(&self) -> bool {
        self.held_room.is_some()
            || (!self.visible || self.suspended)
                && self.ready.as_ref().is_some_and(|r| {
                    r.riding.is_some() && r.net.as_ref().is_some_and(|n| n.closed.is_none())
                })
    }

    /// Hidden or suspended: keep the room live without frames. The socket is
    /// drained, pings continue and a paused presence pose goes out at its
    /// 0.2 Hz cadence (web/multiplayer.js timers keep running in a
    /// background tab; actual suspension sends paused presence).
    fn presence_tick(&mut self) {
        if let Some(net) = &mut self.held_room {
            // The frontend over a held room: socket, reconnects and pings,
            // and the paused ride's presence.
            net.tick(None, true);
            if let Some(ride) = &self.held_ride {
                net.hold(ride);
            }
            if let Some(closed) = &net.closed {
                status(&format!("Multiplayer ended: {closed}"));
                self.held_room = None;
                self.held_ride = None;
            }
            return;
        }
        if !self.presence_due() {
            return;
        }
        let Some(ready) = &mut self.ready else { return };
        let (Some(net), Some(riding)) = (&mut ready.net, &mut ready.riding) else {
            return;
        };
        let tick = net.tick(Some(&riding.ride), true);
        let remote = riding
            .sound
            .as_ref()
            .map(|s| (s.remote.voices(), s.remote.starts, s.remote.impact_starts));
        publish_multiplayer(net, riding.contacts, &riding.ride, remote, 0);
        let content = net.content();
        let course = tick.course;
        for key in tick.loads {
            self.spawn_frame(multiplayer::load_model(content.clone(), key), |model| {
                Message::PeerModel(Box::new(model))
            });
        }
        self.poll_frame_tasks();
        if let Some(environment) = course {
            self.reload_room(environment);
        }
    }

    /// The main menu's Free Ride count, at most every 15 s while it shows
    /// (watchFreerideRiders in frontend.js).
    fn poll_riders(&mut self) {
        let Some(front) = &self.front else {
            return;
        };
        if self.ready.is_some() || front.borrow().state.page != "main" {
            return;
        }
        if self
            .riders_polled
            .is_some_and(|t| t.elapsed().as_secs_f32() < 15.)
        {
            return;
        }
        let Some(relay) = self.defaults.as_ref().map(|d| d.relay.clone()) else {
            return;
        };
        self.riders_polled = Some(Instant::now());
        #[cfg(target_arch = "wasm32")]
        self.spawn(
            async move { net::riders(&relay).await },
            Message::FreeRideRiders,
        );
        // Desktop: a short-lived thread (the event-loop proxy is not Send),
        // collected by `about_to_wait`.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = send.send(net::riders_blocking(&relay));
            });
            self.riders_pending = Some(receive);
        }
    }

    /// Pause → Quit to Main Menu: the ride ends and the frontend returns.
    fn quit_to_menu(&mut self) -> bool {
        self.quit_to(Some("main"))
    }

    /// Ends the ride and hands the GPU back to the frontend, showing `page`
    /// (None: the caller loads another ride at once).
    fn quit_to(&mut self, page: Option<&str>) -> bool {
        let Some(front) = self.front.clone() else {
            return false;
        };
        let Some(ready) = self.ready.take() else {
            return false;
        };
        let Ready { gpu, .. } = ready;
        self.overlay = None;
        self.gpu = Some(gpu);
        self.remember_ride();
        if let Some(page) = page {
            front.borrow_mut().resume(page);
        }
        self.last = Instant::now();
        self.wake();
        true
    }

    /// One frontend frame: controller input, layout and present.
    fn front_frame(&mut self) {
        self.update_diagnostics();
        let (Some(front), Some(gpu)) = (self.front.clone(), self.gpu.as_ref()) else {
            return;
        };
        let started = Instant::now();
        let pad = self.gamepad.sample();
        let mut f = front.borrow_mut();
        f.pad_name = self.gamepad.name();
        let event = f.pad(pad.as_ref());
        paint_front(gpu, &mut f);
        #[cfg(target_arch = "wasm32")]
        {
            let (fields, rects) = (f.state.fields(), f.field_rects());
            let fields = if f.menu.is_none() { fields } else { Vec::new() };
            self.text_inputs.sync(&fields, &rects, &self.proxy);
        }
        if self.ui_diagnostics {
            // The output's peak since the last frame (audible menu tune).
            let peak = self
                .activation
                .as_ref()
                .and_then(|a| a.started_output())
                .and_then(|o| {
                    let mut mixer = o.mixer.lock().ok()?;
                    Some(std::mem::take(&mut mixer.meter.peak))
                });
            publish_ui_state(
                &serde_json::json!({
                    "front": f.diagnostics(),
                    "diagnostics": {
                        "mode": self.diagnostics.mode.name(),
                        "lines": self.diagnostics.lines,
                    },
                    "menuMusic": {
                        "loaded": self.menu_music.tune.is_some(),
                        "playing": self.menu_music.voice.is_some(),
                        "peak": peak,
                    },
                    // The page's soundtrack player (shared with rides).
                    "soundtrack": self.soundtrack.as_ref().map(|m| {
                        let m = m.borrow();
                        let mut report = m.report();
                        report["playing"] = m.player.selected.into();
                        report["paused"] = m.player.paused.into();
                        report
                    }),
                })
                .to_string(),
            );
        }
        let wants = f.wants();
        drop(f);
        self.sync_menu_music();
        // The filled lobby page has been drawn: load the room's course.
        if self.joined.as_ref().is_some_and(|j| Instant::now() >= j.4) {
            if let Some((net, environment, quad, rider, _)) = self.joined.take() {
                self.pending_room = Some(net);
                self.pending_fresh = true;
                self.start_ride(environment, quad, rider, None, None);
            }
            return;
        }
        if self.joined.is_some()
            && let Some(window) = &self.window
        {
            // Keep drawing until the hold ends (menus otherwise idle).
            window.request_redraw();
        }
        let f = front.borrow();
        // The garage shows the authored top speed of the current ATV.
        let wanted = (f.state.page == "garage"
            && f.state.quad_id < 27
            && !f.art.top_speed.contains_key(&f.state.quad_id))
        .then_some(f.state.quad_id);
        drop(f);
        if let (Some(quad), Some(content)) = (wanted, self.content.clone())
            && self.requested.insert(format!("speed:{quad}"))
        {
            self.spawn(
                async move { atv_ui::front::assets::top_speed(&content, quad).await },
                move |result| match result {
                    Ok(speed) => Message::FrontArt(Box::new(ArtPiece::TopSpeed(quad, speed))),
                    Err(error) => Message::SoundFailed(format!("ATV figures: {error}")),
                },
            );
        }
        // Preview models load once each (the vehicle is re-read when
        // revisited after two others, as selectionAssets evicts it).
        for want in wants {
            let Some(content) = self.content.clone() else {
                break;
            };
            match want {
                front::Want::Vehicle(quad) => {
                    if self.requested.insert(format!("vehicle:{quad}")) {
                        self.spawn(
                            async move { atv_ui::front::assets::vehicle_models(&content, quad).await },
                            move |result| match result {
                                Ok((model, rider)) => {
                                    Message::FrontArt(Box::new(ArtPiece::Vehicle(quad, model, rider)))
                                }
                                Err(error) => Message::SoundFailed(format!("Model preview: {error}")),
                            },
                        );
                    }
                }
                front::Want::MenuRider(rider) => {
                    if self.requested.insert(format!("menu-rider:{rider}")) {
                        self.spawn(
                            async move { atv_ui::front::assets::menu_rider(&content, rider == 1).await },
                            move |result| match result {
                                Ok(model) => Message::FrontArt(Box::new(ArtPiece::MenuRider(rider, model))),
                                Err(error) => Message::SoundFailed(format!("Rider preview: {error}")),
                            },
                        );
                    }
                }
            }
        }
        self.meter.cpu += started.elapsed().as_secs_f64() * 1000.;
        self.diagnostics.cpu = started.elapsed().as_secs_f64() * 1000.;
        if let Some(event) = event {
            self.front_event(event);
        }
    }

    /// Frontend pointer input; true when the frontend took it.
    fn front_pointer(
        &mut self,
        pointer: u64,
        phase: winit::event::TouchPhase,
        x: f32,
        y: f32,
    ) -> bool {
        use winit::event::TouchPhase;
        let Some(front) = self.front_input() else {
            return false;
        };
        let event = {
            let mut f = front.borrow_mut();
            match phase {
                TouchPhase::Started => {
                    f.press(pointer, x, y);
                    None
                }
                TouchPhase::Moved => {
                    f.moved(pointer, x, y, pointer == MOUSE);
                    None
                }
                TouchPhase::Ended => f.release(pointer, x, y),
                TouchPhase::Cancelled => {
                    f.cancel_pointer();
                    None
                }
            }
        };
        if let Some(event) = event {
            self.front_event(event);
        }
        true
    }
}

/// Lays out and presents one frontend frame.
fn paint_front(gpu: &Gpu, front: &mut front::Front) {
    front.prepare(&gpu.device, &gpu.queue, gpu.config.width, gpu.config.height);
    if let Err(error) = gpu.present_with(&mut |encoder, target| front.draw(encoder, target)) {
        status(&error);
    }
}

/// Native parity capture of one frontend page with all its artwork.
#[cfg(not(target_arch = "wasm32"))]
fn capture_front(booted: Booted, page: &str, path: &std::path::Path, width: u32, height: u32) {
    let Booted { gpu, content, boot } = booted;
    let view = atv_ui::hud::Viewport {
        width: width as f32,
        height: height as f32,
        scale: 1.,
        hover: true,
        ..Default::default()
    };
    let tracks = boot.tracks.clone();
    let mut front = front::Front::with_settings(
        &gpu.device,
        gpu.config.format,
        view,
        boot.tracks,
        boot.backdrop,
        atv_ui::settings::Settings::memory(),
    );
    pollster::block_on(async {
        front.art.photos.extend(
            atv_ui::front::assets::photos(&content, &tracks)
                .await
                .unwrap_or_default(),
        );
        if let Ok(quads) = atv_ui::front::assets::quad_catalog(&content).await {
            front.art.quads = Some(quads);
        }
        if let Ok(speed) = atv_ui::front::assets::top_speed(&content, 1).await {
            front.art.top_speed.insert(1, speed);
        }
        match atv_ui::front::assets::vehicle_models(&content, 1).await {
            Ok((model, rider)) => front.add_vehicle(1, model, rider),
            Err(error) => status(&format!("Model preview: {error}")),
        }
        match atv_ui::front::assets::menu_rider(&content, false).await {
            Ok(rider) => {
                front.art.menu_riders.insert(0, rider);
            }
            Err(error) => status(&format!("Rider preview: {error}")),
        }
    });
    if page == "loading" {
        let environment = tracks.first().map_or("env01".to_string(), |t| t.id.clone());
        if let Ok(shots) = pollster::block_on(atv_ui::front::assets::shots(&content, &environment))
        {
            front.art.shots = Some((environment.clone(), shots));
        }
        if let Ok(tips) = pollster::block_on(atv_ui::front::assets::tips(&content)) {
            front.art.tips = tips.into_iter().take(1).collect();
        }
        front.begin_loading(&environment);
        front.state.loading_stage("Reading original ATV…");
    } else {
        front.state.show(page, &view);
    }
    // Two passes: the first lays out lists and scrolls the current card in.
    for _ in 0..2 {
        front.prepare(&gpu.device, &gpu.queue, width, height);
    }
    match gpu.capture_ui(width, height, &mut |encoder, target| {
        front.draw(encoder, target)
    }) {
        Ok(rgba) => image_png(path, width, height, &rgba),
        Err(error) => status(&error),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn capture(ready: &mut Ready, path: &std::path::Path, width: u32, height: u32, ui: bool) {
    let view = View {
        camera: ready.camera,
        focal_length: ready.focal_length,
    };
    let mut overlay = ready.riding.as_ref().filter(|_| ui).map(|riding| {
        let gpu = &ready.gpu;
        let mut overlay = overlay::Overlay::with_settings(
            &gpu.device,
            gpu.config.format,
            overlay::viewport(width, height, 1.),
            atv_ui::settings::Settings::memory(),
        );
        overlay.set_course(&ready.venue);
        overlay.finish_course(&ready.venue.terrain);
        if let Some(assets) = ready.hud_assets.take() {
            overlay.set_meter(&gpu.device, &gpu.queue, assets);
        }
        let ride = &riding.ride;
        for _ in 0..2 {
            overlay.prepare(
                &gpu.device,
                &gpu.queue,
                width,
                height,
                0.,
                true,
                true,
                false,
                Some(overlay::HudInput {
                    hud: ride.hud(),
                    rider_world: ride.rider_world(),
                    terrain: &ready.venue.terrain,
                    race: ride
                        .race()
                        .and_then(|r| r.frame.as_ref())
                        .map(|f| (f, ride.committed_points(), ride.frame().origin)),
                    peers: None,
                }),
                None,
            );
        }
        overlay
    });
    let rgba = ready
        .scene
        .capture_with(&ready.gpu, &view, width, height, &mut |encoder, target| {
            if let Some(overlay) = &mut overlay {
                overlay.draw(encoder, target);
            }
        })
        .expect("capture frame");
    image_png(path, width, height, &rgba);
    status(&format!("wrote {}", path.display()));
}

#[cfg(not(target_arch = "wasm32"))]
fn image_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("create capture file"));
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .expect("write capture png");
}

/// `x,y,z,yaw,pitch,focal` as six finite numbers.
pub fn parse_camera(text: &str) -> Option<[f32; 6]> {
    let values: Vec<f32> = text
        .split(',')
        .map(|v| v.trim().parse().ok().filter(|v: &f32| v.is_finite()))
        .collect::<Option<_>>()?;
    values.try_into().ok()
}

/// `on`/`off` (also 1/0, true/false).
/// The saved `atv-audio-v1` sound mode (audio.js loadAudioSettings): Original
/// only when chosen, else Enhanced.
fn saved_sound_mode(saved: Option<&str>) -> atv_game::audio::ride::SoundMode {
    let original = saved
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .is_some_and(|v| v["mode"] == "original");
    if original {
        atv_game::audio::ride::SoundMode::Original
    } else {
        atv_game::audio::ride::SoundMode::Enhanced
    }
}

pub fn parse_switch(text: &str) -> Option<bool> {
    match text {
        "on" | "1" | "true" => Some(true),
        "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

/// `(prefers-reduced-motion: reduce)` (always false on desktop).
fn reduced_motion() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| {
                w.match_media("(prefers-reduced-motion: reduce)")
                    .ok()
                    .flatten()
            })
            .is_some_and(|m| m.matches())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// A saved `atv-graphics` value resolved to a preset; Auto picks the
/// detected tier.
pub fn resolve_graphics(saved: Option<&str>) -> String {
    match saved {
        Some(p @ ("original" | "enhanced" | "high")) => p.into(),
        _ => detect_graphics_tier().into(),
    }
}

/// graphics.js `detectGraphicsTier`: phones, tablets, touch-primary and
/// low-power devices get Enhanced, others High (desktop builds: High).
pub fn detect_graphics_tier() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return "high";
        };
        let nav = window.navigator();
        let ua = nav.user_agent().unwrap_or_default();
        let media = |q: &str| {
            window
                .match_media(q)
                .ok()
                .flatten()
                .is_some_and(|m| m.matches())
        };
        let mobile_ua = [
            "Android", "iPhone", "iPad", "iPod", "Mobile", "Silk", "Kindle",
        ]
        .iter()
        .any(|m| ua.to_ascii_lowercase().contains(&m.to_ascii_lowercase()));
        let touch_mac = ua.contains("Macintosh") && nav.max_touch_points() > 1;
        let touch_primary = media("(pointer: coarse)") && !media("(any-pointer: fine)");
        let cores = nav.hardware_concurrency();
        let memory = js_sys::Reflect::get(&nav, &"deviceMemory".into())
            .ok()
            .and_then(|v| v.as_f64());
        let low_power = (cores > 0. && cores <= 4.) || memory.is_some_and(|m| m > 0. && m <= 4.);
        if mobile_ua || touch_mac || touch_primary || low_power {
            "enhanced"
        } else {
            "high"
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "high"
    }
}

/// Display -> Graphics preset features (presetFeatures in web/graphics.js).
pub fn graphics_preset(name: &str) -> Graphics {
    match name {
        "high" => Graphics {
            interpolation: true,
            sky_smooth: true,
            anisotropy: 16,
            fresnel: true,
            dust: 96,
            contact_shadows: true,
            shadow_map: 2048,
            tire_tracks: (3600, 75),
            wheel_reprojection: true,
            dirt: true,
        },
        "enhanced" => Graphics {
            interpolation: true,
            sky_smooth: true,
            anisotropy: 4,
            fresnel: true,
            dust: 24,
            contact_shadows: true,
            shadow_map: 0,
            tire_tracks: (600, 15),
            wheel_reprojection: true,
            dirt: true,
        },
        _ => Graphics::default(),
    }
}

fn trace_all() -> bool {
    url_flag("traceAll=1")
}

/// A `name=value` pair in the page URL (always false on desktop).
/// A saved browser setting (localStorage); None natively, where the
/// frontend's settings file applies through `apply_preferences`.
fn saved_setting(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()?
            .local_storage()
            .ok()??
            .get_item(key)
            .ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
        None
    }
}

fn url_flag(pair: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|q| q.trim_start_matches('?').split('&').any(|p| p == pair))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = pair;
        false
    }
}

/// The replayed state, for the browser parity tool (`#trace`).
fn publish_trace(text: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(document) = web_sys::window().and_then(|w| w.document())
        && let Some(body) = document.body()
        && let Ok(element) = document.create_element("pre")
    {
        element.set_id("trace");
        element.set_attribute("hidden", "").ok();
        element.set_text_content(Some(text));
        body.append_child(&element).ok();
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = text;
}

/// The relay for menu rides: this page's origin on the web, the production
/// site on desktop.
fn default_relay() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "https://furyremastered.com".into()
    }
}

/// `?trainPlace=`: the centre of Arlington's car 3, where a parked quad
/// stays on the deck (test_engine_multiplayer_train.mjs puts its JavaScript
/// guest on car 6, so the two never collide).
const TRAIN_PLACE_CAR: usize = 3;

/// Multiplayer status for tests (`window.furyMultiplayer`, JSON).
fn publish_multiplayer(
    net: &multiplayer::Net,
    contacts: u64,
    ride: &atv_game::ride::Ride,
    remote_sound: Option<(usize, u64, u64)>,
    tags: usize,
) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let mut report = net.report();
        report["contacts"] = contacts.into();
        report["origin"] = serde_json::json!(ride.frame().origin);
        report["trainTime"] = serde_json::json!(ride.train_time());
        report["ownPlatform"] = serde_json::json!(
            ride.train_platform()
                .map(|(car, matrix)| serde_json::json!({"car": car, "matrix": matrix}))
        );
        // This client's matrices of the cars other riders stand on.
        let cars: serde_json::Map<String, serde_json::Value> = report["remote"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["platform"].as_u64())
            .filter_map(|car| {
                let m = ride.train_car(car as usize)?;
                Some((car.to_string(), serde_json::json!(m)))
            })
            .collect();
        report["trainCars"] = cars.into();
        report["tags"] = tags.into();
        report["remoteSound"] = serde_json::json!(remote_sound.map(|(voices, starts, impacts)| {
            serde_json::json!({"voices": voices, "starts": starts, "impacts": impacts})
        }));
        let _ = js_sys::Reflect::set(
            &window,
            &"furyMultiplayer".into(),
            &report.to_string().into(),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (net, contacts, ride, remote_sound, tags);
}

impl App {
    /// The room moved to another course: reload the ride there, keeping the
    /// room (web/multiplayer.js `follow` -> `ride`).
    fn reload_room(&mut self, environment: String) {
        let Some(mut ready) = self.ready.take() else {
            return;
        };
        let Some(mut net) = ready.net.take() else {
            return;
        };
        self.overlay = None;
        status(&format!(
            "The room moved to {}…",
            environment.to_uppercase()
        ));
        let Ready { gpu, .. } = ready;
        let content = net.content();
        let profile = net.session.profile().cloned();
        let world = World {
            selection: profile.map_or_else(Default::default, |p| atv_game::ride::Selection {
                quad: p.quad,
                rider: p.rider,
            }),
            appearance: net.ride.appearance,
            race: None,
            weather: net.ride.weather,
            steps: None,
            graphics: net.ride.graphics,
            crash: net.ride.crash,
            ride: true,
            script: None,
            sound: net.ride.sound,
            autodrive: None,
        };
        let future = async move {
            let mut ready = prepare(
                gpu,
                content,
                environment.clone(),
                None,
                world,
                Box::new(|_, text| status(text)),
            )
            .await?;
            net.loaded(&environment);
            if let Some(riding) = &mut ready.riding {
                riding.online = true;
            }
            ready.net = Some(net);
            Ok::<_, String>(ready)
        };
        self.spawn(future, |result| match result {
            Ok(ready) => Message::Ready(Box::new(ready)),
            Err(error) => Message::Failed(error),
        });
    }
}

/// Shows the status line even when the debug strip is hidden (engine.css
/// `#status.fatal`): the engine could not start or load anything to draw.
fn fatal() {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("status"))
    {
        element.set_class_name("fatal");
    }
}

thread_local! {
    /// The latest status line, shown in the diagnostics overlay.
    static LAST_STATUS: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The status line: web `#status` (hidden, kept current for tests and shown
/// only for a fatal error; the diagnostics overlay carries it), desktop
/// standard error.
pub(crate) fn status(text: &str) {
    LAST_STATUS.with_borrow_mut(|s| text.clone_into(s));
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::console::log_1(&text.into());
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("status"))
        {
            element.set_text_content(Some(text));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{text}");
}

pub fn run(options: Options) {
    let activation = options
        .sound
        .filter(|_| options.ride && options.script.is_none())
        .map(|(_, music)| sound::Activation::new(music));
    #[cfg(not(target_arch = "wasm32"))]
    let bench_out = options.bench.clone();
    let profile = options.profile;
    let event_loop = EventLoop::<Message>::with_user_event()
        .build()
        .expect("event loop");
    let app = App {
        camera_choice: options.camera_choice,
        wake_lock: Default::default(),
        graphics_pref: None,
        proxy: event_loop.create_proxy(),
        display: event_loop.owned_display_handle(),
        options: Some(options),
        window: None,
        ready: None,
        keys: HashSet::new(),
        dragging: false,
        intro_pad_held: true,
        cursor: None,
        last: Instant::now(),
        meter: Meter::default(),
        gamepad: gamepad::Gamepads::new(),
        drag: 0.,
        started: Instant::now(),
        overlay: None,
        focused: true,
        visible: true,
        suspended: false,
        asleep: false,
        pad_buttons: 0,
        mouse_ui: false,
        ui_backdrop: None,
        ui_diagnostics: ui_diagnostics_enabled(),
        diagnostics: Diagnostics::new(),
        hide_ui: url_flag("ui=0"),
        sound_unlocked: false,
        activation,
        bench: Bench {
            profile,
            ..Default::default()
        },
        #[cfg(not(target_arch = "wasm32"))]
        bench_out,
        front: None,
        menu_music: MenuMusic::default(),
        soundtrack: None,
        soundtrack_requested: false,
        soundtrack_prefs: None,
        gpu: None,
        content: None,
        defaults: None,
        requested: HashSet::new(),
        riders_polled: None,
        held_room: None,
        held_ride: None,
        pending_room: None,
        pending_fresh: false,
        joining: None,
        joined: None,
        frame_tasks: Vec::new(),
        #[cfg(target_arch = "wasm32")]
        text_inputs: Default::default(),
        #[cfg(not(target_arch = "wasm32"))]
        riders_pending: None,
    };
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::EventLoopExtWebSys;
        // Background tabs throttle timers to about once a second but keep
        // them running; the room's presence rides on this one.
        let proxy = app.proxy.clone();
        let tick = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
            let _ = proxy.send_event(Message::PresenceTick);
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
                tick.as_ref().unchecked_ref(),
                1000,
            );
        }
        tick.forget();
        event_loop.spawn_app(app);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        event_loop.run_app(&mut app).expect("run event loop");
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    let param = |name: &str| {
        search
            .trim_start_matches('?')
            .split('&')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
            .map(String::from)
    };
    // `?venue=` rides at once (engine tests and parity tools); otherwise the
    // frontend opens and reads the page's ride link.
    // A room link (`?room=` / `?host=1`) rides at once too.
    // Measurement hook: `?prefetch=off` loads rides one request at a time.
    if param("prefetch").as_deref() == Some("off") {
        PREFETCH.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(ms) = param("pingMs").and_then(|v| v.parse::<u32>().ok()) {
        multiplayer::PING_MS.store(ms.clamp(250, 15_000), std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(ms) = param("joinDelayMs").and_then(|v| v.parse::<u32>().ok()) {
        multiplayer::JOIN_DELAY_MS.store(ms.min(10_000), std::sync::atomic::Ordering::Relaxed);
    }
    // A bare `?room=` / `?host=1` rides at once (tests); a shared invite
    // (`mode`, `action` or `lobby`) opens through the frontend's lobby forms.
    let invite = ["mode", "action", "lobby"]
        .iter()
        .any(|k| param(k).is_some());
    let direct_room =
        !invite && (param("room").is_some() || param("host").is_some_and(|v| v == "1"));
    let direct = param("venue").or_else(|| direct_room.then(|| "env01".to_string()));
    let venue = direct
        .clone()
        .filter(|id| atv_game::launch::environments().contains(id))
        .unwrap_or_else(|| "env01".into());
    run(Options {
        front: direct.is_none().then(|| FrontStart {
            search: search.clone(),
            page: param("front"),
        }),
        store: Store::Http("/content/".into()),
        venue,
        camera: param("camera").and_then(|text| parse_camera(&text)),
        weather: param("weather").and_then(|v| parse_switch(&v)),
        steps: param("steps").and_then(|v| v.parse().ok()),
        graphics: param("graphics").map_or_else(Graphics::default, |v| graphics_preset(&v)),
        graphics_set: param("graphics").is_some(),
        // ?crash= for captures, else the JavaScript build's saved setting.
        crash: param("crash")
            .or_else(|| {
                web_sys::window()?
                    .local_storage()
                    .ok()??
                    .get_item("atv-crash-mode")
                    .ok()?
            })
            .and_then(|v| atv_game::crash::CrashMode::parse(&v))
            .unwrap_or_default(),
        camera_choice: {
            let choice = |key: &str, storage: &str| {
                param(key).or_else(|| {
                    web_sys::window()?
                        .local_storage()
                        .ok()??
                        .get_item(storage)
                        .ok()?
                })
            };
            (
                choice("crashcam", "atv-crash-camera").as_deref() != Some("original"),
                choice("chase", "atv-chase-camera").as_deref() == Some("assisted"),
            )
        },
        ride: param("free").is_none(),
        // `gear=1.0.2.0.0.0.0&paint=3` (captures and tests).
        appearance: atv_game::ride::gear::saved_appearance(&serde_json::json!({"appearance": {
            "gear": param("gear").map(|v| v.split('.').filter_map(|g| g.parse::<u64>().ok()).collect::<Vec<_>>()),
            "paint": param("paint").and_then(|v| v.parse::<u64>().ok()),
        }})),
        selection: {
            let number = |key: &str| param(key).and_then(|v| v.parse::<u16>().ok());
            let defaults = atv_game::ride::Selection::default();
            atv_game::ride::Selection {
                quad: number("quad").unwrap_or(defaults.quad),
                rider: number("rider").unwrap_or(defaults.rider),
            }
        },
        ui_backdrop: param("uibackdrop").and_then(|hex| {
            let v = u32::from_str_radix(&hex, 16).ok()?;
            (hex.len() == 6).then_some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
        }),
        // ?sound= wins; scripted parity and benchmark rides keep the Original
        // baseline; a played ride uses the saved Audio → Sound (Enhanced default).
        sound: Some((
            match param("sound").as_deref() {
                Some("enhanced") => atv_game::audio::ride::SoundMode::Enhanced,
                Some(_) => atv_game::audio::ride::SoundMode::Original,
                None if ["script", "autodrive", "traceAll"]
                    .iter()
                    .any(|k| param(k).is_some()) =>
                {
                    atv_game::audio::ride::SoundMode::Original
                }
                None => saved_sound_mode(
                    atv_ui::settings::Settings::load()
                        .get("atv-audio-v1")
                        .as_deref(),
                ),
            },
            param("music").as_deref() != Some("off"),
        )),
        profile: param("profile").is_some_and(|v| v == "1"),
        autodrive: param("autodrive")
            .and_then(|text| js_sys_decode(&text))
            .and_then(|text| replay::Script::parse(&text).ok()),
        race: param("race").is_some_and(|v| v == "1").then(|| {
            let number = |key: &str| param(key).and_then(|v| v.parse::<u64>().ok());
            let defaults = atv_game::ride::race::Settings::default();
            atv_game::ride::race::Settings {
                laps: number("laps").map_or(defaults.laps, |v| v as u32),
                cpu: number("cpu").map_or(defaults.cpu, |v| v as usize),
                difficulty: number("difficulty").map_or(defaults.difficulty, |v| v as u32),
                seed: number("seed").unwrap_or(defaults.seed),
                ..defaults
            }
        }),
        relay: None,
        // `?host=1` hosts a room (`room=` an optional custom code) and
        // `?room=CODE` joins one, on this page's relay.
        room: direct_room.then(|| RoomStart {
            relay: web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default(),
            code: param("room"),
            host: param("host").is_some_and(|v| v == "1"),
            name: param("name")
                .and_then(|v| js_sys_decode(&v))
                .unwrap_or_else(|| "Rider".into()),
            drop_after: param("dropAfter").and_then(|v| v.parse().ok()),
            train_place: param("trainPlace").and_then(|v| v.parse().ok()),
            freeride: false,
        }),
        // `script=storage` reads a long parity script from localStorage
        // `furyScript` (URLs are bounded).
        script: param("script")
            .and_then(|text| {
                if text == "storage" {
                    web_sys::window()?
                        .local_storage()
                        .ok()??
                        .get_item("furyScript")
                        .ok()?
                } else {
                    js_sys_decode(&text)
                }
            })
            .and_then(|text| replay::Script::parse(&text).ok()),
    });
}

#[cfg(target_arch = "wasm32")]
fn js_sys_decode(text: &str) -> Option<String> {
    js_sys::decode_uri_component(text)
        .ok()
        .and_then(|s| s.as_string())
}

// Opt-in browser integration diagnostics. Normal play never serializes a frame.
fn ui_diagnostics_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    return web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|s| {
            s.trim_start_matches('?')
                .split('&')
                .any(|p| p == "uiDiagnostics=1")
        });
    #[cfg(not(target_arch = "wasm32"))]
    false
}

fn publish_ui_state(text: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        let element = document.get_element_by_id("ui-state").or_else(|| {
            let element = document.create_element("pre").ok()?;
            element.set_id("ui-state");
            element.set_attribute("hidden", "").ok()?;
            document.body()?.append_child(&element).ok()?;
            Some(element)
        });
        if let Some(element) = element {
            element.set_text_content(Some(text));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = text;
}

/// A per-load roster draw seed (the browser's Math.random() * 2^31).
fn fresh_seed() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(1, |d| u64::from(d.subsec_nanos()) ^ d.as_secs())
        & 0x7fff_ffff
}

/// A same-origin image decoded by the browser into RGBA (None when it is
/// missing, e.g. a package without the dev log images).
#[cfg(target_arch = "wasm32")]
async fn web_image(url: &str) -> Option<atv_ui::front::pixels::Rgba> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()?.document()?;
    let image = web_sys::HtmlImageElement::new().ok()?;
    image.set_src(url);
    wasm_bindgen_futures::JsFuture::from(image.decode())
        .await
        .ok()?;
    let (width, height) = (image.natural_width(), image.natural_height());
    if width == 0 || height == 0 {
        return None;
    }
    let canvas = document
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .ok()?;
    context
        .draw_image_with_html_image_element(&image, 0., 0.)
        .ok()?;
    let data = context
        .get_image_data(0., 0., f64::from(width), f64::from(height))
        .ok()?;
    Some(atv_ui::front::pixels::Rgba::new(
        width,
        height,
        data.data().0,
    ))
}

/// The diagnostics overlay's state (web/diagnostics.js DiagnosticsOverlay).
struct Diagnostics {
    mode: atv_ui::diagnostics::Mode,
    stats: atv_ui::diagnostics::FrameStats,
    /// When the lines were last rebuilt (ms since start).
    updated: Option<f64>,
    lines: Vec<String>,
    /// The last frame's CPU time (ms).
    cpu: f64,
    /// The menu row's value last seen (on a ride menu or the frontend's).
    pref: Option<(bool, String)>,
}

impl Diagnostics {
    /// Off unless `?debug=1` (Performance) / `?debug=2` (Debug) or the
    /// saved choice says otherwise.
    fn new() -> Self {
        use atv_ui::diagnostics::{KEY, Mode};
        let saved = || {
            atv_ui::settings::Settings::load()
                .get(KEY)
                .and_then(|v| Mode::parse(&v))
        };
        let mode = debug_param()
            .map(|v| match v.as_str() {
                "0" => Mode::Off,
                "2" => Mode::Debug,
                _ => Mode::Performance,
            })
            .or_else(saved)
            .unwrap_or_default();
        Self {
            mode,
            stats: Default::default(),
            updated: None,
            lines: Vec::new(),
            cpu: 0.,
            pref: None,
        }
    }
}

/// `?debug=` on the web; `ATV_DEBUG` natively.
fn debug_param() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        search.trim_start_matches('?').split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key == "debug").then(|| value.to_string())
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    std::env::var("ATV_DEBUG").ok()
}

/// The original frontend tune and its mixer voice.
#[derive(Default)]
struct MenuMusic {
    tune: Option<std::sync::Arc<atv_game::audio::mixer::Pcm>>,
    requested: bool,
    voice: Option<atv_game::audio::mixer::VoiceId>,
}
