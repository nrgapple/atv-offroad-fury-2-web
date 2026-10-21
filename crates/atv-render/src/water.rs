//! Original water surface (web/water.js): a screen-projected grid displaced
//! by the live WTT height signatures, shaded with the derivative maps and the
//! environment texture. One draw; the grid follows the viewport (00190aa0).
use crate::{Gpu, texture, world::World};
use atv_game::water::Water;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_projection: [f32; 16],
    eye: [f32; 4],
    forward: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    projection: [f32; 4],
    surface: [f32; 4],
    environment_color: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    origins: [[f32; 4]; 8],
    widths: [[f32; 4]; 8],
    channels: [[f32; 4]; 8],
    waves: [[f32; 4]; 4],
    second: [[f32; 4]; 4],
}

pub struct WaterPass {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    uniforms: Uniforms,
    grid: Option<(wgpu::Buffer, wgpu::Buffer, u32, (u32, u32))>,
    normal_dimension: usize,
    layers: Vec<Vec<usize>>,
    wave_layers: Vec<Vec<usize>>,
}

/// 00190aa0 scales strip groups by viewport width and dense rows by height;
/// each native strip adds 19 samples. The browser caps the grid at 12,000
/// (landscape) or 8,000 (portrait) vertices for its 16-bit indices
/// (projectedGridSize in web/water.js); the Rust build draws with 32-bit
/// indices and keeps the native dense-row count (fidelity follow-up; the
/// 8..1024 row bounds are the browser's).
pub fn grid_size(width: u32, height: u32) -> (u32, u32) {
    let strips = ((3. * width as f64 / 640. + 0.5).floor() as u32).clamp(1, 8);
    let columns = strips * 19 + 1;
    let rows = ((334. * height as f64 / 448. + 0.5).floor() as u32).clamp(8, 1024);
    (columns, rows)
}

fn grid(gpu: &Gpu, columns: u32, rows: u32) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    let mut points = Vec::with_capacity((columns * (rows + 1) * 2) as usize);
    for y in 0..=rows {
        for x in 0..columns {
            points.extend([
                x as f32 / (columns - 1) as f32 * 2. - 1.,
                y as f32 / rows as f32,
            ]);
        }
    }
    let mut indices: Vec<u32> = Vec::with_capacity(((columns - 1) * rows * 6) as usize);
    for y in 0..rows {
        for x in 0..columns - 1 {
            let a = y * columns + x;
            let (b, c) = (a + 1, a + columns);
            indices.extend([a, c, b, b, c, c + 1]);
        }
    }
    let buffer = |contents: &[u8], usage| {
        gpu.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water grid"),
                contents,
                usage,
            })
    };
    (
        buffer(bytemuck::cast_slice(&points), wgpu::BufferUsages::VERTEX),
        buffer(bytemuck::cast_slice(&indices), wgpu::BufferUsages::INDEX),
        indices.len() as u32,
    )
}

fn array_texture(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    dimension: u32,
    layers: u32,
    levels: u32,
) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("water maps"),
        size: wgpu::Extent3d {
            width: dimension,
            height: dimension,
            depth_or_array_layers: texture::array_layers(layers.max(2)),
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write_layer(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    level: u32,
    layer: u32,
    size: u32,
    bytes: u32,
    data: &[u8],
) {
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * bytes),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
}

impl WaterPass {
    pub fn new(gpu: &Gpu, world: &World, water: &Water, fresnel: bool) -> Result<Self, String> {
        let device = &gpu.device;
        let height = &water.height;
        if height.textures.is_empty()
            || height.channels.len() > 8
            || water.gradients.waves.len() > 4
        {
            return Err("Invalid original water render inputs.".into());
        }
        // Smaller signatures tile into one atlas dimension (water.js).
        let dimension = height
            .textures
            .iter()
            .map(|t| t.dimension)
            .max()
            .unwrap_or(1);
        if height
            .textures
            .iter()
            .any(|t| dimension % t.dimension != 0 || t.pixels.len() != t.dimension * t.dimension)
        {
            return Err("Invalid water height atlas.".into());
        }
        let heights = array_texture(
            gpu,
            wgpu::TextureFormat::R8Uint,
            dimension as u32,
            height.textures.len() as u32,
            1,
        );
        for (layer, t) in height.textures.iter().enumerate() {
            let mut pixels = vec![0u8; dimension * dimension];
            for y in 0..dimension {
                for x in 0..dimension {
                    pixels[y * dimension + x] =
                        t.pixels[(y % t.dimension) * t.dimension + x % t.dimension];
                }
            }
            write_layer(gpu, &heights, 0, layer as u32, dimension as u32, 1, &pixels);
        }
        let gradients = &water.gradients;
        let normal_dimension = gradients
            .textures
            .iter()
            .map(|t| t.dimension)
            .max()
            .unwrap_or(1);
        if gradients.textures.is_empty()
            || gradients.textures.iter().any(|t| {
                normal_dimension % t.dimension != 0
                    || t.pixels.len() != t.dimension * t.dimension * 4
            })
        {
            return Err("Invalid water derivative atlas.".into());
        }
        let levels = texture::mip_levels(normal_dimension as u32, normal_dimension as u32);
        let normals = array_texture(
            gpu,
            wgpu::TextureFormat::Rgba8Unorm,
            normal_dimension as u32,
            gradients.textures.len() as u32,
            levels,
        );
        for (layer, t) in gradients.textures.iter().enumerate() {
            let n = normal_dimension;
            let mut pixels = vec![0u8; n * n * 4];
            for y in 0..n {
                for x in 0..n {
                    let from = ((y % t.dimension) * t.dimension + x % t.dimension) * 4;
                    pixels[(y * n + x) * 4..][..4].copy_from_slice(&t.pixels[from..from + 4]);
                }
            }
            let mut size = n as u32;
            for level in 0..levels {
                write_layer(gpu, &normals, level, layer as u32, size, 4, &pixels);
                if level + 1 < levels {
                    (pixels, size, _) = texture::downsample(&pixels, size, size);
                }
            }
        }
        let image = &water.environment;
        let environment = texture::upload(
            gpu,
            image.width,
            image.height,
            &image.rgba,
            "water environment",
        );
        let view = |t: &wgpu::Texture, array: bool| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(if array {
                    wgpu::TextureViewDimension::D2Array
                } else {
                    wgpu::TextureViewDimension::D2
                }),
                ..Default::default()
            })
        };
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let float = |dimension| wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: dimension,
            multisampled: false,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water"),
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
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
                entry(2, float(wgpu::TextureViewDimension::D2)),
                entry(3, float(wgpu::TextureViewDimension::D2Array)),
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
        let material = &water.material;
        let mut uniforms: Uniforms = bytemuck::Zeroable::zeroed();
        uniforms.environment_color = [
            material.environment_color[0],
            material.environment_color[1],
            material.environment_color[2],
            0.,
        ];
        uniforms.fog_color = world.globals.fog_color;
        uniforms.fog = [
            world.globals.fog[0],
            world.globals.fog[1],
            (world.globals.fog[2] > 0.5 && material.environment_fog) as u8 as f32,
            fresnel as u8 as f32,
        ];
        let mut layers = Vec::new();
        for (i, channel) in height.channels.iter().enumerate() {
            uniforms.origins[i][2..].copy_from_slice(&channel.inverse_width);
            uniforms.widths[i] = [
                channel.width[0],
                channel.width[1],
                channel.tiled as u8 as f32,
                0.,
            ];
            uniforms.channels[i] = [
                channel.dimension as f32,
                channel.scale,
                if channel.positive { 1. } else { -1. },
                channel.layer as f32,
            ];
            layers.push(if channel.layers.is_empty() {
                vec![channel.layer]
            } else {
                channel.layers.clone()
            });
        }
        let wave_layers = gradients
            .waves
            .iter()
            .map(|wave| {
                if wave.layers.is_empty() {
                    vec![wave.layer]
                } else {
                    wave.layers.clone()
                }
            })
            .collect();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let clamp = texture::sampler(gpu, false, 1);
        let repeat = texture::sampler(gpu, true, 1);
        let (heights_view, environment_view, normals_view) = (
            view(&heights, true),
            view(&environment, false),
            view(&normals, true),
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&heights_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&environment_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&normals_view),
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("water.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DEPTH_FORMAT,
                depth_write_enabled: Some(material.environment_depth_write),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
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
                    blend: material
                        .environment_blend
                        .then_some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut pass = Self {
            pipeline,
            group,
            buffer,
            uniforms,
            grid: None,
            normal_dimension,
            layers,
            wave_layers,
        };
        pass.accept(water)?;
        Ok(pass)
    }

    /// Takes the prepared frame of the shared field (WaterRenderer.accept).
    pub fn accept(&mut self, water: &Water) -> Result<(), String> {
        let frame = water.frame();
        if frame.origins.len() != self.layers.len() || !frame.base_height.is_finite() {
            return Err("Invalid prepared shared water frame.".into());
        }
        self.uniforms.surface[0] = frame.base_height;
        for (i, origin) in frame.origins.iter().enumerate() {
            self.uniforms.origins[i][..2].copy_from_slice(origin);
            let pair = frame.signature_frames.get(i).copied().unwrap_or([0, 0]);
            // The shader samples the actual first frame (no regenerated atlas).
            self.uniforms.channels[i][3] = *self.layers[i]
                .get(pair[0])
                .ok_or("Invalid shared water animation frame.")?
                as f32;
        }
        for (i, wave) in water.gradients.waves.iter().enumerate() {
            let pair = frame
                .signature_frames
                .get(wave.origin_channel)
                .copied()
                .unwrap_or([0, 0]);
            let weight = frame
                .first_frame_weights
                .get(wave.origin_channel)
                .copied()
                .unwrap_or(1.);
            let layers = &self.wave_layers[i];
            let (Some(&first_layer), Some(&second_layer)) =
                (layers.get(pair[0]), layers.get(pair[1]))
            else {
                return Err("Invalid shared water gradient frame.".into());
            };
            // 00197980 rounds GS FIX independently for each original frame pass.
            let first = (128. * wave.scale * weight + 0.5).trunc() / 128.;
            let second = if pair[0] == pair[1] {
                0.
            } else {
                (128. * wave.scale * (1. - weight) + 0.5).trunc() / 128.
            };
            let ratio = water.gradients.textures[first_layer].dimension as f32
                / self.normal_dimension as f32;
            self.uniforms.waves[i] = [wave.origin_channel as f32, first_layer as f32, first, ratio];
            self.uniforms.second[i] = [second_layer as f32, second, 0., 0.];
        }
        self.uniforms.surface[2] = self.layers.len() as f32;
        self.uniforms.surface[3] = water.gradients.waves.len() as f32;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        view_projection: &[f32; 16],
        camera: &crate::Camera,
        focal_length: f32,
        size: (u32, u32),
        far: f32,
    ) {
        let (columns, rows) = grid_size(size.0, size.1);
        if self.grid.as_ref().is_none_or(|g| g.3 != (columns, rows)) {
            let (vertices, indices, count) = grid(gpu, columns, rows);
            self.grid = Some((vertices, indices, count, (columns, rows)));
        }
        let axes = camera.axes();
        let u = &mut self.uniforms;
        u.view_projection = *view_projection;
        u.eye = [camera.x, camera.y, camera.z, 1.];
        u.forward = [axes.forward[0], axes.forward[1], axes.forward[2], 0.];
        u.right = [axes.right[0], axes.right[1], axes.right[2], 0.];
        u.up = [axes.up[0], axes.up[1], axes.up[2], 0.];
        let (sp, cp) = camera.pitch.sin_cos();
        let difference = u.surface[0] - camera.y;
        let boundary = (difference / far - sp) * focal_length / cp;
        let (minimum, maximum) = if difference < 0. {
            (-1., boundary.clamp(-1., 1.))
        } else {
            (boundary.clamp(-1., 1.), 1.)
        };
        let aspect = size.0 as f32 / size.1 as f32;
        u.projection = [aspect / focal_length, 1. / focal_length, minimum, maximum];
        u.surface[1] = far;
        gpu.queue
            .write_buffer(&self.buffer, 0, bytemuck::bytes_of(&self.uniforms));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some((vertices, indices, count, _)) = &self.grid else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..*count, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn grid_keeps_the_native_dense_rows() {
        // 00190aa0: 6 strip groups (115 columns) and 537 dense rows at
        // 1280x720 (the browser caps rows at 103); 39 x 629 at 390x844 (204).
        assert_eq!(super::grid_size(1280, 720), (115, 537));
        assert_eq!(super::grid_size(390, 844), (39, 629));
        assert_eq!(super::grid_size(640, 448), (58, 334));
    }
}
