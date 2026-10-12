//! Original ESB v2 vegetation species and packed placements,00128ca0/00129b70.
//! Raw proxy words are interpreted by vegetation_proxy and the scene consumers.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;

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
