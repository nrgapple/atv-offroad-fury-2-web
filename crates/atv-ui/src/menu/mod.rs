//! Pause and its settings screens (frontend.js `rebuildPausePanel` with the
//! game.css and menu-screens.css pause-panel rules). atv_core::ui_menu owns
//! the rows, labels, values and choice order exactly as for the browser; this
//! module owns the responsive panel, selection and pointer handling.
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
    text::{Font, Shape, TextStyle},
};
use taffy::prelude::*;

/// Hit ids: rows and sliders are numbered by their index in `choices`.
pub const ROW: u32 = 1000;
pub const PANEL: u32 = 900;
pub const CONTENT: u32 = 901;
/// The Jukebox spectrum's area (the host paints the bars each frame).
pub const BARS: u32 = 902;
/// A category in the Options rail while one of its pages is open, numbered
/// by its place in `State::hub`.
pub const TAB: u32 = 800;
/// The rows of the category Options shows beside its rail, numbered as the
/// category's own page numbers them from ROW.
pub const PREVIEW: u32 = 700;

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
    /// Options' categories (action, label): the rail beside its pages.
    pub hub: Vec<(String, String)>,
    /// On Options: the category under the selection and its rows, shown
    /// beside the rail.
    pub preview: Option<(String, Rows)>,
    /// The rail tab under the pointer.
    pub hovered_tab: Option<usize>,
}

/// Outlined buttons under a page's rows rather than rows.
const FOOTER: [&str; 6] = [
    "back",
    "pause",
    "bindcancel",
    "bindclear",
    "bindreset",
    "paddefaults",
];
const PAGER: [&str; 4] = ["bindprev", "bindnext", "musicprev", "musicnext"];
/// Pause's rows after the ride's own (Resume, Restart, Replay, Photo mode
/// and a room's rows): Options and the Jukebox, sharing, then leaving. A
/// rule divides the groups.
const PAUSE_GROUPS: [&[&str]; 3] = [
    &["settings", "jukebox"],
    &["copyride", "devlog"],
    &["main", "leave"],
];
/// Rows that open another page: they end in a chevron.
const OPENS: [&str; 16] = [
    "settings",
    "jukebox",
    "players",
    "game-settings",
    "controls",
    "hud-settings",
    "music",
    "accessibility",
    "extras",
    "alpha",
    "credits",
    "minimap-config",
    "race-overlays",
    "keyboard",
    "controller",
    "padbindings",
];

const INK: Color = Color::hex("#161100");
const TEXT: Color = Color::hex("#e6e6e6");
const MUTED: Color = Color::hex("#a9a9a9");
const RULE: Color = Color::hex("#ffffff1f");

/// The Options page a settings page sits under: its tab in the rail.
pub fn category(page: &str) -> Option<&'static str> {
    Some(match page {
        "game-settings" | "ecosystem-warning" => "game-settings",
        "controls" | "controller-settings" | "bindings" => "controls",
        "hud-settings" | "minimap-config" | "race-overlays" => "hud-settings",
        "music" => "music",
        "accessibility" => "accessibility",
        "extras" => "extras",
        "alpha" => "alpha",
        _ => return None,
    })
}

/// Options and its pages share one panel, categories in a rail beside the
/// open page, wherever that fits (not a phone held upright).
pub fn two_pane(view: &Viewport, page: &str) -> bool {
    (page == "settings" || category(page).is_some())
        && view.width - view.safe[1] - view.safe[3] >= 640.
}

/// One line on what an Options category holds.
fn blurb(page: &str) -> &'static str {
    match page {
        "jukebox" => "Play the soundtrack.",
        "game-settings" => "Units, plant collisions and sound output.",
        "controls" => "Touch, keyboard and controller.",
        "credits" => "The people behind the original game.",
        "hud-settings" => "Graphics, minimap and race overlays.",
        "music" => "Music, volumes and the song list.",
        "accessibility" => "Field of view, HUD size and riding assists.",
        "extras" => "Crashes, cameras and extra tricks.",
        "alpha" => "Experiments that may change or go away.",
        _ => "",
    }
}

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

/// Sizes and the state a page's parts are drawn from.
struct Look<'a> {
    state: &'a State,
    images: &'a glyphs::GlyphImages,
    /// One column of rows: a phone held upright.
    narrow: bool,
    /// A phone on its side.
    short: bool,
    font: f32,
    row: f32,
}

/// The full-viewport layer: dim backdrop and the centred panel.
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
    let (w, h) = (view.width, view.height);
    // The room left between notches (`env(safe-area-inset-*)`).
    let aw = w - view.safe[1] - view.safe[3];
    let ah = h - view.safe[0] - view.safe[2];
    let narrow = aw < 640.;
    let short = ah <= 450. && !narrow;
    let look = Look {
        state,
        images,
        narrow,
        short,
        font: if short { 16. } else { 17. },
        row: if short {
            42.
        } else if narrow {
            50.
        } else {
            46.
        },
    };
    let inset = if narrow || short { 10. } else { 24. };
    let (room_w, room_h) = (aw - 2. * inset, ah - 2. * inset);
    let panel = if state.page == "pause" {
        pause(&look, room_w, room_h)
    } else if two_pane(view, &state.page) {
        options(&look, room_w, room_h)
    } else {
        page(&look, room_w, room_h)
    };
    let stage = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(inset + view.safe[3]),
            top: LengthPercentageAuto::length(inset + view.safe[0]),
            right: LengthPercentageAuto::length(inset + view.safe[1]),
            bottom: LengthPercentageAuto::length(inset + view.safe[2]),
        },
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        ..Default::default()
    })
    .child(panel);
    let mut layer = El::new(Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        ..Default::default()
    });
    // Over a ride the scene dims; the frontend has its own backdrop.
    if !state.from_menu {
        layer = layer.paint(Paint::fill(Fill::ellipse_at(
            [0.5, 0.45],
            &[(0., Color::hex("#00000066")), (1., Color::hex("#000000c4"))],
        )));
    }
    layer
        .child(slide(stage, state))
        .children(notice(view, state))
}

/// The glass panel every page sits in.
fn panel(width: f32, height: Dimension, max_height: f32) -> El {
    El::new(Style {
        size: Size {
            width: length(width),
            height,
        },
        max_size: Size {
            width: auto(),
            height: length(max_height),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        border: sides(1., 1., 1., 1.),
        ..Default::default()
    })
    .paint(Paint {
        radii: [14.; 4],
        fill: Some(Color::hex("#0b0b0df2").into()),
        border: [1.; 4],
        border_color: RULE,
        shadows: vec![BoxShadow::outer(0., 16., 40., Color::hex("#000000a6"))],
        ..Default::default()
    })
}

/// The part of a panel that scrolls: `content` under the kinetic offset.
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

/// A page's name behind a gold mark, with any status line under it.
fn header(look: &Look, name: &str, detail: Option<&str>) -> El {
    let size = if look.short { 24. } else { 28. };
    let mark = El::new(Style {
        size: Size {
            width: length(4.),
            height: length(size * 0.78),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(Paint::fill(crate::hud::tokens::HUD_GOLD).radius(2.));
    let mut el = col(4.);
    el.style.padding = if look.short {
        sides(12., 18., 8., 18.)
    } else {
        sides(18., 20., 12., 20.)
    };
    el.style.flex_shrink = 0.;
    el.child(line(10.).child(mark).child(text(
        name,
        TextStyle::new(display(size, 800, true), Color::WHITE).spacing(size * 0.01),
        size * 1.1,
        false,
    )))
    .children(detail.filter(|d| !d.is_empty()).map(|detail| {
        text(
            detail,
            TextStyle::new(display(14., 400, false), MUTED),
            19.,
            true,
        )
    }))
}

/// The page's name: controller bindings are titled "Controller"
/// (bindingDevice).
fn heading(state: &State, page: &str) -> &'static str {
    if page == "bindings" && state.pad.is_some() {
        "Controller"
    } else if page == "back" {
        title("settings")
    } else {
        title(page)
    }
}

fn row_paint(selected: bool, hovered: bool) -> Paint {
    let mut paint = Paint::default().radius(8.);
    if selected {
        paint.fill = Some(gold_row());
        paint = paint
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fff6c2")))
            .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0007")));
    } else if hovered {
        paint.fill = Some(Color::hex("#ffffff14").into());
    }
    paint
}

/// A 6 by 12 arrowhead pointing left or right.
fn arrow(left: bool, color: Color) -> El {
    El::new(Style {
        size: Size {
            width: length(6.),
            height: length(12.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .content(Content::Shape(Shape {
        path: if left { "M7 0V14L0 7Z" } else { "M0 0V14L7 7Z" }.into(),
        bounds: [0., 0., 7., 14.],
        unit: 6. / 7.,
        fill: color,
        stroke: None,
        drop_shadow: None,
    }))
}

/// On or off as a switch: the knob sits right when on.
fn switch(on: bool, selected: bool) -> El {
    let (track, knob) = match (on, selected) {
        (true, false) => ("#e9c93a", "#1c1600"),
        (false, false) => ("#ffffff26", "#b9b9b9"),
        (true, true) => ("#241c00", "#ffe680"),
        (false, true) => ("#00000033", "#4a3c00"),
    };
    El::new(Style {
        size: Size {
            width: length(40.),
            height: length(22.),
        },
        flex_shrink: 0.,
        padding: sides(3., 3., 3., 3.),
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: if on {
            JustifyContent::FLEX_END
        } else {
            JustifyContent::FLEX_START
        },
        ..Default::default()
    })
    .paint(Paint::fill(Color::hex(track)).radius(11.))
    .child(
        El::new(Style {
            size: Size {
                width: length(16.),
                height: length(16.),
            },
            ..Default::default()
        })
        .paint(Paint::fill(Color::hex(knob)).radius(8.)),
    )
}

/// A row's value: a switch for On and Off, otherwise the choice between
/// the arrows that step it.
fn value(look: &Look, value: String, selected: bool) -> El {
    if value == "On" || value == "Off" {
        return switch(value == "On", selected);
    }
    let color = if selected {
        Color::hex("#3a2e00")
    } else {
        crate::hud::tokens::HUD_GOLD
    };
    let faint = if selected {
        Color::hex("#3a2e0099")
    } else {
        Color::hex("#ffffff59")
    };
    let style = TextStyle::new(display(15., 700, false), color).spacing(0.3);
    // A long value wraps on a phone; wider, it gives way to its label and
    // ends in an ellipsis instead of overrunning it.
    let label = if look.narrow {
        El::new(Style {
            flex_shrink: 1.,
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .content(Content::Text {
            text: value,
            style,
            line_height: 18.,
            wrap: true,
        })
    } else {
        El::new(Style {
            flex_shrink: 1.,
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            ..Default::default()
        })
        .content(Content::Ellipsis {
            text: value,
            style,
            line_height: 18.,
        })
    };
    let mut el = line(8.)
        .child(arrow(true, faint))
        .child(label)
        .child(arrow(false, faint));
    el.style.min_size.width = length(0.);
    if look.narrow {
        el.style.flex_shrink = 0.;
        el.style.max_size.width = percent(0.58);
    } else {
        el.style.flex_shrink = 1000.;
    }
    el
}

/// One row: its label, then its switch, its value or the mark of the page
/// it opens.
fn row(look: &Look, rows: &Rows, id: u32, action: &str, selected: bool, hovered: bool) -> El {
    let label = rows.label(action).unwrap_or(action);
    let (name, setting) = match label.split_once(": ") {
        Some((l, v)) => (l.to_string(), v.to_string()),
        None => (label.to_string(), String::new()),
    };
    let size = look.font;
    let color = if selected {
        INK
    } else if hovered {
        Color::WHITE
    } else {
        TEXT
    };
    let style = TextStyle::new(display(size, 700, false), color).spacing(size * 0.02);
    let mut el = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::SPACE_BETWEEN,
        gap: Size {
            width: px(12.),
            height: px(0.),
        },
        min_size: Size {
            width: length(0.),
            height: length(look.row),
        },
        padding: sides(4., 14., 4., 14.),
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(row_paint(selected, hovered))
    .id(id);
    if action.starts_with("bind:") || action.starts_with("song:") {
        // List rows: the value is a detail line under the label.
        let detail = TextStyle::new(
            display(12., 700, false),
            if selected {
                Color::hex("#3a2e00")
            } else {
                MUTED
            },
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
        let mut lines = col(3.);
        lines.style.min_size.width = length(0.);
        lines.style.flex_shrink = 1.;
        return el.child(
            lines
                .child(label_el(&name, &style, size))
                .child(glyph_row.unwrap_or_else(|| text(setting, detail, 16., true))),
        );
    }
    let mut label = label_el(&name, &style, size);
    // The label keeps its width; a long value ellipsizes instead.
    if !setting.is_empty() && !look.narrow {
        label.style.flex_shrink = 0.;
    }
    el = el.child(label);
    if !setting.is_empty() {
        el = el.child(value(look, setting, selected));
    } else if OPENS.contains(&action) {
        el = el.child(arrow(
            false,
            if selected {
                Color::hex("#3a2e0099")
            } else {
                Color::hex("#ffffff59")
            },
        ));
    }
    el
}

/// An Options category on a phone: its name over what it holds.
fn category_row(rows: &Rows, id: u32, action: &str, selected: bool, hovered: bool) -> El {
    let name = rows.label(action).unwrap_or(action);
    let color = if selected { INK } else { Color::WHITE };
    let mut lines = col(2.);
    lines.style.min_size.width = length(0.);
    lines.style.flex_shrink = 1.;
    lines = lines
        .child(label_el(
            name,
            &TextStyle::new(display(18., 700, false), color).spacing(0.3),
            18.,
        ))
        .child(text(
            blurb(action),
            TextStyle::new(
                display(13., 400, false),
                if selected {
                    Color::hex("#3a2e00")
                } else {
                    MUTED
                },
            ),
            17.,
            true,
        ));
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::SPACE_BETWEEN,
        gap: Size {
            width: px(12.),
            height: px(0.),
        },
        min_size: Size {
            width: length(0.),
            height: length(60.),
        },
        padding: sides(8., 14., 8., 14.),
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(row_paint(selected, hovered))
    .id(id)
    .child(lines)
    .child(arrow(
        false,
        if selected {
            Color::hex("#3a2e0099")
        } else {
            Color::hex("#ffffff59")
        },
    ))
}

/// An outlined button: Back, a list's Previous and Next, the binding
/// actions.
fn button(look: &Look, rows: &Rows, id: u32, action: &str, selected: bool, hovered: bool) -> El {
    let label = rows.label(action).unwrap_or(action);
    let color = if selected {
        INK
    } else if hovered {
        Color::WHITE
    } else {
        TEXT
    };
    let size = look.font - 1.;
    let style = TextStyle::new(display(size, 700, false), color).spacing(size * 0.02);
    let mut paint = row_paint(selected, hovered);
    if !selected {
        if !hovered {
            paint.fill = Some(Color::hex("#ffffff0d").into());
        }
        paint.border = [1.; 4];
        paint.border_color = RULE;
    }
    let mut el = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: px(10.),
            height: px(0.),
        },
        min_size: Size {
            width: length(0.),
            height: length(look.row.min(46.)),
        },
        padding: sides(0., 16., 0., 16.),
        border: sides(1., 1., 1., 1.),
        flex_grow: 1.,
        flex_shrink: 1.,
        // On a phone the binding actions take whole rows.
        flex_basis: if look.narrow && !PAGER.contains(&action) && action != "back" {
            percent(1.)
        } else {
            length(120.)
        },
        ..Default::default()
    })
    .paint(paint)
    .id(id);
    let back = action == "back" || action == "pause" || action.ends_with("prev");
    let label = label_el(label, &style, size);
    if back {
        el = el.child(arrow(true, color)).child(label);
    } else if action.ends_with("next") {
        el = el.child(label).child(arrow(false, color));
    } else {
        el = el.child(label);
    }
    el
}

fn slider(id: u32, s: &Slider, selected: bool) -> El {
    let units = if s.action == "minimap-size" {
        " px"
    } else {
        "%"
    };
    let fill = ((s.value - s.min) / (s.max - s.min)).clamp(0., 1.);
    let mut names = line(12.);
    names.style.justify_content = JustifyContent::SPACE_BETWEEN;
    names = names
        .child(text(
            s.label.clone(),
            TextStyle::new(display(15., 600, false), TEXT),
            20.,
            false,
        ))
        .child(text(
            format!("{}{units}", s.value.round() as i32),
            TextStyle::new(display(15., 700, false), crate::hud::tokens::HUD_GOLD),
            18.,
            false,
        ));
    let mut el = col(0.);
    el.style.padding = sides(8., 14., 2., 14.);
    el.style.flex_shrink = 0.;
    el = el.child(names).child(range(fill).id(id));
    if selected {
        el = el.paint(Paint::fill(Color::hex("#ffffff14")).radius(8.));
    }
    el
}

/// A page's scrolled rows and the buttons fixed under them. `base` is ROW
/// for the open page, or PREVIEW for the category Options shows beside its
/// rail, which has no selection yet. `railed`: the rail draws Back.
fn body(look: &Look, page: &str, rows: &Rows, base: u32, railed: bool) -> (El, Option<El>) {
    let state = look.state;
    let live = base == ROW;
    let selected = |i: usize| live && state.selection == i;
    let hovered = |i: usize| live && state.hovered == Some(i) && state.selection != i;
    let mut content = col(14.);
    content.style.padding = sides(2., 10., 14., 10.);
    let mut list = col(2.);
    let mut audio = col(2.);
    let mut pager = line(8.);
    let mut footer = line(8.);
    footer.style.flex_wrap = FlexWrap::Wrap;
    footer.style.flex_shrink = 0.;
    footer.style.padding = sides(10., 10., 10., 10.);
    footer.style.border = sides(1., 0., 0., 0.);
    footer.paint.border = [1., 0., 0., 0.];
    footer.paint.border_color = RULE;
    for (i, action) in rows.choices.iter().enumerate() {
        let (id, action) = (base + i as u32, action.as_str());
        // Audio: Music, the two volumes and Sound lead the page, over the
        // song list and its pager.
        if page == "music"
            && matches!(
                action,
                "musictoggle" | "musicvolume" | "sfxvolume" | "soundmode"
            )
        {
            audio = audio.child(match rows.slider(action) {
                Some(s) => slider(id, s, selected(i)),
                None => row(look, rows, id, action, selected(i), hovered(i)),
            });
        } else if let Some(s) = rows.slider(action) {
            list = list.child(slider(id, s, selected(i)));
        } else if PAGER.contains(&action) {
            pager = pager.child(button(look, rows, id, action, selected(i), hovered(i)));
        } else if FOOTER.contains(&action) {
            if live && !(railed && action == "back") {
                footer = footer.child(button(look, rows, id, action, selected(i), hovered(i)));
            }
        } else if page == "settings" {
            list = list.child(category_row(rows, id, action, selected(i), hovered(i)));
        } else {
            list = list.child(row(look, rows, id, action, selected(i), hovered(i)));
        }
    }
    if let Some(label) = state.capture.as_ref().filter(|_| live) {
        // The action being bound and how to bind it.
        let mut capture = col(8.).paint(Paint {
            radii: [10.; 4],
            fill: Some(Color::hex("#e3c86708").into()),
            border: [1.; 4],
            border_color: Color::hex("#e3c86760"),
            ..Default::default()
        });
        capture.style.padding = sides(18., 18., 18., 18.);
        capture.style.border = sides(1., 1., 1., 1.);
        content = content.child(
            capture
                .child(text(
                    label.clone(),
                    TextStyle::new(display(20., 600, false), Color::hex("#f4f5f7")),
                    29.,
                    true,
                ))
                .child(text(
                    "Press a key, controller button or move an axis. Hold several together for a combination. Release to save; Escape cancels.",
                    TextStyle::new(display(14., 400, false), MUTED),
                    22.,
                    true,
                )),
        );
    }
    if page == "players" && !state.roster.is_empty() {
        let mut roster = col(0.);
        for (label, status) in &state.roster {
            let mut rider = line(12.);
            rider.style.justify_content = JustifyContent::SPACE_BETWEEN;
            rider.style.padding = sides(0., 14., 0., 14.);
            rider.style.min_size.height = length(look.row);
            roster = roster.child(
                rider
                    .child(text(
                        label.clone(),
                        TextStyle::new(display(16., 600, false), Color::hex("#f4f5f7")),
                        20.,
                        true,
                    ))
                    .child(text(
                        status.clone(),
                        TextStyle::new(display(14., 600, false), MUTED),
                        20.,
                        false,
                    )),
            );
        }
        content = content.child(roster);
    }
    if page == "ecosystem-warning" {
        // Warning 0x90: the original's three two-line paragraphs.
        let mut warning = col(10.);
        warning.style.padding = sides(0., 14., 0., 14.);
        for pair in atv_core::ui_menu::ECOSYSTEM_WARNING.chunks(2) {
            warning = warning.child(text(
                pair.join(" "),
                TextStyle::new(display(16., 500, false), TEXT),
                24.,
                true,
            ));
        }
        content = content.child(warning);
    }
    if !audio.children.is_empty() {
        content = content.child(audio);
    }
    if !list.children.is_empty() {
        content = content.child(list);
    }
    if page == "controls" {
        let mut tips_el = col(6.);
        tips_el.style.margin = margins(0., 14., 0., 14.);
        tips_el.style.padding = sides(2., 0., 2., 14.);
        tips_el.style.border = sides(0., 0., 0., 3.);
        tips_el.paint.border = [0., 0., 0., 3.];
        tips_el.paint.border_color = Color::hex("#e8c832");
        for tip in tips(state.one_handed) {
            tips_el = tips_el.child(text(
                tip,
                TextStyle::new(display(14., 400, false), MUTED),
                21.,
                true,
            ));
        }
        content = content.child(tips_el);
    }
    if !pager.children.is_empty() {
        if let Some(count) = &state.page_count {
            let mut count = text(
                count.clone(),
                TextStyle::new(display(14., 600, false), TEXT),
                14.,
                false,
            );
            count.style.flex_shrink = 0.;
            count.style.padding = sides(0., 6., 0., 6.);
            pager.children.insert(1.min(pager.children.len()), count);
        }
        content = content.child(pager);
    }
    if let Some(pad) = state
        .pad
        .as_ref()
        .filter(|_| live && state.capture.is_none())
    {
        content = content.child(summary_panel(pad, look.images, 1100., false));
    }
    (content, (!footer.children.is_empty()).then_some(footer))
}

/// Pause: the ride's own rows, then the rest in groups. A phone on its
/// side sets the two side by side so nothing scrolls.
fn pause(look: &Look, room_w: f32, room_h: f32) -> El {
    let state = look.state;
    let choices = &state.rows.choices;
    let section = |actions: &mut dyn Iterator<Item = usize>| {
        let mut section = col(2.);
        for i in actions {
            section = section.child(row(
                look,
                &state.rows,
                ROW + i as u32,
                &choices[i],
                state.selection == i,
                state.hovered == Some(i) && state.selection != i,
            ));
        }
        section
    };
    let grouped = |action: &str| PAUSE_GROUPS.iter().any(|g| g.contains(&action));
    let ride = section(&mut (0..choices.len()).filter(|i| !grouped(&choices[*i])));
    let mut rest = col(8.);
    for group in PAUSE_GROUPS {
        let mut section = section(
            &mut group
                .iter()
                .filter_map(|action| choices.iter().position(|c| c == action)),
        );
        if section.children.is_empty() {
            continue;
        }
        if !rest.children.is_empty() || !look.short {
            section.style.padding = sides(8., 0., 0., 0.);
            section.style.border = sides(1., 0., 0., 0.);
            section.paint.border = [1., 0., 0., 0.];
            section.paint.border_color = RULE;
        }
        rest = rest.child(section);
    }
    let mut content;
    if look.short {
        content = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::FLEX_START,
            gap: Size {
                width: px(12.),
                height: px(0.),
            },
            ..Default::default()
        });
        for mut column in [ride, rest] {
            column.style.flex_grow = 1.;
            column.style.flex_basis = length(0.);
            column.style.min_size.width = length(0.);
            content = content.child(column);
        }
    } else {
        content = col(8.).child(ride).child(rest);
    }
    content.style.padding = sides(2., 10., 12., 10.);
    let width: f32 = if look.short { 660. } else { 420. };
    panel(width.min(room_w), auto(), room_h)
        .child(header(look, title("pause"), None))
        .child(scroller(state, content))
}

/// A page on its own: Pause's other pages, and Options on a phone, where
/// its categories are a list and each opens over it.
fn page(look: &Look, room_w: f32, room_h: f32) -> El {
    let state = look.state;
    let (content, footer) = body(look, &state.page, &state.rows, ROW, false);
    panel(600f32.min(room_w), auto(), room_h)
        .child(header(
            look,
            heading(state, &state.page),
            state.status.as_deref(),
        ))
        .child(scroller(state, content))
        .children(footer)
}

/// One category in the Options rail. `selected`: the rail has the
/// selection; `current`: its page is open beside the rail.
fn tab(id: u32, label: &str, height: f32, selected: bool, hovered: bool, current: bool) -> El {
    let color = if selected {
        INK
    } else if current || hovered {
        Color::WHITE
    } else {
        Color::hex("#c2c2c2")
    };
    let mut paint = row_paint(selected, hovered && !current);
    if current && !selected {
        paint.fill = Some(Color::hex("#ffffff1c").into());
    }
    let size = if height < 36. { 15. } else { 16. };
    let mark = El::new(Style {
        size: Size {
            width: length(3.),
            height: length(height * 0.42),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(
        Paint::fill(if current && !selected {
            crate::hud::tokens::HUD_GOLD
        } else {
            Color::hex("#fff0")
        })
        .radius(1.5),
    );
    let mut el = line(9.);
    el.style.min_size = Size {
        width: length(0.),
        height: length(height),
    };
    el.style.padding = sides(0., 10., 0., 8.);
    el.style.flex_shrink = 0.;
    el.paint(paint).id(id).child(mark).child(
        El::new(Style {
            min_size: Size {
                width: length(0.),
                height: auto(),
            },
            flex_shrink: 1.,
            ..Default::default()
        })
        .content(Content::Ellipsis {
            text: label.into(),
            style: TextStyle::new(display(size, 700, false), color).spacing(0.3),
            line_height: size * 1.15,
        }),
    )
}

/// Options where it fits: the categories in a rail and the open one beside
/// it. On Options itself the rail holds the selection and the category
/// under it shows ready to use; on a category's page the rail still
/// switches with one press.
fn options(look: &Look, room_w: f32, room_h: f32) -> El {
    let state = look.state;
    let choices = &state.rows.choices;
    let width = 940f32.min(room_w);
    let height = if look.short {
        room_h
    } else {
        600f32.min(room_h)
    };
    let on_hub = state.page == "settings";
    let current = category(&state.page);
    let top_level = on_hub || current == Some(state.page.as_str());
    let back = choices
        .iter()
        .position(|c| c == "back")
        .filter(|_| top_level);
    // Every category stays on screen: the tabs give up height first, then
    // the rail its name.
    let pad = if look.short { 10. } else { 14. };
    let button_h = look.row.min(46.) + 10.;
    let count = state.hub.len().max(1) as f32;
    let room = height - 2. - 2. * pad - if back.is_some() { button_h } else { 0. };
    let named = !look.short && room - 46. >= count * 40.;
    let tab_h = ((room - if named { 46. } else { 0. }) / count - 2.)
        .floor()
        .clamp(26., 44.);
    let mut tabs = col(2.);
    tabs.style.flex_grow = 1.;
    for (k, (action, label)) in state.hub.iter().enumerate() {
        let at = choices.iter().position(|c| c == action).filter(|_| on_hub);
        let (id, selected, hovered) = match at {
            Some(i) => (
                ROW + i as u32,
                state.selection == i,
                state.hovered == Some(i),
            ),
            None => (TAB + k as u32, false, state.hovered_tab == Some(k)),
        };
        tabs = tabs.child(tab(
            id,
            label,
            tab_h,
            selected,
            hovered,
            !on_hub && current == Some(action.as_str()),
        ));
    }
    let mut rail = col(0.).paint(Paint {
        radii: [13., 0., 0., 13.],
        fill: Some(Color::hex("#ffffff0b").into()),
        border: [0., 1., 0., 0.],
        border_color: RULE,
        ..Default::default()
    });
    rail.style.size.width = length(if width < 760. { 176. } else { 220. });
    rail.style.flex_shrink = 0.;
    rail.style.padding = sides(pad, 10., pad, 10.);
    rail.style.border = sides(0., 1., 0., 0.);
    if named {
        let mut name = text(
            title("settings"),
            TextStyle::new(display(24., 800, true), Color::WHITE).spacing(0.3),
            28.,
            false,
        );
        name.style.padding = sides(2., 8., 16., 8.);
        rail = rail.child(name);
    }
    rail = rail.child(tabs);
    if let Some(i) = back {
        let mut slot = line(0.);
        slot.style.padding = sides(10., 0., 0., 0.);
        rail = rail.child(slot.child(button(
            look,
            &state.rows,
            ROW + i as u32,
            "back",
            state.selection == i,
            state.hovered == Some(i) && state.selection != i,
        )));
    }
    let nothing = Rows::default();
    let (shown, rows, base) = if on_hub {
        match &state.preview {
            Some((page, rows)) => (page.as_str(), rows, PREVIEW),
            None => (
                choices.get(state.selection).map_or("", String::as_str),
                &nothing,
                PREVIEW,
            ),
        }
    } else {
        (state.page.as_str(), &state.rows, ROW)
    };
    let (content, footer) = body(look, shown, rows, base, top_level);
    // Audio's soundtrack line, or what the category holds.
    let detail = state.status.as_deref().or(Some(blurb(shown)));
    let mut pane = col(0.)
        .child(header(look, heading(state, shown), detail))
        .child(scroller(state, content))
        .children(footer);
    pane.style.flex_grow = 1.;
    pane.style.flex_basis = length(0.);
    pane.style.min_size = Size {
        width: length(0.),
        height: length(0.),
    };
    let mut panel = panel(width, length(height), room_h);
    panel.style.flex_direction = FlexDirection::Row;
    panel.child(rail).child(pane)
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

/// The selected row: the pill's gold without its gloss line.
fn gold_row() -> Fill {
    Fill::Linear(Gradient::new(
        180.,
        &[(0., Color::hex("#ffe873")), (1., Color::hex("#e2b40f"))],
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
