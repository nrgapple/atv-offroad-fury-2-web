//! The Credits page (Options → Credits, the original page 18): the original
//! `credits.txt` scroll, then the port's own section
//! (atv_core::credits). The script scrolls up from the bottom at its
//! `ScrollSpeed` (pixels of the original 448-line page per second) and
//! starts over once the last line has left; Back is the only row.
use super::{ROW, State, gold_pill};
use crate::{
    color::Color,
    hud::Viewport,
    layout::{Content, El, Paint, sides},
    text::{Font, TextStyle},
};
use atv_core::credits::{Credits, Entry};
use taffy::prelude::*;

/// The original page height the script's pixel values use.
const PAGE_HEIGHT: f32 = 448.;
/// A heading or name line (22-pixel font, -4 line offset) and an icon's
/// reserved space, in page pixels.
const LINE: f32 = 22.;
const ICON: f32 = 64.;

fn line_height(entry: &Entry, credits: &Credits) -> f32 {
    match entry {
        Entry::Blank => credits.blank_pixels,
        Entry::Icon(_) => ICON,
        Entry::Heading(_) | Entry::Name(_) => LINE,
    }
}

/// The script's height in page pixels.
pub fn script_height(credits: &Credits) -> f32 {
    credits
        .entries
        .iter()
        .map(|e| line_height(e, credits))
        .sum()
}

/// How far the script has moved up after `seconds`, in page pixels: it
/// enters from the bottom of a `view`-tall area and wraps once it has left
/// the top.
pub fn scroll_offset(credits: &Credits, seconds: f32, view: f32) -> f32 {
    let cycle = script_height(credits) + view;
    if cycle <= 0. || !seconds.is_finite() {
        return 0.;
    }
    (seconds.max(0.) * credits.scroll_speed).rem_euclid(cycle)
}

fn text(value: &str, style: TextStyle, line: f32) -> El {
    El::new(Style {
        min_size: Size {
            width: length(0.),
            height: auto(),
        },
        ..Default::default()
    })
    .content(Content::Text {
        text: value.into(),
        style,
        line_height: line,
        wrap: false,
    })
}

/// Disc column geometry, with a readable portrait fallback.
pub fn column(width: f32, height: f32, bounds: [f32; 4]) -> (f32, f32) {
    if width < height {
        return (16., (width - 32.).max(1.));
    }
    let scale = (width / 640.).min(height / 448.);
    (
        (width - 640. * scale) * 0.5 + bounds[0] * scale,
        (bounds[2] - bounds[0]) * scale,
    )
}

pub fn element(view: &Viewport, state: &State) -> El {
    let (w, h) = (view.width, view.height);
    let top = 16. + view.safe[0];
    let header_h = 52.;
    let area_top = top + header_h + 8.;
    let area_h = (h - area_top - 12. - view.safe[2]).max(80.);
    // Page pixels to view pixels: the script's 448-line page fills the area.
    let mut scale = (area_h / PAGE_HEIGHT).clamp(0.8, 2.2);
    let selected = state.rows.choices.get(state.selection).map(String::as_str) == Some("back");
    let label = state.rows.label("back").unwrap_or("Back");
    let dark = Color::hex("#0d0b00");
    let light = Color::hex("#e8e8e8");
    let mut back = El::new(Style {
        position: Position::Absolute,
        inset: Rect {
            left: length(16. + view.safe[3]),
            top: length(top),
            right: auto(),
            bottom: auto(),
        },
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        min_size: Size {
            width: length(88.),
            height: length(44.),
        },
        padding: sides(0., 14., 0., 14.),
        ..Default::default()
    })
    .paint(Paint {
        radii: [4.; 4],
        border: [1.; 4],
        border_color: Color::hex("#ffffff24"),
        fill: selected.then(gold_pill),
        ..Default::default()
    })
    .child(text(
        label,
        TextStyle::new(
            Font::display(20., 700, false),
            if selected { dark } else { light },
        ),
        20.,
    ));
    if let Some(at) = state.rows.choices.iter().position(|c| c == "back") {
        back = back.id(ROW + at as u32);
    }
    let title = El::new(Style {
        position: Position::Absolute,
        inset: Rect {
            left: length(0.),
            top: length(top + 4.),
            right: length(0.),
            bottom: auto(),
        },
        display: Display::Flex,
        justify_content: JustifyContent::CENTER,
        ..Default::default()
    })
    .child(text(
        "Credits",
        TextStyle::new(Font::display(34., 800, true), Color::hex("#f6f6f6")).shadow(
            2.,
            3.,
            0.,
            Color::hex("#000a"),
        ),
        38.,
    ));
    // Preserve the disc's right-hand column on landscape screens. Portrait
    // uses the full width so names remain readable (port adaptation).
    let bounds = state
        .credits
        .as_ref()
        .map_or(Credits::default().bounds, |c| c.bounds);
    let (column_x, column_w) = column(w, h, bounds);
    scale = scale.min(column_w / (bounds[2] - bounds[0]));
    // The scroll: only the lines inside the area are laid out.
    let mut area = El::new(Style {
        position: Position::Absolute,
        inset: Rect {
            left: length(column_x),
            top: length(area_top),
            right: length(0.),
            bottom: auto(),
        },
        size: Size {
            width: length(column_w),
            height: length(area_h),
        },
        overflow: taffy::Point {
            x: taffy::Overflow::Hidden,
            y: taffy::Overflow::Hidden,
        },
        ..Default::default()
    })
    .scroll(0.);
    if let Some(credits) = &state.credits {
        let [hr, hg, hb] = credits.heading_color;
        let [nr, ng, nb] = credits.name_color;
        let size = (LINE * scale).max(16.);
        let offset = scroll_offset(credits, state.credits_seconds, area_h / scale) * scale;
        let mut y = area_h - offset;
        for entry in &credits.entries {
            let height = line_height(entry, credits) * scale;
            let visible = y + height > 0. && y < area_h;
            let line = match entry {
                Entry::Heading(t) => Some((t, Color::rgb(hr, hg, hb), 700, 0.)),
                Entry::Name(t) => {
                    Some((t, Color::rgb(nr, ng, nb), 500, credits.name_indent * scale))
                }
                Entry::Icon(_) | Entry::Blank => None,
            };
            if visible && let Some((value, color, weight, indent)) = line {
                area = area.child(
                    El::new(Style {
                        position: Position::Absolute,
                        inset: Rect {
                            left: length(indent),
                            top: length(y),
                            right: length(0.),
                            bottom: auto(),
                        },
                        display: Display::Flex,
                        justify_content: JustifyContent::START,
                        ..Default::default()
                    })
                    .child(text(
                        value,
                        TextStyle::new(Font::display(size, weight, false), color),
                        height,
                    )),
                );
            }
            y += height;
        }
    }
    El::new(Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Color::hex("#050505f0")))
    .child(area)
    .child(title)
    .child(back)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landscape_uses_disc_column_portrait_stays_readable() {
        assert_eq!(column(640., 448., Credits::default().bounds), (359., 281.));
        assert_eq!(column(390., 844., Credits::default().bounds), (16., 358.));
    }

    #[test]
    fn the_script_enters_from_the_bottom_and_wraps() {
        let credits = Credits {
            scroll_speed: 40.,
            blank_pixels: 30.,
            entries: vec![
                Entry::Icon("logo".into()),
                Entry::Heading("A".into()),
                Entry::Name("B".into()),
                Entry::Blank,
            ],
            ..Default::default()
        };
        assert_eq!(script_height(&credits), 64. + 22. + 22. + 30.);
        assert_eq!(scroll_offset(&credits, 0., 448.), 0.);
        assert_eq!(scroll_offset(&credits, 2., 448.), 80.);
        // Height 138 + view 448: it starts over after 586 pixels.
        assert!((scroll_offset(&credits, 586. / 40. + 1., 448.) - 40.).abs() < 1e-3);
        assert_eq!(scroll_offset(&Credits::default(), 5., 0.), 0.);
    }
}
