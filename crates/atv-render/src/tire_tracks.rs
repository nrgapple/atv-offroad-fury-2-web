//! Enhanced/High tire tracks (TireTrackRenderer): the retained segment ring
//! uploaded by dirty range and drawn in one modulate-2x pass.
use crate::{Gpu, world::World};
use atv_game::tire_tracks::{FLOATS, TrackBuilder, VERTICES};

pub struct TireTracksPass {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    clock: wgpu::Buffer,
    clock_group: wgpu::BindGroup,
    capacity: usize,
    count: u32,
    now: f32,
    life: f32,
}

impl TireTracksPass {
    pub fn new(gpu: &Gpu, world: &World, capacity: usize) -> Self {
        let device = &gpu.device;
        let clock = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tire track clock"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let clock_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tire track clock"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let clock_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tire track clock"),
            layout: &clock_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: clock.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("tire_tracks.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tire tracks"),
            bind_group_layouts: &[Some(&world.globals_layout), Some(&clock_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tire tracks"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // depthFunc(LEQUAL), no writes, polygonOffset(-1, -2).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,
                    slope_scale: -1.,
                    clamp: 0.,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: world.samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                // blendFunc(DST_COLOR, SRC_COLOR): modulate 2x.
                targets: &[Some(wgpu::ColorTargetState {
                    format: world.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Dst,
                            dst_factor: wgpu::BlendFactor::Src,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tire track ring"),
                size: (capacity.max(1) * VERTICES * FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            clock,
            clock_group,
            capacity,
            count: 0,
            now: 0.,
            life: 0.,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Uploads the segments written since the last frame.
    pub fn accept(&mut self, gpu: &Gpu, tracks: &mut TrackBuilder, now: f32) {
        if let Some((from, to)) = tracks.dirty.take() {
            let per = VERTICES * FLOATS;
            gpu.queue.write_buffer(
                &self.vertices,
                (from * per * 4) as u64,
                bytemuck::cast_slice(&tracks.data[from * per..to * per]),
            );
        }
        self.count = tracks.count as u32;
        self.now = now;
        self.life = tracks.life;
    }

    pub fn prepare(&mut self, gpu: &Gpu) -> (u32, u32) {
        gpu.queue.write_buffer(
            &self.clock,
            0,
            bytemuck::cast_slice(&[self.now, self.life, 0., 0.]),
        );
        if self.count == 0 {
            (0, 0)
        } else {
            (1, self.count * 2)
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_bind_group(1, &self.clock_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..self.count * VERTICES as u32, 0..1);
    }
}
