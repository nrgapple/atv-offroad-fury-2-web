//! Pre-race camera (exact-build QuadzillaPreRaceCamera on SimpleCutSceneCamera).
//!
//! Recovered from private exports: shot loader `0030b570`, tick `0030bac0`,
//! state setter `0030df00`, end of shots `0030cea0`, activation `0030f980`.
//! Race load plays every `<venue>nat<n>.vub` shot back to back at 30 frames per
//! second, then a single-player swoop arcs from beside the rider into the
//! follow camera. The countdown starts once less than 2.2s of the swoop
//! remain, or at once when a button skips the intro. The fly-by/orbit states
//! used without shots or in split screen are not reproduced. Host `sin`/`cos`
//! replace the PS2 tables.
use crate::Result;
use crate::math::Det;

type V = [f32; 3];
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(a: V) -> f32 {
    dot(a, a).sqrt()
}
fn normalize(a: V) -> V {
    let l = length(a);
    if l > 0. { scale(a, 1. / l) } else { a }
}
fn lerp(a: V, b: V, t: f32) -> V {
    add(a, scale(sub(b, a), t))
}
/// The cutscene camera's low-pass filters: v += min(dt, τ)/τ · (target − v).
fn filter(value: f32, target: f32, dt: f32, tau: f32) -> f32 {
    value + dt.min(tau) / tau * (target - value)
}
fn filter3(value: V, target: V, dt: f32, tau: f32) -> V {
    std::array::from_fn(|i| filter(value[i], target[i], dt, tau))
}

/// One 40-byte VUB frame: unit forward, unit up, world position (ft), FOV (°).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotFrame {
    pub forward: V,
    pub up: V,
    pub position: V,
    pub fov: f32,
}
/// VUB: little-endian u32 header (2, frame count, 1), then the frames.
pub fn parse_vub(bytes: &[u8]) -> Result<Vec<ShotFrame>> {
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    if bytes.len() < 12 || word(0) != 2 {
        return Err("unsupported VUB camera shot".into());
    }
    let count = word(1) as usize;
    if count == 0 || count > 10_000 || bytes.len() != 12 + count * 40 {
        return Err("VUB frame count does not match its size".into());
    }
    let float = |i: usize| f32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let frames: Vec<_> = (0..count)
        .map(|n| {
            let at = 12 + n * 40;
            let v = |k: usize| {
                [
                    float(at + k * 12),
                    float(at + k * 12 + 4),
                    float(at + k * 12 + 8),
                ]
            };
            ShotFrame {
                forward: v(0),
                up: v(1),
                position: v(2),
                fov: float(at + 36),
            }
        })
        .collect();
    if frames.iter().any(|f| {
        !(f.forward
            .iter()
            .chain(&f.up)
            .chain(&f.position)
            .all(|v| v.is_finite())
            && f.fov > 0.
            && f.fov < 180.)
    }) {
        return Err("invalid VUB camera frame".into());
    }
    Ok(frames)
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pose {
    pub position: V,
    pub target: V,
    pub fov: f32,
}

const FRAME_RATE: f32 = 30.;
const SWOOP: f32 = 2.5;
/// Remaining swoop time below which the camera starts the countdown.
const COUNTDOWN_AT: f32 = 2.2;

#[derive(Clone, Copy, Debug)]
struct Swoop {
    remaining: f32,
    pivot: V,
    radius: f32,
    from: V,
    axis: V,
    look_start: V,
    end: Pose,
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Shots,
    Swoop(Swoop),
    Done,
}

pub struct Intro {
    frames: Vec<ShotFrame>,
    starts: Vec<usize>,
    time: f32,
    shot: Option<usize>,
    phase: Phase,
    position: V,
    look: V,
    fov: f32,
    countdown: bool,
}
/// What the race camera needs from the rider and the world each step.
pub struct Scene<'a> {
    pub rider: V,
    /// Rider +Z forward.
    pub forward: V,
    /// The follow camera pose from the player's selected view.
    pub follow: Pose,
    pub ground: &'a dyn Fn(f32, f32) -> f32,
}
impl Intro {
    /// Shots in load order (nat1, nat2, ...); none starts at the swoop.
    pub fn new(shots: Vec<Vec<ShotFrame>>) -> Self {
        let mut frames = Vec::new();
        let mut starts = Vec::new();
        for shot in shots.into_iter().filter(|s| !s.is_empty()) {
            starts.push(frames.len());
            frames.extend(shot);
        }
        Self {
            phase: if frames.is_empty() {
                Phase::Done
            } else {
                Phase::Shots
            },
            frames,
            starts,
            time: 0.,
            shot: None,
            position: [0.; 3],
            look: [0.; 3],
            fov: 0.,
            countdown: false,
        }
    }
    pub fn active(&self) -> bool {
        !matches!(self.phase, Phase::Done)
    }
    /// The countdown has been requested (swoop under 2.2s, or a skip).
    pub fn countdown_requested(&self) -> bool {
        self.countdown
    }
    /// Input handler `00212d40`: any button before the countdown starts
    /// returns to the follow camera and starts the countdown at once.
    pub fn skip(&mut self) -> bool {
        if self.countdown {
            return false;
        }
        self.countdown = true;
        self.phase = Phase::Done;
        true
    }
    /// The race went live: the follow camera takes over (`001f54b0`).
    pub fn finish(&mut self) {
        self.countdown = true;
        self.phase = Phase::Done;
    }
    pub fn step(&mut self, dt: f32, scene: &Scene<'_>) -> Result<Option<Pose>> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err("invalid intro camera step".into());
        }
        if matches!(self.phase, Phase::Shots) {
            self.time += dt;
            let last = self.frames.len() - 1;
            let raw = self.time * FRAME_RATE;
            let index = (raw as usize).min(last);
            if raw as usize >= last {
                self.begin_swoop(scene);
            } else {
                let shot = self.starts.iter().rposition(|s| *s <= index).unwrap_or(0);
                let end = self.starts.get(shot + 1).map_or(last, |n| n - 1);
                // Linear between frames; a shot's last frame extrapolates.
                let (a, b, t) = if index < end {
                    (index, index + 1, raw - index as f32)
                } else {
                    (index - 1, index, 1. + raw - index as f32)
                };
                let (a, b) = (self.frames[a], self.frames[b]);
                let position = lerp(a.position, b.position, t);
                let forward = normalize(lerp(a.forward, b.forward, t));
                let fov = a.fov + (b.fov - a.fov) * t;
                // Entering a shot resets the filters: a hard cut.
                if self.shot != Some(shot) {
                    self.shot = Some(shot);
                    self.position = position;
                    self.fov = fov;
                } else {
                    self.position = filter3(self.position, position, dt, 0.3);
                    self.fov = filter(self.fov, fov, dt, 0.4);
                }
                return Ok(Some(Pose {
                    position: self.position,
                    target: add(self.position, scale(forward, 100.)),
                    fov: self.fov,
                }));
            }
        }
        let Phase::Swoop(mut swoop) = self.phase else {
            return Ok(None);
        };
        swoop.remaining = (swoop.remaining - dt).max(0.);
        if swoop.remaining < COUNTDOWN_AT {
            self.countdown = true;
        }
        let (position, look, fov) = if swoop.remaining > 0. {
            let angle = std::f32::consts::PI * swoop.remaining / SWOOP;
            let mut p = add(
                swoop.pivot,
                scale(
                    normalize(rotate(swoop.axis, angle, swoop.from)),
                    swoop.radius,
                ),
            );
            p[1] = p[1].max((scene.ground)(p[0], p[2]) + 2.);
            p[1] = p[1].max(0.7 * self.position[1]);
            let u = (SWOOP - swoop.remaining) / SWOOP;
            (
                p,
                lerp(swoop.look_start, swoop.end.target, u),
                90. + (swoop.end.fov - 90.) * u,
            )
        } else {
            (swoop.end.position, swoop.end.target, swoop.end.fov)
        };
        self.position = filter3(self.position, position, dt, 0.5);
        self.look = filter3(self.look, look, dt, 0.6);
        self.fov = filter(self.fov, fov, dt, 0.4);
        self.phase = Phase::Swoop(swoop);
        Ok(Some(Pose {
            position: self.position,
            target: self.look,
            fov: self.fov,
        }))
    }
    /// `0030df00` case 5: start 18ft out at −45° from the rider's forward,
    /// 2ft over the ground (+2), and arc about the midpoint to the follow pose.
    fn begin_swoop(&mut self, scene: &Scene<'_>) {
        let (s, c) = (-std::f32::consts::FRAC_PI_4).sin_cos_d();
        let f = scene.forward;
        let direction = normalize([f[0] * c + f[2] * s, 0., -f[0] * s + f[2] * c]);
        let mut start = add(scene.rider, scale(direction, 18.));
        start[1] = (start[1] + 2.).max((scene.ground)(start[0], start[2])) + 2.;
        let end = scene.follow;
        let pivot = scale(add(start, end.position), 0.5);
        let from = sub(end.position, pivot);
        let n = normalize(from);
        // The axis is (n × up) × n, deliberately left unnormalized.
        let axis = cross(cross(n, [0., 1., 0.]), n);
        let look_start = add(scene.rider, [0., 3., 0.]);
        self.position = start;
        self.look = look_start;
        self.fov = 90.;
        self.phase = Phase::Swoop(Swoop {
            remaining: SWOOP,
            pivot,
            radius: length(from),
            from,
            axis,
            look_start,
            end,
        });
    }
}
/// Rodrigues rotation about an axis used as given (not normalized).
fn rotate(k: V, angle: f32, v: V) -> V {
    let (s, c) = angle.sin_cos_d();
    add(
        add(scale(v, c), scale(cross(k, v), s)),
        scale(k, dot(k, v) * (1. - c)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vub(frames: &[(V, f32)]) -> Vec<u8> {
        let mut b = Vec::new();
        for w in [2u32, frames.len() as u32, 1] {
            b.extend(w.to_le_bytes());
        }
        for (p, fov) in frames {
            for v in [[0., 0., 1.], [0., 1., 0.], *p] {
                for x in v {
                    b.extend(f32::to_le_bytes(x));
                }
            }
            b.extend(fov.to_le_bytes());
        }
        b
    }
    fn scene(follow: Pose) -> Scene<'static> {
        Scene {
            rider: [0.; 3],
            forward: [0., 0., 1.],
            follow,
            ground: &|_, _| 0.,
        }
    }
    #[test]
    fn parses_vub_and_rejects_bad_sizes() {
        let bytes = vub(&[([1., 2., 3.], 60.), ([4., 5., 6.], 61.)]);
        let frames = parse_vub(&bytes).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].position, [4., 5., 6.]);
        assert_eq!(frames[1].fov, 61.);
        assert!(parse_vub(&bytes[..bytes.len() - 1]).is_err());
        let mut wrong = bytes.clone();
        wrong[0] = 3;
        assert!(parse_vub(&wrong).is_err());
    }
    #[test]
    fn shots_play_at_thirty_frames_then_swoop_into_the_follow_pose() {
        // Two shots of 30 and 15 frames: 1.5s of shots.
        let a: Vec<_> = (0..30).map(|i| ([i as f32, 10., 0.], 50.)).collect();
        let b: Vec<_> = (0..15).map(|_| ([0., 20., 500.], 80.)).collect();
        let mut intro = Intro::new(vec![
            parse_vub(&vub(&a)).unwrap(),
            parse_vub(&vub(&b)).unwrap(),
        ]);
        let follow = Pose {
            position: [0., 6., -15.],
            target: [0., 3., 0.],
            fov: 85.,
        };
        let s = scene(follow);
        let first = intro.step(1. / 60., &s).unwrap().unwrap();
        assert_eq!(first.position[0], 0.5, "frame 0.5 at 1/60s");
        // The second shot is a hard cut, not filtered from the first.
        let mut pose = first;
        for _ in 0..60 {
            pose = intro.step(1. / 60., &s).unwrap().unwrap();
        }
        assert_eq!(pose.position, [0., 20., 500.]);
        assert_eq!(pose.fov, 80.);
        assert!(!intro.countdown_requested());
        // 1.5s of shots, then the swoop starts 18ft out at −45°.
        let mut seconds: f32 = 61. / 60.;
        while !intro.countdown_requested() {
            pose = intro.step(1. / 60., &s).unwrap().unwrap();
            seconds += 1. / 60.;
        }
        assert!((seconds - (1.5 + 0.3)).abs() < 0.05, "{seconds}");
        // The arc ends at the follow pose; the filters then settle on it.
        for _ in 0..600 {
            pose = intro.step(1. / 60., &s).unwrap().unwrap();
        }
        assert!(length(sub(pose.position, follow.position)) < 0.01);
        assert!(length(sub(pose.target, follow.target)) < 0.01);
        assert!((pose.fov - 85.).abs() < 0.01);
        intro.finish();
        assert!(!intro.active());
        assert!(intro.step(1. / 60., &s).unwrap().is_none());
    }
    #[test]
    fn swoop_starts_beside_the_rider_and_stays_off_the_ground() {
        let mut intro = Intro::new(vec![]);
        assert!(!intro.active());
        intro.phase = Phase::Shots;
        intro.frames = vec![ShotFrame {
            forward: [0., 0., 1.],
            up: [0., 1., 0.],
            position: [0.; 3],
            fov: 60.,
        }];
        intro.starts = vec![0];
        let follow = Pose {
            position: [0., 6., -15.],
            target: [0., 3., 0.],
            fov: 85.,
        };
        let s = scene(follow);
        let start = intro.step(1. / 60., &s).unwrap().unwrap();
        // Forward +Z rotated −45° about +Y points to −X/+Z.
        let expected = [-18. * 0.70710677, 4., 18. * 0.70710677];
        assert!(length(sub(start.position, expected)) < 0.2, "{start:?}");
        assert!((start.fov - 90.).abs() < 0.01);
        for _ in 0..200 {
            let pose = intro.step(1. / 60., &s).unwrap().unwrap();
            assert!(pose.position[1] >= 2. - 1e-3, "{pose:?}");
        }
    }
    #[test]
    fn skip_starts_the_countdown_once() {
        let a: Vec<_> = (0..90).map(|_| ([0., 10., 0.], 50.)).collect();
        let mut intro = Intro::new(vec![parse_vub(&vub(&a)).unwrap()]);
        assert!(intro.skip());
        assert!(intro.countdown_requested() && !intro.active());
        assert!(!intro.skip());
    }
}
