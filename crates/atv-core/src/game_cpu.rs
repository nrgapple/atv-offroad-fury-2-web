//! **Port adaptation, not recovered behavior.** Authored CPU riders for the
//! mini-games (Tag, Treasure Hunt, King of the Hills, Hockey): the original
//! events are split screen only and have no CPU riders (research/GAMES.md
//! "CPU riders"). Off by default; Games Prep's "CPU Riders" row adds them.
//!
//! A CPU rider is a whole player ride driven through the player's own
//! controls, so it scores through the same rules as a human. This module
//! is the pure part: each mode picks a [`Goal`] from the game's state and
//! [`Driver`] turns it into throttle, brake and steering, with the race
//! CPUs' stuck rule (under 3 ft/s while riding) escalating from backing out
//! to a reset.
//!
//! Deterministic: fixed-step f32 arithmetic through [`Det`] and a seeded
//! [`Rng`]; no clock and no allocation per step.
use crate::math::Det;

/// The most CPU riders a game adds (four riders with the player).
pub const MAX_CPUS: u8 = 3;

/// A rider slower than this while riding is stuck (the race CPUs'
/// `stuck_clock` threshold, ft/s).
pub const STUCK_SPEED: f32 = 3.;
/// Seconds stuck before the rider backs out.
pub const STUCK_SECONDS: f32 = 1.2;
/// Seconds a back-out lasts.
pub const BACK_OUT_SECONDS: f32 = 1.3;
/// Seconds of free riding after which earlier back-outs are forgotten.
pub const FREE_SECONDS: f32 = 6.;

/// Xorshift32: each CPU's seeded random stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        // Zero is xorshift's fixed point.
        Self(seed.wrapping_mul(0x9e37_79b9) | 1)
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// A value in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// A stable value in [0, 1) for `(seed, index)`: a CPU's lasting taste for
/// one target, so its choice does not flicker between steps.
pub fn taste(seed: u32, index: usize) -> f32 {
    Rng::new(seed ^ (index as u32).wrapping_mul(0x85eb_ca6b).wrapping_add(0x1234_5677)).unit()
}

/// How well a CPU rides, from the race difficulty setting (0 Kid, 1 Normal,
/// 2 Expert).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    /// The speed it rides at (ft/s).
    pub cruise: f32,
    /// How much of a moving target's travel it leads (0-1).
    pub lead: f32,
    /// Its aim's wander (ft).
    pub wander: f32,
    /// King of the Hills: its stunt chains' points (low, high).
    pub stunt: (f32, f32),
}

impl Skill {
    pub fn for_difficulty(difficulty: u32) -> Self {
        match difficulty {
            0 => Self {
                cruise: 34.,
                lead: 0.,
                wander: 10.,
                stunt: (150., 700.),
            },
            1 => Self {
                cruise: 48.,
                lead: 0.5,
                wander: 5.,
                stunt: (300., 1600.),
            },
            _ => Self {
                cruise: 62.,
                lead: 0.9,
                wander: 2.,
                stunt: (600., 3200.),
            },
        }
    }
}

/// What the driver reads of its own ride each step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sense {
    pub position: [f32; 3],
    /// The ATV's forward axis on the ground plane (x, z); any length.
    pub forward: [f32; 2],
    /// Signed forward speed (ft/s).
    pub speed: f32,
    /// Any wheel down.
    pub grounded: bool,
    /// Riding (not wrecked or recovering).
    pub riding: bool,
}

/// Where to ride and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Goal {
    /// Ground-plane target (x, z).
    pub target: [f32; 2],
    /// The most speed wanted on the way (ft/s).
    pub speed: f32,
    /// Stop at the target instead of riding through it.
    pub stop: bool,
    /// Hold the preload now, to hop when it is let go.
    pub crouch: bool,
}

impl Goal {
    pub fn ride(target: [f32; 2], speed: f32) -> Self {
        Self {
            target,
            speed,
            stop: false,
            crouch: false,
        }
    }
}

/// What a stuck rider needs beyond its controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Recover {
    #[default]
    None,
    /// Set the ATV down where it stands, facing away from the obstacle.
    Turn,
    /// The player's Reset (back to its start).
    Reset,
}

/// One step's controls.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Drive {
    /// Forward (0-1) or, negative, brake-then-reverse.
    pub axis: f32,
    pub brake: f32,
    pub steer: f32,
    /// The preload: 1 held, -1 the step it is let go.
    pub preload: Option<f32>,
    pub recover: Recover,
}

/// Turns a [`Goal`] into controls and gets the rider unstuck.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Driver {
    stuck: f32,
    backing: f32,
    /// Which way the back-out steers.
    back_steer: f32,
    /// Back-outs since the rider last rode free.
    pub escapes: u32,
    free: f32,
    crouched: bool,
    /// Resets asked for so far (diagnostics).
    pub resets: u32,
}

fn unit2(v: [f32; 2]) -> Option<[f32; 2]> {
    let len = v[0].hypot_d(v[1]);
    (len > 1e-4).then(|| [v[0] / len, v[1] / len])
}

/// The signed angle (radians, positive to the right) from `forward` to the
/// direction of `to` from `from`, as the race driver measures it.
pub fn bearing(forward: [f32; 2], from: [f32; 3], to: [f32; 2]) -> f32 {
    let (Some(f), Some(d)) = (unit2(forward), unit2([to[0] - from[0], to[1] - from[2]])) else {
        return 0.;
    };
    let right = [f[1], -f[0]];
    (d[0] * right[0] + d[1] * right[1]).atan2_d(d[0] * f[0] + d[1] * f[1])
}

fn flat(a: [f32; 3], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot_d(a[2] - b[1])
}

impl Driver {
    /// One fixed step of `dt` seconds. No goal (or a held rider) brakes.
    pub fn step(&mut self, sense: &Sense, goal: Option<&Goal>, dt: f32) -> Drive {
        let Some(goal) = goal.filter(|_| sense.riding) else {
            self.stuck = 0.;
            self.backing = 0.;
            let released = std::mem::take(&mut self.crouched);
            return Drive {
                brake: if sense.riding { 1. } else { 0. },
                preload: released.then_some(-1.),
                ..Drive::default()
            };
        };
        let distance = flat(sense.position, goal.target);
        let angle = bearing(sense.forward, sense.position, goal.target);
        let steer = (3. * angle)
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
            .sin_d();
        let arrived = goal.stop && distance < 10.;
        // The stuck clock: wanting to move, on the ground, and not moving.
        if self.backing > 0. {
            self.backing -= dt;
            self.stuck = 0.;
            return Drive {
                axis: -1.,
                steer: self.back_steer,
                ..Drive::default()
            };
        }
        if sense.grounded && !arrived && sense.speed.abs() < STUCK_SPEED {
            self.stuck += dt;
            self.free = 0.;
        } else {
            self.stuck = 0.;
            self.free += dt;
            if self.free > FREE_SECONDS {
                self.escapes = 0;
            }
        }
        let mut recover = Recover::None;
        if self.stuck > STUCK_SECONDS {
            self.stuck = 0.;
            self.escapes += 1;
            match self.escapes {
                1 | 2 => {
                    self.backing = BACK_OUT_SECONDS;
                    // Back away turning the tail toward the target's side,
                    // the other way the second time.
                    let side = if steer < 0. { 1. } else { -1. };
                    self.back_steer = if self.escapes == 1 { side } else { -side };
                }
                3 => recover = Recover::Turn,
                _ => {
                    recover = Recover::Reset;
                    self.escapes = 0;
                    self.resets += 1;
                }
            }
        }
        // Slow for a turn-around and, when stopping, for the arrival.
        let turn = angle.abs();
        let mut wanted = goal.speed;
        if turn > 2. {
            wanted = wanted.min(14.);
        } else if turn > 0.9 {
            wanted = wanted.min(24.);
        }
        if goal.stop {
            // Keep rolling (above the stuck speed) until inside the stop.
            wanted = wanted.min((distance - 8.).max(0.) * 0.9 + 5.);
        }
        let (axis, brake) = if arrived {
            (0., 1.)
        } else if sense.speed < wanted - 1. {
            (1., 0.)
        } else if sense.speed < wanted + 6. {
            (0.25, 0.)
        } else {
            (0., ((sense.speed - wanted) / 20.).clamp(0.2, 1.))
        };
        let preload = if goal.crouch && sense.grounded {
            self.crouched = true;
            Some(1.)
        } else {
            std::mem::take(&mut self.crouched).then_some(-1.)
        };
        Drive {
            axis,
            brake,
            steer: if sense.grounded { steer } else { 0. },
            preload,
            recover,
        }
    }
}

/// Another rider as a brain sees it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Other {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
}

/// Tag (ball tag: the carrier scores while it holds the ball).
#[derive(Clone, Copy, Debug)]
pub struct TagWorld<'a> {
    /// Every rider in slot order.
    pub riders: &'a [Other],
    pub carrier: Option<usize>,
    pub ball: [f32; 3],
    pub ball_velocity: [f32; 3],
    /// The playing area's corners on the ground plane.
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// The carrier keeps away from the others, preferring open ground; the
/// others chase the carrier (leading it) or the loose ball.
pub fn tag_goal(me: usize, world: &TagWorld, skill: &Skill) -> Option<Goal> {
    let own = world.riders.get(me)?;
    let p = own.position;
    let Some(carrier) = world.carrier else {
        let lead = 0.3 * skill.lead;
        return Some(Goal::ride(
            [
                world.ball[0] + world.ball_velocity[0] * lead,
                world.ball[2] + world.ball_velocity[2] * lead,
            ],
            skill.cruise,
        ));
    };
    if carrier != me {
        let them = world.riders.get(carrier)?;
        let distance = flat(p, [them.position[0], them.position[2]]);
        let time = (distance / skill.cruise.max(1.)).min(1.5) * skill.lead;
        return Some(Goal::ride(
            [
                them.position[0] + them.velocity[0] * time,
                them.position[2] + them.velocity[2] * time,
            ],
            skill.cruise,
        ));
    }
    // Flee: away from each chaser, weighted by how near it is, and back
    // toward the middle as an edge comes up.
    let mut away = [0f32; 2];
    for (index, other) in world.riders.iter().enumerate() {
        if index == me {
            continue;
        }
        let d = [p[0] - other.position[0], p[2] - other.position[2]];
        let distance = d[0].hypot_d(d[1]).max(1.);
        let weight = 1. / (distance * distance);
        away[0] += d[0] / distance * weight;
        away[1] += d[1] / distance * weight;
    }
    let mut heading = unit2(away).unwrap_or([0., 1.]);
    let centre = [
        (world.min[0] + world.max[0]) * 0.5,
        (world.min[1] + world.max[1]) * 0.5,
    ];
    let half = [
        ((world.max[0] - world.min[0]) * 0.5).max(1.),
        ((world.max[1] - world.min[1]) * 0.5).max(1.),
    ];
    // 0 in the middle, 1 at an edge.
    let edge = ((p[0] - centre[0]).abs() / half[0]).max((p[2] - centre[1]).abs() / half[1]);
    let pull = ((edge - 0.55) / 0.35).clamp(0., 1.);
    if let Some(inward) = unit2([centre[0] - p[0], centre[1] - p[2]]) {
        heading = unit2([
            heading[0] * (1. - pull) + inward[0] * pull * 1.5,
            heading[1] * (1. - pull) + inward[1] * pull * 1.5,
        ])
        .unwrap_or(inward);
    }
    Some(Goal::ride(
        [p[0] + heading[0] * 80., p[2] + heading[1] * 80.],
        skill.cruise,
    ))
}

/// Treasure Hunt: the standing treasure that is cheapest for this CPU: near
/// it, not nearer a rival, and to its own taste (so CPUs spread out).
pub fn treasure_goal(
    position: [f32; 3],
    treasures: &[[f32; 3]],
    rivals: &[[f32; 3]],
    seed: u32,
    skill: &Skill,
) -> Option<Goal> {
    let mut best: Option<(f32, [f32; 3])> = None;
    for (index, t) in treasures.iter().enumerate() {
        let at = [t[0], t[2]];
        let mine = flat(position, at);
        let rival = rivals
            .iter()
            .map(|r| flat(*r, at))
            .fold(f32::MAX, f32::min);
        let cost = mine * (1. + 0.5 * taste(seed, index)) + (mine - rival).max(0.) * 0.8;
        if best.is_none_or(|(c, _)| cost < c) {
            best = Some((cost, *t));
        }
    }
    best.map(|(_, t)| Goal::ride([t[0], t[2]], skill.cruise))
}

/// A King of the Hills hill as a brain sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hill {
    /// The trigger box's centre and its ground-plane half extents.
    pub centre: [f32; 3],
    pub half: [f32; 2],
    pub owner: Option<usize>,
    pub score: f32,
}

/// A CPU's King of the Hills run: ride onto a hill it does not hold, off
/// its far side with a hop, and land a stunt chain there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HillRun {
    /// The hill being run and the ground-plane direction across it.
    hill: Option<(usize, [f32; 2])>,
    /// It has been on the hill with a wheel down.
    on: bool,
    /// Seconds in the air since leaving the ground on this run.
    air: f32,
    /// Seconds on this run (a run that takes too long picks again).
    clock: f32,
}

/// The longest a hill run lasts before the CPU picks again (seconds).
pub const HILL_RUN_SECONDS: f32 = 40.;
/// Air time that makes a landing a stunt (seconds).
pub const HILL_AIR: f32 = 0.35;

impl HillRun {
    fn inside(hill: &Hill, p: [f32; 3], margin: f32) -> bool {
        (p[0] - hill.centre[0]).abs() <= hill.half[0] + margin
            && (p[2] - hill.centre[2]).abs() <= hill.half[1] + margin
    }

    /// The hill to run next: one this CPU does not hold, nearer and with a
    /// lower score to beat first, to its own taste.
    pub fn pick(me: usize, position: [f32; 3], hills: &[Hill], seed: u32) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (index, hill) in hills.iter().enumerate() {
            if hill.owner == Some(me) {
                continue;
            }
            let distance = flat(position, [hill.centre[0], hill.centre[2]]);
            let cost = distance * (1. + 0.6 * taste(seed, index)) + hill.score * 0.05;
            if best.is_none_or(|(c, _)| cost < c) {
                best = Some((cost, index));
            }
        }
        best.map(|(_, index)| index)
    }

    /// One step. Returns the goal and, on the step a stunt lands, the
    /// chain's points (the rules decide whether they capture the hill).
    pub fn step(
        &mut self,
        me: usize,
        sense: &Sense,
        hills: &[Hill],
        seed: u32,
        skill: &Skill,
        rng: &mut Rng,
        dt: f32,
    ) -> (Option<Goal>, Option<f32>) {
        if !sense.riding {
            // A wreck ends the run (the rules clear the armed hill too).
            *self = Self::default();
            return (None, None);
        }
        self.clock += dt;
        let stale = self.hill.is_none_or(|(index, _)| {
            hills.get(index).is_none_or(|h| h.owner == Some(me)) && !self.on
        });
        if stale || self.clock > HILL_RUN_SECONDS {
            *self = Self::default();
            let index = Self::pick(me, sense.position, hills, seed ^ rng.next() % 3);
            self.hill = index.map(|index| {
                let c = hills[index].centre;
                let across =
                    unit2([c[0] - sense.position[0], c[2] - sense.position[2]]).unwrap_or([0., 1.]);
                (index, across)
            });
        }
        let Some((index, across)) = self.hill else {
            return (None, None);
        };
        let hill = &hills[index];
        let inside = Self::inside(hill, sense.position, 0.);
        if inside && sense.grounded {
            self.on = true;
        }
        let mut landed = None;
        if self.on {
            if sense.grounded {
                if self.air >= HILL_AIR && !inside {
                    let (low, high) = skill.stunt;
                    landed = Some((low + (high - low) * rng.unit()).round());
                }
                if !inside {
                    // Off the hill: this run is over either way.
                    let points = landed;
                    *self = Self::default();
                    return (None, points);
                }
                self.air = 0.;
            } else {
                self.air += dt;
            }
        }
        // Across the hill and well past its far edge.
        let reach = hill.half[0].max(hill.half[1]) + 60.;
        let target = [
            hill.centre[0] + across[0] * reach,
            hill.centre[2] + across[1] * reach,
        ];
        // Crouch on the hill and let go just before its far edge, so the
        // ATV leaves the box off the ground.
        let ahead = [
            sense.position[0] + across[0] * sense.speed.max(10.) * 0.45,
            sense.position[1],
            sense.position[2] + across[1] * sense.speed.max(10.) * 0.45,
        ];
        let crouch = self.on && inside && Self::inside(hill, ahead, 0.);
        (
            Some(Goal {
                target,
                speed: skill.cruise.max(40.),
                stop: false,
                crouch,
            }),
            landed,
        )
    }
}

/// Hockey: the rink as a brain needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rink {
    /// The goal this CPU's team scores in and the one it defends (x, z).
    pub attack: [f32; 2],
    pub defend: [f32; 2],
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// The attacker gets behind the puck on the line to the goal it attacks and
/// pushes through; a defender (a team's second CPU) waits between the puck
/// and its own goal and clears a puck that comes close.
pub fn hockey_goal(
    position: [f32; 3],
    rink: &Rink,
    puck: [f32; 3],
    puck_velocity: [f32; 3],
    defender: bool,
    skill: &Skill,
) -> Goal {
    let clamp = |p: [f32; 2]| {
        [
            p[0].clamp(rink.min[0] + 3., rink.max[0] - 3.),
            p[1].clamp(rink.min[1] + 3., rink.max[1] - 3.),
        ]
    };
    let lead = 0.4 * skill.lead;
    let ball = [
        puck[0] + puck_velocity[0] * lead,
        puck[2] + puck_velocity[2] * lead,
    ];
    // The rink is short: full race pace only bounces off the boards.
    let pace = skill.cruise.min(44.);
    if defender {
        let threat = (ball[0] - rink.defend[0]).hypot_d(ball[1] - rink.defend[1]);
        if threat > 38. {
            // A third of the way out from its goal toward the puck.
            let post = clamp([
                rink.defend[0] + (ball[0] - rink.defend[0]) * 0.33,
                rink.defend[1] + (ball[1] - rink.defend[1]) * 0.33,
            ]);
            return Goal {
                target: post,
                speed: pace * 0.8,
                stop: true,
                crouch: false,
            };
        }
    }
    let to_goal = unit2([rink.attack[0] - ball[0], rink.attack[1] - ball[1]]).unwrap_or([0., 1.]);
    let behind = [ball[0] - to_goal[0] * 9., ball[1] - to_goal[1] * 9.];
    let offset = [position[0] - ball[0], position[2] - ball[1]];
    // In front of the puck (goal side): swing around to its back first.
    let lined_up = offset[0] * to_goal[0] + offset[1] * to_goal[1] < -2.;
    let across = (offset[0] * to_goal[1] - offset[1] * to_goal[0]).abs();
    if lined_up && across < 5. {
        // Through the puck toward the goal.
        return Goal::ride(
            clamp([ball[0] + to_goal[0] * 14., ball[1] + to_goal[1] * 14.]),
            pace,
        );
    }
    if !lined_up {
        // Round the puck on the side the rider is already on.
        let side = if offset[0] * to_goal[1] - offset[1] * to_goal[0] >= 0. {
            1.
        } else {
            -1.
        };
        let wide = [
            behind[0] + to_goal[1] * side * 8.,
            behind[1] - to_goal[0] * side * 8.,
        ];
        return Goal::ride(clamp(wide), pace * 0.85);
    }
    Goal::ride(clamp(behind), pace * 0.85)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1. / 60.;

    fn sense(position: [f32; 3], forward: [f32; 2], speed: f32) -> Sense {
        Sense {
            position,
            forward,
            speed,
            grounded: true,
            riding: true,
        }
    }

    /// A point mass that follows the drive: enough to check the control law.
    fn ride(driver: &mut Driver, s: &mut Sense, goal: &Goal, steps: usize) {
        for _ in 0..steps {
            let d = driver.step(s, Some(goal), DT);
            let before = s.speed;
            s.speed += (d.axis.max(0.) * 30. - d.brake * 40. - s.speed * 0.2) * DT;
            // Brakes stop the ATV; they do not reverse it.
            if d.axis >= 0. && before >= 0. {
                s.speed = s.speed.max(0.);
            }
            if d.axis < 0. {
                s.speed = (s.speed - 20. * DT).max(-10.);
            }
            let yaw = s.forward[0].atan2_d(s.forward[1]) + d.steer * 1.5 * DT * s.speed.signum();
            let (sin, cos) = yaw.sin_cos_d();
            s.forward = [sin, cos];
            s.position[0] += sin * s.speed * DT;
            s.position[2] += cos * s.speed * DT;
        }
    }

    #[test]
    fn rng_is_seeded_and_repeats() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut c = Rng::new(8);
        let va: Vec<u32> = (0..8).map(|_| a.next()).collect();
        let vb: Vec<u32> = (0..8).map(|_| b.next()).collect();
        let vc: Vec<u32> = (0..8).map(|_| c.next()).collect();
        assert_eq!(va, vb);
        assert_ne!(va, vc);
        let mut zero = Rng::new(0);
        assert!((0..100).all(|_| (0. ..1.).contains(&zero.unit())));
        assert_eq!(taste(3, 4), taste(3, 4));
        assert_ne!(taste(3, 4), taste(3, 5));
    }

    #[test]
    fn bearing_is_positive_to_the_right() {
        // Facing +Z, the race driver's right is +X.
        assert!(bearing([0., 1.], [0.; 3], [10., 10.]) > 0.);
        assert!(bearing([0., 1.], [0.; 3], [-10., 10.]) < 0.);
        assert_eq!(bearing([0., 1.], [0.; 3], [0., 10.]), 0.);
    }

    #[test]
    fn the_driver_turns_to_and_reaches_a_target_behind_it() {
        let mut driver = Driver::default();
        let mut s = sense([0.; 3], [0., 1.], 10.);
        let goal = Goal::ride([-40., -120.], 40.);
        let mut nearest = f32::MAX;
        for _ in 0..40 {
            ride(&mut driver, &mut s, &goal, 15);
            nearest = nearest.min(flat(s.position, goal.target));
        }
        assert!(nearest < 6., "{nearest}");
        assert_eq!(driver.resets, 0);
    }

    #[test]
    fn a_stopping_goal_brakes_at_the_target_without_reading_as_stuck() {
        let mut driver = Driver::default();
        let mut s = sense([0.; 3], [0., 1.], 0.);
        let goal = Goal {
            stop: true,
            ..Goal::ride([0., 60.], 40.)
        };
        ride(&mut driver, &mut s, &goal, 60 * 12);
        assert!(flat(s.position, goal.target) < 10., "{:?}", s.position);
        assert!(s.speed.abs() < 3.);
        assert_eq!(driver.escapes, 0);
    }

    #[test]
    fn a_stuck_rider_backs_out_twice_then_turns_then_resets() {
        let mut driver = Driver::default();
        let s = sense([0.; 3], [0., 1.], 0.);
        let goal = Goal::ride([5., 100.], 40.);
        let mut seen = Vec::new();
        let mut backing = 0;
        for _ in 0..(60 * 12) {
            let d = driver.step(&s, Some(&goal), DT);
            if d.axis < 0. {
                backing += 1;
            }
            if d.recover != Recover::None {
                seen.push(d.recover);
            }
        }
        assert!(backing > 60, "{backing}");
        assert_eq!(seen, [Recover::Turn, Recover::Reset]);
        assert_eq!(driver.resets, 1);
        // A wrecked or held rider is never stuck.
        let mut driver = Driver::default();
        let down = Sense { riding: false, ..s };
        for _ in 0..600 {
            assert_eq!(driver.step(&down, Some(&goal), DT), Drive::default());
            assert_eq!(driver.step(&s, None, DT).brake, 1.);
        }
        assert_eq!(driver.escapes, 0);
    }

    #[test]
    fn a_crouch_is_held_then_let_go_once() {
        let mut driver = Driver::default();
        let s = sense([0.; 3], [0., 1.], 30.);
        let mut goal = Goal::ride([0., 100.], 40.);
        goal.crouch = true;
        assert_eq!(driver.step(&s, Some(&goal), DT).preload, Some(1.));
        goal.crouch = false;
        assert_eq!(driver.step(&s, Some(&goal), DT).preload, Some(-1.));
        assert_eq!(driver.step(&s, Some(&goal), DT).preload, None);
    }

    fn tag_world(riders: &[Other], carrier: Option<usize>) -> TagWorld<'_> {
        TagWorld {
            riders,
            carrier,
            ball: [50., 0., 50.],
            ball_velocity: [0.; 3],
            min: [0., 0.],
            max: [1000., 1000.],
        }
    }

    fn at(x: f32, z: f32) -> Other {
        Other {
            position: [x, 0., z],
            velocity: [0.; 3],
        }
    }

    #[test]
    fn tag_chases_the_loose_ball_then_leads_the_carrier() {
        let skill = Skill::for_difficulty(2);
        let mut riders = [at(500., 500.), at(520., 500.)];
        let goal = tag_goal(1, &tag_world(&riders, None), &skill).unwrap();
        assert_eq!(goal.target, [50., 50.]);
        // The carrier rides +Z: the chaser aims ahead of it.
        riders[0].velocity = [0., 0., 40.];
        let goal = tag_goal(1, &tag_world(&riders, Some(0)), &skill).unwrap();
        assert!(goal.target[1] > 500. && goal.target[0] == 500.);
        // Kid CPUs do not lead.
        let kid = Skill::for_difficulty(0);
        let goal = tag_goal(1, &tag_world(&riders, Some(0)), &kid).unwrap();
        assert_eq!(goal.target, [500., 500.]);
    }

    #[test]
    fn the_tag_carrier_flees_the_chasers_and_turns_in_from_the_edge() {
        let skill = Skill::for_difficulty(1);
        // Chasers to the west and a little south: flee east/north.
        let riders = [at(500., 500.), at(460., 500.), at(480., 470.)];
        let goal = tag_goal(0, &tag_world(&riders, Some(0)), &skill).unwrap();
        assert!(goal.target[0] > 500. && goal.target[1] > 500.);
        // Chased toward the east edge, it bends back to the middle.
        let riders = [at(980., 500.), at(940., 500.)];
        let goal = tag_goal(0, &tag_world(&riders, Some(0)), &skill).unwrap();
        assert!(goal.target[0] < 980., "{:?}", goal.target);
    }

    #[test]
    fn treasure_cpus_take_the_near_one_and_leave_a_rivals() {
        let skill = Skill::for_difficulty(1);
        let treasures = [[100., 0., 0.], [-130., 0., 0.]];
        let goal = treasure_goal([0.; 3], &treasures, &[], 1, &skill).unwrap();
        assert_eq!(goal.target, [100., 0.]);
        // A rival sits on the near one: the other is the better trip.
        let goal = treasure_goal([0.; 3], &treasures, &[[95., 0., 0.]], 1, &skill).unwrap();
        assert_eq!(goal.target, [-130., 0.]);
        assert!(treasure_goal([0.; 3], &[], &[], 1, &skill).is_none());
        // Two CPUs at one spot with many equal treasures do not all agree.
        let ring: Vec<[f32; 3]> = (0..8)
            .map(|i| {
                let (s, c) = (i as f32 * 0.785).sin_cos_d();
                [s * 100., 0., c * 100.]
            })
            .collect();
        let picks: std::collections::BTreeSet<_> = (0..6u32)
            .map(|seed| {
                let g = treasure_goal([0.; 3], &ring, &[], seed, &skill).unwrap();
                (g.target[0] as i32, g.target[1] as i32)
            })
            .collect();
        assert!(picks.len() > 1);
    }

    fn hills() -> [Hill; 2] {
        [
            Hill {
                centre: [0., 10., 200.],
                half: [20., 20.],
                owner: None,
                score: 0.,
            },
            Hill {
                centre: [300., 10., 0.],
                half: [20., 20.],
                owner: None,
                score: 0.,
            },
        ]
    }

    #[test]
    fn a_hill_run_rides_on_hops_off_and_lands_a_stunt() {
        let skill = Skill::for_difficulty(1);
        let mut rng = Rng::new(5);
        let mut run = HillRun::default();
        let mut hills = hills();
        hills[1].owner = Some(0);
        let mut s = sense([0., 0., 0.], [0., 1.], 40.);
        // Approach: the goal lies across hill 0, no crouch, no stunt.
        let (goal, points) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        let goal = goal.unwrap();
        assert!(goal.target[1] > 220. && !goal.crouch && points.is_none());
        // On the hill: crouch until the far edge is near.
        s.position = [0., 10., 190.];
        let (goal, _) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        assert!(goal.unwrap().crouch);
        s.position = [0., 10., 215.];
        let (goal, _) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        assert!(!goal.unwrap().crouch);
        // In the air off the hill for half a second, then down: a stunt.
        s.grounded = false;
        s.position = [0., 14., 240.];
        for _ in 0..30 {
            let (_, points) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
            assert!(points.is_none());
        }
        s.grounded = true;
        let (_, points) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        let points = points.unwrap();
        assert!((skill.stunt.0..=skill.stunt.1).contains(&points), "{points}");
        // The next run starts over.
        assert_eq!(run, HillRun::default());
    }

    #[test]
    fn a_hill_run_without_air_scores_nothing_and_held_hills_are_skipped() {
        let skill = Skill::for_difficulty(1);
        let mut rng = Rng::new(5);
        let mut run = HillRun::default();
        let mut hills = hills();
        let mut s = sense([0., 10., 195.], [0., 1.], 40.);
        run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        s.position = [0., 4., 240.];
        let (_, points) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        assert!(points.is_none());
        // Holding every hill: nothing to run.
        hills[0].owner = Some(0);
        hills[1].owner = Some(0);
        assert!(HillRun::pick(0, s.position, &hills, 1).is_none());
        let (goal, _) = run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        assert!(goal.is_none());
        // A wreck ends the run.
        hills[0].owner = None;
        run.step(0, &s, &hills, 1, &skill, &mut rng, DT);
        s.riding = false;
        assert_eq!(run.step(0, &s, &hills, 1, &skill, &mut rng, DT), (None, None));
        assert_eq!(run, HillRun::default());
    }

    fn rink() -> Rink {
        Rink {
            attack: [384., 437.5],
            defend: [384., 330.5],
            min: [346.5, 334.],
            max: [421.5, 434.],
        }
    }

    #[test]
    fn the_hockey_attacker_gets_behind_the_puck_then_pushes_it_at_the_goal() {
        let skill = Skill::for_difficulty(1);
        let puck = [384., 90., 384.];
        // Goal side of the puck: go round to its back, wide of it.
        let goal = hockey_goal([384., 90., 400.], &rink(), puck, [0.; 3], false, &skill);
        assert!(goal.target[1] < 384. && (goal.target[0] - 384.).abs() > 4.);
        // Behind but off the line: line up behind it.
        let goal = hockey_goal([370., 90., 370.], &rink(), puck, [0.; 3], false, &skill);
        assert!(goal.target[1] < 384. && (goal.target[0] - 384.).abs() < 1.);
        // Lined up behind it: ride through it toward the goal.
        let goal = hockey_goal([384., 90., 372.], &rink(), puck, [0.; 3], false, &skill);
        assert!(goal.target[1] > 384. && !goal.stop);
    }

    #[test]
    fn the_hockey_defender_waits_goal_side_and_clears_a_close_puck() {
        let skill = Skill::for_difficulty(1);
        let far = [384., 90., 420.];
        let goal = hockey_goal([384., 90., 350.], &rink(), far, [0.; 3], true, &skill);
        assert!(goal.stop && goal.target[1] > 340. && goal.target[1] < 384.);
        let near = [384., 90., 350.];
        let goal = hockey_goal([384., 90., 340.], &rink(), near, [0.; 3], true, &skill);
        assert!(!goal.stop && goal.target[1] > 350.);
        // Targets stay inside the boards.
        let corner = [347., 90., 433.];
        let goal = hockey_goal([360., 90., 400.], &rink(), corner, [0.; 3], false, &skill);
        assert!(goal.target[0] >= 349.5 && goal.target[1] <= 431.);
    }
}
