//! Remastered world shadows (authored; the original casts none): one sun
//! shadow map around the rider that the terrain, static scene models and
//! plants cast into and the terrain and scenery receive. It is cached: the
//! casters do not move, so the map is drawn again only once the rider has
//! travelled a good part of its reach.
use crate::{
    Gpu, scenery::SceneryPass, shadows, terrain::TerrainPass, vegetation::VegetationPass,
    world::World,
};
use atv_core::vegetation_render::MESH_FLOATS;

/// Half-width of the map on the ground, feet.
pub const RADIUS: f32 = 360.;
/// The light box's length along the sun: long enough for hills between the
/// rider and a low sun.
const DEPTH: f32 = 4. * RADIUS;
/// The map is drawn again once the rider is this far from its centre; the
/// receivers fade the map out over its last few percent.
const DRIFT: f32 = 0.3 * RADIUS;
/// Scene models wider than this cast nothing (backdrops, stadium shells).
const LARGEST: f32 = 400.;

pub struct WorldShadows {
    light: wgpu::Buffer,
    light_group: wgpu::BindGroup,
    terrain: wgpu::RenderPipeline,
    scenery: wgpu::RenderPipeline,
    vegetation: wgpu::RenderPipeline,
    size: u32,
    /// The centre and texture matrix of the map as last drawn.
    drawn: Option<([f32; 3], [f32; 16])>,
    /// The light's clip matrix for a map that is yet to be drawn.
    pending: Option<[f32; 16]>,
    /// Caster draws of the last redraw (diagnostics).
    pub casters: u32,
}

impl WorldShadows {
    /// `world` already holds a map of `size` ([`World::set_far_size`]).
    pub fn new(gpu: &Gpu, world: &World, size: u32) -> Self {
        let device = &gpu.device;
        let light = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world shadow light"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world shadow light"),
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
            label: Some("world shadow light"),
            layout: &light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("world_shadow.wgsl"));
        let records = world.record_layout(gpu);
        let pipeline = |label,
                        layouts: &[Option<&wgpu::BindGroupLayout>],
                        vertex,
                        fragment,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::world::SHADOW_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    // The receivers also step off the surface along its
                    // normal; this covers what is left on slopes.
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.,
                        clamp: 0.,
                    },
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let terrain = pipeline(
            "world shadow terrain",
            &[Some(&light_layout)],
            "vs_terrain",
            "fs_solid",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: (atv_game::terrain_mesh::FLOATS_PER_VERTEX * 4) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            })],
        );
        let scenery = pipeline(
            "world shadow scenery",
            &[
                Some(&light_layout),
                Some(&world.texture_layout),
                Some(&records),
            ],
            "vs_scenery",
            "fs_cutout",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: crate::scenery::STRIDE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
            })],
        );
        let vegetation = pipeline(
            "world shadow vegetation",
            &[Some(&light_layout), Some(&world.texture_layout)],
            "vs_vegetation",
            "fs_cutout",
            &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: (MESH_FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![4 => Float32x3, 5 => Float32x2, 9 => Float32x3],
                }),
            ],
        );
        Self {
            light,
            light_group,
            terrain,
            scenery,
            vegetation,
            size,
            drawn: None,
            pending: None,
            casters: 0,
        }
    }

    /// For the receivers: the map's texel in texture space, then on the
    /// ground in feet.
    pub fn texels(&self) -> [f32; 2] {
        [1. / self.size as f32, 2. * RADIUS / self.size as f32]
    }

    /// The casters changed (a new terrain patch, a hidden model): draw the
    /// map again next frame.
    pub fn invalidate(&mut self) {
        self.drawn = None;
    }

    /// The world-to-texture matrix for a rider at `center`. Once the rider
    /// has left the middle of the map as last drawn, the map moves there and
    /// [`WorldShadows::render`] draws it again this frame.
    pub fn frame(&mut self, center: [f32; 3], to_sun: [f32; 3]) -> [f32; 16] {
        if let Some((at, matrix)) = &self.drawn
            && (at[0] - center[0]).abs().max((at[2] - center[2]).abs()) <= DRIFT
        {
            return *matrix;
        }
        let (clip, matrix) = shadows::light_box(center, to_sun, self.size, RADIUS, DEPTH);
        self.drawn = Some((center, matrix));
        self.pending = Some(clip);
        matrix
    }

    /// Draws the map if [`WorldShadows::frame`] moved it.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        world: &World,
        terrain: &TerrainPass,
        scenery: &SceneryPass,
        mut vegetation: Option<&mut VegetationPass>,
    ) {
        let (Some(clip), Some((center, _))) = (self.pending.take(), self.drawn) else {
            return;
        };
        gpu.queue
            .write_buffer(&self.light, 0, bytemuck::cast_slice(&clip));
        // Everything that can shade the map's ground: its own footprint
        // plus the light box's run towards the sun.
        let reach = RADIUS * 1.5 + DEPTH * 0.5;
        if let Some(vegetation) = &mut vegetation {
            vegetation.prepare_casters(gpu, center, RADIUS * 1.5);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world shadow map"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &world.far_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.light_group, &[]);
        pass.set_pipeline(&self.terrain);
        let mut casters = terrain.draw_casters(&mut pass, center, reach);
        pass.set_pipeline(&self.scenery);
        casters += scenery.draw_casters(&mut pass, center, reach, LARGEST);
        if let Some(vegetation) = &vegetation {
            pass.set_pipeline(&self.vegetation);
            casters += vegetation.draw_casters(&mut pass);
        }
        self.casters = casters;
    }
}
