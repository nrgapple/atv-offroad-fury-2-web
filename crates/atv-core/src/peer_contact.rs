//! Authored equal-mass multiplayer proxy contacts, separate from recovered PS2
//! scenery collision. Each owner applies only its half of the pair response.
//!
//! Each quad is an upright box: a yaw-oriented footprint (from the body's
//! heading) extruded along world Y. The recovered quad data does not expose a
//! cheap rider-inclusive hull, so the extents below are authored constants.
use crate::dynamics::basis;
use crate::wheel::{Vec3, add, dot, scale, sub};

/// Half track width, lateral (body X) feet. A quad is about 4 ft wide.
pub const HALF_WIDTH: f32 = 2.0;
/// Half length, longitudinal (body Z) feet. A quad is about 6–6.5 ft long.
pub const HALF_LENGTH: f32 = 3.2;
/// Box bottom below the body origin (the chassis reference, roughly hub height).
pub const BELOW_ORIGIN: f32 = 1.0;
/// Box top above the body origin: quad plus seated rider, about 4.8 ft total.
pub const ABOVE_ORIGIN: f32 = 3.8;
/// Full box height; origins farther apart vertically never interact.
pub const HEIGHT: f32 = BELOW_ORIGIN + ABOVE_ORIGIN;
/// Side, front and rear restitution.
pub const RESTITUTION: f32 = 0.25;
/// Bounce when one rider lands on top of the other.
pub const LANDING_RESTITUTION: f32 = 0.3;
/// Tangential Coulomb coefficient for the small contact friction term.
pub const FRICTION: f32 = 0.15;
/// Largest yaw-rate change, rad/s, from one contact.
pub const MAX_YAW_RATE_CHANGE: f32 = 4.0;
/// Yaw inertia per unit mass of the footprint box: (w² + l²) / 12, ft².
pub const YAW_INERTIA: f32 =
    ((2. * HALF_WIDTH) * (2. * HALF_WIDTH) + (2. * HALF_LENGTH) * (2. * HALF_LENGTH)) / 12.;
/// Share of the landing impulse given to the grounded lower rider.
const LOWER_SHARE: f32 = 0.2;
const MAX_IMPULSE: f32 = 100.;
const MAX_STEP: f32 = 1.;
const MAX_STEPS: usize = 8;
const EPSILON: f32 = 1e-4;

#[derive(Debug, Clone, Copy)]
pub struct Body {
    /// `BodyState::origin`, world feet.
    pub origin: Vec3,
    /// World linear velocity, feet per second.
    pub velocity: Vec3,
    /// Orientation quaternion, W,X,Y,Z like `BodyState::orientation`.
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct Response {
    pub displacement: Vec3,
    pub velocity: Vec3,
    /// World-Y angular velocity change, rad/s.
    pub yaw_rate_change: f32,
}

#[derive(Clone, Copy)]
struct Footprint {
    right: Vec3,
    forward: Vec3,
}
impl Footprint {
    fn new(rotation: [f32; 4]) -> Option<Self> {
        let length = rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !length.is_finite() || length < 1e-6 {
            return None;
        }
        let b = basis(rotation.map(|v| v / length));
        let (f, r) = (b[2], b[0]);
        let f_len = f[0].hypot(f[2]);
        let r_len = r[0].hypot(r[2]);
        // A pitched-vertical quad still has a horizontal right axis.
        let forward = if f_len >= r_len {
            [f[0] / f_len, 0., f[2] / f_len]
        } else {
            [-r[2] / r_len, 0., r[0] / r_len]
        };
        Some(Self {
            right: [forward[2], 0., -forward[0]],
            forward,
        })
    }
    fn radius(&self, axis: Vec3) -> f32 {
        HALF_WIDTH * dot(axis, self.right).abs() + HALF_LENGTH * dot(axis, self.forward).abs()
    }
}

/// Minimum-penetration SAT axis (pointing from other toward own) and depth.
fn separate(own: &Footprint, other: &Footprint, delta: Vec3, tie: f32) -> Option<(Vec3, f32)> {
    let vertical = HEIGHT - delta[1].abs();
    if vertical <= 0. {
        return None;
    }
    let sign = |d: f32| if d.abs() > EPSILON { d.signum() } else { tie };
    // Both owners scan the tie-positive rider's axes first, so exact ties
    // resolve to the same world axis on each side.
    let (first, second) = if tie > 0. { (own, other) } else { (other, own) };
    let mut best = ([0., sign(delta[1]), 0.], vertical);
    for axis in [first.right, first.forward, second.right, second.forward] {
        let d = dot(delta, axis);
        let depth = own.radius(axis) + other.radius(axis) - d.abs();
        if depth <= 0. {
            return None;
        }
        if depth <= best.1 {
            best = (scale(axis, sign(d)), depth);
        }
    }
    Some(best)
}

/// Midpoint of the two shadows on `axis`, relative to the own origin.
fn shadow_mid(axis: Vec3, own: f32, other: f32, delta: Vec3) -> f32 {
    let d = -dot(delta, axis);
    ((-own).max(d - other) + own.min(d + other)) * 0.5
}

/// Yaw component of `r × v` for horizontal vectors.
fn yaw_cross(r: Vec3, v: Vec3) -> f32 {
    r[2] * v[0] - r[0] * v[2]
}

pub fn resolve(own: Body, other: Body, dt: f32, tie: f32) -> Option<Response> {
    if !own
        .origin
        .iter()
        .chain(&own.velocity)
        .chain(&own.rotation)
        .chain(&other.origin)
        .chain(&other.velocity)
        .chain(&other.rotation)
        .all(|v| v.is_finite())
        || !tie.is_finite()
        || !(0. ..=0.1).contains(&dt)
    {
        return None;
    }
    let tie = if tie < 0. { -1. } else { 1. };
    let a = Footprint::new(own.rotation)?;
    let b = Footprint::new(other.rotation)?;
    let start = sub(own.origin, other.origin);
    let relative = sub(own.velocity, other.velocity);
    let travel = scale(relative, dt);
    let length = dot(travel, travel).sqrt();
    if !length.is_finite() {
        return None;
    }
    let steps = ((length / MAX_STEP).ceil() as usize).clamp(1, MAX_STEPS);
    // Earliest overlapping sample of the relative sweep; zero is the present.
    let (step, delta, normal, depth) = (0..=steps).find_map(|k| {
        let delta = add(start, scale(travel, k as f32 / steps as f32));
        separate(&a, &b, delta, tie).map(|(n, depth)| (k, delta, n, depth))
    })?;
    let vertical = normal[1] != 0.;
    // Contact point relative to own origin: centre of the shadow overlap.
    let point = if vertical {
        let x = shadow_mid(
            [1., 0., 0.],
            a.radius([1., 0., 0.]),
            b.radius([1., 0., 0.]),
            delta,
        );
        let z = shadow_mid(
            [0., 0., 1.],
            a.radius([0., 0., 1.]),
            b.radius([0., 0., 1.]),
            delta,
        );
        [x, 0., z]
    } else {
        let tangent = [-normal[2], 0., normal[0]];
        let n = shadow_mid(normal, a.radius(normal), b.radius(normal), delta);
        let t = shadow_mid(tangent, a.radius(tangent), b.radius(tangent), delta);
        add(scale(normal, n), scale(tangent, t))
    };
    let other_point = add(point, delta);
    let closing = dot(relative, normal);
    let upper = vertical && normal[1] > 0.;
    let impulse = if closing >= 0. {
        0.
    } else if vertical {
        // The lower rider is grounded, so it acts as a floor for the upper one.
        let full = -(1. + LANDING_RESTITUTION) * closing;
        (if upper { full } else { full * LOWER_SHARE }).min(MAX_IMPULSE)
    } else {
        // Equal unit masses plus yaw inertia about each footprint centre.
        let arm = yaw_cross(point, normal).powi(2) + yaw_cross(other_point, normal).powi(2);
        let effective = 2. + arm / YAW_INERTIA;
        (-(1. + RESTITUTION) * closing / effective).min(MAX_IMPULSE)
    };
    let mut change = scale(normal, impulse);
    let slip = sub(relative, scale(normal, closing));
    let slip = [slip[0], 0., slip[2]];
    let slip_speed = dot(slip, slip).sqrt();
    if slip_speed > EPSILON && impulse > 0. {
        let friction = (FRICTION * impulse).min(slip_speed * 0.5);
        change = add(change, scale(slip, -friction / slip_speed));
    }
    let push = if step > 0 {
        0.
    } else if !vertical {
        (depth * 0.5).min(1.5)
    } else if upper {
        (depth * 0.9).min(2.)
    } else {
        (depth * 0.1).min(0.25)
    };
    if impulse == 0. && push < 0.001 {
        return None;
    }
    let yaw_rate_change =
        (yaw_cross(point, change) / YAW_INERTIA).clamp(-MAX_YAW_RATE_CHANGE, MAX_YAW_RATE_CHANGE);
    Some(Response {
        displacement: scale(normal, push),
        velocity: add(own.velocity, change),
        yaw_rate_change,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1. / 60.;
    const IDENTITY: [f32; 4] = [1., 0., 0., 0.];
    fn yawed(yaw: f32) -> [f32; 4] {
        [(yaw * 0.5).cos(), 0., (yaw * 0.5).sin(), 0.]
    }
    fn body(origin: Vec3, velocity: Vec3, rotation: [f32; 4]) -> Body {
        Body {
            origin,
            velocity,
            rotation,
        }
    }
    fn pair(a: Body, b: Body, dt: f32) -> (Option<Response>, Option<Response>) {
        (resolve(a, b, dt, 1.), resolve(b, a, dt, -1.))
    }

    #[test]
    fn head_on_impulses_are_opposite_and_swept() {
        let a = body([0.; 3], [0., 0., 60.], IDENTITY);
        let b = body([0., 0., 8.], [0., 0., -60.], IDENTITY);
        let (ra, rb) = pair(a, b, DT);
        let (ra, rb) = (ra.unwrap(), rb.unwrap());
        let da = ra.velocity[2] - a.velocity[2];
        let db = rb.velocity[2] - b.velocity[2];
        assert!(da < 0. && db > 0.);
        assert!((da + db).abs() < 1e-3, "{da} {db}");
        // Restitution 0.25 with equal masses: each loses (1 + e) / 2 of closing.
        assert!((da + 0.625 * 120.).abs() < 1e-2, "{da}");
        assert!(ra.yaw_rate_change.abs() < 1e-4 && rb.yaw_rate_change.abs() < 1e-4);
    }

    #[test]
    fn side_by_side_riders_touch_only_when_footprints_overlap() {
        for yaw in [0., 1.1] {
            let q = yawed(yaw);
            let lateral = Footprint::new(q).unwrap().right;
            let a = body([10., 2., 10.], [0.; 3], q);
            let apart = body(add(a.origin, scale(lateral, 4.8)), [0.; 3], q);
            assert!(resolve(a, apart, DT, 1.).is_none());
            let close = body(add(a.origin, scale(lateral, 3.5)), [0.; 3], q);
            let r = resolve(a, close, DT, 1.).unwrap();
            // Pushed half of the 0.5 ft overlap away from the neighbour.
            assert!((dot(r.displacement, lateral) + 0.25).abs() < 1e-4);
            assert!(dot(r.displacement, r.displacement).sqrt() < 0.2501);
        }
    }

    #[test]
    fn t_bone_spins_the_struck_rider() {
        // A heads +X into B's left side, ahead of B's centre.
        let a = body(
            [-4.7, 0., 1.5],
            [30., 0., 0.],
            yawed(std::f32::consts::FRAC_PI_2),
        );
        let b = body([0.; 3], [0.; 3], IDENTITY);
        let (ra, rb) = pair(a, b, DT);
        let (ra, rb) = (ra.unwrap(), rb.unwrap());
        assert!(ra.velocity[0] < 30. && rb.velocity[0] > 0.);
        assert!((ra.velocity[0] - 30. + rb.velocity[0]).abs() < 1e-3);
        // Struck at the front from the left: B's nose swings toward +X (positive yaw).
        assert!(rb.yaw_rate_change > 1., "{}", rb.yaw_rate_change);
        assert!(rb.yaw_rate_change <= MAX_YAW_RATE_CHANGE);
        // A hits nose-first, so it barely rotates.
        assert!(ra.yaw_rate_change.abs() < rb.yaw_rate_change.abs() * 0.25);
        // Mirrored offset reverses the spin.
        let mirrored = body([-4.7, 0., -1.5], [30., 0., 0.], a.rotation);
        assert!(resolve(b, mirrored, DT, -1.).unwrap().yaw_rate_change < -1.);
    }

    #[test]
    fn landing_on_a_rider_bounces_the_upper_one() {
        let a = body([0.3, 4.3, 0.5], [5., -20., 0.], yawed(0.4));
        let b = body([0.; 3], [0.; 3], IDENTITY);
        let (ra, rb) = pair(a, b, DT);
        let (ra, rb) = (ra.unwrap(), rb.unwrap());
        assert!(ra.velocity[1] > 0., "{:?}", ra.velocity);
        assert!((ra.velocity[1] - 20. * LANDING_RESTITUTION).abs() < 1e-3);
        assert!(ra.displacement[1] > 0.4);
        assert!(ra.velocity[0] < 5. && ra.velocity[0] > 0.);
        assert!(rb.velocity[1] < 0. && rb.velocity[1] > -10.);
        assert!(rb.displacement[1] < 0. && rb.displacement[1] > -0.26);
    }

    #[test]
    fn overpass_and_separating_riders_do_not_bounce() {
        let low = body([0.; 3], [0., 0., 30.], IDENTITY);
        let high = body([0., 12., 0.], [0., 0., -30.], IDENTITY);
        assert!(resolve(low, high, DT, 1.).is_none());
        assert!(resolve(high, low, DT, -1.).is_none());
        assert!(resolve(body([0., 5., 0.], [0.; 3], IDENTITY), low, DT, 1.).is_none());
        let r = resolve(
            body([0., 0., 5.], [0., 0., 5.], IDENTITY),
            body([0.; 3], [0.; 3], IDENTITY),
            DT,
            1.,
        )
        .unwrap();
        assert_eq!(r.velocity, [0., 0., 5.]);
        assert!(r.displacement[2] > 0.);
    }

    #[test]
    fn sweep_catches_fast_pass_through() {
        let a = body([0., 0., -7.], [0., 0., 70.], IDENTITY);
        let b = body([0.3, 0., 0.], [0., 0., -70.], IDENTITY);
        let dt = 0.1;
        // Neither frame endpoint overlaps.
        let end = |p: Body| add(p.origin, scale(p.velocity, dt));
        assert!(sub(end(a), end(b))[2] > 2. * HALF_LENGTH);
        let (ra, rb) = pair(a, b, dt);
        let (ra, rb) = (ra.unwrap(), rb.unwrap());
        assert!(ra.velocity[2] < 0. && rb.velocity[2] > 0.);
        assert_eq!(ra.displacement, [0.; 3]);
    }

    #[test]
    fn overlap_tie_break_is_symmetric_and_bounded() {
        let a = body([0.; 3], [0.; 3], yawed(0.7));
        let (ra, rb) = pair(a, a, DT);
        let (ra, rb) = (ra.unwrap(), rb.unwrap());
        assert_eq!(ra.displacement, rb.displacement.map(|v| -v));
        assert!((dot(ra.displacement, ra.displacement).sqrt() - 1.5).abs() < 1e-5);
        assert_eq!(ra.displacement[1], 0.);
    }

    #[test]
    fn rejects_invalid_input() {
        let a = body([0.; 3], [0.; 3], IDENTITY);
        assert!(resolve(a, a, 0.2, 1.).is_none());
        assert!(resolve(a, a, -0.01, 1.).is_none());
        assert!(resolve(body([f32::NAN, 0., 0.], [0.; 3], IDENTITY), a, DT, 1.).is_none());
        assert!(resolve(body([0.; 3], [0.; 3], [0.; 4]), a, DT, 1.).is_none());
        assert!(
            resolve(
                a,
                body([0.; 3], [0.; 3], [f32::INFINITY, 0., 0., 0.]),
                DT,
                1.
            )
            .is_none()
        );
    }
}
