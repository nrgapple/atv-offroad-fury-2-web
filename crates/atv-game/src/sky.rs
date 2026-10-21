//! Scene sky cube (SCN CubeFile): tile layout (atv_core::sky, 00124010) and
//! IPUM tiles. Enhanced presets swap in the deblocked/smoothed decode.
use crate::{Result, venue::Image};
use atv_core::{ipu, sky};

pub struct Tile {
    pub positions: [[f32; 3]; 4],
    pub image: Image,
    /// Deblocked, denoised and smoothly upsampled original tile (Enhanced/High).
    /// Only decoded when requested (`load(.., true)`).
    pub smooth: Option<Vec<u8>>,
}

pub struct Sky {
    pub origin: [f32; 3],
    pub tiles: Vec<Tile>,
}

pub fn load(bytes: &[u8], smooth: bool) -> Result<Sky> {
    let layout = sky::layout(bytes)?;
    let mut tiles = Vec::with_capacity(layout.tiles.len());
    for tile in &layout.tiles {
        let data = bytes
            .get(tile.offset..tile.offset + tile.bytes)
            .ok_or("sky tile exceeds cube")?;
        let decoded = ipu::decode_ipum(data)?;
        if decoded.rgba.len() != (tile.size * tile.size * 4) as usize {
            return Err("sky tile size mismatch".into());
        }
        let smooth = smooth.then(|| decoded.smoothed_rgba());
        if smooth
            .as_ref()
            .is_some_and(|s| s.len() != decoded.rgba.len())
        {
            return Err("smoothed sky tile size mismatch".into());
        }
        tiles.push(Tile {
            positions: tile.positions,
            image: Image {
                width: tile.size,
                height: tile.size,
                rgba: decoded.rgba,
            },
            smooth,
        });
    }
    Ok(Sky {
        origin: layout.origin,
        tiles,
    })
}
