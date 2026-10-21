//! wgpu renderer shared by every build: Metal on macOS, DX12/Vulkan on
//! Windows, Vulkan on Linux and WebGL2 in browsers.
// Presentation only: not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]
mod ambient;
mod blood;
pub mod camera;
mod cull;
mod lingering_dust;
pub mod preview;
mod profile;
mod scenery;
mod shadows;
mod sky;
mod snow;
mod speed_rush;
mod terrain;
mod texture;
mod tire_tracks;
mod train;
mod vegetation;
mod vehicle;
mod water;
mod wheel_effects;
mod world;

use atv_game::venue::Venue;
pub use camera::Camera;

/// Depth plus the stencil the speed-rush pass masks the player's vehicle with.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub adapter_info: wgpu::AdapterInfo,
    /// GPU timestamp queries are available (native Metal/DX12/Vulkan).
    pub timestamps: bool,
    samples: u32,
    depth: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
}

impl Gpu {
    /// Presents a frame drawn only by `draw` over black (menus without a
    /// scene, e.g. the frontend).
    pub fn present_with(
        &self,
        draw: &mut dyn FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<(), String> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            other => return Err(format!("surface: {other:?}")),
        };
        let target = frame.texture.create_view(&Default::default());
        let mut encoder = self.clear(&target);
        draw(&mut encoder, &target);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(())
    }

    /// Offscreen `present_with` (native parity captures): RGBA pixels.
    pub fn capture_ui(
        &self,
        width: u32,
        height: u32,
        draw: &mut dyn FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<Vec<u8>, String> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        let mut encoder = self.clear(&target);
        draw(&mut encoder, &target);
        read_texture(self, encoder, &texture, width, height)
    }

    fn clear(&self, target: &wgpu::TextureView) -> wgpu::CommandEncoder {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        encoder
    }

    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| format!("no graphics adapter: {e}"))?;
        // Timestamp queries only time native GPU passes (profiling); WebGL2 has none.
        let timestamps = !cfg!(target_arch = "wasm32")
            && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("fury"),
                required_features: if timestamps {
                    wgpu::Features::TIMESTAMP_QUERY
                } else {
                    wgpu::Features::empty()
                },
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("graphics device: {e}"))?;
        let capabilities = surface.get_capabilities(&adapter);
        // The browser canvas writes shader output without sRGB encoding; match it.
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        // The WebGL context requests antialias: true (4x on every target we ship).
        let samples = if adapter
            .get_texture_format_features(format)
            .flags
            .sample_count_supported(4)
        {
            4
        } else {
            1
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let (depth, msaa) = targets(&device, &config, samples);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            adapter_info: adapter.get_info(),
            timestamps,
            samples,
            depth,
            msaa,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width, height) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth, self.msaa) = targets(&self.device, &self.config, self.samples);
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }
}

fn targets(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    samples: u32,
) -> (wgpu::TextureView, Option<wgpu::TextureView>) {
    let size = wgpu::Extent3d {
        width: config.width,
        height: config.height,
        depth_or_array_layers: 1,
    };
    let target = |format, label| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    (
        target(DEPTH_FORMAT, "depth"),
        (samples > 1).then(|| target(config.format, "msaa")),
    )
}

/// Per-frame scene state the renderer needs.
pub struct View {
    pub camera: Camera,
    pub focal_length: f32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct FrameStats {
    pub terrain_draws: u32,
    pub terrain_triangles: u32,
    pub sky_tiles: u32,
    pub scenery_draws: u32,
    pub scenery_triangles: u32,
    pub scenery_culled: u32,
    pub vegetation_draws: u32,
    pub vegetation_triangles: u32,
    pub vegetation_visible: u32,
    pub water_draws: u32,
    pub train_draws: u32,
    pub train_triangles: u32,
    pub ambient_draws: u32,
    pub ambient_triangles: u32,
    pub snow_draws: u32,
    pub snow_triangles: u32,
    pub wheel_draws: u32,
    pub wheel_triangles: u32,
    pub shadow_draws: u32,
    pub vehicle_triangles: u32,
    /// Time spent waiting for the swapchain image (vsync), not CPU work.
    pub acquire_ms: f64,
}

impl FrameStats {
    pub fn draws(&self) -> u32 {
        self.terrain_draws
            + self.sky_tiles
            + self.scenery_draws
            + self.vegetation_draws
            + self.water_draws
            + self.train_draws
            + self.ambient_draws
            + self.snow_draws
            + self.wheel_draws
            + self.shadow_draws
    }
    pub fn triangles(&self) -> u32 {
        self.terrain_triangles
            + self.sky_tiles * 2
            + self.scenery_triangles
            + self.vegetation_triangles
            + self.train_triangles
            + self.ambient_triangles
            + self.snow_triangles
            + self.wheel_triangles
            + self.vehicle_triangles
    }
}

/// Presentation options of the Display -> Graphics presets (web/graphics.js).
/// Original keeps both off; Enhanced/High smooth the sky and filter
/// scenery textures anisotropically (4x/16x).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Graphics {
    /// Enhanced/High: draw ATVs between 60 Hz states (graphics.js
    /// `interpolation`; motorcycles interpolate in every preset).
    pub interpolation: bool,
    pub sky_smooth: bool,
    pub anisotropy: u16,
    /// Authored Schlick Fresnel on the water surface.
    pub fresnel: bool,
    /// Lingering dust puff pool (Enhanced 24, High 96; 0 off).
    pub dust: u16,
    /// Soft contact shadows under vehicles (Enhanced/High).
    pub contact_shadows: bool,
    /// Vehicle-only sun shadow map size (High 2048; 0 off).
    pub shadow_map: u16,
    /// Tire track ring segments and their life in seconds (Enhanced 600/15,
    /// High 3600/75; 0 off).
    pub tire_tracks: (u16, u16),
}

pub struct SceneRenderer {
    terrain: terrain::TerrainPass,
    world: world::World,
    sky: Option<sky::SkyPass>,
    scenery: scenery::SceneryPass,
    vegetation: Option<vegetation::VegetationPass>,
    train: Option<(
        train::TrainPass,
        std::rc::Rc<std::cell::RefCell<atv_game::train::Train>>,
    )>,
    ambient: Option<(
        ambient::AmbientPass,
        std::rc::Rc<std::cell::RefCell<atv_game::ambient::Ambient>>,
    )>,
    snow: Option<(
        snow::SnowPass,
        std::rc::Rc<std::cell::RefCell<atv_game::snow::Weather>>,
    )>,
    /// This step's simulation camera (position, target): the Snow rings
    /// follow it. Without one, the latest rendered view stands in.
    weather_view: Option<([f32; 3], [f32; 3])>,
    weather_camera: Camera,
    /// Pre-race shots of a race-course load, stepped with the world.
    intro: Option<(atv_game::intro::IntroCamera, atv_core::terrain::Heightfield)>,
    water: Option<(
        water::WaterPass,
        std::rc::Rc<std::cell::RefCell<atv_game::water::Water>>,
    )>,
    fog_color: [f32; 3],
    far: f32,
    /// Depth range: fog end plus the largest object, so an admitted stadium
    /// stand is not sliced open into the sky (matrix() in web/game.js).
    depth_far: f32,
    clock: f32,
    /// The ride's wheel dust, chunks, spray and wet births.
    wheel_effects: Option<wheel_effects::WheelEffectsPass>,
    /// Enhanced/High tire tracks.
    tire_tracks: Option<tire_tracks::TireTracksPass>,
    /// Enhanced/High lingering dust puffs.
    lingering_dust: Option<lingering_dust::LingeringDustPass>,
    graphics: Graphics,
    /// Visible terrain heights (renderCore.terrainHeight) for contact shadows.
    ground: atv_core::terrain::Heightfield,
    contact_shadows: Option<shadows::ContactShadows>,
    shadow_map: Option<shadows::ShadowMap>,
    /// Smoothed resting body height above the visible ground (restLift).
    rest_lift: Option<f32>,
    /// Quad world, rider world and, when detached, the rider's torso world.
    ride_pose: Option<([f32; 16], Option<[f32; 16]>)>,
    /// The drop gate matrix last posed.
    gate: Option<[f32; 16]>,
    /// Authored speed-rush screen pass, and the travel state it reads.
    speed_rush: Option<speed_rush::SpeedRushPass>,
    rush_motion: ([f32; 3], [f32; 3]),
    coarse_pointer: bool,
    /// Gorn blood, created on the first frame with blood.
    blood: Option<blood::BloodPass>,
    /// The player's quad and rider, once a ride is loaded.
    vehicle: Option<vehicle::VehiclePass>,
    /// Race CPU riders (drawn after the player) and which are near enough.
    cpus: Vec<(vehicle::VehiclePass, bool)>,
    profiler: profile::Profiler,
    /// Scene-pass GPU timestamps, while [`SceneRenderer::gpu_scene_ms`] runs.
    gpu_timer: Option<profile::GpuTimer>,
}

impl SceneRenderer {
    pub fn new(gpu: &Gpu, venue: &Venue, fine_center: Option<[f32; 2]>) -> Self {
        Self::with_graphics(gpu, venue, fine_center, Graphics::default())
    }

    pub fn with_graphics(
        gpu: &Gpu,
        venue: &Venue,
        fine_center: Option<[f32; 2]>,
        graphics: Graphics,
    ) -> Self {
        let fog = venue.scene.fog.as_ref();
        let fog_color = venue.scene.fog_color.map(|c| c / 255.);
        let far = fog.map_or(
            venue.terrain.size_x().max(venue.terrain.size_z()) * 5.,
            |f| f.end,
        );
        let mut world = world::World::new(gpu, venue, u32::from(graphics.shadow_map));
        let terrain = terrain::TerrainPass::new(
            gpu,
            venue,
            fine_center,
            gpu.samples,
            (&world.shadow_view, &world.shadow_sampler),
            graphics.anisotropy,
        );
        world.globals.sky[3] = graphics.sky_smooth as u8 as f32;
        let sky = venue
            .sky
            .as_ref()
            .map(|sky| sky::SkyPass::new(gpu, &world, sky, graphics.sky_smooth));
        let scenery = scenery::SceneryPass::new(gpu, &world, &venue.scenery, graphics.anisotropy);
        let vegetation = venue
            .vegetation
            .as_ref()
            .map(|v| vegetation::VegetationPass::new(gpu, &world, v, graphics.anisotropy));
        let water = venue.water.as_ref().and_then(|shared| {
            match water::WaterPass::new(gpu, &world, &shared.borrow(), graphics.fresnel) {
                Ok(pass) => Some((pass, shared.clone())),
                Err(error) => {
                    // The browser reports and skips an unsupported surface.
                    eprintln!("water: {error}");
                    None
                }
            }
        });
        let train = venue.train.as_ref().and_then(|shared| {
            match train::TrainPass::new(gpu, &world, &shared.borrow()) {
                Ok(pass) => Some((pass, shared.clone())),
                Err(error) => {
                    eprintln!("train: {error}");
                    None
                }
            }
        });
        let ambient = venue.ambient.as_ref().map(|shared| {
            (
                ambient::AmbientPass::new(gpu, &world, &shared.borrow()),
                shared.clone(),
            )
        });
        let snow = venue.snow.as_ref().map(|shared| {
            (
                snow::SnowPass::new(gpu, &world, &shared.borrow()),
                shared.clone(),
            )
        });
        let depth_far = far + scenery.reach.max(terrain.reach()) + 1.;
        let mut scene = Self {
            terrain,
            world,
            sky,
            scenery,
            vegetation,
            train,
            ambient,
            snow,
            weather_view: None,
            weather_camera: Camera::default(),
            intro: atv_game::intro::IntroCamera::new(
                venue.intro_shots.clone(),
                venue.scene.start_position,
                venue.scene.start_direction,
            )
            .map(|intro| {
                let t = &venue.terrain;
                (
                    intro,
                    atv_core::terrain::Heightfield {
                        width: t.width,
                        depth: t.depth,
                        spacing: f64::from(t.spacing),
                        heights: t.heights.clone(),
                    },
                )
            }),
            water,
            fog_color,
            far,
            depth_far,
            clock: 0.,
            vehicle: None,
            cpus: Vec::new(),
            profiler: profile::Profiler::default(),
            gpu_timer: None,
            wheel_effects: None,
            lingering_dust: None,
            tire_tracks: None,
            graphics,
            ground: {
                let t = &venue.terrain;
                atv_core::terrain::Heightfield {
                    width: t.width,
                    depth: t.depth,
                    // The page received the shortest decimal of the binary32.
                    spacing: t
                        .spacing
                        .to_string()
                        .parse()
                        .unwrap_or(f64::from(t.spacing)),
                    heights: t.heights.clone(),
                }
            },
            contact_shadows: None,
            shadow_map: None,
            rest_lift: None,
            speed_rush: None,
            rush_motion: ([0.; 3], [0.; 3]),
            coarse_pointer: false,
            blood: None,
            ride_pose: None,
            gate: None,
        };
        // startSimulation's first follow-camera request (op 46, dt 1/60)
        // already advances the pre-race shots once.
        scene.step_intro();
        scene
    }

    /// Advances world animation (scenery UV scroll and ATM clocks) by
    /// `seconds`, in whole 60 Hz simulation steps like the browser.
    pub fn advance(&mut self, seconds: f32) {
        self.clock = (self.clock + seconds.clamp(0., 0.1)).min(4. / 60.);
        while self.clock >= 1. / 60. {
            self.clock -= 1. / 60.;
            self.step_world();
        }
    }

    /// One simulation step of the world clocks: scenery animation and the
    /// live water field (which physics will share).
    fn step_world(&mut self) {
        self.scenery.step();
        if let Some((_, shared)) = &self.train {
            // Presentation-only clock; physics installs the train collision
            // heights (step_with_scene) once the ride shares this runtime.
            if shared
                .borrow_mut()
                .runtime
                .step(1. / 60., |_| Ok(None))
                .is_err()
            {
                self.train = None;
            }
        }
        if let Some((pass, shared)) = &mut self.ambient {
            // Moving actors (the locomotive's Smoke.slt) publish this step's pose.
            let actors: Vec<(String, [f32; 16])> = self
                .train
                .as_ref()
                .and_then(|(_, train)| train.borrow().runtime.frame().ok())
                .map(|frame| {
                    frame
                        .cars
                        .into_iter()
                        .map(|c| (c.model, c.matrix))
                        .collect()
                })
                .unwrap_or_default();
            let mut ambient = shared.borrow_mut();
            if ambient.step(&actors).is_ok() {
                pass.accept(&ambient);
            } else {
                drop(ambient);
                self.ambient = None;
            }
        }
        self.step_intro();
        if let Some((pass, shared)) = &mut self.snow {
            // The browser steps Snow around its simulation camera (follow or
            // intro); the latest rendered view stands in when none is given.
            let (position, forward) = match self.weather_view {
                Some((position, target)) => {
                    (position, std::array::from_fn(|i| target[i] - position[i]))
                }
                None => (
                    self.weather_camera.eye(),
                    self.weather_camera.axes().forward,
                ),
            };
            let mut weather = shared.borrow_mut();
            if pass.step(&mut weather, position, forward).is_err() {
                drop(weather);
                self.snow = None;
            }
        }
        if let Some((pass, shared)) = &mut self.water {
            let mut water = shared.borrow_mut();
            if water.step().and_then(|_| pass.accept(&water)).is_err() {
                drop(water);
                self.water = None;
            }
        }
    }

    /// One step of the pre-race shot camera; the Snow rings follow it.
    fn step_intro(&mut self) {
        if let Some((intro, ground)) = &mut self.intro {
            // The swoop ends on a follow pose behind the start (the browser
            // uses the player's follow camera; only the swoop reads it).
            let (rider, forward) = intro.start();
            let follow = atv_core::race_intro::Pose {
                position: [
                    rider[0] - forward[0] * 14.,
                    rider[1] + 6.,
                    rider[2] - forward[2] * 14.,
                ],
                target: rider,
                fov: 60.,
            };
            let ground = |x: f32, z: f32| ground.height(f64::from(x), f64::from(z)) as f32;
            match intro.step(follow, &ground) {
                Ok(Some(pose)) => self.weather_view = Some((pose.position, pose.target)),
                Ok(None) => {}
                Err(_) => self.intro = None,
            }
        }
    }

    /// The camera the weather rings follow until the next rendered frame.
    pub fn set_weather_camera(&mut self, camera: Camera) {
        self.weather_camera = camera;
    }

    /// The simulation camera of the next world step (the ride's follow
    /// camera: position and target), which the Snow rings follow.
    pub fn set_weather_view(&mut self, position: [f32; 3], target: [f32; 3]) {
        self.weather_view = Some((position, target));
    }

    /// The pre-race shot camera while it plays (cinematicPresentation in
    /// render-support.js): its pose with the portrait focal adaptation.
    pub fn intro_view(&self, aspect: f32) -> Option<(Camera, f32)> {
        let pose = self.intro.as_ref()?.0.pose()?;
        let v: [f32; 3] = std::array::from_fn(|i| pose.target[i] - pose.position[i]);
        let portrait = ((1. - aspect) / 0.5).clamp(0., 1.);
        let focal =
            (4. / 3.) / (pose.fov * std::f32::consts::PI / 360.).tan() * (1. - 0.12 * portrait);
        Some((
            Camera {
                x: pose.position[0],
                y: pose.position[1],
                z: pose.position[2],
                yaw: v[0].atan2(v[2]),
                pitch: v[1].atan2(v[0].hypot(v[2])),
            },
            focal,
        ))
    }

    /// Advances world animation by exact simulation steps (parity captures).
    pub fn advance_steps(&mut self, steps: u64) {
        for _ in 0..steps {
            self.step_world();
        }
    }

    /// Preset change: swap the sky between the original and smoothed decode
    /// (refreshSkyTextures). The venue must have loaded the smoothed tiles.
    pub fn set_sky_smooth(&mut self, gpu: &Gpu, venue: &Venue, smooth: bool) {
        if let (Some(pass), Some(sky)) = (&mut self.sky, &venue.sky) {
            pass.set_smooth(gpu, sky, smooth);
            self.world.globals.sky[3] = pass.smooth() as u8 as f32;
        }
    }

    /// Scenery clock steps taken so far.
    pub fn world_steps(&self) -> u64 {
        self.scenery.steps()
    }

    /// A stadium crowd flash burst towards `target` (player win/stunt, CPU
    /// finish; 0020ee10). The constant rate runs on its own clock.
    pub fn flash_burst(&mut self, target: f32) {
        if let Some(flashes) = &mut self.scenery.flashes {
            flashes.burst(target);
        }
    }

    /// One race step's world presentation, applied before that step's world
    /// clocks advance (atv-web steps the ambient emitters and flashes after
    /// the race): flash bursts, firework emitter commands, the drop gate.
    pub fn race_step(&mut self, gpu: &Gpu, events: &atv_game::ride::race::StepEvents) {
        use atv_game::ride::race::FlashBurst;
        if let Some(flashes) = &mut self.scenery.flashes {
            let config = flashes.config();
            for burst in &events.flashes {
                flashes.burst(match burst {
                    FlashBurst::PlayerWin => config.player_win,
                    FlashBurst::AiFinish => config.ai_finish,
                    FlashBurst::PlayerStunt => config.player_stunt,
                });
            }
        }
        if !events.fireworks.is_empty()
            && let Some((_, shared)) = &self.ambient
        {
            let mut ambient = shared.borrow_mut();
            for &(emitter, instance, fire) in &events.fireworks {
                ambient.effects.set_mode_emitter(emitter, instance, fire);
            }
        }
        if let Some(gate) = &events.gate
            && self.gate != Some(*gate)
        {
            self.gate = Some(*gate);
            self.scenery.pose_gate(gpu, gate);
        }
    }

    /// Installs the ride's wheel effects (WheelEffectsRenderer).
    pub fn set_wheel_effects(
        &mut self,
        gpu: &Gpu,
        effects: &atv_game::wheel_effects::WheelEffects,
    ) {
        self.wheel_effects = Some(wheel_effects::WheelEffectsPass::new(
            gpu,
            &self.world,
            effects,
        ));
    }

    /// The latest wheel-effect frame and every wet birth since the last call.
    pub fn accept_wheel_effects(
        &mut self,
        gpu: &Gpu,
        effects: &mut atv_game::wheel_effects::WheelEffects,
    ) {
        if let Some(pass) = &mut self.wheel_effects {
            pass.accept(effects);
        }
        let now = effects.frame().time;
        match &mut effects.tracks {
            Some((tracks, ..)) => {
                if self
                    .tire_tracks
                    .as_ref()
                    .is_none_or(|p| p.capacity() != tracks.capacity)
                {
                    self.tire_tracks = Some(tire_tracks::TireTracksPass::new(
                        gpu,
                        &self.world,
                        tracks.capacity,
                    ));
                    tracks.dirty = Some((0, tracks.capacity));
                }
                if let Some(pass) = &mut self.tire_tracks {
                    pass.accept(gpu, tracks, now);
                }
            }
            None => self.tire_tracks = None,
        }
        match &effects.dust {
            Some(dust) => {
                let capacity = dust.emitter.capacity;
                if self
                    .lingering_dust
                    .as_ref()
                    .is_none_or(|p| p.capacity() != capacity)
                {
                    self.lingering_dust = Some(lingering_dust::LingeringDustPass::new(
                        gpu,
                        &self.world,
                        capacity,
                    ));
                }
                if let Some(pass) = &mut self.lingering_dust {
                    pass.accept(gpu, &dust.emitter, effects.frame().time);
                }
            }
            None => self.lingering_dust = None,
        }
    }

    /// Uploads the ride's quad and rider models.
    pub fn set_ride(
        &mut self,
        gpu: &Gpu,
        venue: &Venue,
        quad: &atv_game::ride::Model,
        rider: &atv_game::ride::Model,
    ) -> Result<(), String> {
        let vehicle = vehicle::VehiclePass::new(gpu, venue, quad, rider, gpu.samples)?;
        if self.graphics.shadow_map > 0 {
            self.shadow_map = Some(shadows::ShadowMap::new(gpu, &self.world, &vehicle));
        }
        if self.graphics.contact_shadows {
            self.contact_shadows = Some(shadows::ContactShadows::new(gpu, &self.world));
        }
        self.vehicle = Some(vehicle);
        Ok(())
    }

    /// Uploads the race CPU riders' quads and outfitted riders.
    pub fn set_race_cpus(
        &mut self,
        gpu: &Gpu,
        venue: &Venue,
        models: &[(atv_game::ride::Model, atv_game::ride::Model)],
    ) -> Result<(), String> {
        self.cpus = models
            .iter()
            .map(|(quad, rider)| {
                vehicle::VehiclePass::new(gpu, venue, quad, rider, gpu.samples).map(|v| (v, false))
            })
            .collect::<Result<_, _>>()?;
        Ok(())
    }

    /// CPU poses by slot; the browser skips riders over 864ft from the eye.
    pub fn pose_race_cpus(&mut self, gpu: &Gpu, poses: &[atv_game::ride::Pose], eye: [f32; 3]) {
        for (index, (pass, visible)) in self.cpus.iter_mut().enumerate() {
            *visible = poses.get(index).is_some_and(|pose| {
                let [x, _, z] = [
                    pose.quad_world[12],
                    pose.quad_world[13],
                    pose.quad_world[14],
                ];
                (x - eye[0]).hypot(z - eye[2]) <= 864.
            });
            if *visible {
                pass.pose(gpu, &poses[index]);
            }
        }
    }

    /// One simulation step's body origin while Riding with every wheel down,
    /// for the contact shadows' resting lift (acceptFrame in game.js).
    pub fn observe_rest(&mut self, origin: [f32; 3]) {
        if self.contact_shadows.is_none() {
            return;
        }
        let rest = origin[1] - self.visible_ground(origin[0], origin[2]);
        if rest.is_finite() {
            self.rest_lift = Some(match self.rest_lift {
                Some(previous) => previous + (rest - previous) * 0.1,
                None => rest,
            });
        }
    }

    fn visible_ground(&self, x: f32, z: f32) -> f32 {
        self.ground.height(f64::from(x), f64::from(z)) as f32
    }

    /// This frame's speed-rush amount (the ride's FOV kick shares it), the
    /// render step and the player's origin and velocity.
    pub fn set_speed_rush(
        &mut self,
        gpu: &Gpu,
        amount: f32,
        dt: f32,
        origin: [f32; 3],
        velocity: [f32; 3],
    ) {
        if amount <= 0. && self.speed_rush.is_none() {
            return;
        }
        let coarse = self.coarse_pointer;
        let pass = self.speed_rush.get_or_insert_with(|| {
            let mut pass = speed_rush::SpeedRushPass::new(gpu, gpu.samples);
            pass.set_coarse(coarse);
            pass
        });
        pass.update(amount, dt);
        self.rush_motion = (origin, velocity);
    }

    /// Phones and tablets get fewer blur taps (speedRushTaps).
    pub fn set_coarse_pointer(&mut self, coarse: bool) {
        self.coarse_pointer = coarse;
        if let Some(pass) = &mut self.speed_rush {
            pass.set_coarse(coarse);
        }
    }

    /// Crazy/Gorn crash presentation: severed rider nodes and, under Gorn,
    /// this frame's blood.
    pub fn set_crash(
        &mut self,
        gpu: &Gpu,
        severed: &[usize],
        blood: Option<&atv_core::gore::BloodFrame>,
    ) {
        if let Some(vehicle) = &mut self.vehicle {
            vehicle.set_severed(gpu, severed);
        }
        let blood = blood.filter(|b| b.count() > 0 || !b.stumps.is_empty());
        if blood.is_some() || self.blood.is_some() {
            self.blood
                .get_or_insert_with(|| blood::BloodPass::new(gpu, &self.world))
                .accept(gpu, blood);
        }
    }

    /// The player's quad paint (`playerPaint`; CPU quads keep theirs).
    pub fn set_ride_paint(&mut self, paint: [f32; 2]) {
        if let Some(vehicle) = &mut self.vehicle {
            vehicle.set_paint(paint);
        }
    }

    /// Mud on the player's vehicle and rider (the motorcycle's amount; the
    /// ATV dirt reveal when `reveal`).
    pub fn set_ride_mud(&mut self, amount: f32, reveal: bool) {
        if let Some(vehicle) = &mut self.vehicle {
            vehicle.set_mud(amount, reveal);
        }
    }

    /// The latest simulation pose of the quad linkage and rider skeleton.
    pub fn pose_ride(&mut self, gpu: &Gpu, pose: &atv_game::ride::Pose) {
        if let Some(vehicle) = &self.vehicle {
            // Detached (wreck rig): the torso node under its rider world.
            let torso = (pose.rider_world != pose.quad_world).then(|| {
                pose.rider_nodes
                    .get(vehicle.pelvis)
                    .map_or(pose.rider_world, |node| {
                        camera::multiply(&pose.rider_world, node)
                    })
            });
            self.ride_pose = Some((pose.quad_world, torso));
            vehicle.pose(gpu, pose);
        }
    }

    /// Rebuilds the fine terrain patch around a new rider position.
    pub fn recenter_terrain(&mut self, gpu: &Gpu, venue: &Venue, center: [f32; 2]) {
        self.terrain.recenter(gpu, venue, center);
    }

    /// Per-pass CPU timing (bench runs); resets the accumulated report.
    pub fn set_profiling(&mut self, enabled: bool) {
        self.profiler.set_enabled(enabled);
    }

    /// Mean CPU milliseconds per frame for each pass since profiling began.
    pub fn profile_report(&self) -> Vec<(&'static str, f64)> {
        self.profiler.report()
    }

    /// Mean draws per frame of each pass while profiling.
    pub fn profile_counts(&self) -> Vec<(&'static str, f64)> {
        self.profiler.counts()
    }

    pub fn render(&mut self, gpu: &Gpu, view: &View) -> Result<FrameStats, String> {
        self.render_with(gpu, view, &mut |_, _| {})
    }

    /// Renders the scene, then `overlay` (e.g. the UI pass) into the same
    /// resolved frame before presenting it.
    pub fn render_with(
        &mut self,
        gpu: &Gpu,
        view: &View,
        overlay: &mut dyn FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<FrameStats, String> {
        let acquire = web_time::Instant::now();
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return Ok(FrameStats::default());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(FrameStats::default());
            }
            other => return Err(format!("surface: {other:?}")),
        };
        let acquire_ms = acquire.elapsed().as_secs_f64() * 1000.;
        self.profiler.start();
        let target = frame.texture.create_view(&Default::default());
        let (mut encoder, mut stats) = self.encode(
            gpu,
            view,
            (gpu.config.width, gpu.config.height),
            &target,
            gpu.msaa.as_ref(),
            &gpu.depth,
        );
        self.profiler.mark("encode.speed_rush");
        overlay(&mut encoder, &target);
        self.profiler.mark("encode.overlay");
        let commands = encoder.finish();
        self.profiler.mark("finish");
        gpu.queue.submit([commands]);
        self.profiler.mark("submit");
        gpu.queue.present(frame);
        self.profiler.mark("present");
        self.profiler.end_frame();
        stats.acquire_ms = acquire_ms;
        Ok(stats)
    }

    fn encode(
        &mut self,
        gpu: &Gpu,
        view: &View,
        size: (u32, u32),
        target: &wgpu::TextureView,
        msaa: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
    ) -> (wgpu::CommandEncoder, FrameStats) {
        let aspect = size.0 as f32 / size.1 as f32;
        // 0012b360 applies the active fog end to the far clipping plane.
        // Whole objects are culled there; depth extends by the largest object.
        let view_projection =
            view.camera
                .view_projection(aspect, view.focal_length, 1., self.depth_far);
        let cull = cull::planes(&view.camera.view_projection(
            aspect,
            view.focal_length,
            1.,
            self.far,
        ));
        let eye = view.camera.eye();
        // High: the vehicle-only sun shadow map around the ride, received by
        // terrain and scenery (renderVehicleShadows).
        let light = match (&self.shadow_map, &self.vehicle, &self.ride_pose) {
            (Some(map), Some(_), Some((quad, _))) => {
                let sun = self.world.globals.sun_direction;
                Some(shadows::light_matrices(
                    [quad[12], quad[13], quad[14]],
                    [sun[0], sun[1], sun[2]],
                    map.size(),
                ))
            }
            _ => None,
        };
        let texel = 1. / self.world.shadow_size as f32;
        self.terrain.set_shadow(light.as_ref().map(|l| &l.1), texel);
        match &light {
            Some((_, texture)) => {
                self.world.globals.shadow_matrix = *texture;
                self.world.globals.shadow = [1., texel, 0., 0.];
            }
            None => self.world.globals.shadow = [0.; 4],
        }
        self.profiler.mark("prepare.setup");
        let mut stats = self.terrain.prepare(gpu, &view_projection, &cull, eye);
        self.profiler.mark("prepare.terrain");
        self.world
            .prepare(gpu, &view_projection, eye, view.camera.yaw);
        self.profiler.mark("prepare.world");
        stats.sky_tiles = self.sky.as_ref().map_or(0, |s| s.tiles());
        let scenery = self.scenery.prepare(gpu, &cull);
        stats.scenery_draws = scenery.draws;
        stats.scenery_triangles = scenery.triangles;
        stats.scenery_culled = scenery.culled;
        self.profiler.mark("prepare.scenery");
        if let Some((pass, shared)) = &mut self.train {
            (stats.train_draws, stats.train_triangles) = pass.prepare(gpu, &shared.borrow());
        }
        self.profiler.mark("prepare.train");
        if let Some(vegetation) = &mut self.vegetation {
            let v = vegetation.prepare(gpu, eye, view.camera.yaw);
            stats.vegetation_draws = v.draws;
            stats.vegetation_triangles = v.triangles;
            stats.vegetation_visible = v.visible;
        }
        self.profiler.mark("prepare.vegetation");
        if let Some((water, _)) = &mut self.water {
            water.prepare(
                gpu,
                &view_projection,
                &view.camera,
                view.focal_length,
                size,
                self.far,
            );
            stats.water_draws = 1;
        }
        self.profiler.mark("prepare.water");
        if let Some((pass, shared)) = &mut self.ambient {
            (stats.ambient_draws, stats.ambient_triangles) =
                pass.prepare(gpu, &shared.borrow(), &view.camera, self.far);
        }
        self.profiler.mark("prepare.ambient");
        self.weather_camera = view.camera;
        if let Some((pass, _)) = &mut self.snow {
            (stats.snow_draws, stats.snow_triangles) = pass.prepare(gpu, &view.camera);
        }
        if let Some(pass) = &mut self.wheel_effects {
            (stats.wheel_draws, stats.wheel_triangles) = pass.prepare(gpu, &view.camera);
        }
        if let Some(pass) = &mut self.tire_tracks {
            let (draws, triangles) = pass.prepare(gpu);
            stats.wheel_draws += draws;
            stats.wheel_triangles += triangles;
        }
        if let Some(pass) = &mut self.lingering_dust {
            let fog_end = if self.world.globals.fog[2] > 0.5 {
                self.far
            } else {
                768.
            };
            let (draws, triangles) = pass.prepare(gpu, &view.camera, fog_end);
            stats.wheel_draws += draws;
            stats.wheel_triangles += triangles;
        }
        if let Some(blood) = &self.blood {
            stats.wheel_draws += blood.draws();
            stats.wheel_triangles += blood.triangles();
        }
        self.profiler.mark("prepare.effects");
        if let Some(vehicle) = &mut self.vehicle {
            vehicle.prepare(gpu, &view_projection, view.camera.eye());
            stats.vehicle_triangles = vehicle.triangles;
        }
        for (cpu, visible) in &mut self.cpus {
            if *visible {
                cpu.prepare(gpu, &view_projection, view.camera.eye());
                stats.vehicle_triangles += cpu.triangles;
            }
        }
        self.profiler.mark("prepare.vehicles");
        let contact = match (&mut self.contact_shadows, &self.vehicle, &self.ride_pose) {
            (Some(contact), Some(vehicle), Some((quad, torso))) => {
                let ground = |x: f32, z: f32| self.ground.height(f64::from(x), f64::from(z)) as f32;
                let rest = self.rest_lift.unwrap_or(1.5);
                let distance = |m: &[f32; 16]| {
                    ((m[12] - eye[0]).powi(2) + (m[13] - eye[1]).powi(2) + (m[14] - eye[2]).powi(2))
                        .sqrt()
                };
                let lift = |m: &[f32; 16], rest: f32| m[13] - ground(m[12], m[14]) - rest;
                let [length, width] = vehicle.footprint;
                let strength = if light.is_some() { 0.35 } else { 0.6 };
                let mut footprints = vec![shadows::footprint(
                    quad,
                    length,
                    width,
                    lift(quad, rest),
                    distance(quad),
                    strength,
                )];
                if let Some(torso) = torso {
                    footprints.push(shadows::footprint(
                        torso,
                        3.,
                        2.2,
                        lift(torso, 0.9),
                        distance(torso),
                        0.45,
                    ));
                }
                contact.prepare(gpu, &footprints, ground)
            }
            _ => 0,
        };
        stats.shadow_draws = u32::from(contact > 0) + u32::from(light.is_some());
        let rush_scene = match &mut self.speed_rush {
            Some(rush) if rush.active() => {
                let (origin, velocity) = self.rush_motion;
                rush.prepare(
                    gpu,
                    size,
                    speed_rush::focus(&view_projection, eye, velocity),
                    speed_rush::bounds(&view_projection, origin),
                );
                Some(rush.scene_target(gpu, size))
            }
            _ => None,
        };
        self.profiler.mark("prepare.shadows_rush");
        for (name, n) in [
            ("terrain", stats.terrain_draws),
            ("sky", stats.sky_tiles),
            ("scenery", stats.scenery_draws),
            ("vegetation", stats.vegetation_draws),
            ("water", stats.water_draws),
            ("train", stats.train_draws),
            ("ambient", stats.ambient_draws),
            ("snow", stats.snow_draws),
            ("wheel", stats.wheel_draws),
            ("shadow", stats.shadow_draws),
        ] {
            self.profiler.count(name, n);
        }
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        if let (Some(map), Some(vehicle), Some((clip, _))) =
            (&self.shadow_map, &self.vehicle, &light)
        {
            map.render(gpu, &mut encoder, &self.world, vehicle, clip);
        }
        self.profiler.mark("encode.shadow_map");
        {
            let [r, g, b] = self.fog_color.map(f64::from);
            // With speed rush the scene resolves into its own texture first.
            let scene_target = rush_scene.as_ref().unwrap_or(target);
            let (view, resolve_target) = match msaa {
                Some(msaa) => (msaa, Some(scene_target)),
                None => (scene_target, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                timestamp_writes: self.gpu_timer.as_ref().and_then(|t| t.writes()),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1. }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: if rush_scene.is_some() {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                }),
                ..Default::default()
            });
            self.profiler.mark("encode.begin");
            if let Some(sky) = &self.sky {
                sky.draw(&mut pass, &self.world);
            }
            self.profiler.mark("encode.sky");
            self.terrain.draw(&mut pass);
            self.profiler.mark("encode.terrain");
            self.scenery.draw(&mut pass, &self.world);
            self.profiler.mark("encode.scenery");
            if let Some((train, _)) = &self.train {
                train.draw(&mut pass, &self.world);
            }
            if let Some(contact) = &self.contact_shadows {
                contact.draw(&mut pass, &self.world);
            }
            if let Some(tracks) = &self.tire_tracks {
                tracks.draw(&mut pass, &self.world);
            }
            self.profiler.mark("encode.train_shadows_tracks");
            if let Some(vegetation) = &self.vegetation {
                vegetation.draw(&mut pass, &self.world);
            }
            self.profiler.mark("encode.vegetation");
            // The browser draws vehicles here, then water.
            if let Some(vehicle) = &self.vehicle {
                pass.set_stencil_reference(1);
                vehicle.draw(&mut pass);
            }
            for (cpu, _) in self.cpus.iter().filter(|(_, visible)| *visible) {
                cpu.draw(&mut pass);
            }
            self.profiler.mark("encode.vehicles");
            if let Some((water, _)) = &self.water {
                water.draw(&mut pass);
            }
            self.profiler.mark("encode.water");
            if let Some((ambient, _)) = &self.ambient {
                ambient.draw(&mut pass, &self.world);
            }
            if let Some((snow, _)) = &self.snow {
                snow.draw(&mut pass, &self.world);
            }
            if let Some(blood) = &self.blood {
                blood.draw(&mut pass, &self.world);
            }
            if let Some(effects) = &self.wheel_effects {
                effects.draw(&mut pass, &self.world);
            }
            if let Some(dust) = &self.lingering_dust {
                dust.draw(&mut pass, &self.world);
            }
            self.profiler.mark("encode.effects");
        }
        self.profiler.mark("encode.end_pass");
        if let (Some(rush), Some(_)) = (&self.speed_rush, &rush_scene) {
            rush.draw(&mut encoder, target, msaa, depth);
            stats.shadow_draws += 1;
        }
        (encoder, stats)
    }

    /// Renders one frame offscreen and returns tightly packed RGBA8 rows, for
    /// parity screenshots against the browser build.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(
        &mut self,
        gpu: &Gpu,
        view: &View,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, String> {
        self.capture_with(gpu, view, width, height, &mut |_, _| {})
    }

    /// Median scene-pass GPU time (timestamp queries) over `frames` offscreen
    /// renders of this view, or None without timestamp support. Profiling
    /// only: 30 untimed warm-up frames, then all frames submitted back to back
    /// and read once at the end.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn gpu_scene_ms(
        &mut self,
        gpu: &Gpu,
        view: &View,
        width: u32,
        height: u32,
        frames: u32,
    ) -> Option<f64> {
        if !gpu.timestamps || frames == 0 {
            return None;
        }
        let config = wgpu::SurfaceConfiguration {
            width,
            height,
            ..gpu.config.clone()
        };
        let (depth, msaa) = targets(&gpu.device, &config, gpu.samples);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu timing"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        // Untimed warm-up frames first (pipeline compilation, GPU clocks).
        self.gpu_timer = Some(profile::GpuTimer::new(gpu, frames));
        for frame in 0..frames + 30 {
            if let Some(timer) = &mut self.gpu_timer {
                timer.frame = frame.checked_sub(30);
            }
            let (encoder, _) =
                self.encode(gpu, view, (width, height), &target, msaa.as_ref(), &depth);
            gpu.queue.submit([encoder.finish()]);
        }
        let mut samples = self
            .gpu_timer
            .take()
            .map(|t| t.read_ms(gpu))
            .unwrap_or_default();
        samples.sort_by(f64::total_cmp);
        samples.get(samples.len() / 2).copied()
    }

    /// `capture` with an overlay pass drawn over the scene.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture_with(
        &mut self,
        gpu: &Gpu,
        view: &View,
        width: u32,
        height: u32,
        overlay: &mut dyn FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<Vec<u8>, String> {
        let config = wgpu::SurfaceConfiguration {
            width,
            height,
            ..gpu.config.clone()
        };
        let (depth, msaa) = targets(&gpu.device, &config, gpu.samples);
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        let (mut encoder, _) =
            self.encode(gpu, view, (width, height), &target, msaa.as_ref(), &depth);
        overlay(&mut encoder, &target);
        read_texture(gpu, encoder, &texture, width, height)
    }
}

/// Submits `encoder` with a copy of `texture` and returns its RGBA pixels.
fn read_texture(
    gpu: &Gpu,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let config = &gpu.config;
    let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture readback"),
        size: (row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    buffer.map_async(wgpu::MapMode::Read, .., |result| {
        result.expect("map capture")
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    let mapped = buffer.get_mapped_range(..).map_err(|e| format!("{e:?}"))?;
    let bgra = matches!(
        config.format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for pixel in mapped[(y * row) as usize..][..(width * 4) as usize]
            .as_chunks::<4>()
            .0
        {
            rgba.extend_from_slice(&if bgra {
                [pixel[2], pixel[1], pixel[0], 255]
            } else {
                [pixel[0], pixel[1], pixel[2], 255]
            });
        }
    }
    Ok(rgba)
}
