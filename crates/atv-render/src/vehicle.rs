//! The player's quad and rider: rigid-node meshes posed by the simulation's
//! linkage and skeleton matrices, with original textures, the browser's
//! provisional vehicle lighting and GS fog (vehicleChunks in web/game.js).
use crate::{DEPTH_FORMAT, Gpu};
use atv_game::{
    ride::{Model, Pose},
    venue::{Image, Venue},
};
use wgpu::util::DeviceExt;

/// The browser shader's `quadBones[32]` capacity.
const BONES: usize = 32;
const FLOATS_PER_VERTEX: usize = 9;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_projection: [f32; 16],
    eye: [f32; 4],
    fog_color: [f32; 4],
    // encoded bias, distance coefficient, enabled
    fog: [f32; 4],
    ambient: [f32; 4],
    sun_color: [f32; 4],
    sun_direction: [f32; 4],
    surface: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Bones {
    world: [f32; 16],
    nodes: [[f32; 16]; BONES],
}

struct Chunk {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    rider: bool,
    material: wgpu::BindGroup,
}

pub struct VehiclePass {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_value: Globals,
    globals_group: wgpu::BindGroup,
    quad: (wgpu::Buffer, wgpu::BindGroup),
    rider: (wgpu::Buffer, wgpu::BindGroup),
    chunks: Vec<Chunk>,
    pub triangles: u32,
}

/// The browser's inverseDirection: bind-space normal into node space.
fn inverse_direction(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
    let (a, b, c) = ([m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]);
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let determinant = dot(a, cross(b, c));
    if determinant.abs() < 1e-10 {
        return v;
    }
    [cross(b, c), cross(c, a), cross(a, b)].map(|row| dot(row, v) / determinant)
}

/// Scene lights as the browser reads them for lit models.
fn lights(venue: &Venue) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let lights = &venue.scene.lights;
    let ambient = lights
        .iter()
        .find(|l| l.kind == "Ambient")
        .map_or([0.38; 3], |l| l.color.map(|c| c / 255.));
    let sun = lights.iter().find(|l| l.kind == "Directional");
    let sun_color = sun.map_or([0.8; 3], |l| l.color.map(|c| c / 255.));
    let direction = match sun {
        Some(l) if l.look.is_some() => l.look.unwrap().map(|v| -v),
        Some(l) if l.position.is_some() && l.target.is_some() => {
            let (p, t) = (l.position.unwrap(), l.target.unwrap());
            [p[0] - t[0], p[1] - t[1], p[2] - t[2]]
        }
        _ => [-0.6, 1., 0.35],
    };
    (ambient, sun_color, direction)
}

impl VehiclePass {
    pub fn new(
        gpu: &Gpu,
        venue: &Venue,
        quad: &Model,
        rider: &Model,
        samples: u32,
    ) -> Result<Self, String> {
        let device = &gpu.device;
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vehicle globals"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let bones_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vehicle bones"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vehicle material"),
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let fog = venue.scene.fog.as_ref();
        let (ambient, sun_color, sun_direction) = lights(venue);
        let [fr, fg, fb] = venue.scene.fog_color.map(|c| c / 255.);
        let globals_value = Globals {
            view_projection: [0.; 16],
            eye: [0.; 4],
            fog_color: [fr, fg, fb, 1.],
            fog: [
                fog.map_or(524288., |f| f.encoded_bias),
                fog.map_or(0., |f| f.coefficient),
                fog.is_some() as u8 as f32,
                0.,
            ],
            ambient: [ambient[0], ambient[1], ambient[2], 0.],
            sun_color: [sun_color[0], sun_color[1], sun_color[2], 0.],
            sun_direction: [sun_direction[0], sun_direction[1], sun_direction[2], 0.],
            // Untextured vehicle groups keep the scenery pass's surface colour.
            surface: [0.68, 0.70, 0.73, 1.],
        };
        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vehicle globals"),
            contents: bytemuck::bytes_of(&globals_value),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vehicle globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let bones = |label| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Bones>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &bones_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            (buffer, group)
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vehicle material"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = Image {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let fallback = upload(gpu, &white).create_view(&Default::default());
        let mut chunks = Vec::new();
        let mut triangles = 0;
        for (model, rider) in [(quad, false), (rider, true)] {
            if model.mesh.nodes.len() > BONES {
                return Err("vehicle node count exceeds supported GPU pose capacity".into());
            }
            let mut views = std::collections::HashMap::new();
            for group in &model.mesh.lods.first().ok_or("mesh has no LOD")?.groups {
                let name = model
                    .mesh
                    .materials
                    .get(group.material as usize)
                    .ok_or("mesh material index out of range")?;
                let texture = model.textures.get(name);
                let image = texture.and_then(|t| t.image.as_ref());
                let view = image.map(|image| {
                    views
                        .entry(name.clone())
                        .or_insert_with(|| upload(gpu, image).create_view(&Default::default()))
                        .clone()
                });
                let flags = [
                    view.is_some() as u8 as f32,
                    texture.is_none_or(|t| t.fog) as u8 as f32,
                    0.,
                    0.,
                ];
                let flags = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("vehicle material flags"),
                    contents: bytemuck::cast_slice(&flags),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("vehicle material"),
                    layout: &material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: flags.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                view.as_ref().unwrap_or(&fallback),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                let mut vertices = Vec::with_capacity(group.vertices.len() * FLOATS_PER_VERTEX);
                for vertex in &group.vertices {
                    let node = model
                        .mesh
                        .nodes
                        .get(vertex.node as usize)
                        .ok_or("vehicle vertex node is missing")?;
                    vertices.extend_from_slice(&vertex.local_position);
                    vertices
                        .extend_from_slice(&inverse_direction(&node.world_matrix, vertex.normal));
                    vertices.extend_from_slice(&vertex.uv);
                    vertices.push(vertex.node as f32);
                }
                if group.indices.is_empty() {
                    continue;
                }
                triangles += group.indices.len() as u32 / 3;
                chunks.push(Chunk {
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("vehicle vertices"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("vehicle indices"),
                        contents: bytemuck::cast_slice(&group.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    count: group.indices.len() as u32,
                    rider,
                    material,
                });
            }
        }
        let shader = device.create_shader_module(wgpu::include_wgsl!("vehicle.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vehicle"),
            bind_group_layouts: &[
                Some(&globals_layout),
                Some(&bones_layout),
                Some(&material_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vehicle"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS_PER_VERTEX * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32
                    ],
                })],
            },
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
        Ok(Self {
            pipeline,
            globals,
            globals_value,
            globals_group,
            quad: bones("quad bones"),
            rider: bones("rider bones"),
            chunks,
            triangles,
        })
    }

    pub fn pose(&self, gpu: &Gpu, pose: &Pose) {
        for ((buffer, _), world, nodes) in [
            (&self.quad, pose.quad_world, &pose.quad_nodes),
            (&self.rider, pose.rider_world, &pose.rider_nodes),
        ] {
            let mut bones = Bones {
                world,
                nodes: [[0.; 16]; BONES],
            };
            for (slot, matrix) in bones.nodes.iter_mut().zip(nodes) {
                *slot = *matrix;
            }
            gpu.queue
                .write_buffer(buffer, 0, bytemuck::bytes_of(&bones));
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, view_projection: &[f32; 16], eye: [f32; 3]) {
        self.globals_value.view_projection = *view_projection;
        self.globals_value.eye = [eye[0], eye[1], eye[2], 1.];
        gpu.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&self.globals_value));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_group, &[]);
        for chunk in &self.chunks {
            let bones = if chunk.rider {
                &self.rider.1
            } else {
                &self.quad.1
            };
            pass.set_bind_group(1, bones, &[]);
            pass.set_bind_group(2, &chunk.material, &[]);
            pass.set_vertex_buffer(0, chunk.vertices.slice(..));
            pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.count, 0, 0..1);
        }
    }
}

/// RGBA8 texture with a box-filtered mip chain, as WebGL's generateMipmap.
fn upload(gpu: &Gpu, image: &Image) -> wgpu::Texture {
    let (width, height) = (image.width.max(1), image.height.max(1));
    let levels = 32 - width.max(height).leading_zeros();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vehicle texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let (mut w, mut h, mut pixels) = (width, height, image.rgba.clone());
    for level in 0..levels {
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
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
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                        sum += pixels[((sy * w + sx) * 4 + c) as usize] as u32;
                    }
                    next[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        (pixels, w, h) = (next, nw, nh);
    }
    texture
}
