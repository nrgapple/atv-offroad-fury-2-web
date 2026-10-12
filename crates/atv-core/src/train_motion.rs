//! Native procedural-actor velocity getters (002daea0 / 002dafe0).
use crate::{
    Result,
    contact::{
        ContactConfig, ContactCorrections, StaticContact,
        resolve_static_with_velocity_and_position_scale,
    },
    dynamics::{BodyConfig, BodyState, inverse_transform, transform},
    wheel::{Vec3, add, cross, scale, sub},
};
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionState {
    pub current: [f32; 16],
    pub previous: [f32; 16],
    pub reciprocal_dt: f32,
}
impl MotionState {
    pub fn linear_velocity(self) -> Vec3 {
        scale(
            sub(
                self.current[12..15].try_into().unwrap(),
                self.previous[12..15].try_into().unwrap(),
            ),
            self.reciprocal_dt,
        )
    }
    pub fn angular_velocity(self) -> Vec3 {
        let column = |matrix: [f32; 16], at: usize| [matrix[at], matrix[at + 1], matrix[at + 2]];
        scale(
            add(
                cross(column(self.previous, 8), column(self.current, 8)),
                cross(column(self.previous, 4), column(self.current, 4)),
            ),
            self.reciprocal_dt,
        )
    }
    ///001953d0 queries both world velocity getters and uses the target's model
    ///origin when its authored local center is zero (the shipped train bodies).
    pub fn point_velocity(self, point: Vec3) -> Vec3 {
        let origin = self.current[12..15].try_into().unwrap();
        add(
            self.linear_velocity(),
            cross(self.angular_velocity(), sub(point, origin)),
        )
    }
    ///Verified native zero-mass procedural target response. The caller supplies
    ///the actor's moving-target reflection coefficient (+d8), not its static
    ///obstacle reflection (+d0). Train route poses remain unaffected by impacts.
    pub fn resolve_contact(
        self,
        body: &mut BodyState,
        body_config: &BodyConfig,
        config: ContactConfig,
        contact: StaticContact,
        corrections: &mut ContactCorrections,
    ) -> Result<()> {
        self.resolve_contact_with_position_scale(
            body,
            body_config,
            config,
            contact,
            corrections,
            0.3,
        )
    }
    /// The original secondary chassis pass uses1.0; detached rider callers use
    /// their own coefficient. Impulse and friction arithmetic is unchanged.
    pub fn resolve_contact_with_position_scale(
        self,
        body: &mut BodyState,
        body_config: &BodyConfig,
        config: ContactConfig,
        contact: StaticContact,
        corrections: &mut ContactCorrections,
        position_scale: f32,
    ) -> Result<()> {
        let arm = inverse_transform(body.basis(), sub(contact.point, body.center));
        let own = add(
            body.linear_velocity,
            transform(body.basis(), cross(body.local_angular_velocity, arm)),
        );
        let relative = sub(own, self.point_velocity(contact.point));
        resolve_static_with_velocity_and_position_scale(
            body,
            body_config,
            config,
            contact,
            relative,
            corrections,
            position_scale,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    #[test]
    fn moving_target_pushes_the_body_without_changing_its_route_pose() {
        let config = BodyConfig::new(10., [20.; 3], [0.; 3]).unwrap();
        let mut body = BodyState::new([0.; 3], [1., 0., 0., 0.], &config).unwrap();
        let mut previous = IDENTITY;
        previous[12] = -0.1;
        let state = MotionState {
            current: IDENTITY,
            previous,
            reciprocal_dt: 10.,
        };
        let mut corrections = ContactCorrections::default();
        state
            .resolve_contact(
                &mut body,
                &config,
                ContactConfig {
                    reflection: 0.,
                    angular_impulse_scale: 0.,
                    friction_scale: 0.,
                    friction_torque_scale: 0.,
                },
                StaticContact {
                    point: [0.; 3],
                    normal: [1., 0., 0.],
                    penetration: 0.1,
                    material_grip: 1.,
                },
                &mut corrections,
            )
            .unwrap();
        assert_eq!(body.linear_velocity, [1., 0., 0.]);
        assert_eq!(state.current, IDENTITY);
        assert_eq!(corrections.translation, [0.030000001, 0., 0.]);
    }
    #[test]
    fn native_velocity_uses_prior_translation_and_both_orientation_axes() {
        let mut current = IDENTITY;
        current[12] = 1000.5;
        current[14] = -30.;
        let mut previous = IDENTITY;
        previous[12] = 1000.;
        previous[14] = -31.;
        let state = MotionState {
            current,
            previous,
            reciprocal_dt: 60.,
        };
        assert_eq!(state.linear_velocity(), [30., 0., 60.]);
        assert_eq!(state.angular_velocity(), [0.; 3]);
        assert_eq!(state.point_velocity([1005., 8., -32.]), [30., 0., 60.]);
        // A quarter-turn about X changes both up and forward. The native getter
        //sums two cross products; it is not an axis-angle/dt reconstruction.
        current[4..7].copy_from_slice(&[0., 0., 1.]);
        current[8..11].copy_from_slice(&[0., -1., 0.]);
        let state = MotionState {
            current,
            previous,
            reciprocal_dt: 2.,
        };
        assert_eq!(state.angular_velocity(), [4., 0., 0.]);
        assert_eq!(state.point_velocity([1000.5, 2., -30.]), [1., 0., 10.]);
    }
    #[test]
    fn secondary_pass_changes_only_the_penetration_coefficient() {
        let config = BodyConfig::new(10., [20.; 3], [0.; 3]).unwrap();
        let initial = BodyState::new([0.; 3], [1., 0., 0., 0.], &config).unwrap();
        let mut prior = IDENTITY;
        prior[12] = -1.;
        let target = MotionState {
            current: IDENTITY,
            previous: prior,
            reciprocal_dt: 1.,
        };
        let resolve = |position_scale| {
            let mut body = initial.clone();
            body.force = [-10., 0., 5.];
            let mut corrections = ContactCorrections::default();
            target
                .resolve_contact_with_position_scale(
                    &mut body,
                    &config,
                    ContactConfig {
                        reflection: 0.,
                        angular_impulse_scale: 0.1,
                        friction_scale: 0.1,
                        friction_torque_scale: 0.1,
                    },
                    StaticContact {
                        point: [0., 1., 0.],
                        normal: [1., 0., 0.],
                        penetration: 0.2,
                        material_grip: 1.,
                    },
                    &mut corrections,
                    position_scale,
                )
                .unwrap();
            (body, corrections)
        };
        let (ordinary, normal) = resolve(0.3);
        let (secondary, full) = resolve(1.);
        assert_eq!(ordinary.linear_velocity, secondary.linear_velocity);
        assert_eq!(
            ordinary.local_angular_velocity,
            secondary.local_angular_velocity
        );
        assert_eq!(ordinary.force, secondary.force);
        assert_eq!(ordinary.local_torque, secondary.local_torque);
        assert_eq!(normal.velocity_delta, full.velocity_delta);
        assert!(full.translation[0] > normal.translation[0]);
        assert_eq!(initial.center, secondary.center); // deferred until integration
        assert!(
            target
                .resolve_contact_with_position_scale(
                    &mut initial.clone(),
                    &config,
                    ContactConfig {
                        reflection: 0.,
                        angular_impulse_scale: 0.,
                        friction_scale: 0.,
                        friction_torque_scale: 0.
                    },
                    StaticContact {
                        point: [0.; 3],
                        normal: [1., 0., 0.],
                        penetration: 0.2,
                        material_grip: 1.
                    },
                    &mut ContactCorrections::default(),
                    f32::NAN,
                )
                .is_err()
        );
    }
}
