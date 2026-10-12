//! Integration of recovered drivetrain, wheel, terrain and rigid-body components.
//! This is an incomplete reconstruction, not the complete original vehicle:
//! obstacle queries, rider gameplay gates, wreck transitions, moving surfaces,
//! original math tables, and original runtime validation remain outstanding.
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

#[derive(Debug, Default, Clone, Copy, Deserialize)]
pub struct VehicleInput {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub lean: f32,
    /// Browser gesture adaptation; absent retains the original shared lean axis.
    pub preload_axis: Option<f32>,
    pub reverse: bool,
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
    pub chassis_contact_count: usize,
    pub obstacle_contact_count: usize,
    pub preload_available: bool,
    pub preload_armed: bool,
    pub preload_active: bool,
    pub preload_strength: f32,
    pub wheelie_active: bool,
    pub wreck: crate::wreck::State,
    pub reset_count: u32,
}
struct WreckPhysics {
    rules: crate::wreck::Config,
    contact: ContactConfig,
    center: Vec3,
    checkpoint_rays: crate::collision::CollisionRayTree,
}
pub struct Vehicle {
    body: BodyState,
    body_config: BodyConfig,
    engine: Drivetrain,
    wheels: [WheelState; 4],
    wheel_config: [WheelConfig; 4],
    wheel_rest: [Vec3; 4],
    suspension: [SuspensionState; 4],
    suspension_config: SuspensionConfig,
    contacts: [SuspensionOutput; 4],
    surface: [usize; 4],
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
    obstacle_contact_count: usize,
    wreck_physics: Option<WreckPhysics>,
    normal_contact: ContactConfig,
    wreck: crate::wreck::State,
    stunt_landing_safe: bool,
    stunt_state_four: bool,
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
        let dimensions = [mesh.bind_bounds()?.sphere_radius; 3];
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
        let mut configs = Vec::new();
        let mut rest = Vec::new();
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
            rest.push([
                node.world_matrix[12],
                node.world_matrix[13],
                node.world_matrix[14],
            ]);
        }
        Ok(Self {
            body,
            body_config,
            engine: Drivetrain::new(EngineConfig::from_physics(data)?)?,
            wheels: [WheelState::default(); 4],
            wheel_config: configs.try_into().map_err(|_| "wheel count")?,
            wheel_rest: rest.try_into().map_err(|_| "wheel count")?,
            suspension: [SuspensionState::default(); 4],
            suspension_config,
            contacts: [SuspensionOutput::default(); 4],
            surface: [0; 4],
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
            obstacle_contact_count: 0,
            wreck_physics: None,
            normal_contact: ContactConfig::from_physics(data)?,
            wreck: crate::wreck::State::default(),
            stunt_landing_safe: true,
            stunt_state_four: false,
            reset_origin: origin,
            reset_yaw: yaw,
            reset_count: 0,
        })
    }
    pub fn enable_wrecks(&mut self, data: &PhysicsData) -> Result<()> {
        self.wreck_physics = Some(WreckPhysics {
            checkpoint_rays: crate::wreck::checkpoint_clearance_rays(),
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
    ///001e7940 reads the rider's previously selected mode. Only the main stunt
    /// phase (4), not its held extension (5), limits airborne rider control.
    pub fn set_rider_mode(&mut self, mode: Option<crate::rider_pose::Mode>) {
        self.stunt_state_four = mode == Some(crate::rider_pose::Mode::Stunt);
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
        self.body_config.center_of_mass = self.base_center;
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
        self.wheels = [WheelState::default(); 4];
        self.suspension = [SuspensionState::default(); 4];
        self.contacts = [SuspensionOutput::default(); 4];
        self.engine.state = crate::drivetrain::EngineState::default();
        self.smoothed_axes = [0.; 2];
        self.rider_state = RiderState::default();
        self.rider_crouch = crate::rider_pose::Crouch::default();
        self.lean_gesture = LeanGesture::default();
        self.pose_preload_axis = 0.0;
        self.chassis_config = self.normal_contact;
        self.wreck = crate::wreck::State::default();
        self.stunt_landing_safe = true;
        self.stunt_state_four = false;
        self.chassis_contact_count = 0;
        self.obstacle_contact_count = 0;
        self.reset_count = self.reset_count.wrapping_add(1);
        Ok(true)
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
        Ok(())
    }
    pub fn set_materials(&mut self, materials: [TerrainMaterial; 8]) {
        self.materials = Some(materials);
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
    pub fn frame(&self) -> VehicleFrame {
        VehicleFrame {
            origin: self.body.origin(&self.body_config),
            orientation: self.body.orientation,
            speed: inverse_transform(self.body.basis(), self.body.linear_velocity)[2],
            speed_magnitude: {
                let [x, y, z] = self.body.linear_velocity;
                let squared = (x * x + y * y) + z * z;
                if squared == 0.0 {
                    0.0
                } else {
                    1.0 / (1.0 / squared.sqrt())
                }
            },
            rpm: self.engine.state.rpm,
            linear_velocity: self.body.linear_velocity,
            local_angular_velocity: self.body.local_angular_velocity,
            steering_angle: self.steering_angle,
            visual_steering_angle: self.visual_steering_angle,
            gear: self.engine.state.gear,
            wheel_angles: self.wheels.map(|w| w.angle),
            wheel_compression: self.suspension.map(|s| s.compression),
            grounded: self.contacts.map(|c| c.grounded),
            chassis_contact_count: self.chassis_contact_count,
            obstacle_contact_count: self.obstacle_contact_count,
            preload_available: self.rider_state.preload_status,
            preload_armed: self.rider_state.preload_armed,
            preload_active: self.rider_state.preload_active,
            preload_strength: self.rider_state.preload_strength,
            wheelie_active: self.rider_state.wheelie_active,
            wreck: self.wreck,
            reset_count: self.reset_count,
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
        if ![
            dt,
            input.throttle,
            input.brake,
            input.steer,
            input.lean,
            input.preload_axis.unwrap_or(0.0),
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
        let crashed = self.wreck.phase != crate::wreck::Phase::Riding;
        let input = if crashed {
            VehicleInput::default()
        } else {
            input
        };
        let throttle = input.throttle.clamp(0., 1.);
        let pose_target = input.preload_axis.unwrap_or(0.0).clamp(-1.0, 1.0);
        let pose_limit = self.axis_config.normal_rate[1] * dt;
        self.pose_preload_axis +=
            (pose_target - self.pose_preload_axis).clamp(-pose_limit, pose_limit);
        let brake = input.brake.clamp(0., 1.);
        let speed = inverse_transform(self.body.basis(), self.body.linear_velocity)[2];
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
        } else {
            riding_center
        };
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
        let driven = std::array::from_fn::<_, 2, _>(|i| DrivenWheel {
            angular_speed: self.wheels[i + 2].angular_speed,
            brake,
            surface_drive_scale: materials[self.surface[i + 2]].drive,
        });
        let engine = self.engine.step(
            EngineInput {
                throttle,
                direction: if input.reverse {
                    DriveDirection::Reverse
                } else {
                    DriveDirection::Forward
                },
                speed_ft_per_second: speed,
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
            let m = materials[self.surface[i]];
            let normal = if self.contacts[i].grounded {
                terrain
                    .contact(self.contacts[i].point[0], self.contacts[i].point[2])?
                    .map(|x| x.normal)
                    .unwrap_or(b[1])
            } else {
                b[1]
            };
            let right = if i < 2 {
                transform(b, [steering.cos(), 0., -steering.sin()])
            } else {
                b[0]
            };
            let forward = wheel::cross(right, normal);
            let velocity = self.body.velocity_at(self.contacts[i].point);
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
        let lean = raw_axes[1];
        let lean_rate = self.lean_gesture.sample(
            input.preload_axis.unwrap_or(lean).clamp(-1.0, 1.0),
            self.clock_ms,
        )?;
        //00146850 updates these at the outer update cadence. Host atan2 remains
        // a provisional backend until the original lookup table is loaded.
        let angles = rider_control::body_angles(self.body.orientation, f32::atan2);
        let rider_grounded = self.contacts.iter().any(|c| c.grounded);
        let all_contacts = self.contacts.iter().all(|c| c.grounded);
        let mut average_normal = [0.; 3];
        for contact in self.contacts {
            if let Some(hit) = terrain.contact(contact.point[0], contact.point[2])? {
                average_normal = add(average_normal, hit.normal);
            }
        }
        average_normal = scale(average_normal, 0.25);
        //00193b40 samples chassis terrain planes at the outer-frame cadence;
        //00194930 reprojects the moving vertices against those planes each substep.
        let mut chassis_planes = Vec::with_capacity(self.chassis_points.len());
        for &local in &self.chassis_points {
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
        let b = self.body.basis();
        let origin = self.body.origin(&self.body_config);
        let ray_world = [
            b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1],
            b[2][2], 0., origin[0], origin[1], origin[2], 1.,
        ];
        let active_rays = if crashed {
            self.wreck_rays.as_ref().or(self.obstacle_rays.as_ref())
        } else {
            self.obstacle_rays.as_ref()
        };
        let obstacle_contacts = if let Some(rays) = active_rays {
            crate::obstacle::query_static_scene(rays, &ray_world, &self.obstacles)?
        } else {
            Vec::new()
        };
        let mut elapsed = 0.0;
        let mut impact: Vec3 = [0.0; 3];
        while elapsed < dt {
            let step = (dt - elapsed).min(f32::from_bits(0x3c888889));
            let b = self.body.basis();
            let origin = self.body.origin(&self.body_config);
            let was_grounded = self.contacts.iter().any(|c| c.grounded);
            self.body.force[1] -= self.body_config.mass * f32::from_bits(0x4200cccd);
            if !was_grounded {
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
            if was_grounded {
                let front = add(
                    origin,
                    transform(b, [0., 0., self.body_config.center_of_mass[2] + 3.]),
                );
                let rear = add(
                    origin,
                    transform(b, [0., 0., self.body_config.center_of_mass[2] - 3.]),
                );
                let m = materials[self.surface[0]];
                let forces = grip::lateral_forces(LateralInput {
                    any_contact: true,
                    front_contact: self.contacts[0].grounded || self.contacts[1].grounded,
                    front_point_velocity: inverse_transform(b, self.body.velocity_at(front)),
                    rear_point_velocity: inverse_transform(b, self.body.velocity_at(rear)),
                    steering_sine: steering.sin(),
                    steering_cosine: steering.cos(),
                    tire_friction: self.tire_friction,
                    steering_reduction: self.steering_reduction,
                    material_grip: m.grip,
                    material_powerslide: m.powerslide,
                    throttle,
                })?;
                let (force, moment) =
                    grip::lateral_force_and_moment(forces, self.body_config.center_of_mass)?;
                self.body.force = add(self.body.force, transform(b, force));
                self.body.local_torque = add(self.body.local_torque, moment);
                self.body.force = add(
                    self.body.force,
                    grip::material_drag(self.body.linear_velocity, speed, m.drag)?,
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
                        grounded: rider_grounded,
                        throttle_pressed: throttle > 0.,
                        brake_pressed: brake > 0.,
                        neutral: engine.effective_direction == DriveDirection::Neutral,
                        half_air_control: self.stunt_state_four,
                        lateral_axis: raw_axes[0],
                        lean_axis: lean,
                        preload_axis: input.preload_axis.map(|axis| axis.clamp(-1.0, 1.0)),
                        lean_axis_rate: lean_rate,
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
            for (i, d) in drive.iter().enumerate() {
                let wheel_center = add(origin, transform(b, self.wheel_rest[i]));
                let contact = terrain.contact(wheel_center[0], wheel_center[2])?;
                let (normal, ground, available) = if let Some(c) = contact {
                    if c.surface as usize >= materials.len() {
                        return Err(format!("unsupported terrain material {}", c.surface));
                    }
                    self.surface[i] = c.surface as usize;
                    (c.normal, [wheel_center[0], c.height, wheel_center[2]], true)
                } else {
                    (b[1], wheel_center, false)
                };
                let bottom = sub(wheel_center, scale(normal, self.wheel_config[i].radius));
                let point = add(bottom, scale(normal, dot(sub(ground, bottom), normal)));
                let output = self.suspension[i].contact(
                    self.suspension_config,
                    SuspensionContact {
                        available,
                        normal,
                        ground_point: ground,
                        wheel_center,
                        radius: self.wheel_config[i].radius,
                        body_up: b[1],
                        body_right: b[0],
                        point_velocity: self.body.velocity_at(point),
                        stiffness_scale: 1.0,
                        tilt_angle: dot(normal, b[1]).clamp(-1., 1.).acos(),
                    },
                )?;
                if output.grounded {
                    self.body.apply_world_force(output.force, output.point);
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
            let mut corrections = ContactCorrections::default();
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
            for c in &obstacle_contacts {
                let vertex = add(origin, transform(b, c.local_end));
                let penetration = dot(sub(c.world_point, vertex), c.world_normal);
                if penetration > 0.0 {
                    self.obstacle_contact_count += 1;
                    contact::resolve_static(
                        &mut self.body,
                        &self.body_config,
                        self.chassis_config,
                        StaticContact {
                            point: add(vertex, scale(c.world_normal, penetration)),
                            normal: c.world_normal,
                            penetration,
                            material_grip: materials[0].grip,
                        },
                        &mut corrections,
                    )?;
                }
            }
            self.body.step(&self.body_config, step)?;
            for (value, next) in impact.iter_mut().zip(corrections.velocity_delta) {
                if next.abs() > value.abs() {
                    *value = next;
                }
            }
            self.body.center = add(self.body.center, corrections.translation);
            elapsed += step;
        }
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
        if !crashed {
            if let Some(config) = &self.wreck_physics {
                let angles = rider_control::body_angles(self.body.orientation, f32::atan2);
                if let Some(kind) = config.rules.evaluate(crate::wreck::Input {
                    local_impact: inverse_transform(self.body.basis(), impact),
                    vertical_limit_scale: 1.0,
                    pitch: angles[0],
                    roll: angles[2],
                    wheel_contact: self.contacts.iter().any(|c| c.grounded),
                    body_contact: self.chassis_contact_count + self.obstacle_contact_count > 0,
                    rider_contact: false,
                    forward_speed: inverse_transform(self.body.basis(), self.body.linear_velocity)
                        [2],
                    stunt_landing_safe: self.stunt_landing_safe,
                }) {
                    self.begin_wreck(kind);
                }
            }
            // Recovered checkpoint gates: all wheels down and body-up >0.8.
            // The original water/category gates are still separate. Preserve
            // the eight horizontal clearance rays against supported obstacles.
            if self.wreck.phase == crate::wreck::Phase::Riding
                && self.contacts.iter().all(|c| c.grounded)
                && self.body.basis()[1][1] >= 0.8
                && self.obstacle_contact_count == 0
            {
                let origin = self.body.origin(&self.body_config);
                let forward = self.body.basis()[2];
                let yaw = forward[0].atan2(forward[2]);
                let (sin, cos) = yaw.sin_cos();
                let world = [
                    cos, 0., -sin, 0., 0., 1., 0., 0., sin, 0., cos, 0., origin[0], origin[1],
                    origin[2], 1.,
                ];
                let clear = if let Some(config) = &self.wreck_physics {
                    crate::obstacle::query_static_scene(
                        &config.checkpoint_rays,
                        &world,
                        &self.obstacles,
                    )?
                    .is_empty()
                } else {
                    true
                };
                if clear {
                    self.reset_origin = origin;
                    self.reset_yaw = yaw;
                }
            }
        }
        Ok(self.frame())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn enable_test_wrecks(vehicle: &mut Vehicle) {
        vehicle.wreck_physics = Some(WreckPhysics {
            checkpoint_rays: crate::wreck::checkpoint_clearance_rays(),
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
        let mut recovered = false;
        for _ in 0..120 {
            if vehicle.advance_wreck(1. / 60., true).unwrap() {
                recovered = true;
                break;
            }
        }
        assert!(recovered);
        assert_eq!(vehicle.frame().reset_count, 1);
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
    fn chassis_contacts_hold_body_when_wheels_cannot_reach() {
        let (mut vehicle, terrain) = fixture();
        vehicle
            .set_chassis_points(&[[0., -2., 0.], [0., -2., 0.]])
            .unwrap();
        assert_eq!(vehicle.chassis_points.len(), 1);
        for _ in 0..180 {
            vehicle
                .step(&terrain, VehicleInput::default(), 1. / 60.)
                .unwrap();
        }
        let frame = vehicle.frame();
        assert!(frame.origin[1] > 1.8 && frame.origin[1] < 2.1, "{frame:?}");
        assert!(frame.grounded.iter().all(|x| !x));
        assert_eq!(frame.chassis_contact_count, 1);
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
}
