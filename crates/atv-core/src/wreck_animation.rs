//! Original rider wreck clip tables (002d7dc0), phase selection (001ee990),
//! detached-body constants (001ee090/001efad0) and terrain contact points.
//! Static type-0 prop contacts share the native13 animated point spokes.
//! Authored live water drives four torso points before each solid-contact pass.
//! Moving/compound-prop contacts remain separate integration work.
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
    pub sampled_impact_delta: Vec3,
    pub water_touching: bool,
    pub water_contact_count: usize,
    pub world_matrix: [f32; 16],
    pub origin: Vec3,
    pub linear_velocity: Vec3,
    pub center_of_mass: Vec3,
    pub scene_query: SceneQueryFrame,
}
///Current rider actor query, distinct from the preceding quad query. Plants
///remain candidates; source cylinder contacts and unresolved definitions have
///separate counters.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct SceneQueryFrame {
    pub cached: bool,
    pub refreshed: bool,
    pub cached_nodes: usize,
    pub selected_static: usize,
    pub selected_dynamic: usize,
    pub dynamic_contact_callbacks: usize,
    pub vegetation_candidates: usize,
    pub unsupported_vegetation_proxies: usize,
    pub vegetation_proxy_queries: usize,
    pub vegetation_contact_callbacks: usize,
    pub skip_vegetation_query: bool,
    pub center: Vec3,
    pub radius: f32,
}
#[derive(Clone, Copy)]
struct ContactPoint {
    node: usize,
    offset: Vec3,
}
type BodyContact = (Vec3, Vec3, Vec3, f32, Option<crate::train::MotionState>);
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

//002d7dc0 registers these four low-speed points via0033dbd0; the high
//point list is empty, so these points also serve high-speed rider contacts.
const WATER_POINTS: [Vec3; 4] = [
    [f32::from_bits(0x3eb95810), 0.5, 0.3],
    [-f32::from_bits(0x3eb95810), 0.5, -0.3],
    [f32::from_bits(0x3eb95810), 0.5, -0.3],
    [-f32::from_bits(0x3eb95810), 0.5, 0.3],
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
    water_points: [ContactPoint; 4],
    water_config: crate::water::Config,
    body: Option<BodyState>,
    config: BodyConfig,
    world: [f32; 16],
    previous_time: f32,
    integration_active: bool,
    water_kinematics: crate::water::PointKinematics,
    anchor_pelvis: bool,
    pending_clip: Option<usize>,
    rng: OriginalRandom,
    grounded: bool,
    contact_count: usize,
    static_contact_count: usize,
    sampled_impact_delta: Vec3,
    water_touching: bool,
    water_contact_count: usize,
    obstacles: Vec<crate::obstacle::StaticObstacle>,
    static_exclusions: Vec<usize>,
    scene_query: SceneQueryFrame,
    active: bool,
}
impl Animator {
    /// New detached actor; no player pose, contacts or wreck clock are copied.
    /// Stream seeding is an integration choice, not original global RNG parity.
    pub fn fork_racer(&self, nodes: &[Node], seed: u64) -> Result<Self> {
        let mut rider = Self::new(self.motions.clone(), self.looping.clone(), nodes)?;
        rider.water_config = self.water_config;
        rider.rng = OriginalRandom::with_seed(seed);
        Ok(rider)
    }
    /// Consecutive shared launch yaw, pitch, wreck direction and clip-variant
    /// words. The local stream reproduces this order, not all game-wide RNG calls.
    pub fn boundary_random_words(&mut self) -> [u32; 4] {
        std::array::from_fn(|_| self.rng.next_u31())
    }

    /// Boundary launch raises both quad and rider limits before wreck entry;
    ///001e8d10 restores both on recovery. Other detached-body settings persist.
    pub fn set_boundary_flight(&mut self, active: bool) {
        self.config.max_speed_squared = if active {
            crate::boundary_launch::LAUNCH_MAX_SPEED_SQUARED
        } else {
            crate::boundary_launch::NORMAL_MAX_SPEED_SQUARED
        };
    }

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
            water_points: WATER_POINTS.map(|offset| ContactPoint {
                node: points[1].node,
                offset,
            }),
            water_config: crate::water::Config::rider(mass)?,
            points,
            body: None,
            config,
            world: IDENTITY,
            previous_time: 0.,
            integration_active: false,
            water_kinematics: crate::water::PointKinematics {
                center: [0.; 3],
                angular_velocity: [0.; 3],
            },
            anchor_pelvis: false,
            pending_clip: None,
            rng: OriginalRandom::with_seed(1),
            grounded: false,
            contact_count: 0,
            static_contact_count: 0,
            sampled_impact_delta: [0.; 3],
            water_touching: false,
            water_contact_count: 0,
            obstacles: Vec::new(),
            static_exclusions: Vec::new(),
            scene_query: SceneQueryFrame::default(),
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
        self.static_exclusions.clear();
    }
    ///Explicit actor exclusion metadata in the ordinary loader's index space.
    ///Do not infer the native MUD category from an unverified collision filename.
    ///The live transport currently supplies no exceptional category/object list.
    pub fn set_static_exclusions(&mut self, mut indices: Vec<usize>) -> Result<()> {
        if indices.iter().any(|&index| index >= self.obstacles.len()) {
            return Err("rider static exclusion outside supplied scene".into());
        }
        indices.sort_unstable();
        indices.dedup();
        self.static_exclusions = indices;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.active = false;
        self.body = None;
        self.integration_active = false;
        //001f0740 deactivates the wreck but retains body+60/+290. The next
        //wreck's first water dispatch still reads the last integration cache.
        self.anchor_pelvis = false;
        self.pending_clip = None;
        self.config.center_of_mass = [0.; 3];
        self.grounded = false;
        self.contact_count = 0;
        self.static_contact_count = 0;
        self.sampled_impact_delta = [0.; 3];
        self.water_touching = false;
        self.water_contact_count = 0;
        self.scene_query = SceneQueryFrame::default();
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
        //001efb90 resets velocity/history through00146700, retaining the
        //cached world center/angular velocity from the preceding body step.
        self.anchor_pelvis = false;
        self.pending_clip = None;
        self.grounded = false;
        self.contact_count = 0;
        self.static_contact_count = 0;
        self.sampled_impact_delta = [0.; 3];
        self.water_touching = false;
        self.water_contact_count = 0;
        self.scene_query = SceneQueryFrame::default();
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
            sampled_impact_delta: self.sampled_impact_delta,
            water_touching: self.water_touching,
            water_contact_count: self.water_contact_count,
            world_matrix: self.world,
            origin: [self.world[12], self.world[13], self.world[14]],
            linear_velocity: self.body.as_ref().map_or([0.; 3], |b| b.linear_velocity),
            center_of_mass: self.config.center_of_mass,
            scene_query: self.scene_query,
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
        self.step_with_water(dt, vehicle, nodes, terrain, materials, None)
    }
    ///001ef690 samples the current dynamic field in every physics substep.
    ///The caller advances the water actor once before this outer rider step.
    pub fn step_with_water(
        &mut self,
        dt: f32,
        vehicle: VehicleMotion,
        nodes: &[Node],
        terrain: &crate::terrain::Terrain,
        materials: &[crate::materials::TerrainMaterial; 8],
        water: Option<&crate::water_animation::LiveField>,
    ) -> Result<()> {
        self.step_with_scene(dt, vehicle, nodes, terrain, materials, water, None, false)
    }
    ///00193b40 prioritizes the attached shared-cache binding over direct query.
    ///Caller borrows the Vehicle's exact scene cache throughout wreck phases;
    ///the actor query bit controls plant expansion independently of the cache.
    #[allow(clippy::too_many_arguments)]
    pub fn step_with_scene(
        &mut self,
        dt: f32,
        vehicle: VehicleMotion,
        nodes: &[Node],
        terrain: &crate::terrain::Terrain,
        materials: &[crate::materials::TerrainMaterial; 8],
        water: Option<&crate::water_animation::LiveField>,
        scene: Option<&mut crate::scene_collision::SceneCollision>,
        skip_vegetation_query: bool,
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
        let last_contacts = self.step_body_with_scene(
            dt,
            &pose,
            terrain,
            materials,
            !attached,
            water,
            scene,
            skip_vegetation_query,
        )?;
        let saved_pelvis = multiply(self.world, pose.world_matrices[0]);
        //001ee990 performs physics and saves Pelvis before advancing animation.
        //Category changes request the next clip on the following outer frame.
        if !attached {
            let next = detached_contact_phase(self.phase, last_contacts > 0, self.water_touching);
            if next != self.phase {
                self.phase = next;
                self.pending_clip = Some(if next == Phase::Water {
                    //The one-entry Swim table still consumes a random word.
                    self.rng.next_u31();
                    13
                } else {
                    self.random_four()
                });
            }
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
            if self.water_touching || last_contacts > 0 {
                self.detach(saved_pelvis, current_pelvis, vehicle, dt, false)?;
                if self.water_touching {
                    //Attached phase prioritizes water over solid contacts.
                    //detach's random draw also serves the one-entry Swim table.
                    self.phase = Phase::Water;
                    self.pending_clip = Some(13);
                }
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
    #[cfg(test)]
    fn step_body(
        &mut self,
        dt: f32,
        pose: &Pose,
        terrain: &crate::terrain::Terrain,
        materials: &[crate::materials::TerrainMaterial; 8],
        gravity: bool,
        water: Option<&crate::water_animation::LiveField>,
    ) -> Result<usize> {
        self.step_body_with_scene(dt, pose, terrain, materials, gravity, water, None, false)
    }
    #[allow(clippy::too_many_arguments)]
    fn step_body_with_scene(
        &mut self,
        dt: f32,
        pose: &Pose,
        terrain: &crate::terrain::Terrain,
        materials: &[crate::materials::TerrainMaterial; 8],
        gravity: bool,
        water: Option<&crate::water_animation::LiveField>,
        scene: Option<&mut crate::scene_collision::SceneCollision>,
        skip_vegetation_query: bool,
    ) -> Result<usize> {
        //001469e0 obtains position/orientation from the Model scene on every
        //step. Root rebasing changes that scene basis without rotating stored
        //local angular velocity or discarding Adams–Bashforth history.
        let body = self.body.as_mut().ok_or("missing rider Model body")?;
        //001ef690 updates localCoG from animated bounds but retains world+60
        //and+290 until integration. Detach also changes local angular velocity
        //without refreshing+290, so preserve an explicit integration cache.
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
        contacts.extend(self.static_points_with_scene(pose, scene, skip_vegetation_query)?);
        self.grounded = false;
        self.contact_count = 0;
        self.static_contact_count = 0;
        self.sampled_impact_delta = [0.; 3];
        self.water_touching = false;
        self.water_contact_count = 0;
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
            self.water_touching = false;
            if let Some(water) = water {
                let points = self.water_points.map(|point| {
                    crate::obstacle::transform_point(
                        &model,
                        crate::obstacle::transform_point(
                            &pose.world_matrices[point.node],
                            point.offset,
                        ),
                    )
                });
                //00451470 initializes flow=[0,0,0,1], up=[0,1,0,1].
                //These live profiles have WakeScale=0: ripple injection cannot
                //change sampled heights; rendering wakes remains separate.
                let result = crate::water::apply_contacts_with_kinematics(
                    body,
                    self.water_config,
                    dot(body.linear_velocity, body.linear_velocity).sqrt(),
                    &[],
                    &points,
                    [0.; 3],
                    [0., 1., 0.],
                    |point| water.height(point, false),
                    |_, _| {},
                    self.water_kinematics,
                )?;
                self.water_touching = result.touching;
                self.water_contact_count += result.points;
                if result.touching {
                    self.grounded = true;
                    self.integration_active = true;
                }
            }
            let mut corrections = crate::contact::ContactCorrections::default();
            last_contacts = 0;
            for (index, &(local, ground, normal, grip, target)) in contacts.iter().enumerate() {
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
                    let config = crate::contact::ContactConfig {
                        reflection: 0.,
                        angular_impulse_scale: 1.,
                        friction_scale: 1.,
                        friction_torque_scale: 1.,
                    };
                    let contact = crate::contact::StaticContact {
                        point: add(point, scale(normal, penetration)),
                        normal,
                        penetration,
                        material_grip: grip,
                    };
                    if let Some(target) = target {
                        target.resolve_contact_with_position_scale(
                            body,
                            &self.config,
                            config,
                            contact,
                            &mut corrections,
                            0.25,
                        )?;
                    } else {
                        crate::contact::resolve_static_with_position_scale(
                            body,
                            &self.config,
                            config,
                            contact,
                            &mut corrections,
                            0.25,
                        )?;
                    }
                }
            }
            for (sample, delta) in self
                .sampled_impact_delta
                .iter_mut()
                .zip(corrections.velocity_delta)
            {
                if delta.abs() > sample.abs() {
                    *sample = delta;
                }
            }
            if self.integration_active {
                body.step(&self.config, step)?;
                body.center = add(body.center, corrections.translation);
                self.water_kinematics = crate::water::PointKinematics::from_body(body);
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
    fn static_points_with_scene(
        &mut self,
        pose: &Pose,
        scene: Option<&mut crate::scene_collision::SceneCollision>,
        skip_vegetation_query: bool,
    ) -> Result<Vec<BodyContact>> {
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
        let (center, radius) = rider_query_sphere(&endpoints);
        self.scene_query = SceneQueryFrame {
            center,
            radius,
            skip_vegetation_query,
            ..SceneQueryFrame::default()
        };
        let selected = if let Some(scene) = scene {
            let exclusions = &self.static_exclusions;
            let mud = scene.mud_obstacles().to_vec();
            let query = scene.cached_query(center, radius, |index, _| {
                !exclusions.contains(&index) && !mud[index]
            })?;
            let selected = scene.physical_selection(
                &query,
                crate::scene_collision::PhysicalTrim::CachedRider,
                center,
                radius,
                skip_vegetation_query,
                |index, _| !exclusions.contains(&index) && !mud[index],
            )?;
            self.scene_query.cached = true;
            self.scene_query.refreshed = query.refreshed;
            self.scene_query.cached_nodes = query.candidates.len();
            self.scene_query.selected_static = selected.static_candidates;
            self.scene_query.selected_dynamic = selected.dynamic_candidates;
            self.scene_query.vegetation_candidates = selected.vegetation_candidates;
            self.scene_query.unsupported_vegetation_proxies =
                selected.unsupported_vegetation_proxies;
            self.scene_query.vegetation_proxy_queries = selected.vegetation_proxies();
            selected.meshes
        } else {
            let obstacles = self
                .obstacles
                .iter()
                .enumerate()
                .filter(|(index, _)| !self.static_exclusions.contains(index))
                .map(|(index, obstacle)| crate::scene_collision::PhysicalMesh {
                    node: crate::scene_collision::SceneNode::StaticObstacle(index),
                    proxy_index: None,
                    obstacle: obstacle.clone(),
                    motion: None,
                    mud: false,
                })
                .collect::<Vec<_>>();
            self.scene_query.selected_static = obstacles.len();
            obstacles
        };
        let mut contacts = Vec::new();
        for mesh in selected {
            //The type3 nearest buffer resets for each ordinary object/proxy.
            //Its shared output still has the original sixty-record capacity.
            let hits = crate::wreck::query_static_segments(
                &rays,
                std::slice::from_ref(&mesh.obstacle),
                60 - contacts.len(),
            )?;
            if !hits.is_empty() && mesh.proxy_index.is_some() {
                self.scene_query.vegetation_contact_callbacks += 1;
            }
            if !hits.is_empty() && mesh.motion.is_some() {
                self.scene_query.dynamic_contact_callbacks += 1;
            }
            contacts.extend(hits.into_iter().map(|hit| (hit, mesh.motion)));
        }
        let to_local = inverse(self.world);
        Ok(contacts
            .into_iter()
            .map(|(hit, target)| {
                (
                    crate::obstacle::transform_point(&to_local, hit.world_end),
                    hit.world_point,
                    hit.world_normal,
                    0.2,
                    target,
                )
            })
            .collect())
    }
    fn terrain_points(
        &self,
        pose: &Pose,
        terrain: &crate::terrain::Terrain,
        _materials: &[crate::materials::TerrainMaterial; 8],
    ) -> Result<Vec<BodyContact>> {
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
                    None,
                ));
            }
        }
        Ok(points)
    }
}
//001ee990 priorities depend on the current phase and the LAST substep's
//contact flags. Once swimming, dry frames keep swimming until a solid hit.
fn detached_contact_phase(phase: Phase, solid: bool, water: bool) -> Phase {
    match phase {
        Phase::Air if solid => Phase::Ground,
        Phase::Air | Phase::Ground if water => Phase::Water,
        Phase::Water if solid => Phase::Ground,
        _ => phase,
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
///00327360 derives the actor sphere from the current segment endpoints. Spoke
///starts lie within these same animated bounds and cannot enlarge the sphere.
fn rider_query_sphere(endpoints: &[Vec3]) -> (Vec3, f32) {
    let center = contact_bounds_center(endpoints.iter().copied());
    let mut low = [f32::MAX; 3];
    let mut high = [-f32::MAX; 3];
    for &point in endpoints {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    let half = std::array::from_fn(|axis| (high[axis] - low[axis]) * 0.5);
    (center, dot(half, half).sqrt())
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
            leaf_index: Default::default(),
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
    fn water_fixture() -> crate::water_animation::LiveField {
        //A deliberately small varying signature verifies the rider consumes
        //the supplied live field; this is synthetic test data, not a scene.
        let definition = crate::water_animation::Definition::parse(
            b"[General Information]\nWaterBaseHeight = 24\nWakeScale = 0\nWaterHeightRange = 0\n",
        )
        .unwrap();
        crate::water_animation::LiveField {
            definition,
            swell: crate::water_animation::SwellState::default(),
            states: vec![],
            frames: vec![],
            waves: vec![crate::water_surface::SignatureWave {
                origin: [0.; 2],
                width: [100.; 2],
                inverse_width: [0.01; 2],
                dimension: 2,
                row_stride: 2,
                pixel_stride: 1,
                tiled: true,
                height_channel: true,
                positive: true,
                scale: 16.,
                first_frame: vec![0, 32, 64, 128],
                second_frame: vec![0, 32, 64, 128],
            }],
        }
    }
    fn start_fixture(animator: &mut Animator, nodes: &[Node]) -> VehicleMotion {
        let mut world = IDENTITY;
        world[12..15].copy_from_slice(&[50., 20., 50.]);
        animator
            .begin(
                crate::wreck::Kind::Front,
                animator.current.clone(),
                world,
                nodes,
            )
            .unwrap();
        VehicleMotion {
            world,
            linear_velocity: [0.; 3],
            local_angular_velocity: [0.; 3],
        }
    }
    #[test]
    fn water_only_contact_activates_rider_and_requests_swimming() {
        let (mut animator, nodes, terrain, materials) = fixture();
        let vehicle = start_fixture(&mut animator, &nodes);
        let water = water_fixture();
        animator
            .step_with_water(
                1. / 60.,
                vehicle,
                &nodes,
                &terrain,
                &materials,
                Some(&water),
            )
            .unwrap();
        let frame = animator.frame().unwrap();
        assert_eq!(frame.phase, Phase::Water);
        assert_eq!(animator.pending_clip, Some(13));
        assert!(frame.grounded && frame.water_touching && animator.integration_active);
        assert_eq!(frame.contact_count, 0);
        assert_eq!(frame.static_contact_count, 0);
        assert_eq!(frame.water_contact_count, 8); //four points, two substeps
        //The attached→water detach resets velocity to the vehicle's velocity
        //after this frame's water integration; its position still rose.
        assert_eq!(frame.linear_velocity, [0.; 3]);
        assert!(frame.origin[1] > 20.);
        assert!(frame.linear_velocity.into_iter().all(f32::is_finite));
        animator
            .step_with_water(
                1. / 60.,
                vehicle,
                &nodes,
                &terrain,
                &materials,
                Some(&water),
            )
            .unwrap();
        let frame = animator.frame().unwrap();
        assert_eq!(frame.clip, "Swim");
        assert!(frame.linear_velocity[1] > 0.);
        assert!(frame.linear_velocity.into_iter().all(f32::is_finite));
        //Recovery clears the frame latch; existing dry wrapper keeps the
        //source swimming phase until solid contact but applies no water.
        animator
            .step(1. / 60., vehicle, &nodes, &terrain, &materials)
            .unwrap();
        let frame = animator.frame().unwrap();
        assert_eq!(frame.phase, Phase::Water);
        assert!(!frame.water_touching && !frame.grounded);
        assert_eq!(frame.water_contact_count, 0);
        animator.reset();
        assert!(!animator.water_touching);
    }
    #[test]
    fn animated_model_rebinding_retains_water_kinematics_until_integration() {
        let (mut animator, nodes, terrain, materials) = fixture();
        start_fixture(&mut animator, &nodes);
        animator.integration_active = true;
        animator.phase = Phase::Water;
        animator.water_kinematics.angular_velocity = [0., 0., 1.];
        let body = animator.body.as_mut().unwrap();
        body.center[0] -= 1.; //previous physical center before Model rebasing
        body.local_angular_velocity = [0., 0., 0.25]; //detach changed local state
        animator.water_kinematics.center = body.center;
        let cached = animator.water_kinematics;
        let pose = animator.current.pose(&nodes).unwrap();
        let water = water_fixture();
        let mut expected = animator.body.as_ref().unwrap().clone();
        let mut config = animator.config.clone();
        expected.orientation = quaternion(animator.world);
        expected.center = add(
            [animator.world[12], animator.world[13], animator.world[14]],
            transform(expected.basis(), config.center_of_mass),
        );
        let center = contact_bounds_center(animator.points.iter().map(|point| {
            crate::obstacle::transform_point(
                &animator.world,
                crate::obstacle::transform_point(&pose.world_matrices[point.node], point.offset),
            )
        }));
        recenter_body(&mut expected, &mut config, center);
        //001ef690 consumes the previous integration cache on the first
        //substep and the newly published cache on the second. The dry terrain
        //is far below this body, so only the water actor can activate contacts.
        let dt: f32 = 1. / 60.;
        let mut elapsed = 0.;
        let mut expected_cache = cached;
        let mut point_count = 0;
        while elapsed < dt {
            let step = (dt - elapsed).min(f32::from_bits(0x3c360b61));
            let model = body_matrix(&expected, &config);
            let points = animator.water_points.map(|point| {
                crate::obstacle::transform_point(
                    &model,
                    crate::obstacle::transform_point(
                        &pose.world_matrices[point.node],
                        point.offset,
                    ),
                )
            });
            point_count += crate::water::apply_contacts_with_kinematics(
                &mut expected,
                animator.water_config,
                0.,
                &[],
                &points,
                [0.; 3],
                [0., 1., 0.],
                |point| water.height(point, false),
                |_, _| {},
                expected_cache,
            )
            .unwrap()
            .points;
            expected.step(&config, step).unwrap();
            expected_cache = crate::water::PointKinematics::from_body(&expected);
            elapsed += step;
        }
        assert_eq!(point_count, 8);
        animator
            .step_body(dt, &pose, &terrain, &materials, false, Some(&water))
            .unwrap();
        let body = animator.body.as_ref().unwrap();
        assert_eq!(body.linear_velocity, expected.linear_velocity);
        assert_eq!(body.local_angular_velocity, expected.local_angular_velocity);
        assert_eq!(animator.water_contact_count, point_count);
        assert_eq!(animator.water_kinematics.center, body.center);
        assert_eq!(
            animator.water_kinematics.angular_velocity,
            body.world_angular_velocity()
        );
        assert_ne!(
            animator.water_kinematics.angular_velocity,
            cached.angular_velocity
        );
        //001eff20 changes local angular velocity and adds random spin, but
        //does not publish world+290 until the next body integration.
        let before = animator.water_kinematics.angular_velocity;
        let before_center = animator.water_kinematics.center;
        animator
            .detach(
                IDENTITY,
                IDENTITY,
                VehicleMotion {
                    world: IDENTITY,
                    linear_velocity: [0.; 3],
                    local_angular_velocity: [0.; 3],
                },
                dt,
                false,
            )
            .unwrap();
        assert_eq!(animator.water_kinematics.angular_velocity, before);
        animator.reset();
        assert_eq!(animator.water_kinematics.angular_velocity, before);
        assert_eq!(animator.water_kinematics.center, before_center);
    }
    #[test]
    fn disabled_body_retains_water_cache_across_dry_animated_frames() {
        let (mut animator, nodes, terrain, materials) = fixture();
        let mut vehicle = start_fixture(&mut animator, &nodes);
        animator.water_kinematics.center = [5., 6., 7.];
        animator.water_kinematics.angular_velocity = [0.125, 0.25, 0.375];
        let before = animator.water_kinematics;
        vehicle.world[12] += 5.;
        vehicle.world[13] += 4.;
        //The supplied dry field still dispatches the water actor twice. With
        //no solid or water contacts, the native body remains disabled and its
        //cached world center/angular velocity must survive animated rebinding.
        let mut water = water_fixture();
        water.definition.surface.base_height = -100.;
        animator
            .step_with_water(
                1. / 60.,
                vehicle,
                &nodes,
                &terrain,
                &materials,
                Some(&water),
            )
            .unwrap();
        assert!(!animator.integration_active);
        assert!(!animator.grounded);
        assert_eq!(animator.water_contact_count, 0);
        assert_eq!(animator.water_kinematics.center, before.center);
        assert_eq!(
            animator.water_kinematics.angular_velocity,
            before.angular_velocity
        );
        assert_ne!(animator.body.as_ref().unwrap().center, before.center);
        //Entry resets BodyState velocity/history while retaining the cache;
        //recovery also preserves it until a later enabled body integration.
        animator.reset();
        start_fixture(&mut animator, &nodes);
        assert_eq!(animator.water_kinematics.center, before.center);
        assert_eq!(
            animator.water_kinematics.angular_velocity,
            before.angular_velocity
        );
        let body = animator.body.as_ref().unwrap();
        assert_eq!(body.linear_velocity, [0.; 3]);
        assert_eq!(body.local_angular_velocity, [0.; 3]);
    }
    #[test]
    fn dry_field_matches_the_existing_no_water_path() {
        let (mut dry, nodes, terrain, materials) = fixture();
        let (mut old, _, _, _) = fixture();
        let vehicle = start_fixture(&mut dry, &nodes);
        start_fixture(&mut old, &nodes);
        let mut water = water_fixture();
        water.definition.surface.base_height = -100.;
        for _ in 0..120 {
            dry.step_with_water(
                1. / 60.,
                vehicle,
                &nodes,
                &terrain,
                &materials,
                Some(&water),
            )
            .unwrap();
            old.step(1. / 60., vehicle, &nodes, &terrain, &materials)
                .unwrap();
            let dry = dry.frame().unwrap();
            let old = old.frame().unwrap();
            assert_eq!(dry.phase, old.phase);
            assert_eq!(dry.clip, old.clip);
            assert_eq!(dry.time, old.time);
            assert_eq!(dry.world_matrix, old.world_matrix);
            assert_eq!(dry.linear_velocity, old.linear_velocity);
            assert_eq!(dry.center_of_mass, old.center_of_mass);
            assert_eq!(dry.grounded, old.grounded);
            assert_eq!(dry.contact_count, old.contact_count);
            assert_eq!(dry.water_contact_count, 0);
        }
    }
    #[test]
    fn detached_phase_priorities_match_native_last_substep_flags() {
        for (phase, solid, water, expected) in [
            (Phase::Air, true, true, Phase::Ground),
            (Phase::Air, false, true, Phase::Water),
            (Phase::Ground, true, true, Phase::Water),
            (Phase::Ground, false, false, Phase::Ground),
            (Phase::Water, true, true, Phase::Ground),
            (Phase::Water, false, false, Phase::Water),
        ] {
            assert_eq!(detached_contact_phase(phase, solid, water), expected);
        }
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
    fn collision_scene(
        obstacles: Vec<crate::obstacle::StaticObstacle>,
        ecosystem: Option<&crate::ecosystem::Ecosystem>,
    ) -> crate::scene_collision::SceneCollision {
        crate::scene_collision::SceneCollision::new(
            crate::scene_partition::Config {
                min_x: 0.,
                max_x: 256.,
                min_z: 0.,
                max_z: 256.,
                cell_size: 32.,
            },
            obstacles,
            ecosystem,
            false,
            |_| Ok(0.),
        )
        .unwrap()
    }
    #[test]
    fn shared_rider_prop_query_matches_direct_motion_through_detach_and_recovery_binding() {
        let (mut cached, nodes, terrain, materials) = fixture();
        let (mut direct, _, _, _) = fixture();
        let mut prop =
            crate::wreck::tests::surface([[-30., 20., -30.], [0., 20., 30.], [30., 20., -30.]]);
        prop.world_matrix[12] = 50.;
        prop.world_matrix[14] = 50.;
        cached.set_static_obstacles(vec![prop.clone()]);
        direct.set_static_obstacles(vec![prop.clone()]);
        let mut scene = collision_scene(vec![prop], None);
        let vehicle = start_fixture(&mut cached, &nodes);
        start_fixture(&mut direct, &nodes);
        let quad = scene
            .cached_query([50., 20., 50.], 3., |_, _| true)
            .unwrap();
        assert!(quad.refreshed);
        let mut touched = false;
        for step in 0..120 {
            cached
                .step_with_scene(
                    1. / 60.,
                    vehicle,
                    &nodes,
                    &terrain,
                    &materials,
                    None,
                    Some(&mut scene),
                    false,
                )
                .unwrap();
            direct
                .step(1. / 60., vehicle, &nodes, &terrain, &materials)
                .unwrap();
            let mut a = cached.frame().unwrap();
            let mut b = direct.frame().unwrap();
            assert!(a.scene_query.cached);
            assert_eq!(a.scene_query.cached_nodes, 1);
            assert_eq!(a.scene_query.selected_static, 1);
            if step == 0 {
                assert!(!a.scene_query.refreshed);
                assert!(a.static_contact_count > 0);
                assert_eq!(a.phase, Phase::Ground);
            }
            touched |= a.static_contact_count > 0;
            a.scene_query = SceneQueryFrame::default();
            b.scene_query = SceneQueryFrame::default();
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "rider frame{step}");
        }
        assert!(touched);
        let before = scene
            .cache()
            .entries()
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>();
        let recovery = scene
            .cached_query([50., 20., 50.], 20., |_, _| true)
            .unwrap();
        assert!(!recovery.refreshed);
        cached.reset();
        assert_eq!(
            scene
                .cache()
                .entries()
                .iter()
                .map(|entry| entry.0)
                .collect::<Vec<_>>(),
            before
        );
        start_fixture(&mut cached, &nodes);
        cached
            .step_with_scene(
                1. / 60.,
                vehicle,
                &nodes,
                &terrain,
                &materials,
                None,
                Some(&mut scene),
                false,
            )
            .unwrap();
        assert!(cached.frame().unwrap().scene_query.cached);
        assert!(!cached.frame().unwrap().scene_query.refreshed);
    }
    #[test]
    fn rider_inclusive_selection_rechecks_exclusions_and_refresh_affects_shared_membership() {
        let (mut animator, nodes, terrain, materials) = fixture();
        let mut prop =
            crate::wreck::tests::surface([[-30., 20., -30.], [0., 20., 30.], [30., 20., -30.]]);
        prop.world_matrix[12] = 50.;
        prop.world_matrix[14] = 50.;
        animator.set_static_obstacles(vec![prop.clone()]);
        let mut scene = collision_scene(vec![prop], None);
        let vehicle = start_fixture(&mut animator, &nodes);
        scene
            .cached_query([50., 20., 50.], 3., |_, _| true)
            .unwrap();
        animator.set_static_exclusions(vec![0]).unwrap();
        animator
            .step_with_scene(
                1. / 60.,
                vehicle,
                &nodes,
                &terrain,
                &materials,
                None,
                Some(&mut scene),
                false,
            )
            .unwrap();
        let query = animator.frame().unwrap().scene_query;
        assert!(query.cached && !query.refreshed);
        assert_eq!(query.cached_nodes, 1);
        assert_eq!(query.selected_static, 0);
        assert_eq!(animator.static_contact_count, 0);
        //A rider refresh applies its own eligibility to the shared cache; the
        //following quad reuse must not restore that excluded object's entry.
        let mut moved = vehicle;
        moved.world[12] = 100.;
        animator
            .step_with_scene(
                1. / 60.,
                moved,
                &nodes,
                &terrain,
                &materials,
                None,
                Some(&mut scene),
                false,
            )
            .unwrap();
        assert!(animator.frame().unwrap().scene_query.refreshed);
        assert!(scene.cache().entries().is_empty());
        let quad = scene
            .cached_query([100., 20., 50.], 3., |_, _| true)
            .unwrap();
        assert!(!quad.refreshed);
        assert!(quad.candidates.is_empty());
    }
    #[test]
    fn rider_query_retains_exact_radius_candidate() {
        use crate::collision::{CollisionMesh, RayTreeNode, Triangle};
        let (mut animator, nodes, _, _) = fixture();
        start_fixture(&mut animator, &nodes);
        let pose = animator.current.pose(&nodes).unwrap();
        let local_center = contact_bounds_center(animator.points.iter().map(|point| {
            crate::obstacle::transform_point(&pose.world_matrices[point.node], point.offset)
        }));
        //Place this dispatch-only fixture near zero, avoiding loss of radius
        //bits when constructing the exact-radius point at a large origin.
        animator.world[12..15].copy_from_slice(&scale(local_center, -1.));
        let endpoints: Vec<_> = animator
            .points
            .iter()
            .map(|p| {
                crate::obstacle::transform_point(
                    &animator.world,
                    crate::obstacle::transform_point(&pose.world_matrices[p.node], p.offset),
                )
            })
            .collect();
        let (center, radius) = rider_query_sphere(&endpoints);
        //A degenerate, skipped face supplies a point sphere at exactly the
        //rider radius. This tests dispatch eligibility, not a physical contact.
        let point = add(center, [radius, 0., 0.]);
        let prop = crate::obstacle::StaticObstacle {
            mesh: std::rc::Rc::new(CollisionMesh {
                transform: [0.; 12],
                vertices: vec![point; 3],
                vertex_normal_bits: None,
                triangles: vec![Triangle {
                    indices: [0, 1, 2],
                    normal: [0.; 3],
                    distance: 0.,
                    invalid_plane_bits: Some([0; 4]),
                }],
                broad_phase_nodes: 1,
                flags: 0,
                nodes: vec![RayTreeNode {
                    bound_measure: 0.,
                    center: point,
                    half_extents: [0.; 3],
                    children: [0x8000; 2],
                }],
            }),
            world_matrix: IDENTITY,
        };
        animator.set_static_obstacles(vec![prop.clone()]);
        let mut scene = collision_scene(vec![prop], None);
        let query = scene.cached_query(center, radius, |_, _| true).unwrap();
        assert_eq!(query.candidates[0].distance.to_bits(), radius.to_bits());
        assert!(scene.static_selection(&query).is_empty());
        animator
            .static_points_with_scene(&pose, Some(&mut scene), false)
            .unwrap();
        assert_eq!(animator.scene_query.selected_static, 1);
        assert_eq!(animator.scene_query.radius.to_bits(), radius.to_bits());
    }
    #[test]
    fn rider_plant_query_bit_skips_proxy_expansion_without_changing_cache_membership() {
        use crate::ecosystem::{Ecosystem, Placement, Species};
        let (mut animator, nodes, terrain, materials) = fixture();
        let vehicle = start_fixture(&mut animator, &nodes);
        let eco = Ecosystem {
            species: vec![Species {
                slot: 0,
                model: String::new(),
                billboard: String::new(),
                atlas: None,
                parameters: [0; 23],
                optional_name: None,
                proxies: vec![[0; 9]; 2],
            }],
            placements: vec![Placement {
                slot: 0,
                resample_height: false,
                coordinates: [12800, 0, 12800],
                height: 20.,
                radius: 2.,
            }],
            global_name: None,
            global_word: 0,
            placement_texture: None,
            visibility_index: Default::default(),
        };
        let mut scene = collision_scene(vec![], Some(&eco));
        for (skip, unsupported) in [(false, 2), (true, 0), (false, 2)] {
            animator
                .step_with_scene(
                    1. / 60.,
                    vehicle,
                    &nodes,
                    &terrain,
                    &materials,
                    None,
                    Some(&mut scene),
                    skip,
                )
                .unwrap();
            let frame = animator.frame().unwrap();
            assert_eq!(frame.scene_query.cached_nodes, 1);
            assert_eq!(frame.scene_query.vegetation_candidates, 1);
            assert_eq!(
                frame.scene_query.unsupported_vegetation_proxies,
                unsupported
            );
            assert_eq!(frame.scene_query.skip_vegetation_query, skip);
            assert_eq!(frame.static_contact_count, 0);
            assert_eq!(frame.scene_query.selected_static, 0);
        }
    }

    #[test]
    fn attached_cylinder_contact_activates_rider_and_query_bit_preserves_binding() {
        use crate::ecosystem::{Ecosystem, Placement, Species};
        let mut observed = Vec::new();
        for skip in [false, true] {
            let (mut animator, nodes, terrain, materials) = fixture();
            let vehicle = start_fixture(&mut animator, &nodes);
            let mut parameters = [0; 23];
            parameters[0] = 20f32.to_bits();
            parameters[3] = 2f32.to_bits();
            let eco = Ecosystem {
                species: vec![Species {
                    slot: 0,
                    model: String::new(),
                    billboard: String::new(),
                    atlas: None,
                    parameters,
                    optional_name: None,
                    proxies: vec![[1, 0, 0, 0, 0, 0, 0, 0.5f32.to_bits(), 5f32.to_bits()]],
                }],
                placements: vec![Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [13056, 4608, 12826],
                    height: 20.,
                    radius: 2.,
                }],
                global_name: None,
                global_word: 0,
                placement_texture: None,
                visibility_index: Default::default(),
            };
            let mut scene = collision_scene(vec![], Some(&eco));
            animator
                .step_with_scene(
                    1. / 60.,
                    vehicle,
                    &nodes,
                    &terrain,
                    &materials,
                    None,
                    Some(&mut scene),
                    skip,
                )
                .unwrap();
            let frame = animator.frame().unwrap();
            assert!(frame.scene_query.cached);
            assert_eq!(frame.scene_query.vegetation_candidates, 1);
            assert_eq!(frame.scene_query.unsupported_vegetation_proxies, 0);
            observed.push((frame, animator.integration_active));
        }
        let (hit, active) = &observed[0];
        let (bypassed, inactive) = &observed[1];
        assert!(*active, "rider cylinder query {:?}", hit.scene_query);
        assert!(hit.static_contact_count > 0);
        assert!(hit.scene_query.vegetation_contact_callbacks > 0);
        assert_eq!(hit.phase, Phase::Ground);
        assert!(hit.origin[0] < 50.);
        assert!(!inactive);
        assert_eq!(bypassed.static_contact_count, 0);
        assert_eq!(bypassed.scene_query.vegetation_proxy_queries, 0);
        assert_eq!(bypassed.phase, Phase::Attached);
        assert_eq!(bypassed.origin, [50., 20., 50.]);
    }

    ///Cached rider dispatch (0011fd30) visits Arlington's dynamic list after
    ///static/plant nodes. A moving body's point velocity enters the rider's
    ///contact response (default friction flag1), unlike stationary geometry.
    #[test]
    fn rider_contact_with_a_moving_body_uses_its_point_velocity() {
        let mut observed = Vec::new();
        for speed in [0., 30.] {
            let (mut animator, nodes, terrain, materials) = fixture();
            let vehicle = start_fixture(&mut animator, &nodes);
            let mut prop =
                crate::wreck::tests::surface([[-30., 20., -30.], [0., 20., 30.], [30., 20., -30.]]);
            prop.world_matrix[12] = 50.;
            prop.world_matrix[14] = 50.;
            let mut previous = prop.world_matrix;
            previous[12] -= speed / 60.;
            let motion = crate::train::MotionState {
                current: prop.world_matrix,
                previous,
                reciprocal_dt: 60.,
            };
            let mut scene = collision_scene(vec![], None);
            scene
                .install_dynamic(vec![crate::scene_collision::DynamicObstacle {
                    obstacle: prop,
                    motion,
                }])
                .unwrap();
            //The first contact detaches with the quad's velocity; the
            //following detached Ground frames then slide on the moving body.
            let mut touched = 0;
            for _ in 0..10 {
                animator
                    .step_with_scene(
                        1. / 60.,
                        vehicle,
                        &nodes,
                        &terrain,
                        &materials,
                        None,
                        Some(&mut scene),
                        false,
                    )
                    .unwrap();
                let frame = animator.frame().unwrap();
                touched += usize::from(
                    frame.static_contact_count > 0
                        && frame.scene_query.dynamic_contact_callbacks > 0,
                );
            }
            assert!(touched > 1);
            let frame = animator.frame().unwrap();
            assert!(frame.scene_query.cached);
            assert_ne!(frame.phase, Phase::Attached);
            assert_eq!(frame.scene_query.selected_dynamic, 1);
            assert_eq!(frame.scene_query.selected_static, 0);
            assert!(animator.integration_active);
            // The procedural pose is not pushed by the rider (zero-mass target).
            assert_eq!(scene.dynamic()[0].motion, motion);
            observed.push(frame);
        }
        let (still, moving) = (&observed[0], &observed[1]);
        assert!(
            moving.linear_velocity[0] > still.linear_velocity[0] + 0.1
                && moving.origin[0] > still.origin[0],
            "moving support must drag the rider: {:?} vs {:?}",
            moving.linear_velocity,
            still.linear_velocity
        );
        assert!(moving.linear_velocity.iter().all(|v| v.is_finite()));
    }
}
