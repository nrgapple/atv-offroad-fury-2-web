//! The authored Fury Remastered wordmark (web/fury-remastered.svg), drawn
//! from the same vector paths at the size it is shown: Canvas 2D in browsers
//! (the browser's own path rasterizer), zeno paths natively.
use super::pixels::Rgba;
use crate::color::Color;

/// SVG viewBox size.
pub const SIZE: [f32; 2] = [960., 320.];

const FURY: [(f32, &str); 4] = [
    (0., "M0 0H160V36H44V70H138V106H44V174H0Z"),
    (185., "M0 0H44V132H112V0H156V144L126 174H30L0 144Z"),
    (
        370.,
        "M0 0H128L158 30V79L133 104L165 174H115L88 111H44V174H0ZM44 36V76H112V36Z",
    ),
    (565., "M0 0H48L84 65L120 0H168L106 106V174H62V106Z"),
];
const REMASTERED: [(f32, &str); 10] = [
    (
        0.,
        "M0 0H24L31 7V17L24 24L33 40H23L15 25H8V40H0ZM8 7V18H23V7Z",
    ),
    (42., "M0 0H28V7H8V16H25V23H8V33H28V40H0Z"),
    (84., "M0 40V0H9L18 17L27 0H36V40H28V14L18 31L8 14V40Z"),
    (130., "M0 40L12 0H24L36 40H27L24 30H12L9 40ZM14 23H22L18 9Z"),
    (
        176.,
        "M0 6L6 0H30V7H8V16H24L30 22V34L24 40H0V33H22V23H6L0 17Z",
    ),
    (218., "M0 0H32V8H20V40H12V8H0Z"),
    (260., "M0 0H28V7H8V16H25V23H8V33H28V40H0Z"),
    (
        302.,
        "M0 0H24L31 7V17L24 24L33 40H23L15 25H8V40H0ZM8 7V18H23V7Z",
    ),
    (344., "M0 0H28V7H8V16H25V23H8V33H28V40H0Z"),
    (386., "M0 0H23L31 8V32L23 40H0ZM8 8V32H23V8Z"),
];

/// Fill or stroke paint; gradients run top to bottom over `y` (local units).
#[derive(Clone, Debug)]
pub enum Paint {
    Solid(Color),
    Vertical {
        y: [f32; 2],
        stops: Vec<(f32, Color)>,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl Paint {
    fn at(&self, y: f32) -> Color {
        match self {
            Paint::Solid(c) => *c,
            Paint::Vertical { y: [y0, y1], stops } => {
                let t = ((y - y0) / (y1 - y0)).clamp(0., 1.);
                let mut previous = stops[0];
                for &(at, color) in stops {
                    if t <= at {
                        let span = at - previous.0;
                        let f = if span > 0. {
                            (t - previous.0) / span
                        } else {
                            1.
                        };
                        return mix(previous.1, color, f);
                    }
                    previous = (at, color);
                }
                previous.1
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn mix(a: Color, b: Color, f: f32) -> Color {
    let [ar, ag, ab, aa] = a.premultiplied();
    let [br, bg, bb, ba] = b.premultiplied();
    let p = [
        ar + (br - ar) * f,
        ag + (bg - ag) * f,
        ab + (bb - ab) * f,
        aa + (ba - aa) * f,
    ];
    if p[3] <= 0. {
        return Color::TRANSPARENT;
    }
    Color::rgba(
        (p[0] / p[3] * 255.).round() as u8,
        (p[1] / p[3] * 255.).round() as u8,
        (p[2] / p[3] * 255.).round() as u8,
        p[3],
    )
}

/// One painted element: absolute path, `[a, b, c, d, e, f]` transform.
#[derive(Clone, Debug)]
pub struct Op {
    pub path: String,
    pub transform: [f32; 6],
    pub fill: Option<Paint>,
    pub stroke: Option<(Paint, f32)>,
}

/// Translate an absolute M/H/V/L/Z path along x.
fn shift(path: &str, dx: f32) -> String {
    let mut out = String::new();
    let mut command = 'M';
    let mut index = 0;
    let mut number = String::new();
    let flush = |number: &mut String, out: &mut String, command: char, index: &mut usize| {
        if number.is_empty() {
            return;
        }
        let v: f32 = number.parse().unwrap_or(0.);
        let x =
            matches!(command, 'H') || (matches!(command, 'M' | 'L') && (*index).is_multiple_of(2));
        let v = if x { v + dx } else { v };
        if !out.is_empty() && !out.ends_with(|c: char| c.is_ascii_alphabetic()) {
            out.push(' ');
        }
        out.push_str(&format!("{v}"));
        *index += 1;
        number.clear();
    };
    for c in path.chars() {
        if c.is_ascii_alphabetic() {
            flush(&mut number, &mut out, command, &mut index);
            command = c;
            index = 0;
            out.push(c);
        } else if c == ' ' || c == ',' {
            flush(&mut number, &mut out, command, &mut index);
        } else {
            number.push(c);
        }
    }
    flush(&mut number, &mut out, command, &mut index);
    out
}

fn group(parts: &[(f32, &str)]) -> String {
    parts.iter().map(|(dx, p)| shift(p, *dx)).collect()
}

/// The SVG's painted elements, in order.
pub fn ops() -> Vec<Op> {
    let hex = Color::hex;
    let skew = (-15f32).to_radians().tan();
    let fury = group(&FURY);
    let fire = Paint::Vertical {
        y: [0., 174.],
        stops: vec![
            (0., hex("#ff8050")),
            (0.12, hex("#f23527")),
            (0.5, hex("#c90c25")),
            (0.51, hex("#9a0920")),
            (1., hex("#eb3028")),
        ],
    };
    let chrome = Paint::Vertical {
        y: [0., 174.],
        stops: vec![
            (0., hex("#fffdf4")),
            (0.42, hex("#c9cfce")),
            (0.5, hex("#5e6872")),
            (0.72, hex("#c5c9c9")),
            (1., hex("#f2eee0")),
        ],
    };
    let gold = |y: [f32; 2]| Paint::Vertical {
        y,
        stops: vec![
            (0., hex("#fff1b0")),
            (0.5, hex("#ffc342")),
            (1., hex("#f08b13")),
        ],
    };
    let lettering = |fill: Option<Paint>, stroke: Option<(Paint, f32)>, offset: [f32; 2]| Op {
        path: fury.clone(),
        transform: [
            1.,
            0.,
            skew,
            1.,
            145. + offset[0] + skew * offset[1],
            30. + offset[1],
        ],
        fill,
        stroke,
    };
    let solid = |c: &str| Paint::Solid(hex(c));
    vec![
        lettering(
            Some(solid("#08090d")),
            Some((solid("#08090d"), 20.)),
            [8., 12.],
        ),
        lettering(
            Some(solid("#272b32")),
            Some((solid("#272b32"), 19.)),
            [0., 0.],
        ),
        lettering(None, Some((chrome, 12.)), [0., 0.]),
        lettering(Some(fire), Some((solid("#690619"), 2.)), [0., 0.]),
        Op {
            path: "M85 240H843L831 294H71Z".into(),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some(solid("#0b0d12")),
            stroke: Some((solid("#626971"), 2.)),
        },
        Op {
            path: "M89 239H846L841 246H87Z".into(),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some(gold([239., 246.])),
            stroke: None,
        },
        Op {
            path: group(&REMASTERED),
            transform: [1.22, 0., skew * 0.72, 0.72, 209., 254.],
            fill: Some(gold([0., 40.])),
            stroke: None,
        },
        Op {
            path: "M95 260H164L160 268H93ZM747 270H812L810 278H745Z".into(),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some(solid("#d85820")),
            stroke: None,
        },
    ]
}

/// The wordmark at `width` pixels (height = width / 3).
pub fn raster(width: u32) -> Rgba {
    let width = width.clamp(16, 4096);
    let height = (width as f32 * SIZE[1] / SIZE[0]).round().max(1.) as u32;
    let scale = width as f32 / SIZE[0];
    imp::raster(&ops(), width, height, scale)
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::{Op, Rgba};
    use swash::zeno::{self, Fill, Mask, Stroke, Transform};

    pub fn raster(ops: &[Op], width: u32, height: u32, scale: f32) -> Rgba {
        let mut out = vec![[0f32; 4]; (width * height) as usize];
        for op in ops {
            let [a, b, c, d, e, f] = op.transform;
            let t = Transform::new(
                a * scale,
                b * scale,
                c * scale,
                d * scale,
                e * scale,
                f * scale,
            );
            // Local y of a screen row (b = 0 for every element here).
            let local_y = |row: u32| ((row as f32 + 0.5) / scale - f) / d;
            let mut paint = |coverage: &[u8], paint: &super::Paint| {
                for row in 0..height {
                    let color = paint.at(local_y(row)).premultiplied();
                    for col in 0..width {
                        let i = (row * width + col) as usize;
                        let k = f32::from(coverage[i]) / 255.;
                        if k <= 0. {
                            continue;
                        }
                        let src = color.map(|v| v * k);
                        let dst = &mut out[i];
                        for ch in 0..4 {
                            dst[ch] = src[ch] + dst[ch] * (1. - src[3]);
                        }
                    }
                }
            };
            if let Some(fill) = &op.fill {
                let mut coverage = vec![0u8; (width * height) as usize];
                Mask::new(op.path.as_str())
                    .size(width, height)
                    .style(Fill::EvenOdd)
                    .transform(Some(t))
                    .render_into(&mut coverage, None);
                paint(&coverage, fill);
            }
            if let Some((stroke, line)) = &op.stroke {
                let mut coverage = vec![0u8; (width * height) as usize];
                let mut style = Stroke::new(*line);
                style.join(zeno::Join::Miter).miter_limit(3.);
                Mask::new(op.path.as_str())
                    .size(width, height)
                    .style(style)
                    .transform(Some(t))
                    .render_into(&mut coverage, None);
                paint(&coverage, stroke);
            }
        }
        let mut data = Vec::with_capacity(out.len() * 4);
        for [r, g, b, a] in out {
            let un = |v: f32| {
                if a > 0. {
                    (v / a * 255.).round() as u8
                } else {
                    0
                }
            };
            data.extend_from_slice(&[un(r), un(g), un(b), (a * 255.).round() as u8]);
        }
        Rgba::new(width, height, data)
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::{Op, Paint, Rgba};
    use wasm_bindgen::JsCast;
    use web_sys::{CanvasRenderingContext2d, CanvasWindingRule, HtmlCanvasElement, Path2d};

    fn gradient(
        c: &CanvasRenderingContext2d,
        y: [f32; 2],
        stops: &[(f32, crate::color::Color)],
    ) -> web_sys::CanvasGradient {
        let g = c.create_linear_gradient(0., f64::from(y[0]), 0., f64::from(y[1]));
        for (at, color) in stops {
            g.add_color_stop(*at, &color.css()).ok();
        }
        g
    }

    fn fill(c: &CanvasRenderingContext2d, paint: &Paint) {
        match paint {
            Paint::Solid(color) => c.set_fill_style_str(&color.css()),
            Paint::Vertical { y, stops } => {
                c.set_fill_style_canvas_gradient(&gradient(c, *y, stops))
            }
        }
    }

    fn stroke(c: &CanvasRenderingContext2d, paint: &Paint) {
        match paint {
            Paint::Solid(color) => c.set_stroke_style_str(&color.css()),
            Paint::Vertical { y, stops } => {
                c.set_stroke_style_canvas_gradient(&gradient(c, *y, stops))
            }
        }
    }

    pub fn raster(ops: &[Op], width: u32, height: u32, scale: f32) -> Rgba {
        let empty = || Rgba::new(width, height, vec![0; (width * height * 4) as usize]);
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return empty();
        };
        let Some(canvas) = document
            .create_element("canvas")
            .ok()
            .and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
        else {
            return empty();
        };
        canvas.set_width(width);
        canvas.set_height(height);
        let Some(c) = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
        else {
            return empty();
        };
        let s = f64::from(scale);
        for op in ops {
            let Ok(path) = Path2d::new_with_path_string(&op.path) else {
                continue;
            };
            let [a, b, cc, d, e, f] = op.transform.map(f64::from);
            c.set_transform(a * s, b * s, cc * s, d * s, e * s, f * s)
                .ok();
            if let Some(paint) = &op.fill {
                fill(&c, paint);
                c.fill_with_path_2d_and_winding(&path, CanvasWindingRule::Evenodd);
            }
            if let Some((paint, line)) = &op.stroke {
                stroke(&c, paint);
                c.set_line_width(f64::from(*line));
                c.set_line_join("miter");
                c.set_miter_limit(3.);
                c.stroke_with_path(&path);
            }
        }
        match c.get_image_data(0., 0., f64::from(width), f64::from(height)) {
            Ok(data) => Rgba::new(width, height, data.data().0),
            Err(_) => empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifts_absolute_paths_along_x() {
        assert_eq!(shift("M0 0H160V36L8 14Z", 10.), "M10 0H170V36L18 14Z");
    }

    #[test]
    fn rasterizes_the_wordmark() {
        let logo = raster(480);
        assert_eq!((logo.width, logo.height), (480, 160));
        // The red lettering covers the top-left area; the corners stay clear.
        let at = |x: u32, y: u32| {
            let o = ((y * logo.width + x) * 4) as usize;
            [
                logo.data[o],
                logo.data[o + 1],
                logo.data[o + 2],
                logo.data[o + 3],
            ]
        };
        assert_eq!(at(0, 0)[3], 0);
        let red = at(69, 65);
        assert!(red[3] == 255 && red[0] > 150 && red[1] < 90, "{red:?}");
    }
}
