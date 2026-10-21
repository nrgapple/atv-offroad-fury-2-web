//! Scene sky (CubeFile tiles), drawn first with depth testing disabled.
use crate::{
    Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::sky::Sky;
use wgpu::util::DeviceExt;

/// Every tile is one layer of a texture array (all shipped skies use one
/// tile size), so the whole sky is one draw instead of one per tile: the
/// same texels, filtering and order, without 64 texture rebinds per frame.
pub struct SkyPass {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
    size: u32,
    tiles: u32,
    smooth: bool,
}

fn write_layers(gpu: &Gpu, texture: &wgpu::Texture, sky: &Sky, smooth: bool) {
    for (layer, tile) in sky.tiles.iter().enumerate() {
        let pixels = match (&tile.smooth, smooth) {
            (Some(smooth), true) => smooth,
            _ => &tile.image.rgba,
        };
        texture::write_layer(
            gpu,
            texture,
            layer as u32,
            tile.image.width,
            tile.image.height,
            pixels,
        );
    }
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
        let size = sky.tiles.first().map_or(1, |t| t.image.width);
        if sky
            .tiles
            .iter()
            .any(|t| t.image.width != size || t.image.height != size)
        {
            eprintln!("sky: tiles of different sizes are not supported; drawing the first size");
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky tiles"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: texture::array_layers(sky.tiles.len().max(2) as u32),
            },
            mip_level_count: texture::mip_levels(size, size),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_layers(gpu, &texture, sky, smooth);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky tiles"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky tiles"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
        let pipeline = world.pipeline_with(
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
            &layout,
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
            texture,
            group,
            size,
            tiles: sky.tiles.len() as u32,
            smooth: smooth && sky.tiles.iter().all(|t| t.smooth.is_some()),
        }
    }

    /// Preset changes swap the sky between the original decode and the smoothed one.
    pub fn set_smooth(&mut self, gpu: &Gpu, sky: &Sky, smooth: bool) {
        if smooth == self.smooth {
            return;
        }
        self.smooth = smooth && sky.tiles.iter().all(|t| t.smooth.is_some());
        write_layers(gpu, &self.texture, sky, smooth);
        let _ = self.size;
    }

    pub fn smooth(&self) -> bool {
        self.smooth
    }

    pub fn tiles(&self) -> u32 {
        self.tiles
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_bind_group(1, &self.group, &[]);
        pass.draw_indexed(0..self.tiles * 6, 0, 0..1);
    }
}
