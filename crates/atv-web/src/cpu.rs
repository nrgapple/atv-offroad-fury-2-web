//! Per-CPU integration of original driving and recovered ATV/rider wreck steps.
use super::*;
use atv_core::{cpu_avoidance::Peer, vehicle::VehicleFrame};
pub struct World<'a> {
    pub terrain: &'a terrain::Terrain,
    pub nodes: &'a [mesh::Node],
    pub materials: &'a [TerrainMaterial; 8],
    pub water: Option<&'a atv_core::water_animation::LiveField>,
    pub dynamic: &'a [atv_core::scene_collision::DynamicObstacle],
    /// The countdown holds the grid on the brakes while the quads settle.
    pub hold: bool,
}
/// A CPU racer's own original quad: physics, mesh/linkage, chassis contact
/// points with wreck rays, and actor (obstacle) rays from its ATV archive.
pub struct CpuModel {
    pub physics: PhysicsData,
    pub mesh: mesh::Mesh,
    pub quad_pose: std::rc::Rc<atv_core::quad_pose::QuadPose>,
    pub chassis_points: Vec<[f32; 3]>,
    pub wreck_rays: std::rc::Rc<atv_core::collision::CollisionRayTree>,
    pub obstacle_rays: std::rc::Rc<atv_core::collision::CollisionRayTree>,
}
impl CpuModel {
    /// Bundle: four little-endian u32 lengths, then physics, mesh, chassis
    /// (`ao.col`) and actor (`ar.col`) bytes.
    pub fn parse(bundle: &[u8]) -> Result<Self> {
        let header = bundle.get(..16).ok_or("CPU model bundle is truncated")?;
        let mut at: usize = 16;
        let mut parts = Vec::with_capacity(4);
        for i in 0..4 {
            let length = u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
            parts.push(
                bundle
                    .get(at..at.checked_add(length).ok_or("CPU model size overflow")?)
                    .ok_or("CPU model bundle is truncated")?,
            );
            at += length;
        }
        if at != bundle.len() {
            return Err("CPU model bundle has trailing bytes".into());
        }
        let mesh = mesh::decode_psg(parts[1])?;
        let chassis = atv_core::collision::decode_ray_tree(parts[2])?;
        let actor = atv_core::collision::decode_ray_tree(parts[3])?;
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        if actor.transform != identity {
            return Err("unsupported CPU actor collision header transform".into());
        }
        Ok(Self {
            physics: PhysicsData::parse(parts[0])?,
            quad_pose: std::rc::Rc::new(atv_core::quad_pose::QuadPose::new(&mesh)?),
            mesh,
            chassis_points: chassis.chassis_points()?,
            wreck_rays: std::rc::Rc::new(chassis),
            obstacle_rays: std::rc::Rc::new(actor),
        })
    }
}
pub struct CpuRacer {
    pub vehicle: Vehicle,
    driver: atv_core::race_ai::Driver,
    rider: atv_core::rider_pose::Animator,
    wreck: atv_core::wreck_animation::Animator,
    mode: Option<atv_core::rider_pose::Mode>,
    /// Seconds riding below walking pace (pinned against scenery or a rider).
    stuck: f32,
}
pub fn create_cpu_racer(
    sim: &Simulation,
    mut origin: [f32; 3],
    yaw: f32,
    index: usize,
    difficulty: u32,
) -> Result<CpuRacer> {
    let terrain = sim.terrain.as_ref().ok_or("CPU terrain missing")?;
    origin[1] = terrain
        .contact(origin[0], origin[2])?
        .ok_or("CPU grid outside terrain")?
        .height;
    // Each CPU rides its own original quad when the roster installed one.
    let model = sim.cpu_models.get(index).and_then(Option::as_ref);
    let mesh = model
        .map_or(sim.mesh.as_ref(), |m| Some(&m.mesh))
        .ok_or("CPU mesh missing")?;
    let physics = model
        .map_or(sim.physics.as_ref(), |m| Some(&m.physics))
        .ok_or("CPU physics missing")?;
    let mut vehicle = Vehicle::new(mesh, physics, origin, yaw)?;
    vehicle.place_on_ground(terrain, origin, yaw)?;
    vehicle.enable_wrecks(physics)?;
    vehicle.set_wreck_rays(
        model
            .map_or(sim.wreck_rays.as_ref(), |m| Some(&m.wreck_rays))
            .ok_or("CPU wreck rays missing")?
            .clone(),
    )?;
    if let Some(config) = sim.boundary_config {
        vehicle.set_boundary(config, sim.extended_boundary)?;
    }
    vehicle.set_linkage(
        model
            .map_or(sim.quad_pose.as_ref(), |m| Some(&m.quad_pose))
            .ok_or("CPU linkage missing")?
            .clone(),
    );
    vehicle.set_materials(sim.materials.ok_or("CPU materials missing")?);
    vehicle.set_chassis_points(
        model
            .map_or(sim.chassis_points.as_ref(), |m| Some(&m.chassis_points))
            .ok_or("CPU chassis missing")?,
    )?;
    vehicle.set_obstacles(
        model
            .map_or(sim.obstacle_rays.as_ref(), |m| Some(&m.obstacle_rays))
            .ok_or("CPU rays missing")?
            .clone(),
        sim.obstacles.as_ref().ok_or("CPU scenery missing")?.clone(),
    )?;
    if let Some(config) = sim.scene_partition {
        vehicle.set_scene_partition(config, sim.ecosystem.as_ref(), terrain)?;
    }
    if let Some(scene) = vehicle.scene_collision_mut() {
        scene.set_mud_obstacles(sim.mud_obstacles.clone())?;
    }
    let rider = sim
        .rider_animator
        .as_ref()
        .ok_or("CPU rider motions missing")?
        .fork_racer(
            &sim.rider_mesh
                .as_ref()
                .ok_or("CPU rider mesh missing")?
                .nodes,
        )?;
    let mut wreck = sim
        .wreck_animator
        .as_ref()
        .ok_or("CPU wreck motions missing")?
        .fork_racer(
            &sim.rider_mesh
                .as_ref()
                .ok_or("CPU rider mesh missing")?
                .nodes,
            97211 + index as u64 * 47,
        )?;
    wreck.set_static_obstacles(sim.obstacles.clone().unwrap_or_default());
    Ok(CpuRacer {
        vehicle,
        rider,
        wreck,
        mode: None,
        stuck: 0.,
        driver: atv_core::race_ai::Driver::new(index, difficulty),
    })
}
impl CpuRacer {
    pub fn step(
        &mut self,
        course: &atv_core::race::Course,
        player_speed: f32,
        peers: &[Peer],
        gap: f32,
        station: atv_core::race::Station,
        world: &World<'_>,
    ) -> Result<(VehicleFrame, atv_core::race::CpuPose, bool)> {
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
        let (mut input, decisions) = self.driver.step(
            course,
            &(&before).into(),
            player_speed,
            gap,
            DT,
            atv_core::race_ai::Context {
                peers,
                scene: self.vehicle.scene_collision(),
                clearance: contact.as_ref().map_or(0., |c| before.origin[1] - c.height),
                terrain_normal: predicted.as_ref().map_or([0., 1., 0.], |c| c.normal),
                station: Some(station),
            },
        )?;
        let stunt_input = atv_core::stunt::Input {
            grounded: before.grounded.iter().any(|v| *v),
            horizontal_speed_squared: before.linear_velocity[0].powi(2)
                + before.linear_velocity[2].powi(2),
            crashed: before.wreck.phase != atv_core::wreck::Phase::Riding,
            ..atv_core::stunt::Input::default()
        };
        if world.hold {
            input = atv_core::vehicle::VehicleInput {
                brake: 1.,
                ..Default::default()
            };
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
        let stepped =
            self.vehicle
                .step_with_scene(world.terrain, input, DT, world.water, || {
                    self.wreck.boundary_random_words()
                })?;
        if stepped.reset_count != before.reset_count {
            self.reset_presentation(world.nodes)?;
        }
        if stepped.wreck.phase != atv_core::wreck::Phase::Riding {
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
                atv_core::wreck_animation::VehicleMotion {
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
            update_wheelie(Some(&mut self.rider), &stepped, DT)?;
            if let Some(landing) = stepped.landing_action {
                self.rider.request_landing(landing)?;
            }
            self.mode = Some(self.rider.step_controls(
                world.nodes,
                self.vehicle.rider_pose_input(),
                atv_core::stunt::Input {
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
        if frame.wreck.phase == atv_core::wreck::Phase::Riding && frame.speed.abs() < 3. {
            self.stuck += DT;
        } else {
            self.stuck = 0.;
        }
        let rider_wreck = self.wreck.frame();
        let rider_matrices = if rider_wreck.is_some() {
            self.wreck.pose(world.nodes)?.world_matrices
        } else {
            self.rider.pose(world.nodes)?.world_matrices
        };
        let pose = atv_core::race::CpuPose {
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
    pub fn respawn(&mut self, nodes: &[mesh::Node]) -> Result<()> {
        self.vehicle.reset_now()?;
        self.stuck = 0.;
        self.reset_presentation(nodes)
    }
    fn reset_presentation(&mut self, nodes: &[mesh::Node]) -> Result<()> {
        self.wreck.reset();
        self.wreck.set_boundary_flight(false);
        self.rider.reset(nodes)?;
        self.mode = None;
        self.driver.repositioned();
        Ok(())
    }
}
