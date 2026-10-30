//! Other riders' engines and body impacts: a multiplayer room's riders
//! (atv-web multiplayer.rs op 83 `presentation` and web/audio.js
//! `remoteFrame`), split screen's players 2-4 and a race's CPUs. The
//! nearest six riders keep an engine voice selected by the original engine
//! sound state machine from their throttle, speed and riding state; a new
//! crash serial plays an original body impact at the detached rider. Both
//! are placed by the recovered spatial arithmetic (distance attenuation,
//! stereo pan, Doppler) for the nearest listener.
//!
//! CPU riders follow the original quad engine emitter (`002ca5b0`,
//! `002ca8f0`, update `002caac0`): the race's shared sound table `00213210`
//! gives each CPU slot an engine set ([`EngineKey::Set`], usually the
//! reduced `A` variant), the emitter uses that set's Min/Max attenuation
//! distances, and the sound manager `001736f0` stops an engine whose
//! nearest listener is out of range (`001c12e0`: a looping sample beyond
//! 1.2 x MaxDistance, a one-shot beyond MaxDistance, flags set by
//! `002cb8e0`) and restarts its current sample on return (`002cb880`).
use super::{
    bank::normal,
    mixer::{Bus, Looping, Mixer, Pcm, VoiceId},
};
use crate::Result;
use atv_core::{
    engine_audio::{
        EngineAudioConfig, EngineAudioState, OriginalRandom, PlaybackPosition, sample_index,
    },
    sound_effects,
    spatial_audio::{self, Command, Listener, Source},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

/// Remote engine voices kept for the nearest riders (atv-web ENGINE_VOICES).
pub const ENGINE_VOICES: usize = 6;

/// A race's CPU riders use peer ids from here (room peers are small,
/// split screen's players 250-252).
pub const CPU_PEER: u8 = 200;

/// The engine bank a rider's voice plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EngineKey {
    /// The quad's own authored set (`EngineSets` `QuadN`).
    Quad(u16),
    /// A set by number, `true` for its reduced `A` variant (`00213210`).
    Set(u16, bool),
}

impl EngineKey {
    /// `vehicle.aud`'s section name for an explicit set.
    pub fn set_name(self) -> Option<String> {
        match self {
            Self::Quad(_) => None,
            Self::Set(number, reduced) => {
                Some(format!("{number}{}", if reduced { "A" } else { "" }))
            }
        }
    }

    /// Parses a set name `00213210` chose ("252", "290A").
    pub fn from_set_name(name: &str) -> Option<Self> {
        let (number, reduced) = match name.strip_suffix('A') {
            Some(number) => (number, true),
            None => (name, false),
        };
        Some(Self::Set(number.parse().ok()?, reduced))
    }
}

/// One remote rider as presented this frame.
#[derive(Clone, Copy, Debug)]
pub struct Rider {
    pub peer: u8,
    pub quad: u16,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub rider_position: [f32; 3],
    pub rider_velocity: [f32; 3],
    pub throttle: f32,
    pub speed: f32,
    pub riding: bool,
    pub crash_serial: u32,
    pub stale: bool,
    /// The engine set this rider plays; None plays the quad's own set.
    pub engine: Option<EngineKey>,
    /// A race CPU: the recovered emitter range rules (module docs).
    pub cpu: bool,
}

impl Rider {
    pub fn key(&self) -> EngineKey {
        self.engine.unwrap_or(EngineKey::Quad(self.quad))
    }
}

/// The riders that keep an engine voice: the fresh ones nearest the
/// player's vehicle, slot order breaking ties (atv-web multiplayer.rs ranks
/// by distance to the player's own vehicle, not the camera).
pub fn voiced(riders: &[Rider], own: Option<[f32; 3]>) -> BTreeSet<u8> {
    let mut ranked: Vec<(f32, u8)> = riders
        .iter()
        .filter(|r| !r.stale)
        .map(|r| {
            let d: f32 = own.map_or(0., |own| {
                r.position
                    .iter()
                    .zip(own)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum()
            });
            (d, r.peer)
        })
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    ranked.iter().take(ENGINE_VOICES).map(|(_, p)| *p).collect()
}

/// A remote quad's engine table and decoded samples.
pub struct Engine {
    pub config: EngineAudioConfig,
    pub samples: HashMap<String, Arc<Pcm>>,
}

/// A playing engine sample: (voice, duration, start time).
type Playing = (VoiceId, f32, f64);

struct Voice {
    key: EngineKey,
    state: EngineAudioState,
    previous: [usize; 10],
    random: OriginalRandom,
    voice: Option<Playing>,
    /// Earlier samples an overlapping command left playing to their end.
    tail: Vec<VoiceId>,
    /// The emitter's current sample (name, looping), restarted when the
    /// sound manager brings a stopped emitter back in range (`002cb880`).
    current: Option<(String, bool)>,
    /// The sound manager stopped this emitter out of range.
    culled: bool,
}

#[derive(Default)]
pub struct RemoteAudio {
    engines: HashMap<EngineKey, Engine>,
    requested: BTreeSet<EngineKey>,
    voices: BTreeMap<u8, Voice>,
    impacts: BTreeMap<u8, VoiceId>,
    heard_crash: BTreeMap<u8, u32>,
    last: Option<f64>,
    /// Engine samples started (diagnostics and tests).
    pub starts: u64,
    pub impact_starts: u64,
    /// CPU engine samples started and range stops (diagnostics).
    pub cpu_starts: u64,
    pub cpu_culls: u64,
    /// Each voiced rider's latest engine level: (peer, cpu, distance to
    /// the nearest listener, left/right gain), for diagnostics and tests.
    pub levels: Vec<(u8, bool, f32, [f32; 2])>,
}

fn apply(mixer: &mut Mixer, id: VoiceId, command: &Command) {
    if command.pitch < -1 || command.pitch >= 0x4000 {
        return;
    }
    mixer.set_gains(
        id,
        [command.left as f32 / 100., command.right as f32 / 100.],
    );
    mixer.set_rate(id, command.pitch.max(0) as f32 / 4096.);
}

/// The sound manager's range test for an engine emitter (`001c12e0` with
/// the flags `002cb8e0` writes): a looping sample stays in range while the
/// nearest listener is closer than 1.2 x MaxDistance, a one-shot while it
/// is no farther than MaxDistance.
pub fn engine_in_range(distance: f32, max_distance: f32, looping: bool) -> bool {
    if looping {
        distance < max_distance * 1.2
    } else {
        distance <= max_distance
    }
}

fn listener_distance(position: [f32; 3], listeners: &[Listener]) -> f32 {
    let Some(listener) = listeners.get(spatial_audio::nearest(position, listeners)) else {
        return f32::MAX;
    };
    let d: [f32; 3] = std::array::from_fn(|i| position[i] - listener.position[i]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

impl RemoteAudio {
    /// Engine banks that should be loaded next (each once).
    pub fn wanted(&mut self, riders: &[Rider]) -> Vec<EngineKey> {
        let mut out = Vec::new();
        for rider in riders {
            let key = rider.key();
            let valid = match key {
                EngineKey::Quad(quad) => (1..=26).contains(&quad),
                EngineKey::Set(..) => true,
            };
            if valid && self.requested.insert(key) {
                out.push(key);
            }
        }
        out
    }

    pub fn add_engine(&mut self, key: EngineKey, engine: Engine) {
        self.engines.insert(key, engine);
    }

    /// Engine voices playing (diagnostics).
    pub fn voices(&self) -> usize {
        self.voices.values().filter(|v| v.voice.is_some()).count()
    }

    /// CPU engine voices playing (diagnostics).
    pub fn cpu_voices(&self) -> usize {
        self.voices
            .iter()
            .filter(|(peer, v)| **peer >= CPU_PEER && **peer < CPU_PEER + 32 && v.voice.is_some())
            .count()
    }

    fn stop_voice(mixer: &mut Mixer, voice: &mut Voice) {
        if let Some((id, ..)) = voice.voice.take() {
            mixer.stop(id);
        }
        for id in voice.tail.drain(..) {
            mixer.stop(id);
        }
    }

    /// Stops every remote voice (leaving the room, silence).
    pub fn stop_all(&mut self, mixer: &mut Mixer) {
        for (_, mut voice) in std::mem::take(&mut self.voices) {
            Self::stop_voice(mixer, &mut voice);
        }
        for (_, id) in std::mem::take(&mut self.impacts) {
            mixer.stop(id);
        }
        self.heard_crash.clear();
        self.levels.clear();
    }

    /// One rendered frame of remote sound; `impact_sample` looks up the
    /// player's decoded body-impact samples. `own` is the player's vehicle,
    /// which ranks the riders that keep an engine voice. `audible` is false
    /// while the output is suspended: voices stop, as the browser drops them.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        mixer: &mut Mixer,
        effects: Option<&sound_effects::Config>,
        impact_sample: &dyn Fn(&str) -> Option<Arc<Pcm>>,
        listeners: &[Listener],
        own: Option<[f32; 3]>,
        riders: &[Rider],
        audible: bool,
    ) -> Result<()> {
        let now = mixer.seconds();
        let dt = self
            .last
            .map_or(0.016, |last| (now - last) as f32)
            .clamp(0., 0.1);
        self.last = Some(now);
        self.levels.clear();
        let voiced = voiced(riders, own);
        let present: BTreeSet<u8> = riders.iter().filter(|r| !r.stale).map(|r| r.peer).collect();
        let keys: BTreeMap<u8, EngineKey> = riders.iter().map(|r| (r.peer, r.key())).collect();
        // Silent, stale, departed or re-vehicled riders lose their voice.
        self.voices.retain(|peer, voice| {
            let keep = audible && voiced.contains(peer) && keys.get(peer) == Some(&voice.key);
            if !keep {
                Self::stop_voice(mixer, voice);
            }
            keep
        });
        self.impacts.retain(|peer, id| {
            let keep = audible && present.contains(peer) && mixer.playing(Some(*id));
            if !keep {
                mixer.stop(*id);
            }
            keep
        });
        self.heard_crash.retain(|peer, _| keys.contains_key(peer));
        for rider in riders {
            let previous_crash = self.heard_crash.insert(rider.peer, rider.crash_serial);
            if rider.stale || !audible {
                continue;
            }
            if voiced.contains(&rider.peer)
                && let Some(engine) = self.engines.get(&rider.key())
            {
                // CPUs use their set's attenuation (002ca8f0); room riders
                // keep atv-web's 15/300ft.
                let (min_distance, max_distance) = if rider.cpu {
                    (engine.config.min_distance, engine.config.max_distance)
                } else {
                    (15., 300.)
                };
                let spatial = spatial_audio::command(
                    Source {
                        position: rider.position,
                        velocity: rider.velocity,
                        min_distance,
                        max_distance,
                    },
                    listeners,
                )?;
                let voice = self.voices.entry(rider.peer).or_insert_with(|| Voice {
                    key: rider.key(),
                    state: EngineAudioState::default(),
                    previous: [usize::MAX; 10],
                    random: OriginalRandom::with_seed(u64::from(rider.peer) + 1),
                    voice: None,
                    tail: Vec::new(),
                    current: None,
                    culled: false,
                });
                voice.tail.retain(|id| mixer.playing(Some(*id)));
                if let Some((id, ..)) = voice.voice
                    && !mixer.playing(Some(id))
                {
                    voice.voice = None;
                }
                let (duration, playhead) =
                    voice.voice.map_or((0., 0.), |(_, duration, started)| {
                        (duration, (now - started) as f32)
                    });
                let selection = voice.state.step(
                    &engine.config,
                    dt,
                    rider.throttle,
                    rider.riding,
                    rider.speed,
                    PlaybackPosition { duration, playhead },
                )?;
                // (sample, looping, overlap)
                let mut start = None;
                if let Some(v) = selection {
                    let kind = v.sound as usize;
                    let samples = &engine.config.samples[kind];
                    if !samples.is_empty() {
                        // As the player's engine: a random pick for the
                        // acceleration lists, else the first sample.
                        let choice = if matches!(kind, 2..=6) {
                            let random = if samples.len() >= 2 {
                                voice.random.next_u31()
                            } else {
                                0
                            };
                            sample_index(samples.len(), &mut voice.previous[kind], random)?
                        } else {
                            0
                        };
                        start = Some((samples[choice].clone(), kind == 1, v.overlap));
                    }
                } else if voice.current.is_none() && !engine.config.samples[1].is_empty() {
                    // Nothing heard yet: the idle loop (002cae30 selects
                    // the last idle sample; op 83's fallback).
                    start = engine.config.samples[1]
                        .last()
                        .map(|s| (s.clone(), true, false));
                }
                if let Some((name, looping, _)) = &start {
                    voice.current = Some((name.clone(), *looping));
                }
                let distance = listener_distance(rider.position, listeners);
                let looping = voice.current.as_ref().is_some_and(|(_, l)| *l);
                if rider.cpu && !engine_in_range(distance, max_distance, looping) {
                    // 001736f0: out of range, a playing emitter stops.
                    if voice.voice.is_some() || !voice.tail.is_empty() {
                        Self::stop_voice(mixer, voice);
                        self.cpu_culls += 1;
                    }
                    voice.culled = true;
                    start = None;
                } else if start.is_none() && voice.culled && voice.voice.is_none() {
                    // Back in range: the current sample restarts.
                    start = voice.current.clone().map(|(name, looping)| (name, looping, false));
                }
                if start.is_some() {
                    voice.culled = false;
                }
                if let Some((name, looping, overlap)) = start
                    && let Some(pcm) = engine.samples.get(&normal(&name))
                {
                    if let Some((id, ..)) = voice.voice.take() {
                        if overlap {
                            voice.tail.push(id);
                        } else {
                            mixer.stop(id);
                        }
                    }
                    if !overlap {
                        for id in voice.tail.drain(..) {
                            mixer.stop(id);
                        }
                    }
                    let id = mixer.play(
                        pcm,
                        if looping {
                            Looping::Markers
                        } else {
                            Looping::Once
                        },
                        [0., 0.],
                        1.,
                        Bus::Sfx,
                        false,
                        if rider.cpu { "cpu engine" } else { "remote engine" },
                    );
                    voice.voice = Some((id, pcm.duration(), now));
                    self.starts += 1;
                    if rider.cpu {
                        self.cpu_starts += 1;
                    }
                }
                if let Some((id, ..)) = voice.voice {
                    apply(mixer, id, &spatial);
                    self.levels.push((
                        rider.peer,
                        rider.cpu,
                        distance,
                        [spatial.left as f32 / 100., spatial.right as f32 / 100.],
                    ));
                }
                for id in &voice.tail {
                    apply(mixer, *id, &spatial);
                }
            }
            // A new crash serial: an original body impact at the rider.
            if previous_crash.is_some_and(|v| v != rider.crash_serial)
                && let Some(config) = effects
                && !config.body_impact.is_empty()
            {
                let index = rider.crash_serial as usize % config.body_impact.len();
                let spatial = spatial_audio::command(
                    Source {
                        position: rider.rider_position,
                        velocity: rider.rider_velocity,
                        min_distance: config.min_distance,
                        max_distance: config.max_distance,
                    },
                    listeners,
                )?;
                if let Some(pcm) = impact_sample(&config.body_impact[index]) {
                    if let Some(old) = self.impacts.remove(&rider.peer) {
                        mixer.stop(old);
                    }
                    let id = mixer.play(
                        &pcm,
                        Looping::Once,
                        [0., 0.],
                        1.,
                        Bus::Sfx,
                        false,
                        "remote impact",
                    );
                    apply(mixer, id, &spatial);
                    self.impacts.insert(rider.peer, id);
                    self.impact_starts += 1;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rider(peer: u8, x: f32, crash: u32) -> Rider {
        Rider {
            peer,
            quad: 1,
            position: [x, 0., 0.],
            velocity: [0.; 3],
            rider_position: [x, 0., 0.],
            rider_velocity: [0.; 3],
            throttle: 0.,
            speed: 0.,
            riding: crash == 0,
            crash_serial: crash,
            stale: false,
            engine: None,
            cpu: false,
        }
    }

    fn loop_pcm() -> Arc<Pcm> {
        Arc::new(Pcm {
            channels: 1,
            frames: 48_000,
            data: vec![0.25; 48_000],
            loop_markers: Some((0., 1.)),
            rate: 48_000,
        })
    }

    fn engine() -> Engine {
        let mut samples: [Vec<String>; 10] = Default::default();
        for (kind, list) in samples.iter_mut().enumerate() {
            list.push(format!("s{kind}.vag"));
        }
        Engine {
            config: EngineAudioConfig {
                engine_set: "250A".into(),
                stroke: 2,
                low_torque_speed: 25.,
                mid_torque_speed: 45.,
                airborne_time_to_decel: 0.3,
                overlap_time: 0.15,
                min_distance: 30.,
                max_distance: 250.,
                doppler_factor: 1.3,
                samples,
                impact_samples: Vec::new(),
            },
            samples: (0..10).map(|k| (format!("s{k}"), loop_pcm())).collect(),
        }
    }

    #[test]
    fn a_new_crash_serial_plays_one_spatial_body_impact() {
        let mut mixer = Mixer::new(48_000);
        let pcm = Arc::new(Pcm {
            channels: 1,
            frames: 4800,
            data: vec![0.; 4800],
            loop_markers: None,
            rate: 48_000,
        });
        let effects = sound_effects::Config {
            body_impact: vec!["impact0".into(), "impact1".into()],
            min_distance: 10.,
            max_distance: 200.,
        };
        let listener = Listener {
            position: [0.; 3],
            forward: [0., 0., 1.],
            velocity: [0.; 3],
        };
        let sample = |_: &str| Some(pcm.clone());
        let mut audio = RemoteAudio::default();
        assert_eq!(audio.wanted(&[rider(1, 20., 0)]), [EngineKey::Quad(1)]);
        assert!(audio.wanted(&[rider(1, 20., 0)]).is_empty());
        // The first serial heard is only remembered.
        audio
            .frame(
                &mut mixer,
                Some(&effects),
                &sample,
                std::slice::from_ref(&listener),
                Some([0.; 3]),
                &[rider(1, 20., 0)],
                true,
            )
            .unwrap();
        assert_eq!(audio.impact_starts, 0);
        audio
            .frame(
                &mut mixer,
                Some(&effects),
                &sample,
                std::slice::from_ref(&listener),
                Some([0.; 3]),
                &[rider(1, 20., 1)],
                true,
            )
            .unwrap();
        assert_eq!(audio.impact_starts, 1);
        // Silent output: nothing new starts and the impact stops.
        audio
            .frame(
                &mut mixer,
                Some(&effects),
                &sample,
                std::slice::from_ref(&listener),
                Some([0.; 3]),
                &[rider(1, 20., 2)],
                false,
            )
            .unwrap();
        assert_eq!(audio.impact_starts, 1);
        assert_eq!(mixer.voices(), 0);
    }

    #[test]
    fn engine_voices_go_to_riders_nearest_the_player_not_the_camera() {
        let riders: Vec<Rider> = (1..=8).map(|p| rider(p, f32::from(p) * 10., 0)).collect();
        // Six voices nearest the player at x = 70 (the camera is not asked).
        let near = voiced(&riders, Some([70., 0., 0.]));
        assert_eq!(near.into_iter().collect::<Vec<_>>(), [3, 4, 5, 6, 7, 8]);
        // No player position: slot order.
        let none = voiced(&riders, None);
        assert_eq!(none.into_iter().collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn set_names_round_trip() {
        assert_eq!(EngineKey::Set(290, true).set_name().as_deref(), Some("290A"));
        assert_eq!(EngineKey::from_set_name("290A"), Some(EngineKey::Set(290, true)));
        assert_eq!(EngineKey::from_set_name("252"), Some(EngineKey::Set(252, false)));
        assert_eq!(EngineKey::from_set_name("x"), None);
        assert_eq!(EngineKey::Quad(3).set_name(), None);
    }

    #[test]
    fn engine_range_follows_the_sound_manager() {
        // Loops: closer than 1.2 x MaxDistance; one-shots: within MaxDistance.
        assert!(engine_in_range(299.9, 250., true));
        assert!(!engine_in_range(300., 250., true));
        assert!(engine_in_range(250., 250., false));
        assert!(!engine_in_range(250.1, 250., false));
    }

    #[test]
    fn cpu_engines_attenuate_with_distance_stop_out_of_range_and_restart() {
        let mut mixer = Mixer::new(48_000);
        let listener = Listener {
            position: [0.; 3],
            forward: [0., 0., 1.],
            velocity: [0.; 3],
        };
        let key = EngineKey::Set(250, true);
        let cpu = |x: f32| Rider {
            peer: CPU_PEER,
            engine: Some(key),
            cpu: true,
            ..rider(0, x, 0)
        };
        let sample = |_: &str| None;
        let mut audio = RemoteAudio::default();
        assert_eq!(audio.wanted(&[cpu(10.)]), [key]);
        audio.add_engine(key, engine());
        let mut level = |audio: &mut RemoteAudio, mixer: &mut Mixer, x: f32| {
            audio
                .frame(
                    mixer,
                    None,
                    &sample,
                    std::slice::from_ref(&listener),
                    Some([0.; 3]),
                    &[cpu(x)],
                    true,
                )
                .unwrap();
            audio.levels.first().map(|l| l.3[0] + l.3[1])
        };
        // Inside MinAttenuationDistance (30ft): full level; idle loop starts.
        let near = level(&mut audio, &mut mixer, 20.).unwrap();
        assert_eq!(audio.cpu_starts, 1);
        assert_eq!(audio.cpu_voices(), 1);
        let mid = level(&mut audio, &mut mixer, 140.).unwrap();
        let far = level(&mut audio, &mut mixer, 249.).unwrap();
        assert!(near > mid && mid > far, "{near} {mid} {far}");
        // Past 250ft the volume is zero; the loop still plays until 300ft.
        assert_eq!(level(&mut audio, &mut mixer, 280.), Some(0.));
        assert_eq!(level(&mut audio, &mut mixer, 320.), None);
        assert_eq!(audio.cpu_culls, 1);
        assert_eq!(mixer.voices(), 0);
        // Back in range: the current loop restarts.
        assert!(level(&mut audio, &mut mixer, 40.).unwrap() > 0.);
        assert_eq!(audio.cpu_starts, 2);
    }
}
