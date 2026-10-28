//! The frontend's status bar: a mobile-game style row of chips across the top
//! of the menu pages (authored; the original kept these numbers on its
//! Profile pages). Left: the profile. Right: Profile Points (coins), the
//! championship trophies won, and the Pass Key sync state. Each chip shows
//! only an icon and a number (phones), with a short label when there is
//! room (desktop); tapping one opens a small popover with the details and a
//! link to the full page (progressive disclosure). The title page shows the
//! save chip alone; rides never show the bar (the HUD rules there).
//!
//! Icons are vector shapes in the menus' gold. The original's trophy art
//! (`trophy_goldon` and friends, PAGE26 `ProfileView_Champ`) is not in the
//! hosted runtime package, so the cup is drawn in its colours instead.
use super::{Metrics, display, text};
use crate::{
    color::{Color, Fill, Gradient},
    layout::{Content, El, Paint, px, sides},
    paint::BoxShadow,
    text::{Join, Shape, Stroke, TextStyle},
};
use taffy::prelude::*;

/// Hit ids: the chips, the popover's link and the area outside it.
pub const PROFILE: u32 = 2460;
pub const COINS: u32 = 2461;
pub const TROPHIES: u32 = 2462;
pub const SAVE: u32 = 2463;
pub const POPOVER_LINK: u32 = 2464;
pub const POPOVER_CLOSE: u32 = 2465;
/// The popover's panel (a tap inside it outside the link keeps it open).
pub const POPOVER_PANEL: u32 = 2466;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    Profile,
    Coins,
    Trophies,
    Save,
}

impl Chip {
    pub fn id(self) -> u32 {
        match self {
            Self::Profile => PROFILE,
            Self::Coins => COINS,
            Self::Trophies => TROPHIES,
            Self::Save => SAVE,
        }
    }

    pub fn from_id(id: u32) -> Option<Self> {
        Some(match id {
            PROFILE => Self::Profile,
            COINS => Self::Coins,
            TROPHIES => Self::Trophies,
            SAVE => Self::Save,
            _ => return None,
        })
    }
}

/// The Pass Key sync state the save chip shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sync {
    /// No progress saved yet, so no key.
    #[default]
    NoKey,
    Syncing,
    Synced,
    Offline,
    Error,
}

impl Sync {
    pub fn word(self) -> &'static str {
        match self {
            Self::NoKey => "No key",
            Self::Syncing => "Saving",
            Self::Synced => "Saved",
            Self::Offline => "Offline",
            Self::Error => "Error",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::NoKey => "#b9b9b9",
            Self::Syncing => "#f1d84e",
            Self::Synced => "#7ee07e",
            Self::Offline => "#ffb067",
            Self::Error => "#ff6b5b",
        }
    }
}

/// Everything the bar draws (built by the controller).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusBar {
    /// The loaded profile's name; None: no profile.
    pub profile: Option<String>,
    /// Profile Points to spend, and the value shown while it counts.
    pub points: i64,
    pub shown_points: i64,
    /// The count is animating (the chip glows).
    pub counting: bool,
    /// Championship trophies won, of those that can be won.
    pub trophies: (u32, u32),
    pub sync: Sync,
    /// The title page: the save chip alone.
    pub minimal: bool,
    /// Profile Store pages: the coin chip stands out.
    pub store: bool,
    /// The open popover: its chip, title, (label, value) lines and link,
    /// and the x its chip is centred on.
    pub open: Option<Popover>,
    pub anchor: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Popover {
    pub chip: Option<Chip>,
    pub title: String,
    pub lines: Vec<(String, String)>,
    pub link: Option<String>,
}

/// The bar's height below the safe area (CSS px): compact on short
/// landscape phones.
pub(super) fn height(m: &Metrics) -> f32 {
    if m.short { 40. } else { 48. }
}

/// The whole bar's height including the top safe area.
pub(super) fn total_height(m: &Metrics) -> f32 {
    height(m) + m.safe[0]
}

fn pill_height(m: &Metrics) -> f32 {
    if m.short {
        30.
    } else if m.w >= 900. {
        36.
    } else {
        34.
    }
}

/// Short labels next to the numbers when there is room (desktop, tablets).
fn labelled(m: &Metrics) -> bool {
    m.w >= 900.
}

pub fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn shape(path: &str, size: f32, fill: Color, stroke: Option<Stroke>) -> Content {
    Content::Shape(Shape {
        path: path.into(),
        bounds: [0., 0., 20., 20.],
        unit: size / 20.,
        fill,
        stroke,
        drop_shadow: Some(crate::text::Shadow {
            dx: 0.,
            dy: 1.,
            blur: 1.,
            color: Color::hex("#0009"),
        }),
    })
}

/// A 20-unit icon layered from shapes.
fn icon(size: f32, layers: Vec<Content>) -> El {
    let mut el = El::new(Style {
        position: Position::Relative,
        size: Size {
            width: length(size),
            height: length(size),
        },
        flex_shrink: 0.,
        ..Default::default()
    });
    for layer in layers {
        el = el.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::length(0.),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                size: Size {
                    width: length(size),
                    height: length(size),
                },
                ..Default::default()
            })
            .content(layer),
        );
    }
    el
}

const CIRCLE: &str = "M10 1.5a8.5 8.5 0 1 1 0 17a8.5 8.5 0 1 1 0-17z";

/// A gold coin with a raised rim and a stamped star.
fn coin(size: f32) -> El {
    icon(
        size,
        vec![
            shape(
                CIRCLE,
                size,
                Color::hex("#d9a514"),
                Some(Stroke {
                    width: 1.,
                    color: Color::hex("#7a5200"),
                    join: Join::Round,
                }),
            ),
            shape(
                "M10 4.2a5.8 5.8 0 1 1 0 11.6a5.8 5.8 0 1 1 0-11.6z",
                size,
                Color::hex("#f6d64a"),
                None,
            ),
            shape(
                "M10 5.6l1.3 2.8 3 .3-2.3 2 .7 3-2.7-1.6-2.7 1.6.7-3-2.3-2 3-.3z",
                size,
                Color::hex("#b07d00"),
                None,
            ),
        ],
    )
}

/// A gold cup on a dark plinth (the original trophies' gold).
fn trophy(size: f32) -> El {
    icon(
        size,
        vec![
            // Handles.
            shape(
                "M5 4H2.5v2.2C2.5 8.6 4 10 6.2 10.3M15 4h2.5v2.2c0 2.4-1.5 3.8-3.7 4.1",
                size,
                Color::TRANSPARENT,
                Some(Stroke {
                    width: 1.6,
                    color: Color::hex("#e3b11d"),
                    join: Join::Round,
                }),
            ),
            // Bowl, stem and base.
            shape(
                "M4.8 2.5h10.4v4.3c0 3-2.3 5.4-5.2 5.4S4.8 9.8 4.8 6.8zM8.8 12h2.4v2.6H8.8zM6 14.4h8v2.8H6z",
                size,
                Color::hex("#f1c22c"),
                None,
            ),
            shape("M5.2 17.2h9.6v1.6H5.2z", size, Color::hex("#3a2a00"), None),
            // Shine.
            shape(
                "M7 3.6h1.3v4.1c0 1 .4 1.9 1 2.5-1.4-.3-2.3-1.4-2.3-2.8z",
                size,
                Color::hex("#fff2a8"),
                None,
            ),
        ],
    )
}

/// A cloud with the sync state drawn in its colour.
fn cloud(size: f32, sync: Sync) -> El {
    let color = Color::hex(sync.color());
    let body = "M6 16.5a4.2 4.2 0 0 1-.6-8.4 5.3 5.3 0 0 1 10.1-.9A3.7 3.7 0 0 1 15 16.5z";
    let mark = match sync {
        Sync::Synced => Some(("M7.2 11.6l2 2 3.8-4.2", Color::hex("#0d2a0d"))),
        Sync::Syncing => Some(("M10 8v5.3M7.8 11.2 10 13.4l2.2-2.2", Color::hex("#2a2200"))),
        Sync::Offline => Some(("M3 3l14 14", Color::hex("#fbe6d0"))),
        Sync::Error => Some(("M10 8.2v3.4M10 13.4v.2", Color::hex("#2a0500"))),
        Sync::NoKey => None,
    };
    let mut layers = vec![shape(
        body,
        size,
        color,
        Some(Stroke {
            width: 0.8,
            color: Color::hex("#0008"),
            join: Join::Round,
        }),
    )];
    if let Some((path, ink)) = mark {
        layers.push(shape(
            path,
            size,
            Color::TRANSPARENT,
            Some(Stroke {
                width: 1.7,
                color: ink,
                join: Join::Round,
            }),
        ));
    }
    icon(size, layers)
}

/// The profile's avatar: a gold disc with the name's initial.
fn avatar(size: f32, name: Option<&str>) -> El {
    let initial = name
        .and_then(|n| n.chars().find(|c| c.is_alphanumeric()))
        .map_or("?".to_string(), |c| c.to_uppercase().to_string());
    El::new(Style {
        size: Size {
            width: length(size),
            height: length(size),
        },
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(
        Paint::fill(Fill::Linear(Gradient::new(
            180.,
            &[
                (0., Color::hex("#fff2a8")),
                (0.5, Color::hex("#f1c22c")),
                (1., Color::hex("#b07d00")),
            ],
        )))
        .radius(size / 2.)
        .shadow(BoxShadow::outer(0., 1., 2., Color::hex("#0008"))),
    )
    .child(text(
        initial,
        TextStyle::new(display(size * 0.56, 800, true), Color::hex("#1b1500")),
        size * 0.6,
    ))
}

/// One chip: a dark pill around an icon, a number and an optional label,
/// inside a hit area at least 44px wide and as tall as the bar.
fn chip(m: &Metrics, id: u32, hovered: bool, highlight: bool, body: Vec<El>) -> El {
    let h = pill_height(m);
    let mut pill = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(7.),
            height: px(0.),
        },
        min_size: Size {
            width: length(44.),
            height: length(h),
        },
        padding: sides(0., 11., 0., if m.short { 5. } else { 6. }),
        flex_shrink: 1.,
        ..Default::default()
    })
    .paint({
        let mut paint = Paint::fill(Color::hex(if hovered { "#000000b8" } else { "#0000009c" }))
            .radius(h / 2.)
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1f")));
        if highlight {
            paint.border = [1.5; 4];
            paint.border_color = Color::hex("#f1d84e");
            paint = paint.shadow(BoxShadow::outer(0., 0., 10., Color::hex("#f1d84e66")));
        }
        paint
    });
    for el in body {
        pill = pill.child(el);
    }
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        min_size: Size {
            width: length(44.),
            height: length(height(m)),
        },
        flex_shrink: 1.,
        ..Default::default()
    })
    .id(id)
    .child(pill)
}

fn value(m: &Metrics, text_: String) -> El {
    let size = if m.short { 15. } else { 17. };
    text(
        text_,
        TextStyle::new(display(size, 800, true), Color::hex("#f6f6f6"))
            .spacing(0.4)
            .shadow(0., 1., 2., Color::BLACK),
        size * 1.1,
    )
}

fn label(text_: &str) -> El {
    text(
        text_.to_uppercase(),
        TextStyle::new(display(11., 700, false), Color::hex("#f1d84e"))
            .spacing(1.)
            .shadow(0., 1., 2., Color::BLACK),
        12.,
    )
}

/// The bar across the top of a menu page (`hovered` is the page's hover).
pub(super) fn bar(m: &Metrics, bar: &StatusBar, hovered: Option<u32>) -> El {
    let icon_size = if m.short { 20. } else { 22. };
    let labels = labelled(m);
    let open = bar.open.as_ref().and_then(|p| p.chip);
    let hot = |id: u32| hovered == Some(id) || open.map(Chip::id) == Some(id);
    let mut row = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(0.),
            top: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(m.w),
            height: length(total_height(m)),
        },
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: px(if m.w < 380. { 4. } else { 8. }),
            height: px(0.),
        },
        padding: sides(m.safe[0], m.sides().1 - 4., 0., m.sides().0 - 4.),
        ..Default::default()
    });
    if !bar.minimal {
        let name = bar.profile.as_deref();
        let shown = match name {
            Some(n) if m.w < 380. && n.chars().count() > 8 => {
                format!("{}…", n.chars().take(7).collect::<String>())
            }
            Some(n) => n.to_string(),
            None => if m.w < 380. { "Guest" } else { "No profile" }.to_string(),
        };
        let mut profile = vec![avatar(pill_height(m) - 8., name)];
        profile.push(value(m, shown));
        row = row.child(chip(m, PROFILE, hot(PROFILE), false, profile));
    }
    // Push the rest to the right.
    row = row.child(El::new(Style {
        flex_grow: 1.,
        ..Default::default()
    }));
    if !bar.minimal {
        let mut coins = vec![coin(icon_size), value(m, grouped(bar.shown_points))];
        if labels {
            coins.push(label("Points"));
        }
        row = row.child(chip(m, COINS, hot(COINS), bar.store || bar.counting, coins));
        let mut cups = vec![
            trophy(icon_size),
            value(m, format!("{}/{}", bar.trophies.0, bar.trophies.1)),
        ];
        if labels {
            cups.push(label("Trophies"));
        }
        row = row.child(chip(m, TROPHIES, hot(TROPHIES), false, cups));
    }
    let mut save = vec![cloud(icon_size + 2., bar.sync)];
    if labels || bar.minimal {
        save.push(label(bar.sync.word()));
    }
    row.child(chip(m, SAVE, hot(SAVE), false, save))
}

/// The popover under its chip: a scrim catching taps outside, and the
/// panel with the details and the link (`anchor`: the chip's centre x).
pub(super) fn popover(m: &Metrics, pop: &Popover, anchor: f32) -> El {
    let width = (m.w - 24. - m.safe[1] - m.safe[3]).min(320.);
    let left = (anchor - width / 2.)
        .max(12. + m.safe[3])
        .min(m.w - 12. - m.safe[1] - width);
    let mut panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(left),
            top: LengthPercentageAuto::length(total_height(m) + 2.),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(width),
            height: auto(),
        },
        max_size: Size {
            width: auto(),
            height: length(m.h - total_height(m) - 12. - m.safe[2]),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: px(0.),
            height: px(6.),
        },
        padding: sides(14., 16., 14., 16.),
        overflow: taffy::Point {
            x: taffy::Overflow::Hidden,
            y: taffy::Overflow::Hidden,
        },
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#0c0c0cfc"))
            .radius(8.)
            .shadow(BoxShadow::outer(0., 8., 24., Color::hex("#000b")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1f"))),
    )
    .id(POPOVER_PANEL);
    panel = panel.child(text(
        pop.title.clone(),
        TextStyle::new(display(20., 800, true), Color::hex("#f4f4f4")).shadow(
            0.,
            1.,
            2.,
            Color::BLACK,
        ),
        24.,
    ));
    for (l, v) in &pop.lines {
        let mut line = El::new(Style {
            display: Display::Flex,
            justify_content: JustifyContent::SPACE_BETWEEN,
            gap: Size {
                width: px(12.),
                height: px(0.),
            },
            min_size: Size {
                width: auto(),
                height: length(22.),
            },
            ..Default::default()
        });
        line = line
            .child(text(
                l.clone(),
                TextStyle::new(display(14., 600, false), Color::hex("#c9c9c9")),
                18.,
            ))
            .child(text(
                v.clone(),
                TextStyle::new(display(15., 800, false), Color::hex("#f1d84e")),
                18.,
            ));
        panel = panel.child(line);
    }
    if let Some(link) = &pop.link {
        panel = panel.child(
            El::new(Style {
                display: Display::Flex,
                align_items: AlignItems::CENTER,
                justify_content: JustifyContent::CENTER,
                min_size: Size {
                    width: auto(),
                    height: length(44.),
                },
                margin: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    right: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::length(6.),
                    bottom: LengthPercentageAuto::length(0.),
                },
                ..Default::default()
            })
            .paint(super::pill_shadows(
                Paint::fill(crate::menu::gold_pill()).radius(5.),
            ))
            .id(POPOVER_LINK)
            .child(text(
                link.clone(),
                TextStyle::new(display(16., 800, true), Color::hex("#0d0b00")),
                18.,
            )),
        );
    }
    El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(0.),
            top: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(m.w),
            height: length(m.h),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Color::hex("#00000040")))
    .id(POPOVER_CLOSE)
    .child(panel)
}
