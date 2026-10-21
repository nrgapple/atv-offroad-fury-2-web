//! ENV08 train (TrainRenderer in web/train.js): every car in route order,
//! then the locomotive wheels posed by the Rust wheel matrices.
use crate::{
    Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::train::{Train, WHEELS, mesh_name};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    model: [f32; 16],
    flags: [f32; 4],
    bones: [[f32; 16]; 32],
}

struct Chunk {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    texture: usize,
    material: u32,
}

struct Model {
    chunks: Vec<Chunk>,
    bind: Vec<[f32; 16]>,
}

pub struct TrainPass {
    pipeline: wgpu::RenderPipeline,
    models: Vec<Model>,
    /// Model per car in frame order; wheels last.
    cars: Vec<Option<usize>>,
    wheels: Option<usize>,
    groups: Vec<wgpu::BindGroup>,
    draws: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    material_slot: u64,
    draw_slot: u64,
    /// Draw slots used this frame.
    used: Vec<(usize, u32)>,
}

const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

impl TrainPass {
    pub fn new(gpu: &Gpu, world: &World, train: &Train) -> Result<Self, String> {
        let device = &gpu.device;
        let sampler = texture::sampler(gpu, true, 1);
        let fallback = texture::upload(gpu, 1, 1, &[255; 4], "fallback");
        let mut groups = vec![world.texture_group(gpu, &fallback, &sampler)];
        let mut uploaded: HashMap<*const atv_game::materials::MaterialImage, usize> =
            HashMap::new();
        let material_slot = world.slot(std::mem::size_of::<MaterialUniform>());
        let mut material_bytes = Vec::new();
        let mut models = Vec::with_capacity(train.models.len());
        for source in &train.models {
            let mut chunks = Vec::new();
            for chunk in &source.chunks {
                let pass = &chunk.material.passes[0];
                let texture = match &pass.texture {
                    None => 0,
                    Some(image) => {
                        *uploaded
                            .entry(std::rc::Rc::as_ptr(image))
                            .or_insert_with(|| {
                                let t = texture::upload(
                                    gpu,
                                    image.image.width,
                                    image.image.height,
                                    &image.image.rgba,
                                    "train",
                                );
                                groups.push(world.texture_group(gpu, &t, &sampler));
                                groups.len() - 1
                            })
                    }
                };
                let at = material_bytes.len();
                material_bytes.extend_from_slice(bytemuck::bytes_of(&MaterialUniform {
                    params: [
                        pass.scene_alpha_cutoff,
                        pass.fog() as u8 as f32,
                        pass.texture.is_some() as u8 as f32,
                        0.,
                    ],
                }));
                material_bytes.resize(at + material_slot as usize, 0);
                chunks.push(Chunk {
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("train vertices"),
                        contents: bytemuck::cast_slice(&chunk.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("train indices"),
                        contents: bytemuck::cast_slice(&chunk.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    count: chunk.indices.len() as u32,
                    texture,
                    material: (at as u64 / material_slot) as u32,
                });
            }
            models.push(Model {
                chunks,
                bind: source.bind.clone(),
            });
        }
        if material_bytes.is_empty() {
            material_bytes.resize(material_slot as usize, 0);
        }
        let frame = train.runtime.frame()?;
        let find = |name: &str| train.models.iter().position(|m| m.name == name);
        let cars: Vec<Option<usize>> = frame
            .cars
            .iter()
            .map(|c| find(&mesh_name(&c.model)))
            .collect();
        let wheels = find(WHEELS);
        let draw_slot = world.slot(std::mem::size_of::<DrawUniform>());
        let draws = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("train draws"),
            size: draw_slot * (cars.len() as u64 + 1),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let materials = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("train materials"),
            contents: &material_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let draw_group = world.draw_group(
            gpu,
            &materials,
            std::mem::size_of::<MaterialUniform>(),
            &draws,
            std::mem::size_of::<DrawUniform>(),
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("train.wgsl"));
        let pipeline = world.pipeline(
            gpu,
            PipelineDesc {
                label: "train",
                shader: &shader,
                vertex: "vs",
                fragment: "fs",
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 36,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32],
                })],
                blend: Blend::Opaque,
                depth_write: true,
                depth_compare: wgpu::CompareFunction::Less,
                per_draw: true,
            },
        );
        Ok(Self {
            pipeline,
            models,
            cars,
            wheels,
            groups,
            draws,
            draw_group,
            material_slot,
            draw_slot,
            used: Vec::new(),
        })
    }

    /// Uploads this step's car and wheel poses.
    pub fn prepare(&mut self, gpu: &Gpu, train: &Train) -> (u32, u32) {
        self.used.clear();
        let Ok(frame) = train.runtime.frame() else {
            return (0, 0);
        };
        let mut uniform: DrawUniform = bytemuck::Zeroable::zeroed();
        let mut write =
            |slot: usize, model: usize, matrix: [f32; 16], unlit: bool, bones: &[[f32; 16]]| {
                uniform.model = matrix;
                uniform.flags = [unlit as u8 as f32, 0., 0., 0.];
                for (i, bone) in uniform.bones.iter_mut().enumerate() {
                    *bone = bones.get(i).copied().unwrap_or(IDENTITY);
                }
                gpu.queue.write_buffer(
                    &self.draws,
                    slot as u64 * self.draw_slot,
                    bytemuck::bytes_of(&uniform),
                );
                self.used
                    .push((model, (slot as u64 * self.draw_slot) as u32));
            };
        for (slot, (car, model)) in frame.cars.iter().zip(&self.cars).enumerate() {
            if let Some(model) = *model {
                write(
                    slot,
                    model,
                    car.matrix,
                    !car.use_lighting,
                    &self.models[model].bind,
                );
            }
        }
        if let (Some(wheels), Some(matrices)) = (self.wheels, &frame.wheel_matrices) {
            write(self.cars.len(), wheels, IDENTITY, false, matrices);
        }
        let (mut draws, mut triangles) = (0, 0);
        for (model, _) in &self.used {
            for chunk in &self.models[*model].chunks {
                draws += 1;
                triangles += chunk.count / 3;
            }
        }
        (draws, triangles)
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        for (model, offset) in &self.used {
            for chunk in &self.models[*model].chunks {
                pass.set_bind_group(1, &self.groups[chunk.texture], &[]);
                pass.set_bind_group(
                    2,
                    &self.draw_group,
                    &[
                        (u64::from(chunk.material) * self.material_slot) as u32,
                        *offset,
                    ],
                );
                pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.count, 0, 0..1);
            }
        }
    }
}
