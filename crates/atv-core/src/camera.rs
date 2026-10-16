//! Follow-camera presets and motion recovered from the exact USA v2.00 build.
//! Loader003029a0 preserves constructor defaults unless the preset count changes.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Preset {
    pub id: i32,
    pub name: String,
    pub angle_y: f32,
    pub angle_x: f32,
    pub distance: f32,
    pub fov: f32,
    pub reverse_angle_x: f32,
    pub reverse_distance: f32,
    pub reverse_look_ahead: f32,
    pub direction_switch_speed: f32,
    pub cycle: bool,
    pub filtered: bool,
    pub look_ahead: f32,
    pub look_back: bool,
    pub camera_swing: f32,
    pub look_swing: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_fov: f32,
    pub max_fov: f32,
    pub look_filters: [f32; 3],
    pub position_filters: [f32; 3],
}
impl Preset {
    ///001f2d20 initializes all seven slots before applying disc overrides.
    fn constructor(index: usize) -> Self {
        Self {
            id: index as i32,
            angle_y: f32::from_bits(0x40490fd9),
            angle_x: f32::from_bits(0x3f8192bf),
            distance: 13.5,
            fov: 85.,
            reverse_distance: 10.,
            reverse_look_ahead: 10.,
            direction_switch_speed: 0.35,
            cycle: true,
            filtered: true,
            look_ahead: 15.,
            camera_swing: 1.,
            look_swing: 1.,
            min_distance: 10.,
            max_distance: 100.,
            min_fov: 40.,
            max_fov: 110.,
            look_filters: [0.3; 3],
            position_filters: [0.225; 3],
            ..Self::default()
        }
    }
}

/// Viewport suffixes are selected by the original loader's layout enum.
///0=single,1=horizontal split,2=vertical split,3=quarter screen.
pub fn presets(bytes: &[u8], viewport: u32) -> Result<Vec<Preset>> {
    let data = PhysicsData::parse(bytes)?;
    let count = data.first_scalar("NumberOfPresets", "NumberOfPresets")?;
    if count.fract() != 0. || !(1.0..=64.0).contains(&count) {
        return Err("invalid camera preset count".into());
    }
    let count = count as usize;
    let suffix = match viewport {
        0 => "",
        1 => "h",
        2 => "v",
        3 => "q",
        _ => return Err("invalid camera viewport layout".into()),
    };
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let section = format!("Preset{}{suffix}", index + 1);
        let mut preset = if count == 7 {
            Preset::constructor(index)
        } else {
            Preset::default()
        };
        let value = |key: &str| {
            data.entries
                .iter()
                .find(|e| {
                    e.section.eq_ignore_ascii_case(&section) && e.key.eq_ignore_ascii_case(key)
                })
                .map(|e| e.value.as_str())
        };
        let scalar = |key: &str, default: f32| -> Result<f32> {
            let result = value(key)
                .map(str::parse::<f32>)
                .transpose()
                .map_err(|_| format!("invalid camera {key}"))?
                .unwrap_or(default);
            if !result.is_finite() {
                return Err(format!("nonfinite camera {key}"));
            }
            Ok(result)
        };
        let integer = |key: &str, default: i32| -> Result<i32> {
            value(key)
                .map(str::parse::<i32>)
                .transpose()
                .map_err(|_| format!("invalid camera {key}"))
                .map(|v| v.unwrap_or(default))
        };
        let vector = |key: &str, default: [f32; 3]| -> Result<[f32; 3]> {
            let Some(text) = value(key) else {
                return Ok(default);
            };
            let values = text
                .split(',')
                .map(|s| {
                    s.trim()
                        .parse::<f32>()
                        .map_err(|_| format!("invalid camera {key}"))
                })
                .collect::<Result<Vec<_>>>()?;
            let v: [f32; 3] = values
                .try_into()
                .map_err(|_| format!("camera {key} needs three components"))?;
            if !v.iter().all(|x| x.is_finite()) {
                return Err(format!("nonfinite camera {key}"));
            }
            Ok(v)
        };
        preset.id = integer("ID", preset.id)?;
        if let Some(name) = value("Name") {
            preset.name = name.trim_matches('"').to_owned();
        }
        // The original converts only these two fields on a successful override.
        let radians = f32::from_bits(0x3c8efa34);
        if value("AngleY").is_some() {
            preset.angle_y = scalar("AngleY", 0.)? * radians;
        }
        if value("AngleX").is_some() {
            preset.angle_x = scalar("AngleX", 0.)? * radians;
        }
        macro_rules! field {
            ($key:literal, $field:ident) => {
                preset.$field = scalar($key, preset.$field)?;
            };
        }
        field!("FollowingDistance", distance);
        field!("FOV", fov);
        field!("ReverseAngleXOffset", reverse_angle_x);
        field!("ReverseFollowingOffset", reverse_distance);
        field!("ReverseLookAheadOffset", reverse_look_ahead);
        field!("ForwardReverseSwitchSpeed", direction_switch_speed);
        field!("LookAheadDistance", look_ahead);
        field!("CameraSwing", camera_swing);
        field!("LookSwing", look_swing);
        field!("MinFollowingDistance", min_distance);
        field!("MaxFollowingDistance", max_distance);
        field!("MinFOV", min_fov);
        field!("MaxFOV", max_fov);
        preset.cycle = integer("CycleThruThisPreset", preset.cycle as i32)? != 0;
        preset.filtered = integer("FilterThisPreset", preset.filtered as i32)? != 0;
        preset.look_back = integer("IsLookBackCam", preset.look_back as i32)? != 0;
        preset.look_filters = vector("LookFilters", preset.look_filters)?;
        preset.position_filters = vector("PosFilters", preset.position_filters)?;
        result.push(preset);
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CameraFrame {
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub fov: f32,
    pub preset: usize,
}

/// Explicit actor inputs avoid coupling camera recovery to browser rendering.
#[derive(Debug, Clone, Copy)]
pub struct FollowInput {
    pub origin: [f32; 3],
    pub forward: [f32; 3],
    pub velocity: [f32; 3],
    pub speed: f32,
    pub local_forward_speed: f32,
    pub yaw_rate: f32,
    pub wheel_contact: bool,
    pub orbit: f32,
}

pub struct FollowCamera {
    pub presets: Vec<Preset>,
    pub selected: usize,
    pub speed_reference: f32,
    frame: Option<CameraFrame>,
    terrain_lift: f32,
    wreck_seed: Option<(CameraFrame, [f32; 2])>,
}

/// The original first-order filter saturates its step at the configured time.
pub fn filter_scalar(previous: f32, target: f32, dt: f32, time: f32) -> f32 {
    previous + (dt.min(time) / time) * (target - previous)
}

fn wrapped(angle: f32) -> f32 {
    let angle = angle % std::f32::consts::TAU;
    if angle > std::f32::consts::PI {
        angle - std::f32::consts::TAU
    } else if angle < -std::f32::consts::PI {
        angle + std::f32::consts::TAU
    } else {
        angle
    }
}

///0012b550 projects onto the axis before rotating the orthogonal component.
///Host sin/cos remain provisional until the original lookup is reproduced.
fn rotate(axis: [f32; 3], value: [f32; 3], angle: f32) -> [f32; 3] {
    let projection = (axis[0] * value[0] + axis[1] * value[1]) + axis[2] * value[2];
    let perpendicular = std::array::from_fn::<_, 3, _>(|i| value[i] - projection * axis[i]);
    let (sin, cos) = angle.sin_cos();
    std::array::from_fn(|i| {
        let j = (i + 1) % 3;
        let k = (i + 2) % 3;
        (projection * axis[i] + sin * (perpendicular[k] * axis[j] - perpendicular[j] * axis[k]))
            + cos * perpendicular[i]
    })
}

/// The two actors bound by001f33f0: quad first, detached rider second.
#[derive(Debug, Clone, Copy)]
pub struct WreckActor {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
}

/// Last direct obstruction query. A latched camera keeps this evidence while
/// setting queried_this_step=false on subsequent updates.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CameraSceneQuery {
    pub direct: bool,
    pub queried_this_step: bool,
    pub center: [f32; 3],
    pub radius: f32,
    pub static_candidates: usize,
}

/// Single-player wreck camera,0031cc90 and its arithmetic helpers.
pub struct WreckCamera {
    frame: CameraFrame,
    output_position: [f32; 3],
    motion_blocked: bool,
    obstacles: Vec<crate::obstacle::StaticObstacle>,
    scene_query: Option<CameraSceneQuery>,
    angles: [f32; 2],
    direction: [f32; 3],
    last_velocity: [f32; 3],
    raw_target: [f32; 3],
    base_fov: f32,
    distance: f32,
    distance_offset: f32,
    fov_offset: f32,
    transition: f32,
    separation: f32,
    included: usize,
}

fn wreck_length(v: [f32; 3]) -> f32 {
    ((v[2] * v[2] + v[0] * v[0]) + v[1] * v[1]).sqrt()
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let squared = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if squared == 0. {
        [0.; 3]
    } else {
        let reciprocal = 1. / squared.sqrt();
        v.map(|n| n * reciprocal)
    }
}

/// 0031d190 checks ten feet ahead from the previous rendered camera position,
/// along the desired movement direction, even when this frame moves less.
pub fn wreck_camera_motion_ray(
    previous: [f32; 3],
    candidate: [f32; 3],
) -> Result<crate::collision::CollisionRay> {
    if previous
        .iter()
        .chain(candidate.iter())
        .any(|v| !v.is_finite())
    {
        return Err("invalid wreck camera motion segment".into());
    }
    let delta: [f32; 3] = std::array::from_fn(|i| candidate[i] - previous[i]);
    let squared = (delta[0] * delta[0] + delta[1] * delta[1]) + delta[2] * delta[2];
    // VRSQRT produces Q directly; an intermediate rounded binary32 SQRT then
    // DIV differs by one bit for some finite inputs in the instruction oracle.
    let reciprocal = if squared == 0. {
        0.
    } else {
        (1. / (squared as f64).sqrt()) as f32
    };
    let direction = delta.map(|value| value * reciprocal);
    Ok(crate::collision::CollisionRay {
        start: previous,
        end: std::array::from_fn(|i| previous[i] + 10. * direction[i]),
    })
}

impl WreckCamera {
    ///0031d2f0/0031d490 seed the filters from the outgoing camera, then0031d640
    /// measures its offset from the mean actor origin. Angles are azimuth/elevation.
    pub fn new(previous: CameraFrame, angles: [f32; 2], actors: [WreckActor; 2]) -> Result<Self> {
        if !previous
            .position
            .iter()
            .chain(previous.target.iter())
            .chain([previous.fov].iter())
            .chain(angles.iter())
            .chain(
                actors
                    .iter()
                    .flat_map(|a| a.origin.iter().chain(a.velocity.iter())),
            )
            .all(|v| v.is_finite())
        {
            return Err("invalid wreck-camera initialization".into());
        }
        let offset = std::array::from_fn(|i| {
            previous.position[i] - (actors[0].origin[i] + actors[1].origin[i]) * 0.5
        });
        Ok(Self {
            frame: previous,
            output_position: previous.position,
            motion_blocked: false,
            obstacles: Vec::new(),
            scene_query: None,
            angles: [angles[0].abs(), angles[1]],
            direction: normalized(offset),
            last_velocity: [0.; 3],
            raw_target: previous.target,
            base_fov: previous.fov,
            distance: wreck_length(offset),
            distance_offset: 0.,
            fov_offset: 0.,
            transition: 2.,
            separation: wreck_length(std::array::from_fn(|i| {
                actors[1].origin[i] - actors[0].origin[i]
            })),
            included: 2,
        })
    }

    pub fn set_static_obstacles(&mut self, obstacles: Vec<crate::obstacle::StaticObstacle>) {
        self.obstacles = obstacles;
    }

    pub fn motion_blocked(&self) -> bool {
        self.motion_blocked
    }

    pub fn scene_query(&self) -> Option<CameraSceneQuery> {
        self.scene_query
    }

    pub fn step(
        &mut self,
        actors: [WreckActor; 2],
        fallback: [f32; 3],
        dt: f32,
        height: impl FnMut([f32; 3]) -> Result<f32>,
    ) -> Result<CameraFrame> {
        self.step_with_scene(actors, fallback, dt, height, None)
    }

    ///The live wreck camera uses its independent scene partition query. The
    ///legacy static list supports isolated camera fixtures without a scene.
    pub fn step_with_scene(
        &mut self,
        actors: [WreckActor; 2],
        fallback: [f32; 3],
        dt: f32,
        mut height: impl FnMut([f32; 3]) -> Result<f32>,
        scene: Option<&crate::scene_collision::SceneCollision>,
    ) -> Result<CameraFrame> {
        if !dt.is_finite()
            || dt <= 0.
            || dt > 0.25
            || !actors
                .iter()
                .flat_map(|a| a.origin.iter().chain(a.velocity.iter()))
                .chain(fallback.iter())
                .all(|v| v.is_finite())
        {
            return Err("invalid wreck-camera input".into());
        }
        if self.transition > 0. {
            self.transition -= dt;
        }
        //0031deb0 uses the previous frame's included count and pair distance.
        let transition = (self.transition / 2.).max(0.);
        if self.transition < 0. {
            self.transition = 0.;
        }
        let old_span = if self.included > 1 {
            self.separation
        } else {
            0.
        };
        self.separation = wreck_length(std::array::from_fn(|i| {
            actors[1].origin[i] - actors[0].origin[i]
        }));
        let span = if self.included > 1 {
            self.separation
        } else {
            45.
        };
        let change = if self.included > 1 {
            span - old_span
        } else {
            0.
        };
        let zoom = transition * ((75. - (self.base_fov + self.fov_offset)) * 0.2)
            + (1. - transition) * (change / 10.);
        if zoom.abs() >= 0.1 || self.fov_offset <= 0. {
            self.fov_offset += zoom;
        } else {
            self.fov_offset = (25.
                + (85. - 25.) * ((90. - (30. + self.distance_offset)) / (90. - 30.)))
                - self.base_fov;
        }
        let mut distance_change =
            transition * ((35. - self.distance) * 0.03) + (1. - transition) * (change / 10.);
        if change <= 1. {
            let angle =
                (self.base_fov + self.fov_offset - 20.).max(25.) * 0.5 * f32::from_bits(0x3c8efa34);
            let correction = span / (angle.tan() * 2.) - (30. + self.distance_offset);
            if correction > 0. || -correction < self.distance_offset {
                distance_change += correction * 0.05;
            }
        }
        self.distance_offset += distance_change;
        let mut fov = self.base_fov + self.fov_offset;
        if fov > 85. {
            self.fov_offset = 60.;
            fov = 85.;
        } else if fov < 25. {
            self.fov_offset = 0.;
            fov = 25.;
        }

        //0031d7f0 reduces the second actor's position contribution with distance.
        // Velocity is averaged separately, and slow actors contribute no Y speed.
        let include_rider = (1. - self.separation / 80.).max(0.) > 0.;
        self.included = if include_rider { 2 } else { 1 };
        let mut center = actors[0].origin;
        if include_rider {
            let weight = (1. - 1. / 3.) * (1. - self.separation / 80.).max(0.);
            center = std::array::from_fn(|i| {
                (center[i]
                    + (actors[0].origin[i] + weight * (actors[1].origin[i] - actors[0].origin[i])))
                    * 0.5
            });
        }
        let mut velocity = [0.; 3];
        for actor in actors.iter().take(self.included) {
            let mut v = actor.velocity;
            if wreck_length(v) < 20. {
                v[1] = 0.;
            }
            for i in 0..3 {
                velocity[i] += v[i];
            }
        }
        velocity = velocity.map(|v| v / self.included as f32);
        // The original tests each axis, not vector magnitude (0031cd60).
        let moving = velocity.iter().all(|v| *v != 0.);
        if moving {
            self.last_velocity = velocity;
        } else {
            if self.last_velocity == [0.; 3] {
                self.last_velocity = fallback;
            }
            velocity = self.last_velocity;
        }
        self.angles[0] += (f32::from_bits(0x3fc90fd9) - self.angles[0]) * 0.05;
        self.angles[1] += (f32::from_bits(0x3eb2b8c1) - self.angles[1]) * 0.05;
        let heading = normalized([velocity[0], 0., velocity[2]]);
        let direction = rotate(
            [0., 1., 0.],
            rotate(
                [-heading[2], 0., heading[0]],
                heading,
                wrapped(self.angles[1]),
            ),
            wrapped(self.angles[0]),
        );
        self.direction =
            std::array::from_fn(|i| direction[i] + (direction[i] - self.direction[i]) * 0.1);
        let lead = if !moving {
            0.
        } else if self.included < 2 {
            0.76
        } else {
            1.
        };
        self.raw_target = std::array::from_fn(|i| {
            (center[i] + velocity[i] * lead) + transition * ((self.raw_target[i] - center[i]) * 0.2)
        });
        self.distance = (30. + self.distance_offset).clamp(30., 90.);
        self.distance_offset = self.distance - 30.;
        let direction = normalized(self.direction);
        let position: [f32; 3] = std::array::from_fn(|i| center[i] + self.distance * direction[i]);
        for (i, target) in position.into_iter().enumerate() {
            self.frame.position[i] = filter_scalar(self.frame.position[i], target, dt, 1.2);
            self.frame.target[i] = filter_scalar(self.frame.target[i], self.raw_target[i], dt, 1.2);
        }
        self.frame.fov = filter_scalar(self.frame.fov, fov, dt, 0.4);
        //0031eeb0 clamps the stored position, unlike the normal follow camera.
        self.frame.position[1] = self.frame.position[1].max(height(self.frame.position)? + 5.);
        if !self
            .frame
            .position
            .iter()
            .chain(self.frame.target.iter())
            .chain([self.frame.fov].iter())
            .all(|v| v.is_finite())
        {
            return Err("nonfinite wreck-camera output".into());
        }
        // 0031cc90 latches the collision flag for the rest of this wreck-camera
        // activation. Filters/target/FOV continue; only the rendered position
        // retains its previous value. Recovery constructs a fresh follow view.
        if let Some(query) = &mut self.scene_query {
            query.queried_this_step = false;
        }
        if !self.motion_blocked {
            let ray = wreck_camera_motion_ray(self.output_position, self.frame.position)?;
            self.motion_blocked = if let Some(scene) = scene {
                //Loaded ordinary placements currently supply default enabled
                //eligibility. Custom native category/list masks lack transport.
                let query = scene.direct_camera_candidates(&ray, |_, _| true)?;
                self.scene_query = Some(CameraSceneQuery {
                    direct: true,
                    queried_this_step: true,
                    center: query.bounds.center,
                    radius: query.radius,
                    static_candidates: query.obstacles.len(),
                });
                let mut blocked = false;
                for obstacle in query.obstacles {
                    if !crate::wreck::query_static_segments(
                        std::slice::from_ref(&ray),
                        std::slice::from_ref(obstacle),
                        1,
                    )?
                    .is_empty()
                    {
                        blocked = true;
                        break;
                    }
                }
                blocked
            } else {
                self.scene_query = None;
                !crate::wreck::query_static_segments(&[ray], &self.obstacles, 1)?.is_empty()
            };
            if !self.motion_blocked {
                self.output_position = self.frame.position;
            }
        }
        let mut output = self.frame;
        output.position = self.output_position;
        Ok(output)
    }
}

impl FollowCamera {
    pub fn new(presets: Vec<Preset>, speed_reference: f32) -> Result<Self> {
        if presets.is_empty() || !speed_reference.is_finite() || speed_reference <= 0. {
            return Err("invalid follow-camera setup".into());
        }
        for p in &presets {
            if p.max_fov <= 0.
                || p.min_fov > p.max_fov
                || p.min_distance > p.max_distance
                || p.look_filters
                    .iter()
                    .chain(p.position_filters.iter())
                    .any(|t| !t.is_finite() || *t <= 0.)
            {
                return Err("unsupported camera preset limits/filter times".into());
            }
        }
        Ok(Self {
            presets,
            selected: 0,
            speed_reference,
            frame: None,
            terrain_lift: 0.,
            wreck_seed: None,
        })
    }
    pub fn reset(&mut self) {
        self.frame = None;
        self.terrain_lift = 0.;
        self.wreck_seed = None;
    }
    /// After-finish director cut (`001f3bb0` SetCamera): select a preset and
    /// either keep the filtered pose so it eases in, or snap (`+0xdc`).
    pub fn direct(&mut self, preset: usize, ease: bool) {
        if preset < self.presets.len() {
            self.selected = preset;
        }
        if !ease {
            self.reset();
        }
    }
    pub fn wreck_camera(&self, actors: [WreckActor; 2]) -> Result<WreckCamera> {
        let (frame, angles) = self
            .wreck_seed
            .ok_or("follow camera must precede wreck camera")?;
        WreckCamera::new(frame, angles, actors)
    }
    pub fn cycle(&mut self) {
        let count = self.presets.len();
        for offset in 1..=count {
            let index = (self.selected + offset) % count;
            if self.presets[index].cycle {
                if !self.presets[self.selected].filtered && self.presets[index].filtered {
                    self.frame = None;
                }
                self.selected = index;
                break;
            }
        }
    }
    pub fn step(
        &mut self,
        input: FollowInput,
        dt: f32,
        mut height: impl FnMut([f32; 3]) -> Result<f32>,
    ) -> Result<CameraFrame> {
        if !dt.is_finite()
            || dt <= 0.
            || dt > 0.25
            || !input
                .origin
                .iter()
                .chain(input.forward.iter())
                .chain(input.velocity.iter())
                .chain(
                    [
                        input.speed,
                        input.local_forward_speed,
                        input.yaw_rate,
                        input.orbit,
                    ]
                    .iter(),
                )
                .all(|v| v.is_finite())
            || input.speed < 0.
        {
            return Err("invalid follow-camera input".into());
        }
        let p = self
            .presets
            .get(self.selected)
            .ok_or("invalid camera preset")?;
        let fov = p.fov.clamp(p.min_fov, p.max_fov);
        let ratio = fov / p.max_fov;
        let squared = input.forward[0] * input.forward[0] + input.forward[2] * input.forward[2];
        if squared <= 0. {
            return Err("vertical vehicle heading has no follow-camera direction".into());
        }
        let reciprocal = 1. / squared.sqrt();
        let forward = [
            input.forward[0] * reciprocal,
            0.,
            input.forward[2] * reciprocal,
        ];
        let reverse = input.local_forward_speed < 0.
            && (input.wheel_contact || input.origin[1] < height(input.origin)? + 2.);
        let turn = (-input.yaw_rate * 0.21) * ratio;
        let azimuth = wrapped((p.angle_y - input.orbit) - p.camera_swing * turn);
        let mut elevation = p
            .angle_x
            .clamp(-f32::from_bits(0x3fbde464), f32::from_bits(0x3fbde464));
        let angle_speed = if reverse { 0. } else { input.speed };
        if angle_speed < 30. {
            elevation *= 0.6 + f32::from_bits(0x3ecccccc) * (angle_speed / 30.);
        }
        let direction = rotate(
            [0., 1., 0.],
            rotate([-forward[2], 0., forward[0]], forward, elevation),
            azimuth,
        );
        let distance = p.distance.clamp(p.min_distance, p.max_distance);
        let origin =
            std::array::from_fn::<_, 3, _>(|i| input.origin[i] + dt.min(0.2) * input.velocity[i]);
        let actual_distance = if reverse && !p.look_back {
            distance + distance * (input.local_forward_speed / -33.)
        } else {
            distance
        };
        let mut position =
            std::array::from_fn::<_, 3, _>(|i| origin[i] + actual_distance * direction[i]);
        let stretch = (distance * ratio) / 90. + 1.;
        // Assembly evaluates this in a different order than the decompiler.
        let mut lead = (input.speed / self.speed_reference).min(1.)
            * (stretch * ((1.8 * (1. - distance / 600.)) * stretch));
        if reverse && self.selected != 3 {
            lead *= -1.;
        }
        if input.velocity[1] < -9. {
            lead *= (input.velocity[1] / input.speed) / 1.8 + 1.;
        }
        lead = if p.look_back { 0. } else { lead * p.look_ahead };
        let ahead = rotate([0., 1., 0.], forward, wrapped((turn * p.look_swing) * -0.7));
        let mut target = std::array::from_fn::<_, 3, _>(|i| origin[i] + lead * ahead[i]);
        target[1] += ((1. - ratio) * input.velocity[1]) * 0.25;
        let probe_distance = (elevation.abs() / std::f32::consts::FRAC_PI_2) * 1.2 + 0.3;
        let probe = std::array::from_fn::<_, 3, _>(|i| origin[i] + probe_distance * ahead[i]);
        let floor = height(probe)?;
        self.terrain_lift = if probe[1] < floor {
            (floor - probe[1]) * 0.6
        } else {
            self.terrain_lift * 0.02
        };
        target[1] += 3. + self.terrain_lift;
        //00319c30 clamps raw and filtered camera points to terrain plus2ft.
        position[1] = position[1].max(height(position)? + 2.);
        let mut frame = CameraFrame {
            position,
            target,
            fov,
            preset: self.selected,
        };
        if p.filtered
            && let Some(previous) = self.frame
        {
            for i in 0..3 {
                frame.position[i] =
                    filter_scalar(previous.position[i], position[i], dt, p.position_filters[i]);
                frame.target[i] =
                    filter_scalar(previous.target[i], target[i], dt, p.look_filters[i]);
            }
            frame.fov = filter_scalar(previous.fov, fov, dt, 0.55);
        }
        if !frame
            .position
            .iter()
            .chain(frame.target.iter())
            .chain([frame.fov].iter())
            .all(|v| v.is_finite())
        {
            return Err("nonfinite follow-camera output".into());
        }
        // The post-filter terrain clamp changes the output, not filter history.
        self.frame = Some(frame);
        frame.position[1] = frame.position[1].max(height(frame.position)? + 2.);
        self.wreck_seed = Some((frame, [azimuth, elevation]));
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wreck_transition_matches_original_instruction_checkpoint() {
        let previous = CameraFrame {
            position: [0., 12., -20.],
            target: [0., 5., 0.],
            fov: 85.,
            preset: 0,
        };
        let mut actors = [
            WreckActor {
                origin: [0., 4., 0.],
                velocity: [0., 0., 20.],
            },
            WreckActor {
                origin: [1., 6., 0.],
                velocity: [1., -2., 20.],
            },
        ];
        let mut camera =
            WreckCamera::new(previous, [f32::from_bits(0x4049_0fd8), 0.7], actors).unwrap();
        actors[0].velocity = [3., 4., 18.];
        actors[1].origin = [0., 6., 0.];
        actors[1].velocity = [15., 24., 16.8];
        let frame = camera
            .step(actors, [0., 0., 1.], 1. / 60., |_| Ok(0.))
            .unwrap();
        // Synthetic actor inputs executed by0031cc90; no game assets in fixture.
        let expected = [
            -0.137_360_72,
            12.173_682,
            -20.011_15,
            0.125,
            5.162_77,
            0.241_666_66,
            84.917_35,
        ];
        for (actual, expected) in frame
            .position
            .into_iter()
            .chain(frame.target)
            .chain([frame.fov])
            .zip(expected)
        {
            assert!(
                (actual - expected).abs() < 0.000_03,
                "{actual} != {expected}"
            );
        }
    }
    #[test]
    fn wreck_obstruction_latches_position_while_filters_and_target_continue() {
        let actors = [
            WreckActor {
                origin: [0., 4., 0.],
                velocity: [3., 4., 18.],
            },
            WreckActor {
                origin: [0., 6., 0.],
                velocity: [15., 24., 16.8],
            },
        ];
        let previous = CameraFrame {
            position: [0., 12., -20.],
            target: [0., 5., 0.],
            fov: 85.,
            preset: 0,
        };
        let mut open = WreckCamera::new(previous, [std::f32::consts::PI, 0.7], actors).unwrap();
        let mut blocked = WreckCamera::new(previous, [std::f32::consts::PI, 0.7], actors).unwrap();
        // The desired first-frame movement is shorter than one foot. Native
        // nevertheless casts ten feet ahead and detects this overhead plane.
        blocked.set_static_obstacles(vec![crate::wreck::tests::surface([
            [-100., 13., -100.],
            [100., 13., -100.],
            [0., 13., 100.],
        ])]);
        for i in 0..40 {
            let moving = open
                .step(actors, [0., 0., 1.], 1. / 60., |_| Ok(0.))
                .unwrap();
            let fixed = blocked
                .step(actors, [0., 0., 1.], 1. / 60., |_| Ok(0.))
                .unwrap();
            assert!(blocked.motion_blocked());
            assert_eq!(fixed.position, previous.position);
            assert_eq!(fixed.target, moving.target);
            assert_eq!(fixed.fov, moving.fov);
            assert_eq!(blocked.frame.position, open.frame.position);
            if i == 0 {
                assert!(moving.position[1] < 13.);
                blocked.set_static_obstacles(Vec::new());
            }
        }
        let stationary = wreck_camera_motion_ray(previous.position, previous.position).unwrap();
        assert_eq!(stationary.start, stationary.end);
        assert!(wreck_camera_motion_ray([f32::NAN, 0., 0.], [0.; 3]).is_err());
    }

    #[test]
    fn live_wreck_query_uses_scene_candidates_and_retains_latch_diagnostics() {
        let actors = [
            WreckActor {
                origin: [128., 4., 128.],
                velocity: [3., 4., 18.],
            },
            WreckActor {
                origin: [128., 6., 128.],
                velocity: [15., 24., 16.8],
            },
        ];
        let previous = CameraFrame {
            position: [128., 12., 108.],
            target: [128., 5., 128.],
            fov: 85.,
            preset: 0,
        };
        let config = crate::scene_partition::Config {
            min_x: 0.,
            max_x: 256.,
            min_z: 0.,
            max_z: 256.,
            cell_size: 32.,
        };
        let plane =
            crate::wreck::tests::surface([[28., 13., 28.], [228., 13., 28.], [128., 13., 228.]]);
        let empty =
            crate::scene_collision::SceneCollision::new(config, vec![], None, false, |_| Ok(0.))
                .unwrap();
        let scene = crate::scene_collision::SceneCollision::new(
            config,
            vec![plane.clone()],
            None,
            false,
            |_| Ok(0.),
        )
        .unwrap();
        let mut open = WreckCamera::new(previous, [std::f32::consts::PI, 0.7], actors).unwrap();
        let mut blocked = WreckCamera::new(previous, [std::f32::consts::PI, 0.7], actors).unwrap();
        // An explicit scene is authoritative over the old all-static fixture list.
        open.set_static_obstacles(vec![plane]);
        let moving = open
            .step_with_scene(actors, [0., 0., 1.], 1. / 60., |_| Ok(0.), Some(&empty))
            .unwrap();
        assert!(!open.motion_blocked());
        assert_eq!(open.scene_query().unwrap().static_candidates, 0);
        assert_ne!(moving.position, previous.position);
        let fixed = blocked
            .step_with_scene(actors, [0., 0., 1.], 1. / 60., |_| Ok(0.), Some(&scene))
            .unwrap();
        assert!(blocked.motion_blocked());
        assert_eq!(fixed.position, previous.position);
        let first_query = blocked.scene_query().unwrap();
        assert!(first_query.direct && first_query.queried_this_step);
        assert_eq!(first_query.static_candidates, 1);
        let next = blocked
            .step_with_scene(actors, [0., 0., 1.], 1. / 60., |_| Ok(0.), Some(&empty))
            .unwrap();
        assert_eq!(next.position, previous.position);
        let retained = blocked.scene_query().unwrap();
        assert!(retained.direct && !retained.queried_this_step);
        assert_eq!(retained.static_candidates, first_query.static_candidates);
        assert_eq!(retained.center, first_query.center);
    }

    #[test]
    fn stationary_wreck_retains_heading_and_clamps_stored_camera_to_ground() {
        let actors = [WreckActor {
            origin: [0., 1., 0.],
            velocity: [0.; 3],
        }; 2];
        let previous = CameraFrame {
            position: [0., 2., -10.],
            target: [0., 1., 0.],
            fov: 85.,
            preset: 0,
        };
        let mut camera = WreckCamera::new(previous, [std::f32::consts::PI, 0.7], actors).unwrap();
        for _ in 0..180 {
            let frame = camera
                .step(actors, [0., 0., 1.], 1. / 60., |_| Ok(3.))
                .unwrap();
            assert!(frame.position[1] >= 8.);
            assert!(
                frame
                    .position
                    .into_iter()
                    .chain(frame.target)
                    .all(f32::is_finite)
            );
        }
        assert_eq!(camera.last_velocity, [0., 0., 1.]);
        assert!(camera.step(actors, [0.; 3], 0., |_| Ok(0.)).is_err());
    }
    #[test]
    fn changed_count_clears_defaults_and_layout_selects_section() {
        let text = b"[NumberOfPresets]\nNumberOfPresets=1\n[Preset1]\nFollowingDistance=9\n[Preset1h]\nFollowingDistance=12\n";
        let single = presets(text, 0).unwrap();
        assert_eq!(single[0].distance, 9.);
        assert_eq!(single[0].fov, 0.);
        assert_eq!(presets(text, 1).unwrap()[0].distance, 12.);
    }
    #[test]
    fn seven_presets_keep_defaults_and_convert_only_authored_angles() {
        let text =
            b"[NumberOfPresets]\nNumberOfPresets=7\n[Preset1]\nAngleX=45\nReverseAngleXOffset=2\n";
        let p = presets(text, 0).unwrap();
        assert_eq!(p[0].angle_x, 45. * f32::from_bits(0x3c8efa34));
        assert_eq!(p[0].reverse_angle_x, 2.);
        assert_eq!(p[1].angle_x.to_bits(), 0x3f8192bf);
        assert_eq!(p[1].fov, 85.);
        assert!(presets(b"[NumberOfPresets]\nNumberOfPresets=NaN", 0).is_err());
    }
    #[test]
    fn terrain_clearance_does_not_contaminate_filter_history() {
        let mut preset = Preset::constructor(0);
        preset.angle_x = 0.;
        preset.position_filters = [1.; 3];
        let mut camera = FollowCamera::new(vec![preset], 100.).unwrap();
        let input = FollowInput {
            origin: [0.; 3],
            forward: [0., 0., 1.],
            velocity: [0.; 3],
            speed: 0.,
            local_forward_speed: 0.,
            yaw_rate: 0.,
            wheel_contact: true,
            orbit: 0.,
        };
        assert_eq!(camera.step(input, 0.1, |_| Ok(0.)).unwrap().position[1], 2.);
        assert_eq!(camera.step(input, 0.1, |_| Ok(3.)).unwrap().position[1], 5.);
        let lowered = camera.step(input, 0.1, |_| Ok(0.)).unwrap();
        assert!((lowered.position[1] - 2.27).abs() < 0.00001);
        camera.reset();
        assert_eq!(camera.step(input, 0.1, |_| Ok(0.)).unwrap().position[1], 2.);
    }
}
