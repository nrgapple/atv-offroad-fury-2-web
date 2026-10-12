//! Original rider wreck clip tables (002d7dc0), phase selection (001ee990),
//! detached-body constants (001ee090/001efad0) and terrain contact points.
//! Water and animated-prop contacts remain separate integration work.
use crate::{
    Result,
    animation::{Channels, Pose},
    dynamics::{Basis, BodyConfig, BodyState, inverse_transform, transform},
    engine_audio::OriginalRandom,
    mesh::Node,
    rider_pose::Motion,
    wheel::{Vec3, add, cross, dot, scale, sub},
};
use serde::Serialize;

/// Order follows the exact-build registration calls, not MCF file order.
pub const SUFFIXES: [&str; 14] = [
    "BounceLeft01",
    "BounceLeft02",
    "BounceRight01",
    "BounceRight02",
    "Air01",
    "Air02",
    "Air03",
    "Air04",
    "SepFront",
    "SepLeft",
    "SepRear",
    "SepRight",
    "SepUp",
    "Swim",
];
const BLEND_TIME: f32 = 0.5;
const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Attached,
    Air,
    Ground,
    Water,
}
#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub phase: Phase,
    pub clip: &'static str,
    pub time: f32,
    pub grounded: bool,
    pub contact_count: usize,
    pub world_matrix: [f32; 16],
    pub origin: Vec3,
    pub linear_velocity: Vec3,
}
#[derive(Clone, Copy)]
struct ContactPoint {
    node: usize,
    offset: Vec3,
}
/// Native animated terrain contact offsets, in bone-local coordinates. Four
/// extra UprTorso points in002d7dc0 belong to water and are not terrain points.
const CONTACT_POINTS: [(&str, Vec3); 13] = [
    ("Pelvis", [0., 0., -0.35]),
    ("UprTorso", [0., 0., -0.45]),
    ("Head", [0., 0.9, 0.]),
    ("LwrArmL", [0., 0., -0.2]),
    ("HandL", [-0.3, -0.4, 0.]),
    ("LwrArmR", [0., 0., -0.2]),
    ("HandR", [0.3, -0.4, 0.]),
    ("LwrLegL", [0., 0., 0.25]),
    ("FootL", [0., -0.3, -0.1]),
    ("FootL", [0., -0.3, 0.75]),
    ("LwrLegR", [0., 0., 0.25]),
    ("FootR", [0., -0.3, -0.1]),
    ("FootR", [0., -0.3, 0.75]),
];

#[derive(Clone, Copy)]
pub struct VehicleMotion {
    pub world: [f32; 16],
    pub linear_velocity: Vec3,
    pub local_angular_velocity: Vec3,
}
pub struct Animator {
    motions: Vec<Motion>,
    looping: Vec<bool>,
    current: Channels,
    saved: Channels,
    phase: Phase,
    clip: usize,
    time: f32,
    blend_time: f32,
    points: Vec<ContactPoint>,
    body: Option<BodyState>,
    config: BodyConfig,
    world: [f32; 16],
    previous_root: [f32; 16],
    rng: OriginalRandom,
    grounded: bool,
    contact_count: usize,
    active: bool,
}
impl Animator {
    pub fn new(motions: Vec<Motion>, looping: Vec<bool>, nodes: &[Node]) -> Result<Self> {
        if motions.len() != SUFFIXES.len() || looping.len() != SUFFIXES.len() {
            return Err("wreck requires fourteen original clips".into());
        }
        for motion in &motions {
            motion.animation.sample_pose(nodes, 0., motion.sampling)?;
        }
        let points = CONTACT_POINTS
            .iter()
            .map(|(name, offset)| {
                Ok(ContactPoint {
                    node: nodes
                        .iter()
                        .position(|n| n.name.eq_ignore_ascii_case(name))
                        .ok_or_else(|| format!("wreck contact bone missing: {name}"))?,
                    offset: *offset,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if nodes
            .first()
            .is_none_or(|n| !n.name.eq_ignore_ascii_case("Pelvis") || n.parent != -1)
        {
            return Err("wreck root must be original Pelvis".into());
        }
        let current = motions[8]
            .animation
            .sample_channels(nodes, 0., motions[8].sampling)?;
        let mass = f32::from_bits(0x4095117d);
        let mut config = BodyConfig::new(mass, [mass / 12. * 10.; 3], [0.; 3])?;
        config.max_angular_speed_squared = 5.;
        Ok(Self {
            motions,
            looping,
            saved: current.clone(),
            current,
            phase: Phase::Attached,
            clip: 8,
            time: 0.,
            blend_time: 0.,
            points,
            body: None,
            config,
            world: IDENTITY,
            previous_root: IDENTITY,
            rng: OriginalRandom::with_seed(1),
            grounded: false,
            contact_count: 0,
            active: false,
        })
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        self.active = false;
        self.body = None;
        self.grounded = false;
        self.contact_count = 0;
    }
    pub fn begin(
        &mut self,
        kind: crate::wreck::Kind,
        riding: Channels,
        vehicle_world: [f32; 16],
        nodes: &[Node],
    ) -> Result<()> {
        self.current = riding;
        self.previous_root = self.current.pose(nodes)?.world_matrices[0];
        self.world = vehicle_world;
        self.body = None;
        self.grounded = false;
        self.contact_count = 0;
        self.active = true;
        let clip = match kind {
            crate::wreck::Kind::Front => 8,
            crate::wreck::Kind::Left => 9,
            crate::wreck::Kind::Back => 10,
            crate::wreck::Kind::Right => 11,
            crate::wreck::Kind::Vertical => 12,
        };
        self.select(Phase::Attached, clip);
        Ok(())
    }
    fn select(&mut self, phase: Phase, clip: usize) {
        self.saved = self.current.clone();
        self.phase = phase;
        self.clip = clip;
        self.time = 0.;
        self.blend_time = 0.;
    }
    fn random_four(&mut self) -> usize {
        //001ef470 clamps the randomly selected index to table length - 1.
        (((self.rng.next_u31() as f32 * 4.656_613e-10) * 4.) as usize).min(3)
    }
    fn sample(&mut self, nodes: &[Node], dt: f32) -> Result<Pose> {
        let motion = &self.motions[self.clip];
        self.time += dt;
        if self.looping[self.clip] && self.time > motion.animation.duration {
            self.time -= motion.animation.duration;
        }
        self.time = self.time.min(motion.animation.duration);
        let sampled = motion
            .animation
            .sample_channels(nodes, self.time, motion.sampling)?;
        //0013d7a0 advances clip time but evaluates transition at pre-step blend time.
        let fraction = (self.blend_time / BLEND_TIME).clamp(0., 1.);
        let weight = (3. - fraction * 2.) * fraction * fraction;
        self.blend_time += dt;
        self.current = blend(&self.saved, &sampled, weight);
        self.current.pose(nodes)
    }
    pub fn pose(&self, nodes: &[Node]) -> Result<Pose> {
        self.current.pose(nodes)
    }
    pub fn frame(&self) -> Option<Frame> {
        self.active.then(|| Frame {
            phase: self.phase,
            clip: SUFFIXES[self.clip],
            time: self.time,
            grounded: self.grounded,
            contact_count: self.contact_count,
            world_matrix: self.world,
            origin: [self.world[12], self.world[13], self.world[14]],
            linear_velocity: self.body.as_ref().map_or([0.; 3], |b| b.linear_velocity),
        })
    }
    /// Animation and independent rider-body step. `vehicle_world` is used only
    /// during separation; subsequent matrices belong to the detached rider.
    pub fn step(
        &mut self,
        dt: f32,
        vehicle: VehicleMotion,
        nodes: &[Node],
        terrain: &crate::terrain::Terrain,
        materials: &[crate::materials::TerrainMaterial; 8],
    ) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err("invalid wreck animation step".into());
        }
        let vehicle_world = vehicle.world;
        let pose = self.sample(nodes, dt)?;
        let root = pose.world_matrices[0];
        if self.phase == Phase::Attached {
            self.world = vehicle_world;
            self.contact_count = self
                .terrain_points(&pose, terrain, materials)?
                .iter()
                .filter(|(local, ground, normal, _)| {
                    dot(
                        sub(
                            *ground,
                            crate::obstacle::transform_point(&self.world, *local),
                        ),
                        *normal,
                    ) > 0.
                })
                .count();
            self.grounded = self.contact_count > 0;
            let duration = self.motions[self.clip].animation.duration;
            if duration - self.time <= BLEND_TIME || self.grounded {
                let with_motion = !self.grounded;
                let root_world = multiply(vehicle_world, root);
                let mut body = BodyState::new(
                    [root_world[12], root_world[13], root_world[14]],
                    quaternion(root_world),
                    &self.config,
                )?;
                let (linear, angular) = detach_velocity(
                    self.previous_root,
                    root,
                    vehicle,
                    body.basis(),
                    dt,
                    with_motion,
                );
                body.linear_velocity = linear;
                body.local_angular_velocity = angular;
                // Native draws z,y,x from its shared generator. The algorithm is
                // original, but this port's independently seeded stream is not
                // the complete game's RNG call ordering.
                for axis in [2, 1, 0] {
                    body.local_angular_velocity[axis] +=
                        self.rng.next_u31() as f32 * 4.656_613e-10 * 4. - 2.;
                }
                self.body = Some(body);
                let next = self.random_four();
                self.select(
                    if self.grounded {
                        Phase::Ground
                    } else {
                        Phase::Air
                    },
                    if self.grounded { next } else { 4 + next },
                );
            }
        } else {
            let current_root_inverse = inverse(root);
            // Keep the physical pelvis fixed while changing clips; animation
            // displacement must not teleport the independent rigid body.
            self.world = multiply(
                body_matrix(
                    self.body.as_ref().ok_or("missing detached body")?,
                    &self.config,
                ),
                current_root_inverse,
            );
            let contacts = self.terrain_points(&pose, terrain, materials)?;
            self.grounded = false;
            self.contact_count = 0;
            let mut elapsed = 0.;
            let mut last_contacts = 0;
            while elapsed < dt {
                let step = (dt - elapsed).min(f32::from_bits(0x3c360b61));
                let body = self.body.as_mut().ok_or("missing detached body")?;
                body.force[1] -= self.config.mass * f32::from_bits(0x4200cccd);
                let model = multiply(body_matrix(body, &self.config), current_root_inverse);
                let mut corrections = crate::contact::ContactCorrections::default();
                last_contacts = 0;
                for &(local, ground, normal, grip) in &contacts {
                    let point = crate::obstacle::transform_point(&model, local);
                    let penetration = dot(sub(ground, point), normal);
                    if penetration > 0. {
                        self.grounded = true;
                        self.contact_count += 1;
                        last_contacts += 1;
                        crate::contact::resolve_static_with_position_scale(
                            body,
                            &self.config,
                            crate::contact::ContactConfig {
                                reflection: 0.,
                                angular_impulse_scale: 1.,
                                friction_scale: 1.,
                                friction_torque_scale: 1.,
                            },
                            crate::contact::StaticContact {
                                point: add(point, scale(normal, penetration)),
                                normal,
                                penetration,
                                material_grip: grip,
                            },
                            &mut corrections,
                            0.25,
                        )?;
                    }
                }
                body.step(&self.config, step)?;
                body.center = add(body.center, corrections.translation);
                elapsed += step;
            }
            if last_contacts > 0 {
                let body = self.body.as_mut().unwrap();
                body.linear_velocity = scale(body.linear_velocity, 1. - 0.03);
            }
            self.world = multiply(
                body_matrix(self.body.as_ref().unwrap(), &self.config),
                current_root_inverse,
            );
            if self.phase == Phase::Air && self.grounded {
                let next = self.random_four();
                self.select(Phase::Ground, next);
            }
        }
        self.previous_root = root;
        Ok(())
    }
    fn terrain_points(
        &self,
        pose: &Pose,
        terrain: &crate::terrain::Terrain,
        _materials: &[crate::materials::TerrainMaterial; 8],
    ) -> Result<Vec<(Vec3, Vec3, Vec3, f32)>> {
        let mut points = Vec::new();
        for p in &self.points {
            let local = crate::obstacle::transform_point(&pose.world_matrices[p.node], p.offset);
            let world = crate::obstacle::transform_point(&self.world, local);
            if let Some(hit) = terrain.contact(world[0], world[2])? {
                let ground = [world[0], hit.height, world[2]];
                // Retain even currently nonpenetrating planes for all substeps.
                points.push((
                    local,
                    ground,
                    hit.normal,
                    if hit.surface == 0 { 0.2 } else { 0. },
                ));
            }
        }
        Ok(points)
    }
}
fn blend(a: &Channels, b: &Channels, t: f32) -> Channels {
    Channels {
        translations: a
            .translations
            .iter()
            .zip(&b.translations)
            .map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => Some(std::array::from_fn(|i| a[i] + t * (b[i] - a[i]))),
                (_, Some(b)) => Some(*b),
                (Some(a), _) => Some(*a),
                _ => None,
            })
            .collect(),
        rotations: a
            .rotations
            .iter()
            .zip(&b.rotations)
            .map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => Some(crate::animation::blend_rotation(*a, *b, t)),
                (_, Some(b)) => Some(*b),
                (Some(a), _) => Some(*a),
                _ => None,
            })
            .collect(),
    }
}
///001eff20: transfer vehicle velocity, optionally including separation root
/// motion. Random angular noise is applied by the caller after this kernel.
pub fn detach_velocity(
    previous: [f32; 16],
    current: [f32; 16],
    vehicle: VehicleMotion,
    rider_basis: Basis,
    dt: f32,
    include_motion: bool,
) -> (Vec3, Vec3) {
    let linear = vehicle.linear_velocity;
    let angular = vehicle.local_angular_velocity;
    let vehicle_basis = matrix_basis(vehicle.world);
    if !include_motion {
        return (linear, angular);
    }
    let delta = std::array::from_fn(|i| (current[12 + i] - previous[12 + i]) / dt);
    let prev = matrix_basis(previous);
    let now = matrix_basis(current);
    let rotation = if prev[1] == now[1] {
        if prev[2] == now[2] {
            [0.; 3]
        } else {
            cross(prev[2], now[2])
        }
    } else {
        cross(prev[1], now[1])
    };
    (
        add(linear, transform(vehicle_basis, delta)),
        add(
            angular,
            inverse_transform(
                rider_basis,
                transform(vehicle_basis, scale(rotation, 1. / dt)),
            ),
        ),
    )
}
fn matrix_basis(m: [f32; 16]) -> Basis {
    std::array::from_fn(|c| std::array::from_fn(|r| m[c * 4 + r]))
}
fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn inverse(m: [f32; 16]) -> [f32; 16] {
    crate::obstacle::relative_transform(&IDENTITY, &m)
}
fn body_matrix(b: &BodyState, c: &BodyConfig) -> [f32; 16] {
    let basis = b.basis();
    let o = b.origin(c);
    [
        basis[0][0],
        basis[0][1],
        basis[0][2],
        0.,
        basis[1][0],
        basis[1][1],
        basis[1][2],
        0.,
        basis[2][0],
        basis[2][1],
        basis[2][2],
        0.,
        o[0],
        o[1],
        o[2],
        1.,
    ]
}
fn quaternion(m: [f32; 16]) -> [f32; 4] {
    let trace = m[0] + m[5] + m[10];
    let q = if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        [
            0.25 * s,
            (m[6] - m[9]) / s,
            (m[8] - m[2]) / s,
            (m[1] - m[4]) / s,
        ]
    } else if m[0] > m[5] && m[0] > m[10] {
        let s = (1. + m[0] - m[5] - m[10]).sqrt() * 2.;
        [
            (m[6] - m[9]) / s,
            0.25 * s,
            (m[4] + m[1]) / s,
            (m[8] + m[2]) / s,
        ]
    } else if m[5] > m[10] {
        let s = (1. + m[5] - m[0] - m[10]).sqrt() * 2.;
        [
            (m[8] - m[2]) / s,
            (m[4] + m[1]) / s,
            0.25 * s,
            (m[9] + m[6]) / s,
        ]
    } else {
        let s = (1. + m[10] - m[0] - m[5]).sqrt() * 2.;
        [
            (m[1] - m[4]) / s,
            (m[8] + m[2]) / s,
            (m[9] + m[6]) / s,
            0.25 * s,
        ]
    };
    let length = dot([q[1], q[2], q[3]], [q[1], q[2], q[3]]) + q[0] * q[0];
    q.map(|v| v / length.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attachment_motion_is_added_only_for_normal_separation() {
        let mut current = IDENTITY;
        current[12] = 2.;
        let vehicle = VehicleMotion {
            world: IDENTITY,
            linear_velocity: [3., 4., 5.],
            local_angular_velocity: [0.1, 0.2, 0.3],
        };
        let (linear, angular) = detach_velocity(
            IDENTITY,
            current,
            vehicle,
            matrix_basis(IDENTITY),
            0.5,
            true,
        );
        assert_eq!(linear, [7., 4., 5.]);
        assert_eq!(angular, vehicle.local_angular_velocity);
        assert_eq!(
            detach_velocity(
                IDENTITY,
                current,
                vehicle,
                matrix_basis(IDENTITY),
                0.5,
                false
            ),
            (vehicle.linear_velocity, vehicle.local_angular_velocity)
        );
    }
    #[test]
    fn root_anchoring_preserves_physical_position_across_clip_changes() {
        let b = BodyConfig::new(4., [3.; 3], [0.; 3]).unwrap();
        for q in [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
            [0.5, 0.5, 0.5, 0.5],
        ] {
            let body = BodyState::new([30., 40., 50.], q, &b).unwrap();
            let world = body_matrix(&body, &b);
            let mut root = IDENTITY;
            root[12] = 5.;
            root[13] = 3.;
            root[14] = -2.;
            let reattached = multiply(multiply(world, inverse(root)), root);
            for (a, b) in world.iter().zip(reattached) {
                assert!((a - b).abs() < 1e-5);
            }
            let decoded = quaternion(world);
            let basis = crate::dynamics::basis(decoded);
            for (a, b) in basis.iter().flatten().zip(body.basis().iter().flatten()) {
                assert!((a - b).abs() < 1e-5);
            }
        }
    }
}
