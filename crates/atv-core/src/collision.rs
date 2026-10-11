//! Recovered type-0 COL triangle meshes. Broad-phase records are extent-checked
//! and skipped; their traversal and the original contact solver are not recovered.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Triangle {
    pub indices: [u16; 3],
    /// Plane equation: dot(normal, vertex) + distance = 0.
    pub normal: [f32; 3],
    pub distance: f32,
}

#[derive(Debug, Serialize)]
pub struct CollisionMesh {
    /// Stored 3x3 basis followed by translation. Kept in original file order.
    pub transform: [f32; 12],
    pub vertices: Vec<[f32; 3]>,
    pub vertex_normals: Option<Vec<[f32; 3]>>,
    pub triangles: Vec<Triangle>,
    pub broad_phase_nodes: u32,
    pub flags: u32,
}

fn float(data: &[u8], at: usize) -> Result<f32> {
    let value = f32::from_bits(u32le(data, at)?);
    if !value.is_finite() {
        return Err(format!("nonfinite COL coordinate at {at:#x}"));
    }
    Ok(value)
}

fn vector(data: &[u8], at: usize) -> Result<[f32; 3]> {
    Ok([float(data, at)?, float(data, at + 4)?, float(data, at + 8)?])
}

pub fn decode_col(data: &[u8]) -> Result<CollisionMesh> {
    if data.len() > 128 * 1024 * 1024 {
        return Err("COL exceeds 128 MiB limit".into());
    }
    if u32le(data, 0)? != 1234 || slice(data, 4, 4)? != b"col\0" || u32le(data, 8)? != 1 {
        return Err("unsupported COL signature/version".into());
    }
    if u32le(data, 12)? != 0 {
        return Err("unsupported COL variant; only type-0 triangle meshes recovered".into());
    }
    let mut transform = [0.; 12];
    for (i, value) in transform.iter_mut().enumerate() {
        *value = float(data, 16 + i * 4)?;
    }
    if u32le(data, 64)? != 0 {
        return Err("unsupported COL transform trailer".into());
    }
    let vertices_count = u32le(data, 68)? as usize;
    let broad_phase_nodes = u32le(data, 72)?;
    let triangles_count = u32le(data, 76)? as usize;
    let flags = u32le(data, 80)?;
    if flags != 0 {
        return Err("unsupported COL packed vertex flags".into());
    }
    if vertices_count == 0
        || vertices_count > 65536
        || triangles_count > 32768
        || broad_phase_nodes > 32768
    {
        return Err("COL counts exceed supported indexed mesh bounds".into());
    }
    let normals_at = 84 + vertices_count * 12;
    let normals_present = u32le(data, normals_at)?;
    if normals_present > 1 {
        return Err("unsupported COL vertex-normal marker".into());
    }
    let nodes_at = normals_at + 4 + vertices_count * 12 * normals_present as usize;
    let triangles_at = nodes_at + broad_phase_nodes as usize * 32;
    let expected = triangles_at + triangles_count * 22;
    if expected != data.len() {
        return Err(format!(
            "COL extent mismatch: expected {expected}, got {}",
            data.len()
        ));
    }
    let vertices = (0..vertices_count)
        .map(|i| vector(data, 84 + i * 12))
        .collect::<Result<Vec<_>>>()?;
    let vertex_normals = if normals_present == 1 {
        Some(
            (0..vertices_count)
                .map(|i| vector(data, normals_at + 4 + i * 12))
                .collect::<Result<Vec<_>>>()?,
        )
    } else {
        None
    };
    let mut triangles = Vec::with_capacity(triangles_count);
    for i in 0..triangles_count {
        let at = triangles_at + i * 22;
        let indices = [
            u16le(data, at + 16)?,
            u16le(data, at + 18)?,
            u16le(data, at + 20)?,
        ];
        if indices
            .iter()
            .any(|&index| index as usize >= vertices_count)
        {
            return Err(format!("COL triangle {i} references missing vertex"));
        }
        triangles.push(Triangle {
            indices,
            normal: vector(data, at + 4)?,
            distance: float(data, at)?,
        });
    }
    Ok(CollisionMesh {
        transform,
        vertices,
        vertex_normals,
        triangles,
        broad_phase_nodes,
        flags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle() -> Vec<u8> {
        let mut data = vec![0; 88 + 3 * 12 + 22];
        for (offset, value) in [(0, 1234u32), (8, 1), (68, 3), (76, 1)] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[4..8].copy_from_slice(b"col\0");
        for (offset, value) in [
            (16, 1f32),
            (32, 1.),
            (48, 1.),
            (84 + 12, 1.),
            (84 + 24 + 8, 1.),
            (124 + 8, 1.),
        ] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[142..144].copy_from_slice(&1u16.to_le_bytes());
        data[144..146].copy_from_slice(&2u16.to_le_bytes());
        data
    }
    #[test]
    fn original_triangle_layout_and_rejected_bad_indices() {
        let mut data = triangle();
        let mesh = decode_col(&data).unwrap();
        assert_eq!(mesh.vertices, [[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]);
        assert_eq!(mesh.triangles[0].indices, [0, 1, 2]);
        assert_eq!(mesh.triangles[0].normal, [0., 1., 0.]);
        data[144..146].copy_from_slice(&3u16.to_le_bytes());
        assert!(decode_col(&data).unwrap_err().contains("missing vertex"));
    }
    #[test]
    fn truncation_and_variant_are_not_guessed() {
        let mut data = triangle();
        for end in 0..data.len() {
            assert!(decode_col(&data[..end]).is_err());
        }
        data[12] = 2;
        assert!(decode_col(&data).unwrap_err().contains("variant"));
    }
}
