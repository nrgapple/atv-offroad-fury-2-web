//! Garage vehicle figures from original ATV physics files.
//!
//! The per-quad Engine/Traction/Suspension values shown on the original ATV
//! select page come from `uiQuad.txt` (read by the frontend). The physics
//! files add the authored top speed and gearbox. Their `[Physical
//! Properties]` ratings and Weight are an identical template on all 26 quads
//! (0.45/0.7/1.0/0.6, 525), so they are not presented as stats.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stats {
    /// Authored `[Engine] TopSpeed`, miles per hour (the HUD unit).
    pub top_speed: f32,
    pub gears: usize,
}

pub fn from_physics(physics: &PhysicsData) -> Result<Stats> {
    let top_speed = physics.first_scalar("Engine", "TopSpeed")?;
    if !(1.0..=250.0).contains(&top_speed) {
        return Err("authored top speed is out of range".into());
    }
    Ok(Stats {
        top_speed,
        gears: physics.gear_ratios()?.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a privately extracted ATV physics file"]
    fn original_honda_250ex_figures() {
        let path = std::env::var("ATV_PHYSICS").expect("set ATV_PHYSICS");
        let physics = PhysicsData::parse(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            from_physics(&physics).unwrap(),
            Stats {
                top_speed: 55.,
                gears: 5
            }
        );
    }
}
