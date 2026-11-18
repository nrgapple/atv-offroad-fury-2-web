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
use crate::math::Det;
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
#[path = "motorcycle_chassis.rs"]
mod chassis;
#[path = "motorcycle_drive.rs"]
mod drive;
#[path = "motorcycle_handling.rs"]
mod handling;
#[path = "motorcycle_pose.rs"]
mod pose;
type V = [f32; 3];
type M = [f32; 16];
fn mul(a: &M, b: &M) -> M {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn rotation(axis: usize, angle: f32) -> M {
    let q = {
        let mut q = [(angle * 0.5).cos_d(), 0., 0., 0.];
        q[axis + 1] = (angle * 0.5).sin_d();
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
/// A member's motion name when it is a `.mot` file (lower case, no folder).
fn motion_name(member: &str) -> Option<String> {
    member.to_ascii_lowercase().ends_with(".mot").then(|| {
        member
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".MOT")
            .trim_end_matches(".mot")
            .to_ascii_lowercase()
    })
}
/// The assembled bike and rider meshes and the bike's rig nodes.
struct Rig {
    mesh: Mesh,
    rider: Mesh,
    wheels: [usize; 2],
    wheel_rest: [V; 2],
    steering_axis: V,
    grips: [usize; 2],
}
impl Rig {
    fn load(archive: &Resource<'_>, definition: &str, size: u16) -> Result<Self> {
        let mesh = assembly(
            archive,
            definition,
            if size == 50 { "bike50.psg" } else { "bike.psg" },
        )?;
        let rider = assembly(archive, "THQ1.sub", "mrider.psg")?;
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
        Ok(Self {
            mesh,
            rider,
            wheels,
            wheel_rest,
            steering_axis,
            grips,
        })
    }
}
/// Node world matrices for a steering angle, wheel spins and suspension
/// travel ([`Model::pose`]).
fn posed(
    mesh: &Mesh,
    steering_axis: V,
    wheels: [usize; 2],
    steering: f32,
    angles: [f32; 2],
    compression: [f32; 2],
) -> Vec<M> {
    let mut world = Vec::with_capacity(mesh.nodes.len());
    for (i, n) in mesh.nodes.iter().enumerate() {
        let mut local = n.local_matrix;
        if n.name.eq_ignore_ascii_case("Handlebars") {
            local = mul(&local, &pose::axis_rotation(steering_axis, steering));
        }
        if n.name.eq_ignore_ascii_case("Shocks") {
            for j in 0..3 {
                local[12 + j] += steering_axis[j] * compression[0] / steering_axis[1];
            }
        }
        if n.name.eq_ignore_ascii_case("SwingArm") {
            local = mul(&local, &rotation(0, compression[1] / 2.));
        }
        for (wheel, &index) in wheels.iter().enumerate() {
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
/// What the garage shows of a motorcycle: the meshes and rest pose of
/// [`Model`] and only the rider's selection clips. A [`Model`] also decodes
/// every riding and stunt motion and its configs, which the menu never uses
/// (selectionAssets).
pub struct Selection {
    pub mesh: Mesh,
    pub rider: Mesh,
    /// The bike's node world matrices at rest (`Model::pose(0, 0, 0)`).
    pub pose: Vec<M>,
    /// The `ui_rider_select_*` clips by name.
    pub motions: BTreeMap<String, animation::Animation>,
}
impl Selection {
    pub fn load(bytes: &[u8], id: u16) -> Result<Self> {
        let (size, _, definition, _) = definition(id)?;
        let archive = Resource::parse(bytes)?;
        let rig = Rig::load(&archive, definition, size)?;
        let mut motions = BTreeMap::new();
        for (i, m) in archive.members.iter().enumerate() {
            if let Some(name) = motion_name(&m.name).filter(|n| n.starts_with("ui_rider_select")) {
                motions.insert(name, animation::decode_mot(&archive.extract(i)?)?);
            }
        }
        let pose = posed(
            &rig.mesh,
            rig.steering_axis,
            rig.wheels,
            0.,
            [0., 0.],
            [0., 0.],
        );
        Ok(Self {
            mesh: rig.mesh,
            rider: rig.rider,
            pose,
            motions,
        })
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
    pub chassis: chassis::Chassis,
    /// Rider PelvisBoxPosition (model space) and sweep side (see pose).
    pelvis_box: V,
    sweep_side: f32,
}
impl Model {
    pub fn load(bytes: &[u8], id: u16) -> Result<Self> {
        let (size, pro, definition, _) = definition(id)?;
        let archive = Resource::parse(bytes)?;
        let Rig {
            mesh,
            rider,
            wheels,
            wheel_rest,
            steering_axis,
            grips,
        } = Rig::load(&archive, definition, size)?;
        let tuning = cleaned(&member(
            &archive,
            &format!("mx_{}{size}P.phy", if pro { "Pro" } else { "Race" }),
        )?)?;
        let mut motions = BTreeMap::new();
        for (i, m) in archive.members.iter().enumerate() {
            if let Some(name) = motion_name(&m.name) {
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
        let chassis = chassis::Chassis::load(&archive, size)
            .unwrap_or_else(|| chassis::Chassis::fallback(&mesh, wheel_rest, size));
        let pelvis_box = tuning
            .vector3("Animation", "PelvisBoxPosition")
            .unwrap_or([0., 4.3, -1.7]);
        let mut model = Self {
            chassis,
            pelvis_box,
            sweep_side: 1.,
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
        };
        model.sweep_side = model.sweep_side();
        Ok(model)
    }
    pub fn pose(&self, steering: f32, angles: [f32; 2], compression: [f32; 2]) -> Vec<M> {
        posed(
            &self.mesh,
            self.steering_axis,
            self.wheels,
            steering,
            angles,
            compression,
        )
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
    /// Chassis whisker contacts this frame, those against moving bodies (the
    /// train), and whether a tire stands on a moving body.
    pub chassis_contact_count: usize,
    pub dynamic_contact_count: usize,
    pub moving_support: bool,
    /// Scene meshes selected near the bike this frame, and the moving ones.
    pub scene_candidates: usize,
    pub dynamic_candidates: usize,
    /// Chassis contacts against plant (tree) proxies this frame.
    pub vegetation_contact_count: usize,
    /// Rider pelvis spring offset (ft, bike frame).
    pub rider_pelvis_offset: V,
    /// Rider boot peg pins (0 released for feet down / foot out, 1 pinned).
    pub rider_foot_pins: [f32; 2],
    /// Bank the rider holds (rad, + toward the bike's +x), rear sideslip
    /// (rad) and how hard the rear is sliding (0..1).
    pub lean: f32,
    pub rear_slip: f32,
    pub rear_slide: f32,
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
    handling: handling::State,
    /// Ride the recovered MX side-force curves and steer-proportional bank
    /// instead of the authored single-track handling (parity research).
    classic: bool,
    springs: [f32; 2],
    pub tires: crate::tire_wear::Tires<2>,
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
    /// The thrown rider: an articulated body that tumbles and comes to rest
    /// on the terrain. `rider_body` follows its pelvis.
    ragdoll: Option<crate::ragdoll::Ragdoll>,
    /// World rider pose after the last riding step and the bike's velocity
    /// entering the current step: what the rider carries into a wreck.
    rider_last: Vec<M>,
    entry_velocity: V,
    pose_controls: pose::Controls,
    /// Last two pelvis-box world positions for the rider load estimate.
    pelvis_history: [Option<V>; 2],
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
    surface_source: crate::materials::SurfaceSource,
    scene: Option<crate::scene_collision::SceneCollision>,
    /// This frame's physical candidates (static props, plant proxies, train).
    contacts: Vec<(
        crate::obstacle::StaticObstacle,
        Option<crate::train::MotionState>,
        bool,
    )>,
    chassis_contacts: Vec<chassis::Contact>,
    chassis_response: chassis::Response,
    pub last_input: VehicleInput,
    pub motion_name: String,
    pub motion_time: f32,
    pub stunt_name: Option<String>,
    pub stunt_timer: f32,
    clutch_down: bool,
    active_stunt: Option<usize>,
    stunt_hold: bool,
    /// The trick clip laid over the riding pose and how much of it shows
    /// (0 riding, 1 the clip): tricks ease in and out instead of cutting.
    stunt_clip: Option<(String, f32)>,
    stunt_weight: f32,
}
/// Trick clips play this much faster than the source timings, which were
/// authored for MX Unleashed's long jumps: an ATV2 jump is over in about a
/// second.
const STUNT_RATE: f32 = 1.5;
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
            [(yaw / 2.).cos_d(), 0., (yaw / 2.).sin_d(), 0.],
            &config,
        )?;
        let params = drive::Params::parse(&model.tuning)?;
        let chassis_response = chassis::Response::parse(&model.tuning, params.weight);
        let drive = drive::Drive::new(&params);
        let springs = params.tires.map(|t| t.spring_stopped);
        let mut result = Self {
            model,
            body,
            body_config: config,
            params,
            drive,
            handling: handling::State::default(),
            classic: false,
            springs,
            tires: Default::default(),
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
            ragdoll: None,
            rider_last: Vec::new(),
            entry_velocity: [0.; 3],
            pose_controls: pose::Controls::default(),
            pelvis_history: [None; 2],
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
            surface_source: crate::materials::SurfaceSource::Atv2,
            scene: None,
            contacts: Vec::new(),
            chassis_contacts: Vec::new(),
            chassis_response,
            last_input: VehicleInput::default(),
            motion_name: "mx_turn_still_mid".into(),
            motion_time: 0.,
            stunt_name: None,
            stunt_timer: 0.,
            clutch_down: false,
            active_stunt: None,
            stunt_hold: false,
            stunt_clip: None,
            stunt_weight: 0.,
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
                (self.start_yaw / 2.).cos_d(),
                0.,
                (self.start_yaw / 2.).sin_d(),
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
        self.handling.reset();
        self.springs = self.params.tires.map(|t| t.spring_stopped);
        self.trick_mask = 0;
        self.trick_axes = [0.; 2];
        self.preload_axis = 0.;
        self.reset_count = self.reset_count.wrapping_add(1);
        self.rider_body = None;
        self.ragdoll = None;
        self.rider_last.clear();
        self.pose_controls = pose::Controls::default();
        self.pelvis_history = [None; 2];
        self.normal_pose = None;
        self.visual_steer = 0.;
        self.frame = Frame::default();
        self.last_input = VehicleInput::default();
        self.stunt_name = None;
        self.active_stunt = None;
        self.stunt_hold = false;
        self.stunt_clip = None;
        self.stunt_weight = 0.;
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
    /// Select the recovered MX ground handling (true) or the authored
    /// single-track handling (false, the default).
    pub fn set_classic(&mut self, classic: bool) {
        self.classic = classic;
    }
    pub fn classic(&self) -> bool {
        self.classic
    }
    /// Race recovery returns to the racing line, like the ATV checkpoint.
    pub fn set_race_checkpoint(&mut self, checkpoint: Option<(V, f32)>) {
        self.race_checkpoint = checkpoint
            .filter(|(origin, yaw)| origin.iter().all(|v| v.is_finite()) && yaw.is_finite());
    }
    /// Make `origin`/`yaw` the start pose and reset there.
    pub fn place(&mut self, origin: V, yaw: f32) -> Result<()> {
        if !yaw.is_finite() || origin.iter().any(|v| !v.is_finite()) {
            return Err("invalid motorcycle placement".into());
        }
        self.start = origin;
        self.start_yaw = yaw;
        self.safe_origin = origin;
        self.safe_yaw = yaw;
        self.reset()
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
            [(yaw / 2.).cos_d(), 0., (yaw / 2.).sin_d(), 0.],
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
    /// The map's partitioned scene: static props, vegetation proxies and any
    /// installed moving train bodies, as the ATV queries them.
    pub fn set_scene(&mut self, scene: Option<crate::scene_collision::SceneCollision>) {
        self.scene = scene;
    }
    pub fn scene_collision(&self) -> Option<&crate::scene_collision::SceneCollision> {
        self.scene.as_ref()
    }
    pub fn scene_collision_mut(&mut self) -> Option<&mut crate::scene_collision::SceneCollision> {
        self.scene.as_mut()
    }
    /// Once per frame: the physical meshes near the bike, with moving-target
    /// motion. Without a partitioned scene, every static obstacle is a candidate.
    fn select_contacts(&mut self) -> Result<()> {
        let center = self.body.center;
        let radius = 12. + dot(self.body.linear_velocity, self.body.linear_velocity).sqrt() / 30.;
        self.contacts.clear();
        let selected = if let Some(scene) = &mut self.scene {
            let mud = scene.mud_obstacles().to_vec();
            let eligible = |index: usize, _: &crate::obstacle::StaticObstacle| !mud[index];
            let query = scene.cached_query(center, radius, eligible)?;
            let selection = scene.physical_selection(
                &query,
                crate::scene_collision::PhysicalTrim::Supplied,
                center,
                radius,
                false,
                |index, _| !mud[index],
            )?;
            selection
                .meshes
                .into_iter()
                .map(|m| (m.obstacle, m.motion, m.proxy_index.is_some()))
                .collect()
        } else {
            self.obstacles
                .iter()
                .cloned()
                .map(|o| (o, None, false))
                .collect()
        };
        self.contacts = selected;
        self.frame.scene_candidates = self.contacts.len();
        self.frame.dynamic_candidates = self.contacts.iter().filter(|c| c.1.is_some()).count();
        Ok(())
    }
    /// Select source tire routing without changing raw contact/effect surface indices.
    pub fn set_surface_source(&mut self, source: crate::materials::SurfaceSource) {
        self.surface_source = source;
    }
    /// ATV2 coefficients; also restores the legacy source mode on map changes.
    pub fn set_materials(&mut self, materials: Option<[crate::materials::TerrainMaterial; 8]>) {
        self.materials = materials;
        self.surface_source = crate::materials::SurfaceSource::Atv2;
    }
    /// Rear contact location and tread scrub speed used by enhanced roost.
    pub fn rear_roost(&self) -> Option<(V, f32)> {
        let rear = &self.drive.tires[1];
        if !rear.grounded {
            return None;
        }
        if self.tires.mode != crate::tire_wear::Handling::Original {
            return Some((rear.point, rear.scrub_speed));
        }
        if self.classic {
            return None;
        }
        // Single-track handling: the tread scrubs when the rear runs wide
        // and when it is driving or braking hard.
        let sideways = self.handling.slip.abs() * self.frame.speed_magnitude;
        Some((rear.point, sideways.min(30.) + 7. * self.handling.drive))
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
        self.entry_velocity = self.body.linear_velocity;
        self.frame.crash_impact_speed = 0.;
        self.preload_axis = input.preload_axis.unwrap_or(input.lean).clamp(-1., 1.);
        self.prepare(input, dt);
        self.predict_landing(terrain)?;
        self.select_contacts()?;
        self.detect_chassis(terrain)?;
        // The source world steps at 55 Hz: int(dt·55)+1 substeps per frame.
        let n = (dt * 55.) as usize + 1;
        let h = dt / n as f32;
        for _ in 0..n {
            self.substep(terrain, h)?;
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
                let yaw = b[2][0].atan2_d(b[2][2]);
                let r = rotation(1, yaw);
                world[..12].copy_from_slice(&r[..12]);
                self.rider_body = Some((world, add(self.body.linear_velocity, [0., 8., 0.])));
                self.ragdoll = self.throw_rider(dt);
            }
            let floor = |p: V| {
                terrain
                    .contact(p[0], p[2])
                    .ok()
                    .flatten()
                    .map(|hit| (hit.height, hit.normal))
            };
            if let Some(ragdoll) = &mut self.ragdoll {
                ragdoll.step(dt, &floor);
                let pelvis = ragdoll.center(
                    self.model
                        .rider
                        .nodes
                        .iter()
                        .position(|n| n.name.eq_ignore_ascii_case("Pelvis"))
                        .and_then(|node| ragdoll.body_of_node(node))
                        .unwrap_or(0),
                );
                if pelvis.iter().all(|v| v.is_finite())
                    && let Some((world, velocity)) = &mut self.rider_body
                {
                    let previous: V = [world[12], world[13], world[14]];
                    if dt > 0. {
                        *velocity = scale(sub(pelvis, previous), 1. / dt);
                    }
                    *world = rotation(1, 0.);
                    world[12..15].copy_from_slice(&pelvis);
                }
            } else if let Some((world, velocity)) = &mut self.rider_body {
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
            self.ragdoll = None;
        }
        let load = self.pelvis_load(dt);
        let b = self.body.basis();
        let pose_input = pose::Input {
            lean: self.drive.lean,
            crab: self.pose_crab(),
            max_crab: self.params.max_crossup,
            speed: self.frame.speed_magnitude,
            yaw_rate: self.body.world_angular_velocity()[1],
            up_y: b[1][1],
            grounded: self.frame.grounded.iter().any(|v| *v),
            reverse: self.drive.reverse,
            smooth_release: !self.classic
                || self.tires.mode != crate::tire_wear::Handling::Original,
            load,
        };
        self.pose_controls
            .step(pose_input, self.model.sweep_side, dt);
        self.frame.rider_foot_pins = self.pose_controls.feet;
        let mut riding_pose = self.pose_controls;
        riding_pose.contact_ik = pose_input.smooth_release;
        riding_pose.pelvis = add(riding_pose.pelvis, self.drive.stance.shift);
        if !self.classic && pose_input.grounded {
            riding_pose.roll = self.handling.lean;
            riding_pose.look = self.handling.command;
        }
        let visual = self.steer + self.frame.crossup;
        let showing =
            self.stunt_name.is_some() && self.frame.wreck.phase == crate::wreck::Phase::Riding;
        self.stunt_weight = if showing {
            (self.stunt_weight + 9. * dt).min(1.)
        } else {
            (self.stunt_weight - 7. * dt).max(0.)
        };
        if self.stunt_weight == 0. {
            self.stunt_clip = None;
        }
        let overlay = self.stunt_clip.as_ref().and_then(|(name, time)| {
            let motion = self.model.motions.get(name)?;
            let channels = motion
                .sample_channels(
                    &self.model.rider.nodes,
                    time.clamp(0., motion.duration),
                    Default::default(),
                )
                .ok()?;
            let w = self.stunt_weight;
            Some((channels, w * w * (3. - 2. * w)))
        });
        let (normal, pelvis) =
            self.model
                .normal_pose(riding_pose, self.angles, self.compression, visual, overlay)?;
        self.frame.rider_pelvis_offset = pelvis;
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
                motion.duration * self.pose_controls.sweep
            };
        }
        if self.frame.wreck.phase == crate::wreck::Phase::Riding {
            let world = self.world();
            self.rider_last = self
                .rider_matrices()?
                .iter()
                .map(|m| mul(&world, m))
                .collect();
        }
        Ok(())
    }
    /// The rider leaves the bike as an articulated body. He keeps most of
    /// the speed the bike had before whatever stopped it, each limb keeps the
    /// motion it had in the riding pose, and the kind of wreck sets which way
    /// he is thrown. None when this rider's skeleton cannot be articulated;
    /// the rigid stand-in is used then.
    fn throw_rider(&self, dt: f32) -> Option<crate::ragdoll::Ragdoll> {
        use crate::wreck::Kind;
        let nodes = &self.model.rider.nodes;
        let b = self.body.basis();
        let p = self.body.origin(&self.body_config);
        let world = [
            b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1],
            b[2][2], 0., p[0], p[1], p[2], 1.,
        ];
        let local = match &self.normal_pose {
            Some(pose) => pose.world_matrices.clone(),
            None => self
                .model
                .rider_pose(&self.motion_name, self.motion_time)
                .ok()?,
        };
        let pose: Vec<M> = local.iter().map(|m| mul(&world, m)).collect();
        let previous = (self.rider_last.len() == pose.len() && dt > 0.)
            .then_some((self.rider_last.as_slice(), dt));
        // An impact stops the bike within the step; the rider carries on.
        let carried = if dot(self.entry_velocity, self.entry_velocity)
            > dot(self.body.linear_velocity, self.body.linear_velocity)
        {
            add(
                scale(self.entry_velocity, 0.8),
                scale(self.body.linear_velocity, 0.2),
            )
        } else {
            self.body.linear_velocity
        };
        let flat = |v: V| {
            let l = (v[0] * v[0] + v[2] * v[2]).sqrt().max(1e-6);
            [v[0] / l, 0., v[2] / l]
        };
        let (forward, side) = (flat(b[2]), flat(b[0]));
        // Enough to clear the bike; the carried speed does the rest.
        let kick = match self.frame.wreck.kind {
            Some(Kind::Front) => add(scale(forward, 3.), [0., 9., 0.]),
            Some(Kind::Back) => add(scale(forward, -5.), [0., 4., 0.]),
            Some(Kind::Left) => add(scale(side, -3.), [0., 5., 0.]),
            Some(Kind::Right) => add(scale(side, 3.), [0., 5., 0.]),
            _ => [0., 6., 0.],
        };
        let launch = add(carried, kick);
        let speed = dot(launch, launch).sqrt();
        let launch = scale(launch, 1. / (speed / 95.).max(1.));
        crate::ragdoll::Ragdoll::new(nodes, &pose, previous, launch)
    }
    /// Wrecked bikes keep normal mass, gravity and roll freedom and rest on
    /// the wreck whiskers and tires (source MX class 0 applies no wreck
    /// damping/inertia). Respawn follows the source 0.5 s settle plus
    /// MaxWreckTime; the unrecovered rider-settled gate is substituted by the
    /// bike coming to rest (or 6 s). Returns true after recovery.
    fn wreck_body_step(&mut self, terrain: &Terrain, h: f32) -> Result<bool> {
        if self.frame.wreck.phase == crate::wreck::Phase::Riding {
            return Ok(false);
        }
        self.wreck_time += h;
        self.frame.wreck.timer = self.wreck_time;
        // Authored safety: never let a wreck sink through the terrain.
        let p = self.body.center;
        if let Some(hit) = terrain.contact(p[0], p[2])?
            && p[1] < hit.height
        {
            self.body.center[1] = hit.height;
            self.body.linear_velocity[1] = self.body.linear_velocity[1].max(0.);
        }
        let speed = dot(self.body.linear_velocity, self.body.linear_velocity).sqrt();
        // With an articulated rider, wait for him to stop tumbling too.
        let tumbling = self.ragdoll.is_some()
            && self
                .rider_body
                .as_ref()
                .is_some_and(|(_, v)| dot(*v, *v) > 36.);
        if self.wreck_time > 0.5 + self.params.max_wreck_time
            && ((speed < 3. && !tumbling) || self.wreck_time > 6.)
        {
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
        self.frame.moving_support = d.tires.iter().any(|t| t.grounded && t.ground != [0.; 3]);
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
        self.frame.lean = self.handling.lean;
        self.frame.rear_slip = self.handling.slip;
        self.frame.rear_slide = self.handling.slide;
        let riding = self.frame.wreck.phase == crate::wreck::Phase::Riding;
        if riding
            && d.tires.iter().all(|t| t.grounded)
            && self.frame.crash_impact_speed == 0.
            && self.forward_speed().abs() > 3.
        {
            let b = self.body.basis();
            self.safe_origin = self.body.origin(&self.body_config);
            self.safe_yaw = b[2][0].atan2_d(b[2][2]);
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
    /// What the rider's turn sweep follows. The source sweeps its riding
    /// clips with the crab angle, which its tires always carry in a corner;
    /// the single-track handling corners without crabbing, so the lean
    /// stands in for it and a real slide adds to it.
    fn pose_crab(&self) -> f32 {
        let grounded = self.frame.grounded.iter().any(|g| *g);
        if self.classic || !grounded {
            return self.drive.crab;
        }
        let max = self.params.max_crossup;
        let lean = (self.handling.lean / 0.9).clamp(-1., 1.);
        (self.drive.crab - 0.62 * max * lean).clamp(-max, max)
    }
    /// Rider load at the pelvis box: (gravity − point acceleration) in the bike
    /// frame minus the level 1 g, from the box's world track (which includes
    /// the kinematic bank). Cleared by resets and wrecks.
    fn pelvis_load(&mut self, dt: f32) -> V {
        let point = add(
            self.body.origin(&self.body_config),
            dynamics::transform(self.body.basis(), self.model.pelvis_box),
        );
        let riding = self.frame.wreck.phase == crate::wreck::Phase::Riding;
        let load = match self.pelvis_history {
            [Some(p1), Some(p2)] if riding && dt > 0. => {
                let a = scale(add(sub(point, scale(p1, 2.)), p2), 1. / (dt * dt));
                let g = [0., -32.2, 0.];
                let local = dynamics::inverse_transform(self.body.basis(), sub(g, a));
                sub(local, [0., -32.2, 0.])
            }
            _ => [0.; 3],
        };
        self.pelvis_history = if riding {
            [Some(point), self.pelvis_history[0]]
        } else {
            [None; 2]
        };
        load
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
            self.stunt_clip = None;
            self.stunt_weight = 0.;
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
                self.stunt_clip = Some((self.motion_name.clone(), self.motion_time));
            }
            self.stunt_name = Some(if flip { "Backflip" } else { "Three Sixty" }.into());
            return Ok(());
        }
        if self.active_stunt.is_none() {
            self.stunt_name = None;
        }
        let matches = |s: &StuntDefinition| {
            s.mapping.len() == mapping.len() && s.mapping.chars().all(|c| mapping.contains(c))
        };
        if air
            && self.active_stunt.is_none()
            && modifiers != 0
            // Authored: only once clear of the ground, with air enough for
            // the quickest version of the trick.
            && (self.classic || (self.drive.airtime > 0.12 && self.drive.landing_in > 0.45))
            && let Some(index) = self
                .model
                .stunts
                .iter()
                .position(|s| matches(s) && s.duration > 0.)
        {
            self.active_stunt = Some(index);
            self.stunt_timer = 0.;
            self.stunt_hold = true;
        }
        if let Some(index) = self.active_stunt {
            let s = &self.model.stunts[index];
            self.stunt_hold = air && matches(s);
            let rate = if self.classic {
                1.
            } else {
                // Fit what is left of the trick into the air that is left: a
                // tap on a short jump is a quick trick, and a released hold
                // is back in the seat before the predicted landing.
                let left = (s.duration - self.stunt_timer).max(0.);
                (left / (self.drive.landing_in - 0.12).max(0.05)).clamp(STUNT_RATE, 4.)
            };
            self.stunt_timer = (self.stunt_timer + rate * dt).min(if self.stunt_hold {
                s.hold
            } else {
                s.duration
            });
            self.stunt_name = Some(s.display.clone());
            if !air {
                let exposed = if self.classic {
                    self.stunt_timer < s.safe
                } else {
                    // Authored: a wreck only when the rider is still well out
                    // of the seat, not for a trick that had barely started or
                    // was most of the way back.
                    let back = (self.stunt_timer - s.hold) / (s.safe - s.hold).max(0.05);
                    self.stunt_timer > 0.3 * s.hold && back < 0.55
                };
                if exposed {
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
                    self.stunt_clip = Some((self.motion_name.clone(), self.motion_time));
                }
                if !self.stunt_hold && self.stunt_timer >= s.duration {
                    self.active_stunt = None;
                    self.stunt_name = None;
                }
            }
        }
        Ok(())
    }
    /// Whether the rider is far enough into a trick to wreck if the bike
    /// touches down while it is held.
    fn stunt_exposed(&self) -> bool {
        self.active_stunt
            .is_some_and(|index| self.stunt_timer > 0.3 * self.model.stunts[index].hold)
    }
    /// While airborne: seconds until the bike's ballistic path meets the
    /// terrain (sampled thirty times a second for up to four seconds, 9 when
    /// it never does) and the slope of the ground there along the travel.
    /// Tricks finish before it, and the bike comes down matched to it.
    fn predict_landing(&mut self, terrain: &Terrain) -> Result<()> {
        self.drive.landing_in = 9.;
        self.drive.landing_slope = 0.;
        if self.classic || self.drive.tires.iter().any(|t| t.grounded) {
            return Ok(());
        }
        let gravity = 32.2 * self.params.gravity_scale;
        let (p, v) = (self.body.center, self.body.linear_velocity);
        let clearance = self.body_config.center_of_mass[1]
            - (self.model.wheel_rest[1][1] - self.params.tires[1].radius);
        for i in 1..=120 {
            let t = i as f32 / 30.;
            let y = p[1] + v[1] * t - 0.5 * gravity * t * t;
            let (x, z) = (p[0] + v[0] * t, p[2] + v[2] * t);
            let above = terrain
                .contact(x, z)?
                .is_some_and(|hit| y - clearance > hit.height);
            if !above {
                self.drive.landing_in = t;
                let level = (v[0] * v[0] + v[2] * v[2]).sqrt();
                if level > 1. {
                    let reach = 3. / level;
                    let ahead = terrain.contact(x + v[0] * reach, z + v[2] * reach)?;
                    let behind = terrain.contact(x - v[0] * reach, z - v[2] * reach)?;
                    if let (Some(ahead), Some(behind)) = (ahead, behind) {
                        self.drive.landing_slope = (ahead.height - behind.height).atan2_d(6.);
                    }
                }
                break;
            }
        }
        Ok(())
    }
    pub fn matrices(&self) -> Vec<M> {
        let (steering, angles, compression) = self.linkage();
        self.model.pose(steering, angles, compression)
    }
    /// What [`Model::pose`] draws the bike from: steering, wheel spin and
    /// suspension (multiplayer poses carry these).
    pub fn linkage(&self) -> (f32, [f32; 2], [f32; 2]) {
        let steering = if self.frame.wreck.phase == crate::wreck::Phase::Riding {
            self.visual_steer
        } else {
            self.steer + self.frame.crossup
        };
        (steering, self.angles, self.compression)
    }
    pub fn rider_matrices(&self) -> Result<Vec<M>> {
        if let (Some(ragdoll), Some((world, _))) = (&self.ragdoll, &self.rider_body) {
            // World-space segments, expressed about the pelvis the camera,
            // sound and triggers follow (`rider_world`).
            return Ok(ragdoll
                .matrices()
                .into_iter()
                .map(|mut m| {
                    for k in 0..3 {
                        m[12 + k] -= world[12 + k];
                    }
                    m
                })
                .collect());
        }
        // The riding pose carries any trick clip laid over it.
        if self.frame.wreck.phase == crate::wreck::Phase::Riding
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
