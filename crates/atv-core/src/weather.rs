//! Exact-build Weather metadata and selection (00208cc0,00209dd0,00214d20).
//! SCN's legacy [Rain] is a different declaration. Common/mode TOYs supply Weather.
use crate::{Result, physics_data::PhysicsData};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Definition {
    pub chance: f32,
    pub model: String,
    pub kind: String,
    pub ring_emitters: bool,
    pub number_of_rings: usize,
    pub ring_height: f32,
    pub ring_initial_radius: f32,
    pub ring_delta_radius: f32,
    pub ring_initial_divisions: usize,
    /// `NumParticleEmitters` (camera-relative WeatherEmitter list).
    #[serde(default)]
    pub particle_emitters: usize,
    /// `MaxLensDrops`/`MaxWorldDrops`, read only by ScreenRain (defaults 20/100).
    #[serde(default)]
    pub max_lens_drops: usize,
    #[serde(default)]
    pub max_world_drops: usize,
}
impl Definition {
    /// `00209dd0` creates ScreenRain only without an emitter list or ring
    /// flag and only for the case-insensitive SltName `screenrain`. No
    /// shipped USA v2.00 resource contains that name (all 742 disc files,
    /// every archive member), so original lens/world drops never appear.
    pub fn selects_screen_rain(&self) -> bool {
        self.particle_emitters == 0
            && !self.ring_emitters
            && self.model.eq_ignore_ascii_case("screenrain")
    }
}
#[derive(Debug, Serialize)]
pub struct Selection {
    pub definition: Option<Definition>,
    pub active: bool,
    pub removed_model: Option<String>,
    pub sample: f32,
    pub effective_chance: f32,
    pub native_player_count: usize,
    /// Active Weather that constructs the original ScreenRain object.
    pub screen_rain: bool,
    pub scope: &'static str,
}
impl Definition {
    pub fn parse(common: &[u8], mode: &[u8]) -> Result<Option<Self>> {
        fn weather_entries(bytes: &[u8]) -> Result<Vec<crate::physics_data::Entry>> {
            if bytes.len() > 1024 * 1024 || bytes.contains(&0) {
                return Err("invalid Weather document extent".into());
            }
            let source = std::str::from_utf8(bytes).map_err(|_| "Weather document is not UTF-8")?;
            let mut active = false;
            let mut lines = vec![];
            for raw in source.lines() {
                let line = raw.split(';').next().unwrap_or("").trim();
                if line.starts_with('[') {
                    active = line.eq_ignore_ascii_case("[Weather]");
                    if active {
                        lines.push("[Weather]");
                    }
                    continue;
                }
                if active && line.contains('=') {
                    lines.push(line);
                }
            }
            if lines.is_empty() {
                Ok(vec![])
            } else {
                Ok(PhysicsData::parse(lines.join("\n").as_bytes())?.entries)
            }
        }
        let mut entries = weather_entries(common)?;
        if !mode.is_empty() && common != mode {
            entries.extend(weather_entries(mode)?);
        }
        let data = PhysicsData { entries };
        if !data.entries.iter().any(|entry| entry.section == "Weather") {
            return Ok(None);
        }
        let value = |key: &str| {
            data.entries
                .iter()
                .find(|entry| entry.section == "Weather" && entry.key.eq_ignore_ascii_case(key))
                .map(|entry| entry.value.as_str())
        };
        let number = |key: &str, default: f32| -> Result<f32> {
            let v = value(key)
                .map(|v| {
                    v.parse::<f32>()
                        .map_err(|_| format!("invalid Weather {key}"))
                })
                .transpose()?
                .unwrap_or(default);
            if !v.is_finite() {
                return Err(format!("nonfinite Weather {key}"));
            }
            Ok(v)
        };
        let count = |key: &str, default: f32| -> Result<usize> {
            let v = number(key, default)?;
            if v.fract() != 0. || !(0. ..=64.).contains(&v) {
                return Err(format!("unsupported Weather {key}"));
            }
            Ok(v as usize)
        };
        let model = value("SltName")
            .unwrap_or("NONE")
            .trim_matches('"')
            .to_owned();
        if model.contains(['/', '\\', ':']) || model.contains("..") {
            return Err("invalid Weather model".into());
        }
        let kind = value("WeatherType").unwrap_or("NONE").to_ascii_lowercase();
        let result = Self {
            chance: number("ChanceOfWeather", 0.)?,
            model,
            kind,
            ring_emitters: number("UseRingEmitters", 0.)? != 0.,
            number_of_rings: count("NumberOfRings", 4.)?,
            ring_height: number("RingHeight", 60.)?,
            ring_initial_radius: number("RingInitialRadius", 50.)?,
            ring_delta_radius: number("RingDeltaRadius", 50.)?,
            ring_initial_divisions: count("RingInitialDivisions", 4.)?,
            particle_emitters: if number("UseRingEmitters", 0.)? != 0. {
                0
            } else {
                count("NumParticleEmitters", 0.)?
            },
            max_lens_drops: count("MaxLensDrops", 20.)?,
            max_world_drops: number("MaxWorldDrops", 100.).and_then(|v| {
                if v.fract() != 0. || !(0. ..=1000.).contains(&v) {
                    Err("unsupported Weather MaxWorldDrops".into())
                } else {
                    Ok(v as usize)
                }
            })?,
        };
        if !(0. ..=1.).contains(&result.chance) {
            return Err("Weather chance outside probability range".into());
        }
        Ok(Some(result))
    }
}
pub fn select(
    definition: Option<Definition>,
    random: u32,
    player_count: usize,
) -> Result<Selection> {
    if random > 0x7fff_ffff || !(1..=4).contains(&player_count) {
        return Err("invalid Weather selection input".into());
    }
    let sample = random as f32 * f32::from_bits(0x3000_0000);
    let chance = definition
        .as_ref()
        .map_or(0., |d| if player_count > 1 { 0. } else { d.chance });
    //Native uses chance < random, so the equality endpoint remains accepted.
    let active = definition.is_some() && chance >= sample;
    let removed_model = definition
        .as_ref()
        .filter(|d| !active && !d.model.eq_ignore_ascii_case("NONE"))
        .map(|d| d.model.trim_end_matches(".slt").to_owned());
    let screen_rain = active
        && definition
            .as_ref()
            .is_some_and(Definition::selects_screen_rain);
    Ok(Selection {
        definition,
        active,
        removed_model,
        sample,
        effective_chance: chance,
        native_player_count: player_count,
        screen_rain,
        scope: "source Weather selection; per-load browser RNG is independent of unrecovered native game-wide RNG; Snow rendering is configured independently; ScreenRain requires SltName screenrain, which no shipped resource declares",
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_and_mode_weather_use_first_keys_and_ignore_legacy_rain() {
        let common=b"[General]\nEnabled=1\n[Weather]\nChanceOfWeather=.8\nSltName=test.slt\nWeatherType=rain\n";
        let mode = b"[Weather]\nChanceOfWeather=1\nSltName=other.slt\n";
        let d = Definition::parse(common, mode).unwrap().unwrap();
        assert_eq!(d.chance, 0.8);
        assert_eq!(d.model, "test.slt");
        assert!(
            Definition::parse(b"[Rain]\nChanceOfRain=1\n", b"")
                .unwrap()
                .is_none()
        );
        assert!(!select(Some(d.clone()), 0x7fff_ffff, 1).unwrap().active);
        assert!(select(Some(d.clone()), 0, 1).unwrap().active);
        assert!(!select(Some(d.clone()), 1, 2).unwrap().active);
        assert!(select(Some(d), 0, 2).unwrap().active);
    }
    #[test]
    fn screen_rain_needs_its_exact_model_name_without_emitters() {
        // Shipped rain declarations name placed rain geometry: no lens drops.
        let placed = Definition::parse(
            b"[Weather]\nChanceOfWeather=1\nSltName=synthetic_rain.slt\nWeatherType=rain\n",
            b"",
        )
        .unwrap()
        .unwrap();
        assert!(!placed.selects_screen_rain());
        assert!(select(Some(placed), 0, 1).unwrap().active);
        let lens = Definition::parse(
            b"[Weather]\nChanceOfWeather=1\nSltName=ScreenRain\nWeatherType=RAIN\n",
            b"",
        )
        .unwrap()
        .unwrap();
        assert_eq!((lens.max_lens_drops, lens.max_world_drops), (20, 100));
        assert!(lens.selects_screen_rain());
        assert!(select(Some(lens.clone()), 0, 1).unwrap().screen_rain);
        assert!(
            !select(Some(lens), 1, 2).unwrap().screen_rain,
            "multiplayer chance zero"
        );
        let emitters = Definition::parse(
            b"[Weather]\nChanceOfWeather=1\nSltName=screenrain\nNumParticleEmitters=1\n",
            b"",
        )
        .unwrap()
        .unwrap();
        assert!(!emitters.selects_screen_rain());
        // The legacy SCN [Rain] lens/world counts are never read.
        assert!(
            Definition::parse(b"[Rain]\nChanceOfRain=1\nMaxLensDrops=10\n", b"")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn ring_constructor_defaults_and_native_equality_survive() {
        let d = Definition::parse(
            b"[Weather]\nChanceOfWeather=.5\nWeatherType=SNOW\nUseRingEmitters=1\n",
            b"",
        )
        .unwrap()
        .unwrap();
        assert_eq!(d.number_of_rings, 4);
        assert_eq!(d.ring_height, 60.);
        assert_eq!(d.kind, "snow");
        assert!(select(Some(d.clone()), 0x4000_0000, 1).unwrap().active);
        assert!(!select(Some(d), 0x4000_0080, 1).unwrap().active);
    }
}

/// Camera-relative SnowEmitter birth contract (00209dd0/00209310/001a1ba0).
/// Rendering uses the separately recovered mode2 GPU packet contract.
#[derive(Debug, Clone, Serialize)]
pub struct SnowConfig {
    pub offsets: Vec<[f32; 3]>,
    pub life: f32,
    pub size: f32,
    pub rate: f32,
    pub minimum: f32,
    pub spread: f32,
    pub velocity: [f32; 3],
    pub min_range: [f32; 3],
    pub max_range: [f32; 3],
    pub color: [f32; 3],
    pub min_alpha: f32,
    pub max_alpha: f32,
}
impl SnowConfig {
    pub fn parse(definition: &Definition, bytes: &[u8]) -> Result<Self> {
        if definition.kind != "snow" || !definition.ring_emitters {
            return Err("unsupported Snow selection".into());
        }
        if bytes.len() > 1024 * 1024 {
            return Err("Snow emitter exceeds extent".into());
        }
        let source = std::str::from_utf8(bytes).map_err(|_| "Snow emitter is not UTF-8")?;
        let source = source
            .lines()
            .map(|s| s.split(';').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let data = PhysicsData::parse(source.as_bytes())?;
        let scalar = |key| data.first_scalar("General Info", key);
        let mut offsets = vec![];
        let mut radius = definition.ring_initial_radius;
        let mut divisions = definition.ring_initial_divisions;
        for _ in 0..definition.number_of_rings {
            if divisions == 0 || offsets.len() + divisions > 256 {
                return Err("unsupported Snow ring extent".into());
            }
            let delta = std::f32::consts::TAU / divisions as f32;
            for index in 0..divisions {
                let angle = index as f32 * delta;
                //Native00369120 is cosine;003693c0 is sine. Host trig remains provisional.
                offsets.push([
                    radius * angle.cos(),
                    definition.ring_height,
                    radius * angle.sin(),
                ]);
            }
            divisions *= 2;
            radius += definition.ring_delta_radius;
        }
        let result = Self {
            offsets,
            life: scalar("ParticleLife")?,
            size: scalar("InitialSize")?,
            rate: scalar("EmitRate")?,
            minimum: scalar("MinParticlesPerFrame")?,
            spread: scalar("EmitterSpread")?,
            velocity: data.vector3("General Info", "Velocity")?,
            min_range: data.vector3("General Info", "MinRange")?,
            max_range: data.vector3("General Info", "MaxRange")?,
            color: data.vector3("General Info", "ParticleColor")?,
            min_alpha: scalar("MinAlpha")?,
            max_alpha: scalar("MaxAlpha")?,
        };
        if !(0.0..=60_000.0).contains(&result.life)
            || result.life == 0.0
            || !(0.0..=64.0).contains(&result.minimum)
            || !(0.0..=1000.0).contains(&result.rate)
            || !(0.0..=100.0).contains(&result.size)
            || ![result.spread, result.min_alpha, result.max_alpha]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("unsupported Snow emitter values".into());
        }
        Ok(result)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct SnowBirth {
    pub id: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub time: f32,
    pub life: f32,
    pub size: f32,
    pub mirror: u32,
    pub half: usize,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct SnowFrame {
    pub time: f32,
    pub births: Vec<SnowBirth>,
    pub active_emitters: usize,
    pub total_emitted: u64,
}
pub struct Snow {
    config: SnowConfig,
    time: f32,
    sequence: u64,
    table: [f32; 600],
    cursor: usize,
    frame: SnowFrame,
}
impl Snow {
    pub fn new(config: SnowConfig) -> Self {
        let mut random = crate::engine_audio::OriginalRandom::with_seed(1);
        let table = std::array::from_fn(|_| random.next_u31() as f32 * f32::from_bits(0x30000000));
        Self {
            config,
            time: 0.,
            sequence: 0,
            table,
            cursor: 0,
            frame: SnowFrame::default(),
        }
    }
    pub fn with_table(config: SnowConfig, table: [f32; 600]) -> Result<Self> {
        if table
            .iter()
            .any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
        {
            return Err("invalid Snow random table".into());
        }
        let mut snow = Self::new(config);
        snow.table = table;
        Ok(snow)
    }
    pub fn config(&self) -> &SnowConfig {
        &self.config
    }
    pub fn step(&mut self, position: [f32; 3], forward: [f32; 3]) -> Result<SnowFrame> {
        if position
            .iter()
            .chain(forward.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite Snow camera".into());
        }
        self.time += 1. / 60.;
        self.frame = SnowFrame {
            time: self.time,
            ..SnowFrame::default()
        };
        let count = self
            .config
            .minimum
            .max(self.config.rate * (1. / 60.))
            .ceil() as usize;
        for offset in &self.config.offsets {
            //Normalization preserves sign; equality is enabled in00209310.
            if offset.iter().zip(forward).map(|(a, b)| a * b).sum::<f32>() < 0. {
                continue;
            }
            self.frame.active_emitters += 1;
            let mut half = 0;

            for _ in 0..count {
                if self.cursor > 595 {
                    self.cursor = 0;
                }
                let [rx, ry, rz] = [
                    self.table[self.cursor],
                    self.table[self.cursor + 1],
                    self.table[self.cursor + 2],
                ];
                self.cursor += 3;
                let unquantized: [f32; 3] = std::array::from_fn(|i| {
                    self.config.min_range[i]
                        + (self.config.max_range[i] - self.config.min_range[i]) * [rx, ry, rz][i]
                        + self.config.velocity[i]
                });
                let position = std::array::from_fn(|i| {
                    position[i]
                        + offset[i]
                        + if i == 1 {
                            0.
                        } else {
                            unquantized[i] * self.config.spread
                        }
                });
                let velocity = std::array::from_fn(|i| {
                    let unit = if i == 1 { 50. } else { 100. };
                    let factor = 32767. / unit;
                    ((unquantized[i] * factor) as i16) as f32 / factor
                });
                let life = (self.config.life * ry) * 0.001;
                self.sequence += 1;
                half = usize::from(rz <= 0.5);
                //A zero source lifetime produces no visible particle, but consumes its RNG/id.
                if life > 0. {
                    self.frame.births.push(SnowBirth {
                        id: self.sequence,
                        position,
                        velocity,
                        time: self.time,
                        life,
                        size: ((self.config.size * 655.34) as u16) as f32
                            * f32::from_bits(0x3ac8018f),
                        mirror: u32::from(rx > 0.5) | (u32::from(ry > 0.5) << 1),
                        half,
                    });
                }
            }
            //The emitter writes one UV rectangle to the GPU bank after its birth loop.
            for birth in &mut self.frame.births {
                birth.half = half;
            }
        }
        self.frame.total_emitted = self.sequence;
        Ok(self.frame.clone())
    }
}

#[cfg(test)]
mod snow_tests {
    use super::*;
    fn config() -> SnowConfig {
        SnowConfig {
            offsets: vec![[50., 65., 0.], [-50., 65., 0.]],
            life: 7000.,
            size: 5.,
            rate: 2.,
            minimum: 0.,
            spread: 1.8,
            velocity: [-10., -20., 0.],
            min_range: [-15., -8., -15.],
            max_range: [10., 0., 10.],
            color: [120.; 3],
            min_alpha: 31.,
            max_alpha: 80.,
        }
    }
    #[test]
    fn camera_front_half_emits_ceil_rate_once_per_step_and_changes_with_view() {
        let mut snow =
            Snow::with_table(config(), std::array::from_fn(|i| [0.25, 0.5, 0.75][i % 3])).unwrap();
        let frame = snow.step([0.; 3], [1., 0., 0.]).unwrap();
        assert_eq!(frame.active_emitters, 1);
        assert_eq!(frame.births.len(), 1);
        assert_eq!(frame.births[0].position, [16.25, 65., 6.75]);
        assert_eq!(frame.births[0].life, 3.5000002);
        assert_eq!(frame.births[0].mirror, 0);
        assert_eq!(frame.births[0].half, 0);
        //Reading configuration does not advance the clock or emit; only step does.
        assert_eq!(snow.config().offsets.len(), 2);
        let second = snow.step([100., 10., 0.], [-1., 0., 0.]).unwrap();
        assert_eq!(second.total_emitted, 2);
        assert_eq!(second.births[0].position, [16.25, 75., 6.75]);
        assert!(second.time > frame.time);
    }
    #[test]
    fn zero_lifetime_consumes_random_slot_without_nonfinite_particle() {
        let mut snow = Snow::with_table(config(), [0.; 600]).unwrap();
        let frame = snow.step([0.; 3], [1., 0., 0.]).unwrap();
        assert_eq!(frame.total_emitted, 1);
        assert!(frame.births.is_empty());
        assert!(snow.step([f32::NAN, 0., 0.], [1., 0., 0.]).is_err());
        assert!(Snow::with_table(config(), [1.; 600]).is_err());
    }
}
