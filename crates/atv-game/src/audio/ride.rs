//! The ride's sound decisions, ported from GameAudio.update (web/audio.js),
//! QuadSounds (web/quad-sounds.js) and atv-web's operations 33 (engine),
//! 77 (rider body impacts) and 173 (vocals, chassis, drive-on and world
//! sounds). atv-core chooses every sample, volume and pitch; this module only
//! owns the mixer voices. Steps arrive at the fixed 60 Hz; `frame` runs once
//! per rendered frame like the browser's per-batch update.
use super::{
    bank::{Bank, normal},
    mixer::{Bus, EchoPreset, Looping, Mixer, Pcm, VoiceId},
};
use crate::{Result, ride::sound::AudioStep};
use atv_core::{
    engine_audio::{EngineAudioState, EngineSound, OriginalRandom, PlaybackPosition, sample_index},
    quad_sounds::{self as qs, DriveCommand, Voice},
    sound_effects::{ImpactInput, RiderImpactState, RiderMotionCache},
    spatial_audio::{self, CameraPose, Command, Listener, Source},
    world_sounds::{World, WorldCommand},
};
use std::{collections::HashMap, sync::Arc};

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
    /// Motorcycle cues: the last impact's step and the clutch as last heard.
    bike_impact_tick: Option<u64>,
    clutch_down: bool,
    /// Counters for diagnostics and parity reports.
    pub stats: Stats,
    /// Lamp/gate one-shots, the stadium crowd and the IntOut stingers.
    pub race: super::race::RaceAudio,
    /// ENV08's train car sounds (loadSpatial/updateSpatial) and their voices.
    pub train_sounds: Vec<spatial_audio::Definition>,
    train_voices: HashMap<usize, VoiceId>,
    /// The keyframed actors' loop sounds (`Actors::sounds`) and voices.
    pub actor_sounds: Vec<spatial_audio::Definition>,
    actor_voices: HashMap<usize, VoiceId>,
    /// Active Rain weather: `rain.vag` loops on the effects bus (playWeather).
    pub rain: bool,
    rain_voice: Option<VoiceId>,
    /// Split screen: players 2-4's camera listeners. Player 1's positional
    /// sounds (rider vocals, chassis, drive-on, body impacts, Soultree,
    /// trains and actors) are placed for the nearest of player 1's camera
    /// and these (`001742f0`); empty with one view, which is unchanged.
    pub views: Vec<Listener>,
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
    pub train: u32,
    pub actor: u32,
    pub rain: u32,
    pub history: Vec<String>,
    /// The last frame's listener count and the index (0 = player 1's
    /// camera) nearest player 1's rider, for split-screen diagnostics.
    pub listeners: usize,
    pub nearest: usize,
}

/// Body-attached sounds, their voices, emitter matrices (now, one step
/// before) and the mixer label.
type SpatialList<'a> = (
    &'a [spatial_audio::Definition],
    &'a mut HashMap<usize, VoiceId>,
    &'a [([f32; 16], [f32; 16])],
    &'static str,
);

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
            bike_impact_tick: None,
            clutch_down: false,
            stats: Stats::default(),
            race: Default::default(),
            train_sounds: Vec::new(),
            train_voices: HashMap::new(),
            actor_sounds: Vec::new(),
            actor_voices: HashMap::new(),
            rain: false,
            rain_voice: None,
            views: Vec::new(),
        }
    }

    /// updateSpatial for one step: each train car's sound starts (looping
    /// where authored) and follows the car with the original stereo,
    /// attenuation and Doppler commands; a finished one-shot starts again.
    /// The keyframed actors' loop sounds (blimps, airliners, planes, the
    /// helicopter) follow their attached node the same way.
    fn train_step(
        &mut self,
        mixer: &mut Mixer,
        step: &AudioStep,
        camera: &CameraPose,
        previous: &CameraPose,
    ) -> Result<()> {
        let definitions = std::mem::take(&mut self.train_sounds);
        let mut voices = std::mem::take(&mut self.train_voices);
        let started = self.spatial_step(
            mixer,
            (&definitions, &mut voices, &step.train_cars, "train"),
            camera,
            previous,
        );
        (self.train_sounds, self.train_voices) = (definitions, voices);
        self.stats.train += started?;
        let definitions = std::mem::take(&mut self.actor_sounds);
        let mut voices = std::mem::take(&mut self.actor_voices);
        let started = self.spatial_step(
            mixer,
            (&definitions, &mut voices, &step.actor_emitters, "actor"),
            camera,
            previous,
        );
        (self.actor_sounds, self.actor_voices) = (definitions, voices);
        self.stats.actor += started?;
        Ok(())
    }

    /// One list of body-attached sounds: returns how many voices started.
    fn spatial_step(
        &mut self,
        mixer: &mut Mixer,
        (definitions, voices, cars, label): SpatialList<'_>,
        camera: &CameraPose,
        previous: &CameraPose,
    ) -> Result<u32> {
        if definitions.is_empty() || cars.is_empty() {
            return Ok(0);
        }
        let commands = spatial_audio::scene_commands(spatial_audio::SceneInput {
            dt: crate::ride::STEP,
            camera: camera.clone(),
            previous_camera: previous.clone(),
            cars: cars.iter().map(|c| c.0).collect(),
            previous_cars: cars.iter().map(|c| c.1).collect(),
            definitions: definitions.to_vec(),
            views: self.views.clone(),
        })?;
        let mut started = 0;
        for (index, command) in commands.iter().enumerate() {
            let playing = mixer.playing(voices.get(&index).copied());
            if !playing {
                voices.remove(&index);
                let definition = &definitions[index];
                if !definition.start_disabled {
                    let looping = if definition.looping {
                        Looping::Markers
                    } else {
                        Looping::Once
                    };
                    if let Some(id) =
                        self.start(mixer, &definition.sample, looping, [0., 0.], label)
                    {
                        voices.insert(index, id);
                        started += 1;
                    }
                }
            }
            // The original setter keeps the last parameters on an invalid pitch.
            if let Some(&id) = voices.get(&index)
                && !(command.pitch < -1 || command.pitch >= 0x4000)
            {
                mixer.set_gains(
                    id,
                    [command.left as f32 / 100., command.right as f32 / 100.],
                );
                mixer.set_rate(id, command.pitch.max(0) as f32 / 4096.);
            }
        }
        Ok(started)
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
        Some(self.start_pcm(mixer, &pcm, sample, looping, gains, label))
    }

    fn start_pcm(
        &mut self,
        mixer: &mut Mixer,
        pcm: &Arc<Pcm>,
        sample: &str,
        looping: Looping,
        gains: [f32; 2],
        label: &'static str,
    ) -> VoiceId {
        self.stats.history.push(normal(sample));
        if self.stats.history.len() > 16 {
            self.stats.history.remove(0);
        }
        mixer.play(pcm, looping, gains, 1., Bus::Sfx, false, label)
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
        self.train_step(mixer, step, &camera, &previous)?;
        // A motorcycle has no rider effects, vocals, chassis or drive-on
        // sounds (web/audio.js skips QuadSounds and loadEffects for it).
        if step.bike.is_some() {
            self.last = Some((listener, step.rider_position, step.quad.wheels));
            return Ok(());
        }
        let listeners = self.listeners(listener);
        self.body_impact(mixer, step, &listeners)?;
        self.quad_step(mixer, step, &listeners)?;
        Ok(())
    }

    /// Player 1's camera listener, then the other views' (split screen).
    fn listeners(&self, own: Listener) -> Vec<Listener> {
        std::iter::once(own)
            .chain(self.views.iter().take(7).copied())
            .collect()
    }

    /// Rider body impacts (op 77 per frame).
    fn body_impact(
        &mut self,
        mixer: &mut Mixer,
        step: &AudioStep,
        listeners: &[Listener],
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
            listeners,
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
        listeners: &[Listener],
    ) -> Result<()> {
        let distance = |p: [f32; 3]| {
            let listener = &listeners[spatial_audio::nearest(p, listeners)];
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
            if let Some(pcm) = self.bank.vocal(&cue.sample)? {
                if let Some(id) = self.vocal_voice.take() {
                    mixer.stop(id);
                }
                self.vocal_voice = Some(self.start_pcm(
                    mixer,
                    &pcm,
                    &cue.sample,
                    Looping::Once,
                    [0., 0.],
                    "vocal",
                ));
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
        self.last = Some((listeners[0], position, frame.wheels));
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
        let listeners = self.listeners(listener);
        self.stats.listeners = listeners.len();
        self.stats.nearest = spatial_audio::nearest(rider, &listeners);
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
            &listeners,
        )?;
        apply(mixer, self.vocal_voice, gains(&command, 100, 1.));
        if let Some((position, velocity, volume)) = self.chassis_source {
            let command = spatial_audio::command(
                source(position, velocity, qs::CHASSIS_MIN, qs::CHASSIS_MAX),
                &listeners,
            )?;
            apply(mixer, self.chassis_voice, gains(&command, volume, 1.));
        } else {
            apply(mixer, self.chassis_voice, None);
        }
        for (side, wheel) in wheels.iter().enumerate() {
            let level = self.drive_level[side].and_then(|(record, pitch, volume)| {
                let sound = self.bank.drive.get(record - 1)?;
                let rate = *self.bank.rates.get(&normal(&sound.sample))? as f32;
                if sound.max_distance <= sound.min_distance || rate <= 0. {
                    return None;
                }
                let command = spatial_audio::command(
                    source(*wheel, [0.; 3], sound.min_distance, sound.max_distance),
                    &listeners,
                )
                .ok()?;
                // SPU pitch 4096 plays at 48 kHz.
                gains(&command, volume, pitch as f32 * 48000. / 4096. / rate)
            });
            apply(mixer, self.drive_voices[side], level);
        }
        if let Some(bike) = &step.bike {
            self.bike_frame(mixer, bike);
        }
        // playWeather: the rain loop starts once and keeps looping.
        if self.rain && !mixer.playing(self.rain_voice) {
            self.rain_voice = self.start(mixer, "rain.vag", Looping::Markers, [1., 1.], "rain");
            if self.rain_voice.is_some() {
                self.stats.rain += 1;
            }
        }
        self.world_frame(mixer, &listeners)?;
        self.engine_frame(mixer, step)
    }

    /// GameAudio.update's motorcycle branch: one wreck impact per step and
    /// the clutch sample on each press, unpositioned on the effects bus.
    fn bike_frame(&mut self, mixer: &mut Mixer, bike: &crate::ride::sound::BikeSound) {
        if let Some(sample) = &bike.impact_sample
            && self.bike_impact_tick != Some(bike.tick)
        {
            self.bike_impact_tick = Some(bike.tick);
            self.start(mixer, sample, Looping::Once, [1., 1.], "impact");
            self.stats.impacts += 1;
        }
        if bike.clutch && !self.clutch_down {
            // samples[9]: the MX Clutch bank (EngineSound::Reset).
            if let Some(sample) = self.bank.engine.samples[9].first().cloned() {
                self.start(mixer, &sample, Looping::Once, [1., 1.], "clutch");
            }
        }
        self.clutch_down = bike.clutch;
    }

    fn world_frame(&mut self, mixer: &mut Mixer, listeners: &[Listener]) -> Result<()> {
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
        let frame = world.step(dt.min(0.1), listeners, &ambient, &soultree)?;
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

#[cfg(test)]
mod tests {
    //! web/audio.test.mjs: train body loops, the rain loop and the rider
    //! impact guard, on the shared mixer.
    use super::*;
    use crate::ride::sound::{AudioStep, QuadFrame};
    use atv_core::math::Det;

    fn tone(seconds: f32, markers: Option<(f64, f64)>) -> Arc<Pcm> {
        let frames = (seconds * 48000.) as usize;
        Arc::new(Pcm {
            channels: 1,
            frames,
            data: (0..frames)
                .map(|i| ((i as f32) * 0.05).sin_d() * 0.5)
                .collect(),
            loop_markers: markers,
            rate: 48000,
        })
    }

    fn pose(x: f32, z: f32) -> [f32; 16] {
        [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, 0., z, 1.]
    }

    fn step(placement: u32, impact: f32, riding: bool, train: bool) -> AudioStep {
        let cars = [pose(-20., 20.), pose(20., 20.)];
        AudioStep {
            quad: QuadFrame::default(),
            rider_position: [0., 0., 10.],
            rider_impact: [impact, 0., 0.],
            rider_water: false,
            riding,
            throttle: 0.,
            grounded: true,
            speed: 0.,
            camera: CameraPose {
                position: [0., 0., 0.],
                target: [0., 0., 1.],
            },
            placement,
            bike: None,
            train_cars: if train {
                cars.iter().map(|c| (*c, *c)).collect()
            } else {
                Vec::new()
            },
            actor_emitters: Vec::new(),
        }
    }

    fn render(mixer: &mut Mixer, seconds: f32) -> Vec<f32> {
        let mut out = vec![0.; 2 * (seconds * 48000.) as usize];
        mixer.render(&mut out);
        out
    }

    fn voices(mixer: &Mixer, label: &str) -> Vec<(f32, [f32; 2])> {
        mixer
            .snapshot()
            .into_iter()
            .filter(|v| v.0 == label)
            .map(|v| (v.2, v.3))
            .collect()
    }

    #[test]
    fn train_loops_keep_their_loop_points_and_pan_through_reset_and_pause() {
        let bank =
            super::super::bank::test_bank(&[("train", tone(0.1, Some((480., 2400.))))], None);
        let mut audio = RideAudio::new(bank, SoundMode::Original);
        audio.train_sounds = (0..2)
            .map(|car| spatial_audio::Definition {
                car,
                sample: "train".into(),
                min_distance: 150.,
                max_distance: 350.,
                looping: true,
                start_disabled: false,
                position: None,
            })
            .collect();
        let mut mixer = Mixer::new(48000);
        audio.step(&mut mixer, &step(0, 0., true, true)).unwrap();
        assert_eq!(audio.stats.train, 2);
        let cars = voices(&mixer, "train");
        assert_eq!(cars.len(), 2);
        // Stationary cars play at their own pitch, panned to their sides.
        for (rate, _) in &cars {
            assert!((rate - 1.).abs() < 1e-4, "{rate}");
        }
        let ([l0, r0], [l1, r1]) = (cars[0].1, cars[1].1);
        assert!((l0 - r1).abs() < 1e-6 && (r0 - l1).abs() < 1e-6, "{cars:?}");
        assert!((l0 - r0).abs() > 0.05, "{cars:?}");
        // Past the clip's end the loop runs between the VAG markers.
        render(&mut mixer, 0.3);
        assert_eq!(voices(&mixer, "train").len(), 2);
        audio.step(&mut mixer, &step(0, 0., true, true)).unwrap();
        // Reset ATV keeps the cars' voices (GameAudio.reset leaves them).
        audio.step(&mut mixer, &step(1, 0., true, true)).unwrap();
        assert_eq!(audio.stats.train, 2, "no voice restarts");
        assert_eq!(voices(&mixer, "train").len(), 2);
        // Pause mutes the effects bus (the engine gain at 0) and keeps the
        // voices, which carry on when the ride resumes.
        mixer.active = false;
        assert!(render(&mut mixer, 0.05).iter().all(|v| *v == 0.));
        assert_eq!(voices(&mixer, "train").len(), 2);
        mixer.active = true;
        audio.step(&mut mixer, &step(1, 0., true, true)).unwrap();
        assert_eq!(audio.stats.train, 2);
    }

    #[test]
    fn actor_loops_follow_their_emitters_and_fade_out_of_range() {
        let bank =
            super::super::bank::test_bank(&[("blimp", tone(0.1, Some((480., 2400.))))], None);
        let mut audio = RideAudio::new(bank, SoundMode::Original);
        audio.actor_sounds = vec![spatial_audio::Definition {
            car: 0,
            sample: "blimp".into(),
            min_distance: 55.,
            max_distance: 850.,
            looping: true,
            start_disabled: false,
            position: None,
        }];
        let mut mixer = Mixer::new(48000);
        let mut frame = step(0, 0., true, false);
        // A blimp node 100 ft away to the left of the camera.
        frame.actor_emitters = vec![(pose(-100., 20.), pose(-100., 20.))];
        audio.step(&mut mixer, &frame).unwrap();
        assert_eq!((audio.stats.actor, audio.stats.train), (1, 0));
        let [(rate, [left, right])] = voices(&mixer, "actor")[..] else {
            panic!("one actor voice");
        };
        assert!(
            (rate - 1.).abs() < 1e-4 && (left - right).abs() > 0.05,
            "{left} {right}"
        );
        // Beyond MaxDistance the same looping voice falls silent.
        frame.actor_emitters = vec![(pose(-2000., 20.), pose(-2000., 20.))];
        audio.step(&mut mixer, &frame).unwrap();
        let [(_, gains)] = voices(&mixer, "actor")[..] else {
            panic!("one actor voice");
        };
        assert_eq!(gains, [0., 0.]);
        assert_eq!(audio.stats.actor, 1, "the loop is not restarted");
        // No emitters (no actors on this route): nothing starts.
        let mut quiet = RideAudio::new(
            super::super::bank::test_bank(&[("blimp", tone(0.1, None))], None),
            SoundMode::Original,
        );
        quiet.actor_sounds = audio.actor_sounds.clone();
        quiet
            .step(&mut Mixer::new(48000), &step(0, 0., true, false))
            .unwrap();
        assert_eq!(quiet.stats.actor, 0);
    }

    #[test]
    fn rain_keeps_its_phase_through_reset_and_is_silent_while_paused() {
        let bank =
            super::super::bank::test_bank(&[("rain.vag", tone(0.2, Some((0., 9600.))))], None);
        let mut audio = RideAudio::new(bank, SoundMode::Original);
        audio.rain = true;
        let mut mixer = Mixer::new(48000);
        let first = step(0, 0., true, false);
        audio.step(&mut mixer, &first).unwrap();
        audio.frame(&mut mixer, Some(&first)).unwrap();
        assert_eq!(audio.stats.rain, 1);
        let rain = audio.rain_voice.unwrap();
        render(&mut mixer, 0.15);
        let before = mixer.elapsed(rain).unwrap();
        let reset = step(1, 0., true, false);
        audio.step(&mut mixer, &reset).unwrap();
        audio.frame(&mut mixer, Some(&reset)).unwrap();
        assert_eq!(audio.rain_voice, Some(rain), "one rain voice through Reset");
        assert_eq!(audio.stats.rain, 1);
        render(&mut mixer, 0.15);
        let after = mixer.elapsed(rain).unwrap();
        assert!(after > before, "the loop continues: {before} -> {after}");
        // Paused: silent, the same voice resumes.
        mixer.active = false;
        assert!(render(&mut mixer, 0.05).iter().all(|v| *v == 0.));
        assert!(mixer.playing(Some(rain)));
        mixer.active = true;
        audio.frame(&mut mixer, Some(&reset)).unwrap();
        assert_eq!(audio.stats.rain, 1);
    }

    #[test]
    fn rider_impacts_obey_the_active_voice_guard_and_stop_on_reset() {
        let config = atv_core::sound_effects::Config {
            body_impact: vec!["body1".into(), "body2".into()],
            min_distance: 30.,
            max_distance: 100.,
        };
        let bank = super::super::bank::test_bank(
            &[("body1", tone(0.25, None)), ("body2", tone(0.25, None))],
            Some(config),
        );
        let mut audio = RideAudio::new(bank, SoundMode::Original);
        let mut mixer = Mixer::new(48000);
        audio.step(&mut mixer, &step(0, 3., false, false)).unwrap();
        assert_eq!(audio.stats.impacts, 1);
        let (rate, _) = voices(&mixer, "impact")[0];
        assert!(rate > 0., "spatial pitch applies");
        // A hard contact while the impact still plays starts nothing.
        audio.step(&mut mixer, &step(0, 3., false, false)).unwrap();
        audio.step(&mut mixer, &step(0, 0., false, false)).unwrap();
        assert_eq!(audio.stats.impacts, 1);
        assert_eq!(voices(&mixer, "impact").len(), 1);
        // Once it ends, the next hard contact plays again.
        render(&mut mixer, 0.3);
        assert!(voices(&mixer, "impact").is_empty());
        audio.step(&mut mixer, &step(0, 3., false, false)).unwrap();
        assert_eq!(audio.stats.impacts, 2);
        // Reset ATV stops the rider effect voice.
        audio.step(&mut mixer, &step(1, 0., true, false)).unwrap();
        assert!(voices(&mixer, "impact").is_empty());
        assert!(audio.effect_voice.is_none());
    }
}
