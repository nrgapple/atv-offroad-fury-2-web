//! CSS colours and gradients, kept in the browser's straight-alpha sRGB form.

/// Straight (non-premultiplied) sRGB colour, components 0-1.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0.);
    pub const BLACK: Color = Color::rgba(0, 0, 0, 1.);
    pub const WHITE: Color = Color::rgba(255, 255, 255, 1.);

    pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a,
        }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 1.)
    }

    /// `#rgb`, `#rrggbb` or `#rrggbbaa`, as written in the stylesheets.
    pub const fn hex(text: &str) -> Self {
        let b = text.as_bytes();
        let n = b.len() - 1;
        const fn digit(c: u8) -> u8 {
            match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => panic!("bad hex colour"),
            }
        }
        const fn pair(b: &[u8], i: usize) -> u8 {
            digit(b[i]) * 16 + digit(b[i + 1])
        }
        match n {
            3 | 4 => {
                let r = digit(b[1]) * 17;
                let g = digit(b[2]) * 17;
                let bl = digit(b[3]) * 17;
                let a = if n == 4 { digit(b[4]) * 17 } else { 255 };
                Self::rgba(r, g, bl, a as f32 / 255.)
            }
            6 | 8 => {
                let a = if n == 8 { pair(b, 7) } else { 255 };
                Self::rgba(pair(b, 1), pair(b, 3), pair(b, 5), a as f32 / 255.)
            }
            _ => panic!("bad hex colour length"),
        }
    }

    /// `r,g,b` as the original overlay layouts write them (0-255).
    pub fn from_triplet(text: &str) -> Option<Self> {
        let mut parts = text.split(',').map(|v| v.trim().parse::<f32>().ok());
        let (r, g, b) = (parts.next()??, parts.next()??, parts.next()??);
        Some(Self {
            r: (r / 255.).clamp(0., 1.),
            g: (g / 255.).clamp(0., 1.),
            b: (b / 255.).clamp(0., 1.),
            a: 1.,
        })
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn fade(self, opacity: f32) -> Self {
        Self {
            a: self.a * opacity,
            ..self
        }
    }

    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    pub fn to_u8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a].map(|v| (v.clamp(0., 1.) * 255. + 0.5) as u8)
    }

    /// CSS serialisation for the canvas text backend.
    pub fn css(self) -> String {
        let [r, g, b, _] = self.to_u8();
        format!("rgba({r},{g},{b},{})", self.a)
    }
}

/// One `linear-gradient` stop; `at` is 0-1 along the gradient line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub at: f32,
    pub color: Color,
}

/// A CSS `linear-gradient(angle, stops…)`. Angles follow CSS: 0deg points up,
/// 90deg to the right, 180deg (the default) downward.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub angle_degrees: f32,
    pub stops: Vec<Stop>,
}

impl Gradient {
    pub fn new(angle_degrees: f32, stops: &[(f32, Color)]) -> Self {
        Self {
            angle_degrees,
            stops: stops
                .iter()
                .map(|&(at, color)| Stop { at, color })
                .collect(),
        }
    }

    /// Colour at `t` (0-1), interpolated in premultiplied space like browsers.
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let stops = &self.stops;
        let Some(first) = stops.first() else {
            return [0.; 4];
        };
        if t <= first.at {
            return first.color.premultiplied();
        }
        for pair in stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t <= b.at {
                let span = (b.at - a.at).max(1e-6);
                let f = ((t - a.at) / span).clamp(0., 1.);
                let (pa, pb) = (a.color.premultiplied(), b.color.premultiplied());
                return std::array::from_fn(|i| pa[i] + (pb[i] - pa[i]) * f);
            }
        }
        stops.last().unwrap().color.premultiplied()
    }

    /// A 256-texel premultiplied ramp for the GPU.
    pub fn ramp(&self) -> Vec<u8> {
        (0..256)
            .flat_map(|i| {
                let c = self.sample((i as f32 + 0.5) / 256.);
                c.map(|v| (v.clamp(0., 1.) * 255. + 0.5) as u8)
            })
            .collect()
    }
}

/// Background paint: a flat colour or a gradient.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
    Linear(Gradient),
    /// `radial-gradient(circle at x% y%, …)` sized to the farthest corner;
    /// `center` is a fraction of the box and the stops use `Gradient::stops`.
    Radial {
        center: [f32; 2],
        gradient: Gradient,
        /// `ellipse` instead of `circle` (farthest-corner either way).
        ellipse: bool,
    },
}

impl Fill {
    /// `radial-gradient(circle, stops…)` at the box centre.
    pub fn radial(stops: &[(f32, Color)]) -> Self {
        Self::radial_at([0.5, 0.5], stops)
    }
    pub fn radial_at(center: [f32; 2], stops: &[(f32, Color)]) -> Self {
        Fill::Radial {
            center,
            gradient: Gradient::new(0., stops),
            ellipse: false,
        }
    }
    /// `radial-gradient(ellipse at x% y%, stops…)`.
    pub fn ellipse_at(center: [f32; 2], stops: &[(f32, Color)]) -> Self {
        Fill::Radial {
            center,
            gradient: Gradient::new(0., stops),
            ellipse: true,
        }
    }
}

impl From<Gradient> for Fill {
    fn from(gradient: Gradient) -> Self {
        Fill::Linear(gradient)
    }
}

impl From<Color> for Fill {
    fn from(color: Color) -> Self {
        Fill::Solid(color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_css_hex_forms() {
        assert_eq!(Color::hex("#fff").to_u8(), [255, 255, 255, 255]);
        assert_eq!(Color::hex("#0c0c0cdb").to_u8(), [12, 12, 12, 219]);
        assert_eq!(Color::hex("#000c").to_u8(), [0, 0, 0, 204]);
        assert_eq!(
            Color::from_triplet("218,192,31").unwrap().to_u8(),
            [218, 192, 31, 255]
        );
    }
    #[test]
    fn gradient_interpolates_premultiplied() {
        let g = Gradient::new(
            90.,
            &[(0., Color::hex("#ffffff38")), (1., Color::hex("#fff0"))],
        );
        let mid = g.sample(0.5);
        assert!((mid[3] - 0.1098 / 1.).abs() < 0.01);
        assert!((mid[0] - mid[3]).abs() < 1e-6);
    }
}
