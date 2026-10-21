//! Ring-emitter Snow (SnowRenderer in web/weather.js): Rust births enter a
//! bounded GPU ring (10,800 slots) and age in WGSL; one instanced draw.
use crate::{
    Camera, Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::snow::Weather;
use wgpu::util::DeviceExt;

pub const CAPACITY: usize = 10_800;
const FLOATS: usize = 11;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SnowUniform {
    right: [f32; 4],
    up: [f32; 4],
    color: [f32; 4],
}

pub struct SnowPass {
    pipeline: wgpu::RenderPipeline,
    corners: wgpu::Buffer,
    instances: wgpu::Buffer,
    data: Vec<f32>,
    dirty: Vec<usize>,
    uniforms: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
    sequence: u64,
    count: u32,
    time: f32,
    color: [f32; 3],
    alpha: [f32; 2],
}

impl SnowPass {
    pub fn new(gpu: &Gpu, world: &World, weather: &Weather) -> Self {
        let device = &gpu.device;
        let image = &weather.texture;
        let atlas = texture::upload_flat(gpu, image.width, image.height, &image.rgba, "snow atlas");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("snow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snow uniforms"),
            size: std::mem::size_of::<SnowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("snow.wgsl"));
        let pipeline = world.pipeline(
            gpu,
            PipelineDesc {
                label: "snow",
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
                        attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x3],
                    }),
                ],
                blend: Blend::Alpha,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Less,
                per_draw: true,
            },
        );
        let config = weather.snow.config();
        Self {
            pipeline,
            corners: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("snow corners"),
                contents: bytemuck::cast_slice(&[
                    -1f32, -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.,
                ]),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("snow ring"),
                size: (CAPACITY * FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            data: vec![0.; CAPACITY * FLOATS],
            dirty: Vec::new(),
            draw_group: world.draw_group(
                gpu,
                &uniforms,
                std::mem::size_of::<SnowUniform>(),
                &uniforms,
                16,
            ),
            uniforms,
            texture_group: world.texture_group(gpu, &atlas, &sampler),
            sequence: 0,
            count: 0,
            time: 0.,
            color: config.color.map(|c| c / 128.),
            alpha: [config.min_alpha, config.max_alpha],
        }
    }

    /// One 60 Hz step around the camera: new births enter the ring.
    pub fn step(&mut self, weather: &mut Weather, camera: &Camera) -> Result<(), String> {
        let axes = camera.axes();
        let frame = weather.snow.step(camera.eye(), axes.forward)?;
        self.time = frame.time;
        for birth in frame.births {
            if birth.id <= self.sequence || birth.life <= 0. {
                continue;
            }
            self.sequence = birth.id;
            let slot = ((birth.id - 1) % CAPACITY as u64) as usize;
            let a = &mut self.data[slot * FLOATS..][..FLOATS];
            a[..3].copy_from_slice(&birth.position);
            a[3] = birth.size;
            a[4..7].copy_from_slice(&birth.velocity);
            a[7] = birth.time;
            a[8] = birth.life;
            a[9] = birth.mirror as f32;
            a[10] = birth.half as f32;
            self.dirty.push(slot);
            self.count = self.count.max(slot as u32 + 1);
        }
        Ok(())
    }

    pub fn prepare(&mut self, gpu: &Gpu, camera: &Camera) -> (u32, u32) {
        self.dirty.sort_unstable();
        self.dirty.dedup();
        let mut i = 0;
        while i < self.dirty.len() {
            let first = self.dirty[i];
            let mut last = first;
            while i + 1 < self.dirty.len() && self.dirty[i + 1] == last + 1 {
                i += 1;
                last = self.dirty[i];
            }
            i += 1;
            gpu.queue.write_buffer(
                &self.instances,
                (first * FLOATS * 4) as u64,
                bytemuck::cast_slice(&self.data[first * FLOATS..(last + 1) * FLOATS]),
            );
        }
        self.dirty.clear();
        let (sy, cy) = camera.yaw.sin_cos();
        let (sp, cp) = camera.pitch.sin_cos();
        let uniform = SnowUniform {
            right: [cy, 0., -sy, self.time],
            up: [-sy * sp, cp, -cy * sp, self.alpha[0]],
            color: [self.color[0], self.color[1], self.color[2], self.alpha[1]],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
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
        pass.set_bind_group(1, &self.texture_group, &[]);
        pass.set_bind_group(2, &self.draw_group, &[0, 0]);
        pass.set_vertex_buffer(0, self.corners.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}
