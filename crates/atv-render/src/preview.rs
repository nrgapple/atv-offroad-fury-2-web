//! Menu model previews (frontend.js `ModelView`): one rigid-node mesh posed
//! by its node matrices, turned about its centre and fitted to a transparent
//! texture with the browser's studio lighting and grade. The UI draws the
//! texture in its model slot.
use atv_core::mesh::Mesh;
use wgpu::util::DeviceExt;

/// The browser shader's `bones[32]` capacity.
const BONES: usize = 32;
const FLOATS_PER_VERTEX: usize = 9;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
/// The UI's image textures (`atv_ui::Renderer`).
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A decoded material texture; `cutout` discards texels below alpha 0.15
/// (the original UI rider textures keep a reflection mask in alpha instead).
pub struct PreviewTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub cutout: bool,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    bones: [[f32; 16]; BONES],
    // centre xyz, yaw
    center: [f32; 4],
    // extent x, y, cutout (per group: unused here), 0
    extent: [f32; 4],
}

struct Group {
    vertices: wgpu::Buffer,
    count: u32,
    material: wgpu::BindGroup,
}

pub struct ModelPreview {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    groups: Vec<Group>,
    pose: Vec<[f32; 16]>,
    center: [f32; 3],
    dimensions: [f32; 3],
    /// Turn about the vertical axis (browser `-this.yaw`).
    pub yaw: f32,
    targets: Option<((u32, u32), wgpu::TextureView, wgpu::TextureView)>,
}

/// The browser's localNormal: a bind-space normal into node space.
fn local_normal(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
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

fn transform(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|a| m[a] * p[0] + m[a + 4] * p[1] + m[a + 8] * p[2] + m[a + 12])
}

impl ModelPreview {
    /// `pose`: node world matrices (the mesh bind pose when None).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        textures: &dyn Fn(&str) -> Option<PreviewTexture>,
        pose: Option<&[[f32; 16]]>,
    ) -> Result<Self, String> {
        if mesh.nodes.len() > BONES {
            return Err("preview mesh has too many nodes".into());
        }
        let pose: Vec<[f32; 16]> = match pose {
            Some(p) => p.to_vec(),
            None => mesh.nodes.iter().map(|n| n.world_matrix).collect(),
        };
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
            label: Some("preview globals"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("preview material"),
            entries: &[
                uniform(0, wgpu::ShaderStages::FRAGMENT),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("preview"),
            source: wgpu::ShaderSource::Wgsl(include_str!("preview.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("preview"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("preview"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS_PER_VERTEX * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let upload = |t: &PreviewTexture| {
            device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("preview texture"),
                    size: wgpu::Extent3d {
                        width: t.width.max(1),
                        height: t.height.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &t.rgba,
            )
        };
        let fallback = PreviewTexture {
            width: 1,
            height: 1,
            rgba: vec![185, 185, 185, 255],
            cutout: true,
        };
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut groups = Vec::new();
        let lod = mesh.lods.first().ok_or("preview mesh has no LOD")?;
        let mut cache: Vec<(String, wgpu::Texture, bool)> = Vec::new();
        for group in &lod.groups {
            let mut values = Vec::with_capacity(group.indices.len() * FLOATS_PER_VERTEX);
            for &index in &group.indices {
                let v = group
                    .vertices
                    .get(index as usize)
                    .ok_or("preview index out of range")?;
                let node = mesh
                    .nodes
                    .get(v.node as usize)
                    .ok_or("preview vertex node out of range")?;
                values.extend_from_slice(&v.local_position);
                values.extend_from_slice(&local_normal(&node.world_matrix, v.normal));
                values.extend_from_slice(&v.uv);
                values.push(f32::from(v.node));
                if let Some(matrix) = pose.get(v.node as usize) {
                    let p = transform(matrix, v.local_position);
                    for a in 0..3 {
                        min[a] = min[a].min(p[a]);
                        max[a] = max[a].max(p[a]);
                    }
                }
            }
            if values.is_empty() {
                continue;
            }
            let name = mesh
                .materials
                .get(group.material as usize)
                .cloned()
                .unwrap_or_default();
            let at = match cache.iter().position(|(n, ..)| *n == name) {
                Some(at) => at,
                None => {
                    let decoded = textures(&name);
                    let t = decoded.as_ref().unwrap_or(&fallback);
                    cache.push((name.clone(), upload(t), decoded.is_none() || t.cutout));
                    cache.len() - 1
                }
            };
            let (_, texture, cutout) = &cache[at];
            let flags = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("preview material flags"),
                contents: bytemuck::cast_slice(&[if *cutout { 1f32 } else { 0. }, 0., 0., 0.]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("preview material"),
                layout: &material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: flags.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            groups.push(Group {
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview vertices"),
                    contents: bytemuck::cast_slice(&values),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                count: (values.len() / FLOATS_PER_VERTEX) as u32,
                material,
            });
        }
        if groups.is_empty() || !min[0].is_finite() {
            return Err("preview mesh has no triangles".into());
        }
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        Ok(Self {
            pipeline,
            globals,
            globals_group,
            groups,
            pose,
            center: [0, 1, 2].map(|a| (min[a] + max[a]) / 2.),
            dimensions: [0, 1, 2].map(|a| max[a] - min[a]),
            yaw: 0.6,
            targets: None,
        })
    }

    /// New node matrices (the menu rider's idle animation).
    pub fn set_pose(&mut self, pose: &[[f32; 16]]) {
        self.pose = pose.to_vec();
    }

    /// Sizes the passes for a `width` x `height` target and uploads the
    /// pose and fit.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if self
            .targets
            .as_ref()
            .is_none_or(|(size, ..)| *size != (width, height))
        {
            let make = |format, label| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 4,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            self.targets = Some((
                (width, height),
                make(FORMAT, "preview msaa"),
                make(DEPTH, "preview depth"),
            ));
        }
        // Fit height and horizontal turn radius separately (ModelView.render).
        let horizontal = self.dimensions[0].hypot(self.dimensions[2]) / 2.;
        let aspect = width as f32 / height as f32;
        let (sin, cos) = (0.247_403_96_f32, 0.968_912_4_f32); // sin/cos(0.25)
        let vertical = self.dimensions[1] * cos / 2. + horizontal * sin;
        let extent = vertical.max(horizontal / aspect) * 1.12;
        let mut bones = [[0f32; 16]; BONES];
        for (slot, matrix) in bones.iter_mut().zip(&self.pose) {
            *slot = *matrix;
        }
        let globals = Globals {
            bones,
            center: [self.center[0], self.center[1], self.center[2], self.yaw],
            extent: [extent * aspect, extent, 0., 0.],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    /// Draws into `target` (a `FORMAT` texture of the prepared size),
    /// cleared to transparent.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some((_, msaa, depth)) = &self.targets else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: msaa,
                depth_slice: None,
                resolve_target: Some(target),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_group, &[]);
        for group in &self.groups {
            pass.set_bind_group(1, &group.material, &[]);
            pass.set_vertex_buffer(0, group.vertices.slice(..));
            pass.draw(0..group.count, 0..1);
        }
    }
}
