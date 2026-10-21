//! The game's user interface drawn by the engine: CSS-like boxes laid out with
//! taffy and cached text in the site's display face, following the authored
//! browser controls (web/design-system.css, hud.css, touch.css). Original HUD
//! artwork and menus are later migration checkpoints.
pub mod color;
pub mod front;
pub mod hud;
pub mod layout;
pub mod menu;
pub mod paint;
pub mod render;
pub mod settings;
pub mod text;
pub mod touch;

pub use color::{Color, Fill, Gradient};
pub use paint::{BoxPaint, BoxShadow, DisplayList, ImageId, Item, Rect};
pub use render::Renderer;
pub use text::{Font, Fonts, Shape, TextStyle};

pub use taffy;
