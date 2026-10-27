//! Keyframed TOY actors (research/AMBIENT_ACTORS.md): rigid-node models
//! posed by their motions. Every model's geometry is uploaded once into
//! one merged vertex/index buffer and shared by all its placements; each
//! drawn actor writes one pose slot per frame. Actors outside the view
//! frustum are neither posed for the GPU nor drawn.
use crate::{
    Gpu, cull, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::actors::{Actors, MAX_NODES};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    // use texture, vertex colour, blend
    flags: [f32; 4],
    // alpha cutoff, fog enabled
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    model: [f32; 16],
    // unlit
    flags: [f32; 4],
    bones: [[f32; 16]; MAX_NODES],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    blend: Blend,
    depth_write: bool,
    lequal: bool,
}

struct Chunk {
    first: u32,
    count: u32,
    key: Key,
    /// Bind groups per ATM frame (one for still textures).
    textures: Vec<usize>,
    /// Texture clock index for animated (ATM) textures.
    clock: Option<usize>,
    material: u32,
}

struct Model {
    chunks: Vec<Chunk>,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub draws: u32,
    pub triangles: u32,
    /// Actors posed and drawn this frame.
    pub visible: u32,
    pub culled: u32,
}

pub struct ActorPass {
    pipelines: HashMap<Key, wgpu::RenderPipeline>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    models: Vec<Model>,
    groups: Vec<wgpu::BindGroup>,
    instances: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    material_slot: u64,
    instance_slot: u64,
    /// CPU staging for this frame's visible instance slots.
    staging: Vec<u8>,
    /// Visible actors this frame: (model, instance offset).
    visible: Vec<(usize, u32)>,
    clocks: Vec<(atv_core::scenery_material::TextureClock, usize)>,
    tick: u64,
}

const STRIDE: u64 = 40;
const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

impl ActorPass {
    pub fn new(gpu: &Gpu, world: &World, actors: &Actors) -> Self {
        let device = &gpu.device;
        let sampler = texture::sampler(gpu, true, 1);
        let fallback = texture::upload(gpu, 1, 1, &[255; 4], "fallback");
        let mut groups = vec![world.texture_group(gpu, &fallback, &sampler)];
        let mut uploaded: HashMap<*const atv_game::materials::MaterialImage, Vec<usize>> =
            HashMap::new();
        let material_slot = world.slot(std::mem::size_of::<MaterialUniform>());
        let mut material_bytes = Vec::new();
        let (mut bytes, mut indices) = (Vec::<u8>::new(), Vec::<u32>::new());
        let mut clocks = Vec::new();
        let mut models = Vec::with_capacity(actors.models.len());
        for source in &actors.models {
            let mut chunks = Vec::new();
            for chunk in &source.chunks {
                let pass = chunk.pass();
                let state = pass.pass.as_ref();
                let textures = match &pass.texture {
                    None => vec![0],
                    Some(image) => uploaded
                        .entry(std::rc::Rc::as_ptr(image))
                        .or_insert_with(|| {
                            let (w, h) = (image.image.width, image.image.height);
                            let frames: Vec<&[u8]> = if image.frames.is_empty() {
                                vec![&image.image.rgba]
                            } else {
                                image.frames.iter().map(Vec::as_slice).collect()
                            };
                            frames
                                .into_iter()
                                .map(|rgba| {
                                    let t = texture::upload(gpu, w, h, rgba, "actor");
                                    groups.push(world.texture_group(gpu, &t, &sampler));
                                    groups.len() - 1
                                })
                                .collect()
                        })
                        .clone(),
                };
                let clock = state
                    .and_then(|p| p.texture_animation.clone())
                    .filter(|_| textures.len() > 1)
                    .and_then(|config| {
                        atv_core::scenery_material::TextureClock::new(config, textures.len()).ok()
                    })
                    .map(|clock| {
                        clocks.push((clock, 0));
                        clocks.len() - 1
                    });
                let blend = state.is_some_and(|p| p.blend);
                let key = Key {
                    blend: match (blend, state.is_some_and(|p| p.additive)) {
                        (false, _) => Blend::Opaque,
                        (true, false) => Blend::Alpha,
                        (true, true) => Blend::Additive,
                    },
                    depth_write: state.is_none_or(|p| p.depth_write),
                    lequal: chunk.pass_index > 0,
                };
                let prelit = state.is_some_and(|p| p.prelit);
                let at = material_bytes.len();
                material_bytes.extend_from_slice(bytemuck::bytes_of(&MaterialUniform {
                    flags: [
                        pass.texture.is_some() as u8 as f32,
                        prelit as u8 as f32,
                        blend as u8 as f32,
                        0.,
                    ],
                    params: [pass.scene_alpha_cutoff, pass.fog() as u8 as f32, 0., 0.],
                }));
                material_bytes.resize(at + material_slot as usize, 0);
                let base = (bytes.len() as u64 / STRIDE) as u32;
                let first = indices.len() as u32;
                for (i, color) in chunk.colors.iter().enumerate() {
                    let v = &chunk.vertices[i * 9..i * 9 + 9];
                    bytes.extend_from_slice(bytemuck::cast_slice(v));
                    bytes.extend_from_slice(color);
                }
                indices.extend(chunk.indices.iter().map(|i| i + base));
                chunks.push(Chunk {
                    first,
                    count: chunk.indices.len() as u32,
                    key,
                    textures,
                    clock,
                    material: (at as u64 / material_slot) as u32,
                });
            }
            models.push(Model { chunks });
        }
        if bytes.is_empty() {
            bytes.resize(STRIDE as usize, 0);
            indices.push(0);
        }
        if material_bytes.is_empty() {
            material_bytes.resize(material_slot as usize, 0);
        }
        let init = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let vertices = init("actor vertices", &bytes, wgpu::BufferUsages::VERTEX);
        let indices = init(
            "actor indices",
            bytemuck::cast_slice(&indices),
            wgpu::BufferUsages::INDEX,
        );
        let materials = init(
            "actor materials",
            &material_bytes,
            wgpu::BufferUsages::UNIFORM,
        );
        let instance_slot = world.slot(std::mem::size_of::<Instance>());
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("actor instances"),
            size: instance_slot * actors.actors.len().max(1) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let draw_group = world.draw_group(
            gpu,
            &materials,
            std::mem::size_of::<MaterialUniform>(),
            &instances,
            std::mem::size_of::<Instance>(),
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("actors.wgsl"));
        let mut pipelines = HashMap::new();
        for chunk in models.iter().flat_map(|m| &m.chunks) {
            pipelines
                .entry(chunk.key)
                .or_insert_with(|| pipeline(gpu, world, &shader, chunk.key));
        }
        Self {
            pipelines,
            vertices,
            indices,
            models,
            groups,
            instances,
            draw_group,
            material_slot,
            instance_slot,
            staging: Vec::new(),
            visible: Vec::new(),
            clocks,
            tick: 0,
        }
    }

    /// One 60 Hz step of the actors' ATM texture clocks (the scenery's
    /// `001bd8e0` frame clock).
    pub fn step(&mut self) {
        let milliseconds = (self.tick * 1000 / 60) as u32;
        self.tick += 1;
        for (clock, frame) in &mut self.clocks {
            *frame = clock.tick(milliseconds);
        }
    }

    /// Keeps a lost device's texture clocks (graphics context restored).
    pub fn adopt(&mut self, old: ActorPass) {
        if old.clocks.len() == self.clocks.len() {
            self.clocks = old.clocks;
            self.tick = old.tick;
        }
    }

    /// Poses and uploads the actors inside the view.
    pub fn prepare(&mut self, gpu: &Gpu, actors: &Actors, cull_planes: &[[f32; 4]]) -> Stats {
        self.visible.clear();
        self.staging.clear();
        let mut stats = Stats::default();
        for index in 0..actors.actors.len() {
            let Some(pose) = actors.pose(index) else {
                continue;
            };
            if !pose.min[0].is_finite() || cull::outside(cull_planes, pose.min, pose.max) {
                stats.culled += 1;
                continue;
            }
            let actor = &actors.actors[index];
            let mut instance = Instance {
                model: pose.placement,
                flags: [(!actor.definition.use_lighting) as u8 as f32, 0., 0., 0.],
                bones: [IDENTITY; MAX_NODES],
            };
            for (bone, node) in instance.bones.iter_mut().zip(pose.nodes.iter()) {
                *bone = *node;
            }
            let offset = self.staging.len();
            self.staging
                .extend_from_slice(bytemuck::bytes_of(&instance));
            self.staging.resize(offset + self.instance_slot as usize, 0);
            self.visible.push((actor.model, offset as u32));
            stats.visible += 1;
            for chunk in &self.models[actor.model].chunks {
                stats.draws += 1;
                stats.triangles += chunk.count / 3;
            }
        }
        if !self.staging.is_empty() {
            gpu.queue.write_buffer(&self.instances, 0, &self.staging);
        }
        stats
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        if self.visible.is_empty() {
            return;
        }
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut current: Option<Key> = None;
        let mut bound_texture = usize::MAX;
        // Opaque passes first, then blended ones over them.
        for blended in [false, true] {
            for &(model, offset) in &self.visible {
                for chunk in &self.models[model].chunks {
                    if (chunk.key.blend != Blend::Opaque) != blended {
                        continue;
                    }
                    let frame = chunk.clock.map_or(0, |c| self.clocks[c].1);
                    let texture = chunk.textures[frame.min(chunk.textures.len() - 1)];
                    if texture != bound_texture {
                        pass.set_bind_group(1, &self.groups[texture], &[]);
                        bound_texture = texture;
                    }
                    if current != Some(chunk.key) {
                        pass.set_pipeline(&self.pipelines[&chunk.key]);
                        current = Some(chunk.key);
                    }
                    pass.set_bind_group(
                        2,
                        &self.draw_group,
                        &[
                            (u64::from(chunk.material) * self.material_slot) as u32,
                            offset,
                        ],
                    );
                    pass.draw_indexed(chunk.first..chunk.first + chunk.count, 0, 0..1);
                }
            }
        }
    }
}

fn pipeline(
    gpu: &Gpu,
    world: &World,
    shader: &wgpu::ShaderModule,
    key: Key,
) -> wgpu::RenderPipeline {
    world.pipeline(
        gpu,
        PipelineDesc {
            label: "actors",
            shader,
            vertex: "vs",
            fragment: "fs",
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32, 4 => Unorm8x4],
            })],
            blend: key.blend,
            depth_write: key.depth_write,
            depth_compare: if key.lequal {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Less
            },
            per_draw: true,
        },
    )
}
