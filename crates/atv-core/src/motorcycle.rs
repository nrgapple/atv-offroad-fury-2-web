//! Playable two-wheel reconstruction using SLUS_211.04 resource identities and
//! tuning. The integrator/controller below is authored: native whole-step parity,
//! collision-v2 dispatch, stunt timing and tire curves are not established yet.
use crate::{
    Result, animation, dynamics,
    mesh::{self, Mesh},
    physics_data::PhysicsData,
    resource::Resource,
    terrain::Terrain,
    vehicle::VehicleInput,
    wheel::{add, dot, scale, sub},
};
use serde::Serialize;
use std::collections::BTreeMap;
#[path = "motorcycle_pose.rs"]
mod pose;
type V = [f32; 3];
type M = [f32; 16];
fn mul(a: &M, b: &M) -> M {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn rotation(axis: usize, angle: f32) -> M {
    let q = {
        let mut q = [(angle * 0.5).cos(), 0., 0., 0.];
        q[axis + 1] = (angle * 0.5).sin();
        q
    };
    let b = dynamics::basis(q);
    [
        b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1], b[2][2],
        0., 0., 0., 0., 1.,
    ]
}
pub fn cleaned(data: &[u8]) -> Result<PhysicsData> {
    if data.len() > 1024 * 1024 {
        return Err("MX configuration exceeds limit".into());
    }
    let bytes: Vec<u8> = data
        .split(|b| *b == b'\n')
        .flat_map(|line| {
            line.split(|b| *b == b';')
                .next()
                .unwrap_or_default()
                .iter()
                .copied()
                .chain(*b"\n")
        })
        .collect();
    let text = std::str::from_utf8(&bytes).map_err(|_| "invalid MX configuration encoding")?;
    let text: String = text
        .lines()
        .map(|line| format!("{}\n", line.split(';').next().unwrap_or("").trim()))
        .collect();
    PhysicsData::parse(text.as_bytes())
}
pub fn member(resource: &Resource<'_>, leaf: &str) -> Result<Vec<u8>> {
    let indices: Vec<_> = resource
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.name
                .rsplit('/')
                .next()
                .is_some_and(|n| n.eq_ignore_ascii_case(leaf))
        })
        .map(|(i, _)| i)
        .collect();
    if indices.len() != 1 {
        return Err(format!("missing or ambiguous motorcycle resource {leaf}"));
    }
    resource.extract(indices[0])
}
pub const SELECTION_COUNT: u16 = 46;
pub fn definition(id: u16) -> Result<(u16, bool, &'static str, &'static str)> {
    let entry = match id {
        27 | 28 => (50, "MXF_S_50_Fast50_01.sub", "Fast50"),
        29 | 30 => (125, "MX_S_125_RM1251.sub", "Suzuki RM125"),
        31 | 32 => (250, "MX_S_250_RM2501.sub", "Suzuki RM250"),
        33 | 34 => (500, "MX_S_500_RM500XGX1.sub", "RM500 XGX"),
        35 => (125, "MX_S_125_CR125RTHQ1.sub", "Honda CR125 THQ"),
        36 => (125, "MX_S_125_YZ1251.sub", "Yamaha YZ125"),
        37 => (125, "MX_S_125_KX1251.sub", "Kawasaki KX125"),
        38 => (125, "MX_S_125_125SX1.sub", "KTM 125 SX"),
        39 => (250, "MX_S_250_CR250RTHQ1.sub", "Honda CR250 THQ"),
        40 => (250, "MX_S_250_YZ2501.sub", "Yamaha YZ250"),
        41 => (250, "MX_S_250_KX2501.sub", "Kawasaki KX250"),
        42 => (250, "MX_S_250_250SX1.sub", "KTM 250 SX"),
        43 => (500, "MX_S_500_SX500XGX1.sub", "SX500 XGX"),
        44 => (500, "MX_S_500_YZF500XGX1.sub", "YZF500 XGX"),
        45 => (50, "MXF_S_50_Sano_01.sub", "Sano 50"),
        46 => (50, "MXF_S_50_RedB_01.sub", "Red Baron 50"),
        _ => return Err("unsupported motorcycle selection".into()),
    };
    Ok((entry.0, id <= 34 && id.is_multiple_of(2), entry.1, entry.2))
}
pub fn class(id: u16) -> Result<(u16, bool)> {
    definition(id).map(|(size, pro, _, _)| (size, pro))
}
fn remap(mesh: &mut Mesh, config: &PhysicsData) {
    for material in &mut mesh.materials {
        if let Some(entry) = config
            .entries
            .iter()
            .find(|e| e.section == "Material Changes" && e.key.eq_ignore_ascii_case(material))
        {
            *material = entry.value.trim_matches('"').into();
        }
    }
}
/// Preserve source part nodes beneath each SUB placement bone.
fn assembly(resource: &Resource<'_>, subfile: &str, base: &str) -> Result<Mesh> {
    let config = cleaned(&member(resource, subfile)?)?;
    let mut result = mesh::decode_psg(&member(resource, base)?)?;
    remap(&mut result, &config);
    let count = config
        .first_scalar("Objects", "NumberOfSubObjects")
        .unwrap_or(0.);
    if !(0. ..=16.).contains(&count) || count.fract() != 0. {
        return Err("invalid motorcycle assembly size".into());
    }
    for i in 1..=count as usize {
        let section = format!("SubObject{i}");
        let stem = &config.first(&section, "SLT")?.value;
        let placement = &config.first(&section, "Placement")?.value;
        let target = result
            .nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(placement))
            .ok_or("unknown motorcycle attachment")?;
        let mut part = mesh::decode_psg(&member(resource, &format!("{stem}.psg"))?)?;
        remap(&mut part, &config);
        let offset = result.materials.len() as u32;
        let node_offset = result.nodes.len();
        if node_offset + part.nodes.len() > 32 {
            return Err("motorcycle assembly exceeds pose capacity".into());
        }
        for mut node in part.nodes {
            node.parent = if node.parent < 0 {
                target as i32
            } else {
                node.parent + node_offset as i32
            };
            node.world_matrix = mul(&result.nodes[target].world_matrix, &node.world_matrix);
            result.nodes.push(node);
        }
        result.materials.extend(part.materials);
        for mut group in part.lods.remove(0).groups {
            group.material += offset;
            for vertex in &mut group.vertices {
                vertex.node += node_offset as u16;
                let node = &result.nodes[vertex.node as usize];
                vertex.position =
                    crate::obstacle::transform_point(&node.world_matrix, vertex.local_position);
                vertex.normal =
                    dynamics::transform(matrix_basis(&node.world_matrix), vertex.local_normal);
            }
            result.lods[0].groups.push(group);
        }
    }
    // All attached parts currently expose LOD0; don't select incomplete base-only LODs.
    result.lods.truncate(1);
    Ok(result)
}
fn matrix_basis(m: &M) -> dynamics::Basis {
    [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]]
}
#[derive(Clone, Serialize)]
pub struct StuntDefinition {
    pub name: String,
    pub display: String,
    pub mapping: String,
    pub hold: f32,
    pub safe: f32,
    pub duration: f32,
    pub points: f32,
}
pub struct Model {
    pub mesh: Mesh,
    pub rider: Mesh,
    pub tuning: PhysicsData,
    pub motions: BTreeMap<String, animation::Animation>,
    pub stunts: Vec<StuntDefinition>,
    pub effects_config: crate::motorcycle_effects::Config,
    pub audio_config: crate::engine_audio::EngineAudioConfig,
    wheels: [usize; 2],
    wheel_rest: [V; 2],
    pub size: u16,
    pub pro: bool,
    steering_axis: V,
    grips: [usize; 2],
}
impl Model {
    pub fn load(bytes: &[u8], id: u16) -> Result<Self> {
        let (size, pro, definition, _) = definition(id)?;
        let archive = Resource::parse(bytes)?;
        let mesh = assembly(
            &archive,
            definition,
            if size == 50 { "bike50.psg" } else { "bike.psg" },
        )?;
        let rider = assembly(&archive, "THQ1.sub", "mrider.psg")?;
        if mesh.nodes.len() > 32 || rider.nodes.len() > 32 {
            return Err("motorcycle exceeds pose capacity".into());
        }
        let wheels = ["TireFront", "TireRear"].map(|name| {
            mesh.nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
                .ok_or("missing motorcycle wheel")
        });
        let wheels = [wheels[0]?, wheels[1]?];
        let wheel_rest = wheels.map(|i| mesh.nodes[i].world_matrix[12..15].try_into().unwrap());
        let bar = mesh
            .nodes
            .iter()
            .find(|n| n.name == "Handlebars")
            .ok_or("missing handlebars")?;
        let axis = sub(bar.world_matrix[12..15].try_into().unwrap(), wheel_rest[0]);
        let length = dot(axis, axis).sqrt();
        if length <= 0. {
            return Err("invalid motorcycle fork axis".into());
        }
        let steering_axis = scale(axis, 1. / length);
        let grips = ["Dummy_Hand_L", "Dummy_Hand_R"].map(|name| {
            mesh.nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
                .ok_or("missing motorcycle grip")
        });
        let grips = [grips[0]?, grips[1]?];
        let tuning = cleaned(&member(
            &archive,
            &format!("mx_{}{size}P.phy", if pro { "Pro" } else { "Race" }),
        )?)?;
        let mut motions = BTreeMap::new();
        for (i, m) in archive.members.iter().enumerate() {
            if m.name.to_ascii_lowercase().ends_with(".mot") {
                let name = m
                    .name
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .trim_end_matches(".MOT")
                    .trim_end_matches(".mot")
                    .to_ascii_lowercase();
                motions.insert(name, animation::decode_mot(&archive.extract(i)?)?);
            }
        }
        // Mini bikes have their own source riding/stunt bank and root offsets.
        let bank = if size == 50 { "MXF" } else { "MX" };
        for suffix in ["RiderMotions.mcf", "RiderStunts.mcf"] {
            let catalog = cleaned(&member(&archive, &format!("{bank}_{suffix}"))?)?;
            for entry in catalog
                .entries
                .iter()
                .filter(|e| e.key.eq_ignore_ascii_case("VUE"))
            {
                let file = entry.value.trim_matches('"');
                if let Ok(bytes) = member(&archive, file) {
                    let name = catalog
                        .first(&entry.section, "NAME")?
                        .value
                        .to_ascii_lowercase()
                        .replace("mxf_", "mx_")
                        .replace("mx_rider_", "mx_");
                    motions.insert(name.clone(), animation::decode_mot(&bytes)?);
                    if name == "mx_wreck_air_flail" {
                        motions.insert("wreck_air_flail".into(), animation::decode_mot(&bytes)?);
                    }
                }
            }
        }
        let config = cleaned(&member(&archive, &format!("{bank}_Rider_Stunts.ini"))?)?;
        let mut stunts = Vec::new();
        for i in 1..=46 {
            let section = format!("Stunt{i:02}");
            let text = |key| {
                config
                    .first(&section, key)
                    .map(|e| e.value.trim_matches('"').to_string())
            };
            stunts.push(StuntDefinition {
                name: text("Name")?,
                display: text("DisplayName")?,
                mapping: text("DefaultKeyMapping").unwrap_or_default(),
                hold: config.first_scalar(&section, "HoldTime")?,
                safe: config.first_scalar(&section, "SafeTime")?,
                duration: config.first_scalar(&section, "Duration")?,
                points: config.first_scalar(&section, "PointValue").unwrap_or(0.),
            });
        }
        let effects_config = crate::motorcycle_effects::Config::load(&archive)?;
        let audio_config = crate::engine_audio::EngineAudioConfig::parse_mx(&member(
            &archive,
            &format!("vehicle_{size}A.aud"),
        )?)?;
        Ok(Self {
            mesh,
            rider,
            tuning,
            motions,
            stunts,
            effects_config,
            audio_config,
            wheels,
            wheel_rest,
            size,
            pro,
            steering_axis,
            grips,
        })
    }
    pub fn pose(&self, steering: f32, angles: [f32; 2], compression: [f32; 2]) -> Vec<M> {
        let mut world = Vec::with_capacity(self.mesh.nodes.len());
        for (i, n) in self.mesh.nodes.iter().enumerate() {
            let mut local = n.local_matrix;
            if n.name.eq_ignore_ascii_case("Handlebars") {
                local = mul(&local, &pose::axis_rotation(self.steering_axis, steering));
            }
            if n.name.eq_ignore_ascii_case("Shocks") {
                for j in 0..3 {
                    local[12 + j] += self.steering_axis[j] * compression[0] / self.steering_axis[1];
                }
            }
            if n.name.eq_ignore_ascii_case("SwingArm") {
                local = mul(&local, &rotation(0, compression[1] / 2.));
            }
            for (wheel, &index) in self.wheels.iter().enumerate() {
                if i == index {
                    local = mul(&local, &rotation(0, angles[wheel]));
                }
            }
            world.push(if n.parent >= 0 {
                mul(&world[n.parent as usize], &local)
            } else {
                local
            });
        }
        world
    }
    pub fn rider_pose(&self, name: &str, time: f32) -> Result<Vec<M>> {
        let motion = self
            .motions
            .get(name)
            .ok_or_else(|| format!("missing motorcycle motion {name}"))?;
        Ok(motion
            .sample_pose(
                &self.rider.nodes,
                time.clamp(0., motion.duration),
                animation::Sampling::default(),
            )?
            .world_matrices)
    }
}
#[derive(Clone, Copy, Debug)]
struct Tire {
    radius: f32,
    travel: f32,
    spring: f32,
    bounce: f32,
    rebound: f32,
    drive: [f32; 16],
    side: [f32; 16],
    brake: [f32; 16],
    mud_time: [f32; 16],
    mud_amount: [f32; 16],
}
impl Tire {
    fn parse(data: &PhysicsData, section: &str, mass: f32) -> Result<Self> {
        let v = |key| data.first_scalar(section, key);
        let spring = v("Spring")? * mass * 32.2;
        let array = |key| -> Result<[f32; 16]> {
            let values = (1..=16)
                .map(|i| data.first_scalar(section, &format!("{key}{i}")))
                .collect::<Result<Vec<_>>>()?;
            Ok(values.try_into().unwrap())
        };
        let result = Self {
            radius: v("TireRadius")?,
            travel: v("StrutCompression")?,
            spring,
            bounce: v("BounceDampingRatio")? * 2. * (mass * spring).sqrt(),
            rebound: v("ReboundDampingRatio")? * 2. * (mass * spring).sqrt(),
            drive: array("MaxDriveForce")?,
            side: array("MaxSideForce")?,
            brake: array("MaxBrakingTorque")?,
            mud_time: std::array::from_fn(|i| {
                data.first_scalar(section, &format!("MudTime{}", i + 1))
                    .unwrap_or(0.)
            }),
            mud_amount: std::array::from_fn(|i| {
                data.first_scalar(section, &format!("MudAmount{}", i + 1))
                    .unwrap_or(0.)
            }),
        };
        if result.radius <= 0. || result.travel <= 0. || result.spring <= 0. {
            return Err("invalid motorcycle tire".into());
        }
        Ok(result)
    }
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct Frame {
    pub origin: V,
    pub orientation: [f32; 4],
    pub linear_velocity: V,
    pub local_angular_velocity: V,
    pub speed: f32,
    pub speed_magnitude: f32,
    pub rpm: f32,
    pub gear: usize,
    pub grounded: [bool; 2],
    pub wheel_angles: [f32; 2],
    pub wheel_compression: [f32; 2],
    pub steering_angle: f32,
    pub visual_steering_angle: f32,
    pub crash_impact_speed: f32,
    pub wreck: crate::wreck::State,
    pub reset_count: u32,
    pub preload_available: bool,
    pub preload_armed: bool,
    pub preload_active: bool,
    pub preload_strength: f32,
    pub clutch_active: bool,
    pub clutch_boost: f32,
    pub wheelie_active: bool,
    pub stoppie_active: bool,
    pub crossup: f32,
    pub mud_amount: [f32; 2],
    pub surfaces: [usize; 2],
    pub vehicle_kind: &'static str,
    pub reconstruction: &'static str,
}
#[derive(Serialize)]
pub struct Camera {
    pub position: V,
    pub target: V,
    pub fov: f32,
    pub preset: usize,
}
pub struct Motorcycle {
    pub model: Model,
    body: dynamics::BodyState,
    body_config: dynamics::BodyConfig,
    tires: [Tire; 2],
    start: V,
    start_yaw: f32,
    safe_origin: V,
    safe_yaw: f32,
    rider_body: Option<(M, V)>,
    pose_controls: pose::Controls,
    normal_pose: Option<animation::Pose>,
    visual_steer: f32,
    camera_origin: V,
    camera_yaw: f32,
    frame: Frame,
    gear: usize,
    rpm: f32,
    steer: f32,
    compression: [f32; 2],
    angles: [f32; 2],
    preload: f32,
    previous_preload: f32,
    mud_timer: [f32; 2],
    air_time: f32,
    wreck_time: f32,
    reset_count: u32,
    pub tick: u64,
    pub time: f32,
    obstacles: Vec<crate::obstacle::StaticObstacle>,
    pub last_input: VehicleInput,
    pub motion_name: String,
    pub motion_time: f32,
    pub stunt_name: Option<String>,
    pub stunt_timer: f32,
    spin_command: [f32; 2],
    spin_time: f32,
    clutch_down: bool,
    boost_time: f32,
    camera_preset: usize,
    active_stunt: Option<usize>,
    stunt_hold: bool,
}
impl Motorcycle {
    pub fn new(model: Model, origin: V, yaw: f32) -> Result<Self> {
        if !yaw.is_finite() || origin.iter().any(|x| !x.is_finite()) {
            return Err("invalid motorcycle spawn".into());
        }
        let mass = model.tuning.first_scalar("Mass Properties", "Weight")? / 32.2;
        let mut config = dynamics::BodyConfig::new(
            mass,
            model.tuning.vector3("Mass Properties", "Inertia")?,
            model.tuning.vector3("Mass Properties", "CofG")?,
        )?;
        config.angular_min = [-8.; 3];
        config.angular_max = [8.; 3];
        let body = dynamics::BodyState::new(
            origin,
            [(yaw / 2.).cos(), 0., (yaw / 2.).sin(), 0.],
            &config,
        )?;
        let tires = [
            Tire::parse(&model.tuning, "Front Tire", mass)?,
            Tire::parse(&model.tuning, "Rear Tire", mass)?,
        ];
        let rpm = model.tuning.first_scalar("Engine", "MinRPM")?;
        let mut result = Self {
            model,
            body,
            body_config: config,
            tires,
            start: origin,
            start_yaw: yaw,
            safe_origin: origin,
            safe_yaw: yaw,
            rider_body: None,
            pose_controls: pose::Controls::default(),
            normal_pose: None,
            visual_steer: 0.,
            camera_origin: origin,
            camera_yaw: yaw,
            frame: Frame::default(),
            gear: 1,
            rpm,
            steer: 0.,
            compression: [0.; 2],
            angles: [0.; 2],
            preload: 0.,
            previous_preload: 0.,
            mud_timer: [0.; 2],
            air_time: 0.,
            wreck_time: 0.,
            reset_count: 0,
            tick: 0,
            time: 0.,
            obstacles: Vec::new(),
            last_input: VehicleInput::default(),
            motion_name: "mx_turn_still_mid".into(),
            motion_time: 0.,
            stunt_name: None,
            stunt_timer: 0.,
            spin_command: [0.; 2],
            spin_time: 0.,
            clutch_down: false,
            boost_time: 0.,
            camera_preset: 0,
            active_stunt: None,
            stunt_hold: false,
        };
        result.refresh();
        Ok(result)
    }
    fn value(&self, section: &str, key: &str) -> f32 {
        self.model.tuning.first_scalar(section, key).unwrap_or(0.)
    }
    pub fn reset(&mut self) -> Result<()> {
        self.body = dynamics::BodyState::new(
            self.start,
            [
                (self.start_yaw / 2.).cos(),
                0.,
                (self.start_yaw / 2.).sin(),
                0.,
            ],
            &self.body_config,
        )?;
        self.compression = [0.; 2];
        self.angles = [0.; 2];
        self.preload = 0.;
        self.previous_preload = 0.;
        self.mud_timer = [0.; 2];
        self.spin_command = [0.; 2];
        self.spin_time = 0.;
        self.clutch_down = false;
        self.boost_time = 0.;
        self.air_time = 0.;
        self.wreck_time = 0.;
        self.reset_count = self.reset_count.wrapping_add(1);
        self.rider_body = None;
        self.pose_controls = pose::Controls::default();
        self.normal_pose = None;
        self.visual_steer = 0.;
        self.camera_origin = self.start;
        self.camera_yaw = self.start_yaw;
        self.frame = Frame::default();
        self.last_input = VehicleInput::default();
        self.gear = 1;
        self.rpm = self.value("Engine", "MinRPM");
        self.stunt_name = None;
        self.active_stunt = None;
        self.stunt_hold = false;
        self.refresh();
        Ok(())
    }
    fn recover(&mut self) -> Result<()> {
        let (origin, yaw) = (self.safe_origin, self.safe_yaw);
        self.reset()?;
        self.body = dynamics::BodyState::new(
            add(origin, [0., 0.4, 0.]),
            [(yaw / 2.).cos(), 0., (yaw / 2.).sin(), 0.],
            &self.body_config,
        )?;
        self.refresh();
        self.camera_origin = self.frame.origin;
        self.camera_yaw = yaw;
        Ok(())
    }
    pub fn rider_world(&self) -> M {
        self.rider_body
            .as_ref()
            .map_or_else(|| self.world(), |b| b.0)
    }
    pub fn set_obstacles(&mut self, obstacles: Vec<crate::obstacle::StaticObstacle>) {
        self.obstacles = obstacles;
    }
    pub fn frame(&self) -> &Frame {
        &self.frame
    }
    fn refresh(&mut self) {
        self.frame.origin = self.body.origin(&self.body_config);
        self.frame.orientation = self.body.orientation;
        self.frame.linear_velocity = self.body.linear_velocity;
        self.frame.local_angular_velocity = self.body.local_angular_velocity;
        self.frame.speed =
            dynamics::inverse_transform(self.body.basis(), self.body.linear_velocity)[2];
        self.frame.speed_magnitude =
            dot(self.body.linear_velocity, self.body.linear_velocity).sqrt();
        self.frame.rpm = self.rpm;
        self.frame.clutch_active = self.clutch_down;
        self.frame.clutch_boost = self.boost_time;
        self.frame.gear = self.gear;
        self.frame.wheel_angles = self.angles;
        self.frame.wheel_compression = self.compression;
        self.frame.steering_angle = self.steer;
        self.frame.visual_steering_angle = self.visual_steer;
        self.frame.reset_count = self.reset_count;
        self.frame.vehicle_kind = "motorcycle";
        self.frame.reconstruction =
            "authored two-wheel controller with source tuning; native step parity pending";
    }
    pub fn step(&mut self, terrain: &Terrain, input: VehicleInput, dt: f32) -> Result<()> {
        if !dt.is_finite()
            || !(0. ..=0.1).contains(&dt)
            || [input.throttle, input.brake, input.steer, input.lean]
                .iter()
                .any(|x| !x.is_finite())
        {
            return Err("invalid motorcycle step".into());
        }
        let input = VehicleInput {
            throttle: input.throttle.clamp(0., 1.),
            brake: input.brake.clamp(0., 1.),
            steer: input.steer.clamp(-1., 1.),
            lean: input.lean.clamp(-1., 1.),
            ..input
        };
        self.last_input = input;
        self.frame.crash_impact_speed = 0.;
        let n = (dt * 120.).ceil().max(1.) as usize;
        for _ in 0..n {
            self.substep(terrain, input, dt / n as f32)?;
        }
        if self.frame.wreck.phase != crate::wreck::Phase::Riding {
            if self.rider_body.is_none() {
                let mut world = self.world();
                let b = self.body.basis();
                let yaw = b[2][0].atan2(b[2][2]);
                let r = rotation(1, yaw);
                world[..12].copy_from_slice(&r[..12]);
                self.rider_body = Some((world, add(self.body.linear_velocity, [0., 8., 0.])));
            }
            if let Some((world, velocity)) = &mut self.rider_body {
                velocity[1] -= 32.2 * dt;
                for i in 0..3 {
                    world[12 + i] += velocity[i] * dt;
                }
                if let Some(hit) = terrain.contact(world[12], world[14])?
                    && world[13] < hit.height
                {
                    world[13] = hit.height;
                    velocity[1] = 0.;
                    velocity[0] *= 0.85;
                    velocity[2] *= 0.85;
                }
            }
        } else {
            self.rider_body = None;
        }
        self.pose_controls.step(
            input,
            self.frame.speed.abs(),
            self.frame.grounded.iter().any(|v| *v),
            dt,
        );
        let (normal, visual) =
            self.model
                .normal_pose(self.pose_controls, self.angles, self.compression)?;
        self.normal_pose = Some(normal);
        self.visual_steer = visual;
        let origin = self.body.origin(&self.body_config);
        for (i, value) in origin.iter().enumerate() {
            self.camera_origin[i] += (value - self.camera_origin[i]) * (1. - (-10. * dt).exp());
        }
        let b = self.body.basis();
        let v = self.body.linear_velocity;
        let yaw = if !self.frame.grounded.iter().any(|v| *v) && v[0] * v[0] + v[2] * v[2] > 9. {
            v[0].atan2(v[2])
        } else {
            b[2][0].atan2(b[2][2])
        };
        let difference = (yaw - self.camera_yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.camera_yaw += difference * (1. - (-6. * dt).exp());
        self.tick += 1;
        self.time += dt;
        self.refresh();
        let grounded = self.frame.grounded.iter().any(|x| *x);
        let suffix = if input.lean > 0.3 {
            "bck"
        } else if input.lean < -0.3 {
            "fwd"
        } else {
            "mid"
        };
        self.motion_name = if self.frame.wreck.phase != crate::wreck::Phase::Riding {
            "wreck_air_flail".into()
        } else if !grounded {
            format!("mx_air_{suffix}")
        } else if self.frame.speed.abs() < 2. {
            format!("mx_turn_still_{suffix}")
        } else {
            format!("mx_turn_riding_{suffix}")
        };
        if let Some(motion) = self.model.motions.get(&self.motion_name) {
            self.motion_time = if grounded {
                motion.duration * ((input.steer + 1.) * 0.5)
            } else {
                motion.duration * ((self.pose_controls.steer + 1.) * 0.5)
            };
        }
        Ok(())
    }
    fn substep(&mut self, terrain: &Terrain, input: VehicleInput, dt: f32) -> Result<()> {
        self.boost_time = (self.boost_time - dt).max(0.);
        let origin = self.body.origin(&self.body_config);
        let basis = self.body.basis();
        let local_velocity = dynamics::inverse_transform(basis, self.body.linear_velocity);
        let speed = local_velocity[2];
        let mass = self.body_config.mass;
        let weight = mass * 32.2;
        self.body.force = [
            0.,
            -weight * self.value("Mass Properties", "GravityScale"),
            0.,
        ];
        self.body.local_torque = [0.; 3];
        let wreck = self.frame.wreck.phase != crate::wreck::Phase::Riding;
        let command = input.steer
            * self.value("Front Tire", "MaxSteerAngle").to_radians()
            * (1. - 0.7 * (speed.abs() / 110.).min(1.));
        self.steer += (command - self.steer) * (dt * 12.).min(1.);
        let mut contacts = 0;
        let mut slope = 0.;
        for i in 0..2 {
            let center = add(origin, dynamics::transform(basis, self.model.wheel_rest[i]));
            let tire = self.tires[i];
            self.frame.grounded[i] = false;
            let mut contact = terrain.contact(center[0], center[2])?;
            let ray = crate::collision::CollisionRay {
                start: add(center, [0., tire.travel + tire.radius, 0.]),
                end: add(center, [0., -tire.radius - 0.5, 0.]),
            };
            for hit in crate::wreck::query_static_segments(&[ray], &self.obstacles, 16)? {
                if hit.world_normal[1] > 0.2
                    && contact
                        .as_ref()
                        .is_none_or(|h| hit.world_point[1] > h.height)
                {
                    contact = Some(crate::terrain::TerrainContact {
                        height: hit.world_point[1],
                        normal: hit.world_normal,
                        surface: 0,
                        cell: [0; 2],
                        triangle: [0, 1, 2],
                    });
                }
            }
            if let Some(hit) = contact {
                self.frame.surfaces[i] = (hit.surface as usize).min(15);
                let depth = hit.height + tire.radius - center[1];
                let compression = depth.clamp(0., tire.travel);
                let rate = if compression > self.compression[i] {
                    self.value(
                        if i == 0 { "Front Tire" } else { "Rear Tire" },
                        "MaxCompressionRate",
                    )
                } else {
                    self.value(
                        if i == 0 { "Front Tire" } else { "Rear Tire" },
                        "MaxDecompressionRate",
                    )
                };
                self.compression[i] +=
                    (compression - self.compression[i]).clamp(-rate * dt, rate * dt);
                if depth > 0. {
                    self.frame.grounded[i] = true;
                    contacts += 1;
                    let velocity = dot(self.body.velocity_at(center), hit.normal);
                    let damping = if velocity < 0. {
                        tire.bounce
                    } else {
                        tire.rebound
                    };
                    let force =
                        (tire.spring * compression - damping * velocity).clamp(0., weight * 8.);
                    self.body
                        .apply_world_force(scale(hit.normal, force), center);
                    if depth > tire.travel {
                        self.frame.crash_impact_speed =
                            self.frame.crash_impact_speed.max((-velocity).max(0.));
                        let correction = (depth - tire.travel).min(0.3);
                        self.body.center = add(self.body.center, scale(hit.normal, correction));
                        if velocity < 0. {
                            self.body.linear_velocity =
                                sub(self.body.linear_velocity, scale(hit.normal, velocity * 0.7));
                        }
                    }
                    slope += hit.normal[2] * basis[2][2] + hit.normal[0] * basis[2][0];
                    let surface = self.frame.surfaces[i];
                    let forward = if i == 0 {
                        add(
                            scale(basis[2], self.steer.cos()),
                            scale(basis[0], self.steer.sin()),
                        )
                    } else {
                        basis[2]
                    };
                    let lateral = crate::wheel::cross(hit.normal, forward);
                    let lateral_speed = dot(self.body.velocity_at(center), lateral);
                    let side = (-lateral_speed * mass * 8.)
                        .clamp(-force * tire.side[surface], force * tire.side[surface]);
                    self.body.apply_world_force(scale(lateral, side), center);
                    let brake = if wreck { 0.1 } else { input.brake };
                    let braking = (-speed * mass * 10.).clamp(
                        -brake * tire.brake[surface] / tire.radius,
                        brake * tire.brake[surface] / tire.radius,
                    );
                    let force_position = add(
                        self.body.center,
                        dynamics::transform(
                            basis,
                            self.model.tuning.vector3(
                                if i == 0 { "Front Tire" } else { "Rear Tire" },
                                "BodyTireForcePosition",
                            )?,
                        ),
                    );
                    self.body
                        .apply_world_force(scale(forward, braking), force_position);
                    if i == 1 && !wreck {
                        let ratio = self.value("Engine", &format!("GearRatio{}", self.gear))
                            * self.value("Engine", "FinalGearRatio");
                        let min_rpm = self.value("Engine", "MinRPM");
                        let target = (speed.abs() / tire.radius * ratio * 60.
                            / std::f32::consts::TAU)
                            .max(min_rpm + input.throttle * 1200.);
                        let target = if self.clutch_down {
                            min_rpm + input.throttle * 4500.
                        } else {
                            target
                        };
                        self.rpm += (target - self.rpm) * (dt * 10.).min(1.);
                        if self.rpm > self.value("Engine", "UpShiftRPM")
                            && self.gear < (self.value("Engine", "NumberOfGears") as usize)
                        {
                            self.gear += 1;
                        }
                        if self.rpm < self.value("Engine", "DownShiftRPM") && self.gear > 1 {
                            self.gear -= 1;
                        }
                        let hp = self.horsepower(self.rpm);
                        let drive = (hp * 550. / speed.abs().max(15.) * input.throttle)
                            .min(force * tire.drive[surface]);
                        let boost = 1.
                            + (self.value("Engine", "MaxClutchTorqueScale") - 1.)
                                * (self.boost_time
                                    / self.value("Engine", "ClutchTorqueScaleTime").max(0.01))
                                .clamp(0., 1.)
                                * (1.
                                    - speed.abs()
                                        / (self.value("Engine", "MaxClutchTorqueScaleSpeed")
                                            * 1.46666)
                                            .max(1.))
                                .clamp(0., 1.);
                        let drive = if self.clutch_down { 0. } else { drive * boost };
                        let drive = if input.reverse {
                            -self.value("Engine", "ReverseGain")
                                * mass
                                * (speed - self.value("Engine", "ReverseSpeed") * 1.46666)
                        } else {
                            drive
                        };
                        self.body.apply_world_force(
                            scale(forward, drive.clamp(-weight * 2., weight * 2.)),
                            force_position,
                        );
                    }
                    if speed.abs() > 3. && tire.mud_time[surface] > 0. {
                        self.mud_timer[i] += dt;
                        let interval = tire.mud_time[surface].max(0.01);
                        if self.mud_timer[i] >= interval {
                            self.mud_timer[i] -= interval;
                            self.frame.mud_amount[i] =
                                (self.frame.mud_amount[i] + tire.mud_amount[surface]).clamp(0., 1.);
                        }
                    }
                }
            }
            self.angles[i] += speed / tire.radius * dt;
        }
        // Authored swept probes prevent the bike passing through the existing
        // map's solid props. This is separate from unrecovered MX COL-v2 routing.
        let probes: Vec<_> = [
            self.model.wheel_rest[0],
            self.model.wheel_rest[1],
            [0., 2., 0.],
        ]
        .into_iter()
        .map(|p| {
            let start = add(origin, dynamics::transform(basis, p));
            let travel = add(
                scale(self.body.linear_velocity, dt),
                scale(basis[2], speed.signum() * 0.8),
            );
            crate::collision::CollisionRay {
                start,
                end: add(start, travel),
            }
        })
        .collect();
        for hit in crate::wreck::query_static_segments(&probes, &self.obstacles, 8)? {
            let closing = (-dot(self.body.linear_velocity, hit.world_normal)).max(0.);
            if closing > 0. {
                self.frame.crash_impact_speed = self.frame.crash_impact_speed.max(closing);
                self.body.linear_velocity = add(
                    self.body.linear_velocity,
                    scale(hit.world_normal, closing * 1.05),
                );
                self.body.center = add(self.body.center, scale(hit.world_normal, 0.12));
                let local_normal = dynamics::inverse_transform(basis, hit.world_normal);
                let (limit, kind) = if local_normal[2] < -0.3 {
                    ("FrontWreckSpeedLimit", crate::wreck::Kind::Front)
                } else if local_normal[2] > 0.3 {
                    ("BackWreckSpeedLimit", crate::wreck::Kind::Back)
                } else if local_normal[0] > 0. {
                    ("SideWreckSpeedLimit", crate::wreck::Kind::Left)
                } else {
                    ("SideWreckSpeedLimit", crate::wreck::Kind::Right)
                };
                if !wreck && closing > self.value("Wrecks", limit) {
                    self.frame.wreck.begin(kind);
                    self.wreck_time = 0.;
                }
            }
        }
        if self.frame.wreck.phase == crate::wreck::Phase::Riding
            && contacts == 2
            && self.frame.crash_impact_speed == 0.
            && speed.abs() > 3.
        {
            self.safe_origin = self.body.origin(&self.body_config);
            self.safe_yaw = basis[2][0].atan2(basis[2][2]);
        }
        let grounded = contacts > 0;
        if grounded {
            self.frame.crossup = 0.;
        }
        self.frame.preload_available = grounded && !wreck;
        let preload_axis = input.preload_axis.unwrap_or(input.lean).clamp(-1., 1.);
        self.frame.preload_active = false;
        if grounded && !wreck {
            if preload_axis > 0.5 {
                self.preload =
                    (self.preload + dt / self.value("Preload", "PreloadTime").max(0.01)).min(1.);
            }
            if preload_axis < -0.5 && self.previous_preload >= -0.5 && self.preload > 0. {
                self.body.linear_velocity[1] +=
                    self.value("Preload", "MaxPreload") * self.preload / mass * 0.22;
                self.preload = 0.;
                self.frame.preload_active = true;
            }
        }
        self.previous_preload = preload_axis;
        self.frame.preload_strength = self.preload;
        self.frame.preload_armed = self.preload > 0.;
        let pitch =
            (-basis[2][1]).atan2((basis[2][0] * basis[2][0] + basis[2][2] * basis[2][2]).sqrt());
        let roll = basis[0][1].atan2(basis[1][1]);
        if !wreck {
            if grounded {
                let roll_command = -input.steer
                    * self.value("Roll Control", "MaxRollCommand").to_radians()
                    * (speed.abs() / 45.).min(1.);
                let yaw_rate = speed * self.steer.tan()
                    / (self.model.wheel_rest[0][2] - self.model.wheel_rest[1][2])
                        .abs()
                        .max(1.);
                self.body.local_torque[1] += (yaw_rate.clamp(-3., 3.)
                    - self.body.local_angular_velocity[1])
                    * self.body_config.inertia[1]
                    * 8.;
                self.body.local_torque[2] +=
                    (roll_command - roll) * self.body_config.inertia[2] * 35.
                        - self.body.local_angular_velocity[2] * self.body_config.inertia[2] * 9.;
                let wheelie = input.lean > 0.3 && input.throttle > 0.3 && speed.abs() > 8.;
                let stoppie = input.lean < -0.3
                    && input.brake > self.value("Pitch Control", "StoppieBrake")
                    && speed.abs() > 3.;
                let target = slope / contacts as f32
                    + if wheelie {
                        -self.value("Pitch Control", "MaxPitchCommand").to_radians() * input.lean
                    } else if stoppie {
                        self.value("Pitch Control", "MaxStoppiePitchCommand")
                            .to_radians()
                            * input.brake
                    } else {
                        -input.lean * 0.12
                    };
                self.body.local_torque[0] += (target - pitch) * self.body_config.inertia[0] * 22.
                    - self.body.local_angular_velocity[0] * self.body_config.inertia[0] * 6.;
                self.frame.wheelie_active = wheelie;
                self.frame.stoppie_active = stoppie;
                self.air_time = 0.;
            } else {
                self.air_time += dt;
                let moment = self
                    .model
                    .tuning
                    .vector3("Airborne Control", "AirborneControlMoment")?;
                self.body.local_torque[0] += -input.lean * (-moment[0])
                    - input.throttle * self.value("Airborne Control", "GasGyroTorque")
                    + input.brake * self.value("Airborne Control", "BrakeGyroTorque");
                if self.spin_command.iter().any(|x| *x != 0.) {
                    self.spin_time += dt;
                } else {
                    self.spin_time = 0.;
                }
                if self.spin_command[0] != 0. {
                    let torque = (self.value("Backflip", "MinBackflipTorque")
                        + self.value("Backflip", "BackflipTorqueRateIncrease") * self.spin_time)
                        .min(self.value("Backflip", "MaxBackflipTorque"));
                    self.body.local_torque[0] -= torque * self.spin_command[0];
                }
                if self.spin_command[1] != 0. {
                    self.body.local_torque[1] +=
                        self.value("Three Sixty", "MaxThreeSixtyTorqueY") * self.spin_command[1];
                }
                self.body.local_torque[1] += input.steer * moment[1];
                self.body.local_torque[2] += input.steer * moment[2];
                for j in 0..3 {
                    self.body.local_torque[j] -= self.body.local_angular_velocity[j]
                        * self.value("Airborne Control", "AirborneDamping");
                }
                self.frame.crossup = input.steer
                    * self
                        .value("Airborne Control", "MaxCrossupAngle")
                        .to_radians();
                self.rpm += (self.value("Engine", "MinRPM") + input.throttle * 4500. - self.rpm)
                    * (dt * 8.).min(1.);
                self.frame.wheelie_active = false;
                self.frame.stoppie_active = false;
            }
            if grounded
                && (self.frame.crash_impact_speed
                    > self.value("Wrecks", "FreestyleDownWreckSpeedLimit")
                    || roll.abs()
                        > self
                            .value("Wrecks", "AirborneRollWreckAngle")
                            .to_radians()
                            .max(1.4)
                    || pitch.abs() > 1.4)
            {
                self.frame.wreck.begin(crate::wreck::Kind::Vertical);
                self.wreck_time = 0.;
            }
        } else {
            self.wreck_time += dt;
            self.frame.wreck.timer = self.wreck_time;
            let p = origin;
            if let Some(hit) = terrain.contact(p[0], p[2])?
                && p[1] < hit.height + 0.5
            {
                let speed = (-self.body.linear_velocity[1]).max(0.);
                self.frame.crash_impact_speed = self.frame.crash_impact_speed.max(speed);
                self.body.center[1] += hit.height + 0.5 - p[1];
                self.body.linear_velocity[1] = speed * 0.12;
                self.body.linear_velocity[0] *= 0.97;
                self.body.linear_velocity[2] *= 0.97;
                self.body.local_angular_velocity =
                    self.body.local_angular_velocity.map(|x| x * 0.97);
                if self.wreck_time > self.value("Wrecks", "MaxWreckTime").max(1.75) {
                    self.recover()?;
                    return Ok(());
                }
            }
        }
        let drag = self.value("Aerodynamics", "Drag");
        self.body.force = sub(
            self.body.force,
            scale(
                self.body.linear_velocity,
                drag * self.frame.speed_magnitude.max(1.),
            ),
        );
        self.body.step(&self.body_config, dt)?;
        if wreck && contacts > 0 && self.wreck_time > self.value("Wrecks", "MaxWreckTime").max(1.75)
        {
            self.recover()?;
        }
        if self
            .body
            .center
            .iter()
            .chain(self.body.linear_velocity.iter())
            .any(|x| !x.is_finite())
        {
            return Err("nonfinite motorcycle state".into());
        }
        if terrain
            .contact(self.body.center[0], self.body.center[2])?
            .is_none()
        {
            self.reset()?;
        }
        Ok(())
    }
    fn horsepower(&self, rpm: f32) -> f32 {
        let count = self.value("Engine", "NumberOfRPMPoints") as usize;
        let mut previous = (0., 0.);
        for i in 1..=count.min(32) {
            let next = (
                self.value("Engine", &format!("RPM{i}")),
                self.value("Engine", &format!("HP{i}")),
            );
            if rpm <= next.0 {
                return previous.1
                    + (next.1 - previous.1)
                        * ((rpm - previous.0) / (next.0 - previous.0).max(1.)).clamp(0., 1.);
            }
            previous = next;
        }
        previous.1
    }
    pub fn spin_controls(&mut self, mask: u8, axes: [f32; 2]) -> Result<()> {
        if mask > 7 || axes.iter().any(|x| !x.is_finite()) {
            return Err("invalid motorcycle spin controls".into());
        }
        self.spin_command = if mask == 4 {
            [
                if axes[1] > 0.5 { axes[1].min(1.) } else { 0. },
                if axes[0].abs() > 0.5 {
                    axes[0].clamp(-1., 1.)
                } else {
                    0.
                },
            ]
        } else {
            [0.; 2]
        };
        Ok(())
    }
    pub fn clutch_controls(&mut self, down: bool) {
        if self.clutch_down && !down {
            self.boost_time = self.value("Engine", "ClutchTorqueScaleTime");
        }
        self.clutch_down = down;
    }
    pub fn cancel_controls(&mut self) {
        self.preload = 0.;
        self.previous_preload = 0.;
        self.stunt_hold = false;
        self.spin_command = [0.; 2];
        self.spin_time = 0.;
        self.clutch_down = false;
        self.boost_time = 0.;
    }
    /// Authored control adapter using this disc's mappings/hold/safe/duration.
    /// Native linkage/blend/scoring/repetition rules remain a separate gate.
    pub fn stunt_controls(&mut self, modifiers: u8, axes: [f32; 2], dt: f32) -> Result<()> {
        if modifiers > 7 || axes.iter().any(|v| !v.is_finite()) {
            return Err("invalid motorcycle stunt controls".into());
        }
        let air = !self.frame.grounded.iter().any(|x| *x);
        if self.frame.wreck.phase != crate::wreck::Phase::Riding {
            self.active_stunt = None;
            self.stunt_name = None;
            return Ok(());
        }
        let mut mapping = String::new();
        if modifiers & 4 != 0 {
            mapping.push('1');
        }
        if modifiers & 1 != 0 {
            mapping.push('C');
        }
        if modifiers & 2 != 0 {
            mapping.push('T');
        }
        if let Some(dir) = crate::stunt::direction(axes) {
            mapping.push(['U', 'D', 'L', 'R'][dir]);
        }
        if air && self.spin_command.iter().any(|x| *x != 0.) {
            let name = if self.spin_command[0] != 0. {
                "mx_backflip"
            } else if self.spin_command[1] < 0. {
                "mx_threesixtyl"
            } else {
                "mx_threesixtyr"
            };
            if let Some(motion) = self.model.motions.get(name) {
                self.motion_name = name.into();
                self.motion_time = self.spin_time % motion.duration.max(0.01);
            }
            self.stunt_name = Some(
                if self.spin_command[0] != 0. {
                    "Backflip"
                } else {
                    "Three Sixty"
                }
                .into(),
            );
            return Ok(());
        }
        if self.active_stunt.is_none() {
            self.stunt_name = None;
        }
        if air
            && self.active_stunt.is_none()
            && modifiers != 0
            && let Some(index) = self.model.stunts.iter().position(|s| {
                s.mapping.len() == mapping.len()
                    && s.mapping.chars().all(|c| mapping.contains(c))
                    && s.duration > 0.
            })
        {
            self.active_stunt = Some(index);
            self.stunt_timer = 0.;
            self.stunt_hold = true;
        }
        if let Some(index) = self.active_stunt {
            let s = &self.model.stunts[index];
            self.stunt_hold = air
                && s.mapping.len() == mapping.len()
                && s.mapping.chars().all(|c| mapping.contains(c));
            self.stunt_timer =
                (self.stunt_timer + dt).min(if self.stunt_hold { s.hold } else { s.duration });
            self.stunt_name = Some(s.display.clone());
            if !air {
                if self.stunt_timer < s.safe {
                    self.frame.wreck.begin(crate::wreck::Kind::Vertical);
                    self.wreck_time = 0.;
                }
                self.active_stunt = None;
                self.stunt_name = None;
            } else {
                let motion_name = format!("mx_{}", s.name.to_ascii_lowercase());
                if let Some(motion) = self.model.motions.get(&motion_name) {
                    self.motion_name = motion_name;
                    self.motion_time = self.stunt_timer / s.duration * motion.duration;
                }
                if !self.stunt_hold && self.stunt_timer >= s.duration {
                    self.active_stunt = None;
                    self.stunt_name = None;
                }
            }
        }
        Ok(())
    }
    pub fn matrices(&self) -> Vec<M> {
        self.model.pose(
            if self.frame.wreck.phase == crate::wreck::Phase::Riding {
                self.visual_steer
            } else {
                self.steer + self.frame.crossup
            },
            self.angles,
            self.compression,
        )
    }
    pub fn rider_matrices(&self) -> Result<Vec<M>> {
        if self.stunt_name.is_none()
            && self.frame.wreck.phase == crate::wreck::Phase::Riding
            && let Some(pose) = &self.normal_pose
        {
            return Ok(pose.world_matrices.clone());
        }
        self.model.rider_pose(&self.motion_name, self.motion_time)
    }
    pub fn grip_errors(&self, rider: &[M], bike: &[M]) -> Option<[f32; 2]> {
        if self.stunt_name.is_some() || self.frame.wreck.phase != crate::wreck::Phase::Riding {
            return None;
        }
        Some(self.model.grip_errors(rider, bike))
    }
    pub fn foot_errors(&self, rider: &[M], bike: &[M]) -> Option<[f32; 2]> {
        if self.stunt_name.is_some() || self.frame.wreck.phase != crate::wreck::Phase::Riding {
            return None;
        }
        Some(self.model.foot_errors(rider, bike))
    }
    pub fn render_front_center(&self, pose: &[M]) -> V {
        pose[self.model.wheels[0]][12..15].try_into().unwrap()
    }
    pub fn world(&self) -> M {
        let b = self.body.basis();
        let p = self.frame.origin;
        [
            b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1],
            b[2][2], 0., p[0], p[1], p[2], 1.,
        ]
    }
    pub fn cycle_camera(&mut self, cycle: bool) {
        self.camera_preset = if cycle {
            (self.camera_preset + 1) % 3
        } else {
            0
        };
    }
    pub fn camera(&self, orbit: f32) -> Camera {
        let yaw = self.camera_yaw + orbit;
        let target = add(self.camera_origin, [0., 3., 0.]);
        let distance = [21., 12., 32.][self.camera_preset];
        let lift = [8., 5., 14.][self.camera_preset];
        Camera {
            position: add(target, [-yaw.sin() * distance, lift, -yaw.cos() * distance]),
            target,
            fov: 60.,
            preset: self.camera_preset,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_bounds_and_modes() {
        assert_eq!(class(27).unwrap(), (50, false));
        assert_eq!(class(34).unwrap(), (500, true));
        assert!(class(26).is_err());
        assert_eq!(class(35).unwrap(), (125, false));
        assert!(class(47).is_err());
    }
    #[test]
    fn comments_are_cleaned_before_numeric_loading() {
        let d = cleaned(b"[Engine]\nMinRPM=4500 ; source comment\n").unwrap();
        assert_eq!(d.first_scalar("Engine", "MinRPM").unwrap(), 4500.);
    }
}
