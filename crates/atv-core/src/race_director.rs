//! After-finish camera director (exact-build QuadzillaTVCamera and the
//! QuadzillaCameraController race-over branch).
//!
//! Recovered from private exports: TV loader `0031b550`, switching `0031bb50`,
//! TV step `0031bf40` → `001ac230`, distance FOV `001ac580`, fly-by shake
//! `0031c2d0`, director `001f3bb0`. Three seconds after the race is over
//! (`00215db0`, `001f1990`), the director follows the player between the
//! course's `<venue>nat.tv` trackside cameras. Each cut rolls that camera's
//! weights to show one of the rider's follow presets or the TV view. The
//! original seeds libc `rand` from the clock; the port uses its own stream.
//! `Blur` is loaded but unused by the build; `Stationary` (with Look/Up) is
//! absent from every shipped file and is not reproduced.
//!
//! STOPCAM ("Stop Action Replay Cam", cheat `0033ede0` toggling settings
//! +0x187c, read only at `001f3e94` in the director): while the director
//! makes no cut, a player about a second into a main stunt may swoop the
//! camera from the shown view toward the follow pose over 1.5 s
//! (`0030f680`, the cut-scene camera's swoop), where it then stays still
//! until the director's next cut.
use crate::Result;
use crate::race_intro::{add, cross, length, lerp, normalize, rotate, scale, sub};
use std::sync::atomic::{AtomicBool, Ordering};

type V = [f32; 3];
fn distance2(a: V, b: V) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Seconds after race over before the director takes the camera.
pub const DELAY: f32 = 3.;

/// The STOPCAM session switch (settings +0x187c): off at boot (`0021b8c8`).
static STOP_ACTION: AtomicBool = AtomicBool::new(false);

/// STOPCAM: flips the switch; true when it is now on ("Stop Action Replay
/// Cam Enabled!"), false when off ("... Disabled!").
pub fn toggle_stop_action() -> bool {
    !STOP_ACTION.fetch_xor(true, Ordering::Relaxed)
}

pub fn stop_action() -> bool {
    STOP_ACTION.load(Ordering::Relaxed)
}

/// The stop action's swoop (`0030f680`: 1.5 s).
const STOP_SWOOP: f32 = 1.5;
/// The main stunt clock window that can start it (exclusive, seconds).
const STOP_WINDOW: (f32, f32) = (0.9, 1.1);
/// Seconds between stop actions (the build compares 4000 ms of its clock).
const STOP_GAP: f32 = 4.;

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub position: V,
    pub max_fov: f32,
    pub min_fov: f32,
    pub max_fov_distance: f32,
    pub min_fov_distance: f32,
    pub next: Vec<usize>,
    /// Chase1, Chase2, Slot1, Slot2, Classic, LookBack, FirstPerson (percent).
    pub weights: [i32; 7],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Cameras {
    pub cameras: Vec<Camera>,
    pub min_wait: f32,
    pub max_wait: f32,
}
/// `<venue>nat.tv`: `[CameraPositions]` and `[CameraN]` sections of
/// `key = value` lines. Keys the build does not read (`CameraSwitchWaitTimer`,
/// `Classic1`) keep the loader defaults.
pub fn parse_tv(text: &str) -> Result<Cameras> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name.trim().to_ascii_lowercase(), Vec::new()));
        } else if let Some((key, value)) = line.split_once('=')
            && let Some((_, fields)) = sections.last_mut()
        {
            fields.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let section = |name: &str| {
        sections
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| f.as_slice())
    };
    let get = |fields: &[(String, String)], key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let number = |fields: &[(String, String)], key: &str, default: f32| -> Result<f32> {
        get(fields, key).map_or(Ok(default), |v| {
            v.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("invalid TV camera {key}"))
        })
    };
    let header = section("camerapositions").ok_or("TV cameras need [CameraPositions]")?;
    let count = number(header, "numberofcameras", 0.)? as usize;
    if count == 0 || count > 256 {
        return Err("invalid TV camera count".into());
    }
    let min_wait = number(header, "cameraswitchminwaittimer", 2.)?;
    let max_wait = number(header, "cameraswitchmaxwaittimer", 8.)?;
    let mut cameras = Vec::with_capacity(count);
    for n in 1..=count {
        let fields = section(&format!("camera{n}")).ok_or("missing TV camera section")?;
        let position: Vec<f32> = get(fields, "position")
            .ok_or("TV camera needs a Position")?
            .split(',')
            .map(|v| v.trim().parse::<f32>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| "invalid TV camera Position")?;
        let [x, y, z] = position[..] else {
            return Err("invalid TV camera Position".into());
        };
        let fov = number(fields, "fov", f32::NAN)?;
        let (max_fov, min_fov) = if fov.is_finite() {
            (fov, fov)
        } else {
            (
                number(fields, "maxfov", 70.)?,
                number(fields, "minfov", 10.)?,
            )
        };
        let next_count = number(fields, "numnextcameras", 0.)? as usize;
        let next = (1..=next_count.min(16))
            .map(|i| {
                let index = number(fields, &format!("nextcamera{i}"), 0.)? as usize;
                if index == 0 || index > count {
                    return Err("invalid TV NextCamera".into());
                }
                Ok(index - 1)
            })
            .collect::<Result<Vec<_>>>()?;
        let weight = |key: &str| number(fields, key, 0.).map(|v| v as i32);
        cameras.push(Camera {
            position: [x, y, z],
            max_fov,
            min_fov,
            max_fov_distance: number(fields, "maxfovdistance", 200.)?,
            min_fov_distance: number(fields, "minfovdistance", 20.)?,
            next,
            weights: [
                weight("chase1")?,
                weight("chase2")?,
                weight("slot1")?,
                weight("slot2")?,
                weight("classic")?,
                weight("lookback")?,
                weight("firstperson")?,
            ],
        });
    }
    Ok(Cameras {
        cameras,
        min_wait,
        max_wait,
    })
}

/// What the director shows after a cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// A follow-camera preset; `ease` seeds its filters from the previous
    /// view (`00301220`, one cut in four), otherwise it snaps.
    Follow {
        preset: usize,
        ease: bool,
    },
    Tv,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Pose {
    pub position: V,
    pub target: V,
    pub fov: f32,
}
/// A racer that can trigger the trackside fly-by shake.
#[derive(Clone, Copy, Debug)]
pub struct Racer {
    pub origin: V,
    pub speed: f32,
}
pub struct Step {
    /// Set on the frame of a cut.
    pub cut: Option<View>,
    /// The TV view (or the stop action's view), when it is the shown view.
    pub tv: Option<Pose>,
    /// The STOPCAM stop action shows (`tv` is its view).
    pub stop_action: bool,
}

/// What STOPCAM reads each director tick.
pub struct StopInput<'a> {
    /// The player's main stunt clock (rider +0x884 +0x3fc) while the rider
    /// animation is in the main stunt (mode 4); None otherwise.
    pub stunt_clock: Option<f32>,
    /// The follow camera's pose: the view when a follow preset shows, and
    /// the swoop's end.
    pub follow: Pose,
    pub ground: &'a dyn Fn(f32, f32) -> f32,
}

/// The cut-scene camera's swoop as STOPCAM sets it up (`0030f680`): from
/// the shown view (its position, a look point 30 ft along its forward, its
/// FOV) about the midpoint to the follow pose, the angle linear in time;
/// unfiltered (`0030df00` case 6 clears the output filters).
#[derive(Clone, Copy, Debug)]
struct StopSwoop {
    remaining: f32,
    pivot: V,
    radius: f32,
    from: V,
    axis: V,
    look_start: V,
    fov_start: f32,
    end: Pose,
    position: V,
}

impl StopSwoop {
    fn new(shown: Pose, end: Pose) -> Self {
        let forward = normalize(sub(shown.target, shown.position));
        let pivot = scale(add(shown.position, end.position), 0.5);
        let from = sub(end.position, pivot);
        let n = normalize(from);
        Self {
            remaining: STOP_SWOOP,
            pivot,
            radius: length(from),
            from,
            // (n × up) × n, unnormalized, as the pre-race swoop.
            axis: cross(cross(n, [0., 1., 0.]), n),
            look_start: add(shown.position, scale(forward, 30.)),
            fov_start: shown.fov,
            end,
            position: shown.position,
        }
    }

    /// `0030bac0` case 5 without the output filters.
    fn step(&mut self, dt: f32, ground: &dyn Fn(f32, f32) -> f32) -> Pose {
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining <= 0. {
            self.position = self.end.position;
            return self.end;
        }
        let angle = std::f32::consts::PI * self.remaining / STOP_SWOOP;
        let mut p = add(
            self.pivot,
            scale(normalize(rotate(self.axis, angle, self.from)), self.radius),
        );
        p[1] = p[1].max(ground(p[0], p[2]) + 2.);
        p[1] = p[1].max(0.7 * self.position[1]);
        self.position = p;
        let u = (STOP_SWOOP - self.remaining) / STOP_SWOOP;
        Pose {
            position: p,
            target: lerp(self.look_start, self.end.target, u),
            fov: self.fov_start + (self.end.fov - self.fov_start) * u,
        }
    }
}

struct Shake {
    duration: f32,
    elapsed: f32,
    magnitude: f32,
    direction: V,
}
pub struct Director {
    tv: Cameras,
    current: Option<usize>,
    timer: f32,
    next: Option<(usize, f32)>,
    view: View,
    fov: f32,
    snap: bool,
    shake: Option<Shake>,
    shaken: Option<usize>,
    shake_frame: bool,
    random: crate::engine_audio::OriginalRandom,
    /// The director's running time and its last stop action's.
    clock: f32,
    last_stop: Option<f32>,
    stop: Option<StopSwoop>,
    /// The last TV pose shown (a stop action starts from it).
    shown: Option<Pose>,
}
impl Director {
    pub fn new(tv: Cameras, seed: u64) -> Self {
        Self {
            tv,
            current: None,
            timer: -1.,
            next: None,
            view: View::Tv,
            fov: 70.,
            snap: true,
            shake: None,
            shaken: None,
            shake_frame: false,
            random: crate::engine_audio::OriginalRandom::with_seed(seed),
            clock: 0.,
            last_stop: None,
            stop: None,
            shown: None,
        }
    }
    pub fn current(&self) -> Option<usize> {
        self.current
    }
    /// Starts over from Camera1, as a new director with `seed` would,
    /// keeping the cameras.
    pub fn restart(&mut self, seed: u64) {
        self.current = None;
        self.timer = -1.;
        self.next = None;
        self.view = View::Tv;
        self.fov = 70.;
        self.snap = true;
        self.shake = None;
        self.shaken = None;
        self.shake_frame = false;
        self.random = crate::engine_audio::OriginalRandom::with_seed(seed);
        self.clock = 0.;
        self.last_stop = None;
        self.stop = None;
        self.shown = None;
    }
    fn unit(&mut self) -> f32 {
        self.random.next_u31() as f32 * 4.656613e-10
    }
    /// One director tick: switch cameras, then step the TV view for `rider`
    /// (the player), with every racer able to trigger the fly-by shake.
    pub fn step(&mut self, dt: f32, rider: V, racers: &[Racer]) -> Result<Step> {
        self.step_with(dt, rider, racers, None)
    }
    /// [`Self::step`] with STOPCAM's inputs (`stop`, when the switch is on).
    pub fn step_with(
        &mut self,
        dt: f32,
        rider: V,
        racers: &[Racer],
        stop: Option<&StopInput<'_>>,
    ) -> Result<Step> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err("invalid director step".into());
        }
        self.clock += dt;
        let cut = self.switch(dt, rider).map(|camera| self.cut(camera));
        if cut.is_some() {
            self.stop = None;
        } else if let Some(input) = stop {
            self.stop_action(input);
        }
        if let (Some(swoop), Some(input)) = (&mut self.stop, stop) {
            let pose = swoop.step(dt, input.ground);
            self.shown = Some(pose);
            return Ok(Step {
                cut,
                tv: Some(pose),
                stop_action: true,
            });
        }
        self.stop = None;
        let tv = if self.view == View::Tv {
            let pose = self.tv_pose(dt, rider, racers);
            self.shown = Some(pose);
            Some(pose)
        } else {
            None
        };
        Ok(Step {
            cut,
            tv,
            stop_action: false,
        })
    }
    /// `001f3e8c`–`001f3f78`: with no cut this tick, a main stunt between
    /// 0.9 s and 1.1 s in, more than 4 s since the last stop action and a
    /// 50 % roll start the swoop from the shown view; the switch timer is
    /// held at 2 s or more. Unidentified gates of the build (the controller's
    /// +0x60/+0x6c and the follow preset's +0x50) are taken as passing.
    fn stop_action(&mut self, input: &StopInput<'_>) {
        let Some(clock) = input.stunt_clock else {
            return;
        };
        if clock <= STOP_WINDOW.0 || clock >= STOP_WINDOW.1 {
            return;
        }
        if self.last_stop.is_some_and(|t| self.clock - t <= STOP_GAP) {
            return;
        }
        if (2. * self.unit()) as i32 == 0 {
            return;
        }
        self.last_stop = Some(self.clock);
        // The shown view: a stop action's, the TV camera's or the follow's.
        let shown = match (self.stop.is_some(), self.view) {
            (true, _) | (_, View::Tv) => self.shown.unwrap_or(input.follow),
            (false, View::Follow { .. }) => input.follow,
        };
        self.stop = Some(StopSwoop::new(shown, input.follow));
        self.timer = self.timer.max(2.);
    }
    /// `0031bb50`: cut only after the minimum wait; move to a listed next
    /// camera that is closer, and when the maximum wait expires take the
    /// nearest camera if it is at least twice as close as the best next one.
    fn switch(&mut self, dt: f32, rider: V) -> Option<usize> {
        self.timer -= dt;
        if self.tv.max_wait - self.timer < self.tv.min_wait {
            return None;
        }
        let d2 = |i: usize| distance2(rider, self.tv.cameras[i].position);
        match self.current {
            Some(current) if self.timer >= 0. => {
                let best = self.tv.cameras[current]
                    .next
                    .iter()
                    .map(|&i| (i, d2(i)))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some(best) = best {
                    self.next = Some(best);
                    if best.1 < d2(current) {
                        return Some(best.0);
                    }
                }
                None
            }
            None => Some(0),
            Some(current) => {
                let nearest = (0..self.tv.cameras.len())
                    .map(|i| (i, d2(i)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))?;
                let pick = match self.next {
                    Some(next) if nearest.1 * 2. > next.1 => next.0,
                    _ => nearest.0,
                };
                (pick != current).then_some(pick)
            }
        }
    }
    /// `001f3bb0`: a cut rolls r in 0..100 against the cumulative weights
    /// Chase1, Chase2, Slot1, Slot2, Classic and LookBack (follow presets
    /// 0–5), then FirstPerson, which the build maps to preset 4; the rest
    /// shows the TV view.
    fn cut(&mut self, camera: usize) -> View {
        self.current = Some(camera);
        self.timer = self.tv.max_wait;
        self.snap = true;
        self.shake = None;
        let weights = self.tv.cameras[camera].weights;
        let roll = (self.unit() * 100.) as i32;
        let ease = (self.unit() * 4.) as i32 == 0;
        let mut total = 0;
        let mut view = View::Tv;
        for (i, w) in weights.iter().enumerate() {
            total += w;
            if roll < total {
                view = View::Follow {
                    preset: if i == 6 { 4 } else { i },
                    ease,
                };
                break;
            }
        }
        self.view = view;
        view
    }
    fn tv_pose(&mut self, dt: f32, rider: V, racers: &[Racer]) -> Pose {
        let Some(index) = self.current else {
            return Pose {
                position: rider,
                target: rider,
                fov: 70.,
            };
        };
        let camera = &self.tv.cameras[index];
        let target = [rider[0], rider[1] + 3., rider[2]];
        // `001ac580`: wide near the camera, zoomed in far away (XZ distance).
        let d = ((camera.position[0] - target[0]).powi(2)
            + (camera.position[2] - target[2]).powi(2))
        .sqrt();
        let fov = if camera.max_fov == camera.min_fov {
            camera.max_fov
        } else {
            let span = camera.max_fov_distance - camera.min_fov_distance;
            let t = if span.abs() > 0. {
                (camera.max_fov_distance - d) / span
            } else {
                1.
            };
            (camera.min_fov + (camera.max_fov - camera.min_fov) * t)
                .clamp(camera.min_fov, camera.max_fov)
        };
        self.fov = if std::mem::take(&mut self.snap) {
            fov
        } else {
            self.fov + dt.min(0.4) / 0.4 * (fov - self.fov)
        };
        let mut position = camera.position;
        self.shake_step(dt, position, racers);
        if let Some(shake) = &self.shake {
            self.shake_frame = !self.shake_frame;
            if self.shake_frame {
                let scale = shake.magnitude * (1. - shake.elapsed / shake.duration);
                for (p, d) in position.iter_mut().zip(shake.direction) {
                    *p += scale * d;
                }
            }
        }
        Pose {
            position,
            target,
            fov: self.fov,
        }
    }
    /// `0031c2d0`: a racer passing within 55ft of the TV camera above 170 ft/s
    /// shakes it for 0.45·s seconds by 14·s, s = min(1, (v−170)/170 ·
    /// (1 − d²/3025)), on alternate frames, decaying linearly.
    fn shake_step(&mut self, dt: f32, camera: V, racers: &[Racer]) {
        if let Some(shake) = &mut self.shake {
            shake.elapsed += dt;
            if shake.elapsed >= shake.duration {
                self.shake = None;
            }
            return;
        }
        for (i, r) in racers.iter().enumerate() {
            let d2 = distance2(r.origin, camera);
            if d2 > 3025. || r.speed < 170. || self.shaken == Some(i) {
                continue;
            }
            let s = ((r.speed - 170.) / 170. * (1. - d2 / 3025.)).min(1.);
            if s <= 0. {
                continue;
            }
            // Each component lies in [0, 1): offsets point into the + octant.
            let direction = [self.unit(), self.unit(), self.unit()];
            let length = direction
                .iter()
                .map(|v| v * v)
                .sum::<f32>()
                .sqrt()
                .max(1e-6);
            self.shaken = Some(i);
            self.shake = Some(Shake {
                duration: 0.45 * s,
                elapsed: 0.,
                magnitude: 14. * s,
                direction: direction.map(|v| v / length),
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TV: &str = "; trackside cameras\r\n[CameraPositions]\r\nNumberOfCameras = 3\r\nCameraSwitchWaitTimer = 5.0\r\n\r\n[Camera1]\r\nPosition = 0,10,0\r\nMaxFOV = 70\r\nMinFOV = 30\r\nMaxFOVDistance = 110\r\nMinFOVDistance = 20\r\nLookBack = 100\r\nBlur = 100\r\nInitialZone = 1\r\nNumNextCameras = 1\r\nNextCamera1 = 2\r\n\r\n[Camera2]\r\nPosition = 100,10,0\r\nFirstPerson = 100\r\nClassic1 = 100\r\nNumNextCameras = 1\r\nNextCamera1 = 3\r\n\r\n[Camera3]\r\nPosition = 300,10,0\r\nFOV = 50\r\nNumNextCameras = 1\r\nNextCamera1 = 1\r\n";
    fn step(d: &mut Director, dt: f32, rider: V) -> Step {
        d.step(dt, rider, &[]).unwrap()
    }
    #[test]
    fn parses_shipped_keys_and_keeps_loader_defaults() {
        let tv = parse_tv(TV).unwrap();
        assert_eq!(
            (tv.min_wait, tv.max_wait),
            (2., 8.),
            "CameraSwitchWaitTimer is unread"
        );
        assert_eq!(tv.cameras.len(), 3);
        assert_eq!(tv.cameras[0].next, vec![1]);
        assert_eq!(tv.cameras[0].weights, [0, 0, 0, 0, 0, 100, 0]);
        assert_eq!(
            tv.cameras[1].weights,
            [0, 0, 0, 0, 0, 0, 100],
            "Classic1 is unread"
        );
        assert_eq!((tv.cameras[1].max_fov, tv.cameras[1].min_fov), (70., 10.));
        assert_eq!((tv.cameras[2].max_fov, tv.cameras[2].min_fov), (50., 50.));
        assert!(parse_tv("[CameraPositions]\nNumberOfCameras=1\n").is_err());
        assert!(parse_tv(&TV.replace("NextCamera1 = 3", "NextCamera1 = 9")).is_err());
    }
    #[test]
    fn first_cut_is_camera_one_then_closer_next_cameras_after_the_wait() {
        let mut d = Director::new(parse_tv(TV).unwrap(), 1);
        let first = step(&mut d, 1. / 60., [90., 0., 0.]);
        assert_eq!(
            d.current(),
            Some(0),
            "Camera1 even though Camera2 is nearer"
        );
        // LookBack = 100 shows follow preset 5 (Lookback).
        assert!(matches!(first.cut, Some(View::Follow { preset: 5, .. })));
        // No cut before the 2s minimum wait, even though Camera2 is closer.
        for _ in 0..100 {
            assert!(step(&mut d, 1. / 60., [90., 0., 0.]).cut.is_none());
        }
        let mut cut = None;
        for _ in 0..30 {
            cut = cut.or(step(&mut d, 1. / 60., [90., 0., 0.]).cut);
        }
        assert_eq!(d.current(), Some(1));
        // The build maps FirstPerson to follow preset 4 (Classic).
        assert!(matches!(cut, Some(View::Follow { preset: 4, .. })));
    }
    #[test]
    fn expired_wait_takes_the_nearest_camera_only_if_twice_as_close() {
        let mut d = Director::new(parse_tv(TV).unwrap(), 1);
        step(&mut d, 1. / 60., [0., 0., 0.]);
        // Far beyond Camera3 the next camera (2) is nearer than the current.
        let mut seconds = 0.;
        while d.current() == Some(0) && seconds < 9. {
            step(&mut d, 1. / 60., [-50., 0., 0.]);
            seconds += 1. / 60.;
        }
        assert_eq!(d.current(), Some(0), "the next camera is not closer");
        // After the 8s maximum wait the nearest camera (Camera3) is not twice
        // as close as the last best-next distance, so the cached best next
        // camera (Camera2) is taken.
        step(&mut d, 1. / 60., [590., 0., 0.]);
        assert_eq!(d.current(), Some(1));
        // Waiting there, the listed next camera (Camera3) is closer.
        for _ in 0..150 {
            step(&mut d, 1. / 60., [590., 0., 0.]);
        }
        assert_eq!(d.current(), Some(2));
    }
    #[test]
    fn tv_view_zooms_with_distance_and_tracks_the_rider() {
        let text = TV.replace("LookBack = 100", "");
        let mut d = Director::new(parse_tv(&text).unwrap(), 1);
        let near = step(&mut d, 1. / 60., [10., 0., 0.]).tv.unwrap();
        assert_eq!(
            near.fov, 70.,
            "MaxFOV within MinFOVDistance, snapped on the cut"
        );
        assert_eq!(near.target, [10., 3., 0.]);
        assert_eq!(near.position, [0., 10., 0.]);
        let mut far = near;
        for _ in 0..180 {
            far = step(&mut d, 1. / 60., [-200., 0., 0.]).tv.unwrap();
        }
        assert!(
            (far.fov - 30.).abs() < 0.5,
            "MinFOV beyond MaxFOVDistance: {}",
            far.fov
        );
    }
    #[test]
    fn fast_pass_near_the_camera_shakes_on_alternate_frames() {
        let text = TV.replace("LookBack = 100", "");
        let mut d = Director::new(parse_tv(&text).unwrap(), 1);
        let fast = [Racer {
            origin: [5., 10., 0.],
            speed: 340.,
        }];
        let poses: Vec<_> = (0..6)
            .map(|_| {
                d.step(1. / 60., [5., 0., 0.], &fast)
                    .unwrap()
                    .tv
                    .unwrap()
                    .position
            })
            .collect();
        assert_ne!(poses[0], [0., 10., 0.]);
        assert_eq!(poses[1], [0., 10., 0.]);
        assert!(poses[0].iter().zip([0., 10., 0.]).all(|(p, c)| *p >= c));
    }

    #[test]
    fn stopcam_swoops_from_the_tv_view_to_the_follow_pose_and_holds() {
        let one = "[CameraPositions]\r\nNumberOfCameras = 1\r\n\r\n[Camera1]\r\nPosition = 0,10,0\r\nFOV = 50\r\nNumNextCameras = 0\r\n";
        let mut d = Director::new(parse_tv(one).unwrap(), 7);
        let ground = |_: f32, _: f32| 0.;
        let follow = Pose {
            position: [40., 6., -10.],
            target: [40., 3., 0.],
            fov: 60.,
        };
        let input = |clock: Option<f32>| StopInput {
            stunt_clock: clock,
            follow,
            ground: &ground,
        };
        let dt = 1. / 60.;
        let tick = |d: &mut Director, rider: V, clock: Option<f32>| {
            d.step_with(dt, rider, &[], Some(&input(clock))).unwrap()
        };
        // The TV view, and no stop action outside a main stunt's window.
        let first = tick(&mut d, [40., 0., 0.], None);
        assert!(first.cut == Some(View::Tv) && !first.stop_action);
        for clock in [None, Some(0.5), Some(0.9), Some(1.1), Some(1.4)] {
            for _ in 0..30 {
                assert!(!tick(&mut d, [40., 0., 0.], clock).stop_action);
            }
        }
        // About a second into a trick: a 50 % roll each tick starts it.
        let mut start = None;
        for _ in 0..12 {
            let s = tick(&mut d, [40., 0., 0.], Some(1.));
            if s.stop_action {
                start = s.tv;
                break;
            }
        }
        let start = start.expect("a stop action within the window");
        assert!(length(sub(start.position, [0., 10., 0.])) < 2., "{start:?}");
        assert!((start.fov - 50.).abs() < 1.);
        // 1.5 s later it rests on the follow pose and stays there as the
        // rider rides on, until the next cut.
        let mut last = start;
        for i in 0..120 {
            let s = tick(&mut d, [40. + i as f32, 0., 0.], None);
            assert!(s.stop_action);
            last = s.tv.unwrap();
        }
        assert_eq!(last, follow);
        // Another trick within 4 s does not start another.
        for _ in 0..60 {
            let s = tick(&mut d, [80., 0., 0.], Some(1.));
            assert_eq!(s.tv, Some(follow));
        }
    }

    #[test]
    fn stopcam_is_a_session_switch() {
        let on = toggle_stop_action();
        assert_eq!(stop_action(), on);
        assert_eq!(toggle_stop_action(), !on);
        assert_eq!(stop_action(), !on);
    }
}
