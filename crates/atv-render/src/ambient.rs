//! Placed TOY ambient effects (AmbientEffectsRenderer, AmbientStreamsRenderer).
//! CPU sprites are sorted back to front each frame and drawn as two instanced
//! blend families; fountain births enter a bounded GPU ring aged in WGSL.
use crate::{
    Camera, Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use atv_game::ambient::Ambient;
use wgpu::util::DeviceExt;

const SPRITE_FLOATS: usize = 10;
/// 400 type-11 packets of 12 births (ambient-streams.js).
pub const STREAM_CAPACITY: usize = 400 * 12;
const STREAM_FLOATS: usize = 20;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EffectUniform {
    right: [f32; 4],
    up: [f32; 4],
    light: [f32; 4],
}

struct Sprites {
    alpha: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    scratch: Vec<f32>,
    group: wgpu::BindGroup,
    /// Instance counts of the alpha and additive families this frame.
    counts: [u32; 2],
    width: f32,
    height: f32,
}

struct Streams {
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    data: Vec<f32>,
    group: wgpu::BindGroup,
    id: u64,
    count: u32,
    dirty: Vec<usize>,
}

pub struct AmbientPass {
    corners: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    sprites: Option<Sprites>,
    streams: Option<Streams>,
    /// Sort scratch: (depth, index).
    order: Vec<(f32, usize)>,
    clock: f32,
}

fn quad_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
    }
}

/// 001451a0 persistent-particle near fade, in the source's float order.
fn sprite_alpha(p: &atv_core::ambient_effects::Sprite, camera: &Camera) -> f32 {
    let Some(fade) = p.fade_distance.filter(|f| *f != 0.) else {
        return p.alpha;
    };
    let (dx, dy, dz) = (
        p.position[0] - camera.x,
        p.position[1] - camera.y,
        p.position[2] - camera.z,
    );
    let squared = (dx * dx + dy * dy) + dz * dz;
    let limit = fade * fade;
    if squared < limit {
        p.alpha * (squared * (1. / limit))
    } else {
        p.alpha
    }
}

impl AmbientPass {
    pub fn new(gpu: &Gpu, world: &World, ambient: &Ambient) -> Self {
        let device = &gpu.device;
        let corners = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ambient corners"),
            contents: bytemuck::cast_slice(&[
                -1f32, -1., 1., -1., 1., 1., -1., -1., 1., 1., -1., 1.,
            ]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ambient uniforms"),
            size: std::mem::size_of::<EffectUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let size = std::mem::size_of::<EffectUniform>();
        let draw_group = world.draw_group(gpu, &uniforms, size, &uniforms, 16);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ambient atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("ambient.wgsl"));
        let pipeline = |vertex, fragment, blend, instance: wgpu::VertexBufferLayout<'_>| {
            world.pipeline(
                gpu,
                PipelineDesc {
                    label: "ambient",
                    shader: &shader,
                    vertex,
                    fragment,
                    buffers: &[Some(quad_layout()), Some(instance)],
                    blend,
                    depth_write: false,
                    depth_compare: wgpu::CompareFunction::Less,
                    per_draw: true,
                },
            )
        };
        let capacity = ambient.metadata.capacity.max(1);
        let sprites = match (&ambient.atlas, ambient.metadata.cpu_active > 0) {
            (Some(image), true) => {
                const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
                    wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x2];
                let layout = || wgpu::VertexBufferLayout {
                    array_stride: (SPRITE_FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
                };
                let atlas = texture::upload_flat(
                    gpu,
                    image.width,
                    image.height,
                    &image.rgba,
                    "ambient atlas",
                );
                Some(Sprites {
                    alpha: pipeline("vs_sprite", "fs_sprite", Blend::Alpha, layout()),
                    additive: pipeline("vs_sprite", "fs_sprite", Blend::Additive, layout()),
                    instances: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("ambient sprites"),
                        size: (capacity * 2 * SPRITE_FLOATS * 4) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    scratch: Vec::with_capacity(capacity * 2 * SPRITE_FLOATS),
                    group: world.texture_group(gpu, &atlas, &sampler),
                    counts: [0; 2],
                    width: image.width as f32,
                    height: image.height as f32,
                })
            }
            _ => None,
        };
        let streams = ambient.gpu_atlas.as_ref().map(|image| {
            let atlas = texture::upload_flat(gpu, image.width, image.height, &image.rgba, "fountain atlas");
            Streams {
                pipeline: pipeline(
                    "vs_stream",
                    "fs_stream",
                    Blend::Alpha,
                    wgpu::VertexBufferLayout {
                        array_stride: (STREAM_FLOATS * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4],
                    },
                ),
                instances: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("fountain ring"),
                    size: (STREAM_CAPACITY * STREAM_FLOATS * 4) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                data: vec![0.; STREAM_CAPACITY * STREAM_FLOATS],
                group: world.texture_group(gpu, &atlas, &sampler),
                id: 0,
                count: 0,
                dirty: Vec::new(),
            }
        });
        let mut pass = Self {
            corners,
            uniforms,
            draw_group,
            sprites,
            streams,
            order: Vec::new(),
            clock: 0.,
        };
        pass.accept(ambient);
        pass
    }

    /// Takes this step's new fountain births (AmbientStreamsRenderer.accept).
    pub fn accept(&mut self, ambient: &Ambient) {
        let frame = ambient.frame();
        self.clock = frame.time;
        let Some(streams) = &mut self.streams else {
            return;
        };
        for birth in &frame.gpu_births {
            if birth.id <= streams.id || birth.id == 0 || birth.tile > 63 || birth.mirror > 3 {
                continue;
            }
            let slot = ((birth.id - 1) % STREAM_CAPACITY as u64) as usize;
            let (column, row) = ((birth.tile % 8) as f32, (birth.tile / 8) as f32);
            let (u, v) = (column / 8., row / 16. + 0.51);
            let (mu, mv) = (birth.mirror & 1 != 0, birth.mirror & 2 != 0);
            let a = &mut streams.data[slot * STREAM_FLOATS..][..STREAM_FLOATS];
            a[..3].copy_from_slice(&birth.position);
            a[3] = birth.size;
            a[4..7].copy_from_slice(&birth.velocity);
            a[7] = birth.birth_time;
            a[8] = birth.life;
            a[9] = birth.growth;
            a[10..12].copy_from_slice(&birth.alpha);
            a[12] = u + if mu { 0.125 } else { 0. };
            a[13] = v + if mv { 0.0625 } else { 0. };
            a[14] = u + if mu { 0. } else { 0.125 };
            a[15] = v + if mv { 0. } else { 0.0625 };
            a[16..19].copy_from_slice(&birth.color);
            a[19] = birth.floor;
            streams.id = birth.id;
            streams.count = streams
                .count
                .max(slot as u32 + 1)
                .min(STREAM_CAPACITY as u32);
            streams.dirty.push(slot);
        }
    }

    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        ambient: &Ambient,
        camera: &Camera,
        far: f32,
    ) -> (u32, u32) {
        let (sy, cy) = camera.yaw.sin_cos();
        let (sp, cp) = camera.pitch.sin_cos();
        let uniform = EffectUniform {
            right: [cy, 0., -sy, self.clock],
            up: [-sy * sp, cp, -cy * sp, far],
            light: [1., 1., 1., 0.],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
        let (mut draws, mut triangles) = (0, 0);
        if let Some(streams) = &mut self.streams {
            // Coalesce adjacent new slots; aging needs no uploads.
            streams.dirty.sort_unstable();
            streams.dirty.dedup();
            let mut i = 0;
            while i < streams.dirty.len() {
                let first = streams.dirty[i];
                let mut last = first;
                while i + 1 < streams.dirty.len() && streams.dirty[i + 1] == last + 1 {
                    i += 1;
                    last = streams.dirty[i];
                }
                i += 1;
                gpu.queue.write_buffer(
                    &streams.instances,
                    (first * STREAM_FLOATS * 4) as u64,
                    bytemuck::cast_slice(
                        &streams.data[first * STREAM_FLOATS..(last + 1) * STREAM_FLOATS],
                    ),
                );
            }
            streams.dirty.clear();
            if streams.count > 0 {
                draws += 1;
                triangles += streams.count * 2;
            }
        }
        let Some(sprites) = &mut self.sprites else {
            return (draws, triangles);
        };
        // visibleSprites: within the far distance and not behind the camera.
        let frame = ambient.frame();
        let (fx, fy, fz) = (sy * cp, sp, cy * cp);
        let far = far.max(0.);
        self.order.clear();
        for (index, p) in frame.particles.iter().enumerate() {
            let (dx, dy, dz) = (
                p.position[0] - camera.x,
                p.position[1] - camera.y,
                p.position[2] - camera.z,
            );
            let depth = dx * fx + dy * fy + dz * fz;
            if dx * dx + dy * dy + dz * dz <= (far + p.size).powi(2) && depth >= -p.size {
                self.order.push((depth, index));
            }
        }
        self.order
            .sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        sprites.scratch.clear();
        let frames = ambient
            .metadata
            .atlas
            .as_ref()
            .map(|a| a.frames.as_slice())
            .unwrap_or(&[]);
        for (family, additive) in [false, true].into_iter().enumerate() {
            let start = sprites.scratch.len();
            for &(_, index) in &self.order {
                let p = &frame.particles[index];
                if p.additive != additive {
                    continue;
                }
                let Some(rect) = frames.get(p.frame).map(|f| f.rect) else {
                    continue;
                };
                let (w, h) = (sprites.width, sprites.height);
                let pick =
                    |mirror: bool, a: usize, b: usize| if mirror { rect[b] } else { rect[a] };
                sprites.scratch.extend([
                    p.position[0],
                    p.position[1],
                    p.position[2],
                    p.size,
                    (pick(p.mirror[0], 0, 2) + 0.5) / w,
                    (pick(p.mirror[1], 1, 3) + 0.5) / h,
                    (pick(p.mirror[0], 2, 0) + 0.5) / w,
                    (pick(p.mirror[1], 3, 1) + 0.5) / h,
                    sprite_alpha(p, camera),
                    p.lit as u8 as f32,
                ]);
            }
            sprites.counts[family] = ((sprites.scratch.len() - start) / SPRITE_FLOATS) as u32;
        }
        if !sprites.scratch.is_empty() {
            gpu.queue.write_buffer(
                &sprites.instances,
                0,
                bytemuck::cast_slice(&sprites.scratch),
            );
        }
        for count in sprites.counts {
            if count > 0 {
                draws += 1;
                triangles += count * 2;
            }
        }
        (draws, triangles)
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_bind_group(2, &self.draw_group, &[0, 0]);
        pass.set_vertex_buffer(0, self.corners.slice(..));
        if let Some(streams) = self.streams.as_ref().filter(|s| s.count > 0) {
            pass.set_pipeline(&streams.pipeline);
            pass.set_bind_group(1, &streams.group, &[]);
            pass.set_vertex_buffer(1, streams.instances.slice(..));
            pass.draw(0..6, 0..streams.count);
        }
        if let Some(sprites) = &self.sprites {
            pass.set_bind_group(1, &sprites.group, &[]);
            let mut offset = 0u64;
            for (count, pipeline) in sprites
                .counts
                .iter()
                .zip([&sprites.alpha, &sprites.additive])
            {
                if *count == 0 {
                    continue;
                }
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(1, sprites.instances.slice(offset..));
                pass.draw(0..6, 0..*count);
                offset += u64::from(*count) * (SPRITE_FLOATS * 4) as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_fade_scales_alpha_inside_the_fade_distance_only() {
        let sprite = |fade| atv_core::ambient_effects::Sprite {
            position: [3., 0., 4.],
            size: 1.,
            alpha: 100.,
            frame: 0,
            mirror: [false; 2],
            additive: false,
            lit: false,
            fade_distance: fade,
        };
        let camera = Camera::default();
        assert_eq!(sprite_alpha(&sprite(None), &camera), 100.);
        assert_eq!(sprite_alpha(&sprite(Some(10.)), &camera), 25.);
        assert_eq!(sprite_alpha(&sprite(Some(5.)), &camera), 100.);
    }
}
