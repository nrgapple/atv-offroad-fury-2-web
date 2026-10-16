//! Optional Extras "Flips" for the ATV: the MX vs. ATV Unleashed backflip and
//! left/right three-sixty.
//!
//! ATV2 (USA v2.00) has no flip tricks, and its recovered airborne wreck angles
//! eject the rider long before a full rotation (research/WRECK.md). This is an
//! explicit authored enhancement. It follows the gesture, torque and
//! termination rules recovered from SLUS_211.04 GAME_REL backflip 004e4f20 and
//! three-sixty 004e53d0 (see motorcycle_drive.rs), plus the early-air request
//! the motorcycle adapter also accepts. When the option is off the vehicle never
//! constructs this state, so the recovered ATV2 baseline is unchanged.
use crate::Result;
use serde::Serialize;

/// Unleashed atv_*.phy [Backflip] and [Three Sixty]. All eight ATV setups
/// agree. These are tuning constants, as with stunt::MIN_TIME.
const BACKFLIP_TORQUE: [f32; 2] = [800., 1400.];
const BACKFLIP_DECREASE: f32 = 2000.;
const SPIN_TORQUE_X: [f32; 2] = [600., 1100.];
const SPIN_TORQUE_Y: [f32; 2] = [200., 1100.];
const SPIN_DECREASE: f32 = 1960.;
/// Min/MaxBackflipTime and Min/MaxThreeSixtyTime, in stick-hold frames.
const HOLD_WINDOW: [f32; 2] = [0., 60.];
/// Unleashed ATV AirborneDamping. Air rotation is damping-dominated in both
/// games (time constants near 0.05 s), so scaling torque by ATV2 AeroDamping
/// over this value keeps Unleashed's rotation rates on the ATV2 body.
const SOURCE_AIR_DAMPING: f32 = 450.;
/// Unleashed ATV_Rider_Stunts.ini PointValue for Stunt42/45/46. HoldPointValue
/// semantics are unrecovered; this port credits PointValue per rotation.
pub const POINTS: f32 = 2000.;
/// Authored landing tolerance: a rotation counts when it is at most this far
/// short of a full turn at touchdown.
const ROTATION_TOLERANCE: f32 = std::f32::consts::FRAC_PI_3;
/// Window after takeoff for the motorcycle adapter's early-air request.
const EARLY_AIR: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Trick {
    Backflip,
    ThreeSixtyLeft,
    ThreeSixtyRight,
}
impl Trick {
    pub fn name(self, rotations: u32) -> String {
        let base = match self {
            Trick::Backflip => "Backflip",
            Trick::ThreeSixtyLeft => "Three Sixty Left",
            Trick::ThreeSixtyRight => "Three Sixty Right",
        };
        match rotations {
            0 | 1 => base.into(),
            2 => format!("Double {base}"),
            3 => format!("Triple {base}"),
            n => format!("{base} x{n}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Started {
        trick: Trick,
        name: String,
    },
    Landed {
        trick: Trick,
        name: String,
        rotations: u32,
        points: f32,
    },
    Bailed {
        trick: Trick,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// Any wheel contact.
    pub grounded: bool,
    pub vertical_speed: f32,
    pub forward_speed: f32,
    pub brake: bool,
    /// Circle=1, Triangle=2, L1=4. The source gesture requires exactly bit 4.
    pub mask: u8,
    /// Trick stick: x right, y pulled back.
    pub axes: [f32; 2],
}

#[derive(Debug, Clone, Copy, Default)]
struct Spin {
    armed: bool,
    active: bool,
    airborne: bool,
    torque: [f32; 2],
    timer: f32,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    trick: Trick,
    angle: f32,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Frame {
    pub active: Option<Trick>,
    pub rotation: f32,
}

#[derive(Debug, Clone)]
pub struct FlipTricks {
    scale: f32,
    backflip: Spin,
    spin: Spin,
    spin_dir: f32,
    /// Stick-hold frames for the backflip (y) and three-sixty (x) strengths.
    hold: [f32; 2],
    airtime: f32,
    pending: Option<Pending>,
    events: Vec<Event>,
    /// One-shot touch request, consumed by the next step.
    request: Option<Trick>,
}

fn window(value: f32) -> f32 {
    let [min, max] = HOLD_WINDOW;
    ((value - min) / (max - min)).clamp(0., 1.)
}

impl FlipTricks {
    /// `air_damping` is the ATV's own AeroDamping (negative in ATV2 files).
    pub fn new(air_damping: f32) -> Result<Self> {
        if !air_damping.is_finite() || air_damping == 0. {
            return Err("invalid flip air damping".into());
        }
        Ok(Self {
            scale: air_damping.abs() / SOURCE_AIR_DAMPING,
            backflip: Spin::default(),
            spin: Spin::default(),
            spin_dir: 0.,
            hold: [0.; 2],
            airtime: 0.,
            pending: None,
            events: Vec::new(),
            request: None,
        })
    }

    /// Airborne wreck angle checks are skipped while a trick is active, as in
    /// the Unleashed wreck chain 004e24a0. Touchdown ends the trick first, so
    /// ground angles and impacts still apply on landing.
    pub fn exempts_air_angles(&self) -> bool {
        self.backflip.active || self.spin.active
    }

    pub fn frame(&self) -> Frame {
        Frame {
            active: if self.backflip.active && self.backflip.airborne {
                Some(Trick::Backflip)
            } else if self.spin.active && self.spin.airborne {
                Some(self.spin_trick())
            } else {
                None
            },
            rotation: self.pending.map_or(0., |p| p.angle),
        }
    }

    /// Authored touch adaptation: a flip picked on the touch trick wheel (or a
    /// long swipe) starts at the early-air strength at any point in the air,
    /// because a menu choice cannot meet the 0.6 s takeoff window. Ignored on
    /// the ground or while a trick is already active.
    pub fn request(&mut self, trick: Trick) {
        self.request = Some(trick);
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn reset(&mut self) {
        *self = Self {
            scale: self.scale,
            ..Self::new(SOURCE_AIR_DAMPING).expect("nonzero damping")
        };
    }

    fn spin_trick(&self) -> Trick {
        if self.spin_dir < 0. {
            Trick::ThreeSixtyLeft
        } else {
            Trick::ThreeSixtyRight
        }
    }

    fn start(&mut self, trick: Trick) {
        self.pending = Some(Pending { trick, angle: 0. });
        self.events.push(Event::Started {
            trick,
            name: trick.name(1),
        });
    }

    /// One physics substep. Returns the body-local torque to add.
    pub fn step(&mut self, input: Input, angular_velocity: [f32; 3], h: f32) -> Result<[f32; 3]> {
        if !h.is_finite()
            || h <= 0.
            || input.mask > 7
            || ![input.vertical_speed, input.forward_speed]
                .iter()
                .chain(input.axes.iter())
                .chain(angular_velocity.iter())
                .all(|v| v.is_finite())
        {
            return Err("invalid flip trick input".into());
        }
        let [x, y] = input.axes;
        for (hold, held) in self.hold.iter_mut().zip([y.abs() > 0.7, x.abs() > 0.7]) {
            *hold = if held { *hold + h * 60. } else { 0. };
        }
        self.airtime = if input.grounded { 0. } else { self.airtime + h };
        let ramp = input.grounded
            && input.vertical_speed > 5.
            && input.forward_speed > 30.
            && !input.brake;
        let early_air = !input.grounded && self.airtime < EARLY_AIR;
        let trick_button = input.mask & 0xf == 4;
        let request = self.request.take().filter(|_| !input.grounded);
        let mut torque = [0.; 3];

        // Backflip 004e4f20.
        let spin_busy = self.spin.active;
        let f = &mut self.backflip;
        let mut started = None;
        let wanted = request == Some(Trick::Backflip) || early_air && trick_button && y > 0.8;
        if !f.active && wanted && !spin_busy {
            *f = Spin {
                active: true,
                airborne: true,
                torque: [-1.25 * BACKFLIP_TORQUE[0], 0.],
                ..Spin::default()
            };
            started = Some(Trick::Backflip);
        } else if !f.active {
            if !ramp || spin_busy {
                f.armed = false;
            } else if y < -0.8 && trick_button {
                f.armed = true;
            } else if y > 0.8 && f.armed {
                let low = 1.25 * BACKFLIP_TORQUE[0];
                let t = window(self.hold[0]);
                *f = Spin {
                    active: true,
                    torque: [-(low + (BACKFLIP_TORQUE[1] - low) * t), 0.],
                    timer: 0.6,
                    ..Spin::default()
                };
            }
        } else if !f.airborne {
            if !input.grounded {
                f.airborne = true;
                started = Some(Trick::Backflip);
            } else {
                f.timer -= h;
                if f.timer <= 0. {
                    *f = Spin::default();
                }
            }
        } else if input.grounded || f.torque[0].abs() < BACKFLIP_TORQUE[0] {
            *f = Spin::default();
        } else {
            if input.mask == 0 && y <= 0. {
                f.torque[0] -= h * y * BACKFLIP_DECREASE;
            }
            torque[0] += f.torque[0] * self.scale;
        }
        if let Some(trick) = started {
            self.start(trick);
        }

        // Three-sixty 004e53d0.
        let flipping = self.backflip.active;
        let s = &mut self.spin;
        let mut started = false;
        let wanted = match request {
            Some(Trick::ThreeSixtyLeft) => Some(-1.),
            Some(Trick::ThreeSixtyRight) => Some(1.),
            _ if early_air && trick_button && x.abs() > 0.7 => Some(x.signum()),
            _ => None,
        };
        if !s.active
            && let Some(dir) = wanted
            && !flipping
        {
            self.spin_dir = dir;
            *s = Spin {
                active: true,
                airborne: true,
                torque: [
                    -SPIN_TORQUE_X[0],
                    dir * SPIN_TORQUE_Y[0].max(0.5 * SPIN_TORQUE_Y[1]),
                ],
                ..Spin::default()
            };
            started = true;
        } else if !s.active {
            if !ramp || flipping {
                s.armed = false;
            } else if trick_button && x.abs() > 0.7 && !s.armed {
                s.armed = true;
                self.spin_dir = -x.signum();
            } else if s.armed && x.abs() > 0.7 && x.signum() == self.spin_dir {
                let t = window(self.hold[1]);
                let lerp = |[a, b]: [f32; 2]| a + (b - a) * t;
                *s = Spin {
                    active: true,
                    torque: [-lerp(SPIN_TORQUE_X), self.spin_dir * lerp(SPIN_TORQUE_Y)],
                    timer: 0.9,
                    ..Spin::default()
                };
            }
        } else if !s.airborne {
            if !input.grounded {
                s.airborne = true;
                started = true;
            } else {
                s.timer -= h;
                if s.timer <= 0. {
                    *s = Spin::default();
                }
            }
        } else if input.grounded || s.torque[1].abs() < SPIN_TORQUE_Y[0] {
            *s = Spin::default();
        } else {
            let dir = self.spin_dir;
            if input.mask == 0 && x.signum() == -dir && x != 0. {
                s.torque[1] -= h * SPIN_DECREASE * dir * x.abs();
                s.torque[0] -= h * x * SPIN_DECREASE;
            }
            torque[0] += s.torque[0] * self.scale;
            torque[1] += s.torque[1] * self.scale;
        }
        if started {
            self.start(self.spin_trick());
        }

        // Rotation is credited only while the trick drives the body.
        let [wx, wy, _] = angular_velocity;
        if let Some(p) = &mut self.pending {
            if self.backflip.active && self.backflip.airborne {
                p.angle += -wx * h;
            } else if self.spin.active && self.spin.airborne {
                // The three-sixty turns about a fixed tilted axis.
                p.angle += (wx * wx + wy * wy).sqrt() * h;
            }
        }
        Ok(torque)
    }

    /// After the wreck checks for this frame: credit a clean landing or record
    /// a bail. A trick that ends in the air still waits for the landing.
    pub fn settle(&mut self, grounded: bool, wrecked: bool) {
        if wrecked {
            self.backflip = Spin::default();
            self.spin = Spin::default();
        }
        let Some(p) = self.pending else {
            return;
        };
        if wrecked {
            self.pending = None;
            self.events.push(Event::Bailed { trick: p.trick });
        } else if grounded && !self.exempts_air_angles() {
            self.pending = None;
            let turn = std::f32::consts::TAU;
            let rotations = ((p.angle + ROTATION_TOLERANCE) / turn).floor().max(0.) as u32;
            if rotations > 0 {
                self.events.push(Event::Landed {
                    trick: p.trick,
                    name: p.trick.name(rotations),
                    rotations,
                    points: POINTS * rotations as f32,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(grounded: bool, mask: u8, axes: [f32; 2]) -> Input {
        Input {
            grounded,
            vertical_speed: 20.,
            forward_speed: 50.,
            brake: false,
            mask,
            axes,
        }
    }

    #[test]
    fn ramp_gesture_arms_forward_and_launches_on_pull_back() {
        let mut f = FlipTricks::new(-800.).unwrap();
        let h = 1. / 60.;
        f.step(input(true, 4, [0., -1.]), [0.; 3], h).unwrap();
        assert!(f.backflip.armed);
        for _ in 0..30 {
            f.step(input(true, 4, [0., 1.]), [0.; 3], h).unwrap();
        }
        assert!(f.backflip.active && !f.backflip.airborne);
        let torque = f.step(input(false, 4, [0., 1.]), [0.; 3], h).unwrap();
        assert!(f.backflip.airborne);
        assert_eq!(torque, [0.; 3]);
        let torque = f.step(input(false, 4, [0., 1.]), [0.; 3], h).unwrap();
        // Nose-up is negative local x, scaled by 800/450.
        assert!(torque[0] < -1.25 * 800. * (800. / 450.) * 0.99);
        assert!(matches!(
            f.take_events()[..],
            [Event::Started {
                trick: Trick::Backflip,
                ..
            }]
        ));
    }

    #[test]
    fn ramp_gate_rejects_slow_brake_or_level_takeoff() {
        let h = 1. / 60.;
        for mutate in [
            |i: &mut Input| i.forward_speed = 20.,
            |i: &mut Input| i.vertical_speed = 2.,
            |i: &mut Input| i.brake = true,
        ] {
            let mut f = FlipTricks::new(-800.).unwrap();
            let mut arm = input(true, 4, [0., -1.]);
            mutate(&mut arm);
            f.step(arm, [0.; 3], h).unwrap();
            assert!(!f.backflip.armed);
        }
    }

    #[test]
    fn other_trick_buttons_do_not_request_flips() {
        let h = 1. / 60.;
        for mask in [0, 1, 2, 5, 6, 7] {
            let mut f = FlipTricks::new(-800.).unwrap();
            f.step(input(false, mask, [0., 1.]), [0.; 3], h).unwrap();
            f.step(input(false, mask, [1., 0.]), [0.; 3], h).unwrap();
            assert!(!f.exempts_air_angles(), "mask {mask}");
        }
    }

    #[test]
    fn early_air_request_and_stick_forward_bleed_end_the_flip() {
        let mut f = FlipTricks::new(-800.).unwrap();
        let h = 1. / 60.;
        f.step(input(false, 4, [0., 1.]), [0.; 3], h).unwrap();
        assert!(f.exempts_air_angles());
        let mut frames = 0;
        while f.exempts_air_angles() {
            f.step(input(false, 0, [0., -1.]), [0.; 3], h).unwrap();
            frames += 1;
            assert!(frames < 60);
        }
        // 1000 -> below 800 at 2000/s takes about 0.1 s.
        assert!((5..=8).contains(&frames), "{frames}");
    }

    #[test]
    fn landing_counts_rotations_with_tolerance_and_bails_on_wreck() {
        let h = 1. / 60.;
        let mut f = FlipTricks::new(-800.).unwrap();
        f.step(input(false, 4, [0., 1.]), [0.; 3], h).unwrap();
        // 1.75 turns of nose-up rotation.
        let rate = -std::f32::consts::TAU * 1.75;
        for _ in 0..60 {
            f.step(input(false, 4, [0., 1.]), [rate, 0., 0.], h)
                .unwrap();
        }
        f.step(input(true, 0, [0., 0.]), [0.; 3], h).unwrap();
        assert!(!f.exempts_air_angles());
        f.take_events();
        f.settle(true, false);
        assert_eq!(
            f.take_events(),
            [Event::Landed {
                trick: Trick::Backflip,
                name: "Backflip".into(),
                rotations: 1,
                points: 2000.
            }]
        );
        f.step(input(false, 4, [-1., 0.]), [0.; 3], h).unwrap();
        assert_eq!(f.frame().active, Some(Trick::ThreeSixtyLeft));
        f.settle(false, true);
        let events = f.take_events();
        assert_eq!(
            events.last(),
            Some(&Event::Bailed {
                trick: Trick::ThreeSixtyLeft
            })
        );
        assert!(!f.exempts_air_angles());
    }

    #[test]
    fn touch_requests_start_late_in_the_air_but_not_on_the_ground() {
        let h = 1. / 60.;
        let mut f = FlipTricks::new(-800.).unwrap();
        f.request(Trick::Backflip);
        f.step(input(true, 0, [0., 0.]), [0.; 3], h).unwrap();
        assert!(!f.exempts_air_angles(), "ground requests are dropped");
        for _ in 0..90 {
            f.step(input(false, 0, [0., 0.]), [0.; 3], h).unwrap();
        }
        assert!(!f.exempts_air_angles());
        f.request(Trick::ThreeSixtyLeft);
        f.step(input(false, 0, [0., 0.]), [0.; 3], h).unwrap();
        assert_eq!(f.frame().active, Some(Trick::ThreeSixtyLeft));
        // A second request while spinning is ignored.
        f.request(Trick::Backflip);
        let torque = f.step(input(false, 0, [0., 0.]), [0.; 3], h).unwrap();
        assert_eq!(f.frame().active, Some(Trick::ThreeSixtyLeft));
        assert!(torque[1] < 0.);
    }

    #[test]
    fn three_sixty_flick_spins_toward_the_second_side() {
        let mut f = FlipTricks::new(-450.).unwrap();
        let h = 1. / 60.;
        f.step(input(true, 4, [-1., 0.]), [0.; 3], h).unwrap();
        f.step(input(true, 0, [1., 0.]), [0.; 3], h).unwrap();
        assert!(f.spin.active);
        f.step(input(false, 0, [0., 0.]), [0.; 3], h).unwrap();
        let torque = f.step(input(false, 0, [0., 0.]), [0.; 3], h).unwrap();
        assert_eq!(f.frame().active, Some(Trick::ThreeSixtyRight));
        assert!(torque[1] > 0. && torque[0] < 0.);
        assert_eq!(Trick::ThreeSixtyRight.name(2), "Double Three Sixty Right");
    }
}
