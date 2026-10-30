//! Race CPU riders (crates/atv-web/src/cpu.rs): each rides its own original
//! quad with the recovered driver, rider animation and wreck recovery.
use super::{Simulation, update_wheelie, vehicle_world};
use crate::Result;
use atv_core::{
    collision::{CollisionRayTree, decode_ray_tree},
    cpu_avoidance::Peer,
    materials::TerrainMaterial,
    mesh::{self, Mesh, Node},
    physics_data::PhysicsData,
    quad_pose::QuadPose,
    race::{Course, CpuPose, Station},
    race_ai::{Context, Driver, DriverPose, Options},
    rider_pose, stunt, terrain,
    vehicle::{Vehicle, VehicleFrame, VehicleInput},
    water_animation::LiveField,
    wreck::Phase,
    wreck_animation,
};
use std::rc::Rc;

/// A CPU racer's own original quad: physics, mesh/linkage, chassis contact
/// points with wreck rays, and actor (obstacle) rays from its ATV archive.
pub struct CpuModel {
    pub physics: PhysicsData,
    pub mesh: Mesh,
    pub quad_pose: Rc<QuadPose>,
    pub chassis_points: Vec<[f32; 3]>,
    pub wreck_rays: Rc<CollisionRayTree>,
    pub obstacle_rays: Rc<CollisionRayTree>,
}

impl CpuModel {
    pub fn new(physics: &[u8], mesh: &[u8], chassis: &[u8], actor: &[u8]) -> Result<Self> {
        let mesh = mesh::decode_psg(mesh)?;
        let chassis = decode_ray_tree(chassis)?;
        let actor = decode_ray_tree(actor)?;
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        if actor.transform != identity {
            return Err("unsupported CPU actor collision header transform".into());
        }
        Ok(Self {
            physics: PhysicsData::parse(physics)?,
            quad_pose: Rc::new(QuadPose::new(&mesh)?),
            mesh,
            chassis_points: chassis.chassis_points()?,
            wreck_rays: Rc::new(chassis),
            obstacle_rays: Rc::new(actor),
        })
    }
}

pub struct World<'a> {
    pub terrain: &'a terrain::Terrain,
    pub nodes: &'a [Node],
    pub materials: &'a [TerrainMaterial; 8],
    pub water: Option<&'a LiveField>,
    pub dynamic: &'a [atv_core::scene_collision::DynamicObstacle],
    /// The countdown holds the grid on the brakes while the quads settle.
    pub hold: bool,
}

/// Seconds a riding CPU has stayed below walking pace. The grid hold keeps
/// the clock at zero, or every CPU braked through the intro would trip the
/// stuck recovery at GO.
fn stuck_clock(stuck: f32, hold: bool, riding: bool, speed: f32, dt: f32) -> f32 {
    if !hold && riding && speed.abs() < 3. {
        stuck + dt
    } else {
        0.
    }
}

/// One fixed step's sound inputs for a CPU's quad and rider emitters: the
/// fields of the player's [`super::sound::AudioStep`] that the original
/// per-quad (`001e33c0`: engine, chassis) and per-rider (`002d6380`:
/// vocals, body impact) emitters read. Every rider object builds these
/// emitters, CPUs included.
#[derive(Clone, Debug, Default)]
pub struct CpuSoundStep {
    pub events: atv_core::vehicle::AudioEvents,
    pub chassis: atv_core::quad_sounds::ChassisInput,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// The actor sound position (`001e7600`): the quad origin while riding
    /// or attached; detached phases keep the last attached point.
    pub rider_position: [f32; 3],
    /// The detached rider body's sampled contact delta (zero while riding).
    pub rider_impact: [f32; 3],
    pub riding: bool,
    /// The vehicle reset count (a reset clears the emitters' latches).
    pub reset_count: u32,
}

/// A CPU racer as the audio layer hears it this frame.
#[derive(Clone, Debug, Default)]
pub struct CpuSound {
    /// The CPU's index in the race (its roster entry).
    pub index: usize,
    /// The original quad number (`vehicle.aud` `QuadN`).
    pub quad: u32,
    /// Rider +268: the Female vocal lists.
    pub female: bool,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub throttle: f32,
    pub speed: f32,
    /// On the ground and not wrecked or recovering (the engine state's
    /// riding input, as the room riders' `RiderState::Riding`).
    pub riding: bool,
    pub steps: Vec<CpuSoundStep>,
}

/// Steps of sound kept for the audio layer between reads (the player's
/// frontend batch bound); older steps are dropped while nothing reads them.
pub const SOUND_STEPS: usize = 64;

pub struct CpuRacer {
    pub vehicle: Vehicle,
    /// Sound inputs since the audio layer last took them.
    sound: Vec<CpuSoundStep>,
    /// The throttle the driver applied on the latest step.
    throttle: f32,
    rider_audio_position: [f32; 3],
    driver: Driver,
    rider: rider_pose::Animator,
    wreck: wreck_animation::Animator,
    mode: Option<rider_pose::Mode>,
    stuck: f32,
    /// Port adaptation: the authored repeat/stall recovery (CPU only).
    pub recovery: atv_core::cpu_recovery::Recovery,
}

impl CpuRacer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sim: &Simulation,
        (animator, wreck): (&rider_pose::Animator, &wreck_animation::Animator),
        nodes: &[Node],
        model: &CpuModel,
        mut origin: [f32; 3],
        yaw: f32,
        index: usize,
        options: Options,
    ) -> Result<Self> {
        let terrain = &sim.terrain;
        origin[1] = terrain
            .contact(origin[0], origin[2])?
            .ok_or("CPU grid outside terrain")?
            .height;
        let mut vehicle = Vehicle::new(&model.mesh, &model.physics, origin, yaw)?;
        vehicle.place_on_ground(terrain, origin, yaw)?;
        vehicle.enable_wrecks(&model.physics)?;
        vehicle.set_wreck_rays(model.wreck_rays.clone())?;
        if let Some(config) = sim.boundary {
            vehicle.set_boundary(config, sim.extended_boundary)?;
        }
        vehicle.set_linkage(model.quad_pose.clone());
        vehicle.set_materials(sim.materials);
        vehicle.set_chassis_points(&model.chassis_points)?;
        vehicle.set_obstacles(model.obstacle_rays.clone(), sim.obstacles.clone())?;
        if let Some(config) = sim.scene_partition {
            vehicle.set_scene_partition(config, sim.ecosystem.as_ref(), terrain)?;
        }
        if let Some(scene) = vehicle.scene_collision_mut() {
            scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
        }
        let rider = animator.fork_racer(nodes)?;
        let mut wreck = wreck.fork_racer(nodes, 97211 + index as u64 * 47)?;
        wreck.set_static_obstacles(sim.obstacles.clone());
        let rider_audio_position = vehicle.frame().origin;
        Ok(Self {
            vehicle,
            sound: Vec::new(),
            throttle: 0.,
            rider_audio_position,
            rider,
            wreck,
            mode: None,
            stuck: 0.,
            recovery: Default::default(),
            driver: Driver::new(index, options),
        })
    }

    pub fn step(
        &mut self,
        course: &Course,
        player_speed: f32,
        peers: &[Peer],
        gap: f32,
        // The race station, RaceStatus +0x3c and the Enduro next gate.
        (station, on_track, enduro): (Station, bool, Option<atv_core::race_ai::EnduroTarget>),
        world: &World<'_>,
    ) -> Result<(VehicleFrame, CpuPose, bool)> {
        const DT: f32 = 1. / 60.;
        if let Some(scene) = self.vehicle.scene_collision_mut() {
            if scene.dynamic().is_empty() && !world.dynamic.is_empty() {
                scene.install_dynamic(world.dynamic.to_vec())?;
            }
            for (i, object) in world.dynamic.iter().enumerate() {
                scene.update_dynamic(i, object.motion)?;
            }
        }
        let before = self.vehicle.frame();
        let contact = world.terrain.contact(before.origin[0], before.origin[2])?;
        let predicted = world.terrain.contact(
            before.origin[0] + before.linear_velocity[0] * 0.5,
            before.origin[2] + before.linear_velocity[2] * 0.5,
        )?;
        let mut pose: DriverPose = (&before).into();
        pose.preload_rates = self.vehicle.preload_rates();
        self.driver.assist = self.recovery.assist();
        let (mut input, decisions) = self.driver.step(
            course,
            &pose,
            player_speed,
            gap,
            DT,
            Context {
                peers,
                scene: self.vehicle.scene_collision(),
                clearance: contact.as_ref().map_or(0., |c| before.origin[1] - c.height),
                terrain_normal: predicted.as_ref().map_or([0., 1., 0.], |c| c.normal),
                station: Some(station),
                on_track,
                enduro,
            },
        )?;
        let stunt_input = stunt::Input {
            grounded: before.grounded.iter().any(|v| *v),
            horizontal_speed_squared: before.linear_velocity[0].powi(2)
                + before.linear_velocity[2].powi(2),
            crashed: before.wreck.phase != Phase::Riding,
            ..stunt::Input::default()
        };
        if world.hold {
            input = VehicleInput {
                brake: 1.,
                lean_flick_rate: Some(0.),
                ..Default::default()
            };
        } else if let Some(direct) = decisions.direct {
            self.vehicle.pull_velocity(direct.velocity, direct.grounded);
        }
        let durations = std::array::from_fn(|i| self.rider.stunt_duration(i).unwrap_or(f32::MAX));
        if !world.hold
            && let Some(id) = self.driver.stunt_choice(
                course,
                &before,
                self.rider.stunt_frame().is_some(),
                &durations,
                DT,
            )
        {
            self.rider.request_cpu_stunt(id, stunt_input);
        }
        self.vehicle
            .set_stunt_landing_safe(self.rider.stunt_landing_safe());
        self.vehicle.set_rider_mode(self.mode);
        self.vehicle
            .set_forced_landing(self.rider.landing_frame().is_some());
        self.throttle = input.throttle;
        let wreck = &mut self.wreck;
        let stepped =
            self.vehicle
                .step_with_scene(world.terrain, input, DT, world.water, || {
                    wreck.boundary_random_words()
                })?;
        if stepped.reset_count != before.reset_count {
            self.reset_presentation(world.nodes)?;
        }
        if stepped.wreck.phase != Phase::Riding {
            if !self.wreck.active() {
                self.wreck
                    .set_boundary_flight(stepped.boundary_launch.active);
                self.wreck.begin(
                    stepped.wreck.kind.ok_or("CPU wreck kind missing")?,
                    self.rider.channels(),
                    vehicle_world(&stepped),
                    world.nodes,
                )?;
                self.rider.abort_wreck_scores()?;
                self.rider.reset(world.nodes)?;
            }
            self.wreck.step_with_scene(
                DT,
                wreck_animation::VehicleMotion {
                    world: vehicle_world(&stepped),
                    linear_velocity: stepped.linear_velocity,
                    local_angular_velocity: stepped.local_angular_velocity,
                },
                world.nodes,
                world.terrain,
                world.materials,
                world.water,
                self.vehicle.scene_collision_mut(),
                stepped.skip_vegetation_query,
            )?;
            let grounded = self.wreck.frame().is_some_and(|r| r.grounded);
            if self.vehicle.advance_wreck(DT, grounded)? {
                self.reset_presentation(world.nodes)?;
            }
        } else {
            update_wheelie(&mut self.rider, &stepped, DT)?;
            if let Some(landing) = stepped.landing_action {
                self.rider.request_landing(landing)?;
            }
            self.mode = Some(self.rider.step_controls(
                world.nodes,
                self.vehicle.rider_pose_input(),
                stunt::Input {
                    modifiers: 0,
                    axes: [0.; 2],
                    grounded: stepped.grounded.iter().any(|v| *v),
                    horizontal_speed_squared: stepped.linear_velocity[0].powi(2)
                        + stepped.linear_velocity[2].powi(2),
                    crashed: false,
                    disabled: false,
                },
                DT,
            )?);
        }
        let frame = self.vehicle.frame();
        self.stuck = stuck_clock(
            self.stuck,
            world.hold,
            frame.wreck.phase == Phase::Riding,
            frame.speed,
            DT,
        );
        let rider_wreck = self.wreck.frame();
        self.capture_sound(&frame, rider_wreck.as_ref(), world)?;
        let rider_matrices = if rider_wreck.is_some() {
            self.wreck.pose(world.nodes)?.world_matrices
        } else {
            self.rider.pose(world.nodes)?.world_matrices
        };
        let pose = CpuPose {
            orientation: frame.orientation,
            speed: frame.speed,
            grounded: frame.grounded,
            wheel_compression: frame.wheel_compression,
            wheel_angles: frame.wheel_angles,
            quad_matrices: self
                .vehicle
                .linkage_matrices()
                .ok_or("CPU linkage missing")?
                .to_vec(),
            rider_matrices,
            rider_world_matrix: rider_wreck.as_ref().map(|r| r.world_matrix),
            wreck: frame.wreck,
            rider_wreck,
            reset_count: frame.reset_count,
            stunt: self.rider.stunt_frame(),
            decisions,
        };
        let repositioned = frame.reset_count != before.reset_count;
        Ok((frame, pose, repositioned))
    }

    fn capture_sound(
        &mut self,
        frame: &VehicleFrame,
        rider: Option<&wreck_animation::Frame>,
        world: &World<'_>,
    ) -> Result<()> {
        let events = self.vehicle.take_audio_events();
        if frame.wreck.phase == Phase::Riding
            || rider.is_none_or(|r| r.phase == wreck_animation::Phase::Attached)
        {
            self.rider_audio_position = frame.origin;
        }
        let ground = world
            .terrain
            .contact(frame.origin[0], frame.origin[2])?
            .map(|hit| hit.height);
        if self.sound.len() >= SOUND_STEPS {
            self.sound.remove(0);
        }
        self.sound.push(CpuSoundStep {
            chassis: atv_core::quad_sounds::ChassisInput {
                impact: events.impact,
                wrecked: frame.wreck.phase != Phase::Riding,
                airborne: ground.is_some_and(|g| frame.origin[1] - g > 2.),
                chassis_contact: frame.chassis_contact_count + frame.obstacle_contact_count > 0,
            },
            events,
            origin: frame.origin,
            velocity: frame.linear_velocity,
            rider_position: self.rider_audio_position,
            rider_impact: rider.map_or([0.; 3], |r| r.sampled_impact_delta),
            riding: frame.wreck.phase == Phase::Riding,
            reset_count: frame.reset_count,
        });
        Ok(())
    }

    /// The sound steps since the last call, oldest first.
    pub fn take_sound(&mut self) -> Vec<CpuSoundStep> {
        std::mem::take(&mut self.sound)
    }

    /// The throttle the driver applied on the latest step.
    pub fn throttle(&self) -> f32 {
        self.throttle
    }

    /// The rider's replicable animation recipe (room race CPU motion).
    pub fn recipe(&self) -> rider_pose::Recipe {
        self.rider.recipe()
    }

    /// The detached rider's world matrix and velocity during a wreck.
    pub fn rider_wreck(&self) -> Option<([f32; 16], [f32; 3])> {
        self.wreck
            .frame()
            .map(|w| (w.world_matrix, w.linear_velocity))
    }

    pub fn stuck_time(&self) -> f32 {
        self.stuck
    }

    /// Recover at the race checkpoint on the racing line.
    pub fn respawn(&mut self, nodes: &[Node]) -> Result<()> {
        self.vehicle.reset_now()?;
        self.stuck = 0.;
        self.reset_presentation(nodes)
    }

    fn reset_presentation(&mut self, nodes: &[Node]) -> Result<()> {
        self.wreck.reset();
        self.wreck.set_boundary_flight(false);
        self.rider.reset(nodes)?;
        self.mode = None;
        self.driver.repositioned();
        Ok(())
    }
}

/// A room race's CPU on a guest (research/MULTIPLAYER.md "Room race
/// CPUs"): it does not race here; it is drawn from the host's relayed
/// motion with its own quad linkage and the CPU rider rig, as a remote
/// rider is (`multiplayer::remote::Local`).
pub struct Puppet {
    quad_pose: Rc<QuadPose>,
    rider: rider_pose::Animator,
    wreck: wreck_animation::Animator,
    crash: u32,
    wreck_time: f32,
    initialized: bool,
    /// The drawn pose: the grid until the host's motion arrives.
    pub pose: Option<super::Pose>,
    /// (origin, velocity, orientation) the player pushes against; None
    /// until heard and while stale.
    pub body: Option<([f32; 3], [f32; 3], [f32; 4])>,
}

impl Puppet {
    pub(super) fn new(
        (animator, wreck): (&rider_pose::Animator, &wreck_animation::Animator),
        nodes: &[Node],
        model: &CpuModel,
        index: usize,
    ) -> Result<Self> {
        Ok(Self {
            quad_pose: model.quad_pose.clone(),
            rider: animator.fork_racer(nodes)?,
            wreck: wreck.fork_racer(nodes, 97211 + index as u64 * 47)?,
            crash: 0,
            wreck_time: 0.,
            initialized: false,
            pose: None,
            body: None,
        })
    }

    /// Pose from a presented snapshot `dt` seconds after the last.
    pub fn present(
        &mut self,
        nodes: &[Node],
        s: &atv_net::Snapshot,
        dt: f32,
        stale: bool,
    ) -> Result<()> {
        let a = s.animation.as_ref().ok_or("CPU animation missing")?;
        let quad_nodes = self.quad_pose.sample(a.steering, a.wheels, a.compression)?;
        let step = if stale { 0. } else { dt };
        let blend_dt = if self.initialized { dt } else { 0.25 };
        self.initialized = true;
        let quad_world = s.atv.to_column_major();
        let (rider_world, rider_nodes) = if s.rider_state == atv_net::RiderState::Wreck {
            if self.crash != s.crash_serial {
                self.crash = s.crash_serial;
                self.wreck_time = 0.;
            }
            self.wreck_time += step;
            // The air/bounce clips run on this receiver's clock; the
            // detached root is replicated (as for remote riders).
            let settled = s.rider_velocity.iter().map(|v| v * v).sum::<f32>() < 4.;
            let clip = if settled {
                self.crash as usize % 4
            } else {
                4 + self.crash as usize % 4
            };
            let m = self.wreck.motion(clip).ok_or("CPU wreck clip missing")?;
            let time = self.wreck_time.rem_euclid(m.animation.duration.max(0.001));
            (
                s.rider.to_column_major(),
                m.animation
                    .sample_pose(nodes, time, m.sampling)?
                    .world_matrices,
            )
        } else {
            self.wreck_time = 0.;
            (
                quad_world,
                self.rider
                    .present(
                        nodes,
                        rider_pose::Recipe {
                            clips: a.clips,
                            count: a.count,
                            phase: a.phase,
                            first_weight: a.first_weight,
                            pair_weight: a.pair_weight,
                            look_phase: a.look_phase,
                        },
                        blend_dt,
                    )?
                    .world_matrices,
            )
        };
        self.pose = Some(super::Pose {
            quad_world,
            quad_nodes,
            rider_world,
            rider_nodes,
        });
        self.body = (!stale).then_some((s.atv.position, s.velocity, s.atv.rotation));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::stuck_clock;
    #[test]
    fn grid_hold_never_counts_as_stuck() {
        let mut stuck = 0.;
        for _ in 0..600 {
            stuck = stuck_clock(stuck, true, true, 0., 1. / 60.);
        }
        assert_eq!(stuck, 0.);
        for _ in 0..60 {
            stuck = stuck_clock(stuck, false, true, 1., 1. / 60.);
        }
        assert!((stuck - 1.).abs() < 1e-4);
        assert_eq!(stuck_clock(stuck, false, true, 5., 1. / 60.), 0.);
    }
}
