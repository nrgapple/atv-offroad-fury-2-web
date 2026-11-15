//! The classic pieces the port's own in-ride overlays are built from
//! (research/UI_AUDIT_HUD.md), so Quick Play, the Free Ride hub and quick
//! chat read as part of the original HUD instead of new card styles:
//!
//! - the Race Results panel, its title band, standings table and race-over
//!   buttons (hud::results);
//! - the Select Track course entries with the menu's gold bullet and
//!   selection pill (front `course_list`, `bullet`);
//! - the race HUD lettering: the condensed italic face with the overlays'
//!   dark outline, in the overlay silver (190,190,190) and gold
//!   (218,192,31) of hud::race and hud::meter.
use super::race::{RACE_GOLD, RACE_TEXT};
use super::tokens::HUD_GOLD;
use crate::{
    color::{Color, Gradient},
    layout::{Content, El, Paint, margins, sides},
    menu::gold_pill,
    paint::BoxShadow,
    text::{Font, Join, TextStyle},
};
use taffy::prelude::*;

/// The overlay silver and gold.
pub const SILVER: Color = RACE_TEXT;
pub const GOLD: Color = RACE_GOLD;
/// The Race Results player row.
pub const PLAYER: Color = HUD_GOLD;
/// The Race Results cells and column labels.
pub const CELL: Color = Color::hex("#e8e8e8");
pub const HEAD: Color = Color::hex("#bdbdbd");
/// Dark lettering on the gold pill.
pub const INK: Color = Color::hex("#0d0b00");
/// The Race Results panel's side padding (the title band runs past it).
pub const PAD: f32 = 20.;
/// The 48 px touch target every button keeps.
pub const TARGET: f32 = 48.;

fn display(size: f32, weight: u16, italic: bool) -> Font {
    Font::display(size, weight, italic)
}

fn text(value: impl Into<String>, style: TextStyle, line_height: f32) -> El {
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

/// The Race Results panel's paint.
pub fn panel_paint() -> Paint {
    Paint::fill(Color::hex("#0c0c0cdb"))
        .radius(5.)
        .shadow(BoxShadow::outer(0., 6., 0., Color::hex("#0009")))
        .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#ffffff1a")))
}

/// The Race Results panel: `width` wide, its children in a column.
pub fn panel(width: f32) -> El {
    El::new(Style {
        size: Size {
            width: length(width),
            height: auto(),
        },
        padding: sides(0., PAD, 16., PAD),
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: length(0.),
            height: length(8.),
        },
        ..Default::default()
    })
    .paint(panel_paint())
}

/// The Race Results title band: the heading in condensed italics on the
/// band fading out to the right, `right` (a timer) at its end. The first
/// child of a [`panel`].
pub fn heading(title: &str, right: Option<El>) -> El {
    heading_in(title, Color::hex("#f6f6f6"), right)
}

/// [`heading`] lettered in `color` (gold for a win).
pub fn heading_in(title: &str, color: Color, right: Option<El>) -> El {
    let style = TextStyle::new(display(26., 800, true), color)
        .spacing(0.39)
        .shadow(2., 3., 0., Color::hex("#000a"));
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::SPACE_BETWEEN,
        gap: Size {
            width: length(10.),
            height: length(0.),
        },
        margin: margins(0., -PAD, 4., -PAD),
        padding: sides(6., PAD, 8., PAD),
        min_size: Size {
            width: auto(),
            height: length(44.),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Gradient::new(
        90.,
        &[
            (0., Color::hex("#ffffff38")),
            (0.55, Color::hex("#ffffff14")),
            (1., Color::hex("#fff0")),
        ],
    )))
    .child(ellipsis(title, style, 28.6))
    .children(right)
}

/// A race-over menu button (Race Results' Continue / Hide Menu): the gold
/// pill for the main choice, the quiet one beside it; the focus ring for
/// keyboard and controller use. 48 px tall.
pub fn button(id: u32, label: &str, primary: bool, focused: bool) -> El {
    let paint = if primary {
        Paint::fill(gold_pill())
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fffbe0")))
            .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")))
    } else {
        Paint::fill(Color::hex("#ffffff14"))
    };
    let mut paint = paint.radius(5.);
    if focused {
        paint = paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::outer(0., 0., 0., Color::hex("#f6e061b3"))
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
        padding: sides(0., 22., 0., 22.),
        ..Default::default()
    })
    .id(id)
    .paint(paint)
    .child(text(
        label,
        TextStyle::new(
            display(17., 700, false),
            if primary { INK } else { Color::hex("#f2f2f2") },
        )
        .spacing(0.51),
        17.,
    ))
}

/// A row of race-over buttons, wrapping on a phone.
pub fn buttons(children: Vec<El>) -> El {
    El::new(Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        gap: Size {
            width: length(8.),
            height: length(8.),
        },
        margin: margins(4., 0., 0., 0.),
        ..Default::default()
    })
    .children(children)
}

/// A standings column: its label and whether it sits right.
pub struct Column {
    pub label: &'static str,
    pub right: bool,
}

/// One standings row: its cells and how it is lit.
pub struct Line {
    pub cells: Vec<String>,
    /// The player's row (Race Results' gold).
    pub me: bool,
    /// An empty seat or a rider who left: dimmed.
    pub dim: bool,
}

/// The Race Results standings: grey column labels, then the rows, the
/// player's in gold. The first column takes the room left and shortens.
pub fn standings(columns: &[Column], lines: &[Line]) -> El {
    let head = TextStyle::new(display(13., 700, false), HEAD).spacing(0.52);
    let body = TextStyle::new(display(16., 700, false), CELL);
    let mut cells = Vec::new();
    for (c, column) in columns.iter().enumerate() {
        cells.push(table_cell(
            column.label,
            head.clone(),
            22.1,
            column.right,
            c == 0,
        ));
    }
    for line in lines {
        let style = if line.me {
            TextStyle::new(body.font, PLAYER)
        } else if line.dim {
            TextStyle::new(body.font, Color::hex("#8a8a8a"))
        } else {
            body.clone()
        };
        for (c, column) in columns.iter().enumerate() {
            let value = line.cells.get(c).map_or("", String::as_str);
            cells.push(table_cell(value, style.clone(), 27.2, column.right, c == 0));
        }
    }
    let mut tracks = vec![minmax(length(0.), fr(1.))];
    tracks.extend(columns.iter().skip(1).map(|_| auto()));
    El::new(Style {
        display: Display::Grid,
        grid_template_columns: tracks,
        ..Default::default()
    })
    .children(cells)
}

/// A standings cell: the first column shortens with "…", the others keep
/// their width.
fn table_cell(value: &str, style: TextStyle, line: f32, right: bool, first: bool) -> El {
    let el = El::new(Style {
        display: Display::Flex,
        justify_content: if right {
            JustifyContent::END
        } else {
            JustifyContent::START
        },
        align_items: AlignItems::CENTER,
        padding: sides(0., 6., 0., 6.),
        min_size: Size {
            width: length(0.),
            height: length(line),
        },
        ..Default::default()
    });
    if first {
        el.child(ellipsis(value, style, line))
    } else {
        el.child(text(value, style, line))
    }
}

/// A Select Track course entry: the gold bullet (or, chosen, the glossy
/// gold pill behind dark lettering), the name, its kind in small gold
/// capitals under it and `right` (a vote count) at its end. Focus shows
/// the course list's white ring.
pub fn course(
    id: Option<u32>,
    name: &str,
    kind: &str,
    right: Option<El>,
    chosen: bool,
    focused: bool,
) -> El {
    let mut paint = Paint::default().radius(5.);
    if chosen {
        paint = Paint::fill(gold_pill())
            .radius(5.)
            .shadow(BoxShadow::inset(0., 1., 0., Color::hex("#fffbe0")))
            .shadow(BoxShadow::outer(0., 2., 0., Color::hex("#0009")));
    }
    if focused {
        paint = paint.shadow(BoxShadow {
            spread: 3.,
            ..BoxShadow::inset(0., 0., 0., Color::WHITE)
        });
    }
    let mut name_style = TextStyle::new(
        display(18., 700, false),
        if chosen { INK } else { Color::hex("#ececec") },
    )
    .spacing(0.36);
    if !chosen {
        name_style = name_style.shadow(1., 2., 0., Color::hex("#000b"));
    }
    let copy = El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        flex_grow: 1.,
        flex_shrink: 1.,
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .child(ellipsis(name, name_style, 20.7))
    .children((!kind.is_empty()).then(|| {
        text(
            kind.to_uppercase(),
            TextStyle::new(
                display(12., 700, false),
                Color::hex(if chosen { "#4a3a00" } else { "#e3c74a" }),
            )
            .spacing(1.44),
            15.6,
        )
    }));
    let mut el = El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: length(12.),
            height: length(0.),
        },
        min_size: Size {
            width: auto(),
            height: length(52.),
        },
        padding: sides(4., 12., 4., 10.),
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(paint)
    .child(crate::front::bullet_slot(chosen))
    .child(copy)
    .children(right);
    if let Some(id) = id {
        el = el.id(id);
    }
    el
}
