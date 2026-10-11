//! Exact-build format recovery. No game data or executable code is embedded.
pub mod animation;
pub mod audio;
pub mod camera;
pub mod collision;
pub mod contact;
pub mod drivetrain;
pub mod elf;
pub mod engine_audio;
pub mod grip;
pub mod ipu;
pub mod iso;
pub mod materials;
pub mod mesh;
pub mod music;
pub mod obstacle;
pub mod physics_data;
pub mod quad_pose;
pub mod resource;
pub mod rider_control;
pub mod scene;
pub mod sky;
pub mod terrain;
pub mod texture;

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

pub mod dynamics;

pub mod vehicle;
