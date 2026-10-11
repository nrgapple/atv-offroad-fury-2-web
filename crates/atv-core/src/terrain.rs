//! TR2 v6 terrain geometry reconstructed from the exact executable's loader and
//! disc samples. Contact reconstruction follows 0x0016ac10 / 0x00133e00;
//! host floating-point results are not yet compared against original execution.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug)]
pub struct TerrainTile {
    pub tag: u32,
    pub heights: [u16; 289],
    /// Original packed per-vertex normal and surface attributes.
    pub attributes: [u32; 289],
}

#[derive(Debug, Serialize)]
pub struct TerrainLeaf {
    pub x: u32,
    pub z: u32,
    pub tile: u16,
    pub height_morph: f32,
    pub height_scale: f32,
    pub minimum_height: f32,
    pub maximum_height: f32,
}

#[derive(Debug)]
pub struct Terrain {
    pub spacing: f32,
    pub tiles: Vec<TerrainTile>,
    pub leaves: Vec<TerrainLeaf>,
    pub hierarchy_nodes: usize,
    pub width_cells: u32,
    pub depth_cells: u32,
}

#[derive(Debug, Serialize)]
pub struct TerrainContact {
    pub height: f32,
    pub normal: [f32; 3],
    pub surface: u8,
    pub cell: [u32; 2],
    /// Corner order is A=(0,0), B=(1,0), C=(0,1), D=(1,1).
    pub triangle: [u8; 3],
}

fn float(data: &[u8], at: usize) -> Result<f32> {
    let value = f32::from_bits(u32le(data, at)?);
    if !value.is_finite() {
        return Err(format!("nonfinite TR2 float at {at:#x}"));
    }
    Ok(value)
}

fn raw_block<'a>(data: &'a [u8], at: &mut usize, expected: usize) -> Result<&'a [u8]> {
    let stored = u32le(data, *at)? as usize;
    let decoded = u32le(data, *at + 4)? as usize;
    *at += 8;
    if decoded != expected {
        return Err("unexpected TR2 tile payload size".into());
    }
    if stored != decoded {
        return Err("compressed TR2 tile payload not yet supported".into());
    }
    let result = slice(data, *at, stored)?;
    *at += stored;
    Ok(result)
}

struct Hierarchy<'a> {
    data: &'a [u8],
    tile_count: usize,
    seen: BTreeSet<usize>,
    leaves: Vec<TerrainLeaf>,
}

impl Hierarchy<'_> {
    fn walk(&mut self, offset: usize, depth: u32, x: u32, z: u32) -> Result<()> {
        if self.seen.len() >= 65536 || !self.seen.insert(offset) {
            return Err("TR2 hierarchy is cyclic, aliased, or too large".into());
        }
        slice(self.data, offset, 97)?;
        let height_morph = float(self.data, offset)?;
        let height_scale = float(self.data, offset + 4)?;
        let minimum_height = float(self.data, offset + 8)?;
        let maximum_height = float(self.data, offset + 12)?;
        let tile = u16le(self.data, offset + 18)?;
        if tile as usize >= self.tile_count || height_scale <= 0. || minimum_height > maximum_height
        {
            return Err("invalid TR2 terrain node".into());
        }
        let children = u32le(self.data, offset + 93)? as usize;
        if depth == 0 {
            if children != 0 {
                return Err("TR2 leaf has child hierarchy".into());
            }
            self.leaves.push(TerrainLeaf {
                x,
                z,
                tile,
                height_morph,
                height_scale,
                minimum_height,
                maximum_height,
            });
        } else {
            if children == 0 {
                return Err("TR2 coarse terminal node not yet supported".into());
            }
            slice(self.data, children, 1024)?;
            for i in 0..256 {
                let child = u32le(self.data, children + i * 4)? as usize;
                if child != 0 {
                    self.walk(
                        child,
                        depth - 1,
                        (x + i as u32 % 16) * 16,
                        (z + i as u32 / 16) * 16,
                    )?;
                }
            }
        }
        Ok(())
    }
}

pub fn decode_tr2(data: &[u8]) -> Result<Terrain> {
    if data.len() > 128 * 1024 * 1024 {
        return Err("TR2 exceeds 128 MiB limit".into());
    }
    if slice(data, 0, 4)? != b"TR2\0" || u32le(data, 4)? != 6 {
        return Err("unsupported TR2 signature/version".into());
    }
    let palette = u32le(data, 8)?;
    if palette > 1 {
        return Err("unsupported TR2 palette marker".into());
    }
    let header = 12 + palette as usize * 1024;
    let spacing = float(data, header + 4)?;
    let depth = u32le(data, header + 8)?;
    let root = u32le(data, header + 12)? as usize;
    let tile_count = u32le(data, header + 16)? as usize;
    let loaded_tiles = u32le(data, header + 20)? as usize;
    if spacing <= 0.
        || spacing > 10000.
        || depth > 3
        || tile_count == 0
        || tile_count > 65536
        || loaded_tiles != tile_count
    {
        return Err("unsupported TR2 scale, depth or tile counts".into());
    }
    let table = header + 24;
    slice(data, table, tile_count * 4)?;
    let mut tiles = Vec::with_capacity(tile_count);
    for i in 0..tile_count {
        let mut at = u32le(data, table + i * 4)? as usize;
        let tag = u32le(data, at)?;
        at += 4;
        let height_data = raw_block(data, &mut at, 578)?;
        let attribute_data = raw_block(data, &mut at, 1156)?;
        let mut heights = [0; 289];
        let mut attributes = [0; 289];
        for j in 0..289 {
            heights[j] = u16le(height_data, j * 2)?;
            attributes[j] = u32le(attribute_data, j * 4)?;
        }
        tiles.push(TerrainTile {
            tag,
            heights,
            attributes,
        });
    }
    let mut hierarchy = Hierarchy {
        data,
        tile_count,
        seen: BTreeSet::new(),
        leaves: Vec::new(),
    };
    hierarchy.walk(root, depth, 0, 0)?;
    if hierarchy.leaves.is_empty() {
        return Err("TR2 contains no finest-level terrain".into());
    }
    let width_cells = hierarchy
        .leaves
        .iter()
        .map(|leaf| leaf.x + 16)
        .max()
        .unwrap();
    let depth_cells = hierarchy
        .leaves
        .iter()
        .map(|leaf| leaf.z + 16)
        .max()
        .unwrap();
    Ok(Terrain {
        spacing,
        tiles,
        hierarchy_nodes: hierarchy.seen.len(),
        leaves: hierarchy.leaves,
        width_cells,
        depth_cells,
    })
}

/// Authored texture references on a hierarchy node. Indices use the original
/// `(packed & 0xfffe) >> 1` conversion at 0x0012d6d0; 0xfffe means absent.
#[derive(Debug, Serialize)]
pub struct TerrainTextureRegion {
    pub x: u32,
    pub z: u32,
    pub span_cells: u32,
    pub base_texture: Option<u16>,
    pub subtextures: [Option<u16>; 16],
}
#[derive(Debug, Serialize)]
pub struct TerrainTextureMip {
    pub width: u32,
    pub height: u32,
    pub offset: usize,
    pub bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct TerrainTextureRecord {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub mips: Vec<TerrainTextureMip>,
}
#[derive(Debug, Serialize)]
pub struct TerrainTextures {
    pub records: Vec<TerrainTextureRecord>,
    pub regions: Vec<TerrainTextureRegion>,
}

fn texture_record(data: &[u8], offset: usize, limit: usize) -> Result<TerrainTextureRecord> {
    let width = u32le(data, offset)?;
    let height = u32le(data, offset + 4)?;
    let format = u32le(data, offset + 8)?;
    if width == 0 || width > 4096 || !width.is_power_of_two() || height != width || format != 32 {
        return Err("unsupported TR2 embedded texture dimensions/format".into());
    }
    let levels = width.ilog2() as usize + 1;
    let table = offset + 12;
    let table_bytes = (levels + 1) * 4;
    if u32le(data, table)? as usize != table_bytes {
        return Err("invalid TR2 texture mip offset table".into());
    }
    let mut at = table + table_bytes;
    let mut mips = Vec::with_capacity(levels);
    for level in 0..levels {
        let begin = u32le(data, table + level * 4)? as usize;
        let end = u32le(data, table + (level + 1) * 4)? as usize;
        let bytes = u32le(data, at)? as usize;
        if end.checked_sub(begin) != Some(bytes) || !(16..=16 * 1024 * 1024).contains(&bytes) {
            return Err("invalid TR2 compressed mip length".into());
        }
        at += 4;
        let mip = slice(data, at, bytes)?;
        if at + bytes > limit || &mip[..4] != b"ipum" || u32le(mip, 4)? as usize + 8 != bytes {
            return Err("invalid TR2 IPUM mip extent/header".into());
        }
        let dimension = 1u32 << level;
        let stored_width = u16le(mip, 8)? as u32;
        let stored_height = u16le(mip, 10)? as u32;
        // The original chain retains zero-dimension placeholders below 16 pixels.
        if (stored_width != dimension || stored_height != dimension)
            && !(dimension < 16 && stored_width == 0 && stored_height == 0)
        {
            return Err("TR2 mip dimensions disagree with texture chain".into());
        }
        mips.push(TerrainTextureMip {
            width: stored_width,
            height: stored_height,
            offset: at,
            bytes,
        });
        at += bytes;
    }
    if at != limit {
        return Err("TR2 embedded texture has trailing or missing bytes".into());
    }
    Ok(TerrainTextureRecord {
        width,
        height,
        format,
        mips,
    })
}

/// Parse the original embedded IPUM streams without interpreting them as pixels.
/// Texture upload follows 0x00168490/0x0015a770. This does not decode MPEG/IPU.
pub fn texture_layout(data: &[u8]) -> Result<TerrainTextures> {
    if data.len() > 128 * 1024 * 1024 || slice(data, 0, 4)? != b"TR2\0" || u32le(data, 4)? != 6 {
        return Err("unsupported TR2 texture source".into());
    }
    let palette = u32le(data, 8)?;
    if palette > 1 {
        return Err("unsupported TR2 palette marker".into());
    }
    let header = 12 + palette as usize * 1024;
    let tile_count = u32le(data, header + 16)? as usize;
    if tile_count > 65536 {
        return Err("TR2 tile table exceeds limit".into());
    }
    let texture_table = header + 24 + tile_count * 4;
    let count = u32le(data, texture_table)? as usize;
    if count == 0 || count > 32767 || u32le(data, texture_table + 4)? as usize != count {
        return Err("unsupported TR2 texture counts".into());
    }
    let root = u32le(data, header + 12)? as usize;
    let depth = u32le(data, header + 8)?;
    if depth > 3 {
        return Err("TR2 texture hierarchy depth exceeds limit".into());
    }
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let at = u32le(data, texture_table + 8 + index * 4)? as usize;
        let end = if index + 1 == count {
            root
        } else {
            u32le(data, texture_table + 12 + index * 4)? as usize
        };
        if at >= end || end > data.len() {
            return Err("invalid TR2 texture record extent".into());
        }
        records.push(texture_record(data, at, end)?);
    }
    fn walk(
        data: &[u8],
        at: usize,
        depth: u32,
        origin: [u32; 2],
        result: &mut TerrainTextures,
        seen: &mut BTreeSet<usize>,
    ) -> Result<()> {
        if seen.len() >= 65536 || !seen.insert(at) {
            return Err("invalid TR2 texture hierarchy".into());
        }
        slice(data, at, 97)?;
        let reference = |offset| -> Result<Option<u16>> {
            let packed = u32le(data, offset)? & 0xfffe;
            if packed == 0xfffe {
                return Ok(None);
            }
            let index = (packed >> 1) as u16;
            if usize::from(index) >= result.records.len() {
                return Err("TR2 texture index exceeds table".into());
            }
            Ok(Some(index))
        };
        let base_texture = reference(at + 20)?;
        let mut subtextures = [None; 16];
        for (index, value) in subtextures.iter_mut().enumerate() {
            *value = reference(at + 24 + index * 4)?;
        }
        let span_cells = 16u32 << (depth * 4);
        if base_texture.is_some() || subtextures.iter().any(Option::is_some) {
            result.regions.push(TerrainTextureRegion {
                x: origin[0],
                z: origin[1],
                span_cells,
                base_texture,
                subtextures,
            });
        }
        let children = u32le(data, at + 93)? as usize;
        if children != 0 {
            if depth == 0 {
                return Err("TR2 texture leaf has children".into());
            }
            slice(data, children, 1024)?;
            for index in 0..256 {
                let child = u32le(data, children + index * 4)? as usize;
                if child != 0 {
                    walk(
                        data,
                        child,
                        depth - 1,
                        [
                            origin[0] + (index as u32 % 16) * (span_cells / 16),
                            origin[1] + (index as u32 / 16) * (span_cells / 16),
                        ],
                        result,
                        seen,
                    )?;
                }
            }
        }
        Ok(())
    }
    let mut result = TerrainTextures {
        records,
        regions: Vec::new(),
    };
    walk(data, root, depth, [0, 0], &mut result, &mut BTreeSet::new())?;
    Ok(result)
}

impl Terrain {
    fn vertex_height(&self, leaf: &TerrainLeaf, index: usize) -> f32 {
        let tile = &self.tiles[leaf.tile as usize];
        let raw = tile.heights[index] as f32;
        // 0x00133ef8 selects the direct path when morph is exactly one.
        let value = if leaf.height_morph == 1. {
            raw
        } else {
            let base = tile.tag as i32 as f32;
            base + leaf.height_morph * (raw - base)
        };
        (leaf.height_scale * value) * self.spacing
    }

    /// Reconstruct the TR2 full-resolution contact path at 0x0016ac10.
    ///
    /// Negative coordinates clamp to the zero edge, as in the original MAX.S.
    /// Missing/outside cells return None; nonfinite inputs are rejected. Heights
    /// use the original triangle plane, normals use the original inverse-square
    /// corner weights, and surfaces use the nearest corner's packed index.
    /// The runtime's optional two-cell sampling mode and PS2 floating-point
    /// rounding/reciprocal-square-root precision are not reproduced here.
    pub fn contact(&self, x: f32, z: f32) -> Result<Option<TerrainContact>> {
        if !x.is_finite() || !z.is_finite() {
            return Err("nonfinite terrain contact position".into());
        }
        let inverse_spacing = 1. / self.spacing;
        let gx = (x * inverse_spacing).max(0.);
        let gz = (z * inverse_spacing).max(0.);
        if gx >= self.width_cells as f32 || gz >= self.depth_cells as f32 {
            return Ok(None);
        }
        let cell_x = gx as u32;
        let cell_z = gz as u32;
        let Some(leaf) = self.leaves.iter().find(|leaf| {
            cell_x >= leaf.x && cell_x < leaf.x + 16 && cell_z >= leaf.z && cell_z < leaf.z + 16
        }) else {
            return Ok(None);
        };
        let local_x = (cell_x - leaf.x) as usize;
        let local_z = (cell_z - leaf.z) as usize;
        let start = local_z * 17 + local_x;
        let indices = [start, start + 1, start + 17, start + 18];
        let heights = indices.map(|index| self.vertex_height(leaf, index));
        let tile = &self.tiles[leaf.tile as usize];
        let attributes = indices.map(|index| tile.attributes[index].to_le_bytes());
        let inverse_normal_scale = 1. / 127.;
        let xz_scale = leaf.height_morph * inverse_normal_scale;
        let normals = attributes.map(|bytes| {
            [
                bytes[0] as i8 as f32 * xz_scale,
                bytes[1] as i8 as f32 * inverse_normal_scale,
                bytes[2] as i8 as f32 * xz_scale,
            ]
        });
        let u = gx - cell_x as f32;
        let v = gz - cell_z as f32;
        let u1 = 1. - u;
        let v1 = 1. - v;
        let distances = [
            u * u + v * v,
            u1 * u1 + v * v,
            u * u + v1 * v1,
            u1 * u1 + v1 * v1,
        ]
        .map(|distance| distance.max(0.001));
        let mut nearest = 0;
        for index in 1..4 {
            if distances[index] < distances[nearest] {
                nearest = index;
            }
        }
        // Strict comparison preserves the original diagonal tie direction.
        let (height, triangle) = if distances[0] < distances[3] {
            (
                (heights[0] + u * (heights[1] - heights[0])) + v * (heights[2] - heights[0]),
                [0usize, 1, 2],
            )
        } else {
            (
                (heights[2] + u * (heights[3] - heights[2])) + v1 * (heights[1] - heights[3]),
                [1usize, 3, 2],
            )
        };
        let weights = triangle.map(|index| 1. / distances[index]);
        let mut normal = [0.; 3];
        for axis in 0..3 {
            normal[axis] = (normals[triangle[0]][axis] * weights[0]
                + normals[triangle[1]][axis] * weights[1])
                + normals[triangle[2]][axis] * weights[2];
        }
        let length =
            ((normal[0] * normal[0] + normal[1] * normal[1]) + normal[2] * normal[2]).sqrt();
        if !length.is_finite() || length == 0. || !height.is_finite() {
            return Err("degenerate terrain contact attributes".into());
        }
        let reciprocal_length = 1. / length;
        for component in &mut normal {
            *component *= reciprocal_length;
        }
        Ok(Some(TerrainContact {
            height,
            normal,
            surface: attributes[nearest][3] >> 1,
            cell: [cell_x, cell_z],
            triangle: triangle.map(|index| index as u8),
        }))
    }

    /// Dense original heights, row-major Z then X. Refuses sparse or oversized worlds.
    pub fn heightfield(&self) -> Result<Vec<f32>> {
        let width = self.width_cells as usize + 1;
        let depth = self.depth_cells as usize + 1;
        let count = width
            .checked_mul(depth)
            .ok_or("TR2 heightfield size overflow")?;
        if count > 16 * 1024 * 1024 {
            return Err("TR2 heightfield exceeds 16 million vertices".into());
        }
        let mut heights = vec![f32::NAN; count];
        for leaf in &self.leaves {
            let tile = self
                .tiles
                .get(leaf.tile as usize)
                .ok_or("missing TR2 tile")?;
            for i in 0..tile.heights.len() {
                let x = leaf.x as usize + i % 17;
                let z = leaf.z as usize + i / 17;
                if x >= width || z >= depth {
                    return Err("TR2 tile lies outside heightfield".into());
                }
                let height = self.vertex_height(leaf, i);
                let target = &mut heights[z * width + x];
                // Adjacent original tiles may differ by one quantization unit.
                if target.is_finite()
                    && (*target - height).abs()
                        > leaf.height_scale * self.spacing * leaf.height_morph.abs().max(1.) * 1.1
                            + 0.0001
                {
                    return Err("TR2 neighboring tile heights disagree".into());
                }
                *target = height;
            }
        }
        if heights.iter().any(|height| !height.is_finite()) {
            return Err("TR2 world has holes in its finest-level heightfield".into());
        }
        Ok(heights)
    }

    /// Original positions for one finest-level tile, row-major 17 by 17.
    pub fn positions(&self, leaf: &TerrainLeaf) -> Vec<[f32; 3]> {
        self.tiles[leaf.tile as usize]
            .heights
            .iter()
            .enumerate()
            .map(|(i, _)| {
                [
                    (leaf.x + i as u32 % 17) as f32 * self.spacing,
                    self.vertex_height(leaf, i),
                    (leaf.z + i as u32 / 17) as f32 * self.spacing,
                ]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        let mut data = vec![0; 40 + 1754 + 97];
        data[..4].copy_from_slice(b"TR2\0");
        for (at, value) in [
            (4, 6u32),
            (16, 2f32.to_bits()),
            (24, 1794),
            (28, 1),
            (32, 1),
            (36, 40),
            (44, 578),
            (48, 578),
            (630, 1156),
            (634, 1156),
            (1794, 1f32.to_bits()),
            (1798, 0.5f32.to_bits()),
            (1806, 100f32.to_bits()),
        ] {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[52..54].copy_from_slice(&10u16.to_le_bytes());
        data
    }
    #[test]
    fn tile_height_and_world_spacing() {
        let data = sample();
        let terrain = decode_tr2(&data).unwrap();
        assert_eq!(terrain.width_cells, 16);
        assert_eq!(terrain.positions(&terrain.leaves[0])[0], [0., 10., 0.]);
        assert_eq!(terrain.positions(&terrain.leaves[0])[18], [2., 0., 2.]);
        for end in 0..data.len() {
            assert!(decode_tr2(&data[..end]).is_err());
        }
    }
    #[test]
    fn hierarchy_cycle_and_invalid_tile_fail() {
        let mut data = sample();
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        data[1887..1891].copy_from_slice(&1891u32.to_le_bytes());
        data.resize(2915, 0);
        data[1891..1895].copy_from_slice(&1794u32.to_le_bytes());
        assert!(decode_tr2(&data).unwrap_err().contains("cyclic"));
        let mut data = sample();
        data[1812..1814].copy_from_slice(&1u16.to_le_bytes());
        assert!(decode_tr2(&data).unwrap_err().contains("node"));
    }

    #[test]
    fn contact_uses_triangles_weighted_normals_and_nearest_surface() {
        let mut terrain = decode_tr2(&sample()).unwrap();
        let tile = &mut terrain.tiles[0];
        for (index, height, normal, surface) in [
            (0, 0, [0u8, 127, 0], 2u8),
            (1, 10, [127, 0, 0], 4),
            (17, 20, [0, 0, 127], 6),
            (18, 100, [129, 0, 0], 8),
        ] {
            tile.heights[index] = height;
            // The low fourth-byte bit must not affect the surface index.
            tile.attributes[index] =
                u32::from_le_bytes([normal[0], normal[1], normal[2], (surface << 1) | 1]);
        }
        let first = terrain.contact(0.5, 0.5).unwrap().unwrap();
        assert_eq!(first.height, 7.5); // Bilinear interpolation would give 11.875.
        assert_eq!(first.triangle, [0, 1, 2]);
        assert_eq!(first.surface, 2);
        let expected_length = (1.6f32 * 1.6 * 2. + 8. * 8.).sqrt();
        for (actual, expected) in first.normal.iter().zip([1.6, 8., 1.6]) {
            assert!((*actual - expected / expected_length).abs() < 1e-6);
        }
        let second = terrain.contact(1.5, 1.5).unwrap().unwrap();
        assert_eq!(second.height, 57.5);
        assert_eq!(second.triangle, [1, 3, 2]);
        assert_eq!(second.surface, 8);
        let diagonal = terrain.contact(1., 1.).unwrap().unwrap();
        assert_eq!(diagonal.height, 15.);
        assert_eq!(diagonal.triangle, [1, 3, 2]);
        assert_eq!(diagonal.surface, 2); // Strict nearest-corner tie chooses A.
        assert_eq!(terrain.contact(-5., -2.).unwrap().unwrap().height, 0.);
        assert!(terrain.contact(32., 0.).unwrap().is_none());
        assert!(terrain.contact(f32::NAN, 0.).is_err());
    }

    #[test]
    fn leaf_morph_scales_about_original_tile_baseline() {
        let mut terrain = decode_tr2(&sample()).unwrap();
        terrain.tiles[0].tag = 100;
        terrain.tiles[0].heights.fill(110);
        terrain.tiles[0]
            .attributes
            .fill(u32::from_le_bytes([0, 127, 0, 0]));
        terrain.leaves[0].height_morph = 1.5;
        assert_eq!(terrain.positions(&terrain.leaves[0])[0][1], 115.);
        assert_eq!(terrain.heightfield().unwrap()[0], 115.);
        assert_eq!(terrain.contact(0., 0.).unwrap().unwrap().height, 115.);
    }

    #[test]
    fn embedded_mip_chain_checks_offsets_dimensions_and_extents() {
        let mut data = Vec::new();
        for value in [16u32, 16, 32, 24, 40, 56, 72, 88, 104] {
            data.extend(value.to_le_bytes());
        }
        for level in 0..5 {
            data.extend(16u32.to_le_bytes());
            data.extend(b"ipum");
            data.extend(8u32.to_le_bytes());
            let dimension = if level == 4 { 16u16 } else { 0 };
            data.extend(dimension.to_le_bytes());
            data.extend(dimension.to_le_bytes());
            data.extend(1u32.to_le_bytes());
        }
        let record = texture_record(&data, 0, data.len()).unwrap();
        assert_eq!(record.mips.len(), 5);
        assert_eq!(record.mips[4].width, 16);
        assert_eq!(record.mips[0].width, 0);
        for end in 0..data.len() {
            assert!(texture_record(&data[..end], 0, end).is_err());
        }
        let mut bad = data.clone();
        bad[16] += 1;
        assert!(texture_record(&bad, 0, bad.len()).is_err());
        let last = record.mips[4].offset;
        data[last + 8] = 8;
        assert!(texture_record(&data, 0, data.len()).is_err());
    }
}
