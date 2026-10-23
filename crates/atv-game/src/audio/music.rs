//! The soundtrack: the original 21-song `music.pb` catalog and its
//! non-repeating shuffle (atv_core::music), and a streaming decoder of the
//! original VAG songs into the mixer's music bus. The browser JavaScript
//! streams a hosted AAC re-encode of the same decoded PCM through a media
//! element; the Rust web build does the same (atv-app). Desktop builds decode
//! the same AAC files, bundled in their content pack ([`super::stream`]),
//! and fall back to the original VAG with the decoder the JavaScript fallback
//! uses when a song's file is missing.
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

    /// A new session's shuffle (music.js `load`: op 40 with a fresh
    /// `sessionSeed()` for each ride).
    pub fn reseed(&mut self, seed: u64) {
        self.shuffle = MusicShuffle::new(&self.library);
        self.random = OriginalRandom::with_seed(seed.max(1));
    }

    pub fn next_song(&mut self) -> Result<Option<&Song>> {
        Ok(self
            .shuffle
            .next_song(&mut self.random)?
            .map(|i| &self.library.songs[i]))
    }

    /// The next shuffle pick's index (op 41 without a selection).
    pub fn next_index(&mut self) -> Result<Option<usize>> {
        self.shuffle.next_song(&mut self.random)
    }

    /// A chosen song (op 41 `select`): the shuffle continues after it.
    pub fn select(&mut self, index: usize) -> Result<()> {
        self.shuffle.select(index)
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
    /// The output rate the resampler converts to.
    rate: u32,
    pub title: String,
}

impl VagMusic {
    /// Seconds decoded so far and the song's length.
    pub fn decoded_seconds(&self) -> (f32, f32) {
        let info = &self.stream.info;
        let per = |bytes: usize| {
            (bytes / 16 * 28) as f32 / info.channels.max(1) as f32 / info.sample_rate.max(1) as f32
        };
        (
            per(self.offset),
            info.frames_max as f32 / info.sample_rate.max(1) as f32,
        )
    }
}

/// Bytes decoded per read (the browser fallback's 64 KiB).
const READ: usize = 65536;

impl VagMusic {
    pub async fn open(content: &Content, song: &Song, rate: u32) -> Result<Self> {
        Self::open_path(content, path(song), song.title.clone(), rate).await
    }

    /// [`VagMusic::open`] by content path (the browser's VAG fallback opens
    /// a song outside the catalog borrow).
    pub async fn open_path(
        content: &Content,
        path: String,
        title: String,
        rate: u32,
    ) -> Result<Self> {
        let size = content.size(&path)?;
        let prefix = content.read(&path, 0, size.min(2016)).await?;
        let stream = VagStream::new(&prefix, size)?;
        Ok(Self {
            resampler: Resampler::new(stream.info.sample_rate, rate, 2),
            rate,
            path,
            stream,
            offset: 0,
            title,
        })
    }

    /// Jukebox seek: decoding continues from the block at `seconds` (the
    /// caller drops what the mixer still has queued).
    pub fn seek(&mut self, seconds: f32) -> Result<()> {
        let offset = seek_offset(&self.stream.info, seconds);
        self.stream.seek(offset)?;
        self.offset = offset;
        self.resampler = Resampler::new(self.stream.info.sample_rate, self.rate, 2);
        Ok(())
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

/// A desktop soundtrack song: the bundled remaster (what the web streams)
/// or the original VAG.
#[cfg(not(target_arch = "wasm32"))]
pub enum NativeSong {
    Stream(super::stream::StreamMusic),
    Vag(VagMusic),
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeSong {
    /// Seconds decoded so far and the song's length.
    pub fn decoded_seconds(&self) -> (f32, f32) {
        match self {
            Self::Stream(song) => song.decoded_seconds(),
            Self::Vag(song) => song.decoded_seconds(),
        }
    }

    pub fn finished(&self) -> bool {
        match self {
            Self::Stream(song) => song.finished(),
            Self::Vag(song) => song.finished(),
        }
    }

    pub fn seek(&mut self, seconds: f32) -> Result<()> {
        match self {
            Self::Stream(song) => song.seek(seconds),
            Self::Vag(song) => song.seek(seconds),
        }
    }

    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
    }

    /// Decodes until `ahead` seconds are queued; false once the song ended.
    pub async fn pump(
        &mut self,
        content: &Content,
        mixer: &Arc<Mutex<Mixer>>,
        ahead: f32,
    ) -> Result<bool> {
        match self {
            Self::Stream(song) => song.pump(mixer, ahead),
            Self::Vag(song) => song.pump(content, mixer, ahead).await,
        }
    }
}

/// The data byte of the whole interleaved block at `seconds`, before the
/// song's last block.
fn seek_offset(info: &atv_core::audio::StreamInfo, seconds: f32) -> usize {
    let stride = info.channels as usize * 16;
    let block = (seconds.max(0.) * info.sample_rate as f32 / 28.) as usize;
    let last = (info.data_bytes / stride).saturating_sub(1);
    block.min(last) * stride
}

#[cfg(test)]
mod tests {
    #[test]
    fn seeks_land_on_whole_blocks_inside_the_song() {
        let info = atv_core::audio::StreamInfo {
            sample_rate: 28_000,
            channels: 2,
            data_offset: 2016,
            data_bytes: 32 * 1000,
            frames_max: 28_000,
        };
        // 1000 blocks of 28 frames at 28 kHz: one second, 1000 blocks/s.
        assert_eq!(super::seek_offset(&info, 0.), 0);
        assert_eq!(super::seek_offset(&info, 0.5), 500 * 32);
        assert_eq!(super::seek_offset(&info, -3.), 0);
        assert_eq!(super::seek_offset(&info, 9.), 999 * 32);
    }
}
