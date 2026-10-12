//! Verified vegetation checkpoint exclusion from001ea050/00128090/001280b0.
//! Caller supplies candidates retained by the original spatial cache. Physical
//! mesh/sphere/box proxy contacts and vegetation rendering remain separate.
use crate::{Result, scene_partition::Bounds, wheel::Vec3};

///00129640 sets the coordinate unit to the greatest partition extent/65536.
/// Resampled placement Y must already be quantized by the loader when used for
/// physical proxies; checkpoint exclusion ignores Y.
pub fn placement_position(coordinates: [u16; 3], coordinate_unit: f32) -> Result<Vec3> {
    if !coordinate_unit.is_finite() || coordinate_unit <= 0. {
        return Err("invalid vegetation coordinate unit".into());
    }
    Ok(coordinates.map(|value| f32::from(value) * coordinate_unit))
}

///00129b70 re-quantizes sampled Y with the separately rounded65536/extent,
/// truncates to signed32, then retains the low unsigned16 bits.
pub fn resampled_coordinates(
    mut coordinates: [u16; 3],
    sampled_height: f32,
    coordinate_reciprocal: f32,
) -> Result<[u16; 3]> {
    let quantized = sampled_height * coordinate_reciprocal;
    if !sampled_height.is_finite()
        || !coordinate_reciprocal.is_finite()
        || coordinate_reciprocal <= 0.
        || !(-2_147_483_648. ..2_147_483_648.).contains(&quantized)
    {
        return Err("vegetation height cannot be quantized to native signed32".into());
    }
    coordinates[1] = quantized as i32 as u16;
    Ok(coordinates)
}

///00129b70 registers all placements, including species without proxies.
/// Height and radius are the original signed-byte species interpolation outputs.
pub fn placement_bounds(
    coordinates: [u16; 3],
    coordinate_unit: f32,
    height: f32,
    radius: f32,
) -> Result<Bounds> {
    if !height.is_finite() || height <= 0. || !radius.is_finite() || radius <= 0. {
        return Err("invalid vegetation placement bounds".into());
    }
    let position = placement_position(coordinates, coordinate_unit)?;
    let half_height = height * 0.5;
    Ok(Bounds {
        center: [position[0], half_height + position[1], position[2]],
        half: [radius, half_height, radius],
    })
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
    #[test]
    fn resampling_truncates_then_wraps_and_bounds_use_quantized_y() {
        let coordinates = resampled_coordinates([1, 999, 3], -2., 0.5).unwrap();
        assert_eq!(coordinates, [1, 65535, 3]);
        let bounds = placement_bounds(coordinates, 2., 10., 4.).unwrap();
        assert_eq!(bounds.center, [2., 131075., 6.]);
        assert_eq!(bounds.half, [4., 5., 4.]);
        assert_eq!(resampled_coordinates([1, 999, 3], -1., 0.5).unwrap()[1], 0);
        assert!(resampled_coordinates([0; 3], f32::INFINITY, 1.).is_err());
        assert!(resampled_coordinates([0; 3], 2_147_483_648., 1.).is_err());
    }
}
