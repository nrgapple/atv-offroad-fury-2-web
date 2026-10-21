//! One game application for every build. The same event loop, loader and
//! renderer run natively (Metal/DX12/Vulkan) and in browsers (WebGL2).
mod gamepad;
pub mod replay;
mod riding;
mod sound;

use atv_content::{Content, Store};
use atv_game::venue::{self, Venue};
use atv_render::{Camera, Gpu, SceneRenderer, View, camera::default_focal_length};
use std::{collections::HashSet, sync::Arc};
use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

pub struct Options {
    pub store: Store,
    pub venue: String,
    /// Fixed `x,y,z,yaw,pitch,focal` view, e.g. copied from the browser build.
    pub camera: Option<[f32; 6]>,
    /// Native parity capture: render once at this size, write PNG and exit.
    #[cfg(not(target_arch = "wasm32"))]
    pub capture: Option<(std::path::PathBuf, u32, u32)>,
    /// Ride the player's quad (default); false keeps the free camera.
    pub ride: bool,
    /// Steps replayed at load, then the ride holds (parity captures).
    pub script: Option<replay::Script>,
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
    /// A fixed `--camera` view overrides the chase camera.
    fixed: bool,
    riding: Option<riding::Riding>,
}

enum Message {
    Ready(Box<Ready>),
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
    since: Option<Instant>,
    triangles: u32,
}

async fn prepare(
    window: Arc<Window>,
    display: OwnedDisplayHandle,
    store: Store,
    environment: String,
    fixed: Option<[f32; 6]>,
    ride: bool,
    script: Option<replay::Script>,
    sound: Option<(atv_game::audio::ride::SoundMode, bool)>,
    autodrive: Option<replay::Script>,
) -> Result<Ready, String> {
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
    let venue = venue::load(&content, &environment, false).await?;
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
    let mut scene = SceneRenderer::new(&gpu, &venue, Some([start[0], start[2]]));
    let riding = if ride {
        // Default Garage selection: quad 1, male rider.
        let ride = atv_game::ride::load(&content, &venue, 1, 0, status).await?;
        scene.set_ride(&gpu, &venue, &ride.quad, &ride.rider)?;
        let mut riding = riding::Riding::new(ride, &venue);
        if let Some(script) = &script {
            script.run(&mut riding)?;
            // Hold the replayed state for screenshots.
            riding.paused = true;
            riding.held = true;
            publish_trace(&replay::record(&riding.ride, riding.ride.steps()).to_string());
        }
        if let (Some((mode, music)), None) = (sound, &script) {
            status("Reading original sounds…");
            match sound::Sound::load(
                content.clone(),
                &venue,
                &riding.ride,
                sound::Settings { mode, music },
            )
            .await
            {
                Ok(sound) => riding.sound = Some(sound),
                // A missing device or bank leaves the ride silent, not stopped.
                Err(error) => status(&format!("Sound unavailable: {error}")),
            }
        }
        riding.autodrive = autodrive.map(|script| (script.controls(), 0));
        Some(riding)
    } else {
        None
    };
    Ok(Ready {
        gpu,
        venue,
        scene,
        camera,
        focal_length,
        fixed: fixed.is_some(),
        riding,
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
        match ready.scene.render(
            &ready.gpu,
            &View {
                camera: ready.camera,
                focal_length: ready.focal_length,
            },
        ) {
            Ok(stats) => {
                self.meter.triangles = stats.terrain_triangles;
                acquire = stats.acquire_ms;
            }
            Err(error) => status(&error),
        }
        self.meter.cpu += started.elapsed().as_secs_f64() * 1000. - acquire;
        self.meter_frame(now, "");
    }

    /// One rendered frame of the ride: input, fixed steps, chase view.
    fn ride_frame(&mut self, dt: f32) {
        let now = Instant::now();
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        let started = Instant::now();
        let pad = self.gamepad.sample();
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
        if let Err(error) = riding.frame(&sample, dt) {
            status(&format!("Simulation stopped: {error}"));
            ready.riding = None;
            return;
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
        let render_started = Instant::now();
        let mut acquire = 0.;
        match ready.scene.render(
            &ready.gpu,
            &View {
                camera: ready.camera,
                focal_length: ready.focal_length,
            },
        ) {
            Ok(stats) => {
                self.meter.triangles = stats.terrain_triangles + stats.vehicle_triangles;
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
            Some(sound) if sound.output.started => format!(
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
        let extra = format!(
            " · {:.0} MPH{sound}{}",
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
                "{} · {:.0} fps · {:.1} ms frame · {:.2} ms CPU · {} tris · {:?} {}{extra}",
                ready.venue.environment.to_uppercase(),
                self.meter.frames as f64 / elapsed,
                elapsed * 1000. / self.meter.frames as f64,
                self.meter.cpu / self.meter.frames as f64,
                self.meter.triangles,
                info.backend,
                info.name
            ));
            self.meter = Meter {
                triangles: self.meter.triangles,
                ..Default::default()
            };
        }
    }
}

impl App {
    /// The first key, click or touch starts audio (browser autoplay rules).
    fn gesture(&mut self) {
        if let Some(sound) = self
            .ready
            .as_mut()
            .and_then(|r| r.riding.as_mut())
            .and_then(|r| r.sound.as_mut())
        {
            sound.start();
        }
    }

    fn key_pressed(&mut self, code: KeyCode) {
        let now = (Instant::now() - self.started).as_secs_f64();
        let names: HashSet<String> = self.keys.iter().map(|k| format!("{k:?}")).collect();
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            riding.key_pressed(&format!("{code:?}"), now, &|c: &str| names.contains(c));
        }
    }
}

impl ApplicationHandler<Message> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(options) = self.options.take() else {
            return;
        };
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
            options.ride,
            options.script,
            options.sound,
            options.autodrive,
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
                    let (camera, focal) = riding
                        .present(&ready.gpu, &mut ready.scene, &ready.venue, aspect, 0.)
                        .expect("present ride");
                    if !ready.fixed {
                        ready.camera = camera;
                        ready.focal_length = focal;
                    }
                }
                capture(ready, &path, width, height);
                event_loop.exit();
                return;
            }
            send(result);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, message: Message) {
        match message {
            Message::Ready(ready) => {
                self.ready = Some(*ready);
                // Desktop has no autoplay rule: sound starts with the ride.
                if !cfg!(target_arch = "wasm32") {
                    self.gesture();
                }
                self.last = Instant::now();
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    self.ready
                        .as_mut()
                        .unwrap()
                        .gpu
                        .resize(size.width, size.height);
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(ready) = &mut self.ready {
                    ready.gpu.resize(size.width, size.height);
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
                if !focused {
                    self.keys.clear();
                }
                if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
                    if !focused {
                        riding.clear_input();
                    }
                    riding.set_focused(focused);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
                if self.dragging {
                    self.gesture();
                }
            }
            WindowEvent::Touch(_) => self.gesture(),
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
            }
            WindowEvent::RedrawRequested => {
                self.step();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn capture(ready: &mut Ready, path: &std::path::Path, width: u32, height: u32) {
    let view = View {
        camera: ready.camera,
        focal_length: ready.focal_length,
    };
    let rgba = ready
        .scene
        .capture(&ready.gpu, &view, width, height)
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
        ride: param("free").is_none(),
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
        script: param("script")
            .and_then(|text| js_sys_decode(&text))
            .and_then(|text| replay::Script::parse(&text).ok()),
    });
}

#[cfg(target_arch = "wasm32")]
fn js_sys_decode(text: &str) -> Option<String> {
    js_sys::decode_uri_component(text)
        .ok()
        .and_then(|s| s.as_string())
}
