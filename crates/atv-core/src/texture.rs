//! Observed 4/8-bit indexed and IPUM-compressed PST layouts with mip levels.
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
    if u32le(data, 4)? == 2 {
        return decode_ipum_levels(data, levels);
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

/// Observed frontend ATM: version, frame count, zero flags, then complete
/// indexed PST images concatenated without a per-image length prefix.
/// Validate every frame before returning one, including the exact final extent.
pub fn decode_atm(data: &[u8], frame: usize) -> Result<(usize, Texture)> {
    let count = u32le(data, 4)? as usize;
    if u32le(data, 0)? != 1
        || u32le(data, 8)? != 0
        || count == 0
        || count > 512
        || frame >= count
        || data.len() > 128 * 1024 * 1024
    {
        return Err("unsupported ATM header/frame".into());
    }
    let mut at = 12;
    let mut selected = None;
    for i in 0..count {
        let start = at;
        let bits = match u32le(data, at + 4)? {
            0 => 4,
            1 => 8,
            _ => return Err("unsupported ATM pixel format".into()),
        };
        let levels = u32le(data, at + 8)?;
        if u32le(data, at)? != 1 || levels == 0 || levels > 16 {
            return Err("invalid ATM PST header".into());
        }
        at += 12 + if bits == 8 { 1024 } else { 64 };
        for _ in 0..levels {
            let height = u32le(data, at)? as usize;
            let width = u32le(data, at + 4)? as usize;
            if width == 0
                || height == 0
                || width > 4096
                || height > 4096
                || u32le(data, at + 8)? != 0
                || u32le(data, at + 12)? != 0
            {
                return Err("invalid ATM frame dimensions/flags".into());
            }
            at += 16;
            let length = (width * height * bits).div_ceil(8);
            slice(data, at, length)?;
            at += length;
        }
        if i == frame {
            selected = Some(decode_pst(&data[start..at])?);
        }
    }
    if at != data.len() {
        return Err("ATM payload size mismatch".into());
    }
    Ok((count, selected.ok_or("ATM frame missing")?))
}

// Observed format2 omits the palette and length-prefixes each IPUM image after
// the same height/width/flags header used by the indexed variant.
fn decode_ipum_levels(data: &[u8], levels: u32) -> Result<Texture> {
    let mut at = 12;
    let mut selected: Option<Texture> = None;
    for _ in 0..levels {
        let height = u32le(data, at)?;
        let width = u32le(data, at + 4)?;
        if width == 0
            || height == 0
            || width > 2048
            || height > 2048
            || u32le(data, at + 8)? != 0
            || u32le(data, at + 12)? != 0
        {
            return Err("unsupported compressed PST dimensions/flags".into());
        }
        let bytes = u32le(data, at + 16)? as usize;
        at += 20;
        let image = crate::ipu::decode_ipum(slice(data, at, bytes)?)?;
        if image.info.width != width || image.info.height != height {
            return Err("compressed PST/IPUM dimension mismatch".into());
        }
        if selected
            .as_ref()
            .is_none_or(|v| v.info.width * v.info.height < width * height)
        {
            selected = Some(Texture {
                info: TextureInfo { width, height },
                rgba: image.rgba,
            });
        }
        at += bytes;
    }
    if at != data.len() {
        return Err("compressed PST payload size mismatch".into());
    }
    selected.ok_or_else(|| "compressed PST has no image".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atm_validates_all_frames_and_exact_extent() {
        let mut frame = Vec::new();
        for n in [1u32, 0, 1] {
            frame.extend(n.to_le_bytes());
        }
        frame.extend([255u8; 64]);
        for n in [1u32, 1, 0, 0] {
            frame.extend(n.to_le_bytes());
        }
        frame.push(0);
        let mut data = Vec::new();
        for n in [1u32, 2, 0] {
            data.extend(n.to_le_bytes());
        }
        data.extend(&frame);
        data.extend(&frame);
        let (count, image) = decode_atm(&data, 1).unwrap();
        assert_eq!(count, 2);
        assert_eq!(image.rgba, [255; 4]);
        assert!(decode_atm(&data, 2).is_err());
        assert!(decode_atm(&data[..data.len() - 1], 0).is_err());
        data.push(0);
        assert!(decode_atm(&data, 0).is_err());
    }
    #[test]
    fn compressed_image_checks_outer_dimensions_and_extents() {
        // Authored neutral16x16 MPEG intra macroblock, not a game asset.
        let bits = format!("000000001{}{}000", "10010".repeat(4), "0010".repeat(2));
        let payload: Vec<u8> = bits
            .as_bytes()
            .chunks(8)
            .map(|b| b.iter().fold(0, |v, &c| (v << 1) | (c - b'0')))
            .collect();
        let mut image = b"ipum".to_vec();
        image.extend(((payload.len() + 16) as u32).to_le_bytes());
        image.extend(16u16.to_le_bytes());
        image.extend(16u16.to_le_bytes());
        image.extend(1u32.to_le_bytes());
        image.extend(payload);
        image.extend([0, 0, 1, 0xb0, 0, 0, 1, 0xb1]);
        let mut data = Vec::new();
        for word in [1u32, 2, 1, 16, 16, 0, 0, image.len() as u32] {
            data.extend(word.to_le_bytes());
        }
        data.extend(image);
        let texture = decode_pst(&data).unwrap();
        assert_eq!((texture.info.width, texture.info.height), (16, 16));
        assert_eq!(&texture.rgba[..4], &[130, 130, 130, 255]);
        for end in 0..data.len() {
            assert!(decode_pst(&data[..end]).is_err());
        }
        data[12] = 32;
        assert!(
            decode_pst(&data)
                .err()
                .unwrap()
                .contains("dimension mismatch")
        );
        data[12] = 16;
        data.push(0);
        assert!(decode_pst(&data).err().unwrap().contains("payload size"));
    }
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
