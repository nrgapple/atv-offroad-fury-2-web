//! Exact-build format recovery. No game data or executable code is embedded.
pub mod ambient_effects;
pub mod animation;
pub mod arcade_flips;
pub mod audio;
mod authored_text;
pub mod boundary_launch;
pub mod browser_input;
pub mod camera;
pub mod collision;
pub mod contact;
pub mod crash_effects;
pub mod drivetrain;
pub mod echo;
pub mod ecosystem;
pub mod elf;
pub mod engine_audio;
pub mod flip_tricks;
pub mod fog;
pub mod gore;
pub mod grip;
pub mod hud;
pub mod ipu;
pub mod iso;
pub mod materials;
pub mod math;
pub mod menu_preview;
pub mod mesh;
pub mod motion_catalog;
pub mod motorcycle;
pub mod motorcycle_effects;
pub mod motorcycle_tire;
pub mod music;
pub mod obstacle;
pub mod physics_data;
pub mod psr;
pub mod quad_pose;
pub mod quad_sounds;
pub mod ragdoll;
pub mod resource;
pub mod rider_control;
pub mod rider_ik;
pub mod rider_pose;
pub mod scene;
pub mod scene_candidates;
pub mod scene_collision;
pub mod scene_partition;
pub mod scenery_material;
pub mod sky;
pub mod sound_effects;
pub mod spatial_audio;
pub mod spawn;
pub mod stunt;
pub mod stunt_score;
pub mod terrain;
pub mod texture;
pub mod train;
pub mod water;
pub mod water_animation;
pub mod water_failure;
pub mod water_render;
pub mod water_surface;
pub mod weather;

pub type Result<T> = std::result::Result<T, String>;

pub(crate) fn slice(data: &[u8], start: usize, len: usize) -> Result<&[u8]> {
    let end = start.checked_add(len).ok_or("range overflow")?;
    data.get(start..end)
        .ok_or_else(|| format!("truncated range {start:#x}..{end:#x}"))
}

pub(crate) fn u32le(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(data, at, 4)?.try_into().unwrap()))
}

pub(crate) fn u16le(data: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(slice(data, at, 2)?.try_into().unwrap()))
}

pub mod wheel;
pub mod wheel_effects;
pub mod wheel_stop;
pub mod wheel_support;
pub mod world_sounds;
pub mod wreck;

pub mod dynamics;

pub mod ui;
pub mod ui_menu;
pub mod vegetation_collision;
pub mod vegetation_proxy;
pub mod vegetation_ray;
pub mod vegetation_render;
pub mod vehicle;
pub mod vehicle_stats;

pub mod wreck_animation;

pub mod player_models;

pub mod cpu_avoidance;
pub mod race;
pub mod race_ai;
pub mod race_crowd;
pub mod race_director;
pub mod race_fireworks;
pub mod race_gate;
pub mod race_hud;
pub mod race_intro;
pub mod race_roster;
pub mod rider_gear;

pub mod peer_contact;
