//! First-time tips on the ride HUD: a gold arrow bobbing toward a control
//! and two lines of race-HUD lettering under it (research/FREERIDE_HUB.md
//! "First-time tips"). No box or button, so the tip reads as part of the
//! HUD; it fades in and out and a tap on it does what it points at.
//! Authored port presentation.
use super::classic;
use super::race::{RACE_GOLD, RACE_TEXT};
use crate::{
    color::{Color, Fill},
    paint::{BoxPaint, DisplayList, Item, Rect},
    text::{Fonts, Join, Shape, Stroke},
};

/// A tap on the tip.
pub const TIP_HIT: u32 = 331;

/// An arrow pointing up, in a 20 unit box.
const ARROW: &str = "M10 1l8 9h-5v9H7v-9H2z";

/// One tip: what it says and where it points.
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    /// The point the arrow's tip touches (the control's bottom centre).
    pub at: [f32; 2],
    pub title: String,
    pub detail: String,
}

/// Paints `tip` at `alpha` (its fade), the arrow bobbing on `now` (ms),
/// keeping the lettering `margin` px inside the view's left edge.
pub fn paint(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    hits: &mut Vec<(Rect, u32)>,
    tip: &Tip,
    now: f64,
    alpha: f32,
    margin: f32,
) {
    if alpha <= 0. {
        return;
    }
    // touch-hint-pulse's 1.4 s ease, as the touch coach's ring bobs.
    let phase = ((now % 1400.) / 1400.) as f32;
    let bob = 3. * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos());
    let [x, y] = tip.at;
    let title = classic::hud_style(17., RACE_GOLD);
    let detail = classic::hud_style(12., RACE_TEXT);
    let title_w = fonts.measure(&tip.title, &title.font, 0.).width;
    let detail_w = fonts.measure(&tip.detail, &detail.font, 0.).width;
    let width = title_w.max(detail_w);
    // Left-aligned under the arrow, never past the left margin.
    let left = (x - 14.).max(margin);
    let top = y + 30.;
    list.opacity(alpha);
    // A soft shade under the lettering, as the riders list's.
    list.push(Item::Box(BoxPaint {
        rect: Rect::new(left - 18., top - 12., width + 36., 56.),
        radii: [24.; 4],
        fill: Some(Fill::ellipse_at(
            [0.5, 0.5],
            &[
                (0., Color::hex("#00000066")),
                (0.7, Color::hex("#00000033")),
                (1., Color::TRANSPARENT),
            ],
        )),
        ..Default::default()
    }));
    let size = 22.;
    list.push(Item::Shape {
        shape: Shape {
            path: ARROW.into(),
            bounds: [0., 0., 20., 20.],
            unit: size / 20.,
            fill: RACE_GOLD,
            stroke: Some(Stroke {
                width: 2.5,
                color: Color::hex("#080808bb"),
                join: Join::Round,
            }),
            drop_shadow: None,
        },
        x: x - size / 2.,
        y: y + 2. + bob,
        rotation: 0.,
    });
    list.text(&tip.title, &title, left, top + 16.);
    list.text(&tip.detail, &detail, left, top + 33.);
    list.unopacity();
    hits.push((Rect::new(left - 8., y, width + 16., top + 42. - y), TIP_HIT));
}

/// A tip's opacity `age` seconds after it appeared, shown for `life`
/// seconds: 0.4 s in and out.
pub fn fade(age: f32, life: f32) -> f32 {
    (age / 0.4).min((life - age) / 0.4).clamp(0., 1.)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tips_fade_in_and_out() {
        assert_eq!(super::fade(0., 10.), 0.);
        assert_eq!(super::fade(2., 10.), 1.);
        assert_eq!(super::fade(10., 10.), 0.);
    }
}
