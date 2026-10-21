//! Native vegetation mesh preparation. ESB cylinder records become the same
//! type-0 acceleration tree consumed by chassis bundles and plain segments.
use crate::{
    Result,
    collision::{CollisionMesh, RayTreeNode, Triangle},
    vegetation_proxy::CylinderMesh,
    wheel::{Vec3, cross, dot, scale, sub},
};

#[derive(Clone, Copy)]
struct Face {
    indices: [u16; 3],
    normal: Vec3,
    distance: f32,
    center: Vec3,
}

fn bounds(faces: &[Face], vertices: &[Vec3]) -> (Vec3, Vec3, f32) {
    let mut low = vertices[usize::from(faces[0].indices[0])];
    let mut high = low;
    for face in faces {
        for index in face.indices {
            let point = vertices[usize::from(index)];
            for i in 0..3 {
                if point[i] < low[i] {
                    low[i] = point[i];
                }
                if high[i] < point[i] {
                    high[i] = point[i];
                }
            }
        }
    }
    let center = std::array::from_fn(|i| (low[i] + high[i]) * 0.5);
    let half: Vec3 = std::array::from_fn(|i| (high[i] - low[i]) * 0.5);
    //00118020 uses full dimensions, clamped per axis, then Z*(X*Y).
    let extent = half.map(|x| (2. * x).abs().max(0.01));
    (center, half, extent[2] * (extent[0] * extent[1]))
}

fn partition(faces: &[Face], axis: usize, plane: f32) -> (Vec<Face>, Vec<Face>) {
    let (mut first, mut second): (Vec<_>, Vec<_>) =
        faces.iter().copied().partition(|f| plane < f.center[axis]);
    //00117850 scores a balanced fallback when either side is empty.
    if first.is_empty() || second.is_empty() {
        first = faces[..faces.len() / 2].to_vec();
        second = faces[faces.len() / 2..].to_vec();
    }
    (first, second)
}

fn split(faces: &[Face], vertices: &[Vec3]) -> (usize, f32) {
    let mut low = faces[0].center;
    let mut high = low;
    for face in faces {
        for i in 0..3 {
            if face.center[i] < low[i] {
                low[i] = face.center[i];
            }
            if high[i] < face.center[i] {
                high[i] = face.center[i];
            }
        }
    }
    let width: Vec3 = std::array::from_fn(|i| high[i] - low[i]);
    let mut axis = usize::from(width[0].max(0.) < width[1]);
    if width[axis] < width[2] {
        axis = 2;
    }
    let plane = |axis: usize, step: usize| low[axis] + step as f32 * (width[axis] / 20.);
    let score = |axis, step| {
        let (a, b) = partition(faces, axis, plane(axis, step));
        bounds(&a, vertices).2 + bounds(&b, vertices).2
    };
    let mut step = 4;
    let mut best = score(axis, step);
    //00118420 scans18 candidate planes on each axis. Exact ties retain the
    //initial or earlier choice; replacing with a median changes face order.
    for candidate in 1..=18 {
        for candidate_axis in 0..3 {
            let value = score(candidate_axis, candidate);
            if value < best {
                best = value;
                axis = candidate_axis;
                step = candidate;
            }
        }
    }
    (axis, plane(axis, step))
}

fn build_node(
    faces: &mut [Face],
    vertices: &[Vec3],
    offset: usize,
    nodes: &mut Vec<RayTreeNode>,
) -> u16 {
    let index = nodes.len() as u16;
    let (center, half_extents, bound_measure) = bounds(faces, vertices);
    nodes.push(RayTreeNode {
        center,
        half_extents,
        bound_measure,
        children: [0; 2],
    });
    let (axis, plane) = split(faces, vertices);
    //00119940 stably puts centers above the plane first, then the other side.
    let (first, second) = partition(faces, axis, plane);
    let count = first.len();
    faces[..count].copy_from_slice(&first);
    faces[count..].copy_from_slice(&second);
    //Two sequential recursions retain native preorder node allocation.
    let first = if count == 1 {
        0x8000 | offset as u16
    } else {
        build_node(&mut faces[..count], vertices, offset, nodes)
    };
    let second = if faces.len() - count == 1 {
        0x8000 | (offset + count) as u16
    } else {
        build_node(&mut faces[count..], vertices, offset + count, nodes)
    };
    nodes[index as usize].children = [first, second];
    index
}

///0032eb70→00119ec0: recover face normals/planes and the original exhaustive
///volume-scored tree. No caps or analytic cylinder surfaces are introduced.
///The zero-size source cylinder retains all32 degenerate face slots.
pub fn cylinder_collision(input: &CylinderMesh) -> Result<CollisionMesh> {
    if input.vertices.iter().flatten().any(|v| !v.is_finite())
        || input
            .triangles
            .iter()
            .flatten()
            .any(|&i| usize::from(i) >= input.vertices.len())
    {
        return Err("invalid cylinder mesh constructor input".into());
    }
    let identity = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let mut faces: Vec<_> = input
        .triangles
        .iter()
        .map(|&indices| {
            let [a, b, c] = indices.map(|i| input.vertices[usize::from(i)]);
            let face = cross(sub(b, a), sub(c, a));
            let squared = dot(face, face);
            let normal = if squared == 0. {
                [0.; 3]
            } else {
                scale(face, (1. / f64::from(squared).sqrt()) as f32)
            };
            Face {
                indices,
                normal,
                distance: -dot(normal, a),
                center: [0.; 3],
            }
        })
        .collect();
    //The builder applies its freshly initialized identity matrix to the input
    //vertices and normals before computing centroids. Keep the FP operations.
    let vertices: Vec<_> = input
        .vertices
        .iter()
        .map(|&p| crate::obstacle::transform_point(&identity, p))
        .collect();
    for face in &mut faces {
        face.normal = std::array::from_fn(|i| {
            (identity[i] * face.normal[0] + identity[i + 4] * face.normal[1])
                + identity[i + 8] * face.normal[2]
        });
        let [a, b, c] = face.indices.map(|i| vertices[usize::from(i)]);
        face.center = std::array::from_fn(|i| ((a[i] + b[i]) + c[i]) * f32::from_bits(0x3eaaaaab));
    }
    let mut nodes = Vec::with_capacity(31);
    build_node(&mut faces, &vertices, 0, &mut nodes);
    let triangles = faces
        .into_iter()
        .map(|face| Triangle {
            indices: face.indices,
            normal: face.normal,
            distance: face.distance,
            invalid_plane_bits: None,
        })
        .collect();
    Ok(CollisionMesh {
        transform: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
        vertices,
        vertex_normal_bits: None,
        triangles,
        broad_phase_nodes: nodes.len() as u32,
        nodes,
        flags: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Det;
    fn authored_cylinder() -> CylinderMesh {
        //Authored unit-radius, height-two fixture supplied to00119ec0. These
        //host trigonometric inputs test tree building independently of ESB's
        //separately verified native polynomial cylinder constructor.
        let vertices = std::array::from_fn(|i| {
            let angle = (i % 16) as f64 * std::f64::consts::PI / 8.;
            [
                angle.cos_d() as f32,
                if i < 16 { 2. } else { 0. },
                angle.sin_d() as f32,
            ]
        });
        let triangles = std::array::from_fn(|i| {
            let sector = (i / 2) as u16;
            let next = (sector + 1) % 16;
            if i % 2 == 0 {
                [sector, next, next + 16]
            } else {
                [sector, next + 16, sector + 16]
            }
        });
        CylinderMesh {
            vertices,
            triangles,
        }
    }
    #[test]
    fn source_tree_checkpoint_preserves_volume_split_and_reordered_faces() {
        //2,795,362 actual00119ec0/helper instructions produced this tree. The
        //root's second child24 and leaf order differ from a balanced tree.
        let mesh = cylinder_collision(&authored_cylinder()).unwrap();
        assert_eq!(mesh.nodes.len(), 31);
        assert_eq!(mesh.nodes[0].children, [1, 24]);
        assert_eq!(mesh.nodes[0].bound_measure.to_bits(), 1090519040);
        assert_eq!(
            mesh.nodes[1].center.map(f32::to_bits),
            [1041626650, 1065353216, 0]
        );
        assert_eq!(
            mesh.nodes[24].half_extents.map(f32::to_bits),
            [1041626650, 1065353216, 1060439283]
        );
        assert_eq!(
            mesh.triangles.iter().map(|t| t.indices).collect::<Vec<_>>(),
            [
                [0, 1, 17],
                [0, 17, 16],
                [15, 0, 16],
                [15, 16, 31],
                [1, 2, 18],
                [1, 18, 17],
                [14, 15, 31],
                [14, 31, 30],
                [3, 4, 20],
                [3, 20, 19],
                [4, 5, 21],
                [4, 21, 20],
                [2, 3, 19],
                [2, 19, 18],
                [5, 6, 22],
                [5, 22, 21],
                [13, 14, 30],
                [13, 30, 29],
                [10, 11, 27],
                [10, 27, 26],
                [12, 13, 29],
                [12, 29, 28],
                [11, 12, 28],
                [11, 28, 27],
                [6, 7, 23],
                [6, 23, 22],
                [9, 10, 26],
                [9, 26, 25],
                [7, 8, 24],
                [7, 24, 23],
                [8, 9, 25],
                [8, 25, 24]
            ]
        );
        assert_eq!(mesh.triangles[0].distance.to_bits(), 3212514495);
        assert_eq!(
            mesh.query_normal(0).unwrap(),
            [32137., 0., 6393.].map(|x| x * (1_f32 / 32767.))
        );
    }
    #[test]
    fn side_contact_stays_faceted_and_open_caps_have_no_support() {
        let mesh = std::rc::Rc::new(cylinder_collision(&authored_cylinder()).unwrap());
        let object = crate::obstacle::StaticObstacle {
            mesh,
            world_matrix: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
        };
        let rays = [
            crate::collision::CollisionRay {
                start: [2., 1., 0.1],
                end: [0., 1., 0.1],
            },
            crate::collision::CollisionRay {
                start: [0., 4., 0.],
                end: [0., -2., 0.],
            },
        ];
        let hits = crate::wreck::query_static_segments(&rays, &[object], 60).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].ray_index, 0);
        assert_eq!(hits[0].world_point[1..], [1., 0.1]);
        assert!(hits[0].world_point[0] < (1_f32 - 0.1_f32 * 0.1).sqrt());
        assert!(hits[0].world_normal[0] > 0.98 && hits[0].world_normal[2] > 0.19);
    }
    #[test]
    fn zero_size_native_slot_keeps_tree_and_cannot_emit_contacts() {
        let input = CylinderMesh {
            vertices: [[1., 2., 3.]; 32],
            triangles: authored_cylinder().triangles,
        };
        let mesh = std::rc::Rc::new(cylinder_collision(&input).unwrap());
        assert_eq!(mesh.nodes.len(), 31);
        assert_eq!(mesh.triangles.len(), 32);
        assert!(mesh.triangles.iter().all(|f| f.normal == [0.; 3]));
        let object = crate::obstacle::StaticObstacle {
            mesh,
            world_matrix: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
        };
        let ray = crate::collision::CollisionRay {
            start: [1., 2., 4.],
            end: [1., 2., 2.],
        };
        assert!(
            crate::wreck::query_static_segments(&[ray], &[object], 60)
                .unwrap()
                .is_empty()
        );
    }
}
