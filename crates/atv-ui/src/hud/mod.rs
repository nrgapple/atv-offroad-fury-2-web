//! In-ride HUD pieces drawn by the engine.
pub mod assets;
pub mod games;
pub mod help;
pub mod meter;
pub mod minimap;
pub mod objective;
pub mod peers;
pub mod race;
pub mod results;
pub mod split;
pub mod toolbar;
pub mod training;

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
