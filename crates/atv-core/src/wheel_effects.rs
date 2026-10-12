//! Original rear tire dust/dirt selector, CPU particle updates and atlas input.
//! Rendered billboards adapt GS packets; no original assets are embedded.
use crate::{
    Result,
    engine_audio::OriginalRandom,
    physics_data::PhysicsData,
    wheel::{Vec3, add, dot, scale, sub},
};
use serde::{Deserialize, Serialize};

fn text(bytes: &[u8]) -> Result<PhysicsData> {
    if bytes.len() > 1024 * 1024 {
        return Err("particle text exceeds limit".into());
    }
    let source = std::str::from_utf8(bytes).map_err(|_| "particle text is not UTF-8")?;
    let cleaned: String = source
        .lines()
        .map(|l| {
            format!(
                "{}\n",
                if l.trim().starts_with(':') {
                    ""
                } else {
                    l.split(';').next().unwrap_or_default().trim()
                }
            )
        })
        .collect();
    PhysicsData::parse(cleaned.as_bytes())
}
fn integer(data: &PhysicsData, section: &str, key: &str) -> Result<usize> {
    data.first(section, key)?
        .value
        .parse()
        .map_err(|_| format!("invalid particle integer {section}/{key}"))
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Group {
    pub first: usize,
    pub end: usize,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct AtlasFrame {
    pub rect: [f32; 4],
}
#[derive(Debug, Clone, Serialize)]
pub struct Atlas {
    pub texture: String,
    pub frames: Vec<AtlasFrame>,
    pub dust: [Group; 8],
    pub spray: [Group; 8],
    pub chunk: [Group; 8],
}
impl Atlas {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let data = text(bytes)?;
        let texture = data.first("General Info", "TextureMap")?.value.clone();
        if texture.contains(['/', '\\', ':']) || texture.len() > 128 {
            return Err("unsafe particle atlas texture".into());
        }
        let count = integer(&data, "General Info", "NumFrames")?;
        let groups = integer(&data, "General Info", "NumGroups")?;
        if !(1..=4096).contains(&count) || !(26..=128).contains(&groups) {
            return Err("unsupported particle atlas counts".into());
        }
        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            let section = format!("Frame{i}");
            let rect = ["UVLeft", "UVTop", "UVRight", "UVBottom"]
                .map(|k| data.first_scalar(&section, k))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            let rect: [f32; 4] = rect.try_into().unwrap();
            if rect.iter().any(|v| *v < 0. || *v > 4096.) || rect[0] > rect[2] || rect[1] > rect[3]
            {
                return Err("invalid particle atlas rectangle".into());
            }
            frames.push(AtlasFrame { rect });
        }
        let get_groups = |start: usize, type_id: usize| -> Result<[Group; 8]> {
            let mut found = Vec::new();
            for i in start..start + 8 {
                let section = format!("Group{i}");
                if integer(&data, &section, "TypeID")? != type_id {
                    return Err("unexpected particle group type".into());
                }
                let first = integer(&data, &section, "FirstFrame")?;
                let last = integer(&data, &section, "LastFrame")?;
                if first > last || last >= count {
                    return Err("invalid particle group range".into());
                }
                // Preserve stored LastFrame: native emitters wrap on >= this value.
                found.push(Group { first, end: last });
            }
            Ok(found.try_into().unwrap())
        };
        Ok(Self {
            texture,
            dust: get_groups(0, 1)?,
            spray: get_groups(10, 4)?,
            chunk: get_groups(18, 5)?,
            frames,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Emitter {
    pub life: f32,
    pub size: f32,
    pub growth: f32,
    pub rate: f32,
    pub age: f32,
    pub velocity: Vec3,
    pub flags: u32,
    pub gravity: f32,
}
impl Emitter {
    pub fn parse(bytes: &[u8], initial_flags: u32) -> Result<Self> {
        let data = text(bytes)?;
        let scalar = |key| data.first_scalar("General Info", key);
        let mut flags = initial_flags;
        for (key, bit) in [
            ("Fades", 2),
            ("HasVelocity", 4),
            ("Grows", 8),
            ("FadeNonLinear", 0x1000),
            ("Gravity", 0x20),
        ] {
            let value = data
                .first("Properties", key)
                .map(|v| v.value.as_str())
                .unwrap_or("F");
            match value {
                "T" | "1" => flags |= bit,
                "F" | "0" => flags &= !bit,
                _ => return Err("invalid emitter boolean".into()),
            }
        }
        let value = Self {
            life: scalar("ParticleLife")?,
            size: scalar("InitialSize")?,
            growth: scalar("Growth")?,
            rate: scalar("EmitRate")?,
            age: scalar("AgeAtBirth")?,
            velocity: data.vector3("General Info", "Velocity")?,
            flags,
            gravity: data
                .first_scalar("General Info", "GravityMultiplier")
                .unwrap_or(3.),
        };
        if ![
            value.life,
            value.size,
            value.growth,
            value.rate,
            value.age,
            value.gravity,
        ]
        .iter()
        .all(|v| v.is_finite())
            || value.life <= 0.
            || value.size <= 0.
            || value.rate <= 0.
            || value.rate > 10000.
            || !(0. ..=1.).contains(&value.age)
        {
            return Err("invalid particle emitter values".into());
        }
        Ok(value)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub light_color: Vec3,
    pub atlas: Atlas,
    pub dust: Emitter,
    pub chunk: Emitter,
    pub spray: Emitter,
}
impl Config {
    pub fn parse(prt: &[u8], dust: &[u8], chunk: &[u8], spray: &[u8]) -> Result<Self> {
        let mut chunk = Emitter::parse(chunk, 100)?;
        chunk.flags = 100;
        let mut spray = Emitter::parse(spray, 0x2e)?;
        spray.flags = 0x2e;
        Ok(Self {
            light_color: [0.; 3],
            atlas: Atlas::parse(prt)?,
            dust: Emitter::parse(dust, 0x100c)?,
            chunk,
            spray,
        })
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Input {
    pub time: f32,
    pub dt: f32,
    pub tick: u64,
    pub centers: [Vec3; 4],
    pub surfaces: [usize; 4],
    pub terrain_heights: [Option<f32>; 4],
    pub water_heights: [Option<f32>; 4],
    #[serde(default)]
    pub mud: [bool; 4],
    pub wheel_tangential_speeds: [f32; 4],
    pub grounded: [bool; 4],
    pub throttle: f32,
    pub steering_angle: f32,
    pub linear_velocity: Vec3,
    pub reset_count: u32,
    #[serde(default)]
    pub placement_epoch: u64,
    #[serde(default)]
    pub mud_heights: [Option<f32>; 4],
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Selection {
    pub dust: bool,
    pub dirt: bool,
    pub wet: bool,
    pub velocity: Vec3,
    pub life: f32,
}
/// 001d7620, supplied original geometry/water and caller eligibility.
#[allow(clippy::too_many_arguments)] // mirrors one recovered selector call
pub fn select(
    center: Vec3,
    terrain: f32,
    water: Option<f32>,
    mud: bool,
    suppressed: bool,
    tangent: f32,
    throttle: f32,
    angle: f32,
    velocity: Vec3,
    dirt_enabled: bool,
) -> Selection {
    let ground = center[1] - terrain;
    let wet = mud
        || water.is_some_and(|height| {
            let depth = center[1] - height;
            depth < ground && (-1.6..=1.3).contains(&depth)
        });
    let dust = !wet && ground <= 1.3 && !suppressed && tangent.abs() > 0.5;
    let speed = dot(velocity, velocity).sqrt();
    let dirt = !wet
        && ground <= 1.3
        && !suppressed
        && dirt_enabled
        && throttle > 0.
        && speed > 45.
        && (tangent.abs() < 10. || angle.abs() > 0.2);
    let excess = speed.clamp(45., 115.) - 45.;
    let velocity = if speed > 0. {
        let reciprocal = (1_f64 / (dot(velocity, velocity) as f64).sqrt()) as f32;
        let mut v = velocity.map(|v| (excess * (-v * reciprocal)) * 0.1);
        v[1] = excess * 0.2;
        v
    } else {
        [0.; 3]
    };
    Selection {
        dust,
        dirt,
        wet,
        velocity,
        life: (excess * 0.2 * 1000.) / 16.,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Dust,
    Chunk,
    Spray,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Particle {
    pub kind: Kind,
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub life: f32,
    pub size: f32,
    pub initial_size: f32,
    pub growth: f32,
    pub flags: u32,
    pub gravity: f32,
    pub frame: usize,
}
impl Particle {
    ///00143aa0 finite CPU particle update; chunk terrain is a caller service.
    pub fn advance(
        &mut self,
        dt: f32,
        mut terrain: impl FnMut(Vec3) -> Result<Option<f32>>,
    ) -> Result<bool> {
        let active_flags = self.flags;
        self.age += dt * 1000.;
        if self.life < self.age {
            return Ok(false);
        }
        if self.flags & 8 != 0 {
            let growth = self.growth * dt;
            self.size += growth;
            if self.size <= 0. {
                self.size = 0.;
                self.age = self.life;
            } else {
                self.position[1] += growth;
            }
        }
        if self.flags & 4 != 0 {
            self.position = add(self.position, scale(self.velocity, dt));
            if self.flags & 0x40 != 0
                && self.age > 250.
                && let Some(height) = terrain(self.position)?
                && height + 0.2 >= self.position[1]
            {
                self.position[1] = height + 0.2;
                self.flags = (self.flags & !0x274) | 2;
                if self.life - self.age < 1500. {
                    self.life = 1500.;
                }
            }
        }
        if active_flags & 0x20 != 0 {
            self.velocity[1] -= self.gravity * 32.2 * dt;
        }
        Ok(true)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sprite {
    pub kind: Kind,
    pub position: Vec3,
    pub size: f32,
    pub alpha: f32,
    pub frame: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Frame {
    pub wet_births: Vec<WetBirth>,
    pub wet_total_emitted: [u64; 2],
    pub placement_epoch: u64,
    pub time: f32,
    pub particles: Vec<Sprite>,
    pub emitted: [usize; 3],
    pub total_emitted: [u64; 3],
    pub suppressed: bool,
    pub wet: [bool; 2],
}
#[derive(Debug, Clone)]
struct RearEmitter {
    previous: Option<Vec3>,
    dirt_previous: Option<Vec3>,
    surface: usize,
    dust_frame: usize,
    chunk_frame: usize,
    spray_frame: usize,
    phase: f32,
    sizes: Vec<[f32; 2]>,
    velocities: Vec<[Vec3; 2]>,
    size_indices: [usize; 2],
    velocity_indices: [usize; 2],
    residuals: [f32; 2],
}
pub struct Effects {
    radial: Vec<Vec3>,
    visual_random: OriginalRandom,
    wet_total_emitted: [u64; 2],
    wet_sequence: u64,
    wet_jitter_index: usize,
    placement_epoch: u64,
    config: Config,
    particles: Vec<Particle>,
    rear: [RearEmitter; 2],
    time: Option<f32>,
    tick: Option<u64>,
    reset: u32,
    suppressed: bool,
    sequence: u32,
    total_emitted: [u64; 3],
    frame: Frame,
    jitter: [f32; 16],
    jitter_index: usize,
}
fn random_unit(random: &mut OriginalRandom) -> f32 {
    random.next_u31() as f32 * f32::from_bits(0x30000000)
}
fn sine(angle: f32) -> f32 {
    let square = angle * angle;
    let coefficients = [
        0xbe2aaaab, 0x3c088889, 0xb9500d01, 0x3638ef1d, 0xb2d7322b, 0x2f309231, 0xab573fa0,
    ]
    .map(f32::from_bits);
    let mut value = coefficients[6];
    for c in coefficients[..6].iter().rev() {
        value = *c + square * value;
    }
    angle * (square * value + 1.)
}
fn cosine(angle: f32) -> f32 {
    let square = angle * angle;
    let coefficients = [
        0xbf000000, 0x3d2aaaab, 0xbab60b61, 0x37d00d01, 0xb493f27e, 0x310f76c7, 0xad49cba6,
        0x29573fa0,
    ]
    .map(f32::from_bits);
    let mut value = coefficients[7];
    for c in coefficients[..7].iter().rev() {
        value = *c + square * value;
    }
    square * value + 1.
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WetBirth {
    pub id: u64,
    pub position: Vec3,
    pub velocity: Vec3,
    pub birth_time: f32,
    pub life: f32,
    pub size: f32,
    pub tile: usize,
    pub kind: String,
    pub floor: f32,
}
#[derive(Debug, Clone, Serialize)]
pub struct RoostParticle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub size: f32,
}
fn quantize_roost_velocity(velocity: Vec3) -> Vec3 {
    std::array::from_fn(|i| {
        let units = if i == 1 { 50. } else { 200. };
        let factor = 32767. / units;
        ((velocity[i] * factor) as i32 as i16) as f32 * (1. / factor)
    })
}
///001db4a0 wet CPU birth fan with supplied original constructor tables/heading.
#[allow(clippy::too_many_arguments)]
pub fn roost_births(
    position: Vec3,
    velocity: Vec3,
    dt: f32,
    mud: bool,
    height: f32,
    fan: usize,
    radial: &[Vec3],
    jitter: [f32; 16],
    mut jitter_index: usize,
) -> Vec<RoostParticle> {
    let separation = if mud { 10 } else { 5 };
    let count = 90 / separation;
    let half = (90. / separation as f32) * 0.5;

    let spread = if mud { 1. } else { 6. };
    (0..count)
        .map(|i| {
            let radial = radial[(fan + i * separation) % 120];
            let position = add(position, scale(radial, 0.5));
            let factor = (half - (half - ((i as f32 + half * 0.5).min(half))).abs()) / half;
            let velocity = add(
                velocity,
                [
                    radial[0] * (spread * factor),
                    height * factor,
                    radial[2] * (spread * factor),
                ],
            );
            if jitter_index > 11 {
                jitter_index = 0;
            }
            let r0 = jitter[jitter_index];
            let r1 = jitter[jitter_index + 1];
            let r2 = jitter[jitter_index + 2];
            jitter_index += 3;
            let velocity = [
                velocity[0] * (r0 * 0.1 + 0.9),
                velocity[1] * (r0 * 0.1 + 0.9),
                velocity[2] * (r1 * 0.1 + 0.9),
            ];
            RoostParticle {
                position,
                velocity,
                age: (dt * r2 - dt) * 800.,
                size: if mud { 0.75 } else { 0.5 },
            }
        })
        .collect()
}
impl Effects {
    pub fn new(config: Config) -> Self {
        let mut random = OriginalRandom::with_seed(1);
        let rear = std::array::from_fn(|_| {
            let phase = random_unit(&mut random) * std::f32::consts::TAU;
            let sizes = (0..256)
                .map(|_| {
                    [
                        random_unit(&mut random) * 0.25,
                        random_unit(&mut random) * 0.25 + 0.25,
                    ]
                })
                .collect();
            let velocities = (0..32)
                .map(|_| {
                    let chunk = std::array::from_fn(|_| random_unit(&mut random) + 0.5);
                    let spray = [random_unit(&mut random) * 0.5 + 0.75; 3];
                    [chunk, spray]
                })
                .collect();
            RearEmitter {
                previous: None,
                dirt_previous: None,
                surface: usize::MAX,
                dust_frame: 0,
                chunk_frame: 0,
                spray_frame: 0,
                phase,
                sizes,
                velocities,
                size_indices: [0; 2],
                velocity_indices: [0; 2],
                residuals: [0.; 2],
            }
        });
        let jitter = std::array::from_fn(|_| random_unit(&mut random));
        let radial = (0..120)
            .map(|i| {
                let angle = ((i as f32 * std::f32::consts::TAU) / 120.) - std::f32::consts::PI;
                let x = cosine(angle) * (random_unit(&mut random) * 0.1 + 0.9);
                let z = sine(angle) * (random_unit(&mut random) * 0.1 + 0.9);
                [x, 0., z]
            })
            .collect();
        Self {
            radial,
            visual_random: random,
            wet_total_emitted: [0; 2],
            wet_sequence: 0,
            wet_jitter_index: 0,
            placement_epoch: 0,
            config,
            particles: Vec::with_capacity(600),
            rear,
            time: None,
            tick: None,
            reset: 0,
            suppressed: true,
            sequence: 0,
            total_emitted: [0; 3],
            frame: Frame::default(),
            jitter,
            jitter_index: 0,
        }
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn step(&mut self, input: &Input) -> Result<&Frame> {
        self.step_with_terrain(input, |_| Ok(None))
    }
    pub fn step_with_terrain(
        &mut self,
        input: &Input,
        mut terrain: impl FnMut(Vec3) -> Result<Option<f32>>,
    ) -> Result<&Frame> {
        if !input.time.is_finite()
            || input
                .centers
                .iter()
                .flatten()
                .chain(input.linear_velocity.iter())
                .chain(input.wheel_tangential_speeds.iter())
                .chain(input.terrain_heights.iter().flatten())
                .chain(input.water_heights.iter().flatten())
                .any(|v| !v.is_finite())
            || input.surfaces.iter().any(|s| *s >= 8)
            || ![input.throttle, input.steering_angle]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("invalid wheel effect snapshot".into());
        }
        if self.tick == Some(input.tick)
            && self.reset == input.reset_count
            && self.placement_epoch == input.placement_epoch
        {
            return Ok(&self.frame);
        }
        if self.reset != input.reset_count || self.placement_epoch != input.placement_epoch {
            self.suppressed = true;
            self.rear.iter_mut().for_each(|r| {
                r.previous = None;
                r.dirt_previous = None;
            });
        }
        let dt = input.dt;
        if !dt.is_finite() || dt < 0. || dt > 0.1001 {
            return Err("wheel effect time step exceeds physics bound".into());
        }
        self.time = Some(input.time);
        self.tick = Some(input.tick);
        self.reset = input.reset_count;
        self.placement_epoch = input.placement_epoch;
        if input.grounded.iter().all(|v| *v) {
            self.suppressed = false;
        }
        let mut emitted = [0; 3];
        let mut wet = [false; 2];
        for (side, wet_value) in wet.iter_mut().enumerate() {
            let index = side + 2;
            let terrain = input.terrain_heights[index];
            let center = input.centers[index];
            let position = terrain
                .map(|height| [center[0], height, center[2]])
                .unwrap_or(center);
            let rear = &mut self.rear[side];
            let previous = rear.previous.replace(position);
            let dirt_previous = if terrain.is_some_and(|h| center[1] - h <= 1.3)
                && !self.suppressed
                && !input.mud[index]
                && !select(
                    center,
                    terrain.unwrap_or(center[1]),
                    input.water_heights[index],
                    false,
                    false,
                    0.,
                    0.,
                    0.,
                    [0.; 3],
                    false,
                )
                .wet
            {
                rear.dirt_previous.replace(position)
            } else {
                rear.dirt_previous
            };
            if rear.surface != input.surfaces[index] {
                rear.surface = input.surfaces[index];
                rear.dust_frame = self.config.atlas.dust[rear.surface].first;
                rear.chunk_frame = self.config.atlas.chunk[rear.surface].first;
                rear.spray_frame = self.config.atlas.spray[rear.surface].first;
            }
            let Some(terrain) = terrain else { continue };
            let selection = select(
                center,
                terrain,
                input.water_heights[index],
                input.mud[index],
                self.suppressed,
                input.wheel_tangential_speeds[0],
                input.throttle,
                input.steering_angle,
                input.linear_velocity,
                true,
            );
            *wet_value = selection.wet;
            if let Some(previous) = previous
                && dt > 0.
            {
                let delta = sub(position, previous);
                let distance = dot(delta, delta).sqrt();
                if selection.dust && distance >= 0.1 {
                    let emitter = &self.config.dust;
                    let life =
                        (distance * emitter.size * emitter.life * 2.).clamp(250., emitter.life);
                    let size = (emitter.size * 0.01333 * (distance / dt)).clamp(0.5, 1.);
                    let count =
                        ((distance * distance) / ((size + emitter.size) * 0.45)).clamp(2., 4.);
                    let increment = scale(delta, 1. / count);
                    let mut p = previous;
                    p[1] += increment[1] + size + 0.25;
                    rear.phase =
                        (rear.phase + std::f32::consts::FRAC_PI_2 * dt) % std::f32::consts::TAU;
                    let sway = 2.
                        * sine(rear.phase - std::f32::consts::PI)
                        * ((distance / dt) / 30.).min(1.);
                    let velocity = add(emitter.velocity, [sway, 0., sway]);
                    for _ in 0..count as usize {
                        p[0] += increment[0];
                        p[2] += increment[2];
                        self.sequence = self.sequence.wrapping_add(1);
                        let varied_life = life - life * 0.125 * (self.sequence & 3) as f32;
                        if self.particles.len() < 600 {
                            self.particles.push(Particle {
                                kind: Kind::Dust,
                                position: p,
                                velocity,
                                age: life * 0.25,
                                life: varied_life,
                                size,
                                initial_size: size,
                                growth: size * emitter.growth,
                                flags: emitter.flags,
                                gravity: emitter.gravity,
                                frame: rear.dust_frame,
                            });
                            emitted[0] += 1;
                        }
                        p[1] += increment[1];
                        rear.dust_frame += 1;
                        if rear.dust_frame >= self.config.atlas.dust[rear.surface].end {
                            rear.dust_frame = self.config.atlas.dust[rear.surface].first;
                        }
                    }
                }
                if !selection.dirt {
                    rear.residuals = [0.; 2];
                }
                if selection.dirt {
                    let speed = dot(input.linear_velocity, input.linear_velocity).sqrt();
                    let reciprocal = (1_f64
                        / (dot(input.linear_velocity, input.linear_velocity) as f64).sqrt())
                        as f32;
                    let direction = scale(input.linear_velocity, -reciprocal);
                    let j = self.jitter_index;
                    self.jitter_index = (j + 2) & 15;
                    let changed = [
                        direction[0] + 0.6 * (1. - self.jitter[j] * 2.),
                        direction[1],
                        direction[2] + 0.6 * (1. - self.jitter[j + 1] * 2.),
                    ];
                    let reciprocal = (1_f64 / (dot(changed, changed) as f64).sqrt()) as f32;
                    let excess = speed.clamp(45., 115.) - 45.;
                    let mut chunk_velocity = changed.map(|v| (excess * (v * reciprocal)) * 0.15);
                    chunk_velocity[1] = excess * 0.2;
                    let previous = dirt_previous.unwrap_or(position);
                    let delta = sub(position, previous);
                    for (kind_number, kind, emitter, group) in [
                        (
                            1,
                            Kind::Chunk,
                            &self.config.chunk,
                            self.config.atlas.chunk[rear.surface],
                        ),
                        (
                            2,
                            Kind::Spray,
                            &self.config.spray,
                            self.config.atlas.spray[rear.surface],
                        ),
                    ] {
                        let bank = kind_number - 1;
                        let elapsed = dt + rear.residuals[bank];
                        let count = emitter.rate * elapsed;
                        let inverse = 1. / count;
                        rear.residuals[bank] = elapsed - count / emitter.rate;
                        let base_velocity = if kind_number == 1 {
                            chunk_velocity
                        } else {
                            selection.velocity
                        };
                        let step = base_velocity.map(|v| inverse * elapsed * v);
                        let mut p = add(previous, scale(step, count));
                        for i in 0..count.ceil() as usize {
                            let size_index = rear.size_indices[bank];
                            let velocity_index = rear.velocity_indices[bank];
                            let size = rear.sizes[size_index][bank];
                            //0019b9e0 permits index32: it aliases following index/size words.
                            let multiplier = if velocity_index == 32 {
                                [
                                    f32::from_bits(size_index as u32),
                                    f32::from_bits(32),
                                    rear.sizes[0][bank],
                                ]
                            } else {
                                rear.velocities[velocity_index][bank]
                            };
                            let mut velocity = base_velocity;
                            velocity[1] -= (count - i as f32) * inverse * elapsed * 64.;
                            velocity =
                                std::array::from_fn(|axis| velocity[axis] * multiplier[axis]);
                            let frame = if kind_number == 1 {
                                &mut rear.chunk_frame
                            } else {
                                &mut rear.spray_frame
                            };
                            let life = selection.life * if kind_number == 1 { 1. } else { 2. };
                            if self.particles.len() < 600 {
                                self.particles.push(Particle {
                                    kind,
                                    position: p,
                                    velocity,
                                    age: if kind_number == 2 { selection.life } else { 0. },
                                    life,
                                    size,
                                    initial_size: size,
                                    growth: size * 2.,
                                    flags: emitter.flags,
                                    gravity: if kind_number == 2 {
                                        2.
                                    } else {
                                        emitter.gravity
                                    },
                                    frame: *frame,
                                });
                                emitted[kind_number] += 1;
                            }
                            p = add(p, sub(scale(delta, inverse), step));
                            *frame += 1;
                            if *frame >= group.end {
                                *frame = group.first;
                            }
                            rear.size_indices[bank] = (size_index + 1) & 255;
                            rear.velocity_indices[bank] = if velocity_index == 32 {
                                0
                            } else {
                                velocity_index + 1
                            };
                        }
                    }
                }
            }
        }
        let mut wet_births = Vec::new();
        if dt > 0. && input.wheel_tangential_speeds[0].abs() > 7. {
            for index in 0..4 {
                let mut center = input.centers[index];
                //001d7ea0 lowers front wheel sample by exactly one foot.
                if index < 2 {
                    center[1] -= 1.;
                }
                let Some(terrain) = input.terrain_heights[index] else {
                    continue;
                };
                let selected = select(
                    center,
                    terrain,
                    input.water_heights[index],
                    input.mud[index],
                    false,
                    input.wheel_tangential_speeds[0],
                    input.throttle,
                    input.steering_angle,
                    input.linear_velocity,
                    true,
                );
                if !selected.wet {
                    continue;
                }
                let Some(height) = (if input.mud[index] {
                    input.mud_heights[index]
                } else {
                    input.water_heights[index]
                }) else {
                    continue;
                };
                center[1] = height;
                let speed = dot(input.linear_velocity, input.linear_velocity).sqrt();
                let velocity = scale(input.linear_velocity, 1. + (speed / 200.).min(1.));
                let direction_speed = dot(velocity, velocity).sqrt();
                let heading = if direction_speed > 0. {
                    let angle = (velocity[2] / direction_speed).clamp(-1., 1.).acos();
                    if velocity[0] < 0. {
                        std::f32::consts::TAU - angle
                    } else {
                        angle
                    }
                } else {
                    0.
                };
                let mut angle =
                    (std::f32::consts::TAU - heading) + std::f32::consts::FRAC_PI_4 * 3.;
                if angle > std::f32::consts::TAU {
                    angle -= std::f32::consts::TAU;
                }
                let separation = if input.mud[index] { 10 } else { 5 };
                let fan = ((angle / std::f32::consts::TAU * 120.) as usize
                    + (separation as f32 * random_unit(&mut self.visual_random)) as usize)
                    % 120;
                let births = roost_births(
                    center,
                    velocity,
                    dt,
                    input.mud[index],
                    (speed / 30.).clamp(0.25, 0.5) * 12.,
                    fan,
                    &self.radial,
                    self.jitter,
                    self.wet_jitter_index,
                );
                self.wet_jitter_index = (self.wet_jitter_index + births.len() * 3) % 12;
                let first = if input.mud[index] { 8 } else { 0 };
                let tile = first + (8. * random_unit(&mut self.visual_random)) as usize;
                for birth in births {
                    self.wet_sequence += 1;
                    self.wet_total_emitted[usize::from(input.mud[index])] += 1;
                    wet_births.push(WetBirth {
                        id: self.wet_sequence,
                        position: birth.position,
                        velocity: quantize_roost_velocity(birth.velocity),
                        birth_time: input.time + birth.age * 0.001,
                        life: 0.8,
                        size: ((birth.size * 655.34) as u16) as f32 * f32::from_bits(0x3ac8018f),
                        tile,
                        floor: center[1] - 20.,
                        kind: if input.mud[index] { "mud" } else { "water" }.into(),
                    });
                }
            }
        }
        //00143aa0: emitter births precede age/motion/growth/acceleration.
        let mut alive = Vec::with_capacity(self.particles.len());
        for mut p in self.particles.drain(..) {
            if p.advance(dt, &mut terrain)? {
                alive.push(p);
            }
        }
        self.particles = alive;
        for (total, count) in self.total_emitted.iter_mut().zip(emitted) {
            *total += count as u64;
        }
        let particles = self
            .particles
            .iter()
            .map(|p| {
                //001451a0 native packet alpha is64 (GS128 is unity).
                let ratio = p.age / p.life;
                let alpha = if p.flags & 2 != 0 {
                    0.5 * (1. - ratio)
                } else if p.flags & 0x1000 != 0 {
                    0.5 * ratio * (p.initial_size.powi(3) / p.size.powi(3))
                } else {
                    0.5
                };
                Sprite {
                    kind: p.kind,
                    position: p.position,
                    size: p.size,
                    alpha,
                    frame: p.frame,
                }
            })
            .collect();
        self.frame = Frame {
            wet_births,
            wet_total_emitted: self.wet_total_emitted,
            placement_epoch: self.placement_epoch,
            time: input.time,
            particles,
            emitted,
            total_emitted: self.total_emitted,
            suppressed: self.suppressed,
            wet,
        };
        Ok(&self.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Config {
        let group = Group { first: 0, end: 15 };
        let emitter = Emitter {
            life: 750.,
            size: 1.,
            growth: 1.5,
            rate: 120.,
            age: 0.,
            velocity: [0., 0.2, 0.],
            flags: 0x100e,
            gravity: 3.,
        };
        Config {
            light_color: [128.; 3],
            atlas: Atlas {
                texture: "synthetic.tga".into(),
                frames: vec![
                    AtlasFrame {
                        rect: [0., 0., 15., 15.]
                    };
                    16
                ],
                dust: [group; 8],
                chunk: [group; 8],
                spray: [group; 8],
            },
            dust: emitter.clone(),
            chunk: Emitter {
                flags: 100,
                ..emitter.clone()
            },
            spray: Emitter {
                flags: 0x2e,
                ..emitter
            },
        }
    }
    fn input() -> Input {
        Input {
            time: 0.,
            dt: 0.,
            tick: 0,
            centers: [[0., 0.8, 0.]; 4],
            surfaces: [0; 4],
            terrain_heights: [Some(0.); 4],
            water_heights: [None; 4],
            mud: [false; 4],
            mud_heights: [None; 4],
            wheel_tangential_speeds: [60.; 4],
            grounded: [true; 4],
            throttle: 1.,
            steering_angle: 0.3,
            linear_velocity: [0., 0., 60.],
            reset_count: 0,
            placement_epoch: 0,
        }
    }
    #[test]
    fn fixed_tick_is_idempotent_and_relocation_preserves_world_particles() {
        let mut effects = Effects::new(fixture());
        let mut input = input();
        effects.step(&input).unwrap();
        input.dt = 1. / 60.;
        input.time = input.dt;
        input.tick = 1;
        input.centers.iter_mut().for_each(|p| p[2] += 1.);
        let before = effects.step(&input).unwrap().clone();
        assert!(!before.particles.is_empty());
        assert_eq!(&before, effects.step(&input).unwrap());
        input.placement_epoch = 1;
        input.dt = 0.;
        input.centers = [[100., 10., 100.]; 4];
        input.grounded = [false; 4];
        let relocated = effects.step(&input).unwrap();
        assert!(relocated.suppressed);
        assert_eq!(before.particles, relocated.particles);
        assert_eq!(relocated.emitted, [0; 3]);
        input.dt = 1. / 60.;
        input.time += input.dt;
        input.tick += 1;
        let airborne = effects.step(&input).unwrap();
        assert_eq!(airborne.emitted, [0; 3]);
        assert!(airborne.wet_births.is_empty());
    }
    #[test]
    fn real_height_branches_choose_separate_water_and_mud_tiles() {
        let mut effects = Effects::new(fixture());
        let mut input = input();
        effects.step(&input).unwrap();
        input.dt = 1. / 60.;
        input.time = input.dt;
        input.tick = 1;
        input.water_heights = [Some(0.4); 4];
        let water = effects.step(&input).unwrap();
        assert_eq!(water.wet_births.len(), 72);
        assert!(
            water
                .wet_births
                .iter()
                .all(|p| p.tile < 8 && p.kind == "water")
        );
        assert_eq!(water.emitted, [0; 3]);
        input.tick += 1;
        input.time += input.dt;
        input.mud = [true; 4];
        input.mud_heights = [Some(0.); 4];
        input.water_heights = [None; 4];
        let mud = effects.step(&input).unwrap();
        assert_eq!(mud.wet_births.len(), 36);
        assert!(
            mud.wet_births
                .iter()
                .all(|p| (8..16).contains(&p.tile) && p.kind == "mud")
        );
        assert_eq!(mud.emitted, [0; 3]);
    }
    #[test]
    fn spray_birth_starts_halfway_through_original_lifetime() {
        let mut effects = Effects::new(fixture());
        let mut input = input();
        effects.step(&input).unwrap();
        input.dt = 1. / 60.;
        input.time = input.dt;
        input.tick = 1;
        input.centers.iter_mut().for_each(|p| p[2] += 1.);
        effects.step(&input).unwrap();
        let life = (15. * 0.2 * 1000.) / 16.;
        let sprays: Vec<_> = effects
            .particles
            .iter()
            .filter(|p| p.kind == Kind::Spray)
            .collect();
        assert!(!sprays.is_empty());
        for p in sprays {
            assert_eq!(p.life, life * 2.);
            assert_eq!(p.age, life + input.dt * 1000.);
        }
        assert_eq!(effects.rear[0].size_indices, [2, 2]);
        assert_eq!(effects.jitter_index, 4); // one original vector per rear emitter/frame
    }
    #[test]
    fn settling_uses_current_step_gravity_then_reduced_flags() {
        let mut p = Particle {
            kind: Kind::Chunk,
            position: [0., 0.1, 0.],
            velocity: [1., -1., 0.],
            age: 300.,
            life: 500.,
            size: 0.2,
            initial_size: 0.2,
            growth: 0.,
            flags: 100,
            gravity: 3.,
            frame: 0,
        };
        p.advance(1. / 60., |_| Ok(Some(0.))).unwrap();
        assert_eq!(p.position[1], 0.2);
        assert_eq!(p.life, 1500.);
        assert_eq!(p.flags, 2);
        assert!(p.velocity[1] < -1.);
        let position = p.position;
        let velocity = p.velocity;
        p.advance(1. / 60., |_| Ok(Some(0.))).unwrap();
        assert_eq!(p.position, position);
        assert_eq!(p.velocity, velocity);
    }
    #[test]
    fn original_selector_thresholds() {
        let dry = select(
            [0., 1.3, 0.],
            0.,
            None,
            false,
            false,
            0.5001,
            1.,
            0.201,
            [0., 0., 45.001],
            true,
        );
        assert!(dry.dust && dry.dirt && !dry.wet);
        let exact = select(
            [0., 1.3, 0.],
            0.,
            None,
            false,
            false,
            0.5,
            1.,
            0.2,
            [0., 0., 45.],
            true,
        );
        assert!(!exact.dust && !exact.dirt);
        let wet = select(
            [0., 1., 0.],
            0.,
            Some(0.5),
            false,
            false,
            20.,
            1.,
            0.8,
            [0., 0., 70.],
            true,
        );
        assert!(wet.wet && !wet.dust && !wet.dirt);
        let air = select(
            [0., 5., 0.],
            0.,
            None,
            false,
            false,
            70.,
            1.,
            0.8,
            [0., 0., 70.],
            true,
        );
        assert!(!air.dust && !air.dirt);
    }
    #[test]
    fn spray_uses_original_speed_cap_and_launch_direction() {
        let selection = select(
            [0., 1., 0.],
            0.,
            None,
            false,
            false,
            8.,
            1.,
            0.,
            [130., 0., 0.],
            true,
        );
        assert_eq!(selection.life, 875.);
        assert_eq!(selection.velocity, [-7., 14., -0.]);
    }
    #[test]
    fn source_atlas_validates_group_types_and_ranges() {
        assert!(
            Atlas::parse(b"[General Info]\nTextureMap=x\nNumFrames=4097\nNumGroups=26").is_err()
        );
        assert!(Emitter::parse(b"invalid", 0).is_err());
    }
}
