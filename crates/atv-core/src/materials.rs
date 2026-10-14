//! Terrain material coefficients read from original scene .toy text.
//! Original values are supplied at runtime, never embedded in this module.
use crate::{Result, physics_data::PhysicsData};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainMaterial {
    /// Original 0x20-byte record +4.
    pub grip: f32,
    /// Record +8.
    pub drag: f32,
    /// Record +0xc.
    pub drive: f32,
    /// Record +0x10.
    pub powerslide: f32,
    /// Record +0x14.
    pub braking: f32,
}

/// Reads the eight terrain slots used by the vehicle interface. Original
/// 002028f0/00202770 initialize every slot before 00202950 looks up sections;
/// absent sections retain grip/drive/powerslide/braking=1 and drag=0.
/// Source found-section count is not yet exposed to the vehicle interface.
/// Returned index zero corresponds to TerrainZone1.
pub fn terrain_materials(data: &[u8]) -> Result<[TerrainMaterial; 8]> {
    if data.len() > 1024 * 1024 || data.contains(&0) {
        return Err("invalid or oversized terrain material text".into());
    }
    let text = std::str::from_utf8(data).map_err(|_| "terrain material text is not UTF-8")?;
    let mut selected = String::new();
    let mut active = false;
    let mut seen = [false; 8];
    for line in text.lines() {
        if line.len() > 4096 {
            return Err("terrain material line exceeds limit".into());
        }
        let line = line.trim();
        if line.starts_with('[') {
            active = false;
            if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if matches!(section, "TerrainZone9" | "TerrainZone10") {
                    return Err(
                        "material table exceeds the current eight-zone vehicle interface".into(),
                    );
                }
                for (i, found) in seen.iter_mut().enumerate() {
                    if section == format!("TerrainZone{}", i + 1) {
                        if *found {
                            return Err("duplicate terrain zone section".into());
                        }
                        *found = true;
                        active = true;
                    }
                }
            }
        }
        if active {
            selected.push_str(line);
        }
        selected.push('\n');
    }
    if !seen.iter().any(|s| *s) {
        return Err("requires at least one authored terrain zone section".into());
    }
    let text = PhysicsData::parse(selected.as_bytes())?;
    let mut materials = Vec::with_capacity(8);
    for index in 1..=8 {
        let section = format!("TerrainZone{index}");
        if !seen[index - 1] {
            materials.push(TerrainMaterial {
                grip: 1.,
                drag: 0.,
                drive: 1.,
                powerslide: 1.,
                braking: 1.,
            });
            continue;
        }
        materials.push(TerrainMaterial {
            grip: text.first_scalar(&section, "SurfaceFriction")?,
            drag: text.first_scalar(&section, "SurfaceDrag")?,
            drive: text.first_scalar(&section, "SurfaceTraction")?,
            powerslide: text.first_scalar(&section, "PowerslideFrictionScale")?,
            braking: text.first_scalar(&section, "SurfaceBraking")?,
        });
    }
    materials
        .try_into()
        .map_err(|_| "terrain material count mismatch".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> String {
        let mut text = String::from("[Other]\nuninterpreted toy text\n");
        for i in 1..=8 {
            text.push_str(&format!("[TerrainZone{i}]\nSurfaceFriction=0.6\nSurfaceDrag=0.02\nSurfaceTraction=0.8\nPowerslideFrictionScale=0.4\nSurfaceBraking=0.7\n"));
        }
        text
    }
    #[test]
    fn isolates_material_sections_and_preserves_authored_coefficients() {
        let materials = terrain_materials(fixture().as_bytes()).unwrap();
        assert_eq!(materials.len(), 8);
        assert_eq!(
            materials[7],
            TerrainMaterial {
                grip: 0.6,
                drag: 0.02,
                drive: 0.8,
                powerslide: 0.4,
                braking: 0.7
            }
        );
    }
    #[test]
    fn absent_slots_keep_native_constructor_defaults_without_compacting_later_zones() {
        let source = fixture().replace("[TerrainZone2]", "[TerrainZone2\nGenerateTireTrack=T]");
        let materials = terrain_materials(source.as_bytes()).unwrap();
        assert_eq!(
            materials[1],
            TerrainMaterial {
                grip: 1.,
                drag: 0.,
                drive: 1.,
                powerslide: 1.,
                braking: 1.
            }
        );
        assert_eq!(materials[2].grip, 0.6);
        assert_eq!(materials[7].drag, 0.02);
        assert!(terrain_materials(b"[Other]\nSurfaceFriction=1").is_err());
    }
    #[test]
    fn nonfinite_and_ambiguous_tables_fail() {
        assert!(
            terrain_materials(
                fixture()
                    .replace("SurfaceTraction=0.8", "SurfaceTraction=NaN")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            terrain_materials((fixture() + "[TerrainZone1]\nSurfaceDrag=9\n").as_bytes()).is_err()
        );
    }
}
