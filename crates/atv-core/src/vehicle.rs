//! Integration of recovered drivetrain, wheel, terrain and rigid-body components.
//! This is an incomplete reconstruction, not the complete original vehicle:
//! obstacle impulses, rider gameplay gates, wreck transitions, moving surfaces,
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
    pub reverse: bool,
}
#[derive(Debug, Serialize)]
pub struct VehicleFrame {
    pub origin: Vec3,
    pub orientation: [f32; 4],
    pub speed: f32,
    pub speed_magnitude: f32,
    pub rpm: f32,
    pub gear: usize,
    pub wheel_angles: [f32; 4],
    pub wheel_compression: [f32; 4],
    pub grounded: [bool; 4],
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
    base_center: Vec3,
    center_range: Vec3,
    rider_stand_up_speed: f32,
    chassis_points: Vec<Vec3>,
    chassis_config: ContactConfig,
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
            base_center: center,
            center_range: [
                data.first_scalar("Physical Properties", "CofGRangeX")?,
                data.first_scalar("Physical Properties", "CofGRangeY")?,
                data.first_scalar("Physical Properties", "CofGRangeZ")?,
            ],
            //002d5dd0 constructor default, also authored by both supplied riders.
            rider_stand_up_speed: f32::from_bits(0x41200000),
            chassis_points: Vec::new(),
            chassis_config: ContactConfig::from_physics(data)?,
        })
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
    pub fn set_materials(&mut self, materials: [TerrainMaterial; 8]) {
        self.materials = Some(materials);
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
            gear: self.engine.state.gear,
            wheel_angles: self.wheels.map(|w| w.angle),
            wheel_compression: self.suspension.map(|s| s.compression),
            grounded: self.contacts.map(|c| c.grounded),
        }
    }
    pub fn step(
        &mut self,
        terrain: &Terrain,
        input: VehicleInput,
        dt: f32,
    ) -> Result<VehicleFrame> {
        if ![dt, input.throttle, input.brake, input.steer, input.lean]
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
        let throttle = input.throttle.clamp(0., 1.);
        let brake = input.brake.clamp(0., 1.);
        let speed = inverse_transform(self.body.basis(), self.body.linear_velocity)[2];
        let raw_axes = rider_control::smooth_axes(
            self.axis_config,
            &mut self.smoothed_axes,
            [input.steer.clamp(-1., 1.), input.lean.clamp(-1., 1.)],
            self.contacts.iter().any(|c| c.grounded),
            false,
            dt,
        )?;
        let center = rider_control::shifted_center(
            self.base_center,
            self.center_range,
            self.smoothed_axes,
            speed,
            self.rider_stand_up_speed,
        );
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
        let steering = ((self.smoothed_axes[0] * 30.0) * f32::from_bits(0x3c8efa34)) * steer_scale;
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
        let lean_rate = self.lean_gesture.sample(lean, self.clock_ms)?;
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
        let mut elapsed = 0.0;
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
            rider_control::step(
                self.rider_config,
                &mut self.rider_state,
                RiderInput {
                    grounded: rider_grounded,
                    throttle_pressed: throttle > 0.,
                    brake_pressed: brake > 0.,
                    neutral: engine.effective_direction == DriveDirection::Neutral,
                    // Gameplay rider state4 gate is not yet reconstructed.
                    half_air_control: false,
                    lateral_axis: raw_axes[0],
                    lean_axis: lean,
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
            for &(local, ground, normal, grip) in &chassis_planes {
                let vertex = add(origin, transform(b, local));
                let penetration = dot(sub(ground, vertex), normal);
                if penetration > 0.0 {
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
            self.body.step(&self.body_config, step)?;
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
        Ok(self.frame())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
