//! The original speed/preload meter (OverlayObject-5) and stunt text
//! (OverlayObject-12), placed as web/hud.js `draw` places them: the 640x448
//! logical frame scaled by height (wide screens extend horizontally), with the
//! compact portrait layout below 560 CSS pixels. Values and animation clocks
//! come from atv_core::hud; the text keeps the original glyph metrics but is
//! drawn in the display face at final resolution, as the browser game does.
use super::assets::{FONT_DEFAULT, OverlayAssets, Section};
use crate::{
    color::Color,
    paint::{DisplayList, ImageId, Item, Rect},
    text::{Font, Fonts, Join, TextStyle},
};
use atv_core::hud::Frame;
use std::collections::HashMap;

pub struct Meter {
    pub assets: OverlayAssets,
    /// Uploaded icon textures by name.
    pub images: HashMap<String, ImageId>,
    /// King of the Hills' tinted `koh_pillbkg` copies, when loaded.
    pub pills: Option<super::peers::PillArt>,
}

pub(super) struct Logical {
    /// CSS pixels per logical unit.
    pub unit: f32,
    pub logical_width: f32,
}

impl Meter {
    pub fn new(assets: OverlayAssets) -> Self {
        Self {
            assets,
            images: HashMap::new(),
            pills: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn text(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        f: &Logical,
        value: &str,
        at: [f32; 2],
        layout: &Section,
        index: usize,
        scale: f32,
        color: Option<Color>,
        alpha: Option<f32>,
        offset: [f32; 2],
    ) {
        let prefix = format!("text{index}");
        let name = layout
            .text(&format!("{prefix}font"))
            .unwrap_or_else(|| FONT_DEFAULT.into())
            .to_ascii_lowercase();
        let Some(font) = self.assets.fonts.get(&name) else {
            return;
        };
        let color = color
            .or_else(|| {
                layout
                    .get(&format!("{prefix}color"))
                    .and_then(Color::from_triplet)
            })
            .unwrap_or(Color::WHITE);
        let (width, height) = font.measure(value);
        let position = layout.pair(&format!("{prefix}offset")).unwrap_or([0., 0.]);
        let alignment = layout
            .get(&format!("{prefix}justification"))
            .unwrap_or("L")
            .to_ascii_uppercase();
        let mut scale = scale * layout.number(&format!("{prefix}scale")).unwrap_or(1.);
        scale = scale.min((f.logical_width - 20.) / width.max(1.));
        let alpha = alpha
            .unwrap_or_else(|| layout.number(&format!("{prefix}alpha")).unwrap_or(255.))
            / 255.;
        if alpha <= 0. || value.is_empty() {
            return;
        }
        let x = match alignment.as_str() {
            "R" => -width,
            "H" | "B" => -width / 2.,
            _ => 0.,
        };
        let y = if matches!(alignment.as_str(), "V" | "B") {
            -height / 2.
        } else {
            0.
        };
        // ctx.translate(at + position + offset) then scale(scale).
        let k = f.unit * scale;
        let left = (at[0] + position[0] + offset[0]) * f.unit + x * k;
        let top = (at[1] + position[1] + offset[1]) * f.unit + y * k;
        let mut style = TextStyle::new(Font::display(height * k, 800, true), color.fade(alpha))
            .stroke(2. * k, Color::hex("#080808bb").fade(alpha), Join::Round);
        style.max_width = Some(width * k);
        let baseline = fonts.top_offset(&style.font);
        list.text(value, &style, left, top + baseline);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn icon(
        &self,
        list: &mut DisplayList,
        f: &Logical,
        at: [f32; 2],
        layout: &Section,
        index: usize,
        alpha: Option<f32>,
        fill: Option<f32>,
    ) {
        let prefix = format!("icon{index}");
        let Some(name) = layout.text(&format!("{prefix}texture")) else {
            return;
        };
        let stem = name.trim_end_matches(".tga").trim_end_matches(".pst");
        let (Some(&image), Some(source)) = (self.images.get(stem), self.assets.images.get(stem))
        else {
            return;
        };
        let offset = layout.pair(&format!("{prefix}offset")).unwrap_or([0., 0.]);
        let scale = layout.number(&format!("{prefix}scale")).unwrap_or(1.);
        let width = layout
            .number(&format!("{prefix}width"))
            .unwrap_or(source.width as f32)
            * scale;
        let full = layout
            .number(&format!("{prefix}height"))
            .unwrap_or(source.height as f32)
            * scale;
        let mut height = full;
        let mut y = at[1] + offset[1];
        // 002e2290 stretches the whole orange image up from its bottom edge.
        if let Some(fill) = fill {
            height *= fill;
            y += full - height;
        }
        let alpha = alpha
            .unwrap_or_else(|| layout.number(&format!("{prefix}alpha")).unwrap_or(255.))
            / 255.;
        if height <= 0. || width <= 0. || alpha <= 0. {
            return;
        }
        list.opacity(alpha);
        list.push(Item::Image {
            image,
            src: Rect::new(0., 0., source.width as f32, source.height as f32),
            dst: Rect::new(
                (at[0] + offset[0]) * f.unit,
                y * f.unit,
                width * f.unit,
                height * f.unit,
            ),
            rotation: 0.,
            pivot: [0., 0.],
        });
        list.unopacity();
    }

    /// Paints the meter (when `speed_visible`) and any stunt text for a
    /// `width` x `height` CSS-pixel viewport.
    pub fn paint(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        hud: &Frame,
        speed_visible: bool,
        width: f32,
        height: f32,
    ) {
        self.paint_speed(fonts, list, hud, speed_visible, width, height);
        self.paint_stunt(fonts, list, hud, width, height);
    }

    /// hud.js's frame: the 640x448 logical frame scaled by height (wide
    /// screens extend horizontally), compact portrait below 560 px.
    fn frame(width: f32, height: f32) -> (Logical, f32, bool) {
        let compact = width < 560.;
        let unit = if compact {
            (width / 400.).min(height / 448.)
        } else {
            height / 448.
        };
        (
            Logical {
                unit,
                logical_width: width / unit,
            },
            height / unit,
            compact,
        )
    }

    /// The speed and preload meter.
    pub fn paint_speed(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        hud: &Frame,
        speed_visible: bool,
        width: f32,
        height: f32,
    ) {
        let (f, logical_height, compact) = Self::frame(width, height);
        if speed_visible {
            let at = if compact {
                [f.logical_width - 24., 72.]
            } else {
                [f.logical_width - 43., logical_height - 76.]
            };
            let speed = &self.assets.speed;
            // Native draw depth: background (5), blue (4), orange (2), highlight (1).
            self.icon(list, &f, at, speed, 2, None, None);
            if hud.preload_visible {
                self.icon(
                    list,
                    &f,
                    at,
                    speed,
                    1,
                    Some(hud.preload_blue_alpha as f32),
                    None,
                );
                self.icon(list, &f, at, speed, 4, None, Some(hud.preload_fill));
            }
            self.icon(list, &f, at, speed, 3, None, None);
            for (index, value) in [(1, hud.speed_text.as_str()), (2, hud.units)] {
                self.text(
                    fonts,
                    list,
                    &f,
                    value,
                    at,
                    speed,
                    index,
                    1.,
                    None,
                    None,
                    [0., 0.],
                );
            }
        }
    }

    /// The stunt name and points.
    pub fn paint_stunt(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        hud: &Frame,
        width: f32,
        height: f32,
    ) {
        let (f, logical_height, compact) = Self::frame(width, height);
        if hud.stunt_visible {
            let stunt = &self.assets.stunt;
            let at = [
                f.logical_width / 2.,
                if compact {
                    logical_height * 0.62
                } else {
                    logical_height - 100.
                },
            ];
            let [r, g, b] = hud.stunt_color.map(|c| c.clamp(0, 255) as u8);
            let color = Some(Color::rgb(r, g, b));
            let alpha = Some(hud.stunt_alpha as f32);
            self.text(
                fonts,
                list,
                &f,
                &hud.stunt_name,
                at,
                stunt,
                1,
                1.,
                color,
                alpha,
                hud.stunt_offsets[0],
            );
            self.text(
                fonts,
                list,
                &f,
                &hud.stunt_points,
                at,
                stunt,
                2,
                hud.stunt_scale,
                color,
                alpha,
                hud.stunt_offsets[1],
            );
        }
    }
}
