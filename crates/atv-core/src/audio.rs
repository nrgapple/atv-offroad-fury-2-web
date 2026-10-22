//! PS ADPCM sample decoding; does not emulate the SPU2 mixer or resampler.
use crate::{Result, slice};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: usize,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
}

pub struct Audio {
    pub info: AudioInfo,
    pub samples: Vec<i16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub data_offset: usize,
    pub data_bytes: usize,
    pub frames_max: usize,
}

/// Incremental VAG decoding retains predictor history across browser reads.
/// This shares the preview decoder's PCM arithmetic, not SPU2 mixing behavior.
#[derive(Clone)]
pub struct VagStream {
    pub info: StreamInfo,
    history: [[i32; 2]; 2],
    consumed: usize,
    ended: bool,
}
impl VagStream {
    pub fn new(prefix: &[u8], file_bytes: usize) -> Result<Self> {
        let header = slice(prefix, 0, 48)?;
        if &header[..4] != b"VAGp" && header[..12] != [0; 12] {
            return Err("unsupported VAG stream header".into());
        }
        let bytes = u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize;
        let sample_rate = u32::from_be_bytes(header[16..20].try_into().unwrap());
        if !(8000..=96000).contains(&sample_rate)
            || bytes == 0
            || !bytes.is_multiple_of(16)
            || bytes > 32 * 1024 * 1024
        {
            return Err("invalid VAG stream size/rate".into());
        }
        let (channels, data_offset, data_bytes) = if file_bytes == bytes + 48 {
            (1, 48, bytes)
        } else if file_bytes == bytes * 2 + 2016
            && header[..12] == [0; 12]
            && slice(prefix, 20, 1996)?.iter().all(|b| *b == 0)
        {
            (2, 2016, bytes * 2)
        } else {
            return Err("unsupported VAG stream channel layout".into());
        };
        Ok(Self {
            info: StreamInfo {
                sample_rate,
                channels,
                data_offset,
                data_bytes,
                frames_max: bytes / 16 * 28,
            },
            history: [[0; 2]; 2],
            consumed: 0,
            ended: false,
        })
    }
    /// Continues decoding at data byte `consumed` (a whole interleaved block
    /// boundary): a Jukebox seek. The predictor restarts from silence, as a
    /// fresh stream does; the first block after the jump may click faintly.
    pub fn seek(&mut self, consumed: usize) -> Result<()> {
        let stride = self.info.channels as usize * 16;
        if !consumed.is_multiple_of(stride) || consumed > self.info.data_bytes {
            return Err("invalid VAG stream seek".into());
        }
        self.consumed = consumed;
        self.history = [[0; 2]; 2];
        self.ended = false;
        Ok(())
    }
    /// Chunks contain complete interleaved blocks and must arrive in file order.
    /// A failed chunk leaves predictor history and the byte cursor unchanged.
    pub fn decode_chunk(&mut self, bytes: &[u8]) -> Result<Vec<i16>> {
        let stride = self.info.channels as usize * 16;
        if bytes.len() > 256 * 1024
            || !bytes.len().is_multiple_of(stride)
            || self.consumed + bytes.len() > self.info.data_bytes
        {
            return Err("invalid VAG stream chunk extent".into());
        }
        let mut next = self.clone();
        let mut samples = Vec::with_capacity(bytes.len() / 16 * 28);
        for pair in bytes.chunks_exact(stride) {
            if next.ended {
                break;
            }
            let mut blocks = [[0i16; 28]; 2];
            let mut flags = [0u8; 2];
            for channel in 0..self.info.channels as usize {
                let block: &[u8; 16] = pair[channel * 16..channel * 16 + 16].try_into().unwrap();
                blocks[channel] = decode_block(block, &mut next.history[channel])?;
                flags[channel] = block[1];
            }
            if self.info.channels == 2 && flags[0] != flags[1] {
                return Err("stereo VAG stream flags disagree".into());
            }
            for frame in 0..28 {
                for channel in blocks.iter().take(self.info.channels as usize) {
                    samples.push(channel[frame]);
                }
            }
            next.ended = flags[0] & 1 != 0;
        }
        next.consumed += bytes.len();
        *self = next;
        Ok(samples)
    }
}

fn decode_block(block: &[u8; 16], history: &mut [i32; 2]) -> Result<[i16; 28]> {
    const FILTER: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
    let shift = block[0] & 15;
    let filter = usize::from(block[0] >> 4);
    if shift > 12 || filter >= FILTER.len() || block[1] > 7 {
        return Err("unsupported ADPCM block parameters".into());
    }
    let (f0, f1) = FILTER[filter];
    let mut result = [0; 28];
    for (i, &byte) in block[2..].iter().enumerate() {
        for (j, nibble) in [byte & 15, byte >> 4].into_iter().enumerate() {
            let signed = ((nibble << 4) as i8 >> 4) as i32;
            let sample = (((signed << 12) >> shift)
                + ((history[0] * f0 + history[1] * f1 + 32) >> 6))
                .clamp(-32768, 32767);
            result[i * 2 + j] = sample as i16;
            history[1] = history[0];
            history[0] = sample;
        }
    }
    Ok(result)
}

pub fn decode_vag(data: &[u8]) -> Result<Audio> {
    let header = slice(data, 0, 48)?;
    if &header[..4] != b"VAGp" && header[..12] != [0; 12] {
        return Err("unsupported VAG header".into());
    }
    let bytes = u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize;
    let rate = u32::from_be_bytes(header[16..20].try_into().unwrap());
    if !(8000..=96000).contains(&rate)
        || bytes == 0
        || !bytes.is_multiple_of(16)
        || bytes > 32 * 1024 * 1024
    {
        return Err("invalid VAG rate/size".into());
    }
    if data.len() == bytes + 48 {
        let (samples, loop_start, loop_end) = decode_adpcm(&data[48..])?;
        let frames = samples.len();
        return Ok(Audio {
            info: AudioInfo {
                sample_rate: rate,
                channels: 1,
                frames,
                loop_start,
                loop_end,
            },
            samples,
        });
    }
    // Exact-build stereo streams have a padded first sector and alternating
    // 16-byte channel blocks. Preserve the two silent init blocks at 0x7e0.
    if data.len() != bytes * 2 + 2016
        || header[..12] != [0; 12]
        || slice(data, 20, 1996)?.iter().any(|&v| v != 0)
    {
        return Err("unsupported VAG channel layout".into());
    }
    let mut left = Vec::with_capacity(bytes);
    let mut right = Vec::with_capacity(bytes);
    for pair in data[2016..].as_chunks::<32>().0 {
        left.extend(&pair[..16]);
        right.extend(&pair[16..]);
    }
    let (left, loop_start, loop_end) = decode_adpcm(&left)?;
    let (right, right_start, right_end) = decode_adpcm(&right)?;
    if left.len() != right.len() || loop_start != right_start || loop_end != right_end {
        return Err("stereo channel lengths/loop markers disagree".into());
    }
    let frames = left.len();
    let mut samples = Vec::with_capacity(frames * 2);
    for (l, r) in left.into_iter().zip(right) {
        samples.push(l);
        samples.push(r);
    }
    Ok(Audio {
        info: AudioInfo {
            sample_rate: rate,
            channels: 2,
            frames,
            loop_start,
            loop_end,
        },
        samples,
    })
}

fn decode_adpcm(data: &[u8]) -> Result<(Vec<i16>, Option<usize>, Option<usize>)> {
    if !data.len().is_multiple_of(16) {
        return Err("partial ADPCM block".into());
    }
    let mut samples = Vec::with_capacity(data.len() / 16 * 28);
    let mut history = [0, 0];
    let (mut loop_start, mut loop_end) = (None, None);
    for (i, block) in data.as_chunks::<16>().0.iter().enumerate() {
        if block[1] & 4 != 0 {
            loop_start = Some(samples.len());
        }
        samples.extend(decode_block(block, &mut history).map_err(|e| format!("{e} in block {i}"))?);
        if block[1] & 1 != 0 {
            if block[1] & 2 != 0 {
                loop_end = Some(samples.len());
            } else {
                loop_start = None;
            }
            break;
        }
    }
    if loop_end.is_some() && loop_start.is_none() {
        loop_start = Some(0);
    }
    Ok((samples, loop_start, loop_end))
}

impl Audio {
    /// Plain PCM WAV for previews. Loop points are supplied separately in metadata.
    pub fn wav(&self) -> Vec<u8> {
        let bytes = (self.samples.len() * 2) as u32;
        let channels = self.info.channels;
        let mut out = Vec::with_capacity(bytes as usize + 44);
        out.extend(b"RIFF");
        out.extend((bytes + 36).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(self.info.sample_rate.to_le_bytes());
        out.extend((self.info.sample_rate * u32::from(channels) * 2).to_le_bytes());
        out.extend((channels * 2).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(bytes.to_le_bytes());
        for sample in &self.samples {
            out.extend(sample.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_nibbles_low_first_and_loops() {
        let mut b = [0u8; 32];
        b[0] = 12;
        b[1] = 4;
        b[2] = 0x87;
        b[16] = 12;
        b[17] = 3;
        let (pcm, start, end) = decode_adpcm(&b).unwrap();
        assert_eq!(&pcm[..4], &[7, -8, 0, 0]);
        assert_eq!(start, Some(0));
        assert_eq!(end, Some(56));
    }
    #[test]
    fn predictor_history_crosses_blocks() {
        let mut b = [0u8; 32];
        b[0] = 0;
        b[15] = 0x70;
        b[16] = 0x1c;
        let (pcm, _, _) = decode_adpcm(&b).unwrap();
        assert_eq!(pcm[27], 28672);
        assert_eq!(pcm[28], 26880);
    }
    #[test]
    fn validates_size_and_rate() {
        let mut b = vec![0u8; 64];
        b[12..16].copy_from_slice(&16u32.to_be_bytes());
        b[16..20].copy_from_slice(&44100u32.to_be_bytes());
        let audio = decode_vag(&b).unwrap();
        assert_eq!(audio.info.frames, 28);
        assert_eq!(audio.wav().len(), 100);
        b.push(0);
        assert!(decode_vag(&b).is_err());
        assert!(decode_adpcm(&[0xff; 16]).is_err());
    }
    #[test]
    fn stereo_interleave_and_independent_history() {
        let mut b = vec![0u8; 2016 + 64];
        b[12..16].copy_from_slice(&32u32.to_be_bytes());
        b[16..20].copy_from_slice(&44100u32.to_be_bytes());
        b[2016] = 12;
        b[2018] = 0x21;
        b[2032] = 12;
        b[2034] = 0x43;
        b[2048] = 12;
        b[2064] = 12;
        let a = decode_vag(&b).unwrap();
        assert_eq!(a.info.channels, 2);
        assert_eq!(a.info.frames, 56);
        assert_eq!(&a.samples[..4], &[1, 3, 2, 4]);
        b[100] = 1;
        assert!(decode_vag(&b).is_err());
    }
    #[test]
    fn stream_boundaries_preserve_stereo_predictors_and_reject_partial_blocks() {
        let mut data = vec![0; 2016 + 32 * 13];
        data[12..16].copy_from_slice(&(16u32 * 13).to_be_bytes());
        data[16..20].copy_from_slice(&32000u32.to_be_bytes());
        for (i, block) in data[2016..].as_chunks_mut::<16>().0.iter_mut().enumerate() {
            block[0] = ((i % 5) as u8) << 4 | 9;
            for (j, b) in block[2..].iter_mut().enumerate() {
                *b = (i * 17 + j * 23) as u8;
            }
        }
        let expected = decode_vag(&data).unwrap();
        let mut stream = VagStream::new(&data[..2016], data.len()).unwrap();
        assert!(stream.decode_chunk(&data[2016..2032]).is_err());
        let mut actual = Vec::new();
        for chunk in data[2016..].chunks(32 * 3) {
            actual.extend(stream.decode_chunk(chunk).unwrap());
        }
        assert_eq!(actual, expected.samples);
        assert!(stream.decode_chunk(&[0; 32]).is_err());
        // A seek back to a block boundary decodes from there again, the
        // predictor restarting from silence; partial blocks are refused.
        assert!(stream.seek(16).is_err());
        assert!(stream.seek(32 * 14).is_err());
        stream.seek(0).unwrap();
        assert_eq!(
            stream.decode_chunk(&data[2016..2048]).unwrap(),
            &expected.samples[..56]
        );
        stream.seek(32 * 12).unwrap();
        assert_eq!(
            stream.decode_chunk(&data[2016 + 32 * 12..]).unwrap().len(),
            56
        );
    }
}
