//! Rider stunt input and clip clocks, USA v2.00 002d9530/002da6c0/002d8ba0.
//! Scoring, profile unlocks and crash transitions are separate systems.
use crate::{Result, engine_audio::OriginalRandom};

pub const SUFFIXES: [&str; 24] = [
    "CirUp",
    "CirDown",
    "CirLeft",
    "CirRight",
    "TriUp",
    "TriDown",
    "TriLeft",
    "TriRight",
    "CirTriUp",
    "CirTriDown",
    "CirTriLeft",
    "CirTriRight",
    "CirL1Up",
    "CirL1Down",
    "CirL1Left",
    "CirL1Right",
    "TriL1Up",
    "TriL1Down",
    "TriL1Left",
    "TriL1Right",
    "CirTriL1Up",
    "CirTriL1Down",
    "CirTriL1Left",
    "CirTriL1Right",
];
pub const NAMES: [&str; 24] = [
    "Nac Nac",
    "No Footed Can Can",
    "Suicide No Hander",
    "Bar Hop",
    "Heel Clicker",
    "Cliff Hanger",
    "Nothing",
    "Point Back",
    "Cat Nac",
    "Cordova",
    "Scorpion",
    "Double Heart Attack",
    "Indian Air",
    "Disco Can",
    "Tsunami",
    "Shaolin",
    "One Handed Indian Air",
    "Tweak Air",
    "Lazy Boy",
    "Boogie Nights",
    "Heart Attack Indian Air",
    "One Footed Cordova",
    "Holy Man",
    "Sidewinder",
];
// Exact-build minimum main-clip seconds before hold/another stunt is accepted.
// These are control tuning constants, not motion samples or executable bytes.
pub const MIN_TIME: [f32; 24] = [
    0.91, 0.83, 0.66, 1., 0.91, 0.83, 0.66, 0.91, 0.83, 1., 1.08, 1.08, 0.83, 1., 0.91, 1.07, 1.16,
    0.83, 0.91, 1., 0.91, 1., 0.66, 1.16,
];

#[derive(Debug, Clone, Copy, Default)]
pub struct Input {
    /// Circle=1, Triangle=2, L1=4. Direction axes follow physical stick signs.
    pub modifiers: u8,
    pub axes: [f32; 2],
    pub grounded: bool,
    pub horizontal_speed_squared: f32,
    pub crashed: bool,
    pub disabled: bool,
}
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Frame {
    pub id: usize,
    pub name: &'static str,
    pub holding: bool,
    pub main_time: f32,
    pub hold_time: f32,
    pub started: bool,
}
impl Frame {
    /// Ground contact while holding, or more than/equal to 0.5s from the main
    /// clip end, triggers original crash type4 (001e8690 + 002d8a20).
    pub fn landing_safe(&self, duration: f32) -> bool {
        !self.holding && duration - self.main_time < 0.5
    }
    /// Native single-clip phase after snapshot selection (002d8e50 states 4/5).
    pub fn phase(&self, duration: f32, looping: bool, snapshot: bool, noise: f32) -> f32 {
        let phase = if snapshot {
            0.
        } else if !self.holding {
            self.main_time / duration
        } else if looping {
            0.4 + 3. * noise
        } else {
            self.hold_time / duration
        };
        phase.clamp(0., 1.)
    }
}
pub struct State {
    pub active: Option<usize>,
    main_time: f32,
    hold_time: f32,
    accepted_mask: u8,
    ready: bool,
    pub cooldown: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            active: None,
            main_time: 0.,
            hold_time: 0.,
            accepted_mask: 0xff,
            ready: true,
            cooldown: 0.,
        }
    }
}
/// Original direction priority: vertical wins beyond 0.25 once magnitude >0.5.
pub fn direction(axes: [f32; 2]) -> Option<usize> {
    let [x, y] = axes;
    if x * x + y * y <= 0.25 {
        None
    } else if y < -0.25 {
        Some(0)
    } else if y > 0.25 {
        Some(1)
    } else if x < 0. {
        Some(2)
    } else {
        Some(3)
    }
}
/// 002d9870 uses D-pad Up/Down/Left/Right in that priority only when the
/// analog stick does not exceed its radial threshold. Bits follow that order.
pub fn direction_with_buttons(axes: [f32; 2], buttons: u8) -> Option<usize> {
    direction(axes).or_else(|| (0..4).find(|bit| buttons & (1 << bit) != 0))
}
impl State {
    pub fn reset(&mut self) {
        *self = Self {
            cooldown: 2.,
            ..Self::default()
        };
    }
    pub fn step(&mut self, input: Input, dt: f32, durations: &[f32; 24]) -> Result<Option<Frame>> {
        self.step_controls(input, 0, dt, durations, true)
    }
    /// Control acceptance continues during a forced landing action, but
    /// 002d8ba0 pauses both stunt clocks while its action mode6 has priority.
    pub fn step_controls(
        &mut self,
        input: Input,
        direction_buttons: u8,
        dt: f32,
        durations: &[f32; 24],
        advance_clocks: bool,
    ) -> Result<Option<Frame>> {
        if !dt.is_finite()
            || dt < 0.
            || !input.horizontal_speed_squared.is_finite()
            || input.axes.iter().any(|x| !x.is_finite())
            || input.modifiers > 7
            || direction_buttons > 15
            || durations.iter().any(|x| !x.is_finite() || *x <= 0.)
        {
            return Err("invalid stunt input or clip duration".into());
        }
        // Vehicle 001e5260 decrements the reset timer without clamping.
        if self.cooldown > 0. {
            self.cooldown -= dt;
        }
        let mut started = false;
        let mut holding = false;
        let mask = input.modifiers;
        if self.accepted_mask != self.accepted_mask & mask || input.horizontal_speed_squared <= 16.
        {
            self.accepted_mask = 0xff;
            if mask == 0 || mask == 4 {
                self.ready = true;
            } else if self.ready
                && !input.grounded
                && !input.crashed
                && !input.disabled
                && self.cooldown <= 0.
                && input.horizontal_speed_squared > 16.
            {
                let group = match mask {
                    1 => 0,
                    2 => 4,
                    3 => 8,
                    5 => 12,
                    6 => 16,
                    7 => 20,
                    _ => unreachable!(),
                };
                if let Some(direction) = direction_with_buttons(input.axes, direction_buttons)
                    && self.active.is_none_or(|id| self.main_time >= MIN_TIME[id])
                {
                    self.active = Some(group + direction);
                    self.main_time = 0.;
                    self.hold_time = 0.;
                    self.accepted_mask = mask;
                    self.ready = false;
                    started = true;
                }
            }
        } else if let Some(id) = self.active {
            holding = self.main_time >= MIN_TIME[id];
        }
        if input.crashed || input.disabled {
            self.active = None;
            self.accepted_mask = 0xff;
        }
        if let Some(id) = self.active {
            if self.main_time >= durations[id] {
                self.active = None;
                self.accepted_mask = 0xff;
                return Ok(None);
            }
            if advance_clocks {
                if holding {
                    self.hold_time += dt;
                } else {
                    self.main_time += dt;
                }
            }
            return Ok(Some(Frame {
                id,
                name: NAMES[id],
                holding,
                main_time: self.main_time,
                hold_time: self.hold_time,
                started,
            }));
        }
        Ok(None)
    }
}

/// Original 003454c0 one-dimensional gradient noise. Initialization 00345650
/// consumes six libc draws per lattice point, then 255 permutation swaps.
/// The caller's seed is local: full-game shared RNG ordering remains unknown.
pub struct HoldNoise {
    permutation: [usize; 256],
    gradients: [f32; 256],
}
impl HoldNoise {
    pub fn new(random: &mut OriginalRandom) -> Self {
        let mut permutation = std::array::from_fn(|i| i);
        let mut gradients = [0.; 256];
        for gradient in &mut gradients {
            *gradient = ((random.next_u31() % 512) as i32 - 256) as f32 / 256.;
            for _ in 0..5 {
                random.next_u31();
            }
        }
        for i in (1..256).rev() {
            permutation.swap(i, (random.next_u31() % 256) as usize);
        }
        Self {
            permutation,
            gradients,
        }
    }
    pub fn sample(&self, time: f32) -> f32 {
        let t = time + 4096.;
        let lattice = t as usize;
        let fraction = t - lattice as f32;
        let a = fraction * self.gradients[self.permutation[lattice & 255]];
        let b = (fraction - 1.) * self.gradients[self.permutation[(lattice + 1) & 255]];
        a + (3. - fraction * 2.) * fraction * fraction * (b - a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digital_direction_fallback_has_original_priority_and_analog_precedence() {
        for buttons in 0..16 {
            assert_eq!(
                direction_with_buttons([0.5, 0.], buttons),
                (0..4).find(|i| buttons & (1 << i) != 0)
            );
            assert_eq!(direction_with_buttons([0.5001, 0.], buttons), Some(3));
            assert_eq!(direction_with_buttons([-1., -0.251], buttons), Some(0));
        }
        for (mask_group, modifiers) in [1, 2, 3, 5, 6, 7].into_iter().enumerate() {
            for direction in 0..4 {
                let frame = State::default()
                    .step_controls(
                        Input {
                            modifiers,
                            axes: [0.; 2],
                            ..airborne()
                        },
                        1 << direction,
                        1. / 60.,
                        &[3.; 24],
                        true,
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(frame.id, mask_group * 4 + direction);
            }
        }
    }
    fn airborne() -> Input {
        Input {
            modifiers: 1,
            axes: [0., -1.],
            horizontal_speed_squared: 100.,
            ..Input::default()
        }
    }
    #[test]
    fn all_modifier_direction_combinations_select_original_order() {
        for (group, mask) in [1, 2, 3, 5, 6, 7].into_iter().enumerate() {
            for (direction, axes) in [[0., -1.], [0., 1.], [-1., 0.], [1., 0.]]
                .into_iter()
                .enumerate()
            {
                let frame = State::default()
                    .step(
                        Input {
                            modifiers: mask,
                            axes,
                            ..airborne()
                        },
                        1. / 60.,
                        &[3.; 24],
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(frame.id, group * 4 + direction);
                assert!(frame.started);
                assert!(!frame.holding);
            }
        }
        assert_eq!(direction([0.5, 0.]), None);
        assert_eq!(direction([1., -0.26]), Some(0));
        assert_eq!(direction([-1., 0.25]), Some(2));
    }
    #[test]
    fn rejects_ground_slow_crash_reset_and_invalid_inputs() {
        for input in [
            Input {
                grounded: true,
                ..airborne()
            },
            Input {
                horizontal_speed_squared: 16.,
                ..airborne()
            },
            Input {
                crashed: true,
                ..airborne()
            },
            Input {
                disabled: true,
                ..airborne()
            },
        ] {
            assert!(
                State::default()
                    .step(input, 1. / 60., &[3.; 24])
                    .unwrap()
                    .is_none()
            );
        }
        let mut state = State::default();
        state.reset();
        for _ in 0..119 {
            assert!(
                state
                    .step(airborne(), 1. / 60., &[3.; 24])
                    .unwrap()
                    .is_none()
            );
        }
        for _ in 0..3 {
            state.step(airborne(), 1. / 60., &[3.; 24]).unwrap();
        }
        assert_eq!(state.active, Some(0));
        assert!(
            state
                .step(
                    Input {
                        axes: [f32::NAN, 0.],
                        ..airborne()
                    },
                    1. / 60.,
                    &[3.; 24]
                )
                .is_err()
        );
    }
    #[test]
    fn hold_freezes_main_clock_release_resumes_and_requires_rearm() {
        let mut state = State::default();
        let dt = 1. / 60.;
        let mut frame = state.step(airborne(), dt, &[3.; 24]).unwrap().unwrap();
        while !frame.holding {
            frame = state.step(airborne(), dt, &[3.; 24]).unwrap().unwrap();
        }
        assert!(frame.main_time >= MIN_TIME[0]);
        let frozen = frame.main_time;
        for _ in 0..60 {
            frame = state.step(airborne(), dt, &[3.; 24]).unwrap().unwrap();
            assert_eq!(frame.main_time, frozen);
        }
        assert!(frame.hold_time > 1.);
        frame = state
            .step(
                Input {
                    modifiers: 2,
                    ..airborne()
                },
                dt,
                &[3.; 24],
            )
            .unwrap()
            .unwrap();
        assert_eq!(frame.id, 0);
        assert!(!frame.holding);
        assert!(frame.main_time > frozen);
        state
            .step(
                Input {
                    modifiers: 0,
                    ..airborne()
                },
                dt,
                &[3.; 24],
            )
            .unwrap();
        frame = state
            .step(
                Input {
                    modifiers: 2,
                    ..airborne()
                },
                dt,
                &[3.; 24],
            )
            .unwrap()
            .unwrap();
        assert_eq!(frame.id, 4);
        assert!(frame.started);
        assert_eq!(frame.hold_time, 0.);
        for _ in 0..200 {
            state
                .step(
                    Input {
                        modifiers: 0,
                        ..airborne()
                    },
                    dt,
                    &[3.; 24],
                )
                .unwrap();
        }
        assert_eq!(state.active, None);
    }
    #[test]
    fn landing_requires_released_stunt_with_less_than_half_second_remaining() {
        let mut frame = Frame {
            id: 0,
            name: NAMES[0],
            holding: false,
            main_time: 1.5,
            hold_time: 0.,
            started: false,
        };
        assert!(!frame.landing_safe(2.));
        frame.main_time = 1.5001;
        assert!(frame.landing_safe(2.));
        frame.holding = true;
        assert!(!frame.landing_safe(2.));
    }
    #[test]
    fn noise_has_native_lattice_zeroes_period_and_draw_count() {
        let mut random = OriginalRandom::with_seed(1);
        let noise = HoldNoise::new(&mut random);
        let mut reference = OriginalRandom::with_seed(1);
        for _ in 0..(256 * 6 + 255) {
            reference.next_u31();
        }
        assert_eq!(random.next_u31(), reference.next_u31());
        for t in [0., 1., 17., 255., 256.] {
            assert_eq!(noise.sample(t), 0.);
        }
        for t in [0.25, 17.75, 255.5] {
            assert_eq!(noise.sample(t), noise.sample(t + 256.));
            assert!(noise.sample(t).abs() <= 1.);
        }
    }
}
