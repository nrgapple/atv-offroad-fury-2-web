//! Text and vector shapes, rasterized at display resolution.
//!
//! Browsers draw with the page's own Canvas 2D text (the same fonts, shaping and
//! antialiasing the JavaScript HUD and DOM use); desktop builds shape with
//! cosmic-text against the installed system fonts in the same CSS family order.
//! No font files ship with the game.
use crate::color::Color;
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
use native::Backend;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
use web::Backend;

/// `--atv-font-display` in web/design-system.css.
pub const DISPLAY: &str = "'Avenir Next Condensed', 'Arial Narrow', 'Roboto Condensed', 'Sans-Serif Condensed', 'Helvetica Neue', Arial, sans-serif";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    /// CSS family list.
    pub family: &'static str,
    /// CSS pixels.
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
}

impl Font {
    pub const fn display(size: f32, weight: u16, italic: bool) -> Self {
        Self {
            family: DISPLAY,
            size,
            weight,
            italic,
        }
    }
    /// The CSS `font` shorthand (canvas backend).
    pub fn css(&self) -> String {
        format!(
            "{}{} {}px {}",
            if self.italic { "italic " } else { "" },
            self.weight,
            self.size,
            self.family
        )
    }
    fn key(&self) -> (&'static str, u32, u16, bool) {
        (self.family, self.size.to_bits(), self.weight, self.italic)
    }
}

/// CSS `text-shadow` / canvas shadow: offset and blur radius in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Join {
    Round,
    Miter,
}

/// Canvas `strokeText` under the fill. Round joins also round open path caps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
    pub join: Join,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font: Font,
    pub color: Color,
    /// CSS `letter-spacing`, pixels.
    pub letter_spacing: f32,
    pub stroke: Option<Stroke>,
    /// CSS order: the first shadow paints on top.
    pub shadows: Vec<Shadow>,
    /// Canvas `fillText` maxWidth: wider runs are condensed horizontally.
    pub max_width: Option<f32>,
}

impl TextStyle {
    pub fn new(font: Font, color: Color) -> Self {
        Self {
            font,
            color,
            letter_spacing: 0.,
            stroke: None,
            shadows: Vec::new(),
            max_width: None,
        }
    }
    pub fn spacing(mut self, px: f32) -> Self {
        self.letter_spacing = px;
        self
    }
    pub fn stroke(mut self, width: f32, color: Color, join: Join) -> Self {
        self.stroke = Some(Stroke { width, color, join });
        self
    }
    pub fn shadow(mut self, dx: f32, dy: f32, blur: f32, color: Color) -> Self {
        self.shadows.push(Shadow {
            dx,
            dy,
            blur,
            color,
        });
        self
    }
    fn key(&self) -> String {
        format!(
            "{:?}|{:?}|{}|{:?}|{:?}|{:?}",
            self.font.key(),
            self.color.to_u8(),
            self.letter_spacing,
            self.stroke.map(|s| (s.width, s.color.to_u8(), s.join)),
            self.shadows
                .iter()
                .map(|s| (s.dx, s.dy, s.blur, s.color.to_u8()))
                .collect::<Vec<_>>(),
            self.max_width
        )
    }
}

/// Horizontal metrics of one line plus the font's ascent/descent (the
/// `fontBoundingBox` values CSS line boxes use), CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

/// Premultiplied RGBA. `left`/`top` place the bitmap relative to the pen
/// (text baseline origin or path origin), in physical pixels.
#[derive(Clone, Debug, Default)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub rgba: Vec<u8>,
}

/// A filled (and optionally stroked) SVG path, e.g. a toolbar icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// SVG path data in local units.
    pub path: String,
    /// Path extent `[min x, min y, max x, max y]` in local units (the viewBox).
    pub bounds: [f32; 4],
    /// CSS pixels per path unit.
    pub unit: f32,
    pub fill: Color,
    pub stroke: Option<Stroke>,
    /// CSS `filter: drop-shadow(dx dy 0 color)`.
    pub drop_shadow: Option<Shadow>,
}

impl Shape {
    fn key(&self) -> String {
        format!(
            "{}|{:?}|{}|{:?}|{:?}|{:?}",
            self.path,
            self.bounds,
            self.unit,
            self.fill.to_u8(),
            self.stroke.map(|s| (s.width, s.color.to_u8(), s.join)),
            self.drop_shadow
                .map(|s| (s.dx, s.dy, s.blur, s.color.to_u8()))
        )
    }
}

/// Raster cache key: content, style, scale and the quarter-pixel phase.
pub(crate) fn raster_key(kind: &str, scale: f32, phase: [u8; 2]) -> String {
    format!("{scale}|{phase:?}|{kind}")
}

type MeasureKey = (String, (&'static str, u32, u16, bool), u32);

pub struct Fonts {
    backend: Backend,
    measured: HashMap<MeasureKey, LineMetrics>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

/// Quarter-pixel phase of a physical coordinate: (whole, phase 0-3).
pub(crate) fn phase(value: f32) -> (i32, u8) {
    let q = (value * 4.).round() as i32;
    (q.div_euclid(4), q.rem_euclid(4) as u8)
}

impl Fonts {
    pub fn new() -> Self {
        Self {
            backend: Backend::new(),
            measured: HashMap::new(),
        }
    }

    pub fn measure(&mut self, text: &str, font: &Font, letter_spacing: f32) -> LineMetrics {
        let key = (text.to_string(), font.key(), letter_spacing.to_bits());
        if let Some(m) = self.measured.get(&key) {
            return *m;
        }
        let m = self.backend.measure(text, font, letter_spacing);
        if self.measured.len() > 4096 {
            self.measured.clear();
        }
        self.measured.insert(key, m);
        m
    }

    /// Greedy CSS-style wrapping at spaces.
    pub fn wrap(
        &mut self,
        text: &str,
        font: &Font,
        letter_spacing: f32,
        width: f32,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty()
                && self.measure(&candidate, font, letter_spacing).width > width + 0.01
            {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        lines.push(line);
        lines
    }

    pub(crate) fn text_key(text: &str, style: &TextStyle) -> String {
        format!("T{}|{}", style.key(), text)
    }

    pub(crate) fn shape_key(shape: &Shape) -> String {
        format!("S{}", shape.key())
    }

    /// Rasterizes one line with its pen at `phase` quarter pixels.
    pub(crate) fn raster_text(
        &mut self,
        text: &str,
        style: &TextStyle,
        scale: f32,
        phase: [u8; 2],
    ) -> Bitmap {
        self.backend.text(
            text,
            style,
            scale,
            [phase[0] as f32 / 4., phase[1] as f32 / 4.],
        )
    }

    pub(crate) fn raster_shape(&mut self, shape: &Shape, scale: f32, phase: [u8; 2]) -> Bitmap {
        self.backend
            .shape(shape, scale, [phase[0] as f32 / 4., phase[1] as f32 / 4.])
    }
}

/// Premultiplies straight RGBA in place.
pub(crate) fn premultiply(rgba: &mut [u8]) {
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
}
