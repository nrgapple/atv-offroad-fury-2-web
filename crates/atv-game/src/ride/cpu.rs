//! Race CPU riders (crates/atv-web/src/cpu.rs): each rides its own original
//! quad with the recovered driver, rider animation and wreck recovery.
use super::{QuadRig, Simulation, update_wheelie, vehicle_world};
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

pub struct CpuRacer {
    pub vehicle: Vehicle,
    driver: Driver,
    rider: rider_pose::Animator,
    wreck: wreck_animation::Animator,
    mode: Option<rider_pose::Mode>,
    stuck: f32,
}

impl CpuRacer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sim: &Simulation,
        rig: &QuadRig,
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
        let rider = rig.animator.fork_racer(nodes)?;
        let mut wreck = rig.wreck.fork_racer(nodes, 97211 + index as u64 * 47)?;
        wreck.set_static_obstacles(sim.obstacles.clone());
        Ok(Self {
            vehicle,
            rider,
            wreck,
            mode: None,
            stuck: 0.,
            driver: Driver::new(index, options),
        })
    }

    pub fn step(
        &mut self,
        course: &Course,
        player_speed: f32,
        peers: &[Peer],
        gap: f32,
        station: Station,
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
