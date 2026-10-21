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
    /// The browser's 1 s presence timer (a hidden tab runs no frames).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    PresenceTick,
    /// Another rider's engine bank finished loading.
    RemoteEngine(u16, Box<Result<atv_game::audio::remote::Engine, String>>),
}

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
    sound_unlocked: bool,
    activation: Option<sound::Activation>,
    bench: Bench,
    #[cfg(not(target_arch = "wasm32"))]
    bench_out: Option<std::path::PathBuf>,
    /// The frontend (title and menus), its GPU while no ride runs, and the
    /// content they share with rides.
    front: Option<Rc<RefCell<front::Front>>>,
    gpu: Option<Gpu>,
    content: Option<Rc<Content>>,
    defaults: Option<Defaults>,
    /// Frontend loads already started (top speeds, loader shots).
    requested: HashSet<String>,
    /// When the Free Ride rider count was last requested.
    riders_polled: Option<Instant>,
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
        let tick = net.tick(&riding.ride, false);
        for key in tick.loads {
            net.model_loaded(pollster::block_on(multiplayer::load_model(
                net.content(),
                key,
            )));
        }
        if let Some((origin, yaw)) = tick.place {
            let _ = riding.place_beside(origin, yaw);
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
                acquire = stats.acquire_ms;
            }
            Err(error) => status(&error),
        }
        self.meter.cpu += started.elapsed().as_secs_f64() * 1000. - acquire;
        self.meter_frame(now, "");
    }

    /// One rendered frame of the ride: input, fixed steps, chase view.
    fn ride_frame(&mut self, dt: f32) {
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
        let mut loads = Vec::new();
        let mut engine_loads = Vec::new();
        let mut course = None;
        if let Some(net) = &mut ready.net {
            let was_open = net.closed.is_none();
            // Only actual suspension (hidden page, suspended app) sends the
            // paused presence pose; the online pause menu and blur keep
            // neutral-input riding (Riding::online).
            let tick = net.tick(&riding.ride, suspended);
            if let Some((origin, yaw)) = tick.place
                && let Err(error) = riding.place_beside(origin, yaw)
            {
                status(&format!("Unable to join beside that rider: {error}"));
            }
            if tick.train_place
                && let Err(error) = riding.place_on_train("flatbeda")
            {
                status(&format!("Train placement: {error}"));
            }
            riding.peers = net.contact_bodies(&riding.ride);
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
            self.spawn(future, move |engine| {
                Message::RemoteEngine(quad, Box::new(engine))
            });
        }
        for (content, key) in loads {
            self.spawn(multiplayer::load_model(content, key), |model| {
                Message::PeerModel(Box::new(model))
            });
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
            state["active"] = active.into();
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
                "interpolation": riding.interpolation,
                "skySmooth": graphics.sky_smooth, "fresnel": graphics.fresnel,
                "contactShadows": graphics.contact_shadows, "anisotropy": graphics.anisotropy,
                "dust": graphics.dust, "tireTracks": graphics.tire_tracks.0,
                "sfx": mixer.map(|m| m.0), "music": mixer.map(|m| m.1),
            });
            publish_ui_state(&state.to_string());
        }
        let overlay = &self.overlay;
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
                acquire = stats.acquire_ms;
            }
            Err(error) => status(&error),
        }
        let cpu = started.elapsed().as_secs_f64() * 1000. - acquire;
        self.meter.cpu += cpu;
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
        let since = *self.meter.since.get_or_insert(now);
        let elapsed = (now - since).as_secs_f64();
        if elapsed >= 1. {
            let info = &ready.gpu.adapter_info;
            status(&format!(
                "{} · {:.0} fps · {:.1} ms frame · {:.2} ms CPU ({:.2} UI) · {} draws · {} tris · {:?} {}{extra}",
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
            self.meter = Meter {
                triangles: self.meter.triangles,
                draws: self.meter.draws,
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
        let (Some(ready), Some(overlay)) = (&mut self.ready, &mut self.overlay) else {
            return;
        };
        let Some(sound) = ready.riding.as_mut().and_then(|r| r.sound.as_mut()) else {
            return;
        };
        if !std::mem::take(&mut sound.player.dirty) {
            return;
        }
        let mut record = overlay
            .settings
            .get("atv-soundtrack-v1")
            .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({"v": 3, "enabled": true, "volume": 0.45}));
        record["v"] = 3.into();
        record["history"] = sound.player.history.clone().into();
        record["historyIndex"] = sound.player.history_index.into();
        overlay
            .settings
            .set("atv-soundtrack-v1", &record.to_string());
    }

    /// The open menu's music view, and the Jukebox art on its first visit.
    fn update_jukebox(&mut self) {
        let (Some(ready), Some(overlay)) = (&self.ready, &mut self.overlay) else {
            return;
        };
        if overlay.menu.is_none() {
            return;
        }
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

    /// Pause → Copy ride link (frontend.js `rideUrl` + launch.js
    /// `copyRideUrl`): this page with the ride's setup; the clipboard, or
    /// the link itself on the notice line when the copy is refused.
    fn copy_ride_link(&mut self) {
        let (Some(ready), Some(overlay)) = (&self.ready, &mut self.overlay) else {
            return;
        };
        let Some(riding) = &ready.riding else {
            return;
        };
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
        let url = atv_game::launch::ride_url(
            &page,
            &atv_game::launch::RideLink {
                map: &ready.venue.environment,
                quad: selection.quad,
                rider: selection.rider,
                player: &player,
                race,
            },
        );
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
                            "Ride link copied.".into()
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
        riding.ride.options = atv_game::ride::RideOptions {
            flips: prefs.flips,
            realistic_train: prefs.realistic_train,
        };
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
            if let Some(effects) = &mut riding.effects
                && !riding.ride.is_motorcycle()
            {
                effects.set_lingering_dust(usize::from(graphics.dust), &ready.venue);
                effects.set_tire_tracks(
                    usize::from(graphics.tire_tracks.0),
                    f32::from(graphics.tire_tracks.1),
                    &ready.venue,
                );
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
        } else if let Some(activation) = &self.activation
            && let Err(error) = activation.activate()
        {
            status(&format!("Sound unavailable: {error}"));
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
        let future = async move {
            let result = sound::Sound::load(
                pending.content,
                &pending.environment,
                pending.settings,
                activation,
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
                status(&format!(
                    "Multiplayer room {} ({} riding). Invite: {}",
                    net.code(),
                    net.session.lobby.as_ref().map_or(1, |l| l.players.len()),
                    net.invite()
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
                    // The saved soundtrack history (music.js v3 record).
                    if let Some(record) = self
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
                        sound.restore_history(history, index);
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
            Message::JukeboxArt(art) => {
                if let (Some(overlay), Some(ready)) = (&mut self.overlay, &self.ready) {
                    overlay.set_jukebox_art(&ready.gpu.device, &ready.gpu.queue, *art);
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
                if let PhysicalKey::Code(code) = event.physical_key {
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
            // The Rust engine has no race setup page yet: Single Player.
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
        if setup.mode == "multiplayer" {
            errors.push("Multiplayer is not available in the Rust engine yet.".into());
        }
        if setup.activity == "race" && setup.quad_id >= 27 {
            errors.push("Races on a motorcycle are not ported to the Rust engine yet.".into());
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
        }
    }

    /// Shows the course loader and loads the ride on the frontend's GPU.
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
        let Some(gpu) = self.gpu.take() else {
            return;
        };
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
        // Crash mode follows the saved setting (Settings may have changed it).
        let crash = front
            .borrow()
            .settings
            .get("atv-crash-mode")
            .and_then(|v| atv_game::crash::CrashMode::parse(&v))
            .unwrap_or(default_crash);
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
        let future = async move {
            // A room ride claims its slot first; the room decides the course.
            let (net, environment) = match room {
                Some(room) => {
                    let profile = atv_game::multiplayer::Profile::new(
                        &room.name,
                        quad,
                        rider,
                        ride.appearance,
                    );
                    let (net, environment) =
                        multiplayer::Net::join(&room, profile, &environment, content.clone(), ride)
                            .await?;
                    (Some(net), environment)
                }
                None => (None, environment),
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
        (!self.visible || self.suspended)
            && self.ready.as_ref().is_some_and(|r| {
                r.riding.is_some() && r.net.as_ref().is_some_and(|n| n.closed.is_none())
            })
    }

    /// Hidden or suspended: keep the room live without frames. The socket is
    /// drained, pings continue and a paused presence pose goes out at its
    /// 0.2 Hz cadence (web/multiplayer.js timers keep running in a
    /// background tab; actual suspension sends paused presence).
    fn presence_tick(&mut self) {
        if !self.presence_due() {
            return;
        }
        let Some(ready) = &mut self.ready else { return };
        let (Some(net), Some(riding)) = (&mut ready.net, &mut ready.riding) else {
            return;
        };
        let tick = net.tick(&riding.ride, true);
        let remote = riding
            .sound
            .as_ref()
            .map(|s| (s.remote.voices(), s.remote.starts, s.remote.impact_starts));
        publish_multiplayer(net, riding.contacts, &riding.ride, remote, 0);
        let content = net.content();
        let course = tick.course;
        for key in tick.loads {
            self.spawn(multiplayer::load_model(content.clone(), key), |model| {
                Message::PeerModel(Box::new(model))
            });
        }
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
        if let Some(page) = page {
            front.borrow_mut().resume(page);
        }
        self.last = Instant::now();
        self.wake();
        true
    }

    /// One frontend frame: controller input, layout and present.
    fn front_frame(&mut self) {
        let (Some(front), Some(gpu)) = (self.front.clone(), self.gpu.as_ref()) else {
            return;
        };
        let started = Instant::now();
        let pad = self.gamepad.sample();
        let mut f = front.borrow_mut();
        f.pad_name = self.gamepad.name();
        let event = f.pad(pad.as_ref());
        paint_front(gpu, &mut f);
        if self.ui_diagnostics {
            publish_ui_state(&serde_json::json!({ "front": f.diagnostics() }).to_string());
        }
        let wants = f.wants();
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
        },
        _ => Graphics::default(),
    }
}

fn trace_all() -> bool {
    url_flag("traceAll=1")
}

/// A `name=value` pair in the page URL (always false on desktop).
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

pub(crate) fn status(text: &str) {
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
        wake_lock: Default::default(),
        graphics_pref: None,
        proxy: event_loop.create_proxy(),
        display: event_loop.owned_display_handle(),
        options: Some(options),
        window: None,
        ready: None,
        keys: HashSet::new(),
        dragging: false,
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
        sound_unlocked: false,
        activation,
        bench: Bench {
            profile,
            ..Default::default()
        },
        #[cfg(not(target_arch = "wasm32"))]
        bench_out,
        front: None,
        gpu: None,
        content: None,
        defaults: None,
        requested: HashSet::new(),
        riders_polled: None,
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
    let direct = param("venue").or_else(|| {
        (param("room").is_some() || param("host").is_some_and(|v| v == "1"))
            .then(|| "env01".to_string())
    });
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
        sound: Some((
            if param("sound").as_deref() == Some("enhanced") {
                atv_game::audio::ride::SoundMode::Enhanced
            } else {
                atv_game::audio::ride::SoundMode::Original
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
        room: (param("host").is_some_and(|v| v == "1") || param("room").is_some()).then(|| {
            RoomStart {
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
            }
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
