//! Authored synthesized one-shots (GameAudio in web/audio.js), rendered to
//! PCM for the mixer: the Alpha landing thump and the Crazy/Gorn crash boom.
use super::mixer::{Lowpass, Pcm};
use atv_core::math::Det;

/// One synthesized thump: a sine sweep plus a burst of lowpassed noise under
/// an exponential envelope from `level` to 0.001 (GameAudio's oscillator
/// graphs, rendered to mono PCM).
struct Thump {
    level: f64,
    decay: f64,
    sweep: (f64, f64, f64),
    duration: f64,
    noise: (f64, u32, f64, bool, f32),
}

fn render(t: &Thump, rate: u32) -> Pcm {
    let rate_f = f64::from(rate);
    let frames = (rate_f * t.duration).ceil() as usize;
    let (noise_seconds, noise_seed, noise_gain, fade, cutoff) = t.noise;
    let noise_frames = (rate_f * noise_seconds).ceil() as usize;
    let mut lowpass = Lowpass::new(cutoff, 1., rate);
    let mut seed = noise_seed;
    let mut phase = 0f64;
    let mut data = Vec::with_capacity(frames);
    let (from, to, over) = t.sweep;
    for i in 0..frames {
        let time = i as f64 / rate_f;
        let envelope = t.level * (0.001 / t.level).powf_d((time / t.decay).min(1.));
        let frequency = from * (to / from).powf_d((time / over).min(1.));
        let bass = phase.sin_d();
        phase += std::f64::consts::TAU * frequency / rate_f;
        let noise = if i < noise_frames {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let shape = if fade {
                1. - i as f64 / noise_frames as f64
            } else {
                1.
            };
            (f64::from(seed) / 2_147_483_648. - 1.) * shape * noise_gain
        } else {
            0.
        };
        let crunch = f64::from(lowpass.process(0, noise as f32));
        data.push(((bass + crunch) * envelope) as f32);
    }
    Pcm {
        channels: 1,
        frames,
        data,
        loop_markers: None,
        rate,
    }
}

/// landingThump: a sine drop 78 -> 34 Hz over 0.3 s plus 0.16 s of fading
/// noise through a 900 Hz lowpass, under an exponential envelope from
/// `min(0.55, (severity - 8) / 80)` to 0.001 over 0.42 s; 0.45 s long. None
/// for landings too soft to hear. Mono, for the SFX bus.
pub fn landing_thump(severity: f32, rate: u32) -> Option<Pcm> {
    let level = ((f64::from(severity) - 8.).max(0.) / 80.).min(0.55);
    (level > 0.).then(|| {
        render(
            &Thump {
                level,
                decay: 0.42,
                sweep: (78., 34., 0.3),
                duration: 0.45,
                noise: (0.16, 29, 0.6, true, 900.),
            },
            rate,
        )
    })
}

/// crashBoom (Crazy/Gorn crashes): a sine drop 95 -> 28 Hz over 0.38 s plus
/// 0.2 s of noise through a 650 Hz lowpass, under an envelope from
/// `min(0.6, severity / 150)` to 0.001 over 0.48 s; 0.5 s long. Mono.
pub fn crash_boom(severity: f32, rate: u32) -> Option<Pcm> {
    let level = (f64::from(severity) / 150.).min(0.6);
    (level > 0.).then(|| {
        render(
            &Thump {
                level,
                decay: 0.48,
                sweep: (95., 28., 0.38),
                duration: 0.5,
                noise: (0.2, 73, 0.5, false, 650.),
            },
            rate,
        )
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
        let boom = crash_boom(60., 48_000).unwrap();
        assert_eq!(boom.frames, 24_000);
        let peak = boom.data.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.3 && peak < 0.8, "{peak}");
        assert!(crash_boom(0., 48_000).is_none());
    }
}
