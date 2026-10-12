//! Buoyancy contact kernel recovered from USA v2.00 0033da90/0033dd90.
//! Callers supply the original animated points and displaced water heights.
use crate::{
    Result,
    dynamics::BodyState,
    wheel::{Vec3, add, scale, sub},
};

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
}
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub maximum_depth: f32,
    pub high_speed: Spring,
    pub low_speed: Spring,
    pub speed_threshold: f32,
    pub drag: f32,
}
impl Config {
    /// Each spring's stiffness is mass * gravity * its tuning multiplier;
    /// damping is twice the critical coefficient times its damping ratio.
    pub fn new(
        mass: f32,
        maximum_depth: f32,
        high: [f32; 2],
        low: [f32; 2],
        speed_threshold: f32,
        drag: f32,
    ) -> Result<Self> {
        if ![
            mass,
            maximum_depth,
            high[0],
            high[1],
            low[0],
            low[1],
            speed_threshold,
            drag,
        ]
        .iter()
        .all(|v| v.is_finite())
            || mass <= 0.
            || maximum_depth < 0.
        {
            return Err("invalid water contact configuration".into());
        }
        let spring = |values: [f32; 2]| {
            let stiffness = (values[0] * mass * f32::from_bits(0x4200cccd)).abs();
            let damping = if stiffness > 0. {
                values[1] * 2. * (mass * stiffness).sqrt()
            } else {
                0.
            };
            Spring { stiffness, damping }
        };
        Ok(Self {
            maximum_depth,
            high_speed: spring(high),
            low_speed: spring(low),
            speed_threshold,
            drag,
        })
    }
    /// Rider constructor001ee090; the ATV supplies its own PHY tuning.
    pub fn rider(mass: f32) -> Result<Self> {
        Self::new(
            mass,
            1.,
            [5., 0.1],
            [0.4, 0.3],
            f32::from_bits(0x426aaa65),
            -5.,
        )
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ContactResult {
    pub touching: bool,
    pub points: usize,
}

/// Selects one complete point list before resolving its contacts, then applies
/// translational drag once if any point is submerged. There is no restitution
/// impulse or positional projection in the original water response.
/// `height` may update the original ripple field between successive points;
/// `ripple` receives its native strength at each penetrating world point.
#[allow(clippy::too_many_arguments)]
pub fn apply_contacts(
    body: &mut BodyState,
    config: Config,
    speed: f32,
    high_points: &[Vec3],
    low_points: &[Vec3],
    flow: Vec3,
    up: Vec3,
    mut height: impl FnMut(Vec3) -> Result<f32>,
    mut ripple: impl FnMut(Vec3, f32),
) -> Result<ContactResult> {
    if !speed.is_finite()
        || !flow
            .iter()
            .chain(up.iter())
            .chain(high_points.iter().flatten())
            .chain(low_points.iter().flatten())
            .all(|v| v.is_finite())
    {
        return Err("nonfinite water contact input".into());
    }
    let (points, spring) = if speed > config.speed_threshold && !high_points.is_empty() {
        (high_points, config.high_speed)
    } else {
        (low_points, config.low_speed)
    };
    let mut result = ContactResult::default();
    for &point in points {
        let surface = height(point)?;
        if !surface.is_finite() {
            return Err("nonfinite water height".into());
        }
        let depth = (surface - point[1]).min(config.maximum_depth);
        if depth > 0. {
            result.touching = true;
            result.points += 1;
            let velocity = sub(body.velocity_at(point), flow);
            ripple(point, (velocity[1] * 0.02).clamp(-0.2, 0.2));
            let force = spring.stiffness * depth - spring.damping * velocity[1];
            body.apply_world_force(scale(up, force), point);
        }
    }
    if result.touching {
        body.force = add(
            body.force,
            scale(sub(body.linear_velocity, flow), config.drag),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamics::BodyConfig;

    #[test]
    fn submerged_points_match_original_force_and_torque_checkpoint() {
        let config = BodyConfig::new(4., [1.; 3], [0.; 3]).unwrap();
        let mut body = BodyState::new([0.; 3], [1., 0., 0., 0.], &config).unwrap();
        body.linear_velocity = [2., -3., 4.];
        let points = [[1., -0.5, 0.], [-1., -2., 1.], [0., 1., 0.], [0.; 3]];
        let mut ripples = Vec::new();
        let result = apply_contacts(
            &mut body,
            Config::rider(4.).unwrap(),
            0.,
            &[],
            &points,
            [0.; 3],
            [0., 1., 0.],
            |_| Ok(0.),
            |point, strength| ripples.push((point, strength)),
        )
        .unwrap();
        assert!(result.touching);
        assert_eq!(result.points, 2);
        assert_eq!(
            body.force.map(f32::to_bits),
            [0xc1200000, 0x430ff5b2, 0xc1a00000]
        );
        assert_eq!(
            body.local_torque.map(f32::to_bits),
            [0xc29ab842, 0, 0xc1ce147c]
        );
        assert_eq!(ripples.len(), 2);
        assert_eq!(ripples[0].1, -3. * 0.02);
    }

    #[test]
    fn high_speed_selects_a_complete_list_and_dry_contact_adds_no_drag() {
        let config = BodyConfig::new(4., [1.; 3], [0.; 3]).unwrap();
        let water = Config::rider(4.).unwrap();
        for (speed, high, touching) in [
            (water.speed_threshold, vec![[0., -1., 0.]], false),
            (water.speed_threshold + 1., vec![[0., -1., 0.]], true),
            (water.speed_threshold + 1., vec![], false),
        ] {
            let mut body = BodyState::new([0.; 3], [1., 0., 0., 0.], &config).unwrap();
            body.linear_velocity = [20., 0., 0.];
            let result = apply_contacts(
                &mut body,
                water,
                speed,
                &high,
                &[[0., 1., 0.]],
                [0.; 3],
                [0., 1., 0.],
                |_| Ok(0.),
                |_, _| {},
            )
            .unwrap();
            assert_eq!(result.touching, touching);
            assert_eq!(body.force[0], if touching { -100. } else { 0. });
        }
    }
}
