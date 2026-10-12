//! Original ESB v2 vegetation species and packed placements,00128ca0/00129b70.
//! Raw proxy words are interpreted by vegetation_proxy and the scene consumers.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

/// Frame0013a550 supplies global Sun matrices; near packet002fe710 replaces
/// their ambient column and alpha64. Point lights/LOD transitions are separate.
#[derive(Debug, Serialize)]
pub struct Lighting {
    pub ambient: [f32; 3],
    pub sunlight: [f32; 3],
    pub direction: [f32; 3],
    pub sun_present: bool,
}
impl Lighting {
    pub fn prepare(ambient: [f32; 3], sun: Option<([f32; 3], [f32; 3])>) -> Result<Self> {
        let valid_color = |v: [f32; 3]| v.iter().all(|v| v.is_finite() && (0.0..=255.).contains(v));
        if !valid_color(ambient) || sun.is_some_and(|(color, _)| !valid_color(color)) {
            return Err("unsupported vegetation lighting color".into());
        }
        let (sunlight, direction) = if let Some((color, look)) = sun {
            //00138570: three rounded squares, (x²+y²)+z², then VRSQRT.
            let squared = (look[0] * look[0] + look[1] * look[1]) + look[2] * look[2];
            if !look.iter().all(|v| v.is_finite()) || !squared.is_finite() {
                return Err("unsupported vegetation sunlight direction".into());
            }
            //00137ca0 caches a zero look but initializes direction to +X.
            //00138570 skips the setter when a supplied zero equals that cache.
            let direction = if look == [0.; 3] {
                [-1., -0., -0.]
            } else {
                if squared <= 0. {
                    return Err("unsupported vegetation sunlight direction".into());
                }
                let reciprocal = (1.0 / f64::from(squared).sqrt()) as f32;
                look.map(|v| -(v * reciprocal))
            };
            (color.map(|v| v * 0.5), direction)
        } else {
            ([0.; 3], [0.; 3])
        };
        Ok(Self {
            ambient: ambient.map(|v| v * 0.5),
            sunlight,
            direction,
            sun_present: sun.is_some(),
        })
    }

    /// Finite binary32 reference for the compressed diffuse arithmetic window.
    /// Normal is authored (not normalized); scale and Y rotation are per instance.
    pub fn near_color(&self, normal: [f32; 3], scale: [f32; 2], rotation: [f32; 2]) -> [u8; 4] {
        let [c, s] = rotation;
        let [x, y, z] = self.direction;
        let local = [
            (c * scale[0]) * x + (s * scale[0]) * z,
            y * scale[1],
            (-s * scale[0]) * x + (c * scale[0]) * z,
        ];
        let weight = ((local[0] * normal[0] + local[1] * normal[1]) + local[2] * normal[2]).max(0.);
        std::array::from_fn(|i| {
            let ambient = if i == 3 { 64. } else { self.ambient[i] };
            let color = if i == 3 {
                f32::from(self.sun_present)
            } else {
                self.sunlight[i]
            };
            (((8388608f32 + ambient) + color * weight)
                .min(8388863.)
                .to_bits()
                & 255) as u8
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Species {
    pub slot: u8,
    pub model: String,
    pub billboard: String,
    pub parameters: [u32; 23],
    pub optional_name: Option<String>,
    pub proxies: Vec<[u32; 9]>,
}
impl Species {
    /// 00127d20/00127d60 signed-byte interpolation. Width is a half extent.
    pub fn dimensions(&self, height: i8, width: i8) -> [f32; 2] {
        let f = self.parameters.map(f32::from_bits);
        [
            f[0] + f32::from(height) * (f[0] - f[1]) / 128.,
            f[3] + f32::from(width) * (f[5] - f[3]) / 128.,
        ]
    }
}
#[derive(Debug, Serialize)]
pub struct Placement {
    pub slot: u8,
    pub resample_height: bool,
    pub coordinates: [u16; 3],
    pub height: f32,
    pub radius: f32,
}
#[derive(Debug, Serialize)]
pub struct Ecosystem {
    pub species: Vec<Species>,
    pub placements: Vec<Placement>,
    pub global_name: Option<String>,
    pub global_word: u32,
    pub placement_texture: Option<String>,
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn byte(&mut self) -> Result<u8> {
        let v = slice(self.bytes, self.at, 1)?[0];
        self.at += 1;
        Ok(v)
    }
    fn word(&mut self) -> Result<u32> {
        let v = u32le(self.bytes, self.at)?;
        self.at += 4;
        Ok(v)
    }
    fn string(&mut self) -> Result<String> {
        let length = self.byte()? as usize;
        let bytes = slice(self.bytes, self.at, length)?;
        self.at += length;
        let bytes = bytes
            .strip_suffix(&[0])
            .ok_or("ESB string lacks terminator")?;
        if bytes.contains(&0) || bytes.iter().any(|b| *b < 32) {
            return Err("invalid ESB string".into());
        }
        Ok(bytes.iter().map(|b| char::from(*b)).collect())
    }
    fn optional(&mut self) -> Result<Option<String>> {
        if self.byte()? != 0 {
            Ok(Some(self.string()?))
        } else {
            Ok(None)
        }
    }
}
pub fn decode(bytes: &[u8]) -> Result<Ecosystem> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("ESB exceeds size limit".into());
    }
    let mut r = Reader { bytes, at: 0 };
    if r.word()? != 2 {
        return Err("unsupported ESB version".into());
    }
    let mut species = Vec::new();
    for slot in 0..256 {
        if r.byte()? == 0 {
            continue;
        }
        let model = r.string()?;
        let billboard = r.string()?;
        let mut parameters = [0; 23];
        for v in &mut parameters {
            *v = r.word()?;
        }
        if parameters[..6]
            .iter()
            .any(|v| !f32::from_bits(*v).is_finite() || f32::from_bits(*v) <= 0.)
        {
            return Err("invalid ESB dimensions".into());
        }
        let optional_name = r.optional()?;
        let count = r.word()? as usize;
        if count > 4096 {
            return Err("ESB proxy count exceeds limit".into());
        }
        let mut proxies = Vec::with_capacity(count);
        for _ in 0..count {
            let mut p = [0; 9];
            for v in &mut p {
                *v = r.word()?;
            }
            proxies.push(p);
        }
        species.push(Species {
            slot: slot as u8,
            model,
            billboard,
            parameters,
            optional_name,
            proxies,
        });
    }
    let global_name = r.optional()?;
    let global_word = r.word()?;
    let placement_texture = r.optional()?;
    let count = r.word()? as usize;
    if count > 1_000_000 || r.at.checked_add(count * 9) != Some(bytes.len()) {
        return Err("ESB placement extent mismatch".into());
    }
    let mut placements = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = r.byte()?;
        let slot = tag & 127;
        let coordinates = [
            u16le(bytes, r.at)?,
            u16le(bytes, r.at + 2)?,
            u16le(bytes, r.at + 4)?,
        ];
        r.at += 6;
        let height = r.byte()? as i8;
        let width = r.byte()? as i8;
        let species = species
            .iter()
            .find(|s| s.slot == slot)
            .ok_or("ESB placement references absent species")?;
        let [height, radius] = species.dimensions(height, width);
        if !height.is_finite() || !radius.is_finite() || height <= 0. || radius <= 0. {
            return Err("invalid ESB placement dimensions".into());
        }
        placements.push(Placement {
            slot,
            resample_height: tag & 128 != 0,
            coordinates,
            height,
            radius,
        });
    }
    Ok(Ecosystem {
        species,
        placements,
        global_name,
        global_word,
        placement_texture,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn near_packet_preserves_bias_rounding_sunlight_scaling_and_alpha() {
        let color = |ambient| {
            Lighting::prepare(ambient, None)
                .unwrap()
                .near_color([0., 1., 0.], [1.; 2], [1., 0.])
        };
        assert_eq!(color([79., 80., 92.]), [40, 40, 46, 64]);
        assert_eq!(color([1., 3., 255.]), [0, 2, 128, 64]);
        let sun = Lighting::prepare([100.; 3], Some(([128.; 3], [0., -10., 0.]))).unwrap();
        assert_eq!(
            sun.near_color([0., 1., 0.], [1., 2.], [1., 0.]),
            [178, 178, 178, 66]
        );
        assert_eq!(
            sun.near_color([0., -1., 0.], [1., 2.], [1., 0.]),
            [50, 50, 50, 64]
        );
        assert!(Lighting::prepare([f32::NAN, 0., 0.], None).is_err());
        assert_eq!(
            Lighting::prepare([100.; 3], Some(([128.; 3], [0.; 3])))
                .unwrap()
                .direction,
            [-1., -0., -0.]
        );
    }
    fn sample() -> Vec<u8> {
        let mut b = 2u32.to_le_bytes().to_vec();
        for slot in 0..256 {
            b.push(u8::from(slot == 1));
            if slot != 1 {
                continue;
            }
            b.extend_from_slice(&[2, b'A', 0, 2, b'B', 0]);
            for f in [10f32, 8., 12., 2., 1., 3.] {
                b.extend_from_slice(&f.to_bits().to_le_bytes());
            }
            b.extend_from_slice(&[0; 17 * 4]);
            b.push(0);
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.push(0);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.push(0);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&[129, 1, 0, 2, 0, 3, 0, 64, 192]);
        b
    }
    #[test]
    fn original_packing_and_signed_scaling() {
        let e = decode(&sample()).unwrap();
        assert_eq!(e.placements[0].coordinates, [1, 2, 3]);
        assert!(e.placements[0].resample_height);
        assert_eq!(e.placements[0].height, 11.);
        assert_eq!(e.placements[0].radius, 1.5);
    }
    #[test]
    fn reject_truncation_missing_species_and_trailing_data() {
        let b = sample();
        for end in 0..b.len() {
            assert!(decode(&b[..end]).is_err());
        }
        let mut b = b;
        b.push(0);
        assert!(decode(&b).is_err());
        b.pop();
        let at = b.len() - 9;
        b[at] = 3;
        assert!(decode(&b).is_err());
    }
}
