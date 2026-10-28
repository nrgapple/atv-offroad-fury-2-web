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
//! - Knock loose (`00244ef0`): while the ball rides on the carrier's ATV,
//!   a trick that has just begun (the stunt record's open-event flag,
//!   racer +0x30 +8 & 0x40, set by `0024da00`; the rider's main stunt
//!   clock +0x884 +0x3fc, reset to 0 by `002da6c0` as a trick starts, no
//!   more than one frame) knocks it loose with a 25 % chance (a uniform
//!   draw above 0.75): the ball jumps 2 units over the carrier's centre with
//!   the carrier's velocity plus 3 units/s up and homes back. The carrier
//!   keeps the ball (and its points).
//! - Handicap (`00246310`, `00246000`): the carrier's engine (racer +0x178,
//!   the `[Engine]` block of `001e3b80`) gets effect +0x68 = -0.34 with
//!   +0x70 = TopSpeed + the garage speed adjustment (+0x60 + +0xa0). The
//!   engine update `001f5b40` adds effect x +0x70 to both ends of its speed
//!   taper, so the carrier's top speed is cut by 34 %; the drop clears both.
//! - Boundary (`0020aec0`): outside `margin` terrain cells of the
//!   heightfield's edge (110 cells; 0 when world +0x658 is set). The
//!   throw-in `00246ec0` sets the ball's velocity to the horizontal unit
//!   vector toward the heightfield's centre (tiles x 128 cells) times
//!   170.4714 plus 112.152245 up; the position is kept.
//! - End `00247160`: riders are sorted by score, highest first
//!   (`002fa120`); each run of equal scores is then sorted by rider slot
//!   (`002fa200`), and places are numbered from 1; the TagOver menu shows
//!   the result.
//!
//! The time limit is Games Prep's (`0026b7a0` writes minutes x 60 to
//! settings +0x57c; `0020f780` copies it to the event clock for Tag), 2-20
//! minutes, 5 by default (`crate::games`).

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
/// `00244ef0`: a trick start knocks the ball loose when a uniform draw is
/// above this (a 25 % chance).
pub const KNOCK_LOOSE_DRAW: f32 = 0.75;
/// `00244ef0`: the knocked ball's lift over the carrier and upward speed.
pub const KNOCK_HEIGHT: f32 = 2.0;
pub const KNOCK_UP: f32 = 3.0;
/// `00246310`: the carrier's engine effect (+0x68).
pub const CARRIER_EFFECT: f32 = -0.34;

/// The carrier's engine modifiers (`00246310`): effect -0.34 scaled by its
/// top speed (TopSpeed + the garage speed adjustment, feet/second), which
/// lowers both ends of the engine's speed taper by 34 % of the top speed.
/// Horsepower's effect term (+0x6c) is left as it was.
pub fn carrier_engine(
    mut modifiers: crate::drivetrain::EngineModifiers,
    top_speed: f32,
) -> crate::drivetrain::EngineModifiers {
    modifiers.effect = CARRIER_EFFECT;
    modifiers.effect_speed = top_speed + modifiers.speed_adjustment;
    modifiers
}

/// `0020aec0`: the heightfield's extent (terrain +0x520/+0x524 tiles of
/// 256 cells, +0x528/+0x52c cell sizes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub tiles: [f32; 2],
    pub cell: [f32; 2],
}

impl Bounds {
    /// Outside the playing area: within `margin` cells of an edge or past it.
    pub fn outside(&self, p: [f32; 3], margin: f32) -> bool {
        let x =
            p[0] < margin * self.cell[0] || p[0] > self.cell[0] * (self.tiles[0] * 256. - margin);
        let z =
            p[2] < margin * self.cell[1] || p[2] > self.cell[1] * (self.tiles[1] * 256. - margin);
        x || z
    }

    /// The port's heightfield (`Terrain::width_cells`/`depth_cells`, read
    /// as tiles x 256 cells).
    pub fn from_cells(width_cells: u32, depth_cells: u32, spacing: f32) -> Self {
        Self {
            tiles: [width_cells as f32 / 256., depth_cells as f32 / 256.],
            cell: [spacing, spacing],
        }
    }

    /// `00246ec0`'s target: the heightfield's centre.
    pub fn centre(&self) -> [f32; 3] {
        [
            self.tiles[0] * 128. * self.cell[0],
            0.,
            self.tiles[1] * 128. * self.cell[1],
        ]
    }
}

/// `0020aec0`'s margin in cells for Tag (`00244ef0`: 110, or 0 when world
/// +0x658 is set).
pub const BOUNDARY_CELLS: f32 = BOUNDARY_MARGIN;

/// Tag's boundary margin for a venue: world +0x658 is the scene's
/// `[Sounds] CrowdPresent` (`00209680` reads the key into it, `00209460`
/// defaults it to 0), so stadium venues test the ball only against the
/// heightfield's edge (`00244ef0`).
pub fn boundary_margin(crowd_present: bool) -> f32 {
    if crowd_present { 0. } else { BOUNDARY_CELLS }
}

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
    /// A trick began this frame (`00244ef0`: the stunt record's open-event
    /// flag with the main stunt clock within one frame).
    pub stunt_started: bool,
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
    /// A trick knocked the carried ball loose (`00244ef0`); the rider keeps
    /// it and the ball homes back.
    KnockLoose { rider: usize },
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
    /// The knock-loose draw's generator state.
    pub seed: u32,
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
            seed: 0x2545_f491,
        }
    }

    /// Games Prep's minutes (`0026b7a0`: minutes x 60).
    pub fn with_minutes(riders: usize, minutes: u32) -> Self {
        Self::new(riders, minutes as f32 * 60.)
    }

    pub fn restart(&mut self) {
        let seed = self.seed;
        *self = Self::new(self.records.len(), self.time_limit);
        self.seed = seed;
    }

    /// A uniform draw in [0, 1) (xorshift; the original's shared generator
    /// `0036e7a0` and its call order are not reproduced).
    pub fn draw(&mut self) -> f32 {
        let mut x = self.seed.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
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
            let rider = riders.get(c).copied().unwrap_or_default();
            if rider.wrecked {
                self.drop_ball(DropReason::Wreck, &mut events);
            } else if self.attached && rider.stunt_started && self.draw() > KNOCK_LOOSE_DRAW {
                // `00244ef0`: knocked off the ATV; the rider keeps the ball.
                self.attached = false;
                events.push(Event::KnockLoose { rider: c });
            }
            // A wreck's drop reset the current possession, so nobody takes
            // the ball this frame either.
            if self.records[c].current < IMMUNITY {
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

    /// `00247160`: riders by place: highest score first (`002fa120`), then
    /// each run of equal scores by rider slot (`002fa200`).
    pub fn standings(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.records.len()).collect();
        order.sort_by(|a, b| {
            self.records[*b]
                .score
                .cmp(&self.records[*a].score)
                .then(a.cmp(b))
        });
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

    /// `00246ec0`: thrown back toward `centre` from where it is, 170.4714
    /// units/s horizontally and 112.152245 up (the original's values; the
    /// position is kept).
    pub fn throw_in(from: [f32; 3], centre: [f32; 3]) -> Self {
        let (dx, dz) = (centre[0] - from[0], centre[2] - from[2]);
        let len = dx.hypot_d(dz);
        let (ux, uz) = if len > 0. {
            (dx / len, dz / len)
        } else {
            (0., 0.)
        };
        Self {
            position: from,
            velocity: [ux * THROW_IN_SPEED, THROW_IN_UP, uz * THROW_IN_SPEED],
        }
    }

    /// A shorter throw toward `centre` that lands near it (the port's
    /// earlier authored scaling, kept for venues without a heightfield
    /// extent).
    pub fn lob(from: [f32; 3], centre: [f32; 3]) -> Self {
        let (dx, dz) = (centre[0] - from[0], centre[2] - from[2]);
        let len = dx.hypot_d(dz).max(1e-3);
        let flight = 2. * THROW_IN_UP / GRAVITY;
        let speed = (len / flight).min(THROW_IN_SPEED);
        Self {
            position: from,
            velocity: [dx / len * speed, THROW_IN_UP, dz / len * speed],
        }
    }

    /// `00244ef0`: knocked loose off the carrier: 2 units over its centre
    /// with its velocity plus 3 units/s up.
    pub fn knock(carrier: [f32; 3], velocity: [f32; 3]) -> Self {
        Self {
            position: [carrier[0], carrier[1] + KNOCK_HEIGHT, carrier[2]],
            velocity: [velocity[0], velocity[1] + KNOCK_UP, velocity[2]],
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

    /// `00246500`, after the search: thrown at the nearest world hit
    /// `hit` from `from`. The direction is the unit vector to the hit plus
    /// the up axis, halved (about 45 degrees up for a level hit); each axis
    /// is scaled by its own speed `d + max(40, d * draw)`, `d` the hit
    /// distance and `draws` three uniform draws in [0, 1).
    pub fn respawn_throw(from: [f32; 3], hit: [f32; 3], draws: [f32; 3]) -> Self {
        let d = [hit[0] - from[0], hit[1] - from[1], hit[2] - from[2]];
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let unit = if dist > 0. {
            [d[0] / dist, d[1] / dist, d[2] / dist]
        } else {
            [0.; 3]
        };
        let half = [unit[0] * 0.5, (unit[1] + 1.) * 0.5, unit[2] * 0.5];
        let speed = |r: f32| dist + (dist * r).max(RESPAWN_MIN_SPEED);
        Self {
            position: from,
            velocity: [
                half[0] * speed(draws[0]),
                half[1] * speed(draws[1]),
                half[2] * speed(draws[2]),
            ],
        }
    }

    /// `00246500` with no hit within range: back to the event's stored
    /// start, 10 units over the ground there, at rest.
    pub fn respawn_start(start: [f32; 3], ground: f32) -> Self {
        Self {
            position: [start[0], ground + RESPAWN_START_LIFT, start[2]],
            velocity: [0.; 3],
        }
    }

    /// Lying within a unit of the ground (`00246da0`).
    pub fn resting(&self, ground: impl Fn(f32, f32) -> f32) -> bool {
        self.position[1] - BALL_RADIUS <= ground(self.position[0], self.position[2]) + 1.
    }
}

/// `00246500`'s rays: eighteen, 20 degrees apart from `heading` (degrees),
/// each `RESPAWN_RANGE` units out level with the ball.
pub const RESPAWN_RAYS: usize = 18;
pub const RESPAWN_STEP: f32 = 20.0;
pub const RESPAWN_RANGE: f32 = 300.0;
/// A hit nearer than this ends the search early.
pub const RESPAWN_NEAR: f32 = 100.0;
/// The smallest random speed term of the throw.
pub const RESPAWN_MIN_SPEED: f32 = 40.0;
/// The fallback start's lift over the ground.
pub const RESPAWN_START_LIFT: f32 = 10.0;

/// `00246500`'s search: casts the rays from `from` with `cast(from, to)`
/// (the nearest world hit along the segment) and returns the nearest hit
/// under `RESPAWN_RANGE`, stopping at the first one under `RESPAWN_NEAR`.
pub fn respawn_search(
    from: [f32; 3],
    heading: f32,
    mut cast: impl FnMut([f32; 3], [f32; 3]) -> Option<[f32; 3]>,
) -> Option<[f32; 3]> {
    // The search starts one unit past the range (301), as the original.
    let mut best = RESPAWN_RANGE + 1.;
    let mut found = None;
    for i in 0..RESPAWN_RAYS {
        let a = (heading + RESPAWN_STEP * i as f32).to_radians();
        let to = [
            from[0] + a.sin_d() * RESPAWN_RANGE,
            from[1],
            from[2] + a.cos_d() * RESPAWN_RANGE,
        ];
        if let Some(hit) = cast(from, to) {
            let d = [hit[0] - from[0], hit[1] - from[1], hit[2] - from[2]];
            let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if dist < best {
                best = dist;
                found = Some(hit);
                if dist < RESPAWN_NEAR {
                    break;
                }
            }
        }
    }
    found.filter(|_| best < RESPAWN_RANGE)
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
    fn a_trick_on_a_carried_ball_knocks_it_loose_a_quarter_of_the_time() {
        let mut knocked = 0;
        let trials = 4000;
        for seed in 0..trials {
            let mut g = Game::new(2, 120.);
            g.seed = 0x9e37_79b9u32.wrapping_mul(seed + 1);
            let mut r = riders(2);
            r[0].touches_ball = true;
            g.tick(DT, &r, false);
            r[0].touches_ball = false;
            // Not yet on the ATV: a trick does nothing.
            r[0].stunt_started = true;
            assert!(
                !g.tick(DT, &r, false)
                    .iter()
                    .any(|e| matches!(e, Event::KnockLoose { .. }))
            );
            g.catch();
            let events = g.tick(DT, &r, false);
            if events.contains(&Event::KnockLoose { rider: 0 }) {
                knocked += 1;
                assert!(!g.attached);
                // The rider keeps the ball and its points.
                assert_eq!(g.carrier, Some(0));
            } else {
                assert!(g.attached);
            }
        }
        let share = knocked as f32 / trials as f32;
        assert!((0.22..0.28).contains(&share), "{share}");
    }

    #[test]
    fn knocked_ball_rises_two_units_and_homes_back() {
        let carrier = [5., 10., 5.];
        let b = Ball::knock(carrier, [4., 0., 0.]);
        assert_eq!(b.position, [5., 12., 5.]);
        assert_eq!(b.velocity, [4., 3., 0.]);
    }

    #[test]
    fn carrier_engine_handicap_cuts_top_speed_by_a_third() {
        let m = carrier_engine(
            crate::drivetrain::EngineModifiers {
                speed_adjustment: 2.,
                ..Default::default()
            },
            80.,
        );
        assert_eq!(m.effect, -0.34);
        assert_eq!(m.effect_speed, 82.);
        // 001f5b40's taper end: TopSpeed + adjustment + effect x +0x70.
        let end = 80. + m.speed_adjustment + m.effect * m.effect_speed;
        assert!((end - 82. * 0.66).abs() < 1e-4);
    }

    #[test]
    fn boundary_uses_cells_and_throw_in_heads_for_the_centre_at_full_speed() {
        // Two 256-cell tiles of 8 ft each way: 4096 ft square.
        let b = Bounds {
            tiles: [2., 2.],
            cell: [8., 8.],
        };
        assert_eq!(b.centre(), [2048., 0., 2048.]);
        assert!(!b.outside([2048., 0., 2048.], BOUNDARY_CELLS));
        assert!(b.outside([879., 0., 2048.], BOUNDARY_CELLS));
        assert!(!b.outside([881., 0., 2048.], BOUNDARY_CELLS));
        assert!(b.outside([2048., 0., 4096. - 879.], BOUNDARY_CELLS));
        // A CrowdPresent stadium tests only the heightfield's edge.
        assert!(!b.outside([1., 0., 2048.], boundary_margin(true)));
        assert!(b.outside([-1., 0., 2048.], boundary_margin(true)));
        assert_eq!(boundary_margin(false), 110.);
        assert!(!b.outside([10., 0., 10.], 0.));
        let t = Ball::throw_in([4000., 50., 2048.], b.centre());
        assert_eq!(t.position, [4000., 50., 2048.]);
        assert!((t.velocity[0] + THROW_IN_SPEED).abs() < 1e-3);
        assert_eq!(t.velocity[1], THROW_IN_UP);
        assert!(t.velocity[2].abs() < 1e-3);
    }

    #[test]
    fn equal_scores_rank_by_rider_slot() {
        let mut g = Game::new(4, 60.);
        g.records[0].score = 500;
        g.records[1].score = 750;
        g.records[2].score = 750;
        g.records[3].score = 500;
        assert_eq!(g.standings(), vec![1, 2, 0, 3]);
    }

    #[test]
    fn games_prep_minutes_set_the_clock() {
        let g = Game::with_minutes(2, crate::games::DEFAULT_MINUTES);
        assert_eq!(g.time_limit, 300.);
        assert_eq!(g.clock, 300.);
        assert_eq!(Game::with_minutes(2, 20).time_limit, 1200.);
    }

    #[test]
    fn a_wreck_drop_blocks_takes_that_frame() {
        let mut g = Game::new(2, 120.);
        let mut r = riders(2);
        r[0].touches_ball = true;
        g.tick(DT, &r, false);
        r[0].touches_ball = false;
        run(&mut g, &r, 2.5);
        r[0].wrecked = true;
        r[1].touches_ball = true;
        let events = g.tick(DT, &r, false);
        assert!(!events.iter().any(|e| matches!(e, Event::Gain { .. })));
        assert!(matches!(
            g.tick(DT, &r, false)[..],
            [Event::Gain { rider: 1 }]
        ));
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
        let t = Ball::lob([500., 0., 0.], [0., 0., 0.]);
        assert!(t.velocity[0] < 0. && t.velocity[1] > 100.);
    }
    #[test]
    fn respawn_search_takes_the_nearest_hit_and_stops_under_100() {
        // A wall 150 units east and one 250 north: the nearer wins (the
        // ray at 10 + 4 x 20 = 90 degrees points east).
        let wall = |from: [f32; 3], to: [f32; 3]| {
            let mut hit = None;
            for k in 1..=300 {
                let t = k as f32 / 300.;
                let p = [
                    from[0] + (to[0] - from[0]) * t,
                    from[1],
                    from[2] + (to[2] - from[2]) * t,
                ];
                if p[0] >= 150. || p[2] >= 250. {
                    hit = Some(p);
                    break;
                }
            }
            hit
        };
        let hit = respawn_search([0.; 3], 10., wall).unwrap();
        assert!((hit[0] - 150.).abs() < 2. && hit[2].abs() < 2., "{hit:?}");
        // Under 100 units the first such hit ends the search.
        let mut casts = 0;
        let near = respawn_search([0.; 3], 0., |_, to| {
            casts += 1;
            Some([to[0] / 10., 0., to[2] / 10.])
        });
        assert_eq!(casts, 1);
        assert!(near.is_some());
        // Nothing within range: no throw (the caller restarts the ball).
        assert!(respawn_search([0.; 3], 0., |_, _| None).is_none());
        assert!(respawn_search([0.; 3], 0., |_, to| Some(to)).is_none());
    }

    #[test]
    fn respawn_throw_aims_up_toward_the_hit() {
        let b = Ball::respawn_throw([0., 0., 0.], [200., 0., 0.], [0., 0.5, 1.]);
        // x: (1 + 0) / 2 x (200 + max(40, 0)); y: (0 + 1) / 2 x (200 + 100);
        // z: 0.
        assert_eq!(b.velocity, [120., 150., 0.]);
        let s = Ball::respawn_start([5., 0., 7.], 30.);
        assert_eq!((s.position, s.velocity), ([5., 40., 7.], [0.; 3]));
    }
}
