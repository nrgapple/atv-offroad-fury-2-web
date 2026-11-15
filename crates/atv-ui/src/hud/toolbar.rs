//! Top-left Pause and Camera (play.html #toolbar), and Chat or the Waypoint
//! Editor's Gate and Undo where they apply: bare icons in the race HUD's
//! overlay silver with its dark outline, gold while pressed, no boxes or
//! labels, so the ride keeps the original's few on-screen marks. Keyboard
//! riders toggle the controls card with H (the card says so).
use super::Viewport;
use super::race::{RACE_GOLD, RACE_TEXT};
use crate::{
    color::Color,
    layout::{Content, El},
    text::{Join, Shape, Stroke},
};
use taffy::prelude::*;

pub const PAUSE_ICON: &str = "M5 3h4v14H5zM12 3h4v14h-4z";
/// play.html's camera path with each implicit arc repeat spelled out (`a`), which
/// zeno's parser needs; browsers draw both forms identically.
pub const CAMERA_ICON: &str =
    "M6 3h8l2 3h2v11H2V6h2zm4 4a4 4 0 1 0 0 8a4 4 0 0 0 0-8zm0 2a2 2 0 1 1 0 4a2 2 0 0 1 0-4z";
/// A gate post pair and a plus (GATE), a counter-clockwise arrow (UNDO).
pub const GATE_ICON: &str = "M3 3h3v14H3zM14 3h3v14h-3zM9 9h2V7h1v2h2v1h-2v2h-1v-2H9z";
pub const UNDO_ICON: &str = "M8 4v3h5a5 5 0 0 1 0 10H7v-2h6a3 3 0 0 0 0-6H8v3L3 8z";
/// A button's touch target, and the icon drawn in it.
pub const TARGET: f32 = 44.;
const ICON: f32 = 26.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Pause = 1,
    Camera = 2,
    /// The Waypoint Editor's drop and take-back.
    Gate = 4,
    Undo = 5,
    /// Quick chat in a room whose server relays it (hud::chat).
    Chat = 6,
}

impl Button {
    pub fn from_id(id: u32) -> Option<Self> {
        [
            Self::Pause,
            Self::Camera,
            Self::Gate,
            Self::Undo,
            Self::Chat,
        ]
        .into_iter()
        .find(|b| *b as u32 == id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub paused: bool,
    pub hovered: Option<Button>,
    pub pressed: Option<Button>,
    pub camera_enabled: bool,
    /// The Waypoint Editor ride: GATE and UNDO follow.
    pub editor: bool,
    /// A room with quick chat: CHAT follows.
    pub chat: bool,
}

/// An icon in the race HUD's lettering: the overlay silver (or gold) with
/// the overlays' dark outline (hud::meter).
fn icon(path: &str, color: Color) -> Shape {
    let unit = ICON / 20.;
    Shape {
        path: path.into(),
        bounds: [0., 0., 20., 20.],
        unit,
        fill: color,
        stroke: Some(Stroke {
            width: 2.5,
            color: Color::hex("#080808bb"),
            join: Join::Round,
        }),
        drop_shadow: None,
    }
}

/// The buttons shown, in order: (button, icon, enabled).
pub fn buttons(state: &State) -> Vec<(Button, &'static str, bool)> {
    let mut buttons = vec![
        (
            Button::Pause,
            if state.paused {
                "M5 3l12 7-12 7z"
            } else {
                PAUSE_ICON
            },
            true,
        ),
        (Button::Camera, CAMERA_ICON, state.camera_enabled),
    ];
    if state.chat {
        buttons.push((Button::Chat, super::chat::CHAT_ICON, !state.paused));
    }
    if state.editor {
        buttons.push((Button::Gate, GATE_ICON, !state.paused));
        buttons.push((Button::Undo, UNDO_ICON, !state.paused));
    }
    buttons
}

/// The bar's width: its buttons side by side.
pub fn width(state: &State) -> f32 {
    buttons(state).len() as f32 * TARGET
}

/// The toolbar row positioned at the safe-area inset.
pub fn element(view: &Viewport, state: &State) -> El {
    let row = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            top: LengthPercentageAuto::length(view.safe[0].max(14.)),
            left: LengthPercentageAuto::length(view.safe[3].max(14.)),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        display: Display::Flex,
        ..Default::default()
    });
    row.children(buttons(state).into_iter().map(|(button, path, enabled)| {
        let active = enabled
            && (state.pressed == Some(button) || (view.hover && state.hovered == Some(button)));
        let color = if active { RACE_GOLD } else { RACE_TEXT };
        let mut el = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            size: Size {
                width: Dimension::length(TARGET),
                height: Dimension::length(TARGET),
            },
            ..Default::default()
        })
        .id(if enabled { button as u32 } else { 0 })
        .child(
            El::new(Style {
                size: Size {
                    width: Dimension::length(ICON),
                    height: Dimension::length(ICON),
                },
                flex_shrink: 0.,
                ..Default::default()
            })
            .content(Content::Shape(icon(path, color))),
        );
        if !enabled {
            el = el.opacity(0.4);
        }
        el
    }))
}
