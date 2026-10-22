//! Software mixer with the browser game's Web Audio graph semantics: one SFX
//! bus and one music bus into a 0.65 master, AudioBufferSourceNode playback
//! (samples resampled to the output rate once, as decodeAudioData does, then
//! linear interpolation for playback-rate changes and loop points in
//! seconds), per-voice stereo gains, the Enhanced engine crossfade and the
//! optional shared echo send (web/audio-enhance.js). Platform output (cpal)
//! pulls `render`; everything here is shared Rust.
use atv_core::math::Det;
use std::sync::Arc;

/// GameAudio's master gain (web/audio.js) and the music player's default.
pub const MASTER: f32 = 0.65;
pub const MUSIC_VOLUME: f32 = 0.45;
/// Enhanced engine crossfade (CROSSFADE_SECONDS in web/audio-enhance.js).
pub const CROSSFADE: f32 = 0.12;

/// Decoded PCM at the mixer's output rate.
pub struct Pcm {
    pub channels: usize,
    pub frames: usize,
    /// Interleaved samples in -1..1.
    pub data: Vec<f32>,
    /// Original VAG loop markers, in output frames.
    pub loop_markers: Option<(f64, f64)>,
    pub rate: u32,
}

impl Pcm {
    pub fn duration(&self) -> f32 {
        self.frames as f32 / self.rate as f32
    }
}

const HALF: usize = 16;
const PHASES: usize = 512;

/// Windowed-sinc rate conversion, standing in for decodeAudioData's
/// conversion of the 11-48 kHz originals to the context rate. Blackman
/// window, 32 taps, 512 fractional phases normalised to unity DC gain.
pub struct Resampler {
    ratio: f64,
    table: std::rc::Rc<Vec<f32>>,
    channels: usize,
    /// Pending input (interleaved) and the read position within it.
    input: Vec<f32>,
    position: f64,
}

impl Resampler {
    pub fn new(from: u32, to: u32, channels: usize) -> Self {
        Self {
            ratio: from as f64 / to as f64,
            table: TABLES.with_borrow_mut(|tables| {
                tables
                    .entry((from, to))
                    .or_insert_with(|| std::rc::Rc::new(Self::table(from, to)))
                    .clone()
            }),
            channels,
            // Leading silence so the first output sample is centred on input 0.
            input: vec![0.; (HALF - 1) * channels],
            position: (HALF - 1) as f64,
        }
    }

    /// The kernel for one rate pair (shared: every sample at that pair uses
    /// the same table).
    fn table(from: u32, to: u32) -> Vec<f32> {
        let ratio = from as f64 / to as f64;
        let cutoff = (1. / ratio).min(1.) * 0.97;
        let mut table = vec![0f32; (PHASES + 1) * 2 * HALF];
        for phase in 0..=PHASES {
            let frac = phase as f64 / PHASES as f64;
            let row = &mut table[phase * 2 * HALF..(phase + 1) * 2 * HALF];
            let mut sum = 0.;
            let mut weights = [0f64; 2 * HALF];
            for (k, w) in weights.iter_mut().enumerate() {
                let x = frac + HALF as f64 - 1. - k as f64;
                let u = std::f64::consts::PI * x * cutoff;
                let sinc = if u == 0. {
                    cutoff
                } else {
                    cutoff * u.sin_d() / u
                };
                let n = (x / HALF as f64 + 1.) * 0.5;
                let window = if (0. ..=1.).contains(&n) {
                    0.42 - 0.5 * (2. * std::f64::consts::PI * n).cos_d()
                        + 0.08 * (4. * std::f64::consts::PI * n).cos_d()
                } else {
                    0.
                };
                *w = sinc * window;
                sum += *w;
            }
            for (out, w) in row.iter_mut().zip(weights) {
                *out = if sum.abs() > 1e-9 {
                    (w / sum) as f32
                } else {
                    0.
                };
            }
        }
        table
    }

    /// Appends interleaved input and returns every output frame it completes;
    /// `last` flushes the tail.
    pub fn process(&mut self, samples: &[f32], last: bool) -> Vec<f32> {
        let c = self.channels;
        self.input.extend_from_slice(samples);
        if last {
            self.input.extend(std::iter::repeat_n(0., HALF * c));
        }
        let frames = self.input.len() / c;
        let mut out = Vec::with_capacity(
            ((frames as f64 - self.position) / self.ratio).max(0.) as usize * c + c,
        );
        while (self.position.floor() as usize) + HALF < frames {
            let center = self.position.floor() as usize;
            let phase = ((self.position - center as f64) * PHASES as f64).round() as usize;
            let row = &self.table[phase * 2 * HALF..(phase + 1) * 2 * HALF];
            let first = center + 1 - HALF;
            // The window's inputs as one slice: no per-tap bounds checks; the
            // sum runs in the same order as before (bit-identical output).
            let window = &self.input[first * c..(first + 2 * HALF) * c];
            if c == 1 {
                out.push(row.iter().zip(window).fold(0f32, |v, (w, x)| v + w * x));
            } else {
                for channel in 0..c {
                    out.push(
                        row.iter()
                            .zip(window[channel..].iter().step_by(c))
                            .fold(0f32, |v, (w, x)| v + w * x),
                    );
                }
            }
            self.position += self.ratio;
        }
        // Keep the history the next window needs.
        let keep_from = (self.position.floor() as usize)
            .saturating_sub(HALF - 1)
            .min(frames);
        self.input.drain(..keep_from * c);
        self.position -= keep_from as f64;
        out
    }
}

type Kernels = std::collections::HashMap<(u32, u32), std::rc::Rc<Vec<f32>>>;

thread_local! {
    /// Resampling kernels by (from, to) rate.
    static TABLES: std::cell::RefCell<Kernels> = Default::default();
}

pub fn resample(samples: &[i16], channels: usize, from: u32, to: u32) -> Vec<f32> {
    let input: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.).collect();
    if from == to || input.is_empty() {
        return input;
    }
    let frames = input.len() / channels.max(1);
    let expected = ((frames as f64) * to as f64 / from as f64).ceil() as usize;
    let mut out = Resampler::new(from, to, channels).process(&input, true);
    out.resize(expected * channels, 0.);
    out
}

/// Input frames converted between frame yields by [`Pcm::from_vag_spread`]:
/// about 1-2 ms of wasm work per piece at 48 kHz.
const SPREAD_FRAMES: usize = 4096;

impl Pcm {
    /// An original decoded VAG at `rate`, keeping its loop markers.
    pub fn from_vag(audio: &atv_core::audio::Audio, rate: u32) -> Self {
        let channels = audio.info.channels as usize;
        let data = resample(&audio.samples, channels, audio.info.sample_rate, rate);
        Self::with_data(audio, rate, data)
    }

    /// [`Pcm::from_vag`] converted in pieces with a frame yield between them,
    /// for loads that run while a ride is drawing (remote riders' engines).
    /// The streaming resampler matches the one-read conversion to 1e-5.
    pub async fn from_vag_spread(audio: &atv_core::audio::Audio, rate: u32) -> Self {
        let channels = (audio.info.channels as usize).max(1);
        let from = audio.info.sample_rate;
        if from == rate || audio.samples.is_empty() {
            return Self::from_vag(audio, rate);
        }
        let frames = audio.samples.len() / channels;
        let expected = ((frames as f64) * rate as f64 / from as f64).ceil() as usize;
        let mut resampler = Resampler::new(from, rate, channels);
        let mut data = Vec::with_capacity(expected * channels + channels);
        let pieces: Vec<&[i16]> = audio.samples.chunks(SPREAD_FRAMES * channels).collect();
        for (i, piece) in pieces.iter().enumerate() {
            let input: Vec<f32> = piece.iter().map(|&s| s as f32 / 32768.).collect();
            data.extend(resampler.process(&input, i + 1 == pieces.len()));
            crate::yield_frame().await;
        }
        data.resize(expected * channels, 0.);
        Self::with_data(audio, rate, data)
    }

    fn with_data(audio: &atv_core::audio::Audio, rate: u32, data: Vec<f32>) -> Self {
        let channels = audio.info.channels as usize;
        let scale = rate as f64 / audio.info.sample_rate as f64;
        Self {
            channels,
            frames: data.len() / channels,
            data,
            loop_markers: match (audio.info.loop_start, audio.info.loop_end) {
                (Some(start), Some(end)) => Some((start as f64 * scale, end as f64 * scale)),
                _ => None,
            },
            rate,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Sfx,
    Music,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Looping {
    Once,
    /// Loop between the VAG markers, or the whole buffer without them.
    Markers,
    /// Loop the whole buffer.
    Whole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId {
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy)]
struct Ramp {
    from: f32,
    to: f32,
    start: u64,
    frames: u64,
}

impl Ramp {
    fn at(&self, clock: u64) -> f32 {
        if clock <= self.start {
            return self.from;
        }
        let t = ((clock - self.start) as f32 / self.frames.max(1) as f32).min(1.);
        self.from + (self.to - self.from) * t
    }
}

struct Voice {
    pcm: Arc<Pcm>,
    position: f64,
    rate: f64,
    gains: [f32; 2],
    loop_range: Option<(f64, f64)>,
    bus: Bus,
    started: u64,
    fade: Option<Ramp>,
    stop_at: Option<u64>,
    generation: u32,
    label: &'static str,
}

/// A parameterless Web Audio BiquadFilter lowpass (Q in dB, per the spec).
#[derive(Clone, Copy, Default)]
pub(crate) struct Lowpass {
    b: [f32; 3],
    a: [f32; 2],
    x: [[f32; 2]; 2],
    y: [[f32; 2]; 2],
}

impl Lowpass {
    pub(crate) fn new(frequency: f32, q_db: f32, rate: u32) -> Self {
        let w0 = 2. * std::f32::consts::PI * frequency / rate as f32;
        let alpha = w0.sin_d() / (2. * 10f32.powf_d(q_db / 20.));
        let cos = w0.cos_d();
        let a0 = 1. + alpha;
        let b1 = (1. - cos) / a0;
        Self {
            b: [b1 / 2., b1, b1 / 2.],
            a: [-2. * cos / a0, (1. - alpha) / a0],
            ..Default::default()
        }
    }
    pub(crate) fn process(&mut self, channel: usize, x: f32) -> f32 {
        let [x1, x2] = self.x[channel];
        let [y1, y2] = self.y[channel];
        let y = self.b[0] * x + self.b[1] * x1 + self.b[2] * x2 - self.a[0] * y1 - self.a[1] * y2;
        self.x[channel] = [x, x1];
        self.y[channel] = [y, y1];
        y
    }
}

/// Enhanced echo: a lowpass and two damped feedback delays (left/right) fed
/// once from the SFX bus and mixed back at the send level.
struct Echo {
    damp: Lowpass,
    lines: [(Vec<[f32; 2]>, usize); 2],
    feedback: f32,
    send: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoPreset {
    Stadium,
    Canyon,
}

impl Echo {
    fn new(preset: EchoPreset, rate: u32) -> Self {
        let (delays, feedback, damping, send) = match preset {
            EchoPreset::Stadium => ([0.083, 0.127], 0.32, 2800., 0.16),
            EchoPreset::Canyon => ([0.21, 0.29], 0.38, 2200., 0.14),
        };
        let line = |seconds: f32| (vec![[0.; 2]; (seconds * rate as f32).round() as usize], 0);
        Self {
            damp: Lowpass::new(damping, 1., rate),
            lines: [line(delays[0]), line(delays[1])],
            feedback,
            send,
        }
    }
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let damped = [
            self.damp.process(0, input[0]),
            self.damp.process(1, input[1]),
        ];
        let mut out = [0.; 2];
        for (channel, (line, at)) in self.lines.iter_mut().enumerate() {
            let delayed = line[*at];
            line[*at] = [
                damped[0] + self.feedback * delayed[0],
                damped[1] + self.feedback * delayed[1],
            ];
            *at = (*at + 1) % line.len();
            // ChannelMerger inputs down-mix each stereo delay output to mono.
            out[channel] = (delayed[0] + delayed[1]) * 0.5 * self.send;
        }
        out
    }
}

/// Levels measured while rendering (the parity tool reads these).
#[derive(Clone, Copy, Debug, Default)]
pub struct Meter {
    pub frames: u64,
    pub sum_squares: [f64; 2],
    pub peak: f32,
}

pub struct Mixer {
    pub rate: u32,
    clock: u64,
    voices: Vec<Option<Voice>>,
    generations: Vec<u32>,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub active: bool,
    /// Music stays active in the pause menu, but stops on focus loss.
    pub music_active: bool,
    echo: Option<Echo>,
    sfx_scratch: Vec<[f32; 2]>,
    music_scratch: Vec<[f32; 2]>,
    pub meter: Meter,
    /// Streamed soundtrack frames at the output rate (the music bus).
    music: std::collections::VecDeque<[f32; 2]>,
    /// Gain on the streamed soundtrack only (a race's hold fade).
    pub stream_gain: f32,
    /// The Jukebox paused the soundtrack: its queue waits where it is.
    pub stream_paused: bool,
    pub music_underruns: u32,
    /// Seconds spent in `render`, for CPU reports.
    pub render_seconds: f64,
    /// Voice start log: (output frame, label, frames, rate) for parity.
    pub log: Option<Vec<(u64, &'static str, usize, f32)>>,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            clock: 0,
            voices: Vec::new(),
            generations: Vec::new(),
            sfx_volume: 1.,
            music_volume: MUSIC_VOLUME,
            active: true,
            music_active: true,
            echo: None,
            sfx_scratch: Vec::new(),
            music_scratch: Vec::new(),
            meter: Meter::default(),
            music: Default::default(),
            stream_gain: 1.,
            stream_paused: false,
            music_underruns: 0,
            render_seconds: 0.,
            log: None,
        }
    }

    pub fn seconds(&self) -> f64 {
        self.clock as f64 / self.rate as f64
    }

    pub fn set_echo(&mut self, preset: Option<EchoPreset>) {
        self.echo = preset.map(|p| Echo::new(p, self.rate));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &mut self,
        pcm: &Arc<Pcm>,
        looping: Looping,
        gains: [f32; 2],
        rate: f32,
        bus: Bus,
        fade_in: bool,
        label: &'static str,
    ) -> VoiceId {
        let loop_range = match looping {
            Looping::Once => None,
            Looping::Markers => Some(pcm.loop_markers.unwrap_or((0., pcm.frames as f64))),
            Looping::Whole => Some((0., pcm.frames as f64)),
        };
        let slot = self
            .voices
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.voices.push(None);
                self.generations.push(0);
                self.voices.len() - 1
            });
        self.generations[slot] = self.generations[slot].wrapping_add(1);
        let generation = self.generations[slot];
        if let Some(log) = &mut self.log {
            log.push((self.clock, label, pcm.frames, rate));
        }
        self.voices[slot] = Some(Voice {
            pcm: pcm.clone(),
            position: 0.,
            rate: rate as f64,
            gains,
            loop_range,
            bus,
            started: self.clock,
            fade: fade_in.then_some(Ramp {
                from: 0.,
                to: 1.,
                start: self.clock,
                frames: (CROSSFADE * self.rate as f32) as u64,
            }),
            stop_at: None,
            generation,
            label,
        });
        VoiceId {
            slot: slot as u32,
            generation,
        }
    }

    fn voice(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices
            .get_mut(id.slot as usize)?
            .as_mut()
            .filter(|v| v.generation == id.generation)
    }

    pub fn playing(&self, id: Option<VoiceId>) -> bool {
        id.is_some_and(|id| {
            self.voices
                .get(id.slot as usize)
                .and_then(Option::as_ref)
                .is_some_and(|v| v.generation == id.generation && v.stop_at.is_none())
        })
    }

    /// Seconds since the voice started (`currentTime - started`).
    pub fn elapsed(&self, id: VoiceId) -> Option<f32> {
        let voice = self.voices.get(id.slot as usize)?.as_ref()?;
        (voice.generation == id.generation)
            .then(|| (self.clock - voice.started) as f32 / self.rate as f32)
    }

    pub fn stop(&mut self, id: VoiceId) {
        if let Some(voice) = self.voices.get_mut(id.slot as usize)
            && voice
                .as_ref()
                .is_some_and(|v| v.generation == id.generation)
        {
            *voice = None;
        }
    }

    /// Ramp a voice to silence over the crossfade and then stop it.
    pub fn fade_out(&mut self, id: VoiceId) {
        let (clock, frames) = (self.clock, (CROSSFADE * self.rate as f32) as u64);
        if let Some(voice) = self.voice(id) {
            let from = voice.fade.map_or(1., |f| f.at(clock));
            voice.fade = Some(Ramp {
                from,
                to: 0.,
                start: clock,
                frames,
            });
            voice.stop_at = Some(clock + frames + frames / 12);
        }
    }

    pub fn set_gains(&mut self, id: VoiceId, gains: [f32; 2]) {
        if let Some(voice) = self.voice(id) {
            voice.gains = gains;
        }
    }

    pub fn set_rate(&mut self, id: VoiceId, rate: f32) {
        if let Some(voice) = self.voice(id) {
            voice.rate = rate as f64;
        }
    }

    /// Queues interleaved stereo soundtrack frames.
    pub fn push_music(&mut self, frames: &[f32]) {
        self.music.extend(frames.as_chunks::<2>().0.iter().copied());
    }

    /// Seconds of soundtrack still queued.
    pub fn music_queued(&self) -> f32 {
        self.music.len() as f32 / self.rate as f32
    }

    pub fn clear_music(&mut self) {
        self.music.clear();
    }

    pub fn voices(&self) -> usize {
        self.voices.iter().flatten().count()
    }

    pub fn stop_all(&mut self) {
        self.voices.iter_mut().for_each(|v| *v = None);
    }

    /// Mixes `out.len() / 2` interleaved stereo frames.
    pub fn render(&mut self, out: &mut [f32]) {
        let started = web_time::Instant::now();
        self.mix(out);
        self.render_seconds += started.elapsed().as_secs_f64();
    }

    fn mix(&mut self, out: &mut [f32]) {
        if !self.active && !self.music_active {
            out.fill(0.);
            return;
        }
        let frames = out.len() / 2;
        let mut sfx = std::mem::take(&mut self.sfx_scratch);
        let mut music = std::mem::take(&mut self.music_scratch);
        sfx.resize(frames, [0.; 2]);
        music.resize(frames, [0.; 2]);
        sfx.fill([0.; 2]);
        music.fill([0.; 2]);
        for slot in self.voices.iter_mut() {
            let Some(voice) = slot.as_mut() else {
                continue;
            };
            let target = if voice.bus == Bus::Sfx {
                &mut sfx
            } else {
                &mut music
            };
            let pcm = &*voice.pcm;
            let mut ended = false;
            for (i, frame) in target.iter_mut().enumerate() {
                let clock = self.clock + i as u64;
                if voice.stop_at.is_some_and(|at| clock >= at) {
                    ended = true;
                    break;
                }
                if let Some((start, end)) = voice.loop_range
                    && end > start
                {
                    while voice.position >= end {
                        voice.position -= end - start;
                    }
                } else if voice.position >= pcm.frames as f64 {
                    ended = true;
                    break;
                }
                let index = voice.position.floor() as usize;
                let t = (voice.position - index as f64) as f32;
                let fade = voice.fade.map_or(1., |f| f.at(clock));
                let sample = |c: usize| {
                    let a = pcm.data[index.min(pcm.frames - 1) * pcm.channels + c];
                    let next = if index + 1 < pcm.frames {
                        index + 1
                    } else if let Some((start, _)) = voice.loop_range {
                        start as usize
                    } else {
                        index
                    };
                    let b = pcm.data[next.min(pcm.frames - 1) * pcm.channels + c];
                    a + (b - a) * t
                };
                let (left, right) = if pcm.channels == 1 {
                    let s = sample(0);
                    (s, s)
                } else {
                    (sample(0), sample(1))
                };
                frame[0] += left * voice.gains[0] * fade;
                frame[1] += right * voice.gains[1] * fade;
                voice.position += voice.rate;
            }
            if ended {
                *slot = None;
            }
        }
        if self.music_active && !self.stream_paused && !self.music.is_empty() {
            for frame in music.iter_mut() {
                match self.music.pop_front() {
                    Some(sample) => {
                        frame[0] += sample[0] * self.stream_gain;
                        frame[1] += sample[1] * self.stream_gain;
                    }
                    None => {
                        self.music_underruns += 1;
                        break;
                    }
                }
            }
        }
        let sfx_gain = if self.active { self.sfx_volume } else { 0. };
        for (i, out) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let bus = [sfx[i][0] * sfx_gain, sfx[i][1] * sfx_gain];
            let echo = self.echo.as_mut().map_or([0.; 2], |e| e.process(bus));
            for c in 0..2 {
                let effects = if self.active { bus[c] + echo[c] } else { 0. };
                let soundtrack = if self.music_active { music[i][c] } else { 0. };
                let value = MASTER * (effects + self.music_volume * soundtrack);
                out[c] = value;
                self.meter.sum_squares[c] += f64::from(value * value);
                self.meter.peak = self.meter.peak.max(value.abs());
            }
        }
        self.meter.frames += frames as u64;
        self.clock += frames as u64;
        self.sfx_scratch = sfx;
        self.music_scratch = music;
    }

    /// Label, length (frames), playback rate and gains of every live voice.
    pub fn snapshot(&self) -> Vec<(&'static str, usize, f32, [f32; 2])> {
        self.voices
            .iter()
            .flatten()
            .map(|v| (v.label, v.pcm.frames, v.rate as f32, v.gains))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize) -> Arc<Pcm> {
        Arc::new(Pcm {
            channels: 1,
            frames,
            data: (0..frames).map(|i| (i as f32 * 0.1).sin_d()).collect(),
            loop_markers: None,
            rate: 48000,
        })
    }

    #[test]
    fn one_shot_ends_and_loops_wrap() {
        let mut mixer = Mixer::new(48000);
        let once = mixer.play(
            &tone(100),
            Looping::Once,
            [1., 1.],
            1.,
            Bus::Sfx,
            false,
            "t",
        );
        let looped = mixer.play(
            &tone(100),
            Looping::Whole,
            [1., 0.],
            1.,
            Bus::Sfx,
            false,
            "t",
        );
        let mut out = vec![0.; 512];
        mixer.render(&mut out);
        assert!(!mixer.playing(Some(once)));
        assert!(mixer.playing(Some(looped)));
        assert!((mixer.elapsed(looped).unwrap() - 256. / 48000.).abs() < 1e-6);
        assert!(out[2 * 200 + 1].abs() < 1e-6, "right silent after one-shot");
        assert!(out[2 * 205].abs() > 0.);
    }

    #[test]
    fn pause_keeps_music_and_focus_loss_freezes_playback() {
        let mut mixer = Mixer::new(48000);
        let voice = mixer.play(
            &tone(100),
            Looping::Whole,
            [1.; 2],
            1.,
            Bus::Sfx,
            false,
            "test",
        );
        mixer.push_music(&[0.25; 32]);
        mixer.active = false;
        let mut out = [0.; 16];
        mixer.render(&mut out);
        assert!(
            out.iter()
                .all(|v| (*v - MASTER * MUSIC_VOLUME * 0.25).abs() < 1e-6)
        );
        let queued = mixer.music_queued();
        let elapsed = mixer.elapsed(voice);
        let clock = mixer.seconds();
        mixer.music_active = false;
        mixer.render(&mut out);
        assert_eq!(out, [0.; 16]);
        assert_eq!(mixer.music_queued(), queued);
        assert_eq!(mixer.elapsed(voice), elapsed);
        assert_eq!(mixer.seconds(), clock);
        mixer.active = true;
        mixer.music_active = true;
        mixer.render(&mut out);
        assert!(out.iter().any(|v| *v > MASTER * MUSIC_VOLUME * 0.25));
        assert!(mixer.seconds() > clock);
    }

    #[test]
    fn streaming_resampling_matches_one_read_across_chunk_boundaries() {
        let input: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.17).sin_d()).collect();
        let mut complete = Resampler::new(22050, 48000, 2);
        let expected = complete.process(&input, true);
        let mut streaming = Resampler::new(22050, 48000, 2);
        let mut actual = Vec::new();
        for (i, chunk) in input.chunks(74).enumerate() {
            actual.extend(streaming.process(chunk, (i + 1) * 74 >= input.len()));
        }
        assert_eq!(actual.len(), expected.len());
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() < 1e-5)
        );
    }

    #[test]
    fn spread_vag_conversion_matches_one_read() {
        let samples: Vec<i16> = (0..11_000)
            .map(|i| ((i as f32 * 0.031).sin_d() * 12000.) as i16)
            .collect();
        let audio = atv_core::audio::Audio {
            info: atv_core::audio::AudioInfo {
                sample_rate: 22050,
                channels: 1,
                frames: samples.len(),
                loop_start: Some(100),
                loop_end: Some(10_000),
            },
            samples,
        };
        let complete = Pcm::from_vag(&audio, 48000);
        let mut future = std::pin::pin!(Pcm::from_vag_spread(&audio, 48000));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let mut polls = 0;
        let spread = loop {
            polls += 1;
            if let std::task::Poll::Ready(pcm) = future.as_mut().poll(&mut context) {
                break pcm;
            }
        };
        assert!(polls > 2, "conversion is spread across frames");
        assert_eq!(spread.frames, complete.frames);
        assert_eq!(spread.loop_markers, complete.loop_markers);
        assert!(
            spread
                .data
                .iter()
                .zip(&complete.data)
                .all(|(a, b)| (a - b).abs() < 1e-5)
        );
    }

    #[test]
    fn resampling_keeps_level_and_length() {
        let samples: Vec<i16> = (0..2205)
            .map(|i| {
                ((i as f32 * 2. * std::f32::consts::PI * 441. / 22050.).sin_d() * 16000.) as i16
            })
            .collect();
        let out = resample(&samples, 1, 22050, 48000);
        assert_eq!(out.len(), (2205f64 * 48000. / 22050.).ceil() as usize);
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let input: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.).collect();
        assert!((rms(&out[200..4000]) - rms(&input[100..1800])).abs() < 0.01);
    }
}
