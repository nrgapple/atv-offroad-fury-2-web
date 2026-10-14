//! Finite static type11 stream births,0019f580; packet quantization001d1910.
use super::{Emitter, OriginalRandom, PhysicsData, Result, boolean, number, random, vector};
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize)]
pub struct Config {
    pub length: [f32; 2],
    pub thickness: [f32; 2],
    pub separation: f32,
    pub tail_life: f32,
    pub bright_head: bool,
    pub color: [f32; 3],
    pub alpha: [f32; 2],
    pub floor: f32,
}
impl Config {
    pub fn parse(d: &PhysicsData) -> Result<Self> {
        let config = Self {
            length: [
                number(d, "General Info", "MinStreamLength", 3.)?,
                number(d, "General Info", "MaxStreamLength", 5.)?,
            ],
            thickness: [
                number(d, "General Info", "TailThickness", 1.)?,
                number(d, "General Info", "HeadThickness", 2.)?,
            ],
            separation: number(d, "General Info", "ParticleSeparationFactor", 1.5)?,
            tail_life: number(d, "General Info", "TailLifeModifier", 0.6)?,
            bright_head: boolean(d, "General Info", "SuperbrightHead", true)?,
            color: vector(d, "General Info", "ParticleColor")?,
            alpha: [
                number(d, "General Info", "MinAlpha", 0.)?,
                number(d, "General Info", "MaxAlpha", 128.)?,
            ],
            floor: number(d, "General Info", "GroundLevel", 0.)? - 5.,
        };
        if config.length[0] < 1.
            || config.length[1] < config.length[0]
            || config.length[1] > 32.
            || config.thickness[0] < 0.
            || config.thickness[1] < config.thickness[0]
            || config.thickness[1] > 16.
            || !(0. ..=1.).contains(&config.tail_life)
            || config.alpha[0] < 0.
            || config.alpha[1] > 128.
            || config.alpha[1] < config.alpha[0]
        {
            return Err("invalid original stream bounds".into());
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires private original type11 instruction cases"]
    fn private_original_stream_cases() {
        #[derive(Deserialize)]
        struct Case {
            seed: u64,
            dt: f32,
            rate: f32,
            life: f32,
            size: f32,
            growth: f32,
            velocity: [f32; 3],
            position: [f32; 3],
            spread: [[f32; 3]; 2],
            config: Config,
            output: Vec<Vec<u32>>,
        }
        #[derive(Deserialize)]
        struct Report {
            cases: Vec<Case>,
        }
        let report: Report = serde_json::from_slice(
            &std::fs::read(std::env::var("ATV_AMBIENT_STREAM_CASES").unwrap()).unwrap(),
        )
        .unwrap();
        let mut comparisons = 0;
        for (index, case) in report.cases.into_iter().enumerate() {
            let base = Emitter {
                life: case.life,
                size: case.size,
                growth: case.growth,
                rate: case.rate,
                velocity: case.velocity,
                age: 0.,
                gravity: 2.,
                flags: 14,
            };
            let output = births(
                &case.config,
                &base,
                case.spread,
                case.position,
                case.dt,
                &mut OriginalRandom::with_seed(case.seed),
            );
            assert_eq!(output.len(), case.output.len(), "stream count case{index}");
            for (raw, expected) in output.into_iter().zip(case.output) {
                let actual = raw
                    .position
                    .into_iter()
                    .chain(raw.velocity)
                    .chain([raw.life, raw.size, raw.growth])
                    .map(f32::to_bits)
                    .chain([raw.mirror])
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "stream words case{index}");
                comparisons += 10;
            }
        }
        eprintln!("native stream raw-word comparisons: {comparisons}");
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Birth {
    pub id: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub birth_time: f32,
    pub life: f32,
    pub size: f32,
    pub growth: f32,
    pub mirror: u32,
    pub tile: usize,
    pub color: [f32; 3],
    pub alpha: [f32; 2],
    pub floor: f32,
}
#[derive(Clone, Debug)]
pub struct Raw {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub life: f32,
    pub size: f32,
    pub growth: f32,
    pub mirror: u32,
}
/// Static finite branch. MovingEmitter policy and native packet batching remain separate.
pub fn births(
    config: &Config,
    base: &Emitter,
    spread: [[f32; 3]; 2],
    position: [f32; 3],
    dt: f32,
    random_source: &mut OriginalRandom,
) -> Vec<Raw> {
    let count = (base.rate * dt).ceil() as usize;
    let mut result = Vec::with_capacity(count * 16);
    for _ in 0..count {
        let rx = random(random_source);
        let ry = random(random_source);
        let rz = random(random_source);
        let velocity: [f32; 3] = std::array::from_fn(|axis| {
            base.velocity[axis]
                + (spread[0][axis] + [rx, ry, rz][axis] * (spread[1][axis] - spread[0][axis]))
        });
        let life = base.life * random(random_source);
        let mut position: [f32; 3] =
            std::array::from_fn(|axis| position[axis] - dt * base.velocity[axis]);
        let growth = base.growth * random(random_source);
        let squared =
            velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2];
        // Finite rsqrt contract rounds the reciprocal once; hardware precision is open.
        let inverse = (1.0f64 / (squared as f64).sqrt()) as f32;
        let step: [f32; 3] = if squared == 0. {
            [0.; 3]
        } else {
            std::array::from_fn(|axis| velocity[axis] * inverse * (config.separation * base.size))
        };
        let length = (config.length[0]
            + (config.length[1] - config.length[0]) * random(random_source)
            + 0.5) as usize;
        let mut velocity: [f32; 3] =
            std::array::from_fn(|axis| velocity[axis] + length as f32 * step[axis]);
        for section in 0..length {
            let extra = usize::from(section + 1 == length && config.bright_head);
            let thickness = (config.thickness[0]
                + (1. / length as f32)
                    * section as f32
                    * (config.thickness[1] - config.thickness[0])
                + 0.5) as usize
                + extra;
            let factor = ((section + 1) as f32 / length as f32).max(0.1);
            let life = life - (1. - factor) * (config.tail_life * life);
            for _ in 0..thickness {
                result.push(Raw {
                    position,
                    velocity,
                    life,
                    size: base.size,
                    growth,
                    mirror: u32::from(rx > 0.5) | (u32::from(ry > 0.5) << 1),
                });
            }
            for axis in 0..3 {
                position[axis] += step[axis];
            }
            velocity[1] -= (1. - factor) * step[1];
            velocity[0] -= step[0] * (factor * factor);
            velocity[2] -= step[2] * (factor * factor);
            velocity[1] -= step[1] * factor;
        }
    }
    result
}
impl Birth {
    pub fn packet(raw: Raw, config: &Config, id: u64, time: f32, tile: usize) -> Self {
        let velocity = std::array::from_fn(|axis| {
            let factor = 32767. / if axis == 1 { 60. } else { 50. };
            ((raw.velocity[axis] * factor) as i32 as i16) as f32 * (1. / factor)
        });
        Self {
            id,
            position: raw.position,
            velocity,
            birth_time: time,
            life: (raw.life as i32 as i16) as f32 * 0.001,
            size: ((raw.size * 655.34) as i32 as i16) as f32 * (1. / 655.34),
            growth: (raw.growth as i32 as i16) as f32,
            mirror: raw.mirror,
            tile,
            color: config.color,
            alpha: config.alpha,
            floor: config.floor,
        }
    }
}
