//! The ride's sound decisions, ported from GameAudio.update (web/audio.js),
//! QuadSounds (web/quad-sounds.js) and atv-web's operations 33 (engine),
//! 77 (rider body impacts) and 173 (vocals, chassis, drive-on and world
//! sounds). atv-core chooses every sample, volume and pitch; this module only
//! owns the mixer voices. Steps arrive at the fixed 60 Hz; `frame` runs once
//! per rendered frame like the browser's per-batch update.
use super::{
    bank::{Bank, normal},
    mixer::{Bus, EchoPreset, Looping, Mixer, VoiceId},
};
use crate::{Result, ride::sound::AudioStep};
use atv_core::{
    engine_audio::{EngineAudioState, EngineSound, OriginalRandom, PlaybackPosition, sample_index},
    quad_sounds::{self as qs, DriveCommand, Voice},
    sound_effects::{ImpactInput, RiderImpactState, RiderMotionCache},
    spatial_audio::{self, CameraPose, Command, Listener, Source},
    world_sounds::{World, WorldCommand},
};
use std::collections::HashMap;

/// Settings → Audio → Sound. Original is the recovered baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SoundMode {
    #[default]
    Original,
    Enhanced,
}

/// GameAudio's `current`: the latest engine voice. It is not cleared when
/// its clip ends, so the playhead keeps growing past the duration.
#[derive(Clone, Copy)]
struct EngineVoice {
    duration: f32,
    started: f64,
}

pub struct RideAudio {
    pub bank: Bank,
    pub mode: SoundMode,
    engine: EngineAudioState,
    engine_previous: [usize; 10],
    engine_random: OriginalRandom,
    engine_voices: Vec<VoiceId>,
    current: Option<EngineVoice>,
    commands: u32,
    elapsed: f32,
    impact: RiderImpactState,
    impact_motion: RiderMotionCache,
    impact_random: OriginalRandom,
    effect_voice: Option<VoiceId>,
    effect_spatial: Option<Command>,
    vocals: Option<qs::RiderVocals>,
    chassis: Option<qs::Chassis>,
    motion: RiderMotionCache,
    chassis_source: Option<([f32; 3], [f32; 3], i32)>,
    water: qs::WaterEntry,
    rider_water: qs::RiderWater,
    drive: [qs::DriveEmitter; 2],
    drive_level: [Option<(usize, i32, i32)>; 2],
    drive_random: OriginalRandom,
    vocal_voice: Option<VoiceId>,
    chassis_voice: Option<VoiceId>,
    drive_voices: [Option<VoiceId>; 2],
    world: Option<World>,
    ambient_voices: HashMap<usize, VoiceId>,
    soultree_voices: HashMap<usize, VoiceId>,
    camera: Option<CameraPose>,
    last: Option<(Listener, [f32; 3], [[f32; 3]; 2])>,
    world_dt: f32,
    placement: Option<u32>,
    echo: Option<EchoPreset>,
    /// Counters for diagnostics and parity reports.
    pub stats: Stats,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub engine_commands: u32,
    pub last_engine: Option<EngineSound>,
    pub vocals: u32,
    pub chassis: u32,
    pub drive: u32,
    pub ambient: u32,
    pub soultree: u32,
    pub impacts: u32,
    pub history: Vec<String>,
}

fn gains(command: &Command, volume: i32, rate: f32) -> Option<([f32; 2], f32)> {
    if command.pitch < -1 || command.pitch >= 0x4000 {
        return None;
    }
    // 0014ed40 multiplies the sound volume into the attenuation before 002f4500.
    let (left, right) =
        spatial_audio::stereo(volume.clamp(0, 100) * command.volume / 100, command.pan);
    Some((
        [left as f32 / 100., right as f32 / 100.],
        rate * command.pitch.max(0) as f32 / 4096.,
    ))
}

impl RideAudio {
    pub fn new(bank: Bank, mode: SoundMode) -> Self {
        let vocals = bank
            .vocals
            .clone()
            .filter(|lists| lists.iter().any(|l| !l.is_empty()))
            .map(|lists| qs::RiderVocals::new(lists, 97211 + 9 * 47));
        let chassis = bank
            .chassis
            .clone()
            .map(|banks| qs::Chassis::new(banks, true, 97211 + 10 * 47));
        let world = (!bank.ambient.is_empty() || !bank.soultree.is_empty())
            .then(|| World::new(bank.ambient.clone(), bank.soultree.clone(), 97211 + 12 * 47));
        Self {
            bank,
            mode,
            engine: EngineAudioState::default(),
            engine_previous: [usize::MAX; 10],
            engine_random: OriginalRandom::with_seed(1),
            engine_voices: Vec::new(),
            current: None,
            commands: 0,
            elapsed: 0.,
            impact: RiderImpactState::default(),
            impact_motion: RiderMotionCache::default(),
            impact_random: OriginalRandom::with_seed(1),
            effect_voice: None,
            effect_spatial: None,
            vocals,
            chassis,
            motion: RiderMotionCache::default(),
            chassis_source: None,
            water: Default::default(),
            rider_water: Default::default(),
            drive: Default::default(),
            drive_level: [None; 2],
            drive_random: OriginalRandom::with_seed(97211 + 11 * 47),
            vocal_voice: None,
            chassis_voice: None,
            drive_voices: [None; 2],
            world,
            ambient_voices: HashMap::new(),
            soultree_voices: HashMap::new(),
            camera: None,
            last: None,
            world_dt: 0.,
            placement: None,
            echo: None,
            stats: Stats::default(),
        }
    }

    /// Venue echo type for Enhanced sound (op 172 index 2): stadiums from
    /// the original data, otherwise the authored canyon test at `origin`.
    pub fn configure_echo(
        &mut self,
        crowd_present: bool,
        ground: impl Fn(f32, f32) -> Option<f32>,
        origin: [f32; 3],
    ) -> Result<()> {
        let stadium = crowd_present || self.bank.crowd_venue;
        let enclosure = atv_core::echo::enclosure(|x, z| Ok(ground(x, z)), origin)?;
        self.echo = match atv_core::echo::classify(stadium, enclosure) {
            atv_core::echo::Echo::Open => None,
            atv_core::echo::Echo::Stadium => Some(EchoPreset::Stadium),
            atv_core::echo::Echo::Canyon => Some(EchoPreset::Canyon),
        };
        Ok(())
    }

    pub fn apply_mode(&self, mixer: &mut Mixer) {
        mixer.set_echo(if self.mode == SoundMode::Enhanced {
            self.echo
        } else {
            None
        });
    }

    /// Reset ATV (GameAudio.reset: ops 34, 78 and 174).
    fn reset(&mut self, mixer: &mut Mixer) {
        for id in self.engine_voices.drain(..) {
            mixer.stop(id);
        }
        self.current = None;
        self.engine = EngineAudioState::default();
        self.engine_previous = [usize::MAX; 10];
        self.commands = 0;
        self.elapsed = 0.;
        if let Some(id) = self.effect_voice.take() {
            mixer.stop(id);
        }
        self.impact = RiderImpactState::default();
        if let Some(chassis) = &mut self.chassis {
            chassis.reset();
        }
        if let Some(vocals) = &mut self.vocals {
            vocals.reset();
        }
        self.chassis_source = None;
        self.motion = RiderMotionCache::default();
        self.drive = Default::default();
        self.drive_level = [None; 2];
        if let Some(id) = self.chassis_voice.take() {
            mixer.stop(id);
        }
        for id in self.drive_voices.iter_mut().filter_map(Option::take) {
            mixer.stop(id);
        }
    }

    fn start(
        &mut self,
        mixer: &mut Mixer,
        sample: &str,
        looping: Looping,
        gains: [f32; 2],
        label: &'static str,
    ) -> Option<VoiceId> {
        let pcm = self.bank.get(sample)?.clone();
        self.stats.history.push(normal(sample));
        if self.stats.history.len() > 16 {
            self.stats.history.remove(0);
        }
        Some(mixer.play(&pcm, looping, gains, 1., Bus::Sfx, false, label))
    }

    /// One fixed step's sound inputs (QuadSounds.capture, captureEffects).
    pub fn step(&mut self, mixer: &mut Mixer, step: &AudioStep) -> Result<()> {
        if self.placement != Some(step.placement) {
            if self.placement.is_some() {
                self.reset(mixer);
            }
            self.placement = Some(step.placement);
        }
        let dt = crate::ride::STEP;
        let camera = step.camera.clone();
        let previous = self
            .camera
            .replace(camera.clone())
            .unwrap_or(camera.clone());
        let listener = spatial_audio::camera_listener(&camera, &previous, dt)?;
        self.elapsed += dt;
        self.world_dt += dt;
        self.body_impact(mixer, step, &listener)?;
        self.quad_step(mixer, step, &listener)?;
        Ok(())
    }

    /// Rider body impacts (op 77 per frame).
    fn body_impact(
        &mut self,
        mixer: &mut Mixer,
        step: &AudioStep,
        listener: &Listener,
    ) -> Result<()> {
        let Some(config) = &self.bank.effects else {
            return Ok(());
        };
        let position = step.rider_position;
        let velocity = self
            .impact_motion
            .update(position, 1. / crate::ride::STEP)?;
        self.effect_spatial = Some(spatial_audio::command(
            Source {
                position,
                velocity,
                min_distance: config.min_distance,
                max_distance: config.max_distance,
            },
            std::slice::from_ref(listener),
        )?);
        let playing = mixer.playing(self.effect_voice);
        let random = &mut self.impact_random;
        let event = self.impact.advance(
            ImpactInput {
                sampled_impact_delta: step.rider_impact,
                quad_wreck_phase: Some(if step.riding { 0 } else { 1 }),
                playing,
            },
            config.body_impact.len(),
            || random.next_u31(),
        )?;
        if let Some(event) = event
            && event.play
        {
            let sample = config.body_impact[event.sample].clone();
            if let Some(old) = self.effect_voice.take() {
                mixer.stop(old);
            }
            self.effect_voice = self.start(mixer, &sample, Looping::Once, [0., 0.], "impact");
            self.stats.impacts += 1;
        }
        if let (Some(id), Some(command)) = (self.effect_voice, &self.effect_spatial)
            && !(command.pitch < -1 || command.pitch >= 0x4000)
        {
            mixer.set_gains(
                id,
                [command.left as f32 / 100., command.right as f32 / 100.],
            );
            mixer.set_rate(id, command.pitch.max(0) as f32 / 4096.);
        }
        Ok(())
    }

    /// Vocals, chassis and drive-on for one frame (op 173's frame loop).
    fn quad_step(
        &mut self,
        mixer: &mut Mixer,
        step: &AudioStep,
        listener: &Listener,
    ) -> Result<()> {
        let distance = |p: [f32; 3]| {
            let d: [f32; 3] = std::array::from_fn(|i| p[i] - listener.position[i]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        let mut vocal = Voice {
            playing: mixer.playing(self.vocal_voice),
            in_range: true,
        };
        let mut chassis = Voice {
            playing: mixer.playing(self.chassis_voice),
            in_range: true,
        };
        let mut drive_playing = [0, 1].map(|s| mixer.playing(self.drive_voices[s]));
        let position = step.rider_position;
        self.motion.update(position, 1. / crate::ride::STEP)?;
        vocal.in_range = distance(position) <= qs::VOCAL_MAX;
        let frame = &step.quad;
        chassis.in_range = distance(frame.origin) <= qs::CHASSIS_MAX;
        let events = frame.events;
        let mut vocal_cue = None;
        let mut chassis_cue = None;
        // 001eb6f0 during physics, then 001e8690 after it.
        if events.quad_contact > 1. {
            if let Some(v) = &mut self.vocals {
                vocal_cue = v
                    .request(qs::event::BANG, events.quad_contact, &mut vocal)
                    .or(vocal_cue);
            }
            if let Some(c) = &mut self.chassis {
                chassis_cue = c.quad_contact(&mut chassis).or(chassis_cue);
            }
        }
        if let Some(v) = &mut self.vocals {
            if let Some((event, magnitude)) = events.wreck_entry.and_then(qs::WreckEntry::event) {
                vocal_cue = v.request(event, magnitude, &mut vocal).or(vocal_cue.take());
            } else if events.hard_landing {
                vocal_cue = v
                    .request(qs::event::JOSTLE, 0., &mut vocal)
                    .or(vocal_cue.take());
            }
        }
        let splash = self.water.update(frame.water_depth, frame.velocity)
            | self.rider_water.update(step.rider_water);
        if let Some(c) = &mut self.chassis {
            if splash {
                chassis_cue = c.splash(&mut chassis).or(chassis_cue.take());
            }
            let (cue, _stop) = c.update(frame.chassis, &mut chassis);
            chassis_cue = cue.or(chassis_cue.take());
        }
        if let Some(cue) = &chassis_cue {
            // 002cdb80 sets the voice position only when a clip starts.
            self.chassis_source = Some((frame.origin, frame.velocity, cue.volume));
        }
        let mut drive_commands: [Vec<DriveCommand>; 2] = Default::default();
        if !self.bank.drive.is_empty() {
            let rates = &self.bank.rates;
            let native = |sample: &str| rates.get(&normal(sample)).map(|&r| qs::native_pitch(r));
            drive_commands = qs::drive_on(
                &mut self.drive,
                &self.bank.drive,
                &native,
                frame.drive,
                drive_playing,
                &mut self.drive_random,
            );
            for (side, commands) in drive_commands.iter().enumerate() {
                for command in commands {
                    match *command {
                        DriveCommand::Stop => {
                            drive_playing[side] = false;
                            self.drive_level[side] = None;
                        }
                        DriveCommand::Start {
                            record,
                            pitch,
                            volume,
                            ..
                        }
                        | DriveCommand::Update {
                            record,
                            pitch,
                            volume,
                        } => {
                            if matches!(command, DriveCommand::Start { .. }) {
                                drive_playing[side] = true;
                            }
                            self.drive_level[side] = Some((record, pitch, volume));
                        }
                    }
                }
            }
        }
        if let Some(v) = &mut self.vocals {
            let impact = step.rider_impact;
            let magnitude =
                ((impact[0] * impact[0] + impact[1] * impact[1]) + impact[2] * impact[2]).sqrt();
            vocal_cue = v
                .update(magnitude, frame.chassis.wrecked, &mut vocal)
                .or(vocal_cue.take());
        }
        // QuadSounds.apply.
        if let Some(cue) = vocal_cue {
            if cue.stop_current
                && let Some(id) = self.vocal_voice.take()
            {
                mixer.stop(id);
            }
            if self.bank.get(&cue.sample).is_some() {
                if let Some(id) = self.vocal_voice.take() {
                    mixer.stop(id);
                }
                self.vocal_voice = self.start(mixer, &cue.sample, Looping::Once, [0., 0.], "vocal");
                self.stats.vocals += 1;
            }
        }
        if let Some(cue) = chassis_cue
            && self.bank.get(&cue.sample).is_some()
        {
            if let Some(id) = self.chassis_voice.take() {
                mixer.stop(id);
            }
            self.chassis_voice = self.start(mixer, &cue.sample, Looping::Once, [0., 0.], "chassis");
            self.stats.chassis += 1;
        }
        for (side, commands) in drive_commands.iter().enumerate() {
            for command in commands {
                match command {
                    DriveCommand::Stop => {
                        if let Some(id) = self.drive_voices[side].take() {
                            mixer.stop(id);
                        }
                    }
                    DriveCommand::Start { sample, .. } if self.bank.get(sample).is_some() => {
                        if let Some(id) = self.drive_voices[side].take() {
                            mixer.stop(id);
                        }
                        // Original looping requests use the VAG loop markers.
                        let looping = if self
                            .bank
                            .get(sample)
                            .is_some_and(|p| p.loop_markers.is_some())
                        {
                            Looping::Markers
                        } else {
                            Looping::Once
                        };
                        self.drive_voices[side] =
                            self.start(mixer, sample, looping, [0., 0.], "drive");
                        self.stats.drive += 1;
                    }
                    _ => {}
                }
            }
        }
        self.last = Some((*listener, position, frame.wheels));
        Ok(())
    }

    /// One rendered frame: spatial gains of the latest step, world sounds
    /// and the engine (the browser's per-batch GameAudio.update).
    pub fn frame(&mut self, mixer: &mut Mixer, step: Option<&AudioStep>) -> Result<()> {
        let Some(step) = step else {
            return Ok(());
        };
        let Some((listener, rider, wheels)) = self.last else {
            return Ok(());
        };
        let source = |position, velocity, min_distance, max_distance| Source {
            position,
            velocity,
            min_distance,
            max_distance,
        };
        let apply = |mixer: &mut Mixer, id: Option<VoiceId>, level: Option<([f32; 2], f32)>| {
            if let Some(id) = id {
                match level {
                    Some((gains, rate)) => {
                        mixer.set_gains(id, gains);
                        if rate.is_finite() && rate > 0. {
                            mixer.set_rate(id, rate.min(16.));
                        }
                    }
                    None => mixer.set_gains(id, [0., 0.]),
                }
            }
        };
        let command = spatial_audio::command(
            source(rider, self.motion.velocity, qs::VOCAL_MIN, qs::VOCAL_MAX),
            std::slice::from_ref(&listener),
        )?;
        apply(mixer, self.vocal_voice, gains(&command, 100, 1.));
        if let Some((position, velocity, volume)) = self.chassis_source {
            let command = spatial_audio::command(
                source(position, velocity, qs::CHASSIS_MIN, qs::CHASSIS_MAX),
                std::slice::from_ref(&listener),
            )?;
            apply(mixer, self.chassis_voice, gains(&command, volume, 1.));
        } else {
            apply(mixer, self.chassis_voice, None);
        }
        for side in 0..2 {
            let level = self.drive_level[side].and_then(|(record, pitch, volume)| {
                let sound = self.bank.drive.get(record - 1)?;
                let rate = *self.bank.rates.get(&normal(&sound.sample))? as f32;
                if sound.max_distance <= sound.min_distance || rate <= 0. {
                    return None;
                }
                let command = spatial_audio::command(
                    source(
                        wheels[side],
                        [0.; 3],
                        sound.min_distance,
                        sound.max_distance,
                    ),
                    std::slice::from_ref(&listener),
                )
                .ok()?;
                // SPU pitch 4096 plays at 48 kHz.
                gains(&command, volume, pitch as f32 * 48000. / 4096. / rate)
            });
            apply(mixer, self.drive_voices[side], level);
        }
        self.world_frame(mixer, &listener)?;
        self.engine_frame(mixer, step)
    }

    fn world_frame(&mut self, mixer: &mut Mixer, listener: &Listener) -> Result<()> {
        let dt = std::mem::take(&mut self.world_dt);
        let Some(world) = &mut self.world else {
            return Ok(());
        };
        if dt <= 0. {
            return Ok(());
        }
        let ambient: Vec<bool> = (0..world.ambient.len())
            .map(|i| mixer.playing(self.ambient_voices.get(&i).copied()))
            .collect();
        let soultree: Vec<bool> = (0..world.soultree.len())
            .map(|i| mixer.playing(self.soultree_voices.get(&i).copied()))
            .collect();
        let frame = world.step(dt.min(0.1), listener, &ambient, &soultree)?;
        for command in frame.ambient {
            if let WorldCommand::Start {
                source,
                sample,
                looping,
            } = command
            {
                if let Some(id) = self.ambient_voices.remove(&source) {
                    mixer.stop(id);
                }
                let looping = if looping {
                    Looping::Markers
                } else {
                    Looping::Once
                };
                // Ambient sounds are unregistered 2D emitters at full volume.
                if let Some(id) = self.start(mixer, &sample, looping, [1., 1.], "ambient") {
                    self.ambient_voices.insert(source, id);
                    self.stats.ambient += 1;
                }
            }
        }
        for command in frame.soultree {
            match command {
                WorldCommand::Stop { source } => {
                    if let Some(id) = self.soultree_voices.remove(&source) {
                        mixer.stop(id);
                    }
                }
                WorldCommand::Start {
                    source,
                    sample,
                    looping,
                } => {
                    if let Some(id) = self.soultree_voices.remove(&source) {
                        mixer.stop(id);
                    }
                    let looping = if looping {
                        Looping::Markers
                    } else {
                        Looping::Once
                    };
                    if let Some(id) = self.start(mixer, &sample, looping, [0., 0.], "soultree") {
                        self.soultree_voices.insert(source, id);
                        self.stats.soultree += 1;
                    }
                }
            }
        }
        for (index, command) in frame.spatial {
            if let Some(&id) = self.soultree_voices.get(&index) {
                if command.pitch < -1 || command.pitch >= 0x4000 {
                    mixer.set_gains(id, [0., 0.]);
                } else {
                    mixer.set_gains(
                        id,
                        [command.left as f32 / 100., command.right as f32 / 100.],
                    );
                    mixer.set_rate(id, command.pitch.max(0) as f32 / 4096.);
                }
            }
        }
        Ok(())
    }

    /// Engine sample selection (op 33) and playback (GameAudio.play).
    fn engine_frame(&mut self, mixer: &mut Mixer, step: &AudioStep) -> Result<()> {
        self.engine_voices.retain(|id| mixer.playing(Some(*id)));
        let idle = &self.bank.engine.samples[EngineSound::Idle as usize];
        if self.current.is_none() && self.commands == 0 && !idle.is_empty() {
            // 002cae30 loads and selects the last idle sample before updates.
            let name = idle.last().unwrap().clone();
            self.play_engine(mixer, &name, EngineSound::Idle, false);
        }
        let elapsed = std::mem::take(&mut self.elapsed);
        let (duration, playhead) = match self.current {
            Some(voice) => (voice.duration, (mixer.seconds() - voice.started) as f32),
            None => (0., 0.),
        };
        let command = self.engine.step(
            &self.bank.engine,
            elapsed.min(0.1),
            step.throttle,
            step.grounded,
            step.speed,
            PlaybackPosition { duration, playhead },
        )?;
        if let Some(command) = command {
            let sound = command.sound as usize;
            let count = self.bank.engine.samples[sound].len();
            if count > 0 {
                let choice = if matches!(sound, 2..=6) {
                    let random = if count >= 2 {
                        self.engine_random.next_u31()
                    } else {
                        0
                    };
                    sample_index(count, &mut self.engine_previous[sound], random)?
                } else {
                    0
                };
                let name = self.bank.engine.samples[sound][choice].clone();
                self.play_engine(mixer, &name, command.sound, command.overlap);
            }
        }
        Ok(())
    }

    fn play_engine(&mut self, mixer: &mut Mixer, name: &str, sound: EngineSound, overlap: bool) {
        let Some(pcm) = self.bank.get(name).cloned() else {
            return;
        };
        if std::env::var_os("FURY_AUDIO_DEBUG").is_some() {
            eprintln!(
                "engine {:.3} {sound:?} overlap={overlap} {name}",
                mixer.seconds()
            );
        }
        let enhanced = self.mode == SoundMode::Enhanced;
        // Enhanced crossfades the previous engine voice; Original cuts it.
        let crossfade = enhanced && !overlap && !self.engine_voices.is_empty();
        if !overlap {
            for id in self.engine_voices.drain(..) {
                if crossfade {
                    mixer.fade_out(id);
                } else {
                    mixer.stop(id);
                }
            }
            self.current = None;
        }
        // Original idle load flags include 0x80 (looping, whole buffer
        // unless the VAG marks a loop).
        let looping = if sound == EngineSound::Idle {
            Looping::Markers
        } else {
            Looping::Once
        };
        let duration = pcm.duration();
        let id = mixer.play(&pcm, looping, [1., 1.], 1., Bus::Sfx, crossfade, "engine");
        self.engine_voices.push(id);
        self.current = Some(EngineVoice {
            duration,
            started: mixer.seconds(),
        });
        self.commands += 1;
        self.stats.engine_commands += 1;
        self.stats.last_engine = Some(sound);
    }
}
