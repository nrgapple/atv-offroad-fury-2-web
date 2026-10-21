//! One game application for every build. The same event loop, loader and
//! renderer run natively (Metal/DX12/Vulkan) and in browsers (WebGL2).
mod gamepad;
mod overlay;
pub mod replay;
mod riding;
mod sound;

use atv_content::{Content, Store};
use atv_game::venue::{self, Venue};
use atv_render::{Camera, Gpu, Graphics, SceneRenderer, View, camera::default_focal_length};
use std::{collections::HashSet, sync::Arc};
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
    /// Native parity capture: render once at this size, write PNG and exit.
    #[cfg(not(target_arch = "wasm32"))]
    pub capture: Option<(std::path::PathBuf, u32, u32)>,
    /// Draw the ride UI (toolbar, HUD, minimap, keyboard card) into captures,
    /// with default in-memory settings.
    #[cfg(not(target_arch = "wasm32"))]
    pub capture_ui: bool,
    /// Ride the player's quad (default); false keeps the free camera.
    pub ride: bool,
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
    /// Native: write the autodrive CPU report here and exit when it ends.
    #[cfg(not(target_arch = "wasm32"))]
    pub bench: Option<std::path::PathBuf>,
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
}

struct PendingSound {
    content: std::rc::Rc<Content>,
    environment: String,
    settings: sound::Settings,
}

enum Message {
    Ready(Box<Ready>),
    SoundReady(Box<sound::Sound>),
    SoundFailed(String),
    Failed(String),
}

struct App {
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
    race: Option<atv_game::ride::race::Settings>,
    weather: Option<bool>,
    steps: Option<u64>,
    graphics: Graphics,
    ride: bool,
    script: Option<replay::Script>,
    sound: Option<(atv_game::audio::ride::SoundMode, bool)>,
    autodrive: Option<replay::Script>,
}

async fn prepare(
    window: Arc<Window>,
    display: OwnedDisplayHandle,
    store: Store,
    environment: String,
    fixed: Option<[f32; 6]>,
    world: World,
) -> Result<Ready, String> {
    let World {
        race,
        weather,
        steps,
        graphics,
        ride,
        script,
        sound,
        autodrive,
    } = world;
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
    let gpu = Gpu::new(&instance, surface, size.width, size.height).await?;
    status(&format!("Reading original {}…", environment.to_uppercase()));
    let content = std::rc::Rc::new(Content::open(store).await?);
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
    let start = venue.scene.start_position;
    let direction = venue.scene.start_direction;
    let yaw = direction[0].atan2(direction[2]);
    let mut camera = Camera {
        x: start[0],
        y: start[1],
        z: start[2],
        yaw,
        pitch: -0.25,
    };
    camera.x -= yaw.sin() * 14.;
    camera.z -= yaw.cos() * 14.;
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
        // Default Garage selection: quad 1, male rider.
        let ride = atv_game::ride::load_with(&content, &venue, 1, 0, race, status).await?;
        let mut ride = ride;
        scene.set_ride(&gpu, &venue, &ride.quad, &ride.rider)?;
        if let Some(race) = ride.race_mut() {
            scene.set_race_cpus(&gpu, &venue, &std::mem::take(&mut race.draw))?;
        }
        let mut riding = riding::Riding::new(ride, &venue);
        // `?autopilot=1` (parity tooling): the race path driver rides the
        // player before the finish too; traces record its controls.
        if url_flag("autopilot=1")
            && let Some(state) = riding.ride.race_mut()
        {
            state.autopilot_always = true;
        }
        // startSimulation's first frame (op 23) primes the wheel effects.
        if let Some(mut effects) = atv_game::wheel_effects::load(&content, &venue, 1).await? {
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
                settings: sound::Settings { mode, music },
            });
        }
        riding.autodrive = autodrive.map(|script| (script.controls(), 0));
        Some(riding)
    } else {
        None
    };
    let hud_assets = if riding.is_some() {
        match atv_ui::hud::assets::load(&content, &environment).await {
            Ok(assets) => Some(assets),
            Err(error) => {
                status(&format!("HUD resources unavailable: {error}"));
                None
            }
        }
    } else {
        None
    };
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
    })
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
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        let Some(ready) = &mut self.ready else { return };
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
            overlay.sync_menu(riding.paused && !riding.held);
            riding.menu_open = overlay.menu.is_some();
            if riding.menu_open
                && let Some(event) = overlay.menu_pad(
                    pad.as_ref().map_or([0.; 17], |p| p.buttons),
                    pad.as_ref().map_or([0.; 4], |p| p.axes),
                )
            {
                self.menu_event(event);
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
        let active = !riding.paused && !riding.held && focused;
        let ui_started = Instant::now();
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
            overlay.prepare(
                &gpu.device,
                &gpu.queue,
                gpu.config.width,
                gpu.config.height,
                ms,
                active,
                !riding.paused,
                riding.paused,
                Some(overlay::HudInput {
                    hud: ride.hud(),
                    rider_world: ride.rider_world(),
                    terrain: &ready.venue.terrain,
                }),
            );
        }
        self.meter.ui += ui_started.elapsed().as_secs_f64() * 1000.;
        if self.ui_diagnostics {
            let mut state = replay::record(&riding.ride, riding.ride.steps());
            state["camera"]["preset"] = riding.ride.camera().0.preset.into();
            state["paused"] = riding.paused.into();
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
            bench.frames += 1;
            bench.cpu += cpu;
            bench.sim += riding.sim_ms;
            bench.render += render;
            bench.audio += audio;
        } else if riding.autodrive_done && !self.bench.reported {
            self.bench.reported = true;
            let b = &self.bench;
            let n = f64::from(b.frames.max(1));
            let report = format!(
                r#"{{"frames":{},"cpu_ms_per_frame":{:.4},"cpu_ms_per_60hz_step":{:.4},"sim_and_sound_ms":{:.4},"render_ms":{:.4},"mixer_ms":{:.4},"steps":{}}}"#,
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
    /// autodrive benchmarks keep rendering. (No multiplayer clock runs in the
    /// Rust engine yet; a shared ride must keep its clocks live when it does.)
    fn awake(&self) -> bool {
        let Some(ready) = &self.ready else {
            return true;
        };
        if let Some(riding) = &ready.riding
            && (riding.held || riding.autodrive.is_some())
        {
            return true;
        }
        self.effective_focused() && ready.riding.as_ref().is_none_or(|r| !r.paused)
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
            Event::Changed => {
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
            match touch.phase {
                TouchPhase::Started => overlay.menu_press(id, x, y),
                TouchPhase::Moved => overlay.menu_drag(id, x, y),
                TouchPhase::Ended => {
                    if let Some(event) = overlay.menu_release(id, x, y) {
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

    fn key_pressed(&mut self, code: KeyCode) {
        if let Some(event) = self
            .overlay
            .as_mut()
            .filter(|o| o.menu.is_some())
            .map(|o| o.menu_key(&format!("{code:?}")))
        {
            if let Some(event) = event {
                self.menu_event(event);
            }
            return;
        }
        let now = (Instant::now() - self.started).as_secs_f64();
        let names: HashSet<String> = self.keys.iter().map(|k| format!("{k:?}")).collect();
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            riding.key_pressed(&format!("{code:?}"), now, &|c: &str| names.contains(c));
            // Any key skips the pre-race camera (00212d40).
            riding.ride.skip_intro();
            let riding = !riding.paused && !riding.held;
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
        let future = prepare(
            window,
            self.display.clone(),
            options.store,
            options.venue,
            options.camera,
            World {
                race: options.race,
                weather: options.weather,
                steps: options.steps,
                graphics: options.graphics,
                ride: options.ride,
                script: options.script,
                sound: options.sound,
                autodrive: options.autodrive,
            },
        );
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
            Message::Ready(ready) => {
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
                        overlay.set_course(&ready.venue);
                        if let Some(assets) = ready.hud_assets.take() {
                            overlay.set_meter(&ready.gpu.device, &ready.gpu.queue, assets);
                        }
                        self.overlay = Some(overlay);
                    }
                    window.request_redraw();
                }
            }
            Message::Failed(error) => {
                status(&error);
                if !cfg!(target_arch = "wasm32") {
                    event_loop.exit();
                }
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
                if let Some(overlay) = &mut self.overlay {
                    let dy = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => -y * 40.,
                        winit::event::MouseScrollDelta::PixelDelta(p) => {
                            -(p.y / self.window.as_ref().map_or(1., |w| w.scale_factor())) as f32
                        }
                    };
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
                if let Some(overlay) = self.overlay.as_mut().filter(|o| o.menu.is_some()) {
                    self.dragging = false;
                    if state == ElementState::Pressed {
                        overlay.menu_press(overlay::MOUSE, x, y);
                    } else if let Some(event) = overlay.menu_release(overlay::MOUSE, x, y) {
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
                        overlay.menu_drag(overlay::MOUSE, x, y);
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
        if self.asleep && matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.paused_tick();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Asleep in focus (paused): a 50 ms tick; away: wait for events.
        event_loop.set_control_flow(if self.asleep && self.effective_focused() {
            ControlFlow::WaitUntil(Instant::now() + std::time::Duration::from_millis(50))
        } else {
            ControlFlow::Wait
        });
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
                }),
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

/// Display -> Graphics preset features (presetFeatures in web/graphics.js).
pub fn graphics_preset(name: &str) -> Graphics {
    match name {
        "high" => Graphics {
            sky_smooth: true,
            anisotropy: 16,
            fresnel: true,
            dust: 96,
            contact_shadows: true,
            shadow_map: 2048,
            tire_tracks: (3600, 75),
        },
        "enhanced" => Graphics {
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
    let event_loop = EventLoop::<Message>::with_user_event()
        .build()
        .expect("event loop");
    let app = App {
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
        bench: Bench::default(),
        #[cfg(not(target_arch = "wasm32"))]
        bench_out,
    };
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
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
    let venue = param("venue")
        .filter(|id| atv_game::launch::environments().contains(id))
        .unwrap_or_else(|| "env01".into());
    run(Options {
        store: Store::Http("/content/".into()),
        venue,
        camera: param("camera").and_then(|text| parse_camera(&text)),
        weather: param("weather").and_then(|v| parse_switch(&v)),
        steps: param("steps").and_then(|v| v.parse().ok()),
        graphics: param("graphics").map_or_else(Graphics::default, |v| graphics_preset(&v)),
        ride: param("free").is_none(),
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
