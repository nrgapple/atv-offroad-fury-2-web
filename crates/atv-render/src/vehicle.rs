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
    // mud amount (0-1), authored ATV2 reveal (1) or MX linear blend (0)
    mud: [f32; 4],
    // quad paint mode and hue
    paint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Bones {
    world: [f32; 16],
    nodes: [[f32; 16]; BONES],
}

/// A material group's range of the vehicle's shared buffers (one vertex and
/// one index buffer per vehicle, indices rebased: no per-group attribute
/// setup on the GL backend; the same draws in the same order).
struct Chunk {
    first: u32,
    count: u32,
    /// First vertex of the group in the shared buffer (severed index rebase).
    base: u32,
    rider: bool,
    material: wgpu::BindGroup,
    /// Rider groups: CPU indices and vertex nodes for Gorn severing.
    source: Option<(Vec<u32>, Vec<u16>)>,
    /// Indices without the triangles bridging a severed joint.
    cut: Option<(wgpu::Buffer, u32)>,
}

pub struct VehiclePass {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    globals: wgpu::Buffer,
    globals_value: Globals,
    globals_group: wgpu::BindGroup,
    quad: (wgpu::Buffer, wgpu::BindGroup),
    rider: (wgpu::Buffer, wgpu::BindGroup),
    chunks: Vec<Chunk>,
    pub triangles: u32,
    /// Shadow casting (High shadow map) reuses the bone uniforms.
    pub(crate) bones_layout: wgpu::BindGroupLayout,
    /// Quad bind-pose length and width (setVehicle: bounds Z and X).
    pub(crate) footprint: [f32; 2],
    /// Rider node carrying the torso (pelvisNode in game.js).
    pub(crate) pelvis: usize,
    /// Rider node parents, and the severed set the cut indices were built for.
    rider_parents: Vec<i32>,
    severed: Vec<usize>,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
            mud: [0.; 4],
            paint: [0.; 4],
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
        let (mut all_vertices, mut all_indices) = (Vec::<f32>::new(), Vec::<u32>::new());
        let mut triangles = 0;
        for (model, rider) in [(quad, false), (rider, true)] {
            if model.mesh.nodes.len() > BONES {
                return Err("vehicle node count exceeds supported GPU pose capacity".into());
            }
            let mut views = std::collections::HashMap::new();
            let mut mud_views = std::collections::HashMap::new();
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
                let mud = texture.and_then(|t| t.mud.as_ref()).map(|image| {
                    mud_views
                        .entry(name.clone())
                        .or_insert_with(|| upload(gpu, image).create_view(&Default::default()))
                        .clone()
                });
                let flags = [
                    view.is_some() as u8 as f32,
                    texture.is_none_or(|t| t.fog) as u8 as f32,
                    mud.is_some() as u8 as f32,
                    // The paint recolours quad body materials, never the rider.
                    (!rider && atv_game::ride::gear::paintable(name)) as u8 as f32,
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
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                mud.as_ref().unwrap_or(&fallback),
                            ),
                        },
                    ],
                });
                let base = (all_vertices.len() / FLOATS_PER_VERTEX) as u32;
                let vertices = &mut all_vertices;
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
                    first: all_indices.len() as u32,
                    count: group.indices.len() as u32,
                    base,
                    rider,
                    material,
                    source: rider.then(|| {
                        (
                            group.indices.clone(),
                            group.vertices.iter().map(|v| v.node).collect(),
                        )
                    }),
                    cut: None,
                });
                all_indices.extend(group.indices.iter().map(|i| i + base));
            }
        }
        if all_vertices.is_empty() {
            all_vertices.resize(FLOATS_PER_VERTEX, 0.);
            all_indices.push(0);
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
                // Stencil ref 1 marks the player's own quad and rider so the
                // speed-rush pass leaves them crisp (game.js rushMask).
                stencil: {
                    let face = wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Always,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::Replace,
                    };
                    wgpu::StencilState {
                        front: face,
                        back: face,
                        read_mask: 0xff,
                        write_mask: 0xff,
                    }
                },
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
        let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for group in quad.mesh.lods.first().map_or(&[][..], |l| &l.groups[..]) {
            for vertex in &group.vertices {
                for axis in 0..3 {
                    min[axis] = min[axis].min(vertex.position[axis]);
                    max[axis] = max[axis].max(vertex.position[axis]);
                }
            }
        }
        let nodes = &rider.mesh.nodes;
        let pelvis = [&["pelvis"][..], &["hip"], &["torso", "spine"]]
            .iter()
            .find_map(|patterns| {
                nodes.iter().position(|n| {
                    let name = n.name.to_ascii_lowercase();
                    patterns.iter().any(|p| name.contains(p))
                })
            })
            .unwrap_or(0);
        Ok(Self {
            pipeline,
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vehicle vertices"),
                contents: bytemuck::cast_slice(&all_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vehicle indices"),
                contents: bytemuck::cast_slice(&all_indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            globals,
            globals_value,
            globals_group,
            quad: bones("quad bones"),
            rider: bones("rider bones"),
            chunks,
            triangles,
            footprint: [max[2] - min[2], max[0] - min[0]],
            rider_parents: nodes.iter().map(|n| n.parent).collect(),
            severed: Vec::new(),
            pelvis,
            bones_layout,
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

    /// The mud blended over materials with a mud map: the motorcycle's
    /// amount (linear, `mudReveal` off) or the ATV2 dirt reveal.
    pub fn set_mud(&mut self, amount: f32, reveal: bool) {
        self.globals_value.mud = [amount.clamp(0., 1.), reveal as u8 as f32, 0., 0.];
    }

    /// The player's quad paint (mode, hue; `[0, 0]` keeps the livery).
    pub fn set_paint(&mut self, paint: [f32; 2]) {
        self.globals_value.paint = [paint[0], paint[1], 0., 0.];
    }

    pub fn prepare(&mut self, gpu: &Gpu, view_projection: &[f32; 16], eye: [f32; 3]) {
        self.globals_value.view_projection = *view_projection;
        self.globals_value.eye = [eye[0], eye[1], eye[2], 1.];
        gpu.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&self.globals_value));
    }

    /// Gorn: drop rider triangles that bridge a severed joint
    /// (severedIndices in render-support.js), rebuilt only when the set changes.
    pub(crate) fn set_severed(&mut self, gpu: &Gpu, severed: &[usize]) {
        if self.severed == severed {
            return;
        }
        self.severed = severed.to_vec();
        let parents = &self.rider_parents;
        // Each node's component: the nearest severed ancestor (+1), else 0.
        let components: Vec<usize> = (0..parents.len())
            .map(|start| {
                let mut i = start;
                for _ in 0..parents.len() {
                    if severed.contains(&i) {
                        return i + 1;
                    }
                    match usize::try_from(parents[i]) {
                        Ok(parent) if parent < parents.len() => i = parent,
                        _ => return 0,
                    }
                }
                0
            })
            .collect();
        for chunk in &mut self.chunks {
            let Some((indices, nodes)) = &chunk.source else {
                continue;
            };
            if severed.is_empty() {
                chunk.cut = None;
                continue;
            }
            let component = |v: u32| {
                components
                    .get(usize::from(nodes[v as usize]))
                    .copied()
                    .unwrap_or(0)
            };
            let kept: Vec<u32> = indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|t| {
                    component(t[0]) == component(t[1]) && component(t[0]) == component(t[2])
                })
                .flatten()
                .map(|i| i + chunk.base)
                .collect();
            let buffer = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("severed rider indices"),
                    contents: bytemuck::cast_slice(if kept.is_empty() {
                        &[0u32; 3][..]
                    } else {
                        &kept
                    }),
                    usage: wgpu::BufferUsages::INDEX,
                });
            chunk.cut = Some((buffer, kept.len() as u32));
        }
    }

    /// Depth-only casting into the High shadow map: bones at group 1, the
    /// caller's light uniforms at group 0.
    pub(crate) fn draw_casters(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_chunks(pass, false);
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_group, &[]);
        self.draw_chunks(pass, true);
    }

    /// The groups in order from the shared buffers; bones rebind only when
    /// the group switches between quad and rider.
    fn draw_chunks(&self, pass: &mut wgpu::RenderPass<'_>, materials: bool) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        let mut main_bound = false;
        let mut bound_rider = None;
        for chunk in &self.chunks {
            if bound_rider != Some(chunk.rider) {
                let bones = if chunk.rider {
                    &self.rider.1
                } else {
                    &self.quad.1
                };
                pass.set_bind_group(1, bones, &[]);
                bound_rider = Some(chunk.rider);
            }
            if materials {
                pass.set_bind_group(2, &chunk.material, &[]);
            }
            match &chunk.cut {
                Some((cut, count)) => {
                    pass.set_index_buffer(cut.slice(..), wgpu::IndexFormat::Uint32);
                    main_bound = false;
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
                None => {
                    if !main_bound {
                        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
                        main_bound = true;
                    }
                    pass.draw_indexed(chunk.first..chunk.first + chunk.count, 0, 0..1);
                }
            }
        }
    }
}

fn upload(gpu: &Gpu, image: &Image) -> wgpu::Texture {
    crate::texture::upload(
        gpu,
        image.width.max(1),
        image.height.max(1),
        &image.rgba,
        "vehicle texture",
    )
}
