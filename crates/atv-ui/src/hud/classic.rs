//! The original front end's look, for the port's own in-ride overlays
//! (research/UI_AUDIT_HUD.md), from the exact-build menu pages
//! (private/reports/original-menu-reference): Quick Play, the Free Ride hub
//! and quick chat use these instead of card styles of their own.
//!
//! - The light silver panel (`bkg_01`, about #b2b2b2 to #9e9e9e, the
//!   front's `silver`) with dark lettering and thin row separators, as the
//!   Hall of Fame sheet lays out its table.
//! - Page titles in condensed bold italics, white, on the faint white strip
//!   (front `title_strip`).
//! - The glossy gold pill on the chosen row and small gold bullets
//!   (front `bullet`), gold values (#e0bc00).
//! - The player colours of MultiATVRiderSelect's four columns: red
//!   #e12b00, blue #0f91c4, green #87c71a, yellow #fee50e.
//! - Select Track's photo in a thin gold frame.
//! - The race HUD lettering for clocks and counts: the condensed italic at
//!   800 with the overlays' dark outline, in the overlay silver and gold.
use super::race::{RACE_GOLD, RACE_TEXT};
use crate::{
    color::{Color, Fill, Gradient},
    layout::{Content, El, Fit, Paint, margins, sides},
    menu::gold_pill,
    paint::{BoxShadow, ImageId},
    text::{Font, Join, TextStyle},
};
use taffy::prelude::*;

/// The overlay silver and gold (race HUD lettering).
pub const SILVER: Color = RACE_TEXT;
pub const GOLD: Color = RACE_GOLD;
/// The original's gold values and arrows.
pub const VALUE: Color = Color::hex("#e0bc00");
/// Lettering on the silver panel and the gold pill.
pub const INK: Color = Color::hex("#111111");
pub const BODY: Color = Color::hex("#3a3a3a");
/// Small capitals on silver: a dark gold.
pub const DARK_GOLD: Color = Color::hex("#6e5600");
/// MultiATVRiderSelect's player columns.
pub const PLAYERS: [Color; 4] = [
    Color::hex("#e12b00"),
    Color::hex("#0f91c4"),
    Color::hex("#87c71a"),
    Color::hex("#fee50e"),
];
/// The 48 px touch target every button keeps.
pub const TARGET: f32 = 48.;
/// The silver panel's padding.
pub const PAD: f32 = 12.;

/// A track photo registered with the renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Photo {
    pub image: ImageId,
    pub size: [f32; 2],
}

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

pub fn text(value: impl Into<String>, style: TextStyle, line_height: f32) -> El {
    El::new(Style::default()).content(Content::Text {
        text: value.into(),
        style,
        line_height,
        wrap: false,
    })
}

/// One line that shortens with "…" in the room it gets.
pub fn ellipsis(value: impl Into<String>, style: TextStyle, line_height: f32) -> El {
    El::new(Style {
        flex_grow: 1.,
        flex_shrink: 1.,
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .content(Content::Ellipsis {
        text: value.into(),
        style,
        line_height,
    })
}

/// The race HUD's lettering (Meter::text): the italic face, 800, with the
/// overlays' `#080808bb` outline.
pub fn hud_style(size: f32, color: Color) -> TextStyle {
    TextStyle::new(display(size, 800, true), color).stroke(
        (size * 0.08).max(1.5),
        Color::hex("#080808bb"),
        Join::Round,
    )
}

pub fn hud_text(value: impl Into<String>, size: f32, color: Color) -> El {
    text(value, hud_style(size, color), size * 1.1)
}

/// Dark lettering on silver or gold.
pub fn ink(size: f32, weight: u16, color: Color) -> TextStyle {
    TextStyle::new(display(size, weight, false), color)
}

/// The front's silver (`bkg_01`).
pub fn silver() -> Fill {
    Fill::Linear(Gradient::new(
        180.,
        &[
            (0., Color::hex("#bcbcbc")),
            (0.48, Color::hex("#a9a9a9")),
            (1., Color::hex("#8c8c8c")),
        ],
    ))
}

/// A lighter silver for a button or tile on the scene or the panel.
pub fn silver_light() -> Fill {
    Fill::Linear(Gradient::new(
        180.,
        &[
            (0., Color::hex("#e6e6e6")),
            (0.5, Color::hex("#c9c9c9")),
            (1., Color::hex("#a8a8a8")),
        ],
    ))
}

/// The glossy gold pill's paint (front `pill_shadows`).
pub fn gold_paint(radius: f32) -> Paint {
    Paint::fill(gold_pill())
        .radius(radius)
        .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fffbe0")))
        .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")))
}

/// A silver plate's paint: light silver with the panel's edge light.
pub fn silver_paint(radius: f32) -> Paint {
    Paint::fill(silver_light())
        .radius(radius)
        .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffffb3")))
        .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0006")))
}

/// The silver panel: `width` wide, its children in a column.
pub fn panel(width: f32) -> El {
    El::new(Style {
        size: Size {
            width: length(width),
            height: auto(),
        },
        padding: sides(PAD, PAD, PAD, PAD),
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: length(0.),
            height: length(8.),
        },
        ..Default::default()
    })
    .paint(
        Paint::fill(silver())
            .radius(6.)
            .shadow(BoxShadow::outer(0., 6., 16., Color::hex("#0008")))
            .shadow(BoxShadow::inset(2., 0., 0., Color::hex("#f2f2f2")))
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff80"))),
    )
}

/// The page title: condensed bold italics, white, on the faint white strip
/// fading out to the right; `right` (a clock) at its end.
pub fn strip(title: &str, color: Color, right: Option<El>) -> El {
    let style = TextStyle::new(display(28., 800, true), color)
        .spacing(0.42)
        .shadow(2., 3., 0., Color::hex("#000a"))
        .shadow(0., 0., 18., Color::hex("#0006"));
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::SPACE_BETWEEN,
        gap: Size {
            width: length(10.),
            height: length(0.),
        },
        padding: sides(4., PAD, 6., PAD),
        min_size: Size {
            width: auto(),
            height: length(44.),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Gradient::new(
        90.,
        &[
            (0., Color::hex("#ffffff52")),
            (0.55, Color::hex("#ffffff26")),
            (1., Color::hex("#fff0")),
        ],
    )))
    .child(ellipsis(title, style, 29.4))
    .children(right)
}

/// A titled sheet: the strip over the silver panel holding `body`.
pub fn sheet(title: &str, right: Option<El>, width: f32, body: Vec<El>) -> El {
    sheet_in(title, Color::hex("#f6f6f6"), right, width, body)
}

/// [`sheet`] with its title lettered in `color` (gold for a win).
pub fn sheet_in(title: &str, color: Color, right: Option<El>, width: f32, body: Vec<El>) -> El {
    El::new(Style {
        size: Size {
            width: length(width),
            height: auto(),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: length(0.),
            height: length(4.),
        },
        ..Default::default()
    })
    .child(strip(title, color, right))
    .child(panel(width).children(body))
}

/// A button: the glossy gold pill for the main choice, a silver plate
/// beside it, dark lettering on both; the gold focus ring for keyboard and
/// controller use. 48 px tall.
pub fn button(id: u32, label: &str, primary: bool, focused: bool) -> El {
    let mut paint = if primary {
        gold_paint(5.)
    } else {
        silver_paint(5.)
    };
    if focused {
        paint = paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::outer(0., 0., 0., Color::hex("#f6e061e6"))
        });
    }
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        min_size: Size {
            width: length(TARGET * 2.),
            height: length(TARGET),
        },
        padding: sides(0., 20., 0., 20.),
        ..Default::default()
    })
    .id(id)
    .paint(paint)
    .child(text(label, ink(17., 800, INK).spacing(0.51), 17.))
}

/// A row of buttons, wrapping on a phone.
pub fn buttons(children: Vec<El>) -> El {
    El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: length(8.),
            height: length(8.),
        },
        margin: margins(4., 0., 0., 0.),
        ..Default::default()
    })
    .children(children)
}

/// A button the rider has chosen: the course list's white inset ring.
pub fn chosen(mut button: El, on: bool) -> El {
    if on {
        button.paint = button.paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::inset(0., 0., 0., Color::WHITE)
        });
    }
    button
}

/// A standings column: its label and whether it sits right.
pub struct Column {
    pub label: &'static str,
    pub right: bool,
}

/// One standings row.
pub struct Line {
    pub cells: Vec<String>,
    /// 1-based place: the top three get gold, silver and bronze.
    pub place: Option<usize>,
    /// The player's row: on the gold pill.
    pub me: bool,
    /// A rider who did not finish: greyed.
    pub dim: bool,
}

/// The place marker: the menu's gold bullet, in silver and bronze for 2nd
/// and 3rd, with the place on it; a plain number below the podium.
fn place(n: usize) -> El {
    let stops: Option<[&str; 4]> = match n {
        1 => Some(["#fff8bf", "#ead046", "#b49704", "#5e4d02"]),
        2 => Some(["#ffffff", "#d9d9d9", "#9a9a9a", "#4a4a4a"]),
        3 => Some(["#ffd9b0", "#d08a4b", "#8f5520", "#4a2a0c"]),
        _ => None,
    };
    let mut el = El::new(Style {
        size: Size {
            width: length(22.),
            height: length(22.),
        },
        flex_shrink: 0.,
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        ..Default::default()
    });
    if let Some(stops) = stops {
        el = el.paint(
            Paint::fill(Fill::radial_at(
                [0.34, 0.3],
                &[
                    (0., Color::hex(stops[0])),
                    (0.32, Color::hex(stops[1])),
                    (0.62, Color::hex(stops[2])),
                    (1., Color::hex(stops[3])),
                ],
            ))
            .radius(11.)
            .shadow(BoxShadow::outer(0., 1., 2., Color::hex("#000a"))),
        );
    }
    el.child(text(n.to_string(), ink(14., 800, INK), 14.))
}

/// The Hall of Fame sheet's table on the silver panel: small dark column
/// labels, then rows with thin separators; the player's row on the gold
/// pill. The first column takes the room left and shortens.
pub fn table(columns: &[Column], lines: &[Line]) -> El {
    let head = ink(12., 700, BODY).spacing(0.6);
    let mut rows = vec![table_row(
        columns.iter().map(|c| (c.label.to_string(), c.right)),
        head,
        None,
        Paint::default(),
        22.,
    )];
    for line in lines {
        let color = if line.dim {
            Color::hex("#6a6a6a")
        } else if line.me {
            INK
        } else {
            Color::hex("#1e1e1e")
        };
        let paint = if line.me {
            gold_paint(5.)
        } else {
            Paint {
                border: [0., 0., 1., 0.],
                border_color: Color::hex("#0000002e"),
                ..Default::default()
            }
        };
        rows.push(table_row(
            columns
                .iter()
                .enumerate()
                .map(|(c, col)| (line.cells.get(c).cloned().unwrap_or_default(), col.right)),
            ink(16., 700, color),
            line.place,
            paint,
            30.,
        ));
    }
    El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: length(0.),
            height: length(2.),
        },
        ..Default::default()
    })
    .children(rows)
}

fn table_row(
    cells: impl Iterator<Item = (String, bool)>,
    style: TextStyle,
    place_n: Option<usize>,
    paint: Paint,
    height: f32,
) -> El {
    let mut row = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: length(12.),
            height: length(0.),
        },
        min_size: Size {
            width: auto(),
            height: length(height),
        },
        padding: sides(0., 8., 0., 6.),
        ..Default::default()
    })
    .paint(paint);
    for (c, (value, right)) in cells.enumerate() {
        if c == 0 {
            row = row.child(
                El::new(Style {
                    display: Display::Flex,
                    align_items: AlignItems::CENTER,
                    gap: Size {
                        width: length(8.),
                        height: length(0.),
                    },
                    flex_grow: 1.,
                    flex_shrink: 1.,
                    min_size: Size {
                        width: length(0.),
                        height: auto(),
                    },
                    ..Default::default()
                })
                .children(place_n.map(place))
                .child(ellipsis(value, style.clone(), height * 0.7)),
            );
        } else {
            row = row.child(
                El::new(Style {
                    display: Display::Flex,
                    justify_content: if right {
                        JustifyContent::END
                    } else {
                        JustifyContent::START
                    },
                    min_size: Size {
                        width: length(44.),
                        height: auto(),
                    },
                    flex_shrink: 0.,
                    ..Default::default()
                })
                .child(text(value, style.clone(), height * 0.7)),
            );
        }
    }
    row
}

/// A Select Track choice: the track photo in the thin gold frame (or
/// `mark`, large, in its place: Random's "?"), the vote count in the gold
/// values over its corner, then the venue's whole name (wrapping to a
/// second line) and its kind in small capitals; the chosen card sits on the
/// gold pill. Focus shows the course list's white ring.
#[allow(clippy::too_many_arguments)]
pub fn card(
    id: Option<u32>,
    name: &str,
    kind: &str,
    votes: u8,
    chosen: bool,
    focused: bool,
    photo: Option<Photo>,
    mark: Option<&str>,
    width: f32,
) -> El {
    let inner = width - 8.;
    let photo_h = (inner * 2. / 3.).round();
    let mut frame = El::new(Style {
        size: Size {
            width: length(inner),
            height: length(photo_h),
        },
        border: sides(2., 2., 2., 2.),
        display: Display::Flex,
        align_items: AlignItems::FLEX_START,
        justify_content: JustifyContent::FLEX_END,
        padding: sides(2., 6., 0., 0.),
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint({
        let mut p = Paint::fill(Gradient::new(
            180.,
            &[(0., Color::hex("#9a9a9a")), (1., Color::hex("#7a7a7a"))],
        ));
        p.border = [2.; 4];
        p.border_color = VALUE;
        p
    });
    if let Some(photo) = photo {
        frame = frame.child(
            // Inside the 2 px frame, cropped to it (Cover).
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::length(0.),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                size: Size {
                    width: length(inner - 4.),
                    height: length(photo_h - 4.),
                },
                ..Default::default()
            })
            .content(Content::Image {
                image: photo.image,
                size: photo.size,
                fit: Fit::Cover,
                shadow: None,
            }),
        );
    }
    if let Some(mark) = mark.filter(|_| photo.is_none()) {
        frame = frame.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::length(0.),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                size: Size {
                    width: length(inner - 4.),
                    height: length(photo_h - 4.),
                },
                display: Display::Flex,
                align_items: AlignItems::CENTER,
                justify_content: JustifyContent::CENTER,
                ..Default::default()
            })
            .child(hud_text(
                mark,
                (photo_h * 0.6).clamp(28., 72.),
                Color::WHITE,
            )),
        );
    }
    if votes > 0 {
        frame = frame.child(hud_text(votes.to_string(), 26., VALUE));
    }
    let mut paint = if chosen {
        gold_paint(6.)
    } else {
        Paint::default().radius(6.)
    };
    if focused {
        paint = paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::inset(0., 0., 0., Color::WHITE)
        });
    }
    let mut el = El::new(Style {
        size: Size {
            width: length(width),
            height: auto(),
        },
        min_size: Size {
            width: auto(),
            height: length(TARGET),
        },
        padding: sides(4., 4., 5., 4.),
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: length(0.),
            height: length(2.),
        },
        ..Default::default()
    })
    .paint(paint)
    .child(frame)
    .child(
        El::new(Style {
            max_size: Size {
                width: percent(1.),
                height: auto(),
            },
            ..Default::default()
        })
        .content(Content::Text {
            text: name.into(),
            style: ink(15., 800, INK),
            line_height: 17.,
            wrap: true,
        }),
    )
    .children((!kind.is_empty()).then(|| {
        caps(
            kind,
            if chosen {
                Color::hex("#4a3a00")
            } else {
                DARK_GOLD
            },
        )
    }));
    if let Some(id) = id {
        el = el.id(id);
    }
    el
}

/// The width each of `per_row` cards gets on a panel `width` wide.
pub fn card_width(width: f32, per_row: usize) -> f32 {
    let n = per_row.max(1) as f32;
    ((width - 2. * PAD - 8. * (n - 1.)) / n).floor()
}

/// Cards or plates in a wrapping row.
pub fn grid(children: Vec<El>) -> El {
    El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: length(8.),
            height: length(8.),
        },
        ..Default::default()
    })
    .children(children)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.,
    }
}

/// A seat: MultiATVRiderSelect's column in the player's colour (by slot,
/// repeating past four), the name in white over it, a HOST or CPU marker,
/// ping bars and the ready tick. The player's own plate has the gold edge;
/// an open seat is a plain silver plate.
#[allow(clippy::too_many_arguments)]
pub fn plate(
    slot: usize,
    name: &str,
    marker: Option<&str>,
    ping: Option<u16>,
    tick: bool,
    me: bool,
    open: bool,
    width: f32,
) -> El {
    let base = PLAYERS[slot % PLAYERS.len()];
    let fill = if open {
        silver_light()
    } else {
        Fill::Linear(Gradient::new(
            180.,
            &[
                (0., mix(base, Color::WHITE, 0.3)),
                (0.45, base),
                (1., mix(base, Color::BLACK, 0.45)),
            ],
        ))
    };
    let mut paint = Paint::fill(fill)
        .radius(4.)
        .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff66")))
        .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0007")));
    if me {
        paint.border = [2.; 4];
        paint.border_color = VALUE;
    }
    let name_style = if open {
        ink(15., 700, Color::hex("#6a6a6a"))
    } else {
        TextStyle::new(display(16., 800, true), Color::WHITE).stroke(
            2.,
            Color::hex("#000000b3"),
            Join::Round,
        )
    };
    let mut row = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: length(5.),
            height: length(0.),
        },
        size: Size {
            width: length(width),
            height: length(40.),
        },
        padding: sides(0., 8., 0., 8.),
        ..Default::default()
    })
    .paint(paint)
    .child(ellipsis(if open { "Open" } else { name }, name_style, 20.));
    if let Some(marker) = marker {
        let host = marker == "HOST";
        row = row.child(
            El::new(Style {
                padding: sides(1., 5., 1., 5.),
                flex_shrink: 0.,
                ..Default::default()
            })
            .paint(if host {
                gold_paint(3.)
            } else {
                silver_paint(3.)
            })
            .child(text(marker, ink(11., 800, INK).spacing(0.6), 13.)),
        );
    }
    if let Some(ping) = ping {
        row = row.child(ping_bars(ping));
    }
    if tick {
        row = row.child(text(
            "✓",
            TextStyle::new(display(18., 900, false), Color::WHITE).stroke(
                2.,
                Color::hex("#000000b3"),
                Join::Round,
            ),
            18.,
        ));
    }
    row
}

/// Ping bars: three lit under 80 ms, two to 150, one above; white on the
/// plate, the unlit ones shaded.
fn ping_bars(ping: u16) -> El {
    let lit = match ping {
        0..80 => 3,
        80..=150 => 2,
        _ => 1,
    };
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::END,
        gap: Size {
            width: length(2.),
            height: length(0.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .children((0..3).map(move |i| {
        El::new(Style {
            size: Size {
                width: length(4.),
                height: length(5. + i as f32 * 4.),
            },
            ..Default::default()
        })
        .paint(
            Paint::fill(if i < lit {
                Color::WHITE
            } else {
                Color::hex("#00000059")
            })
            .radius(1.)
            .shadow(BoxShadow::outer(0., 1., 0., Color::hex("#0008"))),
        )
    }))
}

/// The menu sliders' track (`.atv-range` without its thumb): an 8 px bar,
/// gold to `fill` (0-1), dark after it, `width` wide.
pub fn meter(fill: f32, width: f32) -> El {
    let fill = fill.clamp(0., 1.);
    El::new(Style {
        size: Size {
            width: length(width),
            height: length(8.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(Paint {
        radii: [4.; 4],
        fill: Some(
            Gradient::new(
                90.,
                &[
                    (0., Color::hex("#e5d360")),
                    (fill, Color::hex("#e5d360")),
                    (fill, Color::hex("#080808")),
                    (1., Color::hex("#080808")),
                ],
            )
            .into(),
        ),
        border: [1.; 4],
        border_color: Color::hex("#b9b9b98c"),
        shadows: vec![BoxShadow::inset(0., 1., 2., Color::BLACK)],
        ..Default::default()
    })
}

/// Small capitals: a Select Track entry's kind line.
pub fn caps(value: impl Into<String>, color: Color) -> El {
    text(
        value.into().to_uppercase(),
        ink(12., 800, color).spacing(1.44),
        15.6,
    )
}
