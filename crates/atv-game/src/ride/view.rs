//! Browser presentation of the recovered camera frame (chasePresentation in
//! web/render-support.js and presentCameraFrame/matrix in web/game.js): the
//! portrait adaptation, Chase camera Assisted (UphillSightline), the tight
//! Crazy/Gorn crash shot (crashSubjects/crashPresentation) and the
//! speed-rush FOV kick.
use atv_core::camera::CameraFrame;
use atv_core::math::Det;

/// Eye, heading and focal length for the scene camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub focal_length: f32,
}

/// The recovered follow/wreck frame as the browser frames it. Portrait
/// screens pull back and widen, as on phones.
pub fn chase(frame: &CameraFrame, aspect: f32) -> View {
    chase_assisted(frame, aspect, None, &|_, _| f32::NAN)
}

/// [`chase`] with Chase camera: Assisted. `assist` is the sightline and the
/// presented actor origin; the orbit rotates eye and aim about rider height,
/// and the visible ground (`height`) floors the eye 2 ft above it.
pub fn chase_assisted(
    frame: &CameraFrame,
    aspect: f32,
    assist: Option<(&UphillSightline, [f32; 3])>,
    height: &dyn Fn(f32, f32) -> f32,
) -> View {
    let aspect = f64::from(aspect);
    let portrait = ((1. - aspect) / 0.5).clamp(0., 1.);
    let pullback = assist.map_or(0., |(s, _)| s.pullback);
    let distance_scale = (1. + 0.35 * portrait) * (1. + pullback);
    let position = frame.position.map(f64::from);
    let target = frame.target.map(f64::from);
    let offset: [f64; 3] = std::array::from_fn(|i| target[i] - position[i]);
    let horizontal = offset[0].hypot_d(offset[2]);
    let yaw = offset[0].atan2_d(offset[2]);
    let mut position: [f64; 3] =
        std::array::from_fn(|i| position[i] - offset[i] * (distance_scale - 1.));
    let mut target = target;
    if let Some((sightline, origin)) = assist
        && sightline.lift > 0.
        && horizontal > 0.
    {
        let pivot = [
            f64::from(origin[0]),
            f64::from(origin[1]) + 3.,
            f64::from(origin[2]),
        ];
        let (dx, dz) = (offset[0] / horizontal, offset[2] / horizontal);
        let (s, c) = sightline.lift.sin_cos_d();
        let orbit = |p: [f64; 3]| {
            let along = (p[0] - pivot[0]) * dx + (p[2] - pivot[2]) * dz;
            let up = p[1] - pivot[1];
            let shift = along * c - up * s - along;
            [
                p[0] + dx * shift,
                pivot[1] + up * c + along * s,
                p[2] + dz * shift,
            ]
        };
        position = orbit(position);
        target = orbit(target);
        let ground = f64::from(height(position[0] as f32, position[2] as f32));
        // Math.max: a NaN ground keeps the eye (JavaScript yields NaN; never reached).
        if ground + 2. > position[1] {
            position[1] = ground + 2.;
        }
    }
    let view: [f64; 3] = std::array::from_fn(|i| target[i] - position[i]);
    let pitch = view[1].atan2_d(view[0].hypot_d(view[2])) + 0.12 * portrait;
    let focal_length = (4. / 3.) / (f64::from(frame.fov) * std::f64::consts::PI / 360.).tan_d()
        * (1. - 0.12 * portrait);
    View {
        position: position.map(|v| v as f32),
        yaw: yaw as f32,
        pitch: pitch as f32,
        focal_length: focal_length as f32,
    }
}

/// Chase camera: Assisted (UphillSightline in render-support.js): on
/// uphill grades the follow camera orbits up to 0.52 rad about rider height
/// and dollies back up to 24%, easing in over 0.45 s and out over 0.8 s.
/// Off (or any other camera preset, the wreck camera, a wreck) is the
/// unmodified rig.
#[derive(Clone, Copy, Debug, Default)]
pub struct UphillSightline {
    pub lift: f64,
    pub pullback: f64,
    air_time: f64,
    reset_count: Option<u32>,
}

/// What the sightline reads from the presented actor.
#[derive(Clone, Copy, Debug)]
pub struct SightlineActor {
    pub origin: [f32; 3],
    pub orientation: [f32; 4],
    pub speed: f32,
    pub grounded: [bool; 4],
    pub riding: bool,
    pub reset_count: u32,
}

impl UphillSightline {
    pub fn clear(&mut self) {
        self.lift = 0.;
        self.pullback = 0.;
        self.air_time = 0.;
    }

    /// One rendered frame; `actor` None is Chase camera Original.
    pub fn update(
        &mut self,
        actor: Option<&SightlineActor>,
        follow: bool,
        preset: usize,
        dt: f32,
        height: &dyn Fn(f32, f32) -> f32,
    ) {
        let Some(actor) = actor.filter(|a| follow && preset == 0 && a.riding) else {
            self.clear();
            return;
        };
        if self.reset_count != Some(actor.reset_count) {
            self.clear();
            self.reset_count = Some(actor.reset_count);
        }
        let step = if dt.is_finite() {
            f64::from(dt).max(0.)
        } else {
            0.
        };
        let mut desired = self.lift;
        if actor.speed < -1. {
            desired = 0.;
        } else if actor.grounded.iter().any(|g| *g) {
            self.air_time = 0.;
            let [w, x, y, z] = actor.orientation.map(f64::from);
            // Right-axis heading survives a wheelie beyond vertical.
            let rx = 1. - 2. * (y * y + z * z);
            let rz = 2. * (x * z - w * y);
            let horizontal = rx.hypot_d(rz);
            let [ox, _, oz] = actor.origin.map(f64::from);
            let ground = f64::from(height(ox as f32, oz as f32));
            let mut grades = if horizontal > 0.2 {
                [8., 20., 36.].map(|span: f64| {
                    let h = f64::from(height(
                        (ox - rz / horizontal * span) as f32,
                        (oz + rx / horizontal * span) as f32,
                    ));
                    (h - ground).atan2_d(span)
                })
            } else {
                [0.; 3]
            };
            let grade = if grades.iter().all(|g| g.is_finite()) {
                grades.sort_by(f64::total_cmp);
                grades[1]
            } else {
                0.
            };
            let amount = ((grade - 0.10) / 0.75).clamp(0., 1.);
            desired = 0.52 * amount * amount * (3. - 2. * amount);
        } else {
            self.air_time += step;
            // Carry takeoff framing through short contact gaps; ignore flips.
            if self.air_time > 0.35 {
                desired = 0.;
            }
        }
        let tau = if desired > self.lift { 0.45 } else { 0.80 };
        let blend = 1. - (-step / tau).exp_d();
        self.lift += (desired - self.lift) * blend;
        self.pullback = 0.24 * self.lift / 0.52;
    }
}

/// A node's bind-local vertex box (meshNodeBounds over LOD 0).
pub type NodeBox = Option<([f32; 3], [f32; 3])>;

pub fn mesh_node_bounds(mesh: &atv_core::mesh::Mesh) -> Vec<NodeBox> {
    let mut bounds: Vec<NodeBox> = vec![None; mesh.nodes.len()];
    for group in mesh.lods.first().map_or(&[][..], |l| &l.groups[..]) {
        for v in &group.vertices {
            let Some(slot) = bounds.get_mut(usize::from(v.node)) else {
                continue;
            };
            let b = slot.get_or_insert(([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]));
            for a in 0..3 {
                b.0[a] = b.0[a].min(v.local_position[a]);
                b.1[a] = b.1[a].max(v.local_position[a]);
            }
        }
    }
    bounds
}

fn world_bounds(b: ([f32; 3], [f32; 3]), m: &[f32; 16]) -> NodeBox {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for corner in 0..8 {
        let p: [f32; 3] = std::array::from_fn(|a| {
            if corner & (1 << a) != 0 {
                b.1[a]
            } else {
                b.0[a]
            }
        });
        for a in 0..3 {
            let v = m[a] * p[0] + m[4 + a] * p[1] + m[8 + a] * p[2] + m[12 + a];
            min[a] = min[a].min(v);
            max[a] = max[a].max(v);
        }
    }
    min.iter()
        .chain(&max)
        .all(|v| v.is_finite())
        .then_some((min, max))
}

/// poseBounds over the nodes whose names match `keep`.
fn central(
    bounds: &[NodeBox],
    nodes: &[atv_core::mesh::Node],
    keep: &dyn Fn(&str) -> bool,
    matrices: &[[f32; 16]],
    world: Option<&[f32; 16]>,
) -> Option<[f32; 3]> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for (i, b) in bounds.iter().enumerate() {
        let (Some(b), Some(node), Some(m)) = (b, nodes.get(i), matrices.get(i)) else {
            continue;
        };
        if !keep(&node.name.to_ascii_lowercase()) {
            continue;
        }
        let Some(mut t) = world_bounds(*b, m) else {
            continue;
        };
        if let Some(w) = world {
            let Some(wt) = world_bounds(t, w) else {
                continue;
            };
            t = wt;
        }
        for a in 0..3 {
            min[a] = min[a].min(t.0[a]);
            max[a] = max[a].max(t.1[a]);
        }
    }
    min.iter()
        .chain(&max)
        .all(|v| v.is_finite())
        .then(|| std::array::from_fn(|a| (min[a] + max[a]) / 2.))
}

/// The tight crash shot's subjects (crashSubjects): the rider's torso/pelvis
/// and the quad's chassis/body/frame centres; detached parts never count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrashSubjects {
    pub rider: [f32; 3],
    pub bike: [f32; 3],
}

/// Inputs for [`crash_subjects`]: models, their node boxes and the crash pose.
pub struct CrashPose<'a> {
    pub quad: (&'a atv_core::mesh::Mesh, &'a [NodeBox]),
    pub rider: (&'a atv_core::mesh::Mesh, &'a [NodeBox]),
    /// The crash frame's quad nodes (relative to `actor_world`).
    pub quad_nodes: &'a [[f32; 16]],
    /// The ragdoll's world matrices, else the simulated rider nodes under
    /// `rider_world`.
    pub ragdoll: &'a [[f32; 16]],
    pub rider_nodes: &'a [[f32; 16]],
    pub rider_world: [f32; 16],
    pub actor_world: [f32; 16],
    pub origin: [f32; 3],
}

pub fn crash_subjects(p: &CrashPose) -> CrashSubjects {
    let (rider_nodes, rider_world) = if p.ragdoll.is_empty() {
        (p.rider_nodes, Some(&p.rider_world))
    } else {
        (p.ragdoll, None)
    };
    let torso = central(
        p.rider.1,
        &p.rider.0.nodes,
        &|n| n.contains("torso") || n.contains("pelvis"),
        rider_nodes,
        rider_world,
    );
    let bike = central(
        p.quad.1,
        &p.quad.0.nodes,
        &|n| n.contains("chassis") || n.contains("body") || n.contains("frame"),
        p.quad_nodes,
        Some(&p.actor_world),
    );
    CrashSubjects {
        rider: torso.unwrap_or_else(|| match rider_world {
            Some(w) => [w[12], w[13] + 2., w[14]],
            None => [p.origin[0], p.origin[1] + 2., p.origin[2]],
        }),
        bike: bike.unwrap_or(p.origin),
    }
}

/// The tight authored crash shot (crashPresentation): nearby subjects share
/// the frame; as they separate the bike drops out and the camera stays with
/// the rider. Pitch is held within -0.55..-0.22, the distance within 16..32
/// ft, and the visible ground raises the eye instead of dollying back.
pub fn crash(
    view: View,
    aspect: f32,
    subjects: &CrashSubjects,
    height: &dyn Fn(f32, f32) -> f32,
) -> View {
    let rider = subjects.rider.map(f64::from);
    let bike = subjects.bike.map(f64::from);
    let separation = (0..3)
        .map(|i| (bike[i] - rider[i]) * (bike[i] - rider[i]))
        .sum::<f64>()
        .sqrt();
    let t = ((separation - 8.) / 14.).clamp(0., 1.);
    let bike_weight = 0.5 * (1. - t * t * (3. - 2. * t));
    let target: [f64; 3] = std::array::from_fn(|i| rider[i] + (bike[i] - rider[i]) * bike_weight);
    let pitch = f64::from(view.pitch).clamp(-0.55, -0.22);
    let yaw = f64::from(view.yaw);
    let (sy, cy) = yaw.sin_cos_d();
    let (sp, cp) = pitch.sin_cos_d();
    let forward = [sy * cp, sp, cy * cp];
    let right = [cy, 0., -sy];
    let up = [-sy * sp, cp, -cy * sp];
    let f = f64::from(view.focal_length);
    let aspect = f64::from(aspect);
    let mut distance: f64 = 16.;
    for (point, weight) in [(rider, 1.), (bike, bike_weight * 2.)] {
        let p: [f64; 3] = std::array::from_fn(|i| (point[i] - target[i]) * weight);
        let dot = |v: [f64; 3]| v[0] * p[0] + v[1] * p[1] + v[2] * p[2];
        let depth = dot(forward);
        distance = distance
            .max((dot(right).abs() + 2.5) * f / aspect / 0.8 - depth)
            .max((dot(up).abs() + 2.5) * f / 0.8 - depth);
    }
    let distance = distance.min(32.);
    let mut position: [f64; 3] = std::array::from_fn(|i| target[i] - forward[i] * distance);
    let ground = f64::from(height(position[0] as f32, position[2] as f32));
    if ground + 2. > position[1] {
        position[1] = ground + 2.;
    }
    let aim = (target[1] - position[1])
        .atan2_d((target[0] - position[0]).hypot_d(target[2] - position[2]));
    View {
        position: position.map(|v| v as f32),
        yaw: view.yaw,
        pitch: aim as f32,
        focal_length: view.focal_length,
    }
}

/// Authored camera shots (the pre-race camera and the after-finish
/// trackside cameras) keep their pose; only the portrait focal adaptation of
/// the chase view applies (cinematicPresentation in render-support.js).
pub fn cinematic(frame: &CameraFrame, aspect: f32) -> View {
    let portrait = ((1. - f64::from(aspect)) / 0.5).clamp(0., 1.);
    let view: [f64; 3] = std::array::from_fn(|i| f64::from(frame.target[i] - frame.position[i]));
    let focal_length = (4. / 3.) / (f64::from(frame.fov) * std::f64::consts::PI / 360.).tan_d()
        * (1. - 0.12 * portrait);
    View {
        position: frame.position,
        yaw: view[0].atan2_d(view[2]) as f32,
        pitch: view[1].atan2_d(view[0].hypot_d(view[2])) as f32,
        focal_length: focal_length as f32,
    }
}

/// Authored speed presentation's FOV kick (web/speed-rush.js): above 40 MPH
/// the view widens by up to about six degrees at Normal.
#[derive(Clone, Copy, Debug)]
pub struct SpeedRush {
    amount: f64,
    /// Settings → Speed rush peak: Off 0, Low 0.45, Normal 0.75, High 1.
    pub peak: f64,
}

impl Default for SpeedRush {
    fn default() -> Self {
        Self {
            amount: 0.,
            peak: 0.75,
        }
    }
}

/// `speedRushAmount`'s intensity scale for a saved `atv-speed-rush` value.
pub fn speed_rush_peak(intensity: &str) -> f64 {
    match intensity {
        "low" => 0.45,
        "normal" => 0.75,
        "high" => 1.,
        _ => 0.,
    }
}

impl SpeedRush {
    pub fn reset(&mut self) {
        self.amount = 0.;
    }

    /// `speed` in ft/s; `active` while riding, focused and unpaused.
    pub fn update(&mut self, speed: f32, active: bool, dt: f32) {
        let t = ((f64::from(speed).abs() * 3600. / 5280. - 40.) / 35.).clamp(0., 1.);
        let desired = if active && speed.is_finite() {
            t * t * (3. - 2. * t) * self.peak
        } else {
            0.
        };
        if desired == 0. {
            self.reset();
            return;
        }
        let step = f64::from(dt).max(0.);
        self.amount += (desired - self.amount) * (1. - (-step / 0.25).exp_d());
    }

    /// The smoothed amount (0..peak) the screen pass shares.
    pub fn amount(&self) -> f32 {
        self.amount as f32
    }

    pub fn focal_scale(&self) -> f32 {
        (1. - 0.09 * self.amount.clamp(0., 1.)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_keeps_the_recovered_eye_and_aim() {
        let frame = CameraFrame {
            position: [10., 5., 0.],
            target: [10., 4., 10.],
            fov: 60.,
            preset: 0,
        };
        let view = chase(&frame, 16. / 9.);
        assert_eq!(view.position, [10., 5., 0.]);
        assert!(view.yaw.abs() < 1e-6);
        assert!((view.pitch - (-0.1f64).atan_d() as f32).abs() < 1e-6);
        let focal = (4. / 3.) / (std::f64::consts::PI / 6.).tan_d();
        assert!((view.focal_length - focal as f32).abs() < 1e-6);
    }

    // Expected values from web/render-support.js on the same inputs
    // (UphillSightline, chasePresentation, crashPresentation).
    #[test]
    fn assisted_chase_and_tight_crash_follow_the_browser() {
        let height = |x: f32, z: f32| 0.3 * z + 0.02 * x;
        let mut sightline = UphillSightline::default();
        let actor = SightlineActor {
            origin: [100., 30., 100.],
            orientation: [0.25f32.cos(), 0., 0.25f32.sin(), 0.],
            speed: 20.,
            grounded: [true; 4],
            riding: true,
            reset_count: 0,
        };
        let mut lifts = Vec::new();
        for _ in 0..40 {
            sightline.update(Some(&actor), true, 0, 1. / 64., &height);
            lifts.push(sightline.lift);
        }
        let air = SightlineActor {
            grounded: [false; 4],
            ..actor
        };
        for _ in 0..30 {
            sightline.update(Some(&air), true, 0, 1. / 64., &height);
            lifts.push(sightline.lift);
        }
        let expected = [
            (0, 0.0022324475465924365),
            (19, 0.03275099046393171),
            (39, 0.049105256130493845),
            (45, 0.049105256130493845),
            (69, 0.042001951377452086),
        ];
        for (i, value) in expected {
            assert!((lifts[i] - value).abs() < 2e-6, "{i} {} {value}", lifts[i]);
        }
        assert!((sightline.pullback - 0.0193855160203625).abs() < 2e-6);
        sightline.lift = 0.3;
        sightline.pullback = 0.24 * 0.3 / 0.52;
        let frame = CameraFrame {
            position: [100., 40., 80.],
            target: [100., 33., 100.],
            fov: 70.,
            preset: 0,
        };
        let view = chase_assisted(&frame, 16. / 9., Some((&sightline, actor.origin)), &height);
        let close = |a: f32, b: f64| (f64::from(a) - b).abs() < 1e-4;
        assert!(close(view.position[1], 33.88452916166587));
        assert!(close(view.position[2], 75.89265429297737));
        assert!(close(view.pitch, -0.03667481938672718));
        assert!(close(view.focal_length, 1.9041973423228193));
        let near = CrashSubjects {
            rider: [105., 31., 110.],
            bike: [112., 30.5, 104.],
        };
        let shot = crash(view, 16. / 9., &near, &height);
        assert!(close(shot.position[0], 108.42332671296408));
        assert!(close(shot.position[1], 34.247150632653614));
        assert!(close(shot.position[2], 91.45136077102681));
        assert!(close(shot.pitch, -0.22));
        let far = CrashSubjects {
            bike: [140., 30.5, 90.],
            ..near
        };
        let shot = crash(view, 9. / 16., &far, &height);
        assert!(close(shot.position[0], 105.));
        assert!(close(shot.position[1], 34.491673969293906));
        assert!(close(shot.position[2], 94.38564081071031));
    }

    #[test]
    fn speed_rush_starts_above_forty_mph() {
        let mut rush = SpeedRush::default();
        rush.update(40. * 5280. / 3600., true, 0.1);
        assert_eq!(rush.focal_scale(), 1.);
        for _ in 0..100 {
            rush.update(80. * 5280. / 3600., true, 0.05);
        }
        assert!((rush.focal_scale() - (1. - 0.09 * 0.75)).abs() < 1e-4);
    }
}
