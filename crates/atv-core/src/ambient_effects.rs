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

mod fog;
mod streams;
pub const GPU_CAPACITY: usize = 400 * 12;
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
// The indexed TOY emitter reader selects only its own sections. Shipped
// unrelated terrain sections can contain malformed multiline authoring headers.
fn emitter_document(source: &str) -> Result<Option<PhysicsData>> {
    if source.len() > 1024 * 1024 || source.contains('\0') {
        return Err("invalid ambient TOY extent".into());
    }
    let mut selected = String::new();
    let mut active = false;
    for line in source.lines() {
        let line = line.split(';').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            let lower = line.to_lowercase();
            active = lower == "[particleemitters]"
                || lower
                    .strip_prefix("[particleemitter")
                    .and_then(|s| s.strip_suffix(']'))
                    .is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
        }
        if active && (line.starts_with('[') || line.contains('=')) {
            selected.push_str(line);
            selected.push('\n');
        }
    }
    if selected.is_empty() {
        Ok(None)
    } else {
        crate::scene::document(selected.as_bytes()).map(Some)
    }
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
    // Particle sidecars never supply the gameplay spawn.
    let models = crate::scene::static_models(source.scene.as_bytes())?;
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
        let Some(d) = emitter_document(source_text)? else {
            continue;
        };
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
                for model in models
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
#[derive(Clone, Deserialize)]
struct Smoke {
    size: [f32; 2],
    life: [f32; 2],
    rate: [f32; 2],
    percent: f32,
    convergence: f32,
    initial_age: f32,
    noise: [[f32; 3]; 2],
    minimum_speed: f32,
}
#[derive(Clone, Default)]
struct Timer {
    remaining: f32,
    delay: f32,
    on: bool,
    emitted: bool,
}
impl Timer {
    fn active(
        &mut self,
        mode: usize,
        on_range: [f32; 2],
        off_range: [f32; 2],
        random_source: &mut OriginalRandom,
        dt: f32,
    ) -> bool {
        if self.delay > 0. {
            if self.on {
                self.delay -= dt;
            }
            return false;
        }
        match mode {
            1 => {
                if !self.on {
                    return false;
                }
                if self.remaining > 0. {
                    self.remaining -= dt;
                    return true;
                }
                if self.emitted {
                    self.on = false;
                    return false;
                }
            }
            2 | 3 if self.remaining > 0. => {
                self.remaining -= dt;
                if self.remaining > 0. {
                    return self.on;
                }
                self.on = !self.on;
                let range = if self.on { on_range } else { off_range };
                self.remaining = range[usize::from(mode == 3)]
                    + (range[1] - range[0]) * if mode == 2 { random(random_source) } else { 0. };
                return self.on;
            }
            _ => {}
        }
        self.on
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
    smoke: Option<Smoke>,
    stream: Option<streams::Config>,
    fog: Option<fog::Config>,
    on_range: [f32; 2],
    off_range: [f32; 2],
    delay: f32,
}
#[derive(Clone)]
struct Runtime {
    placement: Placement,
    definition: Definition,
    phase: f32,
    first: bool,
    frame: usize,
    timer: Timer,
    oscillator: fog::Oscillator,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade_distance: Option<f32>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Frame {
    pub time: f32,
    pub particles: Vec<Sprite>,
    pub emitted: u64,
    pub total_emitted: u64,
    pub dropped: u64,
    pub gpu_births: Vec<streams::Birth>,
    pub gpu_total: u64,
}
#[derive(Debug, Serialize)]
pub struct Metadata {
    pub atlas: Option<Atlas>,
    pub plan: Plan,
    pub active: usize,
    pub unsupported: Vec<String>,
    pub capacity: usize,
    pub cpu_active: usize,
    pub gpu_active: usize,
    pub gpu_capacity: usize,
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
    gpu_total: u64,
    gpu_initialized: bool,
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
        let mut random_source = OriginalRandom::with_seed(1);
        let mut unsupported = plan.unresolved.clone();
        for placement in &plan.placements {
            // Placed raindrops and fog use the CPU atlas; Weather selection/Snow are separate.
            if ![0, 2, 8, 11, 12].contains(&placement.type_id) {
                unsupported.push(format!(
                    "{}: original emitter type{} not connected",
                    placement.filename, placement.type_id
                ));
                continue;
            }
            let Some(s) = source.emts.get(&placement.filename) else {
                unsupported.push(format!(
                    "{}: original EMT missing; native constructor fallback not connected",
                    placement.filename
                ));
                continue;
            };
            let d = text(s)?;
            let mut base = Emitter::parse(s.as_bytes(), 0)?;
            let fog = if placement.type_id == 12 {
                base.flags = (base.flags | 0x80) & !8;
                Some(fog::Config::parse(&d, &base)?)
            } else {
                None
            };
            let group = integer(&d, "General Info", "ParticleGroup", 0, 127)?;
            let group = *atlas
                .as_ref()
                .and_then(|a| a.groups.get(group))
                .ok_or("ambient group outside original atlas")?;
            let emit_mode = integer(&d, "General Info", "EmitMode", 0, 3)?;
            let ranges = if emit_mode == 2 {
                [
                    [
                        number(&d, "General Info", "RandomMinTimeOn", 0.)?,
                        number(&d, "General Info", "RandomMaxTimeOn", 0.)?,
                    ],
                    [
                        number(&d, "General Info", "RandomMinTimeOff", 0.)?,
                        number(&d, "General Info", "RandomMaxTimeOff", 0.)?,
                    ],
                ]
            } else {
                [
                    [number(
                        &d,
                        "General Info",
                        if emit_mode == 1 {
                            "OneShotTime"
                        } else {
                            "TimedTimeOn"
                        },
                        0.,
                    )?; 2],
                    [number(&d, "General Info", "TimedTimeOff", 0.)?; 2],
                ]
            };
            let smoke = if placement.type_id == 0 {
                Some(Smoke {
                    size: [
                        number(&d, "General Info", "MinimumInitialSize", 0.)?,
                        number(&d, "General Info", "MaximumInitialSize", 5.)?,
                    ],
                    life: [
                        number(&d, "General Info", "MinimumLife", 500.)?,
                        number(&d, "General Info", "MaximumLife", 3500.)?,
                    ],
                    rate: [
                        number(&d, "General Info", "MinimumEmitRate", 0.)?,
                        number(&d, "General Info", "MaximumEmitRate", 10.)?,
                    ],
                    percent: number(&d, "General Info", "MinimumPercentLife", 0.5)?,
                    convergence: number(&d, "General Info", "MinimumPercentLifeConvergance", 1.)?,
                    initial_age: number(&d, "General Info", "InitialPercentage", 0.)?,
                    noise: [
                        vector(&d, "General Info", "MinimumVelocityNoise")?,
                        vector(&d, "General Info", "MaximumVelocityNoise")?,
                    ],
                    minimum_speed: number(&d, "General Info", "MinimumEmissionSpeed", 0.)?,
                })
            } else {
                None
            };
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
                smoke,
                fog,
                stream: if placement.type_id == 11 {
                    Some(streams::Config::parse(&d)?)
                } else {
                    None
                },
                on_range: ranges[0],
                off_range: ranges[1],
                delay: number(&d, "General Info", "DelayTime", 0.)?,
            };
            let timer = Timer {
                on: placement.start_on,
                remaining: ranges[0][0]
                    + (ranges[0][1] - ranges[0][0])
                        * if emit_mode == 2 {
                            random(&mut random_source)
                        } else {
                            0.
                        },
                delay: definition.delay,
                emitted: false,
            };
            runtime.push(Runtime {
                placement: placement.clone(),
                frame: group[0],
                definition,
                phase: 0.,
                first: true,
                timer,
                oscillator: fog::Oscillator::default(),
            });
        }
        // Stream emitters keep their authored order while their worst-case
        // packets fit; race-only firework sets beyond the bounded GPU packet
        // are reported instead of failing the whole course load.
        let mut stream_budget = 0usize;
        runtime.retain(|r| {
            let Some(c) = r.definition.stream.as_ref() else {
                return true;
            };
            let cost = (r.definition.base.rate * 0.1).ceil() as usize
                * c.length[1].ceil() as usize
                * (c.thickness[1].ceil() as usize + usize::from(c.bright_head));
            if stream_budget + cost > GPU_CAPACITY {
                unsupported.push(format!(
                    "{}: stream exceeds bounded packet budget",
                    r.placement.filename
                ));
                return false;
            }
            stream_budget += cost;
            true
        });
        let metadata = Metadata {
            active: runtime.len(),
            cpu_active: runtime.iter().filter(|r| r.placement.type_id != 11).count(),
            gpu_active: runtime.iter().filter(|r| r.placement.type_id == 11).count(),
            gpu_capacity: GPU_CAPACITY,
            atlas,
            plan,
            unsupported,
            capacity: CAPACITY,
        };
        let mut effects = Self {
            runtime,
            particles: Vec::with_capacity(CAPACITY),
            random: random_source,
            serial: 0,
            time: 0.,
            total: 0,
            dropped: 0,
            frame: Frame::default(),
            gpu_total: 0,
            gpu_initialized: false,
        };
        effects.initialize_fog();
        effects.frame.particles = effects.sprites();
        effects.frame.total_emitted = effects.total;
        effects.frame.dropped = effects.dropped;
        Ok((effects, metadata))
    }
    fn initialize_fog(&mut self) {
        for (index, r) in self.runtime.iter_mut().enumerate() {
            let Some(c) = &r.definition.fog else {
                continue;
            };
            let wanted = c.count.ceil() as usize;
            let available = CAPACITY - self.particles.len();
            self.dropped += wanted.saturating_sub(available) as u64;
            for _ in 0..wanted.min(available) {
                let p = fog::initial(
                    c,
                    r.placement.position,
                    r.definition.spread,
                    r.definition.mirror,
                    &mut self.random,
                );
                self.particles.push(Particle {
                    position: p.position,
                    velocity: p.noise,
                    size: p.size,
                    initial_size: p.size,
                    growth: 0.,
                    life: r.definition.base.life,
                    age: r.definition.base.age * r.definition.base.life,
                    frame: r.frame,
                    set_first: r.frame,
                    emitter: index,
                    mirror: p.mirror,
                });
                r.frame = r.definition.group[0]
                    + ((r.definition.group[1] - r.definition.group[0] + 1) as f32
                        * random(&mut self.random)) as usize;
                self.total += 1;
            }
        }
    }
    pub fn step(&mut self, dt: f32) -> Result<&Frame> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err("invalid ambient fixed step".into());
        }
        self.time += dt;
        for r in &mut self.runtime {
            if let Some(c) = &r.definition.fog {
                r.oscillator.step(c.speed, dt);
            }
        }
        //00143aa0 moves before gravity and tests ground after velocity update.
        self.particles.retain_mut(|p| {
            let r = &self.runtime[p.emitter];
            let d = &r.definition;
            if let Some(c) = &d.fog {
                fog::move_particle(&mut p.position, p.velocity, c.speed, r.oscillator.phase);
                return true;
            }
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
        let mut gpu_births = vec![];
        for (index, r) in self.runtime.iter_mut().enumerate() {
            if r.definition.fog.is_some() {
                continue;
            }
            if r.first {
                r.first = false;
                continue;
            } //0019a680 constructor dirty-position gate.
            if !r.timer.active(
                r.definition.emit_mode,
                r.definition.on_range,
                r.definition.off_range,
                &mut self.random,
                dt,
            ) {
                continue;
            }
            let d = &r.definition;
            let base = &d.base;
            if let Some(config) = &d.stream {
                if !self.gpu_initialized {
                    self.gpu_initialized = true;
                    continue;
                }
                let births = streams::births(
                    config,
                    base,
                    d.spread,
                    r.placement.position,
                    dt,
                    &mut self.random,
                );
                let streams = (base.rate * dt).ceil() as usize;
                for _ in 0..streams {
                    r.frame += 1;
                    if r.frame > d.group[1] {
                        r.frame = d.group[0];
                    }
                }
                let tile = r.frame - d.group[0];
                for raw in births {
                    self.gpu_total += 1;
                    let birth =
                        streams::Birth::packet(raw, config, self.gpu_total, self.time, tile);
                    gpu_births.push(birth);
                }
                continue;
            }
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
            let count = if let Some(smoke) = &d.smoke {
                if smoke.minimum_speed > 0. {
                    0
                } else {
                    let amount = smoke.rate[0] * dt + r.phase;
                    if amount > 1. {
                        let count = amount as usize;
                        r.phase = amount - count as f32;
                        count
                    } else {
                        r.phase += amount;
                        0
                    }
                }
            } else {
                emission_count(&mut r.phase, base.rate * dt * factor)
            };
            if count > 0 {
                r.timer.emitted = true;
            }
            for _ in 0..count {
                if self.particles.len() == CAPACITY {
                    self.dropped += 1;
                    break;
                }
                if let Some(smoke) = &d.smoke {
                    let life = smoke.life[0] * smoke.percent
                        + smoke.life[0]
                            * (1. - smoke.percent)
                            * (1. / (smoke.convergence * random(&mut self.random) + 1.));
                    let mut velocity = base.velocity;
                    for (axis, v) in velocity.iter_mut().enumerate() {
                        *v += smoke.noise[0][axis]
                            + (smoke.noise[1][axis] - smoke.noise[0][axis])
                                * random(&mut self.random);
                    }
                    self.particles.push(Particle {
                        position: r.placement.position,
                        velocity,
                        size: smoke.size[0],
                        initial_size: smoke.size[0],
                        growth: smoke.size[0] * base.growth,
                        life,
                        age: smoke.life[0] * smoke.initial_age,
                        frame: r.frame,
                        set_first: r.frame,
                        emitter: index,
                        mirror: [false; 2],
                    });
                    r.frame = d.group[0]
                        + ((d.group[1] - d.group[0] + 1) as f32 * random(&mut self.random))
                            as usize;
                    r.timer.emitted = true;
                    emitted += 1;
                    self.total += 1;
                    continue;
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
                r.timer.emitted = true;
                emitted += 1;
                self.total += 1;
            }
        }
        let particles = self.sprites();
        self.frame = Frame {
            time: self.time,
            particles,
            emitted,
            total_emitted: self.total,
            dropped: self.dropped,
            gpu_births,
            gpu_total: self.gpu_total,
        };
        Ok(&self.frame)
    }
    fn sprites(&self) -> Vec<Sprite> {
        self.particles
            .iter()
            .map(|p| {
                let d = &self.runtime[p.emitter].definition;
                let ratio = p.age / p.life;
                let alpha = if d.fog.is_some() {
                    0.5 * (1. - p.age * (1. / p.life))
                } else if d.base.flags & 2 != 0 {
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
                    fade_distance: d.fog.as_ref().map(|c| c.fade_distance),
                }
            })
            .collect()
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
        let scene = "[Environment]\nCubeFile=synthetic.cub\n[Lights]\nNumberOfLights=1\n[Light1]\nType=Ambient\nColorRGB=1,2,3\n[Fog]\nColorRGB=0,0,0\n[StaticModels]\nNumberOfStaticModels=1\n[Model1]\nSLT=prop.slt\nPosition=10,20,30\nLookVector=1,0,0\n";
        let common = "[TerrainZone2\nGenerateTireTrack=T]\n[ParticleEmitters]\nNumberOfEmitters=1\n[ParticleEmitter1]\nTypeID=8\nAttachToSoultree=prop.slt\nOffsetFromSoultree=0,5,0\nEmitterFilename=smoke.emt\nNumberOfInstances=2\n";
        let mode = "[SceneInfo]\nDefaultStartPosition=10,20,30\n[ParticleEmitters]\nNumberOfEmitters=1\n[ParticleEmitter1]\nTypeID=2\nPosition=9,8,7\nEmitterFilename=spray.emt\nStartOn=F\n";
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
    fn persistent_volume_initializes_off_and_stays_bounded_after_lifetime() {
        let source = Source {
            scene: "[StaticModels]\nNumberOfStaticModels=0\n".into(),
            common: "[ParticleEmitters]\nNumberOfEmitters=1\n[ParticleEmitter1]\nTypeID=12\nPosition=0,0,0\nStartOn=F\nEmitterFilename=synthetic.emt\n".into(),
            mode: None,
            prt: "[General Info]\nTextureMap=synthetic.tga\nNumFrames=1\nNumGroups=1\n[Frame0]\nUVLeft=0\nUVTop=0\nUVRight=1\nUVBottom=1\n[Group0]\nFirstFrame=0\nLastFrame=0\n".into(),
            emts: BTreeMap::from([("synthetic.emt".into(), "[General Info]\nParticleLife=100\nInitialSize=1\nGrowth=1\nEmitRate=500\nAgeAtBirth=0.5\nVelocity=0,0,0\nParticlesInVolume=700\nMinSize=2\nMaxSize=3\nDistanceToFade=10\nOscillationSpeed=-1\nMinRange=-1,-1,-1\nMaxRange=1,1,1\n[Properties]\nGrows=T\n".into())]),
        };
        let (mut effects, metadata) = Effects::from_source(&source).unwrap();
        assert_eq!(metadata.active, 1);
        assert_eq!(effects.particles.len(), CAPACITY);
        assert_eq!(effects.frame.particles.len(), CAPACITY);
        let initial = effects
            .particles
            .iter()
            .map(|p| (p.position, p.size, p.age))
            .collect::<Vec<_>>();
        for _ in 0..600 {
            effects.step(1. / 60.).unwrap();
        }
        assert_eq!(
            effects
                .particles
                .iter()
                .map(|p| (p.position, p.size, p.age))
                .collect::<Vec<_>>(),
            initial
        );
        assert_eq!(effects.frame.total_emitted, CAPACITY as u64);
        assert_eq!(effects.frame.emitted, 0);
        assert_eq!(effects.frame.dropped, 100);
        assert!(
            effects
                .frame
                .particles
                .iter()
                .all(|p| p.fade_distance == Some(10.) && p.alpha == 0.25)
        );
    }
    #[test]
    fn timer_switches_keep_strict_native_boundary_and_reads_do_not_advance() {
        let mut timer = Timer {
            on: true,
            remaining: 0.25,
            ..Timer::default()
        };
        let mut random = OriginalRandom::with_seed(1);
        assert!(timer.active(3, [0.25; 2], [0.25; 2], &mut random, 0.125));
        assert!(!timer.active(3, [0.25; 2], [0.25; 2], &mut random, 0.125));
        assert_eq!(timer.remaining, 0.25);
        assert!(!timer.active(3, [0.25; 2], [0.25; 2], &mut random, 0.125));
        assert!(timer.active(3, [0.25; 2], [0.25; 2], &mut random, 0.125));
        let mut once = Timer {
            on: true,
            emitted: true,
            ..Timer::default()
        };
        assert!(!once.active(1, [0.; 2], [0.; 2], &mut random, 1. / 60.));
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
            #[serde(default)]
            smoke: Option<Smoke>,
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
                group: if c.kind == 0 { [152, 167] } else { [128, 151] },
                frames_per_set: c.fps,
                mirror: [true, true],
                ground: None,
                restitution: 1.,
                spread_velocity: true,
                lit: false,
                additive: false,
                emit_mode: 0,
                smoke: c.smoke,
                stream: None,
                fog: None,
                on_range: [0.; 2],
                off_range: [0.; 2],
                delay: 0.,
            };
            let runtime = Runtime {
                placement: Placement {
                    filename: String::new(),
                    type_id: c.kind,
                    position: c.position,
                    start_on: true,
                },
                definition,
                oscillator: fog::Oscillator::default(),
                phase: c.phase,
                first: false,
                frame: if c.kind == 0 { 152 } else { 128 },
                timer: Timer {
                    on: true,
                    ..Timer::default()
                },
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
                gpu_total: 0,
                gpu_initialized: false,
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
    #[test]
    #[ignore = "requires private original timer instruction cases"]
    fn private_original_timer_cases() {
        #[derive(Deserialize)]
        struct Case {
            mode: usize,
            on: bool,
            emitted: bool,
            remaining: f32,
            delay: f32,
            dt: f32,
            on_range: [f32; 2],
            off_range: [f32; 2],
            seed: u64,
            active: bool,
            out_on: bool,
            out_remaining: u32,
            out_delay: u32,
        }
        #[derive(Deserialize)]
        struct Report {
            cases: Vec<Case>,
        }
        let report: Report = serde_json::from_slice(
            &std::fs::read(std::env::var("ATV_AMBIENT_TIMER_CASES").unwrap()).unwrap(),
        )
        .unwrap();
        let count = report.cases.len();
        for (index, c) in report.cases.into_iter().enumerate() {
            let mut timer = Timer {
                remaining: c.remaining,
                delay: c.delay,
                on: c.on,
                emitted: c.emitted,
            };
            let active = timer.active(
                c.mode,
                c.on_range,
                c.off_range,
                &mut OriginalRandom::with_seed(c.seed),
                c.dt,
            );
            assert_eq!(
                (
                    active,
                    timer.on,
                    timer.remaining.to_bits(),
                    timer.delay.to_bits()
                ),
                (c.active, c.out_on, c.out_remaining, c.out_delay),
                "timer case{index}"
            );
        }
        println!("native ambient timer comparisons: {}", count * 4);
    }
}
