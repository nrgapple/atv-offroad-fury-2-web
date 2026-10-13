//! Original engine sample selection (002cb1f0 and its transition helpers).
//! The SPU2 mixer, spatialization and gameplay scheduling are separate systems.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(usize)]
pub enum EngineSound {
    Silent = 0,
    Idle = 1,
    LowAccel = 2,
    MidAccel = 3,
    HighAccel = 4,
    Power = 5,
    Sputter = 6,
    Decel = 7,
    Rev = 8,
    Reset = 9,
}

#[derive(Debug, Serialize)]
pub struct EngineAudioConfig {
    pub engine_set: String,
    pub stroke: u32,
    pub low_torque_speed: f32,
    pub mid_torque_speed: f32,
    pub airborne_time_to_decel: f32,
    pub overlap_time: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub doppler_factor: f32,
    /// Index corresponds to EngineSound; strings are authored resource names.
    pub samples: [Vec<String>; 10],
    pub impact_samples: Vec<String>,
}
impl EngineAudioConfig {
    /// MX source banks have direct sections and SoundN keys. The shared ATV
    /// transition controller remains a provisional browser playback adapter.
    pub fn parse_mx(bytes: &[u8]) -> Result<Self> {
        let data = crate::motorcycle::cleaned(bytes)?;
        let mut samples: [Vec<String>; 10] = Default::default();
        for (sound, name) in [
            (EngineSound::Idle, "Idle"),
            (EngineSound::LowAccel, "LowAccel"),
            (EngineSound::HighAccel, "HighAccel"),
            (EngineSound::Power, "Power"),
            (EngineSound::Sputter, "Sputter"),
            (EngineSound::Decel, "Decel"),
            (EngineSound::Rev, "Panic"),
            (EngineSound::Reset, "Clutch"),
        ] {
            let count = data.first_scalar(name, "NumSounds")?;
            if !(0. ..=256.).contains(&count) || count.fract() != 0. {
                return Err("invalid MX audio bank size".into());
            }
            for i in 1..=count as usize {
                samples[sound as usize].push(data.first(name, &format!("Sound{i}"))?.value.clone());
            }
        }
        let impact_count = data.first_scalar("Wreck", "NumSounds")? as usize;
        if impact_count > 256 {
            return Err("MX impact bank exceeds limit".into());
        }
        let impact_samples = (1..=impact_count)
            .map(|i| {
                data.first("Wreck", &format!("Sound{i}"))
                    .map(|v| v.value.clone())
            })
            .collect::<Result<Vec<_>>>()?;
        let low = data.first_scalar("Params", "LowTorqueSpeed")? * 1.46666;
        Ok(Self {
            engine_set: "MX".into(),
            stroke: 0,
            low_torque_speed: low,
            mid_torque_speed: low,
            airborne_time_to_decel: data.first_scalar("Params", "AirborneTimeToDecel")?,
            overlap_time: data.first_scalar("Params", "OverlapTime")?,
            min_distance: data.first_scalar("Params", "MinAttenuationDistance")?,
            max_distance: data.first_scalar("Params", "MaxAttenuationDistance")?,
            doppler_factor: data.first_scalar("Params", "DopplerFactor")?,
            samples,
            impact_samples,
        })
    }
    /// Single-player authored set. Multiplayer may substitute shared engine sets
    /// to fit the original sound-memory budget (00213210).
    pub fn parse(bytes: &[u8], quad: u32) -> Result<Self> {
        if bytes.len() > 1024 * 1024 {
            return Err("engine audio table exceeds limit".into());
        }
        // AUD permits inline semicolon comments, including after scalar values.
        // Preserve quoted semicolons for the strict value parser to reject.
        let text = std::str::from_utf8(bytes).map_err(|_| "engine audio table is not UTF-8")?;
        let cleaned: String = text
            .lines()
            .map(|line| {
                let mut quoted = false;
                let end = line
                    .char_indices()
                    .find_map(|(i, c)| {
                        if c == '"' {
                            quoted = !quoted;
                        }
                        (c == ';' && !quoted).then_some(i)
                    })
                    .unwrap_or(line.len());
                let line = &line[..end];
                // Authored zero-count banks retain empty Sound_1 placeholders.
                // Required sample entries still fail lookup instead of becoming paths.
                if line
                    .split_once('=')
                    .is_some_and(|(_, v)| v.trim().is_empty())
                {
                    "\n".into()
                } else {
                    format!("{line}\n")
                }
            })
            .collect();
        let data = PhysicsData::parse(cleaned.as_bytes())?;
        let mapping = &data.first("EngineSets", &format!("Quad{quad}"))?.value;
        let (set, stroke) = mapping
            .split_once(',')
            .ok_or("invalid engine set mapping")?;
        let set: u32 = set.trim().parse().map_err(|_| "invalid engine set")?;
        let stroke = stroke.trim().parse().map_err(|_| "invalid engine stroke")?;
        let section = format!("{set} Params");
        let value = |key, default| -> Result<f32> {
            match data.first(&section, key) {
                Ok(_) => data.first_scalar(&section, key),
                Err(_) => Ok(default),
            }
        };
        let mut samples: [Vec<String>; 10] = Default::default();
        for (sound, name) in [
            (EngineSound::Idle, "Idle"),
            (EngineSound::LowAccel, "LowAccel"),
            (EngineSound::MidAccel, "MidAccel"),
            (EngineSound::HighAccel, "HighAccel"),
            (EngineSound::Power, "Power"),
            (EngineSound::Sputter, "Sputter"),
            (EngineSound::Decel, "Decel"),
            (EngineSound::Rev, "Rev"),
            (EngineSound::Reset, "Reset"),
        ] {
            let section = format!("{name}-{set}");
            let count: usize = data
                .first(&section, "NumSounds")?
                .value
                .parse()
                .map_err(|_| "invalid engine sound count")?;
            if count > 256 {
                return Err("engine sound count exceeds limit".into());
            }
            for i in 1..=count {
                samples[sound as usize]
                    .push(data.first(&section, &format!("Sound_{i}"))?.value.clone());
            }
        }
        // Emitter defaults proven by002ca5b0; attenuation values required here
        // because their defaults belong to a separate spatial emitter class.
        let result = Self {
            engine_set: set.to_string(),
            stroke,
            low_torque_speed: value("LowTorqueSpeed", 15.)? * f32::from_bits(0x3fbbbb84),
            mid_torque_speed: value("MidTorqueSpeed", 30.)? * f32::from_bits(0x3fbbbb84),
            airborne_time_to_decel: value("AirborneTimeToDecel", 1.)?,
            overlap_time: value("OverlapTime", f32::from_bits(0x3e19999a))?,
            min_distance: data.first_scalar(&section, "MinAttenuationDistance")?,
            max_distance: data.first_scalar(&section, "MaxAttenuationDistance")?,
            doppler_factor: data.first_scalar(&section, "DopplerFactor")?,
            samples,
            impact_samples: Vec::new(),
        };
        if result.overlap_time < 0. || result.airborne_time_to_decel < 0. {
            return Err("negative engine audio time".into());
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SoundCommand {
    pub sound: EngineSound,
    /// False explicitly stops the previous sample before starting this one;
    /// true permits overlap, matching002cb8e0's third argument.
    pub overlap: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PlaybackPosition {
    pub duration: f32,
    /// Original inactive voice sentinel is -1.0.
    pub playhead: f32,
}

pub struct EngineAudioState {
    pub current: EngineSound,
    pub next: EngineSound,
    release_timer: f32,
    airborne_timer: f32,
    was_grounded: bool,
    throttle_was_pressed: bool,
    /// Original emitter +0x220 gate; its runtime owner must set this explicitly.
    pub delay_throttle_release: bool,
}
impl Default for EngineAudioState {
    fn default() -> Self {
        Self {
            current: EngineSound::Silent,
            next: EngineSound::Silent,
            release_timer: 0.25,
            airborne_timer: 0.,
            was_grounded: true,
            throttle_was_pressed: false,
            delay_throttle_release: false,
        }
    }
}
impl EngineAudioState {
    fn select(&mut self, sound: EngineSound, next: EngineSound, overlap: bool) -> SoundCommand {
        self.current = sound;
        self.next = next;
        SoundCommand { sound, overlap }
    }
    fn accelerating(&self) -> bool {
        matches!(
            self.current,
            EngineSound::LowAccel
                | EngineSound::MidAccel
                | EngineSound::HighAccel
                | EngineSound::Power
                | EngineSound::Sputter
        )
    }
    /// Preserve the original comparison duration <= overlap_time + playhead;
    /// subtracting playhead first changes binary32 threshold behavior.
    pub fn step(
        &mut self,
        config: &EngineAudioConfig,
        dt: f32,
        throttle: f32,
        grounded: bool,
        body_speed: f32,
        playback: PlaybackPosition,
    ) -> Result<Option<SoundCommand>> {
        if [
            dt,
            throttle,
            body_speed,
            playback.duration,
            playback.playhead,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || dt < 0.
        {
            return Err("invalid engine audio update".into());
        }
        let pressed = throttle > f32::from_bits(0x3dcccccd);
        let mut command = None;
        if !grounded {
            if self.was_grounded {
                self.airborne_timer = config.airborne_time_to_decel;
            }
            if self.airborne_timer > 0. {
                self.airborne_timer -= dt;
            }
            if pressed {
                if !self.throttle_was_pressed {
                    self.airborne_timer = 0.;
                    command = Some(self.select(EngineSound::Rev, EngineSound::Idle, false));
                } else if self.airborne_timer <= 0. && self.accelerating() {
                    self.airborne_timer = 0.;
                    command = Some(self.select(EngineSound::Decel, EngineSound::Idle, false));
                }
            } else if self.accelerating() {
                command = Some(self.select(EngineSound::Decel, EngineSound::Idle, false));
            }
        } else if pressed {
            self.release_timer = 0.25;
            if matches!(
                self.current,
                EngineSound::Silent | EngineSound::Rev | EngineSound::Idle | EngineSound::Decel
            ) {
                let sound =
                    if self.current == EngineSound::Rev || body_speed > config.mid_torque_speed {
                        EngineSound::HighAccel
                    } else if body_speed <= config.low_torque_speed {
                        EngineSound::LowAccel
                    } else {
                        EngineSound::MidAccel
                    };
                command = Some(self.select(sound, EngineSound::Power, false));
            }
        } else {
            self.release_timer -= dt;
            if (!self.delay_throttle_release || self.release_timer <= 0.) && self.accelerating() {
                command = Some(self.select(EngineSound::Decel, EngineSound::Idle, false));
            }
        }
        self.was_grounded = grounded;
        self.throttle_was_pressed = pressed;
        if command.is_none() && playback.duration <= config.overlap_time + playback.playhead {
            command = match self.next {
                EngineSound::Power => {
                    Some(self.select(EngineSound::Power, EngineSound::Sputter, true))
                }
                EngineSound::Sputter => {
                    Some(self.select(EngineSound::Sputter, EngineSound::Power, true))
                }
                EngineSound::Idle if self.current != EngineSound::Idle => {
                    Some(self.select(EngineSound::Idle, EngineSound::Idle, true))
                }
                EngineSound::Silent => {
                    self.current = EngineSound::Silent;
                    None
                }
                _ => None,
            };
        }
        // Idle and Rev helpers update state but skip playback for empty banks.
        Ok(command.filter(|c| {
            !matches!(c.sound, EngineSound::Idle | EngineSound::Rev)
                || !config.samples[c.sound as usize].is_empty()
        }))
    }
}

/// Recovered libc generator0036e7a0 / unsigned product003b2b28.
/// A local seed reproduces this stream, not the full game's shared RNG call order.
#[derive(Debug, Clone, Copy)]
pub struct OriginalRandom {
    state: u64,
}
impl OriginalRandom {
    pub fn with_seed(seed: u64) -> Self {
        Self { state: seed }
    }
    pub fn next_u31(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(0x5851f42d4c957f2d).wrapping_add(1);
        ((self.state >> 32) & 0x7fff_ffff) as u32
    }
}

/// Exact non-repeating index rule002c9b00 with caller-supplied original RNG output.
pub fn sample_index(count: usize, previous: &mut usize, random: u32) -> Result<usize> {
    if count == 0 || count > 256 || random > 0x7fff_ffff {
        return Err("invalid engine sample choice".into());
    }
    let mut index = if count < 2 {
        0
    } else {
        ((count as f32 * random as f32) * f32::from_bits(0x30000000)) as usize
    };
    if index >= count {
        return Err("engine random sample index outside bank".into());
    }
    if count >= 2 && index == *previous {
        index = (index + 1) % count;
    }
    *previous = index;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> EngineAudioConfig {
        EngineAudioConfig {
            engine_set: "1".into(),
            stroke: 2,
            low_torque_speed: 15.,
            mid_torque_speed: 30.,
            airborne_time_to_decel: 0.3,
            overlap_time: 0.15,
            min_distance: 1.,
            max_distance: 100.,
            doppler_factor: 1.,
            samples: std::array::from_fn(|_| vec!["test.vag".into()]),
            impact_samples: Vec::new(),
        }
    }
    #[test]
    fn grounded_acceleration_alternates_then_releases_to_idle() {
        let c = config();
        let mut s = EngineAudioState::default();
        assert_eq!(
            s.step(
                &c,
                0.016,
                1.,
                true,
                10.,
                PlaybackPosition {
                    duration: 0.,
                    playhead: 0.
                }
            )
            .unwrap()
            .unwrap()
            .sound,
            EngineSound::LowAccel
        );
        assert!(
            s.step(
                &c,
                0.016,
                1.,
                true,
                40.,
                PlaybackPosition {
                    duration: 1.,
                    playhead: 0.
                }
            )
            .unwrap()
            .is_none()
        );
        let cmd = s
            .step(
                &c,
                0.016,
                1.,
                true,
                40.,
                PlaybackPosition {
                    duration: 0.1,
                    playhead: 0.,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(cmd.sound, EngineSound::Power);
        assert!(cmd.overlap);
        assert_eq!(
            s.step(
                &c,
                0.016,
                1.,
                true,
                40.,
                PlaybackPosition {
                    duration: 0.1,
                    playhead: 0.
                }
            )
            .unwrap()
            .unwrap()
            .sound,
            EngineSound::Sputter
        );
        assert_eq!(
            s.step(
                &c,
                0.016,
                0.,
                true,
                40.,
                PlaybackPosition {
                    duration: 1.,
                    playhead: 0.
                }
            )
            .unwrap()
            .unwrap()
            .sound,
            EngineSound::Decel
        );
        assert_eq!(
            s.step(
                &c,
                0.016,
                0.,
                true,
                40.,
                PlaybackPosition {
                    duration: 0.,
                    playhead: 0.
                }
            )
            .unwrap()
            .unwrap()
            .sound,
            EngineSound::Idle
        );
    }
    #[test]
    fn airborne_throttle_repress_selects_rev_and_landing_high_accel() {
        let c = config();
        let mut s = EngineAudioState::default();
        assert_eq!(
            s.step(
                &c,
                0.016,
                1.,
                false,
                0.,
                PlaybackPosition {
                    duration: 0.,
                    playhead: 0.
                }
            )
            .unwrap()
            .unwrap()
            .sound,
            EngineSound::Rev
        );
        assert_eq!(
            s.step(
                &c,
                0.016,
                1.,
                true,
                0.,
                PlaybackPosition {
                    duration: 1.,
                    playhead: 0.
                }
            )
            .unwrap()
            .unwrap()
            .sound,
            EngineSound::HighAccel
        );
    }
    #[test]
    fn duration_comparison_and_empty_banks_preserve_original_state() {
        let mut c = config();
        let mut state = EngineAudioState {
            current: EngineSound::Power,
            next: EngineSound::Sputter,
            ..Default::default()
        };
        let command = state
            .step(
                &c,
                0.,
                1.,
                true,
                50.,
                PlaybackPosition {
                    duration: 100.15,
                    playhead: 100.,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(command.sound, EngineSound::Sputter);
        state.current = EngineSound::Decel;
        state.next = EngineSound::Idle;
        c.samples[1].clear();
        assert!(
            state
                .step(
                    &c,
                    0.,
                    0.,
                    true,
                    0.,
                    PlaybackPosition {
                        duration: 0.,
                        playhead: 0.
                    }
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(state.current, EngineSound::Idle);
    }
    #[test]
    fn recovered_random_stream_and_explicit_seed() {
        let mut random = OriginalRandom::with_seed(1);
        assert_eq!(
            (0..5).map(|_| random.next_u31()).collect::<Vec<_>>(),
            vec![1481765933, 1085377743, 1270216262, 1191391529, 812669700]
        );
    }
    #[test]
    fn sample_selection_avoids_immediate_repetition() {
        let mut previous = 0;
        assert_eq!(sample_index(4, &mut previous, 0).unwrap(), 1);
        assert_eq!(sample_index(4, &mut previous, 0).unwrap(), 0);
        assert_eq!(sample_index(1, &mut previous, 0).unwrap(), 0);
    }
}
