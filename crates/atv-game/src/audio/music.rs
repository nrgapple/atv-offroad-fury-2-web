//! The soundtrack: the original 21-song `music.pb` catalog and its
//! non-repeating shuffle (atv_core::music), and a streaming decoder of the
//! original VAG songs into the mixer's music bus. The browser JavaScript
//! streams a hosted AAC re-encode of the same decoded PCM through a media
//! element; the Rust web build does the same (atv-app), while desktop builds
//! decode the original VAG with the decoder the JavaScript fallback uses.
use super::mixer::{Mixer, Resampler};
use crate::{Result, archive::Archive};
use atv_content::Content;
use atv_core::{
    audio::VagStream,
    engine_audio::OriginalRandom,
    music::{MusicLibrary, MusicShuffle, Song},
};
use std::sync::{Arc, Mutex};

pub struct Catalog {
    pub library: MusicLibrary,
    shuffle: MusicShuffle,
    random: OriginalRandom,
}

impl Catalog {
    /// `seed` stands in for the unrecovered original RTC seed, as the
    /// browser's session seed does.
    pub async fn load(content: &Content, seed: u64) -> Result<Self> {
        let archive = Archive::open(content, "/STARTAPP.RES").await?;
        let index = archive
            .named("music.pb")
            .ok_or("original music catalog is missing")?;
        let library = MusicLibrary::parse(&archive.member(index).await?)?;
        Ok(Self {
            shuffle: MusicShuffle::new(&library),
            library,
            random: OriginalRandom::with_seed(seed.max(1)),
        })
    }

    pub fn next_song(&mut self) -> Result<Option<&Song>> {
        Ok(self
            .shuffle
            .next_song(&mut self.random)?
            .map(|i| &self.library.songs[i]))
    }
}

/// Content path of a catalog song.
pub fn path(song: &Song) -> String {
    format!("/AUDIO/{}", song.filename.replace('\\', "/"))
}

/// Streams one original VAG song at a time, about two seconds ahead.
pub struct VagMusic {
    path: String,
    stream: VagStream,
    offset: usize,
    resampler: Resampler,
    pub title: String,
}

/// Bytes decoded per read (the browser fallback's 64 KiB).
const READ: usize = 65536;

impl VagMusic {
    pub async fn open(content: &Content, song: &Song, rate: u32) -> Result<Self> {
        let path = path(song);
        let size = content.size(&path)?;
        let prefix = content.read(&path, 0, size.min(2016)).await?;
        let stream = VagStream::new(&prefix, size)?;
        Ok(Self {
            resampler: Resampler::new(stream.info.sample_rate, rate, 2),
            path,
            stream,
            offset: 0,
            title: song.title.clone(),
        })
    }

    pub fn finished(&self) -> bool {
        self.offset >= self.stream.info.data_bytes
    }

    /// Decodes until `ahead` seconds are queued; false once the song ended.
    pub async fn pump(
        &mut self,
        content: &Content,
        mixer: &Arc<Mutex<Mixer>>,
        ahead: f32,
    ) -> Result<bool> {
        loop {
            if mixer.lock().map_err(|_| "mixer lock")?.music_queued() >= ahead {
                return Ok(true);
            }
            if self.finished() {
                return Ok(false);
            }
            let stride = self.stream.info.channels as usize * 16;
            let count = (READ / stride * stride).min(self.stream.info.data_bytes - self.offset);
            let bytes = content
                .read(
                    &self.path,
                    self.stream.info.data_offset + self.offset,
                    count,
                )
                .await?;
            self.offset += count;
            let pcm = self.stream.decode_chunk(&bytes)?;
            let mut samples: Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.).collect();
            if self.stream.info.channels == 1 {
                samples = samples.iter().flat_map(|&s| [s, s]).collect();
            }
            let out = self.resampler.process(&samples, self.finished());
            mixer.lock().map_err(|_| "mixer lock")?.push_music(&out);
        }
    }
}
