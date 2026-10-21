//! Static scene models in scene order (the sceneDraws loop of web/game.js's
//! render()): per-pass blend, depth write/compare, cutouts and the GS
//! FB_ONLY alpha-failure pass, UV scroll, ATM frames and reflection maps.
use crate::{
    Gpu, cull, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::scenery::{Clocks, Scenery};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    flags: [f32; 4],
    params: [f32; 4],
    uv_offset: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    model: [f32; 16],
    flags: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    blend: Blend,
    depth_write: bool,
    lequal: bool,
    reflect: bool,
}

struct Chunk {
    vertices: wgpu::Buffer,
    reflection: Option<wgpu::Buffer>,
    indices: wgpu::Buffer,
    count: u32,
    key: Key,
    /// Material slot; the alpha-failure variant follows at `material + 1`.
    material: u32,
    alpha_fail: bool,
    /// Bind groups per ATM frame (one for still textures; fallback when untextured).
    textures: Vec<usize>,
    uniform: MaterialUniform,
}

struct Draw {
    chunk: usize,
    flash: Option<usize>,
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub draws: u32,
    pub triangles: u32,
    pub culled: u32,
}

pub struct SceneryPass {
    pipelines: HashMap<Key, wgpu::RenderPipeline>,
    chunks: Vec<Chunk>,
    draws: Vec<Draw>,
    groups: Vec<wgpu::BindGroup>,
    materials: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    material_slot: u64,
    draw_slot: u64,
    clocks: Clocks,
    /// Chunks whose UV offset animates (rewritten each frame).
    animated: Vec<usize>,
    visible: Vec<usize>,
    /// Lit stadium camera flashes (pool indices).
    pub lit_flashes: Vec<usize>,
    /// Farthest world-bounds diagonal, for the depth range.
    pub reach: f32,
}

const STRIDE: u64 = 36;
const REFLECTION_STRIDE: u64 = 72;

impl SceneryPass {
    pub fn new(gpu: &Gpu, world: &World, scenery: &Scenery, anisotropy: u16) -> Self {
        let device = &gpu.device;
        let sampler = texture::sampler(gpu, true, anisotropy);
        let fallback = texture::upload(gpu, 1, 1, &[255; 4], "fallback");
        let mut groups = vec![world.texture_group(gpu, &fallback, &sampler)];
        let mut uploaded: HashMap<*const atv_game::materials::MaterialImage, Vec<usize>> =
            HashMap::new();
        let material_slot = world.slot(std::mem::size_of::<MaterialUniform>());
        let draw_slot = world.slot(std::mem::size_of::<DrawUniform>());
        let mut chunks = Vec::with_capacity(scenery.chunks.len());
        let mut material_bytes = Vec::new();
        let clocks = Clocks::new(scenery);
        let mut animated = Vec::new();
        for (index, source) in scenery.chunks.iter().enumerate() {
            let pass = source.pass();
            let state = pass.pass.as_ref();
            let reflection_plane = state.map_or(0, |p| p.reflection_plane);
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
                                let t = texture::upload(gpu, w, h, rgba, "scenery");
                                groups.push(world.texture_group(gpu, &t, &sampler));
                                groups.len() - 1
                            })
                            .collect()
                    })
                    .clone(),
            };
            let blend = state.is_some_and(|p| p.blend);
            let key = Key {
                blend: match (blend, state.is_some_and(|p| p.additive)) {
                    (false, _) => Blend::Opaque,
                    (true, false) => Blend::Alpha,
                    (true, true) => Blend::Additive,
                },
                depth_write: state.is_none_or(|p| p.depth_write),
                lequal: source.pass_index > 0,
                reflect: reflection_plane != 0,
            };
            let uniform = MaterialUniform {
                flags: [
                    pass.texture.is_some() as u8 as f32,
                    state.is_some_and(|p| p.prelit) as u8 as f32,
                    blend as u8 as f32,
                    0.,
                ],
                params: [
                    pass.scene_alpha_cutoff,
                    pass.fog() as u8 as f32,
                    f32::from(reflection_plane),
                    0.,
                ],
                uv_offset: [0.; 4],
            };
            let alpha_fail = pass.scene_alpha_failure == 1. && pass.scene_alpha_cutoff > 0.;
            let material = (material_bytes.len() as u64 / material_slot) as u32;
            let mut push = |uniform: &MaterialUniform| {
                let at = material_bytes.len();
                material_bytes.extend_from_slice(bytemuck::bytes_of(uniform));
                material_bytes.resize(at + material_slot as usize, 0);
            };
            push(&uniform);
            if alpha_fail {
                let mut fail = uniform;
                fail.flags[3] = 1.;
                push(&fail);
            }
            if clocks.animated(index) {
                animated.push(index);
            }
            let mut bytes = Vec::with_capacity(source.colors.len() * STRIDE as usize);
            for (i, color) in source.colors.iter().enumerate() {
                bytes.extend_from_slice(bytemuck::cast_slice(&source.vertices[i * 8..i * 8 + 8]));
                bytes.extend_from_slice(color);
            }
            let reflection = (reflection_plane != 0).then(|| {
                let mut data = Vec::with_capacity(source.colors.len() * 18);
                for v in source.reflection.as_chunks::<7>().0 {
                    data.extend_from_slice(&v[..6]);
                    let node = &source.nodes[(v[6] as usize).min(source.nodes.len() - 1)];
                    for column in 0..4 {
                        data.extend_from_slice(&node[column * 4..column * 4 + 3]);
                    }
                }
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("scenery reflection"),
                    contents: bytemuck::cast_slice(&data),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            });
            chunks.push(Chunk {
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("scenery vertices"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                reflection,
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("scenery indices"),
                    contents: bytemuck::cast_slice(&source.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: source.indices.len() as u32,
                key,
                material,
                alpha_fail,
                textures,
                uniform,
            });
        }
        if material_bytes.is_empty() {
            material_bytes.resize(material_slot as usize, 0);
        }
        let materials = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scenery materials"),
            contents: &material_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mut draw_bytes = Vec::with_capacity(scenery.draws.len().max(1) * draw_slot as usize);
        let mut draws = Vec::with_capacity(scenery.draws.len());
        let mut reach = 0f32;
        for draw in &scenery.draws {
            let at = draw_bytes.len();
            draw_bytes.extend_from_slice(bytemuck::bytes_of(&DrawUniform {
                model: draw.matrix,
                flags: [draw.unlit as u8 as f32, 0., 0., 0.],
            }));
            draw_bytes.resize(at + draw_slot as usize, 0);
            let d: [f32; 3] = std::array::from_fn(|a| draw.max[a] - draw.min[a]);
            let diagonal = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if diagonal.is_finite() {
                reach = reach.max(diagonal);
            }
            draws.push(Draw {
                chunk: draw.chunk,
                flash: draw.flash,
                min: draw.min,
                max: draw.max,
            });
        }
        if draw_bytes.is_empty() {
            draw_bytes.resize(draw_slot as usize, 0);
        }
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scenery draws"),
            contents: &draw_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let draw_group = world.draw_group(
            gpu,
            &materials,
            std::mem::size_of::<MaterialUniform>(),
            &instances,
            std::mem::size_of::<DrawUniform>(),
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("scenery.wgsl"));
        let mut pipelines = HashMap::new();
        for chunk in &chunks {
            let mut keys = vec![chunk.key];
            if chunk.alpha_fail {
                keys.push(Key {
                    depth_write: false,
                    ..chunk.key
                });
            }
            for key in keys {
                pipelines
                    .entry(key)
                    .or_insert_with(|| pipeline(gpu, world, &shader, key));
            }
        }
        Self {
            pipelines,
            chunks,
            draws,
            groups,
            materials,
            draw_group,
            material_slot,
            draw_slot,
            clocks,
            animated,
            visible: Vec::new(),
            lit_flashes: Vec::new(),
            reach,
        }
    }

    /// Advances the scenery clocks by one 60 Hz simulation step.
    pub fn step(&mut self) {
        self.clocks.step();
    }

    pub fn steps(&self) -> u64 {
        self.clocks.steps()
    }

    pub fn prepare(&mut self, gpu: &Gpu, cull_planes: &[[f32; 4]]) -> Stats {
        for &index in &self.animated {
            let chunk = &mut self.chunks[index];
            let [u, v] = self.clocks.uv_offset(index);
            chunk.uniform.uv_offset = [u, v, 0., 0.];
            let mut slots = vec![chunk.uniform];
            if chunk.alpha_fail {
                let mut fail = chunk.uniform;
                fail.flags[3] = 1.;
                slots.push(fail);
            }
            for (i, uniform) in slots.iter().enumerate() {
                gpu.queue.write_buffer(
                    &self.materials,
                    (u64::from(chunk.material) + i as u64) * self.material_slot,
                    bytemuck::bytes_of(uniform),
                );
            }
        }
        self.visible.clear();
        let mut stats = Stats::default();
        for (index, draw) in self.draws.iter().enumerate() {
            if draw.flash.is_some_and(|f| !self.lit_flashes.contains(&f)) {
                continue;
            }
            if cull::outside(cull_planes, draw.min, draw.max) {
                stats.culled += 1;
                continue;
            }
            self.visible.push(index);
            let chunk = &self.chunks[draw.chunk];
            let passes = 1 + chunk.alpha_fail as u32;
            stats.draws += passes;
            stats.triangles += chunk.count / 3 * passes;
        }
        stats
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_bind_group(0, &world.globals_group, &[]);
        let mut current: Option<Key> = None;
        let mut bound_chunk = usize::MAX;
        let mut bound_texture = usize::MAX;
        for &index in &self.visible {
            let draw = &self.draws[index];
            let chunk = &self.chunks[draw.chunk];
            let frame = self.clocks.texture_frame(draw.chunk);
            let texture = chunk.textures[frame.min(chunk.textures.len() - 1)];
            if texture != bound_texture {
                pass.set_bind_group(1, &self.groups[texture], &[]);
                bound_texture = texture;
            }
            if bound_chunk != draw.chunk {
                pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                if let Some(reflection) = &chunk.reflection {
                    pass.set_vertex_buffer(1, reflection.slice(..));
                }
                pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
                bound_chunk = draw.chunk;
            }
            let instance = (index as u64 * self.draw_slot) as u32;
            for fail in [false, true] {
                if fail && !chunk.alpha_fail {
                    break;
                }
                // GS FB_ONLY keeps failed fragments in the framebuffer without
                // writing Z; the predicates are exclusive, each pixel blends once.
                let key = Key {
                    depth_write: chunk.key.depth_write && !fail,
                    ..chunk.key
                };
                if current != Some(key) {
                    pass.set_pipeline(&self.pipelines[&key]);
                    current = Some(key);
                }
                let material =
                    ((u64::from(chunk.material) + fail as u64) * self.material_slot) as u32;
                pass.set_bind_group(2, &self.draw_group, &[material, instance]);
                pass.draw_indexed(0..chunk.count, 0, 0..1);
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
    let main = wgpu::VertexBufferLayout {
        array_stride: STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Unorm8x4],
    };
    let reflection = wgpu::VertexBufferLayout {
        array_stride: REFLECTION_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x3, 6 => Float32x3, 7 => Float32x3, 8 => Float32x3, 9 => Float32x3],
    };
    let buffers = [Some(main), Some(reflection)];
    world.pipeline(
        gpu,
        PipelineDesc {
            label: "scenery",
            shader,
            vertex: if key.reflect { "vs_reflect" } else { "vs" },
            fragment: "fs",
            buffers: &buffers[..if key.reflect { 2 } else { 1 }],
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
