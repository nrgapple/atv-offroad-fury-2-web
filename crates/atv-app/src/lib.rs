//! One game application for every build. The same event loop, loader and
//! renderer run natively (Metal/DX12/Vulkan) and in browsers (WebGL2).
use atv_content::{Content, Store};
use atv_game::venue::{self, Venue};
use atv_render::{Camera, Gpu, Graphics, SceneRenderer, View, camera::default_focal_length};
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
    /// Forced Weather outcome (parity captures); random per load otherwise.
    pub weather: Option<bool>,
    /// World clock steps to run before the first frame, then hold the clocks
    /// (parity captures against a paused JavaScript frame).
    pub steps: Option<u64>,
    pub graphics: Graphics,
    /// Native parity capture: render once at this size, write PNG and exit.
    #[cfg(not(target_arch = "wasm32"))]
    pub capture: Option<(std::path::PathBuf, u32, u32)>,
}

struct Ready {
    gpu: Gpu,
    venue: Venue,
    scene: SceneRenderer,
    camera: Camera,
    focal_length: f32,
    /// World clocks held at the requested steps (parity captures).
    hold: bool,
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
}

#[derive(Default)]
struct Meter {
    frames: u32,
    cpu: f64,
    since: Option<Instant>,
    triangles: u32,
    draws: u32,
}

async fn prepare(
    window: Arc<Window>,
    display: OwnedDisplayHandle,
    store: Store,
    environment: String,
    fixed: Option<[f32; 6]>,
    world: (Option<bool>, Option<u64>, Graphics),
) -> Result<Ready, String> {
    let (weather, steps, graphics) = world;
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
    let content = Content::open(store).await?;
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
        false,
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
        scene.advance_steps(steps);
    }
    Ok(Ready {
        gpu,
        venue,
        scene,
        camera,
        focal_length,
        hold: steps.is_some(),
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
        if !ready.hold {
            ready.scene.advance(dt);
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
            }
            Err(error) => status(&error),
        }
        self.meter.cpu += started.elapsed().as_secs_f64() * 1000.;
        self.meter.frames += 1;
        let since = *self.meter.since.get_or_insert(now);
        let elapsed = (now - since).as_secs_f64();
        if elapsed >= 1. {
            let info = &ready.gpu.adapter_info;
            status(&format!(
                "{} · {:.0} fps · {:.1} ms frame · {:.2} ms CPU · {} draws · {} tris · {:?} {}",
                ready.venue.environment.to_uppercase(),
                self.meter.frames as f64 / elapsed,
                elapsed * 1000. / self.meter.frames as f64,
                self.meter.cpu / self.meter.frames as f64,
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
            (options.weather, options.steps, options.graphics),
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
                        ElementState::Pressed => self.keys.insert(code),
                        ElementState::Released => self.keys.remove(&code),
                    };
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (true, Some((x, y)), Some(ready)) =
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
        },
        "enhanced" => Graphics {
            sky_smooth: true,
            anisotropy: 4,
            fresnel: true,
        },
        _ => Graphics::default(),
    }
}

fn status(text: &str) {
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
    });
}
