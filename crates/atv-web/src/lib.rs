//! Small single-threaded Wasm ABI. Buffers are Rust-owned, initialized, and bounded.
//! JS copies into input_ptr only after input_resize and copies output before another call.
use atv_core::{
    Result, animation, audio, elf,
    engine_audio::{EngineAudioConfig, EngineAudioState, OriginalRandom, sample_index},
    iso,
    materials::{self, TerrainMaterial},
    mesh,
    physics_data::PhysicsData,
    resource::Resource,
    terrain, texture,
    vehicle::{Vehicle, VehicleInput},
};
use std::cell::RefCell;

#[derive(Default)]
struct Buffers {
    input: Vec<u8>,
    output: Vec<u8>,
}
thread_local! { static BUFFERS: RefCell<Buffers> = RefCell::default(); }

#[derive(Default)]
struct Simulation {
    terrain: Option<terrain::Terrain>,
    physics: Option<PhysicsData>,
    mesh: Option<mesh::Mesh>,
    materials: Option<[TerrainMaterial; 8]>,
    vehicle: Option<Vehicle>,
    rider_mesh: Option<mesh::Mesh>,
    rider_animation: Option<animation::Animation>,
    chassis_points: Option<Vec<[f32; 3]>>,
    quad_pose: Option<atv_core::quad_pose::QuadPose>,
    obstacle_rays: Option<std::rc::Rc<atv_core::collision::CollisionRayTree>>,
    obstacle_meshes: Vec<std::rc::Rc<atv_core::collision::CollisionMesh>>,
    obstacles: Option<Vec<atv_core::obstacle::StaticObstacle>>,
    camera: Option<atv_core::camera::FollowCamera>,
}
thread_local! { static SIMULATION: RefCell<Simulation> = RefCell::default(); }

struct EnginePlayback {
    config: EngineAudioConfig,
    state: EngineAudioState,
    previous: [usize; 10],
    random: OriginalRandom,
}
thread_local! { static ENGINE_AUDIO: RefCell<Option<EnginePlayback>> = const { RefCell::new(None) }; }

struct MusicPlayback {
    library: atv_core::music::MusicLibrary,
    shuffle: atv_core::music::MusicShuffle,
    // Initial RTC seed/global call order remain outside this reconstruction.
    random: OriginalRandom,
}
thread_local! {
    static MUSIC: RefCell<Option<MusicPlayback>> = const { RefCell::new(None) };
    static MUSIC_STREAM: RefCell<Option<audio::VagStream>> = const { RefCell::new(None) };
}
fn music(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    if op == 42 {
        let stream = audio::VagStream::new(input, index)?;
        let result = serde_json::to_vec(&stream.info).map_err(|e| e.to_string())?;
        MUSIC_STREAM.with_borrow_mut(|slot| *slot = Some(stream));
        return Ok(result);
    }
    if op == 43 {
        return MUSIC_STREAM.with_borrow_mut(|slot| {
            let samples = slot
                .as_mut()
                .ok_or("initialize music stream first")?
                .decode_chunk(input)?;
            Ok(samples.iter().flat_map(|s| s.to_le_bytes()).collect())
        });
    }
    MUSIC.with_borrow_mut(|slot| {
        if op == 40 {
            let library = atv_core::music::MusicLibrary::parse(input)?;
            let bytes = serde_json::to_vec(&library).map_err(|e| e.to_string())?;
            let shuffle = atv_core::music::MusicShuffle::new(&library);
            *slot = Some(MusicPlayback {
                library,
                shuffle,
                random: OriginalRandom::with_seed(1),
            });
            Ok(bytes)
        } else {
            let music = slot.as_mut().ok_or("load original music catalog first")?;
            let song = music.shuffle.next_song(&mut music.random)?;
            serde_json::to_vec(&song.map(|i| &music.library.songs[i])).map_err(|e| e.to_string())
        }
    })
}

fn engine_audio(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    ENGINE_AUDIO.with_borrow_mut(|slot| {
        if op == 32 {
            let config = EngineAudioConfig::parse(input, index as u32)?;
            let bytes = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
            *slot = Some(EnginePlayback { config, state: EngineAudioState::default(), previous: [usize::MAX; 10], random: OriginalRandom::with_seed(1) });
            return Ok(bytes);
        }
        let player = slot.as_mut().ok_or("load original engine audio table first")?;
        if op == 34 {
            player.state = EngineAudioState::default();
            player.previous = [usize::MAX; 10];
            return Ok(b"null".to_vec());
        }
        let value: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        let command = player.state.step(&player.config, json_number(&value,"dt")?,
            json_number(&value,"throttle")?, value.get("grounded").and_then(serde_json::Value::as_bool).ok_or("missing audio grounded")?,
            json_number(&value,"speed")?, atv_core::engine_audio::PlaybackPosition {duration: json_number(&value,"duration")?, playhead: json_number(&value,"playhead")?})?;
        if let Some(command) = command {
            let sound = command.sound as usize;
            let count = player.config.samples[sound].len();
            if count == 0 { return Ok(b"null".to_vec()); }
            let choice = if matches!(sound, 2..=6) {
                let random = if count >= 2 { player.random.next_u31() } else { 0 };
                sample_index(count, &mut player.previous[sound], random)?
            } else { 0 };
            serde_json::to_vec(&serde_json::json!({"command": command, "sample": player.config.samples[sound][choice]})).map_err(|e| e.to_string())
        } else { Ok(b"null".to_vec()) }
    })
}

fn json_number(value: &serde_json::Value, key: &str) -> Result<f32> {
    let number = value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| format!("missing simulation number {key}"))? as f32;
    if !number.is_finite() {
        return Err(format!("nonfinite simulation number {key}"));
    }
    Ok(number)
}

fn simulation_frame(sim: &Simulation, frame: &atv_core::vehicle::VehicleFrame) -> Result<Vec<u8>> {
    let matrices = sim
        .quad_pose
        .as_ref()
        .ok_or("load original quad linkage first")?
        .sample(
            frame.steering_angle,
            frame.wheel_angles,
            frame.wheel_compression,
        )?;
    let mut value = serde_json::to_value(frame).map_err(|error| error.to_string())?;
    value["quad_node_matrices"] =
        serde_json::to_value(matrices).map_err(|error| error.to_string())?;
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}

fn simulation(input: &[u8], op: u32) -> Result<Vec<u8>> {
    SIMULATION.with_borrow_mut(|sim| {
        match op {
            20 => {
                let terrain = terrain::decode_tr2(input)?;
                sim.terrain = Some(terrain);
                sim.vehicle = None;
                sim.obstacle_rays = None;
                sim.obstacle_meshes.clear();
                sim.obstacles = None;
            }
            21 => {
                let physics = PhysicsData::parse(input)?;
                sim.physics = Some(physics);
                sim.vehicle = None;
            }
            22 => {
                let mesh = mesh::decode_psg(input)?;
                sim.quad_pose = Some(atv_core::quad_pose::QuadPose::new(&mesh)?);
                sim.mesh = Some(mesh);
                sim.chassis_points = None;
                sim.vehicle = None;
            }
            23 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|error| error.to_string())?;
                let array = value
                    .get("origin")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("missing simulation origin")?;
                if array.len() != 3 {
                    return Err("simulation origin needs three coordinates".into());
                }
                let mut origin = [0.; 3];
                for (index, number) in array.iter().enumerate() {
                    origin[index] = number.as_f64().ok_or("invalid origin coordinate")? as f32;
                }
                if !origin.iter().all(|number| number.is_finite()) {
                    return Err("nonfinite simulation origin".into());
                }
                let mut vehicle = Vehicle::new(
                    sim.mesh.as_ref().ok_or("load original mesh first")?,
                    sim.physics.as_ref().ok_or("load original physics first")?,
                    origin,
                    json_number(&value, "yaw")?,
                )?;
                vehicle.set_materials(
                    sim.materials
                        .ok_or("load original terrain materials first")?,
                );
                vehicle.set_chassis_points(
                    sim.chassis_points
                        .as_ref()
                        .ok_or("load original chassis collision first")?,
                )?;
                vehicle.set_obstacles(
                    sim.obstacle_rays
                        .as_ref()
                        .ok_or("load original actor collision rays first")?
                        .clone(),
                    sim.obstacles
                        .as_ref()
                        .ok_or("load original scene collision first")?
                        .clone(),
                )?;
                let result = simulation_frame(sim, &vehicle.frame())?;
                sim.vehicle = Some(vehicle);
                return Ok(result);
            }
            24 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|error| error.to_string())?;
                let raw = value.get("input").ok_or("missing simulation input")?;
                let input = VehicleInput {
                    throttle: json_number(raw, "throttle")?,
                    brake: json_number(raw, "brake")?,
                    steer: json_number(raw, "steer")?,
                    lean: json_number(raw, "lean")?,
                    reverse: raw
                        .get("reverse")
                        .and_then(serde_json::Value::as_bool)
                        .ok_or("missing reverse input")?,
                };
                let terrain = sim.terrain.as_ref().ok_or("load original terrain first")?;
                let frame = sim.vehicle.as_mut().ok_or("start simulation first")?.step(
                    terrain,
                    input,
                    json_number(&value, "dt")?,
                )?;
                return simulation_frame(sim, &frame);
            }
            26 => {
                let materials = materials::terrain_materials(input)?;
                sim.materials = Some(materials);
                sim.vehicle = None;
            }
            27 => {
                let mesh = mesh::decode_psg(input)?;
                let result = serde_json::to_vec(&mesh).map_err(|error| error.to_string())?;
                sim.rider_mesh = Some(mesh);
                sim.rider_animation = None;
                return Ok(result);
            }
            28 => {
                let clip = animation::decode_mot(input)?;
                sim.rider_animation = Some(clip);
            }
            29 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|error| error.to_string())?;
                let mesh = sim
                    .rider_mesh
                    .as_ref()
                    .ok_or("load original rider mesh first")?;
                let clip = sim
                    .rider_animation
                    .as_ref()
                    .ok_or("load original rider motion first")?;
                let pose = clip.sample_pose(
                    &mesh.nodes,
                    json_number(&value, "time")?,
                    animation::Sampling::default(),
                )?;
                return serde_json::to_vec(&pose).map_err(|error| error.to_string());
            }
            44 => {
                let tree = atv_core::collision::decode_ray_tree(input)?;
                let points = tree.chassis_points()?;
                let result = serde_json::to_vec(
                    &serde_json::json!({"rays": tree.rays.len(), "points": points.len()}),
                )
                .map_err(|error| error.to_string())?;
                sim.chassis_points = Some(points);
                sim.vehicle = None;
                return Ok(result);
            }
            45 => {
                let presets = atv_core::camera::presets(input, 0)?;
                let result = serde_json::to_vec(&presets).map_err(|e| e.to_string())?;
                //00319c10 ->003007b0 supplies70MPH and the original ft/s conversion.
                sim.camera = Some(atv_core::camera::FollowCamera::new(
                    presets,
                    70. * f32::from_bits(0x3fbbbb84),
                )?);
                return Ok(result);
            }
            46 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|e| e.to_string())?;
                let vehicle = sim
                    .vehicle
                    .as_ref()
                    .ok_or("start simulation first")?
                    .frame();
                let basis = atv_core::dynamics::basis(vehicle.orientation);
                let velocity = vehicle.linear_velocity;
                let local = atv_core::dynamics::inverse_transform(basis, velocity);
                let terrain = sim.terrain.as_ref().ok_or("load original terrain first")?;
                let frame = sim
                    .camera
                    .as_mut()
                    .ok_or("load original camera presets first")?
                    .step(
                        atv_core::camera::FollowInput {
                            origin: vehicle.origin,
                            forward: basis[2],
                            velocity,
                            speed: ((velocity[0] * velocity[0] + velocity[1] * velocity[1])
                                + velocity[2] * velocity[2])
                                .sqrt(),
                            local_forward_speed: local[2],
                            yaw_rate: vehicle.local_angular_velocity[1],
                            wheel_contact: vehicle.grounded.iter().any(|v| *v),
                            orbit: json_number(&value, "orbit")?,
                        },
                        json_number(&value, "dt")?,
                        |point| {
                            Ok(terrain
                                .contact(point[0], point[2])?
                                .map_or(point[1], |contact| contact.height))
                        },
                    )?;
                return serde_json::to_vec(&frame).map_err(|e| e.to_string());
            }
            47 => {
                let value: serde_json::Value =
                    serde_json::from_slice(input).map_err(|e| e.to_string())?;
                let camera = sim
                    .camera
                    .as_mut()
                    .ok_or("load original camera presets first")?;
                if value
                    .get("cycle")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                {
                    camera.cycle();
                } else {
                    camera.reset();
                }
            }
            _ => return Err("unknown simulation operation".into()),
        }
        Ok(b"{}".to_vec())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn input_resize(len: usize) -> u32 {
    if len > 128 * 1024 * 1024 {
        return 0;
    }
    BUFFERS.with_borrow_mut(|b| {
        b.input.resize(len, 0);
    });
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn input_ptr() -> *const u8 {
    BUFFERS.with_borrow(|b| b.input.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn output_ptr() -> *const u8 {
    BUFFERS.with_borrow(|b| b.output.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn output_len() -> usize {
    BUFFERS.with_borrow(|b| b.output.len())
}

fn obstacle_input(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    SIMULATION.with_borrow_mut(|sim| {
        if op == 48 {
            if index == 0 {
                let rays = atv_core::collision::decode_ray_tree(input)?;
                let identity = [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.];
                if rays.transform != identity { return Err("unsupported actor collision header transform".into()); }
                let result = serde_json::json!({"rays":rays.rays.len()});
                sim.obstacle_rays = Some(std::rc::Rc::new(rays));
                sim.vehicle = None;
                return serde_json::to_vec(&result).map_err(|e| e.to_string());
            }
            if sim.obstacle_meshes.len() >= 4096 { return Err("too many collision meshes".into()); }
            let mesh = atv_core::collision::decode_col(input)?;
            if mesh.transform != [1.,0.,0.,0.,1.,0.,0.,0.,1.,0.,0.,0.] {
                return Err("unsupported static collision header transform".into());
            }
            let result = serde_json::json!({"mesh":sim.obstacle_meshes.len(),"triangles":mesh.triangles.len()});
            sim.obstacle_meshes.push(std::rc::Rc::new(mesh));
            sim.vehicle = None;
            return serde_json::to_vec(&result).map_err(|e| e.to_string());
        }
        let source: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        let placements = source.as_array().ok_or("collision placements need an array")?;
        if placements.len() > 4096 { return Err("too many collision placements".into()); }
        let mut obstacles = Vec::with_capacity(placements.len());
        for placement in placements {
            let index = placement.get("mesh").and_then(serde_json::Value::as_u64).ok_or("missing collision mesh index")?;
            let mesh = sim.obstacle_meshes.get(usize::try_from(index).map_err(|_| "invalid collision mesh index")?).ok_or("collision mesh index out of bounds")?.clone();
            let values = placement.get("world_matrix").and_then(serde_json::Value::as_array).ok_or("missing collision matrix")?;
            if values.len() != 16 { return Err("collision matrix needs 16 elements".into()); }
            let mut world_matrix = [0.;16];
            for (output, value) in world_matrix.iter_mut().zip(values) {
                *output = value.as_f64().ok_or("invalid collision matrix element")? as f32;
                if !output.is_finite() { return Err("nonfinite collision matrix".into()); }
            }
            obstacles.push(atv_core::obstacle::StaticObstacle { mesh, world_matrix });
        }
        let result = serde_json::json!({"instances":obstacles.len(),"meshes":sim.obstacle_meshes.len()});
        sim.obstacles = Some(obstacles);
        sim.vehicle = None;
        serde_json::to_vec(&result).map_err(|e| e.to_string())
    })
}

/// Bounded transport batch. Each step still runs the original vehicle operation,
/// then the original camera operation, with the same fixed 1/60 second input.
/// Retain intermediate frames so consumers such as audio see every step.
fn simulation_batch(input: &[u8]) -> Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let count = value
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .filter(|count| (1..=4).contains(count))
        .ok_or("simulation batch count must be 1 through 4")?;
    let driving = value.get("input").ok_or("missing simulation input")?;
    let orbit = json_number(&value, "orbit")?;
    let step = serde_json::to_vec(&serde_json::json!({"dt":1.0/60.0,"input":driving}))
        .map_err(|e| e.to_string())?;
    let camera = serde_json::to_vec(&serde_json::json!({"dt":1.0/60.0,"orbit":orbit}))
        .map_err(|e| e.to_string())?;
    if value
        .get("cycle")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        simulation(br#"{"cycle":true}"#, 47)?;
    }
    let mut frames = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let frame: serde_json::Value =
            serde_json::from_slice(&simulation(&step, 24)?).map_err(|e| e.to_string())?;
        let camera: serde_json::Value =
            serde_json::from_slice(&simulation(&camera, 46)?).map_err(|e| e.to_string())?;
        frames.push(serde_json::json!({"frame":frame,"camera":camera}));
    }
    serde_json::to_vec(&frames).map_err(|e| e.to_string())
}

fn dispatch(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    if op == 50 {
        return simulation_batch(input);
    }
    if (48..=49).contains(&op) {
        return obstacle_input(input, op, index);
    }
    if (40..=43).contains(&op) {
        return music(input, op, index);
    }
    if (32..=34).contains(&op) {
        return engine_audio(input, op, index);
    }
    if (20..=29).contains(&op) || (44..=47).contains(&op) {
        return simulation(input, op);
    }
    let json = match op {
        0 => serde_json::to_vec(&iso::root(input)?),
        1 => serde_json::to_vec(&iso::directory(input)?),
        2 => serde_json::to_vec(&Resource::parse(input)?.members),
        3 => return Resource::parse(input)?.extract(index),
        4 => return Ok(audio::decode_vag(input)?.wav()),
        5 => serde_json::to_vec(&audio::decode_vag(input)?.info),
        6 => serde_json::to_vec(&elf::inspect(input)?),
        7 => serde_json::to_vec(&texture::decode_pst(input)?.info),
        8 => return Ok(texture::decode_pst(input)?.rgba),
        9 => {
            let terrain = terrain::decode_tr2(input)?;
            serde_json::to_vec(&serde_json::json!({
                "width_cells": terrain.width_cells,
                "depth_cells": terrain.depth_cells,
                "spacing": terrain.spacing,
                "tiles": terrain.tiles.len(),
                "leaves": terrain.leaves.len(),
            }))
        }
        10 => {
            let heights = terrain::decode_tr2(input)?.heightfield()?;
            return Ok(heights.iter().flat_map(|h| h.to_le_bytes()).collect());
        }
        11 => serde_json::to_vec(&mesh::decode_psg(input)?),
        30 => serde_json::to_vec(&terrain::texture_layout(input)?),
        35 => return Ok(atv_core::ipu::decode_ipum(input)?.rgba),
        36 => serde_json::to_vec(&atv_core::ipu::decode_ipum(input)?.info),
        37 => serde_json::to_vec(&atv_core::scene::parse(input)?),
        38 => serde_json::to_vec(&atv_core::sky::layout(input)?),
        31 => {
            let layout = terrain::texture_layout(input)?;
            let record = layout
                .records
                .get(index)
                .ok_or("terrain texture index exceeds table")?;
            let mip = record.mips.last().ok_or("terrain texture has no mip")?;
            return Ok(input[mip.offset..mip.offset + mip.bytes].to_vec());
        }
        _ => return Err("unknown operation".into()),
    };
    json.map_err(|e| e.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn run(op: u32, index: usize) -> u32 {
    BUFFERS.with_borrow_mut(|b| match dispatch(&b.input, op, index) {
        Ok(out) => {
            b.output = out;
            1
        }
        Err(e) => {
            b.output = e.into_bytes();
            0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_rejects_unbounded_or_fractional_step_counts_before_simulation() {
        for count in ["0", "5", "1.5", "-1", "18446744073709551615"] {
            let input = format!("{{\"count\":{count}}}");
            assert_eq!(
                simulation_batch(input.as_bytes()).unwrap_err(),
                "simulation batch count must be 1 through 4"
            );
        }
    }
}
