//! In-ride HUD pieces drawn by the engine.
pub mod assets;
pub mod chat;
pub mod classic;
pub mod games;
pub mod help;
pub mod hub;
pub mod meter;
pub mod minimap;
pub mod objective;
pub mod palette;
pub mod peers;
pub mod quickplay;
pub mod race;
pub mod results;
pub mod riders;
pub mod split;
pub mod tips;
pub mod toolbar;
pub mod training;
pub mod viewer;

/// The viewport in CSS pixels and the device traits media queries test.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    /// Physical pixels per CSS pixel.
    pub scale: f32,
    /// `env(safe-area-inset-*)`: top, right, bottom, left.
    pub safe: [f32; 4],
    /// `(hover: hover)`.
    pub hover: bool,
    /// `(any-pointer: coarse)`.
    pub coarse: bool,
    /// `(hover: none) and (pointer: coarse)`: touch-first phones and tablets.
    pub touch_first: bool,
}

impl Viewport {
    /// A panel at most `width` wide, `margin` clear of each side and of the
    /// side safe-area insets (a landscape notch, or a side status bar in
    /// portrait such as the iPhone Duo's), centred between them:
    /// (left, width).
    pub fn centred(&self, width: f32, margin: f32) -> (f32, f32) {
        let [left, right] = [self.safe[3], self.safe[1]];
        let room = self.width - left - right;
        let width = width.min((room - 2. * margin).max(0.));
        (left + (room - width) / 2., width)
    }
}

/// web/design-system.css and hud.css tokens.
pub mod tokens {
    use crate::{color::Color, text::Shadow};
    pub const INK: Color = Color::hex("#080808");
    pub const SILVER: Color = Color::hex("#b9b9b9");
    pub const SILVER_LIGHT: Color = Color::hex("#e9e9e9");
    pub const GOLD: Color = Color::hex("#e5d360");
    pub const GOLD_LIGHT: Color = Color::hex("#fffaa1");
    pub const ORANGE: Color = Color::hex("#f4a13c");
    pub const HUD_GOLD: Color = Color::hex("#f1d84e");
    /// `--hud-text-shadow: 2px 2px 0 #000c, 0 0 8px #0007`.
    pub const HUD_TEXT_SHADOW: [Shadow; 2] = [
        Shadow {
            dx: 2.,
            dy: 2.,
            blur: 0.,
            color: Color::hex("#000c"),
        },
        Shadow {
            dx: 0.,
            dy: 0.,
            blur: 8.,
            color: Color::hex("#0007"),
        },
    ];
}

#[cfg(test)]
mod tests {
    use super::Viewport;

    /// Panels centre between the side insets and keep their margin there.
    #[test]
    fn panels_centre_between_the_side_insets() {
        let view = |safe| Viewport {
            width: 430.,
            height: 932.,
            safe,
            ..Default::default()
        };
        assert_eq!(view([0.; 4]).centred(340., 12.), (45., 340.));
        // The iPhone Duo's status column on the right in portrait.
        assert_eq!(view([20., 62., 34., 0.]).centred(560., 12.), (12., 344.));
        assert_eq!(view([20., 62., 34., 0.]).centred(300., 12.), (34., 300.));
        // Mirrored, and no room at all.
        assert_eq!(view([0., 0., 0., 62.]).centred(560., 12.), (74., 344.));
        assert_eq!(view([0., 300., 0., 200.]).centred(560., 12.).1, 0.);
    }
}
