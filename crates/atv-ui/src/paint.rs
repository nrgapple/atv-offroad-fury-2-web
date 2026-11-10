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

/// A view of the frame turned toward a player around a table (split
/// screen's Face to Face seating): `rect` on the screen (CSS pixels),
/// turned clockwise by `turns` quarter turns. Items inside a frame paint
/// in its own local space, `(0, 0)` to [`Frame::size`], upright for the
/// player it faces.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub rect: Rect,
    /// Clockwise quarter turns: 0 upright, 1 for a player at the screen's
    /// left edge, 2 upside down (the top edge), 3 for the right edge.
    pub turns: u8,
}

impl Frame {
    pub const fn new(rect: Rect, turns: u8) -> Self {
        Self {
            rect,
            turns: turns % 4,
        }
    }

    /// The local width and height (swapped by a quarter turn).
    pub fn size(&self) -> [f32; 2] {
        if self.turns % 2 == 1 {
            [self.rect.h, self.rect.w]
        } else {
            [self.rect.w, self.rect.h]
        }
    }

    /// The screen direction of local +x, and the screen point of the local
    /// origin; local +y is local +x turned a quarter clockwise.
    pub fn axes(&self) -> ([f32; 2], [f32; 2]) {
        let r = self.rect;
        match self.turns % 4 {
            1 => ([0., 1.], [r.right(), r.y]),
            2 => ([-1., 0.], [r.right(), r.bottom()]),
            3 => ([0., -1.], [r.x, r.bottom()]),
            _ => ([1., 0.], [r.x, r.y]),
        }
    }

    /// A local point on the screen.
    pub fn to_screen(&self, x: f32, y: f32) -> [f32; 2] {
        let ([ax, ay], [ox, oy]) = self.axes();
        [ox + x * ax - y * ay, oy + x * ay + y * ax]
    }

    /// A screen point in local space.
    pub fn to_local(&self, x: f32, y: f32) -> [f32; 2] {
        let ([ax, ay], [ox, oy]) = self.axes();
        let (dx, dy) = (x - ox, y - oy);
        [dx * ax + dy * ay, dy * ax - dx * ay]
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
    /// Following images (and image shadows) sway in 3D until `PopTilt`:
    /// rotated about `center` by the renderer's current tilt angles
    /// ([`crate::Renderer::set_tilt`]) and seen through a CSS-style
    /// `perspective` (CSS pixels). The angles live in a uniform, so an
    /// animated sway does not change the display list. Boxes and text are
    /// not tilted.
    PushTilt {
        center: [f32; 2],
        perspective: f32,
    },
    PopTilt,
    /// Following items paint in `Frame`'s local space, clipped to it and
    /// turned onto the screen, until `PopFrame`. Frames do not nest.
    PushFrame(Frame),
    PopFrame,
}

impl Item {
    /// The top and bottom (CSS pixels) of everything the item paints,
    /// generously for text. `None`: not known without drawing it, or it moves
    /// between prepares (rotated or tilted), or it paints nothing itself.
    fn vertical_reach(&self, tilted: bool) -> Option<(f32, f32)> {
        match self {
            Item::Box(b) if b.rotation == 0. => {
                let reach = b
                    .shadows
                    .iter()
                    .filter(|s| !s.inset)
                    .map(|s| s.dy.abs() + s.spread.max(0.) + s.blur * 1.5 + 1.)
                    .fold(0., f32::max);
                Some((b.rect.y - reach, b.rect.bottom() + reach))
            }
            Item::Text { style, y, .. } => {
                let reach = style
                    .shadows
                    .iter()
                    .map(|s| s.dy.abs() + s.blur * 1.5 + 1.)
                    .fold(0., f32::max)
                    + style.stroke.as_ref().map_or(0., |s| s.width)
                    + style.font.size * 2.;
                Some((y - reach, y + reach))
            }
            Item::Image { dst, rotation, .. } if *rotation == 0. && !tilted => {
                Some((dst.y, dst.bottom()))
            }
            Item::ImageShadow { dst, dy, blur, .. } if !tilted => {
                let reach = dy.abs() + blur * 1.5 + 1.;
                Some((dst.y - reach, dst.bottom() + reach))
            }
            _ => None,
        }
    }

    /// The item paints nothing between `top` and `bottom` (a clip's edges, in
    /// CSS pixels): a long scrolled list's rows out of view.
    pub fn outside(&self, top: f32, bottom: f32, tilted: bool) -> bool {
        self.vertical_reach(tilted)
            .is_some_and(|(t, b)| b < top || t > bottom)
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub items: Vec<Item>,
}

impl DisplayList {
    pub fn clear(&mut self) {
        self.items.clear();
    }
    /// Moves every item by (`dx`, `dy`) CSS pixels (a split-screen
    /// player's HUD painted for its own view, placed in the frame).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.translate_range(0..self.items.len(), dx, dy);
    }
    /// [`Self::translate`] for the items in `range`.
    pub fn translate_range(&mut self, range: std::ops::Range<usize>, dx: f32, dy: f32) {
        let shift = |r: &mut Rect| {
            r.x += dx;
            r.y += dy;
        };
        for item in &mut self.items[range] {
            match item {
                Item::Box(b) => {
                    shift(&mut b.rect);
                    b.pivot[0] += dx;
                    b.pivot[1] += dy;
                }
                Item::Text { x, y, .. } | Item::Shape { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
                Item::Image { dst, pivot, .. } => {
                    shift(dst);
                    pivot[0] += dx;
                    pivot[1] += dy;
                }
                Item::ImageShadow { dst, .. } | Item::PushClip { rect: dst, .. } => shift(dst),
                Item::PushTilt { center, .. } => {
                    center[0] += dx;
                    center[1] += dy;
                }
                Item::PushFrame(frame) => shift(&mut frame.rect),
                Item::PopClip
                | Item::PushOpacity(_)
                | Item::PopOpacity
                | Item::PopTilt
                | Item::PopFrame => {}
            }
        }
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
    pub fn frame(&mut self, frame: Frame) {
        self.items.push(Item::PushFrame(frame));
    }
    pub fn unframe(&mut self) {
        self.items.push(Item::PopFrame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(y: f32) -> BoxPaint {
        BoxPaint {
            rect: Rect::new(0., y, 100., 50.),
            ..Default::default()
        }
    }

    #[test]
    fn frames_map_local_points_onto_their_turned_rectangle() {
        let rect = Rect::new(100., 50., 200., 400.);
        // Local top-left corner, and local +x, for each seat.
        let cases = [
            (0, [100., 50.], [1., 0.]),
            (1, [300., 50.], [0., 1.]),
            (2, [300., 450.], [-1., 0.]),
            (3, [100., 450.], [0., -1.]),
        ];
        for (turns, origin, ax) in cases {
            let frame = Frame::new(rect, turns);
            assert_eq!(frame.to_screen(0., 0.), origin, "turns {turns}");
            let [x, y] = frame.to_screen(1., 0.);
            assert_eq!([x - origin[0], y - origin[1]], ax, "turns {turns}");
            let [w, h] = frame.size();
            // The local far corner is the rectangle's opposite corner.
            let opposite = [400. - origin[0], 500. - origin[1]];
            assert_eq!(frame.to_screen(w, h), opposite, "turns {turns}");
            for p in [[10., 20.], [w - 1., 3.], [5., h - 7.]] {
                let s = frame.to_screen(p[0], p[1]);
                assert!(rect.contains(s[0], s[1]), "turns {turns} {p:?}");
                assert_eq!(frame.to_local(s[0], s[1]), p, "turns {turns}");
            }
        }
        // A player at the left edge reads local +y toward the left edge.
        let left = Frame::new(rect, 1);
        assert_eq!(left.to_screen(0., 10.), [290., 50.]);
    }

    #[test]
    fn only_items_that_paint_nothing_inside_a_clip_are_outside_it() {
        let outside = |item: &Item| item.outside(100., 200., false);
        assert!(outside(&Item::Box(card(0.))));
        assert!(outside(&Item::Box(card(201.))));
        assert!(!outside(&Item::Box(card(60.))));
        // A shadow reaches into the clip.
        let shadow = BoxShadow::outer(0., 20., 8., crate::color::Color::WHITE);
        let shaded = BoxPaint {
            shadows: vec![shadow],
            ..card(40.)
        };
        assert!(!outside(&Item::Box(shaded)));
        // Rotated boxes and tilted images are not judged by their rectangle.
        let turned = BoxPaint {
            rotation: 1.,
            ..card(0.)
        };
        assert!(!outside(&Item::Box(turned)));
        let image = Item::Image {
            image: ImageId(1),
            src: Rect::new(0., 0., 1., 1.),
            dst: Rect::new(0., 0., 100., 50.),
            rotation: 0.,
            pivot: [0., 0.],
        };
        assert!(outside(&image));
        assert!(!image.outside(100., 200., true));
        // Clip and state items always stay.
        assert!(!outside(&Item::PopClip));
    }
}
