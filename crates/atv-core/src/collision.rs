//! Recovered type-0 triangle meshes and type-2 ray bundles. Type-0 broad-phase
//! traversal and ray/triangle obstacle queries remain separate reconstruction work.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Triangle {
    pub indices: [u16; 3],
    /// Preserve invalid source-plane words on duplicate-vertex, zero-area faces.
    /// These retain their tree indices but cannot produce a surface contact.
    pub invalid_plane_bits: Option<[u32; 4]>,
    /// Plane equation: dot(normal, vertex) + distance = 0.
    pub normal: [f32; 3],
    pub distance: f32,
}

#[derive(Debug, Serialize)]
pub struct CollisionMesh {
    /// Stored 3x3 basis followed by translation. Kept in original file order.
    pub transform: [f32; 12],
    pub vertices: Vec<[f32; 3]>,
    /// Optional authored shading normals, retained as raw bits. Several original
    /// meshes contain NaNs here; these are not geometry or contact planes.
    pub vertex_normal_bits: Option<Vec<[u32; 3]>>,
    pub triangles: Vec<Triangle>,
    pub broad_phase_nodes: u32,
    pub nodes: Vec<RayTreeNode>,
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

fn packed_coordinate(bits: u16) -> f32 {
    (32767.0 * f32::from(bits as i16)) * f32::from_bits(0x38000100)
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
    if flags > 1 {
        return Err("unsupported COL packed vertex flags".into());
    }
    if vertices_count == 0
        || vertices_count > 65536
        || triangles_count > 32768
        || broad_phase_nodes > 32768
    {
        return Err("COL counts exceed supported indexed mesh bounds".into());
    }
    // 00330850 stores flag-1 vertices/normals as signed 16-bit triples;
    // 0010b800 multiplies by global32767 then its binary32 reciprocal.
    // Preserve that operation order, including its rounding.
    let vertex_stride = if flags == 0 { 12 } else { 6 };
    let normals_at = 84 + vertices_count * vertex_stride;
    let normals_present = u32le(data, normals_at)?;
    if normals_present > 1 {
        return Err("unsupported COL vertex-normal marker".into());
    }
    let nodes_at = normals_at + 4 + vertices_count * vertex_stride * normals_present as usize;
    let triangles_at = nodes_at + broad_phase_nodes as usize * 32;
    let expected = triangles_at + triangles_count * 22;
    if expected != data.len() {
        return Err(format!(
            "COL extent mismatch: expected {expected}, got {}",
            data.len()
        ));
    }
    let vertices = (0..vertices_count)
        .map(|i| {
            let at = 84 + i * vertex_stride;
            if flags == 0 {
                vector(data, at)
            } else {
                Ok([
                    packed_coordinate(u16le(data, at)?),
                    packed_coordinate(u16le(data, at + 2)?),
                    packed_coordinate(u16le(data, at + 4)?),
                ])
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let vertex_normal_bits = if normals_present == 1 {
        Some(
            (0..vertices_count)
                .map(|i| {
                    let at = normals_at + 4 + i * vertex_stride;
                    if flags == 0 {
                        Ok([u32le(data, at)?, u32le(data, at + 4)?, u32le(data, at + 8)?])
                    } else {
                        Ok([
                            u16le(data, at)? as u32,
                            u16le(data, at + 2)? as u32,
                            u16le(data, at + 4)? as u32,
                        ])
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        )
    } else {
        None
    };
    let mut nodes = Vec::with_capacity(broad_phase_nodes as usize);
    for i in 0..broad_phase_nodes as usize {
        let at = nodes_at + i * 32;
        nodes.push(RayTreeNode {
            bound_measure: float(data, at)?,
            center: vector(data, at + 4)?,
            half_extents: vector(data, at + 16)?,
            children: [u16le(data, at + 28)?, u16le(data, at + 30)?],
        });
    }
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
        let plane_bits = [
            u32le(data, at)?,
            u32le(data, at + 4)?,
            u32le(data, at + 8)?,
            u32le(data, at + 12)?,
        ];
        let plane = plane_bits.map(f32::from_bits);
        let invalid = plane.iter().any(|v| !v.is_finite());
        let [a, b, c] = indices.map(|i| vertices[i as usize]);
        // ENV02 ruins has duplicate vertices; ENV08 distillery also has distinct
        // collinear vertices. Only exactly zero-area faces may lack a plane.
        // Evaluate in f64 so small valid faces do not underflow into this exception.
        let ab = std::array::from_fn::<_, 3, _>(|j| f64::from(b[j]) - f64::from(a[j]));
        let ac = std::array::from_fn::<_, 3, _>(|j| f64::from(c[j]) - f64::from(a[j]));
        let zero_area = ab[1] * ac[2] == ab[2] * ac[1]
            && ab[2] * ac[0] == ab[0] * ac[2]
            && ab[0] * ac[1] == ab[1] * ac[0];
        if invalid && !zero_area {
            return Err(format!("nonfinite COL plane on nondegenerate triangle {i}"));
        }
        triangles.push(Triangle {
            indices,
            invalid_plane_bits: invalid.then_some(plane_bits),
            normal: if invalid {
                [0.; 3]
            } else {
                [plane[1], plane[2], plane[3]]
            },
            distance: if invalid { 0. } else { plane[0] },
        });
    }
    Ok(CollisionMesh {
        transform,
        vertices,
        vertex_normal_bits,
        triangles,
        broad_phase_nodes,
        nodes,
        flags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_signed_vertices_use_original_expansion_without_changing_planes() {
        let mut data = triangle();
        data[80..84].copy_from_slice(&1u32.to_le_bytes());
        let packed = [0i16, 0, 0, -1, 0, 0, 0, 0, 1]
            .into_iter()
            .flat_map(i16::to_le_bytes);
        data.splice(84..120, packed);
        let mesh = decode_col(&data).unwrap();
        assert_eq!(
            mesh.vertices[1],
            [packed_coordinate((-1i16) as u16), 0., 0.]
        );
        assert_eq!(mesh.triangles[0].normal, [0., 1., 0.]);
        assert_eq!(mesh.triangles[0].indices, [0, 1, 2]);
    }
    #[test]
    fn optional_shading_normals_preserve_nan_bits_without_changing_contacts() {
        let mut data = triangle();
        data[120..124].copy_from_slice(&1u32.to_le_bytes());
        let normals = [0xffc00000u32; 9].into_iter().flat_map(u32::to_le_bytes);
        data.splice(124..124, normals);
        let mesh = decode_col(&data).unwrap();
        assert_eq!(
            mesh.vertex_normal_bits.as_ref().unwrap()[0],
            [0xffc00000; 3]
        );
        assert_eq!(mesh.triangles[0].normal, [0., 1., 0.]);
        assert_eq!(mesh.query_normal(0).unwrap(), [0., 1., 0.]);
    }
    #[test]
    fn nan_planes_only_allowed_for_duplicate_vertex_faces() {
        let mut data = triangle();
        data[124..128].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert!(decode_col(&data).is_err());
        let first = data[84..96].to_vec();
        data[96..108].copy_from_slice(&first);
        let mesh = decode_col(&data).unwrap();
        assert_eq!(mesh.triangles.len(), 1);
        assert!(mesh.triangles[0].invalid_plane_bits.is_some());
        assert_eq!(mesh.triangles[0].indices, [0, 1, 2]);
        assert_eq!(mesh.query_normal(0).unwrap(), [0.; 3]);
        data[84..88].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert!(decode_col(&data).is_err());
    }
    #[test]
    fn collinear_nan_face_keeps_tree_index_but_has_no_contact_normal() {
        let mut data = triangle();
        data[108..112].copy_from_slice(&2f32.to_le_bytes());
        data[116..120].copy_from_slice(&0f32.to_le_bytes());
        data[124..128].copy_from_slice(&f32::NAN.to_le_bytes());
        let mesh = decode_col(&data).unwrap();
        assert_eq!(mesh.triangles[0].indices, [0, 1, 2]);
        assert!(mesh.triangles[0].invalid_plane_bits.is_some());
        assert_eq!(mesh.query_normal(0).unwrap(), [0.; 3]);
    }
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

/// Type-2 COL ray bundle, loaded by0032a480. Tree nodes retain the original
/// first-child/second-child order used by00194590 to build chassis contacts.
#[derive(Debug, Serialize)]
pub struct CollisionRayTree {
    pub transform: [f32; 16],
    pub query_capacity: u32,
    pub nodes: Vec<RayTreeNode>,
    pub rays: Vec<CollisionRay>,
}
#[derive(Debug, Serialize)]
pub struct RayTreeNode {
    pub bound_measure: f32,
    pub center: [f32; 3],
    pub half_extents: [f32; 3],
    pub children: [u16; 2],
}
#[derive(Debug, Serialize)]
pub struct CollisionRay {
    pub start: [f32; 3],
    pub end: [f32; 3],
}
impl CollisionRayTree {
    /// Original00194590 emits leaf endpoints directly in their authored node
    /// coordinates; it does not apply the ray bundle's stored transform here.
    pub fn chassis_points(&self) -> Result<Vec<[f32; 3]>> {
        let mut points = Vec::with_capacity(self.rays.len());
        let mut stack = vec![0_u16];
        let mut seen = vec![false; self.nodes.len()];
        while let Some(index) = stack.pop() {
            if index & 0x8000 != 0 {
                let ray = self
                    .rays
                    .get((index & 0x7fff) as usize)
                    .ok_or("COL ray leaf outside ray table")?;
                points.push(ray.end);
            } else {
                let index = index as usize;
                let node = self
                    .nodes
                    .get(index)
                    .ok_or("COL ray child outside node table")?;
                if seen[index] {
                    return Err("COL ray hierarchy is not a tree".into());
                }
                seen[index] = true;
                stack.push(node.children[1]);
                stack.push(node.children[0]);
            }
        }
        Ok(points)
    }
}
pub fn decode_ray_tree(data: &[u8]) -> Result<CollisionRayTree> {
    let tree = decode_ray_tree_version(data, 1)?;
    tree.chassis_points()?;
    Ok(tree)
}
/// MX vs. ATV Unleashed COL version 2 ray trees (bike chassis whiskers) share
/// ATV2's type-2 layout after one extra header word at offset 16.
pub fn decode_mx_ray_tree(data: &[u8]) -> Result<CollisionRayTree> {
    decode_ray_tree_version(data, 2)
}
fn decode_ray_tree_version(data: &[u8], version: u32) -> Result<CollisionRayTree> {
    if u32le(data, 0)? != 1234
        || slice(data, 4, 4)? != b"col\0"
        || u32le(data, 8)? != version
        || u32le(data, 12)? != 2
    {
        return Err("unsupported COL ray-tree signature/version/type".into());
    }
    let base = if version == 2 { 4 } else { 0 };
    let mut transform = [0.; 16];
    for (i, x) in transform.iter_mut().enumerate() {
        *x = float(data, base + 16 + i * 4)?;
    }
    let query_capacity = u32le(data, base + 80)?;
    let node_count = u32le(data, base + 84)? as usize;
    let ray_count = u32le(data, base + 88)? as usize;
    if query_capacity > 32768
        || node_count == 0
        || node_count > 32768
        || ray_count == 0
        || ray_count > 32768
    {
        return Err("COL ray-tree counts outside supported bounds".into());
    }
    let nodes_at = base + 92;
    let rays_at = nodes_at + node_count * 32;
    if data.len() != rays_at + ray_count * 24 {
        return Err("COL ray-tree extent mismatch".into());
    }
    let mut nodes = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let at = nodes_at + i * 32;
        nodes.push(RayTreeNode {
            bound_measure: float(data, at)?,
            center: vector(data, at + 4)?,
            half_extents: vector(data, at + 16)?,
            children: [u16le(data, at + 28)?, u16le(data, at + 30)?],
        });
    }
    let mut rays = Vec::with_capacity(ray_count);
    for i in 0..ray_count {
        let at = rays_at + i * 24;
        let ray = CollisionRay {
            start: vector(data, at)?,
            end: vector(data, at + 12)?,
        };
        if ray
            .start
            .iter()
            .chain(ray.end.iter())
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite COL ray".into());
        }
        rays.push(ray);
    }
    Ok(CollisionRayTree {
        transform,
        query_capacity,
        nodes,
        rays,
    })
}
#[cfg(test)]
mod ray_tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 92 + 32 + 48];
        b[0..4].copy_from_slice(&1234u32.to_le_bytes());
        b[4..8].copy_from_slice(b"col\0");
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[12..16].copy_from_slice(&2u32.to_le_bytes());
        for i in 0..4 {
            b[16 + i * 20..20 + i * 20].copy_from_slice(&1f32.to_le_bytes());
        }
        for (o, x) in [(80, 2u32), (84, 1), (88, 2)] {
            b[o..o + 4].copy_from_slice(&x.to_le_bytes());
        }
        b[120..122].copy_from_slice(&0x8001u16.to_le_bytes());
        b[122..124].copy_from_slice(&0x8000u16.to_le_bytes());
        b[136..140].copy_from_slice(&1f32.to_le_bytes());
        b[160..164].copy_from_slice(&2f32.to_le_bytes());
        b
    }
    #[test]
    fn ray_tree_retains_traversal_order() {
        let t = decode_ray_tree(&fixture()).unwrap();
        assert_eq!(t.chassis_points().unwrap(), [[2., 0., 0.], [1., 0., 0.]]);
    }
    #[test]
    fn ray_tree_rejects_cycle_and_truncation() {
        let b = fixture();
        for end in 0..b.len() {
            assert!(decode_ray_tree(&b[..end]).is_err());
        }
        let mut cyclic = b;
        cyclic[120..122].copy_from_slice(&0u16.to_le_bytes());
        assert!(decode_ray_tree(&cyclic).is_err());
    }
}

impl CollisionMesh {
    /// Validate the original root-zero hierarchy once before retaining it for simulation.
    pub fn validate_tree(&self) -> Result<()> {
        let mut stack = vec![0_u16];
        let mut seen = vec![false; self.nodes.len()];
        while let Some(child) = stack.pop() {
            if child & 0x8000 != 0 {
                let face = self
                    .triangles
                    .get((child & 0x7fff) as usize)
                    .ok_or("COL triangle leaf outside table")?;
                if face
                    .indices
                    .iter()
                    .any(|&i| i as usize >= self.vertices.len())
                {
                    return Err("COL triangle references missing vertex".into());
                }
            } else {
                let i = child as usize;
                let node = self
                    .nodes
                    .get(i)
                    .ok_or("COL triangle child outside node table")?;
                if seen[i] {
                    return Err("COL triangle hierarchy is not a tree".into());
                }
                seen[i] = true;
                if node.half_extents.iter().any(|v| !v.is_finite() || *v < 0.0)
                    || !node.bound_measure.is_finite()
                    || !node.center.iter().all(|v| v.is_finite())
                {
                    return Err("invalid COL triangle node bounds".into());
                }
                stack.extend(node.children);
            }
        }
        Ok(())
    }
    /// Runtime face normals are packed by00330850, including its asymmetric
    /// +0.5 then truncation for negative components, and unpacked by0010adc0.
    pub fn query_normal(&self, triangle: usize) -> Result<[f32; 3]> {
        let face = self.triangles.get(triangle).ok_or("missing COL triangle")?;
        Ok(face.normal.map(|x| {
            let packed = ((x * 32767.0 + 0.5) as i32) as i16;
            f32::from(packed) * (1.0_f32 / 32767.0)
        }))
    }
}
