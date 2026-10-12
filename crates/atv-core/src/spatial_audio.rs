//! Original body-attached train sounds and stereo command arithmetic.
//! Web Audio transports original PCM; it does not reproduce SPU2 resampling.
use crate::{Result, physics_data::PhysicsData, wheel::Vec3};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Definition {
    pub car: usize,
    pub sample: String,
    pub min_distance: f32,
    pub max_distance: f32,
    pub looping: bool,
    pub start_disabled: bool,
}

/// Match TOY animation slots to the same procedural route family as train.rs.
pub fn train_definitions(bytes: &[u8]) -> Result<Vec<Definition>> {
    let data = PhysicsData::parse(bytes)?;
    let get = |section: &str, key: &str| {
        data.entries
            .iter()
            .find(|e| e.section.eq_ignore_ascii_case(section) && e.key.eq_ignore_ascii_case(key))
            .map(|e| e.value.as_str())
    };
    let count = get("Animations", "NumberOfAnimations")
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|_| "invalid audio animation count")?;
    if count > 4096 {
        return Err("audio animation count exceeds limit".into());
    }
    let boolean = |value: &str| -> Result<bool> {
        match value.to_ascii_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "on" | "1" => Ok(true),
            "false" | "f" | "no" | "n" | "off" | "0" => Ok(false),
            _ => Err("invalid train audio flag".into()),
        }
    };
    let mut result = Vec::new();
    let mut car = 0;
    for index in 1..=count {
        let section = format!("Animation{index}");
        if !get(&section, "AnimationType").is_some_and(|v| v.eq_ignore_ascii_case("Procedural"))
            || !get(&section, "VUE").is_some_and(|v| {
                v.replace('\\', "/")
                    .rsplit('/')
                    .next()
                    .is_some_and(|v| v.eq_ignore_ascii_case("locomotive.vue"))
            })
        {
            continue;
        }
        let sounds = get(&section, "NumberOfLoopingSounds")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| "invalid train loop count")?;
        if sounds > 32 {
            return Err("train sound count exceeds limit".into());
        }
        for sound in 1..=sounds {
            let section = format!("Animation{index}LoopSound{sound}");
            if !get(&section, "AttachToObject").is_some_and(|v| v.eq_ignore_ascii_case("Body")) {
                return Err("unsupported train sound attachment".into());
            }
            let scalar = |key: &str| -> Result<f32> {
                get(&section, key)
                    .ok_or("train sound range missing")?
                    .parse::<f32>()
                    .map_err(|_| "invalid train sound range".into())
            };
            let min_distance = scalar("MinDistance")?;
            let max_distance = scalar("MaxDistance")?;
            if !min_distance.is_finite()
                || !max_distance.is_finite()
                || min_distance < 0.
                || max_distance <= min_distance
            {
                return Err("invalid train sound interval".into());
            }
            let sample = get(&section, "SoundResourceFile").ok_or("train sound sample missing")?;
            if sample.is_empty() || sample.contains("..") || sample.len() > 260 {
                return Err("invalid train sound resource".into());
            }
            result.push(Definition {
                car,
                sample: sample.to_owned(),
                min_distance,
                max_distance,
                looping: boolean(get(&section, "Looping").unwrap_or("FALSE"))?,
                start_disabled: boolean(get(&section, "StartDisabled").unwrap_or("FALSE"))?,
            });
        }
        car += 1;
    }
    if car != crate::train::definitions(bytes)?.len() {
        return Err("train audio/body slot mismatch".into());
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub velocity: Vec3,
}
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Source {
    pub position: Vec3,
    pub velocity: Vec3,
    pub min_distance: f32,
    pub max_distance: f32,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CameraPose {
    pub position: Vec3,
    pub target: Vec3,
}
pub fn camera_listener(current: &CameraPose, previous: &CameraPose, dt: f32) -> Result<Listener> {
    if !dt.is_finite() || dt <= 0. || dt > 0.1 {
        return Err("invalid audio listener timestep".into());
    }
    let forward: Vec3 = std::array::from_fn(|i| current.target[i] - current.position[i]);
    let squared = (forward[0] * forward[0] + forward[1] * forward[1]) + forward[2] * forward[2];
    if squared <= 0. || !squared.is_finite() {
        return Err("invalid audio camera direction".into());
    }
    let reciprocal = 1. / squared.sqrt();
    Ok(Listener {
        position: current.position,
        forward: forward.map(|v| v * reciprocal),
        velocity: std::array::from_fn(|i| (current.position[i] - previous.position[i]) * (1. / dt)),
    })
}
#[derive(Debug, Deserialize)]
pub struct SceneInput {
    pub dt: f32,
    pub camera: CameraPose,
    pub previous_camera: CameraPose,
    pub cars: Vec<[f32; 16]>,
    pub previous_cars: Vec<[f32; 16]>,
    pub definitions: Vec<Definition>,
}
pub fn scene_commands(input: SceneInput) -> Result<Vec<Command>> {
    if !input.dt.is_finite()
        || input.dt <= 0.
        || input.dt > 0.1
        || input.cars.len() > 4096
        || input.cars.len() != input.previous_cars.len()
        || input.definitions.len() > 64
    {
        return Err("invalid train audio frame history".into());
    }
    let listener = camera_listener(&input.camera, &input.previous_camera, input.dt)?;
    input
        .definitions
        .iter()
        .map(|definition| {
            let current = input
                .cars
                .get(definition.car)
                .ok_or("train audio car missing")?;
            let previous = input
                .previous_cars
                .get(definition.car)
                .ok_or("train audio history missing")?;
            let source = Source {
                position: std::array::from_fn(|i| current[12 + i]),
                velocity: std::array::from_fn(|i| {
                    (current[12 + i] - previous[12 + i]) * (1. / input.dt)
                }),
                min_distance: definition.min_distance,
                max_distance: definition.max_distance,
            };
            command(source, &[listener])
        })
        .collect()
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Command {
    pub active: bool,
    pub distance: f32,
    pub volume: i32,
    pub pan: i32,
    pub doppler: i32,
    pub left: i32,
    pub right: i32,
    pub pitch: i32,
}

//001aab50 builds30001 entries;001ab6b0 interpolates29999 intervals.
//The transcendental constructor uses host f64 acos converted to f32. Source
//table interpolation/arithmetic is recovered; PS2 libm equivalence is unproven.
fn heading(vector: Vec3) -> f32 {
    let squared = vector[2] * vector[2] + vector[0] * vector[0];
    if squared == 0. {
        return 0.;
    } //Finite browser guard for coincident sources.
    let cosine = vector[0] * (1. / squared.sqrt());
    let table = ACOS.get_or_init(|| {
        let step = 2. / 29999_f32;
        let mut values = (0..30000)
            .map(|i| {
                let x = ((i as f32) * step - 1.).min(1.);
                f64::from(x).acos() as f32
            })
            .collect::<Vec<_>>();
        values.push(values[29999]);
        values
    });
    let index = (cosine + 1.) * (29999_f32 * 0.5);
    let integer = (index as usize).min(29999);
    let fraction = index - integer as f32;
    let angle = (1. - fraction) * table[integer] + fraction * table[integer + 1];
    if vector[2] < 0. { -angle } else { angle }
}
static ACOS: OnceLock<Vec<f32>> = OnceLock::new();

///00172e70 heading/attenuation/doppler helper. The normalized vector really is
///listener-forward minus relative-source-position; preserve this unusual input.
pub fn parameters(
    distance: f32,
    min: f32,
    max: f32,
    forward: Vec3,
    relative: Vec3,
    listener_velocity: Vec3,
    source_velocity: Vec3,
) -> (bool, i32, i32, i32) {
    if distance >= max {
        return (false, 0, 0, 0);
    }
    let volume = if distance <= min {
        100
    } else {
        100 - (((distance - min) / (max - min)) * 100.) as i32
    };
    let degrees = |v| (heading(v) * f32::from_bits(0x42652ee1) + 0.5) as i32;
    let mut pan = degrees(forward) - degrees(relative);
    if pan < 0 {
        pan += 360;
    }
    let direction: Vec3 = std::array::from_fn(|i| forward[i] - relative[i]);
    let squared =
        (direction[0] * direction[0] + direction[1] * direction[1]) + direction[2] * direction[2];
    let reciprocal = if squared == 0. {
        f32::MAX
    } else {
        1. / squared.sqrt()
    };
    let normalized: Vec3 = direction.map(|v| v * reciprocal);
    let doppler = ((normalized[0] * (source_velocity[0] - listener_velocity[0])
        + normalized[1] * 0.)
        + normalized[2] * (source_velocity[2] - listener_velocity[2])
        + 0.5) as i32;
    (true, volume, pan, doppler)
}

///002f4500 ordinary stereo integer channel gains. Surround mode is separate.
pub fn stereo(volume: i32, pan: i32) -> (i32, i32) {
    if pan == 0 || pan == 360 {
        (volume, volume)
    } else if pan <= 90 {
        (volume * (90 - pan) / 90, volume)
    } else if pan < 180 {
        (volume * (pan - 91) / 89, volume)
    } else if pan == 180 {
        (volume, volume)
    } else if pan < 270 {
        (volume, volume * (269 - pan) / 89)
    } else {
        (volume, volume * (pan - 270) / 90)
    }
}
///0014ed40 uses the constructor's0.25 DopplerFactor,1125 divisor and0.01 gate.
pub fn pitch(doppler: i32) -> i32 {
    let mut change = 0.25 * ((doppler as f32) / 1125.);
    if change.abs() < 0.01 {
        change = 0.;
    }
    (4096. * (1. + change)) as i32
}
pub fn command(source: Source, listeners: &[Listener]) -> Result<Command> {
    if listeners.is_empty()
        || listeners.len() > 8
        || source
            .position
            .iter()
            .chain(source.velocity.iter())
            .chain([&source.min_distance, &source.max_distance])
            .any(|v| !v.is_finite())
        || source.min_distance < 0.
        || source.max_distance <= source.min_distance
    {
        return Err("invalid spatial audio source/listeners".into());
    }
    let mut nearest = f32::MAX;
    let mut chosen = &listeners[0];
    for listener in listeners {
        if listener
            .position
            .iter()
            .chain(listener.forward.iter())
            .chain(listener.velocity.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite spatial listener".into());
        }
        let delta: Vec3 = std::array::from_fn(|i| source.position[i] - listener.position[i]);
        let distance = ((delta[2] * delta[2] + delta[1] * delta[1]) + delta[0] * delta[0]).sqrt();
        if distance < nearest {
            nearest = distance;
            chosen = listener;
        }
    }
    let relative: Vec3 = std::array::from_fn(|i| source.position[i] - chosen.position[i]);
    let (active, volume, pan, doppler) = parameters(
        nearest,
        source.min_distance,
        source.max_distance,
        chosen.forward,
        relative,
        chosen.velocity,
        source.velocity,
    );
    let (left, right) = stereo(volume, pan);
    let pitch = pitch(doppler);
    Ok(Command {
        active,
        distance: nearest,
        volume,
        pan,
        doppler,
        left,
        right,
        pitch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_distance_boundaries_and_nearest_listener() {
        let source = Source {
            position: [250., 0., 0.],
            velocity: [0.; 3],
            min_distance: 150.,
            max_distance: 350.,
        };
        let a = Listener {
            position: [0.; 3],
            forward: [0., 0., 1.],
            velocity: [0.; 3],
        };
        assert_eq!(command(source, &[a]).unwrap().volume, 50);
        assert_eq!(
            command(
                source,
                &[
                    a,
                    Listener {
                        position: [200., 0., 0.],
                        ..a
                    }
                ]
            )
            .unwrap()
            .volume,
            100
        );
        assert!(
            !command(
                Source {
                    position: [350., 0., 0.],
                    ..source
                },
                &[a]
            )
            .unwrap()
            .active
        );
        assert_eq!(stereo(100, 90), (0, 100));
        assert_eq!(stereo(100, 270), (100, 0));
        assert_eq!(pitch(44), 4096);
        assert!(pitch(45) > 4096);
        assert_eq!(pitch(-44), 4096);
        assert!(pitch(-45) < 4096);
    }
    #[test]
    fn empty_and_nonfinite_are_rejected() {
        let source = Source {
            position: [0.; 3],
            velocity: [0.; 3],
            min_distance: 0.,
            max_distance: 1.,
        };
        assert!(command(source, &[]).is_err());
        assert!(
            command(
                source,
                &[Listener {
                    position: [f32::NAN; 3],
                    forward: [0.; 3],
                    velocity: [0.; 3]
                }]
            )
            .is_err()
        );
    }
}
