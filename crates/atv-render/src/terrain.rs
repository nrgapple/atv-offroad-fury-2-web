//! Original terrain: base layers per 64-cell tile, detail blend and GS fog.
use crate::{DEPTH_FORMAT, FrameStats, Gpu};
use atv_game::{
    terrain_mesh,
    venue::{Image, Venue},
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_projection: [f32; 16],
    eye: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    detail_scale: [f32; 4],
}

struct Chunk {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    min: [f32; 3],
    max: [f32; 3],
}

pub struct TerrainPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    globals: Globals,
    chunks: Vec<Chunk>,
    visible: Vec<usize>,
}

impl TerrainPass {
    pub fn new(gpu: &Gpu, venue: &Venue, fine_center: Option<[f32; 2]>, samples: u32) -> Self {
        let device = &gpu.device;
        let terrain = &venue.terrain;
        let colors = upload_layers(gpu, &terrain.layers);
        let mapping = device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("terrain mapping"),
                size: wgpu::Extent3d {
                    width: terrain.mapping_width,
                    height: terrain.mapping_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&terrain.mapping),
        );
        let detail = upload_layers(gpu, std::slice::from_ref(&terrain.detail));
        let clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain colors"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let repeat = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain detail"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let fog = venue.scene.fog.as_ref();
        let globals = Globals {
            view_projection: [0.; 16],
            eye: [0.; 4],
            fog_color: {
                let [r, g, b] = venue.scene.fog_color.map(|c| c / 255.);
                [r, g, b, 1.]
            },
            fog: [
                fog.map_or(524288., |f| f.encoded_bias),
                fog.map_or(0., |f| f.coefficient),
                fog.is_some() as u8 as f32,
                terrain.spacing,
            ],
            detail_scale: [
                32. / terrain.detail.width as f32,
                32. / terrain.detail.height as f32,
                0.,
                0.,
            ],
        };
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let texture = |sample_type, view_dimension| wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    texture(
                        wgpu::TextureSampleType::Float { filterable: true },
                        wgpu::TextureViewDimension::D2Array,
                    ),
                ),
                entry(
                    2,
                    texture(
                        wgpu::TextureSampleType::Uint,
                        wgpu::TextureViewDimension::D2,
                    ),
                ),
                entry(
                    3,
                    texture(
                        wgpu::TextureSampleType::Float { filterable: true },
                        wgpu::TextureViewDimension::D2,
                    ),
                ),
                entry(
                    4,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    5,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let array_view = colors.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let mapping_view = mapping.create_view(&Default::default());
        let detail_view = detail.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&mapping_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&detail_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&repeat),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("terrain.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (terrain_mesh::FLOATS_PER_VERTEX * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            // The browser disables culling for the whole scene.
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(gpu.config.format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let chunks = chunks(gpu, terrain, fine_center);
        Self {
            pipeline,
            bind_group,
            uniforms,
            globals,
            chunks,
            visible: Vec::new(),
        }
    }

    /// Moves the fine patch (buildTerrainMesh after the rider leaves it).
    pub fn recenter(&mut self, gpu: &Gpu, venue: &Venue, center: [f32; 2]) {
        self.chunks = chunks(gpu, &venue.terrain, Some(center));
    }

    pub fn prepare(&mut self, gpu: &Gpu, view_projection: &[f32; 16], eye: [f32; 3]) -> FrameStats {
        self.globals.view_projection = *view_projection;
        self.globals.eye = [eye[0], eye[1], eye[2], 1.];
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&self.globals));
        let planes = frustum(view_projection);
        self.visible.clear();
        let mut stats = FrameStats::default();
        for (index, chunk) in self.chunks.iter().enumerate() {
            if outside(&planes, chunk.min, chunk.max) {
                continue;
            }
            self.visible.push(index);
            stats.terrain_draws += 1;
            stats.terrain_triangles += chunk.count / 3;
        }
        stats
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        for &index in &self.visible {
            let chunk = &self.chunks[index];
            pass.set_vertex_buffer(0, chunk.vertices.slice(..));
            pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.count, 0, 0..1);
        }
    }
}

/// Same-size RGBA8 images as one texture (array), with a box-filtered mip
/// chain as WebGL's generateMipmap produces.
fn upload_layers(gpu: &Gpu, images: &[Image]) -> wgpu::Texture {
    let (width, height) = (images[0].width, images[0].height);
    let levels = 32 - width.max(height).leading_zeros();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: images.len() as u32,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, image) in images.iter().enumerate() {
        let (mut w, mut h, mut pixels) = (width, height, image.rgba.clone());
        for level in 0..levels {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            (pixels, w, h) = downsample(&pixels, w, h);
        }
    }
    texture
}

fn downsample(pixels: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let mut sum = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                    sum += pixels[((sy * w + sx) * 4 + c) as usize] as u32;
                }
                out[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// Clip planes (a, b, c, d) of a 0..1 depth view-projection.
fn frustum(m: &[f32; 16]) -> [[f32; 4]; 6] {
    let row = |r: usize| [m[r], m[4 + r], m[8 + r], m[12 + r]];
    let (x, y, z, w) = (row(0), row(1), row(2), row(3));
    let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
    let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    [add(w, x), sub(w, x), add(w, y), sub(w, y), z, sub(w, z)]
}

fn outside(planes: &[[f32; 4]; 6], min: [f32; 3], max: [f32; 3]) -> bool {
    planes.iter().any(|p| {
        let corner = [0, 1, 2].map(|a| if p[a] >= 0. { max[a] } else { min[a] });
        p[0] * corner[0] + p[1] * corner[1] + p[2] * corner[2] + p[3] < 0.
    })
}

fn chunks(gpu: &Gpu, terrain: &atv_game::venue::Terrain, center: Option<[f32; 2]>) -> Vec<Chunk> {
    let device = &gpu.device;
    let patch = center.map(|center| terrain_mesh::patch(terrain, center));
    terrain_mesh::build(terrain, patch)
        .into_iter()
        .map(|chunk| Chunk {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain vertices"),
                contents: bytemuck::cast_slice(&chunk.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain indices"),
                contents: bytemuck::cast_slice(&chunk.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: chunk.indices.len() as u32,
            min: chunk.min,
            max: chunk.max,
        })
        .collect()
}
