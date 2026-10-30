//! Race CPU riders' rider and quad emitters besides the engine (which
//! plays through [`super::remote`]): rider vocals, the rider body impact
//! and the quad chassis sounds. The original builds these emitters for
//! every rider object, CPUs included (`002d6380`: vocals `002cc430`/
//! `002cc640`, body impact `002cce90`/`002ccf90`; `001e33c0`: chassis
//! `002cd4e0`/`002cd6c0`), so each CPU runs the same atv-core decisions as
//! the player's [`super::ride::RideAudio`] over its own fixed steps, with
//! its own random streams and one voice per emitter. Positions are heard
//! from the nearest listener (`001742f0`), so split screen's views apply.
use super::{
    bank::Bank,
    mixer::{Bus, Looping, Mixer, VoiceId},
};
use crate::{
    Result,
    ride::cpu::{CpuSound, CpuSoundStep},
};
use atv_core::{
    engine_audio::OriginalRandom,
    quad_sounds::{self as qs, Voice},
    sound_effects::{ImpactInput, RiderImpactState, RiderMotionCache},
    spatial_audio::{self, Command, Listener, Source},
};
use std::collections::BTreeMap;

/// Each CPU's emitters (one voice each, as the original's).
struct Emitters {
    female: bool,
    vocals: Option<qs::RiderVocals>,
    vocal_voice: Option<VoiceId>,
    motion: RiderMotionCache,
    impact: RiderImpactState,
    impact_random: OriginalRandom,
    impact_voice: Option<VoiceId>,
    chassis: Option<qs::Chassis>,
    chassis_voice: Option<VoiceId>,
    /// `002cdb80` sets the chassis voice position only when a clip starts.
    chassis_source: Option<([f32; 3], [f32; 3], i32)>,
    reset_count: Option<u32>,
    /// The latest step's actor sound position and its cached velocity.
    rider: Option<([f32; 3], [f32; 3])>,
}

impl Emitters {
    fn new(bank: &Bank, index: usize, female: bool) -> Self {
        // Each rider object owns its emitters' random streams; the player's
        // use slot 9-12 seeds, so the CPUs take their own (authored).
        let seed = 197_211 + index as u64 * 53;
        let vocals = bank
            .vocal_banks
            .as_ref()
            .map(|b| b.lists(female).clone())
            .filter(|lists| lists.iter().any(|l| !l.is_empty()))
            .map(|lists| qs::RiderVocals::new(lists, seed));
        let chassis = bank
            .chassis
            .clone()
            .map(|banks| qs::Chassis::new(banks, false, seed + 1));
        Self {
            female,
            vocals,
            vocal_voice: None,
            motion: RiderMotionCache::default(),
            impact: RiderImpactState::default(),
            impact_random: OriginalRandom::with_seed(seed + 2),
            impact_voice: None,
            chassis,
            chassis_voice: None,
            chassis_source: None,
            reset_count: None,
            rider: None,
        }
    }

    fn stop(&mut self, mixer: &mut Mixer) {
        for id in [
            self.vocal_voice.take(),
            self.impact_voice.take(),
            self.chassis_voice.take(),
        ]
        .into_iter()
        .flatten()
        {
            mixer.stop(id);
        }
    }

    /// A vehicle reset (recovery, respawn) clears the latches and voices,
    /// as the player's reset (ops 78 and 174).
    fn reset(&mut self, mixer: &mut Mixer) {
        self.stop(mixer);
        self.impact = RiderImpactState::default();
        if let Some(c) = &mut self.chassis {
            c.reset();
        }
        if let Some(v) = &mut self.vocals {
            v.reset();
        }
        self.chassis_source = None;
        self.motion = RiderMotionCache::default();
    }
}

/// CPU emitter diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub vocals: u64,
    pub impacts: u64,
    pub chassis: u64,
}

#[derive(Default)]
pub struct CpuAudio {
    riders: BTreeMap<usize, Emitters>,
    pub stats: Stats,
}

fn distance(p: [f32; 3], listeners: &[Listener]) -> f32 {
    let Some(l) = listeners.get(spatial_audio::nearest(p, listeners)) else {
        return f32::MAX;
    };
    let d: [f32; 3] = std::array::from_fn(|i| p[i] - l.position[i]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The emitter's volume times the attenuation into stereo gains and rate,
/// None when the pitch is out of range (as the player's `gains`).
fn level(command: &Command, volume: i32) -> Option<([f32; 2], f32)> {
    if command.pitch < -1 || command.pitch >= 0x4000 {
        return None;
    }
    let (left, right) =
        spatial_audio::stereo(volume.clamp(0, 100) * command.volume / 100, command.pan);
    Some((
        [left as f32 / 100., right as f32 / 100.],
        command.pitch.max(0) as f32 / 4096.,
    ))
}

fn apply(mixer: &mut Mixer, id: Option<VoiceId>, level: Option<([f32; 2], f32)>) {
    let Some(id) = id else { return };
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

impl CpuAudio {
    /// Voices playing (diagnostics): vocals, impacts, chassis.
    pub fn voices(&self, mixer: &Mixer) -> [usize; 3] {
        let count = |f: fn(&Emitters) -> Option<VoiceId>| {
            self.riders.values().filter(|e| mixer.playing(f(e))).count()
        };
        [
            count(|e| e.vocal_voice),
            count(|e| e.impact_voice),
            count(|e| e.chassis_voice),
        ]
    }

    pub fn stop_all(&mut self, mixer: &mut Mixer) {
        for (_, mut e) in std::mem::take(&mut self.riders) {
            e.stop(mixer);
        }
    }

    /// One rendered frame: every CPU's steps since the last frame, then the
    /// voices' spatial levels for the nearest listener. `audible` false
    /// (locked output, focus loss) silences them as the room riders.
    pub fn frame(
        &mut self,
        mixer: &mut Mixer,
        bank: &mut Bank,
        listeners: &[Listener],
        cpus: &[CpuSound],
        audible: bool,
    ) -> Result<()> {
        if listeners.is_empty() || !audible {
            self.stop_all(mixer);
            return Ok(());
        }
        let present: Vec<usize> = cpus.iter().map(|c| c.index).collect();
        self.riders.retain(|index, e| {
            let keep = present.contains(index);
            if !keep {
                e.stop(mixer);
            }
            keep
        });
        for cpu in cpus {
            let emitters = self
                .riders
                .entry(cpu.index)
                .or_insert_with(|| Emitters::new(bank, cpu.index, cpu.female));
            if emitters.female != cpu.female {
                emitters.stop(mixer);
                *emitters = Emitters::new(bank, cpu.index, cpu.female);
            }
            for step in &cpu.steps {
                Self::step(&mut self.stats, emitters, mixer, bank, step, listeners)?;
            }
            Self::place(emitters, mixer, bank, listeners)?;
        }
        Ok(())
    }

    /// One fixed step of the vocal, body impact and chassis emitters (the
    /// player's `quad_step` and `body_impact` order).
    fn step(
        stats: &mut Stats,
        e: &mut Emitters,
        mixer: &mut Mixer,
        bank: &mut Bank,
        step: &CpuSoundStep,
        listeners: &[Listener],
    ) -> Result<()> {
        if e.reset_count
            .replace(step.reset_count)
            .is_some_and(|c| c != step.reset_count)
        {
            e.reset(mixer);
        }
        let position = step.rider_position;
        let velocity = e.motion.update(position, 1. / crate::ride::STEP)?;
        e.rider = Some((position, velocity));
        // Rider body impact (op 77).
        if let Some(config) = &bank.effects
            && !config.body_impact.is_empty()
        {
            let playing = mixer.playing(e.impact_voice);
            let random = &mut e.impact_random;
            let event = e.impact.advance(
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
                && let Some(pcm) = bank.get(&config.body_impact[event.sample]).cloned()
            {
                if let Some(old) = e.impact_voice.take() {
                    mixer.stop(old);
                }
                e.impact_voice = Some(mixer.play(
                    &pcm,
                    Looping::Once,
                    [0., 0.],
                    1.,
                    Bus::Sfx,
                    false,
                    "cpu impact",
                ));
                stats.impacts += 1;
            }
        }
        // Vocals and chassis (op 173).
        let mut vocal = Voice {
            playing: mixer.playing(e.vocal_voice),
            in_range: distance(position, listeners) <= qs::VOCAL_MAX,
        };
        let mut chassis = Voice {
            playing: mixer.playing(e.chassis_voice),
            in_range: distance(step.origin, listeners) <= qs::CHASSIS_MAX,
        };
        let events = step.events;
        let mut vocal_cue = None;
        let mut chassis_cue = None;
        if events.quad_contact > 1. {
            if let Some(v) = &mut e.vocals {
                vocal_cue = v.request(qs::event::BANG, events.quad_contact, &mut vocal);
            }
            if let Some(c) = &mut e.chassis {
                chassis_cue = c.quad_contact(&mut chassis);
            }
        }
        if let Some(v) = &mut e.vocals {
            if let Some((event, magnitude)) = events.wreck_entry.and_then(qs::WreckEntry::event) {
                vocal_cue = v.request(event, magnitude, &mut vocal).or(vocal_cue.take());
            } else if events.hard_landing {
                vocal_cue = v
                    .request(qs::event::JOSTLE, 0., &mut vocal)
                    .or(vocal_cue.take());
            }
        }
        if let Some(c) = &mut e.chassis {
            let (cue, _) = c.update(step.chassis, &mut chassis);
            chassis_cue = cue.or(chassis_cue.take());
        }
        if let Some(cue) = &chassis_cue {
            e.chassis_source = Some((step.origin, step.velocity, cue.volume));
        }
        if let Some(v) = &mut e.vocals {
            let i = step.rider_impact;
            let magnitude = ((i[0] * i[0] + i[1] * i[1]) + i[2] * i[2]).sqrt();
            vocal_cue = v
                .update(magnitude, step.chassis.wrecked, &mut vocal)
                .or(vocal_cue.take());
        }
        if let Some(cue) = vocal_cue {
            if cue.stop_current
                && let Some(id) = e.vocal_voice.take()
            {
                mixer.stop(id);
            }
            if let Some(pcm) = bank.vocal(&cue.sample)? {
                if let Some(id) = e.vocal_voice.take() {
                    mixer.stop(id);
                }
                e.vocal_voice = Some(mixer.play(
                    &pcm,
                    Looping::Once,
                    [0., 0.],
                    1.,
                    Bus::Sfx,
                    false,
                    "cpu vocal",
                ));
                stats.vocals += 1;
            }
        }
        if let Some(cue) = chassis_cue
            && let Some(pcm) = bank.get(&cue.sample).cloned()
        {
            if let Some(id) = e.chassis_voice.take() {
                mixer.stop(id);
            }
            e.chassis_voice = Some(mixer.play(
                &pcm,
                Looping::Once,
                [0., 0.],
                1.,
                Bus::Sfx,
                false,
                "cpu chassis",
            ));
            stats.chassis += 1;
        }
        Ok(())
    }

    /// The voices' levels from the latest step (the player's `frame`).
    fn place(
        e: &mut Emitters,
        mixer: &mut Mixer,
        bank: &Bank,
        listeners: &[Listener],
    ) -> Result<()> {
        for slot in [
            &mut e.vocal_voice,
            &mut e.impact_voice,
            &mut e.chassis_voice,
        ] {
            if slot.is_some_and(|id| !mixer.playing(Some(id))) {
                *slot = None;
            }
        }
        let source = |position, velocity, min_distance, max_distance| Source {
            position,
            velocity,
            min_distance,
            max_distance,
        };
        if let Some((position, velocity)) = e.rider {
            let command = spatial_audio::command(
                source(position, velocity, qs::VOCAL_MIN, qs::VOCAL_MAX),
                listeners,
            )?;
            apply(mixer, e.vocal_voice, level(&command, 100));
            if let Some(config) = &bank.effects {
                let command = spatial_audio::command(
                    source(position, velocity, config.min_distance, config.max_distance),
                    listeners,
                )?;
                apply(mixer, e.impact_voice, level(&command, 100));
            }
        }
        match e.chassis_source {
            Some((position, velocity, volume)) => {
                let command = spatial_audio::command(
                    source(position, velocity, qs::CHASSIS_MIN, qs::CHASSIS_MAX),
                    listeners,
                )?;
                apply(mixer, e.chassis_voice, level(&command, volume));
            }
            None => apply(mixer, e.chassis_voice, None),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{bank::test_bank, mixer::Pcm};
    use atv_core::{quad_sounds::WreckEntry, vehicle::AudioEvents};
    use std::sync::Arc;

    fn pcm() -> Arc<Pcm> {
        Arc::new(Pcm {
            channels: 1,
            frames: 24_000,
            data: vec![0.25; 24_000],
            loop_markers: None,
            rate: 48_000,
        })
    }

    fn bank() -> Bank {
        let mut bank = test_bank(&[("impact/landing.vag", pcm()), ("mhurt01", pcm())], None);
        let lists: [Vec<String>; 6] = std::array::from_fn(|_| vec!["mhurt01".to_string()]);
        bank.vocal_banks = Some(qs::VocalBanks {
            female: Default::default(),
            male: lists,
        });
        bank.chassis = Some(qs::ChassisBanks {
            quad_to_quad: Vec::new(),
            wreck: Vec::new(),
        });
        bank
    }

    fn listener(x: f32) -> Listener {
        Listener {
            position: [x, 0., 0.],
            forward: [0., 0., 1.],
            velocity: [0.; 3],
        }
    }

    fn cpu(steps: Vec<CpuSoundStep>) -> CpuSound {
        CpuSound {
            index: 0,
            quad: 1,
            female: false,
            steps,
            ..Default::default()
        }
    }

    fn step(origin: [f32; 3]) -> CpuSoundStep {
        CpuSoundStep {
            origin,
            rider_position: origin,
            riding: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_cpu_wreck_plays_its_vocal_within_the_cull_distance_only() {
        let mut mixer = Mixer::new(48_000);
        let mut bank = bank();
        let mut audio = CpuAudio::default();
        let wreck = |origin| CpuSoundStep {
            events: AudioEvents {
                wreck_entry: Some(WreckEntry::classify(0., false, 25.)),
                ..Default::default()
            },
            ..step(origin)
        };
        // 150ft from the only listener: beyond the 100ft vocal cull.
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(0.)],
                &[cpu(vec![wreck([150., 0., 0.])])],
                true,
            )
            .unwrap();
        assert_eq!(audio.stats.vocals, 0);
        // Split screen: a second view 20ft away is the nearest listener.
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(0.), listener(130.)],
                &[cpu(vec![wreck([150., 0., 0.])])],
                true,
            )
            .unwrap();
        assert_eq!(audio.stats.vocals, 1);
        assert_eq!(audio.voices(&mixer), [1, 0, 0]);
        // One voice: a second request while it plays starts nothing.
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(140.)],
                &[cpu(vec![wreck([150., 0., 0.])])],
                true,
            )
            .unwrap();
        assert_eq!(audio.stats.vocals, 1);
        // A CPU that leaves the race loses its voices.
        audio
            .frame(&mut mixer, &mut bank, &[listener(0.)], &[], true)
            .unwrap();
        assert_eq!(audio.voices(&mixer), [0, 0, 0]);
    }

    #[test]
    fn a_cpu_landing_plays_the_chassis_landing_scaled_by_impact() {
        let mut mixer = Mixer::new(48_000);
        let mut bank = bank();
        let mut audio = CpuAudio::default();
        let mut air = step([10., 0., 0.]);
        air.chassis.airborne = true;
        let mut land = step([10., 0., 0.]);
        land.chassis.impact = 30.;
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(0.)],
                &[cpu(vec![air, land])],
                true,
            )
            .unwrap();
        assert_eq!(audio.stats.chassis, 1);
        assert_eq!(audio.voices(&mixer), [0, 0, 1]);
        // Silenced output stops every CPU voice.
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(0.)],
                &[cpu(vec![])],
                false,
            )
            .unwrap();
        assert_eq!(audio.voices(&mixer), [0, 0, 0]);
    }

    #[test]
    fn a_cpu_reset_stops_its_voices() {
        let mut mixer = Mixer::new(48_000);
        let mut bank = bank();
        let mut audio = CpuAudio::default();
        let mut air = step([10., 0., 0.]);
        air.chassis.airborne = true;
        let mut land = step([10., 0., 0.]);
        land.chassis.impact = 30.;
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(0.)],
                &[cpu(vec![air, land])],
                true,
            )
            .unwrap();
        assert_eq!(audio.voices(&mixer), [0, 0, 1]);
        let mut reset = step([10., 0., 0.]);
        reset.reset_count = 1;
        audio
            .frame(
                &mut mixer,
                &mut bank,
                &[listener(0.)],
                &[cpu(vec![reset])],
                true,
            )
            .unwrap();
        assert_eq!(audio.voices(&mixer), [0, 0, 0]);
    }
}
