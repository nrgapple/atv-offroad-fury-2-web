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
    /// Quantizer scale code (1..=31) of each 16x16 macroblock, row-major.
    pub quantizers: Vec<u8>,
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
    let mut quantizers = Vec::with_capacity(luma_size / 256);
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
            quantizers.push((quant / 2).max(1) as u8);
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
    let rgba = to_rgba(&yuv, width, height, |x, y| {
        let uv = y / 2 * (width / 2) + x / 2;
        (yuv[luma_size + uv], yuv[luma_size + chroma_size + uv])
    });
    Ok(Image {
        info: ImageInfo {
            width: width as u32,
            height: height as u32,
        },
        rgba,
        yuv420: yuv,
        quantizers,
    })
}
const DENOISE_RANGE: f32 = 1.6;
/// Bounded integer BT.601 conversion of the luma plane with sampled chroma.
fn to_rgba(
    yuv: &[u8],
    width: usize,
    height: usize,
    chroma: impl Fn(usize, usize) -> (u8, u8),
) -> Vec<u8> {
    let mut rgba = vec![0; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let at = y * width + x;
            let (cb, cr) = chroma(x, y);
            let luma = (i32::from(yuv[at]) - 16).max(0);
            let cb = i32::from(cb) - 128;
            let cr = i32::from(cr) - 128;
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
    rgba
}

impl Image {
    /// Authored presentation enhancement, not PS2 output. Removes visible 8x8
    /// DCT block edges with the MPEG-4 Annex F post-filter, scaled by each
    /// macroblock's quantizer, then upsamples 4:2:0 chroma with centred
    /// triangle weights instead of repeating each chroma sample over 2x2 pixels.
    pub fn smoothed_rgba(&self) -> Vec<u8> {
        let (width, height) = (self.info.width as usize, self.info.height as usize);
        let (luma_size, chroma_size) = (width * height, width * height / 4);
        let columns = width / 16;
        let mut yuv = self.yuv420.clone();
        let (luma, chroma) = yuv.split_at_mut(luma_size);
        let luma_quantizer = |x: usize, y: usize| self.quantizers[y / 16 * columns + x / 16];
        let chroma_quantizer = |x: usize, y: usize| self.quantizers[y / 8 * columns + x / 8];
        deblock(luma, width, height, luma_quantizer);
        denoise(luma, width, height, 2, luma_quantizer);
        for plane in chroma.chunks_mut(chroma_size) {
            deblock(plane, width / 2, height / 2, chroma_quantizer);
            denoise(plane, width / 2, height / 2, 1, chroma_quantizer);
        }
        let (cw, ch) = (width / 2, height / 2);
        let sample = |plane: usize, x: usize, y: usize| {
            u32::from(yuv[luma_size + plane * chroma_size + y * cw + x])
        };
        to_rgba(&yuv, width, height, |x, y| {
            // Chroma sits between luma pairs: weight the nearest sample 3/4
            // and the next one away 1/4 on each axis.
            let (cx, cy) = (x / 2, y / 2);
            let nx = if x % 2 == 0 {
                cx.saturating_sub(1)
            } else {
                (cx + 1).min(cw - 1)
            };
            let ny = if y % 2 == 0 {
                cy.saturating_sub(1)
            } else {
                (cy + 1).min(ch - 1)
            };
            let blend = |p| {
                ((9 * sample(p, cx, cy)
                    + 3 * sample(p, nx, cy)
                    + 3 * sample(p, cx, ny)
                    + sample(p, nx, ny)
                    + 8)
                    / 16) as u8
            };
            (blend(0), blend(1))
        })
    }
}

/// Edge-preserving (bilateral) smoothing of quantization noise. The range
/// weight falls off over a few quantizer steps, so low-amplitude ringing and
/// speckle inside blocks flatten while cloud edges and horizons keep their
/// contrast.
fn denoise(
    plane: &mut [u8],
    width: usize,
    height: usize,
    radius: isize,
    quantizer: impl Fn(usize, usize) -> u8,
) {
    let source = plane.to_vec();
    let spatial_scale = 2. * (radius as f32 * 0.75).powi(2);
    let spatial: Vec<f32> = (-radius..=radius)
        .flat_map(|dy| (-radius..=radius).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| (-((dx * dx + dy * dy) as f32) / spatial_scale).exp())
        .collect();
    // Range weights per absolute difference, rebuilt only when the quantizer changes.
    let mut range = [0f32; 256];
    let mut range_quantizer = 0;
    let side = 2 * radius + 1;
    for y in 0..height {
        for x in 0..width {
            let q = quantizer(x, y);
            if q != range_quantizer {
                let scale = 2. * (DENOISE_RANGE * f32::from(q)).powi(2);
                for (difference, weight) in range.iter_mut().enumerate() {
                    *weight = (-((difference * difference) as f32) / scale).exp();
                }
                range_quantizer = q;
            }
            let centre = source[y * width + x];
            let (mut sum, mut total) = (0., 0.);
            for dy in -radius..=radius {
                let sy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                for dx in -radius..=radius {
                    let sx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    let value = source[sy * width + sx];
                    let weight = spatial[((dy + radius) * side + dx + radius) as usize]
                        * range[usize::from(value.abs_diff(centre))];
                    sum += weight * f32::from(value);
                    total += weight;
                }
            }
            plane[y * width + x] = (sum / total).round() as u8;
        }
    }
}

/// Filters every interior 8-pixel block edge of one plane: vertical edges
/// first, then horizontal, each using the quantizer of the block after the edge.
fn deblock(plane: &mut [u8], width: usize, height: usize, quantizer: impl Fn(usize, usize) -> u8) {
    for y in 0..height {
        for x in (8..width).step_by(8) {
            let at = y * width + x - 5;
            filter_edge(plane, at, 1, quantizer(x, y));
        }
    }
    for y in (8..height).step_by(8) {
        for x in 0..width {
            let at = (y - 5) * width + x;
            filter_edge(plane, at, width, quantizer(x, y));
        }
    }
}

/// MPEG-4 Annex F deblocking of ten samples v0..v9 straddling an edge between
/// v4 and v5. Flat runs get a 9-tap low-pass; detailed runs only have the
/// step at the edge reduced, so real cloud and ridge edges survive.
fn filter_edge(plane: &mut [u8], start: usize, stride: usize, quantizer: u8) {
    let qp = i32::from(quantizer);
    let mut v: [i32; 10] = std::array::from_fn(|i| i32::from(plane[start + i * stride]));
    let flat = v.windows(2).filter(|p| (p[0] - p[1]).abs() <= 2).count();
    if flat >= 6 {
        let (min, max) = v[1..9]
            .iter()
            .fold((255, 0), |(lo, hi), &s| (s.min(lo), s.max(hi)));
        if max - min >= 2 * qp {
            return;
        }
        let left = if (v[1] - v[0]).abs() < qp { v[0] } else { v[1] };
        let right = if (v[8] - v[9]).abs() < qp { v[9] } else { v[8] };
        let padded = |m: isize| match m {
            ..1 => left,
            9.. => right,
            _ => v[m as usize],
        };
        const TAPS: [i32; 9] = [1, 1, 2, 2, 4, 2, 2, 1, 1];
        let out: [i32; 8] = std::array::from_fn(|n| {
            let sum: i32 = TAPS
                .iter()
                .zip(-4..=4)
                .map(|(t, k)| t * padded(n as isize + 1 + k))
                .sum();
            (sum + 8) >> 4
        });
        v[1..9].copy_from_slice(&out);
    } else {
        let energy = |a: i32, b: i32, c: i32, d: i32| (2 * a - 5 * b + 5 * c - 2 * d) / 8;
        let a0 = energy(v[3], v[4], v[5], v[6]);
        if a0.abs() >= qp {
            return;
        }
        let a1 = energy(v[1], v[2], v[3], v[4]);
        let a2 = energy(v[5], v[6], v[7], v[8]);
        let corrected = a0.signum() * a0.abs().min(a1.abs()).min(a2.abs());
        let limit = (v[4] - v[5]) / 2;
        let d = (5 * (corrected - a0) / 8).clamp(limit.min(0), limit.max(0));
        v[4] -= d;
        v[5] += d;
    }
    for (i, s) in v.iter().enumerate() {
        plane[start + i * stride] = *s as u8;
    }
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
    fn smoothing_keeps_flat_images_and_real_edges() {
        let img = decode_ipum(&gray_image()).unwrap();
        assert_eq!(img.quantizers, vec![1]);
        assert_eq!(img.smoothed_rgba(), img.rgba);

        // A small step at a block edge in a flat run is spread out monotonically.
        let mut row = [100u8, 100, 100, 100, 100, 104, 104, 104, 104, 104];
        filter_edge(&mut row, 0, 1, 3);
        assert!(
            row.windows(2).all(|p| p[0] <= p[1]) && row[5] - row[4] < 4,
            "{row:?}"
        );
        // A strong edge (cloud, horizon) is left alone.
        let mut row = [40u8, 40, 40, 40, 40, 160, 160, 160, 160, 160];
        filter_edge(&mut row, 0, 1, 3);
        assert_eq!(row, [40, 40, 40, 40, 40, 160, 160, 160, 160, 160]);

        // Low-amplitude stripes flatten; a large step keeps its contrast.
        let (width, height) = (16, 8);
        let mut plane: Vec<u8> = (0..width * height)
            .map(|i| {
                let x = i % width;
                if x < 8 { 100 + (x % 2) as u8 * 3 } else { 200 }
            })
            .collect();
        denoise(&mut plane, width, height, 2, |_, _| 3);
        let row = &plane[3 * width..4 * width];
        assert!(
            row[1..6].iter().all(|&v| v.abs_diff(row[2]) <= 1),
            "{row:?}"
        );
        assert!(row[7] < 110 && row[8] > 190, "{row:?}");
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
