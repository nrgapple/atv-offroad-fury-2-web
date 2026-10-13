//! Placed original TOY emitters. Finite static type2/type8 paths follow
//! 002427b0/00241120/00242120 and0019d870/0019e150. GPU streams remain explicit.
use crate::{
    Result,
    engine_audio::OriginalRandom,
    physics_data::PhysicsData,
    wheel_effects::{AtlasFrame, Emitter},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CAPACITY: usize = 600;
#[derive(Deserialize)]
pub struct Source {
    pub scene: String,
    #[serde(default)]
    pub common: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub prt: String,
    #[serde(default)]
    pub emts: BTreeMap<String, String>,
}
fn text(s: &str) -> Result<PhysicsData> {
    let s: String = s
        .lines()
        .filter(|l| !l.trim().starts_with(':'))
        .map(|l| format!("{}\n", l.split(';').next().unwrap_or_default().trim()))
        .filter(|l| {
            let l = l.trim();
            l.starts_with('[') || l.split_once('=').is_some_and(|(_, v)| !v.is_empty())
        })
        .collect();
    PhysicsData::parse(s.as_bytes())
}
fn value<'a>(d: &'a PhysicsData, s: &str, k: &str) -> Option<&'a str> {
    d.entries
        .iter()
        .find(|e| e.section.eq_ignore_ascii_case(s) && e.key.eq_ignore_ascii_case(k))
        .map(|e| e.value.as_str())
}
fn number(d: &PhysicsData, s: &str, k: &str, default: f32) -> Result<f32> {
    let n = value(d, s, k).map_or(Ok(default), |v| {
        v.parse::<f32>()
            .map_err(|_| format!("invalid emitter {s}/{k}"))
    })?;
    if !n.is_finite() {
        return Err("nonfinite emitter value".into());
    }
    Ok(n)
}
fn integer(d: &PhysicsData, s: &str, k: &str, default: usize, max: usize) -> Result<usize> {
    let n = number(d, s, k, default as f32)?;
    if n < 0. || n > max as f32 || n.fract() != 0. {
        return Err(format!("invalid emitter count {k}"));
    }
    Ok(n as usize)
}
fn boolean(d: &PhysicsData, s: &str, k: &str, default: bool) -> Result<bool> {
    match value(d, s, k) {
        None => Ok(default),
        Some("T" | "1") => Ok(true),
        Some("F" | "0") => Ok(false),
        _ => Err(format!("invalid emitter boolean {k}")),
    }
}
fn vector(d: &PhysicsData, s: &str, k: &str) -> Result<[f32; 3]> {
    let Some(v) = value(d, s, k) else {
        return Ok([0.; 3]);
    };
    let parts = v
        .split(',')
        .map(|v| {
            v.trim()
                .parse::<f32>()
                .map_err(|_| "invalid emitter vector".to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let result: [f32; 3] = parts
        .try_into()
        .map_err(|_| "emitter vector must have three components")?;
    if !result.iter().all(|x| x.is_finite()) {
        return Err("nonfinite emitter vector".into());
    }
    Ok(result)
}
#[derive(Clone, Debug, Serialize)]
pub struct Placement {
    pub filename: String,
    pub type_id: usize,
    pub position: [f32; 3],
    pub start_on: bool,
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub declared: usize,
    pub placements: Vec<Placement>,
    pub unresolved: Vec<String>,
}
pub fn plan(source: &Source) -> Result<Plan> {
    let scene = crate::scene::parse_with_start(
        source.scene.as_bytes(),
        source.mode.as_deref().map(str::as_bytes),
    )?;
    let mut result = Plan {
        declared: 0,
        placements: vec![],
        unresolved: vec![],
    };
    //002427b0 appends sections from successive TOYs; mode is not a replacement.
    for source_text in [Some(source.common.as_str()), source.mode.as_deref()]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
    {
        let d = text(source_text)?;
        let count = integer(&d, "ParticleEmitters", "NumberOfEmitters", 0, 256)?;
        result.declared += count;
        for i in 1..=count {
            let section = format!("ParticleEmitter{i}");
            let filename = value(&d, &section, "EmitterFilename")
                .ok_or("missing emitter filename")?
                .to_lowercase();
            if filename.contains(['/', '\\', ':']) || filename.len() > 128 {
                return Err("unsafe emitter filename".into());
            }
            let type_id = integer(&d, &section, "TypeID", 0, 13)?;
            let start_on = boolean(&d, &section, "StartOn", true)?;
            let attach = value(&d, &section, "AttachToSoultree").filter(|s| !s.is_empty());
            if let Some(attach) = attach {
                let offset = vector(&d, &section, "OffsetFromSoultree")?;
                let name = attach.trim_end_matches(".slt");
                let limit = integer(&d, &section, "NumberOfInstances", 1, 256)?;
                let mut found = 0;
                for model in scene
                    .models
                    .iter()
                    .filter(|m| m.name.eq_ignore_ascii_case(name))
                    .take(limit)
                {
                    let m = &model.world_matrix;
                    let position = std::array::from_fn(|axis| {
                        m[12 + axis]
                            + m[axis] * offset[0]
                            + m[4 + axis] * offset[1]
                            + m[8 + axis] * offset[2]
                    });
                    result.placements.push(Placement {
                        filename: filename.clone(),
                        type_id,
                        position,
                        start_on,
                    });
                    found += 1;
                }
                if found < limit {
                    result
                        .unresolved
                        .push(format!("{filename}: {found}/{limit} attachment {attach}"));
                }
            } else {
                result.placements.push(Placement {
                    filename,
                    type_id,
                    position: vector(&d, &section, "Position")?,
                    start_on,
                });
            }
        }
    }
    if result.placements.len() > 512 {
        return Err("too many ambient placements".into());
    }
    Ok(result)
}
#[derive(Clone, Debug, Serialize)]
pub struct Atlas {
    pub texture: String,
    pub frames: Vec<AtlasFrame>,
    pub groups: Vec<[usize; 2]>,
}
impl Atlas {
    pub fn parse(s: &str) -> Result<Self> {
        let d = text(s)?;
        let texture = value(&d, "General Info", "TextureMap")
            .ok_or("missing ambient atlas")?
            .to_owned();
        if texture.contains(['/', '\\', ':']) || texture.len() > 128 {
            return Err("unsafe ambient atlas".into());
        }
        let count = integer(&d, "General Info", "NumFrames", 0, 4096)?;
        let groups = integer(&d, "General Info", "NumGroups", 0, 128)?;
        if count == 0 || groups == 0 {
            return Err("empty ambient atlas".into());
        }
        let frames = (0..count)
            .map(|i| {
                let section = format!("Frame{i}");
                let rect = ["UVLeft", "UVTop", "UVRight", "UVBottom"]
                    .map(|k| number(&d, &section, k, -1.))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;
                let rect: [f32; 4] = rect.try_into().unwrap();
                if rect.iter().any(|v| *v < 0. || *v > 4096.)
                    || rect[0] > rect[2]
                    || rect[1] > rect[3]
                {
                    return Err("invalid ambient atlas rectangle".into());
                }
                Ok(AtlasFrame { rect })
            })
            .collect::<Result<Vec<_>>>()?;
        let groups = (0..groups)
            .map(|i| {
                let s = format!("Group{i}");
                let first = integer(&d, &s, "FirstFrame", 0, count - 1)?;
                let last = integer(&d, &s, "LastFrame", 0, count - 1)?;
                if first > last {
                    return Err("invalid ambient atlas group".into());
                }
                Ok([first, last])
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            texture,
            frames,
            groups,
        })
    }
}
#[derive(Clone)]
struct Definition {
    base: Emitter,
    spread: [[f32; 3]; 2],
    group: [usize; 2],
    frames_per_set: usize,
    mirror: [bool; 2],
    ground: Option<f32>,
    restitution: f32,
    spread_velocity: bool,
    lit: bool,
    additive: bool,
    emit_mode: usize,
}
#[derive(Clone)]
struct Runtime {
    placement: Placement,
    definition: Definition,
    phase: f32,
    first: bool,
    frame: usize,
}
#[derive(Clone)]
struct Particle {
    position: [f32; 3],
    velocity: [f32; 3],
    size: f32,
    initial_size: f32,
    growth: f32,
    life: f32,
    age: f32,
    frame: usize,
    set_first: usize,
    emitter: usize,
    mirror: [bool; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sprite {
    pub position: [f32; 3],
    pub size: f32,
    pub alpha: f32,
    pub frame: usize,
    pub mirror: [bool; 2],
    pub additive: bool,
    pub lit: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Frame {
    pub time: f32,
    pub particles: Vec<Sprite>,
    pub emitted: u64,
    pub total_emitted: u64,
    pub dropped: u64,
}
#[derive(Debug, Serialize)]
pub struct Metadata {
    pub atlas: Option<Atlas>,
    pub plan: Plan,
    pub active: usize,
    pub unsupported: Vec<String>,
    pub capacity: usize,
}
pub struct Effects {
    runtime: Vec<Runtime>,
    particles: Vec<Particle>,
    random: OriginalRandom,
    serial: u32,
    time: f32,
    total: u64,
    dropped: u64,
    frame: Frame,
}
fn random(r: &mut OriginalRandom) -> f32 {
    (r.next_u31() as f32) * 4.656_613e-10
}
/// Native accumulation discards the fractional remainder after a successful batch.
fn emission_count(phase: &mut f32, amount: f32) -> usize {
    if amount > 1. {
        *phase = 0.;
        amount as usize
    } else {
        *phase += amount;
        if *phase > 1. {
            let count = *phase as usize;
            *phase = 0.;
            count
        } else {
            0
        }
    }
}
impl Effects {
    pub fn from_source(source: &Source) -> Result<(Self, Metadata)> {
        let plan = plan(source)?;
        let atlas = if source.prt.is_empty() {
            None
        } else {
            Some(Atlas::parse(&source.prt)?)
        };
        let mut runtime = vec![];
        let mut unsupported = plan.unresolved.clone();
        for placement in &plan.placements {
            // Freeport fog/raindrops belong to the camera weather path.
            if ![2, 8].contains(&placement.type_id) {
                unsupported.push(format!(
                    "{}: original emitter type{} not connected",
                    placement.filename, placement.type_id
                ));
                continue;
            }
            let s = source
                .emts
                .get(&placement.filename)
                .ok_or_else(|| format!("missing ambient EMT {}", placement.filename))?;
            let d = text(s)?;
            let base = Emitter::parse(s.as_bytes(), 0)?;
            let group = integer(&d, "General Info", "ParticleGroup", 0, 127)?;
            let group = *atlas
                .as_ref()
                .and_then(|a| a.groups.get(group))
                .ok_or("ambient group outside original atlas")?;
            let emit_mode = integer(&d, "General Info", "EmitMode", 0, 3)?;
            if emit_mode != 0 {
                unsupported.push(format!(
                    "{}: timed/random emitter mode{emit_mode} not connected",
                    placement.filename
                ));
                continue;
            }
            let definition = Definition {
                base,
                spread: [
                    vector(&d, "General Info", "MinRange")?,
                    vector(&d, "General Info", "MaxRange")?,
                ],
                group,
                frames_per_set: integer(&d, "Animation", "FramesPerSet", 0, 128)?,
                mirror: [
                    boolean(&d, "Properties", "RandomMirrorAboutX", false)?,
                    boolean(&d, "Properties", "RandomMirrorAboutY", false)?,
                ],
                ground: value(&d, "General Info", "GroundLevel")
                    .map(|_| number(&d, "General Info", "GroundLevel", 0.))
                    .transpose()?,
                restitution: number(&d, "General Info", "Restitution", 0.55)?,
                spread_velocity: boolean(&d, "General Info", "ApplySpreadToVelocity", true)?,
                lit: boolean(&d, "Properties", "Lit", true)?,
                additive: boolean(&d, "Properties", "Additive", false)?,
                emit_mode,
            };
            runtime.push(Runtime {
                placement: placement.clone(),
                frame: group[0],
                definition,
                phase: 0.,
                first: true,
            });
        }
        let metadata = Metadata {
            active: runtime.len(),
            atlas,
            plan,
            unsupported,
            capacity: CAPACITY,
        };
        Ok((
            Self {
                runtime,
                particles: Vec::with_capacity(CAPACITY),
                random: OriginalRandom::with_seed(1),
                serial: 0,
                time: 0.,
                total: 0,
                dropped: 0,
                frame: Frame::default(),
            },
            metadata,
        ))
    }
    pub fn step(&mut self, dt: f32) -> Result<&Frame> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err("invalid ambient fixed step".into());
        }
        self.time += dt;
        //00143aa0 moves before gravity and tests ground after velocity update.
        self.particles.retain_mut(|p| {
            let d = &self.runtime[p.emitter].definition;
            p.age += dt * 1000.;
            if p.life < p.age {
                return false;
            }
            if d.base.flags & 8 != 0 {
                let growth = p.growth * dt;
                p.size += growth;
                if p.size <= 0. {
                    p.size = 0.;
                    p.age = p.life;
                } else {
                    p.position[1] += growth;
                }
            }
            if d.base.flags & 4 != 0 {
                for axis in 0..3 {
                    p.position[axis] += p.velocity[axis] * dt;
                }
            }
            if d.base.flags & 0x20 != 0 {
                p.velocity[1] -= d.base.gravity * 32.2 * dt;
            }
            if let Some(ground) = d.ground
                && p.position[1] < ground
                && p.velocity[1] < 0.
            {
                for v in &mut p.velocity {
                    *v *= d.restitution;
                }
                p.velocity[1] = -p.velocity[1];
                if (p.position[1] - ground).abs() < 0.1 {
                    p.velocity[1] = 0.;
                }
            }
            if d.frames_per_set > 1 {
                p.frame = p.set_first + (p.age / (p.life / d.frames_per_set as f32)) as usize;
                p.frame = p
                    .frame
                    .min(p.set_first + d.frames_per_set - 1)
                    .min(d.group[1]);
            }
            true
        });
        let mut emitted = 0;
        for (index, r) in self.runtime.iter_mut().enumerate() {
            if r.first {
                r.first = false;
                continue;
            } //0019a680 constructor dirty-position gate.
            if !r.placement.start_on || r.definition.emit_mode != 0 {
                continue;
            }
            let d = &r.definition;
            let base = &d.base;
            let factor = if r.placement.type_id == 8 {
                0.6
            } else if d.spread_velocity {
                let speed = base.velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
                if speed < 0.5 {
                    0.
                } else {
                    (speed / 5.).min(1.)
                }
            } else {
                1.
            };
            let count = emission_count(&mut r.phase, base.rate * dt * factor);
            for _ in 0..count {
                if self.particles.len() == CAPACITY {
                    self.dropped += 1;
                    break;
                }
                let rx = random(&mut self.random);
                let ry = random(&mut self.random);
                let rz = random(&mut self.random);
                let spread = std::array::from_fn::<_, 3, _>(|axis| {
                    d.spread[0][axis] + [rx, ry, rz][axis] * (d.spread[1][axis] - d.spread[0][axis])
                });
                let (position, velocity, life, growth, frame) = if r.placement.type_id == 8 {
                    self.serial = self.serial.wrapping_add(1);
                    let set = if rz > 0.66 { 2 } else { usize::from(rz > 0.33) };
                    let frame = (d.group[0] + set * d.frames_per_set).min(d.group[1]);
                    //Static source8 uses emitter movement velocity (zero), plus authored Y.
                    (
                        [
                            r.placement.position[0] + spread[0],
                            r.placement.position[1],
                            r.placement.position[2] + spread[2],
                        ],
                        [0., base.velocity[1] * ry, 0.],
                        base.life - base.life * (self.serial & 3) as f32 * 0.125,
                        base.growth,
                        frame,
                    )
                } else {
                    let growth = base.growth * random(&mut self.random);
                    let life = base.life * random(&mut self.random);
                    let velocity = std::array::from_fn(|axis| base.velocity[axis] + spread[axis]);
                    let frame = r.frame;
                    r.frame += 1;
                    if r.frame > d.group[1] {
                        r.frame = d.group[0];
                    }
                    (
                        r.placement.position,
                        velocity,
                        life.max(0.001),
                        growth,
                        frame,
                    )
                };
                self.particles.push(Particle {
                    position,
                    velocity,
                    size: base.size,
                    initial_size: base.size,
                    growth,
                    life,
                    age: 0.,
                    frame,
                    set_first: frame,
                    emitter: index,
                    mirror: [d.mirror[0] && rx > 0.5, d.mirror[1] && ry > 0.5],
                });
                emitted += 1;
                self.total += 1;
            }
        }
        let particles = self
            .particles
            .iter()
            .map(|p| {
                let d = &self.runtime[p.emitter].definition;
                let ratio = p.age / p.life;
                let alpha = if d.base.flags & 2 != 0 {
                    0.5 * (1. - ratio)
                } else if d.base.flags & 0x1000 != 0 {
                    0.5 * ratio * p.initial_size.powi(3) / p.size.powi(3)
                } else {
                    0.5
                };
                Sprite {
                    position: p.position,
                    size: p.size,
                    alpha,
                    frame: p.frame,
                    mirror: p.mirror,
                    additive: d.additive,
                    lit: d.lit,
                }
            })
            .collect();
        self.frame = Frame {
            time: self.time,
            particles,
            emitted,
            total_emitted: self.total,
            dropped: self.dropped,
        };
        Ok(&self.frame)
    }
    pub fn frame(&self) -> &Frame {
        &self.frame
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_and_mode_append_and_local_offsets_follow_model_basis() {
        let scene = "[SceneInfo]\nDefaultStartPosition=1,2,3\nDefaultStartDirection=0,0,1\n[Environment]\nCubeFile=synthetic.cub\n[Lights]\nNumberOfLights=1\n[Light1]\nType=Ambient\nColorRGB=1,2,3\n[Fog]\nColorRGB=0,0,0\n[StaticModels]\nNumberOfStaticModels=1\n[Model1]\nSLT=prop.slt\nPosition=10,20,30\nLookVector=1,0,0\n";
        let common = "[ParticleEmitters]\nNumberOfEmitters=1\n[ParticleEmitter1]\nTypeID=8\nAttachToSoultree=prop.slt\nOffsetFromSoultree=0,5,0\nEmitterFilename=smoke.emt\nNumberOfInstances=2\n";
        let mode = "[ParticleEmitters]\nNumberOfEmitters=1\n[ParticleEmitter1]\nTypeID=2\nPosition=9,8,7\nEmitterFilename=spray.emt\nStartOn=F\n";
        let source = Source {
            scene: scene.into(),
            common: common.into(),
            mode: Some(mode.into()),
            prt: String::new(),
            emts: BTreeMap::new(),
        };
        let result = plan(&source).unwrap();
        assert_eq!(result.declared, 2);
        assert_eq!(result.placements.len(), 2);
        assert_eq!(result.placements[0].position, [10., 25., 30.]);
        assert!(!result.placements[1].start_on);
        assert_eq!(result.unresolved, ["smoke.emt: 1/2 attachment prop.slt"]);
    }
    #[test]
    fn native_fractional_emission_discards_remainder_and_retains_strict_gate() {
        let mut phase = 0.;
        assert_eq!(emission_count(&mut phase, 1.), 0);
        assert_eq!(emission_count(&mut phase, 0.2), 1);
        assert_eq!(phase, 0.);
        assert_eq!(emission_count(&mut phase, 2.7), 2);
        assert_eq!(phase, 0.);
    }
    #[test]
    fn no_unbounded_or_invalid_atlas_values() {
        assert!(
            Atlas::parse("[General Info]\nTextureMap=../bad\nNumFrames=1\nNumGroups=1").is_err()
        );
        let d = text("[General Info]\nValue=NaN").unwrap();
        assert!(number(&d, "General Info", "Value", 0.).is_err());
    }
    #[test]
    #[ignore = "requires privately generated original-instruction cases"]
    fn private_original_birth_cases() {
        #[derive(Deserialize)]
        struct Case {
            kind: usize,
            seed: u64,
            dt: f32,
            rate: f32,
            life: f32,
            size: f32,
            growth: f32,
            velocity: [f32; 3],
            position: [f32; 3],
            spread: [[f32; 3]; 2],
            phase: f32,
            fps: usize,
            flags: u32,
            expected: Vec<Vec<u32>>,
            out_phase: u32,
        }
        #[derive(Deserialize)]
        struct Report {
            cases: Vec<Case>,
        }
        let path = std::env::var("ATV_AMBIENT_NATIVE_CASES").expect("set ATV_AMBIENT_NATIVE_CASES");
        let report: Report = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut comparisons = 0;
        for (index, c) in report.cases.into_iter().enumerate() {
            let definition = Definition {
                base: Emitter {
                    life: c.life,
                    size: c.size,
                    growth: c.growth,
                    rate: c.rate,
                    age: 0.,
                    velocity: c.velocity,
                    flags: c.flags,
                    gravity: 1.,
                },
                spread: c.spread,
                group: [128, 151],
                frames_per_set: c.fps,
                mirror: [true, true],
                ground: None,
                restitution: 1.,
                spread_velocity: true,
                lit: false,
                additive: false,
                emit_mode: 0,
            };
            let runtime = Runtime {
                placement: Placement {
                    filename: String::new(),
                    type_id: c.kind,
                    position: c.position,
                    start_on: true,
                },
                definition,
                phase: c.phase,
                first: false,
                frame: 128,
            };
            let mut effects = Effects {
                runtime: vec![runtime],
                particles: vec![],
                random: OriginalRandom::with_seed(c.seed),
                serial: 0,
                time: 0.,
                total: 0,
                dropped: 0,
                frame: Frame::default(),
            };
            effects.step(c.dt).unwrap();
            assert_eq!(
                effects.particles.len(),
                c.expected.len(),
                "birth count case{index}"
            );
            assert_eq!(
                effects.runtime[0].phase.to_bits(),
                c.out_phase,
                "phase case{index}"
            );
            comparisons += 1;
            for (p, native) in effects.particles.iter().zip(c.expected) {
                let words = p
                    .position
                    .into_iter()
                    .chain(p.velocity)
                    .chain([p.life, p.size, p.growth])
                    .map(f32::to_bits)
                    .chain([p.frame as u32])
                    .collect::<Vec<_>>();
                assert_eq!(words, &native[..10], "birth words case{index}");
                assert_eq!(
                    p.mirror,
                    [native[10] & 0x4000 != 0, native[10] & 0x8000 != 0],
                    "mirror case{index}"
                );
                comparisons += 12;
            }
        }
        println!("native ambient birth comparisons: {comparisons}");
    }
}
