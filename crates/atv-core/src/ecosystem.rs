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
    pub atlas: Option<BillboardAtlas>,
    pub optional_name: Option<String>,
    pub proxies: Vec<[u32; 9]>,
}
/// 002fe140 packs U-left/center/right and V-top/bottom for EcoSystemCode2D.
/// The center is authored independently; asymmetric trees retain their pivot.
#[derive(Debug, Serialize)]
pub struct BillboardAtlas {
    pub u: [f32; 3],
    pub v: [f32; 2],
    /// Horizontal offsets in units of the placement's half width.
    pub horizontal: [f32; 3],
}
impl BillboardAtlas {
    pub fn from_parameters(parameters: &[u32; 23]) -> Result<Option<Self>> {
        let f = parameters.map(f32::from_bits);
        let u = [f[6], f[10], f[7]];
        let v = [f[9], f[8]];
        // Shipped Tropical PlantA has no atlas rectangle. Do not invent one.
        if u == [0.; 3] && v == [0.; 2] {
            return Ok(None);
        }
        if !u.iter().chain(v.iter()).all(|x| x.is_finite())
            || !(u[0] < u[2] && u[0] <= u[1] && u[1] <= u[2] && v[0] < v[1])
        {
            return Err("invalid ESB billboard rectangle".into());
        }
        let extent = 2. / (u[2] - u[0]);
        Ok(Some(Self {
            u,
            v,
            horizontal: [-(extent * (u[1] - u[0])), 0., extent * (u[2] - u[1])],
        }))
    }
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
    /// Browser acceleration only:64² cells in original packed X/Z space.
    /// Stable source indices preserve asset identity and collision ordering.
    pub visibility_index: VisibilityIndex,
}
#[derive(Debug, Default, Serialize)]
pub struct VisibilityIndex {
    pub offsets: Vec<u32>,
    pub placements: Vec<u32>,
}
impl VisibilityIndex {
    pub fn build(placements: &[Placement]) -> Self {
        let cell = |p: &Placement| {
            usize::from(p.coordinates[2] >> 10) * 64 + usize::from(p.coordinates[0] >> 10)
        };
        let mut offsets = vec![0u32; 4097];
        for p in placements {
            offsets[cell(p) + 1] += 1;
        }
        for i in 1..offsets.len() {
            offsets[i] += offsets[i - 1];
        }
        let mut next = offsets[..4096].to_vec();
        let mut indices = vec![0; placements.len()];
        for (i, p) in placements.iter().enumerate() {
            let at = &mut next[cell(p)];
            indices[*at as usize] = i as u32;
            *at += 1;
        }
        Self {
            offsets,
            placements: indices,
        }
    }
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
    let mut species_by_slot = [None; 256];
    for (slot, species_index) in species_by_slot.iter_mut().enumerate() {
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
        *species_index = Some(species.len());
        let atlas = BillboardAtlas::from_parameters(&parameters)?;
        species.push(Species {
            slot: slot as u8,
            model,
            billboard,
            parameters,
            atlas,
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
        let species = &species[species_by_slot[usize::from(slot)]
            .ok_or("ESB placement references absent species")?];
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
    let visibility_index = VisibilityIndex::build(&placements);
    Ok(Ecosystem {
        species,
        placements,
        global_name,
        global_word,
        placement_texture,
        visibility_index,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_preserves_an_asymmetric_pivot_and_absent_rectangle() {
        let mut p = [0; 23];
        assert!(BillboardAtlas::from_parameters(&p).unwrap().is_none());
        p[6..11].copy_from_slice(&[0.25, 0.75, 0.9, 0.1, 0.375].map(f32::to_bits));
        let atlas = BillboardAtlas::from_parameters(&p).unwrap().unwrap();
        assert_eq!(atlas.u, [0.25, 0.375, 0.75]);
        assert_eq!(atlas.v, [0.1, 0.9]);
        assert_eq!(atlas.horizontal, [-0.5, 0., 1.5]);
        p[10] = 1f32.to_bits();
        assert!(BillboardAtlas::from_parameters(&p).is_err());
    }
    #[test]
    fn packed_visibility_index_preserves_all_source_indices_and_cell_boundaries() {
        let mut eco = decode(&sample()).unwrap();
        for coordinates in [
            [0, 0, 0],
            [1023, 0, 1023],
            [1024, 0, 1024],
            [65535, 0, 65535],
        ] {
            eco.placements.push(Placement {
                slot: 0,
                resample_height: false,
                coordinates,
                height: 1.,
                radius: 1.,
            });
        }
        let index = VisibilityIndex::build(&eco.placements);
        assert_eq!(&index.placements[..3], &[0, 1, 2]);
        assert_eq!(index.offsets[1], 3);
        assert_eq!(
            &index.placements[index.offsets[65] as usize..index.offsets[66] as usize],
            &[3]
        );
        assert_eq!(index.placements.last(), Some(&4));
        assert_eq!(*index.offsets.last().unwrap(), eco.placements.len() as u32);
    }
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
