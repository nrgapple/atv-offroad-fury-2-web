//! Authored sun flares (atv_game::sun_flare; research/SUN_FLARES.md).
//!
//! Both games draw the flare only while the sun is inside the view frustum
//! (ATV2 0020e3a0, MX 004f3770), centred on it. They z-test a square around
//! the sun against the scene's depth, average it on the GS into the flare
//! render target's alpha (ATV2 003413e0/003417b0, MX 001dc0c0), and draw
//! the target additively, its alpha modulating the vertex alpha.
//!
//! The port tests the same square with 4x4 occlusion queries at the end of
//! the scene pass: every depth-writing draw (terrain, scenery, plants,
//! riders) hides the sun. A query is read two frames later, resolved
//! straight into the uniform the sprite reads, so nothing waits on the GPU.
//! WebGL only releases query results between frames; until a sentinel
//! query reports in, the share falls back to 16 rays across the square
//! against the terrain heightfield.
use crate::world::{Blend, PipelineDesc, World};
use crate::{Gpu, texture};
use atv_core::terrain::Heightfield;
use atv_game::sun_flare::SunFlare;
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Rays, and occlusion queries, per side of the sample square.
const RAYS: usize = 4;
/// Queries per flare: the RAYS x RAYS cells, then an always-passing
/// sentinel that tells a reported result from an unavailable one.
const QUERIES: u32 = (RAYS * RAYS) as u32 + 1;
/// Query sets in flight: a set is resolved two frames after it is drawn.
const RING: usize = 3;
/// The original's screen height, in pixels.
const PS2_HEIGHT: f32 = 448.;
/// The rays stop after this distance (ft): beyond it nothing is drawn.
const REACH: f64 = 12_000.;

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    center: [f32; 4],
    /// RGB over 128, then the GS vertex alpha (0-255).
    color: [f32; 4],
    /// The sample square in NDC: centre, half size.
    probe: [f32; 4],
    /// Query results are resolved this frame (1 or 0), then the terrain
    /// rays' visible share.
    occlusion: [f32; 4],
}

struct Flare {
    source: SunFlare,
    texture_group: wgpu::BindGroup,
    /// Visible share of the terrain rays this frame.
    coverage: f32,
    /// The sun is in front and inside the viewport this frame.
    visible: bool,
    ndc: [f32; 2],
}

/// One flare as last prepared (tools/test_engine_sun_flares.mjs).
#[derive(Clone, Copy, Debug, Default)]
pub struct FlareState {
    pub visible: bool,
    /// The terrain rays' visible share (the fallback).
    pub coverage: f32,
    /// The sun in NDC.
    pub ndc: [f32; 2],
    /// The sprite read depth-query results this frame.
    pub queried: bool,
    /// The query cells' visible share, last read back (diagnostics only;
    /// None until a sentinel has reported).
    pub depth_coverage: Option<f32>,
}

/// Diagnostics read the query results back (only after [`FlarePass::state`]
/// asks, so play never maps a buffer): copied in one frame, mapped in the
/// next once that copy is submitted, then read.
#[derive(Clone, Copy, PartialEq)]
enum Readback {
    Idle,
    Copied,
    Mapping,
}

pub struct FlarePass {
    sprite: wgpu::RenderPipeline,
    probe: wgpu::RenderPipeline,
    flares: Vec<Flare>,
    uniforms: wgpu::Buffer,
    stride: u64,
    draw_group: wgpu::BindGroup,
    /// Per flare at `stride`: its query results, resolved for the sprite.
    results: wgpu::Buffer,
    sets: [wgpu::QuerySet; RING],
    /// Sets drawn and not yet resolved.
    pending: [bool; RING],
    /// The set this frame's probes write.
    slot: usize,
    queried: bool,
    readback: wgpu::Buffer,
    reading: Readback,
    /// 0 pending, 1 mapped, 2 failed.
    mapped: Arc<AtomicU8>,
    wanted: Cell<bool>,
    depth_coverage: RefCell<Vec<Option<f32>>>,
    /// The highest ground, found on the first prepare.
    top: Option<f64>,
    _textures: Vec<wgpu::Texture>,
}

/// Whether the ray from `eye` along unit `direction` meets the terrain.
/// Steps grow with distance; the ray ends once above the highest ground.
fn terrain_blocks(ground: &Heightfield, top: f64, eye: [f64; 3], direction: [f64; 3]) -> bool {
    let size = [
        (ground.width.saturating_sub(1)) as f64 * ground.spacing,
        (ground.depth.saturating_sub(1)) as f64 * ground.spacing,
    ];
    let mut t = ground.spacing;
    while t < REACH {
        let p = [
            eye[0] + direction[0] * t,
            eye[1] + direction[1] * t,
            eye[2] + direction[2] * t,
        ];
        if (p[1] > top && direction[1] >= 0.)
            || p[0] < 0.
            || p[2] < 0.
            || p[0] > size[0]
            || p[2] > size[1]
        {
            return false;
        }
        if ground.height(p[0], p[2]) > p[1] {
            return true;
        }
        t += (ground.spacing * 0.5).max(t * 0.01);
    }
    false
}

/// The visible share of a RAYS x RAYS grid spanning `half` tangent units
/// either side of the unit `toward` sun direction.
fn coverage(ground: &Heightfield, top: f64, eye: [f32; 3], toward: [f32; 3], half: f32) -> f32 {
    let t = toward.map(f64::from);
    // A basis around the sun direction (the sun is never straight up here:
    // a vertical direction falls back to world X).
    let mut right = [t[2], 0., -t[0]];
    let length = (right[0] * right[0] + right[2] * right[2]).sqrt();
    right = if length > 1e-6 {
        right.map(|c| c / length)
    } else {
        [1., 0., 0.]
    };
    let up = [
        t[1] * right[2] - t[2] * right[1],
        t[2] * right[0] - t[0] * right[2],
        t[0] * right[1] - t[1] * right[0],
    ];
    let eye = eye.map(f64::from);
    let mut seen = 0;
    for row in 0..RAYS {
        for column in 0..RAYS {
            let offset = |i: usize| ((i as f64 + 0.5) / RAYS as f64 * 2. - 1.) * f64::from(half);
            let (dx, dy) = (offset(column), offset(row));
            let d: [f64; 3] = std::array::from_fn(|k| t[k] + right[k] * dx + up[k] * dy);
            let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if !terrain_blocks(ground, top, eye, d.map(|c| c / n)) {
                seen += 1;
            }
        }
    }
    seen as f32 / (RAYS * RAYS) as f32
}

impl FlarePass {
    /// None when the scene has no flares.
    pub fn new(gpu: &Gpu, world: &World, flares: &[SunFlare]) -> Option<Self> {
        if flares.is_empty() {
            return None;
        }
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("flare.wgsl"));
        let sprite = world.pipeline(
            gpu,
            PipelineDesc {
                label: "sun flare",
                shader: &shader,
                vertex: "vs_sprite",
                fragment: "fs_sprite",
                buffers: &[],
                blend: Blend::Additive,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::Always,
                per_draw: true,
            },
        );
        // Probes add nothing to the colour; only their depth test counts.
        let probe = world.pipeline(
            gpu,
            PipelineDesc {
                label: "sun flare probe",
                shader: &shader,
                vertex: "vs_probe",
                fragment: "fs_probe",
                buffers: &[],
                blend: Blend::Additive,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                per_draw: true,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun flare"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut textures = Vec::new();
        let flares: Vec<Flare> = flares
            .iter()
            .map(|source| {
                let image = &source.image;
                let texture =
                    texture::upload_flat(gpu, image.width, image.height, &image.rgba, "sun flare");
                let texture_group = world.texture_group(gpu, &texture, &sampler);
                textures.push(texture);
                Flare {
                    source: source.clone(),
                    texture_group,
                    coverage: 0.,
                    visible: false,
                    ndc: [0.; 2],
                }
            })
            .collect();
        let size = std::mem::size_of::<Uniform>() as u64;
        // Query resolves land on 256-byte boundaries.
        let stride = size
            .next_multiple_of(world.uniform_alignment)
            .max(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT)
            .next_multiple_of(world.uniform_alignment);
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun flare uniforms"),
            size: stride * flares.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // One u64 per query, padded to whole vec4<u32>s for the shader.
        let result_size = u64::from(QUERIES.next_multiple_of(2)) * 8;
        let results = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun flare query results"),
            size: stride * flares.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::QUERY_RESOLVE
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun flare query readback"),
            size: results.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sets = std::array::from_fn(|_| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("sun flare occlusion"),
                ty: wgpu::QueryType::Occlusion,
                count: QUERIES * flares.len() as u32,
            })
        });
        let draw_group = world.draw_group(
            gpu,
            &uniforms,
            size as usize,
            &results,
            result_size as usize,
        );
        let count = flares.len();
        Some(Self {
            sprite,
            probe,
            flares,
            uniforms,
            stride,
            draw_group,
            results,
            sets,
            pending: [false; RING],
            slot: 0,
            queried: false,
            readback,
            reading: Readback::Idle,
            mapped: Arc::new(AtomicU8::new(0)),
            wanted: Cell::new(false),
            depth_coverage: RefCell::new(vec![None; count]),
            top: None,
            _textures: textures,
        })
    }

    /// Projects, occludes and writes this frame's sprites, and resolves the
    /// queries drawn two frames ago for them. Call once per single-view
    /// scene pass, which must then run [`Self::probe`] with
    /// [`Self::query_set`].
    /// `view_projection` is column-major (clip space already turned);
    /// `size` the scene target in pixels; `turned` swaps which target axis
    /// is the viewer's height; `focal_length` is the projection's cot(half
    /// vertical field of view).
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        view_projection: &[f32; 16],
        size: (u32, u32),
        turned: bool,
        eye: [f32; 3],
        focal_length: f32,
        ground: &Heightfield,
    ) {
        let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
        // One PS2 pixel in NDC, scaled with the viewer's screen height.
        let viewer_height = if turned { width } else { height };
        let pixel = [
            2. * viewer_height / PS2_HEIGHT / width,
            2. * viewer_height / PS2_HEIGHT / height,
        ];
        let top = *self.top.get_or_insert_with(|| {
            f64::from(ground.heights.iter().copied().fold(f32::MIN, f32::max))
        });
        // The oldest set in the ring was drawn two frames ago.
        self.slot = (self.slot + 1) % RING;
        let oldest = (self.slot + 1) % RING;
        self.queried = std::mem::take(&mut self.pending[oldest]);
        if self.queried {
            for i in 0..self.flares.len() as u32 {
                encoder.resolve_query_set(
                    &self.sets[oldest],
                    i * QUERIES..(i + 1) * QUERIES,
                    &self.results,
                    u64::from(i) * self.stride,
                );
            }
        }
        self.pending[self.slot] = true;
        self.read_back(encoder);
        let m = view_projection;
        for (i, flare) in self.flares.iter_mut().enumerate() {
            let [x, y, z] = flare.source.toward_sun;
            // The sun is a point at infinity: w = 0.
            let clip = [
                m[0] * x + m[4] * y + m[8] * z,
                m[1] * x + m[5] * y + m[9] * z,
                m[3] * x + m[7] * y + m[11] * z,
            ];
            let ndc = [clip[0] / clip[2], clip[1] / clip[2]];
            flare.ndc = ndc;
            // 0016eea0: drawn only while the sun projects inside the view.
            flare.visible = clip[2] > 0. && ndc.iter().all(|v| v.is_finite() && v.abs() <= 1.);
            flare.coverage = if flare.visible {
                // Half the sample square in tangent units: one PS2 pixel
                // spans 2 / 448 / focal_length.
                let half = flare.source.sample_size * 0.5 * 2. / PS2_HEIGHT / focal_length;
                coverage(ground, top, eye, flare.source.toward_sun, half)
            } else {
                0.
            };
            let half = flare.source.size * 0.5;
            // The sample square stays on screen (ATV2 002cf8c0, MX 001dbeb0).
            let probe_half = [
                (flare.source.sample_size * 0.5 * pixel[0]).min(1.),
                (flare.source.sample_size * 0.5 * pixel[1]).min(1.),
            ];
            let probe_center: [f32; 2] = std::array::from_fn(|k| {
                let c = if ndc[k].is_finite() { ndc[k] } else { 0. };
                c.clamp(probe_half[k] - 1., 1. - probe_half[k])
            });
            let uniform = Uniform {
                center: [ndc[0], ndc[1], half * pixel[0], half * pixel[1]],
                color: [
                    flare.source.color[0] / 128.,
                    flare.source.color[1] / 128.,
                    flare.source.color[2] / 128.,
                    flare.source.alpha,
                ],
                probe: [
                    probe_center[0],
                    probe_center[1],
                    probe_half[0],
                    probe_half[1],
                ],
                occlusion: [f32::from(u8::from(self.queried)), flare.coverage, 0., 0.],
            };
            gpu.queue.write_buffer(
                &self.uniforms,
                i as u64 * self.stride,
                bytemuck::bytes_of(&uniform),
            );
        }
    }

    /// Advances the diagnostics readback by one step.
    fn read_back(&mut self, encoder: &mut wgpu::CommandEncoder) {
        match self.reading {
            Readback::Idle if self.wanted.get() && self.queried => {
                encoder.copy_buffer_to_buffer(&self.results, 0, &self.readback, 0, None);
                self.reading = Readback::Copied;
            }
            Readback::Copied => {
                let mapped = self.mapped.clone();
                mapped.store(0, Ordering::Release);
                self.readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        mapped.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
                    });
                self.reading = Readback::Mapping;
            }
            Readback::Mapping => match self.mapped.load(Ordering::Acquire) {
                1 => {
                    if let Ok(data) = self.readback.get_mapped_range(..) {
                        let words: Vec<u32> = bytemuck::pod_collect_to_vec(&data[..]);
                        let mut shares = self.depth_coverage.borrow_mut();
                        for (i, share) in shares.iter_mut().enumerate() {
                            let base = i * (self.stride / 4) as usize;
                            let passed = |q: usize| {
                                words.get(base + q * 2..base + q * 2 + 2) != Some(&[0, 0])
                            };
                            *share = passed(QUERIES as usize - 1).then(|| {
                                (0..QUERIES as usize - 1).filter(|&q| passed(q)).count() as f32
                                    / (QUERIES - 1) as f32
                            });
                        }
                    }
                    self.readback.unmap();
                    self.reading = Readback::Idle;
                }
                2 => self.reading = Readback::Idle,
                _ => {}
            },
            Readback::Idle => {}
        }
    }

    pub fn state(&self) -> Vec<FlareState> {
        self.wanted.set(true);
        let depth = self.depth_coverage.borrow();
        self.flares
            .iter()
            .zip(depth.iter())
            .map(|(f, &depth_coverage)| FlareState {
                visible: f.visible,
                coverage: f.coverage,
                ndc: f.ndc,
                queried: self.queried,
                depth_coverage,
            })
            .collect()
    }

    /// The set this frame's [`Self::probe`] writes, for the scene pass.
    pub fn query_set(&self) -> &wgpu::QuerySet {
        &self.sets[self.slot]
    }

    /// The depth probes; after every depth-writing draw of the scene pass.
    pub fn probe(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_pipeline(&self.probe);
        for (i, flare) in self.flares.iter().enumerate() {
            let offset = (i as u64 * self.stride) as u32;
            pass.set_bind_group(1, &flare.texture_group, &[]);
            pass.set_bind_group(2, &self.draw_group, &[offset, offset]);
            for cell in 0..QUERIES {
                pass.begin_occlusion_query(i as u32 * QUERIES + cell);
                pass.draw(cell * 6..cell * 6 + 6, 0..1);
                pass.end_occlusion_query();
            }
        }
    }

    /// The visible sprites; last in the scene pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_pipeline(&self.sprite);
        for (i, flare) in self.flares.iter().enumerate() {
            if !flare.visible || flare.source.alpha <= 0. {
                continue;
            }
            let offset = (i as u64 * self.stride) as u32;
            pass.set_bind_group(1, &flare.texture_group, &[]);
            pass.set_bind_group(2, &self.draw_group, &[offset, offset]);
            pass.draw(0..6, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 x 64 cell field, 4 ft spacing, flat at 0 with a 100 ft wall,
    /// flat on top, over x 124-132.
    fn wall() -> Heightfield {
        let (width, depth) = (65, 65);
        let heights = (0..width * depth)
            .map(|i| {
                if (31..=33).contains(&(i % width)) {
                    100.
                } else {
                    0.
                }
            })
            .collect();
        Heightfield {
            width,
            depth,
            spacing: 4.,
            heights,
        }
    }

    #[test]
    fn terrain_hides_the_sun_behind_a_wall_only() {
        let ground = wall();
        let top = 100.;
        let eye = [16., 10., 128.];
        // Low sun behind the wall (+X): hidden. High sun: clear.
        assert_eq!(coverage(&ground, top, eye, [1., 0.05, 0.], 0.01), 0.);
        let high = {
            let d = [1f32, 2., 0.];
            let n = (d[0] * d[0] + d[1] * d[1]).sqrt();
            d.map(|c| c / n)
        };
        assert_eq!(coverage(&ground, top, eye, high, 0.01), 1.);
        // Sun the other way (-X), nothing between: clear.
        assert_eq!(coverage(&ground, top, eye, [-1., 0.05, 0.], 0.01), 1.);
    }

    #[test]
    fn grazing_the_wall_top_gives_partial_coverage() {
        let ground = wall();
        // The wall's near top edge (y 100 at x 124) seen from x 16 at y 10.
        let rise = 90. / 108.;
        let d = [1f32, rise, 0.];
        let n = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let c = coverage(&ground, 100., [16., 10., 128.], d.map(|v| v / n), 0.05);
        assert!(c > 0. && c < 1., "coverage {c}");
    }
}
