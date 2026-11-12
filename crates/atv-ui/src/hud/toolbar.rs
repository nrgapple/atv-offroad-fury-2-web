//! Top-left Pause / Camera / Keys buttons (play.html #toolbar, keyboard-help.js
//! toggle; game.css + hud.css `#toolbar .atv-control`).
use super::{Viewport, tokens};
use crate::{
    color::Color,
    layout::{Content, El, Paint, sides},
    text::{Font, Shadow, Shape, TextStyle},
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
pub const KEYS_ICON: &str = "M2 5h16v10H2zm2 2v2h2V7zm3 0v2h2V7zm3 0v2h2V7zm3 0v2h3V7zM4 10v2h2v-2zm3 0v2h6v-2zm7 0v2h2v-2z";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Pause = 1,
    Camera = 2,
    Keys = 3,
    /// The Waypoint Editor's drop and take-back.
    Gate = 4,
    Undo = 5,
}

impl Button {
    pub fn from_id(id: u32) -> Option<Self> {
        [
            Self::Pause,
            Self::Camera,
            Self::Keys,
            Self::Gate,
            Self::Undo,
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
    pub keys_enabled: bool,
    pub keys_visible: bool,
    /// Icon-only buttons (a top-right minimap meets the toolbar, or <=360px).
    pub compact: bool,
    /// The Waypoint Editor ride: GATE and UNDO follow.
    pub editor: bool,
}

/// `#toolbar .atv-control` label: italic 800 14px/1, .05em tracking.
pub fn label_style(color: Color) -> TextStyle {
    let shadow = tokens::HUD_TEXT_SHADOW;
    let mut style = TextStyle::new(Font::display(14., 800, true), color).spacing(0.7);
    style.shadows = shadow.to_vec();
    style
}

fn icon(path: &str, color: Color) -> Shape {
    Shape {
        path: path.into(),
        bounds: [0., 0., 20., 20.],
        unit: 1.,
        fill: color,
        stroke: None,
        drop_shadow: Some(Shadow {
            dx: 1.,
            dy: 1.,
            blur: 0.,
            color: Color::hex("#000c"),
        }),
    }
}

/// The toolbar row positioned at the safe-area inset.
pub fn element(view: &Viewport, state: &State) -> El {
    let mut buttons = vec![(
        Button::Pause,
        if state.paused {
            "M5 3l12 7-12 7z"
        } else {
            PAUSE_ICON
        },
        if state.paused { "RESUME" } else { "PAUSE" },
        true,
    )];
    buttons.push((Button::Camera, CAMERA_ICON, "CAMERA", state.camera_enabled));
    if state.keys_visible {
        buttons.push((Button::Keys, KEYS_ICON, "KEYS", state.keys_enabled));
    }
    if state.editor {
        buttons.push((Button::Gate, GATE_ICON, "GATE", !state.paused));
        buttons.push((Button::Undo, UNDO_ICON, "UNDO", !state.paused));
    }
    let compact = state.compact || view.width <= 360.;
    let row = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            top: LengthPercentageAuto::length(view.safe[0].max(14.)),
            left: LengthPercentageAuto::length(view.safe[3].max(14.)),
            right: LengthPercentageAuto::auto(),
            bottom: LengthPercentageAuto::auto(),
        },
        display: Display::Flex,
        gap: Size {
            width: LengthPercentage::length(8.),
            height: LengthPercentage::length(8.),
        },
        ..Default::default()
    });
    row.children(buttons.into_iter().map(|(button, path, label, enabled)| {
        let active = enabled
            && (state.pressed == Some(button) || (view.hover && state.hovered == Some(button)));
        let color = if active {
            tokens::GOLD_LIGHT
        } else {
            Color::WHITE
        };
        let mut el = El::new(Style {
            display: Display::Flex,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            gap: Size {
                width: LengthPercentage::length(8.),
                height: LengthPercentage::length(8.),
            },
            padding: sides(0., 12., 0., 12.),
            min_size: Size {
                width: LengthPercentageAuto::length(44.),
                height: LengthPercentageAuto::length(44.),
            },
            ..Default::default()
        })
        .id(if enabled { button as u32 } else { 0 })
        .child(
            El::new(Style {
                size: Size {
                    width: Dimension::length(20.),
                    height: Dimension::length(20.),
                },
                flex_shrink: 0.,
                ..Default::default()
            })
            .content(Content::Shape(icon(path, color))),
        );
        if active {
            el = el.paint(Paint::fill(Color::hex("#0002")));
        }
        if !compact {
            el = el.child(El::new(Style::default()).content(Content::Text {
                text: label.into(),
                style: label_style(color),
                line_height: 14.,
                wrap: false,
            }));
        }
        if !enabled {
            el = el.opacity(0.4);
        }
        el
    }))
}
