//! Original vehicle/rider material textures (baseTextures in web/game.js,
//! single-pass vehicle materials).
use crate::{Result, venue::Image};
use atv_core::{mesh::Mesh, resource::Resource, scenery_material, texture};
use std::collections::HashMap;

/// One material's base texture as the browser binds it.
pub struct MaterialTexture {
    /// Decoded RGBA; `None` draws the untextured surface colour.
    pub image: Option<Image>,
    /// Original pass fog flag (`pass.fog ?? true`).
    pub fog: bool,
}

pub(crate) fn basename_index(resource: &Resource, name: &str) -> Option<usize> {
    resource.members.iter().position(|member| {
        member
            .name
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|last| last.eq_ignore_ascii_case(name))
    })
}

/// Materials in first-use order of the mesh's first LOD, keyed by name.
pub fn load(mesh: &Mesh, archive: &Resource) -> Result<HashMap<String, MaterialTexture>> {
    let mut textures = HashMap::new();
    let lod = mesh.lods.first().ok_or("original mesh has no LOD")?;
    for group in &lod.groups {
        let material = mesh
            .materials
            .get(group.material as usize)
            .ok_or("mesh material index out of range")?;
        if textures.contains_key(material) {
            continue;
        }
        if let Some(texture) = material_texture(material, archive)? {
            textures.insert(material.clone(), texture);
        }
    }
    Ok(textures)
}

/// Textures for material names outside a mesh (race CPU outfits).
pub fn load_named(
    names: &[String],
    archive: &Resource,
) -> Result<HashMap<String, MaterialTexture>> {
    let mut textures = HashMap::new();
    for name in names {
        if !textures.contains_key(name)
            && let Some(texture) = material_texture(name, archive)?
        {
            textures.insert(name.clone(), texture);
        }
    }
    Ok(textures)
}

/// Outfit slot of a rider material (outfitSlot in web/game.js).
fn outfit_slot(material: &str) -> Option<&'static str> {
    let lower = material.to_ascii_lowercase();
    [
        "helmet", "glove", "goggle", "pant", "shirt", "sleeve", "boot",
    ]
    .into_iter()
    .find(|slot| lower.contains(slot))
}

/// A race CPU's rider textures: the rig's own, with each outfit slot's
/// material replaced by the roster outfit (outfitTextures in web/game.js).
pub fn outfit(
    rider: &Mesh,
    outfit: &atv_core::race_roster::Outfit,
    archive: &Resource,
) -> Result<HashMap<String, MaterialTexture>> {
    let mut textures = load(rider, archive)?;
    let mut replacements = Vec::new();
    for material in &rider.materials {
        let Some(slot) = outfit_slot(material) else {
            continue;
        };
        let name = match slot {
            "helmet" => &outfit.helmet,
            "glove" => &outfit.glove,
            "goggle" => &outfit.goggle,
            "pant" => &outfit.pant,
            "shirt" => &outfit.shirt,
            "sleeve" => &outfit.sleeve,
            _ => &outfit.boot,
        };
        if name.is_empty() {
            continue;
        }
        let name = if slot == "shirt" {
            let digits: String = material
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("{name}{}", if digits.is_empty() { "001" } else { &digits })
        } else {
            name.clone()
        };
        replacements.push((material.clone(), name));
    }
    let names: Vec<String> = replacements.iter().map(|(_, n)| n.clone()).collect();
    // Older content packages carry only the player outfit.
    let Ok(mut decoded) = load_named(&names, archive) else {
        return Ok(textures);
    };
    for (material, name) in replacements {
        if let Some(texture) = decoded.remove(&name).filter(|t| t.image.is_some()) {
            textures.insert(material, texture);
        } else if let Some(texture) = load_named(std::slice::from_ref(&name), archive)
            .ok()
            .and_then(|mut m| m.remove(&name))
            .filter(|t| t.image.is_some())
        {
            textures.insert(material, texture);
        }
    }
    Ok(textures)
}

fn material_texture(material: &str, archive: &Resource) -> Result<Option<MaterialTexture>> {
    let psm = basename_index(archive, &format!("{material}.psm"));
    let text = match psm {
        Some(index) => String::from_utf8_lossy(&archive.extract(index)?).into_owned(),
        None => String::new(),
    };
    // Without an authored PSM, 001bf430 -> 001b5080 binds the material's own stem.
    let passes = scenery_material::describe(&text, false, psm.is_none().then_some(material))?;
    let Some(described) = passes.into_iter().next() else {
        return Ok(None);
    };
    let fog = described.pass.as_ref().is_none_or(|pass| pass.fog);
    let Some(binding) = described.binding.as_deref() else {
        return Ok(None);
    };
    if described.unsupported_reflection {
        return Ok(Some(MaterialTexture { image: None, fog }));
    }
    let animated = described
        .pass
        .as_ref()
        .is_some_and(|pass| pass.texture_animation.is_some());
    let pst = basename_index(archive, &format!("{binding}.pst"));
    let atm = (animated || pst.is_none())
        .then(|| basename_index(archive, &format!("{binding}.atm")))
        .flatten();
    let tga = (pst.is_none() && atm.is_none())
        .then(|| basename_index(archive, &format!("{binding}.tga")))
        .flatten();
    let decoded = if let Some(index) = atm {
        archive
            .extract(index)
            .and_then(|bytes| texture::decode_atm(&bytes, 0).map(|(_, t)| t))
    } else if let Some(index) = tga {
        archive
            .extract(index)
            .and_then(|bytes| texture::decode_tga(&bytes))
    } else if let Some(index) = pst {
        archive
            .extract(index)
            .and_then(|bytes| texture::decode_pst(&bytes))
    } else {
        if psm.is_none() {
            return Ok(None);
        }
        return Err(format!("original material texture is missing: {binding}"));
    };
    // The browser draws a material whose image fails to decode untextured.
    let image = decoded.ok().map(|texture| Image {
        width: texture.info.width,
        height: texture.info.height,
        rgba: texture.rgba,
    });
    Ok(Some(MaterialTexture { image, fog }))
}
