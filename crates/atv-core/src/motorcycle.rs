//! MX vs. ATV Unleashed motorcycle: model assembly, rider pose and the
//! riding controller. The controller in `motorcycle_drive.rs` and the tire
//! model in `motorcycle_tire.rs` are recovered component by component from
//! SLUS_211.04/GAME_REL.BIN (input filtering, kinematic steer and bank, tire
//! and suspension forces, engine/clutch/shifting, wheelie/stoppie, airborne
//! control, preload, backflip/three-sixty and the wreck decision chain). They
//! run through authored adapters: ATV2 terrain/obstacle contact, ATV2 map
//! surface coefficients, browser input, an omitted chassis collision hull and
//! the wreck body below. Native whole-step parity has not been measured.
//!
//! Remaining authored stabilizers (marked `Authored stabilizer:`):
//! - wreck body held at terrain + 0.5 ft, keeping 12% of the impact speed as
//!   rebound and damping other motion by 0.97 per step;
//! - detached rider clamped to the terrain with 0.85 horizontal friction.
//!
//! See `research/MOTORCYCLES.md`.
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
#[path = "motorcycle_balance.rs"]
mod balance;
#[path = "motorcycle_drive.rs"]
mod drive;
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
/// Authored ATV2 roster stand-in for a motorcycle player: the CPU group of a
/// comparable stock quad per MX class (Mini, 125, 250, 500). ATV players pass
/// through unchanged.
pub fn roster_quad(quad: u32) -> u32 {
    match u16::try_from(quad).ok().and_then(|id| class(id).ok()) {
        Some((50, _)) => 14,
        Some((125, _)) => 5,
        Some((250, _)) => 4,
        Some(_) => 7,
        None => quad,
    }
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
    pub boundary_launch: crate::boundary_launch::State,
    pub mud_amount: [f32; 2],
    pub surfaces: [usize; 2],
    pub vehicle_kind: &'static str,
    pub reconstruction: &'static str,
}
pub struct Motorcycle {
    pub model: Model,
    body: dynamics::BodyState,
    body_config: dynamics::BodyConfig,
    params: drive::Params,
    drive: drive::Drive,
    springs: [f32; 2],
    preload_axis: f32,
    trick_mask: u8,
    trick_axes: [f32; 2],
    start: V,
    start_yaw: f32,
    safe_origin: V,
    safe_yaw: f32,
    race_checkpoint: Option<(V, f32)>,
    boundary: Option<(crate::boundary_launch::Config, bool)>,
    boundary_launch: crate::boundary_launch::State,
    boundary_random: crate::engine_audio::OriginalRandom,
    rider_body: Option<(M, V)>,
    pose_controls: pose::Controls,
    normal_pose: Option<animation::Pose>,
    visual_steer: f32,
    frame: Frame,
    steer: f32,
    compression: [f32; 2],
    angles: [f32; 2],
    mud_timer: [f32; 2],
    wreck_time: f32,
    reset_count: u32,
    pub tick: u64,
    pub time: f32,
    obstacles: Vec<crate::obstacle::StaticObstacle>,
    materials: Option<[crate::materials::TerrainMaterial; 8]>,
    pub last_input: VehicleInput,
    pub motion_name: String,
    pub motion_time: f32,
    pub stunt_name: Option<String>,
    pub stunt_timer: f32,
    clutch_down: bool,
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
        let params = drive::Params::parse(&model.tuning)?;
        let drive = drive::Drive::new(&params);
        let springs = params.tires.map(|t| t.spring_stopped);
        let mut result = Self {
            model,
            body,
            body_config: config,
            params,
            drive,
            springs,
            preload_axis: 0.,
            trick_mask: 0,
            trick_axes: [0.; 2],
            start: origin,
            start_yaw: yaw,
            safe_origin: origin,
            safe_yaw: yaw,
            race_checkpoint: None,
            boundary: None,
            boundary_launch: crate::boundary_launch::State::default(),
            boundary_random: crate::engine_audio::OriginalRandom::with_seed(1),
            rider_body: None,
            pose_controls: pose::Controls::default(),
            normal_pose: None,
            visual_steer: 0.,
            frame: Frame::default(),
            steer: 0.,
            compression: [0.; 2],
            angles: [0.; 2],
            mud_timer: [0.; 2],
            wreck_time: 0.,
            reset_count: 0,
            tick: 0,
            time: 0.,
            obstacles: Vec::new(),
            materials: None,
            last_input: VehicleInput::default(),
            motion_name: "mx_turn_still_mid".into(),
            motion_time: 0.,
            stunt_name: None,
            stunt_timer: 0.,
            clutch_down: false,
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
        self.body_config.center_of_mass = self.params.cofg;
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
        self.body_config.max_speed_squared = crate::boundary_launch::NORMAL_MAX_SPEED_SQUARED;
        self.boundary_launch.reset();
        self.compression = [0.; 2];
        self.angles = [0.; 2];
        self.mud_timer = [0.; 2];
        self.clutch_down = false;
        self.wreck_time = 0.;
        self.drive = drive::Drive::new(&self.params);
        self.springs = self.params.tires.map(|t| t.spring_stopped);
        self.trick_mask = 0;
        self.trick_axes = [0.; 2];
        self.preload_axis = 0.;
        self.reset_count = self.reset_count.wrapping_add(1);
        self.rider_body = None;
        self.pose_controls = pose::Controls::default();
        self.normal_pose = None;
        self.visual_steer = 0.;
        self.frame = Frame::default();
        self.last_input = VehicleInput::default();
        self.stunt_name = None;
        self.active_stunt = None;
        self.stunt_hold = false;
        self.refresh();
        Ok(())
    }
    /// The original ATV2 map-edge launch (001e5260) applies to bikes riding
    /// ATV2 maps; CrowdPresent/Training select the extended boundary.
    pub fn set_boundary(
        &mut self,
        config: crate::boundary_launch::Config,
        extended: bool,
    ) -> Result<()> {
        config.outside([0.; 3], extended)?;
        self.boundary = Some((config, extended));
        Ok(())
    }
    fn boundary_step(&mut self, dt: f32) -> Result<()> {
        self.boundary_launch.tick(dt, true)?;
        let origin = self.body.origin(&self.body_config);
        if !self.boundary_launch.active
            && self.frame.wreck.phase == crate::wreck::Phase::Riding
            && let Some((config, extended)) = self.boundary
            && config.outside(origin, extended)?
        {
            let words: [u32; 4] = std::array::from_fn(|_| self.boundary_random.next_u31());
            let launch = config.launch(origin, [words[0], words[1]])?;
            let kind = crate::boundary_launch::random_wreck_kind([words[2], words[3]])?;
            self.body.linear_velocity = launch.linear_velocity;
            self.body.local_angular_velocity = launch.local_angular_velocity;
            self.body_config.max_speed_squared = crate::boundary_launch::LAUNCH_MAX_SPEED_SQUARED;
            self.boundary_launch.begin();
            self.frame.wreck.begin(kind);
            self.wreck_time = 0.;
        }
        self.frame.boundary_launch = self.boundary_launch;
        Ok(())
    }
    /// Race recovery returns to the racing line, like the ATV checkpoint.
    pub fn set_race_checkpoint(&mut self, checkpoint: Option<(V, f32)>) {
        self.race_checkpoint = checkpoint
            .filter(|(origin, yaw)| origin.iter().all(|v| v.is_finite()) && yaw.is_finite());
    }
    /// Manual reset: the race checkpoint when racing, otherwise the start.
    pub fn reset_now(&mut self) -> Result<()> {
        if self.race_checkpoint.is_some() {
            self.recover()
        } else {
            self.reset()
        }
    }
    /// Rider-to-rider contact through the shared equal-mass peer proxy. A
    /// contact impulse beyond the source side/front/back wreck limits ejects.
    pub fn peer_contact(
        &mut self,
        origin: V,
        velocity: V,
        rotation: [f32; 4],
        dt: f32,
        tie: f32,
    ) -> bool {
        let own = crate::peer_contact::Body {
            origin: self.body.origin(&self.body_config),
            velocity: self.body.linear_velocity,
            rotation: self.body.orientation,
        };
        let other = crate::peer_contact::Body {
            origin,
            velocity,
            rotation,
        };
        let Some(response) = crate::peer_contact::resolve(own, other, dt, tie) else {
            return false;
        };
        let change = sub(response.velocity, self.body.linear_velocity);
        self.body.center = add(self.body.center, response.displacement);
        self.body.linear_velocity = response.velocity;
        let spin =
            dynamics::inverse_transform(self.body.basis(), [0., response.yaw_rate_change, 0.]);
        self.body.local_angular_velocity = add(self.body.local_angular_velocity, spin);
        if self.frame.wreck.phase == crate::wreck::Phase::Riding {
            let local = dynamics::inverse_transform(self.body.basis(), change);
            let (limit, kind) = if local[2] < 0. && local[2].abs() > local[0].abs() {
                ("FrontWreckSpeedLimit", crate::wreck::Kind::Front)
            } else if local[2].abs() > local[0].abs() {
                ("BackWreckSpeedLimit", crate::wreck::Kind::Back)
            } else if local[0] > 0. {
                ("SideWreckSpeedLimit", crate::wreck::Kind::Left)
            } else {
                ("SideWreckSpeedLimit", crate::wreck::Kind::Right)
            };
            if dot(local, local).sqrt() > self.value("Wrecks", limit) {
                self.frame.wreck.begin(kind);
                self.wreck_time = 0.;
            }
        }
        self.refresh();
        true
    }
    fn recover(&mut self) -> Result<()> {
        let (origin, yaw) = self
            .race_checkpoint
            .unwrap_or((self.safe_origin, self.safe_yaw));
        self.reset()?;
        self.body = dynamics::BodyState::new(
            add(origin, [0., 0.4, 0.]),
            [(yaw / 2.).cos(), 0., (yaw / 2.).sin(), 0.],
            &self.body_config,
        )?;
        self.refresh();
        Ok(())
    }
    pub fn rider_velocity(&self) -> V {
        self.rider_body
            .as_ref()
            .map_or(self.frame.linear_velocity, |b| b.1)
    }
    pub fn rider_world(&self) -> M {
        self.rider_body
            .as_ref()
            .map_or_else(|| self.world(), |b| b.0)
    }
    pub fn set_obstacles(&mut self, obstacles: Vec<crate::obstacle::StaticObstacle>) {
        self.obstacles = obstacles;
    }
    /// The map's terrain zone coefficients (`.toy` TerrainZone1..8).
    pub fn set_materials(&mut self, materials: Option<[crate::materials::TerrainMaterial; 8]>) {
        self.materials = materials;
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
        self.frame.rpm = self.drive.rpm;
        self.frame.clutch_active = self.clutch_down;
        self.frame.clutch_boost = (self.drive.boost_scale - 1.).max(0.);
        self.frame.gear = self.drive.gear;
        self.frame.wheel_angles = self.angles;
        self.frame.wheel_compression = self.compression;
        self.frame.steering_angle = self.steer;
        self.frame.visual_steering_angle = self.visual_steer;
        self.frame.reset_count = self.reset_count;
        self.frame.vehicle_kind = "motorcycle";
        self.frame.reconstruction = "recovered MX controller components through authored contact adapters; native step parity unmeasured";
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
        self.preload_axis = input.preload_axis.unwrap_or(input.lean).clamp(-1., 1.);
        self.prepare(input, dt);
        // The source world steps at 55 Hz: int(dt·55)+1 substeps per frame.
        let n = (dt * 55.) as usize + 1;
        let h = dt / n as f32;
        for _ in 0..n {
            self.substep(terrain, h)?;
            self.obstacle_step(h)?;
            if self.wreck_body_step(terrain, h)? {
                return Ok(());
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
                return self.reset();
            }
        }
        self.finish(dt);
        let riding = self.frame.wreck.phase == crate::wreck::Phase::Riding;
        if let Some(kind) = self.wreck_check()
            && riding
        {
            self.frame.wreck.begin(kind);
            self.wreck_time = 0.;
        }
        self.sync_frame(dt);
        self.boundary_step(dt)?;
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
                // Authored stabilizer: detached-rider terrain clamp, 0.85 friction.
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
        let (normal, visual) = self.model.normal_pose(
            self.pose_controls,
            self.angles,
            self.compression,
            self.steer + self.frame.crossup,
        )?;
        self.normal_pose = Some(normal);
        self.visual_steer = visual;
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
    /// Authored swept probes keep the bike out of the map's solid props; the
    /// closing-speed change feeds the recovered front/back/side wreck limits.
    /// This is separate from the unrecovered MX COL-v2 routing.
    fn obstacle_step(&mut self, h: f32) -> Result<()> {
        let origin = self.body.origin(&self.body_config);
        let basis = self.body.basis();
        let speed = self.forward_speed();
        let probes: Vec<_> = [
            self.model.wheel_rest[0],
            self.model.wheel_rest[1],
            [0., 2., 0.],
        ]
        .into_iter()
        .map(|p| {
            let start = add(origin, dynamics::transform(basis, p));
            let travel = add(
                scale(self.body.linear_velocity, h),
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
                let change = scale(hit.world_normal, closing * 1.05);
                self.body.linear_velocity = add(self.body.linear_velocity, change);
                self.body.center = add(self.body.center, scale(hit.world_normal, 0.12));
                let local = dynamics::inverse_transform(basis, change);
                self.drive.latch_impact(local);
            }
        }
        Ok(())
    }
    /// Wreck body handling. Authored stabilizer: the wrecked bike is held at
    /// terrain + 0.5 ft with 12 % rebound and 0.97 damping because the source
    /// chassis collision hull is not reconstructed. Respawn follows the
    /// source 0.5 s settle plus MaxWreckTime. Returns true after recovery.
    fn wreck_body_step(&mut self, terrain: &Terrain, h: f32) -> Result<bool> {
        if self.frame.wreck.phase == crate::wreck::Phase::Riding {
            return Ok(false);
        }
        self.wreck_time += h;
        self.frame.wreck.timer = self.wreck_time;
        let p = self.body.origin(&self.body_config);
        let mut on_ground = self.drive.tires.iter().any(|t| t.grounded);
        if let Some(hit) = terrain.contact(p[0], p[2])?
            && p[1] < hit.height + 0.5
        {
            on_ground = true;
            let speed = (-self.body.linear_velocity[1]).max(0.);
            self.frame.crash_impact_speed = self.frame.crash_impact_speed.max(speed);
            self.body.center[1] += hit.height + 0.5 - p[1];
            self.body.linear_velocity[1] = speed * 0.12;
            self.body.linear_velocity[0] *= 0.97;
            self.body.linear_velocity[2] *= 0.97;
            self.body.local_angular_velocity = self.body.local_angular_velocity.map(|x| x * 0.97);
        }
        if on_ground && self.wreck_time > 0.5 + self.params.max_wreck_time {
            self.recover()?;
            return Ok(true);
        }
        Ok(false)
    }
    /// Publish controller state to the frame and the pose/effect fields.
    fn sync_frame(&mut self, dt: f32) {
        let d = &self.drive;
        self.steer = d.delta;
        self.compression = d.tires.map(|t| t.display);
        self.angles = d.tires.map(|t| t.angle);
        self.frame.grounded = d.tires.map(|t| t.grounded);
        self.frame.surfaces = d.tires.map(|t| t.surface);
        self.frame.wheelie_active = d.pitch_control && !d.stoppie;
        self.frame.stoppie_active = d.stoppie;
        let grounded = d.tires.iter().any(|t| t.grounded);
        self.frame.preload_available = grounded;
        self.frame.preload_armed = d.preload.armed;
        self.frame.preload_active = d.preload.firing;
        self.frame.preload_strength = if d.preload.armed {
            (d.hold[0] / self.params.preload_window[1].max(1.)).clamp(0., 1.)
        } else {
            0.
        };
        self.frame.crossup = if grounded {
            0.
        } else {
            d.crab
                .clamp(-self.params.max_crossup, self.params.max_crossup)
        };
        let riding = self.frame.wreck.phase == crate::wreck::Phase::Riding;
        if riding
            && d.tires.iter().all(|t| t.grounded)
            && self.frame.crash_impact_speed == 0.
            && self.forward_speed().abs() > 3.
        {
            let b = self.body.basis();
            self.safe_origin = self.body.origin(&self.body_config);
            self.safe_yaw = b[2][0].atan2(b[2][2]);
        }
        // Source mud: front-section MudTime/MudAmount per surface.
        let speed = self.forward_speed().abs();
        for i in 0..2 {
            let t = self.drive.tires[i];
            if !t.grounded || speed <= 3. {
                continue;
            }
            let key = |k: &str| self.value("Front Tire", &format!("{k}{}", t.surface + 1));
            let (time, amount) = (key("MudTime"), key("MudAmount"));
            if time > 0. {
                self.mud_timer[i] += dt;
                if self.mud_timer[i] >= time {
                    self.mud_timer[i] -= time;
                    self.frame.mud_amount[i] = (self.frame.mud_amount[i] + amount).clamp(0., 1.);
                }
            }
        }
    }
    /// Trick button mask and trick stick for preload/backflip/three-sixty.
    pub fn spin_controls(&mut self, mask: u8, axes: [f32; 2]) -> Result<()> {
        if mask > 7 || axes.iter().any(|x| !x.is_finite()) {
            return Err("invalid motorcycle spin controls".into());
        }
        self.trick_mask = mask;
        self.trick_axes = axes.map(|v| v.clamp(-1., 1.));
        Ok(())
    }
    pub fn clutch_controls(&mut self, down: bool) {
        self.clutch_down = down;
    }
    pub fn cancel_controls(&mut self) {
        self.drive.preload = drive::Preload::default();
        self.drive.backflip = drive::Spin::default();
        self.drive.spin = drive::Spin::default();
        self.stunt_hold = false;
        self.trick_mask = 0;
        self.trick_axes = [0.; 2];
        self.clutch_down = false;
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
        let flip = self.drive.backflip.active && self.drive.backflip.airborne;
        let spin = self.drive.spin.active && self.drive.spin.airborne;
        if air && (flip || spin) {
            let name = if flip {
                "mx_backflip"
            } else if self.drive.spin_dir < 0. {
                "mx_threesixtyl"
            } else {
                "mx_threesixtyr"
            };
            if let Some(motion) = self.model.motions.get(name) {
                self.motion_name = name.into();
                self.motion_time = self.drive.airtime % motion.duration.max(0.01);
            }
            self.stunt_name = Some(if flip { "Backflip" } else { "Three Sixty" }.into());
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
