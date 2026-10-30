//! Single-player races: the original course (`<venue>nate.tdf`), grid,
//! pre-race camera, countdown, lap/polygon progress, CPU riders, rider
//! contacts and the after-finish director. Ports atv-web's operations 95,
//! 96, 105, 106, 113 and 114 and the race paths of its op 50 batch
//! (`race_controls`, `step_race`, `follow_camera`), calling atv-core directly.
//! Each step also reports its presentation ([`StepEvents`]): stadium crowd
//! cues and camera-flash bursts, Supercross firework commands, the National
//! drop gate pose, the 321 lamp sounds and the race music events.
use super::{Mount, Ride, STEP, cpu::CpuModel, cpu::CpuRacer, cpu::Puppet, cpu::World};
use crate::Result;
use atv_core::cpu_recovery;
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
    /// Which of the venue's races (National, Enduro long/short, Short Track).
    pub course: crate::launch::RaceVariant,
    /// A room race (research/MULTIPLAYER.md "Room races"): this rider's
    /// start slot and the human grid size. The race holds on the grid until
    /// the room host's GO ([`Ride::room_go`]).
    pub room: Option<(u32, usize)>,
    /// A room race's CPU riders (research/MULTIPLAYER.md "Room race CPUs"):
    /// their start slots on the room grid and the host's roster draw. The
    /// host races them; a guest's follow the host's relayed motion.
    pub room_cpus: Option<RoomCpus>,
}

/// A room race's CPU field (see [`Settings::room_cpus`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomCpus {
    /// Each CPU's start slot (the first [`Settings::cpu`] entries).
    pub slots: [u32; 4],
    /// The host's ATV (1-26) and rider (0 male, 1 female): the roster draw.
    pub roster: (u16, u16),
    /// A guest: the CPUs are puppets of the host's relayed motion.
    pub puppets: bool,
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
            course: Default::default(),
            room: None,
            room_cpus: None,
        }
    }
}

/// Room standings in race order replace the solo race's (its only racer
/// is the player).
fn room_standings(frame: &mut Frame, rows: &[RoomRow]) {
    let mine = frame.standings.iter().find(|s| s.player);
    let (best, stunt) = mine.map_or((None, 0.), |s| (s.best, s.stunt));
    frame.field = rows.len().max(1);
    if let Some(at) = rows.iter().position(|r| r.player) {
        frame.position = at + 1;
    }
    frame.standings = rows
        .iter()
        .enumerate()
        .map(|(at, r)| atv_core::race::Standing {
            name: r.name.clone(),
            position: at + 1,
            laps: r.laps,
            time: r.time,
            best: if r.player { best } else { None },
            estimated: false,
            progress: r.progress,
            stunt: if r.player { stunt } else { 0. },
            player: r.player,
        })
        .collect();
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
    /// An Enduro race: the player's next gate (0-based), drawn opaque.
    pub enduro_next: Option<usize>,
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

/// Split screen: the other local humans as this race sees them
/// (research/SPLIT_SCREEN.md). Player 1's race holds the CPU field.
#[derive(Clone, Debug, Default)]
pub struct SplitLink {
    /// The other humans' (origin, velocity, orientation): the CPUs steer
    /// around them (human peers, RaceStatus flag 4) and push against them.
    pub humans: Vec<([f32; 3], [f32; 3], [f32; 4])>,
    /// The combined place from the latest standings merge: the PLACE sign,
    /// the finish sign and the race-over music read it.
    pub place: Option<usize>,
    /// Another human has not finished: the race is not over yet.
    pub waiting: bool,
}

pub struct RaceState {
    /// Split screen's other human (none in a one-player race).
    pub split: SplitLink,
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
    /// The player's vehicle reset count at the last race update.
    player_resets: Option<u32>,
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
    /// A room race guest's grid pose for CPUs not heard from yet.
    rest_pose: Option<GridRest>,
    /// The latest step's presentation, until the host takes it.
    pub events: StepEvents,
    /// The Hall of Fame and personal records as the event started; a
    /// restart keeps them for its fresh race.
    records_start: Option<atv_core::hall_of_fame::RecordWatch>,
    /// A room race's standings across every room rider, set by the room
    /// each frame; they replace the frame's position, field and standings.
    pub room: Option<Vec<RoomRow>>,
    /// A room race guest's CPUs, drawn from the host's motion (empty
    /// otherwise; then `cpus` race here).
    pub puppets: Vec<Puppet>,
    /// The host who raced them left: the guest's CPUs are no longer drawn.
    pub puppets_retired: bool,
}

/// A room race host's CPU for the room ([`Ride::room_cpu_frames`]).
pub struct RoomCpuFrame {
    pub index: usize,
    /// Its original quad (1-26).
    pub quad: u16,
    pub lap: u32,
    pub distance: f32,
    /// Finish time (race seconds).
    pub finish: Option<f32>,
    pub frame: atv_core::vehicle::VehicleFrame,
    pub recipe: atv_core::rider_pose::Recipe,
    /// The detached rider's world matrix and velocity during a wreck.
    pub wreck: Option<([f32; 16], [f32; 3])>,
}

/// One room-race rider's standing (research/MULTIPLAYER.md "Room races").
#[derive(Clone, Debug, PartialEq)]
pub struct RoomRow {
    pub name: String,
    pub laps: u32,
    /// Finish time (s).
    pub time: Option<f32>,
    pub progress: f32,
    pub player: bool,
}

impl RaceState {
    /// The player's Hall of Fame and personal records for this event.
    pub fn set_records(&mut self, records: atv_core::hall_of_fame::RecordWatch) {
        self.records_start = Some(records.clone());
        self.race.set_records(records);
    }
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
        // A loaded CPU rig (a motorcycle player's, or a room race whose host
        // rides the other rider) wins.
        (_, Some(cpu)) => Ok(((&cpu.animator, &cpu.wreck), &cpu.mesh.nodes)),
        (Mount::Quad { rig, .. }, None) => Ok(((&rig.animator, &rig.wreck), &rider.mesh.nodes)),
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
        // A room race takes its own slot on the shared grid and holds there
        // for the host's countdown (authored, research/MULTIPLAYER.md); its
        // CPUs take theirs.
        if let Some((slot, riders)) = settings.room {
            match settings.room_cpus.filter(|_| settings.cpu > 0) {
                Some(field) => {
                    let slots = field
                        .slots
                        .get(..settings.cpu)
                        .ok_or("invalid room race CPUs")?;
                    race.set_room_grid(slot, riders, slots)?;
                }
                None => race.set_player_grid(slot, riders)?,
            }
            race.begin_intro();
        }
        // A guest's CPUs are drawn from the host's motion, not raced here.
        let puppets = settings.room_cpus.is_some_and(|c| c.puppets);
        let puppet_list = if puppets {
            (0..race.opponents().len())
                .map(|index| {
                    let model = setup.models.get(index).ok_or("CPU quad missing")?;
                    Puppet::new(rig, nodes, model, index)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let cpus = race
            .opponents()
            .iter()
            .enumerate()
            .filter(|_| !puppets)
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
        if intro && settings.room.is_none() && !setup.shots.is_empty() {
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
            player_resets: None,
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
            split: SplitLink::default(),
            fireworks: None,
            firework_positions: Vec::new(),
            lights: None,
            music_phase: None,
            music_intro: false,
            rest_pose: (!puppet_list.is_empty()).then(|| rest.clone()),
            rest: Some(rest),
            events: StepEvents::default(),
            records_start: None,
            room: None,
            puppets: puppet_list,
            puppets_retired: false,
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
        if let (Some(records), Some(race)) = (state.records_start, &mut self.race) {
            race.set_records(records);
        }
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
                on_track: race.on_track(None),
                enduro: race.enduro_target(None),
            },
        )?;
        Ok(Some(input))
    }

    /// Race progress, CPU racers and rider contact after the player's step.
    /// Split screen: each CPU's contact body (origin, velocity,
    /// orientation) for the other human's ride.
    pub fn race_cpu_bodies(&self) -> Vec<([f32; 3], [f32; 3], [f32; 4])> {
        self.race.as_ref().map_or_else(Vec::new, |state| {
            state
                .cpus
                .iter()
                .map(|c| {
                    let f = c.vehicle.frame();
                    (f.origin, f.linear_velocity, f.orientation)
                })
                .collect()
        })
    }

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
        // A player reset (wreck recovery, deep water, Reset) returns the
        // race stations to the accepted reset point first.
        if state
            .player_resets
            .replace(player.reset_count)
            .is_some_and(|count| count != player.reset_count)
        {
            race.restore_checkpoint(None, player.origin);
        }
        race.waiting = state.split.waiting;
        race.step(player.origin, STEP);
        race.record_player_stunt(committed);
        race.record_player_jump(
            player.grounded.iter().any(|g| *g),
            player.origin,
            player.reset_count,
            STEP,
        );
        for (success, count, sum) in self.race_chains.drain(..) {
            race.record_player_chain(success, count, sum);
        }
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
        // Each on-track racer offers its reset point; the vehicle keeps it
        // only where 001ea4d0 finds it clear (offer_race_checkpoint). A new
        // race's first point (the grid, 001f80e0) is set as is.
        let offer =
            |v: &mut Vehicle, race: &mut atv_core::race::Race, cpu: Option<usize>| -> Result<()> {
                let (point, heading) = on_line(race.checkpoint(cpu))?;
                if !race.has_checkpoint(cpu) {
                    v.set_race_checkpoint(Some((point, heading)));
                    race.accept_checkpoint(cpu);
                } else if race.on_track(cpu) && v.offer_race_checkpoint(terrain, point, heading)? {
                    race.accept_checkpoint(cpu);
                }
                Ok(())
            };
        let (point, heading) = on_line(race.checkpoint(None))?;
        // Split screen's other humans: human peers the CPUs avoid.
        let humans: Vec<Peer> = state
            .split
            .humans
            .iter()
            .map(|&(origin, velocity, _)| Peer {
                origin,
                velocity,
                human: Some(
                    race.course
                        .tangent(race.course.nearest_station(origin).segment),
                ),
            })
            .collect();
        let positions: Vec<[f32; 3]> = std::iter::once(player.origin)
            .chain(snapshots.iter().map(|p| p.origin))
            .chain(humans.iter().map(|h| h.origin))
            .chain(state.puppets.iter().filter_map(|p| p.body.map(|b| b.0)))
            .collect();
        let others = |own: usize| {
            positions
                .iter()
                .enumerate()
                .filter(move |(i, _)| *i != own)
                .map(|(_, p)| *p)
        };
        match &mut vehicle {
            Player::Quad(v) => {
                v.set_reset_peers(others(0));
                offer(v, race, None)?;
            }
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
            cpu.vehicle.set_reset_peers(others(index + 1));
            offer(&mut cpu.vehicle, race, Some(index))?;
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
            .chain(humans.iter().copied())
            .collect();
            let (frame, physical, repositioned) = cpu.step(
                &race.course,
                player.speed_magnitude,
                &peers,
                race.progress_difference(index),
                (
                    race.station(Some(index)),
                    race.on_track(Some(index)),
                    race.enduro_target(Some(index)),
                ),
                &world,
            )?;
            let cpu_yaw = yaw(frame.orientation);
            // Settling on the grid earns no race distance.
            if repositioned || hold {
                race.reposition_cpu(index, frame.origin, cpu_yaw, Some(physical.clone()));
                if repositioned && !hold {
                    race.restore_checkpoint(Some(index), frame.origin);
                    cpu.recovery.reset(race.race_distance(index));
                }
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
                        Some(physical.clone()),
                    );
                    race.restore_checkpoint(Some(index), reset.origin);
                    cpu.recovery.reset(race.race_distance(index));
                }
            }
            // Port adaptation (atv_core::cpu_recovery): a CPU that keeps
            // returning to the same reset point, or makes no route progress
            // while riding, is reset to a validated point further on.
            if !hold
                && !race.cpu_finished(index)
                && cpu.recovery.observe(
                    race.race_distance(index),
                    cpu.vehicle.frame().wreck.phase == Phase::Riding,
                    STEP,
                ) == cpu_recovery::Action::SkipAhead
            {
                let candidates = race.recovery_stations(index, &cpu_recovery::SKIP_DISTANCES);
                let mut chosen = None;
                for candidate in &candidates {
                    let (point, heading) = on_line((candidate.point, candidate.heading))?;
                    if cpu
                        .vehicle
                        .offer_recovery_checkpoint(terrain, point, heading)?
                    {
                        chosen = Some((candidate.station, candidate.gained));
                        break;
                    }
                }
                // None clear: the furthest candidate, placed by the ring
                // search of the off-track reset.
                if chosen.is_none()
                    && let Some(last) = candidates.last()
                {
                    cpu.vehicle
                        .set_race_checkpoint(Some(on_line((last.point, last.heading))?));
                    chosen = Some((last.station, last.gained));
                }
                if let Some((station, gained)) = chosen {
                    race.accept_recovery_station(index, station, gained);
                    cpu.respawn(nodes)?;
                    let reset = cpu.vehicle.frame();
                    race.reposition_cpu(
                        index,
                        reset.origin,
                        yaw(reset.orientation),
                        Some(physical),
                    );
                    race.restore_checkpoint(Some(index), reset.origin);
                    cpu.recovery.skipped(race.race_distance(index));
                } else {
                    // Nowhere ahead to go (at the gate): count afresh.
                    cpu.recovery.skipped(race.race_distance(index));
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
        // Split screen: each CPU applies its half against the other human
        // (whose own ride applies the other half, split.rs).
        for &(origin, velocity, orientation) in &state.split.humans {
            for cpu in &mut state.cpus {
                if cpu
                    .vehicle
                    .peer_contact(origin, velocity, orientation, STEP, 1.)
                {
                    race.contacts += 1;
                }
            }
        }
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
        // A room race guest pushes against the host's CPUs; the host's
        // CPUs push back on the host's machine (each applies its own half).
        if !state.puppets_retired {
            for &(origin, velocity, orientation) in
                state.puppets.iter().filter_map(|p| p.body.as_ref())
            {
                if vehicle.peer_contact(origin, velocity, orientation, 1.) {
                    race.contacts += 1;
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
        events.enduro_next = frame.enduro_gate.map(|[next, _]| next - 1);
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
                    won: state.split.place.unwrap_or(frame.position) == 1,
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
        // Hall of Fame and personal record messages (`002e2cf0`, 1.5 s).
        for message in state.race.take_record_messages() {
            state
                .hud
                .say(message, atv_core::hall_of_fame::MESSAGE_SECONDS);
        }
        // Room standings first: the HUD's place and finish sign use them.
        if let Some(rows) = &state.room
            && state.settings.room.is_some()
        {
            room_standings(&mut frame, rows);
        }
        frame.hud = Some(state.hud.step(
            STEP,
            &race_hud::Input {
                racing: frame.phase == "racing",
                finished: frame.phase == "finished",
                laps: frame.laps,
                lap_times: &frame.lap_times,
                best_lap: frame.best_lap,
                place: state.split.place.unwrap_or(frame.position),
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
    /// A room race: start the countdown with `remaining` seconds to GO
    /// (the host's clock less half the relay round trip). Idempotent.
    pub fn room_go(&mut self, remaining: f32) {
        if let Some(state) = &mut self.race
            && state.settings.room.is_some()
            && state.race.in_intro()
        {
            state.race.start_countdown();
            state.race.countdown = remaining.clamp(0., atv_core::race::COUNTDOWN);
        }
    }

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

    /// A room race host's CPUs for the room (research/MULTIPLAYER.md "Room
    /// race CPUs"): each one's race progress, vehicle state and rider.
    pub fn room_cpu_frames(&self) -> Vec<RoomCpuFrame> {
        let Some(state) = self.race.as_ref().filter(|s| s.settings.room.is_some()) else {
            return Vec::new();
        };
        let race = &state.race;
        state
            .cpus
            .iter()
            .enumerate()
            .map(|(index, cpu)| RoomCpuFrame {
                index,
                quad: state.roster.get(index).map_or(1, |e| e.quad as u16),
                lap: race.laps_completed(Some(index)),
                distance: race.race_distance(index),
                finish: race.cpu_finish(index),
                frame: cpu.vehicle.frame(),
                recipe: cpu.recipe(),
                wreck: cpu.rider_wreck(),
            })
            .collect()
    }

    /// A room race guest's CPUs from the host's presented motion
    /// (index, snapshot, stale), `dt` seconds after the last call.
    pub fn present_room_cpus(
        &mut self,
        poses: &[(u8, atv_net::Snapshot, bool)],
        dt: f32,
    ) -> Result<()> {
        let Some(state) = self.race.as_mut().filter(|s| !s.puppets_retired) else {
            return Ok(());
        };
        let nodes: &[atv_core::mesh::Node] = match state.cpu_rider.as_deref() {
            Some(cpu) => &cpu.mesh.nodes,
            None => &self.rider.mesh.nodes,
        };
        for (index, snapshot, stale) in poses {
            let index = usize::from(*index);
            let Some(puppet) = state.puppets.get_mut(index) else {
                continue;
            };
            puppet.present(nodes, snapshot, dt, *stale)?;
            let origin = snapshot.atv.position;
            state
                .race
                .reposition_cpu(index, origin, yaw(snapshot.atv.rotation), None);
        }
        Ok(())
    }

    /// The room race host left (research/MULTIPLAYER.md "Room race CPUs"):
    /// a guest's CPUs stop being drawn and pushed against.
    pub fn retire_room_cpus(&mut self) {
        if let Some(state) = self.race.as_mut() {
            state.puppets_retired = true;
            for p in &mut state.puppets {
                p.body = None;
            }
        }
    }

    /// World pose of each CPU's quad and rider for drawing.
    pub fn cpu_poses(&self) -> Vec<super::Pose> {
        let Some(state) = &self.race else {
            return Vec::new();
        };
        // A room race guest draws the host's CPUs (none once retired), on
        // the grid until their motion arrives.
        if !state.puppets.is_empty() {
            if state.puppets_retired {
                return Vec::new();
            }
            let rest = |index: usize| {
                let cpu = state.race.opponents().get(index)?;
                let (quads, rider) = state.rest_pose.as_ref()?;
                let (s, c) = (cpu.yaw / 2.).sin_cos_d();
                let world = super::quaternion_world(cpu.origin, [c, 0., s, 0.]);
                Some(super::Pose {
                    quad_world: world,
                    quad_nodes: quads.get(index)?.clone(),
                    rider_world: world,
                    rider_nodes: rider.clone(),
                })
            };
            return state
                .puppets
                .iter()
                .enumerate()
                .filter_map(|(index, p)| p.pose.clone().or_else(|| rest(index)))
                .collect();
        }
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
