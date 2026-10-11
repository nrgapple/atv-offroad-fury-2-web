//! CUB texture layout and face grid, recovered from 00124010,00123450,
//! 00123790 and00125110. The observed single-cube IPUM variant is supported.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct Tile {
    pub face: u32,
    pub index: u32,
    pub size: u32,
    pub offset: usize,
    pub bytes: usize,
    pub positions: [[f32; 3]; 4],
}
#[derive(Debug, Serialize)]
pub struct Sky {
    pub origin: [f32; 3],
    pub look: [f32; 3],
    pub up: [f32; 3],
    pub tiles: Vec<Tile>,
}
/// Original cube grid: four subdivisions spanning -25600..25600 per face.
pub fn grid_point(face: u32, column: u32, row: u32) -> Result<[f32; 3]> {
    if column > 4 || row > 4 {
        return Err("sky grid coordinate outside face".into());
    }
    let x = (column as f32 * 128. - 256.) * 100.;
    let y = (256. - row as f32 * 128.) * 100.;
    Ok(match face {
        0 => [x, y, 25600.],
        1 => [-25600., y, x],
        2 => [-x, y, -25600.],
        3 => [25600., y, -x],
        4 => [x, 25600., -y],
        5 => [x, -25600., y],
        _ => return Err("sky face exceeds six-face cube".into()),
    })
}
pub fn layout(data: &[u8]) -> Result<Sky> {
    if data.len() > 32 * 1024 * 1024 || u32le(data, 0)? != 1 || u32le(data, 4)? != 1 {
        return Err("unsupported CUB palette/cube count".into());
    }
    let palette = u32le(data, 8)? as usize;
    let header = u32le(data, 12)? as usize;
    if palette < 16 || palette >= header {
        return Err("invalid CUB wrapper offsets".into());
    }
    slice(data, header, 80)?;
    if u32le(data, header)? != 32 || u16le(data, header + 4)? != 0 || u16le(data, header + 6)? != 0
    {
        return Err("unsupported CUB format/flags".into());
    }
    let vector = |offset| -> Result<[f32; 3]> {
        let mut values = [0.; 3];
        for (i, v) in values.iter_mut().enumerate() {
            *v = f32::from_bits(u32le(data, offset + i * 4)?);
        }
        if !values.iter().all(|v| v.is_finite()) {
            return Err("nonfinite CUB vector".into());
        }
        Ok(values)
    };
    let mut sky = Sky {
        origin: vector(header + 44)?,
        look: vector(header + 56)?,
        up: vector(header + 68)?,
        tiles: Vec::new(),
    };
    let mut at = header + 80;
    for face in 0..6 {
        let mask = u16le(data, header + 8 + face * 2)?;
        let sizes = u32le(data, header + 20 + face * 4)?;
        for index in 0..16 {
            if mask & (1 << index) == 0 {
                continue;
            }
            //001231b0 maps 2-bit codes0..3 to512,64,128,256.
            let size = [512u32, 64, 128, 256][((sizes >> (index * 2)) & 3) as usize];
            if size != 128 {
                return Err("unsupported CUB tile size".into());
            }
            let expected = [0u32, 32, 0, 16, 4096, 4];
            for (i, value) in expected.iter().enumerate() {
                if u32le(data, at + i * 4)? != *value {
                    return Err("unsupported CUB compressed tile header".into());
                }
            }
            let levels = (size / 16).ilog2() as usize + 1;
            at += 24;
            slice(data, at, (levels + 1) * 4)?;
            at += (levels + 1) * 4; // Streaming mip offset slots; original loader ignores values.
            let mut selected = (0, 0);
            for level in 0..levels {
                let bytes = u32le(data, at)? as usize;
                at += 4;
                let image = slice(data, at, bytes)?;
                if bytes < 16
                    || &image[..4] != b"ipum"
                    || u32le(image, 4)? as usize + 8 != bytes
                    || u16le(image, 8)? as u32 != 16 << level
                    || u16le(image, 10)? as u32 != 16 << level
                {
                    return Err("invalid CUB IPUM mip extent/dimensions".into());
                }
                selected = (at, bytes);
                at += bytes;
            }
            let (x, y) = (index as u32 % 4, index as u32 / 4);
            sky.tiles.push(Tile {
                face: face as u32,
                index: index as u32,
                size,
                offset: selected.0,
                bytes: selected.1,
                positions: [
                    grid_point(face as u32, x, y)?,
                    grid_point(face as u32, x + 1, y)?,
                    grid_point(face as u32, x, y + 1)?,
                    grid_point(face as u32, x + 1, y + 1)?,
                ],
            });
        }
    }
    if at != data.len() {
        return Err("CUB trailing or missing tile data".into());
    }
    Ok(sky)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cube_faces_meet_with_original_orientation() {
        assert_eq!(grid_point(0, 0, 0).unwrap(), grid_point(1, 4, 0).unwrap());
        assert_eq!(grid_point(0, 4, 0).unwrap(), grid_point(3, 0, 0).unwrap());
        assert_eq!(grid_point(0, 0, 0).unwrap(), grid_point(4, 0, 4).unwrap());
        assert_eq!(grid_point(2, 0, 4).unwrap(), grid_point(5, 4, 4).unwrap());
        assert!(grid_point(6, 0, 0).is_err());
        assert!(grid_point(0, 5, 0).is_err());
    }
    #[test]
    fn bounded_single_tile_stream() {
        let mut data = vec![0u8; 96];
        for (offset, value) in [
            (0, 1u32),
            (4, 1),
            (8, 16),
            (12, 16 + 0x10),
            (32, 32),
            (40, 1),
            (52, 2),
        ] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        data.resize(112, 0); // header at32 ends112.
        for value in [0u32, 32, 0, 16, 4096, 4, 0, 0, 0, 0, 0] {
            data.extend(value.to_le_bytes());
        }
        for size in [16u16, 32, 64, 128] {
            data.extend(16u32.to_le_bytes());
            data.extend(b"ipum");
            data.extend(8u32.to_le_bytes());
            data.extend(size.to_le_bytes());
            data.extend(size.to_le_bytes());
            data.extend(1u32.to_le_bytes());
        }
        let sky = layout(&data).unwrap();
        assert_eq!(sky.tiles.len(), 1);
        assert_eq!(sky.tiles[0].size, 128);
        for end in 0..data.len() {
            assert!(layout(&data[..end]).is_err());
        }
    }
}
