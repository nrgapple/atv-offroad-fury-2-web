//! A frame's UI as a list of CSS-like paint operations in CSS pixels.
use crate::{
    color::{Color, Fill},
    text::{Shape, TextStyle},
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn inset(&self, t: f32, r: f32, b: f32, l: f32) -> Self {
        Self::new(self.x + l, self.y + t, self.w - l - r, self.h - t - b)
    }
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// CSS `box-shadow`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
}

impl BoxShadow {
    pub const fn outer(dx: f32, dy: f32, blur: f32, color: Color) -> Self {
        Self {
            dx,
            dy,
            blur,
            spread: 0.,
            color,
            inset: false,
        }
    }
    pub const fn inset(dx: f32, dy: f32, blur: f32, color: Color) -> Self {
        Self {
            dx,
            dy,
            blur,
            spread: 0.,
            color,
            inset: true,
        }
    }
}

/// A CSS box: background, border, radii and shadows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxPaint {
    pub rect: Rect,
    /// Top-left, top-right, bottom-right, bottom-left.
    pub radii: [f32; 4],
    /// Vertical radii of elliptical corners (`border-radius: a / b`); 0 keeps
    /// the corner circular.
    pub radii_y: [f32; 4],
    pub fill: Option<Fill>,
    /// Top, right, bottom, left widths.
    pub border: [f32; 4],
    pub border_color: Color,
    pub shadows: Vec<BoxShadow>,
    /// The fill paints only the border ring (an SVG circle stroke).
    pub ring: bool,
    /// Clockwise rotation (radians) about `pivot` (CSS pixels).
    pub rotation: f32,
    pub pivot: [f32; 2],
    /// CSS `skewX`: tan(angle); x shifts by (y - pivot.y) * skew.
    pub skew_x: f32,
}

/// Registered image (its own GPU texture), e.g. original HUD artwork.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Box(BoxPaint),
    /// One line of text with its pen (baseline origin) at `x`, `y`.
    Text {
        text: String,
        style: TextStyle,
        x: f32,
        y: f32,
    },
    /// A vector shape with its local origin at `x`, `y`, rotated (radians,
    /// clockwise on screen) about that origin.
    Shape {
        shape: Shape,
        x: f32,
        y: f32,
        rotation: f32,
    },
    /// `drawImage`: source texels into `dst`, rotated about `pivot`.
    Image {
        image: ImageId,
        src: Rect,
        dst: Rect,
        rotation: f32,
        pivot: [f32; 2],
    },
    /// CSS `filter: drop-shadow()` of an image: its alpha, offset, blurred
    /// (`blur` is the CSS radius) and tinted.
    ImageShadow {
        image: ImageId,
        src: Rect,
        dst: Rect,
        dx: f32,
        dy: f32,
        blur: f32,
        color: crate::color::Color,
    },
    /// Clip following items to a rounded rectangle until `PopClip`.
    PushClip {
        rect: Rect,
        radius: f32,
    },
    PopClip,
    /// Multiply following items' alpha until `PopOpacity`.
    PushOpacity(f32),
    PopOpacity,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub items: Vec<Item>,
}

impl DisplayList {
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn push(&mut self, item: Item) {
        self.items.push(item);
    }
    pub fn rect(&mut self, rect: Rect, fill: impl Into<Fill>) {
        self.items.push(Item::Box(BoxPaint {
            rect,
            fill: Some(fill.into()),
            ..Default::default()
        }));
    }
    pub fn text(&mut self, text: impl Into<String>, style: &TextStyle, x: f32, y: f32) {
        self.items.push(Item::Text {
            text: text.into(),
            style: style.clone(),
            x,
            y,
        });
    }
    pub fn clip(&mut self, rect: Rect, radius: f32) {
        self.items.push(Item::PushClip { rect, radius });
    }
    pub fn unclip(&mut self) {
        self.items.push(Item::PopClip);
    }
    pub fn opacity(&mut self, opacity: f32) {
        self.items.push(Item::PushOpacity(opacity));
    }
    pub fn unopacity(&mut self) {
        self.items.push(Item::PopOpacity);
    }
}
