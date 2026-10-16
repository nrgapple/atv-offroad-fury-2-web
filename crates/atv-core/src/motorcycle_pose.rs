//! Normal riding rider pose following the MX rider system (MotocrossRider
//! 004ea840/004eb380/004eb6f0, foot pins 004ef320): stick-lean and crab-sweep
//! filters, the stop/still/riding speed bands, linear state crossfades, feet
//! down at a stop and the inside foot out in slides. The source drives an
//! 18-part powered ragdoll pinned to the grips and pegs; this port adapts it
//! as clip-preserving limb IK plus one spring-damped pelvis offset using the
//! source per-part spring/damping and PelvisBox (authored reduction).
use super::*;

/// Bike state the rider system reads each frame.
#[derive(Clone, Copy, Default)]
pub(super) struct Input {
    /// Rate-limited stick Y (+1 back).
    pub lean: f32,
    /// Crab angle (heading vs travel, rad) and its MaxCrossupAngle scale.
    pub crab: f32,
    pub max_crab: f32,
    /// World speed (ft/s), yaw rate (rad/s) and the bike up axis's world y.
    pub speed: f32,
    pub yaw_rate: f32,
    pub up_y: f32,
    pub grounded: bool,
    pub reverse: bool,
    /// Pelvis-point specific force minus the level 1 g, in the bike frame.
    pub load: V,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Controls {
    /// Filtered lean fraction (0 forward, 1 back) and crab sweep fraction.
    pub lean: f32,
    pub sweep: f32,
    pub speed: f32,
    air: f32,
    /// Peg pin strength per foot (left, right).
    pub feet: [f32; 2],
    foot_return: [f32; 2],
    /// Pelvis offset (ft, bike frame) and its rate.
    pub pelvis: V,
    pelvis_rate: V,
    started: bool,
}
impl Controls {
    pub fn step(&mut self, input: Input, sweep_side: f32, dt: f32) {
        let lean_raw = ((input.lean + 1.) * 0.5).clamp(0., 1.);
        let max = input.max_crab.max(1e-3);
        let sweep_raw = ((sweep_side * input.crab + max) / (2. * max)).clamp(0., 1.);
        if !self.started {
            *self = Self {
                lean: lean_raw,
                sweep: sweep_raw,
                feet: [1.; 2],
                started: true,
                ..Self::default()
            };
        }
        let filter = |value: &mut f32, target: f32, tau: f32| {
            *value += dt.min(tau) / tau * (target - *value);
        };
        filter(&mut self.lean, lean_raw, 0.125);
        filter(&mut self.sweep, sweep_raw, 0.15);
        self.speed = input.speed;
        // Linear crossfades: 3/s into the air, 2/s back to riding.
        self.air = if input.grounded {
            (self.air - 2. * dt).max(0.)
        } else {
            (self.air + 3. * dt).min(1.)
        };
        // Foot pins: feet down when slow and not yawing; the inside foot
        // leaves its peg in a leaned slide. Pins return over 0.5 s / 2 s.
        let outer = !input.grounded
            || (!input.reverse && (input.yaw_rate.abs() >= 1.5 || input.speed >= 7.3333));
        let slide = outer && input.grounded && input.speed < 110. && input.up_y < 0.9;
        let release = [
            !outer || (slide && sweep_raw < 0.3),
            !outer || (slide && sweep_raw > 0.7),
        ];
        for ((pin, back), released) in self.feet.iter_mut().zip(&mut self.foot_return).zip(release)
        {
            if released {
                *pin = 0.;
                // Feet-down pins return over 0.5 s, a slide foot-out over 2 s.
                *back = if outer { 2. } else { 0.5 };
            } else {
                *pin = (*pin + dt / back.max(0.1)).min(1.);
            }
        }
        // Pelvis spring: source riding part springs (80, 30, 30) and damping
        // (3.2, 2, 2) on the 0.35 slug pelvis, damping doubled at a stop and
        // fading to normal by 5 mph; driven by the pelvis-point load.
        let damping_scale = 2. - (input.speed / 7.3333).clamp(0., 1.);
        let stiffness = [80. / 0.35, 30. / 0.35, 30. / 0.35];
        let damping = [3.2 / 0.35, 2. / 0.35, 2. / 0.35];
        // Authored reduction: the source's limb pins and joint springs hold
        // the pelvis far tighter than one free spring, so the drive is scaled
        // and the travel kept within part of the PelvisBox half extents.
        const RESPONSE: f32 = 0.35;
        let half_box = [0.12, 0.2, 0.3];
        for k in 0..3 {
            let a = RESPONSE * input.load[k].clamp(-200., 200.)
                - stiffness[k] * self.pelvis[k]
                - damping_scale * damping[k] * self.pelvis_rate[k];
            self.pelvis_rate[k] += a * dt;
            self.pelvis[k] += self.pelvis_rate[k] * dt;
            if self.pelvis[k].abs() > half_box[k] {
                self.pelvis[k] = self.pelvis[k].clamp(-half_box[k], half_box[k]);
                self.pelvis_rate[k] = 0.;
            }
        }
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
fn rotate_world(pose: &mut animation::Pose, nodes: &[mesh::Node], node: usize, from: V, to: V) {
    let (a, b) = (unit(from), unit(to));
    let axis = crate::wheel::cross(a, b);
    let sine = dot(axis, axis).sqrt();
    if sine < 1e-7 {
        return;
    }
    let angle = sine.atan2(dot(a, b));
    let r = axis_rotation(scale(axis, 1. / sine), angle);
    let world = pose.world_matrices[node];
    let mut next = mul(&r, &world);
    next[12..15].copy_from_slice(&world[12..15]);
    let parent = nodes[node].parent;
    pose.local_matrices[node] = if parent >= 0 {
        mul(&inverse(&pose.world_matrices[parent as usize]), &next)
    } else {
        next
    };
    compose(pose, nodes);
}
fn unit(v: V) -> V {
    scale(v, 1. / dot(v, v).sqrt().max(1e-9))
}
fn position(m: &M) -> V {
    [m[12], m[13], m[14]]
}
/// Two-bone limb IK that keeps the clip's elbow/knee side and twist: the
/// joint stays in the plane of its authored position, and only the upper and
/// lower bones turn by the smallest rotations needed. `end` keeps its clip
/// orientation relative to the lower bone; the marker (a child of `end`)
/// lands on `target`.
fn limb(pose: &mut animation::Pose, nodes: &[mesh::Node], end: usize, marker: usize, target: V) {
    let lower = nodes[end].parent as usize;
    let upper = nodes[lower].parent as usize;
    for _ in 0..3 {
        let offset = sub(
            position(&pose.world_matrices[marker]),
            position(&pose.world_matrices[end]),
        );
        let goal = sub(target, offset);
        let s = position(&pose.world_matrices[upper]);
        let e = position(&pose.world_matrices[lower]);
        let w = position(&pose.world_matrices[end]);
        let (a, b) = (
            dot(sub(e, s), sub(e, s)).sqrt(),
            dot(sub(w, e), sub(w, e)).sqrt(),
        );
        let to_goal = sub(goal, s);
        let reach = dot(to_goal, to_goal).sqrt().max(1e-6);
        let d = reach.clamp((a - b).abs() + 1e-4, a + b - 1e-4);
        let u = scale(to_goal, 1. / reach);
        let elbow = sub(e, s);
        let mut pole = sub(elbow, scale(u, dot(elbow, u)));
        if dot(pole, pole) < 1e-10 {
            pole = sub(w, s);
            pole = sub(pole, scale(u, dot(pole, u)));
        }
        let pole = unit(pole);
        let cos = ((a * a + d * d - b * b) / (2. * a * d)).clamp(-1., 1.);
        let joint = add(
            s,
            add(scale(u, a * cos), scale(pole, a * (1. - cos * cos).sqrt())),
        );
        rotate_world(pose, nodes, upper, elbow, sub(joint, s));
        let e = position(&pose.world_matrices[lower]);
        let w = position(&pose.world_matrices[end]);
        rotate_world(pose, nodes, lower, sub(w, e), sub(goal, e));
    }
}
impl Model {
    fn pair(&self, set: &str, c: Controls) -> Result<animation::Channels> {
        let sample = |suffix: &str| -> Result<animation::Channels> {
            let name = format!("mx_{set}_{suffix}");
            let a = self
                .motions
                .get(&name)
                .ok_or_else(|| format!("missing riding motion {name}"))?;
            a.sample_channels(&self.rider.nodes, a.duration * c.sweep, Default::default())
        };
        Ok(if c.lean < 0.5 {
            blend(sample("fwd")?, sample("mid")?, 2. * c.lean)
        } else {
            blend(sample("mid")?, sample("bck")?, 2. * (c.lean - 0.5))
        })
    }
    fn riding_channels(&self, c: Controls) -> Result<animation::Channels> {
        let v = c.speed;
        let ground = if v < 7.3333 && self.motions.contains_key("mx_turn_stop_mid") {
            blend(
                self.pair("turn_still", c)?,
                self.pair("turn_stop", c)?,
                1. - v / 7.3333,
            )
        } else if v <= 14.6666 {
            self.pair("turn_still", c)?
        } else if v <= 22. {
            blend(
                self.pair("turn_still", c)?,
                self.pair("turn_riding", c)?,
                (v - 14.6666) / 7.3333,
            )
        } else {
            self.pair("turn_riding", c)?
        };
        Ok(if c.air > 0. {
            blend(ground, self.pair("air", c)?, c.air)
        } else {
            ground
        })
    }
    fn node(&self, name: &str) -> Result<usize> {
        self.rider
            .nodes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("missing rider node {name}"))
    }
    /// +1 when the sweep's t=1 end extends the rider's +x foot; the crab
    /// sweep is signed so the inside foot leaves its peg.
    pub(super) fn sweep_side(&self) -> f32 {
        let foot = |t: f32| -> Option<[f32; 2]> {
            let a = self.motions.get("mx_turn_riding_mid")?;
            let pose = a
                .sample_pose(&self.rider.nodes, a.duration * t, Default::default())
                .ok()?;
            let x = |n: &str| self.node(n).ok().map(|i| pose.world_matrices[i][12]);
            Some([x("FootL")?, x("FootR")?])
        };
        match (foot(0.5), foot(1.)) {
            (Some(mid), Some(end)) => {
                let moved = [end[0] - mid[0], end[1] - mid[1]];
                let out = if moved[0].abs() > moved[1].abs() {
                    moved[0]
                } else {
                    moved[1]
                };
                if out >= 0. { 1. } else { -1. }
            }
            _ => 1.,
        }
    }
    pub(super) fn normal_pose(
        &self,
        c: Controls,
        angles: [f32; 2],
        compression: [f32; 2],
        steering: f32,
    ) -> Result<(animation::Pose, f32)> {
        let mut pose = self.riding_channels(c)?.pose(&self.rider.nodes)?;
        // Pelvis offset in the bike frame; the upper body follows it.
        let pelvis = self.node("Pelvis")?;
        if c.pelvis.iter().any(|v| *v != 0.) {
            let mut world = pose.world_matrices[pelvis];
            for k in 0..3 {
                world[12 + k] += c.pelvis[k];
            }
            let parent = self.rider.nodes[pelvis].parent;
            pose.local_matrices[pelvis] = if parent >= 0 {
                mul(&inverse(&pose.world_matrices[parent as usize]), &world)
            } else {
                world
            };
            compose(&mut pose, &self.rider.nodes);
        }
        let bike = self.pose(steering, angles, compression);
        for (hand, marker, grip) in [
            ("HandL", "Dummy_hand_L", self.grips[0]),
            ("HandR", "Dummy_hand_R", self.grips[1]),
        ] {
            let target = position(&bike[grip]);
            limb(
                &mut pose,
                &self.rider.nodes,
                self.node(hand)?,
                self.node(marker)?,
                target,
            );
        }
        for (i, (foot, marker, peg)) in [
            ("FootL", "Dummy_foot_L", "Dummy_Foot_L"),
            ("FootR", "Dummy_foot_R", "Dummy_Foot_R"),
        ]
        .into_iter()
        .enumerate()
        {
            let weight = c.feet[i];
            if weight <= 0. {
                continue;
            }
            let peg = self
                .mesh
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(peg))
                .ok_or("missing motorcycle foot peg")?;
            let marker = self.node(marker)?;
            let clip = position(&pose.world_matrices[marker]);
            let target = add(clip, scale(sub(position(&bike[peg]), clip), weight));
            limb(
                &mut pose,
                &self.rider.nodes,
                self.node(foot)?,
                marker,
                target,
            );
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
