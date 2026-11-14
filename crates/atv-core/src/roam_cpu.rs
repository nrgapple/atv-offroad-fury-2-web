//! **Port adaptation, not recovered behavior.** Goal selection for the
//! authored CPU riders of a free ride (research/CPU_MINIGAMES.md "Free
//! roam"): the original Freestyle has no CPU riders. Off by default;
//! Options → Game Settings "Free Ride CPU riders" adds them.
//!
//! A roaming CPU keeps one [`Plan`] at a time: ride to a point of interest
//! (a hilltop of the venue, [`points_of_interest`]), follow the venue's
//! course line for a stretch where it has one, or drift a few seconds on a
//! heading of its own. Each quarter second it looks along its heading and
//! turns away from water too deep to ford and from the map's edge; the race
//! driver's peer correction (`cpu_avoidance::peers`) steers it round other
//! riders. [`crate::game_cpu::Driver`] turns the goal into controls and
//! gets a stuck rider out, as in the mini-games.
//!
//! Deterministic: fixed-step f32 arithmetic through [`Det`] and the CPU's
//! seeded [`Rng`]; no clock and no allocation per step.
use crate::cpu_avoidance;
use crate::game_cpu::{Goal, Other, Rng, Sense, Skill, taste};
use crate::math::Det;

/// A point of interest is reached inside this distance (ft).
pub const ARRIVE: f32 = 30.;
/// Seconds between looks at the ground ahead.
pub const LOOK_SECONDS: f32 = 0.25;
/// Distances along a heading that a look tests (ft).
pub const PROBES: [f32; 3] = [25., 50., 80.];
/// Turns tried to each side of a blocked heading (radians).
pub const TURNS: [f32; 6] = [0.45, 0.9, 1.35, 1.8, 2.3, std::f32::consts::PI];
/// The longest one plan lasts (seconds).
pub const PLAN_SECONDS: f32 = 50.;
/// Seconds of turning away from one plan's aim before another is picked.
pub const BLOCKED_SECONDS: f32 = 8.;
/// How far ahead on the course line a CPU aims (ft).
pub const LINE_AHEAD: f32 = 60.;
/// Water deeper than this over the ground is not ridden into (ft; the
/// deep-water reset needs the ATV 3 ft under for 5 s, `water_failure`).
pub const WADE: f32 = 2.;
/// Ground steeper than this (rise over run, about 27°) is not ridden onto:
/// across it the ATV rolls, and straight at a crest over it it flies off
/// and lands on its back.
pub const STEEP: f32 = 0.5;
/// Half the span the slope is measured over (ft).
pub const SLOPE_SPAN: f32 = 6.;
/// Seconds a roamer may sit still (under the stuck speed) before it is
/// returned to the start: longer than the driver's whole stuck escalation
/// (back out twice, turn, reset), so this only catches what that cannot,
/// such as an ATV beached with its wheels off the ground.
pub const STALL_SECONDS: f32 = 12.;
/// The most points of interest a venue keeps.
pub const MAX_POINTS: usize = 24;
/// The share of new plans that follow the course line, and that drift.
pub const LINE_SHARE: f32 = 0.4;
pub const DRIFT_SHARE: f32 = 0.12;
/// How far a synthesized target lies along the heading (ft).
const REACH: f32 = 80.;

/// What a venue offers a roaming CPU, on the ground plane (x, z).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    /// The rideable area's corners (inside the boundary launch margin).
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Points of interest: dry and inside the area.
    pub points: Vec<[f32; 2]>,
    /// The course line in riding order; empty where the venue has none.
    pub line: Vec<[f32; 2]>,
    /// The line's end joins its start.
    pub closed: bool,
}

impl Map {
    pub fn outside(&self, p: [f32; 2]) -> bool {
        p[0] < self.min[0] || p[0] > self.max[0] || p[1] < self.min[1] || p[1] > self.max[1]
    }

    pub fn centre(&self) -> [f32; 2] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
        ]
    }

    /// A course line for this map: `points` in order, closed when its end
    /// comes back to its start.
    pub fn set_line(&mut self, points: Vec<[f32; 2]>) {
        self.closed = points.len() > 2
            && points
                .first()
                .zip(points.last())
                .is_some_and(|(a, b)| flat(*a, *b) < 200.);
        self.line = points;
    }
}

fn flat(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot_d(a[1] - b[1])
}

fn unit(v: [f32; 2]) -> Option<[f32; 2]> {
    let len = v[0].hypot_d(v[1]);
    (len > 1e-4).then(|| [v[0] / len, v[1] / len])
}

/// Whether the ground around `p` is steeper than [`STEEP`] (or partly off
/// the terrain), from `ground`'s heights [`SLOPE_SPAN`] to each side.
pub fn steep(ground: &dyn Fn(f32, f32) -> Option<f32>, p: [f32; 2]) -> bool {
    let d = SLOPE_SPAN;
    match (
        ground(p[0] + d, p[1]),
        ground(p[0] - d, p[1]),
        ground(p[0], p[1] + d),
        ground(p[0], p[1] - d),
    ) {
        (Some(east), Some(west), Some(north), Some(south)) => {
            ((east - west) / (2. * d)).hypot_d((north - south) / (2. * d)) > STEEP
        }
        _ => true,
    }
}

/// `h` turned by `angle` radians (positive to the right, as
/// [`crate::game_cpu::bearing`] measures).
fn turned(h: [f32; 2], angle: f32) -> [f32; 2] {
    let (sin, cos) = angle.sin_cos_d();
    [h[0] * cos + h[1] * sin, h[1] * cos - h[0] * sin]
}

/// The venue's points of interest: the start, then its most prominent dry
/// hilltops on a coarse grid (a cell higher than its eight neighbours and
/// at least four feet over their mean). A venue too flat for six gets dry
/// grid points instead, so there is always somewhere to ride to.
/// `ground` is the terrain height (None off the terrain) and `hazard` the
/// test a look uses.
pub fn points_of_interest(
    min: [f32; 2],
    max: [f32; 2],
    start: [f32; 2],
    ground: &dyn Fn(f32, f32) -> Option<f32>,
    hazard: &dyn Fn([f32; 2]) -> bool,
) -> Vec<[f32; 2]> {
    const N: usize = 28;
    let step = [(max[0] - min[0]) / N as f32, (max[1] - min[1]) / N as f32];
    let at = |i: usize, j: usize| {
        [
            min[0] + (i as f32 + 0.5) * step[0],
            min[1] + (j as f32 + 0.5) * step[1],
        ]
    };
    let mut points = vec![start];
    if !(step[0] > 0. && step[1] > 0.) {
        return points;
    }
    let heights: Vec<Option<f32>> = (0..N * N)
        .map(|n| {
            let p = at(n % N, n / N);
            ground(p[0], p[1]).filter(|_| !hazard(p))
        })
        .collect();
    let mut hills: Vec<(f32, [f32; 2])> = Vec::new();
    for j in 1..N - 1 {
        for i in 1..N - 1 {
            let Some(h) = heights[j * N + i] else {
                continue;
            };
            let mut sum = 0.;
            let mut top = true;
            for (di, dj) in [
                (0, 0),
                (1, 0),
                (2, 0),
                (0, 1),
                (2, 1),
                (0, 2),
                (1, 2),
                (2, 2),
            ] {
                match heights[(j + dj - 1) * N + i + di - 1] {
                    Some(n) if n <= h => sum += n,
                    _ => top = false,
                }
            }
            let prominence = h - sum / 8.;
            if top && prominence >= 4. {
                hills.push((prominence, at(i, j)));
            }
        }
    }
    hills.sort_by(|a, b| b.0.total_cmp(&a.0));
    points.extend(hills.iter().take(MAX_POINTS - 1).map(|(_, p)| *p));
    if points.len() < 6 {
        for j in (2..N - 1).step_by(5) {
            for i in (2..N - 1).step_by(5) {
                if heights[j * N + i].is_some() && points.len() < 12 {
                    points.push(at(i, j));
                }
            }
        }
    }
    points
}

/// The point of `line` nearest `p` among `count` segments from `first`:
/// its segment and the fraction along it.
fn project(
    line: &[[f32; 2]],
    closed: bool,
    first: usize,
    count: usize,
    p: [f32; 2],
) -> (usize, f32) {
    let segments = if closed { line.len() } else { line.len() - 1 };
    let mut best = (first.min(segments - 1), 0f32, f32::MAX);
    for n in 0..count.min(segments) {
        let segment = if closed {
            (first + n) % segments
        } else if first + n < segments {
            first + n
        } else {
            break;
        };
        let a = line[segment];
        let b = line[(segment + 1) % line.len()];
        let d = [b[0] - a[0], b[1] - a[1]];
        let fraction = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
            / (d[0] * d[0] + d[1] * d[1]).max(1e-4))
        .clamp(0., 1.);
        let error = flat(p, [a[0] + d[0] * fraction, a[1] + d[1] * fraction]);
        if error < best.2 {
            best = (segment, fraction, error);
        }
    }
    (best.0, best.1)
}

/// The point `distance` further along `line`; None past an open line's end.
fn ahead(
    line: &[[f32; 2]],
    closed: bool,
    mut segment: usize,
    mut fraction: f32,
    mut distance: f32,
) -> Option<[f32; 2]> {
    let segments = if closed { line.len() } else { line.len() - 1 };
    for _ in 0..=segments {
        let a = line[segment];
        let b = line[(segment + 1) % line.len()];
        let length = flat(a, b);
        let left = (1. - fraction) * length;
        if distance <= left {
            let f = fraction + distance / length.max(1e-4);
            return Some([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
        }
        distance -= left;
        fraction = 0.;
        segment += 1;
        if segment >= segments {
            if !closed {
                return None;
            }
            segment = 0;
        }
    }
    None
}

/// What a roaming CPU is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Plan {
    #[default]
    None,
    /// Riding to this point of interest.
    Point(usize),
    /// Following the course line from this segment for `left` more feet.
    Line { segment: usize, left: f32 },
    /// Riding a heading of its own for `left` more seconds.
    Drift { heading: [f32; 2], left: f32 },
}

/// One CPU's roaming state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Roam {
    pub plan: Plan,
    /// Seconds on this plan.
    clock: f32,
    /// Seconds to the next look.
    look: f32,
    /// The heading the last look chose, and whether it turned away from
    /// the plan's aim.
    heading: Option<[f32; 2]>,
    turning: bool,
    /// Seconds turned away from this plan's aim.
    blocked: f32,
    /// The side the last turn took (kept while blocked, so it does not
    /// flip from look to look).
    side: f32,
    /// This plan's share of the cruise speed.
    pace: f32,
    /// Plans picked so far (a plan's own taste).
    serial: u32,
    /// The points visited last.
    recent: [Option<usize>; 3],
    /// Diagnostics: points reached, turns away from a hazard and feet
    /// ridden on line plans.
    pub visits: u32,
    pub turns: u32,
    pub line_feet: f32,
}

impl Roam {
    fn pick(&mut self, p: [f32; 2], map: &Map, seed: u32, rng: &mut Rng) {
        let on_line = matches!(self.plan, Plan::Line { .. });
        self.clock = 0.;
        self.blocked = 0.;
        self.turning = false;
        self.heading = None;
        self.look = 0.;
        self.serial = self.serial.wrapping_add(1);
        self.pace = 0.7 + 0.3 * rng.unit();
        let roll = rng.unit();
        let length = rng.unit();
        let angle = rng.unit() * std::f32::consts::TAU;
        if map.line.len() >= 2 && !on_line && roll < LINE_SHARE {
            let (segment, _) = project(&map.line, map.closed, 0, map.line.len(), p);
            self.plan = Plan::Line {
                segment,
                left: 500. + 900. * length,
            };
            return;
        }
        // A point some way off, not one just visited, to this plan's taste.
        let wanted = 150. + 450. * length;
        let mut best: Option<(f32, usize)> = None;
        if roll <= 1. - DRIFT_SHARE {
            for (index, point) in map.points.iter().enumerate() {
                let distance = flat(p, *point);
                if self.recent.contains(&Some(index)) || distance < ARRIVE * 2. {
                    continue;
                }
                let cost =
                    (distance - wanted).abs() * (1. + 0.5 * taste(seed ^ self.serial, index));
                if best.is_none_or(|(c, _)| cost < c) {
                    best = Some((cost, index));
                }
            }
        }
        self.plan = match best {
            Some((_, index)) => Plan::Point(index),
            None => {
                let (sin, cos) = angle.sin_cos_d();
                Plan::Drift {
                    heading: [sin, cos],
                    left: 3. + 4. * length,
                }
            }
        };
    }

    /// The plan's aim this step, picking a new plan when this one is done.
    fn aim(
        &mut self,
        sense: &Sense,
        p: [f32; 2],
        map: &Map,
        seed: u32,
        rng: &mut Rng,
        dt: f32,
    ) -> Option<[f32; 2]> {
        for _ in 0..3 {
            match self.plan {
                Plan::Point(index) => match map.points.get(index) {
                    Some(point) if flat(p, *point) >= ARRIVE => return Some(*point),
                    Some(_) => {
                        self.visits += 1;
                        self.recent.rotate_right(1);
                        self.recent[0] = Some(index);
                    }
                    None => {}
                },
                Plan::Line { segment, left } if left > 0. && map.line.len() >= 2 => {
                    let (segment, fraction) = project(&map.line, map.closed, segment, 5, p);
                    if let Some(point) = ahead(&map.line, map.closed, segment, fraction, LINE_AHEAD)
                    {
                        let ridden = sense.speed.max(0.) * dt;
                        self.line_feet += ridden;
                        self.plan = Plan::Line {
                            segment,
                            left: left - ridden,
                        };
                        return Some(point);
                    }
                }
                Plan::Drift { heading, left } if left > 0. => {
                    self.plan = Plan::Drift {
                        heading,
                        left: left - dt,
                    };
                    return Some([p[0] + heading[0] * REACH, p[1] + heading[1] * REACH]);
                }
                _ => {}
            }
            self.pick(p, map, seed, rng);
        }
        None
    }

    /// One fixed step of `dt` seconds: where to ride. `hazard` is true for
    /// ground a CPU keeps off (deep water, off the terrain); `others` are
    /// the other riders.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        sense: &Sense,
        map: &Map,
        hazard: &dyn Fn([f32; 2]) -> bool,
        others: &[Other],
        seed: u32,
        skill: &Skill,
        rng: &mut Rng,
        dt: f32,
    ) -> Option<Goal> {
        if !sense.riding {
            // A wreck or a reset: look again from wherever it ends.
            self.heading = None;
            return None;
        }
        let p = [sense.position[0], sense.position[2]];
        self.clock += dt;
        if self.clock > PLAN_SECONDS || self.blocked > BLOCKED_SECONDS {
            self.pick(p, map, seed, rng);
        }
        let aim = self.aim(sense, p, map, seed, rng, dt)?;
        let distance = flat(p, aim);
        let blocked = |q: [f32; 2]| map.outside(q) || hazard(q);
        self.look -= dt;
        if self.look <= 0. || self.heading.is_none() {
            self.look = LOOK_SECONDS;
            let direct = unit([aim[0] - p[0], aim[1] - p[1]])
                .or(unit(sense.forward))
                .unwrap_or([0., 1.]);
            // Clear probes along a heading (the direct one only as far as
            // the aim: a hilltop by a shore is still reached), one fewer
            // when the turn to it from the way the ATV faces would sweep
            // over blocked ground (the nearest probe, half way round).
            let facing = unit(sense.forward);
            let clear = |h: [f32; 2], limit: f32| {
                let ahead = PROBES
                    .iter()
                    .filter(|d| {
                        let d = d.min(limit);
                        !blocked([p[0] + h[0] * d, p[1] + h[1] * d])
                    })
                    .count();
                let sweep = facing
                    .and_then(|f| unit([f[0] + h[0], f[1] + h[1]]))
                    .is_some_and(|m| blocked([p[0] + m[0] * PROBES[0], p[1] + m[1] * PROBES[0]]));
                ahead - usize::from(sweep && ahead > 0)
            };
            if clear(direct, distance + 5.) == PROBES.len() {
                self.heading = Some(direct);
                self.turning = false;
            } else {
                if !self.turning {
                    self.side = if rng.unit() < 0.5 { 1. } else { -1. };
                    self.turns += 1;
                }
                // The smallest turn with every probe clear; else the
                // clearest; else back toward the middle of the map.
                let mut best = (
                    0,
                    unit([map.centre()[0] - p[0], map.centre()[1] - p[1]]),
                    0.,
                );
                for turn in TURNS {
                    for side in [self.side, -self.side] {
                        let h = turned(direct, turn * side);
                        let score = clear(h, f32::MAX);
                        if score > best.0 {
                            best = (score, Some(h), side);
                        }
                    }
                    if best.0 == PROBES.len() {
                        break;
                    }
                }
                self.heading = best.1.or(Some(direct));
                if best.2 != 0. {
                    self.side = best.2;
                }
                self.turning = true;
            }
        }
        if self.turning {
            self.blocked += dt;
        } else {
            self.blocked = 0.;
        }
        let mut heading = self.heading?;
        // Slower while turning away, and slowest for a heading well off the
        // way the ATV faces, so a sharp turn by a hazard keeps a tight line.
        let facing = unit(sense.forward).map_or(1., |f| f[0] * heading[0] + f[1] * heading[1]);
        let speed = skill.cruise
            * self.pace
            * if self.turning { 0.75 } else { 1. }
            * (0.35 + 0.65 * facing.max(0.));
        // The race driver's peer correction on the wanted velocity
        // (`cpu_avoidance::peers`), unless it would turn into a hazard.
        let mut peers = [cpu_avoidance::Peer {
            origin: [0.; 3],
            velocity: [0.; 3],
            human: None,
        }; 4];
        let count = others.len().min(peers.len());
        for (peer, other) in peers.iter_mut().zip(others) {
            peer.origin = other.position;
            peer.velocity = other.velocity;
        }
        let response = cpu_avoidance::peers(
            sense.position,
            [heading[0] * speed, 0., heading[1] * speed],
            &peers[..count],
            0,
            false,
        );
        if response.peers > 0
            && let Some(h) = unit([response.velocity[0], response.velocity[2]])
            && !blocked([p[0] + h[0] * PROBES[0], p[1] + h[1] * PROBES[0]])
        {
            heading = h;
        }
        // Ride onto a near aim itself; else a point along the heading.
        let reach = if self.turning {
            REACH
        } else {
            distance.clamp(ARRIVE * 0.5, REACH)
        };
        Some(Goal::ride(
            [p[0] + heading[0] * reach, p[1] + heading[1] * reach],
            speed,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_cpu::Driver;

    const DT: f32 = 1. / 60.;

    fn map(points: &[[f32; 2]]) -> Map {
        Map {
            min: [0., 0.],
            max: [2000., 2000.],
            points: points.to_vec(),
            ..Map::default()
        }
    }

    struct Rider {
        sense: Sense,
        driver: Driver,
        roam: Roam,
        rng: Rng,
        seed: u32,
    }

    impl Rider {
        fn new(at: [f32; 2], seed: u32) -> Self {
            Self {
                sense: Sense {
                    position: [at[0], 0., at[1]],
                    forward: [0., 1.],
                    speed: 0.,
                    grounded: true,
                    riding: true,
                },
                driver: Driver::default(),
                roam: Roam::default(),
                rng: Rng::new(seed),
                seed,
            }
        }

        fn at(&self) -> [f32; 2] {
            [self.sense.position[0], self.sense.position[2]]
        }

        /// One step of a point mass that follows the drive.
        fn step(&mut self, map: &Map, hazard: &dyn Fn([f32; 2]) -> bool, others: &[Other]) {
            let skill = Skill::for_difficulty(1);
            let goal = self.roam.step(
                &self.sense,
                map,
                hazard,
                others,
                self.seed,
                &skill,
                &mut self.rng,
                DT,
            );
            let d = self.driver.step(&self.sense, goal.as_ref(), DT);
            let s = &mut self.sense;
            if d.axis < 0. {
                s.speed = (s.speed - 20. * DT).max(-10.);
            } else {
                s.speed = (s.speed + (d.axis * 30. - d.brake * 40. - s.speed * 0.2) * DT).max(0.);
            }
            let yaw = s.forward[0].atan2_d(s.forward[1]) + d.steer * 1.5 * DT * s.speed.signum();
            let (sin, cos) = yaw.sin_cos_d();
            s.forward = [sin, cos];
            s.position[0] += sin * s.speed * DT;
            s.position[2] += cos * s.speed * DT;
        }
    }

    const DRY: &dyn Fn([f32; 2]) -> bool = &|_| false;

    #[test]
    fn hilltops_are_points_of_interest_and_wet_ones_are_not() {
        // Two hills; the second stands in water.
        let ground = |x: f32, z: f32| {
            let hill =
                |cx: f32, cz: f32| 40. * (-((x - cx).powi(2) + (z - cz).powi(2)) / 20000.).exp();
            Some(hill(500., 500.) + hill(1500., 1400.))
        };
        let wet = |p: [f32; 2]| flat(p, [1500., 1400.]) < 300.;
        let points = points_of_interest([0., 0.], [2000., 2000.], [100., 100.], &ground, &wet);
        assert_eq!(points[0], [100., 100.]);
        assert!(
            points.iter().any(|p| flat(*p, [500., 500.]) < 80.),
            "{points:?}"
        );
        assert!(points.iter().all(|p| !wet(*p)));
        assert!(points.len() <= MAX_POINTS);
        // A flat venue still gets somewhere to go.
        let level = points_of_interest(
            [0., 0.],
            [2000., 2000.],
            [100., 100.],
            &|_, _| Some(0.),
            DRY,
        );
        assert!(level.len() >= 6, "{}", level.len());
        // Off the terrain there is nothing but the start.
        let none = points_of_interest([0., 0.], [2000., 2000.], [100., 100.], &|_, _| None, DRY);
        assert_eq!(none, [[100., 100.]]);
    }

    #[test]
    fn steep_ground_is_a_hazard() {
        // A 30° ramp along x, level beyond x = 100.
        let ground = |x: f32, _: f32| Some(x.min(100.) * 0.577);
        assert!(steep(&ground, [50., 0.]));
        assert!(!steep(&ground, [200., 0.]));
        // Gentle in both directions (about 20° overall) is rideable.
        let gentle = |x: f32, z: f32| Some(0.25 * x + 0.25 * z);
        assert!(!steep(&gentle, [0., 0.]));
        // The terrain's edge counts as steep.
        assert!(steep(&|x, _| (x < 3.).then_some(0.), [0., 0.]));
    }

    #[test]
    fn a_roamer_visits_different_points_and_repeats_with_its_seed() {
        let map = map(&[
            [300., 300.],
            [900., 400.],
            [1500., 600.],
            [600., 1300.],
            [1400., 1500.],
        ]);
        let run = |seed: u32| {
            let mut rider = Rider::new([1000., 1000.], seed);
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..(60 * 240) {
                rider.step(&map, DRY, &[]);
                if let Plan::Point(index) = rider.roam.plan {
                    seen.insert(index);
                }
                assert!(!map.outside(rider.at()), "{:?}", rider.at());
            }
            (rider.roam.visits, seen.len(), rider.at())
        };
        let (visits, seen, at) = run(11);
        assert!(visits >= 4, "{visits}");
        assert!(seen >= 3, "{seen}");
        assert_eq!(run(11), (visits, seen, at));
        assert_ne!(run(12).2, at);
    }

    #[test]
    fn a_roamer_rides_round_a_lake_and_stays_on_the_map() {
        // Points either side of a lake in the middle and one in a corner.
        let lake = |p: [f32; 2]| flat(p, [1000., 1000.]) < 250.;
        let map = map(&[[400., 1000.], [1600., 1000.], [1000., 300.], [1900., 1900.]]);
        let mut turns = 0;
        for seed in 1..13 {
            let mut rider = Rider::new([450., 1000.], seed);
            for _ in 0..(60 * 300) {
                rider.step(&map, &lake, &[]);
                let at = rider.at();
                assert!(
                    flat(at, [1000., 1000.]) > 235.,
                    "seed {seed} in the lake at {at:?}: {:?}",
                    rider.roam
                );
                assert!(!map.outside(at), "seed {seed} off the map at {at:?}");
            }
            assert!(rider.roam.visits >= 3, "seed {seed}: {}", rider.roam.visits);
            assert_eq!(rider.driver.resets, 0, "seed {seed}");
            turns += rider.roam.turns;
        }
        assert!(turns >= 3, "{turns}");
    }

    #[test]
    fn a_roamer_follows_the_course_line_where_there_is_one() {
        // A square circuit; nothing else to ride to.
        let mut map = map(&[]);
        map.set_line(vec![
            [400., 400.],
            [1600., 400.],
            [1600., 1600.],
            [400., 1600.],
            [400., 420.],
        ]);
        assert!(map.closed);
        let on_line = |p: [f32; 2]| {
            let (segment, fraction) = project(&map.line, true, 0, 5, p);
            let a = map.line[segment];
            let b = map.line[(segment + 1) % 5];
            flat(
                p,
                [
                    a[0] + (b[0] - a[0]) * fraction,
                    a[1] + (b[1] - a[1]) * fraction,
                ],
            )
        };
        let mut rider = Rider::new([500., 500.], 3);
        let mut near = 0;
        let mut line_steps = 0;
        for _ in 0..(60 * 300) {
            rider.step(&map, DRY, &[]);
            if matches!(rider.roam.plan, Plan::Line { .. }) && rider.roam.clock > 15. {
                line_steps += 1;
                if on_line(rider.at()) < 40. {
                    near += 1;
                }
            }
        }
        assert!(rider.roam.line_feet > 1500., "{}", rider.roam.line_feet);
        assert!(
            line_steps > 600 && near * 10 >= line_steps * 9,
            "{near}/{line_steps}"
        );
        // An open line ends: the rider picks something else there.
        let mut open = Map::default();
        open.set_line(vec![[0., 0.], [0., 500.], [0., 1500.]]);
        assert!(!open.closed);
        assert_eq!(ahead(&open.line, false, 1, 0.5, 400.), Some([0., 1400.]));
        assert_eq!(ahead(&open.line, false, 1, 0.5, 600.), None);
        assert_eq!(project(&open.line, false, 0, 9, [30., 700.]), (1, 0.2));
    }

    #[test]
    fn a_roamer_steers_round_a_rider_in_its_way_and_a_wreck_wants_nothing() {
        let map = map(&[[1000., 1800.]]);
        let skill = Skill::for_difficulty(1);
        let goal = |others: &[Other]| {
            let mut rider = Rider::new([1000., 1000.], 2);
            rider.roam.plan = Plan::Point(0);
            rider.roam.pace = 1.;
            rider.sense.speed = 40.;
            rider
                .roam
                .step(
                    &rider.sense,
                    &map,
                    DRY,
                    others,
                    2,
                    &skill,
                    &mut rider.rng,
                    DT,
                )
                .unwrap()
        };
        let free = goal(&[]);
        assert!((free.target[0] - 1000.).abs() < 0.01 && free.target[1] > 1000.);
        // A stopped rider ahead and a little to the right: aim left of it.
        let parked = Other {
            position: [1004., 0., 1030.],
            velocity: [0.; 3],
        };
        assert!(goal(&[parked]).target[0] < free.target[0] - 0.5);
        let mut rider = Rider::new([1000., 1000.], 2);
        rider.sense.riding = false;
        let wreck = rider
            .roam
            .step(&rider.sense, &map, DRY, &[], 2, &skill, &mut rider.rng, DT);
        assert_eq!(wreck, None);
    }
}
