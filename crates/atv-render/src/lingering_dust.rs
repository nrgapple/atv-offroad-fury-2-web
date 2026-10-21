//! Enhanced/High lingering dust (LingeringDustRenderer): one instanced draw of
//! the capped puff pool, uploaded only when a puff was emitted.
use crate::{
    Camera, Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::lingering_dust::{DustEmitter, FLOATS};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DustUniform {
    right: [f32; 4],
    up: [f32; 4],
    light: [f32; 4],
}

pub struct LingeringDustPass {
    pipeline: wgpu::RenderPipeline,
    corners: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    uniforms: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
    uploaded: Option<u64>,
    live: u32,
    now: f32,
}

impl LingeringDustPass {
    pub fn new(gpu: &Gpu, world: &World, capacity: usize) -> Self {
        let device = &gpu.device;
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dust uniforms"),
            size: std::mem::size_of::<DustUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let white = texture::upload_flat(gpu, 1, 1, &[255; 4], "dust");
        let sampler = device.create_sampler(&Default::default());
        let shader = device.create_shader_module(wgpu::include_wgsl!("lingering_dust.wgsl"));
        let pipeline = world.pipeline(
            gpu,
            PipelineDesc {
                label: "lingering dust",
                shader: &shader,
                vertex: "vs",
                fragment: "fs",
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: (FLOATS * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x2, 3 => Float32x3],
                    }),
                ],
                blend: Blend::Alpha,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Less,
                per_draw: true,
            },
        );
        Self {
            pipeline,
            corners: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dust corners"),
                contents: bytemuck::cast_slice(&[
                    -1f32, -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.,
                ]),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dust puffs"),
                size: (capacity.max(1) * FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            capacity,
            draw_group: world.draw_group(
                gpu,
                &uniforms,
                std::mem::size_of::<DustUniform>(),
                &uniforms,
                16,
            ),
            uniforms,
            texture_group: world.texture_group(gpu, &white, &sampler),
            uploaded: None,
            live: 0,
            now: 0.,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The pool after this frame's steps; `now` is the wheel-effects clock.
    pub fn accept(&mut self, gpu: &Gpu, emitter: &DustEmitter, now: f32) {
        if self.uploaded != Some(emitter.emitted) {
            self.uploaded = Some(emitter.emitted);
            gpu.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&emitter.data));
        }
        self.live = emitter.live as u32;
        self.now = now;
    }

    /// `fog_end` is the fog end, or 768 without fog.
    pub fn prepare(&mut self, gpu: &Gpu, camera: &Camera, fog_end: f32) -> (u32, u32) {
        let now = self.now;
        let (sy, cy) = camera.yaw.sin_cos();
        let (sp, cp) = camera.pitch.sin_cos();
        let uniform = DustUniform {
            right: [cy, 0., -sy, now],
            up: [-sy * sp, cp, -cy * sp, fog_end],
            light: [1.1, 1.1, 1.1, 0.],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
        if self.live == 0 {
            (0, 0)
        } else {
            (1, self.live * 2)
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        if self.live == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_bind_group(1, &self.texture_group, &[]);
        pass.set_bind_group(2, &self.draw_group, &[0, 0]);
        pass.set_vertex_buffer(0, self.corners.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.draw(0..6, 0..self.live);
    }
}
