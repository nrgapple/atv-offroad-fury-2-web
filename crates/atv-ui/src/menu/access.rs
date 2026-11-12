//! Options → Accessibility (authored; the original has no such page): the
//! rider's field of view, HUD size and colour assistance. Every value
//! defaults to the original behaviour and is kept like the other
//! preferences, one settings key each.
use crate::{hud::palette, settings::Settings};

pub const HUD_SIZES: [&str; 3] = ["small", "normal", "large"];
pub const FOV_KEY: &str = "atv-fov";
pub const HUD_KEY: &str = "atv-hud-size";
pub const COLOUR_KEY: &str = "atv-colour-assist";
/// Field of view limits and step, percent of the original angle.
pub const FOV: (f32, f32, f32) = (85., 125., 5.);

#[derive(Clone, Debug, PartialEq)]
pub struct Access {
    /// Percent of the original field of view (85-125; 100 original).
    pub fov: u8,
    pub hud: String,
    pub colour: String,
}

impl Default for Access {
    fn default() -> Self {
        Self {
            fov: 100,
            hud: "normal".into(),
            colour: "off".into(),
        }
    }
}

fn one_of(value: Option<String>, list: &[&str], default: &str) -> String {
    value
        .filter(|v| list.contains(&v.as_str()))
        .unwrap_or_else(|| default.into())
}

fn cycle<'a>(list: &[&'a str], current: &str, direction: i32) -> &'a str {
    let at = list.iter().position(|v| *v == current).unwrap_or(0) as i32;
    let n = list.len() as i32;
    list[((at + if direction < 0 { -1 } else { 1 } + n) % n) as usize]
}

fn title(value: &str) -> String {
    let mut out = value.to_string();
    if let Some(first) = out.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    out
}

impl Access {
    pub fn load(settings: &Settings) -> Self {
        let get = |k: &str| settings.get(k);
        Self {
            fov: get(FOV_KEY)
                .and_then(|v| v.parse::<f32>().ok())
                .map_or(100, snap_fov),
            hud: one_of(get(HUD_KEY), &HUD_SIZES, "normal"),
            colour: one_of(get(COLOUR_KEY), &palette::MODES, "off"),
        }
    }

    /// The HUD's size against the original layout.
    pub fn hud_scale(&self) -> f32 {
        match self.hud.as_str() {
            "small" => 0.85,
            "large" => 1.25,
            _ => 1.,
        }
    }

    pub fn colour(&self) -> palette::Assist {
        palette::Assist::from_name(&self.colour)
    }

    /// The field of view slider (percent).
    pub fn set_fov(&mut self, percent: f32, settings: &mut Settings) {
        self.fov = snap_fov(percent);
        settings.set(FOV_KEY, &self.fov.to_string());
    }

    /// The page's rows: (id, label, value). The menu shows "label: value"
    /// and answers a press with `access:<id>`.
    pub fn rows(&self) -> Vec<(&'static str, &'static str, String)> {
        vec![
            ("hud", "HUD size", title(&self.hud)),
            ("colour", "Colour assist", self.colour().label().into()),
        ]
    }

    /// One `access:<id>` press; false when the id is not a row.
    pub fn apply(&mut self, id: &str, direction: i32, settings: &mut Settings) -> bool {
        match id {
            "hud" => {
                self.hud = cycle(&HUD_SIZES, &self.hud, direction).into();
                settings.set(HUD_KEY, &self.hud);
            }
            "colour" => {
                self.colour = cycle(&palette::MODES, &self.colour, direction).into();
                settings.set(COLOUR_KEY, &self.colour);
            }
            _ => return false,
        }
        true
    }
}

fn snap_fov(percent: f32) -> u8 {
    let (min, max, step) = FOV;
    if !percent.is_finite() {
        return 100;
    }
    (((percent.clamp(min, max) - min) / step).round() * step + min) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_defaults_to_the_original() {
        let access = Access::load(&Settings::memory());
        assert_eq!(access, Access::default());
        assert_eq!(access.fov, 100);
        assert_eq!(access.hud_scale(), 1.);
        assert_eq!(access.colour(), palette::Assist::Off);
    }

    #[test]
    fn choices_persist_and_bad_values_fall_back() {
        let mut settings = Settings::memory();
        let mut access = Access::load(&settings);
        access.set_fov(112.4, &mut settings);
        assert_eq!(access.fov, 110);
        access.set_fov(900., &mut settings);
        assert_eq!(access.fov, 125);
        assert!(access.apply("hud", 1, &mut settings));
        assert_eq!((access.hud.as_str(), access.hud_scale()), ("large", 1.25));
        assert!(access.apply("hud", 1, &mut settings));
        assert_eq!((access.hud.as_str(), access.hud_scale()), ("small", 0.85));
        assert!(access.apply("colour", -1, &mut settings));
        assert_eq!(access.colour(), palette::Assist::Tritanopia);
        assert!(!access.apply("nonsense", 1, &mut settings));
        assert_eq!(Access::load(&settings), access);
        let rows = access.rows();
        assert_eq!(rows[0], ("hud", "HUD size", "Small".to_string()));
        assert_eq!(rows[1].2, "Tritanopia");

        settings.set(FOV_KEY, "abc");
        settings.set(HUD_KEY, "huge");
        settings.set(COLOUR_KEY, "sepia");
        assert_eq!(Access::load(&settings), Access::default());
    }
}
