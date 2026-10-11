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
        })
    }
    pub fn reset(&mut self) {
        self.frame = None;
        self.terrain_lift = 0.;
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
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
