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

/// Reads all eight authored TerrainZone sections. Unrelated toy directives are
/// ignored; incomplete material tables fail instead of receiving invented defaults.
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
    if seen.iter().any(|s| !s) {
        return Err("requires all eight authored terrain zone sections".into());
    }
    let text = PhysicsData::parse(selected.as_bytes())?;
    let mut materials = Vec::with_capacity(8);
    for index in 1..=8 {
        let section = format!("TerrainZone{index}");
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
    fn incomplete_and_ambiguous_tables_fail() {
        assert!(
            terrain_materials(fixture().replace("[TerrainZone8]", "[Other]").as_bytes()).is_err()
        );
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
