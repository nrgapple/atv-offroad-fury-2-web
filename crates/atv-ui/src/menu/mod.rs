//! Pause and its settings screens (frontend.js `rebuildPausePanel` with the
//! game.css and menu-screens.css pause-panel rules). atv_core::ui_menu owns
//! the rows, labels, values and choice order exactly as for the browser; this
//! module owns the responsive panel, selection and pointer handling.
pub mod controller;
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
        "settings" => "Settings",
        "extras" => "Extras",
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
}

const FOOTER: [&str; 9] = [
    "back",
    "pause",
    "bindcancel",
    "bindclear",
    "bindreset",
    "paddefaults",
    "musictoggle",
    "musicvolume",
    "sfxvolume",
];
const PAGER: [&str; 4] = ["bindprev", "bindnext", "musicprev", "musicnext"];
const PAUSE_GROUPS: [&[&str]; 3] = [
    &["resume", "reset", "players", "changeride", "hosttrack"],
    &["settings", "jukebox"],
    &["copyride", "devlog", "main", "leave"],
];

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

/// The full-viewport layer: dim backdrop and the centred panel.
pub fn element(view: &Viewport, state: &State, images: &glyphs::GlyphImages) -> El {
    if state.page == "jukebox"
        && let Some(music) = &state.music
    {
        return jukebox::element(view, state, music, state.jukebox_art.as_ref(), state.frame)
            .children(notice(view, state));
    }
    let (w, h) = (view.width, view.height);
    let narrow = w <= 580.;
    let short = h <= 450. && !narrow;
    let pause = state.page == "pause";
    let inset = if narrow {
        12.
    } else if short {
        10.
    } else {
        24.
    };
    let width = if pause {
        if narrow { w - 24. } else { 520f32.min(w - 48.) }
    } else if narrow {
        w - 24.
    } else if short {
        820f32.min(w - 20.)
    } else {
        700f32.min(w - 48.)
    };
    let padding = if narrow || short { 20. } else { 30. };
    let gutter = (w * 0.032).clamp(16., 44.);
    let title_size = if state.from_menu {
        (w * 0.036).clamp(26., 44.)
    } else if narrow {
        30.
    } else if short {
        28.
    } else {
        (w * 0.044).clamp(30., 42.)
    };
    let (title_margin, title_pad) = if state.from_menu {
        (gutter, [6., 8.])
    } else if narrow || short {
        (20., if short { [2., 4.] } else { [4., 6.] })
    } else {
        (30., [4., 6.])
    };
    let font_size = (w * 0.02).clamp(17., 21.);
    let row_height = if short { 46. } else { 50. };

    let heading = El::new(Style {
        margin: margins(
            0.,
            -title_margin,
            if state.from_menu { 18. } else { 0. },
            -title_margin,
        ),
        padding: sides(title_pad[0], title_margin, title_pad[1], title_margin),
        ..Default::default()
    })
    .paint(Paint::fill(Gradient::new(
        90.,
        &if state.from_menu {
            [
                (0., Color::hex("#ffffff52")),
                (0.55, Color::hex("#ffffff26")),
                (1., Color::hex("#fff0")),
            ]
        } else {
            [
                (0., Color::hex("#ffffff38")),
                (0.55, Color::hex("#ffffff14")),
                (1., Color::hex("#fff0")),
            ]
        },
    )))
    .content(Content::Text {
        // Controller bindings are titled "Controller" (bindingDevice).
        text: if state.page == "bindings" && state.pad.is_some() {
            "Controller".into()
        } else {
            title(&state.page).into()
        },
        style: TextStyle::new(display(title_size, 800, true), Color::hex("#f6f6f6"))
            .spacing(title_size * 0.015)
            .shadow(2., 3., 0., Color::hex("#000a")),
        line_height: title_size * 1.1,
        wrap: false,
    });
    // The heading's bottom margin collapses into the header's (CSS blocks).
    let header = El::new(Style {
        // A block: the heading strip spans the panel.
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        margin: margins(0., 0., if state.from_menu { 0. } else { 14. }, 0.),
        ..Default::default()
    })
    .child(heading);

    let button = |index: usize, action: &str, footer: bool, big: bool| -> El {
        let label = state.rows.label(action).unwrap_or(action);
        let (text, value) = match label.split_once(": ") {
            Some((l, v)) => (l.to_string(), v.to_string()),
            None => (label.to_string(), String::new()),
        };
        let selected = state.selection == index;
        let hovered = state.hovered == Some(index) && !selected;
        let size = if big { 22. } else { font_size };
        let color = if selected {
            Color::hex("#0d0b00")
        } else if hovered {
            Color::WHITE
        } else {
            Color::hex("#d6d6d6")
        };
        let mut style = TextStyle::new(display(size, 700, false), color).spacing(size * 0.03);
        if !selected {
            style = style.shadow(1., 2., 0., Color::hex("#000c"));
        }
        let mut paint = Paint::default().radius(5.);
        if selected {
            paint.fill = Some(gold_pill());
            paint = paint
                .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fffbe0")))
                .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")));
        } else if hovered {
            paint.fill = Some(Color::hex("#ffffff12").into());
        } else if footer {
            paint.fill = Some(Color::hex("#ffffff0d").into());
            paint = paint.shadow(BoxShadow {
                spread: 1.,
                ..BoxShadow::inset(0., 0., 0., Color::hex("#ffffff1f"))
            });
        }
        let mut el = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: if footer {
                JustifyContent::CENTER
            } else {
                JustifyContent::SPACE_BETWEEN
            },
            gap: Size {
                width: px(if footer { 10. } else { 12. }),
                height: px(0.),
            },
            min_size: Size {
                width: LengthPercentageAuto::length(0.),
                height: LengthPercentageAuto::length(if big { 56. } else { row_height }),
            },
            padding: sides(0., 22., 0., 22.),
            flex_grow: if footer { 1. } else { 0. },
            flex_shrink: 1.,
            // ≤580px: footer buttons take whole rows (game.css).
            flex_basis: if footer && narrow && !PAGER.contains(&action) {
                percent(1.)
            } else if footer {
                length(140.)
            } else {
                auto()
            },
            ..Default::default()
        })
        .paint(paint)
        .id(ROW + index as u32);
        let back = action == "back" || action.ends_with("prev");
        if back || action.ends_with("next") {
            let path = if back { "M7 0V14L0 7Z" } else { "M0 0V14L7 7Z" };
            let icon = El::new(Style {
                size: Size {
                    width: length(7.),
                    height: length(14.),
                },
                flex_shrink: 0.,
                ..Default::default()
            })
            .content(Content::Shape(Shape {
                path: path.into(),
                bounds: [0., 0., 7., 14.],
                unit: 1.,
                fill: color,
                stroke: None,
                drop_shadow: None,
            }));
            let text = label_el(&text, &style, size);
            el = if back {
                el.child(icon).child(text)
            } else {
                el.child(text).child(icon)
            };
        } else if action.starts_with("bind:") || action.starts_with("song:") {
            // List rows: the value is a detail line under the label.
            let detail = TextStyle::new(
                display(12., 700, false),
                if selected {
                    Color::hex("#3a2e00")
                } else {
                    Color::hex("#c4c4c4")
                },
            );
            let glyph_row = state
                .pad
                .as_ref()
                .and_then(|p| p.row(action))
                .map(|chords| {
                    let mut row = binding_glyphs(chords, images, selected, 28.);
                    row.style.margin = margins(4., 0., 0., 0.);
                    row
                });
            el = el.child(
                El::new(Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    gap: Size {
                        width: px(0.),
                        height: px(4.),
                    },
                    min_size: Size {
                        width: LengthPercentageAuto::length(0.),
                        height: LengthPercentageAuto::auto(),
                    },
                    flex_shrink: 1.,
                    ..Default::default()
                })
                .child(label_el(&text, &style, size))
                .child(glyph_row.unwrap_or_else(|| {
                    El::new(Style::default()).content(Content::Text {
                        text: value.clone(),
                        style: detail,
                        line_height: 18.,
                        wrap: true,
                    })
                })),
            );
            return el;
        } else {
            el = el.child(label_el(&text, &style, size));
        }
        if !value.is_empty() {
            let value_color = if selected {
                Color::hex("#3a2e00")
            } else {
                Color::hex("#f1d84e")
            };
            // ≤580px: `.pause-value { max-width: 50% }` wraps long values.
            el = el.child(
                El::new(Style {
                    flex_shrink: 0.,
                    max_size: Size {
                        width: if narrow { percent(0.5) } else { auto() },
                        height: auto(),
                    },
                    ..Default::default()
                })
                .content(Content::Text {
                    text: value,
                    style: TextStyle::new(display(15., 700, false), value_color).spacing(0.45),
                    line_height: 18.,
                    wrap: narrow,
                }),
            );
        }
        el
    };

    let slider = |index: usize, s: &Slider| -> El {
        let selected = state.selection == index;
        let units = if s.action == "minimap-size" {
            " px"
        } else {
            "%"
        };
        let fill = ((s.value - s.min) / (s.max - s.min)).clamp(0., 1.);
        let heading = El::new(Style {
            display: Display::Flex,
            justify_content: JustifyContent::SPACE_BETWEEN,
            align_items: AlignItems::CENTER,
            gap: Size {
                width: px(12.),
                height: px(0.),
            },
            ..Default::default()
        })
        .child(El::new(Style::default()).content(Content::Text {
            text: s.label.clone(),
            style: TextStyle::new(display(15., 400, false), Color::hex("#e8e8e8")),
            line_height: 21.75,
            wrap: false,
        }))
        .child(El::new(Style::default()).content(Content::Text {
            text: format!("{}{units}", s.value.round() as i32),
            style: TextStyle::new(display(15., 700, false), Color::hex("#f1d84e")),
            line_height: 18.,
            wrap: false,
        }));
        let input = range(fill).id(ROW + index as u32);
        let mut el = El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            gap: Size {
                width: px(0.),
                height: px(4.),
            },
            padding: sides(10., 22., 10., 22.),
            ..Default::default()
        })
        .child(heading)
        .child(input);
        if selected {
            el = el.paint(Paint::fill(Color::hex("#ffffff0f")).radius(5.));
        }
        el
    };

    let column = |gap: f32| {
        El::new(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            gap: Size {
                width: px(0.),
                height: px(gap),
            },
            ..Default::default()
        })
    };
    let mut body;
    let choices = &state.rows.choices;
    if pause {
        body = column(8.);
        let mut first = true;
        for group in PAUSE_GROUPS {
            let mut section = column(2.);
            let mut any = false;
            for action in group {
                if let Some(i) = choices.iter().position(|c| c == action) {
                    section = section.child(button(i, action, false, false));
                    any = true;
                }
            }
            if !any {
                continue;
            }
            if !first {
                section.style.padding = sides(8., 0., 0., 0.);
                section.style.border = sides(1., 0., 0., 0.);
                section.paint.border = [1., 0., 0., 0.];
                section.paint.border_color = Color::hex("#ffffff1f");
            }
            first = false;
            body = body.child(section);
        }
    } else {
        body = column(if narrow { 20. } else { 24. });
        let mut settings = column(6.);
        let mut footer = El::new(Style {
            display: Display::Flex,
            flex_wrap: FlexWrap::Wrap,
            gap: Size {
                width: px(8.),
                height: px(8.),
            },
            padding: sides(8., 0., 0., 0.),
            ..Default::default()
        });
        let mut pager = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            gap: Size {
                width: px(8.),
                height: px(8.),
            },
            ..Default::default()
        });
        let big = state.page == "settings";
        for (i, action) in choices.iter().enumerate() {
            if let Some(s) = state.rows.slider(action) {
                settings = settings.child(slider(i, s));
            } else if PAGER.contains(&action.as_str()) {
                pager = pager.child(button(i, action, true, false));
            } else if FOOTER.contains(&action.as_str()) {
                footer = footer.child(button(i, action, true, false));
            } else {
                settings = settings.child(button(i, action, false, big));
            }
        }
        if let Some(label) = &state.capture {
            // .pause-capture: the action being bound and how to bind it.
            let mut capture = column(8.)
                .paint(Paint {
                    radii: [10.; 4],
                    fill: Some(Color::hex("#e3c86708").into()),
                    border: [1.; 4],
                    border_color: Color::hex("#e3c86760"),
                    ..Default::default()
                })
                .child(El::new(Style::default()).content(Content::Text {
                    text: label.clone(),
                    style: TextStyle::new(display(20., 600, false), Color::hex("#f4f5f7")),
                    line_height: 29.,
                    wrap: true,
                }))
                .child(El::new(Style::default()).content(Content::Text {
                    text: "Press a key, controller button or move an axis. Hold several together for a combination. Release to save; Escape cancels.".into(),
                    style: TextStyle::new(display(14., 400, false), Color::hex("#c4c4c4")),
                    line_height: 23.1,
                    wrap: true,
                }));
            capture.style.padding = sides(20., 20., 20., 20.);
            capture.style.border = sides(1., 1., 1., 1.);
            body = body.child(capture);
        }
        if state.page == "players" && !state.roster.is_empty() {
            let mut roster = column(0.);
            for (label, value) in &state.roster {
                let mut row = El::new(Style {
                    display: Display::Flex,
                    align_items: AlignItems::CENTER,
                    justify_content: JustifyContent::SPACE_BETWEEN,
                    gap: Size {
                        width: length(12.),
                        height: length(0.),
                    },
                    padding: sides(14., 14., 14., 14.),
                    min_size: Size {
                        width: auto(),
                        height: length(48.),
                    },
                    ..Default::default()
                });
                row = row
                    .child(El::new(Style::default()).content(Content::Text {
                        text: label.clone(),
                        style: TextStyle::new(display(16., 600, false), Color::hex("#f4f5f7")),
                        line_height: 20.,
                        wrap: true,
                    }))
                    .child(El::new(Style::default()).content(Content::Text {
                        text: value.clone(),
                        style: TextStyle::new(display(14., 600, false), Color::hex("#c4c4c4")),
                        line_height: 20.,
                        wrap: false,
                    }));
                roster = roster.child(row);
            }
            body = body.child(roster);
        }
        body = body.child(settings);
        if state.page == "controls" {
            let mut tips_el = column(6.);
            tips_el.style.padding = sides(2., 0., 2., 16.);
            tips_el.style.border = sides(0., 0., 0., 3.);
            tips_el.paint.border = [0., 0., 0., 3.];
            tips_el.paint.border_color = Color::hex("#e8c832");
            for line in tips(state.one_handed) {
                tips_el = tips_el.child(El::new(Style::default()).content(Content::Text {
                    text: line.into(),
                    style: TextStyle::new(display(14., 400, false), Color::hex("#c4c4c4")),
                    line_height: 23.1,
                    wrap: true,
                }));
            }
            body = body.child(tips_el);
        }
        if !pager.children.is_empty() {
            if let Some(count) = &state.page_count {
                let text = El::new(Style {
                    flex_shrink: 0.,
                    padding: sides(0., 6., 0., 6.),
                    ..Default::default()
                })
                .content(Content::Text {
                    text: count.clone(),
                    style: TextStyle::new(display(14., 600, false), Color::hex("#e8e8e8")),
                    line_height: 14.,
                    wrap: false,
                });
                pager.children.insert(1.min(pager.children.len()), text);
            }
            body = body.child(pager);
        }
        if !footer.children.is_empty() {
            body = body.child(footer);
        }
    }
    if state.from_menu {
        // `.pause-panel > :not(header) { max-width: 620px }`.
        body.style.max_size.width = length(620.);
    }
    let mut content = column(0.).id(CONTENT).child(header).child(body);
    if let Some(pad) = state.pad.as_ref().filter(|_| state.capture.is_none()) {
        // Wide menu screens put the panel in the empty right side.
        let side = state.from_menu && w >= 1100. && h >= 560.;
        let summary = summary_panel(pad, images, w, side);
        if side {
            let left = 620. + 2. * gutter;
            let mut slot = El::new(Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(left),
                    right: LengthPercentageAuto::length(gutter),
                    top: LengthPercentageAuto::length((h * 0.14).clamp(96., 128.)),
                    bottom: LengthPercentageAuto::auto(),
                },
                max_size: Size {
                    width: length(560.),
                    height: auto(),
                },
                ..Default::default()
            });
            slot = slot.child(summary);
            content = content.child(slot);
        } else {
            let mut slot = El::new(Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                margin: margins(22., 0., 0., 0.),
                ..Default::default()
            });
            if state.from_menu {
                slot.style.max_size.width = length(620.);
            }
            content = content.child(slot.child(summary));
        }
    }
    if state.from_menu {
        content.style.size.width = percent(1.);
        let panel = El::new(Style {
            size: Size {
                width: length(w),
                height: length(h),
            },
            padding: sides((h * 0.03).clamp(12., 28.), gutter, 20., gutter),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            overflow: taffy::Point {
                x: taffy::Overflow::Visible,
                y: taffy::Overflow::Scroll,
            },
            scrollbar_width: 0.,
            ..Default::default()
        })
        .id(PANEL)
        .scroll(state.scroll)
        .child(content);
        return El::new(Style {
            size: Size {
                width: length(w),
                height: length(h),
            },
            ..Default::default()
        })
        .child(panel)
        .children(notice(view, state));
    }
    let panel = El::new(Style {
        size: Size {
            width: length(width),
            height: auto(),
        },
        max_size: Size {
            width: auto(),
            height: length(h - 2. * inset),
        },
        padding: sides(padding, padding, padding, padding),
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        overflow: taffy::Point {
            x: taffy::Overflow::Visible,
            y: taffy::Overflow::Scroll,
        },
        scrollbar_width: 0.,
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#0c0c0cdb"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 6., 0., Color::hex("#0009")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1a"))),
    )
    .id(PANEL)
    .scroll(state.scroll)
    .child(content);
    // `inset` + `margin: auto` + `height: fit-content`: centred in the inset box.
    let stage = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(inset),
            top: LengthPercentageAuto::length(inset),
            right: LengthPercentageAuto::length(inset),
            bottom: LengthPercentageAuto::length(inset),
        },
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        ..Default::default()
    })
    .child(panel);
    El::new(Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Fill::ellipse_at(
        [0.5, 0.45],
        &[(0., Color::hex("#0000004d")), (1., Color::hex("#000000b3"))],
    )))
    .child(stage)
    .children(notice(view, state))
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
