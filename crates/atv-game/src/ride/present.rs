//! Presentation-only interpolation between delivered 60 Hz states, a port of
//! web/motorcycle-presentation.js: `MotorcyclePresentation` (every preset) and
//! `AtvPresentation` (Enhanced/High, `graphics.interpolation`). Inputs,
//! physics and every simulation state stay untouched; only the drawn vehicle,
//! rider, crash pose, race CPUs and chase camera are blended, and anything
//! discontinuous snaps.
use super::{CameraMode, Phase, Pose, Ride, quaternion_world};
use atv_core::camera::CameraFrame;

type M = [f32; 16];

/// A race CPU's drawn pose (race frame `opponents[i]` with `physical`).
#[derive(Clone, Debug)]
pub struct Opponent {
    pub origin: [f32; 3],
    /// None: the CPU is not simulated physically (not drawn by the engine).
    pub physical: Option<Physical>,
}

#[derive(Clone, Debug)]
pub struct Physical {
    pub orientation: [f32; 4],
    pub quad_nodes: Vec<M>,
    pub rider_nodes: Vec<M>,
    pub rider_world: Option<M>,
}

/// The fields of one delivered state the browser's presentation reads.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub origin: [f32; 3],
    pub orientation: [f32; 4],
    pub reset_count: u32,
    /// The motorcycle's `motorcycle_tick` and whether the frame is `normal`
    /// (Riding, no stunt, no detached rider); None on an ATV.
    pub bike: Option<(u64, bool)>,
    pub quad_nodes: Vec<M>,
    pub rider_nodes: Vec<M>,
    /// `rider_world_matrix`: the detached (wreck) rider's world.
    pub rider_world: Option<M>,
    /// Crazy/Gorn `funny_crash` quad and rider node matrices.
    pub crash: Option<(Vec<M>, Vec<M>)>,
    pub camera: CameraFrame,
    pub camera_mode: CameraMode,
    pub opponents: Option<Vec<Opponent>>,
}

impl Snapshot {
    /// The ride's latest state, with the optional crash presentation pose.
    /// Also returns the wheel lift it drew (game.js `wheelLift`).
    pub fn capture(
        ride: &Ride,
        crash: Option<&atv_core::crash_effects::Frame>,
        reproject: Option<&dyn Fn(f32, f32) -> f32>,
    ) -> crate::Result<(Self, f32)> {
        let mut lift = 0.;
        let frame = ride.frame();
        let mut pose = ride.pose()?;
        // atvPresentation.quadPose: Enhanced/High wheels on the visible
        // ground (not during a Crazy/Gorn crash).
        if let Some(height) = reproject.filter(|_| crash.is_none())
            && let Some((quad, lifted)) = ride.reprojected_quad(height)
        {
            pose.quad_nodes = quad;
            lift = lifted;
        }
        let (camera, camera_mode) = ride.camera();
        let bike = ride.motorcycle().map(|b| {
            (
                b.tick,
                frame.wreck.phase == Phase::Riding && b.stunt_name.is_none(),
            )
        });
        let opponents = ride.race().and_then(|r| r.frame.as_ref()).map(|f| {
            f.opponents
                .iter()
                .map(|o| Opponent {
                    origin: o.origin,
                    physical: o.physical.as_ref().map(|p| Physical {
                        orientation: p.orientation,
                        quad_nodes: p.quad_matrices.clone(),
                        rider_nodes: p.rider_matrices.clone(),
                        rider_world: p.rider_world_matrix,
                    }),
                })
                .collect()
        });
        let snapshot = Self {
            origin: frame.origin,
            orientation: frame.orientation,
            reset_count: frame.reset_count,
            bike,
            quad_nodes: pose.quad_nodes,
            rider_nodes: pose.rider_nodes,
            rider_world: ride.detached_rider_world(),
            crash: crash.map(|c| (c.quad_node_matrices.clone(), c.rider_node_matrices.clone())),
            camera: *camera,
            camera_mode,
            opponents,
        };
        Ok((snapshot, lift))
    }

    /// This state in the browser's simulation-frame shape, the fields
    /// web/motorcycle-presentation.js reads (parity tooling).
    pub fn browser_frame(&self) -> serde_json::Value {
        use serde_json::json;
        let mode = match self.camera_mode {
            CameraMode::Follow => serde_json::Value::Null,
            CameraMode::Wreck => json!("wreck"),
            CameraMode::Intro => json!("intro"),
            CameraMode::Tv => json!("tv"),
        };
        let normal = self.bike.is_some_and(|(_, normal)| normal);
        json!({
            "frame": {
                "vehicle_kind": if self.bike.is_some() { "motorcycle" } else { "atv" },
                "origin": self.origin,
                "orientation": self.orientation,
                "reset_count": self.reset_count,
                "motorcycle_tick": self.bike.map(|(tick, _)| tick),
                "wreck": {"phase": if self.bike.is_none() || normal { "Riding" } else { "Wrecked" }},
                "stunt": serde_json::Value::Null,
                "quad_node_matrices": self.quad_nodes,
                "rider_node_matrices": self.rider_nodes,
                "rider_world_matrix": self.rider_world,
                "funny_crash": self.crash.as_ref().map(|(quad, rider)| json!({
                    "quad_node_matrices": quad, "rider_node_matrices": rider})),
                "race": self.opponents.as_ref().map(|opponents| json!({"opponents": opponents.iter().map(|o| json!({
                    "origin": o.origin,
                    "physical": o.physical.as_ref().map(|p| json!({
                        "orientation": p.orientation,
                        "quad_matrices": p.quad_nodes,
                        "rider_matrices": p.rider_nodes,
                        "rider_world_matrix": p.rider_world,
                    })),
                })).collect::<Vec<_>>()})),
            },
            "camera": {
                "position": self.camera.position,
                "target": self.camera.target,
                "preset": self.camera.preset,
                "mode": mode,
            },
        })
    }

    /// The drawn player pose: the vehicle at this origin and orientation, the
    /// rider under its detached world or the vehicle's.
    pub fn pose(&self) -> Pose {
        let quad_world = quaternion_world(self.origin, self.orientation);
        Pose {
            quad_world,
            quad_nodes: self.quad_nodes.clone(),
            rider_world: self.rider_world.unwrap_or(quad_world),
            rider_nodes: self.rider_nodes.clone(),
        }
    }

    /// The race CPUs' drawn poses (physical CPUs only, as `cpu_poses`).
    pub fn cpu_poses(&self) -> Vec<Pose> {
        self.opponents
            .iter()
            .flatten()
            .filter_map(|o| {
                let p = o.physical.as_ref()?;
                let quad_world = quaternion_world(o.origin, p.orientation);
                Some(Pose {
                    quad_world,
                    quad_nodes: p.quad_nodes.clone(),
                    rider_world: p.rider_world.unwrap_or(quad_world),
                    rider_nodes: p.rider_nodes.clone(),
                })
            })
            .collect()
    }
}

fn mix<const N: usize>(a: [f32; N], b: [f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn length(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// `quaternion(r)`: a rotation's [x, y, z, w] from column-major 3x3 entries.
fn quaternion(r: [f32; 9]) -> [f32; 4] {
    let [m00, m10, m20, m01, m11, m21, m02, m12, m22] = r;
    let trace = m00 + m11 + m22;
    if trace > 0. {
        let s = 0.5 / (trace + 1.).sqrt();
        return [(m21 - m12) * s, (m02 - m20) * s, (m10 - m01) * s, 0.25 / s];
    }
    if m00 > m11 && m00 > m22 {
        let s = 2. * (1. + m00 - m11 - m22).sqrt();
        return [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s];
    }
    if m11 > m22 {
        let s = 2. * (1. + m11 - m00 - m22).sqrt();
        return [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s];
    }
    let s = 2. * (1. + m22 - m00 - m11).sqrt();
    [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
}

/// `rigid(m)`: rotation and per-axis scale (a mirrored node keeps its
/// reflection in X scale); None when degenerate.
fn rigid(m: &M) -> Option<([f32; 4], [f32; 3])> {
    let mut scale: [f32; 3] = std::array::from_fn(|c| length(&m[c * 4..c * 4 + 3]));
    let det = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[4] * (m[1] * m[10] - m[2] * m[9])
        + m[8] * (m[1] * m[6] - m[2] * m[5]);
    if !scale.iter().all(|v| *v > 1e-8) || !det.is_finite() {
        return None;
    }
    if det < 0. {
        scale[0] = -scale[0];
    }
    let r: [f32; 9] = std::array::from_fn(|i| m[(i / 3) * 4 + i % 3] / scale[i / 3]);
    Some((quaternion(r), scale))
}

/// `blendMatrix`: node matrices blend as rotation (shortest-arc quaternion),
/// per-axis scale and translation, so a spinning wheel keeps its size.
pub fn blend_matrix(a: &M, b: &M, t: f32) -> M {
    let (ra, rb) = (rigid(a), rigid(b));
    let (Some((qa, sa)), Some((qb, sb))) = (ra, rb) else {
        return mix(*a, *b, t);
    };
    if sa[0].signum() != sb[0].signum()
        || [a[3], a[7], a[11], b[3], b[7], b[11]]
            .iter()
            .any(|v| *v != 0.)
    {
        return mix(*a, *b, t);
    }
    let dot: f32 = (0..4).map(|i| qa[i] * qb[i]).sum();
    let qb = if dot < 0. { qb.map(|v| -v) } else { qb };
    let q = mix(qa, qb, t);
    let n = length(&q);
    let [x, y, z, w] = q.map(|v| v / n);
    let scale = mix(sa, sb, t);
    let r = [
        1. - 2. * (y * y + z * z),
        2. * (x * y + z * w),
        2. * (x * z - y * w),
        2. * (x * y - z * w),
        1. - 2. * (x * x + z * z),
        2. * (y * z + x * w),
        2. * (x * z + y * w),
        2. * (y * z - x * w),
        1. - 2. * (x * x + y * y),
    ];
    let mut out = [0.; 16];
    for c in 0..3 {
        for k in 0..3 {
            out[c * 4 + k] = r[c * 3 + k] * scale[c];
        }
    }
    for k in 12..16 {
        out[k] = a[k] + (b[k] - a[k]) * t;
    }
    out
}

fn blend_all(a: &[M], b: &[M], t: f32) -> Vec<M> {
    a.iter()
        .zip(b)
        .map(|(a, b)| blend_matrix(a, b, t))
        .collect()
}

/// `slerpish`: normalized lerp along the shorter arc.
fn slerpish(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let b = if dot < 0. { b.map(|v| -v) } else { b };
    let q = mix(a, b, t);
    let n = length(&q);
    q.map(|v| v / n)
}

fn near(a: [f32; 3], b: [f32; 3], limit: f32) -> bool {
    length(&[a[0] - b[0], a[1] - b[1], a[2] - b[2]]) <= limit
}

struct Segment {
    previous: Snapshot,
    current: Snapshot,
    now: f64,
    /// Milliseconds.
    duration: f64,
    ticks: u32,
}

/// What a rendered frame draws.
pub struct Presented {
    pub snapshot: Snapshot,
    pub alpha: f32,
    pub interpolated: bool,
}

/// Both browser presenters; `accept` after each delivered batch, `sample`
/// on each rendered frame. Times are milliseconds on one clock.
#[derive(Default)]
pub struct Presentation {
    bike: Option<Segment>,
    quad: Option<Segment>,
}

fn bike_normal(f: &Snapshot) -> bool {
    f.bike.is_some_and(|(_, normal)| normal) && f.rider_world.is_none()
}

impl Presentation {
    pub fn reset(&mut self) {
        self.bike = None;
        self.quad = None;
    }

    /// A delivered batch of `ticks` steps from `previous` to `current`.
    /// `continuous`: focused, no menu and no camera cycle in the batch;
    /// `interpolation`: the preset's ATV interpolation (Enhanced/High).
    #[allow(clippy::too_many_arguments)]
    pub fn accept(
        &mut self,
        previous: &Snapshot,
        current: &Snapshot,
        now: f64,
        ticks: u32,
        continuous: bool,
        interpolation: bool,
        // Alpha slow motion (`scale` < 1) stretches each 60 Hz interval.
        scale: f32,
    ) {
        let scale = f64::from(scale.clamp(0.05, 1.));
        // MotorcyclePresentation.accept (every preset).
        self.bike = None;
        let compatible = bike_normal(previous)
            && bike_normal(current)
            && previous.reset_count == current.reset_count
            && current.bike.unwrap().0 > previous.bike.unwrap().0
            && previous.camera.preset == current.camera.preset
            && previous.quad_nodes.len() == current.quad_nodes.len()
            && previous.rider_nodes.len() == current.rider_nodes.len();
        if continuous && compatible {
            let ticks = current.bike.unwrap().0 - previous.bike.unwrap().0;
            if ticks <= 4 {
                self.bike = Some(Segment {
                    previous: previous.clone(),
                    current: current.clone(),
                    now,
                    duration: ticks as f64 * 1000. / 60. / scale,
                    ticks: ticks as u32,
                });
            }
        }
        // AtvPresentation.accept (Enhanced/High).
        self.quad = None;
        if !interpolation {
            return;
        }
        let shape = |a: &[M], b: &[M]| a.len() == b.len();
        let compatible = previous.bike.is_none()
            && current.bike.is_none()
            && previous.reset_count == current.reset_count
            && near(previous.origin, current.origin, 30.)
            && shape(&previous.quad_nodes, &current.quad_nodes)
            && shape(&previous.rider_nodes, &current.rider_nodes)
            && previous.rider_world.is_some() == current.rider_world.is_some()
            && previous.crash.is_some() == current.crash.is_some();
        if continuous
            && (1..=4).contains(&ticks)
            && compatible
            && previous.camera.preset == current.camera.preset
            && previous.camera_mode == current.camera_mode
        {
            self.quad = Some(Segment {
                previous: previous.clone(),
                current: current.clone(),
                now,
                duration: ticks as f64 * 1000. / 60. / scale,
                ticks,
            });
        }
    }

    /// The state to draw at `now`; `active`: focused, not lost, no menu.
    /// `phase`: the fraction of the next fixed step already accumulated.
    /// When given (Alpha slow motion), the blend tracks simulated time one
    /// step behind the latest state instead of a wall-clock segment, so a
    /// changing time scale or uneven step arrival never freezes or snaps.
    pub fn sample(
        &mut self,
        frame: &Snapshot,
        now: f64,
        active: bool,
        phase: Option<f32>,
    ) -> Presented {
        if !active {
            self.reset();
        }
        let at = |s: &Segment| match phase {
            Some(p) => (s.ticks as f32 - 1. + p.clamp(0., 1.)) / s.ticks as f32,
            None => ((now - s.now) / s.duration).clamp(0., 1.) as f32,
        };
        if let Some(s) = &self.bike {
            let alpha = at(s);
            return Presented {
                snapshot: blend_bike(s, frame, alpha),
                alpha,
                interpolated: alpha < 1.,
            };
        }
        if let Some(s) = &self.quad {
            let t = at(s);
            if t < 1. {
                return Presented {
                    snapshot: blend_quad(s, t),
                    alpha: t,
                    interpolated: true,
                };
            }
        }
        Presented {
            snapshot: frame.clone(),
            alpha: 1.,
            interpolated: false,
        }
    }
}

/// MotorcyclePresentation.sample: origin, orientation, both rigs' node
/// matrices and the camera position and target.
fn blend_bike(s: &Segment, frame: &Snapshot, alpha: f32) -> Snapshot {
    let (a, b) = (&s.previous, &s.current);
    let mut q = b.orientation;
    if (0..4).map(|i| q[i] * a.orientation[i]).sum::<f32>() < 0. {
        q = q.map(|v| -v);
    }
    let orientation = mix(a.orientation, q, alpha);
    let n = length(&orientation);
    let mut out = frame.clone();
    out.origin = mix(a.origin, frame.origin, alpha);
    out.orientation = orientation.map(|v| v / n);
    out.quad_nodes = blend_all(&a.quad_nodes, &b.quad_nodes, alpha);
    out.rider_nodes = blend_all(&a.rider_nodes, &b.rider_nodes, alpha);
    out.camera.position = mix(a.camera.position, frame.camera.position, alpha);
    out.camera.target = mix(a.camera.target, frame.camera.target, alpha);
    out
}

/// AtvPresentation.sample: the player quad, attached or detached rider,
/// Crazy/Gorn pose, race CPUs and the chase camera.
fn blend_quad(s: &Segment, t: f32) -> Snapshot {
    let (a, b) = (&s.previous, &s.current);
    let mut out = b.clone();
    out.origin = mix(a.origin, b.origin, t);
    out.orientation = slerpish(a.orientation, b.orientation, t);
    out.quad_nodes = blend_all(&a.quad_nodes, &b.quad_nodes, t);
    out.rider_nodes = blend_all(&a.rider_nodes, &b.rider_nodes, t);
    out.rider_world = match (a.rider_world, b.rider_world) {
        (Some(x), Some(y)) => Some(blend_matrix(&x, &y, t)),
        (_, other) => other,
    };
    out.crash = match (&a.crash, &b.crash) {
        (Some((aq, ar)), Some((bq, br))) if aq.len() == bq.len() && ar.len() == br.len() => {
            Some((blend_all(aq, bq, t), blend_all(ar, br, t)))
        }
        (_, other) => other.clone(),
    };
    if let (Some(old), Some(new)) = (&a.opponents, &b.opponents)
        && old.len() == new.len()
    {
        out.opponents = Some(
            new.iter()
                .zip(old)
                .map(|(cpu, old)| blend_opponent(old, cpu, t))
                .collect(),
        );
    }
    out.camera.position = mix(a.camera.position, b.camera.position, t);
    out.camera.target = mix(a.camera.target, b.camera.target, t);
    out
}

/// blendOpponents for one CPU; a respawned or teleported CPU snaps.
fn blend_opponent(old: &Opponent, cpu: &Opponent, t: f32) -> Opponent {
    if !near(old.origin, cpu.origin, 30.) || old.physical.is_some() != cpu.physical.is_some() {
        return cpu.clone();
    }
    let mut out = cpu.clone();
    out.origin = mix(old.origin, cpu.origin, t);
    if let (Some(q), Some(p)) = (&old.physical, &cpu.physical) {
        if q.quad_nodes.len() != p.quad_nodes.len()
            || q.rider_nodes.len() != p.rider_nodes.len()
            || q.rider_world.is_some() != p.rider_world.is_some()
        {
            return cpu.clone();
        }
        out.physical = Some(Physical {
            orientation: slerpish(q.orientation, p.orientation, t),
            quad_nodes: blend_all(&q.quad_nodes, &p.quad_nodes, t),
            rider_nodes: blend_all(&q.rider_nodes, &p.rider_nodes, t),
            rider_world: match (q.rider_world, p.rider_world) {
                (Some(x), Some(y)) => Some(blend_matrix(&x, &y, t)),
                (_, other) => other,
            },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use atv_core::math::Det;

    fn rotation(angle: f32, scale: f32, x: f32) -> M {
        let (s, c) = (angle.sin_d(), angle.cos_d());
        [
            scale,
            0.,
            0.,
            0.,
            0.,
            c * scale,
            s * scale,
            0.,
            0.,
            -s * scale,
            c * scale,
            0.,
            x,
            2.,
            3.,
            1.,
        ]
    }

    fn close(got: &M, want: M) {
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-5, "{got:?} against {want:?}");
        }
    }

    /// Values from web/motorcycle-presentation.js `blendMatrix` in Node:
    /// a spinning, scaling node keeps its size halfway through; a mirrored
    /// node against an unmirrored one falls back to the entry-wise blend.
    #[test]
    fn blend_matrix_matches_the_browser() {
        close(
            &blend_matrix(&rotation(0.2, 1., 1.), &rotation(1.7, 1.2, 3.), 0.5),
            [
                1.1, 0., 0., 0., 0., 0.6398514, 0.8947571, 0., 0., -0.8947571, 0.6398514, 0., 2.,
                2., 3., 1.,
            ],
        );
        let mirrored = [
            -1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 5., 5., 5., 1.,
        ];
        close(
            &blend_matrix(&rotation(0.3, 1., 0.), &mirrored, 0.25),
            [
                0.5, 0., 0., 0., 0., 0.9665024, 0.2216402, 0., 0., -0.2216402, 0.9665024, 0., 1.25,
                2.75, 3.5, 1.,
            ],
        );
    }

    /// A quad moving one unit per step along X.
    fn at(x: f32) -> Snapshot {
        Snapshot {
            origin: [x, 0., 0.],
            orientation: [0., 0., 0., 1.],
            reset_count: 0,
            bike: None,
            quad_nodes: Vec::new(),
            rider_nodes: Vec::new(),
            rider_world: None,
            crash: None,
            camera: CameraFrame {
                position: [x, 3., -10.],
                target: [x, 0., 0.],
                fov: 60.,
                preset: 0,
            },
            camera_mode: CameraMode::Follow,
            opponents: None,
        }
    }

    #[test]
    fn slow_motion_phase_moves_with_scaled_time_at_uneven_frames() {
        let mut p = Presentation::default();
        let (mut accumulator, mut x, mut now, mut drawn) = (0f32, 0f32, 0f64, None);
        let mut delivered = at(x);
        for frame in 0..400 {
            // A phone's uneven frames while the time scale eases 1 -> 0.3 -> 1.
            let dt = if frame % 3 == 0 { 0.021 } else { 0.0145 };
            let scale = 1. - 0.7 * atv_core::math::Det::sin_d(frame as f32 / 400. * std::f32::consts::PI);
            now += f64::from(dt) * 1000.;
            accumulator += dt * scale;
            let ticks = (accumulator * 60.).floor() as u32;
            if ticks > 0 {
                accumulator -= ticks as f32 / 60.;
                x += ticks as f32;
                let current = at(x);
                p.accept(&delivered, &current, now, ticks, true, true, scale);
                delivered = current;
            }
            let shown = p.sample(&delivered, now, true, Some(accumulator * 60.));
            let origin = shown.snapshot.origin[0];
            if let Some(last) = drawn {
                // Drawn motion is exactly simulated time: no freeze, no snap.
                let moved: f32 = origin - last;
                assert!((moved - dt * scale * 60.).abs() < 1e-3, "{frame}: {moved}");
            }
            drawn = Some(origin);
        }
    }
}
