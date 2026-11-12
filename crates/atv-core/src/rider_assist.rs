//! Options → Accessibility → Steering and Stability assists (authored; the
//! original has neither). Both are off by default and work inside the fixed
//! simulation step with plain arithmetic, so a run is the same on every
//! platform:
//!
//! - Steering eases the stick at speed (up to 20% Low, 35% High less steer
//!   at full speed) and, while grounded and fast, bleeds off a yaw rate past
//!   a limit so a slide straightens instead of swapping ends.
//! - Stability rolls the quad gently toward level in the air (pitch stays the
//!   rider's own) and damps roll for a moment after touchdown.
//!
//! Neither acts during flips, stunts or wrecks. The ride gates them like Kid
//! Mode: online rooms run them off and assisted runs set no records.
use crate::{
    dynamics::{Basis, inverse_transform},
    wheel::{Vec3, cross},
};

/// Settings values, in menu order. 0 is off.
pub const LEVELS: [&str; 3] = ["off", "low", "high"];

/// Forward speed (ft/s) where steering easing starts and where it is full.
const EASE_SPEED: (f32, f32) = (30., 90.);
/// Steps of roll damping after touchdown (0.4 s).
const LANDING_STEPS: u8 = 24;

/// What the step left behind for the next one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    airborne: bool,
    landing: u8,
}

/// The parts of a stepped vehicle the assists read.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub basis: Basis,
    /// Body-local rates: pitch (x), yaw (y), roll (z).
    pub local_angular_velocity: Vec3,
    pub speed: f32,
    pub grounded: bool,
    /// Riding (no wreck), no flip and no stunt held.
    pub free: bool,
}

/// The stick after steering assist at `speed` (ft/s).
pub fn ease_steer(steer: f32, speed: f32, level: u8) -> f32 {
    let most = match level {
        1 => 0.2,
        2 => 0.35,
        _ => return steer,
    };
    let (from, to) = EASE_SPEED;
    let t = ((speed.abs() - from) / (to - from)).clamp(0., 1.);
    steer * (1. - most * t)
}

impl State {
    /// One fixed step of both assists: the change to the body-local spin.
    pub fn step(&mut self, body: Body, steering: u8, stability: u8, dt: f32) -> Vec3 {
        let mut delta = [0.; 3];
        let [_, yaw, roll] = body.local_angular_velocity;
        let landed = self.airborne && body.grounded;
        self.airborne = !body.grounded;
        if landed {
            self.landing = LANDING_STEPS;
        }
        if !body.free {
            self.landing = 0;
            return delta;
        }
        // Steering: a grounded yaw rate past the limit bleeds off.
        if let Some((limit, gain)) = match steering {
            1 => Some((2.2, 3.)),
            2 => Some((1.6, 6.)),
            _ => None,
        } && body.grounded
            && body.speed.abs() > EASE_SPEED.0
            && yaw.abs() > limit
        {
            delta[1] = -dt * gain * (yaw - limit.copysign(yaw));
        }
        let Some((spring, damping, settle)) = (match stability {
            1 => Some((4., 2., 4.)),
            2 => Some((9., 4., 8.)),
            _ => None,
        }) else {
            return delta;
        };
        if !body.grounded {
            // Roll toward level: the turn that brings the body's up to the
            // world's, kept to its roll part.
            let tilt = inverse_transform(body.basis, cross(body.basis[1], [0., 1., 0.]));
            delta[2] = dt * (spring * tilt[2] - damping * roll);
        } else if self.landing > 0 {
            self.landing -= 1;
            delta[2] = -dt * settle * roll;
        }
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(roll: f32, yaw: f32, grounded: bool) -> Body {
        Body {
            basis: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            local_angular_velocity: [0., yaw, roll],
            speed: 80.,
            grounded,
            free: true,
        }
    }

    #[test]
    fn off_changes_nothing() {
        assert_eq!(ease_steer(0.7, 120., 0), 0.7);
        let mut state = State::default();
        for grounded in [false, true, false, true] {
            assert_eq!(state.step(body(3., 5., grounded), 0, 0, 1. / 60.), [0.; 3]);
        }
    }

    #[test]
    fn steering_eases_with_speed() {
        assert_eq!(ease_steer(1., 20., 2), 1.);
        assert!((ease_steer(1., 60., 1) - 0.9).abs() < 1e-6);
        assert!((ease_steer(-1., 200., 2) + 0.65).abs() < 1e-6);
        assert!(ease_steer(1., 90., 2) < ease_steer(1., 90., 1));
    }

    #[test]
    fn steering_bleeds_only_an_excess_grounded_yaw() {
        let mut state = State::default();
        let dt = 1. / 60.;
        assert_eq!(state.step(body(0., 1., true), 2, 0, dt), [0.; 3]);
        let d = state.step(body(0., 4., true), 2, 0, dt);
        assert!(d[1] < 0. && d[1] > -0.5, "{d:?}");
        let d = state.step(body(0., -4., true), 1, 0, dt);
        assert!(d[1] > 0., "{d:?}");
        assert_eq!(state.step(body(0., 4., false), 2, 0, dt)[1], 0.);
        let slow = Body {
            speed: 10.,
            ..body(0., 4., true)
        };
        assert_eq!(state.step(slow, 2, 0, dt), [0.; 3]);
    }

    #[test]
    fn stability_damps_air_roll_and_settles_landings() {
        let dt = 1. / 60.;
        let mut state = State::default();
        let d = state.step(body(2., 0., false), 0, 1, dt);
        assert!(d[2] < 0. && d[0] == 0. && d[1] == 0.);
        let mut steps = 0;
        while state.step(body(1., 0., true), 0, 2, dt)[2] < 0. {
            steps += 1;
        }
        assert_eq!(steps, LANDING_STEPS);
        // Wrecks, flips and stunts stay the rider's own.
        let mut state = State::default();
        let held = Body {
            free: false,
            ..body(2., 4., false)
        };
        assert_eq!(state.step(held, 2, 2, dt), [0.; 3]);
    }
}
