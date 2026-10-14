//! Persistent type12 volume:001a1070/001a12e0 and triangle clock001a0f30.
use super::{Emitter, OriginalRandom, PhysicsData, Result, number, random};
use serde::Deserialize;
#[derive(Clone, Deserialize)]
pub struct Config {
    pub size: [f32; 2],
    pub count: f32,
    pub speed: f32,
    pub magnitude: f32,
    pub fade_distance: f32,
}
impl Config {
    pub fn parse(d: &PhysicsData, base: &Emitter) -> Result<Self> {
        let config = Self {
            size: [
                number(d, "General Info", "MinSize", base.size)?,
                number(d, "General Info", "MaxSize", base.size)?,
            ],
            count: number(d, "General Info", "ParticlesInVolume", 100.)?.max(number(
                d,
                "General Info",
                "MinParticlesPerFrame",
                0.,
            )?),
            speed: number(d, "General Info", "OscillationSpeed", 10.)?,
            magnitude: number(d, "General Info", "OscillationMagnitude", 1.)?,
            fade_distance: number(d, "General Info", "DistanceToFade", 200.)?,
        };
        if config.size[0] <= 0.
            || config.size[1] < config.size[0]
            || config.count < 0.
            || config.count > 10000.
            || (config.speed != -1. && config.speed <= 0.)
            || config.magnitude < 0.
            || config.fade_distance <= 0.
        {
            return Err("invalid original fog volume bounds".into());
        }
        Ok(config)
    }
}
#[derive(Clone, Default)]
pub struct Oscillator {
    pub elapsed: f32,
    pub phase: f32,
    pub descending: bool,
}
impl Oscillator {
    pub fn step(&mut self, speed: f32, dt: f32) {
        if speed == -1. {
            return;
        }
        self.elapsed += if self.descending { -dt } else { dt };
        // Source computes the phase before clamping: one step can overshoot.
        self.phase = self.elapsed / speed;
        if self.elapsed >= 0. && self.phase > 1. {
            self.descending = true;
            self.elapsed = speed - speed / 1000.;
        } else if self.elapsed < 0. && self.phase < -1. {
            self.descending = false;
            self.elapsed = -speed + speed / 1000.;
        }
    }
}
pub struct Initial {
    pub position: [f32; 3],
    pub noise: [f32; 3],
    pub size: f32,
    pub mirror: [bool; 2],
}
pub fn initial(
    c: &Config,
    origin: [f32; 3],
    spread: [[f32; 3]; 2],
    mirror: [bool; 2],
    rng: &mut OriginalRandom,
) -> Initial {
    let samples = [random(rng), random(rng), random(rng)];
    let position = std::array::from_fn(|axis| {
        origin[axis] + (spread[0][axis] + samples[axis] * (spread[1][axis] - spread[0][axis]))
    });
    let mut noise = [0.; 3];
    if c.speed != -1. {
        for axis in [2, 1, 0] {
            noise[axis] = c.magnitude * random(rng);
        }
        for n in &mut noise {
            if random(rng) > 0.5 {
                *n *= -1.;
            }
        }
    }
    Initial {
        position,
        noise,
        size: c.size[0] + (c.size[1] - c.size[0]) * random(rng),
        mirror: [mirror[0] && samples[0] > 0.5, mirror[1] && samples[1] > 0.5],
    }
}
pub fn move_particle(position: &mut [f32; 3], noise: [f32; 3], speed: f32, phase: f32) {
    if speed != -1. {
        for axis in 0..3 {
            position[axis] += phase * noise[axis];
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triangle_clock_retains_native_strict_edges_and_overshoot() {
        let mut clock = Oscillator::default();
        clock.step(0.1, 0.1);
        assert_eq!(clock.phase, 1.);
        assert!(!clock.descending);
        clock.step(0.1, 0.01);
        assert!(clock.phase > 1.);
        assert!(clock.descending);
        assert_eq!(clock.elapsed, 0.1 - 0.1 / 1000.);
        let before = clock.clone();
        clock.step(-1., 0.05);
        assert_eq!(clock.elapsed, before.elapsed);
        assert_eq!(clock.phase, before.phase);
        for _ in 0..21 {
            clock.step(0.1, 0.01);
        }
        assert!(!clock.descending);
    }
    #[test]
    #[ignore = "requires private original fog clock/motion instruction cases"]
    fn private_original_fog_updates() {
        #[derive(Deserialize)]
        struct Clock {
            speed: f32,
            elapsed: f32,
            phase: f32,
            descending: bool,
            dt: f32,
            output: Vec<[u32; 3]>,
        }
        #[derive(Deserialize)]
        struct Move {
            position: [f32; 3],
            noise: [f32; 3],
            phase: f32,
            speed: f32,
            output: [u32; 3],
        }
        #[derive(Deserialize)]
        struct Report {
            clocks: Vec<Clock>,
            moves: Vec<Move>,
        }
        let path = std::env::var("ATV_AMBIENT_FOG_UPDATE_CASES").expect("private fog update path");
        let report: Report = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut checked = 0;
        for c in report.clocks {
            let mut clock = Oscillator {
                elapsed: c.elapsed,
                phase: c.phase,
                descending: c.descending,
            };
            for expected in c.output {
                clock.step(c.speed, c.dt);
                assert_eq!(
                    [
                        clock.elapsed.to_bits(),
                        clock.phase.to_bits(),
                        u32::from(clock.descending)
                    ],
                    expected
                );
                checked += 3;
            }
        }
        for mut c in report.moves {
            move_particle(&mut c.position, c.noise, c.speed, c.phase);
            assert_eq!(c.position.map(f32::to_bits), c.output);
            checked += 3;
        }
        println!("original fog clock/motion: {checked} raw comparisons");
    }
    #[test]
    #[ignore = "requires private original fog instruction cases"]
    fn private_original_fog_initialization() {
        #[derive(Deserialize)]
        struct Case {
            seed: u64,
            position: [f32; 3],
            spread: [[f32; 3]; 2],
            count: usize,
            life: f32,
            age: f32,
            size: [f32; 2],
            magnitude: f32,
            osc_speed: f32,
            first: usize,
            last: usize,
            output: Vec<[u32; 13]>,
            next_frame: usize,
        }
        #[derive(Deserialize)]
        struct Report {
            cases: Vec<Case>,
        }
        let path = std::env::var("ATV_AMBIENT_FOG_CASES").expect("private fog cases path");
        let report: Report = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut checked = 0;
        for case in report.cases {
            let mut rng = OriginalRandom::with_seed(case.seed);
            let c = Config {
                size: case.size,
                count: case.count as f32,
                speed: case.osc_speed,
                magnitude: case.magnitude,
                fade_distance: 20.,
            };
            let mut frame = case.first;
            assert_eq!(case.output.len(), case.count);
            for expected in case.output {
                let p = initial(&c, case.position, case.spread, [true; 2], &mut rng);
                let words = [
                    p.position[0].to_bits(),
                    p.position[1].to_bits(),
                    p.position[2].to_bits(),
                    p.noise[0].to_bits(),
                    p.noise[1].to_bits(),
                    p.noise[2].to_bits(),
                    frame as u32,
                    case.age.to_bits(),
                    case.life.to_bits(),
                    (1. / case.life).to_bits(),
                    p.size.to_bits(),
                    p.size.to_bits(),
                    0x80 | if p.mirror[0] { 0x4000 } else { 0 }
                        | if p.mirror[1] { 0x8000 } else { 0 },
                ];
                assert_eq!(words, expected, "fog init seed {}", case.seed);
                checked += 13;
                frame =
                    case.first + ((case.last - case.first + 1) as f32 * random(&mut rng)) as usize;
            }
            assert_eq!(frame, case.next_frame);
        }
        println!("original fog initializer: {checked} raw field comparisons");
    }
}
