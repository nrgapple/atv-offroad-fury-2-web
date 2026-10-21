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

/// A draw's material followed by its instance (scenery.wgsl `Record`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Record {
    m: MaterialUniform,
    d: DrawUniform,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    blend: Blend,
    depth_write: bool,
    lequal: bool,
    reflect: bool,
}

/// Every chunk's geometry lives in one of two merged sets (plain, and
/// reflection-mapped with its second vertex stream), indices rebased, so
/// consecutive draws of different models no longer rebind vertex buffers:
/// the GL backend re-specifies attributes only on pipeline changes.
struct Geometry {
    vertices: wgpu::Buffer,
    reflection: Option<wgpu::Buffer>,
    indices: wgpu::Buffer,
}

struct Chunk {
    /// 0 plain, 1 reflection-mapped ([`SceneryPass::sets`]).
    set: usize,
    first: u32,
    count: u32,
    key: Key,
    alpha_fail: bool,
    /// Bind groups per ATM frame (one for still textures; fallback when untextured).
    textures: Vec<usize>,
    uniform: MaterialUniform,
}

struct Draw {
    chunk: usize,
    /// Record slot (material + instance); the alpha-failure pass follows.
    record: u32,
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
    sets: Vec<Geometry>,
    chunks: Vec<Chunk>,
    draws: Vec<Draw>,
    groups: Vec<wgpu::BindGroup>,
    /// Per-draw records ([`Record`]): one dynamic offset per draw. Draws of
    /// UV-animated chunks come first so one write refreshes them all.
    records: wgpu::Buffer,
    record_group: wgpu::BindGroup,
    record_slot: u64,
    /// CPU copy of the animated draws' records (the buffer's leading block).
    animated_records: Vec<u8>,
    animated_draws: Vec<usize>,
    clocks: Clocks,
    /// Chunks whose UV offset animates (rewritten each frame).
    animated: Vec<usize>,
    visible: Vec<usize>,
    /// Lit stadium camera flashes (pool indices).
    pub lit_flashes: Vec<usize>,
    /// The stadium flash clock (0020ee10), seeded as the browser's op 115.
    pub flashes: Option<atv_core::race_crowd::Flashes>,
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
        let record_slot = world.slot(std::mem::size_of::<Record>());
        let mut chunks = Vec::with_capacity(scenery.chunks.len());
        let clocks = Clocks::new(scenery);
        let mut animated = Vec::new();
        // Plain and reflection-mapped sets: vertex bytes, indices, reflection floats.
        let mut merged: [(Vec<u8>, Vec<u32>, Vec<f32>); 2] = Default::default();
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
            if clocks.animated(index) {
                animated.push(index);
            }
            let set = usize::from(reflection_plane != 0);
            let (bytes, indices, reflection) = &mut merged[set];
            let base = (bytes.len() as u64 / STRIDE) as u32;
            let first = indices.len() as u32;
            for (i, color) in source.colors.iter().enumerate() {
                // Position, U, normal, V: the original values in two vec4s.
                let v = &source.vertices[i * 8..i * 8 + 8];
                let packed = [v[0], v[1], v[2], v[6], v[3], v[4], v[5], v[7]];
                bytes.extend_from_slice(bytemuck::cast_slice(&packed));
                bytes.extend_from_slice(color);
            }
            indices.extend(source.indices.iter().map(|i| i + base));
            if set == 1 {
                for v in source.reflection.as_chunks::<7>().0 {
                    reflection.extend_from_slice(&v[..6]);
                    let node = &source.nodes[(v[6] as usize).min(source.nodes.len() - 1)];
                    for column in 0..4 {
                        reflection.extend_from_slice(&node[column * 4..column * 4 + 3]);
                    }
                }
                // One reflection record per vertex keeps both streams aligned.
                reflection.resize(bytes.len() / STRIDE as usize * 18, 0.);
            }
            chunks.push(Chunk {
                set,
                first,
                count: source.indices.len() as u32,
                key,
                alpha_fail,
                textures,
                uniform,
            });
        }
        let sets = merged
            .into_iter()
            .enumerate()
            .map(|(set, (mut bytes, mut indices, reflection))| {
                // Buffers may not be empty; an unused set keeps one dummy record.
                if bytes.is_empty() {
                    bytes.resize(STRIDE as usize, 0);
                    indices.push(0);
                }
                let buffer = |label, contents: &[u8], usage| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents,
                        usage,
                    })
                };
                let reflection = (set == 1).then(|| {
                    let mut reflection = reflection;
                    reflection.resize(bytes.len() / STRIDE as usize * 18, 0.);
                    buffer(
                        "scenery reflection",
                        bytemuck::cast_slice(&reflection),
                        wgpu::BufferUsages::VERTEX,
                    )
                });
                Geometry {
                    vertices: buffer("scenery vertices", &bytes, wgpu::BufferUsages::VERTEX),
                    reflection,
                    indices: buffer(
                        "scenery indices",
                        bytemuck::cast_slice(&indices),
                        wgpu::BufferUsages::INDEX,
                    ),
                }
            })
            .collect();
        let mut draws = Vec::with_capacity(scenery.draws.len());
        let mut reach = 0f32;
        for draw in &scenery.draws {
            let d: [f32; 3] = std::array::from_fn(|a| draw.max[a] - draw.min[a]);
            let diagonal = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if diagonal.is_finite() {
                reach = reach.max(diagonal);
            }
            draws.push(Draw {
                chunk: draw.chunk,
                record: 0,
                flash: draw.flash,
                min: draw.min,
                max: draw.max,
            });
        }
        // Records: animated chunks' draws first, then the rest, in draw order.
        let animated_chunk: Vec<bool> = (0..chunks.len()).map(|c| animated.contains(&c)).collect();
        let is_animated = |i: usize| animated_chunk[draws[i].chunk];
        let animated_draws: Vec<usize> = (0..draws.len()).filter(|&i| is_animated(i)).collect();
        let order: Vec<usize> = animated_draws
            .iter()
            .copied()
            .chain((0..draws.len()).filter(|&i| !is_animated(i)))
            .collect();
        let mut record_bytes = Vec::new();
        let mut animated_bytes = 0;
        for index in order {
            let draw = &mut draws[index];
            let chunk = &chunks[draw.chunk];
            draw.record = (record_bytes.len() as u64 / record_slot) as u32;
            let source = &scenery.draws[index];
            let instance = DrawUniform {
                model: source.matrix,
                flags: [source.unlit as u8 as f32, 0., 0., 0.],
            };
            let mut push = |m: MaterialUniform| {
                let at = record_bytes.len();
                record_bytes.extend_from_slice(bytemuck::bytes_of(&Record { m, d: instance }));
                record_bytes.resize(at + record_slot as usize, 0);
            };
            push(chunk.uniform);
            if chunk.alpha_fail {
                let mut fail = chunk.uniform;
                fail.flags[3] = 1.;
                push(fail);
            }
            if animated_chunk[draw.chunk] {
                animated_bytes = record_bytes.len();
            }
        }
        if record_bytes.is_empty() {
            record_bytes.resize(record_slot as usize, 0);
        }
        let animated_records = record_bytes[..animated_bytes].to_vec();
        let records = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scenery records"),
            contents: &record_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let record_layout = world.record_layout(gpu);
        let record_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scenery records"),
            layout: &record_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &records,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<Record>() as u64),
                }),
            }],
        });
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
                    .or_insert_with(|| pipeline(gpu, world, &shader, key, &record_layout));
            }
        }
        Self {
            pipelines,
            sets,
            chunks,
            draws,
            groups,
            records,
            record_group,
            record_slot,
            animated_records,
            animated_draws,
            clocks,
            animated,
            visible: Vec::new(),
            lit_flashes: Vec::new(),
            flashes: scenery.flashes.map(|(config, pool)| {
                atv_core::race_crowd::Flashes::new(config, pool, 97211 + 8 * 47)
            }),
            reach,
        }
    }

    /// Advances the scenery clocks by one 60 Hz simulation step.
    pub fn step(&mut self) {
        self.clocks.step();
        if let Some(flashes) = &mut self.flashes {
            let lit = flashes.step(1. / 60.);
            self.lit_flashes.clear();
            self.lit_flashes.extend(lit.iter().map(|&i| usize::from(i)));
        }
    }

    pub fn steps(&self) -> u64 {
        self.clocks.steps()
    }

    pub fn prepare(&mut self, gpu: &Gpu, cull_planes: &[[f32; 4]]) -> Stats {
        for &index in &self.animated {
            let [u, v] = self.clocks.uv_offset(index);
            self.chunks[index].uniform.uv_offset = [u, v, 0., 0.];
        }
        if !self.animated_records.is_empty() {
            let offset = std::mem::offset_of!(MaterialUniform, uv_offset);
            for &index in &self.animated_draws {
                let draw = &self.draws[index];
                let chunk = &self.chunks[draw.chunk];
                let uv = bytemuck::bytes_of(&chunk.uniform.uv_offset);
                for pass in 0..1 + usize::from(chunk.alpha_fail) {
                    let at = (draw.record as usize + pass) * self.record_slot as usize + offset;
                    self.animated_records[at..at + uv.len()].copy_from_slice(uv);
                }
            }
            gpu.queue
                .write_buffer(&self.records, 0, &self.animated_records);
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
        let mut bound_set = usize::MAX;
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
            if bound_set != chunk.set {
                let set = &self.sets[chunk.set];
                pass.set_vertex_buffer(0, set.vertices.slice(..));
                if let Some(reflection) = &set.reflection {
                    pass.set_vertex_buffer(1, reflection.slice(..));
                }
                pass.set_index_buffer(set.indices.slice(..), wgpu::IndexFormat::Uint32);
                bound_set = chunk.set;
            }
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
                let record = (u64::from(draw.record + fail as u32) * self.record_slot) as u32;
                pass.set_bind_group(2, &self.record_group, &[record]);
                pass.draw_indexed(chunk.first..chunk.first + chunk.count, 0, 0..1);
            }
        }
    }
}

fn pipeline(
    gpu: &Gpu,
    world: &World,
    shader: &wgpu::ShaderModule,
    key: Key,
    records: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let main = wgpu::VertexBufferLayout {
        array_stride: STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 3 => Unorm8x4],
    };
    let reflection = wgpu::VertexBufferLayout {
        array_stride: REFLECTION_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x3, 6 => Float32x3, 7 => Float32x3, 8 => Float32x3, 9 => Float32x3],
    };
    let buffers = [Some(main), Some(reflection)];
    world.pipeline_layouts(
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
        &world.texture_layout,
        records,
    )
}
