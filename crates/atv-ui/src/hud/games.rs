//! The mini-game HUD (research/GAMES.md): Overlays.pb sets 6 (Treasure
//! Hunt) and 7 (King of the Hill) show TIME (the event time left), the
//! game's count (TOKENS collected / total, HILLS held / total), SCORE (the
//! stunt score), PLACE and the signs ("ATV %d captured hill %d", "All
//! Treasures Collected", "More Treasures in"). The port draws them as one
//! panel at the top centre with the start countdown and sign under it.
use super::Viewport;
use super::objective::grouped;
use crate::{
    color::Color,
    layout::{Content, El, Paint, sides},
    paint::BoxShadow,
    text::{Font, TextStyle},
};
use taffy::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub title: String,
    /// M:SS left.
    pub clock: String,
    /// Ten seconds or fewer left.
    pub low: bool,
    /// "TOKENS", "HILLS".
    pub count_label: String,
    /// "3 / 25".
    pub count: String,
    pub score: i64,
    pub place: usize,
    /// The start countdown's number, before the game begins.
    pub start: Option<u32>,
    /// The current sign.
    pub sign: Option<String>,
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
    let width = 320f32.min(view.width - 24.);
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
    .child(line(state.title.clone(), 16., 800, "#f1d84e"))
    .child(line(
        format!("TIME {}", state.clock),
        22.,
        800,
        if state.low { "#ff6b5b" } else { "#f6f6f6" },
    ))
    .child(line(
        format!(
            "{} {}   PLACE {}",
            state.count_label, state.count, state.place
        ),
        15.,
        700,
        "#e8e8e8",
    ))
    .child(line(
        format!("SCORE {}", grouped(state.score)),
        15.,
        700,
        "#e8e8e8",
    ));
    if let Some(n) = state.start {
        panel = panel.child(line(n.to_string(), 34., 900, "#ffd166"));
    }
    if let Some(sign) = &state.sign {
        panel = panel.child(line(sign.clone(), 16., 800, "#ffd166"));
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
