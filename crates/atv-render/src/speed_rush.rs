//! Authored speed-rush screen pass (SpeedRush.draw in web/speed-rush.js). The
//! amount and FOV kick come from the ride (atv_game::ride::view::SpeedRush);
//! this pass keeps the browser's animation clock, focus smoothing and tap
//! budget, and blurs the resolved frame outside the stencil-marked vehicle.
use crate::{DEPTH_FORMAT, Gpu};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RushUniform {
    params: [f32; 4],
    focus: [f32; 4],
    vehicle: [f32; 4],
}

pub struct SpeedRushPass {
    rush: wgpu::RenderPipeline,
    copy: Option<wgpu::RenderPipeline>,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    copy_uniforms: wgpu::Buffer,
    /// The resolved scene: (size, view, rush group, copy group).
    scene: Option<(
        (u32, u32),
        wgpu::TextureView,
        wgpu::BindGroup,
        wgpu::BindGroup,
    )>,
    format: wgpu::TextureFormat,
    pub amount: f32,
    time: f32,
    frame_time: f32,
    focus: [f32; 2],
    coarse: bool,
    dt: f32,
}

/// Fewer taps on phones; drop two while frames exceed 1/45 s.
pub fn taps(coarse: bool, slow: bool) -> u32 {
    ((if coarse { 5 } else { 8 }) - if slow { 2 } else { 0 }).max(3)
}

/// GL screen-space travel focus, clamped to the middle of the screen.
pub fn focus(view_projection: &[f32; 16], eye: [f32; 3], velocity: [f32; 3]) -> Option<[f32; 2]> {
    let length =
        (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]).sqrt();
    if length.is_nan() || length <= 1. {
        return None;
    }
    let p: [f32; 3] = std::array::from_fn(|i| eye[i] + velocity[i] / length * 600.);
    let clip = |row: usize| {
        view_projection[row] * p[0]
            + view_projection[4 + row] * p[1]
            + view_projection[8 + row] * p[2]
            + view_projection[12 + row]
    };
    let w = clip(3);
    if w.is_nan() || w <= 1e-3 {
        return None;
    }
    let clamp = |v: f32| v.clamp(0.2, 0.8);
    Some([
        clamp(clip(0) / w * 0.5 + 0.5),
        clamp(clip(1) / w * 0.5 + 0.5),
    ])
}

/// Screen bounds of the box around the vehicle: centre and half extents (uv).
pub fn bounds(view_projection: &[f32; 16], origin: [f32; 3]) -> [f32; 4] {
    let extent = [5., 7., 5.];
    let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for i in 0..8 {
        let p = [
            origin[0] + if i & 1 != 0 { 1. } else { -1. } * extent[0],
            origin[1] + if i & 2 != 0 { extent[1] } else { -1. },
            origin[2] + if i & 4 != 0 { 1. } else { -1. } * extent[2],
        ];
        let clip = |row: usize| {
            view_projection[row] * p[0]
                + view_projection[4 + row] * p[1]
                + view_projection[8 + row] * p[2]
                + view_projection[12 + row]
        };
        let w = clip(3);
        if w.is_nan() || w <= 1e-3 {
            return [0.; 4];
        }
        for k in 0..2 {
            let v = clip(k) / w * 0.5 + 0.5;
            min[k] = min[k].min(v);
            max[k] = max[k].max(v);
        }
    }
    [
        (min[0] + max[0]) / 2.,
        (min[1] + max[1]) / 2.,
        ((max[0] - min[0]) / 2.).min(1.),
        ((max[1] - min[1]) / 2.).min(1.),
    ]
}

impl SpeedRushPass {
    pub fn new(gpu: &Gpu, samples: u32) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("speed rush"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("speed_rush.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("speed rush"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |samples: u32, stencil: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("speed rush"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                // Stencil ref 1 marks the player's own ATV and rider: keep them crisp.
                depth_stencil: stencil.then(|| {
                    let face = wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::NotEqual,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::Keep,
                    };
                    wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::Always),
                        stencil: wgpu::StencilState {
                            front: face,
                            back: face,
                            read_mask: 0xff,
                            write_mask: 0,
                        },
                        bias: Default::default(),
                    }
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
            })
        };
        let buffer = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<RushUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            rush: pipeline(samples, true),
            // Without MSAA the resolved scene is copied out before the masked pass.
            copy: (samples == 1).then(|| pipeline(1, false)),
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("speed rush scene"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            uniforms: buffer("speed rush"),
            copy_uniforms: buffer("speed rush copy"),
            scene: None,
            format: gpu.config.format,
            amount: 0.,
            time: 0.,
            frame_time: 1. / 60.,
            focus: [0.5; 2],
            coarse: false,
            dt: 1. / 60.,
        }
    }

    pub fn set_coarse(&mut self, coarse: bool) {
        self.coarse = coarse;
    }

    /// The ride's smoothed amount for this frame; advances the line clock.
    pub fn update(&mut self, amount: f32, dt: f32) {
        self.amount = amount;
        if amount <= 0. {
            self.focus = [0.5; 2];
            return;
        }
        let step = dt.max(0.);
        self.time = (self.time + step * (1.5 + 2.5 * amount)) % 1000.;
        self.frame_time += (step - self.frame_time) * (1. - (-step / 0.5).exp());
        self.dt = step;
    }

    pub fn active(&self) -> bool {
        self.amount >= 0.001
    }

    /// The single-sample texture the scene resolves into this frame.
    pub fn scene_target(&mut self, gpu: &Gpu, size: (u32, u32)) -> wgpu::TextureView {
        if self.scene.as_ref().is_none_or(|s| s.0 != size) {
            let view = gpu
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("speed rush scene"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default());
            let group = |buffer: &wgpu::Buffer| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("speed rush"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            };
            let (rush, copy) = (group(&self.uniforms), group(&self.copy_uniforms));
            self.scene = Some((size, view.clone(), rush, copy));
        }
        self.scene.as_ref().unwrap().1.clone()
    }

    /// Writes this frame's uniforms; `focus`/`vehicle` from the view.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        size: (u32, u32),
        focus: Option<[f32; 2]>,
        vehicle: [f32; 4],
    ) {
        if let Some(target) = focus {
            let k = 1. - (-self.dt / 0.12).exp();
            self.focus = std::array::from_fn(|i| self.focus[i] + (target[i] - self.focus[i]) * k);
        }
        let pixel = [1. / size.0 as f32, 1. / size.1 as f32];
        let taps = taps(self.coarse, self.frame_time > 1. / 45.);
        let uniform = RushUniform {
            params: [
                self.amount,
                size.0 as f32 / size.1 as f32,
                self.time,
                taps as f32,
            ],
            focus: [self.focus[0], self.focus[1], pixel[0], pixel[1]],
            vehicle,
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
        let copy = RushUniform {
            params: [0., uniform.params[1], 0., 1.],
            ..uniform
        };
        gpu.queue
            .write_buffer(&self.copy_uniforms, 0, bytemuck::bytes_of(&copy));
    }

    /// The masked pass into `target` (resolving `msaa`), after the scene.
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        msaa: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
    ) {
        let Some((_, _, rush, copy)) = &self.scene else {
            return;
        };
        if let (None, Some(pipeline)) = (msaa, &self.copy) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("speed rush copy"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, copy, &[]);
            pass.draw(0..3, 0..1);
        }
        let (view, resolve_target) = match msaa {
            Some(msaa) => (msaa, Some(target)),
            None => (target, None),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("speed rush"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Discard,
                }),
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.rush);
        pass.set_stencil_reference(1);
        pass.set_bind_group(0, rush, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_follow_the_browser_budget() {
        assert_eq!(
            (taps(false, false), taps(true, false), taps(true, true)),
            (8, 5, 3)
        );
    }
}
