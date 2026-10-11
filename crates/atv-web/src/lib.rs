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

fn simulation(input: &[u8], op: u32) -> Result<Vec<u8>> {
    SIMULATION.with_borrow_mut(|sim| {
        match op {
            20 => {
                let terrain = terrain::decode_tr2(input)?;
                sim.terrain = Some(terrain);
                sim.vehicle = None;
            }
            21 => {
                let physics = PhysicsData::parse(input)?;
                sim.physics = Some(physics);
                sim.vehicle = None;
            }
            22 => {
                let mesh = mesh::decode_psg(input)?;
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
                let result =
                    serde_json::to_vec(&vehicle.frame()).map_err(|error| error.to_string())?;
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
                return serde_json::to_vec(&frame).map_err(|error| error.to_string());
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

fn dispatch(input: &[u8], op: u32, index: usize) -> Result<Vec<u8>> {
    if (40..=43).contains(&op) {
        return music(input, op, index);
    }
    if (32..=34).contains(&op) {
        return engine_audio(input, op, index);
    }
    if (20..=29).contains(&op) || op == 44 {
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
