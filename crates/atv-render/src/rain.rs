//! Remastered rain (authored, not from the original game). The shipped game
//! draws rain only as its placed, scrolling rain cards and Freeport's type 8
//! emitter; its ScreenRain lens/world drops are never created
//! (research/WEATHER.md). Under the Remastered preset only, rainy venues
//! gain falling streaks around the camera, splash rings on the ground and
//! water ahead of it and, on the live riding view, beads on the lens.
//! Streaks are placed in the shader from the instance index; splashes and
//! beads are fixed pools stepped here.
use crate::{
    Camera, Gpu, texture,
    world::{Blend, PipelineDesc, World},
};
use wgpu::util::DeviceExt;

const STREAKS: u32 = 3_200;
const SPLASHES: usize = 64;
/// ScreenRain's default `MaxLensDrops`.
const BEADS: usize = 20;
const BOX_HALF_WIDTH: f32 = 45.;
const BOX_HEIGHT: f32 = 40.;
const SPLASH_LIFE: f32 = 0.4;
const FALL: [f32; 3] = [2.5, -32., 1.5];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RainUniform {
    eye: [f32; 4],
    fall: [f32; 4],
    color: [f32; 4],
    shape: [f32; 4],
}

/// Small deterministic generator for the authored pools.
#[derive(Clone, Copy)]
struct Random(u32);
impl Random {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[derive(Clone, Copy, Default)]
struct Bead {
    /// Clip-space centre.
    at: [f32; 2],
    radius: f32,
    life: f32,
    age: f32,
    /// Clip units per second downward once heavy enough to run.
    slide: f32,
}

/// A splash ring: where it landed, when (seconds on the rain clock), and
/// the ground's slope there (rise per foot east and north) so the ring lies
/// on a hillside instead of cutting into it.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Splash {
    at: [f32; 3],
    birth: f32,
    slope: [f32; 2],
    unused: [f32; 2],
}

const HIDDEN: Splash = Splash {
    at: [0.; 3],
    birth: -10.,
    slope: [0.; 2],
    unused: [0.; 2],
};

/// What the pass drew last frame (diagnostics and tests).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RainState {
    /// The rain clock, seconds. It stops with the world.
    pub clock: f32,
    pub streaks: u32,
    /// Splash rings in their visible life.
    pub splashes: u32,
    /// Lens beads drawn (0 when the view takes none).
    pub beads: u32,
}

/// The splash and lens-bead pools, stepped on the rain clock.
pub struct Drops {
    random: Random,
    splashes: [Splash; SPLASHES],
    beads: [Bead; BEADS],
    /// Beads in use: the lens arrives wet, every bead at a random age.
    live: usize,
    clock: f32,
}

impl Drops {
    pub fn new() -> Self {
        Self {
            random: Random(0x9e37_79b9),
            splashes: [HIDDEN; SPLASHES],
            beads: [Bead::default(); BEADS],
            live: 0,
            clock: 0.,
        }
    }

    /// Advances to `clock` seconds: expired splashes land again on the
    /// ground ahead of the camera, expired beads form again on the lens.
    /// `ground` is the height a drop lands on (terrain or water).
    pub fn step(&mut self, clock: f32, camera: &Camera, ground: impl Fn(f32, f32) -> f32) {
        let dt = (clock - self.clock).clamp(0., 0.1);
        // The clock wraps (the vegetation wind's 4096 s): start over.
        if clock < self.clock {
            self.splashes.iter_mut().for_each(|s| s.birth = -10.);
        }
        self.clock = clock;
        // A stopped clock (pause, photo mode) keeps every drop where it is,
        // wherever the camera goes.
        if dt <= 0. {
            return;
        }
        let (sy, cy) = camera.yaw.sin_cos();
        for index in 0..SPLASHES {
            if clock - self.splashes[index].birth < SPLASH_LIFE {
                continue;
            }
            // Spread over a fan 2-30 ft ahead, staggered so they never
            // land in step.
            let angle = (self.random.next() - 0.5) * 2.2;
            let distance = 2. + 28. * self.random.next().sqrt();
            let (sa, ca) = angle.sin_cos();
            let (dx, dz) = (sy * ca + cy * sa, cy * ca - sy * sa);
            let (x, z) = (camera.x + dx * distance, camera.z + dz * distance);
            let y = ground(x, z);
            let birth = clock + (self.random.next() - 0.5) * SPLASH_LIFE;
            let slope = [
                ground(x + 0.5, z) - ground(x - 0.5, z),
                ground(x, z + 0.5) - ground(x, z - 0.5),
            ];
            // Off the terrain, or on a face too steep to hold a ring, the
            // slot stays hidden and tries again.
            let lands = y.is_finite() && slope.iter().all(|s| s.abs() <= 1.2);
            self.splashes[index] = if lands {
                Splash {
                    at: [x, y, z],
                    birth,
                    slope,
                    unused: [0.; 2],
                }
            } else {
                HIDDEN
            };
        }
        let fresh = self.live == 0;
        self.live = BEADS;
        for index in 0..self.live {
            let mut bead = self.beads[index];
            bead.age += dt;
            if bead.age >= bead.life {
                bead = Bead {
                    at: [self.random.next() * 2. - 1., self.random.next() * 2. - 1.],
                    radius: 0.012 + 0.03 * self.random.next() * self.random.next(),
                    life: 1.5 + 3.5 * self.random.next(),
                    age: 0.,
                    slide: 0.,
                };
                // Large beads run down the glass.
                if bead.radius > 0.03 {
                    bead.slide = 0.05 + 0.25 * self.random.next();
                }
                if fresh {
                    bead.age = self.random.next() * bead.life;
                }
            }
            bead.at[1] -= bead.slide * dt;
            self.beads[index] = bead;
        }
    }

    /// Clip-space bead instances: x, y, radius, opacity. A view that
    /// takes no beads (`lens` false) gets none.
    fn bead_instances(&self, lens: bool) -> [[f32; 4]; BEADS] {
        std::array::from_fn(|index| {
            let bead = &self.beads[index];
            if !lens || index >= self.live || bead.life <= 0. {
                return [0.; 4];
            }
            let fade_in = (bead.age / 0.15).min(1.);
            let fade_out = ((bead.life - bead.age) / 0.8).clamp(0., 1.);
            [bead.at[0], bead.at[1], bead.radius, fade_in * fade_out]
        })
    }
}

impl Default for Drops {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RainPass {
    streaks: wgpu::RenderPipeline,
    splashes: wgpu::RenderPipeline,
    lens: wgpu::RenderPipeline,
    splash_buffer: wgpu::Buffer,
    bead_buffer: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    draw_group: wgpu::BindGroup,
    texture_group: wgpu::BindGroup,
    drops: Drops,
    state: RainState,
}

impl RainPass {
    pub fn new(gpu: &Gpu, world: &World) -> Self {
        let device = &gpu.device;
        let white = texture::upload_flat(gpu, 1, 1, &[255; 4], "rain");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rain"),
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rain uniforms"),
            size: std::mem::size_of::<RainUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("rain.wgsl"));
        let bead = [Some(wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4],
        })];
        let splash = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Splash>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
        })];
        let pipeline = |label, vertex, fragment, buffers, depth_compare| {
            world.pipeline(
                gpu,
                PipelineDesc {
                    label,
                    shader: &shader,
                    vertex,
                    fragment,
                    buffers,
                    blend: Blend::Alpha,
                    depth_write: false,
                    depth_compare,
                    per_draw: true,
                },
            )
        };
        let instances = |label, bytes: usize| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: &vec![0u8; bytes],
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            })
        };
        Self {
            streaks: pipeline(
                "rain streaks",
                "vs_streak",
                "fs_streak",
                &[],
                wgpu::CompareFunction::Less,
            ),
            splashes: pipeline(
                "rain splashes",
                "vs_splash",
                "fs_splash",
                &splash,
                wgpu::CompareFunction::Less,
            ),
            lens: pipeline(
                "rain lens",
                "vs_lens",
                "fs_lens",
                &bead,
                wgpu::CompareFunction::Always,
            ),
            splash_buffer: instances("rain splashes", std::mem::size_of::<Splash>() * SPLASHES),
            bead_buffer: instances("rain beads", 16 * BEADS),
            draw_group: world.draw_group(
                gpu,
                &uniforms,
                std::mem::size_of::<RainUniform>(),
                &uniforms,
                16,
            ),
            uniforms,
            texture_group: world.texture_group(gpu, &white, &sampler),
            drops: Drops::new(),
            state: RainState::default(),
        }
    }

    /// What the latest prepared frame draws.
    pub fn state(&self) -> RainState {
        self.state
    }

    /// Steps the pools to `clock` (paused with the world) and uploads them.
    /// `light` is the venue's ambient plus sun colour; drops take a little
    /// of the sky and fog so night and dusk rain stay dim. `lens` is false
    /// for a view that is not the live riding camera (a menu, photo mode, a
    /// replay, a split-screen view): it gets no beads.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        camera: &Camera,
        aspect: f32,
        clock: f32,
        light: [f32; 3],
        lens: bool,
        ground: impl Fn(f32, f32) -> f32,
    ) -> (u32, u32) {
        self.drops.step(clock, camera, ground);
        let tint = light.map(|c| (0.35 + 0.5 * c).clamp(0.3, 0.95));
        let uniform = RainUniform {
            eye: [camera.x, camera.y, camera.z, clock],
            fall: [FALL[0], FALL[1], FALL[2], 0.32],
            color: [tint[0], tint[1], tint[2] * 1.05, aspect.max(0.1)],
            shape: [BOX_HALF_WIDTH, BOX_HEIGHT, STREAKS as f32, SPLASH_LIFE],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));
        gpu.queue.write_buffer(
            &self.splash_buffer,
            0,
            bytemuck::cast_slice(&self.drops.splashes),
        );
        let beads = self.drops.bead_instances(lens);
        gpu.queue.write_buffer(
            &self.bead_buffer,
            0,
            bytemuck::cast_slice(&beads),
        );
        self.state = RainState {
            clock,
            streaks: STREAKS,
            splashes: self
                .drops
                .splashes
                .iter()
                .filter(|s| (0. ..SPLASH_LIFE).contains(&(clock - s.birth)))
                .count() as u32,
            beads: beads.iter().filter(|b| b[3] > 0.).count() as u32,
        };
        (3, (STREAKS + SPLASHES as u32 + BEADS as u32) * 2)
    }

    /// World streaks and splashes, depth tested against the scene, then
    /// the lens beads over everything.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, world: &World) {
        pass.set_bind_group(0, &world.globals_group, &[]);
        pass.set_bind_group(1, &self.texture_group, &[]);
        pass.set_bind_group(2, &self.draw_group, &[0, 0]);
        pass.set_pipeline(&self.streaks);
        pass.draw(0..6, 0..STREAKS);
        pass.set_pipeline(&self.splashes);
        pass.set_vertex_buffer(0, self.splash_buffer.slice(..));
        pass.draw(0..6, 0..SPLASHES as u32);
        pass.set_pipeline(&self.lens);
        pass.set_vertex_buffer(0, self.bead_buffer.slice(..));
        pass.draw(0..6, 0..BEADS as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMERA: Camera = Camera {
        x: 100.,
        y: 20.,
        z: 50.,
        yaw: 0.,
        pitch: 0.,
    };

    #[test]
    fn splashes_land_ahead_on_the_ground_and_beads_fill_the_lens() {
        let mut drops = Drops::new();
        for step in 1..=40 {
            drops.step(step as f32 / 60., &CAMERA, |x, _| x * 0.1);
        }
        assert_eq!(drops.live, BEADS);
        for splash in drops.splashes {
            let [x, y, z] = splash.at;
            // yaw 0 looks along +Z: every splash is in front of the camera.
            assert!(z > CAMERA.z && (x - CAMERA.x).abs() < 31.);
            assert!((y - x * 0.1).abs() < 1e-4);
            assert!(splash.birth <= 40. / 60. + SPLASH_LIFE);
            // The ring takes the ground's slope: 0.1 ft per foot east.
            assert!((splash.slope[0] - 0.1).abs() < 1e-3 && splash.slope[1].abs() < 1e-3);
        }
        let beads = drops.bead_instances(true);
        assert!(beads.iter().all(|b| b[0].abs() <= 1. && b[2] > 0.));
        assert!(beads.iter().any(|b| b[3] > 0.));
        // A view that is not the live ride takes no beads.
        assert!(drops.bead_instances(false).iter().all(|b| b == &[0.; 4]));
    }

    #[test]
    fn a_stopped_clock_holds_every_drop() {
        let mut drops = Drops::new();
        for step in 1..=40 {
            drops.step(step as f32 / 60., &CAMERA, |_, _| 0.);
        }
        let (beads, splashes) = (drops.bead_instances(true), drops.splashes);
        // Paused, and the photo camera turned round and moved away: nothing
        // ages and no splash follows it.
        let elsewhere = Camera {
            x: -400.,
            yaw: 3.,
            ..CAMERA
        };
        drops.step(40. / 60., &elsewhere, |_, _| 5.);
        assert_eq!(beads, drops.bead_instances(true));
        assert_eq!(splashes, drops.splashes);
    }

    #[test]
    fn nothing_splashes_off_the_terrain_or_on_a_wall() {
        let mut drops = Drops::new();
        drops.step(1., &CAMERA, |_, _| f32::NAN);
        assert!(drops.splashes.iter().all(|s| s == &HIDDEN));
        // A face steeper than 1.2 ft per foot holds no ring.
        let mut drops = Drops::new();
        drops.step(1., &CAMERA, |x, _| x * 3.);
        assert!(drops.splashes.iter().all(|s| s == &HIDDEN));
    }
}
