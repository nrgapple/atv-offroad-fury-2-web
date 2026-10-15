//! Rider gear catalog from the original Profile Store (`ui/frontend/uiRider.txt`
//! in `RIDEINFO.RES`) and the player appearance shared in multiplayer.
//!
//! The catalog lists Helmets, Gloves, Boots, Shirts, Sleeves, Pants and Goggles,
//! each item with a display name, a `_ui` preview texture, store access, brand
//! logo and points price (read by 0021db50 for the store pages). In-game rider
//! materials share the item stem without `_ui`; shirts use their rider-mesh slot
//! variant (`<stem>001`) and boot previews name the in-game `_boots` material.
//! Sleeves pair with the shirt at the same index, as the Jersey page sells them.
//! The port has no store progression, so every item is selectable.
use crate::{Result, ui::Document};
use serde::Serialize;

/// Appearance gear slots, in protocol order.
pub const SLOTS: [&str; 7] = [
    "helmet", "glove", "boot", "shirt", "sleeve", "pant", "goggle",
];
const SECTIONS: [&str; 7] = [
    "Helmets", "Gloves", "Boots", "Shirts", "Sleeves", "Pants", "Goggles",
];
/// Authored quad paint options: 0 keeps the original livery.
pub const PAINTS: [&str; 11] = [
    "Original", "Red", "Orange", "Yellow", "Green", "Teal", "Blue", "Purple", "Pink", "White",
    "Black",
];

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Item {
    pub name: String,
    /// In-game material candidates in priority order.
    pub materials: Vec<String>,
    pub locked: bool,
    pub points: u32,
    pub logo: String,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Category {
    pub slot: &'static str,
    pub items: Vec<Item>,
}

fn stem(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Category>> {
    let doc = Document::parse(bytes)?;
    let mut categories = Vec::with_capacity(SLOTS.len());
    for (slot, section) in SLOTS.iter().zip(SECTIONS) {
        let count: usize = doc
            .value(section, "NumItems")
            .and_then(|v| v.trim().parse().ok())
            .filter(|n| (1..=64).contains(n))
            .ok_or_else(|| format!("rider gear catalog is missing {section}"))?;
        let mut items = Vec::with_capacity(count);
        for i in 1..=count {
            let texture = doc
                .value(section, &format!("Texture_{i}"))
                .ok_or_else(|| format!("{section} item {i} has no texture"))?;
            let base = stem(texture);
            let base = base.strip_suffix("_ui").unwrap_or(&base).to_string();
            let mut materials = vec![base.clone()];
            if *slot == "shirt" {
                materials.insert(0, format!("{base}001"));
            }
            if *slot == "boot" && base.ends_with("_boot") {
                materials.insert(0, format!("{base}s"));
            }
            items.push(Item {
                name: doc
                    .value(section, &format!("Name_{i}"))
                    .unwrap_or_default()
                    .trim()
                    .trim_matches('"')
                    .to_string(),
                materials,
                locked: doc.value(section, &format!("Access_{i}")).map(str::trim) == Some("0"),
                points: doc
                    .value(section, &format!("Points_{i}"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0),
                logo: stem(
                    doc.value(section, &format!("Image_{i}"))
                        .unwrap_or_default(),
                ),
            });
        }
        categories.push(Category { slot, items });
    }
    Ok(categories)
}

/// Player appearance: per-slot catalog index (0 = the rider's original
/// material, n = item n) and a quad paint index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Appearance {
    pub gear: [u8; 7],
    pub paint: u8,
}
impl Appearance {
    pub fn valid(&self) -> bool {
        self.gear.iter().all(|g| *g <= 64) && usize::from(self.paint) < PAINTS.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> Vec<u8> {
        let mut text = String::new();
        for (section, texture) in [
            ("Helmets", "Thor_Black_helmet_ui"),
            ("Gloves", "thor_black_gloves_ui"),
            ("Boots", "alpinestars_red_boot_ui"),
            ("Shirts", "azonic_shirt02_ui"),
            ("Sleeves", "thor_black_sleeves_ui"),
            ("Pants", "thor_black_pants_ui"),
            ("Goggles", "scott_yellow_goggles_ui"),
        ] {
            text += &format!(
                "[{section}]\nNumItems = 2\nName_1 = \"Thor Black\"\nTexture_1 = ui\\frontend\\textures\\{texture}\nAccess_1 = 1\nImage_1 = \"logo_thor\"\nPoints_1 = 50\nName_2 = Two\nTexture_2 = x_ui\nAccess_2 = 0\n"
            );
        }
        text.into_bytes()
    }
    #[test]
    fn catalog_resolves_in_game_materials_for_every_slot() {
        let c = parse(&catalog()).unwrap();
        assert_eq!(c.iter().map(|c| c.slot).collect::<Vec<_>>(), SLOTS);
        assert_eq!(c[0].items[0].materials, ["thor_black_helmet"]);
        assert_eq!(c[0].items[0].name, "Thor Black");
        assert_eq!(
            c[2].items[0].materials,
            ["alpinestars_red_boots", "alpinestars_red_boot"]
        );
        assert_eq!(
            c[3].items[0].materials,
            ["azonic_shirt02001", "azonic_shirt02"]
        );
        assert!(!c[0].items[0].locked && c[0].items[1].locked);
        assert_eq!(
            (c[0].items[0].points, c[0].items[0].logo.as_str()),
            (50, "logo_thor")
        );
        assert!(parse(b"[Helmets]\nNumItems=1\n").is_err());
        assert!(Appearance::default().valid());
        assert!(
            !Appearance {
                gear: [0; 7],
                paint: 11
            }
            .valid()
        );
    }
}
