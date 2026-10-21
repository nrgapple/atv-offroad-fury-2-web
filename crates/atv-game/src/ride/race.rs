//! Single-player races: the original course (`<venue>nate.tdf`), grid,
//! pre-race camera, countdown, lap/polygon progress, CPU riders, rider
//! contacts and the after-finish director. Ports atv-web's operations 95,
//! 96, 105, 106, 113 and 114 and the race paths of its op 50 batch
//! (`race_controls`, `step_race`, `follow_camera`), calling atv-core directly.
//! Stadium crowd cues, camera-flash bursts, Supercross fireworks and the
//! National drop gate are presentation that is not connected yet.
use super::{Ride, STEP, cpu::CpuModel, cpu::CpuRacer, cpu::World};
use crate::Result;
use atv_core::math::Det;
use atv_core::{
    camera::CameraFrame,
    cpu_avoidance::Peer,
    race::{Course, Frame, Race},
    race_ai::{Context, Driver, DriverPose, Options},
    race_director::{self, Cameras, Director},
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
        }
    }
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
}

pub struct RaceState {
    pub race: Race,
    pub cpus: Vec<CpuRacer>,
    /// Original CPU quads and riders by slot (for drawing).
    pub roster: Vec<Entry>,
    pub models: Vec<CpuModel>,
    /// Each CPU's quad and outfitted rider for drawing (taken by the app).
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
        let mut race = Race::new(course, settings.laps, names, settings.difficulty)?;
        // Race creation 001f9690: difficulty plus the EATDIRT/BRINGIT flags.
        let options = Options {
            aggressive: settings.aggressive,
            all_out: settings.all_out,
            ..Options::difficulty(settings.difficulty)
        };
        let nodes = &self.rider.mesh.nodes;
        let (rig, _) = self.mount.quad()?;
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
        let (_, vehicle) = self.mount.quad_mut()?;
        super::race::place_racer(vehicle, &self.sim.terrain)?;
        self.frame = vehicle.frame();
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
        });
        Ok(())
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
        let (_, vehicle) = self.mount.quad()?;
        let mut pose: DriverPose = (&vehicle.frame()).into();
        pose.preload_rates = vehicle.preload_rates();
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
                scene: vehicle.scene_collision(),
                clearance: contact.as_ref().map_or(0., |c| pose.origin[1] - c.height),
                terrain_normal: contact.as_ref().map_or([0., 1., 0.], |c| c.normal),
                station: Some(race.station(None)),
            },
        )?;
        Ok(Some(input))
    }

    /// Race progress, CPU racers and rider contact after the player's step.
    pub(super) fn step_race(&mut self) -> Result<()> {
        let Some(state) = &mut self.race else {
            return Ok(());
        };
        let (_, vehicle) = self.mount.quad_mut()?;
        let player = vehicle.frame();
        let race = &mut state.race;
        race.step(player.origin, STEP);
        let terrain = &self.sim.terrain;
        let hold = race.holding();
        let nodes = &self.rider.mesh.nodes;
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
        let dynamic = vehicle
            .scene_collision()
            .map_or_else(Vec::new, |s| s.dynamic().to_vec());
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
        vehicle.set_race_checkpoint(Some(on_line(race.checkpoint(None))?));
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
                    let vehicle = if own == 0 {
                        &mut *vehicle
                    } else {
                        &mut state.cpus[own - 1].vehicle
                    };
                    let contact = vehicle.peer_contact(origin, velocity, orientation, STEP, tie);
                    if contact && tie > 0. {
                        race.contacts += 1;
                    }
                }
            }
        }
        let mut frame = race.frame(yaw(player.orientation));
        let _ = race.take_finishes();
        let too_deep = Some(vehicle.frame().water_failure)
            .filter(|w| w.submerged)
            .map(|w| w.remaining);
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
