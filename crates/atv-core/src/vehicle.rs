//! Integration of recovered drivetrain, wheel, terrain and rigid-body components.
//! This is an incomplete reconstruction, not the complete original vehicle:
//! moving surfaces, complete scene query ordering/category rules, remaining
//! gameplay gates, original math tables and whole-game replay remain outstanding.
use crate::{
    Result,
    contact::{self, ContactConfig, ContactCorrections, StaticContact},
    drivetrain::{
        DriveDirection, DrivenWheel, Drivetrain, EngineConfig, EngineInput, EngineModifiers,
    },
    dynamics::{BodyConfig, BodyState, inverse_transform, transform},
    grip::{self, LateralInput},
    materials::TerrainMaterial,
    mesh::Mesh,
    physics_data::PhysicsData,
    rider_control::{self, AxisConfig, LeanGesture, RiderConfig, RiderInput, RiderState},
    terrain::Terrain,
    wheel::{
        self, SuspensionConfig, SuspensionContact, SuspensionOutput, SuspensionState, Vec3,
        WheelConfig, WheelInput, WheelOutput, WheelState, add, dot, scale, sub,
    },
};
use serde::{Deserialize, Serialize};

/// Body+34 at 00146850 is total world speed, distinct from body-local Z.
fn speed_magnitude(velocity: Vec3) -> f32 {
    let [x, y, z] = velocity;
    let squared = (x * x + y * y) + z * z;
    if squared == 0.0 {
        0.0
    } else {
        1.0 / (1.0 / squared.sqrt())
    }
}

/// 001ed330 applies a spring at a world point through the Model inverse matrix
/// (00166510), then subtracts the current local CoG. Preserve its separate
/// rotated point and negative rotated origin: subtracting world CoG first is
/// algebraically equivalent but changes binary32 cancellation on large tracks.
fn apply_spring_world_force(body: &mut BodyState, config: &BodyConfig, force: Vec3, point: Vec3) {
    let basis = body.basis();
    let origin = body.origin(config);
    let local_point = add(
        inverse_transform(basis, point),
        scale(inverse_transform(basis, origin), -1.),
    );
    let arm = sub(local_point, config.center_of_mass);
    body.force = add(body.force, force);
    body.local_torque = add(
        body.local_torque,
        wheel::cross(arm, inverse_transform(basis, force)),
    );
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
pub struct VehicleInput {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub lean: f32,
    /// Browser gesture adaptation; absent retains the original shared lean axis.
    pub preload_axis: Option<f32>,
    pub reverse: bool,
    /// Full brake before excluding a held, remappable reverse chord. Rust owns
    /// the original speed gate; `brake` retains other independent brake inputs.
    pub reverse_chord_brake: Option<f32>,
}
/// Presentation-only: keep the strongest contact speed and where it occurred.
fn observe_impact(speed: &mut f32, point: &mut Vec3, closing: f32, at: Vec3) {
    if closing > *speed {
        *speed = closing;
        *point = at;
    }
}
#[derive(Debug, Serialize)]
pub struct VehicleFrame {
    pub origin: Vec3,
    pub orientation: [f32; 4],
    pub speed: f32,
    pub speed_magnitude: f32,
    pub linear_velocity: Vec3,
    pub local_angular_velocity: Vec3,
    pub steering_angle: f32,
    pub visual_steering_angle: f32,
    pub rpm: f32,
    pub gear: usize,
    pub wheel_angles: [f32; 4],
    pub wheel_compression: [f32; 4],
    pub grounded: [bool; 4],
    pub wheel_obstacle_support: [bool; 4],
    pub suspension_stop_contacts: usize,
    pub chassis_contact_count: usize,
    pub obstacle_contact_count: usize,
    pub dynamic_contact_count: usize,
    pub dynamic_secondary_contacts: usize,
    /// Presentation-only closing speed of this step’s strongest actual ATV contact.
    pub crash_impact_speed: f32,
    /// Presentation-only world point of that contact (valid when speed > 0).
    pub crash_impact_point: Vec3,
    pub preload_available: bool,
    pub preload_armed: bool,
    pub preload_active: bool,
    pub preload_strength: f32,
    pub wheelie_active: bool,
    pub wreck: crate::wreck::State,
    pub boundary_launch: crate::boundary_launch::State,
    pub skip_vegetation_query: bool,
    pub water_height: f32,
    pub water_failure: crate::water_failure::Clock,
    pub landing_action: Option<crate::rider_pose::Landing>,
    pub reset_count: u32,
    pub scene_query: SceneQueryFrame,
}

#[derive(Debug, Serialize)]
pub struct WheelEffectState {
    pub time: f32,
    pub radii: [f32; 4],
    pub mud: [bool; 4],
    pub mud_heights: [f32; 4],
    pub surfaces: [usize; 4],
    pub wheel_tangential_speeds: [f32; 4],
    pub throttle: f32,
}

///Most recent quad query, before the shared cache is used for recovery.
///Plant candidates are registered; physical plant proxy expansion is separate.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct SceneQueryFrame {
    pub enabled: bool,
    pub cached_nodes: usize,
    pub selected_static: usize,
    pub selected_vegetation: usize,
    pub selected_dynamic: usize,
    pub dynamic_chassis_callbacks: usize,
    pub dynamic_wheel_callbacks: usize,
    pub refreshed: bool,
    pub vegetation_proxy_queries: usize,
    pub vegetation_chassis_callbacks: usize,
    pub vegetation_wheel_callbacks: usize,
    pub unsupported_vegetation_proxies: usize,
}
struct WreckPhysics {
    rules: crate::wreck::Config,
    contact: ContactConfig,
    center: Vec3,
    landing: crate::rider_pose::LandingConfig,
}
pub struct Vehicle {
    body: BodyState,
    body_config: BodyConfig,
    engine: Drivetrain,
    wheels: [WheelState; 4],
    wheel_config: [WheelConfig; 4],
    wheel_rest: [Vec3; 4],
    wheel_stops: [Vec3; 4],
    suspension_stop_contacts: usize,
    wheel_right: [Vec3; 4],
    linkage: Option<std::rc::Rc<crate::quad_pose::QuadPose>>,
    linkage_matrices: Option<Vec<[f32; 16]>>,
    suspension: [SuspensionState; 4],
    suspension_config: SuspensionConfig,
    contacts: [SuspensionOutput; 4],
    ///001ed330 retains this projected force point even when its depth/angle
    ///gate rejects contact;001ecc40 consumes the preceding substep's value.
    wheel_contact_points: [Vec3; 4],
    wheel_world_angular_velocity: Vec3,
    supports: [crate::wheel_support::Support; 4],
    materials: Option<[TerrainMaterial; 8]>,
    tire_friction: f32,
    steering_reduction: f32,
    steering_angle: f32,
    visual_steering_angle: f32,
    steer_speed: [f32; 2],
    steer_scale: [f32; 2],
    airborne_gravity: f32,
    rider_config: RiderConfig,
    rider_state: RiderState,
    lean_gesture: LeanGesture,
    clock_ms: f32,
    stiction_speed: f32,
    axis_config: AxisConfig,
    smoothed_axes: [f32; 2],
    rider_crouch: crate::rider_pose::Crouch,
    pose_preload_axis: f32,
    base_center: Vec3,
    center_range: Vec3,
    rider_stand_up_speed: f32,
    chassis_points: Vec<Vec3>,
    chassis_contact_count: usize,
    chassis_config: ContactConfig,
    obstacle_rays: Option<std::rc::Rc<crate::collision::CollisionRayTree>>,
    wreck_rays: Option<std::rc::Rc<crate::collision::CollisionRayTree>>,
    obstacles: Vec<crate::obstacle::StaticObstacle>,
    scene_collision: Option<crate::scene_collision::SceneCollision>,
    scene_sphere_center: Vec3,
    scene_sphere_radius: f32,
    scene_query: SceneQueryFrame,
    obstacle_contact_count: usize,
    dynamic_contact_count: usize,
    dynamic_secondary_contacts: usize,
    crash_impact_speed: f32,
    crash_impact_point: Vec3,
    ///Secondary nonstatic contacts occur after the final integrator. Native
    ///translation and impact delta remain until the next first integration.
    pending_contact_corrections: ContactCorrections,
    wreck_physics: Option<WreckPhysics>,
    normal_contact: ContactConfig,
    wreck: crate::wreck::State,
    boundary_config: Option<crate::boundary_launch::Config>,
    extended_boundary: bool,
    boundary_launch: crate::boundary_launch::State,
    skip_vegetation_query: bool,
    boundary_random: crate::engine_audio::OriginalRandom,
    water_height: f32,
    water_failure: crate::water_failure::Clock,
    landing_action: Option<crate::rider_pose::Landing>,
    stunt_landing_safe: bool,
    stunt_state_four: bool,
    forced_landing: bool,
    rider_contact_cooldown: f32,
    rear_contact_speed: f32,
    applied_drive_controls: (f32, f32, bool),
    reset_origin: Vec3,
    reset_yaw: f32,
    reset_count: u32,
}
impl Vehicle {
    pub fn new(mesh: &Mesh, data: &PhysicsData, origin: Vec3, yaw: f32) -> Result<Self> {
        if !yaw.is_finite() {
            return Err("nonfinite vehicle heading".into());
        }
        let mass = data.first_scalar("Physical Properties", "Weight")? / f32::from_bits(0x4200cccd);
        let center = [
            data.first_scalar("Physical Properties", "CofGX")?,
            data.first_scalar("Physical Properties", "CofGY")?,
            data.first_scalar("Physical Properties", "CofGZ")?,
        ];
        //001e3b80 sets the sphere-bounds flag before requesting inertia extents.
        //001b3ed0 returns the aggregate radius in all three components.
        let bounds = mesh.bind_bounds()?;
        let dimensions = [bounds.sphere_radius; 3];
        let inertia = [
            data.first_scalar("Physical Properties", "PitchInertiaScale")?
                * (mass / 12.0)
                * (dimensions[1] * dimensions[1] + dimensions[2] * dimensions[2]),
            data.first_scalar("Physical Properties", "YawInertiaScale")?
                * (mass / 12.0)
                * (dimensions[0] * dimensions[0] + dimensions[2] * dimensions[2]),
            data.first_scalar("Physical Properties", "RollInertiaScale")?
                * (mass / 12.0)
                * (dimensions[0] * dimensions[0] + dimensions[1] * dimensions[1]),
        ];
        let body_config = BodyConfig::new(mass, inertia, center)?;
        let body = BodyState::new(
            origin,
            [(yaw * 0.5).cos(), 0., (yaw * 0.5).sin(), 0.],
            &body_config,
        )?;
        let suspension_config = SuspensionConfig::new(
            data.first_scalar("Suspension", "Compression")?,
            data.first_scalar("Suspension", "DecompressionRate")?,
            data.first_scalar("Suspension", "Spring")?,
            data.first_scalar("Suspension", "Shock")?,
            mass,
        )?;
        let brake = (mass * f32::from_bits(0x4200cccd))
            * data.first_scalar("Tires", "BrakeFriction")?.abs();
        let bearing = (mass * f32::from_bits(0x4200cccd))
            * data.first_scalar("Tires", "BearingFriction")?.abs();
        let brake_configuration = data.first_scalar("Tires", "BrakeConfiguration")?;
        let body_node = mesh
            .nodes
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case("body"))
            .ok_or("missing quad body node")?;
        let body_translation = [
            body_node.local_matrix[12],
            body_node.local_matrix[13],
            body_node.local_matrix[14],
        ];
        let mut configs = Vec::new();
        let mut stops = Vec::new();
        let mut rest = Vec::new();
        let mut right = Vec::new();
        for (i, name) in [
            "wheelfrontleft",
            "wheelfrontright",
            "wheelrearleft",
            "wheelrearright",
        ]
        .iter()
        .enumerate()
        {
            let node = mesh
                .nodes
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("missing {name}"))?;
            let enabled = if brake_configuration == 1.0 {
                i < 2
            } else if brake_configuration == 2.0 {
                i >= 2
            } else {
                true
            };
            configs.push(WheelConfig::from_dimensions(
                node.bounds[4],
                //00166d10 stores the authored half-X at node+0x110;
                //001ec600 consumes that value directly as wheel width.
                node.bounds[3],
                bearing,
                if enabled { brake } else { 0.0 },
            )?);
            stops.push(crate::wheel_stop::local_point(
                [
                    node.local_matrix[12],
                    node.local_matrix[13],
                    node.local_matrix[14],
                ],
                suspension_config.travel,
                configs[i].radius,
                body_translation,
            ));
            rest.push([
                node.world_matrix[12],
                node.world_matrix[13],
                node.world_matrix[14],
            ]);
            right.push([
                node.world_matrix[0],
                node.world_matrix[1],
                node.world_matrix[2],
            ]);
        }
        let right: [Vec3; 4] = right.try_into().map_err(|_| "wheel count")?;
        Ok(Self {
            body,
            body_config,
            engine: Drivetrain::new(EngineConfig::from_physics(data)?)?,
            wheels: [WheelState::default(); 4],
            wheel_stops: stops.try_into().map_err(|_| "wheel stop count")?,
            wheel_config: configs.try_into().map_err(|_| "wheel count")?,
            suspension_stop_contacts: 0,
            wheel_rest: rest.try_into().map_err(|_| "wheel count")?,
            wheel_right: right,
            linkage: None,
            linkage_matrices: None,
            suspension: [SuspensionState::default(); 4],
            suspension_config,
            contacts: [SuspensionOutput::default(); 4],
            wheel_contact_points: [[0.; 3]; 4],
            wheel_world_angular_velocity: [0.; 3],
            supports: [crate::wheel_support::Support::default(); 4],
            materials: None,
            tire_friction: data.first_scalar("Tires", "TireFriction")?,
            steering_reduction: data.first_scalar("Steering", "SteeringReduction")?,
            steering_angle: 0.0,
            visual_steering_angle: 0.0,
            steer_speed: [
                data.first_scalar("Steering", "SteerSpeed1")?,
                data.first_scalar("Steering", "SteerSpeed2")?,
            ],
            steer_scale: [
                data.first_scalar("Steering", "SteerScale1")?,
                data.first_scalar("Steering", "SteerScale2")?,
            ],
            airborne_gravity: data.first_scalar("Physical Properties", "AirborneGravity")?,
            rider_config: RiderConfig::from_physics(data)?,
            rider_state: RiderState::default(),
            lean_gesture: LeanGesture::default(),
            clock_ms: 0.0,
            stiction_speed: data.first_scalar("Tires", "StictionSpeed")?,
            axis_config: AxisConfig::from_physics(data)?,
            smoothed_axes: [0.; 2],
            rider_crouch: crate::rider_pose::Crouch::default(),
            pose_preload_axis: 0.0,
            base_center: center,
            center_range: [
                data.first_scalar("Physical Properties", "CofGRangeX")?,
                data.first_scalar("Physical Properties", "CofGRangeY")?,
                data.first_scalar("Physical Properties", "CofGRangeZ")?,
            ],
            //002d5dd0 constructor default, also authored by both supplied riders.
            rider_stand_up_speed: f32::from_bits(0x41200000),
            chassis_points: Vec::new(),
            chassis_contact_count: 0,
            chassis_config: ContactConfig::from_physics(data)?,
            obstacle_rays: None,
            wreck_rays: None,
            obstacles: Vec::new(),
            scene_collision: None,
            //001e1fd0 configures this query before001e3b80 selects sphere
            //bounds for inertia; retain the bind AABB center/diagonal here.
            scene_sphere_center: bounds.center,
            scene_sphere_radius: dot(bounds.half_extents, bounds.half_extents).sqrt(),
            scene_query: SceneQueryFrame::default(),
            obstacle_contact_count: 0,
            dynamic_contact_count: 0,
            dynamic_secondary_contacts: 0,
            crash_impact_speed: 0.,
            crash_impact_point: [0.; 3],
            pending_contact_corrections: ContactCorrections::default(),
            wreck_physics: None,
            normal_contact: ContactConfig::from_physics(data)?,
            wreck: crate::wreck::State::default(),
            boundary_config: None,
            extended_boundary: false,
            boundary_launch: crate::boundary_launch::State::default(),
            skip_vegetation_query: false,
            boundary_random: crate::engine_audio::OriginalRandom::with_seed(1),
            water_height: -256.,
            water_failure: crate::water_failure::Clock::default(),
            landing_action: None,
            stunt_landing_safe: true,
            stunt_state_four: false,
            forced_landing: false,
            rider_contact_cooldown: 0.,
            rear_contact_speed: 0.,
            applied_drive_controls: (0., 0., false),
            reset_origin: origin,
            reset_yaw: yaw,
            reset_count: 0,
        })
    }
    pub fn enable_wrecks(&mut self, data: &PhysicsData) -> Result<()> {
        self.wreck_physics = Some(WreckPhysics {
            landing: crate::rider_pose::LandingConfig::from_physics(data, 0.)?,
            rules: crate::wreck::Config::from_physics(data, 0.0)?,
            contact: ContactConfig {
                reflection: self.normal_contact.reflection,
                angular_impulse_scale: data
                    .first_scalar("Collisions", "WreckAngularImpulseScale")?,
                friction_scale: data.first_scalar("Collisions", "WreckFriction")?,
                friction_torque_scale: data
                    .first_scalar("Collisions", "WreckFrictionTorqueScale")?,
            },
            center: [
                data.first_scalar("Physical Properties", "WreckCofGX")?,
                data.first_scalar("Physical Properties", "WreckCofGY")?,
                data.first_scalar("Physical Properties", "WreckCofGZ")?,
            ],
        });
        Ok(())
    }
    pub fn set_stunt_landing_safe(&mut self, safe: bool) {
        self.stunt_landing_safe = safe;
    }
    /// CrowdPresent or native mode6 (Training) selects the extended boundary.
    /// The browser course adapter retains that policy for the Training venue.
    pub fn set_boundary(
        &mut self,
        config: crate::boundary_launch::Config,
        extended: bool,
    ) -> Result<()> {
        config.outside([0.; 3], extended)?;
        if self.wreck_physics.is_none() {
            return Err("boundary launch requires original wreck configuration".into());
        }
        self.boundary_config = Some(config);
        self.extended_boundary = extended;
        Ok(())
    }
    ///001e7940 reads the rider's previously selected mode. Only the main stunt
    /// phase (4), not its held extension (5), limits airborne rider control.
    pub fn set_rider_mode(&mut self, mode: Option<crate::rider_pose::Mode>) {
        self.stunt_state_four = mode == Some(crate::rider_pose::Mode::Stunt);
    }
    /// The pending forced action at rider+40c gates drive before mode selection.
    pub fn set_forced_landing(&mut self, active: bool) {
        self.forced_landing = active;
    }
    /// Physics reads the preceding mechanical pose; sample the new pose only
    /// after completing the outer physics update, as in 001e5260.
    pub fn set_linkage(&mut self, linkage: std::rc::Rc<crate::quad_pose::QuadPose>) {
        self.linkage = Some(linkage);
        self.linkage_matrices = None;
    }
    pub fn linkage_matrices(&self) -> Option<&[[f32; 16]]> {
        self.linkage_matrices.as_deref()
    }
    pub fn applied_drive_controls(&self) -> (f32, f32, bool) {
        self.applied_drive_controls
    }
    ///Presentation-only original tire values; reading this does not step physics.
    pub fn wheel_effect_state(&self) -> WheelEffectState {
        WheelEffectState {
            time: self.clock_ms * 0.001,
            radii: self.wheel_config.map(|wheel| wheel.radius),
            surfaces: self.supports.map(|support| support.surface),
            mud: self.supports.map(|support| support.mud),
            mud_heights: self.supports.map(|support| support.mud_height),
            wheel_tangential_speeds: std::array::from_fn(|i| {
                self.wheels[i].angular_speed * self.wheel_config[i].radius
            }),
            throttle: self.applied_drive_controls.0,
        }
    }
    /// Browser lifecycle cancellation is not a physical stick release. Forget
    /// any unfinished gesture while preserving a jump force already underway.
    pub fn cancel_preload_gesture(&mut self) {
        self.lean_gesture = LeanGesture::default();
        self.rider_state.preload_armed = false;
        if !self.rider_state.preload_active {
            self.rider_state.timer = 0.0;
        }
    }
    pub fn begin_wreck(&mut self, kind: crate::wreck::Kind) {
        if self.wreck.phase != crate::wreck::Phase::Riding {
            return;
        }
        if let Some(config) = &self.wreck_physics {
            self.wreck.begin(kind);
            self.chassis_config = config.contact;
            self.rider_state = RiderState::default();
            self.pose_preload_axis = 0.0;
        }
    }
    /// Rider contact is supplied by the separate original wreck-body simulation.
    pub fn advance_wreck(&mut self, dt: f32, rider_grounded: bool) -> Result<bool> {
        if !dt.is_finite() || dt <= 0.0 || dt > 0.1 {
            return Err("invalid wreck time step".into());
        }
        if !self.wreck.step(dt, rider_grounded) {
            return Ok(false);
        }
        self.reset_to_checkpoint()?;
        Ok(true)
    }
    ///001e35d0 is shared by wreck recovery and the expired deep-water clock.
    fn reset_to_checkpoint(&mut self) -> Result<()> {
        self.body_config.center_of_mass = self.base_center;
        self.body_config.max_speed_squared = crate::boundary_launch::NORMAL_MAX_SPEED_SQUARED;
        self.boundary_launch.reset();
        self.skip_vegetation_query = false;
        self.body = BodyState::new(
            self.reset_origin,
            [
                (self.reset_yaw * 0.5).cos(),
                0.,
                (self.reset_yaw * 0.5).sin(),
                0.,
            ],
            &self.body_config,
        )?;
        self.pending_contact_corrections = ContactCorrections::default();
        self.crash_impact_speed = 0.;
        self.wheel_contact_points = [[0.; 3]; 4];
        self.wheel_world_angular_velocity = [0.; 3];
        self.wheels = [WheelState::default(); 4];
        // 001dd440 resets wheel state but retains animated node orientation;
        // the next completed physics update refreshes the mechanical pose.
        self.suspension = [SuspensionState::default(); 4];
        self.contacts = [SuspensionOutput::default(); 4];
        self.supports = [crate::wheel_support::Support::default(); 4];
        self.suspension_stop_contacts = 0;
        self.forced_landing = false;
        self.rider_contact_cooldown = 2.;
        self.water_failure = crate::water_failure::Clock::default();
        // The native reset also preserves the grip controller's cached rear
        // velocity until the next grounded substep overwrites it.
        self.engine.state = crate::drivetrain::EngineState::default();
        self.smoothed_axes = [0.; 2];
        self.rider_state = RiderState::default();
        self.rider_crouch = crate::rider_pose::Crouch::default();
        self.lean_gesture = LeanGesture::default();
        self.pose_preload_axis = 0.0;
        self.chassis_config = self.normal_contact;
        self.wreck = crate::wreck::State::default();
        self.landing_action = None;
        self.stunt_landing_safe = true;
        self.stunt_state_four = false;
        self.chassis_contact_count = 0;
        self.obstacle_contact_count = 0;
        self.reset_count = self.reset_count.wrapping_add(1);
        Ok(())
    }
    ///002d87c0 copies this rider field without MPH conversion.
    pub fn set_rider_physics(&mut self, data: &PhysicsData) -> Result<()> {
        self.rider_stand_up_speed = data.first_scalar("Rider Properties", "RiderStandUpSpeed")?;
        Ok(())
    }
    /// Chassis points in the original ray-tree traversal order.001943d0 keeps
    /// the first coincident point and the main solver has a sixty-point capacity.
    pub fn set_chassis_points(&mut self, points: &[Vec3]) -> Result<()> {
        if points.len() > 65536 || points.iter().flatten().any(|x| !x.is_finite()) {
            return Err("invalid chassis point list".into());
        }
        self.chassis_points.clear();
        for &point in points {
            if !self.chassis_points.contains(&point) {
                self.chassis_points.push(point);
                if self.chassis_points.len() == 60 {
                    break;
                }
            }
        }
        Ok(())
    }
    pub fn set_obstacles(
        &mut self,
        rays: std::rc::Rc<crate::collision::CollisionRayTree>,
        obstacles: Vec<crate::obstacle::StaticObstacle>,
    ) -> Result<()> {
        rays.chassis_points()?;
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        if rays.transform != identity
            || obstacles
                .iter()
                .any(|o| o.mesh.transform != [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.])
        {
            return Err("nonidentity COL header transforms are not yet supported".into());
        }
        if obstacles.len() > 4096
            || obstacles
                .iter()
                .any(|o| o.mesh.nodes.is_empty() || !o.world_matrix.iter().all(|v| v.is_finite()))
        {
            return Err("unsupported obstacle list or nonfinite placement".into());
        }
        for obstacle in &obstacles {
            obstacle.mesh.validate_tree()?;
        }
        self.obstacle_rays = Some(rays);
        self.obstacles = obstacles;
        self.scene_collision = None;
        self.scene_query = SceneQueryFrame::default();
        Ok(())
    }
    ///Single-region ordinary placements and ESB nodes share one original tree
    ///and one40/50-foot candidate cache, including recovery queries.
    pub fn set_scene_partition(
        &mut self,
        config: crate::scene_partition::Config,
        ecosystem: Option<&crate::ecosystem::Ecosystem>,
        terrain: &Terrain,
    ) -> Result<()> {
        self.scene_collision = Some(crate::scene_collision::SceneCollision::from_terrain(
            config,
            self.obstacles.clone(),
            ecosystem,
            terrain,
            false,
        )?);
        self.scene_query = SceneQueryFrame {
            enabled: true,
            ..SceneQueryFrame::default()
        };
        Ok(())
    }
    pub fn set_materials(&mut self, materials: [TerrainMaterial; 8]) {
        self.materials = Some(materials);
    }
    ///001e4c90/001ee630 bind the rider to this same cache, including during
    ///separation and recovery. Borrow it; cloning would split actor membership.
    pub fn scene_collision_mut(&mut self) -> Option<&mut crate::scene_collision::SceneCollision> {
        self.scene_collision.as_mut()
    }
    ///Direct-query consumers borrow the partition without touching this cache.
    pub fn scene_collision(&self) -> Option<&crate::scene_collision::SceneCollision> {
        self.scene_collision.as_ref()
    }
    ///001e8a20 selects the AO obstacle hull during a wreck; normal riding uses AR.
    pub fn set_wreck_rays(
        &mut self,
        rays: std::rc::Rc<crate::collision::CollisionRayTree>,
    ) -> Result<()> {
        rays.chassis_points()?;
        if rays.transform
            != [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ]
        {
            return Err("nonidentity wreck collision transform".into());
        }
        self.wreck_rays = Some(rays);
        Ok(())
    }
    /// Multiplayer owner applies only its response; remote poses remain immutable.
    pub fn peer_contact(&mut self, origin: Vec3, velocity: Vec3, dt: f32, tie: f32) -> bool {
        let own = self.body.origin(&self.body_config);
        let Some(response) =
            crate::peer_contact::resolve(own, self.body.linear_velocity, origin, velocity, dt, tie)
        else {
            return false;
        };
        let delta_velocity = sub(response.velocity, self.body.linear_velocity);
        self.body.center = add(self.body.center, response.displacement);
        self.body.linear_velocity = response.velocity;
        // Use actual owner impulse in the recovered per-quad impact classifier.
        // Speed alone, penetration correction and tangential motion do not eject
        // the rider. The proxy remains an authored equal-mass approximation.
        if self.wreck.phase == crate::wreck::Phase::Riding
            && let Some(config) = &self.wreck_physics
            && let Some(kind) = config.rules.evaluate(crate::wreck::Input {
                local_impact: inverse_transform(self.body.basis(), delta_velocity),
                vertical_limit_scale: 1.,
                pitch: 0.,
                roll: 0.,
                wheel_contact: false,
                body_contact: true,
                rider_contact: false,
                forward_speed: 0.,
                stunt_landing_safe: true,
            })
        {
            self.begin_wreck(kind);
        }
        true
    }
    pub fn network_platform(&self) -> Option<crate::train::MotionState> {
        self.supports
            .iter()
            .zip(&self.contacts)
            .find_map(|(support, contact)| {
                if contact.grounded {
                    support.motion
                } else {
                    None
                }
            })
    }
    pub fn translate_network(&mut self, displacement: Vec3) {
        self.body.center = add(self.body.center, displacement);
        for point in &mut self.wheel_contact_points {
            *point = add(*point, displacement);
        }
    }
    pub fn set_network_velocity(&mut self, velocity: Vec3) -> Result<()> {
        if velocity.iter().any(|v| !v.is_finite() || v.abs() > 1000.) {
            return Err("invalid network velocity".into());
        }
        self.body.linear_velocity = velocity;
        Ok(())
    }
    pub fn frame(&self) -> VehicleFrame {
        VehicleFrame {
            origin: self.body.origin(&self.body_config),
            orientation: self.body.orientation,
            speed: inverse_transform(self.body.basis(), self.body.linear_velocity)[2],
            speed_magnitude: speed_magnitude(self.body.linear_velocity),
            rpm: self.engine.state.rpm,
            linear_velocity: self.body.linear_velocity,
            local_angular_velocity: self.body.local_angular_velocity,
            steering_angle: self.steering_angle,
            visual_steering_angle: self.visual_steering_angle,
            gear: self.engine.state.gear,
            wheel_angles: self.wheels.map(|w| w.angle),
            wheel_compression: self.suspension.map(|s| s.compression),
            grounded: self.contacts.map(|c| c.grounded),
            wheel_obstacle_support: self.supports.map(|s| s.obstacle),
            suspension_stop_contacts: self.suspension_stop_contacts,
            chassis_contact_count: self.chassis_contact_count,
            obstacle_contact_count: self.obstacle_contact_count,
            dynamic_contact_count: self.dynamic_contact_count,
            dynamic_secondary_contacts: self.dynamic_secondary_contacts,
            crash_impact_speed: self.crash_impact_speed,
            crash_impact_point: self.crash_impact_point,
            preload_available: self.rider_state.preload_status,
            preload_armed: self.rider_state.preload_armed,
            preload_active: self.rider_state.preload_active,
            preload_strength: self.rider_state.preload_strength,
            wheelie_active: self.rider_state.wheelie_active,
            wreck: self.wreck,
            boundary_launch: self.boundary_launch,
            skip_vegetation_query: self.skip_vegetation_query,
            water_height: self.water_height,
            water_failure: self.water_failure,
            landing_action: self.landing_action,
            reset_count: self.reset_count,
            scene_query: self.scene_query,
        }
    }
    /// Recovered riding inputs, with the separate touch preload also expressed
    /// through the original lean clips. Look-target acquisition remains separate.
    pub fn rider_pose_input(&self) -> crate::rider_pose::Input {
        let mut axes = self.smoothed_axes;
        if self.contacts.iter().any(|contact| contact.grounded) && axes[1].abs() < 0.01 {
            axes[1] += self.pose_preload_axis;
        }
        crate::rider_pose::Input {
            axes,
            local_forward_speed: inverse_transform(self.body.basis(), self.body.linear_velocity)[2],
            stand_up_speed: self.rider_stand_up_speed,
            wheel_contact: self.contacts.iter().any(|contact| contact.grounded),
            reverse_gear: matches!(
                self.engine.state.previous_direction,
                DriveDirection::Reverse
            ),
            reverse_override: false,
            crouch: self.rider_crouch.blend,
            look_phase: 0.5,
            force_snapshot: false,
            suppress_snapshot: false,
        }
    }
    pub fn step(
        &mut self,
        terrain: &Terrain,
        input: VehicleInput,
        dt: f32,
    ) -> Result<VehicleFrame> {
        let mut random = self.boundary_random;
        let result = self.step_with_boundary_random(terrain, input, dt, || {
            std::array::from_fn(|_| random.next_u31())
        });
        self.boundary_random = random;
        result
    }
    /// The browser shares the wreck animator's local RNG so edge spin, random
    /// separation and later detached clips consume one consecutive stream.
    pub fn step_with_boundary_random(
        &mut self,
        terrain: &Terrain,
        input: VehicleInput,
        dt: f32,
        random: impl FnMut() -> [u32; 4],
    ) -> Result<VehicleFrame> {
        self.step_with_scene(terrain, input, dt, None, random)
    }
    pub fn step_with_scene(
        &mut self,
        terrain: &Terrain,
        input: VehicleInput,
        dt: f32,
        water: Option<&crate::water_animation::LiveField>,
        mut random: impl FnMut() -> [u32; 4],
    ) -> Result<VehicleFrame> {
        if ![
            dt,
            input.throttle,
            input.brake,
            input.steer,
            input.lean,
            input.preload_axis.unwrap_or(0.0),
            input.reverse_chord_brake.unwrap_or(0.0),
        ]
        .iter()
        .all(|x| x.is_finite())
            || dt <= 0.0
            || dt > 0.1
        {
            return Err("invalid vehicle input/time step".into());
        }
        let materials = self
            .materials
            .ok_or("original terrain material table is required")?;
        //001e5870 checks +814 before physics, even when the rider is wrecked.
        if self.water_failure.reset_due() {
            self.reset_to_checkpoint()?;
        }
        self.landing_action = None;
        self.crash_impact_speed = 0.;
        let crashed = self.wreck.phase != crate::wreck::Phase::Riding;
        // 001e5260 decrements this riding cooldown before physics. The current
        // Freestyle route has no native countdown/session-state-2 override.
        if !crashed && self.rider_contact_cooldown > 0. {
            self.rider_contact_cooldown -= dt;
        }
        let input = if crashed {
            VehicleInput::default()
        } else {
            input
        };
        let throttle = if self.forced_landing {
            0.
        } else {
            input.throttle.clamp(0., 1.)
        };
        let pose_target = input.preload_axis.unwrap_or(0.0).clamp(-1.0, 1.0);
        let pose_limit = self.axis_config.normal_rate[1] * dt;
        self.pose_preload_axis +=
            (pose_target - self.pose_preload_axis).clamp(-pose_limit, pose_limit);
        let mut brake = input.brake.clamp(0., 1.);
        let reverse = if let Some(chord_brake) = input.reverse_chord_brake {
            let selected = !self.forced_landing
                && rider_control::select_native_reverse_chord(
                    throttle > 0.,
                    chord_brake > 0.,
                    self.rear_contact_speed,
                    self.rider_stand_up_speed,
                )?
                .reverse;
            if !selected {
                brake = brake.max(chord_brake.clamp(0., 1.));
            }
            selected
        } else {
            input.reverse && !self.forced_landing
        };
        self.applied_drive_controls = (throttle, brake, reverse);
        let speed = inverse_transform(self.body.basis(), self.body.linear_velocity)[2];
        let magnitude = speed_magnitude(self.body.linear_velocity);
        // 001dd830 snapshots grip/rider gates and stored support normals before
        // the outer wheel queries. Substeps keep those controller inputs fixed.
        let rider_grounded = self.contacts.iter().any(|c| c.grounded);
        let front_grounded = self.contacts[0].grounded || self.contacts[1].grounded;
        let all_contacts = self.contacts.iter().all(|c| c.grounded);
        let grip_material = materials[self.supports[0].surface];
        let mut average_normal = self.supports[0].normal;
        for support in &self.supports[1..] {
            average_normal = add(average_normal, support.normal);
        }
        average_normal = scale(average_normal, 0.25);
        let raw_axes = rider_control::smooth_axes(
            self.axis_config,
            &mut self.smoothed_axes,
            [input.steer.clamp(-1., 1.), input.lean.clamp(-1., 1.)],
            self.contacts.iter().any(|c| c.grounded),
            self.stunt_state_four,
            dt,
        )?;
        let riding_center = rider_control::shifted_center(
            self.base_center,
            self.center_range,
            self.smoothed_axes,
            speed,
            self.rider_stand_up_speed,
        );
        let center = if crashed {
            self.wreck_physics
                .as_ref()
                .map(|c| c.center)
                .unwrap_or(riding_center)
        } else if self.forced_landing {
            [0., self.body_config.center_of_mass[1], 0.]
        } else {
            riding_center
        };
        //Wheel point velocity retains body+60 until the first001469e0
        //integration. Geometry, force torque and chassis solver CoG already
        //use the unchanged Model origin and newly selected local CoG.
        let cached_world_center = self.body.center;
        //001469e0 reconstructs world center from the scene origin and current
        //local center before integration. Preserve that origin when the rider shifts.
        self.body.center = add(
            self.body.center,
            transform(
                self.body.basis(),
                sub(center, self.body_config.center_of_mass),
            ),
        );
        self.body_config.center_of_mass = center;
        let steer_scale = if self.steer_speed[0] == self.steer_speed[1] {
            0.0
        } else {
            (self.steer_scale[0]
                + ((speed.abs() - self.steer_speed[0])
                    * (self.steer_scale[1] - self.steer_scale[0]))
                    / (self.steer_speed[1] - self.steer_speed[0]))
                .clamp(
                    self.steer_scale[0].min(self.steer_scale[1]),
                    self.steer_scale[0].max(self.steer_scale[1]),
                )
        };
        if crashed {
            //001e7940 decays the stored angle directly in wreck mode.
            let limit = self.axis_config.normal_rate[0] * dt;
            self.steering_angle += (-self.steering_angle).clamp(-limit, limit);
            self.visual_steering_angle += (-self.visual_steering_angle).clamp(-limit, limit);
        } else if self.forced_landing {
            let rate = if !rider_grounded && self.stunt_state_four {
                self.axis_config.stunt_rate[0]
            } else {
                self.axis_config.normal_rate[0]
            };
            let limit = rate * dt;
            self.steering_angle += (-self.steering_angle).clamp(-limit, limit);
            // Native leaves the visual linkage angle untouched in this branch.
        } else {
            //001e7940 keeps linkage angle +440 separate from tire angle +444.
            //001d65c0 consumes +440 without the physical speed reduction.
            if self.stunt_state_four {
                self.visual_steering_angle *= 0.8;
            } else {
                self.visual_steering_angle =
                    (self.smoothed_axes[0] * 30.0) * f32::from_bits(0x3c8efa34);
            }
            self.steering_angle =
                ((self.smoothed_axes[0] * 30.0) * f32::from_bits(0x3c8efa34)) * steer_scale;
        }
        let steering = self.steering_angle;
        let b = self.body.basis();
        let origin = self.body.origin(&self.body_config);
        let ray_world = [
            b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1],
            b[2][2], 0., origin[0], origin[1], origin[2], 1.,
        ];
        let (ordered_obstacles, scene_candidates) = if let Some(scene) = &mut self.scene_collision {
            let center = crate::obstacle::transform_point(&ray_world, self.scene_sphere_center);
            let query = scene.cached_query(center, self.scene_sphere_radius, |_, _| true)?;
            let selected = scene.static_selection(&query);
            self.scene_query = SceneQueryFrame {
                enabled: true,
                cached_nodes: query.candidates.len(),
                selected_static: query
                    .strictly_within()
                    .filter(|c| {
                        matches!(c.node, crate::scene_collision::SceneNode::StaticObstacle(_))
                    })
                    .count(),
                selected_dynamic: query
                    .strictly_within()
                    .filter(|c| {
                        matches!(
                            c.node,
                            crate::scene_collision::SceneNode::DynamicObstacle(_)
                        )
                    })
                    .count(),
                selected_vegetation: query
                    .strictly_within()
                    .filter(|candidate| {
                        matches!(
                            candidate.node,
                            crate::scene_collision::SceneNode::Vegetation(_)
                        )
                    })
                    .count(),
                refreshed: query.refreshed,
                ..SceneQueryFrame::default()
            };
            (
                std::borrow::Cow::Owned(selected.into_iter().cloned().collect::<Vec<_>>()),
                Some(query),
            )
        } else {
            (std::borrow::Cow::Borrowed(self.obstacles.as_slice()), None)
        };
        let stop_planes = crate::wheel_stop::sample(terrain, &ray_world, self.wheel_stops)?;
        for i in 0..4 {
            self.supports[i] = if let Some((scene, query)) =
                self.scene_collision.as_ref().zip(scene_candidates.as_ref())
            {
                let sampled = crate::wheel_support::sample_with_scene(
                    terrain,
                    scene,
                    query,
                    self.skip_vegetation_query,
                    &ray_world,
                    self.wheel_rest[i],
                    self.suspension_config.travel,
                    self.wheel_config[i].radius,
                    self.supports[i],
                )?;
                self.scene_query.vegetation_proxy_queries += sampled.vegetation_proxies;
                self.scene_query.vegetation_wheel_callbacks += sampled.vegetation_callbacks;
                self.scene_query.dynamic_wheel_callbacks += sampled.dynamic_callbacks;
                self.scene_query.unsupported_vegetation_proxies +=
                    sampled.unsupported_vegetation_proxies;
                sampled.support
            } else {
                crate::wheel_support::sample(
                    terrain,
                    &ordered_obstacles,
                    &ray_world,
                    self.wheel_rest[i],
                    self.suspension_config.travel,
                    self.wheel_config[i].radius,
                    self.supports[i],
                )?
            };
            if self.supports[i].surface >= materials.len() {
                return Err("unsupported wheel surface".into());
            }
        }
        //001ecc40 computes the moving support's point velocity once per outer
        //wheel update, using the last001ed330 force point, then retains C0 for
        //all spring substeps. Static/plant geometry does not clear body ownership.
        let ground_velocities = std::array::from_fn::<_, 4, _>(|i| {
            self.supports[i].motion.map_or([0.; 3], |target| {
                target.point_velocity(self.wheel_contact_points[i])
            })
        });
        let driven = std::array::from_fn::<_, 2, _>(|i| DrivenWheel {
            angular_speed: self.wheels[i + 2].angular_speed,
            brake,
            surface_drive_scale: materials[self.supports[i + 2].surface].drive,
        });
        let engine = self.engine.step(
            EngineInput {
                throttle,
                direction: if reverse {
                    DriveDirection::Reverse
                } else {
                    DriveDirection::Forward
                },
                // 001dd830 supplies total speed with the sign of body-local Z.
                speed_ft_per_second: magnitude * if speed < 0.0 { -1.0 } else { 1.0 },
                allow_shifting: self.contacts[2].grounded || self.contacts[3].grounded,
                modifiers: EngineModifiers::default(),
            },
            &driven,
        )?;
        let b = self.body.basis();
        let mut drive = [WheelOutput {
            longitudinal_force: 0.,
            airborne_brake_torque: 0.,
            resistance_torque: 0.,
        }; 4];
        for (i, output) in drive.iter_mut().enumerate() {
            let m = materials[self.supports[i].surface];
            // 001ecc40 always projects through the stored wheel plane,
            // including when the previous contact flag is airborne.
            let normal = self.supports[i].normal;
            let right = transform(b, self.wheel_right[i]);
            let forward = wheel::cross(right, normal);
            let velocity = add(
                self.body.linear_velocity,
                wheel::cross(
                    self.wheel_world_angular_velocity,
                    sub(self.wheel_contact_points[i], cached_world_center),
                ),
            );
            let velocity = sub(velocity, ground_velocities[i]);
            *output = self.wheels[i].step(
                self.wheel_config[i],
                WheelInput {
                    dt,
                    grounded: self.contacts[i].grounded,
                    longitudinal_velocity: dot(velocity, forward),
                    drive_torque: if i >= 2 {
                        engine.wheel_torques[i - 2]
                    } else {
                        0.0
                    },
                    brake,
                    surface_brake_scale: m.braking,
                    surface_drive_scale: m.drive,
                },
            )?;
        }
        self.clock_ms += dt * 1000.0;
        let lean = if self.forced_landing { 0. } else { raw_axes[1] };
        let lean_rate = self.lean_gesture.sample(
            input.preload_axis.unwrap_or(lean).clamp(-1.0, 1.0),
            self.clock_ms,
        )?;
        //00146850 updates these at the outer update cadence. Host atan2 remains
        // a provisional backend until the original lookup table is loaded.
        let angles = rider_control::body_angles(self.body.orientation, f32::atan2);
        //00193b40 samples chassis terrain planes at the outer-frame cadence;
        //00194930 reprojects the moving vertices against those planes each substep.
        let mut chassis_planes = Vec::with_capacity(self.chassis_points.len());
        // 001ddcf0 snapshots the Model up.Y gate before querying. Upright
        // riding uses wheel support; chassis terrain response starts at <=0.4.
        let chassis_terrain_enabled = b[1][1] <= f32::from_bits(0x3ecccccd);
        for &local in self
            .chassis_points
            .iter()
            .filter(|_| chassis_terrain_enabled)
        {
            let world = add(
                self.body.origin(&self.body_config),
                transform(self.body.basis(), local),
            );
            if let Some(hit) = terrain.contact(world[0], world[2])? {
                let material = materials
                    .get(hit.surface as usize)
                    .ok_or("unsupported chassis surface")?;
                chassis_planes.push((
                    local,
                    [world[0], hit.height, world[2]],
                    hit.normal,
                    material.grip,
                ));
            }
        }
        let active_rays = if crashed {
            self.wreck_rays.as_ref().or(self.obstacle_rays.as_ref())
        } else {
            self.obstacle_rays.as_ref()
        };
        let mut obstacle_targets = Vec::new();
        let obstacle_contacts = if let Some(rays) = active_rays {
            if let Some((scene, query)) =
                self.scene_collision.as_ref().zip(scene_candidates.as_ref())
            {
                let queried =
                    scene.quad_contacts(query, rays, &ray_world, self.skip_vegetation_query)?;
                self.scene_query.vegetation_proxy_queries += queried.vegetation_proxies;
                self.scene_query.vegetation_chassis_callbacks += queried.vegetation_callbacks;
                self.scene_query.dynamic_chassis_callbacks += queried.dynamic_callbacks;
                self.scene_query.unsupported_vegetation_proxies +=
                    queried.unsupported_vegetation_proxies;
                obstacle_targets = queried.targets;
                queried.contacts
            } else {
                crate::obstacle::query_static_scene(rays, &ray_world, &ordered_obstacles)?
            }
        } else {
            Vec::new()
        };
        obstacle_targets.resize(obstacle_contacts.len(), None);
        let mut elapsed = 0.0;
        let mut impact: Vec3 = [0.0; 3];
        while elapsed < dt {
            let step = (dt - elapsed).min(f32::from_bits(0x3c888889));
            let b = self.body.basis();
            let origin = self.body.origin(&self.body_config);
            let contact_center = if elapsed == 0. {
                cached_world_center
            } else {
                self.body.center
            };
            self.body.force[1] -= self.body_config.mass * f32::from_bits(0x4200cccd);
            if !rider_grounded {
                self.body.force[1] -= self.airborne_gravity * f32::from_bits(0x4200cccd);
            }
            if all_contacts {
                self.body.force = add(
                    self.body.force,
                    grip::gravity_stiction(grip::StictionInput {
                        all_contacts: true,
                        mass: self.body_config.mass,
                        normal: average_normal,
                        forward: b[2],
                        local_forward_speed: inverse_transform(b, self.body.linear_velocity)[2],
                        brake,
                        stiction_speed: self.stiction_speed,
                    })?,
                );
            }
            if rider_grounded {
                let [front_velocity, rear_velocity] = grip::contact_point_velocities(
                    inverse_transform(b, self.body.linear_velocity),
                    self.body.local_angular_velocity,
                    self.body_config.center_of_mass,
                );
                self.rear_contact_speed = rear_velocity[2];
                let m = grip_material;
                let forces = grip::lateral_forces(LateralInput {
                    any_contact: true,
                    front_contact: front_grounded,
                    front_point_velocity: front_velocity,
                    rear_point_velocity: rear_velocity,
                    steering_sine: steering.sin(),
                    steering_cosine: steering.cos(),
                    tire_friction: self.tire_friction,
                    steering_reduction: self.steering_reduction,
                    material_grip: m.grip,
                    material_powerslide: m.powerslide,
                    throttle,
                })?;
                // 001d8540 rotates and accumulates front then rear separately.
                for (force, offset) in forces.into_iter().zip([3., -3.]) {
                    self.body.apply_local_force(
                        &self.body_config,
                        [force, 0., 0.],
                        [0., 0., self.body_config.center_of_mass[2] + offset],
                    );
                }
                self.body.force = add(
                    self.body.force,
                    grip::material_drag(self.body.linear_velocity, magnitude, m.drag)?,
                );
                if let Some(local) = grip::settle_local_velocity(
                    inverse_transform(b, self.body.linear_velocity),
                    brake != 0.,
                )? {
                    self.body.linear_velocity = transform(b, local);
                }
            }
            if !crashed {
                rider_control::step(
                    self.rider_config,
                    &mut self.rider_state,
                    RiderInput {
                        grounded: rider_grounded || self.forced_landing,
                        throttle_pressed: throttle > 0.,
                        brake_pressed: brake > 0.,
                        neutral: engine.effective_direction == DriveDirection::Neutral,
                        half_air_control: self.stunt_state_four && !self.forced_landing,
                        lateral_axis: if self.forced_landing { 0. } else { raw_axes[0] },
                        lean_axis: lean,
                        preload_axis: if self.forced_landing {
                            None
                        } else {
                            input.preload_axis.map(|axis| axis.clamp(-1.0, 1.0))
                        },
                        lean_axis_rate: if self.forced_landing { 0. } else { lean_rate },
                        pitch: angles[0],
                        roll: angles[2],
                        local_angular_velocity: self.body.local_angular_velocity,
                        local_forward_speed: inverse_transform(b, self.body.linear_velocity)[2],
                        world_vertical_speed: self.body.linear_velocity[1],
                        world_body_right: b[0],
                        zero_right_fallback: [0.; 3],
                        preload_world_direction: [0., 1., 0.],
                    },
                    step,
                    &mut self.body.force,
                    &mut self.body.local_torque,
                )?;
            }
            let mut corrections = if elapsed == 0. {
                std::mem::take(&mut self.pending_contact_corrections)
            } else {
                ContactCorrections::default()
            };
            // A secondary contact was already observed in the previous frame;
            // only native translation/velocity corrections carry into this step.
            corrections.closing_speed = 0.;
            self.suspension_stop_contacts = 0;
            //001dd830 updates the separate four-point travel-stop actor before
            // the wheel springs. It remains active while the chassis is upright.
            for (&local, &plane) in self.wheel_stops.iter().zip(&stop_planes) {
                let point = add(origin, transform(b, local));
                let closing_speed = -dot(self.body.linear_velocity, plane.normal);
                if crate::wheel_stop::resolve(
                    &mut self.body,
                    plane,
                    point,
                    &mut corrections.velocity_delta,
                )? {
                    self.suspension_stop_contacts += 1;
                    observe_impact(
                        &mut self.crash_impact_speed,
                        &mut self.crash_impact_point,
                        closing_speed,
                        point,
                    );
                }
            }
            for (i, d) in drive.iter().enumerate() {
                let wheel_center = add(origin, transform(b, self.wheel_rest[i]));
                let support = self.supports[i];
                let (normal, ground, available) =
                    (support.normal, support.point, support.available);
                let bottom = sub(wheel_center, scale(normal, self.wheel_config[i].radius));
                let point = add(bottom, scale(normal, dot(sub(ground, bottom), normal)));
                if available {
                    self.wheel_contact_points[i] = point;
                }
                let point_velocity = sub(
                    add(
                        self.body.linear_velocity,
                        wheel::cross(
                            self.wheel_world_angular_velocity,
                            sub(point, contact_center),
                        ),
                    ),
                    ground_velocities[i],
                );
                let output = self.suspension[i].contact(
                    self.suspension_config,
                    SuspensionContact {
                        available,
                        normal,
                        ground_point: ground,
                        wheel_center,
                        radius: self.wheel_config[i].radius,
                        body_up: b[1],
                        body_forward: b[2],
                        point_velocity,
                        stiffness_scale: 1.0,
                        tilt_angle: dot(normal, b[1]).clamp(-1., 1.).acos(),
                    },
                )?;
                if output.grounded {
                    observe_impact(
                        &mut self.crash_impact_speed,
                        &mut self.crash_impact_point,
                        -dot(point_velocity, normal),
                        point,
                    );
                    apply_spring_world_force(
                        &mut self.body,
                        &self.body_config,
                        output.force,
                        output.point,
                    );
                } else {
                    // 001ed330 invalidates this plane until the next outer query.
                    self.supports[i].available = false;
                }
                self.contacts[i] = output;
                // Original001ed330 applies the driveforce at CofG-(0,0.3,0).
                let application = sub(self.body_config.center_of_mass, [0., 0.3, 0.]);
                self.body.apply_local_force(
                    &self.body_config,
                    [0., 0., d.longitudinal_force],
                    application,
                );
                self.body.local_torque[0] += d.airborne_brake_torque;
            }
            self.chassis_contact_count = 0;
            for &(local, ground, normal, grip) in &chassis_planes {
                let vertex = add(origin, transform(b, local));
                let penetration = dot(sub(ground, vertex), normal);
                if penetration > 0.0 {
                    self.chassis_contact_count += 1;
                    contact::resolve_static(
                        &mut self.body,
                        &self.body_config,
                        self.chassis_config,
                        StaticContact {
                            point: add(vertex, scale(normal, penetration)),
                            normal,
                            penetration,
                            material_grip: grip,
                        },
                        &mut corrections,
                    )?;
                }
            }
            self.obstacle_contact_count = 0;
            self.dynamic_contact_count = 0;
            for (c, target) in obstacle_contacts.iter().zip(&obstacle_targets) {
                let vertex = add(origin, transform(b, c.local_end));
                let penetration = dot(sub(c.world_point, vertex), c.world_normal);
                if penetration > 0.0 {
                    self.obstacle_contact_count += 1;
                    self.dynamic_contact_count += usize::from(target.is_some());
                    let contact = StaticContact {
                        point: add(vertex, scale(c.world_normal, penetration)),
                        normal: c.world_normal,
                        penetration,
                        material_grip: materials[0].grip,
                    };
                    if let Some(target) = target {
                        let mut config = self.chassis_config;
                        //001dcc70 disables moving-target tangential friction.
                        config.friction_scale = 0.;
                        target.resolve_contact(
                            &mut self.body,
                            &self.body_config,
                            config,
                            contact,
                            &mut corrections,
                        )?;
                    } else {
                        contact::resolve_static(
                            &mut self.body,
                            &self.body_config,
                            self.chassis_config,
                            contact,
                            &mut corrections,
                        )?;
                    }
                }
            }
            observe_impact(
                &mut self.crash_impact_speed,
                &mut self.crash_impact_point,
                corrections.closing_speed,
                corrections.closing_point,
            );
            self.body.step(&self.body_config, step)?;
            self.wheel_world_angular_velocity = self.body.world_angular_velocity();
            for (value, next) in impact.iter_mut().zip(corrections.velocity_delta) {
                if next.abs() > value.abs() {
                    *value = next;
                }
            }
            self.body.center = add(self.body.center, corrections.translation);
            elapsed += step;
        }
        self.dynamic_secondary_contacts = 0;
        //001dd830 re-queries admitted nonstatic actors against the completed
        //body pose, uses full correction scale1, and performs no new integration.
        //Immediate velocity changes do not enter this frame's sampled impact.
        if let Some((scene, query, rays)) = self
            .scene_collision
            .as_ref()
            .zip(scene_candidates.as_ref())
            .zip(active_rays)
            .map(|((s, q), r)| (s, q, r))
            .filter(|(_, query, _)| {
                query.strictly_within().any(|c| {
                    matches!(
                        c.node,
                        crate::scene_collision::SceneNode::DynamicObstacle(_)
                    )
                })
            })
        {
            let b = self.body.basis();
            let origin = self.body.origin(&self.body_config);
            let world = [
                b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1],
                b[2][2], 0., origin[0], origin[1], origin[2], 1.,
            ];
            if chassis_terrain_enabled {
                for &local in &self.chassis_points {
                    let vertex = add(origin, transform(b, local));
                    if let Some(hit) = terrain.contact(vertex[0], vertex[2])? {
                        let penetration =
                            dot(sub([vertex[0], hit.height, vertex[2]], vertex), hit.normal);
                        if penetration > 0. {
                            contact::resolve_static_with_position_scale(
                                &mut self.body,
                                &self.body_config,
                                self.chassis_config,
                                StaticContact {
                                    point: add(vertex, scale(hit.normal, penetration)),
                                    normal: hit.normal,
                                    penetration,
                                    material_grip: materials[hit.surface as usize].grip,
                                },
                                &mut self.pending_contact_corrections,
                                1.,
                            )?;
                        }
                    }
                }
            }
            let queried = scene.dynamic_contacts(query, rays, &world)?;
            for (c, target) in queried.contacts.iter().zip(&queried.targets) {
                let vertex = add(origin, transform(b, c.local_end));
                let penetration = dot(sub(c.world_point, vertex), c.world_normal);
                if penetration > 0.
                    && let Some(target) = target
                {
                    self.dynamic_secondary_contacts += 1;
                    let mut config = self.chassis_config;
                    config.friction_scale = 0.;
                    target.resolve_contact_with_position_scale(
                        &mut self.body,
                        &self.body_config,
                        config,
                        StaticContact {
                            point: add(vertex, scale(c.world_normal, penetration)),
                            normal: c.world_normal,
                            penetration,
                            material_grip: materials[0].grip,
                        },
                        &mut self.pending_contact_corrections,
                        1.,
                    )?;
                }
            }
        }
        observe_impact(
            &mut self.crash_impact_speed,
            &mut self.crash_impact_point,
            self.pending_contact_corrections.closing_speed,
            self.pending_contact_corrections.closing_point,
        );
        //001e5260 runs edge launch after physics and before impact/wreck gates.
        // Query bit0 excludes vegetation expansion, not terrain/static objects.
        if self.boundary_launch.tick(dt, true)? {
            self.skip_vegetation_query = false;
        }
        if !self.boundary_launch.active
            && self.wreck.phase == crate::wreck::Phase::Riding
            && let Some(config) = self.boundary_config
            && config.outside(self.body.origin(&self.body_config), self.extended_boundary)?
        {
            let words = random();
            let launch =
                config.launch(self.body.origin(&self.body_config), [words[0], words[1]])?;
            let kind = crate::boundary_launch::random_wreck_kind([words[2], words[3]])?;
            self.body.linear_velocity = launch.linear_velocity;
            self.body.local_angular_velocity = launch.local_angular_velocity;
            self.body_config.max_speed_squared = crate::boundary_launch::LAUNCH_MAX_SPEED_SQUARED;
            self.boundary_launch.begin();
            self.skip_vegetation_query = true;
            self.begin_wreck(kind);
        }
        //001e5ff8 applies the deep-water clock after physics/edge launch and
        //before the impact classifier. Its height sample is still last frame's.
        self.water_failure.update(
            self.body.origin(&self.body_config)[1],
            self.water_height,
            self.wreck.phase == crate::wreck::Phase::Riding,
            dt,
        );
        for i in 0..4 {
            self.suspension[i].visual_position(
                self.suspension_config,
                dt,
                self.wheel_rest[i],
                [0., -1., 0.],
            )?;
        }
        self.rider_crouch.step(
            inverse_transform(self.body.basis(), self.body.linear_velocity)[1],
            dt,
        )?;
        if self.wreck.phase == crate::wreck::Phase::Riding
            && let Some(config) = &self.wreck_physics
        {
            let angles = rider_control::body_angles(self.body.orientation, f32::atan2);
            let local_impact = inverse_transform(self.body.basis(), impact);
            let landing_candidate = config.landing.evaluate(local_impact, 1.0);
            let rider_contact = crate::wreck::rider_height_allowed(
                crate::wreck::rider_height_contact(
                    self.chassis_contact_count + self.obstacle_contact_count > 0,
                    obstacle_contacts.first().map(|contact| contact.local_end),
                ),
                self.rider_contact_cooldown,
                landing_candidate.is_some(),
            );
            if let Some(kind) = config.rules.evaluate(crate::wreck::Input {
                local_impact,
                vertical_limit_scale: 1.0,
                pitch: angles[0],
                roll: angles[2],
                wheel_contact: self.contacts.iter().any(|c| c.grounded),
                body_contact: self.chassis_contact_count + self.obstacle_contact_count > 0,
                rider_contact,
                forward_speed: inverse_transform(self.body.basis(), self.body.linear_velocity)[2],
                stunt_landing_safe: self.stunt_landing_safe,
            }) {
                self.begin_wreck(kind);
            } else {
                self.landing_action = landing_candidate;
            }
        }
        //001ea4d0 permits a terrain-projected boundary checkpoint during flight.
        // Keep the port's outer-update sampling cadence; the original virtual
        // caller's complete scheduling is still being recovered separately.
        let origin = self.body.origin(&self.body_config);
        if let Some(ground) = terrain.contact(origin[0], origin[2])? {
            let (candidate, allowed) = crate::boundary_launch::checkpoint_candidate(
                origin,
                crate::boundary_launch::CheckpointGates {
                    boundary_flight: self.boundary_launch.active,
                    player_kind_two: false,
                    forced: false,
                    all_wheels_grounded: self.contacts.iter().all(|c| c.grounded),
                    wrecked: self.wreck.phase != crate::wreck::Phase::Riding,
                    body_up_y: self.body.basis()[1][1],
                    terrain_height: ground.height,
                    water_height: self.water_height,
                },
            )?;
            if allowed {
                let forward = self.body.basis()[2];
                let yaw = forward[0].atan2(forward[2]);
                let support = if self.wreck_physics.is_some() {
                    if let Some(scene) = &mut self.scene_collision {
                        //001ea050 queries the same cache at radius twenty,
                        //then uses its own strict trim and plant exclusion.
                        let query = scene.cached_query(candidate, 20., |_, _| true)?;
                        if scene.vegetation_blocks_checkpoint(&query)? {
                            None
                        } else {
                            let selected: Vec<_> = scene
                                .static_selection(&query)
                                .into_iter()
                                .cloned()
                                .collect();
                            crate::wreck::checkpoint_static_support(candidate, yaw, &selected)?
                        }
                    } else {
                        crate::wreck::checkpoint_static_support(candidate, yaw, &self.obstacles)?
                    }
                } else {
                    Some(candidate)
                };
                if let Some(support) = support {
                    self.reset_origin = support;
                    self.reset_yaw = yaw;
                }
            }
        }
        //001e5260 caches the current point's dynamic water height late in the
        // outer update. Earlier gates read the preceding update's cached value.
        if let Some(field) = water {
            self.water_height = field.height(self.body.origin(&self.body_config), false)?;
        }
        if let Some(linkage) = &self.linkage {
            let matrices = linkage.sample(
                self.visual_steering_angle,
                self.wheels.map(|wheel| wheel.angle),
                self.suspension.map(|suspension| suspension.compression),
            )?;
            self.wheel_right = linkage.wheel_right_axes(&matrices)?;
            self.linkage_matrices = Some(matrices);
        }
        Ok(self.frame())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deep_water_uses_cached_height_and_resets_to_retained_checkpoint() {
        let (mut vehicle, terrain) = fixture();
        // Synthetic constant field isolates the outer cached-height/reset gate.
        let water = crate::water_animation::LiveField {
            definition: crate::water_animation::Definition::parse(
                b"[General Information]\nWaterBaseHeight=100\nWakeScale=0\nWaterHeightRange=0\n",
            )
            .unwrap(),
            swell: crate::water_animation::SwellState::default(),
            states: vec![],
            waves: vec![],
            frames: vec![],
        };
        let checkpoint = vehicle.reset_origin;
        vehicle.body =
            BodyState::new([700., 1., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
        let step = |vehicle: &mut Vehicle| {
            vehicle
                .step_with_scene(
                    &terrain,
                    VehicleInput::default(),
                    1. / 60.,
                    Some(&water),
                    || panic!("water must not request a random wreck"),
                )
                .unwrap()
        };
        let first = step(&mut vehicle);
        assert_eq!(first.water_height, 100.);
        assert_eq!(first.water_failure.remaining, 5.);
        assert!(!first.water_failure.submerged);
        let second = step(&mut vehicle);
        assert_eq!(second.water_failure.remaining, 5. - 1. / 60.);
        assert!(second.water_failure.submerged);
        let mut last = second;
        while last.water_failure.remaining > 0. {
            assert_eq!(last.reset_count, 0);
            assert_eq!(last.wreck.phase, crate::wreck::Phase::Riding);
            last = step(&mut vehicle);
        }
        assert_eq!(
            last.reset_count, 0,
            "late expiration waits until next update"
        );
        let reset = step(&mut vehicle);
        assert_eq!(reset.reset_count, 1);
        assert!((reset.origin[0] - checkpoint[0]).abs() < 0.001);
        assert_eq!(
            vehicle.reset_origin, checkpoint,
            "water cannot replace the dry checkpoint"
        );
        assert_eq!(reset.wreck.phase, crate::wreck::Phase::Riding);
        assert_eq!(reset.water_failure.remaining, 5. - 1. / 60.);
        assert_eq!(
            reset.water_height, 100.,
            "reset preserves the shared surface cache"
        );
    }

    #[test]
    fn expired_water_clock_resets_wreck_state_before_physics() {
        let (mut vehicle, terrain) = fixture();
        enable_test_wrecks(&mut vehicle);
        vehicle.begin_wreck(crate::wreck::Kind::Front);
        vehicle.water_failure.remaining = 0.;
        let frame = vehicle
            .step(&terrain, VehicleInput::default(), 1. / 60.)
            .unwrap();
        assert_eq!(frame.reset_count, 1);
        assert_eq!(frame.wreck.phase, crate::wreck::Phase::Riding);
        assert_eq!(frame.water_failure.remaining, 5.);
        assert!(!frame.water_failure.submerged);
    }

    #[test]
    fn spring_force_retains_model_inverse_order_at_large_world_positions() {
        let config = BodyConfig::new(10., [2., 3., 4.], [0.125, 0.2, -0.25]).unwrap();
        let mut body = BodyState::new([4096., 100., 3072.], [0.8, 0.6, 0., 0.], &config).unwrap();
        body.force = [1., 2., 3.];
        body.local_torque = [4., 5., 6.];
        let point = [4094.5, 99.75, 3073.25];
        let force = [17., 1400., -30.];
        let before = body.clone();
        let mut world_center_order = body.clone();
        world_center_order.apply_world_force(force, point);
        apply_spring_world_force(&mut body, &config, force, point);
        // Authored fixture; expected bits come from the isolated original
        // 001ed85c..001edad0 force/torque instruction block, not game assets.
        assert_eq!(body.force, [18., 1402., -27.]);
        assert_eq!(
            body.local_torque.map(f32::to_bits),
            [0xc4c2d4c6, 0xc50825ee, 0xc4160082],
        );
        assert_ne!(body.local_torque, world_center_order.local_torque);
        assert_eq!(body.center, before.center);
        assert_eq!(body.orientation, before.orientation);
        assert_eq!(body.linear_velocity, before.linear_velocity);
        assert_eq!(body.local_angular_velocity, before.local_angular_velocity);
    }

    #[test]
    fn boundary_flight_replaces_motion_once_and_restores_limits_on_recovery() {
        let (mut vehicle, terrain) = fixture();
        enable_test_wrecks(&mut vehicle);
        let config = crate::boundary_launch::Config {
            terrain_width: 4.,
            terrain_breadth: 4.,
            width_scale: 1.,
            breadth_scale: 1.,
        };
        vehicle.set_boundary(config, false).unwrap();
        vehicle.body =
            BodyState::new([100., 100., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
        vehicle.body.linear_velocity = [-50., -10., 0.];
        let mut draws = 0;
        let frame = vehicle
            .step_with_boundary_random(&terrain, VehicleInput::default(), 1. / 60., || {
                draws += 1;
                [0, 0, 0, 0]
            })
            .unwrap();
        let expected = config.launch(frame.origin, [0, 0]).unwrap();
        assert_eq!(frame.linear_velocity, expected.linear_velocity);
        assert_eq!(
            frame.local_angular_velocity,
            expected.local_angular_velocity
        );
        assert!(
            frame.origin[0] < 100.,
            "outward physics must precede launch"
        );
        assert!(frame.origin[1] < 100., "launch must not teleport the body");
        assert_eq!(frame.wreck.phase, crate::wreck::Phase::Falling);
        assert_eq!(frame.wreck.kind, Some(crate::wreck::Kind::Front));
        assert!(frame.boundary_launch.active && frame.skip_vegetation_query);
        assert_eq!(frame.boundary_launch.elapsed, 0.);
        assert_eq!(
            vehicle.body_config.max_speed_squared,
            crate::boundary_launch::LAUNCH_MAX_SPEED_SQUARED
        );
        for _ in 0..61 {
            vehicle
                .step_with_boundary_random(&terrain, VehicleInput::default(), 1. / 60., || {
                    draws += 1;
                    [0; 4]
                })
                .unwrap();
        }
        assert_eq!(
            draws, 1,
            "active boundary flight must not draw or launch again"
        );
        assert!(vehicle.frame().boundary_launch.active);
        assert!(!vehicle.frame().skip_vegetation_query);
        for _ in 0..600 {
            if vehicle.advance_wreck(1. / 60., true).unwrap() {
                break;
            }
        }
        assert_eq!(vehicle.frame().reset_count, 1);
        assert!(
            vehicle.frame().origin[0] > frame.origin[0] + 50.,
            "boundary recovery must follow the inward flight, not the old edge checkpoint"
        );
        assert_eq!(
            vehicle.frame().origin[1],
            0.,
            "boundary checkpoint projects onto the dry terrain"
        );
        assert!(!vehicle.frame().boundary_launch.active);
        assert_eq!(
            vehicle.body_config.max_speed_squared,
            crate::boundary_launch::NORMAL_MAX_SPEED_SQUARED
        );
    }
    #[test]
    fn crowd_boundary_allows_the_original_extended_margin() {
        let (mut vehicle, terrain) = fixture();
        enable_test_wrecks(&mut vehicle);
        vehicle
            .set_boundary(
                crate::boundary_launch::Config {
                    terrain_width: 4.,
                    terrain_breadth: 4.,
                    width_scale: 1.,
                    breadth_scale: 1.,
                },
                true,
            )
            .unwrap();
        vehicle.body =
            BodyState::new([100., 100., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
        let frame = vehicle
            .step_with_boundary_random(&terrain, VehicleInput::default(), 1. / 60., || {
                panic!("inside extended bounds")
            })
            .unwrap();
        assert!(!frame.boundary_launch.active);
        assert_eq!(frame.wreck.phase, crate::wreck::Phase::Riding);
    }
    #[test]
    fn reverse_chord_uses_cached_contact_speed_and_retains_brakes_during_landing() {
        for (cached_speed, forced, independent_brake, expected_reverse) in [
            (9., false, 0., true),
            (10., false, 0., false),
            (100., false, 0., false),
            (-100., false, 0., true),
            (0., true, 0., false),
            (0., false, 1., false),
        ] {
            let (mut vehicle, terrain) = fixture();
            vehicle.body =
                BodyState::new([500., 100., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
            vehicle.body.linear_velocity = [0., 0., 20.];
            vehicle.rear_contact_speed = cached_speed;
            vehicle.set_forced_landing(forced);
            vehicle
                .step(
                    &terrain,
                    VehicleInput {
                        throttle: 1.,
                        brake: independent_brake,
                        reverse: true,
                        reverse_chord_brake: Some(1.),
                        ..Default::default()
                    },
                    1. / 60.,
                )
                .unwrap();
            assert_eq!(
                vehicle.engine.state.previous_direction == DriveDirection::Reverse,
                expected_reverse
            );
            // No wheel was grounded in the previous outer update. Native keeps
            // the cached rear point velocity while airborne, even as body speed changes.
            assert_eq!(vehicle.rear_contact_speed, cached_speed);
            if !expected_reverse {
                assert_eq!(vehicle.engine.state.torque_per_wheel, 0.);
            }
        }
    }
    #[test]
    fn hard_landing_suppresses_drive_and_body_shift_until_clip_finishes() {
        let mut traces = Vec::new();
        for brake in [0., 1.] {
            let (mut vehicle, terrain) = fixture();
            vehicle.body =
                BodyState::new([500., 100., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
            vehicle.body_config.center_of_mass = [0.5, 1.2, -0.4];
            vehicle.steering_angle = 0.2;
            vehicle.visual_steering_angle = 0.2;
            vehicle.axis_config.normal_rate[0] = 6.;
            for wheel in &mut vehicle.wheels {
                wheel.angular_speed = 10.;
            }
            vehicle.set_forced_landing(true);
            vehicle
                .step(
                    &terrain,
                    VehicleInput {
                        throttle: 1.,
                        brake,
                        steer: 1.,
                        lean: 1.,
                        reverse: true,
                        ..Default::default()
                    },
                    1. / 60.,
                )
                .unwrap();
            assert_ne!(
                vehicle.engine.state.previous_direction,
                DriveDirection::Reverse
            );
            assert_eq!(vehicle.engine.state.torque_per_wheel, 0.);
            assert_eq!(vehicle.body_config.center_of_mass, [0., 1.2, 0.]);
            // Matches the native forced-landing control branch: physical angle
            // returns to center while the visual linkage retains its last angle.
            assert_eq!(vehicle.steering_angle, 0.2 - 6. * (1. / 60.));
            assert_eq!(vehicle.visual_steering_angle, 0.2);
            traces.push(vehicle.wheels[0].angular_speed);
            vehicle.set_forced_landing(false);
            vehicle
                .step(
                    &terrain,
                    VehicleInput {
                        throttle: 1.,
                        steer: 1.,
                        ..Default::default()
                    },
                    1. / 60.,
                )
                .unwrap();
            assert!(vehicle.engine.state.torque_per_wheel > 0.);
            assert_ne!(vehicle.visual_steering_angle, 0.2);
        }
        assert!(
            traces[1] < traces[0],
            "landing must preserve the player's brakes"
        );
    }

    #[test]
    fn sideways_speed_feeds_native_engine_taper_and_surface_drag() {
        let (mut vehicle, terrain) = fixture();
        vehicle.body =
            BodyState::new([500., 50., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
        // The body is moving above TopSpeed even though its forward component
        // is small. 001dd830 passes signed magnitude into the engine taper.
        vehicle.body.linear_velocity = [100., 0., 1.];
        vehicle
            .step(
                &terrain,
                VehicleInput {
                    throttle: 1.,
                    ..Default::default()
                },
                1. / 60.,
            )
            .unwrap();
        assert_eq!(vehicle.engine.state.torque_per_wheel, 0.);

        let mut traces = Vec::new();
        let mut mass = 0.;
        for drag in [0., 1.] {
            let (mut vehicle, terrain) = fixture();
            mass = vehicle.body_config.mass;
            vehicle.body =
                BodyState::new([500., 50., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
            vehicle.body.linear_velocity = [30., 0., 40.];
            vehicle.tire_friction = 0.;
            for material in vehicle.materials.as_mut().unwrap() {
                material.drag = drag;
            }
            // Previous contact gates the original outer update's grip pass.
            for contact in &mut vehicle.contacts {
                contact.grounded = true;
            }
            traces.push(
                vehicle
                    .step(&terrain, VehicleInput::default(), 1. / 60.)
                    .unwrap(),
            );
        }
        // Speed 50: native drag is 2500 along normalized world velocity,
        // followed by AB2's zero-history first acceleration update.
        for (axis, force) in [(0, -1500.), (2, -2000.)] {
            let delta = traces[1].linear_velocity[axis] - traces[0].linear_velocity[axis];
            assert!((delta - force / mass * (1.5 / 60.)).abs() < 0.00001);
        }
    }

    #[test]
    fn cancelled_hold_cannot_release_from_resumed_forward_lean() {
        let mut results = Vec::new();
        for cancelled in [false, true] {
            let (mut vehicle, terrain) = fixture();
            // A rising, fast-moving quad still touching a ramp is the original
            // force gate. Forward lean is independent of the held browser brace.
            vehicle.body =
                BodyState::new([500., -1., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
            vehicle
                .step(&terrain, VehicleInput::default(), 1. / 60.)
                .unwrap();
            vehicle.body.linear_velocity = [0., 10., 50.];
            let held = vehicle
                .step(
                    &terrain,
                    VehicleInput {
                        lean: -1.,
                        preload_axis: Some(1.),
                        ..Default::default()
                    },
                    1. / 60.,
                )
                .unwrap();
            assert!(
                held.preload_armed,
                "fixture must reach native arming: {held:?}"
            );
            if cancelled {
                vehicle.cancel_preload_gesture();
            }
            let released = vehicle
                .step(
                    &terrain,
                    VehicleInput {
                        lean: -1.,
                        ..Default::default()
                    },
                    1. / 60.,
                )
                .unwrap();
            results.push(released);
        }
        assert!(results[0].preload_active, "uncancelled hold must release");
        assert!(results[0].preload_strength > 0.);
        assert!(!results[1].preload_active);
        assert!(!results[1].preload_armed);
        assert_eq!(results[1].preload_strength, 0.);
        assert!(results[0].linear_velocity[1] > results[1].linear_velocity[1]);
    }

    #[test]
    fn cancellation_preserves_an_already_active_jump() {
        let (mut vehicle, _) = fixture();
        vehicle.rider_state.preload_active = true;
        vehicle.rider_state.preload_armed = true;
        vehicle.rider_state.preload_strength = 0.75;
        vehicle.rider_state.timer = 0.2;
        vehicle.cancel_preload_gesture();
        assert!(vehicle.rider_state.preload_active);
        assert!(!vehicle.rider_state.preload_armed);
        assert_eq!(vehicle.rider_state.preload_strength, 0.75);
        assert_eq!(vehicle.rider_state.timer, 0.2);
    }

    fn enable_test_wrecks(vehicle: &mut Vehicle) {
        vehicle.wreck_physics = Some(WreckPhysics {
            landing: crate::rider_pose::LandingConfig {
                wreck_impact: [1000.; 3],
                vertical: 800.,
                longitudinal: 800.,
            },
            rules: crate::wreck::Config {
                impact_limit: [1000.; 3],
                ground_angles: [1.4; 3],
                air_angles: [1.5; 3],
            },
            contact: ContactConfig {
                reflection: 0.002,
                angular_impulse_scale: 1.,
                friction_scale: 0.75,
                friction_torque_scale: 0.4,
            },
            center: [0., 1.5, 0.],
        });
    }
    #[test]
    fn peer_impulses_trigger_directional_wrecks_but_gentle_bumps_do_not() {
        for (velocity, other_delta, other_velocity, expected) in [
            (
                [0., 0., 60.],
                [0., 0., 5.],
                [0., 0., -60.],
                Some(crate::wreck::Kind::Front),
            ),
            (
                [0.; 3],
                [0., 0., -5.],
                [0., 0., 60.],
                Some(crate::wreck::Kind::Back),
            ),
            (
                [60., 0., 0.],
                [5., 0., 0.],
                [0.; 3],
                Some(crate::wreck::Kind::Right),
            ),
            ([0., 0., 5.], [0., 0., 5.], [0.; 3], None),
            ([0., 0., 60.], [5., 0., 0.], [0.; 3], None),
            ([0.; 3], [0.; 3], [0.; 3], None),
        ] {
            let (mut vehicle, _) = fixture();
            enable_test_wrecks(&mut vehicle);
            vehicle.wreck_physics.as_mut().unwrap().rules.impact_limit = [19., 84., 21.];
            vehicle.body.linear_velocity = velocity;
            let other = add(vehicle.body.origin(&vehicle.body_config), other_delta);
            assert!(vehicle.peer_contact(other, other_velocity, 1. / 60., 1.));
            assert_eq!(vehicle.wreck.kind, expected);
            if expected.is_some() {
                assert_eq!(vehicle.wreck.phase, crate::wreck::Phase::Falling);
                let serial_phase = vehicle.wreck;
                vehicle.peer_contact(other, other_velocity, 1. / 60., 1.);
                assert_eq!(vehicle.wreck.kind, serial_phase.kind);
            }
        }
    }

    #[test]
    fn wreck_uses_passive_body_and_recovers_only_after_rider_contact() {
        let (mut vehicle, terrain) = fixture();
        enable_test_wrecks(&mut vehicle);
        vehicle.body.linear_velocity = [5., 0., 0.];
        vehicle.body.local_angular_velocity = [1., 0., 0.];
        vehicle.steering_angle = 0.4;
        vehicle.visual_steering_angle = 0.5;
        vehicle.set_rider_mode(Some(crate::rider_pose::Mode::Stunt));
        vehicle.begin_wreck(crate::wreck::Kind::Front);
        vehicle
            .step(
                &terrain,
                VehicleInput {
                    throttle: 1.,
                    lean: 1.,
                    steer: 1.,
                    ..Default::default()
                },
                1. / 60.,
            )
            .unwrap();
        assert_eq!(vehicle.smoothed_axes, [0.; 2]);
        assert_eq!(
            vehicle.steering_angle,
            0.4 - (vehicle.axis_config.normal_rate[0] / 60.0).min(0.4)
        );
        assert_eq!(
            vehicle.visual_steering_angle,
            0.5 - vehicle.axis_config.normal_rate[0] / 60.0
        );
        assert_eq!(vehicle.body_config.center_of_mass, [0., 1.5, 0.]);
        assert_eq!(vehicle.chassis_config.friction_scale, 0.75);
        for _ in 0..180 {
            assert!(!vehicle.advance_wreck(1. / 60., false).unwrap());
        }
        assert_eq!(vehicle.wreck.phase, crate::wreck::Phase::Recovering);
        assert_eq!(vehicle.wreck.timer, 0.);
        vehicle.rear_contact_speed = 21.;
        let mut recovered = false;
        for _ in 0..120 {
            if vehicle.advance_wreck(1. / 60., true).unwrap() {
                recovered = true;
                break;
            }
        }
        assert!(recovered);
        assert_eq!(vehicle.frame().reset_count, 1);
        assert_eq!(vehicle.rider_contact_cooldown, 2.);
        assert_eq!(vehicle.rear_contact_speed, 21.);
        assert_eq!(vehicle.frame().origin, [500., 1., 500.]);
        assert_eq!(vehicle.body.linear_velocity, [0.; 3]);
        assert_eq!(vehicle.body.local_angular_velocity, [0.; 3]);
        assert_eq!(vehicle.body_config.center_of_mass, vehicle.base_center);
        assert_eq!(
            vehicle.chassis_config.friction_scale,
            vehicle.normal_contact.friction_scale
        );
        assert_eq!(vehicle.wreck.phase, crate::wreck::Phase::Riding);
        assert!(!vehicle.stunt_state_four);
    }
    #[test]
    fn visual_steering_is_unscaled_and_decays_without_changing_tire_steering() {
        use crate::rider_pose::Mode;
        let (mut vehicle, terrain) = fixture();
        vehicle.body =
            BodyState::new([500., 100., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
        vehicle.body.linear_velocity = [0., 0., 100.];
        vehicle.smoothed_axes = [0.5, 0.];
        // No input moment keeps the physical comparison independent of body
        // rotation; high speed selects the fixture's original 0.3 tire scale.
        vehicle.rider_config.air_moment = 0.;
        vehicle.rider_config.air_force = 0.;
        let input = VehicleInput {
            steer: 0.5,
            ..Default::default()
        };
        let first = vehicle.step(&terrain, input, 1. / 60.).unwrap();
        let unscaled = (0.5 * 30.0) * f32::from_bits(0x3c8efa34);
        assert_eq!(first.visual_steering_angle, unscaled);
        assert_eq!(first.steering_angle, unscaled * 0.3);
        vehicle.set_rider_mode(Some(Mode::Stunt));
        let main = vehicle.step(&terrain, input, 1. / 60.).unwrap();
        assert_eq!(main.visual_steering_angle, unscaled * 0.8);
        assert_eq!(main.steering_angle, first.steering_angle);
        let second_main = vehicle.step(&terrain, input, 1. / 60.).unwrap();
        assert_eq!(second_main.visual_steering_angle, (unscaled * 0.8) * 0.8);
        assert_eq!(second_main.steering_angle, first.steering_angle);
        vehicle.set_rider_mode(Some(Mode::Hold));
        let held = vehicle.step(&terrain, input, 1. / 60.).unwrap();
        assert_eq!(held.visual_steering_angle, unscaled);
        assert_eq!(held.steering_angle, first.steering_angle);
    }
    #[test]
    fn unfinished_stunt_on_wheel_contact_enters_original_vertical_wreck() {
        let (mut vehicle, terrain) = fixture();
        enable_test_wrecks(&mut vehicle);
        for _ in 0..90 {
            vehicle
                .step(&terrain, VehicleInput::default(), 1. / 60.)
                .unwrap();
        }
        assert!(vehicle.frame().grounded.iter().any(|c| *c));
        assert_eq!(vehicle.wreck.phase, crate::wreck::Phase::Riding);
        vehicle.set_stunt_landing_safe(false);
        // Recovery cooldown only gates rider-height impacts, not unsafe tricks.
        vehicle.rider_contact_cooldown = 2.;
        let frame = vehicle
            .step(&terrain, VehicleInput::default(), 1. / 60.)
            .unwrap();
        assert_eq!(frame.wreck.kind, Some(crate::wreck::Kind::Vertical));
        assert_eq!(frame.wreck.phase, crate::wreck::Phase::Falling);
    }
    #[test]
    fn airborne_main_stunt_limits_control_but_held_stunt_matches_normal() {
        use crate::rider_pose::Mode;
        let mut traces = Vec::new();
        for mode in [Mode::Air, Mode::Stunt, Mode::Hold] {
            let (mut vehicle, terrain) = fixture();
            vehicle.body =
                BodyState::new([500., 100., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
            vehicle.set_rider_mode(Some(mode));
            let input = VehicleInput {
                lean: 1.,
                steer: 0.5,
                ..Default::default()
            };
            let dt = 1. / 60.;
            vehicle.step(&terrain, input, dt).unwrap();
            let rate = if mode == Mode::Stunt { 2.5 } else { 4.0 };
            assert_eq!(vehicle.smoothed_axes, [rate * dt; 2]);
            let mut trace = Vec::new();
            for _ in 0..20 {
                let frame = vehicle.step(&terrain, input, dt).unwrap();
                assert!(frame.grounded.iter().all(|c| !c));
                trace.push(frame.local_angular_velocity);
            }
            traces.push(trace);
            vehicle.set_rider_mode(Some(Mode::Hold));
            assert!(!vehicle.stunt_state_four);
            vehicle.set_rider_mode(None);
            assert!(!vehicle.stunt_state_four);
        }
        assert_eq!(traces[0], traces[2]);
        for (normal, main) in traces[0].iter().zip(&traces[1]) {
            assert!(normal[0] < main[0] && main[0] < 0.0);
        }
    }
    use crate::{
        mesh::Node,
        terrain::{TerrainLeaf, TerrainTile},
    };
    fn fixture() -> (Vehicle, Terrain) {
        let text=b"[Physical Properties]\nWeight=600\nCofGX=0\nCofGY=0.2\nCofGZ=0\nCofGRangeX=0.8\nCofGRangeY=0\nCofGRangeZ=1\nPitchInertiaScale=1\nYawInertiaScale=1\nRollInertiaScale=1\nAirborneGravity=1\n[Suspension]\nCompression=1\nDecompressionRate=4\nSpring=0.7\nShock=0.2\n[Tires]\nBrakeFriction=0.2\nBearingFriction=0.05\nBrakeConfiguration=3\nTireFriction=1.5\nStictionSpeed=5\n[Steering]\nSteerSpeed1=1\nSteerSpeed2=80\nSteerScale1=1\nSteerScale2=0.3\nSteeringReduction=0.1\n[Engine]\nNumberOfRPMPoints=3\nRPM1=1000\nHP1=10\nRPM2=5000\nHP2=40\nRPM3=8000\nHP3=20\nNumberOfGears=3\nGearRatio1=2\nGearRatio2=1.5\nGearRatio3=1\nFinalGearRatio=6\nTransmissionLoss=0.8\nRampUpFilter=0.1\nRampDownFilter=0.2\nRampDownSpeed=45\nTopSpeed=60\n[Aerodynamics]\nAeroForce=10\nAeroMoment=40\nAeroDamping=-80\nAeroMomentScaleX=-1\nAeroMomentScaleY=1\nAeroMomentScaleZ=-1\nUnrollGain=2\nMaxWheelieAngle=45\nWheelieGain=2\nTopWheelieSpeed=100\nMaxPreload=100\nPreloadMinAxisRate=1\nPreloadMaxAxisRate=80\n[Joystick]\nXAxisRateLimit=4\nYAxisRateLimit=4\nStuntXAxisRateLimit=2.5\nStuntYAxisRateLimit=2.5\nXAxisDeadZone=0\n[Collisions]\nObstacleReflection=0.002\nDefaultAngularImpulseScale=0\nDefaultFriction=0.3\nDefaultFrictionTorqueScale=0.1\n";
        let mut nodes = Vec::new();
        for (name, p) in [
            ("body", [0.; 3]),
            ("wheelfrontleft", [-1.5, 0.8, 2.]),
            ("wheelfrontright", [1.5, 0.8, 2.]),
            ("wheelrearleft", [-1.5, 0.8, -2.]),
            ("wheelrearright", [1.5, 0.8, -2.]),
        ] {
            let mut m = [0.; 16];
            for i in [0, 5, 10, 15] {
                m[i] = 1.;
            }
            m[12..15].copy_from_slice(&p);
            nodes.push(Node {
                name: name.into(),
                parent: -1,
                local_matrix: m,
                world_matrix: m,
                bounds: if name == "body" {
                    [0., 1., 0., 2., 1.5, 3., 4.]
                } else {
                    [0., 0., 0., 0.4, 0.8, 0.8, 1.2]
                },
            });
        }
        let mesh = Mesh {
            nodes,
            materials: vec![],
            lods: vec![],
            trailing_bytes: 0,
        };
        let mut v = Vehicle::new(
            &mesh,
            &PhysicsData::parse(text).unwrap(),
            [500., 1., 500.],
            0.,
        )
        .unwrap();
        v.set_materials(
            [TerrainMaterial {
                grip: 1.,
                drag: 0.,
                drive: 1.,
                powerslide: 1.,
                braking: 1.,
            }; 8],
        );
        let mut leaves = Vec::new();
        for z in 0..8 {
            for x in 0..8 {
                leaves.push(TerrainLeaf {
                    x: x * 16,
                    z: z * 16,
                    tile: 0,
                    height_morph: 1.,
                    height_scale: 1.,
                    minimum_height: 0.,
                    maximum_height: 0.,
                });
            }
        }
        let t = Terrain {
            spacing: 8.,
            tiles: vec![TerrainTile {
                tag: 0,
                heights: [0; 289],
                attributes: [0x00007f00; 289],
            }],
            leaves,
            hierarchy_nodes: 64,
            width_cells: 128,
            depth_cells: 128,
        };
        (v, t)
    }
    #[test]
    fn shifted_cog_retains_first_substep_velocity_center_without_moving_model_queries() {
        let (mut vehicle, terrain) = fixture();
        let origin = [500., -0.4, 500.];
        vehicle.body.center = add(origin, vehicle.body_config.center_of_mass);
        vehicle.body.linear_velocity = [3., -2., 40.];
        vehicle.body.local_angular_velocity = [0., 0., 1.];
        vehicle.wheel_world_angular_velocity = vehicle.body.world_angular_velocity();
        let old_center = vehicle.body.center;
        let points = vehicle.wheel_rest.map(|rest| {
            let center = add(origin, rest);
            [center[0], 0., center[2]]
        });
        let expected = points.map(|point| vehicle.body.velocity_at(point)[1]);
        vehicle
            .step(
                &terrain,
                VehicleInput {
                    steer: 1.,
                    ..VehicleInput::default()
                },
                1. / 60.,
            )
            .unwrap();
        assert_ne!(vehicle.body_config.center_of_mass[0], 0.);
        assert_ne!(old_center, add(origin, vehicle.body_config.center_of_mass));
        for (i, contact) in vehicle.contacts.iter().enumerate() {
            assert!(contact.grounded);
            assert_eq!(contact.point, points[i], "Model query moved with local CoG");
            assert_eq!(
                contact.normal_speed, expected[i],
                "spring used refreshed center before integration"
            );
        }
    }
    #[test]
    fn suspension_travel_stops_catch_landings_before_springs_on_flat_and_side_slopes() {
        for sloped in [false, true] {
            let (mut vehicle, mut terrain) = fixture();
            let normal = if sloped {
                // Continuous authored synthetic plane y=x/2, including tile seams.
                terrain.tiles.clear();
                for leaf in &mut terrain.leaves {
                    leaf.tile = terrain.tiles.len() as u16;
                    leaf.height_scale = 0.5;
                    terrain.tiles.push(TerrainTile {
                        tag: 0,
                        heights: std::array::from_fn(|i| (leaf.x + (i % 17) as u32) as u16),
                        attributes: [u32::from_le_bytes([(-57_i8) as u8, 114, 0, 0]); 289],
                    });
                }
                let angle = 0.5_f32.atan();
                vehicle.body.orientation = [(angle / 2.).cos(), 0., 0., (angle / 2.).sin()];
                [-angle.sin(), angle.cos(), 0.]
            } else {
                [0., 1., 0.]
            };
            let ground = terrain.contact(500., 500.).unwrap().unwrap().height;
            let origin = [500., ground - 1.3, 500.];
            vehicle.body.center = add(
                origin,
                transform(vehicle.body.basis(), vehicle.body_config.center_of_mass),
            );
            // Inward landing speed plus motion across the slope and into a turn.
            let tangent = [normal[1] * 5., -normal[0] * 5., 15.];
            vehicle.body.linear_velocity = add(tangent, scale(normal, -30.));
            let frame = vehicle
                .step(
                    &terrain,
                    VehicleInput {
                        steer: 0.5,
                        ..VehicleInput::default()
                    },
                    1. / 60.,
                )
                .unwrap();
            assert_eq!(frame.suspension_stop_contacts, 4);
            assert_eq!(
                frame.chassis_contact_count, 0,
                "upright stop must work without chassis terrain response"
            );
            assert!(
                dot(frame.linear_velocity, normal) >= 0.,
                "landing still moving into terrain: {frame:?}"
            );
            assert!(
                dot(sub(frame.origin, origin), normal) >= 0.,
                "bottomed suspension sank further: {frame:?}"
            );
            assert!(
                frame.linear_velocity[2] > 10.,
                "stop must preserve travel along the surface"
            );
            assert!(frame.grounded.iter().all(|&grounded| grounded));
        }
    }
    #[test]
    fn obstacle_ray_contact_rejects_wall_penetration() {
        use crate::collision::{
            CollisionMesh, CollisionRay, CollisionRayTree, RayTreeNode, Triangle,
        };
        use std::rc::Rc;
        let (mut vehicle, terrain) = fixture();
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let node = || RayTreeNode {
            bound_measure: 10.,
            center: [0.; 3],
            half_extents: [10.; 3],
            children: [0x8000, 0x8000],
        };
        let rays = Rc::new(CollisionRayTree {
            transform: identity,
            query_capacity: 1,
            nodes: vec![node()],
            rays: vec![CollisionRay {
                start: [0., 0., -1.],
                end: [0., 0., 1.],
            }],
        });
        let mesh = Rc::new(CollisionMesh {
            transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
            vertices: vec![[-10., -10., 0.], [0., 10., 0.], [10., -10., 0.]],
            vertex_normal_bits: None,
            triangles: vec![Triangle {
                invalid_plane_bits: None,
                indices: [0, 1, 2],
                normal: [0., 0., -1.],
                distance: 0.,
            }],
            broad_phase_nodes: 1,
            nodes: vec![node()],
            flags: 0,
        });
        let mut placement = identity;
        placement[12] = 500.;
        placement[13] = 1.;
        placement[14] = 500.;
        vehicle
            .set_obstacles(
                rays,
                vec![crate::obstacle::StaticObstacle {
                    mesh,
                    world_matrix: placement,
                }],
            )
            .unwrap();
        vehicle.body.linear_velocity = [0., 0., 10.];
        let frame = vehicle
            .step(&terrain, VehicleInput::default(), 1. / 60.)
            .unwrap();
        assert_eq!(frame.obstacle_contact_count, 1);
        assert!(frame.linear_velocity[2] < 1.);
        assert!(frame.origin[2] < 500.);
    }

    #[test]
    fn planted_cylinder_rejects_chassis_motion_and_actor_bit_disables_only_expansion() {
        use crate::{
            collision::{CollisionRay, CollisionRayTree, RayTreeNode},
            ecosystem::{Ecosystem, Placement, Species},
            scene_partition::Config,
        };
        let mut frames = Vec::new();
        for skip in [false, true] {
            let (mut vehicle, terrain) = fixture();
            let identity = [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ];
            let rays = std::rc::Rc::new(CollisionRayTree {
                transform: identity,
                query_capacity: 1,
                nodes: vec![RayTreeNode {
                    center: [0.; 3],
                    half_extents: [2.; 3],
                    bound_measure: 48.,
                    children: [0x8000; 2],
                }],
                rays: vec![CollisionRay {
                    start: [0.2, 0., -2.],
                    end: [0.2, 0., 1.],
                }],
            });
            vehicle.set_obstacles(rays, vec![]).unwrap();
            let eco = Ecosystem {
                species: vec![Species {
                    slot: 0,
                    model: String::new(),
                    billboard: String::new(),
                    atlas: None,
                    parameters: [0; 23],
                    optional_name: None,
                    proxies: vec![[1, 0, 0, 0, 0, 0, 0, 1f32.to_bits(), 5f32.to_bits()]],
                }],
                placements: vec![Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32000, 0, 32000],
                    height: 5.,
                    radius: 2.,
                }],
                global_name: None,
                global_word: 0,
                placement_texture: None,
                visibility_index: Default::default(),
            };
            vehicle
                .set_scene_partition(
                    Config {
                        min_x: 0.,
                        max_x: 1024.,
                        min_z: 0.,
                        max_z: 1024.,
                        cell_size: 32.,
                    },
                    Some(&eco),
                    &terrain,
                )
                .unwrap();
            vehicle.skip_vegetation_query = skip;
            vehicle.body.linear_velocity = [0., 0., 10.];
            frames.push(
                vehicle
                    .step(&terrain, VehicleInput::default(), 1. / 60.)
                    .unwrap(),
            );
        }
        let hit = &frames[0];
        let bypassed = &frames[1];
        assert_eq!(hit.scene_query.selected_static, 0);
        assert_eq!(hit.scene_query.selected_vegetation, 1);
        assert_eq!(hit.scene_query.vegetation_chassis_callbacks, 1);
        assert_eq!(hit.scene_query.unsupported_vegetation_proxies, 0);
        assert_eq!(hit.obstacle_contact_count, 1);
        assert!(hit.linear_velocity[2] < 1.);
        assert!(hit.origin[2] < 500.);
        assert_eq!(bypassed.scene_query.selected_vegetation, 1);
        assert_eq!(bypassed.scene_query.vegetation_proxy_queries, 0);
        assert_eq!(bypassed.obstacle_contact_count, 0);
        assert!(bypassed.linear_velocity[2] > 9.);
    }

    #[test]
    fn chassis_terrain_response_uses_the_native_outer_body_up_gate() {
        for inverted in [false, true] {
            let (mut vehicle, terrain) = fixture();
            let local = [0., if inverted { 2. } else { -2. }, 0.];
            vehicle.set_chassis_points(&[local, local]).unwrap();
            assert_eq!(vehicle.chassis_points.len(), 1);
            let orientation = if inverted {
                [0., 0., 0., 1.]
            } else {
                [1., 0., 0., 0.]
            };
            vehicle.body =
                BodyState::new([500., 1., 500.], orientation, &vehicle.body_config).unwrap();
            let frame = vehicle
                .step(&terrain, VehicleInput::default(), 1. / 60.)
                .unwrap();
            assert_eq!(frame.chassis_contact_count, usize::from(inverted));
            if inverted {
                assert!(frame.origin[1] > 1., "{frame:?}");
            } else {
                assert!(frame.origin[1] < 1., "{frame:?}");
            }
        }
    }
    #[test]
    fn recovered_components_drive_brake_and_remain_finite() {
        let (mut v, t) = fixture();
        for _ in 0..180 {
            v.step(&t, VehicleInput::default(), 1. / 60.).unwrap();
        }
        let rest = v.frame();
        assert!(rest.grounded.iter().all(|&x| x), "{rest:?}");
        assert!(rest.origin[1] > -1. && rest.origin[1] < 1.);
        let throttle = VehicleInput {
            throttle: 1.,
            ..Default::default()
        };
        for _ in 0..240 {
            v.step(&t, throttle, 1. / 60.).unwrap();
        }
        let moving = v.frame();
        assert!(moving.speed > 5., "{moving:?}");
        assert!(moving.origin[2] > rest.origin[2] + 5.);
        let brake = VehicleInput {
            brake: 1.,
            ..Default::default()
        };
        for _ in 0..180 {
            v.step(&t, brake, 1. / 60.).unwrap();
        }
        assert!(
            v.frame().speed.abs() < moving.speed.abs(),
            "{:?}",
            v.frame()
        );
    }
    #[test]
    fn finite_wheel_rays_support_a_prop_above_terrain() {
        use crate::collision::{
            CollisionMesh, CollisionRay, CollisionRayTree, RayTreeNode, Triangle,
        };
        use std::rc::Rc;
        let (mut vehicle, terrain) = fixture();
        vehicle.body =
            BodyState::new([500., 51., 500.], [1., 0., 0., 0.], &vehicle.body_config).unwrap();
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let node = || RayTreeNode {
            bound_measure: 100.,
            center: [0.; 3],
            half_extents: [10.; 3],
            children: [0x8000, 0x8000],
        };
        let rays = Rc::new(CollisionRayTree {
            transform: identity,
            query_capacity: 1,
            nodes: vec![node()],
            rays: vec![CollisionRay {
                start: [0., 0., -1.],
                end: [0., 0., 1.],
            }],
        });
        let mesh = Rc::new(CollisionMesh {
            transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
            vertices: vec![[-10., 0., -10.], [0., 0., 10.], [10., 0., -10.]],
            vertex_normal_bits: None,
            triangles: vec![Triangle {
                indices: [0, 1, 2],
                normal: [0., 1., 0.],
                distance: 0.,
                invalid_plane_bits: None,
            }],
            broad_phase_nodes: 1,
            nodes: vec![node()],
            flags: 0,
        });
        let mut placement = identity;
        placement[12..15].copy_from_slice(&[500., 50., 500.]);
        vehicle
            .set_obstacles(
                rays,
                vec![crate::obstacle::StaticObstacle {
                    mesh,
                    world_matrix: placement,
                }],
            )
            .unwrap();
        for _ in 0..180 {
            vehicle
                .step(&terrain, VehicleInput::default(), 1. / 60.)
                .unwrap();
        }
        let resting = vehicle.frame();
        assert!(resting.grounded.iter().all(|c| *c), "{resting:?}");
        assert!(
            resting.wheel_obstacle_support.iter().all(|c| *c),
            "{resting:?}"
        );
        assert!(
            resting.origin[1] > 49.4 && resting.origin[1] < 50.1,
            "{resting:?}"
        );
        assert_eq!(resting.chassis_contact_count, 0);
        assert_eq!(resting.obstacle_contact_count, 0);
        // Removing the prop must release wheel support, allowing the vehicle
        // to fall toward terrain fifty feet below rather than retain its plane.
        vehicle.obstacles.clear();
        for _ in 0..30 {
            vehicle
                .step(&terrain, VehicleInput::default(), 1. / 60.)
                .unwrap();
        }
        let falling = vehicle.frame();
        assert!(falling.origin[1] < resting.origin[1] - 2., "{falling:?}");
        assert!(falling.grounded.iter().all(|c| !c));
        assert!(falling.wheel_obstacle_support.iter().all(|c| !c));
    }
    #[test]
    fn empty_scene_partition_preserves_legacy_vehicle_physics_on_flat_and_slope() {
        for sloped in [false, true] {
            let (mut legacy, mut terrain) = fixture();
            let (mut combined, _) = fixture();
            if sloped {
                //Continuous synthetic y=x/8 surface with recovered triangle
                //height/normal queries, crossing the original tile seams.
                terrain.tiles.clear();
                for leaf in &mut terrain.leaves {
                    leaf.tile = terrain.tiles.len() as u16;
                    leaf.height_scale = 0.125;
                    terrain.tiles.push(TerrainTile {
                        tag: 0,
                        heights: std::array::from_fn(|i| (leaf.x + (i % 17) as u32) as u16),
                        attributes: [u32::from_le_bytes([(-16_i8) as u8, 126, 0, 0]); 289],
                    });
                }
            }
            let angle = if sloped { 0.125_f32.atan() } else { 0. };
            let orientation = [(angle / 2.).cos(), 0., 0., (angle / 2.).sin()];
            let ground = terrain.contact(500., 500.).unwrap().unwrap().height;
            for vehicle in [&mut legacy, &mut combined] {
                enable_test_wrecks(vehicle);
                vehicle.body =
                    BodyState::new([500., ground + 1., 500.], orientation, &vehicle.body_config)
                        .unwrap();
            }
            combined
                .set_scene_partition(
                    crate::scene_partition::Config {
                        min_x: 0.,
                        max_x: 1024.,
                        min_z: 0.,
                        max_z: 1024.,
                        cell_size: 96.,
                    },
                    None,
                    &terrain,
                )
                .unwrap();
            let mut grounded_frames = 0;
            for step in 0..120 {
                let input = VehicleInput {
                    throttle: if step < 30 { 0. } else { 0.7 },
                    brake: if step >= 100 { 0.3 } else { 0. },
                    steer: if step < 60 { 0. } else { 0.25 },
                    lean: if step >= 80 { -0.4 } else { 0. },
                    ..VehicleInput::default()
                };
                let mut before = legacy.step(&terrain, input, 1. / 60.).unwrap();
                let mut after = combined.step(&terrain, input, 1. / 60.).unwrap();
                assert!(after.scene_query.enabled);
                assert_eq!(after.scene_query.cached_nodes, 0);
                assert_eq!(after.scene_query.selected_static, 0);
                assert_eq!(after.scene_query.selected_vegetation, 0);
                assert!(!before.scene_query.enabled);
                grounded_frames += usize::from(after.grounded.iter().all(|contact| *contact));
                //Compare all exported physical/gameplay channels, excluding
                //only the intentional difference in candidate diagnostics.
                before.scene_query = SceneQueryFrame::default();
                after.scene_query = SceneQueryFrame::default();
                assert_eq!(
                    format!("{before:?}"),
                    format!("{after:?}"),
                    "surface sloped={sloped}, step={step}"
                );
                assert_eq!(legacy.reset_origin, combined.reset_origin);
                assert_eq!(legacy.reset_yaw.to_bits(), combined.reset_yaw.to_bits());
            }
            assert!(
                grounded_frames > 60,
                "fixture did not exercise grounded driving"
            );
            assert!(legacy.frame().origin[2] > 500., "fixture did not move");
        }
    }

    #[test]
    fn scene_vegetation_checkpoint_gate_preserves_safe_point_and_respects_source_edges() {
        use crate::ecosystem::{Ecosystem, Placement, Species};
        for proxy_count in [1, 0] {
            let (mut vehicle, terrain) = fixture();
            enable_test_wrecks(&mut vehicle);
            let ecosystem = Ecosystem {
                species: vec![Species {
                    slot: 0,
                    model: String::new(),
                    billboard: String::new(),
                    atlas: None,
                    parameters: [0; 23],
                    optional_name: None,
                    proxies: vec![[0; 9]; proxy_count],
                }],
                placements: vec![Placement {
                    slot: 0,
                    resample_height: true,
                    coordinates: [35200, 0, 32000],
                    height: 20.,
                    radius: 2.,
                }],
                global_name: None,
                global_word: 0,
                placement_texture: None,
                visibility_index: Default::default(),
            };
            vehicle
                .set_scene_partition(
                    crate::scene_partition::Config {
                        min_x: 0.,
                        max_x: 1024.,
                        min_z: 0.,
                        max_z: 1024.,
                        cell_size: 96.,
                    },
                    Some(&ecosystem),
                    &terrain,
                )
                .unwrap();
            for _ in 0..120 {
                vehicle
                    .step(&terrain, VehicleInput::default(), 1. / 60.)
                    .unwrap();
            }
            let settled = vehicle.frame();
            assert!(settled.grounded.iter().all(|contact| *contact));
            assert_eq!(settled.wreck.phase, crate::wreck::Phase::Riding);
            let safe_point = vehicle.reset_origin;
            assert_eq!(safe_point[0], 500.);
            //Controlled candidate relocation keeps the same settled pose. All
            //grounding, quad cache and recovery-radius queries run through real
            //Vehicle steps; no contact flags or checkpoint result are forced.
            for x in [550., 570., 572., 580.] {
                vehicle.body = BodyState::new(
                    [x, settled.origin[1], 500.],
                    settled.orientation,
                    &vehicle.body_config,
                )
                .unwrap();
                for _ in 0..30 {
                    vehicle
                        .step(&terrain, VehicleInput::default(), 1. / 60.)
                        .unwrap();
                }
                let frame = vehicle.frame();
                assert!(
                    frame.grounded.iter().all(|contact| *contact),
                    "x={x}: {frame:?}"
                );
                assert_eq!(frame.wreck.phase, crate::wreck::Phase::Riding);
                let scene = vehicle.scene_collision.as_ref().unwrap();
                assert_eq!(scene.vegetation()[0].proxy_count, proxy_count);
                let plant_distance = scene
                    .cache()
                    .entries()
                    .iter()
                    .find(|(index, _)| *index == 0)
                    .unwrap()
                    .1;
                assert_eq!(plant_distance, (x - 550. - 2.).max(0.));
                if proxy_count > 0 && x <= 570. {
                    //Exact horizontal proximity20 remains excluded despite
                    //its width-based cached distance18 being strictly below20.
                    assert_eq!(
                        vehicle.reset_origin, safe_point,
                        "unsafe checkpoint advanced at x={x}"
                    );
                } else {
                    //Proxyless plants do not reject. At x572 the retained
                    //distance is exactly20, so the recovery strict gate skips
                    //it; at x580 the candidate is clearly outside the circle.
                    assert_eq!(vehicle.reset_origin[0], x);
                    assert_eq!(vehicle.reset_origin, frame.origin);
                }
                if x == 550. {
                    assert_eq!(frame.scene_query.selected_vegetation, 1);
                }
                assert_eq!(frame.scene_query.selected_static, 0);
                assert_eq!(
                    frame.obstacle_contact_count, 0,
                    "plant metadata is not physical proxy geometry"
                );
            }
        }
    }
}
