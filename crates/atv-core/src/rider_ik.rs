//! Original two-link hand solver, SCUS_972.11 002b7070/002b7a60.
//! Normal riding/stunt/landing selectors clear their hand-IK gate. Targets are
//! explicitly optional; constructing this solver never enables hand contacts.
use crate::{Result, animation::Pose, mesh::Node};
type Vec3 = [f32; 3];
type Matrix = [f32; 16];
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn position(m: Matrix) -> Vec3 {
    [m[12], m[13], m[14]]
}
fn length(v: Vec3) -> f32 {
    let squared = dot(v, v);
    if squared == 0. {
        0.
    } else {
        1. / (1. / squared.sqrt())
    }
}
fn unit(v: Vec3) -> Result<Vec3> {
    let squared = dot(v, v);
    if !squared.is_finite() || squared <= 0. {
        return Err("degenerate rider IK direction".into());
    }
    let scale = 1. / squared.sqrt();
    Ok(v.map(|value| value * scale))
}
fn inverse_direction(m: Matrix, v: Vec3) -> Vec3 {
    std::array::from_fn(|i| m[i * 4] * v[0] + m[i * 4 + 1] * v[1] + m[i * 4 + 2] * v[2])
}
fn inverse_point(m: Matrix, v: Vec3) -> Vec3 {
    inverse_direction(m, sub(v, position(m)))
}
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn rotation(axis: Vec3, cosine: f32, sine: f32, translation: Vec3) -> Matrix {
    let [x, y, z] = axis;
    let d = 1. - cosine;
    [
        cosine + d * x * x,
        d * x * y + sine * z,
        d * x * z - sine * y,
        0.,
        d * y * x - sine * z,
        cosine + d * y * y,
        d * y * z + sine * x,
        0.,
        d * z * x + sine * y,
        d * z * y - sine * x,
        cosine + d * z * z,
        0.,
        translation[0],
        translation[1],
        translation[2],
        1.,
    ]
}
fn compose(nodes: &[Node], pose: &mut Pose) -> Result<()> {
    for (index, node) in nodes.iter().enumerate() {
        if node.parent < -1 || node.parent >= index as i32 {
            return Err("invalid rider IK hierarchy".into());
        }
        pose.world_matrices[index] = if node.parent < 0 {
            pose.local_matrices[index]
        } else {
            multiply(
                pose.world_matrices[node.parent as usize],
                pose.local_matrices[index],
            )
        };
        if pose.world_matrices[index].iter().any(|v| !v.is_finite()) {
            return Err("nonfinite rider IK pose".into());
        }
    }
    Ok(())
}
#[derive(Debug, Clone)]
pub struct Arm {
    pub shoulder: usize,
    pub elbow: usize,
    pub hand: usize,
    bend_axis: Vec3,
    elbow_translation: Vec3,
    projected_length_sum: f32,
    inverse_projected_lengths: f32,
    angle_offset: f32,
}
impl Arm {
    /// Native caller002d8810 follows the hand's parent and grandparent. Its
    /// local hinge axis is supplied separately, as in002b7070's fifth argument.
    pub fn bind(nodes: &[Node], hand_name: &str, bend_axis: Vec3) -> Result<Self> {
        let hand = nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(hand_name))
            .ok_or("rider IK hand node is missing")?;
        let elbow = usize::try_from(nodes[hand].parent).map_err(|_| "rider IK elbow is missing")?;
        let shoulder = nodes
            .get(elbow)
            .and_then(|n| usize::try_from(n.parent).ok())
            .ok_or("rider IK shoulder is missing")?;
        if shoulder >= elbow
            || elbow >= hand
            || bend_axis.iter().any(|v| !v.is_finite())
            || (dot(bend_axis, bend_axis) - 1.).abs() > 0.0001
        {
            return Err("invalid rider IK chain or hinge axis".into());
        }
        let joint = nodes[elbow].world_matrix;
        let a = inverse_point(joint, position(nodes[shoulder].world_matrix));
        let b = inverse_point(joint, position(nodes[hand].world_matrix));
        let a_height = dot(a, bend_axis);
        let b_height = dot(b, bend_axis);
        let projected_a = std::array::from_fn(|i| a[i] - a_height * bend_axis[i]);
        let projected_b = std::array::from_fn(|i| b[i] - b_height * bend_axis[i]);
        let a_length = length(projected_a);
        let b_length = length(projected_b);
        if a_length <= 0. || b_length <= 0. || !a_length.is_finite() || !b_length.is_finite() {
            return Err("degenerate projected rider IK lengths".into());
        }
        let a_direction = unit(projected_a)?;
        let b_direction = unit(projected_b)?.map(|v| -v);
        let cosine = dot(a_direction, b_direction).clamp(-1., 1.);
        let angle = cosine.acos();
        let angle_offset = if dot(cross(a_direction, b.map(|v| -v)), bend_axis) > 0. {
            -angle
        } else {
            angle
        };
        Ok(Self {
            shoulder,
            elbow,
            hand,
            bend_axis,
            elbow_translation: position(nodes[elbow].local_matrix),
            projected_length_sum: b_length * b_length
                + (a_height - b_height) * (a_height - b_height)
                + a_length * a_length,
            inverse_projected_lengths: 1. / (b_length * a_length * 2.),
            angle_offset,
        })
    }
    /// Largest shoulder-to-wrist distance allowed by the rigid hinge geometry.
    pub fn maximum_reach(&self) -> f32 {
        (self.projected_length_sum + 1. / self.inverse_projected_lengths).sqrt()
    }
    /// Target and pose world matrices must share a coordinate frame. The native
    /// solver replaces elbow rotation, then rotates the shoulder toward target;
    /// hand translation remains the original rigid joint offset.
    pub fn solve(&self, nodes: &[Node], pose: &mut Pose, target: Vec3) -> Result<()> {
        if pose.local_matrices.len() != nodes.len()
            || pose.world_matrices.len() != nodes.len()
            || self.hand >= nodes.len()
            || target.iter().any(|v| !v.is_finite())
        {
            return Err("invalid rider IK pose or target".into());
        }
        let origin = position(pose.world_matrices[self.shoulder]);
        let requested = sub(target, origin);
        let distance = length(requested);
        if distance <= 0. || !distance.is_finite() {
            return Err("degenerate rider IK target".into());
        }
        let cosine = (self.inverse_projected_lengths
            * (distance * distance - self.projected_length_sum))
            .clamp(-1., 1.);
        let angle = cosine.acos() + self.angle_offset;
        pose.local_matrices[self.elbow] = rotation(
            unit(self.bend_axis)?,
            angle.cos(),
            angle.sin(),
            self.elbow_translation,
        );
        compose(nodes, pose)?;
        let current = unit(sub(position(pose.world_matrices[self.hand]), origin))?;
        let desired = unit(requested)?;
        let cosine = dot(current, desired).clamp(-1., 1.);
        let axis = cross(current, desired);
        // Collinear directions have no defined native cross-product axis. Keep
        // the already aligned shoulder unchanged; reject the opposite case.
        if dot(axis, axis) == 0. {
            if cosine < 0. {
                return Err("opposite rider IK target has no rotation axis".into());
            }
            return Ok(());
        }
        let axis = unit(inverse_direction(
            pose.world_matrices[self.shoulder],
            unit(axis)?,
        ))?;
        let sine = if cosine * cosine >= 1. {
            0.
        } else {
            (1. - cosine * cosine).sqrt()
        };
        let mut next = multiply(
            pose.local_matrices[self.shoulder],
            rotation(axis, cosine, sine, [0.; 3]),
        );
        next[12..15].copy_from_slice(&pose.local_matrices[self.shoulder][12..15]);
        pose.local_matrices[self.shoulder] = next;
        compose(nodes, pose)
    }
}
pub struct Hands {
    pub left: Arm,
    pub right: Arm,
}
impl Hands {
    /// 00450070 initializes rider-local +Y at004a9450;002d8810 mirrors it
    /// for the right arm. Binding does not activate the002d8e50 hand gate.
    pub fn bind_native(nodes: &[Node]) -> Result<Self> {
        Self::bind(nodes, [0., 1., 0.])
    }
    pub fn bind(nodes: &[Node], left_axis: Vec3) -> Result<Self> {
        Ok(Self {
            left: Arm::bind(nodes, "HandL", left_axis)?,
            right: Arm::bind(nodes, "HandR", left_axis.map(|v| -v))?,
        })
    }
    /// None is the recovered normal/stunt/landing gate: preserve every matrix.
    pub fn apply(
        &self,
        nodes: &[Node],
        pose: &mut Pose,
        targets: Option<[Vec3; 2]>,
    ) -> Result<bool> {
        let Some([left, right]) = targets else {
            return Ok(false);
        };
        self.left.solve(nodes, pose, left)?;
        self.right.solve(nodes, pose, right)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<Node>, Pose) {
        let rows = [
            ("Root", -1, [0., 0., 0.]),
            ("ShoulderL", 0, [0., 0., 0.]),
            ("ElbowL", 1, [1., 0., 0.]),
            ("HandL", 2, [1., 0., 0.]),
            ("ShoulderR", 0, [0., 2., 0.]),
            ("ElbowR", 4, [-1., 0., 0.]),
            ("HandR", 5, [-1., 0., 0.]),
        ];
        let mut nodes: Vec<Node> = rows
            .iter()
            .map(|(name, parent, t)| {
                let local = rotation([0., 0., 1.], 1., 0., *t);
                Node {
                    name: (*name).into(),
                    parent: *parent,
                    local_matrix: local,
                    world_matrix: local,
                    bounds: [0.; 7],
                }
            })
            .collect();
        let mut pose = Pose {
            local_matrices: nodes.iter().map(|n| n.local_matrix).collect(),
            world_matrices: vec![[0.; 16]; nodes.len()],
        };
        compose(&nodes, &mut pose).unwrap();
        for (node, world) in nodes.iter_mut().zip(&pose.world_matrices) {
            node.world_matrix = *world;
        }
        (nodes, pose)
    }
    #[test]
    fn hand_rotation_matches_original_instruction_fixture() {
        // 002b7070+002b7a60 run over synthetic rigid arm transforms. Trig and
        // transform helpers were hooked; all scalar/VU solver math executed.
        let (nodes, mut pose) = fixture();
        let arm = Arm::bind(&nodes, "HandL", [0., 0., 1.]).unwrap();
        arm.solve(&nodes, &mut pose, [1.4721534, 0.6990451, 0.20764521])
            .unwrap();
        let expected = [
            0x3f7b3af2, 0xbe2944bf, 0x3dc8942d, 0, 0x3e2185db, 0x3f7bf3dd, 0x3da4fad5, 0,
            0xbde0add3, 0xbd824469, 0x3f7deee9, 0, 0, 0, 0, 0x3f800000,
        ]
        .map(f32::from_bits);
        for (actual, expected) in pose.local_matrices[arm.shoulder].iter().zip(expected) {
            assert!((actual - expected).abs() < 0.000005);
        }
    }
    #[test]
    fn native_hinge_axes_are_mirrored_and_binding_keeps_targets_disabled() {
        let (nodes, mut pose) = fixture();
        let hands = Hands::bind_native(&nodes).unwrap();
        assert_eq!(hands.left.bend_axis, [0., 1., 0.]);
        assert_eq!(hands.right.bend_axis, [-0., -1., -0.]);
        let before = pose.local_matrices.clone();
        assert!(!hands.apply(&nodes, &mut pose, None).unwrap());
        assert_eq!(pose.local_matrices, before);
        let targets = [[1., 0., 0.7], [-1., 2., -0.7]];
        hands.apply(&nodes, &mut pose, Some(targets)).unwrap();
        for (index, target) in [hands.left.hand, hands.right.hand].into_iter().zip(targets) {
            assert!(length(sub(position(pose.world_matrices[index]), target)) < 0.00001);
        }
    }
    #[test]
    fn disabled_gate_preserves_riding_pose_and_enabled_solver_reaches_both_targets() {
        let (nodes, mut pose) = fixture();
        let hands = Hands::bind(&nodes, [0., 0., 1.]).unwrap();
        let before = pose.local_matrices.clone();
        assert!(!hands.apply(&nodes, &mut pose, None).unwrap());
        assert_eq!(pose.local_matrices, before);
        let targets = [[1., 0.7, 0.], [-1., 2.7, 0.]];
        hands.apply(&nodes, &mut pose, Some(targets)).unwrap();
        for (index, target) in [hands.left.hand, hands.right.hand].into_iter().zip(targets) {
            assert!(length(sub(position(pose.world_matrices[index]), target)) < 0.00001);
        }
    }
    #[test]
    fn unreachable_targets_clamp_hinge_without_stretching_and_invalid_binding_fails() {
        let (nodes, mut pose) = fixture();
        let arm = Arm::bind(&nodes, "HandL", [0., 0., 1.]).unwrap();
        arm.solve(&nodes, &mut pose, [100., 50., 0.]).unwrap();
        assert!(
            (length(sub(position(pose.world_matrices[arm.hand]), [0.; 3])) - 2.).abs() < 0.00001
        );
        assert!(
            (length(sub(
                position(pose.world_matrices[arm.hand]),
                position(pose.world_matrices[arm.shoulder])
            )) - arm.maximum_reach())
            .abs()
                < 0.00001
        );
        assert_eq!(pose.local_matrices[arm.hand], nodes[arm.hand].local_matrix);
        assert!(Arm::bind(&nodes, "Missing", [0., 0., 1.]).is_err());
        assert!(Arm::bind(&nodes, "HandL", [0., 0., 0.]).is_err());
        assert!(arm.solve(&nodes, &mut pose, [f32::NAN, 0., 0.]).is_err());
    }
}
