//! Alpha Air dark blur (AirBlurRenderer in web/hype-gl.js): the frame renders
//! into a texture while the blur is on, then this pass draws it radially
//! blurred, desaturated and darkened towards the edges into the target.
use crate::Gpu;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniform {
    params: [f32; 4],
}

pub struct AirBlurPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    /// The frame the blur reads: (size, view, bind group).
    scene: Option<((u32, u32), wgpu::TextureView, wgpu::BindGroup)>,
    format: wgpu::TextureFormat,
    pub amount: f32,
    /// Phones and tablets take fewer taps (as speed rush does).
    pub coarse: bool,
}

impl AirBlurPass {
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("air blur"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("air_blur.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("air blur"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("air blur"),
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
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(gpu.config.format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("air blur scene"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("air blur"),
                size: std::mem::size_of::<BlurUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            scene: None,
            format: gpu.config.format,
            amount: 0.,
            coarse: false,
        }
    }

    /// AirBlurRenderer draws only above 0.002.
    pub fn active(&self) -> bool {
        self.amount > 0.002
    }

    /// The texture this frame renders into before the blur; writes the
    /// uniforms for `size`.
    pub fn scene_target(&mut self, gpu: &Gpu, size: (u32, u32)) -> wgpu::TextureView {
        if self.scene.as_ref().is_none_or(|s| s.0 != size) {
            let view = gpu
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("air blur scene"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
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
                .create_view(&Default::default());
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("air blur"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.scene = Some((size, view, group));
        }
        let uniform = BlurUniform {
            params: [
                size.0 as f32,
                size.1 as f32,
                self.amount.min(1.),
                if self.coarse { 6. } else { 10. },
            ],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
        self.scene.as_ref().unwrap().1.clone()
    }

    /// The blurred frame into `target`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some((_, _, group)) = &self.scene else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("air blur"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}
