//! Medal and weekly badge icons (research/GHOST_CHALLENGES.md; a port
//! addition): a coin in the medal's colour with its initial, and a weekly
//! badge's pill, shared by the results, the course lists and the profile.
use crate::{
    color::{Color, Fill},
    layout::{Content, El, Paint},
    paint::BoxShadow,
    text::{Font, TextStyle},
};
use atv_core::medals::{Badge, Medal};
use taffy::prelude::*;

fn label(value: &str, size: f32, color: Color) -> El {
    El::new(Style::default()).content(Content::Text {
        text: value.into(),
        style: TextStyle::new(Font::display(size, 800, false), color),
        line_height: size,
        wrap: false,
    })
}

/// The menus' glossy bullet (`front::bullet`) in `base`: a lit top left
/// shading to a dark rim, with its short hard shadow.
fn gloss(base: Color) -> Paint {
    let shade = |k: f32| Color {
        r: base.r * k,
        g: base.g * k,
        b: base.b * k,
        a: base.a,
    };
    let light = Color {
        r: base.r + (1. - base.r) * 0.7,
        g: base.g + (1. - base.g) * 0.7,
        b: base.b + (1. - base.b) * 0.7,
        a: base.a,
    };
    Paint::fill(Fill::radial_at(
        [0.34, 0.3],
        &[
            (0., light),
            (0.32, base),
            (0.62, shade(0.75)),
            (1., shade(0.4)),
        ],
    ))
    .shadow(BoxShadow::outer(0., 1., 2., Color::BLACK))
    .shadow(BoxShadow::inset(0., -1., 1., Color::hex("#0006")))
}

/// A medal coin `size` pixels across.
pub fn coin(medal: Medal, size: f32) -> El {
    let size = size.max(0.);
    let base = Color::hex(medal.color());
    El::new(Style {
        size: Size {
            width: length(size),
            height: length(size),
        },
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(gloss(base).radius(size / 2.))
    .children((size >= 12.).then(|| label(&medal.label()[..1], size * 0.52, Color::hex("#1a1300"))))
}

/// A coin and a count ("G 3"), for the profile's medal strip.
pub fn counted(coin_el: El, count: u32, size: f32, color: Color) -> El {
    El::new(Style {
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        gap: Size {
            width: length(size * 0.25),
            height: length(0.),
        },
        flex_shrink: 0.,
        ..Default::default()
    })
    .child(coin_el)
    .child(label(&count.to_string(), size * 0.7, color))
}

/// The colour of a weekly badge's pill.
pub fn badge_color(badge: Badge) -> &'static str {
    match badge {
        Badge::Top1 => "#36c37a",
        Badge::Top10 => "#f2c832",
        Badge::Top25 => "#c9d2dc",
        Badge::Top50 => "#c8793a",
        Badge::Finished => "#8a8a8a",
    }
}

/// A weekly badge: a pill with its percent ("1%", "10%"… or "✓").
pub fn badge(badge: Badge, size: f32) -> El {
    let text = match badge {
        Badge::Finished => "Done".to_string(),
        other => other.label().trim_start_matches("Top ").to_string(),
    };
    El::new(Style {
        min_size: Size {
            width: length(size * 1.4),
            height: length(size),
        },
        padding: crate::layout::sides(0., size * 0.25, 0., size * 0.25),
        display: Display::Flex,
        align_items: AlignItems::CENTER,
        justify_content: JustifyContent::CENTER,
        flex_shrink: 0.,
        ..Default::default()
    })
    .paint(gloss(Color::hex(badge_color(badge))).radius(size / 2.))
    .child(label(&text, size * 0.5, Color::hex("#1a1300")))
}

/// The results' reveal: a quick overshoot (ease-out-back) from nothing to
/// full size over `t` 0..1.
pub fn pop(t: f32) -> f32 {
    let t = t.clamp(0., 1.) - 1.;
    1. + 2.7 * t * t * t + 1.7 * t * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pop_overshoots_then_settles() {
        assert!(pop(0.).abs() < 1e-6);
        assert!((pop(1.) - 1.).abs() < 1e-6);
        assert!((0.0..1.2).contains(&pop(0.3)));
        assert!(pop(0.7) > 1.);
    }
}
