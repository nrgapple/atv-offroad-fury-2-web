//! Other riders' engines and body impacts in a multiplayer room (atv-web
//! multiplayer.rs op 83 `presentation` and web/audio.js `remoteFrame`):
//! the nearest six riders keep an engine voice selected by the original
//! engine sound state machine from their replicated throttle, speed and
//! riding state; a new crash serial plays an original body impact at the
//! detached rider. Both are placed by the recovered spatial arithmetic
//! (distance attenuation, stereo pan, Doppler) for the player's camera.
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

struct Voice {
    quad: u16,
    state: EngineAudioState,
    previous: [usize; 10],
    random: OriginalRandom,
    voice: Option<(VoiceId, f32, f64)>,
}

#[derive(Default)]
pub struct RemoteAudio {
    engines: HashMap<u16, Engine>,
    requested: BTreeSet<u16>,
    voices: BTreeMap<u8, Voice>,
    impacts: BTreeMap<u8, VoiceId>,
    heard_crash: BTreeMap<u8, u32>,
    last: Option<f64>,
    /// Engine samples started (diagnostics and tests).
    pub starts: u64,
    pub impact_starts: u64,
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

impl RemoteAudio {
    /// Quads whose engine banks should be loaded next (each once).
    pub fn wanted(&mut self, riders: &[Rider]) -> Vec<u16> {
        let mut out = Vec::new();
        for rider in riders {
            if (1..=26).contains(&rider.quad) && self.requested.insert(rider.quad) {
                out.push(rider.quad);
            }
        }
        out
    }

    pub fn add_engine(&mut self, quad: u16, engine: Engine) {
        self.engines.insert(quad, engine);
    }

    /// Engine voices playing (diagnostics).
    pub fn voices(&self) -> usize {
        self.voices.values().filter(|v| v.voice.is_some()).count()
    }

    /// Stops every remote voice (leaving the room, silence).
    pub fn stop_all(&mut self, mixer: &mut Mixer) {
        for (_, voice) in std::mem::take(&mut self.voices) {
            if let Some((id, ..)) = voice.voice {
                mixer.stop(id);
            }
        }
        for (_, id) in std::mem::take(&mut self.impacts) {
            mixer.stop(id);
        }
        self.heard_crash.clear();
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
        let voiced = voiced(riders, own);
        let present: BTreeSet<u8> = riders.iter().filter(|r| !r.stale).map(|r| r.peer).collect();
        let quads: BTreeMap<u8, u16> = riders.iter().map(|r| (r.peer, r.quad)).collect();
        // Silent, stale, departed or re-vehicled riders lose their voice.
        self.voices.retain(|peer, voice| {
            let keep = audible && voiced.contains(peer) && quads.get(peer) == Some(&voice.quad);
            if !keep && let Some((id, ..)) = voice.voice {
                mixer.stop(id);
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
        self.heard_crash.retain(|peer, _| quads.contains_key(peer));
        for rider in riders {
            let previous_crash = self.heard_crash.insert(rider.peer, rider.crash_serial);
            if rider.stale || !audible {
                continue;
            }
            let spatial = spatial_audio::command(
                Source {
                    position: rider.position,
                    velocity: rider.velocity,
                    min_distance: 15.,
                    max_distance: 300.,
                },
                listeners,
            )?;
            if voiced.contains(&rider.peer)
                && let Some(engine) = self.engines.get(&rider.quad)
            {
                let voice = self.voices.entry(rider.peer).or_insert_with(|| Voice {
                    quad: rider.quad,
                    state: EngineAudioState::default(),
                    previous: [usize::MAX; 10],
                    random: OriginalRandom::with_seed(u64::from(rider.peer) + 1),
                    voice: None,
                });
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
                let mut start = None;
                if let Some(v) = selection {
                    let kind = v.sound as usize;
                    let samples = &engine.config.samples[kind];
                    if !samples.is_empty() {
                        let random = if samples.len() > 1 {
                            voice.random.next_u31()
                        } else {
                            0
                        };
                        let choice =
                            sample_index(samples.len(), &mut voice.previous[kind], random)?;
                        start = Some((samples[choice].clone(), kind == 1));
                    }
                } else if voice.voice.is_none() && !engine.config.samples[1].is_empty() {
                    // Nothing heard yet: the idle loop (op 83's fallback).
                    start = engine.config.samples[1].last().map(|s| (s.clone(), true));
                }
                if let Some((name, looping)) = start
                    && let Some(pcm) = engine.samples.get(&normal(&name))
                {
                    if let Some((id, ..)) = voice.voice.take() {
                        mixer.stop(id);
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
                        "remote engine",
                    );
                    voice.voice = Some((id, pcm.duration(), now));
                    self.starts += 1;
                }
                if let Some((id, ..)) = voice.voice {
                    apply(mixer, id, &spatial);
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
        assert_eq!(audio.wanted(&[rider(1, 20., 0)]), [1]);
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
}
