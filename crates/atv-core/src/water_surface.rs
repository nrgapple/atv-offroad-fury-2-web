//! Water height composition recovered from exact-build 00199cd0.
//! The caller supplies the native animated displacement field; it must not
//! replace that field with a flat plane when integrating live contacts.
use crate::{Result, wheel::Vec3};

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub base_height: f32,
    /// The environment's authored base, before the current base swell.
    pub authored_base_height: f32,
    pub ripple_scale: f32,
    pub displacement_scale: f32,
    pub maximum_height_difference: f32,
    pub ripples_enabled: bool,
    pub infinite_plane: bool,
}

#[derive(Debug, Clone)]
pub struct RippleGrid {
    pub origin: [f32; 2],
    pub inverse_spacing: f32,
    pub dimension: usize,
    /// Signed heights, in logical row order. Original storage packs eight
    /// heights into the first sixteen bytes of each sixty-four-byte block.
    pub heights: Vec<i16>,
}
impl RippleGrid {
    fn sample(&self, position: Vec3) -> Result<f32> {
        if self.dimension == 0
            || self.dimension > 4096
            || self.heights.len() != self.dimension * self.dimension
            || !self.inverse_spacing.is_finite()
            || self.origin.iter().any(|v| !v.is_finite())
        {
            return Err("invalid water ripple grid".into());
        }
        let x = (self.inverse_spacing * (position[0] - self.origin[0])) as i32;
        let z = (self.inverse_spacing * (position[2] - self.origin[1])) as i32;
        if x < 0 || z < 0 || x as usize >= self.dimension || z as usize >= self.dimension {
            Ok(0.)
        } else {
            Ok(f32::from(
                self.heights[z as usize * self.dimension + x as usize],
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct BaseDisplacement {
    pub dimension: usize,
    pub inverse_spacing: f32,
    /// The supported native eight-bit source map. Lower-bit packed formats
    /// require their own recovery; 00199cd0 reads an addressed whole byte.
    pub heights: Vec<u8>,
}
impl BaseDisplacement {
    fn sample(&self, position: Vec3) -> Result<f32> {
        if self.dimension == 0
            || self.dimension > 4096
            || self.heights.len() != self.dimension * self.dimension
            || !self.inverse_spacing.is_finite()
        {
            return Err("invalid base water displacement".into());
        }
        let x = (position[0] * self.inverse_spacing) as i32 % self.dimension as i32;
        let z = (position[2] * self.inverse_spacing) as i32 % self.dimension as i32;
        // The original performs signed remainders and unchecked pointer reads.
        // Reject out-of-map negative coordinates instead of inventing wrapping.
        if x < 0 || z < 0 {
            return Err("negative base water displacement coordinates".into());
        }
        Ok(f32::from(
            self.heights[z as usize * self.dimension + x as usize],
        ))
    }
}

/// Execute the original composed query. `displaced_height` is the result of
/// water+e0's virtual field sampler (+7c), in original fixed-height units.
/// The caller should sample it at the same world X/Z. Returning zero only makes
/// sense for a native field that has actually been initialized to zero.
pub fn height(
    config: Config,
    position: Vec3,
    base_only: bool,
    ripple_grids: &[RippleGrid],
    base_displacement: Option<&BaseDisplacement>,
    mut displaced_height: impl FnMut(Vec3) -> Result<f32>,
) -> Result<f32> {
    if !position
        .iter()
        .chain(
            [
                config.base_height,
                config.authored_base_height,
                config.ripple_scale,
                config.displacement_scale,
                config.maximum_height_difference,
            ]
            .iter(),
        )
        .all(|value| value.is_finite())
    {
        return Err("nonfinite water height query".into());
    }
    let mut ripple = 0.;
    if config.ripples_enabled {
        for grid in ripple_grids {
            ripple += config.ripple_scale * grid.sample(position)?;
        }
    }
    let displacement = if let Some(map) = base_displacement {
        config.displacement_scale * map.sample(position)?
    } else {
        0.
    };
    if !config.infinite_plane && displacement == 0. {
        return Ok(0.);
    }
    let field = displaced_height(position)?;
    if !field.is_finite() {
        return Err("nonfinite animated water displacement".into());
    }
    let base = if displacement == 0. {
        config.base_height
    } else {
        config.authored_base_height
    };
    // Assembly sums ripple+base-map first, then adds the field sampler.
    let result = (field + (ripple + displacement)) * f32::from_bits(0x3980_0000) + base;
    if !result.is_finite() {
        return Err("water height overflow".into());
    }
    Ok(
        if base_only || result - position[1] > config.maximum_height_difference {
            config.base_height
        } else {
            result
        },
    )
}

/// One original signature-wave height channel from 00192090. Texture samples
/// are the raw bytes consumed by the native field, not decoded display colors.
/// Positive and negative height maps are separate waves; non-height channels
/// never contribute to this query.
#[derive(Debug, Clone)]
pub struct SignatureWave {
    pub origin: [f32; 2],
    pub width: [f32; 2],
    pub inverse_width: [f32; 2],
    pub dimension: usize,
    pub row_stride: usize,
    pub pixel_stride: usize,
    pub tiled: bool,
    pub height_channel: bool,
    pub positive: bool,
    pub scale: f32,
    pub first_frame: Vec<u8>,
    pub second_frame: Vec<u8>,
}

/// Native 00192090 sums each signed texture-frame channel separately, then
/// blends the sums in the original order. Integer coordinates wrap even for
/// non-tiled waves at their inclusive authored upper edge.
pub fn signature_height(
    waves: &[SignatureWave],
    position: [f32; 2],
    first_weight: f32,
    second_weight: f32,
) -> Result<f32> {
    if !position
        .iter()
        .chain([first_weight, second_weight].iter())
        .all(|v| v.is_finite())
    {
        return Err("nonfinite signature water query".into());
    }
    let mut first = 0.;
    let mut second = 0.;
    for wave in waves {
        if !wave.height_channel {
            continue;
        }
        if wave.dimension == 0
            || wave.dimension > 4096
            || wave.pixel_stride == 0
            || wave.pixel_stride > 16
            || wave.row_stride < wave.dimension * wave.pixel_stride
            || wave.row_stride > 65536
            || wave.first_frame.len() != wave.dimension * wave.row_stride
            || wave.second_frame.len() != wave.dimension * wave.row_stride
            || !wave
                .origin
                .iter()
                .chain(wave.width.iter())
                .chain(wave.inverse_width.iter())
                .chain([wave.scale].iter())
                .all(|v| v.is_finite())
        {
            return Err("invalid signature water field".into());
        }
        let offset = [position[0] - wave.origin[0], position[1] - wave.origin[1]];
        if !wave.tiled
            && (offset[0] < 0.
                || offset[0] > wave.width[0]
                || offset[1] < 0.
                || offset[1] > wave.width[1])
        {
            continue;
        }
        let coordinate = |axis: usize| {
            // Native multiplies dimension*offset before inverse width and
            // uses DIVU on the signed conversion's raw integer bits.
            let value = (wave.inverse_width[axis] * (wave.dimension as f32 * offset[axis])) as i32;
            (value as u32 % wave.dimension as u32) as usize
        };
        let index = coordinate(1) * wave.row_stride + wave.pixel_stride * coordinate(0);
        let a = wave.scale * f32::from(wave.first_frame[index]);
        let b = wave.scale * f32::from(wave.second_frame[index]);
        if wave.positive {
            first += a;
            second += b;
        } else {
            first -= a;
            second -= b;
        }
    }
    let result = second * second_weight + first * first_weight;
    if result.is_finite() {
        Ok(result)
    } else {
        Err("signature water height overflow".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            base_height: 10.,
            authored_base_height: 20.,
            ripple_scale: 16.,
            displacement_scale: 16.,
            maximum_height_difference: 100.,
            ripples_enabled: true,
            infinite_plane: true,
        }
    }
    #[test]
    fn combines_signed_ripples_field_and_authored_displacement() {
        let grid = RippleGrid {
            origin: [0.; 2],
            inverse_spacing: 1.,
            dimension: 8,
            heights: vec![-256; 64],
        };
        let map = BaseDisplacement {
            inverse_spacing: 1.,
            dimension: 8,
            heights: vec![128; 64],
        };
        assert_eq!(
            height(
                config(),
                [1., 15., 1.],
                false,
                std::slice::from_ref(&grid),
                None,
                |_| Ok(4096.)
            )
            .unwrap(),
            10.
        );
        assert_eq!(
            height(config(), [1., 15., 1.], false, &[grid], Some(&map), |_| Ok(
                4096.
            ))
            .unwrap(),
            20.5
        );
    }
    #[test]
    fn base_fallback_and_disabled_plane_do_not_invent_water() {
        let c = config();
        assert_eq!(
            height(c, [0., -91., 0.], false, &[], None, |_| Ok(0.)).unwrap(),
            10.
        );
        assert_eq!(
            height(c, [0., 0., 0.], true, &[], None, |_| Ok(4096.)).unwrap(),
            10.
        );
        let c = Config {
            infinite_plane: false,
            ..c
        };
        assert_eq!(
            height(c, [0.; 3], false, &[], None, |_| panic!(
                "native skips sampler"
            ))
            .unwrap(),
            0.
        );
    }
    #[test]
    fn signature_query_keeps_native_unsigned_wrap_and_inclusive_edge() {
        let wave = SignatureWave {
            origin: [0.; 2],
            width: [7.; 2],
            inverse_width: [1. / 7.; 2],
            dimension: 7,
            row_stride: 7,
            pixel_stride: 1,
            tiled: true,
            height_channel: true,
            positive: true,
            scale: 1.,
            first_frame: (0..49).collect(),
            second_frame: vec![0; 49],
        };
        // CVT.W of -1 followed by DIVU: 0xffffffff % 7 == 3.
        assert_eq!(
            signature_height(std::slice::from_ref(&wave), [-1., 0.], 1., 0.).unwrap(),
            3.
        );
        let wave = SignatureWave {
            tiled: false,
            ..wave
        };
        assert_eq!(
            signature_height(std::slice::from_ref(&wave), [7., 1.], 1., 0.).unwrap(),
            7.
        );
        assert_eq!(signature_height(&[wave], [7.1, 1.], 1., 0.).unwrap(), 0.);
    }
}
