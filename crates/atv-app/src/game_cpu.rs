//! **Port adaptation, not recovered behavior.** Authored CPU riders in the
//! local mini-games (research/GAMES.md "CPU riders"): Games Prep's "CPU
//! Riders" row adds one to three. Each is a whole player ride, loaded like a
//! split screen player without a view, and the game sessions take it as one
//! more local rider after the humans, so it scores through the same rules
//! and standings. `atv_core::game_cpu` chooses its controls.
//!
//! Solo and split screen only: an online room has no CPU riders.
//!
//! A free ride takes the same riders with Options → Game Settings "Free Ride CPU
//! riders" (research/CPU_MINIGAMES.md "Free roam"): with no game to play
//! they roam the venue (`atv_core::roam_cpu`), on no standings.
use crate::games::GameSession;
use crate::hockey::HockeySession;
use crate::riding::Riding;
use crate::tag::TagSession;
use atv_core::game_cpu::{self as brain, Driver, Goal, HillRun, Other, Recover, Rng, Sense, Skill};
use atv_core::games::GameKind;
use atv_core::math::Det;
use atv_core::roam_cpu::{self as roam, Roam};
use atv_game::multiplayer::PeerBody;
use atv_game::ride::{Controls, Ride};

/// Games Prep's CPU Riders and the race difficulty they ride at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub count: u8,
    /// The race setup's difficulty (1 Normal, 2 Expert, 3 Kid).
    pub difficulty: u32,
    /// A free ride's riders (Game Settings' Free Ride CPU riders): they roam.
    pub roam: bool,
}

impl Request {
    /// A free ride's riders as Game Settings → Free Ride CPU riders saves
    /// them, at Normal (the race driver's 50 ft/s off its line).
    pub fn free_ride(settings: &atv_ui::settings::Settings) -> Self {
        Self {
            count: atv_ui::menu::prefs::free_ride_cpus(settings),
            difficulty: 1,
            roam: true,
        }
    }
}

/// The CPU riders' names on standings and results.
pub const NAMES: [&str; 3] = ["CPU 1", "CPU 2", "CPU 3"];

/// Every local rider's name in slot order: the humans (one player keeps
/// `solo`), then the CPUs.
pub fn names(humans: usize, cpus: usize, solo: &str) -> Vec<String> {
    (0..humans)
        .map(|i| {
            if humans == 1 {
                solo.to_string()
            } else {
                crate::split::player_name(i as u8 + 1).to_string()
            }
        })
        .chain(NAMES.iter().take(cpus).map(|n| n.to_string()))
        .collect()
}

pub struct Cpu {
    /// The rider's player number (scene slot `split::peer(player)`; rider
    /// slot `player - 1`).
    pub player: u8,
    pub riding: Riding,
    driver: Driver,
    hill: HillRun,
    roam: Roam,
    /// Diagnostics: wrecks so far, and whether it is wrecked now.
    wrecks: u32,
    wrecked: bool,
    /// A roamer's seconds sitting still, and its returns to the start.
    still: f32,
    stalls: u32,
    rng: Rng,
    seed: u32,
    skill: Skill,
    /// King of the Hills: a landed stunt's points for the game's next
    /// sample (the rules decide whether they capture).
    pub landed: Option<f32>,
    /// The last goal's ground target (a stuck rider turns toward it).
    target: Option<[f32; 2]>,
    /// Wrecks so far and whether the rider is off now (impact voices).
    crash: (u32, bool),
}

/// What every CPU reads of the game this frame.
pub enum Mode {
    /// No game: hold still.
    Idle,
    /// A free ride: roam this venue.
    Roam(std::rc::Rc<roam::Map>),
    Tag {
        carrier: Option<usize>,
        ball: [f32; 3],
        ball_velocity: [f32; 3],
        min: [f32; 2],
        max: [f32; 2],
    },
    Treasure(Vec<[f32; 3]>),
    Hills(Vec<brain::Hill>),
    Hockey {
        teams: Vec<usize>,
        arena: atv_core::hockey::Arena,
        puck: [f32; 3],
        puck_velocity: [f32; 3],
    },
}

pub struct World {
    pub mode: Mode,
    /// Every rider in slot order (humans, then CPUs).
    pub riders: Vec<Other>,
}

fn other(ride: &Ride) -> Other {
    let f = ride.frame();
    Other {
        position: f.origin,
        velocity: f.linear_velocity,
    }
}

impl World {
    /// The active local game as the CPUs see it; `rides` in slot order.
    pub fn of<'a>(
        tag: Option<&TagSession>,
        hockey: Option<&HockeySession>,
        game: Option<&GameSession>,
        rides: impl Iterator<Item = &'a Ride>,
        terrain: &atv_core::terrain::Terrain,
        roam: Option<&std::rc::Rc<roam::Map>>,
    ) -> Self {
        let riders = rides.map(other).collect();
        let mode = if let Some(tag) = tag.filter(|t| t.local) {
            let bounds = atv_core::tag::Bounds::from_cells(
                terrain.width_cells,
                terrain.depth_cells,
                terrain.spacing,
            );
            let margin = if tag.crowd {
                0.
            } else {
                atv_core::tag::BOUNDARY_CELLS
            };
            Mode::Tag {
                carrier: tag.game.carrier,
                ball: tag.ball.position,
                ball_velocity: tag.ball.velocity,
                min: [margin * bounds.cell[0], margin * bounds.cell[1]],
                max: [
                    bounds.cell[0] * (bounds.tiles[0] * 256. - margin),
                    bounds.cell[1] * (bounds.tiles[1] * 256. - margin),
                ],
            }
        } else if let Some(hockey) = hockey {
            Mode::Hockey {
                teams: hockey.game.teams.clone(),
                arena: hockey.game.arena,
                puck: hockey.puck.position,
                puck_velocity: hockey.puck.velocity,
            }
        } else if let Some(game) = game.filter(|g| g.room.is_none()) {
            match (game.kind, &game.session) {
                (GameKind::TreasureHunt, _) => {
                    Mode::Treasure(game.markers().iter().map(|m| m.position).collect())
                }
                (GameKind::KingOfTheHills, Some(session)) => {
                    let marks = session.rules.markers();
                    Mode::Hills(
                        session
                            .rules
                            .zones()
                            .iter()
                            .zip(&marks)
                            .map(|((centre, half), m)| brain::Hill {
                                centre: *centre,
                                half: [half[0], half[2]],
                                owner: m.owner,
                                score: m.score,
                            })
                            .collect(),
                    )
                }
                _ => Mode::Idle,
            }
        } else if let Some(map) = roam {
            Mode::Roam(map.clone())
        } else {
            Mode::Idle
        };
        Self { mode, riders }
    }
}

/// What the brain reads of its own ride.
pub fn sense(ride: &Ride) -> Sense {
    let f = ride.frame();
    let [w, x, y, z] = f.orientation;
    // The ATV's +Z axis on the ground plane.
    let forward = [2. * (x * z + w * y), 1. - 2. * (x * x + y * y)];
    let length = forward[0].hypot_d(forward[1]).max(1e-4);
    let v = f.linear_velocity;
    Sense {
        position: f.origin,
        forward,
        speed: (v[0] * forward[0] + v[2] * forward[1]) / length,
        grounded: f.grounded.iter().any(|g| *g),
        riding: ride.rider_wreck().is_none() && f.wreck.phase == atv_core::wreck::Phase::Riding,
    }
}

/// Ground a roaming CPU keeps off: off the terrain, water too deep to
/// ford, or a slope steeper than `roam::STEEP`.
pub fn hazard(ride: &Ride, p: [f32; 2]) -> bool {
    ride.ground(p[0], p[1]).is_none()
        || ride
            .water_depth(p[0], p[1])
            .is_some_and(|depth| depth > roam::WADE)
        || roam::steep(&|x, z| ride.ground(x, z), p)
}

/// The venue's National race line (`<venue>nate.tdf` in its race archive)
/// as a roaming CPU follows it: the lap circuit on the ground plane. None
/// where the venue has no race (the Freeride-only venues).
async fn course_line(content: &atv_content::Content, environment: &str) -> Option<Vec<[f32; 2]>> {
    let path = atv_game::launch::course_resource(environment, true)
        .ok()?
        .archive;
    if !content.has(&path) {
        return None;
    }
    let archive = atv_game::archive::Archive::open(content, &path)
        .await
        .ok()?;
    let index = archive.named(&format!("{environment}nate.tdf"))?;
    let course = atv_core::race::Course::parse(&archive.member(index).await.ok()?).ok()?;
    let lap = course.points.get(course.loop_start..)?;
    (lap.len() > 2).then(|| lap.iter().map(|p| [p[0], p[2]]).collect())
}

/// What the venue offers roaming CPUs: its rideable area (40 ft inside the
/// boundary launch margin, `0020aec0`), its dry hilltops and its race line.
pub async fn roam_map(
    content: &atv_content::Content,
    venue: &atv_game::venue::Venue,
    ride: &Ride,
) -> roam::Map {
    const INSIDE: f32 = 40.;
    let terrain = ride.terrain();
    let bounds = atv_core::tag::Bounds::from_cells(
        terrain.width_cells,
        terrain.depth_cells,
        terrain.spacing,
    );
    let margin = atv_core::tag::boundary_margin(venue.scene.crowd_present);
    let min = [
        margin * bounds.cell[0] + INSIDE,
        margin * bounds.cell[1] + INSIDE,
    ];
    let max = [
        bounds.cell[0] * (bounds.tiles[0] * 256. - margin) - INSIDE,
        bounds.cell[1] * (bounds.tiles[1] * 256. - margin) - INSIDE,
    ];
    let start = venue.scene.start_position;
    let mut map = roam::Map {
        min,
        max,
        points: roam::points_of_interest(
            min,
            max,
            [start[0], start[2]],
            &|x, z| ride.ground(x, z),
            &|p| hazard(ride, p),
        ),
        ..roam::Map::default()
    };
    if let Some(line) = course_line(content, &venue.environment).await {
        map.set_line(line);
    }
    map
}

/// The player controls for one step's drive (reverse: the Reverse button
/// with the throttle, as a player backs up).
pub fn controls(drive: &brain::Drive) -> Controls {
    let reversing = drive.axis < 0.;
    Controls {
        throttle: drive.axis.abs(),
        brake: drive.brake,
        steer: drive.steer,
        reverse: reversing,
        preload_axis: drive.preload,
        ..Controls::default()
    }
}

impl Cpu {
    /// Loads CPU rider `player` (3-4 with split players, else 2-4) on
    /// player 1's ATV in a paint of its own.
    pub async fn load(
        gpu: &atv_render::Gpu,
        content: &atv_content::Content,
        venue: &atv_game::venue::Venue,
        scene: &mut atv_render::SceneRenderer,
        player: u8,
        request: Request,
        (selection, appearance, graphics): (
            atv_game::ride::Selection,
            atv_game::ride::gear::Appearance,
            atv_render::Graphics,
        ),
    ) -> Result<Self, String> {
        let mut ride = atv_game::ride::load_with(
            content,
            venue,
            selection.quad,
            selection.rider,
            None,
            |_| {},
        )
        .await?;
        let paints = atv_game::ride::gear::PAINTS.len() as u8;
        let look = atv_game::ride::gear::Appearance {
            gear: [0; 7],
            paint: (appearance.paint + 3 * player) % paints,
        };
        ride.dress(content, look).await?;
        atv_game::yield_frame().await;
        let start = venue.scene.start_position;
        let direction = venue.scene.start_direction;
        let (mut origin, yaw) =
            crate::split::freeride_start_for(start, direction[0].atan2_d(direction[2]), player);
        if let Some(ground) = ride.ground(origin[0], origin[2]) {
            origin[1] = ground + 2.5;
        }
        ride.set_start_pose((origin, yaw));
        ride.place_beside(origin, yaw, [0.; 3])?;
        let key: atv_game::multiplayer::remote::Key = (
            selection.quad,
            selection.rider,
            std::array::from_fn(|_| 0xff),
        );
        scene.set_peer(
            gpu,
            venue,
            crate::split::peer(player),
            key,
            (&ride.quad, &ride.rider),
            ride.paint(),
        )?;
        let mut riding = Riding::new(ride, venue);
        riding.interpolation = graphics.interpolation;
        riding.wheel_reprojection = graphics.wheel_reprojection;
        // A fixed seed per seat: the same game plays the same way.
        let seed = 0x5eed_0000 ^ (u32::from(player) * 7919);
        Ok(Self {
            player,
            riding,
            driver: Driver::default(),
            hill: HillRun::default(),
            roam: Roam::default(),
            wrecks: 0,
            wrecked: false,
            still: 0.,
            stalls: 0,
            rng: Rng::new(seed),
            seed,
            skill: Skill::for_difficulty(request.difficulty),
            landed: None,
            target: None,
            crash: (0, false),
        })
    }

    fn slot(&self) -> usize {
        usize::from(self.player) - 1
    }

    /// One rendered frame: each fixed step's goal from `world`, driven
    /// through the player's controls, then any stuck recovery.
    pub fn frame(&mut self, world: &World, dt: f32, bodies: Vec<PeerBody>) -> Result<(), String> {
        let me = self.slot();
        let Self {
            riding,
            driver,
            hill,
            roam,
            rng,
            seed,
            skill,
            landed,
            target,
            ..
        } = self;
        let seed = *seed;
        let skill = *skill;
        // A held rider (countdown, face-off, game over) wants nothing, so
        // it is never stuck.
        let held = riding.locked;
        let mut recover = Recover::None;
        let sampler = move |_: u64, _: &Ride| bodies.clone();
        riding.drive(dt, Some((0, &sampler)), |ride| {
            let sense = sense(ride);
            let goal = if held {
                None
            } else if let Mode::Roam(map) = &world.mode {
                // The other riders where the frame began (no allocation).
                let mut others = [Other::default(); 4];
                let mut count = 0;
                for (_, other) in world.riders.iter().enumerate().filter(|(i, _)| *i != me) {
                    if count < others.len() {
                        others[count] = *other;
                        count += 1;
                    }
                }
                roam.step(
                    &sense,
                    map,
                    &|p| hazard(ride, p),
                    &others[..count],
                    seed,
                    &skill,
                    rng,
                    atv_game::ride::STEP,
                )
            } else {
                goal(me, &sense, world, seed, &skill, hill, rng, landed)
            };
            *target = goal.map(|g| g.target).or(*target);
            let drive = driver.step(&sense, goal.as_ref(), atv_game::ride::STEP);
            if drive.recover != Recover::None {
                recover = drive.recover;
            }
            controls(&drive)
        })?;
        let f = self.riding.ride.frame();
        let wrecked = f.wreck.phase != atv_core::wreck::Phase::Riding;
        self.wrecks += u32::from(wrecked && !self.wrecked);
        self.wrecked = wrecked;
        // A roamer still for longer than the stuck rule can mend (beached,
        // or holding on a goal it cannot ride to) goes back to its start.
        if matches!(world.mode, Mode::Roam(_)) && !held {
            if !wrecked && f.speed_magnitude < brain::STUCK_SPEED {
                self.still += dt;
            } else {
                self.still = 0.;
            }
            if self.still > roam::STALL_SECONDS {
                self.still = 0.;
                self.stalls += 1;
                self.roam.plan = roam::Plan::None;
                return self.riding.reset();
            }
        }
        match recover {
            Recover::None => Ok(()),
            // The rink has no start to return to: turn in place there.
            Recover::Reset if !matches!(world.mode, Mode::Hockey { .. }) => {
                self.hill = HillRun::default();
                // A roamer picks somewhere else to go from its start.
                self.roam.plan = roam::Plan::None;
                self.riding.reset()
            }
            _ => {
                let f = self.riding.ride.frame();
                let mut origin = f.origin;
                let yaw = self
                    .target
                    .map_or(0., |t| (t[0] - origin[0]).atan2_d(t[1] - origin[2]));
                if let Some(ground) = self.riding.ride.ground(origin[0], origin[2]) {
                    origin[1] = origin[1].max(ground + 2.5);
                }
                self.riding.place_beside(origin, yaw, [0.; 3])
            }
        }
    }

    /// This rider's pose for its scene slot (the caller poses every slot in
    /// one `pose_peers` call).
    pub fn present(
        &mut self,
        gpu: &atv_render::Gpu,
        scene: &mut atv_render::SceneRenderer,
        venue: &atv_game::venue::Venue,
        dt: f32,
    ) -> Result<(u8, atv_game::ride::Pose), String> {
        let (_, _, pose) =
            self.riding
                .present_view(gpu, None, venue, crate::split::SPLIT_PRESENT_ASPECT, dt)?;
        let fade = self.riding.reset_fade(venue);
        scene.set_peer_fade(gpu, self.player, fade);
        Ok((crate::split::peer(self.player), pose))
    }

    /// The rider as the room riders' engine and impact voices hear it.
    pub fn audio_rider(&mut self) -> atv_game::audio::remote::Rider {
        let ride = &self.riding.ride;
        let f = ride.frame();
        let wreck = ride.rider_wreck();
        if wreck.is_some() && !self.crash.1 {
            self.crash.0 = self.crash.0.wrapping_add(1);
        }
        self.crash.1 = wreck.is_some();
        let (rider_position, rider_velocity) = wreck
            .map_or((f.origin, f.linear_velocity), |(m, v)| {
                ([m[12], m[13], m[14]], v)
            });
        atv_game::audio::remote::Rider {
            peer: crate::split::peer(self.player),
            quad: ride.selection().quad,
            position: f.origin,
            velocity: f.linear_velocity,
            rider_position,
            rider_velocity,
            throttle: ride.applied().throttle,
            speed: f.speed_magnitude,
            riding: wreck.is_none()
                && f.wreck.phase != atv_core::wreck::Phase::Recovering
                && f.grounded.iter().any(|g| *g),
            crash_serial: self.crash.0,
            stale: false,
            engine: None,
            cpu: false,
        }
    }

    /// Diagnostics for tools/test_engine_game_cpus.mjs.
    pub fn state(&self) -> serde_json::Value {
        let f = self.riding.ride.frame();
        serde_json::json!({
            "player": self.player,
            "origin": f.origin,
            "speed": f.speed_magnitude,
            "target": self.target,
            "escapes": self.driver.escapes,
            "resets": self.driver.resets,
            "locked": self.riding.locked,
            "riding": f.wreck.phase == atv_core::wreck::Phase::Riding,
            "wreck": f.wreck.kind,
            "wrecks": self.wrecks,
            "stalls": self.stalls,
            "grounded": f.grounded.iter().any(|g| *g),
            // A free ride's roaming (`atv_core::roam_cpu`).
            "roam": {
                "plan": match self.roam.plan {
                    roam::Plan::None => "none",
                    roam::Plan::Point(_) => "point",
                    roam::Plan::Line { .. } => "line",
                    roam::Plan::Drift { .. } => "drift",
                },
                "visits": self.roam.visits,
                "turns": self.roam.turns,
                "lineFeet": self.roam.line_feet,
                "wet": self
                    .riding
                    .ride
                    .water_depth(f.origin[0], f.origin[2])
                    .is_some_and(|depth| depth > roam::WADE),
            },
        })
    }

    /// The HUD's name tag and map marker.
    pub fn tag(&self, index: usize) -> atv_ui::hud::peers::Peer {
        atv_ui::hud::peers::Peer {
            slot: self.player - 1,
            name: NAMES[index.min(NAMES.len() - 1)].into(),
            position: self.riding.ride.frame().origin,
            stale: false,
            pill: None,
            map_only: false,
        }
    }
}

/// One step's goal for rider slot `me`.
#[allow(clippy::too_many_arguments)]
fn goal(
    me: usize,
    sense: &Sense,
    world: &World,
    seed: u32,
    skill: &Skill,
    hill: &mut HillRun,
    rng: &mut Rng,
    landed: &mut Option<f32>,
) -> Option<Goal> {
    // Every rider with this one where it stands now (no allocation).
    let mut riders = [Other::default(); 4];
    let count = world.riders.len().min(4);
    riders[..count].copy_from_slice(&world.riders[..count]);
    if me < count {
        riders[me].position = sense.position;
    }
    let riders = &riders[..count];
    match &world.mode {
        // Roaming is stepped by `Cpu::frame` (it reads the ride's ground).
        Mode::Idle | Mode::Roam(_) => None,
        Mode::Tag {
            carrier,
            ball,
            ball_velocity,
            min,
            max,
        } => brain::tag_goal(
            me,
            &brain::TagWorld {
                riders,
                carrier: *carrier,
                ball: *ball,
                ball_velocity: *ball_velocity,
                min: *min,
                max: *max,
            },
            skill,
        ),
        Mode::Treasure(treasures) => {
            let mut rivals = [[0f32; 3]; 4];
            let mut n = 0;
            for (index, r) in riders.iter().enumerate() {
                if index != me {
                    rivals[n] = r.position;
                    n += 1;
                }
            }
            brain::treasure_goal(sense.position, treasures, &rivals[..n], seed, skill)
        }
        Mode::Hills(hills) => {
            let (goal, points) =
                hill.step(me, sense, hills, seed, skill, rng, atv_game::ride::STEP);
            if points.is_some() {
                *landed = points;
            }
            goal
        }
        Mode::Hockey {
            teams,
            arena,
            puck,
            puck_velocity,
        } => {
            let team = teams.get(me).copied().unwrap_or(1).min(1);
            // A team's later riders hang back (one defender per team).
            let defender = teams[..me.min(teams.len())].contains(&team);
            let goal = |i: usize| [arena.goals[i][0], arena.goals[i][2]];
            let rink = brain::Rink {
                attack: goal(team),
                defend: goal(1 - team),
                min: arena.min,
                max: arena.max,
            };
            Some(brain::hockey_goal(
                sense.position,
                &rink,
                *puck,
                *puck_velocity,
                defender,
                skill,
            ))
        }
    }
}

/// Every CPU's contact bodies: the humans (`humans`, player 1 first, as
/// split.rs takes the -1 side of a tie for the lower player number) and the
/// other CPUs.
pub fn bodies(humans: &[PeerBody], cpus: &[Cpu]) -> Vec<Vec<PeerBody>> {
    cpus.iter()
        .map(|own| {
            humans
                .iter()
                .map(|b| PeerBody { tie: -1., ..*b })
                .chain(
                    cpus.iter()
                        .filter(|c| c.player != own.player && !c.riding.ride.reset_ghosted())
                        .map(|c| {
                            crate::split::body(
                                &c.riding.ride,
                                if c.player < own.player { -1. } else { 1. },
                            )
                        }),
                )
                .collect()
        })
        .collect()
}

/// The CPUs as the humans' contact bodies (the higher slot's side).
pub fn human_bodies(cpus: &[Cpu]) -> impl Iterator<Item = PeerBody> + '_ {
    cpus.iter()
        .filter(|c| !c.riding.ride.reset_ghosted())
        .map(|c| crate::split::body(&c.riding.ride, 1.))
}

/// The split players' and CPUs' rides in slot order (players 2.., then
/// the CPUs): a game session's `others`.
pub fn rides<'a>(split: &'a mut [crate::split::Second], cpus: &'a mut [Cpu]) -> Vec<&'a mut Ride> {
    split
        .iter_mut()
        .map(|s| &mut s.riding.ride)
        .chain(cpus.iter_mut().map(|c| &mut c.riding.ride))
        .collect()
}

/// Holds every CPU (a game's countdown, face-off or end).
pub fn lock(cpus: &mut [Cpu], locked: bool) {
    for cpu in cpus {
        cpu.riding.locked = locked;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keep_the_solo_name_and_follow_with_cpus() {
        assert_eq!(names(1, 2, "Ace"), ["Ace", "CPU 1", "CPU 2"]);
        assert_eq!(names(2, 1, "Ace"), ["Player 1", "Player 2", "CPU 1"]);
    }

    #[test]
    fn a_teams_later_rider_defends_and_tag_reads_its_own_slot() {
        let arena = atv_core::hockey::Arena::COONCE;
        let rider = |x: f32, z: f32| Other {
            position: [x, 90., z],
            velocity: [0.; 3],
        };
        let world = World {
            mode: Mode::Hockey {
                teams: vec![0, 1, 0],
                arena,
                puck: [384., 90., 420.],
                puck_velocity: [0.; 3],
            },
            riders: vec![rider(384., 354.), rider(384., 414.), rider(384., 350.)],
        };
        let skill = Skill::for_difficulty(1);
        let mut hill = HillRun::default();
        let mut rng = Rng::new(1);
        let mut landed = None;
        let mut at = |me: usize| {
            let s = Sense {
                position: world.riders[me].position,
                forward: [0., 1.],
                speed: 0.,
                grounded: true,
                riding: true,
            };
            goal(me, &s, &world, 1, &skill, &mut hill, &mut rng, &mut landed).unwrap()
        };
        // Red's CPU behind the human waits goal side; Blue's lone CPU attacks.
        assert!(at(2).stop);
        assert!(!at(1).stop);
        let tag = World {
            mode: Mode::Tag {
                carrier: Some(0),
                ball: [0.; 3],
                ball_velocity: [0.; 3],
                min: [0., 0.],
                max: [1000., 1000.],
            },
            riders: vec![rider(500., 500.), rider(400., 500.)],
        };
        let s = Sense {
            position: [400., 90., 500.],
            forward: [1., 0.],
            speed: 0.,
            grounded: true,
            riding: true,
        };
        let g = goal(1, &s, &tag, 1, &skill, &mut hill, &mut rng, &mut landed).unwrap();
        assert_eq!(g.target, [500., 500.]);
    }

    #[test]
    fn a_reverse_drive_backs_up_with_the_reverse_button() {
        let c = controls(&brain::Drive {
            axis: -1.,
            steer: 0.5,
            ..Default::default()
        });
        assert!(c.reverse && c.throttle == 1. && c.steer == 0.5);
        let c = controls(&brain::Drive {
            axis: 0.25,
            preload: Some(1.),
            ..Default::default()
        });
        assert!(!c.reverse && c.throttle == 0.25 && c.preload_axis == Some(1.));
    }
}
