//! Exact-build in-race resource identities. Browser selections do not imply
//! reconstructed unlock/progression rules.
use crate::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Models {
    pub quad_id: u16,
    pub rider_id: u16,
    pub quad: String,
    pub quad_materials: String,
    pub quad_mesh: String,
    pub physics: String,
    pub chassis: String,
    pub actor: String,
    pub rider: String,
    pub rider_mesh: String,
    pub rider_catalog: String,
    pub label: String,
    pub rider_label: &'static str,
}

pub fn selection(quad: u16, rider: u16) -> Result<Models> {
    if !(1..=26).contains(&quad) || rider > 1 {
        return Err("unsupported player model".into());
    }
    let bank = if quad <= 13 { "A" } else { "B" };
    let stem = format!("ATV{quad:02}");
    let rig = if rider == 0 { "mrider" } else { "frider" };
    let labels = [
        "Ravage Talon Black",
        "Kawasaki Lakota Sport",
        "Kawasaki Mojave",
        "Honda 400EX Red",
        "Honda 300EX",
        "ATV 06 (unlisted model)",
        "Kawasaki KFX700",
        "Patriot",
        "CT Racing",
        "Trinity Racing",
        "Bombardier DS650",
        "Alba Racing",
        "Nac's Racing",
        "Honda 250EX",
        "Duncan Racing",
        "K&K Racing",
        "Curtis Sparks Racing",
        "Lonestar Racing",
        "G Ride",
        "Trenga Racing",
        "B6000",
        "Honda 400EX Yellow",
        "Ravage Talon Blue",
        "Ravage Talon Red",
        "Ravage Talon Yellow",
        "Ravage Talon Green",
    ];
    Ok(Models {
        quad_id: quad,
        rider_id: rider,
        quad: format!("/QUAD/BIKES{bank}/{stem}.RES"),
        quad_materials: format!("/QUAD/BIKES{bank}/{stem}_MT.RES"),
        quad_mesh: format!("{stem}.psg"),
        physics: format!("{stem}s01.phy"),
        chassis: format!("{stem}ao.col"),
        actor: format!("{stem}ar.col"),
        rider: format!("/RIDER/{}.RES", rig.to_ascii_uppercase()),
        rider_mesh: format!("{rig}.psg"),
        rider_catalog: format!("{rig}.mcf"),
        label: labels[quad as usize - 1].into(),
        rider_label: if rider == 0 {
            "Male rider"
        } else {
            "Female rider"
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_boundary_and_ids() {
        assert_eq!(selection(13, 0).unwrap().quad, "/QUAD/BIKESA/ATV13.RES");
        assert_eq!(selection(14, 1).unwrap().quad, "/QUAD/BIKESB/ATV14.RES");
        assert_eq!(selection(26, 1).unwrap().rider_catalog, "frider.mcf");
        assert!(selection(0, 0).is_err());
        assert!(selection(27, 0).is_err());
        assert!(selection(1, 2).is_err());
    }
}
