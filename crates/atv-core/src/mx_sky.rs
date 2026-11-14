//! MX Unleashed CUB43 colored sky strips. Baking the original vertex colors
//! into 128px tiles is a port sampling adapter, not native mesh equivalence.
use crate::{Result, slice, u16le, u32le};

pub struct Tile {
    pub positions: [[f32; 3]; 4],
    strips: Vec<Vec<Vertex>>,
}

#[derive(Clone, Copy)]
struct Vertex {
    xy: [u8; 2],
    rgb: [u8; 3],
}

pub struct Sky {
    pub origin: [f32; 3],
    pub tiles: Vec<Tile>,
}

/// Decode the bounded, full six-face profile found in the USA disc skies.
pub fn decode(data: &[u8]) -> Result<Sky> {
    if data.len() > 16 * 1024 * 1024 {
        return Err("MX sky exceeds size limit".into());
    }
    for (i, expected) in [0, 1, 12, 43].into_iter().enumerate() {
        if u32le(data, i * 4)? != expected {
            return Err("unsupported MX sky header".into());
        }
    }
    if u32le(data, 16)? != 0 {
        return Err("unsupported MX sky flags".into());
    }
    for face in 0..6 {
        if u16le(data, 20 + face * 2)? != u16::MAX || u32le(data, 32 + face * 4)? != 0xaaaa_aaaa {
            return Err("unsupported MX sky face coverage".into());
        }
    }
    let mut vectors = [0.; 9];
    for (i, value) in vectors.iter_mut().enumerate() {
        *value = f32::from_bits(u32le(data, 56 + i * 4)?);
        if !value.is_finite() {
            return Err("nonfinite MX sky frame".into());
        }
    }
    let mut at = 92;
    let mut tiles = Vec::with_capacity(96);
    for tile in 0..96 {
        let header = slice(data, at, 21)?;
        if u32le(header, 0)? != 0
            || header[4] != 9
            || u32le(header, 5)? != 2
            || u32le(header, 9)? != 0
        {
            return Err("unsupported MX sky tile header".into());
        }
        let vertices = u16le(header, 13)? as usize;
        let count = u16le(header, 15)? as usize;
        let bytes = u32le(header, 17)? as usize;
        if vertices == 0 || count > 1024 || count == 0 {
            return Err("MX sky tile geometry exceeds limits".into());
        }
        at += 21;
        let packet = slice(data, at, bytes)?;
        at += bytes;
        let strips = strips(packet, vertices, count)?;
        let (face, x, y) = (tile / 16, tile % 4, tile % 16 / 4);
        tiles.push(Tile {
            positions: [
                crate::sky::grid_point(face, x, y)?,
                crate::sky::grid_point(face, x + 1, y)?,
                crate::sky::grid_point(face, x, y + 1)?,
                crate::sky::grid_point(face, x + 1, y + 1)?,
            ],
            strips,
        });
    }
    if at != data.len() {
        return Err("MX sky has trailing data".into());
    }
    Ok(Sky {
        origin: [vectors[0], vectors[1], vectors[2]],
        tiles,
    })
}

fn strips(packet: &[u8], vertices: usize, count: usize) -> Result<Vec<Vec<Vertex>>> {
    let mut at = 0;
    let mut xy: Option<Vec<[u8; 2]>> = None;
    let mut result = Vec::new();
    let mut total = 0;
    while at < packet.len() {
        let word = u32le(packet, at)?;
        at += 4;
        let command = word >> 24;
        let n = ((word >> 16) & 255) as usize;
        let address = word & 65535;
        let stride = match command {
            0 | 0x17 => continue,
            0x6c => 16,
            0x66 => 2,
            0x6a => 3,
            _ => return Err("unsupported MX sky VIF command".into()),
        };
        let padded = (n * stride).next_multiple_of(4);
        let raw = &slice(packet, at, padded)?[..n * stride];
        at += padded;
        match command {
            0x6c => {
                if n != 1 || address != 0x8000 {
                    return Err("invalid MX sky GIF tag extent".into());
                }
            }
            0x66 => {
                if address != 0xc001 || n < 3 || xy.is_some() || raw.iter().any(|v| *v > 128) {
                    return Err("invalid MX sky strip coordinates".into());
                }
                xy = Some(raw.chunks_exact(2).map(|p| [p[0], p[1]]).collect());
            }
            0x6a => {
                let points = xy.take().ok_or("MX sky colors have no coordinates")?;
                if address != 0xc001 + n as u32 * 3 || points.len() != n {
                    return Err("MX sky strip color count mismatch".into());
                }
                total += n;
                if total > vertices || result.len() >= count {
                    return Err("MX sky geometry exceeds declared counts".into());
                }
                result.push(
                    points
                        .into_iter()
                        .zip(raw.chunks_exact(3))
                        .map(|(xy, c)| Vertex {
                            xy,
                            rgb: [c[0], c[1], c[2]],
                        })
                        .collect(),
                );
            }
            _ => unreachable!(),
        }
    }
    if xy.is_some() || total != vertices || result.len() != count {
        return Err("incomplete MX sky geometry".into());
    }
    Ok(result)
}

impl Tile {
    /// Opaque RGB at pixel centers; reject missing coverage instead of hiding
    /// an invalid strip interpretation behind a fallback color.
    pub fn rgba(&self) -> Result<Vec<u8>> {
        let mut rgba = vec![0; 128 * 128 * 4];
        let mut work = 0usize;
        for strip in &self.strips {
            for triangle in strip.windows(3) {
                let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
                let [ax, ay] = a.xy.map(f64::from);
                let [bx, by] = b.xy.map(f64::from);
                let [cx, cy] = c.xy.map(f64::from);
                let den = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
                if den == 0. {
                    continue;
                }
                let min_x = a.xy[0].min(b.xy[0]).min(c.xy[0]) as usize;
                let max_x = a.xy[0].max(b.xy[0]).max(c.xy[0]) as usize;
                let min_y = a.xy[1].min(b.xy[1]).min(c.xy[1]) as usize;
                let max_y = a.xy[1].max(b.xy[1]).max(c.xy[1]) as usize;
                work += (max_x - min_x) * (max_y - min_y);
                if work > 4 * 1024 * 1024 {
                    return Err("MX sky raster work exceeds limit".into());
                }
                for y in min_y..max_y {
                    for x in min_x..max_x {
                        let px = x as f64 + 0.5 - cx;
                        let py = y as f64 + 0.5 - cy;
                        let u = ((by - cy) * px + (cx - bx) * py) / den;
                        let v = ((cy - ay) * px + (ax - cx) * py) / den;
                        let w = 1. - u - v;
                        if u.min(v).min(w) < -1e-8 {
                            continue;
                        }
                        let out = &mut rgba[(y * 128 + x) * 4..][..4];
                        for k in 0..3 {
                            out[k] =
                                (u * a.rgb[k] as f64 + v * b.rgb[k] as f64 + w * c.rgb[k] as f64)
                                    .round()
                                    .clamp(0., 255.) as u8;
                        }
                        out[3] = 255;
                    }
                }
            }
        }
        if rgba.chunks_exact(4).any(|pixel| pixel[3] != 255) {
            return Err("MX sky strips leave uncovered pixels".into());
        }
        Ok(rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_cube_validates_headers_counts_and_extents() {
        let mut data = vec![0u8; 92];
        for (i, word) in [0u32, 1, 12, 43].into_iter().enumerate() {
            data[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        data[20..32].fill(255);
        data[32..56].fill(0xaa);
        let mut packet = Vec::new();
        packet.extend(0x6604_c001u32.to_le_bytes());
        packet.extend([0, 0, 128, 0, 0, 128, 128, 128]);
        packet.extend(0x6a04_c00du32.to_le_bytes());
        packet.extend([90; 12]);
        for _ in 0..96 {
            data.extend(0u32.to_le_bytes());
            data.push(9);
            data.extend(2u32.to_le_bytes());
            data.extend(0u32.to_le_bytes());
            data.extend(4u16.to_le_bytes());
            data.extend(1u16.to_le_bytes());
            data.extend((packet.len() as u32).to_le_bytes());
            data.extend(&packet);
        }
        let sky = decode(&data).unwrap();
        assert_eq!(sky.tiles.len(), 96);
        assert_eq!(sky.tiles[0].rgba().unwrap()[..4], [90, 90, 90, 255]);
        for end in [0, 15, 91, 92, 112, 113, data.len() - 1] {
            assert!(decode(&data[..end]).is_err());
        }
        data.push(0);
        assert!(decode(&data).is_err());
        data.pop();
        data[109..113].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&data).is_err());
    }
    #[test]
    fn raster_preserves_color_and_rejects_holes() {
        let mut tile = Tile {
            positions: [[0.; 3]; 4],
            strips: vec![
                [[0, 0], [128, 0], [0, 128], [128, 128]]
                    .into_iter()
                    .map(|xy| Vertex {
                        xy,
                        rgb: [40, 90, 170],
                    })
                    .collect(),
            ],
        };
        assert!(
            tile.rgba()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [40, 90, 170, 255])
        );
        tile.strips[0].pop();
        assert!(tile.rgba().is_err());
    }
    #[test]
    fn raster_interpolates_both_axes_at_pixel_centers() {
        let tile = Tile {
            positions: [[0.; 3]; 4],
            strips: vec![
                [[0, 0], [128, 0], [0, 128], [128, 128]]
                    .into_iter()
                    .map(|xy| Vertex {
                        xy,
                        rgb: [xy[0], xy[1], 0],
                    })
                    .collect(),
            ],
        };
        let pixels = tile.rgba().unwrap();
        for (x, y) in [(0, 0), (127, 0), (0, 127), (127, 127), (37, 81)] {
            assert_eq!(
                &pixels[(y * 128 + x) * 4..][..4],
                &[x as u8 + 1, y as u8 + 1, 0, 255]
            );
        }
    }
    #[test]
    fn strips_require_paired_bounded_attributes() {
        let mut packet = Vec::new();
        packet.extend(0x6604_c001u32.to_le_bytes());
        packet.extend([0, 0, 128, 0, 0, 128, 128, 128]);
        packet.extend(0x6a04_c00du32.to_le_bytes());
        packet.extend([90; 12]);
        assert_eq!(strips(&packet, 4, 1).unwrap().len(), 1);
        assert!(strips(&packet, 3, 1).is_err());
        assert!(strips(&packet, 4, 2).is_err());
        for end in 1..packet.len() {
            assert!(strips(&packet[..end], 4, 1).is_err());
        }
        packet[4] = 129;
        assert!(strips(&packet, 4, 1).is_err());
    }
}
