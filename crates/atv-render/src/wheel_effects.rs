//! Wheel effects (WheelEffectsRenderer in web/wheel-effects.js): this step's
//! dust/chunk/spray sprites uploaded once per new frame, and wet births in a
//! 10,800-slot GPU ring that ages them. Two instanced draws.
use crate::{
    Camera, Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::wheel_effects::WheelEffects;
use wgpu::util::DeviceExt;

const FLOATS: usize = 14;
const SPRITES: usize = 600;
pub const WET_CAPACITY: usize = 10_800;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EffectUniform {
    right: [f32; 4],
    up: [f32; 4],
    light: [f32; 4],
}

pub struct WheelEffectsPass {
    pipeline: wgpu::RenderPipeline,
    corners: wgpu::Buffer,
    sprites: wgpu::Buffer,
    wet: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    slot: u64,
    draw_group: wgpu::BindGroup,
    dry_group: wgpu::BindGroup,
    wet_group: wgpu::BindGroup,
    scratch: Vec<f32>,
    wet_data: Vec<f32>,
    wet_dirty: Vec<usize>,
    wet_id: u64,
    wet_count: u32,
    /// Births still alive (wetBirthHistory): (birth time, life).
    wet_live: Vec<(f32, f32)>,
    sprite_count: u32,
    /// (time, placement epoch) of the uploaded sprite frame.
    uploaded: Option<(f32, u64)>,
    /// Sprites packed since the last upload.
    pending: bool,
    time: f32,
    light: [f32; 3],
    size: [f32; 2],
}

impl WheelEffectsPass {
    pub fn new(gpu: &Gpu, world: &World, effects: &WheelEffects) -> Self {
        let device = &gpu.device;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("wheel effects"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let upload = |image: &atv_game::venue::Image, label| {
            let t = texture::upload_flat(gpu, image.width, image.height, &image.rgba, label);
            world.texture_group(gpu, &t, &sampler)
        };
        let slot = world.slot(std::mem::size_of::<EffectUniform>());
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wheel effect uniforms"),
            size: slot * 2,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("wheel_effects.wgsl"));
        let pipeline = world.pipeline(
            gpu,
            PipelineDesc {
                label: "wheel effects",
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
                        attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32, 4 => Float32x4, 5 => Float32],
                    }),
                ],
                blend: Blend::Alpha,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Less,
                per_draw: true,
            },
        );
        let buffer = |count: usize, label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (count * FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            pipeline,
            corners: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wheel effect corners"),
                contents: bytemuck::cast_slice(&[
                    -1f32, -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.,
                ]),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            sprites: buffer(SPRITES, "wheel sprites"),
            wet: buffer(WET_CAPACITY, "wet ring"),
            draw_group: world.draw_group(
                gpu,
                &uniforms,
                std::mem::size_of::<EffectUniform>(),
                &uniforms,
                16,
            ),
            uniforms,
            slot,
            dry_group: upload(&effects.atlas, "wheel atlas"),
            wet_group: upload(&effects.wet_atlas, "wet atlas"),
            scratch: Vec::with_capacity(SPRITES * FLOATS),
            wet_data: vec![0.; WET_CAPACITY * FLOATS],
            wet_dirty: Vec::new(),
            wet_id: 0,
            wet_count: 0,
            wet_live: Vec::new(),
            sprite_count: 0,
            uploaded: None,
            pending: false,
            time: 0.,
            light: effects.light_color.map(|v| v / 128.),
            size: [effects.atlas.width as f32, effects.atlas.height as f32],
        }
    }

    /// One simulation step's frame (WheelEffectsRenderer.accept).
    pub fn accept(&mut self, effects: &mut WheelEffects) {
        for birth in effects.take_wet_births() {
            if birth.id <= self.wet_id || birth.tile > 15 || birth.life <= 0. {
                continue;
            }
            self.wet_id = birth.id;
            let slot = ((birth.id - 1) % WET_CAPACITY as u64) as usize;
            let (column, row) = ((birth.tile / 4) as f32, (birth.tile % 4) as f32);
            let a = &mut self.wet_data[slot * FLOATS..][..FLOATS];
            a[..3].copy_from_slice(&birth.position);
            a[3] = birth.size;
            a[4..8].copy_from_slice(&[column / 4., row / 8., (column + 1.) / 4., (row + 1.) / 8.]);
            a[8] = birth.floor;
            a[9..12].copy_from_slice(&birth.velocity);
            a[12] = birth.birth_time;
            a[13] = birth.life;
            self.wet_dirty.push(slot);
            self.wet_count = self.wet_count.max(slot as u32 + 1);
            self.wet_live.push((birth.birth_time, birth.life));
        }
        let frame = effects.frame();
        self.time = frame.time;
        let time = frame.time;
        self.wet_live.retain(|(birth, life)| time - birth <= *life);
        let key = (frame.time, frame.placement_epoch);
        if self.uploaded != Some(key) {
            self.uploaded = Some(key);
            let frames = &effects.frames;
            let [w, h] = self.size;
            self.scratch.clear();
            for p in frame.particles.iter().take(SPRITES) {
                let Some(rect) = frames.get(p.frame).map(|f| f.rect) else {
                    continue;
                };
                self.scratch.extend([
                    p.position[0],
                    p.position[1],
                    p.position[2],
                    p.size,
                    (rect[0] + 0.5) / w,
                    (rect[1] + 0.5) / h,
                    (rect[2] + 0.5) / w,
                    (rect[3] + 0.5) / h,
                    p.alpha,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                ]);
            }
            self.sprite_count = (self.scratch.len() / FLOATS) as u32;
            self.pending = true;
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, camera: &Camera) -> (u32, u32) {
        if std::mem::take(&mut self.pending) && !self.scratch.is_empty() {
            gpu.queue
                .write_buffer(&self.sprites, 0, bytemuck::cast_slice(&self.scratch));
        }
        self.wet_dirty.sort_unstable();
        self.wet_dirty.dedup();
        for &slot in &self.wet_dirty {
            gpu.queue.write_buffer(
                &self.wet,
                (slot * FLOATS * 4) as u64,
                bytemuck::cast_slice(&self.wet_data[slot * FLOATS..(slot + 1) * FLOATS]),
            );
        }
        self.wet_dirty.clear();
        let (sy, cy) = camera.yaw.sin_cos();
        let (sp, cp) = camera.pitch.sin_cos();
        let up = [-sy * sp, cp, -cy * sp];
        for (index, (wet, light)) in [(0., [1.; 3]), (1., self.light)].into_iter().enumerate() {
            let uniform = EffectUniform {
                right: [cy, 0., -sy, self.time],
                up: [up[0], up[1], up[2], wet],
                light: [light[0], light[1], light[2], 0.],
            };
            gpu.queue.write_buffer(
                &self.uniforms,
                index as u64 * self.slot,
                bytemuck::bytes_of(&uniform),
            );
        }
        let wet = if self.wet_live.is_empty() {
            0
        } else {
            self.wet_count
        };
        let draws = u32::from(self.sprite_count > 0) + u32::from(wet > 0);
        (draws, (self.sprite_count + wet) * 2)
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        let wet = if self.wet_live.is_empty() {
            0
        } else {
            self.wet_count
        };
        if self.sprite_count == 0 && wet == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_vertex_buffer(0, self.corners.slice(..));
        if self.sprite_count > 0 {
            pass.set_bind_group(1, &self.dry_group, &[]);
            pass.set_bind_group(2, &self.draw_group, &[0, 0]);
            pass.set_vertex_buffer(1, self.sprites.slice(..));
            pass.draw(0..6, 0..self.sprite_count);
        }
        if wet > 0 {
            pass.set_bind_group(1, &self.wet_group, &[]);
            pass.set_bind_group(2, &self.draw_group, &[self.slot as u32, 0]);
            pass.set_vertex_buffer(1, self.wet.slice(..));
            pass.draw(0..6, 0..wet);
        }
    }
}
