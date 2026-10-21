//! wgpu renderer shared by every build: Metal on macOS, DX12/Vulkan on
//! Windows, Vulkan on Linux and WebGL2 in browsers.
pub mod camera;
mod terrain;
mod vehicle;

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
    pub vehicle_triangles: u32,
}

pub struct SceneRenderer {
    terrain: terrain::TerrainPass,
    fog_color: [f32; 3],
    far: f32,
    /// The player's quad and rider, once a ride is loaded.
    vehicle: Option<vehicle::VehiclePass>,
}

impl SceneRenderer {
    pub fn new(gpu: &Gpu, venue: &Venue, fine_center: Option<[f32; 2]>) -> Self {
        let fog = venue.scene.fog.as_ref();
        let fog_color = venue.scene.fog_color.map(|c| c / 255.);
        let far = fog.map_or(
            venue.terrain.size_x().max(venue.terrain.size_z()) * 5.,
            |f| f.end,
        );
        Self {
            terrain: terrain::TerrainPass::new(gpu, venue, fine_center, gpu.samples),
            fog_color,
            far,
            vehicle: None,
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
        self.vehicle = Some(vehicle::VehiclePass::new(
            gpu,
            venue,
            quad,
            rider,
            gpu.samples,
        )?);
        Ok(())
    }

    /// The latest simulation pose of the quad linkage and rider skeleton.
    pub fn pose_ride(&self, gpu: &Gpu, pose: &atv_game::ride::Pose) {
        if let Some(vehicle) = &self.vehicle {
            vehicle.pose(gpu, pose);
        }
    }

    /// Rebuilds the fine terrain patch around a new rider position.
    pub fn recenter_terrain(&mut self, gpu: &Gpu, venue: &Venue, center: [f32; 2]) {
        self.terrain.recenter(gpu, venue, center);
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
        let (mut encoder, stats) = self.encode(
            gpu,
            view,
            gpu.aspect(),
            &target,
            gpu.msaa.as_ref(),
            &gpu.depth,
        );
        overlay(&mut encoder, &target);
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
        let view_projection =
            view.camera
                .view_projection(aspect, view.focal_length, 1., self.far + 1.);
        let mut stats = self
            .terrain
            .prepare(gpu, &view_projection, view.camera.eye());
        if let Some(vehicle) = &mut self.vehicle {
            vehicle.prepare(gpu, &view_projection, view.camera.eye());
            stats.vehicle_triangles = vehicle.triangles;
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
            self.terrain.draw(&mut pass);
            if let Some(vehicle) = &self.vehicle {
                vehicle.draw(&mut pass);
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
        self.capture_with(gpu, view, width, height, &mut |_, _| {})
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
        let (mut encoder, _) = self.encode(
            gpu,
            view,
            width as f32 / height as f32,
            &target,
            msaa.as_ref(),
            &depth,
        );
        overlay(&mut encoder, &target);
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
