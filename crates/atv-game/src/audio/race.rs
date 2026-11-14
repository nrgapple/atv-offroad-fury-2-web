//! Race sounds (web/audio.js `oneShot`, `crowdCue`, `raceMusic`/`playIntOut`):
//! the 321 lamp and drop-gate one-shots, the stadium crowd's single reaction
//! voice and the IntOut stingers on the music bus. Which cue plays when is
//! decided by the race step (`ride::race::StepEvents`, atv_core::race_crowd).
use super::{
    bank::Bank,
    mixer::{Bus, Looping, Mixer, Pcm, VoiceId},
};
use atv_core::race_crowd::CrowdFrame;
use std::sync::Arc;

#[derive(Default)]
pub struct RaceAudio {
    /// The crowd voice and whether it loops (a Stomp).
    crowd: Option<(VoiceId, bool)>,
    /// IntOut cues waiting for the music to be enabled: (sample, looping).
    pending: Option<Vec<(String, bool)>>,
    /// Started stingers, and the next one with its start time (seconds).
    voices: Vec<VoiceId>,
    queued: Vec<(Arc<Pcm>, bool, f64)>,
    pub stats: RaceStats,
}

#[derive(Clone, Debug, Default)]
pub struct RaceStats {
    pub one_shots: Vec<String>,
    pub crowd_cues: u32,
    pub int_out_starts: u32,
    pub int_out_cues: Vec<String>,
}

impl RaceAudio {
    /// `oneShot`: the bank's sample once on the effects bus.
    pub fn one_shot(&mut self, bank: &Bank, mixer: &mut Mixer, name: &str) {
        let Some(pcm) = bank.get(name) else {
            return;
        };
        mixer.play(pcm, Looping::Once, [1., 1.], 1., Bus::Sfx, false, "race");
        self.stats.one_shots.push(name.to_string());
        if self.stats.one_shots.len() > 16 {
            self.stats.one_shots.remove(0);
        }
    }

    /// `crowdCue` (002cc370): a playing clip keeps the one crowd voice; a
    /// Stomp loop gives way to the next cue.
    pub fn crowd_cue(&mut self, bank: &Bank, mixer: &mut Mixer, frame: &CrowdFrame) {
        let playing = self.crowd.filter(|(id, _)| mixer.playing(Some(*id)));
        if frame.stop_loop
            && let Some((id, true)) = playing
        {
            mixer.stop(id);
            self.crowd = None;
        }
        let Some(cue) = &frame.cue else {
            return;
        };
        if let Some((_, false)) = self.crowd.filter(|(id, _)| mixer.playing(Some(*id))) {
            return;
        }
        let Some(pcm) = bank.get(&cue.sample) else {
            return;
        };
        if let Some((id, _)) = self.crowd.take() {
            mixer.stop(id);
        }
        let looping = if cue.looping {
            Looping::Whole
        } else {
            Looping::Once
        };
        let id = mixer.play(pcm, looping, [1., 1.], 1., Bus::Sfx, false, "crowd");
        self.crowd = Some((id, cue.looping));
        self.stats.crowd_cues += 1;
    }

    /// Queues stingers (`intOutPending`); they start once music plays.
    pub fn set_int_out(&mut self, mixer: &mut Mixer, cues: Vec<(String, bool)>) {
        self.stop_int_out(mixer);
        self.pending = (!cues.is_empty()).then_some(cues);
    }

    pub fn stop_int_out(&mut self, mixer: &mut Mixer) {
        self.pending = None;
        self.queued.clear();
        for id in self.voices.drain(..) {
            mixer.stop(id);
        }
    }

    /// `playIntOut`, every frame: start pending cues back to back (each
    /// next one 1 s before the previous ends) while music is enabled.
    pub fn update(&mut self, bank: &Bank, mixer: &mut Mixer, music_enabled: bool) {
        if music_enabled && let Some(cues) = &self.pending {
            let found: Option<Vec<(Arc<Pcm>, bool)>> = cues
                .iter()
                .map(|(name, looping)| bank.get(name).map(|p| (p.clone(), *looping)))
                .collect();
            if let Some(found) = found {
                self.stats.int_out_cues = cues.iter().map(|(name, _)| name.clone()).collect();
                self.pending = None;
                let mut at = mixer.seconds();
                for (pcm, looping) in found {
                    let duration = pcm.duration() as f64;
                    self.queued.push((pcm, looping, at));
                    at += (duration - 1.).max(0.);
                }
                self.stats.int_out_starts += 1;
            }
        }
        let now = mixer.seconds();
        while let Some((_, _, at)) = self.queued.first()
            && *at <= now
        {
            let (pcm, looping, _) = self.queued.remove(0);
            let looping = if looping {
                Looping::Whole
            } else {
                Looping::Once
            };
            self.voices.push(mixer.play(
                &pcm,
                looping,
                [1., 1.],
                1.,
                Bus::Stinger,
                false,
                "intout",
            ));
        }
    }

    /// The ride ended: silence the crowd and the stingers.
    pub fn stop(&mut self, mixer: &mut Mixer) {
        self.stop_int_out(mixer);
        if let Some((id, _)) = self.crowd.take() {
            mixer.stop(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::bank::test_bank;

    #[test]
    fn race_load_waits_for_music_and_go_cancels_even_pending_cues() {
        let pcm = Arc::new(Pcm {
            channels: 1,
            frames: 48000,
            data: vec![0.5; 48000],
            loop_markers: None,
            rate: 48000,
        });
        let bank = test_bank(&[("intout/int001.vag", pcm)], None);
        let mut mixer = Mixer::new(48000);
        let mut race = RaceAudio::default();
        let cue = || vec![("intout/int001.vag".into(), false)];
        race.set_int_out(&mut mixer, cue());
        race.update(&bank, &mut mixer, false);
        assert_eq!(mixer.voices(), 0);
        race.update(&bank, &mut mixer, true);
        assert_eq!(race.stats.int_out_cues, ["intout/int001.vag"]);
        assert_eq!(race.stats.int_out_starts, 1);
        race.update(&bank, &mut mixer, true);
        assert_eq!(mixer.voices(), 1);
        race.stop_int_out(&mut mixer);
        assert_eq!(mixer.voices(), 0);
        race.set_int_out(&mut mixer, cue());
        race.stop_int_out(&mut mixer);
        race.update(&bank, &mut mixer, true);
        assert_eq!(mixer.voices(), 0);
        assert_eq!(race.stats.int_out_starts, 1);
    }
}
