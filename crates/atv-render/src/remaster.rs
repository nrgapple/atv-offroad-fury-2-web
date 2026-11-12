//! Remastered preset finish (authored): the frame renders into a texture,
//! its brightest parts blur down and back up a half-size chain, and the
//! composite adds that glow with a film grade, vignette and dither. The
//! other presets never create this pass.
use crate::Gpu;

/// Bloom chain levels below the frame: 1/2, 1/4, 1/8 and 1/16 size.
const LEVELS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    /// Threshold, knee, intensity, exposure.
    bloom: [f32; 4],
    /// Contrast, vignette, saturation, aspect.
    grade: [f32; 4],
}

struct Targets {
    size: (u32, u32),
    scene: wgpu::TextureView,
    levels: Vec<wgpu::TextureView>,
    /// Frame to the first level.
    prefilter: wgpu::BindGroup,
    /// Level `i` to level `i + 1`.
    down: Vec<wgpu::BindGroup>,
    /// Level `i + 1` onto level `i`.
    up: Vec<wgpu::BindGroup>,
    /// Frame and first level to the output.
    composite: wgpu::BindGroup,
}

pub struct RemasterPass {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    prefilter: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    targets: Option<Targets>,
    format: wgpu::TextureFormat,
}

impl RemasterPass {
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let texture = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("remaster"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, texture),
                entry(2, texture),
                entry(
                    3,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("remaster.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("remaster"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let format = gpu.config.format;
        let pipeline = |label, fragment, additive: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: additive.then_some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            prefilter: pipeline("remaster prefilter", "fs_prefilter", false),
            down: pipeline("remaster down", "fs_down", false),
            up: pipeline("remaster up", "fs_up", true),
            composite: pipeline("remaster composite", "fs_composite", false),
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("remaster"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("remaster"),
                size: std::mem::size_of::<Uniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            targets: None,
            format,
        }
    }

    fn targets(&self, gpu: &Gpu, size: (u32, u32)) -> Targets {
        let texture = |label, width: u32, height: u32| {
            gpu.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: width.max(1),
                        height: height.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let group = |source: &wgpu::TextureView, glow: &wgpu::TextureView| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("remaster"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(glow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let scene = texture("remaster scene", size.0, size.1);
        let levels: Vec<_> = (1..=LEVELS as u32)
            .map(|level| texture("remaster bloom", size.0 >> level, size.1 >> level))
            .collect();
        Targets {
            size,
            prefilter: group(&scene, &scene),
            down: levels.windows(2).map(|l| group(&l[0], &l[0])).collect(),
            up: levels.windows(2).map(|l| group(&l[1], &l[1])).collect(),
            composite: group(&scene, &levels[0]),
            scene,
            levels,
        }
    }

    /// The texture this frame renders into before the finish; writes the
    /// uniforms for `size`.
    pub fn scene_target(&mut self, gpu: &Gpu, size: (u32, u32)) -> wgpu::TextureView {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.targets(gpu, size));
        }
        let uniform = Uniform {
            bloom: [0.68, 0.2, 0.42, 1.06],
            grade: [0.06, 0.1, 1.12, size.0 as f32 / size.1.max(1) as f32],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
        self.targets.as_ref().unwrap().scene.clone()
    }

    /// The finished frame into `target`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some(targets) = &self.targets else {
            return;
        };
        let mut step = |label, view: &wgpu::TextureView, pipeline, group, load| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        step(
            "remaster prefilter",
            &targets.levels[0],
            &self.prefilter,
            &targets.prefilter,
            clear,
        );
        for level in 1..LEVELS {
            step(
                "remaster down",
                &targets.levels[level],
                &self.down,
                &targets.down[level - 1],
                clear,
            );
        }
        for level in (0..LEVELS - 1).rev() {
            step(
                "remaster up",
                &targets.levels[level],
                &self.up,
                &targets.up[level],
                wgpu::LoadOp::Load,
            );
        }
        step(
            "remaster composite",
            target,
            &self.composite,
            &targets.composite,
            clear,
        );
    }
}
