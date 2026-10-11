//! Observed 4/8-bit indexed PST layouts with a shared palette and mip levels.
//! Decodes the largest stored level; unknown variants fail explicitly.
use crate::{Result, slice, u32le};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TextureInfo {
    pub width: u32,
    pub height: u32,
}
pub struct Texture {
    pub info: TextureInfo,
    pub rgba: Vec<u8>,
}

pub fn decode_pst(data: &[u8]) -> Result<Texture> {
    let levels = u32le(data, 8)?;
    if u32le(data, 0)? != 1 || levels == 0 || levels > 16 {
        return Err("unsupported PST version/level count".into());
    }
    if data.len() > 128 * 1024 * 1024 {
        return Err("PST exceeds 128 MiB limit".into());
    }
    let indexed8 = match u32le(data, 4)? {
        0 => false,
        1 => true,
        _ => return Err("unsupported PST pixel format".into()),
    };
    let palette_size = if indexed8 { 1024 } else { 64 };
    let palette = slice(data, 12, palette_size)?;
    let mut at = 12 + palette_size;
    let mut selected = (0, 0, 0usize, 0usize);
    for _ in 0..levels {
        // Rectangular loading bars establish height before width. Observed quad
        // mip chains repeat this header followed immediately by that level's pixels.
        let height = u32le(data, at)?;
        let width = u32le(data, at + 4)?;
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || u32le(data, at + 8)? != 0
            || u32le(data, at + 12)? != 0
        {
            return Err("unsupported PST dimensions/flags".into());
        }
        at += 16;
        let count = (width * height) as usize;
        let payload_size = if indexed8 { count } else { count.div_ceil(2) };
        slice(data, at, payload_size)?;
        if count > selected.3 {
            selected = (width, height, at, count);
        }
        at += payload_size;
    }
    if at != data.len() {
        return Err("PST pixel payload size mismatch".into());
    }
    let (width, height, pixel_start, count) = selected;
    let mut rgba = Vec::with_capacity(count * 4);
    for pixel in 0..count {
        let index = if indexed8 {
            data[pixel_start + pixel]
        } else {
            (data[pixel_start + pixel / 2] >> ((pixel % 2) * 4)) & 15
        };
        // CSM1 8-bit CLUT ordering swaps palette index bits 3 and 4.
        let i = usize::from(if indexed8 {
            (index & !24) | ((index & 8) << 1) | ((index & 16) >> 1)
        } else {
            index
        });
        rgba.extend_from_slice(&palette[i * 4..i * 4 + 4]);
    }
    Ok(Texture {
        info: TextureInfo { width, height },
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_order_and_bounds() {
        let mut b = vec![0; 1054];
        for (at, v) in [(0, 1u32), (4, 1), (8, 1), (1036, 2), (1040, 1)] {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[12 + 16 * 4..12 + 16 * 4 + 4].copy_from_slice(&[20, 40, 60, 255]);
        b[1052] = 8;
        assert_eq!(&decode_pst(&b).unwrap().rgba[..4], &[20, 40, 60, 255]);
        assert!(decode_pst(&b[..1053]).is_err());
    }
    #[test]
    fn four_bit_nibbles_and_rectangular_dimensions() {
        let mut b = vec![0; 93];
        for (at, v) in [(0, 1u32), (8, 1), (76, 1), (80, 2)] {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[16..20].copy_from_slice(&[1, 2, 3, 4]);
        b[20..24].copy_from_slice(&[5, 6, 7, 8]);
        b[92] = 0x21;
        let texture = decode_pst(&b).unwrap();
        assert_eq!(texture.info.width, 2);
        assert_eq!(texture.info.height, 1);
        assert_eq!(texture.rgba, [1, 2, 3, 4, 5, 6, 7, 8]);
    }
    #[test]
    fn mip_chain_selects_largest_level_and_checks_entire_extent() {
        let mut b = vec![0; 1036];
        for (at, value) in [(0, 1u32), (4, 1), (8, 2)] {
            b[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        b[12 + 16 * 4..12 + 17 * 4].copy_from_slice(&[20, 40, 60, 255]);
        b[16..20].copy_from_slice(&[1, 2, 3, 4]);
        for (width, height, pixels) in [(1u32, 1u32, vec![1u8]), (2, 2, vec![8, 1, 0, 8])] {
            for value in [height, width, 0, 0] {
                b.extend(value.to_le_bytes());
            }
            b.extend(pixels);
        }
        let texture = decode_pst(&b).unwrap();
        assert_eq!((texture.info.width, texture.info.height), (2, 2));
        assert_eq!(&texture.rgba[..8], &[20, 40, 60, 255, 1, 2, 3, 4]);
        for end in 0..b.len() {
            assert!(decode_pst(&b[..end]).is_err());
        }
        b.push(0);
        assert!(decode_pst(&b).err().unwrap().contains("payload size"));
    }
}
