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
    shadow_matrix: [f32; 16],
    shadow: [f32; 4],
}

/// One tile's range of the shared index buffer.
struct Chunk {
    first: u32,
    count: u32,
    min: [f32; 3],
    max: [f32; 3],
}

/// Every tile in one vertex and one index buffer (indices rebased), so the
/// GL backend specifies the vertex attributes once per frame instead of per
/// tile: the same draws, triangles and order.
struct Mesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    chunks: Vec<Chunk>,
    /// Fine patches drawn (split screen keeps one per player).
    patches: usize,
}

/// The texture views the terrain bind group holds.
struct Views {
    array: wgpu::TextureView,
    mapping: wgpu::TextureView,
    detail: wgpu::TextureView,
}

pub struct TerrainPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    views: Views,
    uniforms: wgpu::Buffer,
    globals: Globals,
    mesh: Mesh,
    /// Index ranges of this frame's visible tiles.
    ranges: Vec<std::ops::Range<u32>>,
}

/// The terrain bind group: textures, samplers at `anisotropy` (the browser's
/// anisotropic() on the atlas and detail map) and the vehicle shadow map.
fn bind(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    views: &Views,
    anisotropy: u16,
    shadow: (&wgpu::TextureView, &wgpu::Sampler),
) -> wgpu::BindGroup {
    let device = &gpu.device;
    let clamp = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("terrain colors"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        // Enhanced/High: the browser's anisotropic() on the terrain atlas.
        anisotropy_clamp: anisotropy.max(1),
        ..Default::default()
    });
    let repeat = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("terrain detail"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: anisotropy.max(1),
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("terrain"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&views.array),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&views.mapping),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&views.detail),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&clamp),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&repeat),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(shadow.0),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(shadow.1),
            },
        ],
    })
}

impl TerrainPass {
    /// `shadow` is the High vehicle shadow map and its comparison sampler.
    pub fn new(
        gpu: &Gpu,
        venue: &Venue,
        fine_center: Option<[f32; 2]>,
        samples: u32,
        shadow: (&wgpu::TextureView, &wgpu::Sampler),
        anisotropy: u16,
    ) -> Self {
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
            shadow_matrix: [0.; 16],
            shadow: [0.; 4],
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
                entry(
                    6,
                    texture(
                        wgpu::TextureSampleType::Depth,
                        wgpu::TextureViewDimension::D2,
                    ),
                ),
                entry(
                    7,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
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
        let views = Views {
            array: array_view,
            mapping: mapping_view,
            detail: detail_view,
        };
        let bind_group = bind(gpu, &layout, &uniforms, &views, anisotropy, shadow);
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
        let mesh = mesh(gpu, terrain, fine_center);
        Self {
            pipeline,
            bind_group,
            layout,
            views,
            uniforms,
            globals,
            mesh,
            ranges: Vec::new(),
        }
    }

    /// applyAnisotropy, or a resized shadow map: the bind group again.
    pub fn rebind(
        &mut self,
        gpu: &Gpu,
        anisotropy: u16,
        shadow: (&wgpu::TextureView, &wgpu::Sampler),
    ) {
        self.bind_group = bind(
            gpu,
            &self.layout,
            &self.uniforms,
            &self.views,
            anisotropy,
            shadow,
        );
    }

    /// Moves the fine patch (buildTerrainMesh after the rider leaves it).
    pub fn recenter(&mut self, gpu: &Gpu, venue: &Venue, center: [f32; 2]) {
        self.mesh = mesh(gpu, &venue.terrain, Some(center));
    }

    /// Split screen: a fine patch around each local player
    /// (`terrain_mesh::disjoint`).
    pub fn recenter_many(&mut self, gpu: &Gpu, venue: &Venue, centers: &[[f32; 2]]) {
        let patches = centers
            .iter()
            .map(|c| terrain_mesh::patch(&venue.terrain, *c))
            .collect();
        let patches = terrain_mesh::disjoint(patches);
        self.mesh = upload(
            gpu,
            terrain_mesh::build_patches(&venue.terrain, &patches),
            patches.len(),
        );
    }

    /// Fine patches in the drawn mesh.
    pub fn patches(&self) -> usize {
        self.mesh.patches
    }

    /// The High vehicle shadow map this frame (shadowReceive), or none.
    pub fn set_shadow(&mut self, matrix: Option<&[f32; 16]>, texel: f32) {
        match matrix {
            Some(matrix) => {
                self.globals.shadow_matrix = *matrix;
                self.globals.shadow = [1., texel, 0., 0.];
            }
            None => self.globals.shadow = [0.; 4],
        }
    }

    /// Longest chunk bounds diagonal (boundsReach in render-support.js).
    pub fn reach(&self) -> f32 {
        self.mesh
            .chunks
            .iter()
            .map(|c| {
                let d: [f32; 3] = std::array::from_fn(|a| c.max[a] - c.min[a]);
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            })
            .fold(0., f32::max)
    }

    /// `cull` holds the browser's fog-end clip planes (crate::cull); the depth
    /// range of `view_projection` extends past them.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        view_projection: &[f32; 16],
        cull: &[[f32; 4]],
        eye: [f32; 3],
    ) -> FrameStats {
        self.globals.view_projection = *view_projection;
        self.globals.eye = [eye[0], eye[1], eye[2], 1.];
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&self.globals));
        self.ranges.clear();
        let mut stats = FrameStats::default();
        for chunk in &self.mesh.chunks {
            if crate::cull::outside(cull, chunk.min, chunk.max) {
                continue;
            }
            // One draw per tile: merging adjacent ranges into one draw
            // measurably changes Metal's output (a few hundred far pixels).
            self.ranges.push(chunk.first..chunk.first + chunk.count);
            stats.terrain_triangles += chunk.count / 3;
        }
        stats.terrain_draws = self.ranges.len() as u32;
        stats
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.mesh.vertices.slice(..));
        pass.set_index_buffer(self.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
        for range in &self.ranges {
            pass.draw_indexed(range.clone(), 0, 0..1);
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
            depth_or_array_layers: crate::texture::array_layers(images.len() as u32),
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

fn mesh(gpu: &Gpu, terrain: &atv_game::venue::Terrain, center: Option<[f32; 2]>) -> Mesh {
    let patch = center.map(|center| terrain_mesh::patch(terrain, center));
    upload(
        gpu,
        terrain_mesh::build(terrain, patch),
        usize::from(patch.is_some()),
    )
}

fn upload(gpu: &Gpu, built: Vec<terrain_mesh::Chunk>, patches: usize) -> Mesh {
    let device = &gpu.device;
    let (mut vertices, mut indices, mut chunks) = (Vec::new(), Vec::new(), Vec::new());
    for chunk in &built {
        let base = (vertices.len() / terrain_mesh::FLOATS_PER_VERTEX) as u32;
        chunks.push(Chunk {
            first: indices.len() as u32,
            count: chunk.indices.len() as u32,
            min: chunk.min,
            max: chunk.max,
        });
        vertices.extend_from_slice(&chunk.vertices);
        indices.extend(chunk.indices.iter().map(|i| i + base));
    }
    Mesh {
        vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        chunks,
        patches,
    }
}
