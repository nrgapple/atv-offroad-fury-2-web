//! Single-player races: the original course (`<venue>nate.tdf`), grid,
//! pre-race camera, countdown, lap/polygon progress, CPU riders, rider
//! contacts and the after-finish director. Ports atv-web's operations 95,
//! 96, 105, 106, 113 and 114 and the race paths of its op 50 batch
//! (`race_controls`, `step_race`, `follow_camera`), calling atv-core directly.
//! Each step also reports its presentation ([`StepEvents`]): stadium crowd
//! cues and camera-flash bursts, Supercross firework commands, the National
//! drop gate pose, the 321 lamp sounds and the race music events.
use super::{Mount, Ride, STEP, cpu::CpuModel, cpu::CpuRacer, cpu::World};
use crate::Result;
use atv_core::math::Det;
use atv_core::{
    camera::CameraFrame,
    cpu_avoidance::Peer,
    motorcycle::Motorcycle,
    race::{Course, Frame, Race},
    race_ai::{Context, Driver, DriverPose, Options},
    race_crowd::{self, Crowd, CrowdFrame},
    race_director::{self, Cameras, Director},
    race_fireworks::{self, Fireworks},
    race_gate::DropGate,
    race_hud::{self, RaceHud},
    race_intro::{Intro, Pose, Scene, ShotFrame},
    race_roster::Entry,
    vehicle::{Vehicle, VehicleInput},
    wreck::Phase,
};

/// Seconds a CPU may stay outside the track polygon before recovering.
const CPU_OFF_TRACK_RESET: f32 = 5.;
/// Seconds a riding CPU may stay below 3 ft/s before recovering.
const CPU_STUCK_RESET: f32 = 4.;
/// Separates the start-slot draw from the roster draw on the same seed.
const GRID_STREAM: u64 = 0x001f_6350;

/// Race setup chosen in the Race menu (`normalizedRace` in web/frontend.js).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub laps: u32,
    /// CPU riders: 0, 3 or 4.
    pub cpu: usize,
    /// 1 Normal, 2 Expert.
    pub difficulty: u32,
    /// The original EATDIRT and BRINGIT cheats.
    pub aggressive: bool,
    pub all_out: bool,
    /// Roster draw seed (the browser's per-load Math.random() * 2^31).
    pub seed: u64,
    /// Play the pre-race camera (Restart Race reloads without it).
    pub intro: bool,
    /// A championship round: the series (index into
    /// `atv_core::championship::SERIES`) and Expert.
    pub championship: Option<(usize, bool)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            laps: 3,
            cpu: 3,
            difficulty: 1,
            aggressive: false,
            all_out: false,
            seed: 1,
            intro: true,
            championship: None,
        }
    }
}

/// A stadium camera-flash burst (`0020ee10` targets of `[CameraFlash]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashBurst {
    PlayerWin,
    AiFinish,
    PlayerStunt,
}

/// IntOut race music and the soundtrack hold (raceMusicEvent in game.js:
/// 00217bb0, 00217590, 00216260).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicEvent {
    /// Race load: hold the soundtrack, a random INT001-INT004 stinger.
    Intro,
    /// Countdown without the pre-race camera (Restart Race): hold only.
    Restart,
    /// GO: cut the stinger, release the soundtrack.
    Go,
    /// Race over: 1.5 s fade, OUT004 then the OUT001 loop after a win.
    Over { won: bool },
}

/// One race step's presentation: the extras atv-web attaches to its op 50
/// race frame and the per-frame race handlers of web/game.js.
#[derive(Clone, Debug, Default)]
pub struct StepEvents {
    pub flashes: Vec<FlashBurst>,
    pub crowd: Option<CrowdFrame>,
    /// Ambient mode emitter (emitter, instance, fire) commands.
    pub fireworks: Vec<(usize, usize, bool)>,
    /// The drop gate's world matrix this step.
    pub gate: Option<[f32; 16]>,
    /// One-shot samples: `beep01.vag`/`go.vag` with the lamps, `gate.vag`.
    pub sounds: Vec<&'static str>,
    pub music: Option<MusicEvent>,
}

/// Samples a race plays besides the ride's bank: the lamps, the gate and
/// the IntOut stingers (GameAudio.loadBank, game.js prefetch).
pub const RACE_SAMPLES: [&str; 9] = [
    "beep01.vag",
    "go.vag",
    "gate.vag",
    "intout/int001.vag",
    "intout/int002.vag",
    "intout/int003.vag",
    "intout/int004.vag",
    "intout/out001.vag",
    "intout/out004.vag",
];

/// Race presentation loaded with the course (op 115, op 116, the ambient
/// mode TOY's fireworks).
#[derive(Clone, Debug, Default)]
pub struct Stage {
    /// CrowdPresent races: `crowd.aud` for the Am or Pro crowd.
    pub crowd: Option<race_crowd::Sounds>,
    pub gate: Option<DropGate>,
    pub fireworks: Fireworks,
    /// The race TOY `[ChampionshipInfo]` Profile Points tables.
    pub awards: Option<atv_core::profile::CourseAwards>,
}

/// What a race load reads besides the ride itself.
pub struct Setup {
    pub course: Course,
    pub shots: Vec<Vec<ShotFrame>>,
    pub tv: Option<Cameras>,
    pub roster: Vec<Entry>,
    pub models: Vec<CpuModel>,
    /// Each CPU's quad and outfitted rider for drawing.
    pub draw: Vec<(super::Model, super::Model)>,
    pub spacing_sources: Vec<String>,
    /// A motorcycle player's CPUs ride with the ATV rider rig.
    pub cpu_rider: Option<Box<super::CpuRider>>,
    pub stage: Stage,
}

/// Each CPU quad's rest node matrices and the seated rider's.
type GridRest = (Vec<Vec<[f32; 16]>>, Vec<[f32; 16]>);

pub struct RaceState {
    pub race: Race,
    pub cpus: Vec<CpuRacer>,
    /// Original CPU quads and riders by slot (for drawing).
    pub roster: Vec<Entry>,
    pub models: Vec<CpuModel>,
    /// Each CPU's quad and outfitted rider for drawing (kept for uploads
    /// to a new graphics device, as game.js keeps raceCpuModels).
    pub draw: Vec<(super::Model, super::Model)>,
    intro: Option<Intro>,
    shots: Vec<Vec<ShotFrame>>,
    tv: Option<Cameras>,
    director: Option<(f32, Director, usize)>,
    autopilot: Option<Driver>,
    autopilot_stuck: f32,
    pub(super) autopilot_reset: bool,
    hud: RaceHud,
    pub frame: Option<Frame>,
    /// Parity tooling: the path driver also rides the player before the
    /// finish, so its controls can be recorded as a script.
    pub autopilot_always: bool,
    /// Controls the race imposed on the latest step.
    pub imposed: Option<VehicleInput>,
    settings: Settings,
    spacing_sources: Vec<String>,
    course: Course,
    cpu_rider: Option<Box<super::CpuRider>>,
    stage: Stage,
    crowd: Option<Crowd>,
    crowd_wrecked: bool,
    crowd_laps: usize,
    fireworks: Option<race_fireworks::Controller>,
    firework_positions: Vec<[f32; 3]>,
    lights: Option<u8>,
    music_phase: Option<&'static str>,
    /// The countdown follows the intro (game.js sends no `restart`), so
    /// the intro's hold and stinger still stand.
    music_intro: bool,
    /// Grid pose before the first step: each CPU quad's rest matrices and
    /// the player's seated rider pose (drawRosterCpu in game.js).
    rest: Option<GridRest>,
    /// The latest step's presentation, until the host takes it.
    pub events: StepEvents,
}

impl RaceState {
    /// The race setup this race loaded with.
    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// Samples the race plays besides the ride's bank: the lamps, gate and
    /// IntOut stingers, and the stadium crowd's reactions.
    pub fn samples(&self) -> Vec<String> {
        let mut names: Vec<String> = RACE_SAMPLES.iter().map(|s| s.to_string()).collect();
        if let Some(c) = &self.stage.crowd {
            for list in [&c.boo, &c.cheer, &c.oh, &c.win, &c.stomp] {
                names.extend(list.iter().cloned());
            }
        }
        names
    }

    /// The music event of the current phase, for sound that becomes ready
    /// after the race began (the browser queues it on the loading bank).
    pub fn music_now(&self) -> Option<MusicEvent> {
        // Before the first step the race holds on the grid.
        let Some(frame) = self.frame.as_ref() else {
            return Some(if self.intro.is_some() {
                MusicEvent::Intro
            } else {
                MusicEvent::Restart
            });
        };
        match frame.phase {
            "intro" => Some(MusicEvent::Intro),
            "countdown" if self.music_intro => Some(MusicEvent::Intro),
            "countdown" => Some(MusicEvent::Restart),
            "finished" => Some(MusicEvent::Over {
                won: frame.position == 1,
            }),
            _ => None,
        }
    }
}

/// The rider rig and skeleton CPU racers fork and pose: the player's own on
/// an ATV, else the loaded ATV rider.
type CpuRig<'a> = (
    (
        &'a atv_core::rider_pose::Animator,
        &'a atv_core::wreck_animation::Animator,
    ),
    &'a [atv_core::mesh::Node],
);
fn cpu_rig<'a>(
    mount: &'a Mount,
    rider: &'a super::Model,
    cpu_rider: Option<&'a super::CpuRider>,
) -> Result<CpuRig<'a>> {
    match (mount, cpu_rider) {
        (Mount::Quad { rig, .. }, _) => Ok(((&rig.animator, &rig.wreck), &rider.mesh.nodes)),
        (Mount::Bike(_), Some(cpu)) => Ok(((&cpu.animator, &cpu.wreck), &cpu.mesh.nodes)),
        (Mount::Bike(_), None) => Err("CPU rider rig missing".into()),
    }
}

/// The racing player as the race reads and pushes it.
enum Player<'a> {
    Quad(&'a mut Vehicle),
    Bike(&'a mut Motorcycle),
}

impl Player<'_> {
    fn frame(&self) -> atv_core::vehicle::VehicleFrame {
        match self {
            Player::Quad(vehicle) => vehicle.frame(),
            Player::Bike(bike) => super::bike::summary(bike.frame()),
        }
    }
    fn peer_contact(
        &mut self,
        origin: [f32; 3],
        velocity: [f32; 3],
        orientation: [f32; 4],
        tie: f32,
    ) -> bool {
        match self {
            Player::Quad(vehicle) => vehicle.peer_contact(origin, velocity, orientation, STEP, tie),
            Player::Bike(bike) => bike.peer_contact(origin, velocity, orientation, STEP, tie),
        }
    }
}

/// Body yaw from an orientation quaternion, as the browser computes it.
fn yaw(orientation: [f32; 4]) -> f32 {
    let [w, x, y, z] = orientation;
    (2. * (w * y + x * z)).atan2_d(1. - 2. * (y * y + x * x))
}

/// 001dd440 ground placement for the player's race grid and race resets;
/// the start keeps its heading.
pub(super) fn place_racer(
    vehicle: &mut Vehicle,
    terrain: &atv_core::terrain::Terrain,
) -> Result<()> {
    let f = vehicle.frame();
    let ground = terrain
        .contact(f.origin[0], f.origin[2])?
        .ok_or("race start outside terrain")?
        .height;
    vehicle.place_on_ground(
        terrain,
        [f.origin[0], ground, f.origin[2]],
        yaw(f.orientation),
    )
}

impl Ride {
    /// Op 96: a race on the loaded course; CPU riders take the grid and the
    /// pre-race camera starts unless `intro` is false (Restart Race).
    pub(super) fn begin_race(
        &mut self,
        setup: Setup,
        settings: Settings,
        intro: bool,
    ) -> Result<()> {
        let mut course = setup.course.clone();
        if let Some(spacing) =
            atv_core::race::rider_spacing(setup.spacing_sources.iter().map(String::as_str))
        {
            course.rider_spacing = spacing;
        }
        let names: Vec<String> = setup
            .roster
            .iter()
            .take(settings.cpu)
            .map(|e| e.name.clone())
            .collect();
        // 001f6350 shuffles the start slots per race; the roster seed gives
        // this race its own stream (the original draws from libc rand).
        let mut race = Race::seeded(
            course,
            settings.laps,
            names,
            settings.difficulty,
            settings.seed ^ GRID_STREAM,
        )?;
        // Race creation 001f9690: difficulty plus the EATDIRT/BRINGIT flags.
        let options = Options {
            aggressive: settings.aggressive,
            all_out: settings.all_out,
            ..Options::difficulty(settings.difficulty)
        };
        let (rig, nodes) = cpu_rig(&self.mount, &self.rider, setup.cpu_rider.as_deref())?;
        // Before the first step the grid draws each quad's rest pose with the
        // player's seated rider (the same rig).
        let rest = (
            setup
                .draw
                .iter()
                .map(|(quad, _)| quad.mesh.nodes.iter().map(|n| n.world_matrix).collect())
                .collect(),
            rig.0.pose(nodes)?.world_matrices,
        );
        let cpus = race
            .opponents()
            .iter()
            .enumerate()
            .map(|(index, cpu)| {
                let model = setup.models.get(index).ok_or("CPU quad missing")?;
                CpuRacer::new(
                    &self.sim, rig, nodes, model, cpu.origin, cpu.yaw, index, options,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        for (index, cpu) in cpus.iter().enumerate() {
            race.observe_cpu(
                index,
                cpu.vehicle.frame().origin,
                race.opponents()[index].yaw,
                None,
            );
        }
        // Race load plays the pre-race camera (00210980 SetCamera(5)).
        let mut intro_camera = None;
        if intro && !setup.shots.is_empty() {
            intro_camera = Some(Intro::new(setup.shots.clone()));
            race.begin_intro();
        }
        // The player takes its own start slot (001f8410); 001dd440 grounds an
        // ATV on the grid, and the bike keeps its op 23 drop.
        self.place_at(race.player_grid())?;
        if let Mount::Quad { vehicle, .. } = &mut self.mount {
            super::race::place_racer(vehicle, &self.sim.terrain)?;
            self.frame = vehicle.frame();
        }
        self.race = Some(RaceState {
            race,
            cpus,
            roster: setup.roster,
            models: setup.models,
            draw: setup.draw,
            intro: intro_camera,
            shots: setup.shots,
            tv: setup.tv,
            director: None,
            autopilot: None,
            autopilot_stuck: 0.,
            autopilot_reset: false,
            hud: RaceHud::default(),
            frame: None,
            autopilot_always: false,
            imposed: None,
            settings,
            spacing_sources: setup.spacing_sources,
            course: setup.course,
            cpu_rider: setup.cpu_rider,
            // Op 115 seeds the crowd per load (atv-web).
            crowd: setup
                .stage
                .crowd
                .clone()
                .map(|sounds| Crowd::new(sounds, 97211 + 7 * 47)),
            stage: setup.stage,
            crowd_wrecked: false,
            crowd_laps: 0,
            fireworks: None,
            firework_positions: Vec::new(),
            lights: None,
            music_phase: None,
            music_intro: false,
            rest: Some(rest),
            events: StepEvents::default(),
        });
        Ok(())
    }

    /// The latest race step's presentation (empty without a race).
    pub fn take_race_events(&mut self) -> StepEvents {
        self.race
            .as_mut()
            .map(|state| std::mem::take(&mut state.events))
            .unwrap_or_default()
    }

    /// Restart Race: a fresh race on the same course straight to the
    /// countdown (00215460), with the player back on the grid.
    pub fn restart_race(&mut self) -> Result<()> {
        let Some(state) = self.race.take() else {
            return Ok(());
        };
        self.reset()?;
        let setup = Setup {
            course: state.course,
            shots: state.shots,
            tv: state.tv,
            roster: state.roster,
            models: state.models,
            draw: state.draw,
            spacing_sources: state.spacing_sources,
            cpu_rider: state.cpu_rider,
            stage: state.stage,
        };
        self.begin_race(setup, state.settings, false)?;
        self.step_camera(0.)
    }

    /// Player controls imposed by the race: full brake on the grid during
    /// the countdown, then the original path driver once the player has
    /// finished (its exact-build routine is not identified).
    pub(super) fn race_controls(&mut self) -> Result<Option<VehicleInput>> {
        let input = self.race_controls_inner()?;
        if let Some(state) = &mut self.race {
            state.imposed = input;
        }
        Ok(input)
    }

    fn race_controls_inner(&mut self) -> Result<Option<VehicleInput>> {
        let Some(state) = &mut self.race else {
            return Ok(None);
        };
        let race = &state.race;
        if race.holding() {
            return Ok(Some(VehicleInput {
                brake: 1.,
                ..Default::default()
            }));
        }
        if !race.player_finished() && !(state.autopilot_always && race.running()) {
            state.autopilot = None;
            state.autopilot_stuck = 0.;
            // The Off Track sign counts down 5s, then the rider returns to
            // the course (RaceStatus +0x170 against the 5s rule of 00247570).
            state.autopilot_reset =
                race.running() && race.off_track_time(None) > race_hud::RESET_SECONDS;
            return Ok(None);
        }
        let (pose, scene) = match &self.mount {
            Mount::Quad { vehicle, .. } => {
                let mut pose: DriverPose = (&vehicle.frame()).into();
                pose.preload_rates = vehicle.preload_rates();
                (pose, vehicle.scene_collision())
            }
            // atv-web drives a finished motorcycle without preload and
            // without the ATV's scene query.
            Mount::Bike(bike) => {
                let f = bike.frame();
                (
                    DriverPose {
                        origin: f.origin,
                        orientation: f.orientation,
                        speed_magnitude: f.speed_magnitude,
                        grounded: f.grounded.iter().any(|g| *g),
                        riding: f.wreck.phase == Phase::Riding,
                        preload_available: false,
                        preload_armed: false,
                        preload_rates: [0.; 2],
                    },
                    None,
                )
            }
        };
        let contact = self.sim.terrain.contact(pose.origin[0], pose.origin[2])?;
        let peers: Vec<_> = state
            .cpus
            .iter()
            .map(|cpu| {
                let f = cpu.vehicle.frame();
                Peer {
                    origin: f.origin,
                    velocity: f.linear_velocity,
                    human: None,
                }
            })
            .collect();
        // The authored CPU recovery also returns a stuck or off-track player.
        if pose.riding && pose.speed_magnitude < 3. {
            state.autopilot_stuck += STEP;
        } else {
            state.autopilot_stuck = 0.;
        }
        if pose.riding
            && (race.off_track_time(None) > CPU_OFF_TRACK_RESET
                || state.autopilot_stuck > CPU_STUCK_RESET)
        {
            state.autopilot_reset = true;
            state.autopilot_stuck = 0.;
        }
        // Slot 4 follows the four CPU slots' driver random streams.
        let driver = state
            .autopilot
            .get_or_insert_with(|| Driver::new(4, Default::default()));
        let (input, _) = driver.step(
            &race.course,
            &pose,
            0.,
            0.,
            STEP,
            Context {
                peers: &peers,
                scene,
                clearance: contact.as_ref().map_or(0., |c| pose.origin[1] - c.height),
                terrain_normal: contact.as_ref().map_or([0., 1., 0.], |c| c.normal),
                station: Some(race.station(None)),
            },
        )?;
        Ok(Some(input))
    }

    /// Race progress, CPU racers and rider contact after the player's step.
    pub(super) fn step_race(&mut self) -> Result<()> {
        let committed = self.committed_points();
        let Some(state) = &mut self.race else {
            return Ok(());
        };
        // The CPUs' rider skeleton: the player's on an ATV, else the ATV rider.
        let nodes: &[atv_core::mesh::Node] = match state.cpu_rider.as_deref() {
            Some(cpu) => &cpu.mesh.nodes,
            None => &self.rider.mesh.nodes,
        };
        let mut vehicle = match &mut self.mount {
            Mount::Quad { vehicle, .. } => Player::Quad(vehicle),
            Mount::Bike(bike) => Player::Bike(bike),
        };
        let player = vehicle.frame();
        let race = &mut state.race;
        let laps_before: Vec<u32> = std::iter::once(None)
            .chain((0..race.opponents().len()).map(Some))
            .map(|r| race.laps_completed(r))
            .collect();
        race.step(player.origin, STEP);
        race.record_player_stunt(committed);
        let terrain = &self.sim.terrain;
        let hold = race.holding();
        let snapshots: Vec<_> = state
            .cpus
            .iter()
            .map(|cpu| {
                let f = cpu.vehicle.frame();
                Peer {
                    origin: f.origin,
                    velocity: f.linear_velocity,
                    human: None,
                }
            })
            .collect();
        let dynamic = match &vehicle {
            Player::Quad(v) => v
                .scene_collision()
                .map_or_else(Vec::new, |s| s.dynamic().to_vec()),
            Player::Bike(_) => Vec::new(),
        };
        let world = World {
            terrain,
            nodes,
            materials: &self.sim.materials,
            water: self.sim.water.as_ref(),
            dynamic: &dynamic,
            hold,
        };
        // Race checkpoints stay on the racing line at each rider's last
        // on-track station, facing the race direction, on the ground.
        let on_line = |(point, heading): ([f32; 3], f32)| -> Result<([f32; 3], f32)> {
            let height = terrain
                .contact(point[0], point[2])?
                .map_or(point[1], |c| c.height);
            Ok(([point[0], height, point[2]], heading))
        };
        let (point, heading) = on_line(race.checkpoint(None))?;
        match &mut vehicle {
            Player::Quad(v) => v.set_race_checkpoint(Some((point, heading))),
            // The motorcycle reset keeps the authored 2.5ft course drop.
            Player::Bike(bike) => bike.set_race_checkpoint(Some((
                [
                    point[0],
                    point[1] + atv_core::spawn::COURSE_CLEARANCE as f32,
                    point[2],
                ],
                heading,
            ))),
        }
        // RaceStatus +0x150: the human's track direction (aggression branch).
        let player_track = race.course.tangent(race.station(None).segment);
        for (index, cpu) in state.cpus.iter_mut().enumerate() {
            cpu.vehicle
                .set_race_checkpoint(Some(on_line(race.checkpoint(Some(index)))?));
            let peers: Vec<_> = std::iter::once(Peer {
                origin: player.origin,
                velocity: player.linear_velocity,
                human: Some(player_track),
            })
            .chain(
                snapshots
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, p)| *p),
            )
            .collect();
            let (frame, physical, repositioned) = cpu.step(
                &race.course,
                player.speed_magnitude,
                &peers,
                race.progress_difference(index),
                race.station(Some(index)),
                &world,
            )?;
            let cpu_yaw = yaw(frame.orientation);
            // Settling on the grid earns no race distance.
            if repositioned || hold {
                race.reposition_cpu(index, frame.origin, cpu_yaw, Some(physical));
            } else {
                race.observe_cpu(index, frame.origin, cpu_yaw, Some(physical.clone()));
                // Authored recovery: a CPU off the course or pinned in place
                // returns to its on-line checkpoint.
                if frame.wreck.phase == Phase::Riding
                    && (race.off_track_time(Some(index)) > CPU_OFF_TRACK_RESET
                        || cpu.stuck_time() > CPU_STUCK_RESET)
                {
                    cpu.respawn(nodes)?;
                    let reset = cpu.vehicle.frame();
                    race.reposition_cpu(
                        index,
                        reset.origin,
                        yaw(reset.orientation),
                        Some(physical),
                    );
                }
            }
        }
        // Rider-to-rider contact with the authored equal-mass proxy: every
        // pair resolves from pre-contact poses; each racer applies its half.
        let body =
            |f: &atv_core::vehicle::VehicleFrame| (f.origin, f.linear_velocity, f.orientation);
        let bodies: Vec<_> = std::iter::once(body(&player))
            .chain(state.cpus.iter().map(|c| body(&c.vehicle.frame())))
            .collect();
        for a in 0..bodies.len() {
            for b in a + 1..bodies.len() {
                for (own, other, tie) in [(a, b, 1.), (b, a, -1.)] {
                    let (origin, velocity, orientation) = bodies[other];
                    let contact = if own == 0 {
                        vehicle.peer_contact(origin, velocity, orientation, tie)
                    } else {
                        state.cpus[own - 1].vehicle.peer_contact(
                            origin,
                            velocity,
                            orientation,
                            STEP,
                            tie,
                        )
                    };
                    if contact && tie > 0. {
                        race.contacts += 1;
                    }
                }
            }
        }
        let mut frame = race.frame(yaw(player.orientation));
        // The ATV's deep-water clock (the bike has none).
        let too_deep = match &vehicle {
            Player::Quad(v) => Some(v.frame().water_failure),
            Player::Bike(_) => None,
        }
        .filter(|w| w.submerged)
        .map(|w| w.remaining);
        // Crowd reactions and flash bursts (00216190, 002164b0).
        let finishes = race.take_finishes();
        let wrecked = vehicle.frame().wreck.phase != Phase::Riding;
        let mut events = StepEvents::default();
        let wreck_began = wrecked && !state.crowd_wrecked;
        state.crowd_wrecked = wrecked;
        let laps_done = frame.lap_times.len();
        let final_lap = laps_done > state.crowd_laps && laps_done as u32 + 1 == frame.laps;
        state.crowd_laps = laps_done;
        for &(player, place) in &finishes {
            if place <= 3 {
                events.flashes.push(if player && place == 1 {
                    FlashBurst::PlayerWin
                } else {
                    FlashBurst::AiFinish
                });
            }
        }
        if std::mem::take(&mut self.stunt_landed) {
            events.flashes.push(FlashBurst::PlayerStunt);
        }
        let stunt = self.stunt_result.take();
        if let Some(crowd) = state.crowd.as_mut() {
            let cue = crowd.step(&race_crowd::Events {
                elapsed: frame.elapsed,
                player_rank: frame.position,
                player_wrecked: wreck_began,
                final_lap,
                finishes,
                single_player: true,
                stunt,
            });
            frame.crowd = (cue.cue.is_some() || cue.stop_loop).then_some(cue);
            events.crowd.clone_from(&frame.crowd);
        }
        // Supercross fireworks: the countdown stops the pre-race set; racers
        // crossing the start/finish triggers fire their groups (00223c60).
        let laps = frame.laps;
        let controller = state.fireworks.get_or_insert_with(|| {
            race_fireworks::Controller::new(state.stage.fireworks.clone(), laps)
        });
        let current: Vec<[f32; 3]> = std::iter::once(player.origin)
            .chain(frame.opponents.iter().map(|o| o.origin))
            .collect();
        if state.firework_positions.len() != current.len() {
            state.firework_positions.clone_from(&current);
        }
        let racers: Vec<_> = current
            .iter()
            .zip(&state.firework_positions)
            .enumerate()
            .map(|(i, (now, before))| race_fireworks::Racer {
                previous: *before,
                current: *now,
                laps: laps_before.get(i).copied().unwrap_or(0),
                place: frame.ranks.get(i).copied().unwrap_or(0),
                cpu: i > 0,
            })
            .collect();
        state.firework_positions = current;
        events.fireworks = controller.step(STEP, frame.phase == "countdown", &racers);
        // National drop gate: standing until GO, then its linear swing.
        if let Some(gate) = &state.stage.gate {
            let live = matches!(frame.phase, "racing" | "finished");
            frame.gate = Some(gate.matrix(gate.angle_at(live.then_some(frame.elapsed))));
            events.gate = frame.gate;
        }
        // 321 LIGHTS (002e79a0): beep01 as each red lamp lights, go with the
        // green lamp; a National drops its gate (00223bf0).
        if frame.lights != state.lights
            && let Some(lights) = frame.lights.filter(|l| *l > 0)
        {
            events
                .sounds
                .push(if lights < 4 { "beep01.vag" } else { "go.vag" });
            if lights == 4 && frame.gate.is_some() {
                events.sounds.push("gate.vag");
            }
        }
        state.lights = frame.lights;
        if state.music_phase != Some(frame.phase) {
            let previous = state.music_phase.replace(frame.phase);
            events.music = match frame.phase {
                "intro" => Some(MusicEvent::Intro),
                "countdown" if previous != Some("intro") => Some(MusicEvent::Restart),
                "racing" => Some(MusicEvent::Go),
                "finished" => Some(MusicEvent::Over {
                    won: frame.position == 1,
                }),
                _ => None,
            };
            match events.music {
                Some(MusicEvent::Intro) => state.music_intro = true,
                Some(_) => state.music_intro = false,
                None => {}
            }
        }
        state.events = events;
        state.rest = None;
        frame.hud = Some(state.hud.step(
            STEP,
            &race_hud::Input {
                racing: frame.phase == "racing",
                finished: frame.phase == "finished",
                laps: frame.laps,
                lap_times: &frame.lap_times,
                best_lap: frame.best_lap,
                place: frame.position,
                ranks: &frame.ranks,
                wrong_way: frame.wrong_way,
                off_track: frame.off_track.then_some(frame.off_track_time),
                too_deep,
            },
        ));
        frame.championship = state.settings.championship.and_then(|(index, expert)| {
            let series = atv_core::championship::SERIES.get(index)?;
            Some(atv_core::championship::Round {
                series: series.key,
                expert,
                awards: state
                    .stage
                    .awards
                    .map(|a| *a.table(series.pro, expert))
                    .unwrap_or_default(),
            })
        });
        state.frame = Some(frame);
        Ok(())
    }

    /// The after-finish director (001f3bb0): three seconds after race over,
    /// cut between the trackside cameras following the player.
    pub(super) fn race_director(&mut self, rider: [f32; 3]) -> Result<Option<race_director::Pose>> {
        // Only the after-finish director reads the player's speed.
        let player_speed = if self.race.as_ref().is_some_and(|r| r.race.player_finished()) {
            self.live_frame().speed_magnitude
        } else {
            0.
        };
        let Some(state) = &mut self.race else {
            return Ok(None);
        };
        let finished = state.race.player_finished();
        let Some(tv) = state.tv.as_ref().filter(|_| finished) else {
            if let Some((_, _, saved)) = state.director.take() {
                self.sim.camera.direct(saved, false);
            }
            return Ok(None);
        };
        if state.director.is_none() {
            let saved = self.sim.camera.selected;
            state.director = Some((0., Director::new(tv.clone(), 97211 + 6 * 47), saved));
        }
        let racers: Vec<_> = std::iter::once(race_director::Racer {
            origin: rider,
            speed: player_speed,
        })
        .chain(state.cpus.iter().map(|cpu| {
            let f = cpu.vehicle.frame();
            race_director::Racer {
                origin: f.origin,
                speed: f.speed_magnitude,
            }
        }))
        .collect();
        let (elapsed, director, _) = state.director.as_mut().ok_or("director missing")?;
        *elapsed += STEP;
        if *elapsed < race_director::DELAY {
            return Ok(None);
        }
        let step = director.step(STEP, rider, &racers)?;
        if let Some(race_director::View::Follow { preset, ease }) = step.cut {
            self.sim.camera.direct(preset, ease);
        }
        Ok(step.tv)
    }

    /// The pre-race camera over the follow pose; it starts the countdown.
    pub(super) fn race_intro(
        &mut self,
        follow: &CameraFrame,
        rider: [f32; 3],
        forward: [f32; 3],
    ) -> Result<Option<Pose>> {
        let Some(state) = &mut self.race else {
            return Ok(None);
        };
        let Some(intro) = &mut state.intro else {
            return Ok(None);
        };
        if !state.race.holding() {
            state.intro = None;
            return Ok(None);
        }
        let terrain = &self.sim.terrain;
        let ground = |x: f32, z: f32| {
            terrain
                .contact(x, z)
                .ok()
                .flatten()
                .map_or(f32::MIN, |c| c.height)
        };
        let pose = intro.step(
            STEP,
            &Scene {
                rider,
                forward,
                follow: Pose {
                    position: follow.position,
                    target: follow.target,
                    fov: follow.fov,
                },
                ground: &ground,
            },
        )?;
        if intro.countdown_requested() {
            state.race.start_countdown();
        }
        Ok(pose)
    }

    /// Skip the pre-race camera on any button (00212d40).
    pub fn skip_intro(&mut self) {
        if let Some(state) = &mut self.race
            && let Some(intro) = &mut state.intro
            && intro.skip()
        {
            state.race.start_countdown();
        }
    }

    pub fn race(&self) -> Option<&RaceState> {
        self.race.as_ref()
    }

    pub fn race_mut(&mut self) -> Option<&mut RaceState> {
        self.race.as_mut()
    }

    /// World pose of each CPU's quad and rider for drawing.
    pub fn cpu_poses(&self) -> Vec<super::Pose> {
        let Some(state) = &self.race else {
            return Vec::new();
        };
        if let (Some((quads, rider)), None) = (&state.rest, &state.frame) {
            return state
                .race
                .opponents()
                .iter()
                .zip(quads)
                .map(|(cpu, quad)| {
                    let (s, c) = (cpu.yaw / 2.).sin_cos_d();
                    let world = super::quaternion_world(cpu.origin, [c, 0., s, 0.]);
                    super::Pose {
                        quad_world: world,
                        quad_nodes: quad.clone(),
                        rider_world: world,
                        rider_nodes: rider.clone(),
                    }
                })
                .collect();
        }
        state
            .cpus
            .iter()
            .zip(state.frame.iter().flat_map(|f| f.opponents.iter()))
            .filter_map(|(cpu, opponent)| {
                let physical = opponent.physical.as_ref()?;
                let frame = cpu.vehicle.frame();
                let quad_world = super::quaternion_world(frame.origin, physical.orientation);
                Some(super::Pose {
                    quad_world,
                    quad_nodes: physical.quad_matrices.clone(),
                    rider_world: physical.rider_world_matrix.unwrap_or(quad_world),
                    rider_nodes: physical.rider_matrices.clone(),
                })
            })
            .collect()
    }
}
