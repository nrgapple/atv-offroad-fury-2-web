//! Shared state of the static-world passes (sky, scenery, vegetation): the
//! per-frame scene uniforms of web/game.js's über-shader and the bind group
//! layouts every world pipeline uses.
use crate::Gpu;
use atv_game::venue::Venue;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_projection: [f32; 16],
    pub eye: [f32; 4],
    pub fog_color: [f32; 4],
    /// Encoded bias, distance coefficient, enabled.
    pub fog: [f32; 4],
    pub ambient: [f32; 4],
    pub sun_color: [f32; 4],
    pub sun_direction: [f32; 4],
    pub eco_ambient: [f32; 4],
    pub eco_sunlight: [f32; 4],
    /// Direction, then sun present.
    pub eco_direction: [f32; 4],
    /// Camera-facing billboard rotation (forward Z, -forward X).
    pub eco_rotation: [f32; 4],
    /// Sky cube origin, then bicubic (Enhanced smooth sky).
    pub sky: [f32; 4],
    /// High preset vehicle shadow map: world to [0,1] shadow texture space.
    pub shadow_matrix: [f32; 16],
    /// Receive (1), shadow texel size.
    pub shadow: [f32; 4],
}

pub struct World {
    /// Vehicle-only sun shadow map (High; 1x1 and never sampled otherwise).
    pub shadow_view: wgpu::TextureView,
    pub shadow_size: u32,
    pub shadow_sampler: wgpu::Sampler,
    pub globals: Globals,
    pub buffer: wgpu::Buffer,
    pub globals_layout: wgpu::BindGroupLayout,
    pub globals_group: wgpu::BindGroup,
    pub texture_layout: wgpu::BindGroupLayout,
    pub draw_layout: wgpu::BindGroupLayout,
    pub uniform_alignment: u64,
    pub samples: u32,
    pub format: wgpu::TextureFormat,
}

pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// The uniform entry plus a depth texture and comparison sampler at
/// `first` and `first + 1` (vehicle shadow receiving).
pub fn shadow_entries(
    uniform: wgpu::BindGroupLayoutEntry,
    first: u32,
) -> [wgpu::BindGroupLayoutEntry; 3] {
    [
        uniform,
        entry(
            first,
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        ),
        entry(
            first + 1,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
    ]
}

/// LINEAR, CLAMP_TO_EDGE, COMPARE_REF_TO_TEXTURE with LEQUAL (shadow-map.js).
pub fn shadow_sampler(gpu: &Gpu) -> wgpu::Sampler {
    gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vehicle shadow"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    })
}

pub(crate) fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty,
        count: None,
    }
}

fn uniform(dynamic: bool) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: dynamic,
        min_binding_size: None,
    }
}

impl World {
    /// `shadow_size` is the High shadow map size (0 without one).
    pub fn new(gpu: &Gpu, venue: &Venue, shadow_size: u32) -> Self {
        let device = &gpu.device;
        let scene = &venue.scene;
        let fog = scene.fog.as_ref();
        let ambient = scene
            .lights
            .iter()
            .find(|l| l.kind == "Ambient")
            .map_or([0.38; 3], |l| l.color.map(|c| c / 255.));
        let sun = scene.lights.iter().find(|l| l.kind == "Directional");
        let sun_color = sun.map_or([0.8; 3], |l| l.color.map(|c| c / 255.));
        let sun_direction = match sun {
            Some(sun) => match (sun.look, sun.position, sun.target) {
                (Some(look), _, _) => look.map(|v| -v),
                (None, Some(p), Some(t)) => [p[0] - t[0], p[1] - t[1], p[2] - t[2]],
                _ => [-0.6, 1., 0.35],
            },
            None => [-0.6, 1., 0.35],
        };
        let [r, g, b] = scene.fog_color.map(|c| c / 255.);
        let eco = &scene.ecosystem_lighting;
        let sky = venue.sky.as_ref().map_or([0.; 3], |s| s.origin);
        let v4 = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
        let globals = Globals {
            view_projection: [0.; 16],
            eye: [0.; 4],
            fog_color: [r, g, b, 1.],
            fog: [
                fog.map_or(524288., |f| f.encoded_bias),
                fog.map_or(0., |f| f.coefficient),
                fog.is_some() as u8 as f32,
                0.,
            ],
            ambient: v4(ambient, 0.),
            sun_color: v4(sun_color, 0.),
            sun_direction: v4(sun_direction, 0.),
            eco_ambient: v4(eco.ambient, 0.),
            eco_sunlight: v4(eco.sunlight, 0.),
            eco_direction: v4(eco.direction, eco.sun_present as u8 as f32),
            eco_rotation: [1., 0., 0., 0.],
            sky: v4(sky, 0.),
            shadow_matrix: [0.; 16],
            shadow: [0.; 4],
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world globals"),
            entries: &shadow_entries(entry(0, uniform(false)), 1),
        });
        let shadow_size = shadow_size.max(1);
        let shadow_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("vehicle shadow map"),
                size: wgpu::Extent3d {
                    width: shadow_size,
                    height: shadow_size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let shadow_sampler = shadow_sampler(gpu);
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world globals"),
            layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world texture"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world draw"),
            entries: &[entry(0, uniform(true)), entry(1, uniform(true))],
        });
        Self {
            shadow_view,
            shadow_size,
            shadow_sampler,
            globals,
            buffer,
            globals_layout,
            globals_group,
            texture_layout,
            draw_layout,
            uniform_alignment: u64::from(device.limits().min_uniform_buffer_offset_alignment),
            samples: gpu.samples,
            format: gpu.config.format,
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, view_projection: &[f32; 16], eye: [f32; 3], yaw: f32) {
        self.globals.view_projection = *view_projection;
        self.globals.eye = [eye[0], eye[1], eye[2], 1.];
        let (forward_x, forward_z) = (yaw.sin(), yaw.cos());
        self.globals.eco_rotation = [forward_z, -forward_x, 0., 0.];
        gpu.queue
            .write_buffer(&self.buffer, 0, bytemuck::bytes_of(&self.globals));
    }

    pub fn texture_group(
        &self,
        gpu: &Gpu,
        texture: &wgpu::Texture,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        let view = texture.create_view(&Default::default());
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world texture"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// A preset change resizes the High vehicle shadow map (0 leaves the
    /// 1x1 placeholder); the globals group rebinds to it.
    pub fn set_shadow_size(&mut self, gpu: &Gpu, size: u32) {
        let size = size.max(1);
        if size == self.shadow_size {
            return;
        }
        self.shadow_view = gpu
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("vehicle shadow map"),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        self.shadow_size = size;
        self.globals_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world globals"),
            layout: &self.globals_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
            ],
        });
    }

    /// Slot size for dynamic-offset uniform arrays.
    pub fn slot(&self, size: usize) -> u64 {
        (size as u64).div_ceil(self.uniform_alignment) * self.uniform_alignment
    }

    /// Dynamic-offset slots of `size` bytes, as one bind group over two
    /// buffers (per material and per draw).
    pub fn draw_group(
        &self,
        gpu: &Gpu,
        materials: &wgpu::Buffer,
        material_size: usize,
        draws: &wgpu::Buffer,
        draw_size: usize,
    ) -> wgpu::BindGroup {
        fn binding(buffer: &wgpu::Buffer, size: usize, binding: u32) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size as u64),
                }),
            }
        }
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world draw"),
            layout: &self.draw_layout,
            entries: &[
                binding(materials, material_size, 0),
                binding(draws, draw_size, 1),
            ],
        })
    }

    pub fn pipeline(&self, gpu: &Gpu, desc: PipelineDesc<'_>) -> wgpu::RenderPipeline {
        self.pipeline_with(gpu, desc, &self.texture_layout)
    }

    /// [`World::pipeline`] with another group-1 layout (e.g. a texture array).
    pub fn pipeline_with(
        &self,
        gpu: &Gpu,
        desc: PipelineDesc<'_>,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        self.pipeline_layouts(gpu, desc, texture_layout, &self.draw_layout)
    }

    /// One dynamic uniform binding per draw (group 2 of [`World::record_layout`]).
    pub fn record_layout(&self, gpu: &Gpu) -> wgpu::BindGroupLayout {
        gpu.device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("world record"),
                entries: &[entry(0, uniform(true))],
            })
    }

    /// [`World::pipeline`] with other group-1 and per-draw group-2 layouts.
    pub fn pipeline_layouts(
        &self,
        gpu: &Gpu,
        desc: PipelineDesc<'_>,
        texture_layout: &wgpu::BindGroupLayout,
        draw_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(desc.label),
                bind_group_layouts: &[
                    Some(&self.globals_layout),
                    Some(texture_layout),
                    desc.per_draw.then_some(draw_layout),
                ][..if desc.per_draw { 3 } else { 2 }],
                immediate_size: 0,
            });
        let blend = match desc.blend {
            Blend::Opaque => None,
            Blend::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
            Blend::Additive => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
        };
        gpu.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(desc.label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: desc.shader,
                    entry_point: Some(desc.vertex),
                    compilation_options: Default::default(),
                    buffers: desc.buffers,
                },
                // The browser disables culling for the whole scene.
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DEPTH_FORMAT,
                    depth_write_enabled: Some(desc.depth_write),
                    depth_compare: Some(desc.depth_compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: self.samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: desc.shader,
                    entry_point: Some(desc.fragment),
                    compilation_options: Default::default(),
                    // The canvas is opaque (alpha: false); keep destination alpha.
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.format,
                        blend,
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Blend {
    Opaque,
    /// SRC_ALPHA, ONE_MINUS_SRC_ALPHA.
    Alpha,
    /// SRC_ALPHA, ONE.
    Additive,
}

pub struct PipelineDesc<'a> {
    pub label: &'a str,
    pub shader: &'a wgpu::ShaderModule,
    pub vertex: &'a str,
    pub fragment: &'a str,
    pub buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    pub blend: Blend,
    pub depth_write: bool,
    pub depth_compare: wgpu::CompareFunction,
    /// Binds the per-material/per-draw uniform group (2).
    pub per_draw: bool,
}
