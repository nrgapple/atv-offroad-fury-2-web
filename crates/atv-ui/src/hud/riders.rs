//! Who rides a shared Free Ride room, at a glance (research/FREERIDE_HUB.md
//! "Riders list"): the room's count, then the nearest few riders in their
//! colours with their distance. Authored port presentation; the PS2 game
//! had no shared world. Bare race-HUD lettering right-aligned under the
//! map, no panel or plates, so it reads as part of the HUD
//! (research/UI_AUDIT_HUD.md). A tap on the count folds the names away.
use super::race::{RACE_GOLD, RACE_TEXT};
use super::{classic, peers};
use crate::{
    color::{Color, Fill},
    paint::{BoxPaint, DisplayList, Item, Rect},
    text::Fonts,
};

/// The count line: folds or unfolds the names.
pub const HEADER_HIT: u32 = 330;
/// Names listed before "+N MORE".
pub const MAX_ROWS: usize = 5;
/// The count line's touch target height.
const HEADER: f32 = 44.;
const ROW: f32 = 20.;

/// Another rider in the room.
#[derive(Clone, Debug, PartialEq)]
pub struct Rider {
    pub slot: u8,
    pub paint: u8,
    pub name: String,
    /// Horizontal distance (ft), when the rider is presented.
    pub feet: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// Everyone in the room, the player included.
    pub count: u32,
    /// The others.
    pub riders: Vec<Rider>,
    /// Only the count shows.
    pub folded: bool,
}

/// Nearest first; riders without a pose after, in slot order. Distances
/// sort in 50 ft steps so two riders side by side do not swap every frame.
pub fn sort(riders: &mut [Rider]) {
    riders.sort_by_key(|r| {
        (
            r.feet.map_or(u32::MAX, |f| (f.max(0.) / 50.) as u32),
            r.slot,
        )
    });
}

/// "120 FT", or "0.4 MI" from a quarter mile.
pub fn distance(feet: f32) -> String {
    if feet >= 1320. {
        format!("{:.1} MI", feet / 5280.)
    } else {
        format!("{} FT", (feet / 10.).round() as u32 * 10)
    }
}

/// Paints the list with its right edge at `right` from `top`; returns its
/// bottom edge.
pub fn paint(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    hits: &mut Vec<(Rect, u32)>,
    right: f32,
    top: f32,
    hovered: bool,
    view: &View,
) -> f32 {
    // The count: a big gold number and the silver word, as the race HUD
    // pairs its values and labels.
    let number = view.count.to_string();
    let word = if view.folded { "RIDING ›" } else { "RIDING" };
    let big = classic::hud_style(24., RACE_GOLD);
    let small = classic::hud_style(13., if hovered { RACE_GOLD } else { RACE_TEXT });
    let word_w = fonts.measure(word, &small.font, 0.).width;
    let number_w = fonts.measure(&number, &big.font, 0.).width;
    let header_w = number_w + 6. + word_w;
    let baseline = top + 26.;
    // A soft shade behind the lettering, as the chat wheel's: it keeps the
    // names readable over snow without drawing a box.
    let rows = match view.riders.len() {
        _ if view.folded => 0,
        // "JUST YOU".
        0 => 1,
        n if n > MAX_ROWS => MAX_ROWS + 1,
        n => n,
    };
    let height = 32. + rows as f32 * ROW;
    let shade_w = 190f32;
    list.push(Item::Box(BoxPaint {
        rect: Rect::new(right - shade_w, top - 6., shade_w + 14., height + 14.),
        radii: [18.; 4],
        fill: Some(Fill::ellipse_at(
            [0.75, 0.4],
            // Clear by 0.68: the box's edges sit at 0.707 of a
            // farthest-corner ellipse, so no edge shows.
            &[
                (0., Color::hex("#0000004d")),
                (0.4, Color::hex("#00000029")),
                (0.68, Color::TRANSPARENT),
            ],
        )),
        ..Default::default()
    }));
    list.text(&number, &big, right - header_w, baseline);
    list.text(word, &small, right - word_w, baseline);
    let hit_w = header_w.max(96.);
    hits.push((
        Rect::new(right - hit_w, top + 14. - HEADER / 2., hit_w, HEADER),
        HEADER_HIT,
    ));
    let mut y = baseline + ROW;
    if view.folded {
        return baseline + 6.;
    }
    if view.riders.is_empty() {
        let style = classic::hud_style(14., RACE_TEXT);
        let text = "JUST YOU";
        let w = fonts.measure(text, &style.font, 0.).width;
        list.text(text, &style, right - w, y);
        return y + 6.;
    }
    for rider in view.riders.iter().take(MAX_ROWS) {
        let mut x = right;
        if let Some(feet) = rider.feet {
            let style = classic::hud_style(12., RACE_GOLD);
            let text = distance(feet);
            x -= fonts.measure(&text, &style.font, 0.).width;
            list.text(&text, &style, x, y);
            x -= 6.;
        }
        // The rider's colour (their paint, else their slot's), outlined
        // light when it is dark, as name tags are.
        let fill = peers::rider_color(rider.slot, rider.paint);
        let mut style = classic::hud_style(15., fill);
        style.stroke = style.stroke.map(|mut s| {
            s.color = peers::outline(fill, s.color);
            s
        });
        let name = clip(fonts, &rider.name, &style, 120.);
        x -= fonts.measure(&name, &style.font, 0.).width;
        list.text(&name, &style, x, y);
        y += ROW;
    }
    let more = view.riders.len().saturating_sub(MAX_ROWS);
    if more > 0 {
        let style = classic::hud_style(12., RACE_TEXT);
        let text = format!("+{more} MORE");
        let w = fonts.measure(&text, &style.font, 0.).width;
        list.text(&text, &style, right - w, y);
        y += ROW;
    }
    y - ROW + 6.
}

/// The name cut to `width` with an ellipsis.
fn clip(fonts: &mut Fonts, name: &str, style: &crate::text::TextStyle, width: f32) -> String {
    if fonts.measure(name, &style.font, 0.).width <= width {
        return name.to_string();
    }
    let mut chars: Vec<char> = name.chars().collect();
    while chars.len() > 1 {
        chars.pop();
        let cut = format!("{}…", chars.iter().collect::<String>());
        if fonts.measure(&cut, &style.font, 0.).width <= width {
            return cut;
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rider(slot: u8, feet: Option<f32>) -> Rider {
        Rider {
            slot,
            paint: 0,
            name: format!("R{slot}"),
            feet,
        }
    }

    #[test]
    fn nearest_first_in_steady_steps() {
        let mut riders = vec![
            rider(4, None),
            rider(3, Some(610.)),
            rider(2, Some(130.)),
            rider(1, Some(140.)),
        ];
        sort(&mut riders);
        let order: Vec<u8> = riders.iter().map(|r| r.slot).collect();
        // 130 and 140 ft share a step: slot order keeps them still.
        assert_eq!(order, [1, 2, 3, 4]);
    }

    #[test]
    fn distances_read_in_feet_then_miles() {
        assert_eq!(distance(84.), "80 FT");
        assert_eq!(distance(1200.), "1200 FT");
        assert_eq!(distance(2640.), "0.5 MI");
    }
}
