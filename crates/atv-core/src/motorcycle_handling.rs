//! Authored single-track ground handling for the motorcycles: the default
//! riding model. It replaces the recovered MX side-force curves and
//! steer-proportional bank, which let the bike crab 20-70 degrees through a
//! corner on ATV2 terrain (research/MOTORCYCLE_HANDLING.md).
//!
//! The rider asks for a lean; the lean sets a lateral acceleration the tires
//! can hold; the front wheel is steered to the angle that gives that turn.
//! Each grounded tire then removes its own sideways motion with an impulse
//! bounded by its grip, so the bike tracks where it points until a tire is
//! asked for more than it has. Braking or spinning the rear spends its side
//! grip and lets it step out; the front steers into the slide. The bank shown
//! is the one that balances the turn the bike is making.
//!
//! Suspension, engine, wheelie/stoppie, airborne control, preload, spins and
//! the wreck chain stay on the recovered MX controller.
use super::*;
use crate::math::Det;
use crate::tire_wear::Handling as Mode;

/// Feet per second squared the tires are asked for at full stick, as a
/// fraction of what they can hold.
const COMMAND_SHARE: f32 = 0.9;
const SLOW_GRIP: f32 = 0.6;
/// Seconds of softened side grip and heading alignment after a touchdown.
pub(super) const LANDING_TIME: f32 = 0.3;
/// Steering lock relative to the source MaxSteerAngle (30 degrees): a
/// motocross bike turns its bars about 40 degrees each way.
pub(super) const LOCK_SCALE: f32 = 1.25;
/// Drive or brake force, per unit of tire load, that spends all of a tire's
/// side grip. The source drive threshold (2 x load) never limits a 125.
pub(super) const DRIVE_SHARE: f32 = 1.15;

#[derive(Clone, Copy, Debug)]
pub(super) struct Tuning {
    /// Side grip in g on standard dirt at 30 ft/s and from 110 ft/s. Below
    /// 30 ft/s it eases to `SLOW_GRIP` of the first, so a walking-pace turn
    /// on full lock does not lay the bike over.
    grip: [f32; 2],
    /// Share of side grip a locked or spinning rear tire keeps.
    slide_grip: f32,
    /// How far the front steers into a rear slide (per radian of slip).
    counter_steer: f32,
    /// Lean response (rad/s natural frequency) at low and high speed.
    response: [f32; 2],
    /// Shown lean per unit of balanced lean tangent.
    lean_scale: f32,
    /// How firmly the rider holds the intended turn rate while the rear
    /// slides (1/s): the difference between a slide and a spin.
    pub catch: f32,
    /// Share of side grip that full drive or braking spends.
    drive_cost: f32,
    /// Side grip of a rear tire that has slid right round, relative to a
    /// rolling one.
    dig: f32,
}

impl Tuning {
    pub fn of(mode: Mode) -> Self {
        match mode {
            Mode::Original => Self {
                grip: [1.7, 1.3],
                slide_grip: 0.62,
                counter_steer: 1.1,
                response: [9.5, 6.5],
                lean_scale: 0.74,
                catch: 6.,
                drive_cost: 0.6,
                dig: 1.4,
            },
            Mode::Sport => Self {
                grip: [1.9, 1.45],
                slide_grip: 0.7,
                counter_steer: 1.2,
                response: [10.5, 7.5],
                lean_scale: 0.68,
                catch: 8.,
                drive_cost: 0.5,
                dig: 1.5,
            },
            Mode::Simulation => Self {
                grip: [1.2, 1.05],
                slide_grip: 0.55,
                counter_steer: 0.8,
                response: [8., 5.5],
                lean_scale: 1.,
                catch: 2.5,
                drive_cost: 0.92,
                dig: 1.15,
            },
        }
    }
    fn grip_at(&self, speed: f32) -> f32 {
        let t = ((speed - 30.) / 80.).clamp(0., 1.);
        let slow = SLOW_GRIP + (1. - SLOW_GRIP) * ((speed - 8.) / 22.).clamp(0., 1.);
        slow * (self.grip[0] + (self.grip[1] - self.grip[0]) * t)
    }
}

/// One grounded tire as the side-grip solver sees it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Patch {
    pub grounded: bool,
    /// Contact point, ground normal and the velocity of the ground there.
    pub point: V,
    pub normal: V,
    pub ground: V,
    /// Rolling direction in the ground plane.
    pub heading: V,
    pub load: f32,
    /// Surface side grip relative to standard dirt.
    pub surface: f32,
    /// Share of the tire's grip already used along its rolling direction.
    pub used: f32,
    pub sliding: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct State {
    /// Filtered stick (-1..1) and its rate.
    pub command: f32,
    rate: f32,
    /// Lateral acceleration asked for (ft/s², + toward the bike's +x).
    pub demand: f32,
    /// Lateral acceleration of the path the bike is actually on.
    pub delivered: f32,
    /// Bank shown (rad, + toward +x).
    pub lean: f32,
    /// Rear sideslip (rad, + when the rear runs wide toward +x).
    pub slip: f32,
    /// 0..1: how hard the rear is sliding, for the rider and the roost.
    pub slide: f32,
    /// Seconds of landing forgiveness left.
    pub landing: f32,
    /// Side grip (g) available at this speed on this surface.
    pub grip: f32,
    /// Share of the rear tire's grip spent driving or braking (0..1).
    pub drive: f32,
    surface: f32,
    travel: Option<f32>,
}

impl State {
    /// Once per frame: the stick becomes a lean command, a lateral
    /// acceleration and the front steer angle that delivers it.
    pub fn steer(
        &mut self,
        tuning: Tuning,
        stick: f32,
        speed: f32,
        wheelbase: f32,
        lock: f32,
        gravity: f32,
        dt: f32,
    ) -> f32 {
        let v = speed.abs();
        let t = ((v - 15.) / 85.).clamp(0., 1.);
        let frequency = tuning.response[0] + (tuning.response[1] - tuning.response[0]) * t;
        // Critically damped, exact for any step length.
        let decay = (-frequency * dt).exp_d();
        let error = self.command - stick;
        let approach = self.rate + frequency * error;
        self.command = stick + (error + approach * dt) * decay;
        self.rate = (self.rate - frequency * approach * dt) * decay;
        let surface = if self.surface > 0. { self.surface } else { 1. };
        self.grip = tuning.grip_at(v) * surface;
        // Full stick is the tightest turn available: the steering lock when
        // slow, the tires when fast.
        let by_lock = v * v * lock.tan_d() / wheelbase;
        let limit = (COMMAND_SHARE * self.grip * gravity).min(by_lock);
        self.demand = self.command * limit;
        let angle = if v < 1. {
            self.command * lock
        } else {
            (self.demand * wheelbase / (v * v)).atan_d()
        };
        // The rider steers into a rear slide to hold it.
        let counter = tuning.counter_steer * self.slip * (v / 12.).clamp(0., 1.);
        (angle + counter).clamp(-lock, lock)
    }

    /// Each substep, after every other force is known: side impulses that
    /// stop each tire moving across its rolling direction, within its grip.
    /// Returns the force and yaw torque (about the body's up axis) to add.
    /// `angular` is the whole angular velocity: on a leaned bike a pitch
    /// rate moves the contacts sideways too.
    #[allow(clippy::too_many_arguments)]
    pub fn side_forces(
        &mut self,
        tuning: Tuning,
        patches: &mut [Patch; 2],
        velocity: V,
        angular: V,
        center: V,
        up: V,
        inverse_mass: f32,
        inverse_yaw_inertia: f32,
        static_load: [f32; 2],
        h: f32,
    ) -> (V, f32) {
        let mut v = velocity;
        // Change in the rate about the body's up axis.
        let mut w = 0_f32;
        let mut impulse = [0_f32; 2];
        let mut budget = [0_f32; 2];
        let mut side = [[0_f32; 3]; 2];
        let mut arm = [0_f32; 2];
        let speed = dot(velocity, velocity).sqrt();
        let landing = (self.landing / LANDING_TIME).clamp(0., 1.);
        let mut surface = 0.;
        let mut grounded = 0.;
        for (i, p) in patches.iter().enumerate() {
            if !p.grounded {
                continue;
            }
            side[i] = crate::wheel::cross(p.normal, p.heading);
            // Yaw lever of a side force at this contact.
            arm[i] = dot(crate::wheel::cross(sub(p.point, center), side[i]), up);
            // A tire unloaded over a crest keeps part of its static share, so
            // a bump mid-corner does not throw the bike wide.
            let load = 0.6 * p.load + 0.4 * static_load[i].min(2. * p.load);
            // Driving or braking spends side grip: a fifth of it at full
            // use by default, nearly all of it in Simulation.
            let combined = (1. - (tuning.drive_cost * p.used.clamp(0., 1.)).powi(2)).sqrt();
            let base = combined.min(match (p.sliding, i) {
                (false, _) => 1.,
                (true, 1) => tuning.slide_grip,
                (true, _) => 0.5 * (1. + tuning.slide_grip),
            });
            // A rear that has come round digs in and the front pushes, so a
            // slide settles at an angle instead of spinning the bike, however
            // little grip the surface has and whatever the throttle is doing.
            let dug = ((self.slip.abs() - 0.12) / 0.3).clamp(0., 1.);
            let dug = dug * dug * (3. - 2. * dug);
            let slide = if i == 1 {
                base + (tuning.dig - base) * dug
            } else {
                base * (1. - 0.35 * dug)
            };
            let soft = 1. - 0.5 * landing;
            budget[i] = tuning.grip_at(speed) * p.surface * load * slide * soft * h;
            surface += p.surface;
            grounded += 1.;
        }
        if grounded > 0. {
            self.surface = surface / grounded;
        }
        let used = if patches[1].grounded {
            patches[1].used.clamp(0., 1.)
        } else {
            0.
        };
        self.drive += (used - self.drive) * (h * 10.).min(1.);
        for _ in 0..4 {
            for i in 0..2 {
                let p = &patches[i];
                if !p.grounded {
                    continue;
                }
                let spin = crate::wheel::cross(add(angular, scale(up, w)), sub(p.point, center));
                let across = dot(sub(add(v, spin), p.ground), side[i]);
                let k = inverse_mass + arm[i] * arm[i] * inverse_yaw_inertia;
                let next = (impulse[i] - across / k).clamp(-budget[i], budget[i]);
                let change = next - impulse[i];
                impulse[i] = next;
                v = add(v, scale(side[i], change * inverse_mass));
                w += change * arm[i] * inverse_yaw_inertia;
            }
        }
        let mut force = [0.; 3];
        let mut torque = 0.;
        for i in 0..2 {
            force = add(force, scale(side[i], impulse[i] / h));
            torque += impulse[i] * arm[i] / h;
            patches[i].sliding = budget[i] > 0. && impulse[i].abs() >= 0.999 * budget[i];
        }
        // Rear sideslip from the velocity the rear contact is left with.
        let rear = &patches[1];
        if rear.grounded && speed > 6. {
            let spin = crate::wheel::cross(add(angular, scale(up, w)), sub(rear.point, center));
            let contact = sub(add(v, spin), rear.ground);
            let across = dot(contact, side[1]);
            let along = dot(contact, rear.heading).abs().max(1.);
            let slip = across.atan2_d(along);
            self.slip += (slip - self.slip) * (h * 30.).min(1.);
        } else {
            self.slip *= 1. - (h * 6.).min(1.);
        }
        let slide = (self.slip.abs() / 0.35).clamp(0., 1.);
        self.slide += (slide - self.slide) * (h * 12.).min(1.);
        (force, torque)
    }

    /// Once per frame after physics: the bank that balances the turn. The
    /// rider leans as asked, but no further than the path being ridden plus a
    /// margin, so a bike that is sliding wide stands up instead of lying down.
    #[allow(clippy::too_many_arguments)]
    pub fn bank(
        &mut self,
        tuning: Tuning,
        velocity: V,
        grounded: bool,
        wheelie: bool,
        current: f32,
        gravity: f32,
        dt: f32,
    ) -> f32 {
        let speed = (velocity[0] * velocity[0] + velocity[2] * velocity[2]).sqrt();
        let travel = velocity[0].atan2_d(velocity[2]);
        let turned = match self.travel {
            Some(previous) if speed > 4. && grounded && dt > 0. => {
                let mut change = travel - previous;
                if change > std::f32::consts::PI {
                    change -= std::f32::consts::TAU;
                } else if change < -std::f32::consts::PI {
                    change += std::f32::consts::TAU;
                }
                (speed * change / dt).clamp(-4. * gravity, 4. * gravity)
            }
            _ => self.demand,
        };
        self.travel = (speed > 4.).then_some(travel);
        self.delivered += (turned - self.delivered) * (dt * 9.).min(1.);
        self.landing = (self.landing - dt).max(0.);
        if !grounded {
            self.lean = current;
            return current;
        }
        let margin = 0.3 * gravity;
        let balanced = self
            .demand
            .clamp(self.delivered - margin, self.delivered + margin);
        // On the rear wheel the bike is balanced nearer upright.
        let share = if wheelie { 0.45 } else { 1. };
        let target = (share * tuning.lean_scale * balanced / gravity).atan_d();
        let step = ((target - current) * (dt * 14.).min(1.)).clamp(-5. * dt, 5. * dt);
        self.lean = current + step;
        self.lean
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patches(steer: f32) -> [Patch; 2] {
        let heading = [steer.sin_d(), 0., steer.cos_d()];
        [
            Patch {
                grounded: true,
                point: [0., 0., 2.5],
                normal: [0., 1., 0.],
                heading,
                load: 200.,
                surface: 1.,
                ..Default::default()
            },
            Patch {
                grounded: true,
                point: [0., 0., -2.5],
                normal: [0., 1., 0.],
                heading: [0., 0., 1.],
                load: 200.,
                surface: 1.,
                ..Default::default()
            },
        ]
    }

    /// Integrate a planar bike under the side-grip solver alone.
    fn ride(steer: f32, speed: f32, rear_sliding: bool, seconds: f32) -> (V, f32, State) {
        let tuning = Tuning::of(Mode::Original);
        let mut state = State::default();
        let (mass, inertia) = (400. / 32.2, 18.4);
        let mut v = [0., 0., speed];
        let mut w = 0.;
        let mut yaw = 0_f32;
        let h = 1. / 110.;
        for _ in 0..(seconds / h) as usize {
            let (s, c) = yaw.sin_cos_d();
            let to_world = |p: V| [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]];
            let mut patches = patches(steer);
            for p in &mut patches {
                p.point = to_world(p.point);
                p.heading = to_world(p.heading);
            }
            patches[1].sliding = rear_sliding;
            let (force, torque) = state.side_forces(
                tuning,
                &mut patches,
                v,
                [0., w, 0.],
                [0.; 3],
                [0., 1., 0.],
                1. / mass,
                1. / inertia,
                [200.; 2],
                h,
            );
            v = add(v, scale(force, h / mass));
            w += torque * h / inertia;
            yaw += w * h;
        }
        let (s, c) = yaw.sin_cos_d();
        // Velocity in the bike frame: x across, z along.
        ([c * v[0] - s * v[2], 0., s * v[0] + c * v[2]], w, state)
    }

    #[test]
    fn a_gripping_bike_follows_its_front_wheel_without_crabbing() {
        let steer = 0.08_f32;
        let (v, w, state) = ride(steer, 40., false, 2.);
        // Kinematic single-track turn: yaw rate = v tan(steer) / wheelbase.
        let expected = v[2] * steer.tan_d() / 5.;
        assert!((w - expected).abs() < 0.05 * expected, "{w} vs {expected}");
        // The centre runs between the wheels, so half the steer angle at most.
        assert!((v[0] / v[2]).atan_d().abs() < 0.6 * steer);
        assert!(state.slide < 0.05);
    }

    #[test]
    fn asking_for_more_than_the_grip_slides_instead_of_turning_harder() {
        // 0.5 rad of steer at 80 ft/s asks for many g.
        let (v, w, _) = ride(0.5, 80., false, 0.5);
        let lateral_g = (v[2] * w / 32.2).abs();
        assert!(lateral_g < 1.8, "{lateral_g}");
    }

    #[test]
    fn a_locked_rear_steps_out_and_reports_the_slide() {
        let (_, grip_rate, _) = ride(0.12, 45., false, 0.6);
        let (_, slide_rate, state) = ride(0.12, 45., true, 0.6);
        assert!(slide_rate > grip_rate);
        assert!(state.slip.abs() > 0.03 && state.slide > 0.05);
    }

    #[test]
    fn full_stick_asks_for_the_lock_when_slow_and_the_tires_when_fast() {
        let tuning = Tuning::of(Mode::Original);
        let lock = 30_f32.to_radians();
        let mut state = State::default();
        let mut slow = 0.;
        let mut fast = 0.;
        for _ in 0..240 {
            slow = state.steer(tuning, 1., 8., 5., lock, 32.2, 1. / 60.);
        }
        assert!((slow - lock).abs() < 0.01);
        let mut state = State::default();
        for _ in 0..240 {
            fast = state.steer(tuning, 1., 80., 5., lock, 32.2, 1. / 60.);
        }
        assert!(fast < 0.1 * lock);
        let g = state.demand / 32.2;
        assert!(g > 1. && g < tuning.grip_at(80.));
    }

    #[test]
    fn the_bank_balances_the_turn_and_stands_up_when_the_bike_runs_wide() {
        let tuning = Tuning::of(Mode::Simulation);
        let mut state = State {
            demand: 32.2,
            ..Default::default()
        };
        // Travel direction turning at 1 g: 45 degrees of lean.
        let mut lean = 0.;
        let (speed, dt) = (40_f32, 1. / 60.);
        let mut heading = 0_f32;
        for _ in 0..180 {
            heading += 32.2 / speed * dt;
            let v = [speed * heading.sin_d(), 0., speed * heading.cos_d()];
            lean = state.bank(tuning, v, true, false, lean, 32.2, dt);
        }
        assert!((lean - std::f32::consts::FRAC_PI_4).abs() < 0.03, "{lean}");
        // Still asking for 1 g but travelling straight: only the margin.
        for _ in 0..180 {
            let v = [speed * heading.sin_d(), 0., speed * heading.cos_d()];
            lean = state.bank(tuning, v, true, false, lean, 32.2, dt);
        }
        assert!(lean < 0.31, "{lean}");
    }
}
