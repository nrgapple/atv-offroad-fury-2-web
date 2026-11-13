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
    Camera = 54,
    Rewind = 55,
    Play = 56,
    Forward = 57,
    Speed = 58,
    Photo = 59,
    /// The replay's scrub bar (pressed and dragged, not tapped).
    Scrub = 60,
}

impl Button {
    const ALL: [Self; 11] = [
        Self::Back,
        Self::Save,
        Self::FovDown,
        Self::FovUp,
        Self::Camera,
        Self::Rewind,
        Self::Play,
        Self::Forward,
        Self::Speed,
        Self::Photo,
        Self::Scrub,
    ];

    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|b| *b as u32 == id)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Back => "BACK",
            Self::Save => "SAVE PHOTO",
            Self::FovDown => "FOV −",
            Self::FovUp => "FOV +",
            Self::Camera => "CAMERA",
            Self::Rewind => "−2s",
            Self::Play => "PLAY",
            Self::Forward => "+2s",
            Self::Speed => "SPEED",
            Self::Photo => "PHOTO",
            Self::Scrub => "",
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
    /// Replay: playing, the camera and speed labels, and the position as a
    /// fraction (a scrub bar shows with one).
    pub playing: bool,
    pub shot: &'static str,
    pub speed: &'static str,
    pub progress: Option<f32>,
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
    let bottom = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            top: LengthPercentageAuto::auto(),
            left: LengthPercentageAuto::length(view.safe[3].max(8.)),
            right: LengthPercentageAuto::length(view.safe[1].max(8.)),
            bottom: LengthPercentageAuto::length(view.safe[2].max(10.)),
        },
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: LengthPercentage::length(4.),
            height: LengthPercentage::length(4.),
        },
        ..Default::default()
    });
    let row = El::new(Style {
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
            Button::Play if state.playing => "PAUSE".into(),
            Button::Camera => state.shot.to_uppercase(),
            Button::Speed => state.speed.into(),
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
    .child(match state.progress {
        Some(progress) => bottom.child(scrub(progress)).child(row),
        None => bottom.child(row),
    })
}

/// The scrub bar: a 44 px target around a thin track filled to `progress`.
fn scrub(progress: f32) -> El {
    let fill = El::new(Style {
        size: Size {
            width: Dimension::percent(progress.clamp(0., 1.)),
            height: Dimension::length(6.),
        },
        ..Default::default()
    })
    .paint(Paint::fill(tokens::GOLD).radius(3.));
    let track = El::new(Style {
        size: Size {
            width: Dimension::percent(1.),
            height: Dimension::length(6.),
        },
        ..Default::default()
    })
    .paint(Paint::fill(Color::hex("#fff5")).radius(3.))
    .child(fill);
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        size: Size {
            width: Dimension::percent(1.),
            height: Dimension::length(44.),
        },
        padding: sides(0., 8., 0., 8.),
        ..Default::default()
    })
    .id(Button::Scrub as u32)
    .child(track)
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

    #[test]
    fn a_replay_lays_out_its_scrub_bar_above_the_buttons() {
        let view = Viewport {
            width: 390.,
            height: 844.,
            scale: 3.,
            ..Default::default()
        };
        let state = State {
            buttons: vec![Button::Back, Button::Play],
            hint: "REPLAY".into(),
            fov: 0,
            playing: true,
            shot: "Chase",
            speed: "1×",
            progress: Some(0.5),
            pressed: None,
            hovered: None,
        };
        let (mut list, mut hits) = (crate::paint::DisplayList::default(), Vec::new());
        crate::layout::render(
            &element(&view, &state),
            &mut crate::text::Fonts::default(),
            view.width,
            view.height,
            &mut list,
            &mut hits,
        );
        let rect = |b: Button| hits.iter().find(|(_, id)| *id == b as u32).map(|(r, _)| *r);
        let (bar, back) = (rect(Button::Scrub).unwrap(), rect(Button::Back).unwrap());
        assert!(bar.y + bar.h <= back.y && bar.h >= 44. && bar.w > 300.);
    }
}
