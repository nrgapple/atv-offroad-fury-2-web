//! wgpu renderer shared by every build: Metal on macOS, DX12/Vulkan on
//! Windows, Vulkan on Linux and WebGL2 in browsers.
pub mod camera;
mod cull;
mod scenery;
mod sky;
mod terrain;
mod texture;
mod vegetation;
mod world;

use atv_game::venue::Venue;
pub use camera::Camera;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub adapter_info: wgpu::AdapterInfo,
    samples: u32,
    depth: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
}

impl Gpu {
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
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("fury"),
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
}

impl FrameStats {
    pub fn draws(&self) -> u32 {
        self.terrain_draws + self.sky_tiles + self.scenery_draws + self.vegetation_draws
    }
    pub fn triangles(&self) -> u32 {
        self.terrain_triangles
            + self.sky_tiles * 2
            + self.scenery_triangles
            + self.vegetation_triangles
    }
}

/// Presentation options of the Display -> Graphics presets (web/graphics.js).
/// Original keeps both off; Enhanced/High smooth the sky and filter
/// scenery textures anisotropically (4x/16x).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Graphics {
    pub sky_smooth: bool,
    pub anisotropy: u16,
}

pub struct SceneRenderer {
    terrain: terrain::TerrainPass,
    world: world::World,
    sky: Option<sky::SkyPass>,
    scenery: scenery::SceneryPass,
    vegetation: Option<vegetation::VegetationPass>,
    fog_color: [f32; 3],
    far: f32,
    /// Depth range: fog end plus the largest object, so an admitted stadium
    /// stand is not sliced open into the sky (matrix() in web/game.js).
    depth_far: f32,
    clock: f32,
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
        let terrain = terrain::TerrainPass::new(gpu, venue, fine_center, gpu.samples);
        let mut world = world::World::new(gpu, venue);
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
        let depth_far = far + scenery.reach.max(terrain.reach()) + 1.;
        Self {
            terrain,
            world,
            sky,
            scenery,
            vegetation,
            fog_color,
            far,
            depth_far,
            clock: 0.,
        }
    }

    /// Advances world animation (scenery UV scroll and ATM clocks) by
    /// `seconds`, in whole 60 Hz simulation steps like the browser.
    pub fn advance(&mut self, seconds: f32) {
        self.clock = (self.clock + seconds.clamp(0., 0.1)).min(4. / 60.);
        while self.clock >= 1. / 60. {
            self.clock -= 1. / 60.;
            self.scenery.step();
        }
    }

    /// Advances world animation by exact simulation steps (parity captures).
    pub fn advance_steps(&mut self, steps: u64) {
        for _ in 0..steps {
            self.scenery.step();
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

    /// Lit stadium camera-flash pool indices (0020f2c0).
    pub fn set_lit_flashes(&mut self, flashes: &[usize]) {
        self.scenery.lit_flashes.clear();
        self.scenery.lit_flashes.extend_from_slice(flashes);
    }

    pub fn render(&mut self, gpu: &Gpu, view: &View) -> Result<FrameStats, String> {
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
        let target = frame.texture.create_view(&Default::default());
        let (encoder, stats) = self.encode(
            gpu,
            view,
            gpu.aspect(),
            &target,
            gpu.msaa.as_ref(),
            &gpu.depth,
        );
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(frame);
        Ok(stats)
    }

    fn encode(
        &mut self,
        gpu: &Gpu,
        view: &View,
        aspect: f32,
        target: &wgpu::TextureView,
        msaa: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
    ) -> (wgpu::CommandEncoder, FrameStats) {
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
        let mut stats = self.terrain.prepare(gpu, &view_projection, &cull, eye);
        self.world
            .prepare(gpu, &view_projection, eye, view.camera.yaw);
        stats.sky_tiles = self.sky.as_ref().map_or(0, |s| s.tiles());
        let scenery = self.scenery.prepare(gpu, &cull);
        stats.scenery_draws = scenery.draws;
        stats.scenery_triangles = scenery.triangles;
        stats.scenery_culled = scenery.culled;
        if let Some(vegetation) = &mut self.vegetation {
            let v = vegetation.prepare(gpu, eye, view.camera.yaw);
            stats.vegetation_draws = v.draws;
            stats.vegetation_triangles = v.triangles;
            stats.vegetation_visible = v.visible;
        }
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let [r, g, b] = self.fog_color.map(f64::from);
            let (view, resolve_target) = match msaa {
                Some(msaa) => (msaa, Some(target)),
                None => (target, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
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
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if let Some(sky) = &self.sky {
                sky.draw(&mut pass, &self.world);
            }
            self.terrain.draw(&mut pass);
            self.scenery.draw(&mut pass, &self.world);
            if let Some(vegetation) = &self.vegetation {
                vegetation.draw(&mut pass, &self.world);
            }
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
        let (mut encoder, _) = self.encode(
            gpu,
            view,
            width as f32 / height as f32,
            &target,
            msaa.as_ref(),
            &depth,
        );
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
}
