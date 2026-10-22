//! The Jukebox spectrum (a Web Audio AnalyserNode as web/music.js taps the
//! soundtrack: fftSize 64, smoothingTimeConstant 0.78, the default -100 to
//! -30 dB byte range): `getByteFrequencyData` over the latest 64 samples.
use atv_core::math::Det;

pub const FFT: usize = 64;
pub const BINS: usize = FFT / 2;
const SMOOTHING: f64 = 0.78;
const MIN_DB: f64 = -100.;
const MAX_DB: f64 = -30.;

#[derive(Clone, Debug)]
pub struct Analyser {
    smoothed: [f64; BINS],
}

impl Default for Analyser {
    fn default() -> Self {
        Self {
            smoothed: [0.; BINS],
        }
    }
}

impl Analyser {
    /// One getByteFrequencyData call on the latest `FFT` mono samples
    /// (oldest first).
    pub fn bytes(&mut self, samples: &[f32; FFT]) -> [u8; BINS] {
        let n = FFT as f64;
        let tau = std::f64::consts::TAU;
        let windowed: [f64; FFT] = std::array::from_fn(|i| {
            let t = i as f64 / n;
            let w = 0.42 - 0.5 * (tau * t).cos_d() + 0.08 * (2. * tau * t).cos_d();
            f64::from(samples[i]) * w
        });
        std::array::from_fn(|k| {
            let (mut re, mut im) = (0f64, 0f64);
            for (i, x) in windowed.iter().enumerate() {
                let (s, c) = (tau * (k * i) as f64 / n).sin_cos_d();
                re += x * c;
                im -= x * s;
            }
            let magnitude = (re * re + im * im).sqrt() / n;
            let v = SMOOTHING * self.smoothed[k] + (1. - SMOOTHING) * magnitude;
            self.smoothed[k] = if v.is_finite() { v } else { 0. };
            let db = 20. * self.smoothed[k].ln_d() / std::f64::consts::LN_10;
            (255. / (MAX_DB - MIN_DB) * (db - MIN_DB))
                .floor()
                .clamp(0., 255.) as u8
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_lights_its_bin_and_silence_none() {
        let mut silent = Analyser::default();
        assert_eq!(silent.bytes(&[0.; FFT]), [0; BINS]);
        let mut a = Analyser::default();
        // A quiet tone on bin 4 (4 cycles per 64 samples), held.
        let tone: [f32; FFT] = std::array::from_fn(|i| {
            0.01 * (std::f64::consts::TAU * 4. * i as f64 / FFT as f64).sin_d() as f32
        });
        let mut bytes = [0; BINS];
        for _ in 0..40 {
            bytes = a.bytes(&tone);
        }
        let loudest = (0..BINS).max_by_key(|&k| bytes[k]).unwrap();
        assert_eq!(loudest, 4);
        // 0.01 * 0.42 / 2 is about -53 dB: byte 171.
        assert!((165..=175).contains(&bytes[4]), "{bytes:?}");
        assert_eq!(bytes[16], 0, "{bytes:?}");
    }
}
