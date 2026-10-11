//! Bounded single-image IPUM decoding for observed flags-zero MPEG-2 intra data.
//! Host floating IDCT is not a claim of bit-identical PS2 IPU arithmetic.
use crate::{Result, slice, u16le, u32le};
use serde::Serialize;
#[path = "ipu_tables.rs"]
mod tables;
use tables::{AC, QUANT, ZIGZAG};

#[derive(Debug, Serialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
}
#[derive(Debug)]
pub struct Image {
    pub info: ImageInfo,
    pub rgba: Vec<u8>,
    pub yuv420: Vec<u8>,
}
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn take(&mut self, count: usize) -> Result<u32> {
        if count > 32 || self.at + count > self.bytes.len() * 8 {
            return Err("truncated IPUM bitstream".into());
        }
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | u32::from((self.bytes[self.at / 8] >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(value)
    }
    fn dc(&mut self, chroma: bool) -> Result<i32> {
        const LUMA: [(u16, u8); 12] = [
            (4, 3),
            (0, 2),
            (1, 2),
            (5, 3),
            (6, 3),
            (14, 4),
            (30, 5),
            (62, 6),
            (126, 7),
            (254, 8),
            (510, 9),
            (511, 9),
        ];
        const CHROMA: [(u16, u8); 12] = [
            (0, 2),
            (1, 2),
            (2, 2),
            (6, 3),
            (14, 4),
            (30, 5),
            (62, 6),
            (126, 7),
            (254, 8),
            (510, 9),
            (1022, 10),
            (1023, 10),
        ];
        let mut code = 0;
        let table = if chroma { &CHROMA } else { &LUMA };
        for length in 1..=10 {
            code = (code << 1) | self.take(1)? as u16;
            if let Some(size) = table
                .iter()
                .position(|&(c, n)| c == code && usize::from(n) == length)
            {
                if size == 0 {
                    return Ok(0);
                }
                let value = self.take(size)? as i32;
                return Ok(if value & (1 << (size - 1)) == 0 {
                    value + 1 - (1 << size)
                } else {
                    value
                });
            }
        }
        Err("invalid IPUM DC VLC".into())
    }
    fn ac(&mut self) -> Result<u16> {
        let mut code = 0;
        for length in 1..=16 {
            code = (code << 1) | self.take(1)? as u16;
            if let Some(&(_, _, value)) = AC.iter().find(|&&(c, n, _)| c == code && n == length) {
                return Ok(value);
            }
        }
        Err("invalid IPUM AC VLC".into())
    }
}
fn block(bits: &mut Bits<'_>, predictor: &mut i32, chroma: bool, quant: i32) -> Result<[i32; 64]> {
    let mut values = [0; 64];
    *predictor += bits.dc(chroma)?;
    if !(-2048..=2047).contains(predictor) {
        return Err("IPUM DC predictor outside range".into());
    }
    values[0] = *predictor * 8;
    let mut position = 0usize;
    for _ in 0..64 {
        let word = bits.ac()?;
        if word == 1 && bits.take(1)? == 0 {
            let parity = values.iter().fold(0, |sum, v| sum ^ v) & 1;
            values[63] ^= parity ^ 1;
            return Ok(values);
        }
        let (run, level) = if word == 65535 {
            let run = bits.take(6)? as usize;
            let raw = bits.take(12)? as i32;
            let level = if raw & 2048 != 0 { raw - 4096 } else { raw };
            if level == 0 || level == -2048 {
                return Err("invalid IPUM escape level".into());
            }
            (run, level)
        } else {
            let sign = bits.take(1)?;
            (
                usize::from(word >> 8),
                i32::from(word & 255) * if sign == 0 { 1 } else { -1 },
            )
        };
        position += run + 1;
        if position >= 64 {
            return Err("IPUM AC run exceeds block".into());
        }
        let index = ZIGZAG[position];
        let magnitude = (level.abs() * quant * QUANT[index] as i32) >> 4;
        values[index] = (if level < 0 { -magnitude } else { magnitude }).clamp(-2048, 2047);
    }
    Err("IPUM block missing end marker".into())
}
fn idct(coefficients: &[i32; 64], basis: &[[f64; 8]; 8]) -> [u8; 64] {
    let mut intermediate = [[0.; 8]; 8];
    for v in 0..8 {
        for x in 0..8 {
            intermediate[v][x] = (0..8)
                .map(|u| basis[x][u] * f64::from(coefficients[v * 8 + u]))
                .sum();
        }
    }
    std::array::from_fn(|i| {
        let x = i % 8;
        let y = i / 8;
        ((0..8)
            .map(|v| basis[y][v] * intermediate[v][x])
            .sum::<f64>()
            * 0.25)
            .round()
            .clamp(0., 255.) as u8
    })
}
pub fn decode_ipum(data: &[u8]) -> Result<Image> {
    if data.len() > 16 * 1024 * 1024 || slice(data, 0, 4)? != b"ipum" {
        return Err("invalid IPUM signature/size".into());
    }
    if u32le(data, 4)? as usize + 8 != data.len() {
        return Err("IPUM extent mismatch".into());
    }
    let width = u16le(data, 8)? as usize;
    let height = u16le(data, 10)? as usize;
    if width == 0
        || height == 0
        || width > 2048
        || height > 2048
        || !width.is_multiple_of(16)
        || !height.is_multiple_of(16)
        || width * height > 4 * 1024 * 1024
    {
        return Err("unsupported IPUM dimensions".into());
    }
    if u32le(data, 12)? != 1 {
        return Err("IPUM decoder accepts one frame".into());
    }
    let mut bits = Bits {
        bytes: &data[16..],
        at: 0,
    };
    if bits.take(8)? != 0 {
        return Err("unsupported IPUM coding flags".into());
    }
    let basis: [[f64; 8]; 8] = std::array::from_fn(|x| {
        std::array::from_fn(|u| {
            (((2 * x + 1) * u) as f64 * std::f64::consts::PI / 16.).cos()
                * if u == 0 {
                    std::f64::consts::FRAC_1_SQRT_2
                } else {
                    1.
                }
        })
    });
    let luma_size = width * height;
    let chroma_size = luma_size / 4;
    let mut yuv = vec![0; luma_size + chroma_size * 2];
    let mut predictors = [128; 3];
    let mut quant = 1;
    for y in (0..height).step_by(16) {
        for x in (0..width).step_by(16) {
            if (x != 0 || y != 0) && bits.take(1)? != 1 {
                return Err("invalid IPUM macroblock increment".into());
            }
            if bits.take(1)? == 0 {
                if bits.take(1)? != 1 {
                    return Err("invalid IPUM macroblock type".into());
                }
                quant = bits.take(5)? as i32 * 2;
                if quant == 0 {
                    return Err("invalid zero IPUM quantizer".into());
                }
            }
            for n in 0..6 {
                let component = if n < 4 { 0 } else { n - 3 };
                let coefficients = block(&mut bits, &mut predictors[component], n >= 4, quant)?;
                let pixels = idct(&coefficients, &basis);
                let (plane, stride, bx, by) = if n < 4 {
                    (0, width, x + (n % 2) * 8, y + (n / 2) * 8)
                } else {
                    (luma_size + (n - 4) * chroma_size, width / 2, x / 2, y / 2)
                };
                for row in 0..8 {
                    let start = plane + (by + row) * stride + bx;
                    yuv[start..start + 8].copy_from_slice(&pixels[row * 8..row * 8 + 8]);
                }
            }
        }
    }
    // Frames end on a byte boundary, then IPU frame-end and sequence-end codes.
    if !bits.at.is_multiple_of(8) {
        let pad = 8 - bits.at % 8;
        if bits.take(pad)? != 0 {
            return Err("nonzero IPUM byte padding".into());
        }
    }
    if bits.take(32)? != 0x000001b0
        || bits.take(32)? != 0x000001b1
        || bits.at != bits.bytes.len() * 8
    {
        return Err("invalid IPUM end markers/extent".into());
    }
    let mut rgba = vec![0; luma_size * 4];
    for y in 0..height {
        for x in 0..width {
            let at = y * width + x;
            let uv = y / 2 * (width / 2) + x / 2;
            let luma = (i32::from(yuv[at]) - 16).max(0);
            let cb = i32::from(yuv[luma_size + uv]) - 128;
            let cr = i32::from(yuv[luma_size + chroma_size + uv]) - 128;
            let rgb = [
                298 * luma + 409 * cr,
                298 * luma - 100 * cb - 208 * cr,
                298 * luma + 516 * cb,
            ];
            for c in 0..3 {
                rgba[at * 4 + c] = ((rgb[c] + 128) >> 8).clamp(0, 255) as u8;
            }
            rgba[at * 4 + 3] = 255;
        }
    }
    Ok(Image {
        info: ImageInfo {
            width: width as u32,
            height: height as u32,
        },
        rgba,
        yuv420: yuv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gray_image() -> Vec<u8> {
        let mut bits = String::from("000000001"); // flags0; intra MB, retain quantizer
        for _ in 0..4 {
            bits.push_str("10010");
        } // zero luminance DC delta + AC EOB
        for _ in 0..2 {
            bits.push_str("0010");
        } // zero chrominance DC delta + AC EOB
        while bits.len() % 8 != 0 {
            bits.push('0');
        }
        let payload: Vec<u8> = bits
            .as_bytes()
            .chunks(8)
            .map(|b| b.iter().fold(0, |v, &c| (v << 1) | (c - b'0')))
            .collect();
        let mut data = b"ipum".to_vec();
        data.extend(((payload.len() + 16) as u32).to_le_bytes());
        data.extend(16u16.to_le_bytes());
        data.extend(16u16.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        data.extend(payload);
        data.extend([0, 0, 1, 0xb0, 0, 0, 1, 0xb1]);
        data
    }
    #[test]
    fn synthetic_neutral_macroblock_and_all_truncations() {
        let data = gray_image();
        let img = decode_ipum(&data).unwrap();
        assert_eq!((img.info.width, img.info.height), (16, 16));
        assert_eq!(img.yuv420, vec![128; 384]);
        assert!(
            img.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|v| *v == [130, 130, 130, 255])
        );
        for end in 0..data.len() {
            assert!(decode_ipum(&data[..end]).is_err());
        }
    }
    #[test]
    fn flags_dimensions_and_end_codes_fail_explicitly() {
        let mut data = gray_image();
        data[16] = 0x80;
        assert!(decode_ipum(&data).unwrap_err().contains("flags"));
        data[16] = 0;
        data[8] = 17;
        assert!(decode_ipum(&data).unwrap_err().contains("dimensions"));
        data[8] = 16;
        let end = data.len() - 1;
        data[end] = 0xb2;
        assert!(decode_ipum(&data).unwrap_err().contains("end markers"));
    }
}
