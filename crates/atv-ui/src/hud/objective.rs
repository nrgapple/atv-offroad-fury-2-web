//! The Freestyle objective HUD (research/CHAMPIONSHIPS.md "Freestyle
//! objectives"): the objective, the time left, its progress and the run's
//! score in a panel at the top centre, with the latest "%d Point Level
//! Reached" / "Repeated Combos Do Not Count" line under it.
use super::Viewport;
use crate::{
    color::Color,
    layout::{Content, El, Paint, sides},
    paint::BoxShadow,
    text::{Font, TextStyle},
};
use taffy::prelude::*;

/// What the panel shows this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub description: String,
    pub time_left: f32,
    pub progress: String,
    pub score: i64,
    /// The newest notice and its age in seconds.
    pub message: Option<String>,
}

/// M:SS, rounded up as a countdown reads.
pub fn clock(seconds: f32) -> String {
    let s = seconds.max(0.).ceil() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// 12,345.
pub fn grouped(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if value < 0 { format!("-{out}") } else { out }
}

fn line(value: String, size: f32, weight: u16, color: &str) -> El {
    El::new(Style {
        display: Display::Flex,
        justify_content: JustifyContent::CENTER,
        ..Default::default()
    })
    .child(
        El::new(Style::default()).content(Content::Text {
            text: value,
            style: TextStyle::new(Font::display(size, weight, false), Color::hex(color))
                .spacing(0.4)
                .shadow(1., 2., 0., Color::hex("#000c")),
            line_height: size * 1.25,
            wrap: false,
        }),
    )
}

/// The panel, laid out over the whole view (`top` clears the toolbar).
pub fn element(view: &Viewport, state: &View, top: f32) -> El {
    let width = 300f32.min(view.width - 24.);
    let low = state.time_left <= 10.;
    let mut panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length((view.width - width) / 2.),
            top: LengthPercentageAuto::length(top),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        size: Size {
            width: length(width),
            height: auto(),
        },
        padding: sides(6., 12., 8., 12.),
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..Default::default()
    })
    .paint(
        Paint::fill(Color::hex("#0c0c0cb8"))
            .radius(5.)
            .shadow(BoxShadow::outer(0., 3., 0., Color::hex("#0008"))),
    )
    .child(line(state.description.clone(), 16., 800, "#f1d84e"))
    .child(line(
        format!("TIME {}", clock(state.time_left)),
        22.,
        800,
        if low { "#ff6b5b" } else { "#f6f6f6" },
    ))
    .child(line(state.progress.clone(), 15., 700, "#e8e8e8"))
    .child(line(
        format!("SCORE {}", grouped(state.score)),
        15.,
        700,
        "#e8e8e8",
    ));
    if let Some(message) = &state.message {
        panel = panel.child(line(message.clone(), 15., 800, "#ffd166"));
    }
    El::new(Style {
        size: Size {
            width: length(view.width),
            height: length(view.height),
        },
        ..Default::default()
    })
    .child(panel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_and_grouping() {
        assert_eq!(clock(120.), "2:00");
        assert_eq!(clock(59.2), "1:00");
        assert_eq!(clock(0.), "0:00");
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(500), "500");
    }
}
