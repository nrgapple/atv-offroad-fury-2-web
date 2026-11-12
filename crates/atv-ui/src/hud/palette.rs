//! Colour assistance (authored; the original has none): the colours that
//! carry meaning on the HUD swap to sets that stay apart for riders who
//! cannot tell red from green (protanopia, deuteranopia) or blue from
//! yellow (tritanopia). Off keeps every original colour.
//!
//! The mode is one process-wide setting, like the display it describes:
//! every view on a shared screen reads the same choice. Tinted artwork
//! (radar racers, hill pills) is coloured when an event loads, so a change
//! made mid-event reaches those at the next load; tags, map dots and the
//! split-time colours follow at once.
use crate::color::Color;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Assist {
    #[default]
    Off,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

/// The Options values, in menu order.
pub const MODES: [&str; 4] = ["off", "protanopia", "deuteranopia", "tritanopia"];

impl Assist {
    pub fn from_name(name: &str) -> Self {
        match name {
            "protanopia" => Self::Protanopia,
            "deuteranopia" => Self::Deuteranopia,
            "tritanopia" => Self::Tritanopia,
            _ => Self::Off,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Protanopia => "Protanopia",
            Self::Deuteranopia => "Deuteranopia",
            Self::Tritanopia => "Tritanopia",
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn set(mode: Assist) {
    MODE.store(mode as u8, Ordering::Relaxed);
}

pub fn mode() -> Assist {
    match MODE.load(Ordering::Relaxed) {
        1 => Assist::Protanopia,
        2 => Assist::Deuteranopia,
        3 => Assist::Tritanopia,
        _ => Assist::Off,
    }
}

/// Player slots 0-3 without red against green: a dark orange, a deep blue,
/// a light blue and a light yellow differ in both blue-yellow hue and
/// lightness (the Okabe-Ito set).
const RED_GREEN: [Color; 4] = [
    Color::rgb(213, 94, 0),
    Color::rgb(0, 114, 178),
    Color::rgb(86, 180, 233),
    Color::rgb(240, 228, 66),
];
/// Player slots 0-3 without blue against yellow: red, a dark teal, a
/// bright green and amber differ in red-green hue and lightness.
const BLUE_YELLOW: [Color; 4] = [
    Color::rgb(238, 51, 51),
    Color::rgb(0, 85, 102),
    Color::rgb(51, 221, 170),
    Color::rgb(255, 176, 0),
];

/// The assisted colour of a player slot; `None` keeps the slot's own.
pub fn slot_in(mode: Assist, slot: u8) -> Option<Color> {
    let set = match mode {
        Assist::Off => return None,
        Assist::Protanopia | Assist::Deuteranopia => &RED_GREEN,
        Assist::Tritanopia => &BLUE_YELLOW,
    };
    set.get(usize::from(slot)).copied()
}

pub fn slot(slot: u8) -> Option<Color> {
    slot_in(mode(), slot)
}

/// Slot colour as 0-1 RGB for tinting 3D markers (the hill
/// flags), `original` when no assistance applies.
pub fn slot_rgb(slot: u8, original: [f32; 3]) -> [f32; 3] {
    self::slot(slot).map_or(original, |c| [c.r, c.g, c.b])
}

/// The split-time colours: ahead (original green) and behind (original
/// red). Red-green assistance shows them light blue and orange.
pub fn lead_in(mode: Assist, ahead: bool) -> Color {
    match (mode, ahead) {
        (Assist::Protanopia | Assist::Deuteranopia, true) => Color::rgb(86, 180, 233),
        (Assist::Protanopia | Assist::Deuteranopia, false) => Color::rgb(230, 159, 0),
        (_, true) => Color::rgb(30, 215, 30),
        (_, false) => Color::rgb(200, 60, 60),
    }
}

pub fn lead(ahead: bool) -> Color {
    lead_in(mode(), ahead)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lightness as the eye weighs it (Rec. 709).
    fn luma(c: Color) -> f32 {
        0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
    }

    #[test]
    fn off_keeps_the_original_colours() {
        for slot in 0..20 {
            assert_eq!(slot_in(Assist::Off, slot), None);
        }
        assert_eq!(lead_in(Assist::Off, true), Color::rgb(30, 215, 30));
        assert_eq!(lead_in(Assist::Off, false), Color::rgb(200, 60, 60));
    }

    #[test]
    fn red_green_slots_differ_without_the_red_green_axis() {
        // With red and green merged, what is left is blue against yellow
        // and lightness: every pair must differ clearly in one of them.
        for mode in [Assist::Protanopia, Assist::Deuteranopia] {
            let set: Vec<Color> = (0..4).map(|s| slot_in(mode, s).unwrap()).collect();
            for a in 0..4 {
                for b in a + 1..4 {
                    let blue_yellow = ((set[a].r + set[a].g) / 2. - set[a].b)
                        - ((set[b].r + set[b].g) / 2. - set[b].b);
                    let light = luma(set[a]) - luma(set[b]);
                    assert!(
                        blue_yellow.abs() > 0.3 || light.abs() > 0.25,
                        "{mode:?} slots {a} and {b} look alike"
                    );
                }
            }
            assert_ne!(lead_in(mode, true), lead_in(Assist::Off, true));
            assert!(
                slot_in(mode, 4).is_none(),
                "authored slots keep their colours"
            );
        }
    }

    #[test]
    fn blue_yellow_slots_differ_without_the_blue_axis() {
        let set: Vec<Color> = (0..4)
            .map(|s| slot_in(Assist::Tritanopia, s).unwrap())
            .collect();
        for a in 0..4 {
            for b in a + 1..4 {
                let red_green = (set[a].r - set[a].g) - (set[b].r - set[b].g);
                let light = luma(set[a]) - luma(set[b]);
                assert!(
                    red_green.abs() > 0.3 || light.abs() > 0.25,
                    "slots {a} and {b} look alike"
                );
            }
        }
    }

    #[test]
    fn names_round_trip() {
        for name in MODES {
            assert_eq!(Assist::from_name(name).label().to_ascii_lowercase(), name);
        }
        assert_eq!(Assist::from_name("nonsense"), Assist::Off);
    }
}
