//! Pause and its settings screens. atv_core::ui_menu owns the rows, labels,
//! values and choice order exactly as for the browser; this module lays them
//! out the way the frontend lays out the main menu (crate::front): a column
//! of gold-bulleted rows with the gold selection pill, the page's name on
//! its title band and Back bottom right. Over a ride the column sits on the
//! left and the paused ride stays in view beside it; over the main menu the
//! frontend's stage takes that side. One page shows at a time.
pub mod access;
pub mod controller;
pub mod credits;
pub mod glyphs;
pub mod jukebox;
pub mod prefs;

use crate::{
    color::{Color, Fill, Gradient},
    hud::Viewport,
    layout::{Content, El, Paint, margins, px, sides},
    paint::BoxShadow,
    text::{Font, TextStyle},
};
use taffy::prelude::*;

/// Hit ids: rows and sliders are numbered by their index in `choices`.
pub const ROW: u32 = 1000;
pub const PANEL: u32 = 900;
pub const CONTENT: u32 = 901;
/// The Jukebox spectrum's area (the host paints the bars each frame).
pub const BARS: u32 = 902;
/// The ui_menu frame parts the panel uses.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Rows {
    pub choices: Vec<String>,
    /// Action -> label (`Label: value` splits into label and value badge).
    pub labels: Vec<(String, String)>,
    pub sliders: Vec<Slider>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slider {
    pub action: String,
    pub label: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl Rows {
    pub fn from_frame(frame: &atv_core::ui_menu::Frame) -> Self {
        Self {
            choices: frame.choices.clone(),
            labels: frame
                .hits
                .iter()
                .map(|h| (h.action.clone(), h.label.clone()))
                .collect(),
            sliders: frame
                .sliders
                .iter()
                .map(|s| Slider {
                    action: s.action.into(),
                    label: s.label.into(),
                    value: s.value,
                    min: s.min,
                    max: s.max,
                    step: s.step,
                })
                .collect(),
        }
    }
    pub fn label(&self, action: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(a, _)| a == action)
            .map(|(_, l)| l.as_str())
    }
    pub fn slider(&self, action: &str) -> Option<&Slider> {
        self.sliders.iter().find(|s| s.action == action)
    }
}

/// Panel title per page (frontend.js `rebuildPausePanel`).
pub fn title(page: &str) -> &'static str {
    match page {
        "settings" => "Options",
        "extras" => "Extras",
        "game-settings" => "Game Settings",
        "ecosystem-warning" => "Warning",
        "accessibility" => "Accessibility",
        "jukebox" => "Jukebox",
        "credits" => "Credits",
        "alpha" => "Alpha",
        "controls" => "Controls",
        "controller-settings" => "Controller Settings",
        "hud-settings" => "Display",
        "minimap-config" => "Minimap",
        "race-overlays" => "Race Overlays",
        "bindings" => "Keyboard",
        "music" => "Audio",
        "players" => "Players",
        _ => "Pause",
    }
}

/// Controls-page tips for the active touch layout.
pub fn tips(one_handed: bool) -> [&'static str; 3] {
    if one_handed {
        [
            "Hold anywhere to accelerate and brace for a jump.",
            "Drag sideways to steer, and up or down to lean. Lift to coast and release preload.",
            "Use a second finger to brake / reverse on the ground, or swipe / circle for tricks in the air.",
        ]
    } else {
        [
            "Left thumb: steer and lean.",
            "Right thumb: hold to accelerate and brace; drag back to brake / reverse. Lift to release preload.",
            "In the air, swipe or circle with your right thumb for tricks.",
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub page: String,
    pub rows: Rows,
    pub selection: usize,
    pub hovered: Option<usize>,
    pub pressed: Option<usize>,
    pub scroll: f32,
    pub one_handed: bool,
    /// The row being captured (bindings page).
    pub capture: Option<String>,
    /// Pager text, e.g. "Page 1 of 5".
    pub page_count: Option<String>,
    /// `.pause-header .pause-status`: a line under the title (Audio's
    /// soundtrack status).
    pub status: Option<String>,
    /// Opened from the frontend's main menu: the panel fills the screen over
    /// the frontend backdrop (`#frontend[data-context=menu]`).
    pub from_menu: bool,
    /// `#menu-notice`: a bottom-centre line (copy ride link results).
    pub notice: Option<String>,
    /// Controller bindings: glyph rows and the summary panel (pad-glyphs.js).
    pub pad: Option<glyphs::PadGlyphs>,
    /// Players page: riders who cannot be spawned beside (label, "You" or
    /// "Connecting"), listed above the spawn rows (`.pause-player`).
    pub roster: Vec<(String, String)>,
    /// The Jukebox page: the player's view, the original art (set by the
    /// host) and the disc animation frame.
    pub music: Option<jukebox::MusicView>,
    pub jukebox_art: Option<jukebox::Art>,
    pub frame: usize,
    /// The Credits page: the script (set by the host once read) and the
    /// seconds since the page opened (its scroll).
    pub credits: Option<std::rc::Rc<atv_core::credits::Credits>>,
    pub credits_seconds: f32,
    /// The panel's slide-in while a page opens (crate::transition).
    pub motion: Option<crate::transition::Motion>,
}

/// Chips under a page's rows rather than rows: a list's actions.
const FOOTER: [&str; 4] = ["bindcancel", "bindclear", "bindreset", "paddefaults"];
/// Back to the page before, bottom right where the frontend keeps it.
const BACK: [&str; 2] = ["back", "pause"];
const PAGER: [&str; 4] = ["bindprev", "bindnext", "musicprev", "musicnext"];
/// Rows drawn outside the scrolled column, which never scrolls to them.
pub fn fixed(action: &str) -> bool {
    BACK.contains(&action) || FOOTER.contains(&action)
}

/// Pause rows that follow the ride's own (Resume, Restart, Replay, Photo
/// mode and a room's rows): Options and the Jukebox, then leaving apart
/// from the rest, then sharing as small chips at the foot.
const PAUSE_MENUS: [&str; 2] = ["settings", "jukebox"];
const PAUSE_LEAVE: [&str; 2] = ["main", "leave"];
const PAUSE_CHIPS: [&str; 2] = ["copyride", "devlog"];

/// The order rows are drawn in, as indexes into `choices`: Up and Down
/// follow it. Pause gathers its rows as [`pause`] draws them; other pages
/// keep ui_menu's order.
pub fn order(page: &str, choices: &[String]) -> Vec<usize> {
    let all = 0..choices.len();
    if page != "pause" {
        return all.collect();
    }
    let in_group = |i: &usize, group: &[&str]| group.contains(&choices[*i].as_str());
    let grouped = |i: &usize| {
        in_group(i, &PAUSE_MENUS) || in_group(i, &PAUSE_LEAVE) || in_group(i, &PAUSE_CHIPS)
    };
    let mut order: Vec<usize> = all.clone().filter(|i| !grouped(i)).collect();
    for group in [&PAUSE_MENUS[..], &PAUSE_LEAVE, &PAUSE_CHIPS] {
        order.extend(all.clone().filter(|i| in_group(i, group)));
    }
    order
}

const INK: Color = Color::hex("#0d0b00");
const TEXT: Color = Color::hex("#d2d2d2");
const MUTED: Color = Color::hex("#b4b4b4");
const GOLD: Color = Color::hex("#f1d84e");
/// Lettering on the gold pill that is not the label.
const ON_GOLD: Color = Color::hex("#3a2e00");
const SHADOW: Color = Color::hex("#000c");

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

fn col(gap: f32) -> El {
    El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: px(0.),
            height: px(gap),
        },
        ..Default::default()
    })
}

fn line(gap: f32) -> El {
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(gap),
            height: px(gap),
        },
        ..Default::default()
    })
}

fn text(text: impl Into<String>, style: TextStyle, line_height: f32, wrap: bool) -> El {
    El::new(Style::default()).content(Content::Text {
        text: text.into(),
        style,
        line_height,
        wrap,
    })
}

/// Sizes and the state a page's parts are drawn from: the frontend's
/// (front::Metrics), so Pause and Options read as the main menu does.
struct Look<'a> {
    state: &'a State,
    images: &'a glyphs::GlyphImages,
    w: f32,
    h: f32,
    safe: [f32; 4],
    /// A phone held upright: the column spans the screen.
    narrow: bool,
    /// A phone on its side.
    short: bool,
    gutter: f32,
    /// The menu column's width, the left safe area included.
    column: f32,
    row: f32,
    font: f32,
}

impl<'a> Look<'a> {
    fn new(view: &Viewport, state: &'a State, images: &'a glyphs::GlyphImages) -> Self {
        let (w, h) = (view.width, view.height);
        let narrow = h > w || w - view.safe[1] - view.safe[3] < 560.;
        let short = !narrow && h <= 500.;
        // Pause's few rows are the main menu's; a settings page holds more
        // rows, each with its value, a size down.
        let pause = state.page == "pause";
        let (row, font) = match (narrow, short, pause) {
            (true, _, true) => (if h <= 640. { 46. } else { 50. }, 19.),
            (true, _, false) => (48., 17.),
            (_, true, true) => (44., 18.),
            (_, true, false) => (42., 16.),
            (_, _, true) => ((h * 0.064).clamp(48., 60.), (w * 0.023).clamp(19., 27.)),
            (_, _, false) => ((h * 0.06).clamp(44., 52.), (w * 0.0155).clamp(17., 21.)),
        };
        Self {
            state,
            images,
            w,
            h,
            safe: view.safe,
            narrow,
            short,
            gutter: (w * 0.032).clamp(16., 44.),
            column: if narrow {
                w
            } else {
                (w * 0.44).max(if short { 280. } else { 300. }).min(580.)
            },
            row,
            font,
        }
    }

    /// Where rows start: clear of the gutter and the notch.
    fn left(&self) -> f32 {
        self.gutter + self.safe[3]
    }

    fn right(&self) -> f32 {
        if self.narrow {
            self.gutter + self.safe[1]
        } else {
            8.
        }
    }
}

/// The full-viewport layer: the menu column on the left (the whole width
/// on a phone held upright) over the paused ride, which stays in view
/// beside it; over the main menu, the frontend draws its stage there.
pub fn element(view: &Viewport, state: &State, images: &glyphs::GlyphImages) -> El {
    if state.page == "credits" {
        return credits::element(view, state).children(notice(view, state));
    }
    if state.page == "jukebox" {
        // Before the ride's soundtrack catalog arrives the page still opens
        // (jukebox.js: "Loading the soundtrack…").
        let waiting = jukebox::MusicView {
            enabled: true,
            volume: 0.7,
            ..Default::default()
        };
        let music = state.music.as_ref().unwrap_or(&waiting);
        return jukebox::element(view, state, music, state.jukebox_art.as_ref(), state.frame)
            .children(notice(view, state));
    }
    let look = Look::new(view, state, images);
    let (w, h) = (look.w, look.h);
    let mut layer = El::new(Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        ..Default::default()
    });
    // Over a ride the scene darkens behind the column and clears beside
    // it; the frontend has its own backdrop.
    if !state.from_menu {
        layer = layer.paint(Paint::fill(if look.narrow {
            Fill::Solid(Color::hex("#000000b8"))
        } else {
            let edge = look.column / w;
            Fill::Linear(Gradient::new(
                90.,
                &[
                    (0., Color::hex("#000000d9")),
                    (edge * 0.75, Color::hex("#000000b3")),
                    ((edge + 0.2).min(1.), Color::hex("#00000024")),
                    (1., Color::hex("#00000014")),
                ],
            ))
        }));
    }
    let mut stage = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(0.),
            top: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::length(0.),
            bottom: LengthPercentageAuto::length(0.),
        },
        ..Default::default()
    })
    .child(column(&look));
    if let Some(i) = state
        .rows
        .choices
        .iter()
        .position(|c| BACK.contains(&c.as_str()))
    {
        stage = stage.child(back(&look, i));
    }
    if state.page == "pause" && !look.narrow {
        stage = stage.child(pause_chips(&look));
    }
    layer
        .child(slide(stage, state))
        .children(notice(view, state))
}

/// The page's name, any status line, its scrolled rows and the chips
/// under them.
fn column(look: &Look) -> El {
    let state = look.state;
    let (content, footer) = if state.page == "pause" {
        (pause(look), None)
    } else {
        body(look)
    };
    // A phone held upright keeps Back's corner clear.
    let has_back = state
        .rows
        .choices
        .iter()
        .any(|c| BACK.contains(&c.as_str()));
    let bottom = 12. + look.safe[2] + if look.narrow && has_back { 58. } else { 0. };
    let mut column = col(0.);
    column.style.position = Position::Absolute;
    column.style.inset = taffy::Rect {
        left: LengthPercentageAuto::length(0.),
        top: LengthPercentageAuto::length(0.),
        right: LengthPercentageAuto::auto(),
        bottom: LengthPercentageAuto::length(0.),
    };
    column.style.size.width = length(look.column);
    column.style.padding = sides(0., 0., bottom, 0.);
    column
        .child(heading(look))
        .children(
            state
                .status
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|status| {
                    let mut el = text(
                        status.clone(),
                        TextStyle::new(display(14., 600, false), Color::hex("#dcdcdc"))
                            .shadow(1., 2., 0., SHADOW),
                        19.,
                        true,
                    );
                    el.style.flex_shrink = 0.;
                    el.style.padding = sides(0., look.right(), 10., look.left() + 6.);
                    el
                }),
        )
        .child(scroller(state, content))
        .children(footer)
}

/// The page's name on the frontend's title band.
fn heading(look: &Look) -> El {
    let state = look.state;
    let size = if look.narrow {
        (look.w * 0.07).clamp(24., 32.)
    } else if look.short {
        24.
    } else {
        (look.w * 0.036).clamp(26., 44.)
    };
    let (top, bottom) = if look.narrow {
        (10., 6.)
    } else if look.short {
        (8., 4.)
    } else {
        (
            (look.h * 0.03).clamp(12., 28.),
            (look.h * 0.016).clamp(8., 16.),
        )
    };
    let name = if state.page == "pause" {
        "Paused"
    } else {
        heading_text(state, &state.page)
    };
    El::new(Style {
        flex_shrink: 0.,
        padding: sides(top + look.safe[0], 0., bottom, 0.),
        ..Default::default()
    })
    .child(crate::front::title_strip(
        name.into(),
        size,
        false,
        look.short,
        (look.left(), look.gutter),
    ))
}

/// The page's name: controller bindings are titled "Controller"
/// (bindingDevice).
fn heading_text(state: &State, page: &str) -> &'static str {
    if page == "bindings" && state.pad.is_some() {
        "Controller"
    } else {
        title(page)
    }
}

/// The part of the column that scrolls: `content` under the kinetic
/// offset. It runs from the screen's edge so the selection pill can too.
fn scroller(state: &State, mut content: El) -> El {
    content.style.flex_shrink = 0.;
    El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        flex_grow: 1.,
        flex_shrink: 1.,
        min_size: Size {
            width: length(0.),
            height: length(0.),
        },
        overflow: taffy::Point {
            x: taffy::Overflow::Visible,
            y: taffy::Overflow::Scroll,
        },
        scrollbar_width: 0.,
        ..Default::default()
    })
    .id(PANEL)
    .scroll(state.scroll)
    .child(content.id(CONTENT))
}

/// `.ms-row::before`: the glossy gold pill running off the left edge.
fn pill(look: &Look) -> El {
    El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(-look.left() - 30.),
            top: LengthPercentageAuto::length(5.),
            right: LengthPercentageAuto::length(0.),
            bottom: LengthPercentageAuto::length(5.),
        },
        ..Default::default()
    })
    .paint(crate::front::pill_shadows(
        Paint::fill(gold_pill()).radius(5.),
    ))
}

/// A row's label style: the frontend's `.ms-row` lettering.
fn row_text(size: f32, selected: bool, hovered: bool) -> TextStyle {
    let color = if selected {
        INK
    } else if hovered {
        Color::WHITE
    } else {
        TEXT
    };
    let style = TextStyle::new(display(size, 700, false), color).spacing(size * 0.035);
    if selected {
        style.shadow(0., 1., 0., Color::hex("#fff6b066"))
    } else {
        style.shadow(1., 2., 0., SHADOW)
    }
}

/// A row's box: bullet, then what `fill` adds, the pill behind it when
/// selected.
fn row_box(look: &Look, height: f32, selected: bool) -> El {
    let mut el = line(14.);
    el.style.position = Position::Relative;
    el.style.min_size = Size {
        width: length(0.),
        height: length(height),
    };
    el.style.padding = sides(0., 6., 0., 6.);
    el.style.flex_shrink = 0.;
    if selected {
        el = el.child(pill(look));
    }
    el.child(crate::front::bullet_slot(selected))
}

fn ellipsis(text: String, style: TextStyle, line_height: f32, grow: f32) -> El {
    El::new(Style {
        flex_grow: grow,
        // The value gives way first; the label ellipsizes only once the
        // value is at its least.
        flex_shrink: 0.01,
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .content(Content::Ellipsis {
        text,
        style,
        line_height,
    })
}

/// A row's value between the arrows that step it (`.ms-stepper`).
fn value(look: &Look, value: String, selected: bool) -> El {
    let size = look.font * 0.9;
    let mut style = TextStyle::new(
        display(size, 700, false),
        if selected { ON_GOLD } else { GOLD },
    );
    if !selected {
        style = style.shadow(1., 2., 0., SHADOW);
    }
    let arrows = Color::hex(if selected { "#1b1500" } else { "#f1d84ebf" });
    // A fixed width keeps the arrows in one column down the page.
    let middle = El::new(Style {
        display: Display::Flex,
        justify_content: JustifyContent::CENTER,
        flex_shrink: 1.,
        size: Size {
            width: length(if look.narrow { 6. } else { 7.5 } * size),
            height: auto(),
        },
        min_size: Size {
            width: length(2.5 * size),
            height: auto(),
        },
        ..Default::default()
    })
    .child(ellipsis(value, style, size * 1.1, 0.));
    let mut el = line(2.)
        .child(crate::front::arrow(true, 16., arrows, !selected))
        .child(middle)
        .child(crate::front::arrow(false, 16., arrows, !selected));
    el.style.flex_shrink = 1.;
    el.style.min_size.width = length(0.);
    el
}

/// One row: its label, then its value. Binding and song rows put their
/// value on a line under the label.
fn row(look: &Look, rows: &Rows, id: u32, action: &str, selected: bool, hovered: bool) -> El {
    let label = rows.label(action).unwrap_or(action);
    let (name, setting) = match label.split_once(": ") {
        Some((l, v)) => (l.to_string(), v.to_string()),
        None => (label.to_string(), String::new()),
    };
    let size = look.font;
    let style = row_text(size, selected, hovered);
    if action.starts_with("bind:") || action.starts_with("song:") {
        let detail = TextStyle::new(
            display(size * 0.72, 700, false),
            if selected { ON_GOLD } else { MUTED },
        );
        let glyph_row = look
            .state
            .pad
            .as_ref()
            .and_then(|p| p.row(action))
            .map(|chords| {
                let mut row = binding_glyphs(chords, look.images, selected, 28.);
                row.style.margin = margins(4., 0., 0., 0.);
                row
            });
        let mut lines = col(2.);
        lines.style.min_size.width = length(0.);
        lines.style.flex_shrink = 1.;
        lines.style.padding = sides(9., 0., 9., 0.);
        return row_box(look, look.row, selected).id(id).child(
            lines
                .child(label_el(&name, &style, size))
                .child(glyph_row.unwrap_or_else(|| text(setting, detail, size * 0.9, true))),
        );
    }
    let mut el =
        row_box(look, look.row, selected)
            .id(id)
            .child(ellipsis(name, style, size * 1.1, 1.));
    if !setting.is_empty() {
        el = el.child(value(look, setting, selected));
    }
    el
}

/// A slider: its name and value as a row, the track under them.
fn slider(look: &Look, id: u32, s: &Slider, selected: bool, hovered: bool) -> El {
    let units = if s.action == "minimap-size" {
        " px"
    } else {
        "%"
    };
    let fill = ((s.value - s.min) / (s.max - s.min)).clamp(0., 1.);
    let size = look.font;
    let mut amount = TextStyle::new(
        display(size * 0.9, 700, false),
        if selected { ON_GOLD } else { GOLD },
    );
    if !selected {
        amount = amount.shadow(1., 2., 0., SHADOW);
    }
    let head = row_box(look, (look.row - 8.).max(36.), selected)
        .child(ellipsis(
            s.label.clone(),
            row_text(size, selected, hovered),
            size * 1.1,
            1.,
        ))
        .child(text(
            format!("{}{units}", s.value.round() as i32),
            amount,
            size,
            false,
        ));
    let mut track = range(fill).id(id);
    // Under the label, past the bullet.
    track.style.margin = margins(0., 6., 0., 34.);
    col(0.).child(head).child(track)
}

/// A small dark chip with a short label: a list's actions, the pager, and
/// Pause's sharing rows (`.ms-hint-back`). Gold while selected.
fn chip(look: &Look, label: &str, id: u32, selected: bool, hovered: bool) -> El {
    let size = if look.short { 15. } else { 16. };
    let color = if selected {
        INK
    } else if hovered {
        Color::hex("#fffaa1")
    } else {
        Color::hex("#f2f2f2")
    };
    let mut style = TextStyle::new(display(size, 700, false), color);
    if !selected {
        style = style.shadow(0., 1., 3., Color::BLACK);
    }
    let mut el = line(7.);
    el.style.min_size = Size {
        width: length(44.),
        height: length(44.),
    };
    el.style.padding = sides(0., 12., 0., 12.);
    el.style.justify_content = JustifyContent::CENTER;
    el.style.flex_shrink = 0.;
    el.paint(if selected {
        crate::front::pill_shadows(Paint::fill(gold_pill()).radius(5.))
    } else {
        Paint::fill(Color::hex("#0000008f")).radius(5.)
    })
    .id(id)
    .child(text(label, style, size, false))
}

/// Back (or Pause's own Back): bottom right, as on every frontend page.
fn back(look: &Look, index: usize) -> El {
    let state = look.state;
    let action = state.rows.choices[index].as_str();
    let selected = state.selection == index;
    let hovered = state.hovered == Some(index) && !selected;
    let label = state.rows.label(action).unwrap_or("Back");
    let size = if look.short { 15. } else { 17. };
    let color = if selected {
        INK
    } else if hovered {
        Color::hex("#fffaa1")
    } else {
        Color::hex("#f2f2f2")
    };
    let mut style = TextStyle::new(display(size, 700, false), color);
    if !selected {
        style = style.shadow(0., 1., 3., Color::BLACK);
    }
    let mut el = corner(look, line(7.));
    el.style.min_size = Size {
        width: length(64.),
        height: length(46.),
    };
    el.style.padding = sides(0., 14., 0., 12.);
    el.paint(if selected {
        crate::front::pill_shadows(Paint::fill(gold_pill()).radius(5.))
    } else {
        Paint::fill(Color::hex("#0000008f")).radius(5.)
    })
    .id(ROW + index as u32)
    .child(crate::front::arrow(true, 16., color, false))
    .child(text(label, style, size, false))
}

/// `el` in the bottom-right corner, where the frontend keeps Back.
fn corner(look: &Look, mut el: El) -> El {
    let bottom = if look.short || look.narrow {
        6.
    } else {
        (look.h * 0.014).clamp(6., 14.)
    } + look.safe[2]
        + 2.;
    el.style.position = Position::Absolute;
    el.style.inset = taffy::Rect {
        left: LengthPercentageAuto::auto(),
        top: LengthPercentageAuto::auto(),
        right: LengthPercentageAuto::length(look.gutter + look.safe[1]),
        bottom: LengthPercentageAuto::length(bottom),
    };
    el
}

/// The rows' column inside the scroller: clear of the gutter on the left.
fn content(look: &Look, gap: f32) -> El {
    let mut el = col(gap);
    el.style.padding = sides(2., look.right(), 12., look.left());
    el
}

/// Pause's sharing rows as chips: under its rows on a phone held upright,
/// otherwise bottom right like Back, which Pause has none of.
fn pause_chips(look: &Look) -> El {
    let state = look.state;
    let choices = &state.rows.choices;
    let mut chips = line(8.);
    chips.style.flex_wrap = FlexWrap::Wrap;
    for i in order("pause", choices) {
        let action = choices[i].as_str();
        if PAUSE_CHIPS.contains(&action) {
            chips = chips.child(chip(
                look,
                state.rows.label(action).unwrap_or(action),
                ROW + i as u32,
                state.selection == i,
                state.hovered == Some(i) && state.selection != i,
            ));
        }
    }
    if look.narrow {
        chips.style.padding = sides(0., 0., 0., 6.);
        chips.style.margin = margins(6., 0., 0., 0.);
        chips
    } else {
        corner(look, chips)
    }
}

/// Pause: the ride's rows with Options and the Jukebox, then leaving set
/// apart. Rows follow [`order`].
fn pause(look: &Look) -> El {
    let state = look.state;
    let choices = &state.rows.choices;
    let entry = |i: usize| {
        row(
            look,
            &state.rows,
            ROW + i as u32,
            &choices[i],
            state.selection == i,
            state.hovered == Some(i) && state.selection != i,
        )
    };
    let in_group = |i: &usize, group: &[&str]| group.contains(&choices[*i].as_str());
    let mut main = col(0.);
    let mut leave = col(0.);
    for i in order("pause", choices) {
        if in_group(&i, &PAUSE_CHIPS) {
            continue;
        } else if in_group(&i, &PAUSE_LEAVE) {
            leave = leave.child(entry(i));
        } else {
            main = main.child(entry(i));
        }
    }
    let mut el = content(look, if look.short { 6. } else { 12. }).child(main);
    if !leave.children.is_empty() {
        el = el.child(leave);
    }
    if look.narrow {
        el = el.child(pause_chips(look));
    }
    el
}

/// Any other page: its rows and what goes with them, and the list actions
/// fixed under them.
fn body(look: &Look) -> (El, Option<El>) {
    let state = look.state;
    let rows = &state.rows;
    let page = state.page.as_str();
    let selected = |i: usize| state.selection == i;
    let hovered = |i: usize| state.hovered == Some(i) && state.selection != i;
    let mut content = content(look, 14.);
    let mut list = col(2.);
    let mut audio = col(2.);
    let mut pager = line(8.);
    pager.style.padding = sides(0., 0., 0., 6.);
    let mut footer = line(8.);
    footer.style.flex_wrap = FlexWrap::Wrap;
    footer.style.flex_shrink = 0.;
    footer.style.padding = sides(10., look.right(), 0., look.left() + 6.);
    for (i, action) in rows.choices.iter().enumerate() {
        let (id, action) = (ROW + i as u32, action.as_str());
        let label = rows.label(action).unwrap_or(action);
        let entry = || match rows.slider(action) {
            Some(s) => slider(look, id, s, selected(i), hovered(i)),
            None => row(look, rows, id, action, selected(i), hovered(i)),
        };
        // Audio: Music, the two volumes and Sound lead the page, over the
        // song list and its pager.
        if page == "music"
            && matches!(
                action,
                "musictoggle" | "musicvolume" | "sfxvolume" | "soundmode"
            )
        {
            audio = audio.child(entry());
        } else if PAGER.contains(&action) {
            pager = pager.child(chip(look, label, id, selected(i), hovered(i)));
        } else if FOOTER.contains(&action) {
            footer = footer.child(chip(look, label, id, selected(i), hovered(i)));
        } else if !BACK.contains(&action) {
            list = list.child(entry());
        }
    }
    if let Some(label) = &state.capture {
        // The action being bound and how to bind it.
        content = content.child(aside(
            look,
            Some(label.clone()),
            &["Press a key, controller button or move an axis. Hold several together for a combination. Release to save; Escape cancels."],
        ));
    }
    if page == "players" && !state.roster.is_empty() {
        let mut roster = col(0.);
        for (label, status) in &state.roster {
            let mut rider = line(12.);
            rider.style.justify_content = JustifyContent::SPACE_BETWEEN;
            rider.style.padding = sides(0., 6., 0., 34.);
            rider.style.min_size.height = length(look.row);
            roster = roster.child(
                rider
                    .child(text(
                        label.clone(),
                        row_text(look.font, false, false),
                        look.font * 1.1,
                        true,
                    ))
                    .child(text(
                        status.clone(),
                        TextStyle::new(display(look.font * 0.85, 700, false), MUTED)
                            .shadow(1., 2., 0., SHADOW),
                        look.font,
                        false,
                    )),
            );
        }
        content = content.child(roster);
    }
    if page == "ecosystem-warning" {
        // Warning 0x90: the original's three two-line paragraphs.
        let paragraphs: Vec<String> = atv_core::ui_menu::ECOSYSTEM_WARNING
            .chunks(2)
            .map(|pair| pair.join(" "))
            .collect();
        let paragraphs: Vec<&str> = paragraphs.iter().map(String::as_str).collect();
        content = content.child(aside(look, None, &paragraphs));
    }
    if !audio.children.is_empty() {
        content = content.child(audio);
    }
    if !list.children.is_empty() {
        content = content.child(list);
    }
    // The touch layout's tips show while its row is selected.
    if page == "controls"
        && rows
            .choices
            .get(state.selection)
            .is_some_and(|a| a == "onehanded")
    {
        content = content.child(aside(look, None, &tips(state.one_handed)));
    }
    if !pager.children.is_empty() {
        if let Some(count) = &state.page_count {
            let mut count = text(
                count.clone(),
                TextStyle::new(display(15., 700, false), TEXT).shadow(1., 2., 0., SHADOW),
                15.,
                false,
            );
            count.style.flex_shrink = 0.;
            count.style.padding = sides(0., 6., 0., 6.);
            pager.children.insert(1.min(pager.children.len()), count);
        }
        content = content.child(pager);
    }
    if let Some(pad) = state.pad.as_ref().filter(|_| state.capture.is_none()) {
        content = content.child(summary_panel(pad, look.images, 1100., look.narrow));
    }
    (content, (!footer.children.is_empty()).then_some(footer))
}

/// Text that explains rather than chooses: a gold rule beside it, an
/// optional heading over it.
fn aside(look: &Look, heading: Option<String>, lines: &[&str]) -> El {
    let mut el = col(8.);
    el.style.margin = margins(0., 6., 0., 6.);
    el.style.padding = sides(4., 0., 4., 16.);
    el.style.border = sides(0., 0., 0., 3.);
    el.paint.border = [0., 0., 0., 3.];
    el.paint.border_color = GOLD;
    if let Some(heading) = heading {
        el = el.child(text(
            heading,
            row_text(look.font * 1.1, false, false),
            look.font * 1.4,
            true,
        ));
    }
    for line in lines {
        el = el.child(text(
            *line,
            TextStyle::new(display(15., 500, false), Color::hex("#e2e2e2"))
                .shadow(1., 1., 0., SHADOW),
            21.,
            true,
        ));
    }
    el
}

/// The moving part of a menu page alone: its panel, without the dim
/// backdrop and `#menu-notice` that stay put. The page a transition leaves
/// is drawn this way while it slides out (crate::transition), so the dim
/// layer is not doubled.
pub fn stage(view: &Viewport, state: &State, images: &glyphs::GlyphImages) -> El {
    let mut el = element(view, state, images);
    el.paint = Paint::default();
    el.children.truncate(1);
    el
}

/// `#menu-stage` while a page slides (crate::transition): the panel moves
/// and fades; the dim backdrop and `#menu-notice` stay.
pub fn slide(stage: El, state: &State) -> El {
    match state.motion {
        Some(m) => stage.opacity(m.opacity).translate(m.dx, 0.),
        None => stage,
    }
}

/// `#menu-notice` (game.css + hud.css): a fit-content line centred above the
/// bottom safe area, gold left rule.
fn notice(view: &Viewport, state: &State) -> Option<El> {
    let text = state.notice.as_ref().filter(|t| !t.is_empty())?;
    let side = |s: f32| s.max(16.);
    Some(
        El::new(Style {
            position: Position::Absolute,
            inset: taffy::Rect {
                left: LengthPercentageAuto::length(side(view.safe[3])),
                right: LengthPercentageAuto::length(side(view.safe[1])),
                bottom: LengthPercentageAuto::length(side(view.safe[2])),
                top: LengthPercentageAuto::auto(),
            },
            display: Display::Flex,
            justify_content: JustifyContent::CENTER,
            ..Default::default()
        })
        .child(
            El::new(Style {
                padding: sides(10., 18., 10., 18.),
                border: sides(0., 0., 0., 3.),
                min_size: Size {
                    width: length(0.),
                    height: auto(),
                },
                ..Default::default()
            })
            .paint(Paint {
                radii: [0., 5., 5., 0.],
                fill: Some(Color::hex("#0c0c0cdb").into()),
                border: [0., 0., 0., 3.],
                border_color: crate::hud::tokens::HUD_GOLD,
                shadows: vec![BoxShadow::outer(0., 4., 0., Color::hex("#0009"))],
                ..Default::default()
            })
            .content(Content::Text {
                text: text.clone(),
                style: TextStyle::new(display(15., 600, false), Color::hex("#eee")),
                line_height: 20.25,
                wrap: true,
            }),
        ),
    )
}

fn label_el(text: &str, style: &TextStyle, size: f32) -> El {
    El::new(Style {
        min_size: Size {
            width: LengthPercentageAuto::length(0.),
            height: LengthPercentageAuto::auto(),
        },
        flex_shrink: 1.,
        ..Default::default()
    })
    .content(Content::Text {
        text: text.into(),
        style: style.clone(),
        line_height: size * 1.15,
        wrap: true,
    })
}

/// `--atv-gold-pill`.
pub fn gold_pill() -> Fill {
    Fill::Linear(Gradient::new(
        180.,
        &[
            (0., Color::hex("#fff7b8")),
            (0.46, Color::hex("#f6dc4a")),
            (0.5, Color::hex("#d4ae0c")),
            (0.78, Color::hex("#c19d07")),
            (1., Color::hex("#a88604")),
        ],
    ))
}

/// One glyph: the shape image with its drop shadow and any label on top.
fn glyph_el(g: &glyphs::Glyph, images: &glyphs::GlyphImages, size: f32) -> El {
    let k = size / 32.;
    let mut el = El::new(Style {
        position: Position::Relative,
        flex_shrink: 0.,
        size: Size {
            width: length(size),
            height: length(size),
        },
        ..Default::default()
    });
    if let Some(image) = images.get(g) {
        el = el.content(Content::Image {
            image,
            size: [size, size],
            fit: crate::layout::Fit::Fill,
            shadow: Some(crate::text::Shadow {
                dx: 0.,
                dy: 2.,
                blur: 0.,
                color: Color::hex("#000b"),
            }),
        });
    }
    if let Some(label) = glyphs::label(g) {
        let fs = label.size * k;
        let mut style = TextStyle::new(
            Font {
                family: "system-ui, sans-serif",
                size: fs,
                weight: 800,
                italic: false,
            },
            label.fill,
        );
        if let Some((color, width)) = label.stroke {
            style = style.stroke(width * k * 2., color, crate::text::Join::Round);
        }
        el = el.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    right: LengthPercentageAuto::length(0.),
                    // SVG baseline to the top of a one-line box.
                    top: LengthPercentageAuto::length(label.baseline * k - fs * 0.72),
                    bottom: LengthPercentageAuto::auto(),
                },
                display: Display::Flex,
                justify_content: JustifyContent::CENTER,
                ..Default::default()
            })
            .child(El::new(Style::default()).content(Content::Text {
                text: label.text,
                style,
                line_height: fs,
                wrap: false,
            })),
        );
    }
    el
}

fn chord_el(
    chord: &[glyphs::Glyph],
    images: &glyphs::GlyphImages,
    selected: bool,
    size: f32,
) -> El {
    let mut el = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(3.),
            height: px(3.),
        },
        ..Default::default()
    });
    for (i, g) in chord.iter().enumerate() {
        if i > 0 {
            let mut style = TextStyle::new(
                display(17., 800, false),
                if selected {
                    Color::hex("#24466f")
                } else {
                    Color::hex("#7ba3d8")
                },
            );
            if !selected {
                style = style.shadow(1., 1., 0., Color::hex("#000c"));
            }
            el = el.child(El::new(Style::default()).content(Content::Text {
                text: "+".into(),
                style,
                line_height: 17.,
                wrap: false,
            }));
        }
        el = el.child(glyph_el(g, images, size));
    }
    el
}

/// `.pad-binding`: alternatives joined by "/", or "Unbound".
fn binding_glyphs(
    chords: &[Vec<glyphs::Glyph>],
    images: &glyphs::GlyphImages,
    selected: bool,
    size: f32,
) -> El {
    let mut el = El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(6.),
            height: px(4.),
        },
        ..Default::default()
    });
    if chords.is_empty() {
        return el.child(El::new(Style::default()).content(Content::Text {
            text: "Unbound".into(),
            style: TextStyle::new(display(13., 700, false), Color::hex("#c4c4c4")),
            line_height: 15.6,
            wrap: false,
        }));
    }
    for (i, chord) in chords.iter().enumerate() {
        if i > 0 {
            el = el.child(El::new(Style::default()).content(Content::Text {
                text: "/".into(),
                style: TextStyle::new(
                    display(15., 700, false),
                    if selected {
                        Color::hex("#5a4a00")
                    } else {
                        Color::hex("#9a9a9a")
                    },
                ),
                line_height: 15.,
                wrap: false,
            }));
        }
        el = el.child(chord_el(chord, images, selected, size));
    }
    el
}

/// `.pad-summary`: the original Controller Settings layout panel.
fn summary_panel(
    pad: &glyphs::PadGlyphs,
    images: &glyphs::GlyphImages,
    w: f32,
    one_column: bool,
) -> El {
    let title_size = (w * 0.016).clamp(16., 19.);
    let row_size = (w * 0.015).clamp(15., 18.);
    let mut panel = El::new(Style {
        display: Display::Grid,
        grid_template_columns: if one_column {
            vec![fr(1.)]
        } else {
            vec![repeat(
                taffy::RepetitionCount::AutoFit,
                vec![minmax(length(220.), fr(1.))],
            )]
        },
        gap: Size {
            width: px(26.),
            height: px(6.),
        },
        padding: sides(18., 22., 18., 22.),
        border: sides(2., 2., 2., 2.),
        ..Default::default()
    })
    .paint(Paint {
        radii: [14.; 4],
        fill: Some(Color::hex("#3f3f3fa8").into()),
        border: [2.; 4],
        border_color: Color::hex("#000"),
        shadows: vec![BoxShadow::outer(0., 4., 0., Color::hex("#0007"))],
        ..Default::default()
    });
    for (title, lines) in &pad.summary {
        let mut group = El::new(Style {
            display: Display::Grid,
            gap: Size {
                width: px(4.),
                height: px(4.),
            },
            align_content: AlignContent::START,
            ..Default::default()
        })
        .child(
            El::new(Style {
                margin: margins(4., 0., 2., 0.),
                ..Default::default()
            })
            .content(Content::Text {
                text: title.clone(),
                style: TextStyle::new(display(title_size, 700, false), Color::hex("#6b9dcf"))
                    .spacing(title_size * 0.03)
                    .shadow(1., 2., 0., Color::hex("#000c")),
                line_height: title_size * 1.2,
                wrap: false,
            }),
        );
        for (label, chords) in lines {
            let mut glyph_row = binding_glyphs(chords, images, false, 30.);
            glyph_row.style.flex_wrap = FlexWrap::NoWrap;
            group = group.child(
                El::new(Style {
                    display: Display::Grid,
                    grid_template_columns: vec![length(112.), minmax(length(0.), fr(1.))],
                    align_items: AlignItems::CENTER,
                    gap: Size {
                        width: px(12.),
                        height: px(12.),
                    },
                    min_size: Size {
                        width: auto(),
                        height: length(32.),
                    },
                    ..Default::default()
                })
                .child(glyph_row)
                .child(
                    El::new(Style::default()).content(Content::Text {
                        text: label.clone(),
                        style: TextStyle::new(display(row_size, 700, false), Color::hex("#c8c8c8"))
                            .spacing(row_size * 0.02)
                            .shadow(1., 2., 0., Color::hex("#000c")),
                        line_height: row_size * 1.2,
                        wrap: true,
                    }),
                ),
            );
        }
        panel = panel.child(group);
    }
    panel
}

/// `.atv-range`: an 8px track with a gold fill and a 22px gold thumb in a
/// 44px-high box (`fill` 0-1).
pub(crate) fn range(fill: f32) -> El {
    let track = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::length(0.),
            top: LengthPercentageAuto::length(18.),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: auto(),
            height: length(8.),
        },
        ..Default::default()
    })
    .paint(Paint {
        radii: [4.; 4],
        fill: Some(Fill::Linear(Gradient::new(
            90.,
            &[
                (0., Color::hex("#e5d360")),
                (fill, Color::hex("#e5d360")),
                (fill, Color::hex("#080808")),
                (1., Color::hex("#080808")),
            ],
        ))),
        border: [1.; 4],
        border_color: Color::hex("#b9b9b98c"),
        shadows: vec![BoxShadow::inset(0., 1., 2., Color::BLACK)],
        skew_x: 0.,
        radii_y: [0.; 4],
    });
    let thumb = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::percent(fill),
            top: LengthPercentageAuto::length(11.),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        margin: margins(0., 0., 0., -22. * fill),
        size: Size {
            width: length(22.),
            height: length(22.),
        },
        ..Default::default()
    })
    .paint(Paint {
        radii: [11.; 4],
        fill: Some(Fill::Linear(Gradient::new(
            180.,
            &[(0., Color::hex("#fffaa1")), (1., Color::hex("#a99900"))],
        ))),
        border: [1.; 4],
        border_color: Color::hex("#e5d360"),
        shadows: vec![BoxShadow::outer(0., 2., 5., Color::hex("#0008"))],
        skew_x: 0.,
        radii_y: [0.; 4],
    });
    // position: relative — the containing block of the track and thumb.
    El::new(Style {
        position: Position::Relative,
        size: Size {
            width: auto(),
            height: length(44.),
        },
        ..Default::default()
    })
    .child(track)
    .child(thumb)
}
