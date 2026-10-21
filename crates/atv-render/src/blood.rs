//! Gorn blood (BloodRenderer / packBlood in web/wheel-effects.js): ground
//! stains, stump caps and droplet streaks packed into one instanced draw,
//! re-packed only when the crash frame changes. Authored, behind the Gorn
//! crash setting.
use crate::{Gpu, world::World};
use atv_core::gore::BloodFrame;
use wgpu::util::DeviceExt;

pub const STRIDE: usize = 12;
pub const CAPACITY: usize = 120 + 64 + 32;

/// seedAt, in double precision like Math.sin.
fn seed_at(x: f32, z: f32) -> f32 {
    let s = (f64::from(x) * 12.9898 + f64::from(z) * 78.233).sin() * 43_758.545_3;
    (s - s.floor()) as f32
}

/// Splats first, then stump caps, then droplets in flight on top.
pub fn pack(blood: &BloodFrame, out: &mut Vec<f32>) -> usize {
    out.clear();
    let mut n = 0;
    let mut put = |v: [f32; STRIDE]| {
        if n < CAPACITY {
            out.extend_from_slice(&v);
            n += 1;
        }
    };
    for s in blood.splats.as_chunks::<10>().0 {
        put([
            s[0],
            s[1],
            s[2],
            1.,
            s[3],
            s[4],
            s[5],
            seed_at(s[0], s[2]),
            s[6],
            s[7],
            s[8],
            s[9],
        ]);
    }
    for s in blood.stumps.as_chunks::<7>().0 {
        put([
            s[0],
            s[1],
            s[2],
            2.,
            s[3],
            s[4],
            s[5],
            seed_at(s[0], s[1]),
            0.,
            0.,
            s[6],
            s[6],
        ]);
    }
    for d in blood.drops.as_chunks::<7>().0 {
        let speed = (d[3] * d[3] + d[4] * d[4] + d[5] * d[5]).sqrt();
        // Motion streak: about 30 ms of travel, never longer than a foot.
        put([
            d[0],
            d[1],
            d[2],
            0.,
            d[3],
            d[4],
            d[5],
            0.,
            0.,
            0.,
            d[6],
            (d[6] + speed * 0.03).min(1.),
        ]);
    }
    n
}

pub struct BloodPass {
    pipeline: wgpu::RenderPipeline,
    corners: wgpu::Buffer,
    instances: wgpu::Buffer,
    scratch: Vec<f32>,
    count: u32,
}

impl BloodPass {
    pub fn new(gpu: &Gpu, world: &World) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("blood.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blood"),
            bind_group_layouts: &[Some(&world.globals_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blood"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: (STRIDE * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4],
                    }),
                ],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // depthMask(false), polygonOffset(-2, -2).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,
                    slope_scale: -2.,
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: world.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            corners: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("blood corners"),
                contents: bytemuck::cast_slice(&[
                    -1f32, -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.,
                ]),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blood instances"),
                size: (CAPACITY * STRIDE * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            scratch: Vec::with_capacity(CAPACITY * STRIDE),
            count: 0,
        }
    }

    /// The current crash frame's blood (at most 216 instances re-packed).
    pub fn accept(&mut self, gpu: &Gpu, blood: Option<&BloodFrame>) {
        self.count = match blood {
            Some(blood) => pack(blood, &mut self.scratch) as u32,
            None => 0,
        };
        if self.count > 0 {
            gpu.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.scratch));
        }
    }

    pub fn draws(&self) -> u32 {
        u32::from(self.count > 0)
    }

    pub fn triangles(&self) -> u32 {
        self.count * 2
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_vertex_buffer(0, self.corners.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_splats_then_stumps_then_drops() {
        let blood = BloodFrame {
            drops: vec![1., 2., 3., 0., -20., 10., 0.05],
            splats: vec![4., 0., 5., 0., 1., 0., 0.6, 0.8, 0.3, 0.7],
            stumps: vec![7., 1., 8., 0., 0., 1., 0.2],
        };
        let mut out = Vec::new();
        assert_eq!(pack(&blood, &mut out), 3);
        assert_eq!(&out[..4], &[4., 0., 5., 1.]);
        assert_eq!(&out[STRIDE..STRIDE + 4], &[7., 1., 8., 2.]);
        let drop = &out[2 * STRIDE..3 * STRIDE];
        assert_eq!(drop[3], 0.);
        assert!(drop[11] > drop[10] && drop[11] <= 1.);
        let many = BloodFrame {
            drops: (0..400)
                .flat_map(|i| [i as f32, 1., 0., 0., 0., 0., 0.05])
                .collect(),
            ..Default::default()
        };
        assert_eq!(pack(&many, &mut out), CAPACITY);
    }
}
