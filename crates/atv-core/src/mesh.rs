//! PSG v3/v4 rigid mesh recovery. Returns the stored bind pose, not animation or
//! the original VU rendering pipeline. Unsupported packet forms fail explicitly.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Node {
    pub name: String,
    pub parent: i32,
    pub local_matrix: [f32; 16],
    pub world_matrix: [f32; 16],
    /// Stored local center XYZ, half-extents XYZ, and bounding-sphere radius.
    /// The final radius is not a wheel's rolling radius.
    pub bounds: [f32; 7],
}
#[derive(Debug, Serialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub local_position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Optional unsigned VIF V4-8 color, preserved before original shader scaling.
    pub color: Option<[u8; 4]>,
    pub node: u16,
}
#[derive(Debug, Serialize)]
pub struct Group {
    pub material: u32,
    pub flags: u32,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
#[derive(Debug, Serialize)]
pub struct Lod {
    pub threshold: f32,
    pub groups: Vec<Group>,
}
#[derive(Debug, Serialize)]
pub struct Mesh {
    pub nodes: Vec<Node>,
    pub materials: Vec<String>,
    pub lods: Vec<Lod>,
    /// Some vehicle files have extra records whose semantics are not recovered.
    pub trailing_bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct BindBounds {
    pub center: [f32; 3],
    pub half_extents: [f32; 3],
    pub sphere_center: [f32; 3],
    /// Original aggregate algorithm uses maximum absolute center displacement,
    /// not Euclidean displacement; preserve it for original inertia behavior.
    pub sphere_radius: f32,
}
impl Mesh {
    /// Original PSG bounds aggregation (001b3ed0, 00167600, 00167730), evaluated
    /// at the loaded bind pose. All loaded PSG nodes have the own-bounds flag.
    pub fn bind_bounds(&self) -> Result<BindBounds> {
        let root = self.nodes.first().ok_or("PSG has no bounds root")?;
        let root_matrix = &root.world_matrix;
        let inverse_point = |p: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|i| {
                (0..3)
                    .map(|j| root_matrix[i * 4 + j] * (p[j] - root_matrix[12 + j]))
                    .sum()
            })
        };
        let mut low = std::array::from_fn::<_, 3, _>(|i| root.bounds[i] - root.bounds[3 + i]);
        let mut high = std::array::from_fn::<_, 3, _>(|i| root.bounds[i] + root.bounds[3 + i]);
        let mut sphere_center = point(
            root_matrix,
            [root.bounds[0], root.bounds[1], root.bounds[2]],
            true,
        );
        let mut sphere_radius = root.bounds[6];
        for node in &self.nodes {
            let world_center = point(
                &node.world_matrix,
                [node.bounds[0], node.bounds[1], node.bounds[2]],
                true,
            );
            if sphere_radius > 0. {
                let displacement = (0..3)
                    .map(|i| (world_center[i] - sphere_center[i]).abs())
                    .fold(0f32, f32::max);
                sphere_radius = sphere_radius.max(displacement + node.bounds[6]);
            } else {
                sphere_radius = node.bounds[6];
                sphere_center = world_center;
            }
            let center = inverse_point(world_center);
            let half: [f32; 3] = std::array::from_fn(|i| {
                (0..3)
                    .map(|k| {
                        let axis: f32 = (0..3)
                            .map(|j| root_matrix[i * 4 + j] * node.world_matrix[k * 4 + j])
                            .sum();
                        (axis * node.bounds[k + 3]).abs()
                    })
                    .sum()
            });
            for i in 0..3 {
                low[i] = low[i].min(center[i] - half[i]);
                high[i] = high[i].max(center[i] + half[i]);
            }
        }
        let result = BindBounds {
            center: std::array::from_fn(|i| (low[i] + high[i]) * 0.5),
            half_extents: std::array::from_fn(|i| (high[i] - low[i]) * 0.5),
            sphere_center: inverse_point(sphere_center),
            sphere_radius,
        };
        if !sphere_radius.is_finite() || result.half_extents.iter().any(|v| !v.is_finite()) {
            return Err("PSG aggregate bounds overflow".into());
        }
        Ok(result)
    }
}
fn float(data: &[u8], at: usize) -> Result<f32> {
    let value = f32::from_bits(u32le(data, at)?);
    if !value.is_finite() {
        return Err("nonfinite PSG value".into());
    }
    Ok(value)
}
fn name(data: &[u8], at: usize, len: usize) -> Result<String> {
    let raw = slice(data, at, len)?;
    let end = raw
        .iter()
        .position(|&v| v == 0)
        .ok_or("unterminated PSG name")?;
    String::from_utf8(raw[..end].to_vec()).map_err(|_| "non-UTF8 PSG name".into())
}
fn point(matrix: &[f32; 16], value: [f32; 3], translation: bool) -> [f32; 3] {
    std::array::from_fn(|row| {
        (0..3).map(|k| matrix[k * 4 + row] * value[k]).sum::<f32>()
            + if translation { matrix[12 + row] } else { 0. }
    })
}
fn packet(
    data: &[u8],
    nodes: &[Node],
    scales: &[f32; 4],
    material: u32,
    flags: u32,
    expected_vertices: usize,
) -> Result<Group> {
    let mut group = Group {
        material,
        flags,
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut at = 0;
    let mut batch: Option<(usize, usize)> = None;
    while at < data.len() {
        let code = u32le(data, at)?;
        at += 4;
        let command = code >> 24;
        let count = ((code >> 16) & 255) as usize;
        let address = code & 65535;
        match command {
            0 => {}
            0x17 => {
                batch = None;
            }
            0x6c | 0x6d | 0x6a | 0x65 | 0x6e => {
                if count == 0 {
                    return Err("unsupported zero-count PSG VIF unpack".into());
                }
                let stride = match command {
                    0x6c => 16,
                    0x6d => 8,
                    0x6a => 3,
                    _ => 4,
                };
                let raw = slice(data, at, count * stride)?;
                at += (count * stride + 3) & !3;
                if command == 0x6c && count == 1 && address == 0x8000 {
                    continue;
                }
                if matches!(command, 0x6c | 0x6d) && address == 0x8001 {
                    let base = group.vertices.len();
                    if base + count > expected_vertices {
                        return Err("PSG packet exceeds declared vertex count".into());
                    }
                    for i in 0..count {
                        let mut components = [0i32; 4];
                        for (j, value) in components.iter_mut().enumerate() {
                            *value = if command == 0x6d {
                                u16le(raw, i * stride + j * 2)? as i16 as i32
                            } else {
                                u32le(raw, i * stride + j * 4)? as i32
                            };
                        }
                        let weight = components[3];
                        let magnitude = weight.unsigned_abs();
                        if magnitude == 0 || magnitude % 16 != 0 {
                            return Err("unsupported PSG rigid node encoding".into());
                        }
                        let node = (magnitude / 16 - 1) as usize;
                        let transform = &nodes
                            .get(node)
                            .ok_or("PSG vertex references missing node")?
                            .world_matrix;
                        let local_position =
                            std::array::from_fn(|k| components[k] as f32 * scales[0]);
                        let position = point(transform, local_position, true);
                        if position.iter().any(|v| !v.is_finite()) {
                            return Err("PSG transformed position overflow".into());
                        }
                        group.vertices.push(Vertex {
                            position,
                            local_position,
                            normal: [0.; 3],
                            uv: [0.; 2],
                            color: None,
                            node: node as u16,
                        });
                        if i >= 2 && weight > 0 {
                            let a = (base + i - 2) as u32;
                            group.indices.extend(if i % 2 == 0 {
                                [a, a + 1, a + 2]
                            } else {
                                [a + 1, a, a + 2]
                            });
                        }
                    }
                    batch = Some((base, count));
                } else if matches!(command, 0x6a | 0x65 | 0x6e) {
                    let (base, positions) = batch.ok_or("PSG attributes precede positions")?;
                    let expected_address = match command {
                        0x6a => 0x8001 + count as u32,
                        0x65 => 0x8001 + count as u32 * 2,
                        _ => 0xc001 + count as u32 * 3,
                    };
                    if address != expected_address {
                        return Err("unsupported PSG attribute destination".into());
                    }
                    if positions != count {
                        return Err("PSG attribute count differs from positions".into());
                    }
                    for i in 0..count {
                        let vertex = &mut group.vertices[base + i];
                        if command == 0x6a {
                            let normal =
                                std::array::from_fn(|k| raw[i * 3 + k] as i8 as f32 * scales[1]);
                            vertex.normal =
                                point(&nodes[vertex.node as usize].world_matrix, normal, false);
                        } else if command == 0x65 {
                            vertex.uv = [
                                u16le(raw, i * 4)? as i16 as f32 * scales[2],
                                u16le(raw, i * 4 + 2)? as i16 as f32 * scales[3],
                            ];
                        } else {
                            vertex.color = Some(raw[i * 4..i * 4 + 4].try_into().unwrap());
                        }
                        if vertex
                            .normal
                            .iter()
                            .chain(vertex.uv.iter())
                            .any(|v| !v.is_finite())
                        {
                            return Err("PSG vertex attribute overflow".into());
                        }
                    }
                } else {
                    return Err(format!("unsupported PSG VIF unpack {code:#x}"));
                }
            }
            _ => return Err(format!("unsupported PSG VIF command {command:#x}")),
        }
    }
    if at != data.len() || group.vertices.len() != expected_vertices {
        return Err("PSG vertex/packet extent mismatch".into());
    }
    Ok(group)
}

pub fn decode_psg(data: &[u8]) -> Result<Mesh> {
    decode_psg_root(data, false)
}

/// Geometry relative to the model root for a static scene placement.
/// 00206ba0 replaces the loaded root translation (001657c0) and basis
/// (00164dd0), rather than composing the scene matrix with its bind matrix.
/// Decode with an identity root so descendants retain their authored local
/// transforms, including normals, without an approximate inverse of the bind.
/// Vehicle, rider and ecosystem readers continue to use the stored bind pose.
pub fn decode_psg_for_scene(data: &[u8]) -> Result<Mesh> {
    decode_psg_root(data, true)
}

fn decode_psg_root(data: &[u8], scene_root: bool) -> Result<Mesh> {
    if data.len() > 128 * 1024 * 1024 {
        return Err("PSG exceeds 128 MiB limit".into());
    }
    // 001b2d40 reads the version word at 001b3198, then uses the same node,
    // material and geometry readers without inspecting it. Both observed
    // versions use this layout; retain explicit admission and packet checks
    // rather than accepting arbitrary future versions.
    if slice(data, 0, 4)? != b"psg\0" || !matches!(u32le(data, 4)?, 3 | 4) {
        return Err("unsupported PSG signature/version".into());
    }
    let count = u32le(data, 8)? as usize;
    if count == 0 || count > 256 {
        return Err("unsupported PSG node count".into());
    }
    let mut nodes: Vec<Node> = Vec::with_capacity(count);
    let mut at = 12;
    for index in 0..count {
        slice(data, at, 224)?;
        let name = name(data, at, 128)?;
        let parent = u32le(data, at + 220)? as i32;
        if parent < -1 || parent >= index as i32 {
            return Err("PSG node hierarchy is not parent-first".into());
        }
        let mut local_matrix = [0.; 16];
        for (i, value) in local_matrix.iter_mut().enumerate() {
            *value = float(data, at + 128 + i * 4)?;
        }
        let world_matrix = if scene_root && index == 0 {
            [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ]
        } else if parent < 0 {
            local_matrix
        } else {
            let pm = &nodes[parent as usize].world_matrix;
            std::array::from_fn(|i| {
                (0..4)
                    .map(|k| pm[k * 4 + i % 4] * local_matrix[i / 4 * 4 + k])
                    .sum()
            })
        };
        let mut bounds = [0.; 7];
        for (i, value) in bounds.iter_mut().enumerate() {
            *value = float(data, at + 192 + i * 4)?;
        }
        if bounds[3..].iter().any(|&value| value < 0.) {
            return Err("negative PSG bounds extent".into());
        }
        nodes.push(Node {
            name,
            parent,
            local_matrix,
            world_matrix,
            bounds,
        });
        at += 224;
    }
    let materials_count = u32le(data, at)? as usize;
    at += 4;
    if materials_count == 0 || materials_count > 4096 {
        return Err("unsupported PSG material count".into());
    }
    let mut materials = Vec::with_capacity(materials_count);
    for _ in 0..materials_count {
        materials.push(name(data, at, 64)?);
        at += 64;
    }
    let scales = [
        float(data, at)?,
        float(data, at + 4)?,
        float(data, at + 8)?,
        float(data, at + 12)?,
    ];
    if scales.iter().any(|&s| s <= 0.) || slice(data, at + 16, 16)?.iter().any(|&v| v != 0) {
        return Err("unsupported PSG vertex scale header".into());
    }
    at += 32;
    let lod_count = u32le(data, at)?;
    at += 4;
    if lod_count == 0 || lod_count > 8 {
        return Err("unsupported PSG LOD count".into());
    }
    let mut lods = Vec::new();
    let mut total_vertices = 0;
    for _ in 0..lod_count {
        let threshold = float(data, at)?;
        let group_count = u32le(data, at + 4)?;
        at += 8;
        if group_count > 10000 {
            return Err("too many PSG groups".into());
        }
        let mut groups = Vec::new();
        for _ in 0..group_count {
            let material = u32le(data, at)?;
            let flags = u32le(data, at + 4)?;
            let vertices = (u32le(data, at + 12)? & 65535) as usize;
            let size = u32le(data, at + 16)? as usize;
            at += 20;
            if material as usize >= materials_count {
                return Err("PSG group references missing material".into());
            }
            total_vertices += vertices;
            if total_vertices > 2_000_000 {
                return Err("PSG vertex count exceeds limit".into());
            }
            groups.push(packet(
                slice(data, at, size)?,
                &nodes,
                &scales,
                material,
                flags,
                vertices,
            )?);
            at += size;
        }
        lods.push(Lod { threshold, groups });
    }
    let trailing_bytes = data.len() - at;
    Ok(Mesh {
        nodes,
        materials,
        lods,
        trailing_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic_psg(version: u32) -> Vec<u8> {
        let mut data = b"psg\0".to_vec();
        data.extend(version.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        let node = identity();
        let mut node_name = [0; 128];
        node_name[..4].copy_from_slice(b"test");
        data.extend(node_name);
        for value in node.local_matrix {
            data.extend(value.to_le_bytes());
        }
        for value in [0f32, 1., 0., 1., 1., 0., 2.] {
            data.extend(value.to_le_bytes());
        }
        data.extend((-1i32).to_le_bytes());
        data.extend(1u32.to_le_bytes());
        let mut material = [0; 64];
        material[..5].copy_from_slice(b"stone");
        data.extend(material);
        for value in [0.5f32, 1., 0.25, 0.25] {
            data.extend(value.to_le_bytes());
        }
        data.extend([0; 16]);
        data.extend(1u32.to_le_bytes());
        data.extend(100f32.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        let mut geometry = 0x6d038001u32.to_le_bytes().to_vec();
        for position in [[-2i16, 0, 0, -16], [2, 0, 0, -16], [0, 4, 0, 16]] {
            for component in position {
                geometry.extend(component.to_le_bytes());
            }
        }
        geometry.extend(0x6a038004u32.to_le_bytes());
        geometry.extend([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0]);
        geometry.extend(0x65038007u32.to_le_bytes());
        for uv in [[0i16, 0], [4, 0], [2, 4]] {
            for component in uv {
                geometry.extend(component.to_le_bytes());
            }
        }
        geometry.extend(0x17000000u32.to_le_bytes());
        for value in [0u32, 7, 0, 3, geometry.len() as u32] {
            data.extend(value.to_le_bytes());
        }
        data.extend(geometry);
        data
    }
    #[test]
    fn static_placement_replaces_root_and_preserves_child_positions_and_normals() {
        let mut data = synthetic_psg(3);
        let root = [
            0f32, 2., 0., 0., -3., 0., 0., 0., 0., 0., 4., 0., 100., 20., -300., 1.,
        ];
        for (i, value) in root.iter().enumerate() {
            data[12 + 128 + i * 4..12 + 132 + i * 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut child = data[12..236].to_vec();
        let child_matrix = [
            1f32, 0., 0., 0., 0., 0., -1., 0., 0., 2., 0., 0., 5., 6., 7., 1.,
        ];
        for (i, value) in child_matrix.iter().enumerate() {
            child[128 + i * 4..132 + i * 4].copy_from_slice(&value.to_le_bytes());
        }
        child[220..224].copy_from_slice(&0i32.to_le_bytes());
        data.splice(236..236, child);
        data[8..12].copy_from_slice(&2u32.to_le_bytes());
        let packet = 12 + 448 + 4 + 64 + 32 + 4 + 8 + 20;
        for i in 0..3 {
            let weight = if i == 2 { 32i16 } else { -32 };
            let offset = packet + 4 + i * 8 + 6;
            data[offset..offset + 2].copy_from_slice(&weight.to_le_bytes());
        }
        let bind = decode_psg(&data).unwrap();
        let scene = decode_psg_for_scene(&data).unwrap();
        let bind_vertex = &bind.lods[0].groups[0].vertices[0];
        let relative_vertex = &scene.lods[0].groups[0].vertices[0];
        assert_eq!(bind_vertex.position, [82., 28., -272.]);
        assert_eq!(relative_vertex.position, [4., 6., 7.]);
        assert_eq!(bind_vertex.normal, [-6., 0., 0.]);
        assert_eq!(relative_vertex.normal, [0., 2., 0.]);
        assert_eq!(scene.lods[0].groups[0].vertices[2].position, [5., 6., 5.]);
        assert_eq!(scene.nodes[0].local_matrix, root);
        assert_eq!(scene.nodes[1].world_matrix, child_matrix);
        assert_eq!(
            scene.lods[0].groups[0].indices,
            bind.lods[0].groups[0].indices
        );
        // Exercise the same scene orientation interface used by the WASM loader.
        let placement =
            crate::scene::placement_matrix([10., 40., 30.], [1., 0., 0.], [0., 1., 0.]).unwrap();
        assert_eq!(
            point(&placement, relative_vertex.position, true),
            [17., 46., 26.]
        );
        assert_eq!(
            point(&placement, relative_vertex.normal, false),
            [0., 2., 0.]
        );
    }

    #[test]
    fn v3_and_v4_share_the_bounded_geometry_layout() {
        for version in [3, 4] {
            let data = synthetic_psg(version);
            let mesh = decode_psg(&data).unwrap();
            assert_eq!(mesh.nodes[0].name, "test");
            assert_eq!(mesh.nodes[0].bounds, [0., 1., 0., 1., 1., 0., 2.]);
            assert_eq!(mesh.materials, ["stone"]);
            assert_eq!(mesh.lods[0].threshold, 100.);
            let group = &mesh.lods[0].groups[0];
            assert_eq!(group.flags, 7);
            assert_eq!(group.vertices[0].position, [-1., 0., 0.]);
            assert_eq!(group.vertices[2].position, [0., 2., 0.]);
            assert_eq!(group.vertices[2].normal, [0., 0., 1.]);
            assert_eq!(group.vertices[2].uv, [0.5, 1.]);
            assert_eq!(group.indices, [0, 1, 2]);
            assert_eq!(mesh.trailing_bytes, 0);
            for end in 0..data.len() {
                assert!(decode_psg(&data[..end]).is_err());
            }
        }
        assert!(
            decode_psg(&synthetic_psg(5))
                .unwrap_err()
                .contains("signature/version")
        );
    }
    #[test]
    fn v4_retains_declared_vertex_and_packet_validation() {
        let mut data = synthetic_psg(4);
        // Header + node + material table + scales + LOD header, then group.
        let group = 12 + 224 + 4 + 64 + 32 + 4 + 8;
        data[group + 12..group + 16].copy_from_slice(&2u32.to_le_bytes());
        assert!(decode_psg(&data).unwrap_err().contains("vertex count"));
        data[group + 12..group + 16].copy_from_slice(&3u32.to_le_bytes());
        data[group + 20..group + 24].copy_from_slice(&0xff038001u32.to_le_bytes());
        assert!(decode_psg(&data).unwrap_err().contains("VIF command"));
    }
    fn identity() -> Node {
        Node {
            name: "test".into(),
            parent: -1,
            bounds: [0.; 7],
            local_matrix: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
            world_matrix: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
        }
    }
    #[test]
    fn aggregate_bounds_preserve_original_max_axis_sphere_rule() {
        let mut root = identity();
        root.bounds = [0., 0., 0., 1., 1., 1., 2.];
        let mut child = identity();
        child.parent = 0;
        child.world_matrix[12] = 3.;
        child.world_matrix[13] = 4.;
        child.bounds = [0., 0., 0., 0.5, 0.5, 0.5, 1.];
        let mesh = Mesh {
            nodes: vec![root, child],
            materials: vec![],
            lods: vec![],
            trailing_bytes: 0,
        };
        let bounds = mesh.bind_bounds().unwrap();
        assert_eq!(bounds.center, [1.25, 1.75, 0.]);
        assert_eq!(bounds.half_extents, [2.25, 2.75, 1.]);
        // Original vmaxy/vmaxz uses max(3,4)+1=5, not sqrt(3²+4²)+1=6.
        assert_eq!(bounds.sphere_radius, 5.);
    }
    #[test]
    fn signed_rigid_vertices_and_strip_restart() {
        let mut data = 0x6d048001u32.to_le_bytes().to_vec();
        for v in [
            [0i16, 0, 0, -16],
            [2, 0, 0, -16],
            [0, 2, 0, 16],
            [2, 2, 0, 16],
        ] {
            for c in v {
                data.extend(c.to_le_bytes());
            }
        }
        let color_at = data.len();
        data.extend(0x6e04c00du32.to_le_bytes());
        for color in [
            [255, 128, 100, 0],
            [1, 2, 3, 4],
            [128; 4],
            [100, 100, 100, 25],
        ] {
            data.extend(color);
        }
        data.extend(0x17000000u32.to_le_bytes());
        let mesh = packet(&data, &[identity()], &[0.5, 1., 1., 1.], 0, 0, 4).unwrap();
        assert_eq!(mesh.vertices[2].position, [0., 1., 0.]);
        assert_eq!(mesh.indices, [0, 1, 2, 2, 1, 3]);
        assert_eq!(mesh.vertices[0].color, Some([255, 128, 100, 0]));
        data[color_at..color_at + 4].copy_from_slice(&0x6e04800du32.to_le_bytes());
        assert!(
            packet(&data, &[identity()], &[0.5, 1., 1., 1.], 0, 0, 4)
                .unwrap_err()
                .contains("destination")
        );
        data[color_at..color_at + 4].copy_from_slice(&0x6e04c00du32.to_le_bytes());
        data[10..12].copy_from_slice(&(-32i16).to_le_bytes());
        assert!(
            packet(&data, &[identity()], &[0.5, 1., 1., 1.], 0, 0, 4)
                .unwrap_err()
                .contains("missing node")
        );
    }
    #[test]
    fn parent_transform_composition_column_major() {
        let mut matrix = identity().world_matrix;
        matrix[12] = 3.;
        matrix[14] = -2.;
        assert_eq!(point(&matrix, [1., 2., 3.], true), [4., 2., 1.]);
        assert_eq!(point(&matrix, [1., 2., 3.], false), [1., 2., 3.]);
    }
}
