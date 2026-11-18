//! Quick chat on the ride HUD (research/QUICK_CHAT.md): bubbles over the
//! senders' riders, a short feed, horn rings and the pick wheel. Authored
//! port presentation; the PS2 game had no chat of this kind. Bubbles and
//! the feed are lettered like the name tags; the wheel's targets are bare
//! race-HUD lettering, overlay silver and gold when lit, on a soft dark
//! glow, so it reads as part of the HUD rather than a panel of buttons.
use super::race::{RACE_GOLD, RACE_TEXT};
use super::{Viewport, classic, peers};
use crate::{
    color::{Color, Fill},
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
    /// The rider's ATV paint: its colour ([`peers::rider_color`]).
    pub paint: u8,
    pub position: [f32; 3],
    pub said: Said,
    pub age: f32,
}

/// A feed line: who (None for the player) and what.
#[derive(Clone, Debug, PartialEq)]
pub struct Feed {
    pub slot: Option<u8>,
    /// The rider's ATV paint, as [`Bubble::paint`].
    pub paint: u8,
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
                border_color: peers::rider_color(b.slot, b.paint).fade(1. - t),
                ..Default::default()
            }));
            continue;
        }
        let alpha = fade(b.age, BUBBLE_S);
        list.opacity(alpha);
        // Lettered like the name tag under it (hud::peers), above its 44 px.
        let style = label(18.);
        let (w, h) = match b.said {
            Said::Line(text) => (fonts.measure(text, &style.font, 0.).width, 24.),
            _ => (40., 40.),
        };
        let x = marker
            .x
            .clamp(w / 2. + 6., (view.width - w / 2. - 6.).max(w / 2. + 6.));
        let bottom = (marker.y - 50.).clamp(h + 6., view.height - 6.);
        match b.said {
            Said::Line(text) => list.text(text, &style, x - w / 2., bottom - 5.),
            Said::Emote(e) => paint_emote(list, e, x, bottom - h / 2., 36.),
            Said::Horn => {}
        }
        list.unopacity();
    }
}

/// The feed: the latest few events under the toolbar (or under a panel
/// ending at `below`), newest last.
pub fn paint_feed(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    view: &Viewport,
    feed: &[Feed],
    below: f32,
) {
    let x = view.safe[3].max(14.);
    let mut y = (view.safe[0].max(14.) + 44. + 30.).max(below + 24.);
    for f in feed {
        list.opacity(fade(f.age, FEED_S));
        let fill = match f.slot {
            Some(slot) => peers::rider_color(slot, f.paint),
            None => classic::VALUE,
        };
        // A black-painted rider's name keeps a light outline to read.
        let style = TextStyle::new(Font::display(15., 800, true), fill).stroke(
            2.,
            peers::outline(fill, Color::BLACK),
            Join::Round,
        );
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

/// The wheel's smallest ring: its 84 x 48 line targets clear their
/// diagonal neighbours from 119 px.
const MIN_RING: f32 = 120.;
/// The height the wheel needs below an open panel: its smallest ring with
/// a 48 px target and margin at each end.
pub const WHEEL_SPACE: f32 = 2. * (MIN_RING + 30.);

/// The wheel's centre and line radius in a `view`, centred in the space
/// below `top` (an open panel's bottom edge, 0 for none).
pub fn wheel_geometry(view: &Viewport, top: f32) -> (f32, f32, f32) {
    let top = top.clamp(0., (view.height - WHEEL_SPACE).max(0.));
    let room = view.height - top;
    // Centred between the side insets (a notch, a side status bar).
    let (left, span) = view.centred(view.width, 0.);
    let cx = left + span / 2.;
    let cy = top + room / 2.;
    let r = 130f32
        .min(span / 2. - 50.)
        .min(room / 2. - 30.)
        .max(MIN_RING);
    (cx, cy, r)
}

const PILL: (f32, f32) = (84., 48.);
const DOT: f32 = 48.;

/// Each wheel target's rectangle and hit id: eight lines (or with
/// `emotes` eight emotes) round the ring, then the page switch and the horn
/// in the middle.
pub fn wheel_targets(view: &Viewport, top: f32, emotes: bool) -> [(Rect, u32); 10] {
    let (cx, cy, r) = wheel_geometry(view, top);
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

/// The open wheel: a dimmed backdrop that closes it, then the targets in
/// the space below `top` (an open panel's bottom edge).
/// `focus` is the controller's highlighted target (its index in
/// [`wheel_targets`]) and `hovered` the pointer's hit.
#[allow(clippy::too_many_arguments)]
pub fn paint_wheel(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    hits: &mut Vec<(Rect, u32)>,
    view: &Viewport,
    top: f32,
    chat: &View,
    lines: &[&str; 8],
    emotes: bool,
    focus: Option<usize>,
    hovered: Option<u32>,
) {
    let full = Rect::new(0., 0., view.width, view.height);
    hits.push((full, CLOSE_HIT));
    let (cx, cy, r) = wheel_geometry(view, top);
    let targets = wheel_targets(view, top, emotes);
    let lit_index = (0..targets.len()).find(|&i| focus == Some(i) || hovered == Some(targets[i].1));
    // A soft dark glow keeps it all readable over snow and sky.
    let glow = r + PILL.0 * 0.75;
    list.push(Item::Box(BoxPaint {
        rect: Rect::new(cx - glow, cy - glow, glow * 2., glow * 2.),
        radii: [glow; 4],
        // Clear before the circle's rim (0.707 of farthest-corner).
        fill: Some(Fill::radial(&[
            (0., Color::hex("#0000008c")),
            (0.42, Color::hex("#00000066")),
            (0.68, Color::TRANSPARENT),
        ])),
        ..Default::default()
    }));
    // The band narrows to stay on screen when the wheel fills a short view.
    let band = 32f32
        .min(cy - r - 2.)
        .min(view.height - view.safe[2] - cy - r - 2.)
        .max(26.);
    let dial = Dial::new(r, band);
    dial.paint(
        list,
        cx,
        cy,
        lit_index.filter(|&i| i < 8),
        lit_index.filter(|&i| i >= 8).map(|i| i == 8),
    );
    // The page's name over the ring (under it when a panel leaves no room
    // above), clear of the toolbar row.
    let title = if emotes { "EMOTES" } else { "QUICK CHAT" };
    let style = classic::hud_style(16., RACE_GOLD);
    let width = fonts.measure(title, &style.font, 0.).width;
    let above = cy - dial.outer - 12.;
    let below = cy + dial.outer + 24.;
    let clear_top = (top + 18.).max(view.safe[0].max(14.) + 44. + 18.);
    if above >= clear_top {
        list.text(title, &style, cx - width / 2., above);
    } else if below <= view.height - view.safe[2] - 48. {
        // Not in the bottom band, where the touch coach's lines sit; a short
        // view goes without the title.
        list.text(title, &style, cx - width / 2., below);
    }
    for (index, (rect, id)) in targets.into_iter().enumerate() {
        let ready = match id {
            HORN_HIT => chat.ready[2],
            PAGE_HIT => true,
            id if id >= EMOTE_HIT => chat.ready[1],
            _ => chat.ready[0],
        };
        let lit = lit_index == Some(index);
        let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
        list.opacity(if ready { 1. } else { 0.4 });
        // The race HUD's lettering: overlay silver, gold when lit.
        let color = if lit { RACE_GOLD } else { RACE_TEXT };
        match id {
            HORN_HIT => hud_icon(
                list,
                HORN_ICON,
                color,
                x,
                y - 2.,
                if lit { 26. } else { 22. },
            ),
            // The other page's look: a face for emotes, a bubble for lines.
            PAGE_HIT if emotes => hud_icon(
                list,
                CHAT_ICON,
                color,
                x,
                y + 2.,
                if lit { 26. } else { 22. },
            ),
            PAGE_HIT => paint_emote(list, 1, x, y + 2., if lit { 28. } else { 24. }),
            id if id >= EMOTE_HIT => paint_emote(
                list,
                (id - EMOTE_HIT) as u8,
                x,
                y,
                if lit { 36. } else { 30. },
            ),
            id => {
                let text = lines[(id - LINE_HIT) as usize];
                let size = if lit { 20. } else { 17. };
                let style = classic::hud_style(size, if lit { Color::WHITE } else { color });
                let width = fonts.measure(text, &style.font, 0.).width;
                list.text(text, &style, x - width / 2., y + size * 0.36);
            }
        }
        // Keyboards send 1-8 (Shift for emotes): each segment's key sits in
        // its leading corner, as a dial's numerals.
        if index < 8 && !view.touch_first {
            let a = segment_angle(index) - 0.29;
            let at = dial.outer - 11.;
            let key = (index + 1).to_string();
            let style = classic::hud_style(10., if lit { RACE_GOLD } else { KEY });
            let w = fonts.measure(&key, &style.font, 0.).width;
            list.text(
                &key,
                &style,
                cx + a.cos() * at - w / 2.,
                cy + a.sin() * at + 4.,
            );
        }
        list.unopacity();
        if ready {
            hits.push((rect, id));
        }
    }
}

/// The key numerals' dim silver.
const KEY: Color = Color::rgb(150, 150, 150);
const OUTLINE: Color = Color::hex("#080808bb");
const GOLD_GLASS: Color = Color::rgba(218, 192, 31, 0.3);
const SMOKE: Color = Color::rgba(0, 0, 0, 0.36);
const HUB_FILL: Color = Color::rgba(0, 0, 0, 0.78);
/// A segment's half-width less its gap either side (radians).
const HALF: f32 = std::f32::consts::FRAC_PI_8 - 0.035;

/// Segment `i`'s centre angle: 0 at the top, clockwise.
fn segment_angle(i: usize) -> f32 {
    -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::FRAC_PI_4
}

/// The wheel's dial: eight smoked segments round the lines with a silver
/// rim and ticks between them, a lit segment in gold glass with a pointer
/// from the hub, and the hub's rimmed disc split between the page switch
/// and the horn. Paths are relative to the centre, so each size and lit
/// segment rasterises once.
struct Dial {
    inner: f32,
    outer: f32,
    hub: f32,
}

impl Dial {
    fn new(r: f32, band: f32) -> Self {
        Self {
            inner: r - band,
            outer: r + band,
            hub: r - band - 8.,
        }
    }

    fn paint(
        &self,
        list: &mut DisplayList,
        cx: f32,
        cy: f32,
        lit: Option<usize>,
        top: Option<bool>,
    ) {
        let place = |list: &mut DisplayList, path: String, fill: Color, stroke: Option<Stroke>| {
            let e = self.outer + 12.;
            list.push(Item::Shape {
                shape: Shape {
                    path,
                    bounds: [-e, -e, e, e],
                    unit: 1.,
                    fill,
                    stroke,
                    drop_shadow: None,
                },
                x: cx,
                y: cy,
                rotation: 0.,
            });
        };
        let line = |width: f32, color: Color| {
            Some(Stroke {
                width,
                color,
                join: Join::Miter,
            })
        };
        // The segments, smoked; the lit one in gold glass.
        let mut smoke = String::new();
        for i in (0..8).filter(|&i| Some(i) != lit) {
            smoke += &sector(self.inner, self.outer, segment_angle(i), HALF);
        }
        place(list, smoke, SMOKE, None);
        // The rim: a dark-outlined silver arc along each segment's edge,
        // and ticks across the gaps.
        let mut rim = String::new();
        let mut ticks = String::new();
        for i in 0..8 {
            let a = segment_angle(i);
            rim += &arc(self.outer, a - HALF, a + HALF);
            let b = a + std::f32::consts::FRAC_PI_8;
            let (c, s) = (b.cos(), b.sin());
            let (r0, r1) = (self.outer - 6., self.outer + 7.);
            ticks += &format!("M{:.2} {:.2}L{:.2} {:.2}", c * r0, s * r0, c * r1, s * r1);
        }
        place(list, rim.clone(), Color::TRANSPARENT, line(5., OUTLINE));
        place(list, rim, Color::TRANSPARENT, line(2.5, RACE_TEXT));
        place(list, ticks.clone(), Color::TRANSPARENT, line(4., OUTLINE));
        place(list, ticks, Color::TRANSPARENT, line(1.5, RACE_TEXT));
        if let Some(i) = lit {
            let a = segment_angle(i);
            place(
                list,
                sector(self.inner, self.outer, a, HALF),
                GOLD_GLASS,
                None,
            );
            let edge = arc(self.outer, a - HALF, a + HALF);
            place(list, edge.clone(), Color::TRANSPARENT, line(7., OUTLINE));
            place(list, edge, Color::TRANSPARENT, line(4., RACE_GOLD));
            place(
                list,
                arc(self.inner, a - HALF, a + HALF),
                Color::TRANSPARENT,
                line(1.5, RACE_GOLD),
            );
        }
        // The hub: a dark disc with the silver rim, split in two.
        let h = self.hub;
        let disc = format!("M{h} 0A{h} {h} 0 1 1 {} 0A{h} {h} 0 1 1 {h} 0Z", -h);
        place(list, disc.clone(), HUB_FILL, line(5., OUTLINE));
        if let Some(upper) = top {
            // The lit half (the page switch above, the horn below) in gold.
            let sweep = if upper { 0 } else { 1 };
            let half = format!("M{h} 0A{h} {h} 0 0 {sweep} {} 0Z", -h);
            place(list, half, GOLD_GLASS, None);
        }
        place(list, disc, Color::TRANSPARENT, line(2.5, RACE_TEXT));
        let bar = h * 0.62;
        place(
            list,
            format!("M{} 0L{bar} 0", -bar),
            Color::TRANSPARENT,
            line(1.5, KEY),
        );
        // A gold pointer on the hub's rim toward the lit segment.
        if let Some(i) = lit {
            let a = segment_angle(i);
            let (c, s) = (a.cos(), a.sin());
            let (tip, base, w) = (h + 13., h + 2., 7.);
            let path = format!(
                "M{:.2} {:.2}L{:.2} {:.2}L{:.2} {:.2}Z",
                c * tip,
                s * tip,
                c * base - s * w,
                s * base + c * w,
                c * base + s * w,
                s * base - c * w,
            );
            place(list, path, RACE_GOLD, line(2., OUTLINE));
        }
    }
}

/// An arc of radius `r` from angle `a0` to `a1` (clockwise, radians).
fn arc(r: f32, a0: f32, a1: f32) -> String {
    let large = u8::from(a1 - a0 > std::f32::consts::PI);
    format!(
        "M{:.2} {:.2}A{r:.2} {r:.2} 0 {large} 1 {:.2} {:.2}",
        r * a0.cos(),
        r * a0.sin(),
        r * a1.cos(),
        r * a1.sin()
    )
}

/// A ring segment between radii `ri` and `ro`, `half` either side of `a`.
fn sector(ri: f32, ro: f32, a: f32, half: f32) -> String {
    let (a0, a1) = (a - half, a + half);
    format!(
        "M{:.2} {:.2}A{ro:.2} {ro:.2} 0 0 1 {:.2} {:.2}L{:.2} {:.2}A{ri:.2} {ri:.2} 0 0 0 {:.2} {:.2}Z",
        ro * a0.cos(),
        ro * a0.sin(),
        ro * a1.cos(),
        ro * a1.sin(),
        ri * a1.cos(),
        ri * a1.sin(),
        ri * a0.cos(),
        ri * a0.sin()
    )
}

/// An icon in the race HUD's lettering (hud::toolbar): a fill with the
/// overlays' dark outline.
fn hud_icon(list: &mut DisplayList, path: &str, color: Color, x: f32, y: f32, size: f32) {
    let mut s = shape(
        path,
        color,
        Some(Stroke {
            width: 2.5,
            color: Color::hex("#080808bb"),
            join: Join::Round,
        }),
    );
    s.unit = size / 20.;
    list.push(Item::Shape {
        shape: s,
        x: x - size / 2.,
        y: y - size / 2.,
        rotation: 0.,
    });
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
            for (emotes, top) in [(false, 0.), (true, 0.), (false, 230.), (true, 300.)] {
                let targets = wheel_targets(&v, top, emotes);
                for (i, (a, _)) in targets.iter().enumerate() {
                    assert!(a.w >= 48. && a.h >= 48., "{a:?}");
                    // Under the panel whenever the room below holds the wheel.
                    if v.height - top >= WHEEL_SPACE {
                        assert!(a.y >= top, "{a:?} under a panel ending at {top}");
                    }
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
