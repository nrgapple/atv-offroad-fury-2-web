//! Training Course speech (research/TRAINING.md): `DIALOGUE.AUD`'s 46
//! instructor lines from the sound bank, one at a time on the effects bus.
//! Which line plays when is decided by `atv_core::training::Course`; this
//! keeps the lines encoded (about 7 MB) and decodes each when it starts.
use super::{
    bank::normal,
    mixer::{Bus, Looping, Mixer, Pcm, VoiceId},
};
use crate::{Result, archive::Archive};
use atv_content::Content;
use atv_core::{
    audio::decode_vag,
    training::{LINE_COUNT, line_sample},
};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct Speech {
    /// Line number → its VAG.
    encoded: HashMap<u16, Vec<u8>>,
    /// The playing line's serial and voice.
    voice: Option<(u32, VoiceId)>,
    /// Lines started (diagnostics).
    pub started: u32,
    pub last: Option<u16>,
}

/// A mono VAG's length from its header (data bytes / 16 × 28 frames).
pub fn vag_seconds(vag: &[u8]) -> Option<f32> {
    let bytes = u32::from_be_bytes(vag.get(12..16)?.try_into().ok()?);
    let rate = u32::from_be_bytes(vag.get(16..20)?.try_into().ok()?);
    (rate > 0).then(|| (bytes / 16 * 28) as f32 / rate as f32)
}

impl Speech {
    /// Reads the lines the bank has; an older package without them gives
    /// an empty set (the captions then keep their own timing).
    pub async fn load(content: &Content) -> Result<Self> {
        let sounds = Archive::open(content, "/AUDIO/AUDIO.RES").await?;
        let mut encoded = HashMap::new();
        for id in 1..=LINE_COUNT {
            let key = line_sample(id);
            let Some(index) = sounds.members.iter().position(|m| normal(&m.name) == key) else {
                continue;
            };
            encoded.insert(id, sounds.member(index).await?);
            crate::yield_frame().await;
        }
        Ok(Self {
            encoded,
            ..Self::default()
        })
    }

    /// Each available line's spoken length.
    pub fn lengths(&self) -> Vec<(u16, f32)> {
        let mut out: Vec<_> = self
            .encoded
            .iter()
            .filter_map(|(id, vag)| Some((*id, vag_seconds(vag)?)))
            .collect();
        out.sort_unstable_by_key(|(id, _)| *id);
        out
    }

    pub fn lines(&self) -> usize {
        self.encoded.len()
    }

    /// Follows the course's current line (`(serial, line)`): a new serial
    /// stops the previous line and starts this one, `None` (hushed or
    /// skipped) stops it.
    pub fn follow(&mut self, mixer: &mut Mixer, rate: u32, line: Option<(u32, u16)>) {
        if self.voice.map(|(serial, _)| serial) == line.map(|(serial, _)| serial) {
            return;
        }
        if let Some((_, id)) = self.voice.take() {
            mixer.stop(id);
        }
        let Some((serial, id)) = line else {
            return;
        };
        let Some(pcm) = self
            .encoded
            .get(&id)
            .and_then(|vag| decode_vag(vag).ok())
            .map(|audio| Arc::new(Pcm::from_vag(&audio, rate)))
        else {
            return;
        };
        let voice = mixer.play(&pcm, Looping::Once, [1., 1.], 1., Bus::Sfx, false, "speech");
        self.voice = Some((serial, voice));
        self.started += 1;
        self.last = Some(id);
    }

    pub fn playing(&self, mixer: &Mixer) -> bool {
        self.voice.is_some_and(|(_, id)| mixer.playing(Some(id)))
    }

    /// Stops the playing line (leaving the course).
    pub fn stop(&mut self, mixer: &mut Mixer) {
        self.follow(mixer, 0, None);
    }

    #[cfg(test)]
    fn insert(&mut self, id: u16, vag: Vec<u8>) {
        self.encoded.insert(id, vag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mono VAG of `blocks` silent ADPCM blocks at `rate`.
    fn vag(blocks: usize, rate: u32) -> Vec<u8> {
        let mut out = b"VAGp".to_vec();
        out.extend([0, 0, 0, 4, 0, 0, 0, 0]);
        out.extend(((blocks * 16) as u32).to_be_bytes());
        out.extend(rate.to_be_bytes());
        out.resize(48, 0);
        for i in 0..blocks {
            let mut block = [0u8; 16];
            block[1] = if i + 1 == blocks { 1 } else { 0 };
            out.extend(block);
        }
        out
    }

    #[test]
    fn lines_play_one_at_a_time_and_stop_when_hushed() {
        let mut speech = Speech::default();
        speech.insert(3, vag(1575, 44_100));
        speech.insert(4, vag(10, 22_050));
        assert_eq!(speech.lengths()[0], (3, 1575. * 28. / 44_100.));
        let mut mixer = Mixer::new(48_000);
        speech.follow(&mut mixer, 48_000, Some((1, 3)));
        assert_eq!((mixer.voices(), speech.started), (1, 1));
        // Finishing a sample must not retrigger it on the next frame.
        mixer.render(&mut vec![0.; 96_002]);
        assert!(!speech.playing(&mixer));
        speech.follow(&mut mixer, 48_000, Some((1, 3)));
        assert_eq!(speech.started, 1);
        // The same serial keeps the voice; a new one replaces it.
        speech.follow(&mut mixer, 48_000, Some((1, 3)));
        speech.follow(&mut mixer, 48_000, Some((2, 4)));
        assert_eq!(
            (mixer.voices(), speech.started, speech.last),
            (1, 2, Some(4))
        );
        // A line the package lacks stays silent; hushing stops the voice.
        speech.follow(&mut mixer, 48_000, Some((3, 9)));
        assert_eq!(mixer.voices(), 0);
        speech.follow(&mut mixer, 48_000, Some((4, 3)));
        speech.stop(&mut mixer);
        assert_eq!(mixer.voices(), 0);
    }
}
