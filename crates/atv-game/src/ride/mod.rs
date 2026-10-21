//! Single-player Freeride on an ATV: the original vehicle physics, rider
//! animation, wreck recovery and follow camera stepped at the fixed 60 Hz.
//!
//! This is the Rust engine's port of the browser path (web/game.js
//! startSimulation/pumpSimulation and atv-web's simulation operations 23, 24,
//! 46, 47 and the op 50 batch). It calls atv-core directly in the same order;
//! races, motorcycles, multiplayer, trains and the optional crash/flip
//! presentation layers are not part of it yet.
pub mod bindings;
mod load;
pub mod sound;
pub mod textures;
pub mod view;

use crate::Result;
pub use atv_core::wreck::Phase;
use atv_core::{
    browser_input::{BrowserDrive, TouchIntent},
    camera::{CameraFrame, FollowCamera, FollowInput, WreckActor, WreckCamera},
    collision::CollisionRayTree,
    dynamics,
    materials::TerrainMaterial,
    mesh::Mesh,
    obstacle::StaticObstacle,
    physics_data::PhysicsData,
    quad_pose::QuadPose,
    rider_pose, stunt, stunt_score, terrain,
    vehicle::{Vehicle, VehicleFrame, VehicleInput},
    water_animation::LiveField,
    wreck_animation,
};
pub use load::load;
use std::{collections::HashMap, rc::Rc};

/// Fixed simulation step (the browser's 1/60 s batches).
pub const STEP: f32 = 1. / 60.;

/// A drawable original model: its mesh and per-material base textures.
pub struct Model {
    pub mesh: Mesh,
    pub textures: HashMap<String, textures::MaterialTexture>,
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
    /// Circle=1, Triangle=2, L1=4 with their direction axes.
    pub stunt_modifiers: u8,
    pub stunt_x: f32,
    pub stunt_y: f32,
    /// Cancellation generation; a change cancels a pending preload gesture.
    pub generation: u64,
}

/// Camera mode of the latest camera step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Follow,
    Wreck,
}

/// Loaded original state the simulation reuses on every reset.
struct Simulation {
    terrain: terrain::Terrain,
    physics: PhysicsData,
    materials: [TerrainMaterial; 8],
    water: Option<LiveField>,
    quad_pose: Rc<QuadPose>,
    chassis_points: Vec<[f32; 3]>,
    wreck_rays: Rc<CollisionRayTree>,
    obstacle_rays: Rc<CollisionRayTree>,
    obstacles: Vec<StaticObstacle>,
    mud_obstacles: Vec<bool>,
    scene_partition: Option<atv_core::scene_partition::Config>,
    ecosystem: Option<atv_core::ecosystem::Ecosystem>,
    boundary: Option<atv_core::boundary_launch::Config>,
    extended_boundary: bool,
    animator: rider_pose::Animator,
    wreck: wreck_animation::Animator,
    camera: FollowCamera,
}

pub struct Ride {
    sim: Simulation,
    vehicle: Vehicle,
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
    pub quad: Model,
    pub rider: Model,
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
    fn start(sim: Simulation, start: ([f32; 3], f32), quad: Model, rider: Model) -> Result<Self> {
        let mut sim = sim;
        let vehicle = Self::place(&mut sim, &quad.mesh, start)?;
        let frame = vehicle.frame();
        let mut ride = Self {
            sim,
            vehicle,
            browser_drive: BrowserDrive::default(),
            input_generation: None,
            rider_mode: None,
            wreck_camera: None,
            flip_heading: None,
            hud: atv_core::hud::State::default(),
            flip_points: (0., 0.),
            start,
            frame,
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
            quad,
            rider,
        };
        ride.after_placement()?;
        // startSimulation: op 45 builds the camera, then op 46 steps it once.
        ride.step_camera(0.)?;
        Ok(ride)
    }

    /// Op 23: a new vehicle at `start` with every original collision source.
    fn place(sim: &mut Simulation, mesh: &Mesh, (origin, yaw): ([f32; 3], f32)) -> Result<Vehicle> {
        let mut vehicle = Vehicle::new(mesh, &sim.physics, origin, yaw)?;
        vehicle.enable_wrecks(&sim.physics)?;
        if let Some(config) = sim.boundary {
            vehicle.set_boundary(config, sim.extended_boundary)?;
        }
        vehicle.set_linkage(sim.quad_pose.clone());
        sim.wreck.reset();
        sim.wreck.set_boundary_flight(false);
        sim.wreck.set_static_obstacles(sim.obstacles.clone());
        vehicle.set_materials(sim.materials);
        vehicle.set_chassis_points(&sim.chassis_points)?;
        vehicle.set_wreck_rays(sim.wreck_rays.clone())?;
        vehicle.set_obstacles(sim.obstacle_rays.clone(), sim.obstacles.clone())?;
        if let Some(config) = sim.scene_partition {
            vehicle.set_scene_partition(config, sim.ecosystem.as_ref(), &sim.terrain)?;
        }
        if let Some(scene) = vehicle.scene_collision_mut() {
            scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
        }
        Ok(vehicle)
    }

    fn after_placement(&mut self) -> Result<()> {
        self.sim.animator.reset(&self.rider.mesh.nodes)?;
        self.sim.animator.reset_score_session();
        self.hud.reset();
        self.flip_points = (0., 0.);
        self.rider_mode = None;
        self.browser_drive.reset();
        self.input_generation = None;
        self.wreck_camera = None;
        self.frame = self.vehicle.frame();
        Ok(())
    }

    /// Reset ATV: back to the scene start with the follow camera reset (ops
    /// 23, 47 and one op 46 step).
    pub fn reset(&mut self) -> Result<()> {
        self.placement = self.placement.wrapping_add(1);
        self.sound = None;
        self.vehicle = Self::place(&mut self.sim, &self.quad.mesh, self.start)?;
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
        self.step_vehicle(controls)?;
        self.step_camera(orbit)?;
        self.steps += 1;
        self.sound = Some(self.capture_sound()?);
        Ok(())
    }

    /// The step's sound inputs (simulation_frame's audio fields).
    fn capture_sound(&mut self) -> Result<sound::AudioStep> {
        let frame = &self.frame;
        let events = self.vehicle.take_audio_events();
        let surfaces = self.vehicle.wheel_effect_state().surfaces;
        let matrices = match self.vehicle.linkage_matrices() {
            Some(matrices) => matrices.to_vec(),
            None => self.sim.quad_pose.sample(
                frame.visual_steering_angle,
                frame.wheel_angles,
                frame.wheel_compression,
            )?,
        };
        let world = vehicle_world(frame);
        let centers = self
            .sim
            .quad_pose
            .wheel_positions(&matrices)?
            .map(|point| atv_core::obstacle::transform_point(&world, point));
        let ground = self
            .sim
            .terrain
            .contact(frame.origin[0], frame.origin[2])?
            .map(|hit| hit.height);
        let water = self.sim.water.as_ref().map(|_| frame.water_height);
        let rider = self.sim.wreck.frame();
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
        })
    }

    /// Sound inputs of the latest step (None right after a reset).
    pub fn sound(&self) -> Option<&sound::AudioStep> {
        self.sound.as_ref()
    }

    fn step_vehicle(&mut self, controls: &Controls) -> Result<()> {
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
        let current = self.vehicle.frame();
        let input = self.browser_drive.resolve(
            input,
            controls.keyboard_drive,
            None,
            current.grounded.iter().any(|c| *c),
            current.speed,
        )?;
        let sim = &mut self.sim;
        let safe = sim.animator.stunt_landing_safe();
        let forced_landing = sim.animator.landing_frame().is_some();
        let vehicle = &mut self.vehicle;
        if self.input_generation != Some(controls.generation) {
            vehicle.cancel_preload_gesture();
            self.input_generation = Some(controls.generation);
        }
        vehicle.set_stunt_landing_safe(safe);
        vehicle.set_realistic_train(false);
        vehicle.set_flips(false)?;
        vehicle.set_arcade_flips(false)?;
        vehicle.set_arcade_spin(None)?;
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
        let wreck = &mut sim.wreck;
        let frame = vehicle.step_with_scene(&sim.terrain, input, dt, sim.water.as_ref(), || {
            wreck.boundary_random_words()
        })?;
        let nodes = &self.rider.mesh.nodes;
        // Deep water can invoke 001e35d0 before physics independently of
        // advance_wreck. Reset attached presentation/camera state too.
        if frame.reset_count != current.reset_count {
            sim.wreck.reset();
            sim.wreck.set_boundary_flight(false);
            sim.animator.reset(nodes)?;
            sim.camera.reset();
            self.wreck_camera = None;
            self.rider_mode = None;
            self.browser_drive.reset();
        }
        // 001e7600 publishes actor+290 before rider physics only while
        // Riding/Attached; detached audio retains the last actor point.
        if frame.wreck.phase == Phase::Riding
            || sim
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
            if !sim.wreck.active() {
                sim.wreck.set_boundary_flight(frame.boundary_launch.active);
                sim.wreck.begin(
                    frame.wreck.kind.ok_or("missing wreck kind")?,
                    sim.animator.channels(),
                    vehicle_world(&frame),
                    nodes,
                )?;
                sim.animator.abort_wreck_scores()?;
                sim.animator.reset(nodes)?;
            }
            update_wheelie(&mut sim.animator, &frame, dt)?;
            sim.wreck.step_with_scene(
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
            let grounded = sim.wreck.frame().is_some_and(|f| f.grounded);
            if vehicle.advance_wreck(dt, grounded)? {
                sim.wreck.reset();
                sim.wreck.set_boundary_flight(false);
                sim.animator.reset(nodes)?;
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
        update_wheelie(&mut sim.animator, &frame, dt)?;
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
            sim.animator.request_landing(landing)?;
        }
        self.rider_mode = Some(sim.animator.step_controls(nodes, pose_input, stunt, dt)?);
        if let Some(water) = &mut sim.water {
            water.prepare_surface()?;
        }
        self.frame = frame;
        self.update_hud(dt)
    }

    /// Op 46: the follow camera, or the wreck camera while the rider is off.
    fn step_camera(&mut self, orbit: f32) -> Result<()> {
        let dt = STEP;
        let vehicle = self.vehicle.frame();
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
        if let Some(rider) = self.sim.wreck.frame() {
            let actors = [
                WreckActor {
                    origin: vehicle.origin,
                    velocity,
                },
                WreckActor {
                    origin: rider.origin,
                    velocity: rider.linear_velocity,
                },
            ];
            if self.wreck_camera.is_none() {
                let mut camera = self.sim.camera.wreck_camera(actors)?;
                camera.set_static_obstacles(self.sim.obstacles.clone());
                self.wreck_camera = Some(camera);
            }
            self.camera_frame = self
                .wreck_camera
                .as_mut()
                .ok_or("wreck camera missing")?
                .step_with_scene(actors, basis[2], dt, height, self.vehicle.scene_collision())?;
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
        let animator = &mut self.sim.animator;
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
                } => hud.finish_event(success, display_count),
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

    /// Quad linkage and rider skeleton for the latest frame (simulation_frame).
    pub fn pose(&self) -> Result<Pose> {
        let frame = &self.frame;
        let quad_nodes = match self.vehicle.linkage_matrices() {
            Some(matrices) => matrices.to_vec(),
            None => self.sim.quad_pose.sample(
                frame.visual_steering_angle,
                frame.wheel_angles,
                frame.wheel_compression,
            )?,
        };
        let quad_world = quaternion_world(frame.origin, frame.orientation);
        let nodes = &self.rider.mesh.nodes;
        let (rider_world, rider_nodes) = match self.sim.wreck.frame() {
            Some(wreck) => (
                wreck.world_matrix,
                self.sim.wreck.pose(nodes)?.world_matrices,
            ),
            None => (quad_world, self.sim.animator.pose(nodes)?.world_matrices),
        };
        Ok(Pose {
            quad_world,
            quad_nodes,
            rider_world,
            rider_nodes,
        })
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
            pitch: atv_core::rider_control::body_angles(frame.orientation, f32::atan2)[0],
            // 0020f370 initializes manager+13c to zero; the active Freestyle
            // route retains that gate.
            context_allows: true,
            scoring_enabled: stunt_score::DEFAULT_WHEELIE_SCORING,
        },
    )
}
