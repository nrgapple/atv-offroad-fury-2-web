//! Quick chat on the ride HUD (research/QUICK_CHAT.md): bubbles over the
//! senders' riders, a short feed, horn rings and the pick wheel. Authored
//! port presentation; the PS2 game had no chat of this kind.
use super::{Viewport, peers};
use crate::{
    color::Color,
    paint::{BoxPaint, DisplayList, Item, Rect},
    text::{Font, Fonts, Join, Shape, Stroke, TextStyle},
};

/// Hit targets: the wheel's lines or emotes, its page switch, the horn and
/// the backdrop that closes it.
pub const LINE_HIT: u32 = 300;
pub const EMOTE_HIT: u32 = 310;
pub const HORN_HIT: u32 = 320;
pub const CLOSE_HIT: u32 = 321;
pub const PAGE_HIT: u32 = 322;

/// How long a bubble, a feed line and a horn ring show (s).
pub const BUBBLE_S: f32 = 4.;
pub const FEED_S: f32 = 6.;
pub const HORN_S: f32 = 0.8;

/// A speech bubble (the toolbar's CHAT button).
pub const CHAT_ICON: &str =
    "M3 3h14a1 1 0 0 1 1 1v9a1 1 0 0 1 -1 1H8l-4 4v-4H3a1 1 0 0 1 -1 -1V4a1 1 0 0 1 1 -1z";
/// A horn: a flared bell and sound waves.
pub const HORN_ICON: &str = "M2 8h3l7 -5v14l-7 -5H2zM14 6h2v8h-2z";

const GOLD: Color = Color::rgb(255, 196, 40);
const FACE: Color = Color::rgb(255, 210, 60);
const INK: Color = Color::rgb(40, 24, 8);
const RED: Color = Color::rgb(235, 50, 60);
const ORANGE: Color = Color::rgb(255, 120, 30);
const DARK: Color = Color::rgb(20, 20, 20);
const FACE_PATH: &str = "M10 1a9 9 0 1 0 0 18a9 9 0 1 0 0 -18z";

/// Each emote (atv_net::chat::EMOTES order) as filled layers in a 20 unit
/// box: thumbs, laugh, wow, cool, heart, star, fire, trophy.
pub const EMOTE_ART: [&[(&str, Color)]; 8] = [
    &[(
        "M2 9h3v9H2zM6 9l4 -7c1.5 0 2.5 1 2.2 2.6L11.6 8H16c1.2 0 2 1 1.8 2.1l-1.2 6.2c-.2 1 -1 1.7 -2 1.7H6z",
        GOLD,
    )],
    &[
        (FACE_PATH, FACE),
        ("M5.5 6h2.5v2.5H5.5zM12 6h2.5v2.5H12z", INK),
        ("M4.5 11h11a5.5 5.5 0 0 1 -11 0z", INK),
    ],
    &[
        (FACE_PATH, FACE),
        ("M6 5.5h2v3H6zM12 5.5h2v3h-2z", INK),
        ("M10 11a2.5 3 0 1 0 0 6a2.5 3 0 1 0 0 -6z", INK),
    ],
    &[
        (FACE_PATH, FACE),
        ("M2.5 6h15v2l-1.5 3h-4l-1 -2h-2l-1 2h-4l-1.5 -3z", DARK),
        ("M6 13h8a4 3 0 0 1 -8 0z", INK),
    ],
    &[(
        "M10 18L2.6 10.6a4.6 4.6 0 0 1 7.4 -5.4a4.6 4.6 0 0 1 7.4 5.4z",
        RED,
    )],
    &[(
        "M10 1l2.6 5.9l6.4 .6l-4.8 4.3l1.4 6.3L10 14.8l-5.6 3.3l1.4 -6.3L1 7.5l6.4 -.6z",
        GOLD,
    )],
    &[
        (
            "M10 1c1 4 6 6 6 11a6 6 0 0 1 -12 0c0 -3 2 -4 2 -7c2 1 3 3 3 5c1 -2 1 -6 1 -9z",
            ORANGE,
        ),
        ("M10 11c1 2 3 3 3 5a3 3 0 0 1 -6 0c0 -2 2 -3 3 -5z", GOLD),
    ],
    &[
        (
            "M5 2h10v5a5 5 0 0 1 -4 4.9V15h3v3H6v-3h3v-3.1A5 5 0 0 1 5 7z",
            GOLD,
        ),
        (
            "M1.5 3H5v2H3.5v1a1.5 1.5 0 0 0 1.5 1.5v2A3.5 3.5 0 0 1 1.5 6z",
            GOLD,
        ),
        (
            "M18.5 3H15v2h1.5v1a1.5 1.5 0 0 1 -1.5 1.5v2a3.5 3.5 0 0 0 3.5 -3.5z",
            GOLD,
        ),
    ],
];

/// What a bubble or feed line shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Said {
    Line(&'static str),
    Emote(u8),
    Horn,
}

/// A bubble over a rider, `age` seconds old.
#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub slot: u8,
    pub position: [f32; 3],
    pub said: Said,
    pub age: f32,
}

/// A feed line: who (None for the player) and what.
#[derive(Clone, Debug, PartialEq)]
pub struct Feed {
    pub slot: Option<u8>,
    pub name: std::rc::Rc<str>,
    pub said: Said,
    pub age: f32,
}

/// The HUD's quick chat this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// The room's server relays chat: the CHAT button shows.
    pub available: bool,
    pub bubbles: Vec<Bubble>,
    pub feed: Vec<Feed>,
    /// Which kinds the player's budget allows now: lines, emotes, horn.
    pub ready: [bool; 3],
}

fn shape(path: &str, fill: Color, stroke: Option<Stroke>) -> Shape {
    Shape {
        path: path.into(),
        bounds: [0., 0., 20., 20.],
        unit: 1.,
        fill,
        stroke,
        drop_shadow: None,
    }
}

/// Draws an emote's layers `size` px wide, centred on `x`, `y`.
pub fn paint_emote(list: &mut DisplayList, emote: u8, x: f32, y: f32, size: f32) {
    let Some(layers) = EMOTE_ART.get(usize::from(emote)) else {
        return;
    };
    for (i, (path, color)) in layers.iter().enumerate() {
        let mut s = shape(path, *color, None);
        s.unit = size / 20.;
        if i == 0 {
            s.stroke = Some(Stroke {
                width: 1.2,
                color: Color::BLACK,
                join: Join::Round,
            });
        }
        list.push(Item::Shape {
            shape: s,
            x: x - size / 2.,
            y: y - size / 2.,
            rotation: 0.,
        });
    }
}

fn icon(list: &mut DisplayList, path: &str, color: Color, x: f32, y: f32, size: f32) {
    let mut s = shape(path, color, None);
    s.unit = size / 20.;
    list.push(Item::Shape {
        shape: s,
        x: x - size / 2.,
        y: y - size / 2.,
        rotation: 0.,
    });
}

fn label(size: f32) -> TextStyle {
    TextStyle::new(Font::display(size, 800, true), Color::WHITE)
        .stroke(2., Color::BLACK, Join::Round)
        .shadow(0., 1., 1., Color::BLACK)
}

fn fade(age: f32, life: f32) -> f32 {
    ((life - age) / 0.4).clamp(0., 1.)
}

/// Bubbles over the senders' riders (above their name tags) and horn rings
/// at their quads.
pub fn paint_bubbles(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    view: &Viewport,
    bubbles: &[Bubble],
    vp: &[f32; 16],
) {
    for b in bubbles {
        let Some(marker) = peers::screen_marker(b.position, vp, view.width, view.height) else {
            continue;
        };
        if marker.edge {
            continue;
        }
        if b.said == Said::Horn {
            let Some(at) = peers::screen_marker(
                [b.position[0], b.position[1] - 6., b.position[2]],
                vp,
                view.width,
                view.height,
            ) else {
                continue;
            };
            let t = (b.age / HORN_S).clamp(0., 1.);
            let r = 14. + 46. * t;
            list.push(Item::Box(BoxPaint {
                rect: Rect::new(at.x - r, at.y - r, r * 2., r * 2.),
                radii: [r; 4],
                border: [3.; 4],
                border_color: peers::color(b.slot).fade(1. - t),
                ..Default::default()
            }));
            continue;
        }
        let alpha = fade(b.age, BUBBLE_S);
        list.opacity(alpha);
        // Above the 44 px name tag.
        let (w, h) = match b.said {
            Said::Line(text) => (fonts.measure(text, &label(16.).font, 0.).width + 20., 32.),
            _ => (44., 44.),
        };
        let x = marker
            .x
            .clamp(w / 2. + 6., (view.width - w / 2. - 6.).max(w / 2. + 6.));
        let bottom = (marker.y - 50.).clamp(h + 6., view.height - 6.);
        let rect = Rect::new(x - w / 2., bottom - h, w, h);
        list.push(Item::Box(BoxPaint {
            rect,
            radii: [h / 2.; 4],
            fill: Some(Color::hex("#101010d9").into()),
            border: [2.; 4],
            border_color: peers::color(b.slot),
            ..Default::default()
        }));
        match b.said {
            Said::Line(text) => {
                let style = label(16.);
                let width = fonts.measure(text, &style.font, 0.).width;
                list.text(text, &style, x - width / 2., bottom - h / 2. + 6.);
            }
            Said::Emote(e) => paint_emote(list, e, x, bottom - h / 2., 32.),
            Said::Horn => {}
        }
        list.unopacity();
    }
}

/// The feed: the latest few events under the toolbar, newest last.
pub fn paint_feed(fonts: &mut Fonts, list: &mut DisplayList, view: &Viewport, feed: &[Feed]) {
    let x = view.safe[3].max(14.);
    let mut y = view.safe[0].max(14.) + 44. + 30.;
    for f in feed {
        list.opacity(fade(f.age, FEED_S));
        let style = TextStyle::new(
            Font::display(15., 800, true),
            match f.slot {
                Some(slot) => peers::color(slot),
                None => GOLD,
            },
        )
        .stroke(2., Color::BLACK, Join::Round);
        let who = format!("{}:", f.name);
        list.text(&who, &style, x, y);
        let after = x + fonts.measure(&who, &style.font, 0.).width + 6.;
        match f.said {
            Said::Line(text) => list.text(text, &label(15.), after, y),
            Said::Emote(e) => paint_emote(list, e, after + 10., y - 5., 20.),
            Said::Horn => icon(list, HORN_ICON, Color::WHITE, after + 10., y - 5., 18.),
        }
        list.unopacity();
        y += 24.;
    }
}

/// The wheel's centre and line radius in a `view`.
pub fn wheel_geometry(view: &Viewport) -> (f32, f32, f32) {
    let cx = view.width / 2.;
    let cy = view.height / 2.;
    let r = 130f32
        .min(view.width / 2. - 50.)
        .min(view.height / 2. - 30.)
        .max(96.);
    (cx, cy, r)
}

const PILL: (f32, f32) = (84., 48.);
const DOT: f32 = 48.;

/// Each wheel target's rectangle and hit id: eight lines (or with
/// `emotes` eight emotes) round the ring, then the page switch and the horn
/// in the middle.
pub fn wheel_targets(view: &Viewport, emotes: bool) -> [(Rect, u32); 10] {
    let (cx, cy, r) = wheel_geometry(view);
    std::array::from_fn(|i| match i {
        0..8 => {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::FRAC_PI_4;
            let (x, y) = (cx + a.cos() * r, cy + a.sin() * r);
            if emotes {
                (
                    Rect::new(x - DOT / 2., y - DOT / 2., DOT, DOT),
                    EMOTE_HIT + i as u32,
                )
            } else {
                (
                    Rect::new(x - PILL.0 / 2., y - PILL.1 / 2., PILL.0, PILL.1),
                    LINE_HIT + i as u32,
                )
            }
        }
        8 => (Rect::new(cx - DOT / 2., cy - DOT - 4., DOT, DOT), PAGE_HIT),
        _ => (Rect::new(cx - DOT / 2., cy + 4., DOT, DOT), HORN_HIT),
    })
}

/// The open wheel: a dimmed backdrop that closes it, then the targets.
/// `focus` is the controller's highlighted target (its index in
/// [`wheel_targets`]) and `hovered` the pointer's hit.
#[allow(clippy::too_many_arguments)]
pub fn paint_wheel(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    hits: &mut Vec<(Rect, u32)>,
    view: &Viewport,
    chat: &View,
    lines: &[&str; 8],
    emotes: bool,
    focus: Option<usize>,
    hovered: Option<u32>,
) {
    let (cx, cy, r) = wheel_geometry(view);
    let full = Rect::new(0., 0., view.width, view.height);
    list.rect(full, Color::hex("#0006"));
    hits.push((full, CLOSE_HIT));
    let disc = r + 48.;
    list.push(Item::Box(BoxPaint {
        rect: Rect::new(cx - disc, cy - disc, disc * 2., disc * 2.),
        radii: [disc; 4],
        fill: Some(Color::hex("#080808b0").into()),
        ..Default::default()
    }));
    for (index, (rect, id)) in wheel_targets(view, emotes).into_iter().enumerate() {
        let ready = match id {
            HORN_HIT => chat.ready[2],
            PAGE_HIT => true,
            id if id >= EMOTE_HIT => chat.ready[1],
            _ => chat.ready[0],
        };
        let lit = focus == Some(index) || hovered == Some(id);
        let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
        list.opacity(if ready { 1. } else { 0.4 });
        list.push(Item::Box(BoxPaint {
            rect,
            radii: [rect.h / 2.; 4],
            fill: Some(
                if lit {
                    Color::hex("#ffc428e6")
                } else {
                    Color::hex("#1a1a1ae6")
                }
                .into(),
            ),
            border: [2.; 4],
            border_color: if lit {
                Color::WHITE
            } else {
                Color::hex("#ffffff59")
            },
            ..Default::default()
        }));
        match id {
            HORN_HIT => icon(list, HORN_ICON, Color::WHITE, x, y, 26.),
            // The other page's look: a face for emotes, a bubble for lines.
            PAGE_HIT if emotes => icon(list, CHAT_ICON, Color::WHITE, x, y, 24.),
            PAGE_HIT => paint_emote(list, 1, x, y, 26.),
            id if id >= EMOTE_HIT => paint_emote(list, (id - EMOTE_HIT) as u8, x, y, 30.),
            id => {
                let text = lines[(id - LINE_HIT) as usize];
                let style = label(14.);
                let width = fonts.measure(text, &style.font, 0.).width;
                list.text(text, &style, x - width / 2., y + 5.);
            }
        }
        list.unopacity();
        if ready {
            hits.push((rect, id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(width: f32, height: f32) -> Viewport {
        Viewport {
            width,
            height,
            scale: 1.,
            safe: [0.; 4],
            hover: false,
            coarse: true,
            touch_first: true,
        }
    }

    #[test]
    fn wheel_targets_are_touch_sized_apart_and_on_screen() {
        for v in [view(360., 640.), view(640., 360.), view(1280., 720.)] {
            for emotes in [false, true] {
                let targets = wheel_targets(&v, emotes);
                for (i, (a, _)) in targets.iter().enumerate() {
                    assert!(a.w >= 48. && a.h >= 48., "{a:?}");
                    assert!(
                        a.x >= 0. && a.y >= 0. && a.right() <= v.width && a.bottom() <= v.height,
                        "{a:?} in {v:?}"
                    );
                    for (b, _) in &targets[i + 1..] {
                        let apart = a.right() <= b.x
                            || b.right() <= a.x
                            || a.bottom() <= b.y
                            || b.bottom() <= a.y;
                        assert!(apart, "{a:?} overlaps {b:?} in {v:?}");
                    }
                }
            }
        }
    }
}
