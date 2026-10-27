//! The Training Course lesson panel (research/TRAINING.md): the lesson
//! title, the current instruction caption (standing in for the original's
//! spoken line), the "Lesson Success" / "Lesson Failed" or lesson-name
//! banner and the distance to the arrow marker.
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
    /// "Lesson 3 of 6: Steering".
    pub title: String,
    /// The spoken line's caption.
    pub caption: Option<String>,
    /// The original's centre banner (lesson name, Lesson Success/Failed).
    pub banner: Option<String>,
    /// Distance to the marked circle, while riding a lesson.
    pub distance: Option<f32>,
    /// The hint while the ATV is held for a line ("Accept skips").
    pub hint: Option<String>,
}

fn line(value: String, size: f32, weight: u16, color: &str, wrap: bool) -> El {
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
            line_height: size * 1.3,
            wrap,
        }),
    )
}

/// The panel at the top centre and the banner across the middle.
pub fn element(view: &Viewport, state: &View, top: f32) -> El {
    // Wide views keep the centre clear for the marker tag: the panel sits
    // at the left under the toolbar.
    let width = if view.width >= 900. {
        380.
    } else {
        420f32.min(view.width - 24.)
    };
    let left = if view.width >= 900. {
        16.
    } else {
        (view.width - width) / 2.
    };
    let mut panel = El::new(Style {
        position: Position::Absolute,
        inset: taffy::Rect {
            left: LengthPercentageAuto::length(left),
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
    .child(line(state.title.clone(), 16., 800, "#f1d84e", false));
    if let Some(caption) = &state.caption {
        panel = panel.child(line(caption.clone(), 15., 700, "#f6f6f6", true));
    }
    if let Some(distance) = state.distance {
        panel = panel.child(line(
            format!("MARKER {:.0} FT", distance.max(0.)),
            14.,
            700,
            "#e8e8e8",
            false,
        ));
    }
    if let Some(hint) = &state.hint {
        panel = panel.child(line(hint.clone(), 12., 600, "#bdbdbd", false));
    }
    let mut root = El::new(Style {
        size: Size {
            width: length(view.width),
            height: length(view.height),
        },
        ..Default::default()
    })
    .child(panel);
    if let Some(banner) = &state.banner {
        let color = match banner.as_str() {
            "Lesson Success" => "#7ee07e",
            "Lesson Failed" => "#ff6b5b",
            _ => "#ffffff",
        };
        root = root.child(
            El::new(Style {
                position: Position::Absolute,
                inset: taffy::Rect {
                    left: LengthPercentageAuto::length(0.),
                    top: LengthPercentageAuto::length(view.height * 0.38),
                    right: LengthPercentageAuto::auto(),
                    bottom: LengthPercentageAuto::auto(),
                },
                size: Size {
                    width: length(view.width),
                    height: auto(),
                },
                display: Display::Flex,
                justify_content: JustifyContent::CENTER,
                ..Default::default()
            })
            .child(line(banner.clone(), 34., 900, color, false)),
        );
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_lays_out_with_and_without_banner() {
        let view = Viewport {
            width: 800.,
            height: 450.,
            scale: 1.,
            ..Default::default()
        };
        let mut state = View {
            title: "Lesson 1 of 6: Gas and Brake".into(),
            caption: Some("Ride to the marker and stop inside it.".into()),
            banner: Some("Lesson Success".into()),
            distance: Some(120.),
            hint: None,
        };
        let el = element(&view, &state, 50.);
        assert_eq!(el.children.len(), 2);
        state.banner = None;
        assert_eq!(element(&view, &state, 50.).children.len(), 1);
    }
}
