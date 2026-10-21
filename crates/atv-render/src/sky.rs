//! Scene sky (CubeFile tiles), drawn first with depth testing disabled.
use crate::{
    Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::sky::Sky;
use wgpu::util::DeviceExt;

struct Tile {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
    size: u32,
}

pub struct SkyPass {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    tiles: Vec<Tile>,
    smooth: bool,
}

impl SkyPass {
    pub fn new(gpu: &Gpu, world: &World, sky: &Sky, smooth: bool) -> Self {
        // 00125960 and 00125570 emit local UVs spanning .03125...96875.
        let uv = [
            [0.03125, 0.03125],
            [0.96875, 0.03125],
            [0.03125, 0.96875],
            [0.96875, 0.96875],
        ];
        let mut vertices = Vec::with_capacity(sky.tiles.len() * 20);
        let mut indices: Vec<u32> = Vec::with_capacity(sky.tiles.len() * 6);
        for (index, tile) in sky.tiles.iter().enumerate() {
            for (corner, position) in tile.positions.iter().enumerate() {
                vertices.extend(position);
                vertices.extend(uv[corner]);
            }
            let base = index as u32 * 4;
            indices.extend([0, 2, 1, 1, 2, 3].map(|i| base + i));
        }
        let device = &gpu.device;
        let sampler = texture::sampler(gpu, false, 1);
        let tiles = sky
            .tiles
            .iter()
            .map(|tile| {
                let pixels = match (&tile.smooth, smooth) {
                    (Some(smooth), true) => smooth,
                    _ => &tile.image.rgba,
                };
                let texture =
                    texture::upload(gpu, tile.image.width, tile.image.height, pixels, "sky tile");
                let group = world.texture_group(gpu, &texture, &sampler);
                Tile {
                    texture,
                    group,
                    size: tile.image.width,
                }
            })
            .collect();
        let shader = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
        let pipeline = world.pipeline(
            gpu,
            PipelineDesc {
                label: "sky",
                shader: &shader,
                vertex: "vs",
                fragment: "fs",
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
                })],
                blend: Blend::Opaque,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Always,
                per_draw: false,
            },
        );
        Self {
            pipeline,
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sky vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sky indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            tiles,
            smooth: smooth && sky.tiles.iter().all(|t| t.smooth.is_some()),
        }
    }

    /// Preset changes swap the sky between the original decode and the smoothed one.
    pub fn set_smooth(&mut self, gpu: &Gpu, sky: &Sky, smooth: bool) {
        if smooth == self.smooth {
            return;
        }
        self.smooth = smooth && sky.tiles.iter().all(|t| t.smooth.is_some());
        for (tile, source) in self.tiles.iter().zip(&sky.tiles) {
            let pixels = match (&source.smooth, smooth) {
                (Some(smooth), true) => smooth,
                _ => &source.image.rgba,
            };
            texture::write(gpu, &tile.texture, tile.size, tile.size, pixels);
        }
    }

    pub fn smooth(&self) -> bool {
        self.smooth
    }

    pub fn tiles(&self) -> u32 {
        self.tiles.len() as u32
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for (index, tile) in self.tiles.iter().enumerate() {
            pass.set_bind_group(1, &tile.group, &[]);
            let first = index as u32 * 6;
            pass.draw_indexed(first..first + 6, 0, 0..1);
        }
    }
}
