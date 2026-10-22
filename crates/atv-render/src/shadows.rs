//! Authored vehicle shadows (the original casts none): Enhanced/High soft
//! contact blobs (web/vehicle-shadows.js) and the High vehicle-only sun shadow
//! map (web/shadow-map.js) that terrain and scenery receive.
use crate::{Gpu, vehicle::VehiclePass, world::World};
use wgpu::util::DeviceExt;

pub const SHADOW_RADIUS: f32 = 24.;
pub const SHADOW_DEPTH: f32 = 160.;
const GRID: usize = 7;
const CELLS: usize = GRID - 1;
const FLOATS: usize = 6;
const CAPACITY: usize = 12;

type V = [f32; 3];
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn unit(v: V) -> V {
    let l = dot(v, v).sqrt();
    let l = if l > 0. { l } else { 1. };
    v.map(|x| x / l)
}

/// Light matrices around `center` looking along the light (lightMatrices):
/// the centre snaps to whole texels. Returns (clip for a 0..1 depth target,
/// world to [0,1] shadow texture with V down as wgpu samples it).
pub fn light_matrices(center: V, to_sun: V, size: u32) -> ([f32; 16], [f32; 16]) {
    let mut sun = unit(to_sun);
    if sun[1].is_nan() || sun[1] <= 0.2 {
        sun = unit([sun[0], 0.2, sun[2]]);
    }
    let forward = sun.map(|v| -v);
    let right = unit(cross(
        forward,
        if forward[1].abs() > 0.99 {
            [0., 0., 1.]
        } else {
            [0., 1., 0.]
        },
    ));
    let up = cross(right, forward);
    let texel = 2. * SHADOW_RADIUS / size as f32;
    let snap = |v: f32| (v / texel).round() * texel;
    let (cr, cu, cf) = (
        snap(dot(center, right)),
        snap(dot(center, up)),
        dot(center, forward),
    );
    let at: V = std::array::from_fn(|i| right[i] * cr + up[i] * cu + forward[i] * cf);
    let eye: V = std::array::from_fn(|i| at[i] - forward[i] * SHADOW_DEPTH / 2.);
    let view = [
        right[0],
        up[0],
        -forward[0],
        0.,
        right[1],
        up[1],
        -forward[1],
        0.,
        right[2],
        up[2],
        -forward[2],
        0.,
        -dot(right, eye),
        -dot(up, eye),
        dot(forward, eye),
        1.,
    ];
    let s = 1. / SHADOW_RADIUS;
    // GL ortho z = -2/(far-near) z - (far+near)/(far-near), then (z + 1) / 2.
    let (near, far) = (0., SHADOW_DEPTH);
    let gl = [
        s,
        0.,
        0.,
        0.,
        0.,
        s,
        0.,
        0.,
        0.,
        0.,
        -2. / (far - near),
        0.,
        0.,
        0.,
        -(far + near) / (far - near),
        1.,
    ];
    let depth = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0.5, 0., 0., 0., 0.5, 1.,
    ];
    let clip = crate::camera::multiply(&crate::camera::multiply(&depth, &gl), &view);
    let bias = [
        0.5, 0., 0., 0., 0., -0.5, 0., 0., 0., 0., 1., 0., 0.5, 0.5, 0., 1.,
    ];
    (clip, crate::camera::multiply(&bias, &clip))
}

pub struct ShadowMap {
    pipeline: wgpu::RenderPipeline,
    light: wgpu::Buffer,
    light_group: wgpu::BindGroup,
    size: u32,
}

impl ShadowMap {
    pub fn new(gpu: &Gpu, world: &World, vehicle: &VehiclePass) -> Self {
        let device = &gpu.device;
        let light = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow light"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow light"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let light_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow light"),
            layout: &light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shadow_cast.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow casters"),
            bind_group_layouts: &[Some(&light_layout), Some(&vehicle.bones_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow casters"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 36,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 32,
                            shader_location: 3,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // polygonOffset(2, 4).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::world::SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 4,
                    slope_scale: 2.,
                    clamp: 0.,
                },
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            light,
            light_group,
            size: world.shadow_size,
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// Renders the casters into the world's shadow map with light `clip`.
    /// Every caster shares the vehicle pipeline's bone layout
    /// (`VehicleShared`): the player, then nearby race CPUs and other riders.
    pub fn render(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        world: &World,
        casters: &[&VehiclePass],
        clip: &[f32; 16],
    ) {
        gpu.queue
            .write_buffer(&self.light, 0, bytemuck::cast_slice(clip));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vehicle shadow map"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &world.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.light_group, &[]);
        for vehicle in casters {
            vehicle.draw_casters(&mut pass);
        }
    }
}

/// One caster's blob (shadowFootprint).
pub struct Footprint {
    pub center: [f32; 2],
    pub forward: [f32; 2],
    pub right: [f32; 2],
    pub half_length: f32,
    pub half_width: f32,
    pub alpha: f32,
}

/// Footprint from a caster's world matrix; `lift` is its clearance above its
/// resting height: the blob softens, spreads and fades as it rises.
pub fn footprint(
    m: &[f32; 16],
    length: f32,
    width: f32,
    lift: f32,
    distance: f32,
    strength: f32,
) -> Footprint {
    let (fx, fz, rx, rz) = (m[8], m[10], m[0], m[2]);
    let f = fx.hypot(fz);
    let r = rx.hypot(rz);
    let forward = if f > 0.2 {
        [fx / f, fz / f]
    } else if r > 0.2 {
        [-rz / r, rx / r]
    } else {
        [0., 1.]
    };
    let height = lift.max(0.);
    let spread = 1. + height.min(12.) * 0.06;
    let fade = (1. - height / 12.).max(0.) * ((240. - distance) / 90.).clamp(0., 1.);
    Footprint {
        center: [m[12], m[14]],
        forward,
        right: [forward[1], -forward[0]],
        half_length: (length * 0.66).max(0.5) * spread,
        half_width: (width * 0.8).max(0.5) * spread,
        alpha: strength * fade * fade,
    }
}

pub struct ContactShadows {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    data: Vec<f32>,
    casters: u32,
}

impl ContactShadows {
    pub fn new(gpu: &Gpu, world: &World) -> Self {
        let device = &gpu.device;
        let mut indices: Vec<u32> = Vec::with_capacity(CAPACITY * CELLS * CELLS * 6);
        for c in 0..CAPACITY {
            for z in 0..CELLS {
                for x in 0..CELLS {
                    let a = (c * GRID * GRID + z * GRID + x) as u32;
                    let (b, d) = (a + 1, a + GRID as u32);
                    indices.extend([a, d, b, b, d, d + 1]);
                }
            }
        }
        let shader = device.create_shader_module(wgpu::include_wgsl!("contact_shadow.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("contact shadows"),
            bind_group_layouts: &[Some(&world.globals_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("contact shadows"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // depthFunc(LEQUAL), no writes, polygonOffset(-1, -2).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,
                    slope_scale: -1.,
                    clamp: 0.,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: world.samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                // blendFunc(ZERO, ONE_MINUS_SRC_ALPHA): darken what is below.
                targets: &[Some(wgpu::ColorTargetState {
                    format: world.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("contact shadow grid"),
                size: (CAPACITY * GRID * GRID * FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("contact shadow indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            data: Vec::with_capacity(CAPACITY * GRID * GRID * FLOATS),
            casters: 0,
        }
    }

    /// Drapes each footprint over the visible ground `height(x, z)`.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        footprints: &[Footprint],
        height: impl Fn(f32, f32) -> f32,
    ) -> u32 {
        self.data.clear();
        let mut count = 0;
        for p in footprints {
            if count >= CAPACITY || p.alpha.is_nan() || p.alpha <= 0.01 {
                continue;
            }
            for j in 0..GRID {
                for i in 0..GRID {
                    let u = i as f32 / CELLS as f32 * 2. - 1.;
                    let v = j as f32 / CELLS as f32 * 2. - 1.;
                    let x = p.center[0]
                        + p.right[0] * u * p.half_width
                        + p.forward[0] * v * p.half_length;
                    let z = p.center[1]
                        + p.right[1] * u * p.half_width
                        + p.forward[1] * v * p.half_length;
                    let y = height(x, z);
                    let y = if y.is_finite() { y + 0.05 } else { -1e6 };
                    self.data.extend([x, y, z, u, v, p.alpha]);
                }
            }
            count += 1;
        }
        self.casters = count as u32;
        if count > 0 {
            gpu.queue
                .write_buffer(&self.vertices, 0, bytemuck::cast_slice(&self.data));
        }
        self.casters
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        if self.casters == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.casters * (CELLS * CELLS * 6) as u32, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &[f32; 16], p: V) -> [f32; 4] {
        std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
    }

    #[test]
    fn light_box_centres_the_caster_in_texture_space() {
        let center = [100., 20., 200.];
        let (clip, texture) = light_matrices(center, [-0.6, 1., 0.35], 2048);
        let s = apply(&texture, center);
        assert!(
            (s[0] - 0.5).abs() < 0.001 && (s[1] - 0.5).abs() < 0.001,
            "{s:?}"
        );
        let c = apply(&clip, center);
        assert!((c[2] - 0.5).abs() < 1e-4, "half the light depth: {c:?}");
    }

    #[test]
    fn footprint_spreads_and_fades_with_lift_and_distance() {
        let m = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 5., 0., 7., 1.,
        ];
        let ground = footprint(&m, 6., 4., 0., 10., 0.6);
        assert_eq!(ground.center, [5., 7.]);
        assert!((ground.alpha - 0.6).abs() < 1e-6);
        let high = footprint(&m, 6., 4., 6., 10., 0.6);
        assert!(high.half_length > ground.half_length && high.alpha < ground.alpha);
        assert_eq!(footprint(&m, 6., 4., 0., 240., 0.6).alpha, 0.);
    }
}
