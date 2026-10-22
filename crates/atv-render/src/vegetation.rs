//! Original ESB vegetation (drawEcosystem in web/game.js): 0012af80 submits
//! distant billboards, depth-disabled transition billboards, then the near
//! material queues; atv_core::vegetation_render selects and orders them.
use crate::{
    Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_core::vegetation_render::{BILLBOARD_FLOATS, MESH_FLOATS};
use atv_game::vegetation::Vegetation;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    /// Alpha cutoff, fog, scenery blend, billboard.
    params: [f32; 4],
    /// Use texture.
    flags: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    billboard: bool,
    blend: bool,
    depth_write: bool,
}

struct Chunk {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    texture: usize,
    material: u32,
    blend: bool,
    depth_write: bool,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub draws: u32,
    pub triangles: u32,
    pub visible: u32,
}

pub struct VegetationPass {
    pipelines: HashMap<Key, wgpu::RenderPipeline>,
    groups: Vec<Vec<Chunk>>,
    /// Billboard chunk, then its far and transition material slots.
    billboard: Option<(Chunk, u32)>,
    textures: Vec<wgpu::BindGroup>,
    /// The textures behind `textures` (rebound when anisotropy changes).
    images: Vec<wgpu::Texture>,
    instances: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    slot: u64,
    warmed: bool,
    counts: atv_core::vegetation_render::Counts,
    group_counts: Vec<u32>,
    group_offsets: Vec<usize>,
    selection: std::rc::Rc<std::cell::RefCell<atv_core::vegetation_render::Vegetation>>,
}

impl VegetationPass {
    /// applyAnisotropy: every vegetation texture rebinds with a new sampler.
    pub fn set_anisotropy(&mut self, gpu: &Gpu, world: &World, anisotropy: u16) {
        let sampler = texture::sampler(gpu, true, anisotropy);
        self.textures = self
            .images
            .iter()
            .map(|t| world.texture_group(gpu, t, &sampler))
            .collect();
    }

    pub fn new(gpu: &Gpu, world: &World, vegetation: &Vegetation, anisotropy: u16) -> Self {
        let device = &gpu.device;
        let sampler = texture::sampler(gpu, true, anisotropy);
        let fallback = texture::upload(gpu, 1, 1, &[255; 4], "fallback");
        let mut textures = vec![world.texture_group(gpu, &fallback, &sampler)];
        let mut images = vec![fallback];
        let mut uploaded: HashMap<*const atv_game::materials::MaterialImage, usize> =
            HashMap::new();
        let slot = world.slot(std::mem::size_of::<MaterialUniform>());
        let mut materials = Vec::new();
        let mut material = |uniform: MaterialUniform| {
            let at = materials.len();
            materials.extend_from_slice(bytemuck::bytes_of(&uniform));
            materials.resize(at + slot as usize, 0);
            (at as u64 / slot) as u32
        };
        let mut texture_of = |pass: &atv_game::materials::MaterialPass| match &pass.texture {
            None => 0,
            Some(image) => *uploaded
                .entry(std::rc::Rc::as_ptr(image))
                .or_insert_with(|| {
                    let t = texture::upload(
                        gpu,
                        image.image.width,
                        image.image.height,
                        &image.image.rgba,
                        "vegetation",
                    );
                    textures.push(world.texture_group(gpu, &t, &sampler));
                    images.push(t);
                    textures.len() - 1
                }),
        };
        let buffer = |data: &[u8], usage, label| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data,
                usage,
            })
        };
        let mut groups = Vec::with_capacity(vegetation.groups.len());
        for group in &vegetation.groups {
            let mut chunks = Vec::with_capacity(group.chunks.len());
            for source in &group.chunks {
                let pass = source.pass();
                let state = pass.pass.as_ref();
                let blend = state.is_some_and(|p| p.blend);
                chunks.push(Chunk {
                    vertices: buffer(
                        bytemuck::cast_slice(&source.vertices),
                        wgpu::BufferUsages::VERTEX,
                        "vegetation vertices",
                    ),
                    indices: buffer(
                        bytemuck::cast_slice(&source.indices),
                        wgpu::BufferUsages::INDEX,
                        "vegetation indices",
                    ),
                    count: source.indices.len() as u32,
                    texture: texture_of(pass),
                    material: material(MaterialUniform {
                        params: [
                            pass.alpha_test,
                            pass.fog() as u8 as f32,
                            blend as u8 as f32,
                            0.,
                        ],
                        flags: [pass.texture.is_some() as u8 as f32, 0., 0., 0.],
                    }),
                    blend,
                    depth_write: state.is_none_or(|p| p.depth_write),
                });
            }
            groups.push(chunks);
        }
        let billboard = vegetation.billboard.as_ref().map(|billboard| {
            let pass = &billboard.material.passes[0];
            let state = pass.pass.as_ref();
            let blend = state.is_none_or(|p| p.blend);
            let mut vertices: Vec<f32> = Vec::with_capacity(48);
            for [column, row] in [[1., 0.], [0., 0.], [0., 1.], [1., 1.], [2., 1.], [2., 0.]] {
                vertices.extend([0., row, 0., column, 1., 0., 0., 0.]);
            }
            let indices: [u32; 12] = [0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5];
            let params = |cutoff| MaterialUniform {
                params: [cutoff, pass.fog() as u8 as f32, blend as u8 as f32, 1.],
                flags: [1., 0., 0., 0.],
            };
            let far = material(params(pass.alpha_test));
            let transition = material(params(1. / 255.));
            (
                Chunk {
                    vertices: buffer(
                        bytemuck::cast_slice(&vertices),
                        wgpu::BufferUsages::VERTEX,
                        "billboard vertices",
                    ),
                    indices: buffer(
                        bytemuck::cast_slice(&indices),
                        wgpu::BufferUsages::INDEX,
                        "billboard indices",
                    ),
                    count: 12,
                    texture: texture_of(pass),
                    material: far,
                    blend,
                    depth_write: state.is_none_or(|p| p.depth_write),
                },
                transition,
            )
        });
        if materials.is_empty() {
            materials.resize(slot as usize, 0);
        }
        let materials = buffer(
            &materials,
            wgpu::BufferUsages::UNIFORM,
            "vegetation materials",
        );
        let size = std::mem::size_of::<MaterialUniform>();
        let draw_group = world.draw_group(gpu, &materials, size, &materials, 16);
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vegetation instances"),
            size: (vegetation.selection.borrow().output.len() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("vegetation.wgsl"));
        let mut pipelines = HashMap::new();
        let mut keys = Vec::new();
        for chunk in groups.iter().flatten() {
            keys.push(Key {
                billboard: false,
                blend: chunk.blend,
                depth_write: chunk.depth_write,
            });
        }
        if let Some((chunk, _)) = &billboard {
            for depth_write in [chunk.depth_write, false] {
                keys.push(Key {
                    billboard: true,
                    blend: chunk.blend,
                    depth_write,
                });
            }
        }
        for key in keys {
            pipelines
                .entry(key)
                .or_insert_with(|| pipeline(gpu, world, &shader, key));
        }
        Self {
            pipelines,
            group_counts: vec![0; groups.len()],
            group_offsets: vec![0; groups.len()],
            groups,
            billboard,
            textures,
            images,
            instances,
            draw_group,
            slot,
            warmed: false,
            counts: Default::default(),
            selection: vegetation.selection.clone(),
        }
    }

    /// Selects this frame's plants and uploads their instances.
    pub fn prepare(&mut self, gpu: &Gpu, eye: [f32; 3], yaw: f32) -> Stats {
        let mut selection = self.selection.borrow_mut();
        let selection = &mut *selection;
        let forward = [f64::from(yaw.sin()), f64::from(yaw.cos())];
        let camera = [f64::from(eye[0]), f64::from(eye[2])];
        // A rising frame serial (the browser's rAF id) drives 0012a1c0's
        // stale-visibility reset: a near plant unseen for more than three
        // frames restarts at its full mesh. The browser's first frame is
        // followed by many more; select once ahead so a single captured frame
        // shows the same transition billboards.
        if !self.warmed {
            selection.select_next(camera, forward);
            self.warmed = true;
        }
        self.counts = selection.select_next(camera, forward);
        let billboards = (self.counts.far + self.counts.transition) as usize * BILLBOARD_FLOATS;
        if billboards > 0 {
            gpu.queue.write_buffer(
                &self.instances,
                0,
                bytemuck::cast_slice(&selection.output[..billboards]),
            );
        }
        let mesh_start = selection.group_offset(0);
        let mesh_end = mesh_start + self.counts.near as usize * MESH_FLOATS;
        if mesh_end > mesh_start {
            gpu.queue.write_buffer(
                &self.instances,
                mesh_start as u64 * 4,
                bytemuck::cast_slice(&selection.output[mesh_start..mesh_end]),
            );
        }
        let mut stats = Stats {
            visible: self.counts.visible,
            ..Default::default()
        };
        for (index, chunks) in self.groups.iter().enumerate() {
            self.group_counts[index] = selection.group_counts[index];
            self.group_offsets[index] = selection.group_offset(index);
            let count = self.group_counts[index];
            if count > 0 {
                for chunk in chunks {
                    stats.draws += 1;
                    stats.triangles += chunk.count / 3 * count;
                }
            }
        }
        if let Some((chunk, _)) = &self.billboard {
            for count in [self.counts.far, self.counts.transition] {
                if count > 0 {
                    stats.draws += 1;
                    stats.triangles += chunk.count / 3 * count;
                }
            }
        }
        stats
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_bind_group(0, &world.globals_group, &[]);
        let mut current = None;
        let mut bind = |pass: &mut wgpu::RenderPass<'_>, key: Key| {
            if current != Some(key) {
                pass.set_pipeline(&self.pipelines[&key]);
                current = Some(key);
            }
        };
        if let Some((chunk, transition_material)) = &self.billboard {
            for transition in [false, true] {
                let (count, first) = if transition {
                    (self.counts.transition, self.counts.far)
                } else {
                    (self.counts.far, 0)
                };
                if count == 0 {
                    continue;
                }
                bind(
                    pass,
                    Key {
                        billboard: true,
                        blend: chunk.blend,
                        depth_write: !transition && chunk.depth_write,
                    },
                );
                let material = if transition {
                    *transition_material
                } else {
                    chunk.material
                };
                self.submit(
                    pass,
                    chunk,
                    material,
                    (first as usize * BILLBOARD_FLOATS * 4) as u64,
                    count,
                );
            }
        }
        for (index, chunks) in self.groups.iter().enumerate() {
            let count = self.group_counts[index];
            if count == 0 {
                continue;
            }
            let offset = (self.group_offsets[index] * 4) as u64;
            for chunk in chunks {
                bind(
                    pass,
                    Key {
                        billboard: false,
                        blend: chunk.blend,
                        depth_write: chunk.depth_write,
                    },
                );
                self.submit(pass, chunk, chunk.material, offset, count);
            }
        }
    }

    fn submit(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        chunk: &Chunk,
        material: u32,
        offset: u64,
        count: u32,
    ) {
        pass.set_bind_group(1, &self.textures[chunk.texture], &[]);
        pass.set_bind_group(
            2,
            &self.draw_group,
            &[(u64::from(material) * self.slot) as u32, 0],
        );
        pass.set_vertex_buffer(0, chunk.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(offset..));
        pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..chunk.count, 0, 0..count);
    }
}

fn pipeline(
    gpu: &Gpu,
    world: &World,
    shader: &wgpu::ShaderModule,
    key: Key,
) -> wgpu::RenderPipeline {
    let vertex = wgpu::VertexBufferLayout {
        array_stride: 32,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
    };
    let instance = if key.billboard {
        wgpu::VertexBufferLayout {
            array_stride: (BILLBOARD_FLOATS * 4) as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x2, 9 => Float32x3, 10 => Float32x4, 11 => Float32x4],
        }
    } else {
        wgpu::VertexBufferLayout {
            array_stride: (MESH_FLOATS * 4) as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x2, 9 => Float32x3],
        }
    };
    world.pipeline(
        gpu,
        PipelineDesc {
            label: "vegetation",
            shader,
            vertex: if key.billboard {
                "vs_billboard"
            } else {
                "vs_mesh"
            },
            fragment: "fs",
            buffers: &[Some(vertex), Some(instance)],
            blend: if key.blend {
                Blend::Alpha
            } else {
                Blend::Opaque
            },
            depth_write: key.depth_write,
            depth_compare: wgpu::CompareFunction::Less,
            per_draw: true,
        },
    )
}
