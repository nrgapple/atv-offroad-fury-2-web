//! Alpha Speed & air muffle in the mixer (GameAudio.setHype in web/audio.js):
//! the music bus passes a lowpass whose cutoff falls from 22 kHz towards
//! 320 Hz and a level down to 0.7 as the muffle rises, and a looped filtered
//! noise "wind" rush plays on the SFX bus. Targets ease in like Web Audio's
//! setTargetAtTime.
use super::mixer::Lowpass;
use atv_core::math::Det;

/// A Web Audio BiquadFilter bandpass (Q linear, constant 0 dB peak).
#[derive(Clone, Copy, Default)]
struct Bandpass {
    b0: f32,
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Bandpass {
    fn set(&mut self, frequency: f32, q: f32, rate: u32) {
        let w0 = 2. * std::f32::consts::PI * frequency / rate as f32;
        let alpha = w0.sin_d() / (2. * q);
        let a0 = 1. + alpha;
        self.b0 = alpha / a0;
        self.a = [-2. * w0.cos_d() / a0, (1. - alpha) / a0];
    }
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x - self.b0 * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// One `setTargetAtTime` parameter, advanced per sample.
#[derive(Clone, Copy)]
struct Param {
    value: f32,
    target: f32,
    tc: f32,
}

impl Param {
    fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            tc: 0.1,
        }
    }
    fn advance(&mut self, frames: usize, rate: u32) {
        let k = 1. - (-(frames as f32) / (self.tc * rate as f32)).exp_d();
        self.value += (self.target - self.value) * k;
    }
}

struct Wind {
    noise: Vec<f32>,
    at: usize,
    band: Bandpass,
    gain: Param,
    frequency: Param,
}

pub struct HypeAudio {
    rate: u32,
    cutoff: Param,
    level: Param,
    filter: Lowpass,
    filtered_cutoff: f32,
    wind: Option<Wind>,
    /// The latest muffle amount (diagnostics: `musicMuffle`).
    pub muffle: f32,
}

/// Parameters are updated each block of this many frames (Web Audio's
/// render quantum).
const BLOCK: usize = 128;
const OPEN: f32 = 22_000.;

impl HypeAudio {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            cutoff: Param::new(OPEN),
            level: Param::new(1.),
            filter: Lowpass::new(OPEN.min(rate as f32 * 0.49), 0.7, rate),
            filtered_cutoff: OPEN,
            wind: None,
            muffle: 0.,
        }
    }

    /// setHype: `muffle` 0..1 closes the music, `wind` 0..1 the rush; `fast`
    /// snaps back (a hard landing).
    pub fn set(&mut self, muffle: f32, wind: f32, fast: bool) {
        let amount = muffle.clamp(0., 1.);
        self.muffle = amount;
        let tc = if fast { 0.02 } else { 0.06 };
        self.cutoff.target = OPEN * (320f32 / OPEN).powf_d(amount);
        self.cutoff.tc = tc;
        self.level.target = 1. - 0.3 * amount;
        self.level.tc = tc;
        let w = wind.clamp(0., 1.);
        if w > 0.001 && self.wind.is_none() {
            let frames = self.rate as usize * 2;
            let (mut seed, mut last) = (91u32, 0f32);
            let noise = (0..frames)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    last = last * 0.96 + (seed as f32 / 2_147_483_648. - 1.) * 0.2;
                    last
                })
                .collect();
            let mut band = Bandpass::default();
            band.set(500., 0.6, self.rate);
            self.wind = Some(Wind {
                noise,
                at: 0,
                band,
                gain: Param::new(0.),
                frequency: Param::new(500.),
            });
        }
        if let Some(wind) = &mut self.wind {
            wind.gain.target = 0.32 * w * w;
            wind.gain.tc = if fast { 0.03 } else { 0.15 };
            wind.frequency.target = 380. + 900. * w;
            wind.frequency.tc = 0.2;
        }
    }

    /// stopHype: the wind loop ends and the music opens at once.
    pub fn stop(&mut self) {
        self.wind = None;
        self.set(0., 0., true);
    }

    /// Whether the music filter is closed enough to process.
    fn filtering(&self) -> bool {
        self.cutoff.value < OPEN - 1. || self.cutoff.target < OPEN - 1. || self.level.value < 0.9999
    }

    /// Filters the music bus and adds the wind to the (pre-volume) SFX bus.
    pub fn process(&mut self, music: &mut [[f32; 2]], sfx: &mut [[f32; 2]]) {
        let rate = self.rate;
        if self.filtering() {
            for block in music.chunks_mut(BLOCK) {
                self.cutoff.advance(block.len(), rate);
                self.level.advance(block.len(), rate);
                let cutoff = self.cutoff.value.min(rate as f32 * 0.49);
                if (cutoff - self.filtered_cutoff).abs() > 0.5 {
                    let state = (self.filter.x, self.filter.y);
                    self.filter = Lowpass::new(cutoff, 0.7, rate);
                    (self.filter.x, self.filter.y) = state;
                    self.filtered_cutoff = cutoff;
                }
                let level = self.level.value;
                for frame in block.iter_mut() {
                    frame[0] = self.filter.process(0, frame[0]) * level;
                    frame[1] = self.filter.process(1, frame[1]) * level;
                }
            }
        }
        if let Some(wind) = &mut self.wind {
            if wind.gain.value < 1e-5 && wind.gain.target < 1e-5 {
                return;
            }
            for block in sfx.chunks_mut(BLOCK) {
                wind.gain.advance(block.len(), rate);
                wind.frequency.advance(block.len(), rate);
                wind.band.set(wind.frequency.value, 0.6, rate);
                for frame in block.iter_mut() {
                    let sample = wind.band.process(wind.noise[wind.at]) * wind.gain.value;
                    wind.at = (wind.at + 1) % wind.noise.len();
                    frame[0] += sample;
                    frame[1] += sample;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frequency: f32, frames: usize, rate: u32) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|i| {
                let v = (std::f32::consts::TAU * frequency * i as f32 / rate as f32).sin_d();
                [v, v]
            })
            .collect()
    }

    fn rms(v: &[[f32; 2]]) -> f32 {
        (v.iter().map(|f| f[0] * f[0]).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn muffle_closes_the_music_and_wind_rushes() {
        let rate = 48_000;
        let mut hype = HypeAudio::new(rate);
        // Open: the music passes untouched.
        let mut music = tone(2_000., 4_800, rate);
        let before = music.clone();
        hype.process(&mut music, &mut vec![[0.; 2]; 4_800]);
        assert_eq!(music, before);
        // Fully muffled for half a second: a 2 kHz tone nearly vanishes, a
        // 100 Hz one keeps about the 0.7 level.
        hype.set(1., 1., false);
        let mut silence = vec![[0.; 2]; 24_000];
        hype.process(&mut tone(2_000., 24_000, rate), &mut silence);
        let mut high = tone(2_000., 4_800, rate);
        let mut low = tone(100., 4_800, rate);
        let mut sfx = vec![[0.; 2]; 4_800];
        hype.process(&mut high, &mut sfx);
        hype.process(&mut low, &mut vec![[0.; 2]; 4_800]);
        assert!(rms(&high) < 0.02, "{}", rms(&high));
        let low_rms = rms(&low) / std::f32::consts::FRAC_1_SQRT_2;
        assert!((0.6..0.8).contains(&low_rms), "{low_rms}");
        assert!(rms(&sfx) > 0.005, "wind {}", rms(&sfx));
        hype.stop();
        assert!(hype.wind.is_none());
    }
}
