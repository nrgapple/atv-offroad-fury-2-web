//! CPU image operations for the frontend's original artwork: joining the
//! two-half course photographs, trimming their dark borders (frontend.js
//! `trimDark`), and the CSS filters the browser applies to backdrops
//! (`blur`, `saturate`, `grayscale`, `contrast`, `brightness`).

/// Straight-alpha RGBA8 pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgba {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        debug_assert_eq!(data.len(), (width * height * 4) as usize);
        Self {
            width,
            height,
            data,
        }
    }

    fn at(&self, x: u32, y: u32) -> usize {
        ((y * self.width + x) * 4) as usize
    }

    /// `drawImage(a, 0, 0); drawImage(b, a.width, 0, b.width, a.height)`.
    pub fn join(a: &Rgba, b: &Rgba) -> Rgba {
        let width = a.width + b.width;
        let height = a.height.max(b.height);
        let mut out = Rgba::new(width, height, vec![0; (width * height * 4) as usize]);
        for y in 0..a.height {
            let src = a.at(0, y);
            let dst = out.at(0, y);
            out.data[dst..dst + (a.width * 4) as usize]
                .copy_from_slice(&a.data[src..src + (a.width * 4) as usize]);
            // b stretched to a's height (nearest row; the halves match).
            if b.height > 0 {
                let by = ((y as u64 * b.height as u64) / a.height.max(1) as u64) as u32;
                let src = b.at(0, by.min(b.height - 1));
                let dst = out.at(a.width, y);
                out.data[dst..dst + (b.width * 4) as usize]
                    .copy_from_slice(&b.data[src..src + (b.width * 4) as usize]);
            }
        }
        out
    }

    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Rgba {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for row in y..y + height {
            let start = self.at(x, row);
            data.extend_from_slice(&self.data[start..start + (width * 4) as usize]);
        }
        Rgba::new(width, height, data)
    }

    /// frontend.js `trimDark`: original photos carry black or near-black
    /// borders; cut whole dark columns and rows from the edges.
    pub fn trim_dark(self) -> Rgba {
        let (w, h) = (self.width as usize, self.height as usize);
        if w == 0 || h == 0 {
            return self;
        }
        let lit = |o: usize| {
            self.data[o + 3] > 16
                && u32::from(self.data[o])
                    + u32::from(self.data[o + 1])
                    + u32::from(self.data[o + 2])
                    > 54
        };
        let dark = |x0: usize, y0: usize, dx: usize, dy: usize, count: usize| {
            let lit_count = (0..count)
                .filter(|i| lit(((y0 + dy * i) * w + x0 + dx * i) * 4))
                .count();
            (lit_count as f64) < count as f64 * 0.04
        };
        let mut starts: Vec<usize> = (0..h)
            .map(|y| {
                let mut x = 0;
                while (x as f64) < w as f64 / 4. && !lit((y * w + x) * 4) {
                    x += 1;
                }
                x
            })
            .collect();
        starts.sort_unstable();
        let mut left = (w / 4).min(starts[(h as f64 * 0.9).floor() as usize]);
        let (mut right, mut top, mut bottom) = (w, 0, h);
        while (left as f64) < w as f64 / 4. && dark(left, 0, 0, 1, h) {
            left += 1;
        }
        while right as f64 > w as f64 * 0.75 && dark(right - 1, 0, 0, 1, h) {
            right -= 1;
        }
        while (top as f64) < h as f64 / 4. && dark(left, top, 1, 0, right - left) {
            top += 1;
        }
        while bottom as f64 > h as f64 * 0.6 && dark(left, bottom - 1, 1, 0, right - left) {
            bottom -= 1;
        }
        if left == 0 && top == 0 && right == w && bottom == h {
            return self;
        }
        self.crop(
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        )
    }

    /// CSS filter functions applied in order, each clamped (Filter Effects 1).
    pub fn filter(&mut self, ops: &[Filter]) {
        let matrices: Vec<[f32; 12]> = ops.iter().map(|op| op.matrix()).collect();
        for px in self.data.as_chunks_mut::<4>().0 {
            let mut c = [f32::from(px[0]), f32::from(px[1]), f32::from(px[2])];
            for m in &matrices {
                c = [
                    (m[0] * c[0] + m[1] * c[1] + m[2] * c[2] + m[3]).clamp(0., 255.),
                    (m[4] * c[0] + m[5] * c[1] + m[6] * c[2] + m[7]).clamp(0., 255.),
                    (m[8] * c[0] + m[9] * c[1] + m[10] * c[2] + m[11]).clamp(0., 255.),
                ];
            }
            px[0] = (c[0] + 0.5) as u8;
            px[1] = (c[1] + 0.5) as u8;
            px[2] = (c[2] + 0.5) as u8;
        }
    }

    /// `mix-blend-mode: multiply` with an opaque colour.
    pub fn multiply(&mut self, rgb: [u8; 3]) {
        for px in self.data.as_chunks_mut::<4>().0 {
            for i in 0..3 {
                px[i] = ((u32::from(px[i]) * u32::from(rgb[i]) + 127) / 255) as u8;
            }
        }
    }

    /// Gaussian blur (three box passes per axis), edges clamped.
    pub fn blur(&mut self, sigma: f32) {
        if sigma < 0.3 || self.width == 0 || self.height == 0 {
            return;
        }
        let boxes = box_sizes(sigma);
        let (w, h) = (self.width as usize, self.height as usize);
        let mut scratch = vec![0u8; self.data.len()];
        for radius in boxes {
            box_pass(&self.data, &mut scratch, w, h, radius, true);
            box_pass(&scratch, &mut self.data, w, h, radius, false);
        }
    }

    /// Downscale by an integer factor (box filter), for cheap large blurs.
    pub fn shrink(&self, factor: u32) -> Rgba {
        if factor <= 1 {
            return self.clone();
        }
        let (w, h) = ((self.width / factor).max(1), (self.height / factor).max(1));
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4];
                let mut n = 0;
                for sy in y * factor..((y + 1) * factor).min(self.height) {
                    for sx in x * factor..((x + 1) * factor).min(self.width) {
                        let o = self.at(sx, sy);
                        for (i, total) in sum.iter_mut().enumerate() {
                            *total += u32::from(self.data[o + i]);
                        }
                        n += 1;
                    }
                }
                let o = ((y * w + x) * 4) as usize;
                for i in 0..4 {
                    data[o + i] = (sum[i] / n.max(1)) as u8;
                }
            }
        }
        Rgba::new(w, h, data)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Saturate(f32),
    Grayscale(f32),
    Contrast(f32),
    Brightness(f32),
}

impl Filter {
    /// Row-major 3x4 colour matrix on 0-255 channels.
    fn matrix(self) -> [f32; 12] {
        let saturate = |s: f32, l: [f32; 3]| {
            [
                l[0] + (1. - l[0]) * s,
                l[1] - l[1] * s,
                l[2] - l[2] * s,
                0.,
                l[0] - l[0] * s,
                l[1] + (1. - l[1]) * s,
                l[2] - l[2] * s,
                0.,
                l[0] - l[0] * s,
                l[1] - l[1] * s,
                l[2] + (1. - l[2]) * s,
                0.,
            ]
        };
        match self {
            Filter::Saturate(s) => saturate(s, [0.213, 0.715, 0.072]),
            Filter::Grayscale(a) => saturate(1. - a.clamp(0., 1.), [0.2126, 0.7152, 0.0722]),
            Filter::Contrast(c) => {
                let o = (0.5 - 0.5 * c) * 255.;
                [c, 0., 0., o, 0., c, 0., o, 0., 0., c, o]
            }
            Filter::Brightness(k) => [k, 0., 0., 0., 0., k, 0., 0., 0., 0., k, 0.],
        }
    }
}

/// Box radii approximating a Gaussian of `sigma` with three passes.
fn box_sizes(sigma: f32) -> [usize; 3] {
    let n = 3.;
    let ideal = ((12. * sigma * sigma / n) + 1.).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12. * sigma * sigma - n * (wl * wl) as f32 - 4. * n * wl as f32 - 3. * n)
        / (-4. * wl as f32 - 4.))
        .round() as i32;
    let size = |i: i32| if i < m { wl } else { wu };
    [0, 1, 2].map(|i| ((size(i) - 1) / 2).max(0) as usize)
}

fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let index = |line: usize, i: usize| {
        if horizontal {
            (line * w + i) * 4
        } else {
            (i * w + line) * 4
        }
    };
    let span = (2 * r + 1) as u32;
    for line in 0..lines {
        let mut sum = [0u32; 4];
        for k in 0..=2 * r {
            let i = k.saturating_sub(r).min(len - 1);
            let i = if k < r { 0 } else { i };
            let o = index(line, i);
            for c in 0..4 {
                sum[c] += u32::from(src[o + c]);
            }
        }
        for i in 0..len {
            let o = index(line, i);
            for c in 0..4 {
                dst[o + c] = ((sum[c] + span / 2) / span) as u8;
            }
            let add = index(line, (i + r + 1).min(len - 1));
            let sub = index(line, i.saturating_sub(r));
            for c in 0..4 {
                sum[c] = sum[c] + u32::from(src[add + c]) - u32::from(src[sub + c]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_dark_borders_and_keeps_lit_pixels() {
        let (w, h) = (40u32, 20u32);
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..20 {
            for x in 5..37 {
                let o = ((y * w + x) * 4) as usize;
                data[o..o + 4].copy_from_slice(&[200, 180, 160, 255]);
            }
        }
        let trimmed = Rgba::new(w, h, data).trim_dark();
        assert_eq!((trimmed.width, trimmed.height), (32, 20));
    }

    #[test]
    fn blur_preserves_flat_colour() {
        let mut image = Rgba::new(8, 8, [10u8, 20, 30, 255].repeat(64));
        image.blur(3.);
        assert!(image.data.chunks(4).all(|p| p == [10, 20, 30, 255]));
    }
}
