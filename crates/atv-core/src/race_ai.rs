//! Authored reconstruction of the National path driver's recovered decisions.
//! Exact-build 0024f9d0 / 001f9690 / 002536b0 with 00252080 direction
//! avoidance and001e9650 clip eligibility. Physics belongs to Vehicle.
use crate::{
    Result,
    race::Course,
    vehicle::{VehicleFrame, VehicleInput},
};

#[derive(Clone, Copy, Debug)]
pub struct RoutePosition {
    pub segment: usize,
    pub fraction: f32,
    pub point: [f32; 3],
    pub error: f32,
    pub distance: f32,
}
impl Course {
    pub fn project(&self, origin: [f32; 3]) -> RoutePosition {
        let mut best = RoutePosition {
            segment: 0,
            fraction: 0.,
            point: self.points[0],
            error: f32::MAX,
            distance: 0.,
        };
        let mut total = 0.;
        for (i, a) in self.points.iter().enumerate() {
            let b = self.points[self.next(i)];
            let dx = b[0] - a[0];
            let dz = b[2] - a[2];
            let length = (dx * dx + dz * dz).sqrt();
            let fraction = (((origin[0] - a[0]) * dx + (origin[2] - a[2]) * dz)
                / (length * length).max(0.0001))
            .clamp(0., 1.);
            let point = std::array::from_fn(|n| a[n] + fraction * (b[n] - a[n]));
            let error = ((point[0] - origin[0]).powi(2) + (point[2] - origin[2]).powi(2)).sqrt();
            if error < best.error {
                best = RoutePosition {
                    segment: i,
                    fraction,
                    point,
                    error,
                    distance: total + fraction * length,
                };
            }
            total += length;
        }
        best
    }
    /// Nearest racing-line point within the route window around a rider's
    /// race station. Native 0017be80 tracks the current node instead of
    /// searching the whole course, so a course that passes beside itself
    /// (e.g. nat01's lap beside its start straight) cannot capture the CPU.
    pub fn project_near(&self, origin: [f32; 3], station: crate::race::Station) -> RoutePosition {
        let in_cycle = station.segment >= self.loop_start;
        let mut first = station.segment;
        let mut behind = station.fraction * self.segment(first);
        while behind < crate::race::TRACK_BEHIND {
            let Some(previous) = self.previous(first, in_cycle) else {
                break;
            };
            first = previous;
            behind += self.segment(previous);
        }
        let mut total: f32 = (0..first).map(|i| self.segment(i)).sum();
        let mut best = RoutePosition {
            segment: station.segment,
            fraction: station.fraction,
            point: self.point_at(station),
            error: f32::MAX,
            distance: 0.,
        };
        let mut segment = first;
        let mut covered = 0.;
        for _ in 0..self.points.len() {
            let a = self.points[segment];
            let b = self.points[self.next(segment)];
            let dx = b[0] - a[0];
            let dz = b[2] - a[2];
            let length = (dx * dx + dz * dz).sqrt();
            let fraction = (((origin[0] - a[0]) * dx + (origin[2] - a[2]) * dz)
                / (length * length).max(0.0001))
            .clamp(0., 1.);
            let point = std::array::from_fn(|n| a[n] + fraction * (b[n] - a[n]));
            let error = ((point[0] - origin[0]).powi(2) + (point[2] - origin[2]).powi(2)).sqrt();
            if error < best.error {
                best = RoutePosition {
                    segment,
                    fraction,
                    point,
                    error,
                    distance: total + fraction * length,
                };
            }
            covered += length;
            if covered > behind + crate::race::TRACK_AHEAD * 2. {
                break;
            }
            total = if self.next(segment) == self.loop_start {
                (0..self.loop_start).map(|i| self.segment(i)).sum()
            } else {
                total + length
            };
            segment = self.next(segment);
        }
        best
    }
    pub fn ahead(&self, mut position: RoutePosition, mut distance: f32) -> RoutePosition {
        for _ in 0..self.points.len() {
            let a = self.points[position.segment];
            let b = self.points[self.next(position.segment)];
            let length = ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
            let available = (1. - position.fraction) * length;
            if distance <= available {
                position.fraction += distance / length;
                position.point = std::array::from_fn(|n| a[n] + position.fraction * (b[n] - a[n]));
                return position;
            }
            distance -= available;
            position.segment = self.next(position.segment);
            position.fraction = 0.;
            position.point = b;
        }
        position
    }
    pub fn tangent(&self, segment: usize) -> [f32; 3] {
        normalize(sub(self.points[self.next(segment)], self.points[segment]))
    }
    pub fn target_speed(&self, position: RoutePosition) -> f32 {
        let speed = |i| {
            let v = self.speeds.get(i).copied().unwrap_or(-1.);
            if v == -1. { 150. } else { v }
        };
        speed(position.segment) * (1. - position.fraction)
            + position.fraction * speed(self.next(position.segment))
    }
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let len = dot(a, a).sqrt().max(0.000001);
    a.map(|v| v / len)
}
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [w, x, y, z] = q;
    let a = [x, y, z];
    let cross = |b: [f32; 3], c: [f32; 3]| {
        [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ]
    };
    let t = cross(a, v).map(|n| 2. * n);
    let c = cross(a, t);
    std::array::from_fn(|i| v[i] + w * t[i] + c[i])
}
/// Original 0024fe4c lateral slew/clamp after the measured route offset.
pub fn lateral_offset(measured: f32, goal: f32, dt: f32, limit: f32) -> f32 {
    (measured + (goal - measured).clamp(-10. * dt, 10. * dt)).clamp(-limit, limit)
}
/// Original speed deadband, with strict comparisons at both boundaries.
pub fn pedals(speed: f32, target: f32) -> (f32, f32) {
    if speed < target - 1. {
        (1., 0.)
    } else if target + 1. < speed {
        (0., 1.)
    } else {
        (0., 0.)
    }
}
/// 002536b0: limit the target only when ahead of the reference player.
pub fn leader_speed(
    target: f32,
    reference: f32,
    ahead: f32,
    minimum: f32,
    policy: [f32; 4],
) -> f32 {
    let base = minimum.max(reference);
    let cap = base * policy[1];
    let half = target * policy[3];
    if ahead > policy[0] {
        if target > cap { half.max(cap) } else { target }
    } else if ahead > 35. {
        target.min(base + (5. + (ahead * ((5. + base) * (policy[1] - 1.))) / policy[0]))
    } else {
        target
    }
}
/// Ordinary airborne terrain-normal feedback, 00250814 branch. Constructor
/// gains +400=10, +404=0.1; the native high/low clearance split is 20ft.
pub fn low_clearance_lean(previous: f32, terrain_normal: [f32; 3], forward: [f32; 3]) -> f32 {
    let target = 10. * -dot(terrain_normal, forward);
    (previous * (1. - 0.1) + 0.1 * target).clamp(-1., 1.)
}
#[derive(serde::Serialize, Debug, Clone, Copy)]
pub struct Decisions {
    pub active: bool,
    pub avoidance: crate::cpu_avoidance::Response,
    pub target: [f32; 3],
    pub desired_speed: f32,
    pub ahead_distance: f32,
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub lean: f32,
    pub route_segment: usize,
    pub route_distance: f32,
    pub route_error: f32,
    pub lane_offset: f32,
}
pub struct Context<'a> {
    pub peers: &'a [crate::cpu_avoidance::Peer],
    pub scene: Option<&'a crate::scene_collision::SceneCollision>,
    pub clearance: f32,
    pub terrain_normal: [f32; 3],
    /// The racer's on-track race station; None searches the whole course.
    pub station: Option<crate::race::Station>,
}
pub struct Driver {
    random: crate::engine_audio::OriginalRandom,
    goal: f32,
    last_target: usize,
    lane: f32,
    lean: f32,
    trick_timer: f32,
    air_timer: f32,
    trick_attempted: bool,
}
impl Driver {
    pub fn new(index: usize, _difficulty: u32) -> Self {
        let mut random = crate::engine_audio::OriginalRandom::with_seed(97211 + index as u64 * 47);
        let goal = random.next_u31() as f32 * 4.656613e-10 * 12. - 6.;
        Self {
            random,
            goal,
            last_target: usize::MAX,
            lane: 0.,
            lean: 0.,
            trick_timer: 0.,
            air_timer: 0.,
            trick_attempted: false,
        }
    }
    pub fn repositioned(&mut self) {
        self.last_target = usize::MAX;
        self.lane = 0.;
        self.lean = 0.;
        self.air_timer = 0.;
    }
    /// Ordinary National gate: 10-second interval, flagged jump, 10% trial;
    /// choose only main clips that fit the remaining TDF airtime minus0.5s.
    /// Per-racer RNG and projected target lookup are integration adaptations.
    pub fn stunt_choice(
        &mut self,
        course: &Course,
        frame: &VehicleFrame,
        in_stunt: bool,
        durations: &[f32; 24],
        dt: f32,
    ) -> Option<usize> {
        if frame.wreck.phase != crate::wreck::Phase::Riding {
            return None;
        }
        let grounded = frame.grounded.iter().any(|v| *v);
        let mut choice = None;
        if grounded
            || in_stunt
            || course.flags.get(self.last_target).copied().unwrap_or(0) & 0x20 == 0
        {
            self.trick_timer += dt;
        } else if self.trick_timer >= 10. {
            let budget =
                course.airtimes.get(self.last_target).copied().unwrap_or(0.) - self.air_timer - 0.5;
            if self.random.next_u31() as f32 * 4.656613e-10 * 100. <= 10. {
                let eligible: Vec<_> = durations
                    .iter()
                    .enumerate()
                    .filter(|(_, duration)| **duration <= budget)
                    .map(|(id, _)| id)
                    .collect();
                if !eligible.is_empty() {
                    let index = (eligible.len() as f32
                        * self.random.next_u31() as f32
                        * 4.656613e-10) as usize;
                    choice = eligible.get(index).copied();
                    self.trick_attempted = choice.is_some();
                }
            }
        }
        if grounded {
            if self.trick_attempted {
                self.trick_timer = 0.;
                self.trick_attempted = false;
            }
            self.air_timer = 0.;
        } else {
            self.air_timer += dt;
        }
        choice
    }
    pub fn step(
        &mut self,
        course: &Course,
        frame: &VehicleFrame,
        player_speed: f32,
        progress_difference: f32,
        dt: f32,
        context: Context<'_>,
    ) -> Result<(VehicleInput, Decisions)> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 || course.points.len() < 4 {
            return Err("invalid CPU driving step".into());
        }
        let p = match context.station {
            Some(station) => course.project_near(frame.origin, station),
            None => course.project(frame.origin),
        };
        if frame.wreck.phase != crate::wreck::Phase::Riding {
            return Ok((
                VehicleInput::default(),
                Decisions {
                    active: false,
                    avoidance: crate::cpu_avoidance::Response::default(),
                    target: p.point,
                    desired_speed: 0.,
                    ahead_distance: progress_difference,
                    throttle: 0.,
                    brake: 0.,
                    steer: 0.,
                    lean: 0.,
                    route_segment: p.segment,
                    route_distance: p.distance,
                    route_error: p.error,
                    lane_offset: self.lane,
                },
            ));
        }
        let on_track = p.error <= course.widths[p.segment] * 0.5;
        let lookahead = (25. + 4. * p.error).min(if on_track {
            60.
        } else {
            course.cycle_length() * 0.5
        });
        let aim = course.ahead(p, lookahead);
        if aim.segment != self.last_target {
            self.goal = course.lane_limit * self.random.next_u31() as f32 * 4.656613e-10 * 2.
                - course.lane_limit;
            self.last_target = aim.segment;
        }
        let tangent = course.tangent(p.segment);
        let delta = sub(p.point, frame.origin);
        let measured = delta[2] * tangent[0] - delta[0] * tangent[2];
        let lane = if course.flags.get(p.segment).copied().unwrap_or(0) & 0x10 == 0 {
            lateral_offset(measured, self.goal, dt, course.lane_limit)
        } else {
            self.lane
        };
        self.lane = lane;
        let tangent = course.tangent(aim.segment);
        let target = [
            aim.point[0] + lane * tangent[2],
            aim.point[1],
            aim.point[2] - lane * tangent[0],
        ];
        let desired = normalize([target[0] - frame.origin[0], 0., target[2] - frame.origin[2]]);
        let avoidance = crate::cpu_avoidance::direction(
            frame.origin,
            desired.map(|v| v * frame.speed_magnitude),
            context.peers,
            course.flags.get(p.segment).copied().unwrap_or(0),
            context.scene,
        )?;
        // 0024f840 leaves the original heading when the correction is zero.
        let direction = if avoidance.velocity[0] != 0. || avoidance.velocity[2] != 0. {
            normalize([avoidance.velocity[0], 0., avoidance.velocity[2]])
        } else {
            desired
        };
        let forward = rotate(frame.orientation, [0., 0., 1.]);
        let up = rotate(frame.orientation, [0., 1., 0.]);
        let horizontal = normalize([forward[0], 0., forward[2]]);
        let right = [horizontal[2], 0., -horizontal[0]];
        let angle = dot(direction, right).atan2(dot(direction, horizontal));
        let steer = (5. * angle)
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
            .sin();
        let mut desired_speed = if on_track {
            course.target_speed(aim)
        } else {
            50.
        };
        desired_speed = leader_speed(
            desired_speed,
            player_speed,
            progress_difference,
            60.,
            course.leader_policy,
        );
        let (throttle, brake) = pedals(frame.speed_magnitude, desired_speed);
        // The ground route uses no guessed corner/braking lean. The airborne
        // feedback reconstructs the ordinary high-clearance 0024f9d0 branch;
        // low-clearance terrain-normal feedback is separate from native stunt choice.
        let corrected = normalize([
            tangent[0] + (target[0] - frame.origin[0]) * 0.05,
            0.,
            tangent[2] + (target[2] - frame.origin[2]) * 0.05,
        ]);
        let airborne = !frame.grounded.iter().any(|v| *v);
        let lean = if airborne && context.clearance > 20. {
            let desired = [
                corrected[0] * 0.9816,
                -f32::from_bits(0x3e436113),
                corrected[2] * 0.9816,
            ];
            let diff = sub(desired, forward);
            let desired_pitch = -0.5 * dot(diff, up).atan2(dot(diff, forward));
            let [w, x, y, z] = frame.orientation;
            let pitch = (2. * (w * x + y * z)).atan2(1. - 2. * (x * x + y * y));
            (1.5 * (pitch - desired_pitch))
                .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
                .sin()
        } else if airborne {
            low_clearance_lean(self.lean, context.terrain_normal, forward)
        } else {
            0.
        };
        self.lean = lean;
        let input = VehicleInput {
            throttle,
            brake,
            steer,
            lean,
            ..VehicleInput::default()
        };
        Ok((
            input,
            Decisions {
                active: true,
                avoidance,
                target,
                desired_speed,
                ahead_distance: progress_difference,
                throttle,
                brake,
                steer,
                lean,
                route_segment: p.segment,
                route_distance: p.distance,
                route_error: p.error,
                lane_offset: lane,
            },
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn landing_feedback_tracks_surface_pitch_and_saturates_without_ground_bias() {
        assert_eq!(low_clearance_lean(0., [0., 1., 0.], [0., 0., 1.]), 0.);
        assert!(low_clearance_lean(0., [0., 0.98, 0.2], [0., 0., 1.]) < 0.);
        assert_eq!(low_clearance_lean(-1., [0., 0., 1.], [0., 0., 1.]), -1.);
        assert_eq!(low_clearance_lean(1., [0., 0., -1.], [0., 0., 1.]), 1.);
    }
    #[test]
    fn exact_deadband_and_ahead_only_speed_policy() {
        assert_eq!(pedals(48.999, 50.), (1., 0.));
        assert_eq!(pedals(49., 50.), (0., 0.));
        assert_eq!(pedals(51., 50.), (0., 0.));
        assert_eq!(pedals(51.001, 50.), (0., 1.));
        let policy = [150., 0.9, 1.5, 0.5];
        assert_eq!(leader_speed(150., 20., -100., 60., policy), 150.);
        assert_eq!(leader_speed(150., 20., 35., 60., policy), 150.);
        assert_eq!(leader_speed(150., 20., 151., 60., policy), 75.);
        assert_eq!(leader_speed(40., 100., 151., 60., policy), 40.);
    }
    #[test]
    fn projection_and_lookahead_wrap_use_horizontal_track_lengths() {
        let c = Course {
            points: vec![
                [0., 0., 0.],
                [0., 0., 100.],
                [100., 0., 100.],
                [100., 0., 0.],
            ],
            widths: vec![30.; 4],
            speeds: vec![-1., 40., 80., -1.],
            ..Course::default()
        };
        let p = c.project([4., 12., 50.]);
        assert_eq!(
            (p.segment, p.fraction, p.error, p.distance),
            (0, 0.5, 4., 50.)
        );
        let a = c.ahead(p, 75.);
        assert_eq!((a.segment, a.fraction, a.point), (1, 0.25, [25., 0., 100.]));
        assert_eq!(c.target_speed(a), 50.);
        let a = c.ahead(c.project([20., 0., 0.]), 50.);
        assert_eq!(a.segment, 0);
        assert!((a.point[2] - 30.).abs() < 0.00001);
        assert_eq!([a.point[0], a.point[1]], [0., 0.]);
    }
}
