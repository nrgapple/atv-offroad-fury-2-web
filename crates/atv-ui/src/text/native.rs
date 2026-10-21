//! Desktop text: cosmic-text shaping on the installed system fonts (in the
//! stylesheet's family order), swash glyph coverage and zeno paths. Stroke and
//! shadow effects reproduce Canvas 2D `strokeText`/`shadowBlur` on coverage.
use super::{Bitmap, Font, Join, LineMetrics, Shape, TextStyle};
use crate::color::Color;
use cosmic_text::{
    Attrs, Buffer, CacheKeyFlags, Family, FontSystem, LetterSpacing, Metrics, Shaping, Style,
    Weight, fontdb,
};
use std::collections::HashMap;
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{self, Angle, Format, Transform, Vector},
};

pub struct Backend {
    system: Option<FontSystem>,
    scale: ScaleContext,
    families: HashMap<&'static str, Option<String>>,
}

/// One shaped glyph, in CSS pixels relative to the pen.
struct Placed {
    font: fontdb::ID,
    weight: Weight,
    glyph: u16,
    size: f32,
    x: f32,
    y: f32,
    flags: CacheKeyFlags,
}

impl Backend {
    pub fn new() -> Self {
        Self {
            system: None,
            scale: ScaleContext::new(),
            families: HashMap::new(),
        }
    }

    fn system(&mut self) -> &mut FontSystem {
        self.system.get_or_insert_with(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            FontSystem::new_with_locale_and_db("en-US".into(), db)
        })
    }

    /// The first installed family of a CSS list (None: generic sans-serif).
    fn family(&mut self, list: &'static str) -> Option<String> {
        if let Some(found) = self.families.get(list) {
            return found.clone();
        }
        let db = self.system().db();
        let found = list
            .split(',')
            .map(|name| name.trim().trim_matches(['\'', '"']))
            .take_while(|name| !name.eq_ignore_ascii_case("sans-serif"))
            .find_map(|name| {
                db.faces().find_map(|face| {
                    face.families
                        .iter()
                        .find(|(family, _)| family.eq_ignore_ascii_case(name))
                        .map(|(family, _)| family.clone())
                })
            });
        self.families.insert(list, found.clone());
        found
    }

    fn layout(&mut self, text: &str, font: &Font, letter_spacing: f32) -> (Vec<Placed>, f32) {
        let family = self.family(font.family);
        let system = self.system();
        let mut buffer = Buffer::new(system, Metrics::new(font.size, font.size * 1.25));
        buffer.set_size(None, None);
        let mut attrs = Attrs::new()
            .family(match &family {
                Some(name) => Family::Name(name),
                None => Family::SansSerif,
            })
            .weight(Weight(font.weight))
            .style(if font.italic {
                Style::Italic
            } else {
                Style::Normal
            });
        if letter_spacing != 0. {
            attrs.letter_spacing_opt = Some(LetterSpacing(letter_spacing / font.size));
        }
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(system, false);
        let mut placed = Vec::new();
        let mut width = 0f32;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            for g in run.glyphs {
                placed.push(Placed {
                    font: g.font_id,
                    weight: g.font_weight,
                    glyph: g.glyph_id,
                    size: g.font_size,
                    x: g.x + g.font_size * g.x_offset,
                    y: g.y - g.font_size * g.y_offset,
                    flags: g.cache_key_flags,
                });
            }
        }
        (placed, width)
    }

    /// The primary face's ascent/descent, as CSS line boxes use.
    fn vertical(&mut self, font: &Font) -> (f32, f32) {
        let family = self.family(font.family);
        let system = self.system();
        let families = [match &family {
            Some(name) => fontdb::Family::Name(name),
            None => fontdb::Family::SansSerif,
        }];
        let query = fontdb::Query {
            families: &families,
            weight: fontdb::Weight(font.weight),
            stretch: fontdb::Stretch::Normal,
            style: if font.italic {
                fontdb::Style::Italic
            } else {
                fontdb::Style::Normal
            },
        };
        let fallback = (font.size * 0.9, font.size * 0.25);
        let Some(id) = system.db().query(&query) else {
            return fallback;
        };
        let Some(face) = system.get_font(id, Weight(font.weight)) else {
            return fallback;
        };
        let m = face.as_swash().metrics(&[]);
        let unit = font.size / m.units_per_em.max(1) as f32;
        (m.ascent * unit, m.descent * unit)
    }

    pub fn measure(&mut self, text: &str, font: &Font, letter_spacing: f32) -> LineMetrics {
        let (_, width) = self.layout(text, font, letter_spacing);
        let (ascent, descent) = self.vertical(font);
        LineMetrics {
            width,
            ascent,
            descent,
        }
    }

    pub fn text(
        &mut self,
        text: &str,
        style: &TextStyle,
        scale: f32,
        fraction: [f32; 2],
    ) -> Bitmap {
        let (placed, natural) = self.layout(text, &style.font, style.letter_spacing);
        let squeeze = match style.max_width {
            Some(max) if natural > max && natural > 0. => max / natural,
            _ => 1.,
        };
        // Glyph coverage images at their physical pen positions.
        let mut glyphs = Vec::new();
        for g in &placed {
            let Some(face) = self.system().get_font(g.font, g.weight) else {
                continue;
            };
            let mut scaler = self
                .scale
                .builder(face.as_swash())
                .size(g.size * scale)
                .hint(false)
                .build();
            let x = g.x * squeeze * scale + fraction[0];
            let y = g.y * scale + fraction[1];
            let (ix, iy) = (x.floor(), y.floor());
            let mut transform = Transform::scale(squeeze, 1.);
            if g.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
                transform = Transform::skew(Angle::from_degrees(14.), Angle::from_degrees(0.))
                    .then(&transform);
            }
            let image = Render::new(&[
                Source::ColorOutline(0),
                Source::ColorBitmap(StrikeWith::BestFit),
                Source::Outline,
            ])
            .format(Format::Alpha)
            .offset(Vector::new(x - ix, y - iy))
            .transform(Some(transform))
            .render(&mut scaler, g.glyph);
            if let Some(image) = image {
                let p = image.placement;
                let alpha: Vec<u8> = match image.content {
                    swash::scale::image::Content::Mask => image.data,
                    _ => image.data.as_chunks::<4>().0.iter().map(|c| c[3]).collect(),
                };
                glyphs.push((
                    ix as i32 + p.left,
                    iy as i32 - p.top,
                    p.width,
                    p.height,
                    alpha,
                ));
            }
        }
        let mut coverage = Coverage::from_parts(&glyphs, usize::MAX);
        coverage.compose(style, scale)
    }

    pub fn shape(&mut self, shape: &Shape, scale: f32, fraction: [f32; 2]) -> Bitmap {
        let unit = shape.unit * scale;
        let transform = Some(Transform::scale(unit, unit));
        let offset = Vector::new(fraction[0], fraction[1]);
        let fill = {
            let mut mask = zeno::Mask::new(shape.path.as_str());
            mask.transform(transform).offset(offset);
            part(mask.render())
        };
        let stroke = shape.stroke.map(|stroke| {
            let mut mask = zeno::Mask::new(shape.path.as_str());
            let mut style = zeno::Stroke::new(stroke.width / shape.unit);
            style.join(match stroke.join {
                Join::Round => zeno::Join::Round,
                Join::Miter => zeno::Join::Miter,
            });
            if stroke.join == Join::Round {
                style.cap(zeno::Cap::Round);
            }
            mask.style(style).transform(transform).offset(offset);
            part(mask.render())
        });
        let mut parts = vec![fill.clone()];
        parts.extend(stroke.clone());
        let mut layers = Coverage::from_parts(&parts, 1);
        let pad = shape.drop_shadow.map_or(0, |s| {
            ((s.dx.abs().max(s.dy.abs()) + s.blur * 1.5) * scale).ceil() as i32 + 1
        });
        layers.pad(pad);
        let fill_layer = layers.layer(&fill);
        let stroke_layer = stroke.map(|s| layers.layer(&s));
        let mut out = vec![[0f32; 4]; (layers.width * layers.height) as usize];
        if let Some(shadow) = shape.drop_shadow {
            // drop-shadow() casts the painted shape's alpha.
            let union: Vec<f32> = match &stroke_layer {
                Some(s) => fill_layer.iter().zip(s).map(|(a, b)| a.max(*b)).collect(),
                None => fill_layer.clone(),
            };
            let cast = layers.shadow(
                &union,
                shadow.dx * scale,
                shadow.dy * scale,
                shadow.blur * scale,
            );
            over(&mut out, &cast, shadow.color);
        }
        if let (Some(layer), Some(stroke)) = (&stroke_layer, shape.stroke) {
            over(&mut out, layer, stroke.color);
        }
        over(&mut out, &fill_layer, shape.fill);
        layers.bitmap(&out)
    }
}

type Part = (i32, i32, u32, u32, Vec<u8>);

fn part((data, p): (Vec<u8>, zeno::Placement)) -> Part {
    (p.left, p.top, p.width, p.height, data)
}

/// A coverage canvas covering several masks placed around the pen.
struct Coverage {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    fill: Vec<f32>,
}

impl Coverage {
    /// Bounds cover every part; the first `filled` parts form the fill coverage.
    fn from_parts(parts: &[Part], filled: usize) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (x, y, w, h, _) in parts {
            if *w == 0 || *h == 0 {
                continue;
            }
            x0 = x0.min(*x);
            y0 = y0.min(*y);
            x1 = x1.max(x + *w as i32);
            y1 = y1.max(y + *h as i32);
        }
        if x0 > x1 {
            (x0, y0, x1, y1) = (0, 0, 1, 1);
        }
        let mut c = Self {
            left: x0,
            top: y0,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
            fill: Vec::new(),
        };
        let mut fill = vec![0.; (c.width * c.height) as usize];
        for p in &parts[..filled.min(parts.len())] {
            c.add(&mut fill, p);
        }
        c.fill = fill;
        c
    }

    fn add(&self, into: &mut [f32], (x, y, w, h, alpha): &Part) {
        for row in 0..*h as i32 {
            for col in 0..*w as i32 {
                let (tx, ty) = (x + col - self.left, y + row - self.top);
                if tx < 0 || ty < 0 || tx >= self.width as i32 || ty >= self.height as i32 {
                    continue;
                }
                let a = alpha[(row * *w as i32 + col) as usize] as f32 / 255.;
                let d = &mut into[(ty * self.width as i32 + tx) as usize];
                *d += a * (1. - *d);
            }
        }
    }

    fn layer(&self, part: &Part) -> Vec<f32> {
        let mut layer = vec![0.; (self.width * self.height) as usize];
        self.add(&mut layer, part);
        layer
    }

    fn pad(&mut self, pad: i32) {
        if pad <= 0 {
            return;
        }
        let (w, h) = (self.width as i32 + 2 * pad, self.height as i32 + 2 * pad);
        let mut fill = vec![0.; (w * h) as usize];
        for row in 0..self.height as i32 {
            for col in 0..self.width as i32 {
                fill[((row + pad) * w + col + pad) as usize] =
                    self.fill[(row * self.width as i32 + col) as usize];
            }
        }
        self.left -= pad;
        self.top -= pad;
        self.width = w as u32;
        self.height = h as u32;
        self.fill = fill;
    }

    /// Round-join stroke centred on the outline: the coverage dilated by
    /// half the line width.
    fn dilate(&self, source: &[f32], radius: f32) -> Vec<f32> {
        let reach = (radius + 0.5).ceil() as i32;
        let mut offsets = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let w = (radius + 0.5 - ((dx * dx + dy * dy) as f32).sqrt()).clamp(0., 1.);
                if w > 0. {
                    offsets.push((dx, dy, w));
                }
            }
        }
        let (w, h) = (self.width as i32, self.height as i32);
        let mut out = vec![0.; source.len()];
        for y in 0..h {
            for x in 0..w {
                let mut best = 0f32;
                for &(dx, dy, weight) in &offsets {
                    let (sx, sy) = (x + dx, y + dy);
                    if sx >= 0 && sy >= 0 && sx < w && sy < h {
                        best = best.max(source[(sy * w + sx) as usize] * weight);
                    }
                }
                out[(y * w + x) as usize] = best;
            }
        }
        out
    }

    /// Offset and Gaussian-blurred alpha (canvas shadowBlur = 2 sigma).
    fn shadow(&self, source: &[f32], dx: f32, dy: f32, blur: f32) -> Vec<f32> {
        let (w, h) = (self.width as i32, self.height as i32);
        let (ox, oy) = (dx.round() as i32, dy.round() as i32);
        let mut moved = vec![0.; source.len()];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (x - ox, y - oy);
                if sx >= 0 && sy >= 0 && sx < w && sy < h {
                    moved[(y * w + x) as usize] = source[(sy * w + sx) as usize];
                }
            }
        }
        let sigma = blur / 2.;
        if sigma < 0.3 {
            return moved;
        }
        let radius = (sigma * 3.).ceil() as i32;
        let kernel: Vec<f32> = (-radius..=radius)
            .map(|i| (-(i * i) as f32 / (2. * sigma * sigma)).exp())
            .collect();
        let total: f32 = kernel.iter().sum();
        let pass = |input: &[f32], horizontal: bool| -> Vec<f32> {
            let mut out = vec![0.; input.len()];
            for y in 0..h {
                for x in 0..w {
                    let mut sum = 0.;
                    for (k, weight) in kernel.iter().enumerate() {
                        let d = k as i32 - radius;
                        let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                        if sx >= 0 && sy >= 0 && sx < w && sy < h {
                            sum += input[(sy * w + sx) as usize] * weight;
                        }
                    }
                    out[(y * w + x) as usize] = sum / total;
                }
            }
            out
        };
        pass(&pass(&moved, true), false)
    }

    fn compose(&mut self, style: &TextStyle, scale: f32) -> Bitmap {
        let stroke_radius = style.stroke.map_or(0., |s| s.width / 2. * scale);
        let pad = stroke_radius.ceil() as i32
            + style
                .shadows
                .iter()
                .map(|s| ((s.dx.abs().max(s.dy.abs()) + s.blur * 1.5) * scale).ceil() as i32)
                .max()
                .unwrap_or(0)
            + 1;
        self.pad(pad);
        let stroke = style.stroke.map(|_| self.dilate(&self.fill, stroke_radius));
        let mut out = vec![[0f32; 4]; self.fill.len()];
        let union: Vec<f32> = match &stroke {
            Some(s) => self.fill.iter().zip(s).map(|(a, b)| a.max(*b)).collect(),
            None => self.fill.clone(),
        };
        for shadow in style.shadows.iter().rev() {
            let cast = self.shadow(
                &union,
                shadow.dx * scale,
                shadow.dy * scale,
                shadow.blur * scale,
            );
            over(&mut out, &cast, shadow.color);
        }
        if let (Some(layer), Some(s)) = (&stroke, style.stroke) {
            over(&mut out, layer, s.color);
        }
        let fill = std::mem::take(&mut self.fill);
        over(&mut out, &fill, style.color);
        self.bitmap(&out)
    }

    fn bitmap(&self, out: &[[f32; 4]]) -> Bitmap {
        Bitmap {
            width: self.width,
            height: self.height,
            left: self.left,
            top: self.top,
            rgba: out
                .iter()
                .flat_map(|p| p.map(|v| (v.clamp(0., 1.) * 255. + 0.5) as u8))
                .collect(),
        }
    }
}

/// Source-over of `color` at `coverage` onto premultiplied `out`.
fn over(out: &mut [[f32; 4]], coverage: &[f32], color: Color) {
    let c = color.premultiplied();
    for (d, a) in out.iter_mut().zip(coverage) {
        if *a <= 0. {
            continue;
        }
        let k = 1. - c[3] * a;
        for i in 0..4 {
            d[i] = c[i] * a + d[i] * k;
        }
    }
}
