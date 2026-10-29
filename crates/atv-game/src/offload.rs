//! Load-time decodes handed to other CPU cores.
//!
//! The PS2 decoded its course textures on one CPU while the loader played.
//! A phone has several cores, and on the web the engine's own thread is also
//! the page's: a long decode there freezes the loader and every input. Pure
//! decodes (bytes in, pixels out) therefore run as jobs:
//!
//! - web: a small pool of Web Workers (`web/decode-worker.js`) running this
//!   same WebAssembly module, sharing the page's compiled module; the page
//!   thread only copies bytes in and pixels out;
//! - desktop and Android: scoped threads.
//!
//! Every job runs the same Rust function ([`run`]) wherever it runs, so the
//! output is byte-identical to decoding inline; a pool that cannot start (no
//! worker support, an old page shell, a worker error) decodes inline instead.
use crate::Result;

/// Job kinds ([`run`]'s `kind`).
pub const IPUM: u32 = 1;
/// [`IPUM`] flag: also produce the deblocked/smoothed tile (Enhanced sky).
pub const SMOOTH: u32 = 1;
/// An original VAG decoded and resampled to the output rate (`arg`).
pub const VAG_PCM: u32 = 3;

/// One decoded IPUM image: its RGBA and, when asked, the smoothed RGBA.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub smooth: Option<Vec<u8>>,
}

/// Runs one job on this thread: the function every worker runs.
/// Output layout for [`IPUM`]: width and height (u32 LE), the RGBA, then
/// the smoothed RGBA when [`SMOOTH`] is set.
pub fn run(kind: u32, arg: u32, bytes: &[u8]) -> Result<Vec<u8>> {
    match kind {
        IPUM => {
            let image = atv_core::ipu::decode_ipum(bytes)?;
            let smooth = (arg & SMOOTH != 0).then(|| image.smoothed_rgba());
            let mut out = Vec::with_capacity(
                8 + image.rgba.len() + smooth.as_ref().map_or(0, Vec::len),
            );
            out.extend_from_slice(&image.info.width.to_le_bytes());
            out.extend_from_slice(&image.info.height.to_le_bytes());
            out.extend_from_slice(&image.rgba);
            if let Some(smooth) = smooth {
                out.extend_from_slice(&smooth);
            }
            Ok(out)
        }
        VAG_PCM => {
            let audio = atv_core::audio::decode_vag(bytes)?;
            let pcm = crate::audio::mixer::Pcm::from_vag(&audio, arg);
            let mut out = Vec::with_capacity(32 + pcm.data.len() * 4);
            out.extend_from_slice(&audio.info.sample_rate.to_le_bytes());
            out.extend_from_slice(&(pcm.channels as u32).to_le_bytes());
            let (start, end) = pcm.loop_markers.unwrap_or((f64::NAN, f64::NAN));
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&end.to_le_bytes());
            for v in &pcm.data {
                out.extend_from_slice(&v.to_le_bytes());
            }
            Ok(out)
        }
        _ => atv_content::run_job(kind, arg, bytes)
            .unwrap_or_else(|| Err(format!("unknown decode job {kind}"))),
    }
}

fn unpack_ipum(out: Vec<u8>, smooth: bool) -> Result<Decoded> {
    let word = |at: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            out.get(at..at + 4)
                .ok_or("short decode result")?
                .try_into()
                .map_err(|_| "short decode result")?,
        ))
    };
    let (width, height) = (word(0)?, word(4)?);
    let size = width as usize * height as usize * 4;
    let expected = 8 + size * if smooth { 2 } else { 1 };
    if out.len() != expected {
        return Err("decode result size mismatch".into());
    }
    let rgba = out[8..8 + size].to_vec();
    let smooth = smooth.then(|| out[8 + size..].to_vec());
    Ok(Decoded {
        width,
        height,
        rgba,
        smooth,
    })
}

/// Decodes IPUM images (terrain layers, sky tiles) in parallel, in order.
pub async fn ipums(inputs: &[&[u8]], smooth: bool) -> Result<Vec<Decoded>> {
    let arg = if smooth { SMOOTH } else { 0 };
    let outputs = many(IPUM, arg, inputs).await?;
    outputs
        .into_iter()
        .map(|out| unpack_ipum(out, smooth))
        .collect()
}

fn unpack_pcm(out: &[u8], rate: u32) -> Result<(u32, crate::audio::mixer::Pcm)> {
    let get = |at: usize, n: usize| out.get(at..at + n).ok_or("short decode result");
    let word = |at| -> Result<u32> { Ok(u32::from_le_bytes(get(at, 4)?.try_into().unwrap())) };
    let float = |at| -> Result<f64> { Ok(f64::from_le_bytes(get(at, 8)?.try_into().unwrap())) };
    let (source, channels) = (word(0)?, word(4)? as usize);
    let (start, end) = (float(8)?, float(16)?);
    let samples = out.get(24..).ok_or("short decode result")?;
    if channels == 0 || samples.len() % (4 * channels) != 0 {
        return Err("decode result size mismatch".into());
    }
    let data: Vec<f32> = samples
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    Ok((
        source,
        crate::audio::mixer::Pcm {
            channels,
            frames: data.len() / channels,
            data,
            loop_markers: (!start.is_nan()).then_some((start, end)),
            rate,
        },
    ))
}

/// Original VAGs decoded and resampled to `rate` in parallel, in order:
/// each with its source sample rate.
pub async fn pcms(inputs: &[&[u8]], rate: u32) -> Result<Vec<(u32, crate::audio::mixer::Pcm)>> {
    many(VAG_PCM, rate, inputs)
        .await?
        .iter()
        .map(|out| unpack_pcm(out, rate))
        .collect()
}

/// Starts the web pool ahead of the first job (a ride load), so its workers
/// instantiate while the course downloads. No-op elsewhere.
pub fn warm() {
    #[cfg(target_arch = "wasm32")]
    atv_content::pool::warm();
}

/// Runs `inputs` as `kind` jobs and returns their outputs in order.
pub async fn many(kind: u32, arg: u32, inputs: &[&[u8]]) -> Result<Vec<Vec<u8>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(outputs) = atv_content::pool::many(kind, arg, inputs).await {
            return outputs;
        }
        // No pool: inline, giving the browser the thread between jobs.
        let mut outputs = Vec::with_capacity(inputs.len());
        for input in inputs {
            outputs.push(run(kind, arg, input)?);
            crate::yield_frame().await;
        }
        Ok(outputs)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .clamp(1, 8)
            .min(inputs.len());
        let per = inputs.len().div_ceil(threads);
        let results: Vec<Result<Vec<Vec<u8>>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = inputs
                .chunks(per)
                .map(|group| {
                    scope.spawn(move || {
                        group
                            .iter()
                            .map(|input| run(kind, arg, input))
                            .collect::<Result<Vec<_>>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err("decode thread panicked".into())))
                .collect()
        });
        let mut outputs = Vec::with_capacity(inputs.len());
        for group in results {
            outputs.extend(group?);
        }
        crate::yield_frame().await;
        Ok(outputs)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_results_round_trip() {
        use crate::audio::mixer::Pcm;
        let pcm = Pcm {
            channels: 2,
            frames: 2,
            data: vec![0.5, -0.25, 1., 0.],
            loop_markers: Some((1., 2.)),
            rate: 48000,
        };
        let mut out = Vec::new();
        out.extend_from_slice(&22050u32.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&1f64.to_le_bytes());
        out.extend_from_slice(&2f64.to_le_bytes());
        for v in &pcm.data {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let (source, back) = unpack_pcm(&out, 48000).unwrap();
        assert_eq!(source, 22050);
        assert_eq!(
            (back.channels, back.frames, &back.data, back.loop_markers, back.rate),
            (2, 2, &pcm.data, pcm.loop_markers, 48000)
        );
    }

    #[test]
    fn unknown_jobs_fail() {
        assert!(run(99, 0, &[]).is_err());
    }

    #[test]
    fn ipum_results_round_trip() {
        let mut out = Vec::new();
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        out.extend_from_slice(&[9; 8]);
        let decoded = unpack_ipum(out.clone(), true).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.rgba, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(decoded.smooth.as_deref(), Some(&[9u8; 8][..]));
        assert!(unpack_ipum(out, false).is_err());
    }
}
