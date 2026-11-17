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

/// The live Free Ride main menu's backdrop (research/FREERIDE_MENU.md; a
/// port addition): a higher, wider chase that keeps the horizon, terrain
/// and the room's riders in the menus' stage while following the
/// autopilot's rider. The subject's height and the heading ease (no snap on
/// landings or turns), the eye rises over terrain between it and the rider,
/// and the follow camera blends in and out of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Backdrop {
    /// The backdrop's share of the view (0: the chase view); None until
    /// first updated, when an active backdrop starts at full share.
    blend: Option<f64>,
    /// The blend's linear progress (eased by smoothstep).
    progress: f64,
    /// The followed rider (its height eased) and heading (yaw, radians).
    focus: [f64; 3],
    heading: f64,
    /// The eye's lift over the framing for terrain (eased).
    lift: f64,
}

impl Backdrop {
    /// Eye height above, and distance behind, the rider (ft).
    const RISE: f64 = 14.5;
    const BACK: f64 = 34.;
    /// The aim: ahead of the rider, a little above it (ft).
    const AHEAD: f64 = 30.;
    const AIM: f64 = 3.;
    /// The view widens to this share of the chase's focal length.
    const WIDEN: f64 = 0.84;
    /// Time constants (s): the subject's height, the heading, the lift's
    /// rise and fall.
    const HEIGHT_EASE: f64 = 0.45;
    const HEADING_EASE: f64 = 1.1;
    const LIFT_RISE: f64 = 0.12;
    const LIFT_FALL: f64 = 0.9;
    /// Blending between the chase and the backdrop takes this long (s).
    const BLEND: f64 = 1.1;
    /// A rider that moves this far in a frame was reset: the framing jumps.
    const JUMP: f64 = 80.;
    /// The eye keeps this clearance over terrain (ft), along its sightline
    /// to the rider too.
    const CLEAR: f64 = 4.;

    /// The view while the menus show (`active`), the chase view `chase`
    /// otherwise, blended between them. `origin` is the presented rider;
    /// `height` the visible ground.
    pub fn update(
        &mut self,
        active: bool,
        chase: View,
        origin: [f32; 3],
        dt: f32,
        height: &dyn Fn(f32, f32) -> f32,
    ) -> View {
        let dt = f64::from(dt).clamp(0., 0.25);
        let origin = origin.map(f64::from);
        let ease = |tau: f64| 1. - (-dt / tau).exp_d();
        let fresh = self.blend.is_none();
        let reset = (origin[0] - self.focus[0]).hypot_d(origin[2] - self.focus[2]) > Self::JUMP
            || !self.focus.iter().all(|v| v.is_finite());
        if fresh && !active {
            return chase;
        }
        if fresh || reset {
            self.focus = origin;
            self.heading = f64::from(chase.yaw);
            self.lift = 0.;
        }
        if fresh {
            self.blend = Some(1.);
            self.progress = 1.;
        }
        // The rider's ground position exactly; its height and the heading ease.
        self.focus[0] = origin[0];
        self.focus[2] = origin[2];
        self.focus[1] += (origin[1] - self.focus[1]) * ease(Self::HEIGHT_EASE);
        let turn = wrap(f64::from(chase.yaw) - self.heading);
        self.heading = wrap(self.heading + turn * ease(Self::HEADING_EASE));
        self.progress = (self.progress + if active { dt } else { -dt } / Self::BLEND).clamp(0., 1.);
        let blend = self.progress * self.progress * (3. - 2. * self.progress);
        self.blend = Some(blend);
        if blend <= 0. {
            return chase;
        }
        let (sx, cz) = self.heading.sin_cos_d();
        let ground = |x: f64, z: f64| f64::from(height(x as f32, z as f32));
        let focus = self.focus;
        let mut eye = [
            focus[0] - sx * Self::BACK,
            focus[1] + Self::RISE,
            focus[2] - cz * Self::BACK,
        ];
        // The eye rises until its sightline to the rider clears the ground.
        let mut need: f64 = 0.;
        for t in [0.15, 0.25, 0.35, 0.5, 0.65, 0.8, 1.] {
            let (x, z) = (
                focus[0] - sx * Self::BACK * t,
                focus[2] - cz * Self::BACK * t,
            );
            let g = ground(x, z);
            if g.is_finite() {
                let floor = focus[1] + Self::AIM + (g + Self::CLEAR - focus[1] - Self::AIM) / t;
                need = need.max(floor - eye[1]);
            }
        }
        let tau = if need > self.lift {
            Self::LIFT_RISE
        } else {
            Self::LIFT_FALL
        };
        self.lift += (need - self.lift) * ease(tau);
        eye[1] += self.lift;
        let aim = [
            focus[0] + sx * Self::AHEAD,
            focus[1] + Self::AIM,
            focus[2] + cz * Self::AHEAD,
        ];
        let look: [f64; 3] = std::array::from_fn(|i| aim[i] - eye[i]);
        let pitch = look[1].atan2_d(look[0].hypot_d(look[2]));
        let mix = |a: f64, b: f64| a + (b - a) * blend;
        let mut position: [f64; 3] =
            std::array::from_fn(|i| mix(f64::from(chase.position[i]), eye[i]));
        // Never under the ground, whatever the blend passes over.
        let floor = ground(position[0], position[2]) + 2.;
        if floor.is_finite() && position[1] < floor {
            position[1] = floor;
        }
        let yaw = f64::from(chase.yaw) + wrap(self.heading - f64::from(chase.yaw)) * blend;
        View {
            position: position.map(|v| v as f32),
            yaw: yaw as f32,
            pitch: mix(f64::from(chase.pitch), pitch) as f32,
            focal_length: mix(
                f64::from(chase.focal_length),
                f64::from(chase.focal_length) * Self::WIDEN,
            ) as f32,
        }
    }

    /// The backdrop's share of the view (tests, diagnostics).
    pub fn blend(&self) -> f64 {
        self.blend.unwrap_or(0.)
    }
}

/// An angle in -π..π.
fn wrap(angle: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    (angle + PI).rem_euclid(TAU) - PI
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

/// Options → Accessibility → Field of view (authored): the limits and
/// step of the rider's choice, in percent of the original angle.
pub const FOV_RANGE: (u8, u8, u8) = (85, 125, 5);

/// `focal` (the 4:3 frame's focal length, any speed-rush widening already
/// in it) with its horizontal angle scaled to `percent` of itself. 100
/// returns `focal` untouched, so the original view is bit-identical; the
/// half angle stays below 80 degrees whatever the inputs.
pub fn fov_focal(focal: f32, percent: u8) -> f32 {
    use atv_core::math::Det;
    if percent == 100 || focal.is_nan() || focal <= 0. {
        return focal;
    }
    let percent = f32::from(percent.clamp(FOV_RANGE.0, FOV_RANGE.1)) / 100.;
    let half = ((4. / 3.) / focal).atan_d() * percent;
    (4. / 3.) / half.min(80f32.to_radians()).tan_d()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_backdrop_rises_eases_and_clears_terrain() {
        let chase = |x: f32, yaw: f32| View {
            position: [x, 4., -12.],
            yaw,
            pitch: -0.1,
            focal_length: 1.45,
        };
        let flat = |_: f32, _: f32| 0.;
        let mut backdrop = Backdrop::default();
        // An inactive backdrop leaves the chase alone.
        assert_eq!(
            backdrop.update(false, chase(0., 0.), [0.; 3], 1. / 60., &flat),
            chase(0., 0.)
        );
        // First shown under the menus it frames at once: high, behind,
        // looking down a little, wider.
        let view = backdrop.update(true, chase(0., 0.), [0., 6., 0.], 1. / 60., &flat);
        assert!(
            view.position[1] > 14. && view.position[2] < -30.,
            "{view:?}"
        );
        assert!(
            view.pitch < -0.1 && view.pitch > -0.35,
            "horizon in view: {view:?}"
        );
        assert!(view.focal_length < 1.45);
        // A sharp turn of the chase and a landing ease in: no snap.
        let mut last = view;
        for frame in 0..120 {
            let landing = if frame < 2 { 6. } else { 0. };
            let view = backdrop.update(true, chase(0., 1.2), [0., landing, 0.], 1. / 60., &flat);
            assert!(
                (view.yaw - last.yaw).abs() < 0.04,
                "frame {frame}: {view:?}"
            );
            assert!(
                (view.position[1] - last.position[1]).abs() < 0.4,
                "frame {frame}"
            );
            last = view;
        }
        // A ridge between the eye and the rider lifts the eye over it.
        let ridge = |_: f32, z: f32| 26. * (1. - ((z + 25.) / 12.).powi(2)).max(0.);
        let mut backdrop = Backdrop::default();
        let mut view = chase(0., 0.);
        for _ in 0..120 {
            view = backdrop.update(true, chase(0., 0.), [0.; 3], 1. / 60., &ridge);
        }
        let eye = view.position;
        for t in [0.35f32, 0.5, 0.65, 0.8] {
            let (y, z) = (3. + (eye[1] - 3.) * t, eye[2] * t);
            assert!(
                y > ridge(0., z),
                "sightline clears the ridge at {t}: {eye:?}"
            );
        }
        // Hop in: the chase comes back over the blend, then exactly.
        let mut previous = view;
        for _ in 0..90 {
            let view = backdrop.update(false, chase(0., 0.), [0.; 3], 1. / 60., &ridge);
            assert!(
                (view.position[1] - previous.position[1]).abs() < 2.,
                "{view:?}"
            );
            previous = view;
        }
        assert_eq!(
            backdrop.update(false, chase(0., 0.), [0.; 3], 1. / 60., &ridge),
            chase(0., 0.)
        );
        assert_eq!(backdrop.blend(), 0.);
    }

    #[test]
    fn field_of_view_scales_the_angle_and_leaves_the_original_alone() {
        let focal = 1.6;
        assert_eq!(fov_focal(focal, 100), focal);
        let angle = |f: f32| 2. * atv_core::math::Det::atan_d((4. / 3.) / f);
        for percent in [85u8, 90, 110, 125] {
            let scaled = fov_focal(focal, percent);
            let want = angle(focal) * f32::from(percent) / 100.;
            assert!((angle(scaled) - want).abs() < 1e-5, "{percent}%");
        }
        // Wider sees more: a shorter focal length.
        assert!(fov_focal(focal, 125) < focal && fov_focal(focal, 85) > focal);
        // Out-of-range values clamp; a very wide authored lens stays finite.
        assert_eq!(fov_focal(focal, 200), fov_focal(focal, 125));
        assert!(fov_focal(0.3, 125).is_finite() && fov_focal(0.3, 125) > 0.);
        // Speed rush composes: the same rush on a wider view is still wider.
        assert!(fov_focal(focal * 0.91, 110) < fov_focal(focal, 110));
    }

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
        let half = 0.25f64.sin_cos_d();
        let actor = SightlineActor {
            origin: [100., 30., 100.],
            orientation: [half.1 as f32, 0., half.0 as f32, 0.],
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
