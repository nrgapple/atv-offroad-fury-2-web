//! Authored synthesized one-shots (GameAudio in web/audio.js), rendered to
//! PCM for the mixer: the Alpha landing thump.
use super::mixer::{Lowpass, Pcm};
use atv_core::math::Det;

/// landingThump: a sine drop 78 -> 34 Hz over 0.3 s plus 0.16 s of fading
/// noise through a 900 Hz lowpass, under an exponential envelope from
/// `min(0.55, (severity - 8) / 80)` to 0.001 over 0.42 s; 0.45 s long. None
/// for landings too soft to hear. Mono, for the SFX bus.
pub fn landing_thump(severity: f32, rate: u32) -> Option<Pcm> {
    let level = ((f64::from(severity) - 8.).max(0.) / 80.).min(0.55);
    if level <= 0. {
        return None;
    }
    let rate_f = f64::from(rate);
    let frames = (rate_f * 0.45).ceil() as usize;
    let noise_frames = (rate_f * 0.16).ceil() as usize;
    let mut lowpass = Lowpass::new(900., 1., rate);
    let mut seed: u32 = 29;
    let mut phase = 0f64;
    let mut data = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = i as f64 / rate_f;
        let envelope = level * (0.001 / level).powf_d((t / 0.42).min(1.));
        let frequency = 78. * (34f64 / 78.).powf_d((t / 0.3).min(1.));
        let bass = phase.sin_d();
        phase += std::f64::consts::TAU * frequency / rate_f;
        let noise = if i < noise_frames {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(seed) / 2_147_483_648. - 1.) * (1. - i as f64 / noise_frames as f64) * 0.6
        } else {
            0.
        };
        let crunch = f64::from(lowpass.process(0, noise as f32));
        data.push(((bass + crunch) * envelope) as f32);
    }
    Some(Pcm {
        channels: 1,
        frames,
        data,
        loop_markers: None,
        rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_landings_are_silent_and_hard_ones_thump() {
        assert!(landing_thump(8., 48_000).is_none());
        let pcm = landing_thump(60., 48_000).unwrap();
        assert_eq!(pcm.frames, 21_600);
        let peak = pcm.data.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.3 && peak < 0.55 * 1.7, "{peak}");
        // The tail has decayed to the envelope floor.
        assert!(pcm.data[21_000..].iter().all(|v| v.abs() < 0.01));
    }
}
