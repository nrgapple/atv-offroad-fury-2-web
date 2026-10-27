//! The original Tag mini-game (research/GAMES.md "Tag").
//!
//! ATV Offroad Fury 2's Tag is ball tag: one rider carries the ball and
//! scores while holding it; the others take it by touching the ball or the
//! carrier. Exact-build evidence (private Ghidra exports of `SCUS_972.11`,
//! its `QuadzillaEventTag.cpp` event, vtable `00459300`; bounded reading,
//! no runtime traces):
//!
//! - Construction `00244ac0`/`00244d20`: four rider records (+0x1cc +
//!   rider * 0x10: current possession, total possession, longest
//!   possession, points awarded); the carrier (+0x1c4, +0x1c8); the drop
//!   time stamp (+0x214, milliseconds); the `tagball.psg` body and
//!   `tagBeep.vag`. The riders are the session's (split screen); the event
//!   has no CPU riders.
//! - Per frame `00244ef0`: the carrier's current and total possession grow
//!   by the frame time, and every whole second of total possession is
//!   worth 250 points (the score shown on the HUD, rider +0x104).
//!   A wrecked carrier drops the ball ("%s dropped the ball!", `00246000`).
//!   For 2 s after a change of carrier nobody can take the ball. After
//!   that, any other upright rider whose body touches the ball or the
//!   carrier takes it, except the rider who just lost it within 500 ms.
//!   A loose ball goes to whoever touches it.
//! - Gain `00246310`: "You are it!" (1.6 s) for the new carrier and
//!   "%s has the ball!" (2 s) for the others, the beep, and the ball is
//!   tossed 3 units over the new carrier with its velocity plus 11 units/s
//!   up; it rides on the ATV once it comes back down within 3 units.
//! - Drop `00246000`: the carrier's longest possession keeps the larger
//!   of itself and the current one (whole seconds); the current one
//!   resets.
//! - Ball `00246da0`: a ball resting on the ground for 2.5 s while carried
//!   is lost ("You have lost the ball!"); after 5 s it is thrown back into
//!   play (`00246500`). A ball outside the course boundary (110-unit
//!   margin, `0020aec0`) is thrown back toward the map centre at about 170
//!   units/s and 112 units/s up (`00246ec0`).
//! - End `00247160`: riders are ranked by score (place 1 first); the
//!   TagOver menu shows the result.
//!
//! The time limit is the session's (settings +0x57c, default 120 s).

use crate::math::Det;

/// `00244ef0`: points per whole second of total possession.
pub const POINTS_PER_SECOND: u32 = 250;
/// `00244ef0`: no steal for this long after a change of carrier.
pub const IMMUNITY: f32 = 2.0;
/// `00244ef0`: the rider who lost the ball cannot retake it this soon.
pub const REGRAB_GUARD: f32 = 0.5;
/// `00246da0`: a carried ball resting on the ground this long is lost.
pub const GROUND_LOSS: f32 = 2.5;
/// `00246da0`: a ball resting this long is thrown back into play.
pub const GROUND_RESPAWN: f32 = 5.0;
/// `00246310` banners.
pub const YOU_ARE_IT_BANNER: f32 = 1.6;
pub const HAS_BALL_BANNER: f32 = 2.0;
/// `00246310`: the toss to the new carrier (height, upward speed).
pub const TOSS_HEIGHT: f32 = 3.0;
pub const TOSS_UP: f32 = 11.0;
/// `00244ef0`: the ball rides on the ATV once it falls back within this
/// height of the carrier.
pub const CATCH_HEIGHT: f32 = 3.0;
/// `00244ef0`: the loose ball's homing step toward the carrier.
pub const HOMING_STEP: f32 = 2.3;
/// `0020aec0` margin and `00246ec0` throw-in.
pub const BOUNDARY_MARGIN: f32 = 110.0;
pub const THROW_IN_SPEED: f32 = 170.4714;
pub const THROW_IN_UP: f32 = 112.152_245;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Record {
    /// +0x1cc: this possession.
    pub current: f32,
    /// +0x1d0: all possessions.
    pub total: f32,
    /// +0x1d4: the longest possession (whole seconds).
    pub longest: u32,
    /// The score (points awarded so far).
    pub score: u32,
}

/// One rider's view this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rider {
    /// The rider is wrecked (+0x7f0).
    pub wrecked: bool,
    /// The rider's body touches the ball.
    pub touches_ball: bool,
    /// The rider's body touches the carrier's.
    pub touches_carrier: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A new carrier: "You are it!" / "%s has the ball!" and the beep.
    Gain { rider: usize },
    /// The carrier lost the ball.
    Drop { rider: usize, reason: DropReason },
    /// Points awarded to a rider.
    Points { rider: usize, points: u32 },
    /// Throw the ball back into play (`00246500`).
    Respawn,
    /// The time limit ran out.
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    /// "%s dropped the ball!" (a wreck).
    Wreck,
    /// Taken by another rider.
    Taken,
    /// "You have lost the ball!" (resting on the ground).
    Lost,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub records: Vec<Record>,
    /// +0x1c4/+0x1c8: the carrier, and the last carrier's index.
    pub carrier: Option<usize>,
    pub last_carrier: usize,
    /// Seconds since the last drop (+0x214's clock).
    pub since_drop: f32,
    /// The ball rides on the carrier's ATV (ball +0x1a0).
    pub attached: bool,
    /// Seconds the ball has rested on the ground (ball +0x1a4).
    pub resting: f32,
    pub clock: f32,
    pub time_limit: f32,
    pub over: bool,
}

impl Game {
    pub fn new(riders: usize, time_limit: f32) -> Self {
        Self {
            records: vec![Record::default(); riders],
            carrier: None,
            last_carrier: 0,
            since_drop: f32::INFINITY,
            attached: false,
            resting: 0.,
            clock: time_limit,
            time_limit,
            over: false,
        }
    }

    pub fn restart(&mut self) {
        *self = Self::new(self.records.len(), self.time_limit);
    }

    fn drop_ball(&mut self, reason: DropReason, events: &mut Vec<Event>) {
        if let Some(rider) = self.carrier.take() {
            let r = &mut self.records[rider];
            r.longest = r.longest.max(r.current as u32);
            r.current = 0.;
            self.last_carrier = rider;
            self.since_drop = 0.;
            self.attached = false;
            events.push(Event::Drop { rider, reason });
        }
    }

    fn gain(&mut self, rider: usize, events: &mut Vec<Event>) {
        self.carrier = Some(rider);
        self.last_carrier = rider;
        self.attached = false;
        self.resting = 0.;
        events.push(Event::Gain { rider });
    }

    /// The ball came back down onto the carrier's ATV.
    pub fn catch(&mut self) {
        if self.carrier.is_some() {
            self.attached = true;
        }
    }

    /// `00244ef0` and `00246da0` for one frame. `ball_resting` is the ball
    /// lying within a unit of the ground.
    pub fn tick(&mut self, dt: f32, riders: &[Rider], ball_resting: bool) -> Vec<Event> {
        let mut events = Vec::new();
        if self.over {
            return events;
        }
        self.since_drop += dt;
        self.clock -= dt;
        if self.clock <= 0. {
            self.clock = 0.;
            self.drop_ball(DropReason::Taken, &mut Vec::new());
            self.over = true;
            events.push(Event::Over);
            return events;
        }
        // `00246da0`: the ball's rest timer.
        if ball_resting {
            self.resting += dt;
            if self.attached && self.resting > GROUND_LOSS {
                self.drop_ball(DropReason::Lost, &mut events);
                self.resting = 0.;
            }
            if self.resting > GROUND_RESPAWN {
                events.push(Event::Respawn);
                self.resting = 0.;
            }
        } else {
            self.resting = 0.;
        }
        if let Some(c) = self.carrier {
            let r = &mut self.records[c];
            r.current += dt;
            r.total += dt;
            let due = r.total as u32 * POINTS_PER_SECOND;
            if due != r.score {
                events.push(Event::Points {
                    rider: c,
                    points: due - r.score,
                });
                r.score = due;
            }
            if riders.get(c).is_some_and(|r| r.wrecked) {
                self.drop_ball(DropReason::Wreck, &mut events);
            } else if self.records[c].current < IMMUNITY {
                return events;
            }
        }
        for (i, rider) in riders.iter().enumerate() {
            if Some(i) == self.carrier || rider.wrecked {
                continue;
            }
            let touched = rider.touches_ball || (self.carrier.is_some() && rider.touches_carrier);
            if !touched {
                continue;
            }
            if i == self.last_carrier && self.since_drop < REGRAB_GUARD {
                continue;
            }
            self.drop_ball(DropReason::Taken, &mut events);
            self.gain(i, &mut events);
            break;
        }
        events
    }

    /// `00247160`: riders by place (highest score first; equal scores keep
    /// slot order, see research/GAMES.md).
    pub fn standings(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.records.len()).collect();
        order.sort_by(|a, b| self.records[*b].score.cmp(&self.records[*a].score));
        order
    }
}

/// The ball: an authored stand-in for the `tagball.psg` rigid body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
}

const GRAVITY: f32 = 32.174;
/// Ball radius (authored).
pub const BALL_RADIUS: f32 = 1.5;
/// The carried ball rides this high over the ATV's origin (authored).
pub const CARRY_HEIGHT: f32 = 4.0;

impl Ball {
    /// `00246310`: tossed over the new carrier.
    pub fn toss(carrier: [f32; 3], velocity: [f32; 3]) -> Self {
        Self {
            position: [carrier[0], carrier[1] + TOSS_HEIGHT, carrier[2]],
            velocity: [velocity[0], velocity[1] + TOSS_UP, velocity[2]],
        }
    }

    /// `00246ec0`: thrown back toward `centre`.
    pub fn throw_in(from: [f32; 3], centre: [f32; 3]) -> Self {
        let (dx, dz) = (centre[0] - from[0], centre[2] - from[2]);
        let len = dx.hypot_d(dz).max(1e-3);
        // Horizontal 170 units/s is far more than a short course needs; the
        // original's value carries it across a whole map. The port scales
        // the throw to land near the centre (research/GAMES.md).
        let flight = 2. * THROW_IN_UP / GRAVITY;
        let speed = (len / flight).min(THROW_IN_SPEED);
        Self {
            position: from,
            velocity: [dx / len * speed, THROW_IN_UP, dz / len * speed],
        }
    }

    /// One step. `ground` gives the terrain height under (x, z); a carried
    /// ball follows `carrier` and reports whether it came back down onto it.
    pub fn step(
        &mut self,
        dt: f32,
        ground: impl Fn(f32, f32) -> f32,
        carrier: Option<([f32; 3], [f32; 3])>,
        attached: bool,
    ) -> bool {
        if let (Some((p, v)), true) = (carrier, attached) {
            self.position = [p[0], p[1] + CARRY_HEIGHT, p[2]];
            self.velocity = v;
            return true;
        }
        self.velocity[1] -= GRAVITY * dt;
        for i in 0..3 {
            self.position[i] += self.velocity[i] * dt;
        }
        if let Some((p, v)) = carrier {
            // `00244ef0`: the loose ball homes on its carrier, at most
            // 2.3 units a step horizontally.
            let (dx, dz) = (p[0] - self.position[0], p[2] - self.position[2]);
            let d = dx.hypot_d(dz);
            let k = if d > HOMING_STEP { HOMING_STEP / d } else { 1. };
            self.position[0] += dx * k;
            self.position[2] += dz * k;
            if self.position[1] < p[1] + CATCH_HEIGHT && self.velocity[1] < v[1] {
                return true;
            }
        }
        let floor = ground(self.position[0], self.position[2]) + BALL_RADIUS;
        if self.position[1] < floor {
            self.position[1] = floor;
            if self.velocity[1] < -3. {
                self.velocity[1] = -self.velocity[1] * 0.5;
            } else {
                self.velocity[1] = 0.;
            }
            let k = (1. - 1.5 * dt).max(0.);
            self.velocity[0] *= k;
            self.velocity[2] *= k;
        }
        false
    }

    /// Lying within a unit of the ground (`00246da0`).
    pub fn resting(&self, ground: impl Fn(f32, f32) -> f32) -> bool {
        self.position[1] - BALL_RADIUS <= ground(self.position[0], self.position[2]) + 1.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1. / 60.;

    fn riders(n: usize) -> Vec<Rider> {
        vec![Rider::default(); n]
    }

    fn run(g: &mut Game, r: &[Rider], seconds: f32) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..(seconds * 60.).round() as usize {
            all.extend(g.tick(DT, r, false));
        }
        all
    }

    #[test]
    fn loose_ball_goes_to_whoever_touches_it() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[1].touches_ball = true;
        assert_eq!(g.tick(DT, &r, false), vec![Event::Gain { rider: 1 }]);
        assert_eq!(g.carrier, Some(1));
    }

    #[test]
    fn carrier_scores_250_per_whole_second() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[0].touches_ball = true;
        g.tick(DT, &r, false);
        r[0].touches_ball = false;
        let events = run(&mut g, &r, 3.01);
        let points: u32 = events
            .iter()
            .filter_map(|e| match e {
                Event::Points { rider: 0, points } => Some(*points),
                _ => None,
            })
            .sum();
        assert_eq!(points, 750);
        assert_eq!(g.records[0].score, 750);
        assert_eq!(g.records[1].score, 0);
    }

    #[test]
    fn two_second_immunity_then_contact_takes_the_ball() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[0].touches_ball = true;
        g.tick(DT, &r, false);
        r[0].touches_ball = false;
        r[1].touches_carrier = true;
        // Within 2 s the chaser touching the carrier takes nothing.
        let events = run(&mut g, &r, 1.9);
        assert!(!events.iter().any(|e| matches!(e, Event::Gain { .. })));
        let events = run(&mut g, &r, 0.2);
        assert!(events.contains(&Event::Drop {
            rider: 0,
            reason: DropReason::Taken
        }));
        assert!(events.contains(&Event::Gain { rider: 1 }));
        assert_eq!(g.carrier, Some(1));
        assert_eq!(g.records[0].longest, 2);
        assert_eq!(g.records[0].current, 0.);
    }

    #[test]
    fn tag_back_needs_the_new_carriers_immunity_to_pass() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[1].touches_ball = true;
        g.tick(DT, &r, false);
        // Both keep touching: rider 0 cannot take it back for 2 s.
        r[0].touches_carrier = true;
        let events = run(&mut g, &r, 1.95);
        assert!(!events.iter().any(|e| matches!(e, Event::Gain { .. })));
        let events = run(&mut g, &r, 0.1);
        assert!(events.contains(&Event::Gain { rider: 0 }));
    }

    #[test]
    fn wrecked_carrier_drops_and_cannot_regrab_for_half_a_second() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[0].touches_ball = true;
        g.tick(DT, &r, false);
        r[0].wrecked = true;
        let events = g.tick(DT, &r, false);
        assert!(events.contains(&Event::Drop {
            rider: 0,
            reason: DropReason::Wreck
        }));
        assert_eq!(g.carrier, None);
        // Upright again and touching the loose ball: guarded for 500 ms.
        r[0].wrecked = false;
        let events = run(&mut g, &r, 0.45);
        assert!(events.is_empty());
        let events = run(&mut g, &r, 0.1);
        assert_eq!(events, vec![Event::Gain { rider: 0 }]);
    }

    #[test]
    fn wrecked_rider_cannot_take_the_ball() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[1].touches_ball = true;
        r[1].wrecked = true;
        assert!(g.tick(DT, &r, false).is_empty());
    }

    #[test]
    fn carried_ball_on_the_ground_is_lost_then_respawns() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[0].touches_ball = true;
        g.tick(DT, &r, false);
        r[0].touches_ball = false;
        g.catch();
        let mut events = Vec::new();
        for _ in 0..(2.6 * 60.) as usize {
            events.extend(g.tick(DT, &r, true));
        }
        assert!(events.contains(&Event::Drop {
            rider: 0,
            reason: DropReason::Lost
        }));
        let mut events = Vec::new();
        for _ in 0..(5.1 * 60.) as usize {
            events.extend(g.tick(DT, &r, true));
        }
        assert!(events.contains(&Event::Respawn));
    }

    #[test]
    fn time_out_ends_and_ranks_by_score() {
        let mut g = Game::new(3, 10.);
        let mut r = riders(3);
        r[2].touches_ball = true;
        g.tick(DT, &r, false);
        r[2].touches_ball = false;
        let events = run(&mut g, &r, 10.);
        assert!(events.contains(&Event::Over));
        assert!(g.over);
        assert_eq!(g.carrier, None);
        assert_eq!(g.standings(), vec![2, 0, 1]);
        assert_eq!(g.records[2].score, 9 * 250);
        // Nothing after the end.
        assert!(g.tick(DT, &r, false).is_empty());
    }

    #[test]
    fn tossed_ball_lands_back_on_the_carrier() {
        let carrier = [10., 0., 10.];
        let mut b = Ball::toss(carrier, [5., 0., 0.]);
        let mut caught = false;
        for _ in 0..120 {
            if b.step(DT, |_, _| 0., Some((carrier, [0.; 3])), false) {
                caught = true;
                break;
            }
        }
        assert!(caught);
        assert!((b.position[0] - carrier[0]).abs() < 1e-3);
    }

    #[test]
    fn loose_ball_settles_and_throw_in_heads_for_the_centre() {
        let mut b = Ball::toss([0., 10., 0.], [3., 0., 0.]);
        for _ in 0..600 {
            b.step(DT, |_, _| 0., None, false);
        }
        assert!(b.resting(|_, _| 0.));
        let t = Ball::throw_in([500., 0., 0.], [0., 0., 0.]);
        assert!(t.velocity[0] < 0. && t.velocity[1] > 100.);
    }
}
