//! The desktop soundtrack's remaster songs: the AAC-LC `.m4a` files the web
//! streams through a media element (web/music.js MusicStream), bundled in
//! the desktop content pack and decoded here with Symphonia, a packet at a
//! time about two seconds ahead into the mixer's music bus, resampled to the
//! output rate. The compressed song stays in memory (3-6 MB); decoded PCM
//! never accumulates beyond what the mixer has queued.
//!
//! The encoder's priming (2112 frames for Apple's AAC encoder) is skipped as
//! a browser does, from the MP4 edit list, so positions, seeks and the song's
//! length match the web's media element.
use super::mixer::{Mixer, Resampler};
use crate::Result;
use std::sync::{Arc, Mutex};
use symphonia::core::{
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
    units::Time,
};

/// Seconds queued before [`StreamMusic::pump`] limits its work per call.
const FLOOR: f32 = 0.5;
/// Packets (1024 frames, 23 ms at 44.1 kHz) decoded per call past the floor:
/// eight times real time at 60 frames per second.
const PACKETS: usize = 8;

pub struct StreamMusic {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    /// The song's sample rate and the output rate.
    source_rate: u32,
    rate: u32,
    /// The song's playable frames (edit list) and those decoded so far.
    frames: u64,
    position: u64,
    /// Decoded frames still to drop: the encoder priming, or a seek's
    /// run-up from the packet before the target.
    skip: u64,
    /// The priming in frames (a seek target is offset by it).
    priming: u64,
    ended: bool,
    resampler: Resampler,
    scratch: Vec<f32>,
    pub title: String,
}

impl StreamMusic {
    /// Opens an `.m4a` held in memory.
    pub fn open(bytes: Vec<u8>, title: String, rate: u32) -> Result<Self> {
        let edit = edit_list(&bytes);
        let source =
            MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("m4a");
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                source,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("music stream: {e}"))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or("music stream has no audio track")?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or("music stream has no audio parameters")?;
        let source_rate = params
            .sample_rate
            .filter(|&r| r > 0)
            .ok_or("music stream has no sample rate")?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|e| format!("music stream: {e}"))?;
        let (priming, frames) = edit.map_or((0, None), |e| {
            let scale =
                |v: u64, timescale: u32| v * u64::from(source_rate) / u64::from(timescale.max(1));
            (
                scale(e.media_time, e.media_timescale),
                e.duration.map(|(d, ts)| scale(d, ts)),
            )
        });
        let coded = track.num_frames.unwrap_or(0).saturating_sub(priming);
        let frames = frames.map_or(coded, |f| if coded > 0 { f.min(coded) } else { f });
        let track = track.id;
        Ok(Self {
            format,
            decoder,
            track,
            source_rate,
            rate,
            frames,
            position: 0,
            skip: priming,
            priming,
            ended: false,
            resampler: Resampler::new(source_rate, rate, 2),
            scratch: Vec::new(),
            title,
        })
    }

    /// Seconds decoded so far and the song's length.
    pub fn decoded_seconds(&self) -> (f32, f32) {
        let rate = self.source_rate as f32;
        (self.position as f32 / rate, self.frames as f32 / rate)
    }

    pub fn finished(&self) -> bool {
        self.ended
    }

    /// Jukebox seek: decoding continues at `seconds` (the caller drops what
    /// the mixer still has queued), as music.js sets the element's time.
    pub fn seek(&mut self, seconds: f32) -> Result<()> {
        let target = ((seconds.max(0.) as f64 * f64::from(self.source_rate)) as u64)
            .min(self.frames.saturating_sub(1));
        let time =
            Time::try_from_secs_f64((target + self.priming) as f64 / f64::from(self.source_rate))
                .ok_or("music seek: bad time")?;
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track),
                },
            )
            .map_err(|e| format!("music seek: {e}"))?;
        self.decoder.reset();
        let (required, actual) = (seeked.required_ts.get(), seeked.actual_ts.get());
        // Track timestamps count coded frames (priming included) at the
        // song's rate for AAC in MP4.
        self.skip = u64::try_from(required - actual).unwrap_or(0);
        self.position = u64::try_from(required)
            .unwrap_or(0)
            .saturating_sub(self.priming);
        self.ended = false;
        self.resampler = Resampler::new(self.source_rate, self.rate, 2);
        Ok(())
    }

    /// Decodes towards `ahead` seconds queued; false once the song ended.
    /// Once half a second is queued a call decodes at most [`PACKETS`]
    /// packets, so a song start or seek fills the queue over a few frames
    /// instead of decoding two seconds in one (about 13 ms of decoding per
    /// second of audio on an Apple M4).
    pub fn pump(&mut self, mixer: &Arc<Mutex<Mixer>>, ahead: f32) -> Result<bool> {
        let mut packets = 0;
        loop {
            let queued = mixer.lock().map_err(|_| "mixer lock")?.music_queued();
            if queued >= ahead || (queued >= FLOOR.min(ahead) && packets >= PACKETS) {
                return Ok(true);
            }
            if self.ended {
                return Ok(false);
            }
            let out = self.decode_packet()?;
            packets += 1;
            mixer.lock().map_err(|_| "mixer lock")?.push_music(&out);
        }
    }

    /// The next packet's stereo frames at the output rate (the resampler's
    /// tail once the song ends).
    fn decode_packet(&mut self) -> Result<Vec<f32>> {
        let packet = loop {
            match self.format.next_packet() {
                Ok(Some(packet)) if packet.track_id == self.track => break Some(packet),
                Ok(Some(_)) => {}
                Ok(None) => break None,
                Err(e) => return Err(format!("music stream: {e}")),
            }
        };
        let Some(packet) = packet.filter(|_| self.position < self.frames) else {
            self.ended = true;
            return Ok(self.resampler.process(&[], true));
        };
        let decoded = match self.decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged packet is skipped (a media element plays on).
            Err(Error::DecodeError(_)) => return Ok(Vec::new()),
            Err(e) => return Err(format!("music stream: {e}")),
        };
        let channels = decoded.spec().channels().count().max(1);
        decoded.copy_to_vec_interleaved(&mut self.scratch);
        let frames = self.scratch.len() / channels;
        let start = (self.skip as usize).min(frames);
        self.skip -= start as u64;
        let end = frames.min(start + (self.frames - self.position) as usize);
        self.position += (end - start) as u64;
        let stereo: Vec<f32> = self.scratch[start * channels..end * channels]
            .chunks_exact(channels)
            .flat_map(|f| [f[0], f[channels.min(2) - 1]])
            .collect();
        let last = self.position >= self.frames;
        self.ended = last;
        Ok(self.resampler.process(&stereo, last))
    }
}

/// An MP4 track's edit: the media time it starts at (the encoder priming)
/// in the media timescale, and its duration in the movie timescale.
#[derive(Debug, PartialEq)]
struct Edit {
    media_time: u64,
    media_timescale: u32,
    duration: Option<(u64, u32)>,
}

/// The first audio track's edit list (`moov/trak/edts/elst`), as a browser
/// applies it; None without one.
fn edit_list(bytes: &[u8]) -> Option<Edit> {
    let moov = atoms(bytes).find(|(kind, _)| kind == b"moov")?.1;
    let timescale = |body: &[u8]| {
        let at = if body.first() == Some(&1) { 20 } else { 12 };
        Some(u32::from_be_bytes(body.get(at..at + 4)?.try_into().ok()?))
    };
    let movie_timescale = atoms(moov)
        .find(|(k, _)| k == b"mvhd")
        .and_then(|(_, b)| timescale(b));
    atoms(moov)
        .filter(|(k, _)| k == b"trak")
        .find_map(|(_, trak)| {
            let mdia = atoms(trak).find(|(k, _)| k == b"mdia")?.1;
            atoms(mdia).find(|(k, b)| k == b"hdlr" && b.get(8..12) == Some(b"soun"))?;
            let media_timescale = atoms(mdia)
                .find(|(k, _)| k == b"mdhd")
                .and_then(|(_, b)| timescale(b))?;
            let elst = atoms(atoms(trak).find(|(k, _)| k == b"edts")?.1)
                .find(|(k, _)| k == b"elst")?
                .1;
            let wide = elst.first() == Some(&1);
            let count = u32::from_be_bytes(elst.get(4..8)?.try_into().ok()?) as usize;
            let size = if wide { 20 } else { 12 };
            (0..count.min(16)).find_map(|i| {
                let entry = elst.get(8 + i * size..8 + (i + 1) * size)?;
                let (duration, media_time) = if wide {
                    (
                        u64::from_be_bytes(entry[..8].try_into().ok()?),
                        i64::from_be_bytes(entry[8..16].try_into().ok()?),
                    )
                } else {
                    (
                        u64::from(u32::from_be_bytes(entry[..4].try_into().ok()?)),
                        i64::from(i32::from_be_bytes(entry[4..8].try_into().ok()?)),
                    )
                };
                // -1 marks an empty edit (a delay before the media starts).
                Some(Edit {
                    media_time: u64::try_from(media_time).ok()?,
                    media_timescale,
                    duration: movie_timescale
                        .filter(|_| duration > 0)
                        .map(|ts| (duration, ts)),
                })
            })
        })
}

/// The atoms in `data`: (type, body).
fn atoms(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut rest = data;
    std::iter::from_fn(move || {
        let size = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let kind: [u8; 4] = rest.get(4..8)?.try_into().ok()?;
        let (header, size) = match size {
            0 => (8, rest.len()),
            1 => (
                16,
                usize::try_from(u64::from_be_bytes(rest.get(8..16)?.try_into().ok()?)).ok()?,
            ),
            n => (8, n),
        };
        if size < header || size > rest.len() {
            return None;
        }
        let body = &rest[header..size];
        rest = &rest[size..];
        Some((kind, body))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One second of authored test signal (no game data): 0.5 s of silence,
    /// then 0.5 s of 440 Hz left / 660 Hz right at half scale, encoded the
    /// way tools/package_content.py encodes the soundtrack (`ffmpeg -c:a
    /// aac_at -movflags +faststart -fflags +bitexact`, 44.1 kHz stereo).
    fn tone() -> Vec<u8> {
        let text = include_str!("../../tests/fixtures/tone.m4a.hex");
        let hex: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
        hex.chunks(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }

    fn drain(mixer: &Arc<Mutex<Mixer>>, song: &mut StreamMusic) -> Vec<[f32; 2]> {
        let mut out = Vec::new();
        let mut block = vec![0f32; 2048];
        loop {
            let more = song.pump(mixer, 0.5).unwrap();
            let mut m = mixer.lock().unwrap();
            let queued = (m.music_queued() * 48000.).round() as usize;
            if queued == 0 && !more {
                return out;
            }
            // Read the queue back through the music bus at unity.
            m.music_volume = 1.;
            let frames = queued.min(1024);
            block.resize(frames * 2, 0.);
            m.render(&mut block);
            out.extend(
                block
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|f| f.map(|s| s / super::super::mixer::MASTER)),
            );
        }
    }

    fn onset(frames: &[[f32; 2]]) -> f32 {
        frames.iter().position(|f| f[0].abs() > 0.1).unwrap() as f32 / 48000.
    }

    #[test]
    fn reads_the_encoder_priming_from_the_edit_list() {
        let edit = edit_list(&tone()).unwrap();
        assert_eq!(edit.media_time, 2112);
        assert_eq!(edit.media_timescale, 44100);
        assert!(edit.duration.is_some());
        assert_eq!(edit_list(b"not an mp4"), None);
    }

    #[test]
    fn decodes_the_song_at_the_output_rate_without_the_priming() {
        let mut song = StreamMusic::open(tone(), "Tone".into(), 48000).unwrap();
        let (decoded, duration) = song.decoded_seconds();
        assert_eq!(decoded, 0.);
        assert!((duration - 1.0).abs() < 0.03, "{duration}");
        let mixer = Arc::new(Mutex::new(Mixer::new(48000)));
        let frames = drain(&mixer, &mut song);
        assert!(song.finished());
        let seconds = frames.len() as f32 / 48000.;
        assert!((seconds - duration).abs() < 0.01, "{seconds} vs {duration}");
        // The tone starts half a second in, as the browser plays it (an
        // untrimmed decode would start 48 ms late).
        assert!((onset(&frames) - 0.5).abs() < 0.01, "{}", onset(&frames));
        let peak = frames
            .iter()
            .fold(0f32, |p, f| p.max(f[0].abs()).max(f[1].abs()));
        // Half scale, plus the codec's ringing at the abrupt onset.
        assert!((0.4..0.7).contains(&peak), "{peak}");
    }

    #[test]
    fn seeks_resume_at_the_requested_time() {
        let mut song = StreamMusic::open(tone(), "Tone".into(), 48000).unwrap();
        let mixer = Arc::new(Mutex::new(Mixer::new(48000)));
        song.pump(&mixer, 0.2).unwrap();
        song.seek(0.25).unwrap();
        mixer.lock().unwrap().clear_music();
        assert!((song.decoded_seconds().0 - 0.25).abs() < 0.001);
        let frames = drain(&mixer, &mut song);
        assert!((onset(&frames) - 0.25).abs() < 0.01, "{}", onset(&frames));
        // Past the end: the last moment of the song, then it ends.
        song.seek(9.).unwrap();
        mixer.lock().unwrap().clear_music();
        let frames = drain(&mixer, &mut song);
        assert!(frames.len() < 4800, "{}", frames.len());
        assert!(song.finished());
    }

    #[test]
    fn refuses_what_is_not_a_song() {
        assert!(StreamMusic::open(b"not an mp4 at all".to_vec(), String::new(), 48000).is_err());
    }
}
