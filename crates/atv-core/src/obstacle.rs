//! Recovered ray-bundle/triangle-tree query, including original broad phases
//! and flat-normal leaf rules. Scene filtering and moving-object response are
//! separate stages.
use crate::wheel::{Vec3, add, cross, dot, scale, sub};
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub point: Vec3,
    pub normal: Vec3,
    pub fraction: f32,
}
/// Tests a ray of the supplied length and direction against one original face.
/// Retains the game's one-sided and projected-edge boundary rules. The caller
/// supplies a normalized direction as in0010b800.
pub fn triangle_hit(
    start: Vec3,
    direction: Vec3,
    length: f32,
    vertices: [Vec3; 3],
) -> Option<RayHit> {
    if length <= 0.0
        || !length.is_finite()
        || !start
            .iter()
            .chain(direction.iter())
            .chain(vertices.iter().flatten())
            .all(|x| x.is_finite())
    {
        return None;
    }
    let a = vertices[0];
    let b = vertices[1];
    let c = vertices[2];
    let face = cross(sub(b, a), sub(c, a));
    let squared = dot(face, face);
    if squared == 0.0 || !squared.is_finite() {
        return None;
    }
    let normal = scale(face, (1.0 / f64::from(squared).sqrt()) as f32);
    let facing = dot(normal, direction);
    if facing >= 0.0 {
        return None;
    }
    let distance = dot(normal, sub(a, start)) / facing;
    if distance < 0.0 || distance > length {
        return None;
    }
    let point = add(start, scale(direction, distance));
    let [nx, ny, nz] = normal;
    let axes = if nx * nx < ny * ny || nx * nx < nz * nz {
        if nz * nz <= ny * ny { [0, 2] } else { [0, 1] }
    } else {
        [2, 1]
    };
    let [x, y] = axes;
    let delta = [point[x] - a[x], point[y] - a[y]];
    let ab = [b[x] - a[x], b[y] - a[y]];
    let bc = [c[x] - b[x], c[y] - b[y]];
    let denominator = delta[1] * bc[0] - bc[1] * delta[0];
    if denominator == 0.0 {
        return None;
    }
    let ratio = (bc[0] * ab[1] - bc[1] * ab[0]) / denominator;
    if ratio < 1.0 {
        return None;
    }
    let along = if bc[1] * bc[1] < bc[0] * bc[0] {
        (delta[0] * ratio - ab[0]) / bc[0]
    } else {
        (delta[1] * ratio - ab[1]) / bc[1]
    };
    if !(0.0..=1.0).contains(&along) {
        return None;
    }
    Some(RayHit {
        point,
        normal,
        fraction: distance / length,
    })
}
/// Column-major rigid transform, with the original XYZ accumulation order.
pub fn transform_point(matrix: &[f32; 16], point: Vec3) -> Vec3 {
    std::array::from_fn(|i| {
        ((matrix[i] * point[0] + matrix[4 + i] * point[1]) + matrix[8 + i] * point[2])
            + matrix[12 + i]
    })
}

/// Original segment/box broad phase (00109750). The endpoints belong to the
/// ray bundle; `ray_to_box` places them in the box coordinate system.
pub fn segment_box_overlap(
    center: Vec3,
    half_extents: Vec3,
    start: Vec3,
    end: Vec3,
    ray_to_box: &[f32; 16],
) -> bool {
    let start = sub(transform_point(ray_to_box, start), center);
    let end = sub(transform_point(ray_to_box, end), center);
    let midpoint = scale(add(start, end), 0.5);
    let half_segment = sub(midpoint, start);
    let absolute = half_segment.map(f32::abs);
    for i in 0..3 {
        if (midpoint[i].abs() - half_extents[i]) - absolute[i] > 0.0 {
            return false;
        }
    }
    let [x, y, z] = midpoint;
    let [dx, dy, dz] = half_segment;
    (y * dz - z * dy).abs() <= half_extents[2] * absolute[1] + half_extents[1] * absolute[2]
        && (x * dz - z * dx).abs() <= half_extents[2] * absolute[0] + half_extents[0] * absolute[2]
        && (x * dy - y * dx).abs() <= half_extents[1] * absolute[0] + half_extents[0] * absolute[1]
}

/// Original 15-axis box/box broad phase (00108cb0). `second_to_first` is a
/// rigid transform; box half extents must be nonnegative finite values.
pub fn boxes_overlap(
    first_center: Vec3,
    first_half: Vec3,
    second_center: Vec3,
    second_half: Vec3,
    second_to_first: &[f32; 16],
) -> bool {
    let offset = sub(
        transform_point(second_to_first, second_center),
        first_center,
    );
    let axes: [Vec3; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| second_to_first[i * 4 + j]));
    let absolute = axes.map(|v| v.map(f32::abs));
    for i in 0..3 {
        let projected = (second_half[0] * absolute[0][i] + second_half[1] * absolute[1][i])
            + second_half[2] * absolute[2][i];
        if offset[i].abs() > first_half[i] + projected {
            return false;
        }
    }
    for i in 0..3 {
        let projected = (first_half[0] * absolute[i][0] + first_half[1] * absolute[i][1])
            + first_half[2] * absolute[i][2];
        if dot(offset, axes[i]).abs() > projected + second_half[i] {
            return false;
        }
    }
    for i in 0..3 {
        let a = (i + 1) % 3;
        let b = (i + 2) % 3;
        for j in 0..3 {
            let c = (j + 1) % 3;
            let d = (j + 2) % 3;
            let separation = (offset[b] * axes[j][a] + offset[a] * -axes[j][b]).abs();
            // The native routine sums larger coordinate indices first.
            let (a, b) = (a.min(b), a.max(b));
            let (c, d) = (c.min(d), c.max(d));
            let radius = ((first_half[b] * absolute[j][a] + first_half[a] * absolute[j][b])
                + second_half[d] * absolute[c][i])
                + second_half[c] * absolute[d][i];
            if separation > radius {
                return false;
            }
        }
    }
    true
}

/// Original triangle/box branch of0010adc0. Vertices and the unpacked stored
/// face normal must already be transformed into the box coordinate system.
pub fn triangle_box_overlap(
    center: Vec3,
    half_extents: Vec3,
    vertices: [Vec3; 3],
    stored_normal: Vec3,
) -> bool {
    let from_center = sub(vertices[0], center);
    let edge0 = sub(vertices[0], vertices[1]);
    let edge1 = sub(vertices[0], vertices[2]);
    let edge2 = sub(vertices[2], vertices[1]);
    let squared = dot(edge2, edge2);
    let edge2 = if squared == 0.0 {
        [0.0; 3]
    } else {
        scale(edge2, (1.0 / f64::from(squared).sqrt()) as f32)
    };
    let mut axes = [[0.0; 3]; 13];
    axes[0] = stored_normal;
    axes[1] = [1.0, 0.0, 0.0];
    axes[2] = [0.0, 1.0, 0.0];
    axes[3] = [0.0, 0.0, 1.0];
    for (i, [x, y, z]) in [edge0, edge1, edge2].into_iter().enumerate() {
        axes[4 + i * 3] = [0.0, -z, y];
        axes[5 + i * 3] = [z, 0.0, -x];
        axes[6 + i * 3] = [-y, x, 0.0];
    }
    for (i, axis) in axes.into_iter().enumerate() {
        let projection = std::array::from_fn::<_, 3, _>(|j| (axis[j] * half_extents[j]).abs());
        let radius = (projection[0] + projection[1]) + projection[2];
        let a = dot(from_center, axis);
        if i == 0 {
            if radius < a.abs() {
                return false;
            }
        } else {
            let b = a - dot(edge0, axis);
            let c = a - dot(edge1, axis);
            if radius < a.min(b.min(c)) || a.max(b.max(c)) < -radius {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_sided_face_and_original_vertex_boundary() {
        let v = [[-2., -2., 0.], [2., -2., 0.], [0., 2., 0.]];
        let hit = triangle_hit([0., 0., 1.], [0., 0., -1.], 2., v).unwrap();
        assert_eq!(hit.point, [0.; 3]);
        assert_eq!(hit.normal, [0., 0., 1.]);
        assert_eq!(hit.fraction, 0.5);
        assert!(triangle_hit([0., 0., -1.], [0., 0., 1.], 2., v).is_none());
        assert!(triangle_hit([-2., -2., 1.], [0., 0., -1.], 2., v).is_none());
    }
}

/// One original query result. Ray indices stay in first-hit traversal order;
/// a closer triangle replaces the result without moving its slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BundleHit {
    pub ray_index: usize,
    pub triangle_index: usize,
    pub hit: RayHit,
}

/// Recovered0010c140 traversal and0010b800 nearest-hit insertion for one
/// type-2 ray bundle against one unpacked type-0 mesh. Results remain in mesh
/// coordinates. Scene category filters and moving-body response are separate.
pub fn query_bundle(
    rays: &crate::collision::CollisionRayTree,
    mesh: &crate::collision::CollisionMesh,
    ray_to_mesh: &[f32; 16],
    mesh_to_ray: &[f32; 16],
) -> crate::Result<Vec<BundleHit>> {
    if rays.nodes.is_empty() || mesh.nodes.is_empty() {
        return Err("collision query requires original tree roots".into());
    }
    let mut stack = vec![(0_u16, 0_u16)];
    let mut hits: Vec<BundleHit> = Vec::new();
    let mut visits = 0;
    while let Some((ray_ref, mesh_ref)) = stack.pop() {
        visits += 1;
        if visits > 2_000_000 {
            return Err("collision tree pair traversal exceeded bound".into());
        }
        let ray_leaf = ray_ref & 0x8000 != 0;
        let mesh_leaf = mesh_ref & 0x8000 != 0;
        let ri = (ray_ref & 0x7fff) as usize;
        let mi = (mesh_ref & 0x7fff) as usize;
        let ray_node = if ray_leaf {
            None
        } else {
            Some(rays.nodes.get(ri).ok_or("invalid ray tree child")?)
        };
        let mesh_node = if mesh_leaf {
            None
        } else {
            Some(mesh.nodes.get(mi).ok_or("invalid mesh tree child")?)
        };
        match (ray_node, mesh_node) {
            (Some(a), Some(b)) => {
                if !boxes_overlap(
                    b.center,
                    b.half_extents,
                    a.center,
                    a.half_extents,
                    ray_to_mesh,
                ) {
                    continue;
                }
                if b.bound_measure < a.bound_measure {
                    stack.push((a.children[1], mesh_ref));
                    stack.push((a.children[0], mesh_ref));
                } else {
                    stack.push((ray_ref, b.children[1]));
                    stack.push((ray_ref, b.children[0]));
                }
            }
            (Some(a), None) => {
                let triangle = mesh.triangles.get(mi).ok_or("invalid mesh leaf")?;
                if triangle.invalid_plane_bits.is_some() {
                    continue;
                }
                let vertices = triangle
                    .indices
                    .map(|i| transform_point(mesh_to_ray, mesh.vertices[i as usize]));
                let normal = mesh.query_normal(mi)?;
                let normal = std::array::from_fn(|i| {
                    (mesh_to_ray[i] * normal[0] + mesh_to_ray[4 + i] * normal[1])
                        + mesh_to_ray[8 + i] * normal[2]
                });
                if triangle_box_overlap(a.center, a.half_extents, vertices, normal) {
                    stack.push((a.children[1], mesh_ref));
                    stack.push((a.children[0], mesh_ref));
                }
            }
            (None, Some(b)) => {
                let ray = rays.rays.get(ri).ok_or("invalid ray leaf")?;
                if segment_box_overlap(b.center, b.half_extents, ray.start, ray.end, ray_to_mesh) {
                    stack.push((ray_ref, b.children[1]));
                    stack.push((ray_ref, b.children[0]));
                }
            }
            (None, None) => {
                let ray = rays.rays.get(ri).ok_or("invalid ray leaf")?;
                let triangle = mesh.triangles.get(mi).ok_or("invalid mesh leaf")?;
                if triangle.invalid_plane_bits.is_some() {
                    continue;
                }
                let start = transform_point(ray_to_mesh, ray.start);
                let end = transform_point(ray_to_mesh, ray.end);
                let segment = sub(end, start);
                let squared = dot(segment, segment);
                if squared == 0.0 {
                    continue;
                }
                let length = 1.0 / ((1.0 / f64::from(squared).sqrt()) as f32);
                let direction = scale(segment, 1.0 / length);
                let vertices = triangle.indices.map(|i| mesh.vertices[i as usize]);
                if let Some(hit) = triangle_hit(start, direction, length, vertices) {
                    if let Some(previous) = hits.iter_mut().find(|h| h.ray_index == ri) {
                        if hit.fraction < previous.hit.fraction {
                            *previous = BundleHit {
                                ray_index: ri,
                                triangle_index: mi,
                                hit,
                            };
                        }
                    } else if hits.len() < rays.query_capacity as usize {
                        hits.push(BundleHit {
                            ray_index: ri,
                            triangle_index: mi,
                            hit,
                        });
                    }
                }
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use crate::collision::{CollisionMesh, CollisionRay, CollisionRayTree, RayTreeNode, Triangle};
    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    #[test]
    fn broad_phases_accept_touching_and_reject_separation() {
        assert!(boxes_overlap(
            [0.; 3],
            [1.; 3],
            [2., 0., 0.],
            [1.; 3],
            &IDENTITY
        ));
        assert!(!boxes_overlap(
            [0.; 3],
            [1.; 3],
            [2.01, 0., 0.],
            [1.; 3],
            &IDENTITY
        ));
        assert!(segment_box_overlap(
            [0.; 3],
            [1.; 3],
            [-2., 1., 0.],
            [2., 1., 0.],
            &IDENTITY
        ));
        assert!(!segment_box_overlap(
            [0.; 3],
            [1.; 3],
            [-2., 1.01, 0.],
            [2., 1.01, 0.],
            &IDENTITY
        ));
        assert!(triangle_box_overlap(
            [0.; 3],
            [1.; 3],
            [[-2., -2., 0.], [2., -2., 0.], [0., 2., 0.]],
            [0., 0., 1.]
        ));
        assert!(!triangle_box_overlap(
            [0.; 3],
            [1.; 3],
            [[-2., -2., 2.], [2., -2., 2.], [0., 2., 2.]],
            [0., 0., 1.]
        ));
    }
    #[test]
    fn nearer_face_keeps_first_ray_slot() {
        let node = |bound_measure, children| RayTreeNode {
            bound_measure,
            center: [0.; 3],
            half_extents: [5.; 3],
            children,
        };
        let rays = CollisionRayTree {
            transform: IDENTITY,
            query_capacity: 2,
            nodes: vec![node(1., [0x8001, 0x8000])],
            rays: vec![
                CollisionRay {
                    start: [0., 0., 3.],
                    end: [0., 0., -3.],
                },
                CollisionRay {
                    start: [0.25, 0., 3.],
                    end: [0.25, 0., -3.],
                },
            ],
        };
        let mesh = CollisionMesh {
            transform: [0.; 12],
            vertex_normals: None,
            flags: 0,
            broad_phase_nodes: 1,
            nodes: vec![node(2., [0x8000, 0x8001])],
            vertices: vec![
                [-2., -2., -1.],
                [2., -2., -1.],
                [0., 2., -1.],
                [-2., -2., 1.],
                [2., -2., 1.],
                [0., 2., 1.],
            ],
            triangles: vec![
                Triangle {
                    invalid_plane_bits: None,
                    indices: [0, 1, 2],
                    normal: [0., 0., 1.],
                    distance: 1.,
                },
                Triangle {
                    invalid_plane_bits: None,
                    indices: [3, 4, 5],
                    normal: [0., 0., 1.],
                    distance: -1.,
                },
            ],
        };
        let hits = query_bundle(&rays, &mesh, &IDENTITY, &IDENTITY).unwrap();
        assert_eq!(hits.iter().map(|h| h.ray_index).collect::<Vec<_>>(), [1, 0]);
        assert!(
            hits.iter()
                .all(|h| h.triangle_index == 1 && h.hit.point[2] == 1.)
        );
    }
}

#[derive(Debug, Clone)]
pub struct StaticObstacle {
    pub mesh: std::rc::Rc<crate::collision::CollisionMesh>,
    /// Original collision placement, distinct from the visual model transform.
    pub world_matrix: [f32; 16],
}

/// Rigid inverse followed by multiplication, using0010c3f0 accumulation order.
pub fn relative_transform(from: &[f32; 16], to: &[f32; 16]) -> [f32; 16] {
    let mut inverse = [0.0; 16];
    for c in 0..3 {
        for r in 0..3 {
            inverse[c * 4 + r] = to[r * 4 + c];
        }
    }
    for r in 0..3 {
        inverse[12 + r] =
            -((inverse[r] * to[12] + inverse[4 + r] * to[13]) + inverse[8 + r] * to[14]);
    }
    inverse[15] = 1.0;
    let mut result = [0.0; 16];
    for c in 0..4 {
        for r in 0..3 {
            result[c * 4 + r] = (inverse[r] * from[c * 4] + inverse[4 + r] * from[c * 4 + 1])
                + inverse[8 + r] * from[c * 4 + 2];
            if c == 3 {
                result[c * 4 + r] += inverse[12 + r] * from[15];
            }
        }
    }
    result[15] = 1.0;
    result
}

#[derive(Debug, Clone, Copy)]
pub struct ObstacleContact {
    pub local_end: Vec3,
    pub world_point: Vec3,
    pub world_normal: Vec3,
}

/// Original001213b0 sphere separation, clamped to zero for overlap.
pub fn sphere_separation(a: Vec3, a_radius: f32, b: Vec3, b_radius: f32) -> f32 {
    let difference = sub(a, b);
    (dot(difference, difference).sqrt() - (a_radius + b_radius)).max(0.0)
}

/// Queries the supplied original scene order. As in00193060, each successful
/// object callback appends the cumulative nearest-ray results, including prior
/// rays; the normal-riding solver keeps at most60 contact records.
/// Spatial-grid candidate ordering and dynamic objects remain caller concerns.
pub fn query_static_scene(
    rays: &crate::collision::CollisionRayTree,
    ray_world: &[f32; 16],
    obstacles: &[StaticObstacle],
) -> crate::Result<Vec<ObstacleContact>> {
    let mut nearest: Vec<(usize, f32, ObstacleContact)> = Vec::new();
    let mut contacts = Vec::new();
    let ray_root = rays.nodes.first().ok_or("missing ray root")?;
    let ray_center = transform_point(ray_world, ray_root.center);
    let ray_radius = dot(ray_root.half_extents, ray_root.half_extents).sqrt();
    for object in obstacles {
        let root = object.mesh.nodes.first().ok_or("missing obstacle root")?;
        let center = transform_point(&object.world_matrix, root.center);
        let radius = dot(root.half_extents, root.half_extents).sqrt();
        if sphere_separation(ray_center, ray_radius, center, radius) != 0.0 {
            continue;
        }
        let ray_to_mesh = relative_transform(ray_world, &object.world_matrix);
        let mesh_to_ray = relative_transform(&object.world_matrix, ray_world);
        let hits = query_bundle(rays, &object.mesh, &ray_to_mesh, &mesh_to_ray)?;
        if hits.is_empty() {
            continue;
        }
        for h in hits {
            let contact = ObstacleContact {
                local_end: rays.rays[h.ray_index].end,
                world_point: transform_point(&object.world_matrix, h.hit.point),
                world_normal: std::array::from_fn(|i| {
                    (object.world_matrix[i] * h.hit.normal[0]
                        + object.world_matrix[4 + i] * h.hit.normal[1])
                        + object.world_matrix[8 + i] * h.hit.normal[2]
                }),
            };
            if let Some(previous) = nearest.iter_mut().find(|x| x.0 == h.ray_index) {
                if h.hit.fraction < previous.1 {
                    *previous = (h.ray_index, h.hit.fraction, contact);
                }
            } else {
                nearest.push((h.ray_index, h.hit.fraction, contact));
            }
        }
        for &(_, _, contact) in &nearest {
            if contacts.len() == 60 {
                break;
            }
            contacts.push(contact);
        }
    }
    Ok(contacts)
}
