//! Venue `<venue>.aud` world sounds: 2D ambient cycles and positional
//! Soultree emitters under the original sound manager rules.
//!
//! Exact-build evidence (private Ghidra exports): `002053d0`/`00204a30`
//! parse `AmbientSoundN` (generic fields plus CycleTime); `00205230` builds
//! each as an unregistered child emitter (flags 2, 10 when Looping) whose
//! update `001c2880` accumulates CycleTime and calls the shared play
//! `001c1a50`. `00205920`/`00204b80` build `SoultreeSoundN` through
//! `00173b20` (flags 2 | OneShot 1 | Looping 8 | ResetWhenInaudible 0x10),
//! registered with the manager `001736f0`. Each manager update `001c12e0`
//! marks an emitter in range when the nearest listener is closer than
//! 1.2 × MaxDistance (non-OneShot), restarts an in-range emitter that is not
//! playing through `001c1a50` and stops (`001c17d0`) a playing emitter out of
//! range. `001c1a50` draws RandomTriggerPercent once per latch (+1c0 bit 4),
//! which only a stop clears. The original libc `rand` stream is replaced by
//! the port's own. The browser additionally caps simultaneous Soultree voices.
use crate::{
    Result,
    engine_audio::OriginalRandom,
    quad_sounds::VenueAud,
    spatial_audio::{Command, Listener, Source, command},
    wheel::Vec3,
};
use serde::Serialize;

const UNIT: f32 = 4.656613e-10;
/// Authored browser bound on simultaneous Soultree voices (nearest first).
pub const MAX_SOULTREE_VOICES: usize = 12;

fn sample(aud: &VenueAud, section: &str) -> Result<String> {
    let name = aud
        .get(section, "SoundResourceFile")
        .ok_or("world sound resource missing")?
        .replace('\\', "/")
        .to_ascii_lowercase();
    if name.is_empty() || name.len() > 260 || name.contains("..") || name.starts_with('/') {
        return Err("invalid world sound resource".into());
    }
    Ok(if name.ends_with(".vag") {
        name
    } else {
        format!("{name}.vag")
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ambient {
    pub sample: String,
    pub looping: bool,
    pub cycle_time: f32,
    pub trigger_percent: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Soultree {
    pub sample: String,
    pub position: Vec3,
    pub looping: bool,
    pub trigger_percent: f32,
    pub min_distance: f32,
    pub max_distance: f32,
}
pub fn parse(aud: &VenueAud) -> Result<(Vec<Ambient>, Vec<Soultree>)> {
    let count = |key| aud.int("Sounds", key, 0);
    let (ambients, soultrees) = (count("NumAmbientSounds"), count("NumSoultreeSounds"));
    if !(0..=32).contains(&ambients) || !(0..=96).contains(&soultrees) {
        return Err("world sound count exceeds limit".into());
    }
    let ambient = (1..=ambients)
        .map(|i| {
            let s = format!("AmbientSound{i}");
            Ok(Ambient {
                sample: sample(aud, &s)?,
                looping: aud.flag(&s, "Looping"),
                cycle_time: aud.float(&s, "CycleTime", 0.).max(0.),
                trigger_percent: aud.float(&s, "RandomTriggerPercent", 100.),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut soultree = Vec::new();
    for i in 1..=soultrees {
        let s = format!("SoultreeSound{i}");
        let position: Vec<f32> = aud
            .get(&s, "Position")
            .ok_or("Soultree sound needs a Position")?
            .split(',')
            .map(|v| v.trim().parse::<f32>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| "invalid Soultree Position")?;
        let [x, y, z] = position[..] else {
            return Err("invalid Soultree Position".into());
        };
        let (min_distance, max_distance) = (
            aud.float(&s, "MinDistance", 0.),
            aud.float(&s, "MaxDistance", 0.),
        );
        // Every shipped Soultree sound is OneShot=0; a OneShot record would use
        // its OneShotDistance range instead and is not reproduced.
        if aud.flag(&s, "OneShot") || min_distance < 0. || max_distance <= min_distance {
            continue;
        }
        soultree.push(Soultree {
            sample: sample(aud, &s)?,
            position: [x, y, z],
            looping: aud.flag(&s, "Looping"),
            trigger_percent: aud.float(&s, "RandomTriggerPercent", 100.),
            min_distance,
            max_distance,
        });
    }
    Ok((ambient, soultree))
}

/// The +1c0 bit 4 random-trigger latch of `001c1a50`.
fn try_play(percent: f32, latch: &mut bool, random: &mut OriginalRandom) -> bool {
    if percent < 100. {
        if *latch {
            return false;
        }
        *latch = true;
        if percent < random.next_u31() as f32 * UNIT * 100. {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Default)]
struct AmbientState {
    timer: f32,
    started_loop: bool,
    latch: bool,
}
#[derive(Debug, Clone, Default)]
struct SoultreeState {
    latch: bool,
    voiced: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum WorldCommand {
    Start {
        source: usize,
        sample: String,
        looping: bool,
    },
    Stop {
        source: usize,
    },
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorldFrame {
    pub ambient: Vec<WorldCommand>,
    pub soultree: Vec<WorldCommand>,
    /// Stereo/pitch commands for every voiced Soultree source.
    pub spatial: Vec<(usize, Command)>,
}
pub struct World {
    pub ambient: Vec<Ambient>,
    pub soultree: Vec<Soultree>,
    ambient_state: Vec<AmbientState>,
    soultree_state: Vec<SoultreeState>,
    random: OriginalRandom,
}
impl World {
    pub fn new(ambient: Vec<Ambient>, soultree: Vec<Soultree>, seed: u64) -> Self {
        Self {
            ambient_state: vec![AmbientState::default(); ambient.len()],
            soultree_state: vec![SoultreeState::default(); soultree.len()],
            ambient,
            soultree,
            random: OriginalRandom::with_seed(seed),
        }
    }
    /// Keeps only sources whose sample the content package provides, so an
    /// older package without them stays silent instead of retrying starts.
    pub fn retain_samples(&mut self, available: impl Fn(&str) -> bool) {
        let keep: Vec<bool> = self.ambient.iter().map(|a| available(&a.sample)).collect();
        let mut i = 0;
        self.ambient.retain(|_| {
            i += 1;
            keep[i - 1]
        });
        let keep: Vec<bool> = self.soultree.iter().map(|s| available(&s.sample)).collect();
        let mut i = 0;
        self.soultree.retain(|_| {
            i += 1;
            keep[i - 1]
        });
        self.ambient_state = vec![AmbientState::default(); self.ambient.len()];
        self.soultree_state = vec![SoultreeState::default(); self.soultree.len()];
    }
    /// One frame. `ambient_playing`/`soultree_playing` report each source's
    /// current clip; a finished one-shot reads false. `listeners` holds one
    /// camera per view: a Soultree source's range and placement use the
    /// nearest (`001742f0`; split screen passes every player's camera).
    pub fn step(
        &mut self,
        dt: f32,
        listeners: &[Listener],
        ambient_playing: &[bool],
        soultree_playing: &[bool],
    ) -> Result<WorldFrame> {
        if !dt.is_finite() || !(0. ..=0.1).contains(&dt) {
            return Err("invalid world sound timestep".into());
        }
        let mut frame = WorldFrame::default();
        for (i, (definition, state)) in self.ambient.iter().zip(&mut self.ambient_state).enumerate()
        {
            // 001c2880: a started loop is left alone.
            if definition.looping && state.started_loop {
                continue;
            }
            state.timer += dt;
            let playing = ambient_playing.get(i).copied().unwrap_or(false);
            if state.timer < definition.cycle_time || playing {
                continue;
            }
            if definition.trigger_percent < 100.
                && self.random.next_u31() as f32 * UNIT * 100. > definition.trigger_percent
            {
                continue;
            }
            state.timer = 0.;
            if try_play(
                definition.trigger_percent,
                &mut state.latch,
                &mut self.random,
            ) {
                if definition.looping {
                    state.started_loop = true;
                }
                frame.ambient.push(WorldCommand::Start {
                    source: i,
                    sample: definition.sample.clone(),
                    looping: definition.looping,
                });
            }
        }
        if listeners.is_empty() {
            return Err("world sounds need a listener".into());
        }
        let distance = |p: Vec3| {
            let listener = &listeners[crate::spatial_audio::nearest(p, listeners)];
            let d: Vec3 = std::array::from_fn(|i| p[i] - listener.position[i]);
            ((d[2] * d[2] + d[1] * d[1]) + d[0] * d[0]).sqrt()
        };
        let mut candidates = Vec::new();
        for (i, (definition, state)) in self
            .soultree
            .iter()
            .zip(&mut self.soultree_state)
            .enumerate()
        {
            let playing = state.voiced && soultree_playing.get(i).copied().unwrap_or(false);
            let range = distance(definition.position);
            if range < definition.max_distance * 1.2 {
                if playing {
                    candidates.push((range, i, false));
                } else if try_play(
                    definition.trigger_percent,
                    &mut state.latch,
                    &mut self.random,
                ) {
                    candidates.push((range, i, true));
                } else if state.voiced {
                    state.voiced = false;
                    frame.soultree.push(WorldCommand::Stop { source: i });
                }
            } else if playing {
                // 001c17d0 clears the trigger latch with the stop.
                state.latch = false;
                state.voiced = false;
                frame.soultree.push(WorldCommand::Stop { source: i });
            } else if state.voiced {
                state.voiced = false;
                frame.soultree.push(WorldCommand::Stop { source: i });
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (rank, (_, i, start)) in candidates.into_iter().enumerate() {
            let definition = &self.soultree[i];
            let state = &mut self.soultree_state[i];
            if rank >= MAX_SOULTREE_VOICES {
                if state.voiced {
                    state.voiced = false;
                    frame.soultree.push(WorldCommand::Stop { source: i });
                }
                continue;
            }
            if start {
                state.voiced = true;
                frame.soultree.push(WorldCommand::Start {
                    source: i,
                    sample: definition.sample.clone(),
                    looping: definition.looping,
                });
            }
            frame.spatial.push((
                i,
                command(
                    Source {
                        position: definition.position,
                        velocity: [0.; 3],
                        min_distance: definition.min_distance,
                        max_distance: definition.max_distance,
                    },
                    listeners,
                )?,
            ));
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn listener(x: f32) -> Listener {
        Listener {
            position: [x, 0., 0.],
            forward: [0., 0., 1.],
            velocity: [0.; 3],
        }
    }
    fn aud() -> VenueAud {
        VenueAud::parse(b"[Sounds]\nENV\nNumSoultreeSounds=3\nNumAmbientSounds=2\n[AmbientSound1]\nSoundResourceFile=Crowd\\Indoor\\icrowd01.VAG\nLooping=T\nCycleTime=0\nRandomTriggerPercent=100\n[AmbientSound2]\nSoundResourceFile=Soultree\\w\\WOLF.VAG\nLooping=F\nCycleTime=2\nRandomTriggerPercent=100\n[SoultreeSound1]\nSoundResourceFile=Crowd\\crowdf1\\crdloop4\nPosition=0,0,0\nLooping=1\nRandomTriggerPercent=100.0\nMaxDistance=100\nMinDistance=50\nOneShot=0\n[SoultreeSound2]\nSoundResourceFile=Soultree\\i\\ICECRACK.vag\nPosition=0,0,0\nLooping=F\nRandomTriggerPercent=40\nMaxDistance=80\nMinDistance=70\n[SoultreeSound3]\nSoundResourceFile=x\nPosition=0,0,0\nOneShot=1\nMaxDistance=80\nMinDistance=70\n").unwrap()
    }
    #[test]
    fn parses_ambient_and_soultree_records() {
        let (ambient, soultree) = parse(&aud()).unwrap();
        assert_eq!(ambient[0].sample, "crowd/indoor/icrowd01.vag");
        assert!(ambient[0].looping && !ambient[1].looping);
        assert_eq!(soultree.len(), 2, "OneShot records are skipped");
        assert!(soultree[0].looping, "numeric Looping=1 is true");
        assert_eq!(
            (soultree[1].trigger_percent, soultree[1].sample.as_str()),
            (40., "soultree/i/icecrack.vag")
        );
    }
    #[test]
    fn ambient_cycles_and_loops_once() {
        let (ambient, _) = parse(&aud()).unwrap();
        let mut world = World::new(ambient, vec![], 1);
        let mut run = |seconds: f32, playing: [bool; 2]| {
            let mut starts = Vec::new();
            for _ in 0..(seconds * 10.).round() as usize {
                starts.extend(
                    world
                        .step(0.1, &[listener(0.)], &playing, &[])
                        .unwrap()
                        .ambient,
                );
            }
            starts
        };
        let first = run(0.1, [false, false]);
        assert_eq!(first.len(), 1, "the loop starts on its first update");
        assert!(run(1.8, [true, false]).is_empty());
        assert!(matches!(
            &run(0.3, [true, false])[..],
            [WorldCommand::Start {
                source: 1,
                looping: false,
                ..
            }]
        ));
        // The cycle accumulates while the one-shot plays, then waits for it.
        assert!(run(2.5, [true, true]).is_empty());
        assert_eq!(run(0.1, [true, false]).len(), 1);
    }
    #[test]
    fn split_views_hear_soultree_from_the_nearest_camera() {
        let (_, soultree) = parse(&aud()).unwrap();
        let mut world = World::new(vec![], soultree.into_iter().take(1).collect(), 1);
        // Player 1's camera is out of range, player 2's is near the source.
        let f = world
            .step(0.02, &[listener(500.), listener(60.)], &[], &[false])
            .unwrap();
        assert_eq!(f.soultree.len(), 1);
        let alone = crate::spatial_audio::command(
            crate::spatial_audio::Source {
                position: [0.; 3],
                velocity: [0.; 3],
                min_distance: 50.,
                max_distance: 100.,
            },
            &[listener(60.)],
        )
        .unwrap();
        assert_eq!(f.spatial[0].1.volume, alone.volume);
        assert!(f.spatial[0].1.active);
        // With player 1 alone the source stays silent.
        let mut one = World::new(
            vec![],
            parse(&aud()).unwrap().1.into_iter().take(1).collect(),
            1,
        );
        assert!(
            one.step(0.02, &[listener(500.)], &[], &[false])
                .unwrap()
                .soultree
                .is_empty()
        );
    }
    #[test]
    fn soultree_range_start_stop_and_trigger_latch() {
        let (_, soultree) = parse(&aud()).unwrap();
        let mut world = World::new(vec![], soultree.into_iter().take(1).collect(), 1);
        // Beyond 1.2 × MaxDistance is out of range; just inside starts the loop.
        assert!(
            world
                .step(0.02, &[listener(120.1)], &[], &[false])
                .unwrap()
                .soultree
                .is_empty()
        );
        let f = world.step(0.02, &[listener(119.)], &[], &[false]).unwrap();
        assert!(matches!(
            &f.soultree[..],
            [WorldCommand::Start {
                source: 0,
                looping: true,
                ..
            }]
        ));
        assert_eq!(f.spatial.len(), 1);
        let f = world.step(0.02, &[listener(60.)], &[], &[true]).unwrap();
        assert!(f.soultree.is_empty() && f.spatial[0].1.active);
        let f = world.step(0.02, &[listener(130.)], &[], &[true]).unwrap();
        assert!(matches!(
            &f.soultree[..],
            [WorldCommand::Stop { source: 0 }]
        ));
        // A 0% one-shot draws once, then stays latched until a stop.
        let mut never = World::new(
            vec![],
            vec![Soultree {
                trigger_percent: 0.,
                ..parse(&aud()).unwrap().1[1].clone()
            }],
            1,
        );
        for _ in 0..3 {
            assert!(
                never
                    .step(0.02, &[listener(0.)], &[], &[false])
                    .unwrap()
                    .soultree
                    .is_empty()
            );
        }
        assert!(never.soultree_state[0].latch);
    }
    #[test]
    fn caps_voices_to_the_nearest() {
        let template = parse(&aud()).unwrap().1[0].clone();
        let sources = (0..20)
            .map(|i| Soultree {
                position: [i as f32, 0., 0.],
                ..template.clone()
            })
            .collect();
        let mut world = World::new(vec![], sources, 1);
        let f = world
            .step(0.02, &[listener(0.)], &[], &[false; 20])
            .unwrap();
        assert_eq!(f.soultree.len(), MAX_SOULTREE_VOICES);
        assert!(f.spatial.iter().all(|(i, _)| *i < MAX_SOULTREE_VOICES));
    }
}
