//! Authored reconstruction of the National path driver's recovered decisions.
//! Exact-build 0024f9d0 / 001f9690 / 002536b0 with 00252080 direction
//! avoidance, the racer jump hooks 0024ebb0 / 001ebb10 and 001e9650 clip
//! eligibility. Physics belongs to Vehicle.
use crate::math::Det;
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
/// Race creation (`001f9690`) AI flags that depend on the race settings.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Expert difficulty. Race prep stores Expert as settings +0x610 = 3
    /// (`002f64a0`), and race creation then leaves out AI flag 0x400: no
    /// ahead-of-the-player speed limit (`002536b0`). Normal and Easy set it.
    pub expert: bool,
    /// EATDIRT "Aggressive AI" cheat (settings+0x618 → AI flag 0x4000000).
    pub aggressive: bool,
    /// BRINGIT "All Out AI" cheat (settings+0x1880 → AI flag 0x800): the
    /// driver also pulls the quad's velocity toward its target (`001ebaf0`).
    pub all_out: bool,
}
impl Options {
    /// Event difficulty index as race prep reads it (`002f64a0`): 2 Expert
    /// (settings +0x610 = 3); 0 and 1 are Normal (+0x610 = 2).
    pub fn difficulty(difficulty: u32) -> Self {
        Self {
            expert: difficulty == 2,
            ..Self::default()
        }
    }
}
/// All Out AI (flag 0x800) request for this frame: the racer blends its
/// horizontal velocity 0.5% toward `velocity` while grounded (`001ebaf0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectVelocity {
    pub velocity: [f32; 2],
    pub grounded: bool,
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
    /// Lean-flick rate the jump hook supplies for a preload (0 when none).
    pub preload_flick: f32,
    #[serde(skip)]
    pub direct: Option<DirectVelocity>,
}
/// The rigid-body state the path driver reads: ATV CPUs, and the player's
/// ATV or motorcycle once it is driven after the finish.
#[derive(Clone, Copy, Debug)]
pub struct DriverPose {
    pub origin: [f32; 3],
    pub orientation: [f32; 4],
    pub speed_magnitude: f32,
    pub grounded: bool,
    pub riding: bool,
    /// Rider status +0x50 / +0x48 from the previous physics step, which the
    /// racer copies into AI flags 0x20000 / 0x10000 (`001eb380`).
    pub preload_available: bool,
    pub preload_armed: bool,
    /// Rider status +0x88 / +0x8c: PreloadMinAxisRate, PreloadMaxAxisRate.
    pub preload_rates: [f32; 2],
}
impl From<&VehicleFrame> for DriverPose {
    fn from(f: &VehicleFrame) -> Self {
        Self {
            origin: f.origin,
            orientation: f.orientation,
            speed_magnitude: f.speed_magnitude,
            grounded: f.grounded.iter().any(|v| *v),
            riding: f.wreck.phase == crate::wreck::Phase::Riding,
            preload_available: f.preload_available,
            preload_armed: f.preload_armed,
            preload_rates: [0.; 2],
        }
    }
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
    options: Options,
    random: crate::engine_audio::OriginalRandom,
    goal: f32,
    last_target: usize,
    lane: f32,
    lean: f32,
    trick_timer: f32,
    air_timer: f32,
    trick_attempted: bool,
    jump: JumpLatch,
}
impl Driver {
    pub fn new(index: usize, options: Options) -> Self {
        let mut random = crate::engine_audio::OriginalRandom::with_seed(97211 + index as u64 * 47);
        let goal = random.next_u31() as f32 * 4.656613e-10 * 12. - 6.;
        Self {
            options,
            random,
            goal,
            last_target: usize::MAX,
            lane: 0.,
            lean: 0.,
            trick_timer: 0.,
            air_timer: 0.,
            trick_attempted: false,
            jump: JumpLatch::default(),
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
        frame: &DriverPose,
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
        if !frame.riding {
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
                    preload_flick: 0.,
                    direct: None,
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
        // 00252080 reads the 0x10 response scale from the aim node (+0x278).
        let mut avoidance = crate::cpu_avoidance::direction(
            frame.origin,
            desired.map(|v| v * frame.speed_magnitude),
            context.peers,
            course.flags.get(aim.segment).copied().unwrap_or(0),
            context.scene,
            self.options.aggressive,
        )?;
        // RaceStatus +0xbc: the race station's node, which stays on the last
        // on-track node while the rider is off the track polygon.
        let node = context.station.map_or(p.segment, |s| s.segment);
        // AI flag 0x400000 tail: probe 20ft along the requested heading; the
        // nearest segment comes from the station window here instead of the
        // native whole-section search (`0017b6a0`).
        let probe = std::array::from_fn(|i| {
            frame.origin[i] + desired[i] * crate::cpu_avoidance::TRACK_EDGE_PROBE
        });
        let near = match context.station {
            Some(station) => course.project_near(probe, station),
            None => course.project(probe),
        };
        if !course.within(probe, near.segment)
            && !course.within(probe, course.next(near.segment))
            && let Some(velocity) = crate::cpu_avoidance::track_edge(
                avoidance.velocity,
                sub(course.points[course.next(node)], course.points[node]),
            )
        {
            avoidance.velocity = velocity;
            avoidance.track_edge = true;
        }
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
        let angle = dot(direction, right).atan2_d(dot(direction, horizontal));
        let mut steer = (5. * angle)
            .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
            .sin_d();
        let mut desired_speed = if on_track {
            course.target_speed(aim)
        } else {
            50.
        };
        // The ground route uses no corner/braking lean. The airborne feedback
        // reconstructs the ordinary high-clearance 0024f9d0 branch;
        // low-clearance terrain-normal feedback is separate from native stunt choice.
        let corrected = normalize([
            tangent[0] + (target[0] - frame.origin[0]) * 0.05,
            0.,
            tangent[2] + (target[2] - frame.origin[2]) * 0.05,
        ]);
        let airborne = !frame.grounded;
        let lean = if airborne && context.clearance > 20. {
            let desired = [
                corrected[0] * 0.9816,
                -f32::from_bits(0x3e436113),
                corrected[2] * 0.9816,
            ];
            let diff = sub(desired, forward);
            let desired_pitch = -0.5 * dot(diff, up).atan2_d(dot(diff, forward));
            let [w, x, y, z] = frame.orientation;
            let pitch = (2. * (w * x + y * z)).atan2_d(1. - 2. * (x * x + y * y));
            (1.5 * (pitch - desired_pitch))
                .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
                .sin_d()
        } else if airborne {
            low_clearance_lean(self.lean, context.terrain_normal, forward)
        } else if course.flags.get(node).copied().unwrap_or(0) & 8 != 0 {
            // Grounded on a flag-8 node (RaceStatus +0xbc): constructor +0x258.
            0.6
        } else {
            0.
        };
        // Racer hook 0024ebb0 (VehicleAIInterface +0x28) on the aim node.
        let aim_flags = course.flags.get(aim.segment).copied().unwrap_or(0);
        let (hook_steer, hook_lean) = jump_hook(
            &mut self.jump,
            JumpNode {
                flags: aim_flags,
                speed: course.speeds.get(aim.segment).copied().unwrap_or(-1.),
                preload_scale: course
                    .preload_scales
                    .get(aim.segment)
                    .copied()
                    .unwrap_or(0.),
            },
            JumpRider {
                speed: frame.speed_magnitude,
                preload_available: frame.preload_available,
                preload_armed: frame.preload_armed,
                clearance: context.clearance,
            },
            steer,
            lean,
        );
        steer = hook_steer;
        let mut lean = hook_lean.clamp(-1., 1.);
        // AI +0xf4 keeps this lean; the release below only changes the input.
        self.lean = lean;
        // Flag 0x400 is set for every difficulty except Expert.
        if !self.options.expert {
            desired_speed = leader_speed(
                desired_speed,
                player_speed,
                progress_difference,
                60.,
                course.leader_policy,
            );
        }
        let (throttle, brake) = pedals(frame.speed_magnitude, desired_speed);
        let direct = self.options.all_out.then(|| DirectVelocity {
            velocity: [direction[0] * desired_speed, direction[2] * desired_speed],
            grounded: frame.grounded,
        });
        // Racer post-update 001ebb10: an armed preload on a flag-0x40 aim node
        // is released by easing the lean to -0.3 and supplying the lean-flick
        // rate that yields the chosen strength. The AI never measures a pad
        // flick, so every other frame supplies none.
        let mut preload_flick = 0.;
        if let Some(flick) = preload_release(
            frame.preload_armed,
            aim_flags,
            self.jump.preload_strength,
            frame.preload_rates,
        ) {
            preload_flick = flick;
            lean = -0.3;
        }
        let input = VehicleInput {
            throttle,
            brake,
            steer,
            lean,
            lean_flick_rate: Some(preload_flick),
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
                preload_flick,
                direct,
            },
        ))
    }
}
/// Racer +0x4c and the AI flags 0x8000 / 0x1000000 that `0024ebb0` keeps.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct JumpLatch {
    /// AI flag 0x8000: set on an ordinary node, read by a flag 2/4 drop.
    pub drop_ready: bool,
    /// AI flag 0x1000000: the preload strength for this jump is chosen.
    pub preload_chosen: bool,
    /// Racer +0x4c: the chosen preload strength.
    pub preload_strength: f32,
}
/// The aim node (AI +0x278) fields `0024ebb0` reads: flags (+4), TDF tag 18
/// target speed (+0x44, −1 = none) and tag 27 preload scale (+0x4c).
#[derive(Debug, Clone, Copy)]
pub struct JumpNode {
    pub flags: u32,
    pub speed: f32,
    pub preload_scale: f32,
}
/// The racer state `0024ebb0` reads: speed (AI +0xa0), rider status +0x50 /
/// +0x48 through AI flags 0x20000 / 0x10000, and the height above the
/// ground reference (AI +100 − +0x234).
#[derive(Debug, Clone, Copy)]
pub struct JumpRider {
    pub speed: f32,
    pub preload_available: bool,
    pub preload_armed: bool,
    pub clearance: f32,
}
/// Constructor `0024ef20` values: preload bias +0x264, the flag 4 / flag 2
/// clearance thresholds +0x26c / +0x270; race creation sets the gain +0x260.
const PRELOAD_BIAS: f32 = 0.;
const PRELOAD_GAIN: f32 = 0.05;
const DROP_CLEARANCE: [f32; 2] = [-2.5, -1.];
/// `0024ebb0` (VehicleAIInterface +0x28): drop and jump handling for the
/// aim node. A flag 2 / 4 node holds a lean −1, straight line while the
/// clearance stays above −1ft / −2.5ft and an ordinary node armed the latch;
/// below zero clearance it straightens (flag 2 also leans 0.3). No shipped
/// TDF uses flags 2 or 4. A flag 0x40 node with preload available (a rising
/// ramp face) chooses a strength once, bias + gain × (tag 27 × (target −
/// speed)) clamped to 0–1 with 150ft/s for an unset target, then leans 1
/// to arm the preload (unless already ≥ 0.9) and −1 once armed. Any other
/// node, or no preload available, re-arms the drop latch and clears the
/// choice. Returns (steer, lean).
pub fn jump_hook(
    latch: &mut JumpLatch,
    node: JumpNode,
    rider: JumpRider,
    mut steer: f32,
    mut lean: f32,
) -> (f32, f32) {
    let flags = node.flags;
    if flags & 6 != 0 {
        let threshold = DROP_CLEARANCE[usize::from(flags & 2 != 0)];
        if rider.clearance <= threshold || !latch.drop_ready {
            latch.drop_ready = false;
            if rider.clearance < 0. {
                steer = 0.;
                if flags & 2 != 0 {
                    lean = 0.3;
                }
            }
        } else {
            lean = -1.;
            steer = 0.;
        }
    } else if !rider.preload_available || flags & 0x40 == 0 {
        latch.drop_ready = true;
        latch.preload_chosen = false;
    } else {
        if !latch.preload_chosen {
            let target = if node.speed >= 0. { node.speed } else { 150. };
            let strength =
                PRELOAD_BIAS + PRELOAD_GAIN * (node.preload_scale * (target - rider.speed));
            latch.preload_strength = strength.clamp(0., 1.);
            latch.preload_chosen = true;
        }
        if rider.preload_armed {
            lean = -1.;
        } else if lean < 0.9 {
            lean = 1.;
        }
    }
    (steer, lean)
}
/// Racer post-update `001ebb10`: with the preload armed on a flag 0x40 aim
/// node the racer input leans −0.3 and supplies the lean-flick rate
/// strength × −(max − min) − min (rider status +0x8c / +0x88), which the
/// rider control turns back into that strength. The AI never measures a pad
/// flick; `001e7940` consumes and clears the rate every frame.
pub fn preload_release(armed: bool, aim_flags: u32, strength: f32, rates: [f32; 2]) -> Option<f32> {
    let [minimum, maximum] = rates;
    (armed && aim_flags & 0x40 != 0).then(|| strength * -(maximum - minimum) - minimum)
}
#[cfg(test)]
mod tests {
    use super::*;
    /// 100ft square loop; a rider at (0, 0, 50) on segment 0 aims 25ft ahead,
    /// still on segment 0.
    fn square(flags: u32, speed: f32) -> Course {
        Course {
            points: vec![
                [0., 0., 0.],
                [0., 0., 100.],
                [100., 0., 100.],
                [100., 0., 0.],
            ],
            widths: vec![30.; 4],
            flags: vec![flags, 0, 0, 0],
            speeds: vec![speed, speed, speed, speed],
            airtimes: vec![0.; 4],
            preload_scales: vec![1.; 4],
            ..Course::default()
        }
    }
    fn pose(speed: f32, available: bool, armed: bool) -> DriverPose {
        DriverPose {
            origin: [0., 0., 50.],
            orientation: [1., 0., 0., 0.],
            speed_magnitude: speed,
            grounded: true,
            riding: true,
            preload_available: available,
            preload_armed: armed,
            preload_rates: [1., 80.],
        }
    }
    fn drive(
        driver: &mut Driver,
        course: &Course,
        pose: &DriverPose,
        ahead: f32,
        clearance: f32,
    ) -> (VehicleInput, Decisions) {
        driver
            .step(
                course,
                pose,
                20.,
                ahead,
                1. / 60.,
                Context {
                    peers: &[],
                    scene: None,
                    clearance,
                    terrain_normal: [0., 1., 0.],
                    station: None,
                },
            )
            .unwrap()
    }
    #[test]
    fn expert_difficulty_drops_the_ahead_of_player_speed_limit() {
        let course = square(0, -1.);
        for (difficulty, expected) in [(0, 75.), (1, 75.), (2, 150.)] {
            let mut driver = Driver::new(0, Options::difficulty(difficulty));
            let (input, d) = drive(&mut driver, &course, &pose(100., false, false), 200., 1.);
            assert_eq!(d.desired_speed, expected, "difficulty {difficulty}");
            assert_eq!(input.brake, if expected < 100. { 1. } else { 0. });
        }
        // Behind the player both difficulties keep the TDF target.
        let mut normal = Driver::new(0, Options::difficulty(1));
        assert_eq!(
            drive(&mut normal, &course, &pose(100., false, false), -50., 1.)
                .1
                .desired_speed,
            150.
        );
    }
    #[test]
    fn track_edge_tail_acts_only_when_the_probe_leaves_the_track() {
        let course = square(0, -1.);
        let at = |x: f32| DriverPose {
            origin: [x, 0., 50.],
            ..pose(60., false, false)
        };
        let mut driver = Driver::new(0, Options::default());
        // On the road the 20ft probe stays inside the 30ft-wide polygon.
        let (_, d) = drive(&mut driver, &course, &at(0.), 0., 1.);
        assert!(!d.avoidance.track_edge);
        // Off the -x side the heading back (+x) crosses the +z node direction
        // positively and loses that cross product along it.
        let (_, d) = drive(&mut driver, &course, &at(-40.), 0., 1.);
        assert!(d.avoidance.track_edge);
        let [x, _, z] = d.avoidance.velocity;
        assert!(
            x > 0. && z < x,
            "steeper return: {:?}",
            d.avoidance.velocity
        );
        // The node direction is the race station's (RaceStatus +0xbc), not the
        // nearest segment: a station still on segment 3 (heading −x) uses
        // that direction for the same pose.
        let mut velocities = Vec::new();
        for segment in [0, 3] {
            let mut driver = Driver::new(0, Options::default());
            let (_, d) = driver
                .step(
                    &course,
                    &at(-40.),
                    20.,
                    0.,
                    1. / 60.,
                    Context {
                        peers: &[],
                        scene: None,
                        clearance: 1.,
                        terrain_normal: [0., 1., 0.],
                        station: Some(crate::race::Station {
                            segment,
                            fraction: 0.5,
                        }),
                    },
                )
                .unwrap();
            assert!(d.avoidance.track_edge && d.route_segment == 0, "{d:?}");
            velocities.push(d.avoidance.velocity);
        }
        assert_ne!(velocities[0], velocities[1]);
    }
    #[test]
    fn grounded_riders_lean_on_flag_eight_nodes_only() {
        let mut driver = Driver::new(0, Options::default());
        let (input, _) = drive(
            &mut driver,
            &square(8, -1.),
            &pose(60., false, false),
            0.,
            1.,
        );
        assert!((input.lean - 0.6).abs() < 1e-6);
        let (input, _) = drive(
            &mut driver,
            &square(0, -1.),
            &pose(60., false, false),
            0.,
            1.,
        );
        assert_eq!((input.lean, input.lean_flick_rate), (0., Some(0.)));
    }
    #[test]
    fn jump_hook_arms_then_releases_the_chosen_preload_strength() {
        let course = square(0x40, 80.);
        let mut driver = Driver::new(0, Options::default());
        // Not on a rising ramp face: nothing happens.
        let (input, _) = drive(&mut driver, &course, &pose(70., false, false), 0., 1.);
        assert_eq!((input.lean, input.lean_flick_rate), (0., Some(0.)));
        // Preload available: 0.05 × tag 27 × (80 − 70) = 0.5, lean back to arm.
        let (input, _) = drive(&mut driver, &course, &pose(70., true, false), 0., 1.);
        assert_eq!((input.lean, input.lean_flick_rate), (1., Some(0.)));
        // Armed: lean −0.3 with the flick that the rider control turns back
        // into strength (rate + min) / (−max + min) = 0.5. The strength was
        // chosen once, so the faster approach now does not change it.
        let (input, d) = drive(&mut driver, &course, &pose(95., true, true), 0., 1.);
        assert_eq!(input.lean, -0.3);
        let flick = input.lean_flick_rate.unwrap();
        assert_eq!(flick, d.preload_flick);
        assert!(((flick + 1.) / (-80. + 1.) - 0.5).abs() < 1e-6);
        // Leaving the jump resets the choice; at or above the node speed the
        // strength is zero and the flick (−min) cannot trigger a preload.
        drive(
            &mut driver,
            &square(0, 80.),
            &pose(95., true, false),
            0.,
            1.,
        );
        drive(&mut driver, &course, &pose(95., true, false), 0., 1.);
        let (input, _) = drive(&mut driver, &course, &pose(95., true, true), 0., 1.);
        assert_eq!(input.lean_flick_rate, Some(-1.));
        // The 150ft/s sentinel is the target for an unlimited jump node.
        let mut driver = Driver::new(0, Options::default());
        drive(
            &mut driver,
            &square(0x40, -1.),
            &pose(140., true, false),
            0.,
            1.,
        );
        let (input, _) = drive(
            &mut driver,
            &square(0x40, -1.),
            &pose(140., true, true),
            0.,
            1.,
        );
        assert!(((input.lean_flick_rate.unwrap() + 1.) / -79. - 0.5).abs() < 1e-6);
    }
    #[test]
    fn drop_nodes_need_an_ordinary_node_first_then_hold_a_straight_lean_back() {
        let mut driver = Driver::new(0, Options::default());
        let (input, _) = drive(
            &mut driver,
            &square(2, -1.),
            &pose(60., false, false),
            0.,
            1.,
        );
        assert_ne!(input.lean, -1.);
        drive(
            &mut driver,
            &square(0, -1.),
            &pose(60., false, false),
            0.,
            1.,
        );
        let (input, _) = drive(
            &mut driver,
            &square(2, -1.),
            &pose(60., false, false),
            0.,
            1.,
        );
        assert_eq!((input.lean, input.steer), (-1., 0.));
        // Below the terrain reference the latch clears: lean 0.3, no steering.
        let (input, _) = drive(
            &mut driver,
            &square(2, -1.),
            &pose(60., false, false),
            0.,
            -2.,
        );
        assert_eq!((input.lean, input.steer), (0.3, 0.));
        let (input, _) = drive(
            &mut driver,
            &square(2, -1.),
            &pose(60., false, false),
            0.,
            1.,
        );
        assert_ne!(input.lean, -1.);
    }
    #[test]
    fn all_out_ai_requests_the_target_velocity_only_with_the_cheat() {
        let course = square(0, -1.);
        let mut driver = Driver::new(0, Options::default());
        assert!(
            drive(&mut driver, &course, &pose(60., false, false), 0., 1.)
                .1
                .direct
                .is_none()
        );
        let mut driver = Driver::new(
            0,
            Options {
                all_out: true,
                ..Options::default()
            },
        );
        let (_, d) = drive(&mut driver, &course, &pose(60., false, false), 0., 1.);
        let direct = d.direct.unwrap();
        assert!(direct.grounded);
        let [x, z] = direct.velocity;
        assert!(((x * x + z * z).sqrt() - d.desired_speed).abs() < 1e-3);
        assert!(z > 0.);
    }
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
