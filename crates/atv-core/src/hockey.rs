//! The original Hockey mini-game on Coonce Arena (research/GAMES.md "Hockey").
//!
//! Exact-build evidence (private Ghidra exports of `SCUS_972.11`, its
//! `QuadzillaEventHockey.cpp` event, vtable `00460770`; the Coonce Arena
//! `game01so.toy`; bounded reading, no runtime traces):
//!
//! - Setup `00335590`: two teams (+0x1c4 + team * 0x38: rider count, score,
//!   per-rider goal counts, last toucher, riders). Every session rider joins
//!   the team picked on the Hockey Team Select page; the event has no CPU
//!   riders. The clock (+0x234) starts at the session time limit
//!   (settings +0x57c, 120 s on a new profile, `0021add0`). The goals are
//!   the scene's `goal1` and `goal2` objects.
//! - Face-off `00336c90`: the riders stand on a 30-unit semicircle around
//!   the rink centre (the goals' midpoint, grounded), team 0 on the goal-2
//!   half and team 1 on the goal-1 half, each facing the centre and held;
//!   the puck waits 5 units above the centre. After a 2 s countdown
//!   `00337150` drops it at a random point within 10 units of the centre,
//!   3 to 11 units up, with a random push of up to 6 units/s and a random
//!   spin, releases the riders and starts play.
//! - Per frame `00336200`: play (state 3) and sudden death (state 4) run
//!   the clock down. At zero, equal scores start Sudden Death (banner 5 s);
//!   otherwise "Game over!" (2 s), the end siren and crowd event 4, then a
//!   2.5 s pause ends the game (`003379a0`). Each puck contact with a rider
//!   records that rider as the last toucher of the rider's team; a puck
//!   moving at 13 units/s or more plays a puck hit.
//! - Goal `003368c0` (the `HockeyGoalTriggerAction` of the TOY's two Goal
//!   trigger boxes): the team whose attacking goal is nearer the puck
//!   scores (puck nearer goal 2 scores for team 1). If that team's last
//!   toucher is on it, the toucher's goal count rises and every third goal
//!   is a "Hat trick!" (3 s banner, crowd event 3, hat emitters, 6 s
//!   pause); otherwise crowd event 1 or 2 and a 4 s pause. Last touchers
//!   reset, the goal siren sounds, and the pause (state 2) ends in a new
//!   face-off while time remains, or ends the game in sudden death.
//! - Restart `003377f0` zeroes scores and goal counts and faces off again.
//!
//! The puck's motion in the original is a generic rigid body (`00337be0`:
//! `hockeypuck.psg` with its own collision hulls); [`Puck`] is an authored
//! flat-ice model of it (see research/GAMES.md).

use crate::math::Det;

/// `00336c90`: face-off semicircle radius.
pub const FACE_OFF_RADIUS: f32 = 30.0;
/// `00336c90` +0x23c: face-off countdown before the drop.
pub const FACE_OFF_SECONDS: f32 = 2.0;
/// `00336c90`: the held puck's height over the centre.
pub const FACE_OFF_PUCK_HEIGHT: f32 = 5.0;
/// `00337150`: the drop lands within this distance of the centre.
pub const DROP_SPREAD: f32 = 10.0;
/// `00337150`: drop height range above the centre.
pub const DROP_HEIGHT: (f32, f32) = (3.0, 8.0);
/// `00337150`: random push speed range.
pub const DROP_PUSH: f32 = 6.0;
/// `003368c0`: pause after a goal, and after a hat trick.
pub const GOAL_PAUSE: f32 = 4.0;
pub const HAT_TRICK_PAUSE: f32 = 6.0;
/// `00336200`: pause after "Game over!".
pub const GAME_OVER_PAUSE: f32 = 2.5;
/// `002e2cf0` banner times.
pub const SUDDEN_DEATH_BANNER: f32 = 5.0;
pub const GAME_OVER_BANNER: f32 = 2.0;
pub const HAT_TRICK_BANNER: f32 = 3.0;
pub const GOAL_BANNER: f32 = 4.0;
/// `00336200`: puck speed that plays a hit sound.
pub const PUCK_SOUND_SPEED: f32 = 13.0;
/// `0021add0`: a new profile's time limit (seconds).
pub const DEFAULT_TIME_LIMIT: f32 = 120.0;
/// Manual p. 18: time limits from 2 to 20 minutes.
pub const TIME_LIMITS: std::ops::RangeInclusive<u32> = 2..=20;

/// Team names (`RiderTex.txt` `Hockey_Red_Default`, `Hockey_Blue_Default`).
pub const TEAMS: [&str; 2] = ["Red", "Blue"];

/// Coonce Arena (`game01.scn`, `game01so.toy`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arena {
    /// Goal 1 and goal 2 trigger box centres (`[Trigger1]`, `[Trigger2]`).
    pub goals: [[f32; 3]; 2],
    /// Trigger half extents (`Box=7, 3.5, 1.5`).
    pub goal_box: [f32; 3],
    /// The ice surface height (`hockeyice.slt`).
    pub ice: f32,
    /// Boards: x and z extents of the rink and the corner chamfer
    /// (`hockeyrink01*` / `02*` placements).
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub chamfer: f32,
    /// Goal mouth half width in the end boards (`hockeyrink03*`/`04*`).
    pub mouth: f32,
    /// How far the goal pockets reach behind the end boards.
    pub pocket: f32,
    /// `[Ball] StartPosition`.
    pub puck_start: [f32; 3],
}

impl Arena {
    pub const COONCE: Arena = Arena {
        goals: [[384.0, 93.5, 437.5], [384.0, 93.5, 330.5]],
        goal_box: [7.0, 3.5, 1.5],
        ice: 90.1,
        min: [346.5, 334.0],
        max: [421.5, 434.0],
        chamfer: 10.0,
        mouth: 7.0,
        pocket: 7.5,
        puck_start: [383.0, 96.5, 386.0],
    };

    /// The rink centre: the goals' midpoint on the ice.
    pub fn centre(&self) -> [f32; 3] {
        let [a, b] = self.goals;
        [(a[0] + b[0]) * 0.5, self.ice, (a[2] + b[2]) * 0.5]
    }

    /// The goal (0 or 1) whose trigger box holds `p`.
    pub fn goal_at(&self, p: [f32; 3]) -> Option<usize> {
        self.goals
            .iter()
            .position(|g| (0..3).all(|i| (p[i] - g[i]).abs() <= self.goal_box[i]))
    }

    /// `003368c0`: the scoring team for a puck at `p`: team 1 when the
    /// puck is no farther from goal 2 than from goal 1.
    pub fn scoring_team(&self, p: [f32; 3]) -> usize {
        let d = |g: [f32; 3]| (0..3).map(|i| (p[i] - g[i]).powi(2)).sum::<f32>();
        usize::from(d(self.goals[1]) <= d(self.goals[0]))
    }

    /// `00336c90`: face-off spot and facing yaw (radians, 0 = +Z, the
    /// port's yaw convention) of rider `index` of `count` on `team`.
    pub fn face_off(&self, team: usize, index: usize, count: usize) -> ([f32; 3], f32) {
        let c = self.centre();
        let [a, b] = self.goals;
        let (ax, az) = (a[0] - b[0], a[2] - b[2]);
        let len = ax.hypot_d(az).max(1e-6);
        // The rink axis turned -90 degrees about +Y.
        let (ux, uz) = rotate_y(ax / len, az / len, -std::f32::consts::FRAC_PI_2);
        let theta = team as f32 * std::f32::consts::PI
            - std::f32::consts::PI / (count as f32 + 1.0) * (index as f32 + 1.0);
        let (dx, dz) = rotate_y(ux, uz, theta);
        let p = [
            c[0] + dx * FACE_OFF_RADIUS,
            c[1],
            c[2] + dz * FACE_OFF_RADIUS,
        ];
        // Facing the centre.
        let yaw = (-dx).atan2_d(-dz);
        (p, yaw)
    }
}

/// Right-handed rotation about +Y of the horizontal vector (x, z).
fn rotate_y(x: f32, z: f32, angle: f32) -> (f32, f32) {
    let (s, c) = angle.sin_cos_d();
    (x * c + z * s, -x * s + z * c)
}

/// Original event states (+0x238).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// 1: riders held at the face-off, puck over the centre.
    FaceOff { countdown: f32 },
    /// 3: the clock runs.
    Play,
    /// 4: tied at the end of time; the next goal wins.
    SuddenDeath,
    /// 2: after a goal or "Game over!".
    Pause { remaining: f32 },
    /// 0: the game ended (the HockeyOver menu).
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Banner {
    Goal { team: usize },
    HatTrick,
    SuddenDeath,
    GameOver,
}

impl Banner {
    pub fn text(self) -> String {
        match self {
            Banner::Goal { team } => format!("{} scores!", TEAMS[team]),
            Banner::HatTrick => "Hat trick!".into(),
            Banner::SuddenDeath => "Sudden Death!".into(),
            Banner::GameOver => "Game over!".into(),
        }
    }
}

/// What the presentation does on a tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Place every rider at its face-off spot and hold it; the puck waits
    /// over the centre (`00336c90`).
    FaceOff,
    /// Drop the puck and release the riders (`00337150`).
    Drop,
    /// A goal (`003368c0`): `scorer` is the credited rider, if any.
    Goal {
        team: usize,
        scorer: Option<usize>,
        hat_trick: bool,
    },
    /// `hockey.aud` event 1–4 (HCHEER01, HCHEER04, HOCKYWIN, HOCKYEND).
    Crowd(u8),
    /// `siren02.vag` (goal) or `gameover.vag`.
    Siren,
    GameOverSound,
    /// `puck1.vag`/`puck2.vag`.
    PuckHit,
    /// The game ended.
    Over,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub arena: Arena,
    /// Each rider's team.
    pub teams: Vec<usize>,
    /// Each rider's goals (+0x1cc + rider * 4).
    pub goals: Vec<u32>,
    /// Team scores (+0x1c8).
    pub scores: [u32; 2],
    /// Each team's last toucher (+0x1d8).
    pub last_touch: [Option<usize>; 2],
    /// Seconds left (+0x234).
    pub clock: f32,
    pub time_limit: f32,
    pub state: State,
    pub banner: Option<(Banner, f32)>,
}

impl Game {
    pub fn new(arena: Arena, teams: Vec<usize>, time_limit: f32) -> Self {
        let goals = vec![0; teams.len()];
        Self {
            arena,
            teams,
            goals,
            scores: [0; 2],
            last_touch: [None; 2],
            clock: time_limit,
            time_limit,
            state: State::FaceOff {
                countdown: FACE_OFF_SECONDS,
            },
            banner: None,
        }
    }

    /// `003377f0`: scores, goals and the clock reset; a new face-off.
    pub fn restart(&mut self) -> Vec<Event> {
        *self = Self::new(self.arena, std::mem::take(&mut self.teams), self.time_limit);
        vec![Event::FaceOff]
    }

    /// Riders on `team`, in slot order.
    pub fn team(&self, team: usize) -> impl Iterator<Item = usize> + '_ {
        self.teams
            .iter()
            .enumerate()
            .filter(move |(_, t)| **t == team)
            .map(|(i, _)| i)
    }

    /// Face-off spot of rider `rider`.
    pub fn face_off(&self, rider: usize) -> ([f32; 3], f32) {
        let team = self.teams[rider];
        let members: Vec<usize> = self.team(team).collect();
        let index = members.iter().position(|r| *r == rider).unwrap_or(0);
        self.arena.face_off(team, index, members.len())
    }

    /// Riders are held at the face-off.
    pub fn held(&self) -> bool {
        matches!(self.state, State::FaceOff { .. })
    }

    /// The puck is live (goals and touches count).
    pub fn live(&self) -> bool {
        matches!(self.state, State::Play | State::SuddenDeath)
    }

    /// The leading team, or None while tied.
    pub fn winner(&self) -> Option<usize> {
        match self.scores[0].cmp(&self.scores[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// A rider touched the puck (`00336200` contact list).
    pub fn touch(&mut self, rider: usize) {
        if self.live()
            && let Some(team) = self.teams.get(rider)
        {
            self.last_touch[*team] = Some(rider);
        }
    }

    /// `003368c0`: the puck entered a goal at `puck`. `cheer` picks crowd
    /// event 1 or 2 (the original's random pick).
    pub fn goal(&mut self, puck: [f32; 3], cheer: bool) -> Vec<Event> {
        if !self.live() {
            return Vec::new();
        }
        let team = self.arena.scoring_team(puck);
        let scorer = self.last_touch[team].filter(|r| self.teams.get(*r) == Some(&team));
        let mut hat_trick = false;
        if let Some(r) = scorer {
            self.goals[r] += 1;
            hat_trick = self.goals[r].is_multiple_of(3);
        }
        self.scores[team] += 1;
        self.last_touch = [None; 2];
        let mut events = vec![Event::Goal {
            team,
            scorer,
            hat_trick,
        }];
        let pause = if hat_trick {
            self.banner = Some((Banner::HatTrick, HAT_TRICK_BANNER));
            events.push(Event::Crowd(3));
            HAT_TRICK_PAUSE
        } else {
            self.banner = Some((Banner::Goal { team }, GOAL_BANNER));
            events.push(Event::Crowd(if cheer { 2 } else { 1 }));
            GOAL_PAUSE
        };
        events.push(Event::Siren);
        self.state = State::Pause { remaining: pause };
        events
    }

    /// `00336200` per frame; `puck_speed` plays the hit sound.
    pub fn tick(&mut self, dt: f32, puck_speed: f32, touched: bool) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some((_, t)) = &mut self.banner {
            *t -= dt;
            if *t <= 0. {
                self.banner = None;
            }
        }
        match self.state {
            State::Over => {}
            State::FaceOff { countdown } => {
                let left = countdown - dt;
                if left <= 0. {
                    self.state = State::Play;
                    events.push(Event::Drop);
                } else {
                    self.state = State::FaceOff { countdown: left };
                }
            }
            State::Pause { remaining } => {
                if remaining > 0. {
                    self.state = State::Pause {
                        remaining: remaining - dt,
                    };
                } else if self.clock > 0. {
                    self.state = State::FaceOff {
                        countdown: FACE_OFF_SECONDS,
                    };
                    events.push(Event::FaceOff);
                } else {
                    self.state = State::Over;
                    events.push(Event::Over);
                }
            }
            State::Play | State::SuddenDeath => {
                self.clock -= dt;
                if self.clock <= 0. {
                    self.clock = 0.;
                    if self.scores[0] == self.scores[1] {
                        if self.state != State::SuddenDeath {
                            self.state = State::SuddenDeath;
                            self.banner = Some((Banner::SuddenDeath, SUDDEN_DEATH_BANNER));
                        }
                    } else {
                        self.banner = Some((Banner::GameOver, GAME_OVER_BANNER));
                        events.push(Event::GameOverSound);
                        events.push(Event::Crowd(4));
                        self.state = State::Pause {
                            remaining: GAME_OVER_PAUSE,
                        };
                    }
                }
                if touched && puck_speed >= PUCK_SOUND_SPEED {
                    events.push(Event::PuckHit);
                }
            }
        }
        events
    }
}

/// The puck: an authored flat-ice body standing in for the original's
/// rigid body (research/GAMES.md "Puck model").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puck {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub spin: f32,
    pub yaw: f32,
}

/// The mode TOY's `[HatTrickEmitters]` (`00335de0`): `NumberOfEmitters`
/// (EmitterNumber, InstanceNumber) pairs, numbered within that TOY (the
/// loader adds the TOY's emitter base +0x3e4). `003360f0` gathers them into
/// a ParticleGroupController; setup (`00335590`) deactivates it, a hat
/// trick (`003368c0`) starts it (`0020eb90`) and the pause's end
/// (`00336200`) and Restart Game (`003377f0`) deactivate it (`0020ec20`).
pub fn hat_trick_emitters(toy: &str) -> Vec<(usize, usize)> {
    let toy = crate::games::Toy::parse(toy);
    let count = toy
        .int("HatTrickEmitters", "NumberOfEmitters")
        .unwrap_or(0)
        .clamp(0, 64);
    (1..=count)
        .filter_map(|n| {
            let section = format!("HatTrickEmitter{n}");
            let number = |key| {
                toy.get(&section, key)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0)
            };
            toy.get(&section, "EmitterNumber")
                .is_some()
                .then(|| (number("EmitterNumber"), number("InstanceNumber")))
        })
        .collect()
}

/// Puck radius (authored).
pub const PUCK_RADIUS: f32 = 1.5;
/// Board restitution (authored; the body's 0.7 contact value).
pub const RESTITUTION: f32 = 0.7;
/// Ice slowdown (units/s^2, authored).
pub const ICE_DRAG: f32 = 3.0;
/// The ATV's contact radius around its centre (authored).
pub const ATV_RADIUS: f32 = 3.5;
const GRAVITY: f32 = 32.174;

impl Puck {
    /// `00336c90`: held over the centre.
    pub fn held(arena: &Arena) -> Self {
        let c = arena.centre();
        Self {
            position: [c[0], c[1] + FACE_OFF_PUCK_HEIGHT, c[2]],
            velocity: [0.; 3],
            spin: 0.,
            yaw: 0.,
        }
    }

    /// `00337150`: dropped near the centre; `r` yields uniform [0, 1).
    pub fn drop(arena: &Arena, mut r: impl FnMut() -> f32) -> Self {
        let c = arena.centre();
        let angle = r() * std::f32::consts::TAU;
        let (dx, dz) = (angle.sin_d(), angle.cos_d());
        let spread = r() * DROP_SPREAD;
        let height = DROP_HEIGHT.0 + r() * DROP_HEIGHT.1;
        let push = r() * DROP_PUSH;
        Self {
            position: [c[0] - dx * spread, c[1] + height, c[2] - dz * spread],
            velocity: [-dx * push, 0., -dz * push],
            spin: r() * 2.0,
            yaw: 0.,
        }
    }

    pub fn speed(&self) -> f32 {
        self.velocity.iter().map(|v| v * v).sum::<f32>().sqrt()
    }

    /// One step against the boards and the riders `(centre, velocity)`.
    /// Returns which riders touched it.
    pub fn step(&mut self, arena: &Arena, riders: &[([f32; 3], [f32; 3])], dt: f32) -> Vec<usize> {
        let mut touched = Vec::new();
        // Gravity onto the ice.
        let rest = arena.ice + 0.2;
        if self.position[1] > rest || self.velocity[1] > 0. {
            self.velocity[1] -= GRAVITY * dt;
        }
        for i in 0..3 {
            self.position[i] += self.velocity[i] * dt;
        }
        if self.position[1] <= rest {
            self.position[1] = rest;
            if self.velocity[1] < -4. {
                self.velocity[1] = -self.velocity[1] * 0.3;
            } else {
                self.velocity[1] = 0.;
            }
        }
        // Ice drag.
        let h = self.velocity[0].hypot_d(self.velocity[2]);
        if h > 0. {
            let k = ((h - ICE_DRAG * dt).max(0.)) / h;
            self.velocity[0] *= k;
            self.velocity[2] *= k;
        }
        self.yaw += self.spin * dt;
        // Riders push the puck (the ATV as an infinite mass).
        for (index, (centre, velocity)) in riders.iter().enumerate() {
            let (dx, dz) = (self.position[0] - centre[0], self.position[2] - centre[2]);
            let dist = dx.hypot_d(dz);
            let reach = ATV_RADIUS + PUCK_RADIUS;
            if dist >= reach || (self.position[1] - centre[1]).abs() > 6. {
                continue;
            }
            let (nx, nz) = if dist > 1e-4 {
                (dx / dist, dz / dist)
            } else {
                (0., 1.)
            };
            self.position[0] = centre[0] + nx * reach;
            self.position[2] = centre[2] + nz * reach;
            let rel = (self.velocity[0] - velocity[0]) * nx + (self.velocity[2] - velocity[2]) * nz;
            if rel < 0. {
                let j = -(1. + RESTITUTION) * rel;
                self.velocity[0] += j * nx;
                self.velocity[2] += j * nz;
            }
            touched.push(index);
        }
        self.boards(arena);
        touched
    }

    /// Bounces off the boards; the goal mouths open into the pockets.
    fn boards(&mut self, arena: &Arena) {
        let r = PUCK_RADIUS;
        let [x, _, z] = self.position;
        let in_mouth = (x - arena.centre()[0]).abs() <= arena.mouth - r;
        let (min_z, max_z) = if in_mouth {
            (arena.min[1] - arena.pocket, arena.max[1] + arena.pocket)
        } else {
            (arena.min[1], arena.max[1])
        };
        let bounce = |p: &mut f32, v: &mut f32, lo: f32, hi: f32| {
            if *p < lo + r {
                *p = lo + r;
                if *v < 0. {
                    *v = -*v * RESTITUTION;
                }
            } else if *p > hi - r {
                *p = hi - r;
                if *v > 0. {
                    *v = -*v * RESTITUTION;
                }
            }
        };
        let (mut px, mut vx) = (self.position[0], self.velocity[0]);
        // A puck inside a pocket is held between the posts.
        let (lo_x, hi_x) = if z > arena.max[1] || z < arena.min[1] {
            (
                arena.centre()[0] - arena.mouth,
                arena.centre()[0] + arena.mouth,
            )
        } else {
            (arena.min[0], arena.max[0])
        };
        bounce(&mut px, &mut vx, lo_x, hi_x);
        let (mut pz, mut vz) = (self.position[2], self.velocity[2]);
        bounce(&mut pz, &mut vz, min_z, max_z);
        self.position[0] = px;
        self.position[2] = pz;
        self.velocity[0] = vx;
        self.velocity[2] = vz;
        // Corner chamfers: the boards cut each corner at 45 degrees.
        for (sx, cx) in [(1., arena.max[0]), (-1., arena.min[0])] {
            for (sz, cz) in [(1., arena.max[1]), (-1., arena.min[1])] {
                // Plane through (cx - sx*chamfer, cz) and (cx, cz - sz*chamfer).
                let (nx, nz) = (
                    sx * std::f32::consts::FRAC_1_SQRT_2,
                    sz * std::f32::consts::FRAC_1_SQRT_2,
                );
                let d0 = (cx - sx * arena.chamfer) * nx + cz * nz;
                let d = self.position[0] * nx + self.position[2] * nz - d0 + r;
                if d > 0. && !(in_mouth && (self.position[2] - cz) * sz > -r) {
                    self.position[0] -= nx * d;
                    self.position[2] -= nz * d;
                    let vn = self.velocity[0] * nx + self.velocity[2] * nz;
                    if vn > 0. {
                        self.velocity[0] -= (1. + RESTITUTION) * vn * nx;
                        self.velocity[2] -= (1. + RESTITUTION) * vn * nz;
                    }
                }
            }
        }
    }
}

/// An authored CPU skater for single-human play (research/GAMES.md): the
/// original event has no CPU riders. Steers to a spot behind the puck on
/// the line to the goal it attacks.
pub fn cpu_target(arena: &Arena, team: usize, puck: [f32; 3]) -> [f32; 3] {
    // Team 0 attacks goal 1 (index 0), team 1 attacks goal 2 (index 1).
    let goal = arena.goals[if team == 0 { 0 } else { 1 }];
    let (dx, dz) = (goal[0] - puck[0], goal[2] - puck[2]);
    let len = dx.hypot_d(dz).max(1e-3);
    [puck[0] - dx / len * 4., puck[1], puck[2] - dz / len * 4.]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(teams: &[usize]) -> Game {
        Game::new(Arena::COONCE, teams.to_vec(), DEFAULT_TIME_LIMIT)
    }

    #[test]
    fn hat_trick_emitters_parse_their_toy_local_numbers() {
        let toy = "[ParticleEmitters]\nNumberOfEmitters=2\n[HatTrickEmitters]\nNumberOfEmitters=3\n\
                   [HatTrickEmitter1]\nEmitterNumber=1\nInstanceNumber=1\n\
                   [HatTrickEmitter2]\nEmitterNumber=4 ; east\nInstanceNumber=2\n";
        // The third entry is absent and skipped.
        assert_eq!(hat_trick_emitters(toy), [(1, 1), (4, 2)]);
        assert!(hat_trick_emitters("[Triggers]\nNumberOfTriggers=0\n").is_empty());
    }

    /// Private probe: Coonce Arena's `[HatTrickEmitters]` name its four
    /// type9 `hats*.emt` emitters, which load, stay off, and throw hats of
    /// the hat atlas group once started. Skips without the disc files.
    #[test]
    fn coonce_hat_trick_emitters_resolve() {
        use crate::ambient_effects::{Effects, ParticleAction, Source, Toy};
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../private/extracted/TRACKS/TRACKSA/GAME01/G01_SO.RES");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skipped: G01_SO.RES is not extracted");
            return;
        };
        let archive = crate::resource::Resource::parse(&bytes).unwrap();
        let member = |name: &str| {
            let i = archive
                .members
                .iter()
                .position(|m| m.name.to_ascii_lowercase().ends_with(name))
                .unwrap_or_else(|| panic!("{name}"));
            String::from_utf8_lossy(&archive.extract(i).unwrap()).into_owned()
        };
        let mode = member("game01so.toy");
        let hats = hat_trick_emitters(&mode);
        assert_eq!(hats, [(1, 1), (2, 1), (3, 1), (4, 1)]);
        let mut source = Source {
            scene: "[StaticModels]\nNumberOfStaticModels=0\n".into(),
            common: member("game01.toy"),
            mode: Some(mode),
            targets: None,
            prt: member("game01particles.prt"),
            emts: Default::default(),
        };
        for name in ["hatse.emt", "hatsn.emt", "hatss.emt"] {
            source.emts.insert(name.into(), member(name));
        }
        let (mut effects, metadata) = Effects::from_source(&source).unwrap();
        assert!(
            metadata.unsupported.is_empty(),
            "{:?}",
            metadata.unsupported
        );
        assert_eq!(metadata.cpu_active, 4);
        let action = |on| {
            hats.iter().map(move |&(emitter, instance)| ParticleAction {
                emitter,
                instance,
                on,
            })
        };
        for a in action(false) {
            assert!(effects.trigger(Toy::Mode, a, None));
        }
        for _ in 0..120 {
            effects.step(1. / 60.).unwrap();
        }
        assert_eq!(effects.frame().total_emitted, 0, "off until a hat trick");
        for a in action(true) {
            assert!(effects.trigger(Toy::Mode, a, None));
        }
        for _ in 0..120 {
            effects.step(1. / 60.).unwrap();
        }
        let frame = effects.frame();
        assert!(frame.total_emitted >= 4, "{}", frame.total_emitted);
        assert!(
            frame
                .particles
                .iter()
                .all(|p| (224..=231).contains(&p.frame))
        );
    }

    fn run(g: &mut Game, seconds: f32) -> Vec<Event> {
        let mut all = Vec::new();
        let steps = (seconds * 60.).round() as usize;
        for _ in 0..steps {
            all.extend(g.tick(1. / 60., 0., false));
        }
        all
    }

    #[test]
    fn faces_off_for_two_seconds_then_drops() {
        let mut g = game(&[0, 1]);
        assert!(g.held());
        let events = run(&mut g, 1.9);
        assert!(events.is_empty());
        let events = run(&mut g, 0.2);
        assert_eq!(events, vec![Event::Drop]);
        assert!(g.live());
    }

    #[test]
    fn face_off_semicircles_put_team_zero_on_goal_two_half() {
        let a = Arena::COONCE;
        let c = a.centre();
        assert_eq!(c, [384.0, 90.1, 384.0]);
        let (p0, yaw0) = a.face_off(0, 0, 1);
        let (p1, _) = a.face_off(1, 0, 1);
        assert!(
            (p0[2] - 354.).abs() < 1e-3 && (p0[0] - 384.).abs() < 1e-3,
            "{p0:?}"
        );
        assert!((p1[2] - 414.).abs() < 1e-3, "{p1:?}");
        // Facing the centre (+Z).
        assert!(yaw0.abs() < 1e-4, "{yaw0}");
        // Two riders spread across the half at 30 units.
        let (q0, _) = a.face_off(0, 0, 2);
        let (q1, _) = a.face_off(0, 1, 2);
        for q in [q0, q1] {
            assert!(((q[0] - c[0]).hypot_d(q[2] - c[2]) - FACE_OFF_RADIUS).abs() < 1e-3);
            assert!(q[2] < c[2]);
        }
        assert!((q0[0] - q1[0]).abs() > 20.);
    }

    #[test]
    fn nearer_goal_two_scores_for_team_one() {
        let a = Arena::COONCE;
        assert_eq!(a.scoring_team(a.goals[1]), 1);
        assert_eq!(a.scoring_team(a.goals[0]), 0);
        assert_eq!(a.goal_at([384., 92., 437.]), Some(0));
        assert_eq!(a.goal_at([384., 92., 331.]), Some(1));
        assert_eq!(a.goal_at([384., 92., 384.]), None);
    }

    #[test]
    fn goal_credits_own_team_toucher_and_pauses() {
        let mut g = game(&[0, 1]);
        run(&mut g, 2.1);
        g.touch(0);
        let events = g.goal(g.arena.goals[0], false);
        assert_eq!(
            events,
            vec![
                Event::Goal {
                    team: 0,
                    scorer: Some(0),
                    hat_trick: false
                },
                Event::Crowd(1),
                Event::Siren
            ]
        );
        assert_eq!(g.scores, [1, 0]);
        assert_eq!(g.goals, vec![1, 0]);
        assert_eq!(g.last_touch, [None, None]);
        // A pause of 4 s, then a new face-off.
        assert!(run(&mut g, 3.9).is_empty());
        assert_eq!(run(&mut g, 0.2), vec![Event::FaceOff]);
        assert!(g.held());
    }

    #[test]
    fn own_goal_scores_for_the_other_team_without_a_scorer() {
        let mut g = game(&[0, 1]);
        run(&mut g, 2.1);
        // Rider 0 (team 0) pushes it into goal 2: team 1 scores, nobody credited.
        g.touch(0);
        let events = g.goal(g.arena.goals[1], true);
        assert_eq!(
            events[0],
            Event::Goal {
                team: 1,
                scorer: None,
                hat_trick: false
            }
        );
        assert_eq!(events[1], Event::Crowd(2));
        assert_eq!(g.goals, vec![0, 0]);
    }

    #[test]
    fn every_third_goal_is_a_hat_trick() {
        let mut g = game(&[0, 1]);
        for n in 1..=3 {
            run(&mut g, 2.1);
            g.touch(0);
            let events = g.goal(g.arena.goals[0], false);
            let hat = n == 3;
            assert_eq!(
                events[0],
                Event::Goal {
                    team: 0,
                    scorer: Some(0),
                    hat_trick: hat
                }
            );
            if hat {
                assert_eq!(events[1], Event::Crowd(3));
                assert_eq!(g.banner, Some((Banner::HatTrick, HAT_TRICK_BANNER)));
                assert_eq!(
                    g.state,
                    State::Pause {
                        remaining: HAT_TRICK_PAUSE
                    }
                );
            }
            run(&mut g, if hat { 6.1 } else { 4.1 });
        }
    }

    #[test]
    fn goals_do_not_count_outside_play() {
        let mut g = game(&[0, 1]);
        assert!(g.goal(g.arena.goals[0], false).is_empty());
        run(&mut g, 2.1);
        g.goal(g.arena.goals[0], false);
        // During the pause the puck may still be in the net.
        assert!(g.goal(g.arena.goals[0], false).is_empty());
        assert_eq!(g.scores, [1, 0]);
    }

    #[test]
    fn time_out_with_a_lead_ends_after_game_over() {
        let mut g = Game::new(Arena::COONCE, vec![0, 1], 10.);
        run(&mut g, 2.1);
        g.touch(1);
        g.goal(g.arena.goals[1], false);
        run(&mut g, 4.1); // pause, face-off
        let events = run(&mut g, 2.1 + 10.);
        assert!(events.contains(&Event::GameOverSound));
        assert!(events.contains(&Event::Crowd(4)));
        assert_eq!(g.banner.map(|b| b.0), Some(Banner::GameOver));
        let events = run(&mut g, 2.6);
        assert_eq!(events, vec![Event::Over]);
        assert_eq!(g.state, State::Over);
        assert_eq!(g.winner(), Some(1));
    }

    #[test]
    fn tie_at_time_out_goes_to_sudden_death_and_next_goal_wins() {
        let mut g = Game::new(Arena::COONCE, vec![0, 1], 5.);
        run(&mut g, 2.1 + 5.1);
        assert_eq!(g.state, State::SuddenDeath);
        assert_eq!(g.banner.map(|b| b.0), Some(Banner::SuddenDeath));
        assert_eq!(g.clock, 0.);
        // Sudden death keeps going.
        assert!(run(&mut g, 30.).is_empty());
        g.touch(0);
        g.goal(g.arena.goals[0], false);
        let events = run(&mut g, 4.1);
        assert_eq!(events, vec![Event::Over]);
        assert_eq!(g.winner(), Some(0));
    }

    #[test]
    fn clock_stops_during_the_goal_pause_and_face_off() {
        // A goal with 0.1 s left: the pause and face-off keep the clock,
        // and play resumes for the remaining time (00336200 state 2/1).
        let mut g = Game::new(Arena::COONCE, vec![0, 1], 3.);
        run(&mut g, 2.1 + 2.9);
        let left = g.clock;
        assert!(left > 0. && left < 0.2, "{left}");
        g.goal(g.arena.goals[0], false);
        let events = run(&mut g, 4.1);
        assert_eq!(events, vec![Event::FaceOff]);
        assert_eq!(g.clock, left);
        run(&mut g, 1.9);
        assert_eq!(g.clock, left);
        let events = run(&mut g, 0.4);
        assert!(events.contains(&Event::GameOverSound));
    }

    #[test]
    fn restart_resets_scores_and_faces_off() {
        let mut g = game(&[0, 1, 0]);
        run(&mut g, 2.1);
        g.touch(2);
        g.goal(g.arena.goals[0], false);
        assert_eq!(g.restart(), vec![Event::FaceOff]);
        assert_eq!(g.scores, [0, 0]);
        assert_eq!(g.goals, vec![0, 0, 0]);
        assert_eq!(g.clock, DEFAULT_TIME_LIMIT);
        assert!(g.held());
        assert_eq!(g.teams, vec![0, 1, 0]);
    }

    #[test]
    fn puck_hit_sound_needs_thirteen_units_per_second() {
        let mut g = game(&[0, 1]);
        run(&mut g, 2.1);
        assert!(g.tick(1. / 60., 12.9, true).is_empty());
        assert_eq!(g.tick(1. / 60., 13.0, true), vec![Event::PuckHit]);
        assert!(g.tick(1. / 60., 20.0, false).is_empty());
    }

    #[test]
    fn dropped_puck_lands_near_the_centre_and_slides_to_rest() {
        let a = Arena::COONCE;
        let mut seq = [0.25f32, 0.5, 0.5, 1.0, 0.5].into_iter().cycle();
        let mut p = Puck::drop(&a, || seq.next().unwrap());
        let c = a.centre();
        assert!((p.position[0] - c[0]).hypot_d(p.position[2] - c[2]) <= DROP_SPREAD);
        assert!(p.position[1] >= c[1] + 3. && p.position[1] <= c[1] + 11.);
        for _ in 0..600 {
            p.step(&a, &[], 1. / 60.);
        }
        assert!((p.position[1] - (a.ice + 0.2)).abs() < 1e-3);
        assert!(p.speed() < 1e-3);
    }

    #[test]
    fn atv_pushes_puck_and_boards_bounce_it() {
        let a = Arena::COONCE;
        let mut p = Puck::held(&a);
        p.position = [400., a.ice + 0.2, 384.];
        // An ATV at 20 units/s driving into it along +X.
        let touched = p.step(&a, &[([397., a.ice + 1., 384.], [20., 0., 0.])], 1. / 60.);
        assert_eq!(touched, vec![0]);
        assert!(p.velocity[0] > 30., "{:?}", p.velocity);
        for _ in 0..120 {
            p.step(&a, &[], 1. / 60.);
        }
        // It bounced back off the side boards.
        assert!(p.velocity[0] < 0.);
        assert!(p.position[0] <= a.max[0] - PUCK_RADIUS + 1e-3);
    }

    #[test]
    fn puck_can_enter_the_goal_mouth_but_not_the_end_boards() {
        let a = Arena::COONCE;
        let mut p = Puck::held(&a);
        p.position = [384., a.ice + 0.2, 425.];
        p.velocity = [0., 0., 30.];
        let mut scored = false;
        for _ in 0..60 {
            p.step(&a, &[], 1. / 60.);
            scored |= a.goal_at(p.position) == Some(0);
        }
        assert!(scored);
        let mut q = Puck::held(&a);
        q.position = [365., a.ice + 0.2, 425.];
        q.velocity = [0., 0., 30.];
        for _ in 0..60 {
            q.step(&a, &[], 1. / 60.);
            assert!(a.goal_at(q.position).is_none());
            assert!(q.position[2] <= a.max[1]);
        }
    }

    #[test]
    fn cpu_target_sits_behind_the_puck_toward_its_goal() {
        let a = Arena::COONCE;
        let puck = [384., a.ice, 384.];
        // Team 0 attacks goal 1 (+Z): it lines up on the -Z side.
        assert!(cpu_target(&a, 0, puck)[2] < puck[2]);
        assert!(cpu_target(&a, 1, puck)[2] > puck[2]);
    }
}
