//! Single-player rides on an ATV or a motorcycle: the original vehicle
//! physics, rider animation, wreck recovery and follow camera stepped at the
//! fixed 60 Hz.
//!
//! This is the Rust engine's port of the browser path (web/game.js
//! startSimulation/pumpSimulation and atv-web's simulation operations 23, 24,
//! 46, 47 and the op 50 batch; motorcycles: crates/atv-web/src/motorcycles.rs).
//! It calls atv-core directly in the same order; trains and the optional flip
//! presentation layers are not part of it yet (rooms: `crate::multiplayer`).
mod bike;
pub mod bindings;
pub mod cpu;
pub mod gear;
pub mod hype;
pub(crate) mod load;
pub mod present;
pub mod race;
pub mod script;
pub mod sound;
pub mod textures;
pub mod view;

use crate::Result;
use atv_core::math::Det;
pub use atv_core::wreck::Phase;
use atv_core::{
    browser_input::{BrowserDrive, TouchIntent},
    camera::{CameraFrame, FollowCamera, FollowInput, WreckActor, WreckCamera},
    collision::CollisionRayTree,
    dynamics,
    materials::TerrainMaterial,
    mesh::Mesh,
    motorcycle::Motorcycle,
    obstacle::StaticObstacle,
    physics_data::PhysicsData,
    quad_pose::QuadPose,
    rider_pose, stunt, stunt_score, terrain,
    vehicle::{Vehicle, VehicleFrame, VehicleInput},
    water_animation::LiveField,
    wreck_animation,
};
pub use load::{load, load_with};
use std::{collections::HashMap, rc::Rc};

/// Fixed simulation step (the browser's 1/60 s batches).
pub const STEP: f32 = 1. / 60.;

/// A drawable original model: its mesh and per-material base textures.
pub struct Model {
    pub mesh: Mesh,
    pub textures: HashMap<String, textures::MaterialTexture>,
}

/// The Garage choice the ride loads: an ATV (1-26) or a motorcycle (27-46,
/// `atv_core::player_models`), and the rider (0 male, 1 female).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub quad: u16,
    pub rider: u16,
}

impl Default for Selection {
    /// The browser's default Garage selection: quad 1, male rider.
    fn default() -> Self {
        Self { quad: 1, rider: 0 }
    }
}

/// Player controls for one batch, as web/bindings.js `read` resolves them
/// before the worker's op 24 (the same value is reused for every step).
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub lean: f32,
    pub reverse: bool,
    pub reverse_chord_brake: Option<f32>,
    /// 1 while the preload key is held, -1 on its release, else absent.
    pub preload_axis: Option<f32>,
    /// Keyboard gas/brake for the authored brake-then-reverse adapter.
    pub keyboard_drive: Option<TouchIntent>,
    /// Touch drive intent (web/touch.js `sample().drive`).
    pub touch_drive: Option<TouchIntent>,
    /// Circle=1, Triangle=2, L1=4 with their direction axes.
    pub stunt_modifiers: u8,
    pub stunt_x: f32,
    pub stunt_y: f32,
    /// Cancellation generation; a change cancels a pending preload gesture.
    pub generation: u64,
    /// Motorcycle clutch held (web/bindings.js `clutch`; ATVs ignore it).
    pub clutch: bool,
    /// Alpha Arcade flips: the keyboard/controller steering that spins the
    /// quad in the air (game.js `input.arcade_spin`).
    pub arcade_spin: Option<f32>,
}

/// Camera mode of the latest camera step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Follow,
    Wreck,
    /// A race's pre-race shots (authored per course; no chase adaptation).
    Intro,
    /// The after-finish trackside director.
    Tv,
}

/// Loaded original venue state the simulation reuses on every reset.
struct Simulation {
    terrain: terrain::Terrain,
    materials: [TerrainMaterial; 8],
    water: Option<LiveField>,
    obstacles: Vec<StaticObstacle>,
    mud_obstacles: Vec<bool>,
    scene_partition: Option<atv_core::scene_partition::Config>,
    ecosystem: Option<atv_core::ecosystem::Ecosystem>,
    boundary: Option<atv_core::boundary_launch::Config>,
    extended_boundary: bool,
    camera: FollowCamera,
    /// ENV08's train, shared with the renderer; an ATV ride steps it with
    /// its physics (collision heights, moving obstacles) as atv-web does.
    train: Option<Rc<std::cell::RefCell<crate::train::Train>>>,
}

/// The train cars as moving scene obstacles (atv-web
/// `train_collision_objects`, op 69).
fn train_obstacles(
    train: &crate::train::Train,
) -> Result<Vec<atv_core::scene_collision::DynamicObstacle>> {
    train
        .collision
        .iter()
        .map(|(car, mesh)| {
            let motion = train
                .runtime
                .motion_state(*car)
                .ok_or("train collision body index out of range")?;
            Ok(atv_core::scene_collision::DynamicObstacle {
                obstacle: StaticObstacle {
                    mesh: mesh.clone(),
                    world_matrix: motion.current,
                },
                motion,
            })
        })
        .collect()
}

/// The ATV's original rig (ops 20-22, 44, 48, 51-53): physics, linkage,
/// collision rays, the rider animator and the wreck rig.
struct QuadRig {
    physics: PhysicsData,
    quad_pose: Rc<QuadPose>,
    chassis_points: Vec<[f32; 3]>,
    wreck_rays: Rc<CollisionRayTree>,
    obstacle_rays: Rc<CollisionRayTree>,
    animator: rider_pose::Animator,
    wreck: wreck_animation::Animator,
}

/// The ATV2 rider rig race CPUs fork when the player rides a motorcycle:
/// MX vs. ATV fields keep the CPUs on their quads with the ATV rider
/// (`loadAtvRider` for race CPUs in web/game.js).
pub struct CpuRider {
    pub(super) animator: rider_pose::Animator,
    pub(super) wreck: wreck_animation::Animator,
    pub(super) mesh: Mesh,
}

/// The player's vehicle: an ATV with its rig, or a motorcycle, whose
/// original model carries its own rider, wreck and tuning (atv-web keeps the
/// two-wheel path separate from the four-wheel solver).
enum Mount {
    Quad {
        rig: Box<QuadRig>,
        vehicle: Box<Vehicle>,
    },
    Bike(Box<Motorcycle>),
}

/// What [`Ride::start`] places: the quad rig, or a motorcycle model.
enum Seed {
    Quad(Box<QuadRig>),
    Bike(Box<atv_core::motorcycle::Model>),
}

pub struct Ride {
    sim: Simulation,
    mount: Mount,
    browser_drive: BrowserDrive,
    input_generation: Option<u64>,
    rider_mode: Option<rider_pose::Mode>,
    wreck_camera: Option<WreckCamera>,
    flip_heading: Option<[f32; 3]>,
    hud: atv_core::hud::State,
    flip_points: (f32, f32),
    start: ([f32; 3], f32),
    frame: VehicleFrame,
    camera_frame: CameraFrame,
    camera_mode: CameraMode,
    applied: Controls,
    steps: u64,
    /// The actor sound position (`001e7600`, atv-web rider_audio_position).
    rider_audio_position: [f32; 3],
    placement: u32,
    sound: Option<sound::AudioStep>,
    race: Option<race::RaceState>,
    /// Stunt chains landed/ended since the race last read them (atv-web's
    /// `stunt_landed`/`stunt_result`, for flash bursts and crowd cues).
    stunt_landed: bool,
    stunt_result: Option<bool>,
    /// Settings → Extras choices the simulation reads each step (game.js
    /// `input.flips`, `input.realistic_train`).
    pub options: RideOptions,
    selection: Selection,
    appearance: gear::Appearance,
    pub quad: Model,
    pub rider: Model,
    /// The relay clock (ms) for the next step's shared train phase.
    pub train_server_ms: Option<f64>,
}

impl Drop for Ride {
    fn drop(&mut self) {
        // The venue (and its train) can outlive the ride's physics.
        if let Some(train) = &self.sim.train {
            train.borrow_mut().driven = false;
        }
    }
}

const IDENTITY_MATRIX: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

/// atv-web multiplayer_world.rs `platform`: grounded on a train car's
/// obstacle support, which car and its matrix.
fn vehicle_platform(vehicle: &Vehicle, train: &crate::train::Train) -> Option<(usize, [f32; 16])> {
    let frame = vehicle.frame();
    if !frame.grounded.iter().any(|v| *v) || !frame.wheel_obstacle_support.iter().any(|v| *v) {
        return None;
    }
    let support = vehicle.network_platform()?;
    train.collision.iter().find_map(|(car, _)| {
        let motion = train.runtime.motion_state(*car)?;
        (motion.current == support.current).then_some((*car, motion.current))
    })
}

/// Optional simulation choices (Settings → Extras): the Unleashed flip
/// gestures and realistic train riding. Off is the original game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RideOptions {
    pub flips: bool,
    pub realistic_train: bool,
    /// Alpha Arcade flips (authored analog air rotation; replaces the
    /// Unleashed gestures while on).
    pub arcade_flips: bool,
}

/// World pose of the player's quad and rider for drawing.
pub struct Pose {
    pub quad_world: [f32; 16],
    pub quad_nodes: Vec<[f32; 16]>,
    pub rider_world: [f32; 16],
    pub rider_nodes: Vec<[f32; 16]>,
}

fn vehicle_world(frame: &VehicleFrame) -> [f32; 16] {
    let b = dynamics::basis(frame.orientation);
    let p = frame.origin;
    [
        b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1], b[2][2],
        0., p[0], p[1], p[2], 1.,
    ]
}

/// The browser's vehicleMatrix: the same rotation built from the quaternion.
pub fn quaternion_world(origin: [f32; 3], orientation: [f32; 4]) -> [f32; 16] {
    let [w, x, y, z] = orientation;
    [
        1. - 2. * (y * y + z * z),
        2. * (x * y + w * z),
        2. * (x * z - w * y),
        0.,
        2. * (x * y - w * z),
        1. - 2. * (x * x + z * z),
        2. * (y * z + w * x),
        0.,
        2. * (x * z + w * y),
        2. * (y * z - w * x),
        1. - 2. * (x * x + y * y),
        0.,
        origin[0],
        origin[1],
        origin[2],
        1.,
    ]
}

impl Ride {
    #[allow(clippy::too_many_arguments)]
    fn start(
        sim: Simulation,
        selection: Selection,
        start: ([f32; 3], f32),
        seed: Seed,
        quad: Model,
        rider: Model,
        race: Option<(race::Setup, race::Settings)>,
    ) -> Result<Self> {
        let mut sim = sim;
        // Motorcycles keep the presentation-only train (the bike scene does
        // not install it yet).
        if matches!(seed, Seed::Bike(_)) {
            sim.train = None;
        }
        if let Some(train) = &sim.train {
            train.borrow_mut().driven = true;
        }
        let mount = match seed {
            Seed::Quad(mut rig) => {
                let vehicle = Box::new(Self::place(&sim, &mut rig, &quad.mesh, start)?);
                Mount::Quad { rig, vehicle }
            }
            Seed::Bike(model) => Mount::Bike(Box::new(bike::start(&mut sim, *model, start)?)),
        };
        let mut ride = Self {
            sim,
            mount,
            browser_drive: BrowserDrive::default(),
            input_generation: None,
            rider_mode: None,
            wreck_camera: None,
            flip_heading: None,
            hud: atv_core::hud::State::default(),
            flip_points: (0., 0.),
            start,
            frame: bike::placeholder_frame(),
            camera_frame: CameraFrame {
                position: [0.; 3],
                target: [0.; 3],
                fov: 60.,
                preset: 0,
            },
            camera_mode: CameraMode::Follow,
            applied: Controls::default(),
            steps: 0,
            rider_audio_position: [0.; 3],
            placement: 0,
            sound: None,
            race: None,
            stunt_landed: false,
            stunt_result: None,
            options: RideOptions::default(),
            selection,
            appearance: gear::Appearance::default(),
            train_server_ms: None,
            quad,
            rider,
        };
        ride.after_placement()?;
        // A race takes the grid before the first camera step (op 96).
        if let Some((setup, settings)) = race {
            ride.begin_race(setup, settings, settings.intro)?;
        }
        // startSimulation: op 45 builds the camera, then op 46 steps it once.
        ride.step_camera(0.)?;
        Ok(ride)
    }

    /// Op 23: a new vehicle at `start` with every original collision source.
    fn place(
        sim: &Simulation,
        rig: &mut QuadRig,
        mesh: &Mesh,
        (origin, yaw): ([f32; 3], f32),
    ) -> Result<Vehicle> {
        let mut vehicle = Vehicle::new(mesh, &rig.physics, origin, yaw)?;
        vehicle.enable_wrecks(&rig.physics)?;
        if let Some(config) = sim.boundary {
            vehicle.set_boundary(config, sim.extended_boundary)?;
        }
        vehicle.set_linkage(rig.quad_pose.clone());
        rig.wreck.reset();
        rig.wreck.set_boundary_flight(false);
        rig.wreck.set_static_obstacles(sim.obstacles.clone());
        vehicle.set_materials(sim.materials);
        vehicle.set_chassis_points(&rig.chassis_points)?;
        vehicle.set_wreck_rays(rig.wreck_rays.clone())?;
        vehicle.set_obstacles(rig.obstacle_rays.clone(), sim.obstacles.clone())?;
        if let Some(config) = sim.scene_partition {
            vehicle.set_scene_partition(config, sim.ecosystem.as_ref(), &sim.terrain)?;
        }
        if let Some(scene) = vehicle.scene_collision_mut() {
            scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
            if let Some(train) = &sim.train {
                scene.install_dynamic(train_obstacles(&train.borrow())?)?;
            }
        }
        Ok(vehicle)
    }

    fn after_placement(&mut self) -> Result<()> {
        match &mut self.mount {
            Mount::Quad { rig, vehicle } => {
                rig.animator.reset(&self.rider.mesh.nodes)?;
                rig.animator.reset_score_session();
                self.frame = vehicle.frame();
            }
            Mount::Bike(bike) => self.frame = bike::summary(bike.frame()),
        }
        self.hud.reset();
        self.flip_points = (0., 0.);
        self.rider_mode = None;
        self.browser_drive.reset();
        self.input_generation = None;
        self.wreck_camera = None;
        Ok(())
    }

    /// The player rides a motorcycle (atv-web's `vehicle_kind`).
    pub fn is_motorcycle(&self) -> bool {
        matches!(self.mount, Mount::Bike(_))
    }

    /// `rider_world_matrix`: the detached rider's world while wrecking (the
    /// ATV's wreck rig; the motorcycle's rider off Riding).
    pub fn detached_rider_world(&self) -> Option<[f32; 16]> {
        match &self.mount {
            Mount::Quad { rig, .. } => rig.wreck.frame().map(|w| w.world_matrix),
            Mount::Bike(bike) => {
                (bike.frame().wreck.phase != Phase::Riding).then(|| bike.rider_world())
            }
        }
    }

    /// The ATV rider's animation recipe (what a multiplayer snapshot sends
    /// so receivers sample the same clips); None on a motorcycle.
    pub fn rider_recipe(&self) -> Option<rider_pose::Recipe> {
        match &self.mount {
            Mount::Quad { rig, .. } => Some(rig.animator.recipe()),
            Mount::Bike(_) => None,
        }
    }

    /// The detached ATV rider's world matrix and velocity while the wreck
    /// rig runs (multiplayer snapshots replicate the root, not the clip).
    pub fn rider_wreck(&self) -> Option<([f32; 16], [f32; 3])> {
        match &self.mount {
            Mount::Quad { rig, .. } => rig
                .wreck
                .frame()
                .map(|w| (w.world_matrix, w.linear_velocity)),
            Mount::Bike(_) => None,
        }
    }

    /// One train step with the ride's scene: each car's height comes from
    /// the scene and its collision body is republished (atv-web
    /// `step_train_scene`). In a room the phase follows the relay clock
    /// (`train_server_ms`, multiplayer_world.rs `synchronize_train`).
    fn step_train(&mut self) -> Result<()> {
        let Some(train) = self.sim.train.clone() else {
            return Ok(());
        };
        let Mount::Quad { vehicle, .. } = &mut self.mount else {
            return Ok(());
        };
        let mut train = train.borrow_mut();
        let train = &mut *train;
        let attached = vehicle_platform(vehicle, train);
        let Some(scene) = vehicle.scene_collision_mut() else {
            train.runtime.step(STEP, |_| Ok(None))?;
            return Ok(());
        };
        let bindings: Vec<usize> = train.collision.iter().map(|(car, _)| *car).collect();
        let scene = std::cell::RefCell::new(scene);
        let step = |runtime: &mut atv_core::train::Runtime| {
            runtime.step_with_scene(
                STEP,
                |car, previous| {
                    let own = bindings.iter().position(|b| *b == car);
                    atv_core::train::height_geometry(&scene.borrow(), own, previous)
                },
                |car, motion| {
                    if let Some(index) = bindings.iter().position(|b| *b == car) {
                        scene.borrow_mut().update_dynamic(index, motion)?;
                    }
                    Ok(())
                },
            )
        };
        let Some(server_ms) = self.train_server_ms.take() else {
            step(&mut train.runtime)?;
            return Ok(());
        };
        let duration = train.runtime.route.duration();
        if !duration.is_finite() || duration <= 0. {
            step(&mut train.runtime)?;
            return Ok(());
        }
        let target = (server_ms / 1000.).rem_euclid(f64::from(duration)) as f32;
        let error =
            (target - train.runtime.time + duration * 0.5).rem_euclid(duration) - duration * 0.5;
        if error.abs() > 0.25 {
            // A large resync: prime the route's heading filter and velocity
            // pair over 16 steps, carrying a rider standing on a car along.
            train.runtime.time = (target - 16. * STEP).rem_euclid(duration);
            for _ in 0..16 {
                step(&mut train.runtime)?;
            }
            if let Some((car, before)) = attached
                && let Some(motion) = train.runtime.motion_state(car)
            {
                let origin = vehicle.frame().origin;
                let local = atv_core::obstacle::transform_point(
                    &atv_core::obstacle::relative_transform(&IDENTITY_MATRIX, &before),
                    origin,
                );
                let moved = atv_core::obstacle::transform_point(&motion.current, local);
                vehicle.translate_network(std::array::from_fn(|i| moved[i] - origin[i]));
            }
        } else {
            // Gentle phase correction avoids impulses from clock jitter.
            train.runtime.time = (train.runtime.time
                + (error - STEP).clamp(-STEP * 0.25, STEP * 0.25))
            .rem_euclid(duration);
            step(&mut train.runtime)?;
        }
        Ok(())
    }

    /// Test hook (`?trainPlace=`): places the ATV on the first car whose
    /// model contains `model`, 17 ft up and 10 ft along its deck, moving with
    /// it (test_multiplayer_features.mjs places riders the same way).
    pub fn place_on_train(&mut self, model: &str) -> Result<()> {
        let (matrix, velocity) = {
            let train = self.sim.train.as_ref().ok_or("no train here")?.borrow();
            let frame = train.runtime.frame()?;
            let car = frame
                .cars
                .iter()
                .position(|c| c.model.to_ascii_lowercase().contains(model))
                .ok_or("train car not found")?;
            let motion = train
                .runtime
                .motion_state(car)
                .ok_or("train car not found")?;
            (motion.current, motion.linear_velocity())
        };
        let m = matrix;
        let origin = std::array::from_fn(|i| m[12 + i] + m[4 + i] * 17. + m[8 + i] * 10.);
        self.place_at((origin, m[8].atan2_d(m[10])))?;
        if let Mount::Quad { vehicle, .. } = &mut self.mount {
            vehicle.set_network_velocity(velocity)?;
            self.frame = vehicle.frame();
        }
        Ok(())
    }

    /// The train car the ATV is standing on and that car's current matrix
    /// (multiplayer snapshots send poses relative to it).
    pub fn train_platform(&self) -> Option<(usize, [f32; 16])> {
        let Mount::Quad { vehicle, .. } = &self.mount else {
            return None;
        };
        let train = self.sim.train.as_ref()?.borrow();
        vehicle_platform(vehicle, &train)
    }

    /// A train car's current matrix (to rebase other riders' platform poses).
    pub fn train_car(&self, car: usize) -> Option<[f32; 16]> {
        let train = self.sim.train.as_ref()?.borrow();
        train.runtime.motion_state(car).map(|m| m.current)
    }

    /// The ride's train clock (seconds along the route).
    pub fn train_time(&self) -> Option<f32> {
        self.sim.train.as_ref().map(|t| t.borrow().runtime.time)
    }

    /// The ATV's scene collision (props and moving objects), when loaded.
    pub fn scene_collision(&self) -> Option<&atv_core::scene_collision::SceneCollision> {
        match &self.mount {
            Mount::Quad { vehicle, .. } => vehicle.scene_collision(),
            Mount::Bike(_) => None,
        }
    }

    /// Places the player at `origin` facing `yaw` (a multiplayer arrival
    /// beside another rider; ops 23/47 as Reset does).
    pub fn place_beside(&mut self, origin: [f32; 3], yaw: f32) -> Result<()> {
        self.place_at((origin, yaw))
    }

    /// One owner-local contact against another rider's yawed box
    /// (`atv_core::peer_contact`, authored); true when they touched.
    pub fn peer_contact(
        &mut self,
        origin: [f32; 3],
        velocity: [f32; 3],
        orientation: [f32; 4],
        tie: f32,
    ) -> bool {
        match &mut self.mount {
            Mount::Quad { vehicle, .. } => {
                vehicle.peer_contact(origin, velocity, orientation, STEP, tie)
            }
            Mount::Bike(bike) => bike.peer_contact(origin, velocity, orientation, STEP, tie),
        }
    }

    /// The player's motorcycle, when riding one.
    pub fn motorcycle(&self) -> Option<&Motorcycle> {
        match &self.mount {
            Mount::Bike(bike) => Some(bike),
            Mount::Quad { .. } => None,
        }
    }

    /// Dresses the ATV rider in the Garage gear and paints the quad
    /// (`rideAppearance` in game.js); motorcycles keep their original look.
    /// Call before handing the models to the renderer.
    pub async fn dress(
        &mut self,
        content: &atv_content::Content,
        appearance: gear::Appearance,
    ) -> Result<()> {
        if self.is_motorcycle() || !appearance.valid() {
            return Ok(());
        }
        self.appearance = appearance;
        if appearance.gear.iter().all(|g| *g == 0) {
            return Ok(());
        }
        let Some(catalog) = gear::catalog(content).await? else {
            return Ok(());
        };
        let bytes = content.file("/RIDER/RIDER_MT.RES").await?;
        let materials = atv_core::resource::Resource::parse(&bytes)?;
        self.rider.textures =
            gear::rider_textures(&self.rider.mesh, &appearance, &catalog, &materials)?;
        Ok(())
    }

    /// The appearance the ride was dressed in.
    pub fn appearance(&self) -> gear::Appearance {
        self.appearance
    }

    /// The quad paint (`playerPaint`: mode and hue), original on a bike.
    pub fn paint(&self) -> [f32; 2] {
        gear::PAINT_PALETTE[usize::from(self.appearance.paint)]
    }

    /// The player's committed stunt points (the race SCORE overlay; the
    /// motorcycle's frame carries none, as in atv-web).
    pub fn committed_points(&self) -> f32 {
        match &self.mount {
            Mount::Quad { rig, .. } => rig.animator.score_frame().committed_points,
            Mount::Bike(_) => 0.,
        }
    }

    /// The Garage choice this ride loaded.
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// The physics terrain (contact planes).
    pub fn terrain(&self) -> &terrain::Terrain {
        &self.sim.terrain
    }

    /// The motorcycle's mud on the bike and rider, 0-1 (the larger of its
    /// two wheels' amounts; `motorcycleMudAmount` in game.js). ATVs: 0.
    pub fn mud_amount(&self) -> f32 {
        self.motorcycle().map_or(0., |bike| {
            bike.frame().mud_amount.into_iter().fold(0., f32::max)
        })
    }

    /// Reset ATV: back to the scene start with the follow camera reset (ops
    /// 23, 47 and one op 46 step).
    pub fn reset(&mut self) -> Result<()> {
        // During a race, Reset returns to the last on-track point facing the
        // race direction; after the finish it does nothing, and before GO the
        // event restarts (straight to the countdown).
        if let Some(state) = &self.race {
            let racing = state.race.running() && !state.race.player_finished();
            if state.race.player_finished() {
                return Ok(());
            }
            let Some(frame) = state.frame.as_ref().filter(|_| racing) else {
                return self.restart_race();
            };
            let origin = atv_core::spawn::start_origin(
                &self.sim.terrain,
                frame.track_point.map(f64::from),
                f64::from(frame.track_heading),
                true,
                None,
            )?;
            return self.place_at((
                [origin[0] as f32, origin[1] as f32, origin[2] as f32],
                frame.track_heading,
            ));
        }
        self.place_at(self.start)
    }

    /// Ops 23, 47 and one op 46 step at `start` (a race places on the ground).
    fn place_at(&mut self, start: ([f32; 3], f32)) -> Result<()> {
        self.placement = self.placement.wrapping_add(1);
        self.sound = None;
        match &mut self.mount {
            Mount::Quad { rig, vehicle } => {
                **vehicle = Self::place(&self.sim, rig, &self.quad.mesh, start)?;
                if self.race.is_some() {
                    race::place_racer(vehicle, &self.sim.terrain)?;
                }
            }
            Mount::Bike(bike) => bike::place(&self.sim, bike, Some(start))?,
        }
        self.after_placement()?;
        self.sim.camera.reset();
        self.step_camera(0.)
    }

    /// Change camera (op 47 with cycle).
    pub fn cycle_camera(&mut self) {
        self.sim.camera.cycle();
    }

    /// One fixed step: vehicle and rider (op 24), then the camera (op 46).
    pub fn step(&mut self, controls: &Controls, orbit: f32) -> Result<()> {
        if self.is_motorcycle() {
            self.step_bike(controls)?;
            self.step_race()?;
            // atv-web reads the bike's frame after the race step, which can
            // move it (rider contact).
            if let Mount::Bike(bike) = &self.mount {
                self.frame = bike::summary(bike.frame());
            }
            self.step_camera(orbit)?;
            self.steps += 1;
            self.sound = Some(self.capture_bike_sound(controls));
            return Ok(());
        }
        self.step_train()?;
        // During the countdown and after the finish the race supplies the
        // player's controls; tricks and touch/keyboard intents are dropped.
        match self.race_controls()? {
            Some(input) => self.step_vehicle(
                &Controls {
                    throttle: input.throttle,
                    brake: input.brake,
                    steer: input.steer,
                    lean: input.lean,
                    ..Default::default()
                },
                true,
            )?,
            None => self.step_vehicle(controls, false)?,
        }
        self.step_race()?;
        self.step_camera(orbit)?;
        self.steps += 1;
        self.sound = Some(self.capture_sound()?);
        Ok(())
    }

    /// The step's sound inputs (simulation_frame's audio fields).
    fn capture_sound(&mut self) -> Result<sound::AudioStep> {
        let Mount::Quad { rig, vehicle } = &mut self.mount else {
            return Err("motorcycle sound is not ported yet".into());
        };
        let frame = &self.frame;
        let events = vehicle.take_audio_events();
        let surfaces = vehicle.wheel_effect_state().surfaces;
        let matrices = match vehicle.linkage_matrices() {
            Some(matrices) => matrices.to_vec(),
            None => rig.quad_pose.sample(
                frame.visual_steering_angle,
                frame.wheel_angles,
                frame.wheel_compression,
            )?,
        };
        let world = vehicle_world(frame);
        let centers = rig
            .quad_pose
            .wheel_positions(&matrices)?
            .map(|point| atv_core::obstacle::transform_point(&world, point));
        let ground = self
            .sim
            .terrain
            .contact(frame.origin[0], frame.origin[2])?
            .map(|hit| hit.height);
        let water = self.sim.water.as_ref().map(|_| frame.water_height);
        let rider = rig.wreck.frame();
        Ok(sound::AudioStep {
            quad: sound::quad_frame(events, frame, surfaces, ground, water, &centers),
            rider_position: self.rider_audio_position,
            rider_impact: rider.as_ref().map_or([0.; 3], |r| r.sampled_impact_delta),
            rider_water: rider.as_ref().is_some_and(|r| r.water_touching),
            riding: frame.wreck.phase == Phase::Riding,
            throttle: self.applied.throttle,
            grounded: frame.grounded.iter().any(|g| *g),
            speed: frame.speed_magnitude,
            camera: atv_core::spatial_audio::CameraPose {
                position: self.camera_frame.position,
                target: self.camera_frame.target,
            },
            placement: self.placement,
            bike: None,
        })
    }

    /// Sound inputs of the latest step (None right after a reset).
    pub fn sound(&self) -> Option<&sound::AudioStep> {
        self.sound.as_ref()
    }

    /// Op 24. `race_input` marks controls the race imposed: they carry no
    /// input generation, so a pending preload gesture is not cancelled.
    fn step_vehicle(&mut self, controls: &Controls, race_input: bool) -> Result<()> {
        let dt = STEP;
        let input = VehicleInput {
            throttle: controls.throttle,
            brake: controls.brake,
            steer: controls.steer,
            lean: controls.lean,
            preload_axis: controls.preload_axis,
            reverse_chord_brake: controls.reverse_chord_brake,
            reverse: controls.reverse,
            lean_flick_rate: None,
        };
        let Mount::Quad { rig, vehicle } = &mut self.mount else {
            return Err("not an ATV".into());
        };
        let current = vehicle.frame();
        let input = self.browser_drive.resolve(
            input,
            controls.keyboard_drive,
            controls.touch_drive,
            current.grounded.iter().any(|c| *c),
            current.speed,
        )?;
        let sim = &mut self.sim;
        let safe = rig.animator.stunt_landing_safe();
        let forced_landing = rig.animator.landing_frame().is_some();
        if !race_input && self.input_generation != Some(controls.generation) {
            vehicle.cancel_preload_gesture();
            self.input_generation = Some(controls.generation);
        }
        vehicle.set_stunt_landing_safe(safe);
        vehicle.set_realistic_train(self.options.realistic_train);
        let arcade = self.options.arcade_flips;
        vehicle.set_flips(self.options.flips && !arcade)?;
        vehicle.set_arcade_flips(arcade)?;
        vehicle.set_arcade_spin(controls.arcade_spin.filter(|_| arcade))?;
        if controls.stunt_modifiers > 7 {
            return Err("invalid stunt modifiers".into());
        }
        vehicle.set_flip_controls(
            controls.stunt_modifiers,
            [controls.stunt_x, controls.stunt_y],
        )?;
        vehicle.set_rider_mode(self.rider_mode);
        vehicle.set_forced_landing(forced_landing);
        if let Some(water) = &mut sim.water {
            water.advance(dt)?;
        }
        if let Some(race) = &mut self.race
            && std::mem::take(&mut race.autopilot_reset)
        {
            vehicle.reset_now()?;
        }
        let wreck = &mut rig.wreck;
        let frame = vehicle.step_with_scene(&sim.terrain, input, dt, sim.water.as_ref(), || {
            wreck.boundary_random_words()
        })?;
        let nodes = &self.rider.mesh.nodes;
        // Deep water can invoke 001e35d0 before physics independently of
        // advance_wreck. Reset attached presentation/camera state too.
        if frame.reset_count != current.reset_count {
            rig.wreck.reset();
            rig.wreck.set_boundary_flight(false);
            rig.animator.reset(nodes)?;
            sim.camera.reset();
            self.wreck_camera = None;
            self.rider_mode = None;
            self.browser_drive.reset();
        }
        // 001e7600 publishes actor+290 before rider physics only while
        // Riding/Attached; detached audio retains the last actor point.
        if frame.wreck.phase == Phase::Riding
            || rig
                .wreck
                .frame()
                .is_none_or(|rider| rider.phase == wreck_animation::Phase::Attached)
        {
            self.rider_audio_position = frame.origin;
        }
        let (throttle, brake, reverse) = vehicle.applied_drive_controls();
        self.applied = Controls {
            throttle,
            brake,
            steer: input.steer,
            lean: input.lean,
            reverse,
            ..*controls
        };
        if frame.wreck.phase != Phase::Riding {
            // Never carry takeoff throttle/direction through recovery.
            self.browser_drive.reset();
            if !rig.wreck.active() {
                rig.wreck.set_boundary_flight(frame.boundary_launch.active);
                rig.wreck.begin(
                    frame.wreck.kind.ok_or("missing wreck kind")?,
                    rig.animator.channels(),
                    vehicle_world(&frame),
                    nodes,
                )?;
                rig.animator.abort_wreck_scores()?;
                rig.animator.reset(nodes)?;
            }
            update_wheelie(&mut rig.animator, &frame, dt)?;
            rig.wreck.step_with_scene(
                dt,
                wreck_animation::VehicleMotion {
                    world: vehicle_world(&frame),
                    linear_velocity: frame.linear_velocity,
                    local_angular_velocity: frame.local_angular_velocity,
                },
                nodes,
                &sim.terrain,
                &sim.materials,
                sim.water.as_ref(),
                vehicle.scene_collision_mut(),
                frame.skip_vegetation_query,
            )?;
            let grounded = rig.wreck.frame().is_some_and(|f| f.grounded);
            if vehicle.advance_wreck(dt, grounded)? {
                rig.wreck.reset();
                rig.wreck.set_boundary_flight(false);
                rig.animator.reset(nodes)?;
                sim.camera.reset();
                self.wreck_camera = None;
                self.rider_mode = None;
            }
            let frame = vehicle.frame();
            if let Some(water) = &mut sim.water {
                water.prepare_surface()?;
            }
            self.frame = frame;
            return self.update_hud(dt);
        }
        update_wheelie(&mut rig.animator, &frame, dt)?;
        let pose_input = vehicle.rider_pose_input();
        let stunt = stunt::Input {
            modifiers: controls.stunt_modifiers,
            axes: [controls.stunt_x, controls.stunt_y],
            grounded: frame.grounded.iter().any(|x| *x),
            horizontal_speed_squared: frame.linear_velocity[0] * frame.linear_velocity[0]
                + frame.linear_velocity[2] * frame.linear_velocity[2],
            crashed: false,
            disabled: false,
        };
        if let Some(landing) = frame.landing_action {
            rig.animator.request_landing(landing)?;
        }
        self.rider_mode = Some(rig.animator.step_controls(nodes, pose_input, stunt, dt)?);
        if let Some(water) = &mut sim.water {
            water.prepare_surface()?;
        }
        self.frame = frame;
        self.update_hud(dt)
    }

    /// Op 46: the follow camera, or the wreck camera while the rider is off.
    /// Op 46: the race director's trackside view after the finish, else the
    /// follow/wreck camera, which the pre-race shots override before GO.
    fn step_camera(&mut self, orbit: f32) -> Result<()> {
        let vehicle = self.live_frame();
        let (rider, forward) = (vehicle.origin, dynamics::basis(vehicle.orientation)[2]);
        let tv = self.race_director(rider)?;
        self.chase_camera(orbit)?;
        if let Some(pose) = tv {
            self.camera_frame.position = pose.position;
            self.camera_frame.target = pose.target;
            self.camera_frame.fov = pose.fov;
            self.camera_mode = CameraMode::Tv;
            return Ok(());
        }
        let follow = self.camera_frame;
        if let Some(pose) = self.race_intro(&follow, rider, forward)? {
            self.camera_frame.position = pose.position;
            self.camera_frame.target = pose.target;
            self.camera_frame.fov = pose.fov;
            self.camera_mode = CameraMode::Intro;
        }
        Ok(())
    }

    /// The vehicle's current state (after race contact), as the camera and
    /// race read it.
    fn live_frame(&self) -> VehicleFrame {
        match &self.mount {
            Mount::Quad { vehicle, .. } => vehicle.frame(),
            Mount::Bike(bike) => bike::summary(bike.frame()),
        }
    }

    /// The detached rider's origin and velocity while wrecking (the ATV's
    /// wreck rig; the motorcycle's rider leaves the bike off Riding).
    fn wrecked_rider(&self) -> Option<([f32; 3], [f32; 3])> {
        match &self.mount {
            Mount::Quad { rig, .. } => rig.wreck.frame().map(|r| (r.origin, r.linear_velocity)),
            Mount::Bike(bike) => (bike.frame().wreck.phase != Phase::Riding).then(|| {
                let w = bike.rider_world();
                ([w[12], w[13], w[14]], bike.rider_velocity())
            }),
        }
    }

    fn chase_camera(&mut self, orbit: f32) -> Result<()> {
        let dt = STEP;
        let vehicle = self.live_frame();
        // The follow camera flattens body forward, which reverses as a
        // backflip passes vertical. Hold the takeoff heading.
        let flipping = vehicle.flip.is_some_and(|f| f.active.is_some());
        if !flipping {
            self.flip_heading = None;
        } else if self.flip_heading.is_none() {
            self.flip_heading = Some(dynamics::basis(vehicle.orientation)[2]);
        }
        let basis = dynamics::basis(vehicle.orientation);
        let velocity = vehicle.linear_velocity;
        let local = dynamics::inverse_transform(basis, velocity);
        let terrain = &self.sim.terrain;
        let height = |point: [f32; 3]| {
            Ok(terrain
                .contact(point[0], point[2])?
                .map_or(point[1], |contact| contact.height))
        };
        if let Some((rider_origin, rider_velocity)) = self.wrecked_rider() {
            let actors = [
                WreckActor {
                    origin: vehicle.origin,
                    velocity,
                },
                WreckActor {
                    origin: rider_origin,
                    velocity: rider_velocity,
                },
            ];
            // atv-web passes the ATV's scene query; the bike's has none.
            let scene = match &self.mount {
                Mount::Quad { vehicle, .. } => vehicle.scene_collision(),
                Mount::Bike(_) => None,
            };
            if self.wreck_camera.is_none() {
                let mut camera = self.sim.camera.wreck_camera(actors)?;
                camera.set_static_obstacles(self.sim.obstacles.clone());
                self.wreck_camera = Some(camera);
            }
            self.camera_frame = self
                .wreck_camera
                .as_mut()
                .ok_or("wreck camera missing")?
                .step_with_scene(actors, basis[2], dt, height, scene)?;
            self.camera_mode = CameraMode::Wreck;
            return Ok(());
        }
        self.wreck_camera = None;
        self.camera_frame = self.sim.camera.step(
            FollowInput {
                origin: vehicle.origin,
                forward: self.flip_heading.unwrap_or(basis[2]),
                velocity,
                speed: ((velocity[0] * velocity[0] + velocity[1] * velocity[1])
                    + velocity[2] * velocity[2])
                    .sqrt(),
                local_forward_speed: local[2],
                yaw_rate: if flipping {
                    0.
                } else {
                    vehicle.local_angular_velocity[1]
                },
                wheel_contact: vehicle.grounded.iter().any(|v| *v),
                orbit,
            },
            dt,
            height,
        )?;
        self.camera_mode = CameraMode::Follow;
        Ok(())
    }

    fn update_hud(&mut self, dt: f32) -> Result<()> {
        let frame = &self.frame;
        let Mount::Quad { rig, .. } = &mut self.mount else {
            return Err("not an ATV".into());
        };
        let animator = &mut rig.animator;
        let hud = &mut self.hud;
        for event in animator.take_score_notifications() {
            match event {
                stunt_score::Notification::Wheelie { event } => hud.wheelie_event(event),
                stunt_score::Notification::WheelieClearedForStunt => hud.clear_wheelie_for_stunt(),
                stunt_score::Notification::DisplayUpdated { id, .. } => {
                    hud.name_event(stunt::NAMES[id]);
                }
                stunt_score::Notification::ChainFinished {
                    success,
                    display_count,
                    ..
                } => {
                    self.stunt_landed |= success;
                    self.stunt_result = Some(success);
                    hud.finish_event(success, display_count)
                }
                _ => {}
            }
        }
        let score = animator.score_frame();
        let mut active = score.active;
        let mut points = score.display_points;
        hud.set_score_cache(points, score.display_count, active)?;
        if self.flip_points.1 > 0. {
            self.flip_points.1 -= dt;
            if !active {
                hud.set_extra_points(self.flip_points.0)?;
                points = self.flip_points.0;
            }
        } else {
            self.flip_points.0 = 0.;
        }
        let wheelie = animator.wheelie_frame();
        if wheelie.eligible {
            hud.set_wheelie_points(wheelie.points)?;
            active = true;
            points = wheelie.points;
        }
        hud.step(
            dt,
            frame.speed_magnitude,
            frame.preload_available,
            frame.preload_strength,
            active,
            points,
        )
    }

    pub fn frame(&self) -> &VehicleFrame {
        &self.frame
    }
    pub fn camera(&self) -> (&CameraFrame, CameraMode) {
        (&self.camera_frame, self.camera_mode)
    }
    /// Controls as op 24 resolved and applied them on the latest step.
    pub fn applied(&self) -> &Controls {
        &self.applied
    }
    pub fn steps(&self) -> u64 {
        self.steps
    }
    pub fn hud(&self) -> atv_core::hud::Frame {
        self.hud.frame()
    }
    pub fn rider_mode(&self) -> Option<rider_pose::Mode> {
        self.rider_mode
    }
    /// Contact-plane ground height (the physics terrain), when inside it.
    pub fn ground(&self, x: f32, z: f32) -> Option<f32> {
        self.sim
            .terrain
            .contact(x, z)
            .ok()
            .flatten()
            .map(|c| c.height)
    }

    /// The rider's world matrix: the wreck root while wrecking, else the
    /// vehicle's (mapPose in web/minimap.js reads this for the GPS marker).
    pub fn rider_world(&self) -> [f32; 16] {
        match &self.mount {
            Mount::Quad { rig, .. } => match rig.wreck.frame() {
                Some(wreck) => wreck.world_matrix,
                None => quaternion_world(self.frame.origin, self.frame.orientation),
            },
            Mount::Bike(bike) if bike.frame().wreck.phase != Phase::Riding => bike.rider_world(),
            Mount::Bike(_) => quaternion_world(self.frame.origin, self.frame.orientation),
        }
    }

    /// The ATV's wheel presentation state (radii, slip, surfaces); None on a
    /// motorcycle, whose wheel particles are its own (motorcycle_effects).
    pub fn wheel_effect_state(&self) -> Option<atv_core::vehicle::WheelEffectState> {
        match &self.mount {
            Mount::Quad { vehicle, .. } => Some(vehicle.wheel_effect_state()),
            Mount::Bike(_) => None,
        }
    }

    /// Crazy/Gorn crash presentation for the latest step (atv-web's
    /// simulation_frame `funny_crash`): ragdoll and broken-part poses,
    /// severed rider nodes and Gorn blood. `tick`/`dt` are the effects clock.
    pub fn step_crash_effects(
        &self,
        effects: &mut atv_core::crash_effects::Effects,
        tick: u64,
        dt: f32,
    ) -> Result<Option<atv_core::crash_effects::Frame>> {
        if !effects.enabled() {
            return Ok(None);
        }
        let frame = &self.frame;
        let pose = self.pose()?;
        let wrecked = frame.wreck.phase != Phase::Riding;
        // The motorcycle passes its rider as detached for the whole wreck,
        // its body world and no contact point (atv-web motorcycles.rs).
        let (rider_world, detached, rider_velocity, world, impact) = match &self.mount {
            Mount::Quad { rig, .. } => {
                let wreck = rig.wreck.frame();
                (
                    wreck
                        .as_ref()
                        .map_or(vehicle_world(frame), |w| w.world_matrix),
                    wreck
                        .as_ref()
                        .is_some_and(|w| w.phase != wreck_animation::Phase::Attached),
                    wreck
                        .as_ref()
                        .map_or(frame.linear_velocity, |w| w.linear_velocity),
                    vehicle_world(frame),
                    (frame.crash_impact_speed > 0.).then_some(frame.crash_impact_point),
                )
            }
            Mount::Bike(bike) => (
                bike.rider_world(),
                wrecked,
                bike.rider_velocity(),
                bike.world(),
                None,
            ),
        };
        effects.set_rider_velocity(rider_velocity);
        let terrain = &self.sim.terrain;
        Ok(effects.update(
            tick,
            frame.reset_count,
            dt,
            wrecked,
            detached,
            frame.linear_velocity,
            frame.crash_impact_speed,
            impact,
            world,
            &pose.quad_nodes,
            &self.quad.mesh.nodes,
            rider_world,
            &pose.rider_nodes,
            &self.rider.mesh.nodes,
            |p| {
                terrain
                    .contact(p[0], p[2])
                    .ok()
                    .flatten()
                    .map(|h| (h.height, h.normal))
            },
        ))
    }

    /// What the ATV dirt rule reads from the latest step; None on a motorcycle.
    pub fn dirt_sample(&self) -> Result<Option<crate::dirt::Sample>> {
        if self.is_motorcycle() {
            return Ok(None);
        }
        let input = self.wheel_effect_input()?;
        Ok(Some(crate::dirt::Sample {
            centers: input.centers,
            radii: self.wheel_radii(),
            water_heights: input.water_heights.map(|h| h.unwrap_or(-256.)),
            grounded: input.grounded,
            surfaces: input.surfaces,
            mud: input.mud,
            speed: self.frame.speed_magnitude,
        }))
    }

    /// Wheel radii (presentation effects place dust at the contact patch).
    pub fn wheel_radii(&self) -> [f32; 4] {
        self.wheel_effect_state()
            .map_or([0.; 4], |state| state.radii)
    }

    /// Wheel-effect snapshot of the latest step, as atv-web's simulation_frame
    /// assembles it for op 75: the vehicle's wheel state, posed wheel centres,
    /// contact-plane and water heights. `time`, `dt` and `tick` are left at
    /// zero for the effects clock to fill. Presentation only; ATVs only.
    pub fn wheel_effect_input(&self) -> Result<atv_core::wheel_effects::Input> {
        let Mount::Quad { rig, vehicle } = &self.mount else {
            return Err("motorcycles have their own wheel effects".into());
        };
        let frame = &self.frame;
        let state = vehicle.wheel_effect_state();
        let matrices = match vehicle.linkage_matrices() {
            Some(matrices) => matrices.to_vec(),
            None => rig.quad_pose.sample(
                frame.visual_steering_angle,
                frame.wheel_angles,
                frame.wheel_compression,
            )?,
        };
        let world = vehicle_world(frame);
        let centers = rig
            .quad_pose
            .wheel_positions(&matrices)?
            .map(|point| atv_core::obstacle::transform_point(&world, point));
        let mut terrain_heights = [None; 4];
        let mut water_heights = [None; 4];
        for (i, point) in centers.iter().enumerate() {
            let hit = self.sim.terrain.contact(point[0], point[2])?;
            terrain_heights[i] = Some(hit.map_or(point[1], |hit| hit.height));
            water_heights[i] = Some(match &self.sim.water {
                Some(water) => water.height(*point, false)?,
                None => -256.,
            });
        }
        Ok(atv_core::wheel_effects::Input {
            time: 0.,
            dt: 0.,
            tick: 0,
            centers,
            surfaces: state.surfaces,
            terrain_heights,
            water_heights,
            mud: state.mud,
            wheel_tangential_speeds: state.wheel_tangential_speeds,
            grounded: frame.grounded,
            throttle: state.throttle,
            steering_angle: frame.steering_angle,
            linear_velocity: frame.linear_velocity,
            reset_count: frame.reset_count,
            placement_epoch: u64::from(self.placement),
            mud_heights: state.mud_heights.map(Some),
        })
    }

    /// graphics.js `wheelReprojection` + game.js `reprojectWheels`
    /// (Enhanced/High): wheels sunk into the visible ground (`height`) after
    /// integration are redrawn on it, the compression clamped to
    /// MAX_WHEEL_LIFT; only when the drawn linkage is the rig's own pose.
    /// Presentation only; None keeps the simulated pose. Also returns the
    /// largest added compression (game.js `wheelLift`).
    pub fn reprojected_quad(
        &self,
        height: &dyn Fn(f32, f32) -> f32,
    ) -> Option<(Vec<[f32; 16]>, f32)> {
        const MAX_WHEEL_LIFT: f32 = 0.5;
        let Mount::Quad { rig, vehicle } = &self.mount else {
            return None;
        };
        let frame = &self.frame;
        if frame.wreck.phase != Phase::Riding {
            return None;
        }
        let matrices = match vehicle.linkage_matrices() {
            Some(m) => m.to_vec(),
            None => rig
                .quad_pose
                .sample(
                    frame.visual_steering_angle,
                    frame.wheel_angles,
                    frame.wheel_compression,
                )
                .ok()?,
        };
        let world = vehicle_world(frame);
        let centers = rig
            .quad_pose
            .wheel_positions(&matrices)
            .ok()?
            .map(|p| atv_core::obstacle::transform_point(&world, p));
        let radii = vehicle.wheel_effect_state().radii;
        let [_, x, _, z] = frame.orientation;
        let up = (1. - 2. * (x * x + z * z)).max(0.5);
        let mut changed = false;
        let mut compression = frame.wheel_compression;
        for (i, c) in compression.iter_mut().enumerate() {
            let depth = height(centers[i][0], centers[i][2]) - (centers[i][1] - radii[i]);
            if depth > 0.01 {
                changed = true;
                *c += (depth / up).min(MAX_WHEEL_LIFT);
            }
        }
        if !changed {
            return None;
        }
        let base = rig
            .quad_pose
            .sample(
                frame.visual_steering_angle,
                frame.wheel_angles,
                frame.wheel_compression,
            )
            .ok()?;
        let same = base.len() == matrices.len()
            && base
                .iter()
                .zip(&matrices)
                .all(|(a, b)| a.iter().zip(b).all(|(p, q)| (p - q).abs() <= 1e-3));
        if !same {
            return None;
        }
        let lift = compression
            .iter()
            .zip(frame.wheel_compression)
            .fold(0f32, |m, (c, b)| m.max(c - b));
        let quad = rig
            .quad_pose
            .sample(frame.visual_steering_angle, frame.wheel_angles, compression)
            .ok()?;
        Some((quad, lift))
    }

    /// Vehicle linkage and rider skeleton for the latest frame
    /// (simulation_frame's quad_node_matrices and rider_node_matrices).
    pub fn pose(&self) -> Result<Pose> {
        let frame = &self.frame;
        let quad_world = quaternion_world(frame.origin, frame.orientation);
        let nodes = &self.rider.mesh.nodes;
        match &self.mount {
            Mount::Quad { rig, vehicle } => {
                let quad_nodes = match vehicle.linkage_matrices() {
                    Some(matrices) => matrices.to_vec(),
                    None => rig.quad_pose.sample(
                        frame.visual_steering_angle,
                        frame.wheel_angles,
                        frame.wheel_compression,
                    )?,
                };
                let (rider_world, rider_nodes) = match rig.wreck.frame() {
                    Some(wreck) => (wreck.world_matrix, rig.wreck.pose(nodes)?.world_matrices),
                    None => (quad_world, rig.animator.pose(nodes)?.world_matrices),
                };
                Ok(Pose {
                    quad_world,
                    quad_nodes,
                    rider_world,
                    rider_nodes,
                })
            }
            Mount::Bike(bike) => Ok(Pose {
                quad_world,
                quad_nodes: bike.matrices(),
                rider_world: self.rider_world(),
                rider_nodes: bike.rider_matrices()?,
            }),
        }
    }
}

/// 001e5260 scores wheel contact/body pitch after physics and wreck entry,
/// including ongoing wreck frames, before the rider accepts airborne stunts.
fn update_wheelie(
    animator: &mut rider_pose::Animator,
    frame: &VehicleFrame,
    dt: f32,
) -> Result<()> {
    animator.update_wheelie(
        dt,
        stunt_score::WheelieInput {
            grounded: frame.grounded,
            pitch: atv_core::rider_control::body_angles(frame.orientation, atv_core::math::atan2f)
                [0],
            // 0020f370 initializes manager+13c to zero; the active Freestyle
            // route retains that gate.
            context_allows: true,
            scoring_enabled: stunt_score::DEFAULT_WHEELIE_SCORING,
        },
    )
}
