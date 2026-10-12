//! Authored browser touch adaptation. Resolve against the current fixed-step
//! vehicle state; these gestures are not original PS2 controller behavior.
use crate::{Result, vehicle::VehicleInput};

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct TouchIntent {
    pub generation: u64,
    pub held: bool,
    /// Right-thumb forward/up is positive throttle; down is brake/reverse.
    pub axis: f32,
    /// Queue one-handed drive while airborne; apply at the first grounded step.
    #[serde(default)]
    pub grounded_only: bool,
    /// One-handed touch supplies forward throttle only, even when rolling back.
    #[serde(default)]
    pub forward_only: bool,
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
        if intent.forward_only {
            self.engaged = false;
            self.reverse = false;
            self.throttle = 0.;
            self.brake = 0.;
            if intent.held {
                independent.throttle = independent.throttle.max(intent.axis.clamp(0., 1.));
            }
            independent.lean = independent.lean.clamp(-1., 1.);
            return Ok(independent);
        }
        if intent.grounded_only && !grounded {
            self.throttle = 0.;
            self.brake = 0.;
            independent.lean = independent.lean.clamp(-1., 1.);
            return Ok(independent);
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
        let strength = axis.abs();
        if strength <= 0.08 {
            self.throttle = 0.;
            self.brake = 0.;
        } else {
            self.engaged = true;
            let wanted_reverse = axis < 0.;
            if !grounded {
                // Authored touch adaptation: body-relative speed changes sign
                // during rotation, so airborne input cannot select another gear.
                self.throttle = if wanted_reverse == self.reverse {
                    strength
                } else {
                    0.
                };
                self.brake = if wanted_reverse == self.reverse {
                    0.
                } else {
                    strength
                };
            } else if (wanted_reverse && speed > 1.) || (!wanted_reverse && speed < -1.) {
                self.reverse = speed < 0.;
                self.throttle = 0.;
                self.brake = strength;
            } else {
                self.reverse = wanted_reverse;
                self.throttle = strength;
                self.brake = 0.;
            }
        }
        independent.throttle = independent.throttle.max(self.throttle);
        independent.brake = independent.brake.max(self.brake);
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
            grounded_only: false,
            forward_only: false,
        })
    }
    fn released() -> Option<TouchIntent> {
        Some(TouchIntent {
            generation: 1,
            held: false,
            axis: 0.,
            grounded_only: false,
            forward_only: false,
        })
    }
    fn channels(i: VehicleInput) -> (f32, f32, f32, bool) {
        (i.throttle, i.brake, i.lean, i.reverse)
    }
    #[test]
    fn forward_only_touch_never_brakes_or_reverses_even_rolling_back_or_airborne() {
        let mut touch = TouchDrive::default();
        let mut held = intent(1.).unwrap();
        held.forward_only = true;
        for grounded in [true, false] {
            for speed in [-30., -2., 0., 30.] {
                let result = touch
                    .resolve(VehicleInput::default(), Some(held), grounded, speed)
                    .unwrap();
                assert_eq!(channels(result), (1., 0., 0., false));
            }
        }
        held.held = false;
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), Some(held), true, -30.)
                    .unwrap()
            ),
            (0., 0., 0., false)
        );
        // Independent controller brake/reverse remains available.
        held.held = true;
        let controller = VehicleInput {
            brake: 0.5,
            reverse: true,
            ..Default::default()
        };
        let result = touch.resolve(controller, Some(held), true, -30.).unwrap();
        assert_eq!(channels(result), (1., 0.5, 0., true));
    }

    #[test]
    fn queued_one_handed_drive_resumes_on_first_grounded_fixed_step() {
        let mut touch = TouchDrive::default();
        let mut queued = intent(0.75).unwrap();
        queued.grounded_only = true;
        let lean = VehicleInput {
            lean: -0.5,
            steer: 0.25,
            ..Default::default()
        };
        for grounded in [true, false, false, true, true] {
            let result = touch.resolve(lean, Some(queued), grounded, 20.).unwrap();
            assert_eq!(result.throttle, if grounded { 0.75 } else { 0. });
            assert_eq!(result.brake, 0.);
            assert_eq!(result.lean, -0.5);
            assert_eq!(result.steer, 0.25);
        }
        // Lift during the flight cancels the queued acceleration for landing.
        queued.held = false;
        queued.axis = 0.;
        touch.resolve(lean, Some(queued), false, 20.).unwrap();
        assert_eq!(
            touch
                .resolve(lean, Some(queued), true, 20.)
                .unwrap()
                .throttle,
            0.
        );
    }

    #[test]
    fn right_drive_never_changes_left_lean_and_release_stops_throttle_in_air() {
        let mut touch = TouchDrive::default();
        let left = VehicleInput {
            lean: 1.,
            steer: 0.5,
            ..Default::default()
        };
        for grounded in [true, false, true] {
            let active = touch.resolve(left, intent(1.), grounded, 30.).unwrap();
            assert_eq!(channels(active), (1., 0., 1., false));
            assert_eq!(active.steer, 0.5);
        }
        assert_eq!(
            channels(touch.resolve(left, released(), false, 30.).unwrap()),
            (0., 0., 1., false)
        );
        assert_eq!(
            channels(touch.resolve(left, intent(-1.), false, 30.).unwrap()),
            (0., 1., 1., false)
        );
    }
    #[test]
    fn each_ground_step_brakes_before_changing_direction() {
        let mut touch = TouchDrive::default();
        for (axis, speed, expected) in [
            (-1., 30., (0., 1., 0., false)),
            (-1., 1.01, (0., 1., 0., false)),
            (-1., 0.99, (1., 0., 0., true)),
            (-1., -5., (1., 0., 0., true)),
            (1., -5., (0., 1., 0., true)),
            (1., -1.01, (0., 1., 0., true)),
            (1., -0.99, (1., 0., 0., false)),
        ] {
            assert_eq!(
                channels(
                    touch
                        .resolve(VehicleInput::default(), intent(axis), true, speed)
                        .unwrap()
                ),
                expected
            );
        }
    }
    #[test]
    fn airborne_body_rotation_cannot_switch_the_selected_gear() {
        let mut touch = TouchDrive::default();
        touch
            .resolve(VehicleInput::default(), intent(-1.), true, 0.)
            .unwrap();
        for speed in [-20., 0., 20.] {
            assert_eq!(
                channels(
                    touch
                        .resolve(VehicleInput::default(), intent(-1.), false, speed)
                        .unwrap()
                ),
                (1., 0., 0., true)
            );
            assert_eq!(
                channels(
                    touch
                        .resolve(VehicleInput::default(), intent(1.), false, speed)
                        .unwrap()
                ),
                (0., 1., 0., true)
            );
            assert_eq!(
                channels(
                    touch
                        .resolve(VehicleInput::default(), released(), false, speed)
                        .unwrap()
                ),
                (0., 0., 0., true)
            );
        }
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), intent(1.), true, 0.)
                    .unwrap()
            ),
            (1., 0., 0., false)
        );
    }
    #[test]
    fn cancellation_generation_and_recovery_clear_drive_memory() {
        let mut touch = TouchDrive::default();
        touch
            .resolve(VehicleInput::default(), intent(-1.), true, -5.)
            .unwrap();
        let cancel = Some(TouchIntent {
            generation: 2,
            held: false,
            axis: -1.,
            grounded_only: false,
            forward_only: false,
        });
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), cancel, false, -5.)
                    .unwrap()
            ),
            (0., 0., 0., false)
        );
        touch
            .resolve(VehicleInput::default(), intent(-1.), true, -5.)
            .unwrap();
        touch.reset();
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), released(), false, -5.)
                    .unwrap()
            ),
            (0., 0., 0., false)
        );
        assert_eq!(
            channels(
                touch
                    .resolve(VehicleInput::default(), None, false, 0.)
                    .unwrap()
            ),
            (0., 0., 0., false)
        );
    }
    #[test]
    fn independent_controller_input_survives_touch_release_and_keeps_its_lean() {
        let mut touch = TouchDrive::default();
        let independent = VehicleInput {
            throttle: 0.6,
            brake: 0.4,
            lean: -0.25,
            ..Default::default()
        };
        assert_eq!(
            channels(touch.resolve(independent, intent(1.), false, 30.).unwrap()),
            (1., 0.4, -0.25, false)
        );
        assert_eq!(
            channels(touch.resolve(independent, released(), false, 30.).unwrap()),
            (0.6, 0.4, -0.25, false)
        );
        assert_eq!(
            touch
                .resolve(
                    VehicleInput {
                        lean: 1.5,
                        ..independent
                    },
                    intent(-1.),
                    false,
                    30.
                )
                .unwrap()
                .lean,
            1.
        );
    }
}
