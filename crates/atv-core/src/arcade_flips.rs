//! Optional "Arcade flips" (Settings → Alpha). Authored for this port; neither
//! ATV Offroad Fury 2 nor MX vs. ATV Unleashed has it.
//!
//! While airborne the riding stick sets the rotation rate directly: lean back
//! spins nose-up (backflip), lean forward nose-down (front flip), steer spins
//! about the body's up axis (360s). Both together give off-axis corks. Holding
//! keeps the rate, releasing bleeds it out quickly and pushing the other way
//! brakes. Releasing the preload jump right at the lip adds a little pop and
//! more rotation authority for that jump.
//!
//! Only the airborne angle wreck limits are skipped while the rotation is
//! engaged, as for the Unleashed tricks. Touchdown ends it before the original
//! ground, impact and stunt-landing gates run, so bad landings still wreck;
//! there is no landing assistance.
use crate::flip_tricks::{Event, POINTS, Trick};
use crate::math::Det;
use serde::Serialize;
use std::f32::consts::{FRAC_PI_3, TAU};

/// Full-stick pitch rate (rad/s). A backflip takes about 1.4 s with spin-up.
pub const PITCH_RATE: f32 = 5.5;
/// Full-stick yaw rate (rad/s).
pub const YAW_RATE: f32 = 6.;
/// How quickly the commanded rate grows toward a held stick (rad/s²).
pub const SPIN_UP: f32 = 14.;
/// Commanded-rate change when pushing against the rotation (rad/s²).
pub const BRAKE: f32 = 30.;
/// Exponential bleed time constant once the stick is released (s).
pub const RELEASE_TAU: f32 = 0.25;
/// Stick dead zone; outside it the response is |u|^1.5 for fine analog control.
pub const DEAD_ZONE: f32 = 0.15;
/// Stick magnitude that hands air rotation to the arcade controller for the
/// rest of the jump. Below it the original ATV2 air control is untouched.
pub const ENGAGE: f32 = 0.25;
/// Feedback torque per rad/s of rate error, as a multiple of |AeroDamping|.
/// The feed-forward term cancels damping at the commanded rate.
pub const TRACK_GAIN: f32 = 1.;
/// Lip pop: preload age at takeoff (s) that still counts as perfect, and the
/// age at which the bonus reaches zero (the recovered 0.25 s preload impulse).
pub const POP_PERFECT: f32 = 0.05;
pub const POP_WINDOW: f32 = 0.25;
/// Extra takeoff vertical speed at a perfect pop (ft/s).
pub const POP_LIFT: f32 = 4.;
/// Extra rotation rate and spin-up at a perfect pop (fraction).
pub const POP_AUTHORITY: f32 = 0.25;
/// Time a landing must stay unwrecked before the trick name is shown (s).
pub const LAND_CONFIRM: f32 = 0.25;
/// A rotation counts when at most this far short of a full turn.
const TOLERANCE: f32 = FRAC_PI_3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Frame {
    /// The arcade controller drives air rotation this jump.
    pub engaged: bool,
    /// Accumulated rotation since takeoff (rad): nose-up and rightward positive.
    pub pitch: f32,
    pub yaw: f32,
    /// Rotation about the combined pitch/yaw axis since takeoff (rad).
    pub total: f32,
    /// Commanded pitch (nose-up) and yaw (right) rates (rad/s).
    pub rate: [f32; 2],
    /// Lip pop quality of the current or last jump, 0..1.
    pub pop: f32,
    /// Preload age at the last takeoff (s), when a preload was active.
    pub pop_age: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct Input {
    pub grounded: bool,
    /// Riding stick: x right, y leaned back.
    pub stick: [f32; 2],
    /// Any original stunt modifier held: the stick belongs to that stunt.
    pub modifiers: bool,
    /// Time since the original preload impulse fired, while it is active.
    pub preload_age: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
struct Landing {
    trick: Trick,
    name: String,
    rotations: u32,
    timer: f32,
}

#[derive(Debug, Clone)]
pub struct ArcadeFlips {
    damping: f32,
    airborne: bool,
    engaged: bool,
    rate: [f32; 2],
    angle: [f32; 2],
    total: f32,
    pop: f32,
    pop_age: Option<f32>,
    lift: f32,
    landing: Option<Landing>,
    events: Vec<Event>,
}

/// Dead zone, then a 1.5 power curve: keyboard full presses stay at 1.
fn shape(u: f32) -> f32 {
    let a = u.abs();
    if a <= DEAD_ZONE {
        return 0.;
    }
    u.signum() * ((a - DEAD_ZONE) / (1. - DEAD_ZONE)).min(1.).powf_d(1.5)
}

/// Lip pop quality from the preload age at takeoff.
pub fn pop_quality(age: f32) -> f32 {
    (1. - (age - POP_PERFECT) / (POP_WINDOW - POP_PERFECT)).clamp(0., 1.)
}

/// Flip, flat spin or cork, from the share of nose-over versus spin rotation.
pub fn classify(pitch: f32, yaw: f32) -> Trick {
    let (p, y) = (pitch.abs(), yaw.abs());
    if p + y == 0. || p >= 3. * y {
        if pitch < 0. {
            Trick::FrontFlip
        } else {
            Trick::Backflip
        }
    } else if y >= 3. * p {
        Trick::Spin
    } else {
        Trick::Cork
    }
}

/// HUD name for a landed rotation, with the count of completed turns.
/// `total` is the rotation about the (possibly tilted) pitch/yaw axis, so a
/// cork counts its off-axis revolutions rather than each component.
pub fn trick_name(pitch: f32, yaw: f32, total: f32) -> Option<(Trick, String, u32)> {
    let n = ((total.abs() + TOLERANCE) / TAU).floor() as u32;
    if n == 0 {
        return None;
    }
    let times = match n {
        1 => String::new(),
        2 => "Double ".into(),
        3 => "Triple ".into(),
        n => format!("{n}x "),
    };
    let trick = classify(pitch, yaw);
    let name = match trick {
        Trick::FrontFlip => format!("{times}Front Flip"),
        Trick::Spin => format!("{}", n * 360),
        Trick::Cork if pitch < 0. => format!("Front Cork {}", n * 360),
        Trick::Cork => format!("Cork {}", n * 360),
        _ => format!("{times}Backflip"),
    };
    Some((trick, name, n))
}

impl ArcadeFlips {
    /// `air_damping` is the ATV's own AeroDamping (validated by FlipTricks).
    pub fn new(air_damping: f32) -> Self {
        Self {
            damping: air_damping.abs(),
            airborne: false,
            engaged: false,
            rate: [0.; 2],
            angle: [0.; 2],
            total: 0.,
            pop: 0.,
            pop_age: None,
            lift: 0.,
            landing: None,
            events: Vec::new(),
        }
    }

    pub fn exempts_air_angles(&self) -> bool {
        self.engaged && self.airborne
    }

    /// Live classification while spinning; leads by the commanded rate so the
    /// first frames already show the direction being pushed.
    pub fn active(&self) -> Option<Trick> {
        if !self.exempts_air_angles() {
            return None;
        }
        Some(classify(
            self.angle[0] + self.rate[0] * 0.1,
            self.angle[1] + self.rate[1] * 0.1,
        ))
    }

    pub fn rotation(&self) -> f32 {
        self.total
    }

    pub fn frame(&self) -> Frame {
        Frame {
            engaged: self.exempts_air_angles(),
            pitch: self.angle[0],
            yaw: self.angle[1],
            total: self.total,
            rate: self.rate,
            pop: self.pop,
            pop_age: self.pop_age,
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Extra vertical speed (ft/s) owed by a lip pop, once.
    pub fn take_lift(&mut self) -> f32 {
        std::mem::take(&mut self.lift)
    }

    /// One physics substep after the original air control. Returns the
    /// body-local torque to add (x nose-down, y right).
    pub fn step(&mut self, input: Input, w: [f32; 3], h: f32) -> [f32; 3] {
        if let Some(landing) = &mut self.landing {
            landing.timer -= h;
        }
        if input.grounded {
            if self.airborne && self.engaged {
                self.landing = trick_name(self.angle[0], self.angle[1], self.total).map(
                    |(trick, name, rotations)| Landing {
                        trick,
                        name,
                        rotations,
                        timer: LAND_CONFIRM,
                    },
                );
            }
            self.airborne = false;
            self.engaged = false;
            self.rate = [0.; 2];
            return [0.; 3];
        }
        if !self.airborne {
            // Takeoff: a preload still pushing scores the lip timing.
            self.airborne = true;
            self.engaged = false;
            self.angle = [0.; 2];
            self.total = 0.;
            self.rate = [0.; 2];
            self.pop_age = input.preload_age;
            self.pop = input.preload_age.map_or(0., pop_quality);
            self.lift = POP_LIFT * self.pop;
        }
        self.angle[0] += -w[0] * h;
        self.angle[1] += w[1] * h;
        self.total += w[0].hypot_d(w[1]) * h;
        let [x, y] = if input.modifiers {
            [0.; 2]
        } else {
            input.stick
        };
        if x.abs() > ENGAGE || y.abs() > ENGAGE {
            self.engaged = true;
        }
        let authority = 1. + POP_AUTHORITY * self.pop;
        for (rate, (u, max)) in self.rate.iter_mut().zip([(y, PITCH_RATE), (x, YAW_RATE)]) {
            let target = shape(u) * max * authority;
            if target == 0. {
                *rate *= (-h / RELEASE_TAU).exp_d();
            } else {
                let toward = |r: f32, step: f32| r + (target - r).clamp(-step, step);
                let opposing = *rate != 0. && rate.signum() != target.signum();
                *rate = if opposing || rate.abs() > target.abs() {
                    toward(*rate, BRAKE * h)
                } else {
                    toward(*rate, SPIN_UP * authority * h)
                };
            }
        }
        if !self.engaged {
            return [0.; 3];
        }
        // Nose-up is negative local x. Feed-forward cancels the original
        // damping at the commanded rate; feedback tracks it.
        let desired = [-self.rate[0], self.rate[1]];
        let mut torque = [0.; 3];
        for i in 0..2 {
            torque[i] = self.damping * (desired[i] + TRACK_GAIN * (desired[i] - w[i]));
        }
        torque
    }

    /// After the wreck checks for the frame: credit a confirmed landing, or
    /// record a bail if the rider wrecked while spinning or just after landing.
    pub fn settle(&mut self, wrecked: bool) {
        if wrecked {
            let trick = self
                .landing
                .take()
                .map(|l| l.trick)
                .or_else(|| self.active());
            if let Some(trick) = trick {
                self.events.push(Event::Bailed { trick });
            }
            self.engaged = false;
            self.rate = [0.; 2];
            self.lift = 0.;
            return;
        }
        if self.landing.as_ref().is_some_and(|l| l.timer <= 0.) {
            let Landing {
                trick,
                name,
                rotations,
                ..
            } = self.landing.take().expect("checked");
            self.events.push(Event::Landed {
                trick,
                name,
                rotations,
                points: POINTS * rotations as f32,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f32 = 1. / 60.;

    fn air(stick: [f32; 2]) -> Input {
        Input {
            grounded: false,
            stick,
            modifiers: false,
            preload_age: None,
        }
    }

    /// Integrate a damping-dominated body like ATV01 (AeroDamping -800,
    /// pitch/yaw inertia near 44) with the original air damping included.
    fn fly(a: &mut ArcadeFlips, inputs: impl IntoIterator<Item = Input>, w: &mut [f32; 3]) {
        for input in inputs {
            let t = a.step(input, *w, H);
            for i in 0..3 {
                w[i] += H * (t[i] - 800. * w[i]) / 44.;
            }
        }
    }

    #[test]
    fn held_stick_sets_the_rate_and_release_bleeds_it_out() {
        let mut a = ArcadeFlips::new(-800.);
        a.step(
            Input {
                grounded: true,
                ..air([0.; 2])
            },
            [0.; 3],
            H,
        );
        let mut w = [0.; 3];
        fly(&mut a, std::iter::repeat_n(air([0., 1.]), 60), &mut w);
        assert!(a.exempts_air_angles());
        assert_eq!(a.active(), Some(Trick::Backflip));
        // Spin-up reaches the full rate within half a second and holds it.
        assert!((a.rate[0] - PITCH_RATE).abs() < 1e-3, "{:?}", a.rate);
        assert!((-w[0] - PITCH_RATE).abs() < 0.1, "body tracks: {w:?}");
        let before = a.angle[0];
        fly(&mut a, std::iter::repeat_n(air([0.; 2]), 30), &mut w);
        assert!(
            a.rate[0] < 0.8 && -w[0] < 1.,
            "released: {:?} {w:?}",
            a.rate
        );
        // Bleed-out adds about a fifth of a turn.
        assert!(a.angle[0] - before < 1.5, "{}", a.angle[0] - before);
        // Still engaged with a neutral stick: the airborne limits stay off
        // until touchdown so the player can finish the rotation.
        assert!(a.exempts_air_angles());
    }

    #[test]
    fn pushing_against_the_rotation_brakes_faster_than_release() {
        let spin = |back: [f32; 2]| {
            let mut a = ArcadeFlips::new(-800.);
            let mut w = [0.; 3];
            fly(&mut a, std::iter::repeat_n(air([0., 1.]), 60), &mut w);
            fly(&mut a, std::iter::repeat_n(air(back), 6), &mut w);
            a.rate[0]
        };
        let released = spin([0.; 2]);
        let braked = spin([0., -1.]);
        assert!(braked < released, "{braked} {released}");
        assert!(braked < PITCH_RATE - BRAKE * 6. * H + 0.01);
    }

    #[test]
    fn forward_stick_front_flips_and_steer_spins() {
        let mut a = ArcadeFlips::new(-800.);
        let mut w = [0.; 3];
        let t = a.step(air([0., -1.]), w, H);
        assert!(t[0] > 0., "nose-down torque is positive local x: {t:?}");
        fly(&mut a, std::iter::repeat_n(air([0., -1.]), 90), &mut w);
        assert_eq!(a.active(), Some(Trick::FrontFlip));
        assert!(a.angle[0] < -TAU * 0.75);

        let mut a = ArcadeFlips::new(-800.);
        let mut w = [0.; 3];
        let t = a.step(air([1., 0.]), w, H);
        assert!(t[1] > 0. && t[0] == 0., "{t:?}");
        fly(&mut a, std::iter::repeat_n(air([1., 0.]), 90), &mut w);
        assert_eq!(a.active(), Some(Trick::Spin));
        assert!(a.angle[1] > TAU * 0.75);
    }

    #[test]
    fn small_inputs_leave_the_original_air_control_alone() {
        let mut a = ArcadeFlips::new(-800.);
        for stick in [[0.2, 0.], [0., -0.25], [0.1, 0.2]] {
            assert_eq!(a.step(air(stick), [0.3, 0.1, 0.], H), [0.; 3]);
        }
        assert!(!a.exempts_air_angles());
        // A held stunt modifier gives the stick to the original stunt.
        let t = a.step(
            Input {
                modifiers: true,
                ..air([0., 1.])
            },
            [0.; 3],
            H,
        );
        assert_eq!(t, [0.; 3]);
        assert!(!a.exempts_air_angles());
    }

    #[test]
    fn analog_stick_scales_the_rate() {
        let rate = |u: f32| {
            let mut a = ArcadeFlips::new(-800.);
            let mut w = [0.; 3];
            fly(&mut a, std::iter::repeat_n(air([0., u]), 90), &mut w);
            a.rate[0]
        };
        let (half, full) = (rate(0.6), rate(1.));
        assert!(half > 0.25 * full && half < 0.6 * full, "{half} {full}");
    }

    #[test]
    fn combos_name_corks_spins_and_multiples() {
        // Components in turns; the total is their combined-axis rotation.
        let name = |p: f32, y: f32| {
            trick_name(p * TAU, y * TAU, p.hypot_d(y) * TAU).map(|(_, n, r)| (n, r))
        };
        assert_eq!(name(0.9, 0.), Some(("Backflip".into(), 1)));
        assert_eq!(name(-1., 0.), Some(("Front Flip".into(), 1)));
        assert_eq!(name(2.1, 0.), Some(("Double Backflip".into(), 2)));
        assert_eq!(name(0., 2.), Some(("720".into(), 2)));
        assert_eq!(name(0.1, -1.), Some(("360".into(), 1)));
        assert_eq!(name(1.2, 1.6), Some(("Cork 720".into(), 2)));
        assert_eq!(name(-0.7, 0.7), Some(("Front Cork 360".into(), 1)));
        assert_eq!(name(0.6, 0.4), None, "short of a full turn");

        // Pitch and yaw together turn about one tilted axis: a full
        // revolution there is one cork, not a backflip plus a 360.
        let mut a = ArcadeFlips::new(-800.);
        let mut w = [0.; 3];
        fly(&mut a, std::iter::repeat_n(air([1., 1.]), 90), &mut w);
        assert_eq!(a.active(), Some(Trick::Cork));
        assert!(
            a.angle[0] > 0.5 * TAU && a.angle[1] > 0.5 * TAU,
            "{:?}",
            a.angle
        );
        let total = a.angle[0].hypot_d(a.angle[1]);
        assert!(
            (a.total - total).abs() < 0.05 * total,
            "{} {total}",
            a.total
        );
    }

    #[test]
    fn landing_is_credited_only_after_an_unwrecked_touchdown() {
        let mut a = ArcadeFlips::new(-800.);
        let mut w = [0.; 3];
        fly(&mut a, std::iter::repeat_n(air([0., 1.]), 100), &mut w);
        fly(&mut a, std::iter::repeat_n(air([0.; 2]), 20), &mut w);
        let ground = Input {
            grounded: true,
            ..air([0.; 2])
        };
        a.step(ground, [0.; 3], H);
        assert!(!a.exempts_air_angles(), "touchdown ends the exemption");
        a.settle(false);
        assert!(a.take_events().is_empty(), "waits for the landing to hold");
        for _ in 0..16 {
            a.step(ground, [0.; 3], H);
            a.settle(false);
        }
        assert!(matches!(
            &a.take_events()[..],
            [Event::Landed { trick: Trick::Backflip, name, rotations: 1, .. }] if name == "Backflip"
        ));

        // A wreck right after touchdown is a bail, not a landed trick.
        let mut a = ArcadeFlips::new(-800.);
        let mut w = [0.; 3];
        fly(&mut a, std::iter::repeat_n(air([0., 1.]), 100), &mut w);
        a.step(ground, [0.; 3], H);
        a.settle(true);
        assert_eq!(
            a.take_events(),
            [Event::Bailed {
                trick: Trick::Backflip
            }]
        );
        for _ in 0..30 {
            a.settle(true);
        }
        assert!(a.take_events().is_empty());
    }

    #[test]
    fn lip_pop_rewards_takeoff_while_the_preload_still_pushes() {
        assert_eq!(pop_quality(0.02), 1.);
        assert!((pop_quality(0.15) - 0.5).abs() < 1e-6);
        assert_eq!(pop_quality(0.25), 0.);
        let launch = |age: Option<f32>| {
            let mut a = ArcadeFlips::new(-800.);
            a.step(
                Input {
                    grounded: true,
                    ..air([0.; 2])
                },
                [0.; 3],
                H,
            );
            let mut w = [0.; 3];
            a.step(
                Input {
                    preload_age: age,
                    ..air([0., 1.])
                },
                w,
                H,
            );
            let lift = a.take_lift();
            assert_eq!(a.take_lift(), 0., "paid once");
            fly(&mut a, std::iter::repeat_n(air([0., 1.]), 60), &mut w);
            (lift, a.rate[0], a.frame().pop)
        };
        let (perfect, early, none) = (launch(Some(H)), launch(Some(0.2)), launch(None));
        assert_eq!(perfect.0, POP_LIFT);
        assert!(early.0 > 0. && early.0 < 0.5 * POP_LIFT);
        assert_eq!(none.0, 0.);
        assert!((perfect.1 - PITCH_RATE * (1. + POP_AUTHORITY)).abs() < 1e-3);
        assert!(perfect.1 > early.1 && early.1 > none.1);
        assert_eq!((perfect.2, none.2), (1., 0.));
    }
}
