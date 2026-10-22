//! The player's rider gear and quad paint (the Garage appearance): the
//! original Profile Store gear catalog (`RIDEINFO.RES` uiRider.txt, op 107)
//! dresses the ATV rider (gearTextures in web/game.js) and the authored paint
//! recolours the quad's body materials (render-support.js PAINT_PALETTE).
//! Motorcycles keep their original livery and rider (`rideAppearance` is null
//! for them in the browser).
use super::textures::{self, MaterialTexture};
use crate::Result;
use atv_content::Content;
pub use atv_core::rider_gear::{Appearance, Category, PAINTS, SLOTS};
use atv_core::{mesh::Mesh, race_roster::Outfit, resource::Resource};
use std::collections::HashMap;

/// Authored paint per `PAINTS` index: mode (0 original, 1 hue, 2 white,
/// 3 black) and hue (render-support.js PAINT_PALETTE).
pub const PAINT_PALETTE: [[f32; 2]; 11] = [
    [0., 0.],
    [1., 0.],
    [1., 0.075],
    [1., 0.15],
    [1., 0.33],
    [1., 0.48],
    [1., 0.62],
    [1., 0.76],
    [1., 0.9],
    [2., 0.],
    [3., 0.],
];

/// Quad body materials the paint applies to (`paintable`: `s<n>m<n>`).
pub fn paintable(material: &str) -> bool {
    let lower = material.to_ascii_lowercase();
    let Some(m) = lower.rfind('m') else {
        return false;
    };
    let (head, digits) = (&lower[..m], &lower[m + 1..]);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let s_digits = head.len() - head.bytes().rev().take_while(u8::is_ascii_digit).count();
    s_digits < head.len() && head[..s_digits].ends_with('s')
}

/// normalizedAppearance (web/frontend.js) of a saved `atv-player-selection`
/// value's `appearance`: seven gear indices 0-64 and a paint index, else
/// the original look.
pub fn saved_appearance(selection: &serde_json::Value) -> Appearance {
    let value = &selection["appearance"];
    let gear = value["gear"]
        .as_array()
        .filter(|g| g.len() == 7)
        .and_then(|g| {
            let mut out = [0u8; 7];
            for (slot, v) in out.iter_mut().zip(g) {
                *slot = u8::try_from(v.as_u64().filter(|v| *v <= 64)?).ok()?;
            }
            Some(out)
        })
        .unwrap_or_default();
    let paint = value["paint"]
        .as_u64()
        .filter(|p| (*p as usize) < PAINTS.len())
        .unwrap_or(0) as u8;
    Appearance { gear, paint }
}

/// The original gear catalog; None for content packages without it.
pub async fn catalog(content: &Content) -> Result<Option<Vec<Category>>> {
    let Ok(bytes) = content.file("/UI/RESOURCE/RIDEINFO.RES").await else {
        return Ok(None);
    };
    let archive = Resource::parse(&bytes)?;
    let Some(index) = archive
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with("uirider.txt"))
    else {
        return Ok(None);
    };
    atv_core::rider_gear::parse(&archive.extract(index)?).map(Some)
}

/// gearTextures: the rider's materials with each chosen item's in-game
/// material (the first candidate RIDER_MT.RES holds; shirts by their base so
/// the mesh slot suffix applies).
pub fn rider_textures(
    rider: &Mesh,
    appearance: &Appearance,
    catalog: &[Category],
    materials: &Resource,
) -> Result<HashMap<String, MaterialTexture>> {
    textures::outfit(rider, &outfit(appearance, catalog, materials), materials)
}

/// gearTextures for a menu preview: only the gear slots' textures, by the
/// given rider mesh's material names.
pub fn gear_textures(
    rider: &Mesh,
    appearance: &Appearance,
    catalog: &[Category],
    materials: &Resource,
) -> HashMap<String, MaterialTexture> {
    textures::outfit_textures(rider, &outfit(appearance, catalog, materials), materials)
}

/// Each chosen item's in-game material (the first candidate the archive holds).
fn outfit(appearance: &Appearance, catalog: &[Category], materials: &Resource) -> Outfit {
    let names: std::collections::HashSet<String> = materials
        .members
        .iter()
        .map(|m| {
            let last = m.name.rsplit(['/', '\\']).next().unwrap_or(&m.name);
            let lower = last.to_ascii_lowercase();
            lower
                .strip_suffix(".psm")
                .or_else(|| lower.strip_suffix(".pst"))
                .unwrap_or(&lower)
                .to_string()
        })
        .collect();
    let mut outfit = Outfit::default();
    for (at, slot) in SLOTS.iter().enumerate() {
        let index = usize::from(appearance.gear[at]);
        let Some(item) = index
            .checked_sub(1)
            .and_then(|i| catalog.get(at)?.items.get(i))
        else {
            continue;
        };
        let Some(found) = item.materials.iter().find(|m| names.contains(*m)) else {
            continue;
        };
        let value = if *slot == "shirt" {
            found.strip_suffix("001").unwrap_or(found).to_string()
        } else {
            found.clone()
        };
        match *slot {
            "helmet" => outfit.helmet = value,
            "glove" => outfit.glove = value,
            "boot" => outfit.boot = value,
            "shirt" => outfit.shirt = value,
            "sleeve" => outfit.sleeve = value,
            "pant" => outfit.pant = value,
            _ => outfit.goggle = value,
        }
    }
    outfit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paintable_matches_the_browser_pattern() {
        for (name, expected) in [
            ("bike01s1m1", true),
            ("BODYS12M3", true),
            ("quads1m", false),
            ("quadm1", false),
            ("tires1", false),
            ("s1m1x", false),
        ] {
            assert_eq!(paintable(name), expected, "{name}");
        }
    }

    #[test]
    fn saved_appearance_is_normalized_like_the_frontend() {
        let saved = serde_json::json!({"appearance": {"gear": [1, 2, 0, 3, 0, 0, 64], "paint": 4}});
        assert_eq!(
            saved_appearance(&saved),
            Appearance {
                gear: [1, 2, 0, 3, 0, 0, 64],
                paint: 4
            }
        );
        let bad = serde_json::json!({"appearance": {"gear": [1, 2, 99, 0, 0, 0, 0], "paint": 11}});
        assert_eq!(saved_appearance(&bad), Appearance::default());
    }
}
