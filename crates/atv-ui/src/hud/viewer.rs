//! Photo mode and replay controls (#542): a row of buttons along the
//! bottom safe area and a one-line hint at the top, over a clean view (the
//! HUD, toolbar and touch controls hide while it shows).
use super::{Viewport, tokens};
use crate::{
    color::Color,
    layout::{Content, El, Paint, sides},
    text::{Font, TextStyle},
};
use taffy::prelude::*;

/// A control's hit id (clear of the toolbar's 1-3, the minimap's 10 and
/// the results' 30-36).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Back = 50,
    Save = 51,
    FovDown = 52,
    FovUp = 53,
}

impl Button {
    const ALL: [Self; 4] = [Self::Back, Self::Save, Self::FovDown, Self::FovUp];

    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|b| *b as u32 == id)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Back => "BACK",
            Self::Save => "SAVE PHOTO",
            Self::FovDown => "FOV −",
            Self::FovUp => "FOV +",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub buttons: Vec<Button>,
    /// The top line: what the camera controls are, or a notice.
    pub hint: String,
    /// The value beside the FOV buttons (degrees).
    pub fov: u32,
    pub pressed: Option<Button>,
    pub hovered: Option<Button>,
}

fn label_style(color: Color) -> TextStyle {
    let mut style = TextStyle::new(Font::display(14., 800, true), color).spacing(0.7);
    style.shadows = tokens::HUD_TEXT_SHADOW.to_vec();
    style
}

fn text(text: String, style: TextStyle) -> El {
    El::new(Style::default()).content(Content::Text {
        text,
        style,
        line_height: 14.,
        wrap: false,
    })
}

/// The hint and the control row, positioned in the safe area.
pub fn element(view: &Viewport, state: &State) -> El {
    let hint = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            top: LengthPercentageAuto::length(view.safe[0].max(14.)),
            left: LengthPercentageAuto::length(0.),
            right: LengthPercentageAuto::length(0.),
            bottom: LengthPercentageAuto::auto(),
        },
        display: Display::Flex,
        justify_content: JustifyContent::CENTER,
        ..Default::default()
    })
    .child(text(state.hint.clone(), label_style(tokens::SILVER_LIGHT)));
    let row = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            top: LengthPercentageAuto::auto(),
            left: LengthPercentageAuto::length(view.safe[3].max(8.)),
            right: LengthPercentageAuto::length(view.safe[1].max(8.)),
            bottom: LengthPercentageAuto::length(view.safe[2].max(10.)),
        },
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::CENTER,
        gap: Size {
            width: LengthPercentage::length(6.),
            height: LengthPercentage::length(6.),
        },
        ..Default::default()
    });
    let row = row.children(state.buttons.iter().map(|&button| {
        let active = state.pressed == Some(button) || (view.hover && state.hovered == Some(button));
        let color = if active {
            tokens::GOLD_LIGHT
        } else {
            Color::WHITE
        };
        let label = match button {
            Button::FovDown => format!("{} {}°", button.label(), state.fov),
            _ => button.label().to_string(),
        };
        El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            padding: sides(0., 14., 0., 14.),
            min_size: Size {
                width: LengthPercentageAuto::length(44.),
                height: LengthPercentageAuto::length(44.),
            },
            ..Default::default()
        })
        .id(button as u32)
        .paint(
            Paint::fill(if active {
                Color::hex("#000a")
            } else {
                Color::hex("#0008")
            })
            .radius(8.),
        )
        .child(text(label, label_style(color)))
    }));
    El::new(Style {
        size: Size {
            width: Dimension::length(view.width),
            height: Dimension::length(view.height),
        },
        ..Default::default()
    })
    .child(hint)
    .child(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_stay_clear_of_other_hud_targets() {
        for button in Button::ALL {
            assert_eq!(Button::from_id(button as u32), Some(button));
            assert!(button as u32 >= 50);
        }
        assert_eq!(Button::from_id(1), None);
    }
}
