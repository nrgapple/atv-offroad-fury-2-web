//! Recovered rider body-impact sample selection and latch, USA v2.00.
use crate::{Result, physics_data::PhysicsData, wheel::Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub body_impact: Vec<String>,
    pub min_distance: f32,
    pub max_distance: f32,
}
impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let table = PhysicsData::parse(bytes)?;
        let value = |key: &str| {
            table
                .entries
                .iter()
                .find(|entry| {
                    entry.section.eq_ignore_ascii_case("BodyImpact")
                        && entry.key.eq_ignore_ascii_case(key)
                })
                .map(|entry| entry.value.as_str())
        };
        let count = value("NumSounds")
            .ok_or("original BodyImpact bank missing")?
            .parse::<usize>()
            .map_err(|_| "invalid BodyImpact count")?;
        if !(1..=70).contains(&count) {
            return Err("BodyImpact count outside original selector range".into());
        }
        let mut body_impact = Vec::with_capacity(count);
        for index in 1..=count {
            let name = value(&format!("Sound_{index}")).ok_or("BodyImpact sample missing")?;
            if name.len() > 260
                || name.is_empty()
                || name.contains("..")
                || name.starts_with(['/', '\\'])
            {
                return Err("invalid BodyImpact sample resource".into());
            }
            body_impact.push(name.to_owned());
        }
        //002d6380 initializes002ccf90 with100; the single-player constructor
        //retains min30/max100. Network/demo profiles are distinct source paths.
        Ok(Self {
            body_impact,
            min_distance: 30.,
            max_distance: 100.,
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ImpactInput {
    pub sampled_impact_delta: Vec3,
    pub quad_wreck_phase: Option<u32>,
    #[serde(default)]
    pub playing: bool,
}
#[derive(Deserialize)]
pub struct FrameRequest {
    pub impact: ImpactInput,
    pub position: Option<Vec3>,
    pub camera: crate::spatial_audio::CameraPose,
    pub previous_camera: crate::spatial_audio::CameraPose,
    pub dt: f32,
}
#[derive(Deserialize)]
pub struct BatchRequest {
    pub frames: Vec<FrameRequest>,
    pub playing: bool,
}
///002d9c10 actor sound velocity cache, distinct from physical body velocity.
#[derive(Default)]
pub struct RiderMotionCache {
    pub previous: Vec3,
    pub velocity: Vec3,
}
impl RiderMotionCache {
    pub fn update(&mut self, position: Vec3, reciprocal_dt: f32) -> Result<Vec3> {
        if position.iter().any(|v| !v.is_finite())
            || !reciprocal_dt.is_finite()
            || reciprocal_dt <= 0.
        {
            return Err("invalid rider audio motion cache".into());
        }
        if self.previous != [0.; 3] {
            self.velocity =
                std::array::from_fn(|i| reciprocal_dt * (position[i] - self.previous[i]));
        }
        self.previous = position;
        Ok(self.velocity)
    }
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Event {
    pub sample: usize,
    pub magnitude: f32,
    ///002cd420 refuses to start over a currently playing body sound. Selection
    ///and its random draw still happened under002cd2e0's initial latch gate.
    pub play: bool,
}
#[derive(Default)]
pub struct RiderImpactState {
    pub latched: bool,
}
impl RiderImpactState {
    ///002cd2e0 invokes002cd1f0 only under the native initial/retrigger gates.
    ///A callback consumes exactly one source random word when a sound triggers.
    pub fn advance(
        &mut self,
        input: ImpactInput,
        count: usize,
        mut random: impl FnMut() -> u32,
    ) -> Result<Option<Event>> {
        if !(1..=70).contains(&count) || input.sampled_impact_delta.iter().any(|v| !v.is_finite()) {
            return Err("invalid rider impact audio input".into());
        }
        if input.quad_wreck_phase == Some(0) {
            self.latched = false;
            return Ok(None);
        }
        let [x, y, z] = input.sampled_impact_delta;
        let magnitude = ((x * x + y * y) + z * z).sqrt();
        if !magnitude.is_finite() || magnitude >= i32::MAX as f32 {
            return Err("rider impact magnitude outside native integer range".into());
        }
        if (!self.latched && magnitude >= 0.3) || (magnitude >= 3. && !input.playing) {
            let word = random();
            if word > 0x7fff_ffff {
                return Err("rider impact random outside source range".into());
            }
            let choice = (magnitude as i32) / (70 / count) as i32 - 1
                + (((word as f32) * f32::from_bits(0x30000000)) * 3.) as i32;
            self.latched = true;
            Ok(Some(Event {
                sample: choice.clamp(0, (count - 1) as i32) as usize,
                magnitude,
                play: !input.playing,
            }))
        } else {
            Ok(None)
        }
    }
}

///001ef690 retains signed per-component maxima across contact substeps; ties
///leave the preceding value. This observer does not alter contact response.
pub fn sample_impact(output: &mut Vec3, substep: Vec3) {
    for (out, next) in output.iter_mut().zip(substep) {
        if out.abs() < next.abs() {
            *out = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_latch_retrigger_and_rng_calls() {
        let mut state = RiderImpactState::default();
        let mut words = 0;
        let mut run = |state: &mut RiderImpactState, magnitude, phase, playing| {
            state.advance(
                ImpactInput {
                    sampled_impact_delta: [magnitude, 0., 0.],
                    quad_wreck_phase: Some(phase),
                    playing,
                },
                4,
                || {
                    words += 1;
                    0
                },
            )
        };
        assert!(run(&mut state, 0.3, 0, false).unwrap().is_none());
        assert!(!state.latched);
        assert_eq!(run(&mut state, 0.3, 2, true).unwrap().unwrap().sample, 0);
        assert!(state.latched);
        assert!(run(&mut state, 2.99, 2, false).unwrap().is_none());
        assert!(run(&mut state, 3., 2, true).unwrap().is_none());
        assert!(run(&mut state, 3., 2, false).unwrap().is_some());
        assert!(run(&mut state, 40., 0, false).unwrap().is_none());
        assert!(!state.latched);
        assert_eq!(words, 2);
    }
    #[test]
    fn selector_clamps_max_random_and_strict_abs_ties() {
        let mut state = RiderImpactState::default();
        assert_eq!(
            state
                .advance(
                    ImpactInput {
                        sampled_impact_delta: [70., 0., 0.],
                        quad_wreck_phase: None,
                        playing: false
                    },
                    4,
                    || 0x7fff_ffff
                )
                .unwrap()
                .unwrap()
                .sample,
            3
        );
        let mut delta = [0.; 3];
        sample_impact(&mut delta, [3., -5., 1.]);
        sample_impact(&mut delta, [-3., 4., -2.]);
        assert_eq!(delta, [3., -5., -2.]);
    }
    #[test]
    fn bank_parser_keeps_order_and_rejects_unsafe_resources() {
        let config = Config::parse(
            b"[BodyImpact]\nNumSounds=2\nSound_1=authored/first.vag\nSound_2=authored/second.vag",
        )
        .unwrap();
        assert_eq!(
            config.body_impact,
            ["authored/first.vag", "authored/second.vag"]
        );
        assert!(Config::parse(b"[BodyImpact]\nNumSounds=1\nSound_1=../private").is_err());
    }
}
