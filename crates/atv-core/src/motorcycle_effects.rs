//! MX emitter ranges and sixteen-surface disable rules, connected through an
//! authored bounded CPU emitter. Native generic emitter scheduling is pending.
use crate::{
    Result,
    motorcycle::{cleaned, member},
    resource::Resource,
    wheel::{add, scale},
    wheel_effects::{AtlasFrame, Frame, Kind, Particle, Sprite},
};
use serde::Serialize;
#[derive(Serialize)]
pub struct Atlas {
    pub texture: String,
    pub frames: Vec<AtlasFrame>,
}
#[derive(Clone)]
struct Emitter {
    life: [f32; 2],
    size: [f32; 2],
    growth: [f32; 2],
    rate: [f32; 2],
    velocity: [f32; 3],
    noise: [[f32; 3]; 2],
    gravity: f32,
    group: [usize; 2],
}
#[derive(Serialize)]
pub struct Config {
    pub atlas: Atlas,
    pub light_color: [f32; 3],
    #[serde(skip)]
    emitters: Vec<Emitter>,
}
impl Config {
    pub fn load(resource: &Resource<'_>) -> Result<Self> {
        let atlas = cleaned(&member(resource, "FS01particles.prt")?)?;
        let count = atlas.first_scalar("General Info", "NumFrames")? as usize;
        if !(1..=4096).contains(&count) {
            return Err("invalid motorcycle particle frames".into());
        }
        let mut frames = Vec::new();
        for i in 0..count {
            let section = format!("Frame{i}");
            let rect = ["UVLeft", "UVTop", "UVRight", "UVBottom"]
                .map(|key| atlas.first_scalar(&section, key));
            let rect = [
                rect[0].clone()?,
                rect[1].clone()?,
                rect[2].clone()?,
                rect[3].clone()?,
            ];
            if rect[0] < 0.
                || rect[1] < 0.
                || rect[2] < rect[0]
                || rect[3] < rect[1]
                || rect[2] > 4096.
                || rect[3] > 4096.
            {
                return Err("invalid motorcycle particle rectangle".into());
            }
            frames.push(AtlasFrame { rect });
        }
        let groups = atlas.first_scalar("General Info", "NumGroups")? as usize;
        if !(1..=128).contains(&groups) {
            return Err("invalid motorcycle particle groups".into());
        }
        let mut emitters = Vec::new();
        for name in [
            "tiredust.emt",
            "tirechunks.emt",
            "tiredirtspray.emt",
            "exhaust.emt",
            "wreckdust.emt",
        ] {
            let data = cleaned(&member(resource, name)?)?;
            let range = |key| -> Result<[f32; 2]> {
                let lo = data.first_scalar("General Info", &format!("Minimum{key}"))?;
                let hi = data.first_scalar("General Info", &format!("Maximum{key}"))?;
                if lo > hi {
                    return Err("invalid motorcycle emitter range".into());
                }
                Ok([lo, hi])
            };
            let group = data
                .first("General Info", "ParticleGroupName")?
                .value
                .trim_matches('"');
            let index = (0..groups)
                .find(|i| {
                    atlas
                        .first(&format!("Group{i}"), "Name")
                        .is_ok_and(|v| v.value.trim_matches('"').eq_ignore_ascii_case(group))
                })
                .ok_or("missing motorcycle emitter atlas group")?;
            let section = format!("Group{index}");
            let bounds = [
                atlas.first_scalar(&section, "FirstFrame")? as usize,
                atlas.first_scalar(&section, "LastFrame")? as usize,
            ];
            if bounds[0] > bounds[1] || bounds[1] >= count {
                return Err("invalid motorcycle particle group extent".into());
            }
            let emitter = Emitter {
                life: range("Life")?,
                size: range("InitialSize")?,
                growth: range("GrowthRate")?,
                rate: range("EmitRate")?,
                velocity: data.vector3("General Info", "Velocity")?,
                noise: [
                    data.vector3("General Info", "MinimumVelocityNoise")?,
                    data.vector3("General Info", "MaximumVelocityNoise")?,
                ],
                gravity: data.first_scalar("General Info", "GravityMultiplier")?,
                group: bounds,
            };
            if emitter.life[0] <= 0. || emitter.size[0] < 0. || emitter.rate[1] > 10000. {
                return Err("invalid motorcycle emitter limits".into());
            }
            emitters.push(emitter);
        }
        Ok(Self {
            atlas: Atlas {
                texture: atlas.first("General Info", "TextureMap")?.value.clone(),
                frames,
            },
            light_color: [128.; 3],
            emitters,
        })
    }
}
pub struct Effects {
    particles: Vec<Particle>,
    random: u32,
    residual: [f32; 5],
    total: [u64; 3],
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            particles: Vec::new(),
            random: 0x829312,
            residual: [0.; 5],
            total: [0; 3],
        }
    }
}
impl Effects {
    fn random(&mut self) -> f32 {
        self.random = self.random.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.random >> 8) as f32 / 16777216.
    }
    fn range(&mut self, r: [f32; 2]) -> f32 {
        r[0] + (r[1] - r[0]) * self.random()
    }
    pub fn step(
        &mut self,
        config: &Config,
        bike: &crate::motorcycle::Frame,
        world: &[f32; 16],
        dt: f32,
        terrain: &crate::terrain::Terrain,
    ) -> Result<()> {
        let mut retained = Vec::with_capacity(self.particles.len());
        for mut particle in self.particles.drain(..) {
            if particle.advance(dt, |p| Ok(terrain.contact(p[0], p[2])?.map(|c| c.height)))? {
                retained.push(particle);
            }
        }
        self.particles = retained;
        let wreck = bike.wreck.phase != crate::wreck::Phase::Riding;
        let rear = bike.grounded[1];
        let zone = bike.surfaces[1];
        let wet = matches!(zone, 6 | 7);
        let enables = [
            rear && !wet && !wreck && bike.speed.abs() > 15.,
            rear && !wreck && bike.speed.abs() > 3.,
            rear && !wreck && bike.speed.abs() > 15.,
            !wreck && !wet && bike.speed.abs() < 15.,
            wreck && bike.crash_impact_speed > 18.,
        ];
        for (index, enabled) in enables.into_iter().enumerate() {
            if !enabled {
                self.residual[index] = 0.;
                continue;
            }
            let e = config.emitters[index].clone();
            let rate = self.range(e.rate)
                * (if index == 1 {
                    bike.speed.abs().min(60.) / 60.
                } else {
                    1.
                });
            self.residual[index] += rate * dt;
            let emit = (self.residual[index] as usize).min(40);
            self.residual[index] -= emit as f32;
            for _ in 0..emit {
                if self.particles.len() >= 600 {
                    break;
                }
                let p = if index == 3 {
                    [0., 2.4, -2.1]
                } else {
                    [self.range([-0.15, 0.15]), 0.15, -2.6]
                };
                let velocity = std::array::from_fn(|i| {
                    e.velocity[i] + self.range([e.noise[0][i], e.noise[1][i]])
                });
                let velocity = crate::dynamics::transform(
                    [
                        [world[0], world[1], world[2]],
                        [world[4], world[5], world[6]],
                        [world[8], world[9], world[10]],
                    ],
                    velocity,
                );
                let kind = if index == 1 {
                    Kind::Chunk
                } else if index == 2 {
                    Kind::Spray
                } else {
                    Kind::Dust
                };
                let size = self.range(e.size);
                let particle = Particle {
                    kind,
                    position: crate::obstacle::transform_point(world, p),
                    velocity: add(
                        velocity,
                        scale(
                            bike.linear_velocity,
                            if index == 1 || index == 2 { -0.15 } else { 0. },
                        ),
                    ),
                    age: 0.,
                    life: self.range(e.life),
                    size,
                    initial_size: size,
                    growth: self.range(e.growth),
                    flags: 0x2e | if index == 1 { 0x40 } else { 0 },
                    gravity: e.gravity,
                    frame: e.group[0]
                        + (self.random() * (e.group[1] - e.group[0] + 1) as f32) as usize,
                };
                self.total[match kind {
                    Kind::Dust => 0,
                    Kind::Chunk => 1,
                    Kind::Spray => 2,
                }] += 1;
                self.particles.push(particle);
            }
        }
        Ok(())
    }
    pub fn frame(&self, time: f32, epoch: u32) -> Frame {
        Frame {
            time,
            placement_epoch: epoch as u64,
            total_emitted: self.total,
            particles: self
                .particles
                .iter()
                .map(|p| Sprite {
                    kind: p.kind,
                    position: p.position,
                    size: p.size,
                    alpha: (1. - p.age / p.life).clamp(0., 1.) * 0.7,
                    frame: p.frame,
                })
                .collect(),
            ..Frame::default()
        }
    }
}
