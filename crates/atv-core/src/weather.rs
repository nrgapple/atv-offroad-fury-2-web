//! Exact-build Weather metadata and selection (00208cc0,00209dd0,00214d20).
//! SCN's legacy [Rain] is a different declaration. Common/mode TOYs supply Weather.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
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
}
#[derive(Debug, Serialize)]
pub struct Selection {
    pub definition: Option<Definition>,
    pub active: bool,
    pub removed_model: Option<String>,
    pub sample: f32,
    pub effective_chance: f32,
    pub native_player_count: usize,
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
    Ok(Selection {
        definition,
        active,
        removed_model,
        sample,
        effective_chance: chance,
        native_player_count: player_count,
        scope: "source Weather selection; per-load browser RNG is independent of unrecovered native game-wide RNG; snow rings/ScreenRain callbacks pending",
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
