//! Authored equal-mass multiplayer proxy contacts, separate from recovered PS2
//! scenery collision. Each owner applies only its half of the pair response.
use crate::wheel::{Vec3, dot, sub};
#[derive(Debug, Clone, Copy)]
pub struct Response {
    pub displacement: Vec3,
    pub velocity: Vec3,
}
pub fn resolve(
    own: Vec3,
    velocity: Vec3,
    other: Vec3,
    other_velocity: Vec3,
    dt: f32,
    tie: f32,
) -> Option<Response> {
    if !own
        .iter()
        .chain(&velocity)
        .chain(&other)
        .chain(&other_velocity)
        .all(|v| v.is_finite())
        || !(0. ..=0.1).contains(&dt)
    {
        return None;
    }
    let delta = sub(own, other);
    let relative = sub(velocity, other_velocity);
    let speed2 = relative[0] * relative[0] + relative[2] * relative[2];
    let closest = if speed2 > 0.0001 {
        (-(delta[0] * relative[0] + delta[2] * relative[2]) / speed2).clamp(0., dt)
    } else {
        0.
    };
    if (delta[1] + relative[1] * closest).abs() > 3.5 {
        return None;
    }
    let future = [
        delta[0] + relative[0] * closest,
        0.,
        delta[2] + relative[2] * closest,
    ];
    if dot(future, future) >= 36. {
        return None;
    }
    let distance = delta[0].hypot(delta[2]);
    let normal = if distance > 0.001 {
        [delta[0] / distance, 0., delta[2] / distance]
    } else {
        [tie.signum(), 0., 0.]
    };
    let closing = dot(relative, normal);
    let impulse = if closing < 0. {
        (-closing * 0.55).min(100.)
    } else {
        0.
    };
    let penetration = ((6. - distance).max(0.) * 0.5).min(1.5);
    if impulse == 0. && penetration < 0.001 {
        return None;
    }
    Some(Response {
        displacement: normal.map(|n| n * penetration),
        velocity: std::array::from_fn(|i| velocity[i] + normal[i] * impulse),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn head_on_impulses_are_opposite_and_swept() {
        let a = resolve(
            [0., 0., 0.],
            [0., 0., 100.],
            [0., 0., 8.],
            [0., 0., -100.],
            1. / 60.,
            1.,
        )
        .unwrap();
        let b = resolve(
            [0., 0., 8.],
            [0., 0., -100.],
            [0., 0., 0.],
            [0., 0., 100.],
            1. / 60.,
            -1.,
        )
        .unwrap();
        assert!(a.velocity[2] <= 0.);
        assert_eq!(a.velocity[2], -b.velocity[2]);
    }
    #[test]
    fn overpass_and_separating_riders_do_not_bounce() {
        assert!(resolve([0., 5., 0.], [0.; 3], [0.; 3], [0.; 3], 1. / 60., 1.).is_none());
        let r = resolve([0., 0., 5.], [0., 0., 5.], [0.; 3], [0.; 3], 1. / 60., 1.).unwrap();
        assert_eq!(r.velocity, [0., 0., 5.]);
        assert!(r.displacement[2] > 0.);
    }
    #[test]
    fn overlap_tie_break_is_symmetric_and_bounded() {
        let a = resolve([0.; 3], [0.; 3], [0.; 3], [0.; 3], 1. / 60., 1.).unwrap();
        let b = resolve([0.; 3], [0.; 3], [0.; 3], [0.; 3], 1. / 60., -1.).unwrap();
        assert_eq!(a.displacement[0], -b.displacement[0]);
        assert_eq!(a.displacement[0], 1.5);
    }
}
