//! The UI render pass: draws a [`DisplayList`] over the finished scene.
use crate::{
    color::Fill,
    paint::{BoxPaint, DisplayList, ImageId, Item},
    text::{self, Bitmap, Fonts},
};
use std::collections::HashMap;

const ATLAS: u32 = 2048;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct Instance {
    rect: [f32; 4],
    shape: [f32; 4],
    radii: [f32; 4],
    color: [f32; 4],
    border: [f32; 4],
    border_color: [f32; 4],
    uv: [f32; 4],
    params: [f32; 4],
    cut: [f32; 4],
    cut_radii: [f32; 4],
    clip: [f32; 4],
    extra: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Any,
    Atlas,
    Image(ImageId),
}

struct Entry {
    x: u32,
    y: u32,
    bitmap_left: i32,
    bitmap_top: i32,
    width: u32,
    height: u32,
}

/// Shelf-packed premultiplied RGBA cache for text, shapes and gradient ramps.
struct Atlas {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    entries: HashMap<String, Entry>,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
}

impl Atlas {
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (w, h) = (width + 2, height + 2);
        if w > ATLAS || h > ATLAS {
            return None;
        }
        if self.shelf_x + w > ATLAS {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > ATLAS {
            return None;
        }
        let at = (self.shelf_x + 1, self.shelf_y + 1);
        self.shelf_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(at)
    }

    fn reset(&mut self) {
        self.entries.clear();
        self.shelf_x = 0;
        self.shelf_y = 0;
        self.shelf_h = 0;
    }
}

struct Texture {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    width: u32,
    height: u32,
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    atlas: Atlas,
    images: HashMap<ImageId, Texture>,
    next_image: u32,
    instances: Vec<Instance>,
    batches: Vec<(Source, std::ops::Range<u32>)>,
    buffer: wgpu::Buffer,
    capacity: usize,
    /// Rasterized this frame (for cost reporting).
    pub rasterized: u32,
    atlas_full: bool,
}

fn upload(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    rgba: &[u8],
) {
    if w == 0 || h == 0 {
        return;
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
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
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui texture"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
            8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let atlas_texture = Self::texture(device, ATLAS, ATLAS, "ui atlas");
        let bind = Self::bind(device, &layout, &sampler, &atlas_texture);
        let capacity = 256;
        Self {
            buffer: Self::instance_buffer(device, capacity),
            capacity,
            pipeline,
            layout,
            sampler,
            globals,
            globals_bind,
            atlas: Atlas {
                texture: atlas_texture,
                bind,
                entries: HashMap::new(),
                shelf_x: 0,
                shelf_y: 0,
                shelf_h: 0,
            },
            images: HashMap::new(),
            next_image: 1,
            instances: Vec::new(),
            batches: Vec::new(),
            rasterized: 0,
            atlas_full: false,
        }
    }

    fn texture(device: &wgpu::Device, width: u32, height: u32, label: &str) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        texture: &wgpu::Texture,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui texture"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Registers straight-alpha RGBA artwork as its own texture.
    pub fn add_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> ImageId {
        let id = ImageId(self.next_image);
        self.next_image += 1;
        let texture = Self::texture(device, width, height, "ui image");
        let bind = Self::bind(device, &self.layout, &self.sampler, &texture);
        self.images.insert(
            id,
            Texture {
                texture,
                bind,
                width,
                height,
            },
        );
        self.update_image(queue, id, rgba);
        id
    }

    /// Replaces an image's pixels (same size).
    pub fn update_image(&self, queue: &wgpu::Queue, id: ImageId, rgba: &[u8]) {
        let Some(t) = self.images.get(&id) else {
            return;
        };
        let mut data = rgba.to_vec();
        text::premultiply(&mut data);
        upload(queue, &t.texture, 0, 0, t.width, t.height, &data);
    }

    pub fn remove_image(&mut self, id: ImageId) {
        self.images.remove(&id);
    }

    pub fn image_size(&self, id: ImageId) -> Option<(u32, u32)> {
        self.images.get(&id).map(|t| (t.width, t.height))
    }

    /// Atlas entry for `key`, rasterizing on a miss. Returns None if empty.
    fn cached(
        &mut self,
        queue: &wgpu::Queue,
        key: String,
        raster: &mut dyn FnMut() -> Bitmap,
    ) -> Option<([f32; 4], i32, i32, u32, u32)> {
        if !self.atlas.entries.contains_key(&key) {
            let bitmap = raster();
            self.rasterized += 1;
            if bitmap.width == 0 || bitmap.height == 0 {
                return None;
            }
            // Never overwrite texels already referenced by this frame. If the
            // cache fills, rebuild the entire frame against a fresh atlas.
            let at = self.atlas.allocate(bitmap.width, bitmap.height);
            if at.is_none() && bitmap.width + 2 <= ATLAS && bitmap.height + 2 <= ATLAS {
                self.atlas_full = true;
            }
            let (x, y) = at?;
            upload(
                queue,
                &self.atlas.texture,
                x,
                y,
                bitmap.width,
                bitmap.height,
                &bitmap.rgba,
            );
            self.atlas.entries.insert(
                key.clone(),
                Entry {
                    x,
                    y,
                    bitmap_left: bitmap.left,
                    bitmap_top: bitmap.top,
                    width: bitmap.width,
                    height: bitmap.height,
                },
            );
        }
        let e = self.atlas.entries.get_mut(&key)?;
        let s = ATLAS as f32;
        Some((
            [
                e.x as f32 / s,
                e.y as f32 / s,
                (e.x + e.width) as f32 / s,
                (e.y + e.height) as f32 / s,
            ],
            e.bitmap_left,
            e.bitmap_top,
            e.width,
            e.height,
        ))
    }

    fn push(&mut self, source: Source, instance: Instance) {
        let index = self.instances.len() as u32;
        self.instances.push(instance);
        if let Some((batch, range)) = self.batches.last_mut() {
            let fits = source == Source::Any || *batch == source || *batch == Source::Any;
            if fits {
                if *batch == Source::Any {
                    *batch = source;
                }
                range.end = index + 1;
                return;
            }
        }
        self.batches.push((source, index..index + 1));
    }

    /// Builds this frame's instances; `scale` is physical pixels per CSS pixel.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        fonts: &mut Fonts,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f32,
    ) {
        self.prepare_inner(device, queue, fonts, list, width, height, scale, true);
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        fonts: &mut Fonts,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f32,
        retry: bool,
    ) {
        self.rasterized = 0;
        self.atlas_full = false;
        self.instances.clear();
        self.batches.clear();
        let full = [0., 0., width as f32, height as f32];
        let mut clips: Vec<([f32; 4], f32)> = vec![(full, 0.)];
        let mut opacity = vec![1f32];
        let s = scale;
        for item in &list.items {
            let (clip, clip_radius) = *clips.last().unwrap();
            let alpha = *opacity.last().unwrap();
            let base = Instance {
                clip,
                extra: [clip_radius, 0., 0., 0.],
                ..Default::default()
            };
            match item {
                Item::PushClip { rect, radius } => {
                    let r = [
                        (rect.x * s).max(clip[0]),
                        (rect.y * s).max(clip[1]),
                        (rect.right() * s).min(clip[2]),
                        (rect.bottom() * s).min(clip[3]),
                    ];
                    clips.push((r, radius * s));
                }
                Item::PopClip => {
                    if clips.len() > 1 {
                        clips.pop();
                    }
                }
                Item::PushOpacity(o) => opacity.push(alpha * o),
                Item::PopOpacity => {
                    if opacity.len() > 1 {
                        opacity.pop();
                    }
                }
                Item::Box(paint) => self.paint_box(queue, paint, s, alpha, base),
                Item::Text { text, style, x, y } => {
                    let (px, fx) = text::phase(x * s);
                    let (py, fy) = text::phase(y * s);
                    let key = text::raster_key(&Fonts::text_key(text, style), s, [fx, fy]);
                    if let Some((uv, left, top, w, h)) = self.cached(queue, key, &mut || {
                        fonts.raster_text(text, style, s, [fx, fy])
                    }) {
                        self.push(
                            Source::Atlas,
                            Instance {
                                rect: [(px + left) as f32, (py + top) as f32, w as f32, h as f32],
                                color: [alpha; 4],
                                uv,
                                params: [3., 0., 0., 0.],
                                ..base
                            },
                        );
                    }
                }
                Item::Shape {
                    shape,
                    x,
                    y,
                    rotation,
                } => {
                    let (px, fx) = text::phase(x * s);
                    let (py, fy) = text::phase(y * s);
                    let phase = if *rotation == 0. { [fx, fy] } else { [0, 0] };
                    let (ox, oy) = if *rotation == 0. {
                        (px as f32, py as f32)
                    } else {
                        (x * s, y * s)
                    };
                    let key = text::raster_key(&Fonts::shape_key(shape), s, phase);
                    if let Some((uv, left, top, w, h)) =
                        self.cached(queue, key, &mut || fonts.raster_shape(shape, s, phase))
                    {
                        self.push(
                            Source::Atlas,
                            Instance {
                                rect: [ox + left as f32, oy + top as f32, w as f32, h as f32],
                                color: [alpha; 4],
                                uv,
                                params: [3., 0., *rotation, 0.],
                                extra: [clip_radius, x * s, y * s, 0.],
                                ..base
                            },
                        );
                    }
                }
                Item::Image {
                    image,
                    src,
                    dst,
                    rotation,
                    pivot,
                } => {
                    let Some((iw, ih)) = self.image_size(*image) else {
                        continue;
                    };
                    let (iw, ih) = (iw as f32, ih as f32);
                    self.push(
                        Source::Image(*image),
                        Instance {
                            rect: [dst.x * s, dst.y * s, dst.w * s, dst.h * s],
                            color: [alpha; 4],
                            uv: [src.x / iw, src.y / ih, src.right() / iw, src.bottom() / ih],
                            params: [3., 0., *rotation, 0.],
                            extra: [clip_radius, pivot[0] * s, pivot[1] * s, 0.],
                            ..base
                        },
                    );
                }
            }
        }
        if self.atlas_full && retry {
            self.atlas.reset();
            self.prepare_inner(device, queue, fonts, list, width, height, scale, false);
            return;
        }
        if self.instances.len() > self.capacity {
            self.capacity = self.instances.len().next_power_of_two();
            self.buffer = Self::instance_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.instances));
        queue.write_buffer(
            &self.globals,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, scale, 0.]),
        );
    }

    fn paint_box(
        &mut self,
        queue: &wgpu::Queue,
        paint: &BoxPaint,
        s: f32,
        alpha: f32,
        base: Instance,
    ) {
        let r = paint.rect;
        let shape = [r.x * s, r.y * s, r.w * s, r.h * s];
        let radii = paint.radii.map(|v| v * s);
        let grow = |b: [f32; 4], by: f32| [b[0] - by, b[1] - by, b[2] + 2. * by, b[3] + 2. * by];
        let fade = |c: [f32; 4]| c.map(|v| v * alpha);
        // Outer shadows paint below the box, outside its border edge.
        for shadow in paint.shadows.iter().rev().filter(|s| !s.inset) {
            let sigma = shadow.blur * s / 2.;
            let spread = shadow.spread * s;
            let b = grow(
                [
                    shape[0] + shadow.dx * s,
                    shape[1] + shadow.dy * s,
                    shape[2],
                    shape[3],
                ],
                spread,
            );
            let shadow_radii = radii.map(|v| if v > 0. { (v + spread).max(0.) } else { 0. });
            self.push(
                Source::Any,
                Instance {
                    rect: grow(b, sigma * 3. + 1.),
                    shape: b,
                    radii: shadow_radii,
                    color: fade(shadow.color.premultiplied()),
                    params: [1., sigma, paint.rotation, 0.],
                    extra: [base.extra[0], paint.pivot[0] * s, paint.pivot[1] * s, 0.],
                    cut: shape,
                    cut_radii: radii,
                    ..base
                },
            );
        }
        let base = Instance {
            params: [0., 0., paint.rotation, 0.],
            extra: [base.extra[0], paint.pivot[0] * s, paint.pivot[1] * s, 0.],
            ..base
        };
        let mut instance = Instance {
            rect: grow(shape, 1.),
            shape,
            radii,
            border: paint.border.map(|v| v * s),
            border_color: fade(paint.border_color.premultiplied()),
            ..base
        };
        let mut source = Source::Any;
        match &paint.fill {
            Some(Fill::Solid(c)) => instance.color = fade(c.premultiplied()),
            Some(Fill::Linear(g)) => {
                let key = format!("G{g:?}");
                let ramp = g.ramp();
                if let Some((uv, ..)) = self.cached(queue, key, &mut || Bitmap {
                    width: 256,
                    height: 1,
                    left: 0,
                    top: 0,
                    rgba: ramp.clone(),
                }) {
                    source = Source::Atlas;
                    let texel = 0.5 / ATLAS as f32;
                    instance.uv = [uv[0] + texel, uv[1], uv[2] - texel, uv[3]];
                    instance.params = [0., 0., paint.rotation, 1.];
                    instance.cut = [g.angle_degrees.to_radians(), 0., 0., 0.];
                    instance.color = [alpha; 4];
                }
            }
            Some(Fill::Radial {
                center,
                gradient,
                ellipse,
            }) => {
                let key = format!("G{gradient:?}");
                let ramp = gradient.ramp();
                if let Some((uv, ..)) = self.cached(queue, key, &mut || Bitmap {
                    width: 256,
                    height: 1,
                    left: 0,
                    top: 0,
                    rgba: ramp.clone(),
                }) {
                    source = Source::Atlas;
                    let texel = 0.5 / ATLAS as f32;
                    instance.uv = [uv[0] + texel, uv[1], uv[2] - texel, uv[3]];
                    instance.params = [0., 0., paint.rotation, 2.];
                    instance.cut = [0., center[0], center[1], *ellipse as u8 as f32];
                    instance.color = [alpha; 4];
                }
            }
            None => {}
        }
        if paint.ring {
            instance.params[1] = 1.;
        }
        if paint.fill.is_some() || paint.border.iter().any(|w| *w > 0.) {
            self.push(source, instance);
        }
        for shadow in paint.shadows.iter().rev().filter(|s| s.inset) {
            let sigma = shadow.blur * s / 2.;
            let spread = shadow.spread * s;
            let hole = grow(
                [
                    shape[0] + shadow.dx * s,
                    shape[1] + shadow.dy * s,
                    shape[2],
                    shape[3],
                ],
                -spread,
            );
            self.push(
                Source::Any,
                Instance {
                    rect: grow(shape, 1.),
                    shape,
                    radii,
                    color: fade(shadow.color.premultiplied()),
                    params: [2., sigma, paint.rotation, 0.],
                    cut: hole,
                    cut_radii: radii.map(|v| (v - spread).max(0.)),
                    ..base
                },
            );
        }
    }

    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if self.instances.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_bind, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for (source, range) in &self.batches {
            let bind = match source {
                Source::Image(id) => match self.images.get(id) {
                    Some(t) => &t.bind,
                    None => continue,
                },
                _ => &self.atlas.bind,
            };
            pass.set_bind_group(1, bind, &[]);
            pass.draw(0..6, range.clone());
        }
    }
}
