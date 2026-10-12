//! Authored browser touch adaptation. Resolve against the current fixed-step
//! vehicle state; these gestures are not original PS2 controller behavior.
use crate::{Result, vehicle::VehicleInput};

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct TouchIntent {
    pub generation: u64,
    pub held: bool,
    /// Forward is positive. The same pullback becomes positive lean in air.
    pub axis: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TouchDrive {
    generation: Option<u64>,
    engaged: bool,
    reverse: bool,
    throttle: f32,
    brake: f32,
}
impl TouchDrive {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn resolve(
        &mut self,
        mut independent: VehicleInput,
        intent: Option<TouchIntent>,
        grounded: bool,
        speed: f32,
    ) -> Result<VehicleInput> {
        let Some(intent) = intent else {
            self.reset();
            independent.lean = independent.lean.clamp(-1., 1.);
            return Ok(independent);
        };
        if !intent.axis.is_finite() || !speed.is_finite() {
            return Err("nonfinite touch drive input".into());
        }
        if self.generation != Some(intent.generation) {
            self.reset();
            self.generation = Some(intent.generation);
        }
        // A released touch must not impose its remembered direction over a
        // keyboard/controller command. A held touch still mixes independently.
        let apply = intent.held
            || (self.engaged
                && independent.throttle == 0.
                && independent.brake == 0.
                && !independent.reverse);
        if !apply {
            independent.lean = independent.lean.clamp(-1., 1.);
            return Ok(independent);
        }
        let axis = if intent.held {
            intent.axis.clamp(-1., 1.)
        } else {
            0.
        };
        let brake = if !grounded && intent.held {
            // Preserve takeoff throttle/direction, never its gesture brake.
            // Right-hand lean remains in `independent` and retains the agreed
            // additive/clamped policy; this patch does not invent new torque.
            independent.lean = (independent.lean - axis).clamp(-1., 1.);
            0.
        } else {
            let strength = axis.abs();
            if strength <= 0.08 {
                self.throttle = 0.;
                self.brake = 0.;
            } else {
                self.engaged = true;
                let wanted_reverse = axis < 0.;
                if (wanted_reverse && speed > 1.) || (!wanted_reverse && speed < -1.) {
                    self.reverse = speed < 0.;
                    self.throttle = 0.;
                    self.brake = strength;
                } else {
                    self.reverse = wanted_reverse;
                    self.throttle = strength;
                    self.brake = 0.;
                }
            }
            self.brake
        };
        independent.throttle = independent.throttle.max(self.throttle);
        independent.brake = independent.brake.max(brake);
        independent.reverse |= self.reverse;
        independent.lean = independent.lean.clamp(-1., 1.);
        Ok(independent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn intent(axis: f32) -> Option<TouchIntent> {
        Some(TouchIntent {
            generation: 1,
            held: true,
            axis,
        })
    }
    fn channels(input: VehicleInput) -> (f32, f32, f32, bool) {
        (input.throttle, input.brake, input.lean, input.reverse)
    }
    #[test]
    fn held_pullback_switches_at_each_takeoff_and_landing_step() {
        let mut touch = TouchDrive::default();
        let states = [(true, 30.), (false, 29.), (false, 28.), (true, 27.)];
        let actual: Vec<_> = states
            .into_iter()
            .map(|(grounded, speed)| {
                channels(
                    touch
                        .resolve(VehicleInput::default(), intent(-1.), grounded, speed)
                        .unwrap(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            vec![
                (0., 1., 0., false),
                (0., 0., 1., false),
                (0., 0., 1., false),
                (0., 1., 0., false)
            ]
        );
    }
    #[test]
    fn batches_and_individual_steps_have_identical_transition_outputs() {
        let states = [
            (true, 35.),
            (false, 34.),
            (false, 33.),
            (true, 25.),
            (true, 1.01),
            (true, 0.99),
            (false, -2.),
            (true, -3.),
        ];
        let mut sequential = TouchDrive::default();
        let expected: Vec<_> = states
            .into_iter()
            .map(|(g, s)| {
                channels(
                    sequential
                        .resolve(VehicleInput::default(), intent(-1.), g, s)
                        .unwrap(),
                )
            })
            .collect();
        let mut batched = TouchDrive::default();
        let mut actual = Vec::new();
        for batch in states.chunks(4) {
            for &(g, s) in batch {
                actual.push(channels(
                    batched
                        .resolve(VehicleInput::default(), intent(-1.), g, s)
                        .unwrap(),
                ));
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(actual[4], (0., 1., 0., false));
        assert_eq!(actual[5], (1., 0., 0., true));
        assert_eq!(actual[6], (1., 0., 1., true));
    }
    #[test]
    fn cancellation_generation_and_recovery_discard_takeoff_memory() {
        let mut touch = TouchDrive::default();
        touch
            .resolve(VehicleInput::default(), intent(-0.8), true, -5.)
            .unwrap();
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), intent(1.), false, -5.)
                    .unwrap()
            ),
            (0.8, 0., -1., true)
        );
        let clear = Some(TouchIntent {
            generation: 2,
            held: false,
            axis: 0.,
        });
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), clear, false, -5.)
                    .unwrap()
            ),
            (0., 0., 0., false)
        );
        touch
            .resolve(VehicleInput::default(), intent(1.), true, 5.)
            .unwrap();
        touch.reset();
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), intent(1.), false, 0.)
                    .unwrap()
            ),
            (0., 0., -1., false)
        );
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), None, true, 0.)
                    .unwrap()
            ),
            (0., 0., 0., false)
        );
    }
    #[test]
    fn releasing_and_independent_controls_do_not_leave_touch_brakes_or_gear() {
        let mut touch = TouchDrive::default();
        touch
            .resolve(VehicleInput::default(), intent(-1.), true, -5.)
            .unwrap();
        let released = Some(TouchIntent {
            generation: 1,
            held: false,
            axis: -1.,
        });
        let independent = VehicleInput {
            throttle: 0.6,
            brake: 0.4,
            lean: -0.25,
            ..Default::default()
        };
        assert_eq!(
            channels(touch.resolve(independent, released, false, -5.).unwrap()),
            (0.6, 0.4, -0.25, false)
        );
        // A held gesture cannot cancel a separate controller brake or lean.
        let mixed = touch.resolve(independent, intent(-1.), false, -5.).unwrap();
        assert_eq!(channels(mixed), (1., 0.4, 0.75, true));
        let combined = touch
            .resolve(
                VehicleInput {
                    lean: 1.5,
                    ..independent
                },
                intent(1.),
                false,
                -5.,
            )
            .unwrap();
        assert_eq!(
            combined.lean, 0.5,
            "Clamp only after all independent and touch lean contributions"
        );
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), released, false, -5.)
                    .unwrap()
            ),
            (0., 0., 0., true)
        );
    }
}
