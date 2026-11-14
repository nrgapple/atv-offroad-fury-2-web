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

/// MX zones retain their raw contact identity; only tire physics follows this routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceSource {
    Atv2,
    Mx { friction_zone: [u8; 16] },
}
impl SurfaceSource {
    /// Routing values are one-based source MapZoneFriction indices.
    pub fn mx(friction_zone: [u8; 16]) -> Result<Self> {
        if friction_zone.iter().any(|z| !(1..=16).contains(z)) {
            return Err("MX friction zone must be in 1..=16".into());
        }
        Ok(Self::Mx { friction_zone })
    }
    /// Grip, traction, braking and additive drag for the motorcycle adapter.
    pub fn motorcycle_multipliers(self, material: Option<TerrainMaterial>) -> (f32, f32, f32, f32) {
        match self {
            Self::Atv2 => {
                material.map_or((1., 1., 1., 0.), |m| (m.grip, m.drive, m.braking, m.drag))
            }
            Self::Mx { .. } => (1., 1., 1., 0.),
        }
    }
    pub fn tire_zone(self, surface: usize) -> usize {
        match self {
            Self::Atv2 => 0,
            Self::Mx { friction_zone } => usize::from(
                friction_zone
                    .get(surface)
                    .copied()
                    .unwrap_or(1)
                    .clamp(1, 16)
                    - 1,
            ),
        }
    }
}

/// Caller supplies original ATV2 coefficients loaded from labeled source profiles.
/// Pavement is a provisional neutral adapter, not a recovered MX ATV tire curve.
#[derive(Clone, Copy, Debug)]
pub struct AtvAdapterMaterials {
    /// ENV02 zone 1.
    pub dirt: TerrainMaterial,
    pub pavement: TerrainMaterial,
    /// ENV02 zone 4.
    pub grass: TerrainMaterial,
    /// NAT05 zone 7.
    pub sand: TerrainMaterial,
    /// ENV01 zone 3.
    pub ice: TerrainMaterial,
    /// ENV02 zone 2.
    pub shallow_mud: TerrainMaterial,
    /// ENV02 zone 6, also used for sludge.
    pub deep_mud: TerrainMaterial,
}
impl AtvAdapterMaterials {
    pub fn mx_zones(self, source: SurfaceSource) -> [TerrainMaterial; 16] {
        let base = [
            self.dirt,
            self.pavement,
            self.deep_mud, // sludge
            self.grass,
            self.sand,
            self.ice,
            self.shallow_mud,
            self.deep_mud,
            self.dirt,
            self.dirt,
            self.dirt,
            self.dirt,
            self.dirt,
            self.dirt,
            self.dirt,
            self.dirt,
        ];
        std::array::from_fn(|raw| base[source.tire_zone(raw)])
    }
}

/// Reads only MX TerrainZone metadata from otherwise arbitrary scene TOY text.
/// Missing overrides retain identity routing; no ATV2 SurfaceFriction is required.
pub fn mx_surface_source(data: &[u8]) -> Result<SurfaceSource> {
    if data.len() > 1024 * 1024 || data.contains(&0) {
        return Err("invalid MX material text".into());
    }
    let text = std::str::from_utf8(data).map_err(|_| "MX material text is not UTF-8")?;
    let mut routing = std::array::from_fn(|i| (i + 1) as u8);
    let mut active = None;
    let mut seen = [false; 16];
    let mut overrides = [false; 16];
    for raw in text.lines() {
        if raw.len() > 4096 {
            return Err("MX material line exceeds limit".into());
        }
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            active = None;
            if let Some(n) = line
                .strip_prefix("[TerrainZone")
                .and_then(|s| s.strip_suffix(']'))
            {
                let n = n.parse::<usize>().map_err(|_| "invalid MX terrain zone")?;
                if !(1..=16).contains(&n) || seen[n - 1] {
                    return Err("invalid or duplicate MX terrain zone".into());
                }
                seen[n - 1] = true;
                active = Some(n - 1);
            }
        } else if let Some(i) = active
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "MapZoneFriction"
        {
            if overrides[i] {
                return Err("duplicate MapZoneFriction".into());
            }
            overrides[i] = true;
            routing[i] = value
                .trim()
                .parse()
                .map_err(|_| "invalid MapZoneFriction")?;
        }
    }
    SurfaceSource::mx(routing)
}

#[cfg(test)]
mod mx_tests {
    use super::*;
    #[test]
    fn mx_friction_remaps_only_physics_and_keeps_sixteen_slots() {
        let source=mx_surface_source(b"[Scene]\nnot a physics assignment\n[TerrainZone9]\nMapZoneFriction=3\nIsWater=T\n[TerrainZone16]\nResetTime=5").unwrap();
        assert_eq!(source.tire_zone(8), 2);
        assert_eq!(source.tire_zone(15), 15);
        assert_eq!(SurfaceSource::Atv2.tire_zone(8), 0);
        let m = |v| TerrainMaterial {
            grip: v,
            drag: v,
            drive: v,
            powerslide: v,
            braking: v,
        };
        let labels = AtvAdapterMaterials {
            dirt: m(1.),
            pavement: m(2.),
            grass: m(3.),
            sand: m(4.),
            ice: m(5.),
            shallow_mud: m(6.),
            deep_mud: m(7.),
        };
        let zones = labels.mx_zones(source);
        assert_eq!(zones[8], m(7.)); // raw zone 9 remaps to sludge
        let identity = SurfaceSource::mx(std::array::from_fn(|i| (i + 1) as u8)).unwrap();
        assert_eq!(
            labels.mx_zones(identity).map(|m| m.grip),
            [
                1., 2., 7., 3., 4., 5., 6., 7., 1., 1., 1., 1., 1., 1., 1., 1.
            ]
        );
        assert_eq!(zones[15], m(1.));
        assert_eq!(zones[7], m(7.));
        assert_eq!(source.motorcycle_multipliers(Some(m(9.))), (1., 1., 1., 0.));
        assert_eq!(
            SurfaceSource::Atv2.motorcycle_multipliers(Some(m(9.))),
            (9., 9., 9., 9.)
        );
    }
    #[test]
    fn mx_rejects_ambiguous_or_out_of_bounds_routing() {
        for text in [
            "[TerrainZone9]\nMapZoneFriction=0",
            "[TerrainZone1]\nMapZoneFriction=17",
            "[TerrainZone17]",
            "[TerrainZone1]\n[TerrainZone1]",
            "[TerrainZone1]\nMapZoneFriction=3\nMapZoneFriction=4",
        ] {
            assert!(mx_surface_source(text.as_bytes()).is_err(), "{text}");
        }
        assert!(SurfaceSource::mx([0; 16]).is_err());
    }
}
