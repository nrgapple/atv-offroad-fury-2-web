//! USA v2.00 procedural VUE route followers and original train resource binding.
use crate::{
    Result,
    physics_data::PhysicsData,
    resource::Resource,
    wheel::{Vec3, cross, dot, scale, sub},
};
use serde::Serialize;
#[path = "train_motion.rs"]
mod motion;
#[path = "train_wheels.rs"]
mod wheels;
pub use motion::MotionState;
#[path = "train_list.rs"]
mod list;
pub use list::DynamicList;
#[path = "train_height.rs"]
mod height;
pub use height::height_geometry;

#[derive(Debug, Clone, Serialize)]
pub struct Definition {
    pub model: String,
    pub route: String,
    pub fps: f32,
    pub lag_distance: i32,
    pub offset: Vec3,
    pub path_is_level: bool,
    pub drive_on_soultrees: bool,
    pub use_collision: bool,
    pub use_lighting: bool,
}
pub(crate) fn integer(text: &str) -> Result<i32> {
    match text.as_bytes().first().map(u8::to_ascii_uppercase) {
        Some(b'T' | b'Y') => return Ok(1),
        Some(b'F' | b'N') => return Ok(0),
        _ => {}
    }
    if text.eq_ignore_ascii_case("ON") {
        return Ok(1);
    }
    if text.eq_ignore_ascii_case("OFF") {
        return Ok(0);
    }
    let text = text.trim_start();
    let (negative, digits) = text
        .strip_prefix('-')
        .map_or((false, text.strip_prefix('+').unwrap_or(text)), |s| {
            (true, s)
        });
    let (radix, digits) = if let Some(s) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, s)
    } else if digits.starts_with('0') {
        (8, digits)
    } else {
        (10, digits)
    };
    let mut value = 0_i64;
    for digit in digits.chars().map_while(|c| c.to_digit(radix)) {
        value = value
            .checked_mul(i64::from(radix))
            .and_then(|v| v.checked_add(i64::from(digit)))
            .ok_or("train integer overflow")?;
    }
    i32::try_from(if negative { -value } else { value })
        .map_err(|_| "train integer outside i32".into())
}
fn vector(text: &str) -> Result<Vec3> {
    let values = text
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<f32>()
                .map_err(|_| "invalid train vector".into())
        })
        .collect::<Result<Vec<_>>>()?;
    let v: Vec3 = values
        .try_into()
        .map_err(|_| "train vector must have three components")?;
    if v.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite train vector".into());
    }
    Ok(v)
}
pub fn definitions(data: &[u8]) -> Result<Vec<Definition>> {
    let document = PhysicsData::parse(data)?;
    let value = |section: &str, key: &str| {
        document
            .entries
            .iter()
            .find(|e| e.section.eq_ignore_ascii_case(section) && e.key.eq_ignore_ascii_case(key))
            .map(|e| e.value.as_str())
    };
    let count = integer(value("Animations", "NumberOfAnimations").unwrap_or("0"))?;
    if !(0..=4096).contains(&count) {
        return Err("train animation count exceeds limit".into());
    }
    let mut definitions = Vec::new();
    for index in 1..=count {
        let section = format!("Animation{index}");
        if !value(&section, "AnimationType").is_some_and(|s| s.eq_ignore_ascii_case("Procedural")) {
            continue;
        }
        let route = value(&section, "VUE").ok_or("procedural route is missing")?;
        //This component binds the verified Arlington route family only. Other
        //procedural toys retain their existing explicit unsupported state.
        if !route
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or(route)
            .eq_ignore_ascii_case("locomotive.vue")
        {
            continue;
        }
        let flag = |key, default| {
            value(&section, key)
                .map(integer)
                .transpose()
                .map(|v| v.map_or(default, |v| v != 0))
        };
        let fps = value(&section, "FPS")
            .unwrap_or("0")
            .parse::<f32>()
            .map_err(|_| "invalid train FPS")?;
        if !fps.is_finite() || fps <= 0. {
            return Err("train FPS must be positive".into());
        }
        if integer(value(&section, "NumberOfTires").unwrap_or("0"))? != 0 {
            return Err("procedural train tires require original attachment binding".into());
        }
        definitions.push(Definition {
            model: value(&section, "SLT")
                .ok_or("procedural train model missing")?
                .to_owned(),
            route: route.to_owned(),
            fps,
            lag_distance: integer(value(&section, "LagDistance").unwrap_or("0"))?,
            offset: vector(value(&section, "Offset").unwrap_or("0,0,0"))?,
            path_is_level: flag("PathIsLevel", true)?,
            drive_on_soultrees: flag("DriveOnSoultrees", true)?,
            use_collision: flag("UseCollision", false)?,
            use_lighting: flag("UseLighting", true)?,
        });
    }
    Ok(definitions)
}
#[derive(Debug, Clone, Serialize)]
pub struct Key {
    pub frame: f32,
    pub position: Vec3,
}
#[derive(Debug, Clone, Serialize)]
pub struct Route {
    pub keys: Vec<Key>,
    pub fps: f32,
    pub frame_seconds: f32,
    pub lengths: Vec<f32>,
}
impl Route {
    pub fn from_vue(data: &[u8], fps: f32) -> Result<Self> {
        if data.len() > 16 * 1024 * 1024 || !fps.is_finite() || fps <= 0. {
            return Err("invalid train route input".into());
        }
        let text = std::str::from_utf8(data).map_err(|_| "VUE text is not UTF8")?;
        let mut keys = Vec::new();
        let mut frame = None;
        for line in text.lines() {
            let words: Vec<_> = line.split_whitespace().collect();
            if words.first() == Some(&"FRAME") {
                frame = Some(
                    words
                        .get(1)
                        .ok_or("missing VUE frame")?
                        .parse::<f32>()
                        .map_err(|_| "invalid VUE frame")?,
                );
            } else if words.first() == Some(&"transform") && words.get(1) == Some(&"\"Body\"") {
                if words.len() != 14 {
                    return Err("VUE Body transform must have twelve floats".into());
                }
                let p = |index: usize| {
                    words[index]
                        .parse::<f32>()
                        .map_err(|_| "invalid VUE coordinate")
                };
                //0011c0b0's scanf writes author XYZ to runtime XZY.
                let position = [p(11)?, p(13)?, p(12)?];
                let frame = frame.ok_or("VUE transform precedes frame")?;
                if !frame.is_finite()
                    || frame < 0.
                    || position.iter().any(|v| !v.is_finite())
                    || keys.last().is_some_and(|k: &Key| k.frame >= frame)
                {
                    return Err("invalid VUE frame order/coordinates".into());
                }
                keys.push(Key { frame, position });
            }
        }
        if keys.len() < 2 || keys.len() > 65536 || keys[0].frame != 0. {
            return Err("train route requires bounded zero-based keys".into());
        }
        let mut route = Self {
            lengths: vec![0.; keys.len()],
            keys,
            fps,
            frame_seconds: 1. / fps,
        };
        for index in 0..route.keys.len() - 1 {
            let start = route.frame_seconds * route.keys[index].frame;
            let width = route.frame_seconds * route.keys[index + 1].frame - start;
            let mut previous = route.sample(start)?;
            let mut length = 0.;
            for substep in 1..=10 {
                let point = route.sample(start + (width * substep as f32) * 0.1)?;
                let d = sub(point, previous);
                let squared = dot(d, d);
                length += if squared == 0. {
                    0.
                } else {
                    1. / ((1. / f64::from(squared).sqrt()) as f32)
                };
                previous = point;
            }
            route.lengths[index] = length;
        }
        Ok(route)
    }
    pub fn duration(&self) -> f32 {
        self.frame_seconds * self.keys.last().unwrap().frame
    }
    fn bracket(&self, frame: f32) -> usize {
        let mut index = 0;
        while index + 1 < self.keys.len() - 1 && self.keys[index + 1].frame < frame {
            index += 1;
        }
        index
    }
    pub fn sample(&self, time: f32) -> Result<Vec3> {
        if !time.is_finite() {
            return Err("nonfinite train sample time".into());
        }
        let mut time = time % self.duration();
        if time < 0. {
            time += self.duration()
        }
        let frame = time * self.fps;
        let i = self.bracket(frame);
        let a = &self.keys[i];
        let b = &self.keys[i + 1];
        let t = (frame - a.frame) / (b.frame - a.frame);
        let t2 = t * t;
        let t3 = t * t2;
        let h1 = (t2 * 3. - t3) - t3;
        let weights = [1. - h1, h1, t + ((t3 - t2) - t2), t3 - t2];
        let delta = sub(b.position, a.position);
        let incoming = if i == 0 {
            delta
        } else {
            scale(
                crate::wheel::add(sub(a.position, self.keys[i - 1].position), delta),
                (b.frame - a.frame) / (b.frame - self.keys[i - 1].frame),
            )
        };
        let outgoing = if i + 1 == self.keys.len() - 1 {
            delta
        } else {
            scale(
                crate::wheel::add(delta, sub(self.keys[i + 2].position, b.position)),
                (b.frame - a.frame) / (self.keys[i + 2].frame - a.frame),
            )
        };
        Ok(std::array::from_fn(|axis| {
            ((weights[0] * a.position[axis] + weights[1] * b.position[axis])
                + weights[2] * incoming[axis])
                + weights[3] * outgoing[axis]
        }))
    }
    pub fn lag_time(&self, time: f32, distance: f32) -> Result<f32> {
        if !time.is_finite() || !distance.is_finite() {
            return Err("nonfinite train lag query".into());
        }
        let frame = time * self.fps;
        let i = self.bracket(frame);
        let mut index = i;
        let mut consumed = if distance > 0. {
            ((frame - self.keys[i].frame) / (self.keys[i + 1].frame - self.keys[i].frame))
                * self.lengths[i]
        } else {
            ((self.keys[i + 1].frame - frame) / (self.keys[i + 1].frame - self.keys[i].frame))
                * self.lengths[i]
        };
        let total = self.lengths.iter().sum::<f32>();
        if total <= 0. {
            return Ok(0.);
        }
        let mut visits = 0;
        while consumed < distance.abs() {
            index = if distance > 0. {
                (index + self.keys.len() - 1) % self.keys.len()
            } else {
                (index + 1) % self.keys.len()
            };
            consumed += self.lengths[index];
            visits += 1;
            if visits > 65536 {
                return Err("train lag traversal exceeded bound".into());
            }
        }
        let length = self.lengths[index];
        if length == 0. {
            return Ok(0.);
        }
        let next = (index + 1) % self.keys.len();
        let a = self.keys[index].frame;
        let b = self.keys[next].frame;
        Ok(if distance > 0. {
            (a + ((consumed - distance) / length) * (b - a)) * self.frame_seconds
        } else {
            (b + (1. - (consumed + distance) / length) * (b - a)) * self.frame_seconds
        })
    }
}
fn unit(v: Vec3) -> Vec3 {
    let squared = dot(v, v);
    if squared == 0. {
        [0.; 3]
    } else {
        scale(v, (1. / f64::from(squared).sqrt()) as f32)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct CarFrame {
    pub model: String,
    pub matrix: [f32; 16],
    pub route_time: f32,
    pub lag_distance: i32,
    pub use_collision: bool,
    pub use_lighting: bool,
}
struct Car {
    definition: Definition,
    previous: Vec3,
    look: Vec3,
    height: f32,
    last_height: f32,
    height_cached: bool,
    matrix: [f32; 16],
    previous_matrix: [f32; 16],
    route_time: f32,
}
#[derive(Serialize)]
pub struct Frame {
    pub time: f32,
    pub duration: f32,
    pub cars: Vec<CarFrame>,
    pub wheel_matrices: Option<Vec<[f32; 16]>>,
}
pub struct Runtime {
    pub route: Route,
    cars: Vec<Car>,
    pub time: f32,
    wheels: Option<(wheels::Motion, Vec<crate::mesh::Node>)>,
    shared_height: f32,
    reciprocal_dt: f32,
}
impl Runtime {
    pub fn new(route: Route, definitions: Vec<Definition>) -> Result<Self> {
        if definitions
            .iter()
            .any(|d| d.fps.to_bits() != route.fps.to_bits())
        {
            return Err("train route followers disagree on FPS".into());
        }
        Ok(Self {
            route,
            time: 0.,
            wheels: None,
            shared_height: 0.,
            reciprocal_dt: 0.,
            cars: definitions
                .into_iter()
                .map(|definition| Car {
                    definition,
                    previous: [0.; 3],
                    look: [0.; 3],
                    height: 0.,
                    last_height: -f32::MAX,
                    height_cached: false,
                    matrix: [
                        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
                    ],
                    previous_matrix: [
                        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
                    ],
                    route_time: 0.,
                })
                .collect(),
        })
    }
    pub fn from_resource(resource: &Resource, index: usize) -> Result<Option<Self>> {
        let definitions = definitions(&resource.extract(index)?)?;
        let Some(first) = definitions.first() else {
            return Ok(None);
        };
        let path = first.route.to_ascii_lowercase();
        let member = resource
            .members
            .iter()
            .position(|m| {
                m.name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&m.name)
                    .eq_ignore_ascii_case(&path)
            })
            .ok_or("original train VUE resource missing")?;
        let mut runtime = Self::new(
            Route::from_vue(&resource.extract(member)?, first.fps)?,
            definitions,
        )?;
        let wheel_member = |filename: &str| {
            resource
                .members
                .iter()
                .position(|m| {
                    m.name
                        .rsplit('/')
                        .next()
                        .unwrap_or(&m.name)
                        .eq_ignore_ascii_case(filename)
                })
                .ok_or_else(|| format!("original train resource missing: {filename}"))
        };
        let motion = wheels::Motion::decode(&resource.extract(wheel_member("loco_wheels.mot")?)?)?;
        let model = crate::mesh::decode_psg(&resource.extract(wheel_member("loco_wheels.psg")?)?)?;
        motion.pose(&model.nodes, 0.)?;
        runtime.wheels = Some((motion, model.nodes));
        Ok(Some(runtime))
    }
    ///0011c840 shared owner clock advances once. The source caller binds no
    ///terrain; supplied heights represent the separate direct SoulTree override.
    pub fn step(
        &mut self,
        dt: f32,
        mut static_height: impl FnMut(Vec3) -> Result<Option<f32>>,
    ) -> Result<Frame> {
        self.step_with_scene(dt, |_, previous| static_height(previous), |_, _| Ok(()))
    }
    ///Native actors query height and publish their model/collision cache in
    ///definition order. Later actors therefore see earlier actors' new poses.
    ///Height services exclude the current body and supply the nearest direct
    ///scene hit from the previous model origin's vertical five-foot segment.
    pub fn step_with_scene(
        &mut self,
        dt: f32,
        mut height_query: impl FnMut(usize, Vec3) -> Result<Option<f32>>,
        mut publish: impl FnMut(usize, MotionState) -> Result<()>,
    ) -> Result<Frame> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err("invalid train update step".into());
        }
        self.reciprocal_dt = 1. / dt;
        self.time = (self.time + dt) % self.route.duration();
        for (index, car) in self.cars.iter_mut().enumerate() {
            car.previous_matrix = car.matrix;
            let height_was_cached = car.height_cached;
            let t = if car.definition.lag_distance > 0 {
                self.route
                    .lag_time(self.time, car.definition.lag_distance as f32)?
            } else {
                self.time
            };
            let position = self.route.sample(t)?;
            let mut displacement = sub(position, car.previous);
            displacement[1] = 0.;
            car.previous = position;
            let heading = unit(displacement);
            car.look = std::array::from_fn(|i| car.look[i] * 0.5 + heading[i] * 0.5);
            let uses_shared_height = index != 0 && car.definition.path_is_level;
            let height = if uses_shared_height {
                self.shared_height
            } else if car.height_cached {
                car.height
            } else {
                let mut height = position[1];
                if car.definition.drive_on_soultrees {
                    let previous = [car.matrix[12], car.matrix[13], car.matrix[14]];
                    if let Some(hit) = height_query(index, previous)?
                        && height < hit
                    {
                        height = hit
                    }
                }
                car.height = height;
                if height == car.last_height && car.definition.path_is_level {
                    car.height_cached = true
                } else {
                    car.last_height = height
                }
                height
            };
            if index == 0 {
                self.shared_height = height;
            }
            let up = if height_was_cached || uses_shared_height {
                [0., 1., 0.]
            } else {
                [0.; 3]
            };
            let forward = unit(car.look);
            let right = cross(up, forward);
            car.matrix = [
                right[0],
                right[1],
                right[2],
                0.,
                up[0],
                up[1],
                up[2],
                0.,
                forward[0],
                forward[1],
                forward[2],
                0.,
                position[0] + car.definition.offset[0],
                height,
                position[2] + car.definition.offset[2],
                1.,
            ];
            car.route_time = t;
            publish(
                index,
                MotionState {
                    current: car.matrix,
                    previous: car.previous_matrix,
                    reciprocal_dt: self.reciprocal_dt,
                },
            )?;
        }
        self.frame()
    }
    ///Previous/current model cache consumed by native procedural velocity getters.
    pub fn motion_state(&self, index: usize) -> Option<MotionState> {
        self.cars.get(index).map(|car| MotionState {
            current: car.matrix,
            previous: car.previous_matrix,
            reciprocal_dt: self.reciprocal_dt,
        })
    }
    pub fn frame(&self) -> Result<Frame> {
        Ok(Frame {
            time: self.time,
            duration: self.route.duration(),
            cars: self
                .cars
                .iter()
                .map(|c| CarFrame {
                    model: c.definition.model.clone(),
                    matrix: c.matrix,
                    route_time: c.route_time,
                    lag_distance: c.definition.lag_distance,
                    use_collision: c.definition.use_collision,
                    use_lighting: c.definition.use_lighting,
                })
                .collect(),
            wheel_matrices: self
                .wheels
                .as_ref()
                .map(|(motion, nodes)| motion.pose(nodes, self.time))
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const VUE:&[u8]=b"FRAME 0\ntransform \"Body\" 1 0 0 0 1 0 0 0 1 1 3 2\nFRAME 1\ntransform \"Body\" 1 0 0 0 1 0 0 0 1 3 2 2\nFRAME 2\ntransform \"Body\" 1 0 0 0 1 0 0 0 1 4 1 2\nFRAME 3\ntransform \"Body\" 1 0 0 0 1 0 0 0 1 1 3 2\n";
    #[test]
    fn route_swaps_author_height_uses_native_cubic_and_wraps() {
        let route = Route::from_vue(VUE, 2.).unwrap();
        assert_eq!(route.keys[0].position, [1., 2., 3.]);
        assert_eq!(route.duration(), 1.5);
        assert_eq!(route.sample(0.25).unwrap(), [2.0625, 2., 2.5]);
        assert_eq!(route.sample(1.5).unwrap(), route.keys[0].position);
        assert_eq!(route.sample(-0.25).unwrap(), route.sample(1.25).unwrap());
        assert_eq!(route.lengths.len(), 4);
        assert_eq!(route.lengths[3], 0.);
    }
    #[test]
    fn procedural_integer_fields_preserve_strtol_and_native_defaults() {
        let document=b"[Animations]\nNumberOfAnimations=2\n[Animation1]\nAnimationType=Procedural\nSLT=animgeometry\\engine.slt\nVUE=locomotive.Vue\nFPS=2\nLagDistance=71.429977\nUseCollision=t\n[Animation2]\nAnimationType=Procedural\nSLT=animgeometry\\car.slt\nVUE=locomotive.Vue\nFPS=2\nLagDistance=0x20.5\n";
        let definitions = definitions(document).unwrap();
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].lag_distance, 71);
        assert_eq!(definitions[1].lag_distance, 32);
        assert!(
            definitions[0].path_is_level
                && definitions[0].drive_on_soultrees
                && definitions[0].use_collision
        );
    }
    #[test]
    fn per_car_lag_turns_independently_and_paused_steps_are_rejected() {
        let route = Route::from_vue(VUE, 2.).unwrap();
        let d = Definition {
            model: "car.slt".into(),
            route: "locomotive.vue".into(),
            fps: 2.,
            lag_distance: 1,
            offset: [0.; 3],
            path_is_level: true,
            drive_on_soultrees: false,
            use_collision: true,
            use_lighting: true,
        };
        let mut lead = d.clone();
        lead.lag_distance = 0;
        let mut runtime = Runtime::new(route, vec![lead, d]).unwrap();
        for _ in 0..20 {
            runtime.step(1. / 60., |_| Ok(None)).unwrap();
        }
        let frame = runtime.frame().unwrap();
        assert_ne!(frame.cars[0].matrix, frame.cars[1].matrix);
        assert_ne!(frame.cars[0].route_time, frame.cars[1].route_time);
        assert_eq!(frame.cars[0].matrix[13], 2.);
        assert!(runtime.step(0., |_| Ok(None)).is_err());
    }
    #[test]
    fn scene_height_sees_earlier_cars_new_pose_before_later_car_update() {
        use std::cell::RefCell;
        let definition = Definition {
            model: "car.slt".into(),
            route: "route.vue".into(),
            fps: 2.,
            lag_distance: 0,
            offset: [0.; 3],
            path_is_level: false,
            drive_on_soultrees: true,
            use_collision: true,
            use_lighting: true,
        };
        let mut runtime = Runtime::new(
            Route::from_vue(VUE, 2.).unwrap(),
            vec![definition.clone(), definition],
        )
        .unwrap();
        let published = RefCell::new(Vec::<(usize, MotionState)>::new());
        let origins = RefCell::new(Vec::new());
        runtime
            .step_with_scene(
                0.1,
                |index, previous| {
                    origins.borrow_mut().push(previous);
                    if index == 0 {
                        Ok(Some(10.))
                    } else {
                        Ok(Some(published.borrow()[0].1.current[13] + 1.))
                    }
                },
                |index, motion| {
                    published.borrow_mut().push((index, motion));
                    Ok(())
                },
            )
            .unwrap();
        let published = published.into_inner();
        assert_eq!(origins.into_inner(), [[0.; 3]; 2]);
        assert_eq!(
            published
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(published[0].1.current[13], 10.);
        assert_eq!(published[1].1.current[13], 11.);
        assert_eq!(published[0].1.linear_velocity()[1], 100.);
        assert_eq!(runtime.motion_state(1), Some(published[1].1));
        //With a no-hit service, the retained wrapper produces identical route
        //poses and history without publishing scene updates.
        let make = || {
            Runtime::new(
                Route::from_vue(VUE, 2.).unwrap(),
                runtime
                    .cars
                    .iter()
                    .map(|car| car.definition.clone())
                    .collect(),
            )
            .unwrap()
        };
        let mut plain = make();
        let mut scene = make();
        plain.step(0.1, |_| Ok(None)).unwrap();
        scene
            .step_with_scene(0.1, |_, _| Ok(None), |_, _| Ok(()))
            .unwrap();
        assert_eq!(plain.motion_state(0), scene.motion_state(0));
        assert_eq!(plain.motion_state(1), scene.motion_state(1));
    }
}
