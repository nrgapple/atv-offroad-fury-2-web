//! Browser start placement. These are authored port rules, not recovered
//! original routines: race grids and race resets sit 2.5 ft above the terrain
//! contact plane, and online riders start 8 ft apart by slot along the start's
//! right-hand axis. Scene starts otherwise keep their authored height.
use crate::{Result, terrain::Terrain};

pub const COURSE_CLEARANCE: f64 = 2.5;
pub const SLOT_SPACING: f64 = 8.;

pub fn start_origin(
    terrain: &Terrain,
    position: [f64; 3],
    yaw: f64,
    course: bool,
    slot: Option<u8>,
) -> Result<[f64; 3]> {
    if !position.iter().all(|v| v.is_finite()) || !yaw.is_finite() {
        return Err("nonfinite start pose".into());
    }
    let mut origin = position;
    if course {
        let ground = terrain
            .clamped_height(position[0] as f32, position[2] as f32)?
            .ok_or("course start lies outside the recovered terrain")?;
        origin[1] = f64::from(ground) + COURSE_CLEARANCE;
    }
    if let Some(slot) = slot {
        let offset = f64::from(slot) * SLOT_SPACING;
        origin[0] += yaw.cos() * offset;
        origin[2] -= yaw.sin() * offset;
    }
    Ok(origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn course_starts_sit_on_terrain_and_slots_step_right() {
        let terrain = crate::vegetation_render::tests::plane();
        // Former browser rule: ground + 2.5 ft, then slot * 8 ft along (cos, -sin).
        let ground = terrain.clamped_height(320., 448.).unwrap().unwrap();
        assert_eq!(ground, 22.);
        assert_eq!(
            start_origin(&terrain, [320., 99., 448.], 0., true, None).unwrap(),
            [320., 24.5, 448.]
        );
        assert_eq!(
            start_origin(&terrain, [320., 99., 448.], 0., false, None).unwrap(),
            [320., 99., 448.]
        );
        assert_eq!(
            start_origin(&terrain, [320., 99., 448.], 0., false, Some(2)).unwrap(),
            [336., 99., 448.]
        );
        let yaw = std::f64::consts::FRAC_PI_2;
        let turned = start_origin(&terrain, [320., 99., 448.], yaw, true, Some(3)).unwrap();
        assert_eq!(turned[1], 24.5);
        assert!((turned[0] - 320.).abs() < 1e-12 && (turned[2] - 424.).abs() < 1e-12);
        assert!(start_origin(&terrain, [f64::NAN, 0., 0.], 0., true, None).is_err());
    }
}
