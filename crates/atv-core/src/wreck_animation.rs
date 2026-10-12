//! Original rider wreck clip tables (002d7dc0), phase selection (001ee990),
//! detached-body constants (001ee090/001efad0) and terrain contact points.
//! Static type-0 prop contacts share the native13 animated point spokes.
//! Water and moving/compound-prop contacts remain separate integration work.
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
    pub static_contact_count: usize,
    pub world_matrix: [f32; 16],
    pub origin: Vec3,
    pub linear_velocity: Vec3,
    pub center_of_mass: Vec3,
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
    previous_time: f32,
    integration_active: bool,
    anchor_pelvis: bool,
    pending_clip: Option<usize>,
    rng: OriginalRandom,
    grounded: bool,
    contact_count: usize,
    static_contact_count: usize,
    obstacles: Vec<crate::obstacle::StaticObstacle>,
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
            previous_time: 0.,
            integration_active: false,
            anchor_pelvis: false,
            pending_clip: None,
            rng: OriginalRandom::with_seed(1),
            grounded: false,
            contact_count: 0,
            static_contact_count: 0,
            obstacles: Vec::new(),
            active: false,
        })
    }
    pub fn active(&self) -> bool {
        self.active
    }
    /// Use the same original scene order and supported type-0 collision
    /// placements as the ATV. Content and candidate filtering stay with caller.
    pub fn set_static_obstacles(&mut self, obstacles: Vec<crate::obstacle::StaticObstacle>) {
        self.obstacles = obstacles;
    }
    pub fn reset(&mut self) {
        self.active = false;
        self.body = None;
        self.integration_active = false;
        self.anchor_pelvis = false;
        self.pending_clip = None;
        self.config.center_of_mass = [0.; 3];
        self.grounded = false;
        self.contact_count = 0;
        self.static_contact_count = 0;
    }
    pub fn begin(
        &mut self,
        kind: crate::wreck::Kind,
        riding: Channels,
        vehicle_world: [f32; 16],
        nodes: &[Node],
    ) -> Result<()> {
        self.current = riding;
        self.current.pose(nodes)?;
        self.previous_time = 0.;
        self.world = vehicle_world;
        self.config.center_of_mass = [0.; 3];
        self.body = Some(BodyState::new(
            [vehicle_world[12], vehicle_world[13], vehicle_world[14]],
            quaternion(vehicle_world),
            &self.config,
        )?);
        self.integration_active = false;
        self.anchor_pelvis = false;
        self.pending_clip = None;
        self.grounded = false;
        self.contact_count = 0;
        self.static_contact_count = 0;
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
            static_contact_count: self.static_contact_count,
            world_matrix: self.world,
            origin: [self.world[12], self.world[13], self.world[14]],
            linear_velocity: self.body.as_ref().map_or([0.; 3], |b| b.linear_velocity),
            center_of_mass: self.config.center_of_mass,
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
        //001e7600 copies the quad Model world matrix during separation. The
        //native body still belongs to the rider Model container, not Pelvis.
        let attached = self.phase == Phase::Attached;
        if attached {
            self.world = vehicle.world;
        }
        if let Some(clip) = self.pending_clip.take() {
            self.select(self.phase, clip);
        }
        let pose = self.current.pose(nodes)?;
        let last_contacts = self.step_body(dt, &pose, terrain, materials, !attached)?;
        let saved_pelvis = multiply(self.world, pose.world_matrices[0]);
        //001ee990 performs physics and saves Pelvis before advancing animation.
        //Category changes request the next clip on the following outer frame.
        if !attached && self.phase == Phase::Air && last_contacts > 0 {
            self.phase = Phase::Ground;
            self.pending_clip = Some(self.random_four());
        }
        let pose = self.sample(nodes, dt)?;
        if attached {
            let duration = self.motions[self.clip].animation.duration;
            let current_pelvis = multiply(self.world, pose.world_matrices[0]);
            if duration - self.time <= BLEND_TIME {
                self.detach(saved_pelvis, current_pelvis, vehicle, dt, true)?;
            }
            //Native first enters Air when the clip window expires, then can
            //override it with Ground in the same frame. Preserve both sets of
            //random draws rather than skipping the first entry when touching.
            if last_contacts > 0 {
                self.detach(saved_pelvis, current_pelvis, vehicle, dt, false)?;
            }
        } else if self.anchor_pelvis
            || self.motions[self.clip].animation.duration - self.time <= dt
            || self.time < self.previous_time
        {
            self.world = rebase_model(self.world, pose.world_matrices[0], saved_pelvis);
            self.anchor_pelvis = true;
        }
        self.previous_time = self.time;
        Ok(())
    }
    fn detach(
        &mut self,
        previous: [f32; 16],
        current: [f32; 16],
        vehicle: VehicleMotion,
        dt: f32,
        with_motion: bool,
    ) -> Result<()> {
        let (linear, angular) = detach_world_velocity(
            previous,
            current,
            vehicle,
            matrix_basis(self.world),
            dt,
            with_motion,
        );
        let body = self.body.as_mut().ok_or("missing rider Model body")?;
        body.linear_velocity = linear;
        body.local_angular_velocity = angular;
        //Native draws z,y,x. The stream remains independently seeded; full
        //game RNG ordering requires all other systems' calls to be restored.
        for axis in [2, 1, 0] {
            body.local_angular_velocity[axis] +=
                self.rng.next_u31() as f32 * 4.656_613e-10 * 4. - 2.;
        }
        self.integration_active = true;
        let next = self.random_four();
        self.phase = if with_motion {
            Phase::Air
        } else {
            Phase::Ground
        };
        self.pending_clip = Some(if with_motion { 4 + next } else { next });
        Ok(())
    }
    fn step_body(
        &mut self,
        dt: f32,
        pose: &Pose,
        terrain: &crate::terrain::Terrain,
        materials: &[crate::materials::TerrainMaterial; 8],
        gravity: bool,
    ) -> Result<usize> {
        //001469e0 obtains position/orientation from the Model scene on every
        //step. Root rebasing changes that scene basis without rotating stored
        //local angular velocity or discarding Adams–Bashforth history.
        let body = self.body.as_mut().ok_or("missing rider Model body")?;
        body.orientation = quaternion(self.world);
        let origin = [self.world[12], self.world[13], self.world[14]];
        body.center = add(origin, transform(body.basis(), self.config.center_of_mass));
        let center = contact_bounds_center(self.points.iter().map(|point| {
            crate::obstacle::transform_point(
                &self.world,
                crate::obstacle::transform_point(&pose.world_matrices[point.node], point.offset),
            )
        }));
        recenter_body(body, &mut self.config, center);
        let mut contacts = self.terrain_points(pose, terrain, materials)?;
        let terrain_count = contacts.len();
        contacts.extend(self.static_points(pose)?);
        self.grounded = false;
        self.contact_count = 0;
        self.static_contact_count = 0;
        let mut elapsed = 0.;
        let mut last_contacts = 0;
        while elapsed < dt {
            let step = (dt - elapsed).min(f32::from_bits(0x3c360b61));
            let body = self.body.as_mut().ok_or("missing rider Model body")?;
            if gravity {
                body.force[1] -= self.config.mass * f32::from_bits(0x4200cccd);
            }
            let model = if self.integration_active {
                body_matrix(body, &self.config)
            } else {
                self.world
            };
            let mut corrections = crate::contact::ContactCorrections::default();
            last_contacts = 0;
            for (index, &(local, ground, normal, grip)) in contacts.iter().enumerate() {
                let point = crate::obstacle::transform_point(&model, local);
                let penetration = dot(sub(ground, point), normal);
                if penetration > 0. {
                    self.grounded = true;
                    self.contact_count += 1;
                    self.static_contact_count += usize::from(index >= terrain_count);
                    last_contacts += 1;
                    //001ef690 activates the body before integration when any
                    //contact is reported, including while still attached.
                    self.integration_active = true;
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
            if self.integration_active {
                body.step(&self.config, step)?;
                body.center = add(body.center, corrections.translation);
            } else {
                //Disabled001469e0 still clears force/correction accumulators.
                body.force = [0.; 3];
                body.local_torque = [0.; 3];
            }
            elapsed += step;
        }
        if last_contacts > 0 {
            let body = self.body.as_mut().unwrap();
            body.linear_velocity = scale(body.linear_velocity, 1. - f32::from_bits(0x3cf5c28f));
        }
        if self.integration_active {
            self.world = body_matrix(self.body.as_ref().unwrap(), &self.config);
        }
        Ok(last_contacts)
    }
    fn static_points(&self, pose: &Pose) -> Result<Vec<(Vec3, Vec3, Vec3, f32)>> {
        if self.obstacles.is_empty() {
            return Ok(Vec::new());
        }
        let endpoints: Vec<_> = self
            .points
            .iter()
            .map(|point| {
                crate::obstacle::transform_point(
                    &self.world,
                    crate::obstacle::transform_point(
                        &pose.world_matrices[point.node],
                        point.offset,
                    ),
                )
            })
            .collect();
        let rays = rider_static_rays(&endpoints);
        let contacts = crate::wreck::query_static_segments(&rays, &self.obstacles, 60)?;
        let to_local = inverse(self.world);
        Ok(contacts
            .into_iter()
            .map(|hit| {
                (
                    crate::obstacle::transform_point(&to_local, hit.world_end),
                    hit.world_point,
                    hit.world_normal,
                    0.2,
                )
            })
            .collect())
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
///00193b40 puts each spoke start at the world-axis bounds midpoint and its
///end at the corresponding animated bone offset. Neither pelvis nor velocity
///is a substitute for this collision hull.
pub(crate) fn rider_static_rays(endpoints: &[Vec3]) -> Vec<crate::collision::CollisionRay> {
    let center = contact_bounds_center(endpoints.iter().copied());
    endpoints
        .iter()
        .map(|&end| crate::collision::CollisionRay { start: center, end })
        .collect()
}
/// Native world-axis bounds midpoint, not a mass-weighted centroid. Callers
/// supply the nonempty, finite animated contact-point list.
pub(crate) fn contact_bounds_center(points: impl Iterator<Item = Vec3>) -> Vec3 {
    let mut low = [f32::MAX; 3];
    let mut high = [-f32::MAX; 3];
    for point in points {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    std::array::from_fn(|axis| (high[axis] + low[axis]) * 0.5)
}
fn recenter_body(body: &mut BodyState, config: &mut BodyConfig, world_center: Vec3) {
    let origin = body.origin(config);
    config.center_of_mass = inverse_transform(body.basis(), sub(world_center, origin));
    //001469e0 reconstructs center from the scene origin and new local CoG.
    //Changing the pose must not translate the rendered rider or add velocity.
    body.center = add(origin, transform(body.basis(), config.center_of_mass));
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
    detach_world_velocity(
        multiply(vehicle.world, previous),
        multiply(vehicle.world, current),
        vehicle,
        rider_basis,
        dt,
        include_motion,
    )
}
///001eff20 receives the Pelvis in world coordinates and converts only the
///angular difference into the Model container's local physics axes.
pub(crate) fn detach_world_velocity(
    previous: [f32; 16],
    current: [f32; 16],
    vehicle: VehicleMotion,
    rider_basis: Basis,
    dt: f32,
    include_motion: bool,
) -> (Vec3, Vec3) {
    let linear = vehicle.linear_velocity;
    let angular = vehicle.local_angular_velocity;
    if !include_motion {
        return (linear, angular);
    }
    let inverse_dt = 1. / dt;
    let delta = std::array::from_fn(|i| (current[12 + i] - previous[12 + i]) * inverse_dt);
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
        add(linear, delta),
        add(
            angular,
            inverse_transform(rider_basis, scale(rotation, inverse_dt)),
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
///001f03e0 adjusts the Model container, while keeping the animated Pelvis
///world transform saved before sampling. Physics axes belong to the container.
pub(crate) fn rebase_model(
    model: [f32; 16],
    pelvis_local: [f32; 16],
    saved_pelvis_world: [f32; 16],
) -> [f32; 16] {
    let correction = multiply(saved_pelvis_world, inverse(multiply(model, pelvis_local)));
    let mut result = multiply(correction, model);
    // Native orthonormalizes forward, then right=up×forward, then up=forward×right.
    let normalize = |v: Vec3| scale(v, (1. / f64::from(dot(v, v)).sqrt()) as f32);
    let forward = normalize([result[8], result[9], result[10]]);
    let right = normalize(cross([result[4], result[5], result[6]], forward));
    let up = cross(forward, right);
    for (column, vector) in [right, up, forward].into_iter().enumerate() {
        result[column * 4..column * 4 + 3].copy_from_slice(&vector);
        result[column * 4 + 3] = 0.;
    }
    let pelvis = crate::obstacle::transform_point(
        &result,
        [pelvis_local[12], pelvis_local[13], pelvis_local[14]],
    );
    for axis in 0..3 {
        result[12 + axis] += saved_pelvis_world[12 + axis] - pelvis[axis];
    }
    result
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
    fn fixture() -> (
        Animator,
        Vec<Node>,
        crate::terrain::Terrain,
        [crate::materials::TerrainMaterial; 8],
    ) {
        let bones = [
            ("Pelvis", [0., 0., 0.]),
            ("UprTorso", [0., 2., 0.]),
            ("Head", [0., 3., 0.]),
            ("LwrArmL", [-1., 2., 0.]),
            ("HandL", [-1., 1.5, 0.]),
            ("LwrArmR", [1., 2., 0.]),
            ("HandR", [1., 1.5, 0.]),
            ("LwrLegL", [-0.5, 0., 0.]),
            ("FootL", [-0.5, -0.8, 0.]),
            ("LwrLegR", [0.5, 0., 0.]),
            ("FootR", [0.5, -0.8, 0.]),
        ];
        let nodes: Vec<_> = bones
            .into_iter()
            .enumerate()
            .map(|(i, (name, position))| {
                let mut matrix = IDENTITY;
                matrix[12..15].copy_from_slice(&position);
                if i == 0 {
                    matrix[0] = 0.;
                    matrix[2] = -1.;
                    matrix[8] = 1.;
                    matrix[10] = 0.;
                }
                Node {
                    name: name.into(),
                    parent: if i == 0 { -1 } else { 0 },
                    local_matrix: matrix,
                    world_matrix: matrix,
                    bounds: [0.; 7],
                }
            })
            .collect();
        let motions = (0..14)
            .map(|_| Motion {
                animation: crate::animation::Animation {
                    version: 2,
                    frame_count: 60,
                    duration: 2.,
                    bone_names: vec![],
                    translation_kind: 0,
                    translations: vec![],
                    rotations: vec![],
                },
                sampling: crate::animation::Sampling::default(),
            })
            .collect();
        let animator = Animator::new(motions, vec![true; 14], &nodes).unwrap();
        let terrain = crate::terrain::Terrain {
            spacing: 10.,
            tiles: vec![crate::terrain::TerrainTile {
                tag: 0,
                heights: [0; 289],
                attributes: [u32::from_le_bytes([0, 127, 0, 0]); 289],
            }],
            leaves: vec![crate::terrain::TerrainLeaf {
                x: 0,
                z: 0,
                tile: 0,
                height_morph: 1.,
                height_scale: 0.,
                minimum_height: 0.,
                maximum_height: 0.,
            }],
            hierarchy_nodes: 1,
            width_cells: 16,
            depth_cells: 16,
        };
        let materials = [crate::materials::TerrainMaterial {
            grip: 0.,
            drag: 0.,
            drive: 0.,
            powerslide: 0.,
            braking: 0.,
        }; 8];
        (animator, nodes, terrain, materials)
    }
    #[test]
    fn separation_keeps_model_axes_and_has_no_gravity_until_detachment() {
        let (mut animator, nodes, terrain, materials) = fixture();
        let channels = animator.pose(&nodes).unwrap();
        assert_ne!(
            matrix_basis(channels.world_matrices[0]),
            matrix_basis(IDENTITY)
        );
        let riding = animator.current.clone();
        let mut world = IDENTITY;
        world[12] = 50.;
        world[13] = 20.;
        world[14] = 50.;
        animator
            .begin(crate::wreck::Kind::Front, riding, world, &nodes)
            .unwrap();
        for _ in 0..10 {
            world[14] += 1.;
            animator
                .step(
                    1. / 60.,
                    VehicleMotion {
                        world,
                        linear_velocity: [0., 0., 60.],
                        local_angular_velocity: [0.; 3],
                    },
                    &nodes,
                    &terrain,
                    &materials,
                )
                .unwrap();
            assert_eq!(animator.world, world);
            assert_eq!(animator.phase, Phase::Attached);
            assert!(!animator.integration_active);
            assert_eq!(animator.body.as_ref().unwrap().basis(), matrix_basis(world));
            assert_eq!(animator.frame().unwrap().linear_velocity, [0.; 3]);
        }
    }
    #[test]
    fn attached_prop_overlap_activates_model_body_before_detach_and_corrects_position() {
        let (mut animator, nodes, terrain, materials) = fixture();
        let mut prop =
            crate::wreck::tests::surface([[-30., 20., -30.], [0., 20., 30.], [30., 20., -30.]]);
        prop.world_matrix[12] = 50.;
        prop.world_matrix[14] = 50.;
        animator.set_static_obstacles(vec![prop]);
        let mut world = IDENTITY;
        world[12] = 50.;
        world[13] = 20.;
        world[14] = 50.;
        animator
            .begin(
                crate::wreck::Kind::Front,
                animator.current.clone(),
                world,
                &nodes,
            )
            .unwrap();
        animator
            .step(
                1. / 60.,
                VehicleMotion {
                    world,
                    linear_velocity: [0.; 3],
                    local_angular_velocity: [0.; 3],
                },
                &nodes,
                &terrain,
                &materials,
            )
            .unwrap();
        let frame = animator.frame().unwrap();
        assert!(frame.static_contact_count > 0);
        assert!(animator.integration_active);
        assert_eq!(frame.phase, Phase::Ground);
        assert!(frame.origin[1] > 20.);
        assert_eq!(frame.clip, "SepFront");
        assert!(animator.pending_clip.is_some());
    }
    #[test]
    fn rebase_preserves_pelvis_world_without_using_pelvis_as_physics_basis() {
        let mut model = IDENTITY;
        model[12] = 30.;
        model[13] = 40.;
        model[14] = 50.;
        let mut old = IDENTITY;
        old[12] = 2.;
        old[13] = 3.;
        let mut current = IDENTITY;
        current[0] = 0.;
        current[2] = -1.;
        current[8] = 1.;
        current[10] = 0.;
        current[12] = 5.;
        let saved = multiply(model, old);
        let result = rebase_model(model, current, saved);
        assert_eq!(multiply(result, current), saved);
        assert_ne!(matrix_basis(result), matrix_basis(saved));
    }
    #[test]
    fn rider_spokes_contact_a_prop_without_terrain_and_resolve_the_endpoint() {
        let endpoints = [[-3., -2., -4.], [3., 2., 4.], [1., 1., 3.]];
        let rays = rider_static_rays(&endpoints);
        assert!(rays.iter().all(|ray| ray.start == [0.; 3]));
        assert_eq!(
            rays.iter().map(|ray| ray.end).collect::<Vec<_>>(),
            endpoints
        );
        let wall = crate::wreck::tests::surface([[-10., -10., 2.], [0., 10., 2.], [10., -10., 2.]]);
        let hits = crate::wreck::query_static_segments(&rays, &[wall], 60).unwrap();
        assert_eq!(hits.len(), 2);
        let config = BodyConfig::new(4., [3.; 3], [0.; 3]).unwrap();
        let mut body = BodyState::new([0.; 3], [1., 0., 0., 0.], &config).unwrap();
        body.linear_velocity = [0., 0., 20.];
        let mut corrections = crate::contact::ContactCorrections::default();
        for hit in hits {
            let penetration = dot(sub(hit.world_point, hit.world_end), hit.world_normal);
            assert!(penetration > 0.);
            crate::contact::resolve_static_with_position_scale(
                &mut body,
                &config,
                crate::contact::ContactConfig {
                    reflection: 0.,
                    angular_impulse_scale: 1.,
                    friction_scale: 1.,
                    friction_torque_scale: 1.,
                },
                crate::contact::StaticContact {
                    point: hit.world_point,
                    normal: hit.world_normal,
                    penetration,
                    material_grip: 0.,
                },
                &mut corrections,
                0.25,
            )
            .unwrap();
        }
        assert!(body.linear_velocity[2] < 20.);
        assert!(corrections.translation[2] < 0.);
        assert!(body.local_angular_velocity.iter().all(|v| v.is_finite()));
    }
    #[test]
    fn animated_bounds_shift_contact_leverage_without_teleporting_the_rider() {
        let points = [[-4., -2., -1.], [8., 6., 3.], [7., 5., 2.]];
        let center = contact_bounds_center(points.into_iter());
        assert_eq!(center, [2., 2., 1.]);
        let mut config = BodyConfig::new(4., [3.; 3], [0.; 3]).unwrap();
        let mut body = BodyState::new([30., 40., 50.], [0.5; 4], &config).unwrap();
        body.linear_velocity = [1., 2., 3.];
        body.local_angular_velocity = [0.1, 0.2, 0.3];
        let origin = body.origin(&config);
        let target = add(origin, transform(body.basis(), center));
        recenter_body(&mut body, &mut config, target);
        assert_eq!(config.center_of_mass, center);
        assert_eq!(body.origin(&config), origin);
        assert_eq!(body.center, target);
        assert_eq!(body.linear_velocity, [1., 2., 3.]);
        assert_eq!(body.local_angular_velocity, [0.1, 0.2, 0.3]);
        body.apply_world_force([0., 10., 0.], target);
        assert_eq!(body.local_torque, [0.; 3]);
        body.apply_world_force([0., 10., 0.], origin);
        assert_ne!(body.local_torque, [0.; 3]);
    }
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
