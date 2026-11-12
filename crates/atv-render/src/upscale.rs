//! A scene drawn below the surface's density: the 3D scene renders into its
//! own smaller texture (the adaptive render ratio) and this pass stretches it
//! over the surface, so the UI drawn after it keeps the display's density.

pub(crate) struct Upscale {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

/// The smaller scene texture: its size, the view the scene draws into and
/// the bind group the pass reads it through.
pub(crate) struct SceneColor {
    pub size: (u32, u32),
    pub view: wgpu::TextureView,
    group: wgpu::BindGroup,
}

/// The scene's size for a `scale` (scene pixels per surface pixel, at most
/// 1) of a `width` x `height` surface; None when it would be the surface's.
pub fn scene_size(width: u32, height: u32, scale: f64) -> Option<(u32, u32)> {
    let scale = scale.clamp(0.1, 1.);
    let fit = |n: u32| ((f64::from(n) * scale).round() as u32).clamp(1, n.max(1));
    let size = (fit(width), fit(height));
    (size != (width, height)).then_some(size)
}

impl Upscale {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("upscale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("upscale.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("upscale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("upscale"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("upscale"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
        }
    }

    pub fn scene(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> SceneColor {
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("scaled scene"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("upscale"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        SceneColor { size, view, group }
    }

    /// `scene` stretched over the whole of `target`.
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &SceneColor,
        target: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("upscale"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &scene.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// A split-screen rectangle in surface pixels, in the scene's pixels.
pub fn scale_rect(rect: [u32; 4], surface: (u32, u32), scene: (u32, u32)) -> [u32; 4] {
    let x = |n: u32| (u64::from(n) * u64::from(scene.0) / u64::from(surface.0.max(1))) as u32;
    let y = |n: u32| (u64::from(n) * u64::from(scene.1) / u64::from(surface.1.max(1))) as u32;
    // Edges map independently, so neighbouring views still meet exactly.
    let (left, top) = (x(rect[0]), y(rect[1]));
    let (right, bottom) = (x(rect[0] + rect[2]), y(rect[1] + rect[3]));
    [
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_size_follows_the_scale_and_skips_the_surface_size() {
        // DPR 3 portrait phone, ride at 1.5: half the surface each way.
        assert_eq!(scene_size(1170, 2532, 0.5), Some((585, 1266)));
        assert_eq!(scene_size(1170, 2532, 1.), None);
        assert_eq!(scene_size(1170, 2532, 1.2), None);
        assert_eq!(scene_size(1, 1, 0.5), None);
    }

    #[test]
    fn split_rects_meet_after_scaling() {
        let surface = (2532, 1170);
        let left = scale_rect([0, 0, 1265, 1170], surface, (1266, 585));
        let right = scale_rect([1265, 0, 1267, 1170], surface, (1266, 585));
        assert_eq!(left[0] + left[2], right[0]);
        assert_eq!(right[0] + right[2], 1266);
        assert_eq!(left[3], 585);
    }
}
