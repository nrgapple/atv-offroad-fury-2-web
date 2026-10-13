//! Continuous source-MOT pose blending and source fork/grip geometry. The hand
//! constraint adapts the existing two-link solver; MX controller parity is open.
use super::*;
#[derive(Clone, Copy, Default)]
pub(super) struct Controls {
    pub steer: f32,
    pub lean: f32,
    moving: f32,
    air: f32,
}
impl Controls {
    pub fn step(&mut self, input: VehicleInput, speed: f32, grounded: bool, dt: f32) {
        let approach = |current: &mut f32, target: f32, rate: f32| {
            *current += (target - *current) * (1. - (-rate * dt).exp())
        };
        approach(&mut self.steer, input.steer, 10.);
        approach(&mut self.lean, input.lean, 8.);
        approach(
            &mut self.moving,
            ((speed - 10. * 1.4666667) / (20. * 1.4666667)).clamp(0., 1.),
            6.,
        );
        approach(&mut self.air, if grounded { 0. } else { 1. }, 10.);
    }
}
pub(super) fn axis_rotation(axis: V, angle: f32) -> M {
    let sine = (angle * 0.5).sin();
    let b = dynamics::basis([
        (angle * 0.5).cos(),
        axis[0] * sine,
        axis[1] * sine,
        axis[2] * sine,
    ]);
    [
        b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1], b[2][2],
        0., 0., 0., 0., 1.,
    ]
}
fn inverse(m: &M) -> M {
    let mut r = [
        m[0], m[4], m[8], 0., m[1], m[5], m[9], 0., m[2], m[6], m[10], 0., 0., 0., 0., 1.,
    ];
    let p = dynamics::transform(matrix_basis(&r), [-m[12], -m[13], -m[14]]);
    r[12..15].copy_from_slice(&p);
    r
}
fn blend(a: animation::Channels, b: animation::Channels, t: f32) -> animation::Channels {
    animation::Channels {
        translations: a
            .translations
            .into_iter()
            .zip(b.translations)
            .map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => Some(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)),
                _ => a.or(b),
            })
            .collect(),
        rotations: a
            .rotations
            .into_iter()
            .zip(b.rotations)
            .map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => {
                    let mut q = animation::blend_rotation(a, b, t);
                    let norm = q.iter().map(|v| v * v).sum::<f32>().sqrt();
                    q = q.map(|v| v / norm);
                    Some(q)
                }
                _ => a.or(b),
            })
            .collect(),
    }
}
fn compose(pose: &mut animation::Pose, nodes: &[mesh::Node]) {
    for (i, n) in nodes.iter().enumerate() {
        pose.world_matrices[i] = if n.parent < 0 {
            pose.local_matrices[i]
        } else {
            mul(
                &pose.world_matrices[n.parent as usize],
                &pose.local_matrices[i],
            )
        };
    }
}
fn constrain(
    arm: &crate::rider_ik::Arm,
    nodes: &[mesh::Node],
    pose: &mut animation::Pose,
    marker: usize,
    target: &M,
) -> Result<()> {
    let wrist = mul(target, &inverse(&nodes[marker].local_matrix));
    arm.solve(nodes, pose, [wrist[12], wrist[13], wrist[14]])?;
    let parent = nodes[arm.hand].parent as usize;
    let local = mul(&inverse(&pose.world_matrices[parent]), &wrist);
    pose.local_matrices[arm.hand][..12].copy_from_slice(&local[..12]);
    compose(pose, nodes);
    Ok(())
}
impl Model {
    fn riding_channels(&self, prefix: &str, c: Controls) -> Result<animation::Channels> {
        let sample = |suffix: &str| -> Result<animation::Channels> {
            let name = format!("{prefix}_{suffix}");
            let a = self
                .motions
                .get(&name)
                .ok_or_else(|| format!("missing riding motion {name}"))?;
            a.sample_channels(
                &self.rider.nodes,
                a.duration * (1. - c.steer) * 0.5,
                Default::default(),
            )
        };
        Ok(blend(
            sample("mid")?,
            sample(if c.lean < 0. { "fwd" } else { "bck" })?,
            c.lean.abs(),
        ))
    }
    pub(super) fn normal_pose(
        &self,
        c: Controls,
        angles: [f32; 2],
        compression: [f32; 2],
        steering: f32,
    ) -> Result<(animation::Pose, f32)> {
        let channels = |c: Controls| -> Result<animation::Channels> {
            let ground = blend(
                self.riding_channels("mx_turn_still", c)?,
                self.riding_channels("mx_turn_riding", c)?,
                c.moving,
            );
            Ok(blend(ground, self.riding_channels("mx_air", c)?, c.air))
        };
        let desired = channels(c)?;
        let mut pose = desired.pose(&self.rider.nodes)?;
        let dummy = ["Dummy_hand_L", "Dummy_hand_R"].map(|name| {
            self.rider
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
                .ok_or("missing rider grip marker")
        });
        let dummy = [dummy[0]?, dummy[1]?];
        let bike = self.pose(steering, angles, compression);
        let hands = crate::rider_ik::Hands::bind_native(&self.rider.nodes)?;
        // Full source pose sweeps can exceed the rigid arm's reach when adapted
        // to the physical fork angle. Fade only input-dependent pose offsets
        // toward the source forward riding/air pose; do not stretch or teleport
        // the wrist. This reach adaptation is not a recovered MX controller.
        let position = |m: &M| [m[12], m[13], m[14]];
        let wrists = [0, 1].map(|i| {
            mul(
                &bike[self.grips[i]],
                &inverse(&self.rider.nodes[dummy[i]].local_matrix),
            )
        });
        let reachable = |pose: &animation::Pose| {
            [&hands.left, &hands.right]
                .into_iter()
                .enumerate()
                .all(|(i, arm)| {
                    let offset = sub(
                        position(&wrists[i]),
                        position(&pose.world_matrices[arm.shoulder]),
                    );
                    dot(offset, offset).sqrt() <= arm.maximum_reach() * 0.98
                })
        };
        if !reachable(&pose) {
            let reference = channels(Controls {
                steer: 0.,
                lean: -1.,
                ..c
            })?;
            let mut low = 0.;
            let mut high = 1.;
            let mut accepted = reference.pose(&self.rider.nodes)?;
            if reachable(&accepted) {
                for _ in 0..8 {
                    let middle = (low + high) * 0.5;
                    let candidate = blend(reference.clone(), desired.clone(), middle)
                        .pose(&self.rider.nodes)?;
                    if reachable(&candidate) {
                        low = middle;
                        accepted = candidate;
                    } else {
                        high = middle;
                    }
                }
                pose = accepted;
            }
        }
        for (i, arm) in [&hands.left, &hands.right].into_iter().enumerate() {
            constrain(
                arm,
                &self.rider.nodes,
                &mut pose,
                dummy[i],
                &bike[self.grips[i]],
            )?;
        }
        // Keep normal riding boots on source foot pegs; stunt and wreck paths
        // bypass this controller, allowing the source release animations.
        for (bone, marker, peg) in [
            ("FootL", "Dummy_foot_L", "Dummy_Foot_L"),
            ("FootR", "Dummy_foot_R", "Dummy_Foot_R"),
        ] {
            let limb = crate::rider_ik::Arm::bind(&self.rider.nodes, bone, [1., 0., 0.])?;
            let marker = self
                .rider
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(marker))
                .ok_or("missing rider foot marker")?;
            let peg = self
                .mesh
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(peg))
                .ok_or("missing motorcycle foot peg")?;
            constrain(&limb, &self.rider.nodes, &mut pose, marker, &bike[peg])?;
        }
        Ok((pose, steering))
    }
    pub(super) fn foot_errors(&self, rider: &[M], bike: &[M]) -> [f32; 2] {
        [
            ("Dummy_foot_L", "Dummy_Foot_L"),
            ("Dummy_foot_R", "Dummy_Foot_R"),
        ]
        .map(|(r, b)| {
            let ri = self
                .rider
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(r))
                .unwrap();
            let bi = self
                .mesh
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(b))
                .unwrap();
            let delta = sub(
                rider[ri][12..15].try_into().unwrap(),
                bike[bi][12..15].try_into().unwrap(),
            );
            dot(delta, delta).sqrt()
        })
    }
    pub(super) fn grip_errors(&self, rider: &[M], bike: &[M]) -> [f32; 2] {
        ["Dummy_hand_L", "Dummy_hand_R"].map(|name| {
            let i = self
                .rider
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
                .unwrap();
            let side = usize::from(name.ends_with('R'));
            let delta = sub(
                rider[i][12..15].try_into().unwrap(),
                bike[self.grips[side]][12..15].try_into().unwrap(),
            );
            dot(delta, delta).sqrt()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steering_keeps_wheel_on_slanted_fork_line() {
        let pivot = [0., 3.9, 0.95];
        let wheel = [0., 1.23, 2.52];
        let line = sub(pivot, wheel);
        let axis = scale(line, 1. / dot(line, line).sqrt());
        for angle in [-0.6, 0., 0.6] {
            let transformed = add(
                pivot,
                dynamics::transform(matrix_basis(&axis_rotation(axis, angle)), sub(wheel, pivot)),
            );
            assert!(sub(transformed, wheel).iter().all(|v| v.abs() < 1e-5));
        }
    }
}
