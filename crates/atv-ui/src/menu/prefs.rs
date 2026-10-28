//! Player preferences under the browser game's keys (graphics.js,
//! render-support.js `RideEffectsSettings`, audio.js, music.js, hud.js,
//! diagnostics.js), so the Rust engine and play.html share one set of choices.
use crate::settings::Settings;
use serde_json::{Value, json};

pub const GRAPHICS_PRESETS: [&str; 4] = ["auto", "original", "enhanced", "high"];
pub const SPEED_RUSH: [&str; 4] = ["off", "low", "normal", "high"];
pub const CRASH_MODES: [&str; 3] = ["original", "crazy", "gorn"];
pub const DIAGNOSTICS: [&str; 3] = ["off", "performance", "debug"];
pub const OVERLAY_GROUPS: [&str; 6] = ["timing", "lap", "score", "info", "speed", "radar"];
/// Settings_Game (PAGE15) "Sound Configuration" values, in the page's order.
pub const SOUND_CONFIGURATIONS: [&str; 4] = ["stereo", "swapped", "dolby", "mono"];

#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub graphics: String,
    /// `detectGraphicsTier` result for Auto.
    pub graphics_detected: String,
    pub vibration: bool,
    pub speed_rush: String,
    pub crash_mode: String,
    pub crash_camera_original: bool,
    pub chase_assist: bool,
    pub flips: bool,
    pub realistic_train: bool,
    pub diagnostics: String,
    pub sound_mode: String,
    pub sfx_volume: f32,
    pub silent: String,
    /// The browser has `navigator.audioSession` (iOS/iPadOS Safari 16.4+), so
    /// Settings → Extras shows the Silent switch row. Set by the host; not saved.
    pub audio_session: bool,
    /// The host can go fullscreen (`document.fullscreenEnabled`; desktop
    /// windows always can). Set by the host; not saved.
    pub fullscreen: bool,
    pub music_enabled: bool,
    pub music_volume: f32,
    pub overlays_off: Vec<String>,
    pub race_position: String,
    /// Settings → Alpha: the experiments switched on (`atv-alpha`).
    pub alpha: Vec<String>,
    /// Game Settings → Measurement: Metric (profile +0x6b0 clear in the
    /// original; `0020f780` then sets the speed factor to 1.08, Kph and
    /// Meters). Off is Standard (U.S. system).
    pub metric: bool,
    /// Game Settings → Ecosystem Collision (settings +0x674). Off lets the
    /// quads and riders pass through vegetation and stops race records.
    pub ecosystem_collision: bool,
    /// Game Settings → Sound Configuration (one of [`SOUND_CONFIGURATIONS`]).
    pub sound_configuration: String,
    /// Game Settings → New Profiles (port option, this device): profiles
    /// created here start with Unlock Everything
    /// (`settings::new_profiles_unlocked`).
    pub new_profiles_unlocked: bool,
}

/// web/alpha.js `ALPHA_FEATURES`: authored experiments, each off by default.
pub const ALPHA_FEATURES: [(&str, &str); 7] = [
    ("air-slowmo", "Air slow-mo"),
    ("big-air-camera", "Big-air camera"),
    ("music-muffle", "Speed & air muffle"),
    ("air-blur", "Air dark blur"),
    ("landing-burst", "Landing burst"),
    ("heavy-mud", "Heavy mud"),
    ("arcade-flips", "Arcade flips"),
];

fn cycle<'a>(list: &[&'a str], current: &str, direction: i32) -> &'a str {
    let at = list.iter().position(|v| *v == current).unwrap_or(0) as i32;
    let n = list.len() as i32;
    list[((at + direction.signum().max(-1) + n) % n) as usize]
}

impl Preferences {
    /// `touch_first` stands in for graphics.js device detection (mobile, touch
    /// primary or low-power devices get Enhanced, others High).
    pub fn load(settings: &Settings, touch_first: bool) -> Self {
        let get = |k: &str| settings.get(k);
        let json = |k: &str| get(k).and_then(|v| serde_json::from_str::<Value>(&v).ok());
        let audio = json("atv-audio-v1").unwrap_or(Value::Null);
        let music = json("atv-soundtrack-v1").unwrap_or(Value::Null);
        let one_of = |value: Option<String>, list: &[&str], default: &str| {
            value
                .filter(|v| list.contains(&v.as_str()))
                .unwrap_or_else(|| default.into())
        };
        let crash_mode = match get("atv-crash-mode") {
            Some(mode) if CRASH_MODES.contains(&mode.as_str()) => mode,
            _ if get("atv-funny-crashes").as_deref() == Some("true") => "crazy".into(),
            _ => "original".into(),
        };
        Self {
            graphics: one_of(get("atv-graphics"), &GRAPHICS_PRESETS, "auto"),
            graphics_detected: if touch_first { "enhanced" } else { "high" }.into(),
            vibration: get("atv-vibration").as_deref() != Some("off"),
            speed_rush: one_of(get("atv-speed-rush"), &SPEED_RUSH, "normal"),
            crash_mode,
            crash_camera_original: get("atv-crash-camera").as_deref() == Some("original"),
            chase_assist: get("atv-chase-camera").as_deref() == Some("assisted"),
            flips: get("atv-flips").as_deref() == Some("unleashed"),
            realistic_train: get("atv-train-riding").as_deref() == Some("realistic"),
            diagnostics: one_of(get("atv.diagnostics.v1"), &DIAGNOSTICS, "off"),
            sound_mode: match audio["mode"].as_str() {
                Some("original") => "original".into(),
                _ => "enhanced".into(),
            },
            sfx_volume: audio["sfx"].as_f64().map_or(1., |v| v.clamp(0., 1.) as f32),
            silent: match audio["silent"].as_str() {
                Some("mute") => "mute".into(),
                _ => "play".into(),
            },
            audio_session: false,
            fullscreen: true,
            music_enabled: music["enabled"].as_bool().unwrap_or(true),
            music_volume: music["volume"]
                .as_f64()
                .map_or(0.45, |v| v.clamp(0., 1.) as f32),
            overlays_off: json("atv-race-overlays-off")
                .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|g| OVERLAY_GROUPS.contains(&g.as_str()))
                .collect(),
            race_position: one_of(get("atv-race-position"), &["name", "place", "off"], "name"),
            alpha: json("atv-alpha")
                .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|id| ALPHA_FEATURES.iter().any(|(f, _)| f == id))
                .collect(),
            metric: get("atv-measurement").as_deref() == Some("metric"),
            ecosystem_collision: get("atv-ecosystem-collision").as_deref() != Some("off"),
            sound_configuration: one_of(
                get("atv-sound-configuration"),
                &SOUND_CONFIGURATIONS,
                "stereo",
            ),
            new_profiles_unlocked: crate::settings::new_profiles_unlocked(settings),
        }
    }

    /// The record and speed units the original picks from the speed factor
    /// (`00332810`, `002585c0`, `002e2030`).
    pub fn units(&self) -> atv_core::hall_of_fame::Units {
        if self.metric {
            atv_core::hall_of_fame::Units::Metric
        } else {
            atv_core::hall_of_fame::Units::Standard
        }
    }

    /// Turns Ecosystem Collision on or off (the page asks before off).
    pub fn set_ecosystem_collision(&mut self, on: bool, settings: &mut Settings) {
        self.ecosystem_collision = on;
        settings.set("atv-ecosystem-collision", if on { "on" } else { "off" });
    }

    pub fn alpha(&self, id: &str) -> bool {
        self.alpha.iter().any(|a| a == id)
    }

    pub fn graphics_resolved(&self) -> &str {
        if self.graphics == "auto" {
            &self.graphics_detected
        } else {
            &self.graphics
        }
    }

    fn save_audio(&self, settings: &mut Settings) {
        settings.set(
            "atv-audio-v1",
            &json!({"sfx": self.sfx_volume, "mode": self.sound_mode, "silent": self.silent})
                .to_string(),
        );
    }

    /// music.js keeps its play history in the same record: update in place.
    fn save_music(&self, settings: &mut Settings) {
        let mut record = settings
            .get("atv-soundtrack-v1")
            .and_then(|v| serde_json::from_str::<Value>(&v).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({"v": 3, "history": [], "historyIndex": -1}));
        record["enabled"] = self.music_enabled.into();
        record["volume"] = self.music_volume.into();
        settings.set("atv-soundtrack-v1", &record.to_string());
    }

    /// The Jukebox volume range (0-1, steps of 5%) and its Play/song choices
    /// that switch the soundtrack on (MusicStream setVolume/setEnabled).
    pub fn set_music(&mut self, volume: f32, enabled: bool, settings: &mut Settings) {
        self.music_volume = ((volume.clamp(0., 1.) * 20.).round() / 20.).clamp(0., 1.);
        self.music_enabled = enabled;
        self.save_music(settings);
    }

    /// Applies one settings-row action; returns false if it is not a
    /// preference (frontend.js `run` branches for these rows).
    pub fn apply(
        &mut self,
        action: &str,
        direction: i32,
        stepped: bool,
        settings: &mut Settings,
    ) -> bool {
        let step = |value: f32, direction: i32| {
            let step = (value * 4.).round() as i32 + direction;
            // A tap wraps 100% back to off; Left/Right stop at either end.
            (if stepped {
                step.clamp(0, 4)
            } else {
                (step + 5) % 5
            }) as f32
                / 4.
        };
        match action {
            "graphics" => {
                self.graphics = cycle(&GRAPHICS_PRESETS, &self.graphics, direction).into();
                settings.set("atv-graphics", &self.graphics);
            }
            "vibration" => {
                self.vibration = !self.vibration;
                settings.set("atv-vibration", if self.vibration { "on" } else { "off" });
            }
            "speed-rush" => {
                self.speed_rush = cycle(&SPEED_RUSH, &self.speed_rush, direction).into();
                settings.set("atv-speed-rush", &self.speed_rush);
            }
            "funnycrashes" => {
                self.crash_mode = cycle(&CRASH_MODES, &self.crash_mode, direction).into();
                settings.set("atv-crash-mode", &self.crash_mode);
                settings.set(
                    "atv-funny-crashes",
                    &(self.crash_mode != "original").to_string(),
                );
            }
            "crash-camera" => {
                self.crash_camera_original = !self.crash_camera_original;
                let v = if self.crash_camera_original {
                    "original"
                } else {
                    "tight"
                };
                settings.set("atv-crash-camera", v);
            }
            "chase-camera" => {
                self.chase_assist = !self.chase_assist;
                let v = if self.chase_assist {
                    "assisted"
                } else {
                    "original"
                };
                settings.set("atv-chase-camera", v);
            }
            "flips" => {
                self.flips = !self.flips;
                settings.set("atv-flips", if self.flips { "unleashed" } else { "off" });
            }
            "train-riding" => {
                self.realistic_train = !self.realistic_train;
                let v = if self.realistic_train {
                    "realistic"
                } else {
                    "original"
                };
                settings.set("atv-train-riding", v);
            }
            "measurement" => {
                self.metric = !self.metric;
                let v = if self.metric { "metric" } else { "standard" };
                settings.set("atv-measurement", v);
            }
            "new-profiles" => {
                self.new_profiles_unlocked = !self.new_profiles_unlocked;
                let v = if self.new_profiles_unlocked {
                    "on"
                } else {
                    "off"
                };
                settings.set(crate::settings::NEW_PROFILES_KEY, v);
            }
            "sound-configuration" => {
                self.sound_configuration =
                    cycle(&SOUND_CONFIGURATIONS, &self.sound_configuration, direction).into();
                settings.set("atv-sound-configuration", &self.sound_configuration);
            }
            "diagnostics" => {
                self.diagnostics = cycle(&DIAGNOSTICS, &self.diagnostics, direction).into();
                settings.set("atv.diagnostics.v1", &self.diagnostics);
            }
            "soundmode" => {
                self.sound_mode = if self.sound_mode == "original" {
                    "enhanced"
                } else {
                    "original"
                }
                .into();
                self.save_audio(settings);
            }
            "silentmode" => {
                self.silent = if self.silent == "mute" {
                    "play"
                } else {
                    "mute"
                }
                .into();
                self.save_audio(settings);
            }
            "sfxvolume" => {
                self.sfx_volume = step(self.sfx_volume, direction);
                self.save_audio(settings);
            }
            "musictoggle" => {
                self.music_enabled = !self.music_enabled;
                self.save_music(settings);
            }
            _ if action.starts_with("alpha:") => {
                let id = &action[6..];
                if !ALPHA_FEATURES.iter().any(|(f, _)| *f == id) {
                    return false;
                }
                match self.alpha.iter().position(|a| a == id) {
                    Some(i) => {
                        self.alpha.remove(i);
                    }
                    None => self.alpha.push(id.into()),
                }
                settings.set("atv-alpha", &json!(self.alpha).to_string());
            }
            "musicvolume" => {
                self.music_volume = step(self.music_volume, direction);
                self.save_music(settings);
            }
            "overlay-position" => {
                self.race_position =
                    cycle(&["name", "place", "off"], &self.race_position, 1).into();
                settings.set("atv-race-position", &self.race_position);
            }
            _ if action.starts_with("overlay-") => {
                let group = &action[8..];
                if !OVERLAY_GROUPS.contains(&group) {
                    return false;
                }
                if let Some(i) = self.overlays_off.iter().position(|g| g == group) {
                    self.overlays_off.remove(i);
                } else {
                    self.overlays_off.push(group.into());
                }
                settings.set(
                    "atv-race-overlays-off",
                    &json!(self.overlays_off).to_string(),
                );
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_cycles_and_shared_keys_match_the_browser() {
        let mut settings = Settings::memory();
        let mut prefs = Preferences::load(&settings, false);
        assert_eq!(prefs.graphics, "auto");
        assert_eq!(prefs.graphics_resolved(), "high");
        assert_eq!(prefs.speed_rush, "normal");
        assert!(prefs.music_enabled);
        prefs.apply("graphics", 1, false, &mut settings);
        assert_eq!(settings.get("atv-graphics").as_deref(), Some("original"));
        prefs.apply("graphics", -1, false, &mut settings);
        assert_eq!(prefs.graphics, "auto");
        prefs.apply("funnycrashes", -1, false, &mut settings);
        assert_eq!(prefs.crash_mode, "gorn");
        assert_eq!(settings.get("atv-funny-crashes").as_deref(), Some("true"));
        // Volume taps wrap 100% to off; stepping stops at the ends.
        prefs.music_volume = 1.;
        prefs.apply("musicvolume", 1, false, &mut settings);
        assert_eq!(prefs.music_volume, 0.);
        prefs.apply("musicvolume", -1, true, &mut settings);
        assert_eq!(prefs.music_volume, 0.);
        settings.set(
            "atv-soundtrack-v1",
            r#"{"v":3,"enabled":true,"volume":0.5,"history":[3],"historyIndex":0}"#,
        );
        prefs.apply("musictoggle", 1, false, &mut settings);
        let saved: Value =
            serde_json::from_str(&settings.get("atv-soundtrack-v1").unwrap()).unwrap();
        assert_eq!(saved["history"], json!([3]));
        assert_eq!(saved["enabled"], json!(false));
        prefs.apply("overlay-radar", 1, false, &mut settings);
        assert_eq!(
            settings.get("atv-race-overlays-off").as_deref(),
            Some(r#"["radar"]"#)
        );
        assert_eq!(Preferences::load(&settings, true).overlays_off, ["radar"]);
    }

    #[test]
    fn game_settings_default_to_the_original_and_persist() {
        let mut settings = Settings::memory();
        let mut prefs = Preferences::load(&settings, false);
        assert!(!prefs.metric && prefs.ecosystem_collision);
        assert_eq!(prefs.sound_configuration, "stereo");
        prefs.apply("measurement", 1, false, &mut settings);
        prefs.apply("sound-configuration", -1, true, &mut settings);
        prefs.set_ecosystem_collision(false, &mut settings);
        let loaded = Preferences::load(&settings, false);
        assert!(loaded.metric && !loaded.ecosystem_collision);
        assert_eq!(loaded.sound_configuration, "mono");
        assert_eq!(loaded.units(), atv_core::hall_of_fame::Units::Metric);
        settings.set("atv-sound-configuration", "surround");
        assert_eq!(
            Preferences::load(&settings, false).sound_configuration,
            "stereo"
        );
    }
}
