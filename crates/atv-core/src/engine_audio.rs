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
    /// The quad's own authored set (`EngineSets` `QuadN`). In a race the
    /// shared sound table `00213210` may instead give a slot another set
    /// ([`assign_sets`]); [`Self::parse_set`] reads such a set by name.
    pub fn parse(bytes: &[u8], quad: u32) -> Result<Self> {
        let data = clean_table(bytes)?;
        let (set, stroke) = quad_set(&data, quad)?;
        Self::from_set(&data, &set.to_string(), stroke)
    }

    /// A set by its section name: the authored number, or the number with
    /// the `A` suffix `00213210` appends for CPU slots (`250A Params`,
    /// `Idle-250A`...). The stroke is the first quad's mapping to that
    /// number (it only steers the substitution, not playback).
    pub fn parse_set(bytes: &[u8], set: &str) -> Result<Self> {
        let data = clean_table(bytes)?;
        let number: u32 = set
            .trim_end_matches('A')
            .parse()
            .map_err(|_| "invalid engine set name")?;
        if set.is_empty() || set.len() > 8 {
            return Err("invalid engine set name".into());
        }
        let stroke = (1..=64)
            .filter_map(|quad| quad_set(&data, quad).ok())
            .find(|(s, _)| *s == number)
            .map_or(0, |(_, stroke)| stroke);
        Self::from_set(&data, set, stroke)
    }

    fn from_set(data: &PhysicsData, set: &str, stroke: u32) -> Result<Self> {
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

/// `vehicle.aud` with its inline comments and empty placeholders removed.
fn clean_table(bytes: &[u8]) -> Result<PhysicsData> {
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
    PhysicsData::parse(cleaned.as_bytes())
}

/// `EngineSets` `QuadN = set,stroke`.
fn quad_set(data: &PhysicsData, quad: u32) -> Result<(u32, u32)> {
    let mapping = &data.first("EngineSets", &format!("Quad{quad}"))?.value;
    let (set, stroke) = mapping
        .split_once(',')
        .ok_or("invalid engine set mapping")?;
    let set = set.trim().parse().map_err(|_| "invalid engine set")?;
    let stroke = stroke.trim().parse().map_err(|_| "invalid engine stroke")?;
    Ok((set, stroke))
}

/// Each quad's `(set, stroke)` from `vehicle.aud` `EngineSets`, quad 1 first.
pub fn engine_sets(bytes: &[u8]) -> Result<Vec<(u32, u32)>> {
    let data = clean_table(bytes)?;
    let count = data.first_scalar("EngineSets", "NumQuads")?;
    if !(1. ..=64.).contains(&count) || count.fract() != 0. {
        return Err("invalid engine set count".into());
    }
    (1..=count as u32).map(|quad| quad_set(&data, quad)).collect()
}

/// The engine set each racer slot loads, as the shared sound table
/// `00213210` chooses them for a race (USA v2.00). `slots` holds every
/// quad's authored `(set, stroke)` in slot order: player 1 first, then the
/// other humans and online riders (`own` counts these, game settings +0x10
/// plus +0x1870), then the CPUs (+0x580, the remaining slots).
///
/// Player 1 keeps its set. The first later slot whose stroke differs from
/// player 1's names the "other" set; when that slot is a CPU it takes the
/// set's reduced `A` variant. A budget of 7 then pays for the remaining
/// distinct sets in slot order: a human's own set costs 5 and a CPU's `A`
/// variant 2 (a slot repeating an earlier slot's name is free). Once a slot
/// cannot be paid for it borrows player 1's set (same stroke) or the other
/// set. These are the original's sound-memory rules, not a voice count.
pub fn assign_sets(slots: &[(u32, u32)], own: usize) -> Vec<String> {
    let mut names: Vec<String> = slots.iter().map(|(set, _)| set.to_string()).collect();
    let Some(&(_, stroke)) = slots.first() else {
        return names;
    };
    let first = names[0].clone();
    let mut other = String::new();
    let mut budget = 7;
    if let Some(i) = (1..slots.len()).find(|&i| slots[i].1 != stroke) {
        if i < own {
            budget = 2;
        } else {
            names[i].push('A');
            budget = 5;
        }
        other = names[i].clone();
    }
    for i in 1..slots.len() {
        if names[..i].contains(&names[i]) {
            continue;
        }
        let borrowed = if slots[i].1 == stroke { &first } else { &other };
        if budget < 2 {
            names[i] = borrowed.clone();
        } else if names[i] != other {
            if i >= own {
                budget -= 2;
                names[i].push('A');
            } else if budget < 5 {
                names[i] = borrowed.clone();
            } else {
                budget -= 5;
            }
        }
    }
    names
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
    fn race_sets_follow_the_sound_table_budget() {
        // One player (two-stroke 252) and five CPUs.
        let slots = [(252, 2), (290, 4), (261, 4), (401, 4), (250, 2), (350, 2)];
        // 290 is the first other stroke: a CPU, so its A variant (7 - 2 = 5
        // left); 261A costs 2 (3 left), 401A 2 (1 left); then the budget is
        // spent: 250 (same stroke) borrows player 1's 252, 350 too.
        assert_eq!(
            assign_sets(&slots, 1),
            ["252", "290A", "261A", "401A", "252", "252"]
        );
        // A repeated set is free; a CPU sharing player 1's set keeps it whole.
        assert_eq!(
            assign_sets(&[(252, 2), (252, 2), (250, 2), (250, 2)], 1),
            ["252", "252", "250A", "250A"]
        );
        // Split screen: player 2 (four-stroke) is the other set and keeps it
        // (5 of 7 spent); its CPUs can then afford one A set, no more.
        assert_eq!(
            assign_sets(&[(252, 2), (290, 4), (250, 2), (300, 4), (253, 2)], 2),
            ["252", "290", "250A", "290", "252"]
        );
        // Two humans on one stroke: player 2's own set costs 5.
        assert_eq!(
            assign_sets(&[(252, 2), (250, 2), (253, 2), (254, 2)], 2),
            ["252", "250", "253A", "252"]
        );
        assert!(assign_sets(&[], 1).is_empty());
    }
    #[test]
    fn cpu_a_sets_parse_with_their_reduced_lists() {
        let table = b"[EngineSets]\nNumQuads=1\nQuad1=250,2\n\
[250A Params]\nMinAttenuationDistance=30.0\nMaxAttenuationDistance=250.0\nDopplerFactor=1.3\n\
[Reset-250A]\nNumSounds = 1\nSound_1 = Engine\\strt400.vag\n\
[Idle-250A]\nNumSounds = 0\nSound_1 = \n\
[Decel-250A]\nNumSounds = 1\nSound_1 = Engine\\250Decel.vag\n\
[Rev-250A]\nNumSounds = 0\nSound_1 = \n\
[LowAccel-250A]\nNumSounds = 1\nSound_1 = Engine\\Low01.vag\n\
[MidAccel-250A]\nNumSounds = 1\nSound_1 = Engine\\Mid01.vag\n\
[HighAccel-250A]\nNumSounds = 1\nSound_1 = Engine\\High01.vag\n\
[Power-250A]\nNumSounds = 1\nSound_1 = Engine\\Power01.vag\n\
[Sputter-250A]\nNumSounds = 1\nSound_1 = Engine\\Sput12.vag\n";
        assert_eq!(engine_sets(table).unwrap(), [(250, 2)]);
        let config = EngineAudioConfig::parse_set(table, "250A").unwrap();
        assert_eq!(config.engine_set, "250A");
        assert_eq!(config.stroke, 2);
        assert!(config.samples[EngineSound::Idle as usize].is_empty());
        assert_eq!(config.max_distance, 250.);
        assert!(EngineAudioConfig::parse_set(table, "250").is_err());
        assert!(EngineAudioConfig::parse_set(table, "x").is_err());
    }
    #[test]
    fn sample_selection_avoids_immediate_repetition() {
        let mut previous = 0;
        assert_eq!(sample_index(4, &mut previous, 0).unwrap(), 1);
        assert_eq!(sample_index(4, &mut previous, 0).unwrap(), 0);
        assert_eq!(sample_index(1, &mut previous, 0).unwrap(), 0);
    }
}
