//! Verified vegetation checkpoint exclusion from001ea050/00128090/001280b0.
//! Caller supplies candidates retained by the original spatial cache. Physical
//! mesh/sphere/box proxy contacts and vegetation rendering remain separate.
use crate::{Result, wheel::Vec3};

///00129640 sets the coordinate unit to the greatest terrain world extent/65536.
/// Resampled placement Y must already be quantized by the loader when used for
/// physical proxies; checkpoint exclusion ignores Y.
pub fn placement_position(coordinates: [u16; 3], coordinate_unit: f32) -> Result<Vec3> {
    if !coordinate_unit.is_finite() || coordinate_unit <= 0. {
        return Err("invalid vegetation coordinate unit".into());
    }
    Ok(coordinates.map(|value| f32::from(value) * coordinate_unit))
}

///0033e2e0: vegetation candidate distance uses the horizontal width radius and
/// ignores altitude. Cache refresh/reuse callers pass query_radius0.
pub fn horizontal_separation(
    query: Vec3,
    query_radius: f32,
    plant: Vec3,
    plant_radius: f32,
) -> Result<f32> {
    if query.iter().chain(plant.iter()).any(|v| !v.is_finite())
        || [query_radius, plant_radius]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err("invalid vegetation horizontal bounds".into());
    }
    let x = query[0] - plant[0];
    let z = query[2] - plant[2];
    Ok(((x * x + z * z).sqrt() - (query_radius + plant_radius)).max(0.))
}

/// A selected Vegetation node blocks a checkpoint within twenty horizontal
/// feet if its species has any configured collision proxy. Exact distance20
/// remains a candidate-consumer rejection; exact squared proximity400 blocks.
/// The cache's plant-width separation is a distinct supplied value.
pub fn blocks_checkpoint(
    candidate: Vec3,
    coordinates: [u16; 3],
    coordinate_unit: f32,
    proxy_count: usize,
    cached_separation: f32,
) -> Result<bool> {
    if !candidate.iter().all(|v| v.is_finite())
        || !cached_separation.is_finite()
        || cached_separation < 0.
    {
        return Err("invalid vegetation checkpoint candidate".into());
    }
    let position = placement_position(coordinates, coordinate_unit)?;
    if proxy_count == 0 || cached_separation >= 20. {
        return Ok(false);
    }
    let x = position[0] - candidate[0];
    let z = position[2] - candidate[2];
    Ok(z * z + x * x <= 400.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_exclusion_uses_proxy_count_and_distinct_strict_cache_gate() {
        let plant = [100, 900, 100];
        assert!(blocks_checkpoint([120., -1000., 100.], plant, 1., 1, 19.).unwrap());
        assert!(!blocks_checkpoint([120., -1000., 100.], plant, 1., 1, 20.).unwrap());
        assert!(!blocks_checkpoint([120., -1000., 100.], plant, 1., 0, 0.).unwrap());
        assert!(!blocks_checkpoint([120.01, 900., 100.], plant, 1., 1, 0.).unwrap());
        assert!(blocks_checkpoint([112., 900., 116.], plant, 1., 3, 0.).unwrap());
    }
    #[test]
    fn packed_coordinates_use_original_binary32_unit() {
        let unit = 5760. / 65536.;
        assert_eq!(
            placement_position([32768, 8192, 16384], unit).unwrap(),
            [2880., 720., 1440.]
        );
        assert!(placement_position([0; 3], 0.).is_err());
        assert!(placement_position([0; 3], f32::NAN).is_err());
    }
    #[test]
    fn horizontal_distance_uses_width_and_ignores_height() {
        assert_eq!(
            horizontal_separation([3., 1000., 4.], 1., [0., -1000., 0.], 2.).unwrap(),
            2.
        );
        assert_eq!(
            horizontal_separation([3., 1000., 4.], 3., [0., -1000., 0.], 2.).unwrap(),
            0.
        );
    }
}
