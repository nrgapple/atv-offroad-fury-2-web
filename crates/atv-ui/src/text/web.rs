//! Canvas 2D text: the browser's own fonts, shaping and antialiasing.
use super::{Bitmap, Font, Join, LineMetrics, Shape, TextStyle, premultiply};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

pub struct Backend {
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    letter_spacing: bool,
}

/// Shadows are cast from a copy drawn this far outside the canvas.
const AWAY: f64 = 8192.;

impl Backend {
    pub fn new() -> Self {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"willReadFrequently".into(), &true.into()).ok();
        let context: CanvasRenderingContext2d = canvas
            .get_context_with_context_options("2d", &options)
            .unwrap()
            .unwrap()
            .dyn_into()
            .unwrap();
        let letter_spacing =
            js_sys::Reflect::has(&context, &"letterSpacing".into()).unwrap_or(false);
        Self {
            canvas,
            context,
            letter_spacing,
        }
    }

    fn set_font(&self, font: &Font, letter_spacing: f32) {
        self.context.set_font(&font.css());
        if self.letter_spacing {
            js_sys::Reflect::set(
                &self.context,
                &"letterSpacing".into(),
                &JsValue::from_str(&format!("{letter_spacing}px")),
            )
            .ok();
        }
    }

    pub fn measure(&mut self, text: &str, font: &Font, letter_spacing: f32) -> LineMetrics {
        self.set_font(font, letter_spacing);
        let Ok(m) = self.context.measure_text(text) else {
            return LineMetrics::default();
        };
        LineMetrics {
            width: m.width() as f32,
            ascent: m.font_bounding_box_ascent() as f32,
            descent: m.font_bounding_box_descent() as f32,
            ..Default::default()
        }
    }

    /// Actual ink bounds of `text`: left, right, ascent, descent.
    pub fn ink(&mut self, text: &str, font: &Font) -> [f32; 4] {
        self.set_font(font, 0.);
        let Ok(m) = self.context.measure_text(text) else {
            return [0.; 4];
        };
        [
            m.actual_bounding_box_left() as f32,
            m.actual_bounding_box_right() as f32,
            m.actual_bounding_box_ascent() as f32,
            m.actual_bounding_box_descent() as f32,
        ]
    }

    /// Distance from canvas `textBaseline = 'top'` down to the alphabetic baseline.
    pub fn top_offset(&mut self, font: &Font) -> f32 {
        self.set_font(font, 0.);
        let ascent = |c: &CanvasRenderingContext2d| {
            c.measure_text("H")
                .map_or(0., |m| m.actual_bounding_box_ascent() as f32)
        };
        let alphabetic = ascent(&self.context);
        self.context.set_text_baseline("top");
        let top = ascent(&self.context);
        self.context.set_text_baseline("alphabetic");
        alphabetic - top
    }

    fn resize(&self, width: u32, height: u32) {
        if self.canvas.width() < width || self.canvas.height() < height {
            self.canvas
                .set_width(self.canvas.width().max(width).next_power_of_two());
            self.canvas
                .set_height(self.canvas.height().max(height).next_power_of_two());
        }
        let c = &self.context;
        c.set_transform(1., 0., 0., 1., 0., 0.).ok();
        c.clear_rect(0., 0., width as f64, height as f64);
    }

    fn read(&self, width: u32, height: u32, left: i32, top: i32) -> Bitmap {
        let Ok(data) = self
            .context
            .get_image_data(0., 0., width as f64, height as f64)
        else {
            return Bitmap::default();
        };
        let mut rgba = data.data().0;
        premultiply(&mut rgba);
        Bitmap {
            width,
            height,
            left,
            top,
            rgba,
        }
    }

    pub fn text(
        &mut self,
        text: &str,
        style: &TextStyle,
        scale: f32,
        fraction: [f32; 2],
    ) -> Bitmap {
        let c = self.context.clone();
        self.set_font(&style.font, style.letter_spacing);
        let Ok(m) = c.measure_text(text) else {
            return Bitmap::default();
        };
        let natural = m.width() as f32;
        let squeeze = match style.max_width {
            Some(max) if natural > max && natural > 0. => max / natural,
            _ => 1.,
        };
        let stroke = style.stroke.map_or(0., |s| s.width / 2.);
        let pad = stroke
            + style
                .shadows
                .iter()
                .map(|s| s.dx.abs().max(s.dy.abs()) + s.blur * 1.5)
                .fold(0., f32::max)
            + 2.;
        let left = (m.actual_bounding_box_left() as f32 * squeeze + pad) * scale;
        let right = (m.actual_bounding_box_right() as f32 * squeeze + pad) * scale;
        let ascent = (m.actual_bounding_box_ascent() as f32 + pad) * scale;
        let descent = (m.actual_bounding_box_descent() as f32 + pad) * scale;
        let (ox, oy) = (left.ceil() as i32 + 1, ascent.ceil() as i32 + 1);
        let width = (ox as f32 + right + fraction[0]).ceil() as u32 + 1;
        let height = (oy as f32 + descent + fraction[1]).ceil() as u32 + 1;
        if width > 4096 || height > 4096 || text.is_empty() {
            return Bitmap::default();
        }
        self.resize(width, height);
        self.set_font(&style.font, style.letter_spacing);
        let s = scale as f64;
        let pen = (
            ox as f64 + fraction[0] as f64,
            oy as f64 + fraction[1] as f64,
        );
        let max_width = style.max_width.map(f64::from);
        let draw = |c: &CanvasRenderingContext2d| {
            if let Some(stroke) = style.stroke {
                c.set_line_width(stroke.width as f64);
                c.set_stroke_style_str(&stroke.color.css());
                c.set_line_join(match stroke.join {
                    Join::Round => "round",
                    Join::Miter => "miter",
                });
                match max_width {
                    Some(w) => c.stroke_text_with_max_width(text, 0., 0., w),
                    None => c.stroke_text(text, 0., 0.),
                }
                .ok();
            }
            c.set_fill_style_str(&style.color.css());
            match max_width {
                Some(w) => c.fill_text_with_max_width(text, 0., 0., w),
                None => c.fill_text(text, 0., 0.),
            }
            .ok();
        };
        // CSS paints the last shadow first; each is cast by a copy drawn
        // outside the canvas so the glyphs are never drawn twice.
        for shadow in style.shadows.iter().rev() {
            c.set_transform(s, 0., 0., s, pen.0 - AWAY, pen.1).ok();
            c.set_shadow_color(&shadow.color.css());
            c.set_shadow_blur((shadow.blur * scale) as f64);
            c.set_shadow_offset_x(AWAY + (shadow.dx * scale) as f64);
            c.set_shadow_offset_y((shadow.dy * scale) as f64);
            draw(&c);
        }
        c.set_shadow_color("transparent");
        c.set_shadow_blur(0.);
        c.set_shadow_offset_x(0.);
        c.set_shadow_offset_y(0.);
        c.set_transform(s, 0., 0., s, pen.0, pen.1).ok();
        draw(&c);
        self.read(width, height, -ox, -oy)
    }

    pub fn shape(&mut self, shape: &Shape, scale: f32, fraction: [f32; 2]) -> Bitmap {
        let Ok(path) = web_sys::Path2d::new_with_path_string(&shape.path) else {
            return Bitmap::default();
        };
        let bounds = shape.bounds;
        let unit = shape.unit * scale;
        let pad = shape.stroke.map_or(0., |s| s.width / 2. * scale)
            + shape
                .drop_shadow
                .map_or(0., |s| (s.dx.abs().max(s.dy.abs()) + s.blur * 1.5) * scale)
            + 2.;
        let ox = (-bounds[0] * unit + pad).ceil() as i32;
        let oy = (-bounds[1] * unit + pad).ceil() as i32;
        let width = (ox as f32 + bounds[2] * unit + pad + fraction[0]).ceil() as u32;
        let height = (oy as f32 + bounds[3] * unit + pad + fraction[1]).ceil() as u32;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Bitmap::default();
        }
        self.resize(width, height);
        let c = &self.context;
        c.set_transform(
            unit as f64,
            0.,
            0.,
            unit as f64,
            ox as f64 + fraction[0] as f64,
            oy as f64 + fraction[1] as f64,
        )
        .ok();
        if let Some(shadow) = shape.drop_shadow {
            c.set_shadow_color(&shadow.color.css());
            c.set_shadow_blur((shadow.blur * scale) as f64);
            c.set_shadow_offset_x((shadow.dx * scale) as f64);
            c.set_shadow_offset_y((shadow.dy * scale) as f64);
        }
        if let Some(stroke) = shape.stroke {
            c.set_line_width((stroke.width / shape.unit) as f64);
            c.set_stroke_style_str(&stroke.color.css());
            c.set_line_join(match stroke.join {
                Join::Round => "round",
                Join::Miter => "miter",
            });
            c.set_line_cap(match stroke.join {
                Join::Round => "round",
                Join::Miter => "butt",
            });
            c.stroke_with_path(&path);
        }
        c.set_fill_style_str(&shape.fill.css());
        c.fill_with_path_2d(&path);
        c.set_shadow_color("transparent");
        c.set_shadow_offset_x(0.);
        c.set_shadow_offset_y(0.);
        self.read(width, height, -ox, -oy)
    }
}
