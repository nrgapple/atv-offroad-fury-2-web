//! Original vehicle/rider material textures (baseTextures in web/game.js,
//! single-pass vehicle materials).
use crate::{Result, venue::Image};
use atv_core::{mesh::Mesh, resource::Resource, scenery_material, texture};
use std::collections::HashMap;

/// One material's base texture as the browser binds it.
#[derive(Clone)]
pub struct MaterialTexture {
    /// Decoded RGBA; `None` draws the untextured surface colour.
    pub image: Option<Image>,
    /// Original pass fog flag (`pass.fog ?? true`).
    pub fog: bool,
    /// The MX MudAnimator map or the ATV2 PixelAnimation dirt pass texture
    /// (`image.mud` in baseTextures), blended by the mud amount.
    pub mud: Option<Image>,
    /// The base pass enables GS source-alpha blending (`alphaBlendingOnOff`;
    /// every surveyed vehicle and rider PSM does): texels below full GS
    /// alpha (128) show what lies behind, e.g. the bike sprockets' cut-outs.
    pub blend: bool,
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

/// [`load`] a material at a time, yielding to the frame loop between them
/// (other riders' models load during a ride).
pub async fn load_spread(
    mesh: &Mesh,
    archive: &Resource<'_>,
) -> Result<HashMap<String, MaterialTexture>> {
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
        crate::yield_frame().await;
    }
    Ok(textures)
}

/// [`load`] with the PST decodes run as jobs (`crate::offload`): the same
/// textures, with the pixels decoded off the page's thread (garage models).
pub async fn load_jobs(
    mesh: &Mesh,
    archive: &Resource<'_>,
) -> Result<HashMap<String, MaterialTexture>> {
    let lod = mesh.lods.first().ok_or("original mesh has no LOD")?;
    let mut plans = Vec::new();
    for group in &lod.groups {
        let material = mesh
            .materials
            .get(group.material as usize)
            .ok_or("mesh material index out of range")?;
        if plans.iter().any(|(name, _)| name == material) {
            continue;
        }
        plans.push((material.clone(), material_plan(material, archive)?));
    }
    // Each PST member once (materials share maps), failing on its own.
    let mut members: Vec<usize> = Vec::new();
    for plan in plans.iter().filter_map(|(_, plan)| plan.as_ref()) {
        let source = match plan.source {
            Source::Pst(index) => Some(index),
            _ => None,
        };
        for index in source.into_iter().chain(plan.mud) {
            if !members.contains(&index) {
                members.push(index);
            }
        }
    }
    let inputs = members
        .iter()
        .map(|&index| archive.extract(index))
        .collect::<Vec<_>>();
    let readable: Vec<&[u8]> = inputs
        .iter()
        .filter_map(|input| input.as_deref().ok())
        .collect();
    let mut decoded = crate::offload::psts(&readable).await?.into_iter();
    let images: Vec<Result<Image>> = inputs
        .into_iter()
        .map(|input| {
            input?;
            let (width, height, rgba) = decoded.next().ok_or("missing decode result")??;
            Ok(Image {
                width,
                height,
                rgba,
            })
        })
        .collect();
    let mut pst = |index: usize| {
        let at = members.iter().position(|&m| m == index);
        match at.map(|at| &images[at]) {
            Some(Ok(image)) => Ok(image.clone()),
            Some(Err(error)) => Err(error.clone()),
            None => Err("texture was not decoded".to_string()),
        }
    };
    let mut textures = HashMap::new();
    for (name, plan) in plans {
        if let Some(plan) = plan {
            textures.insert(name, material_finish(plan, archive, &mut pst)?);
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
    textures.extend(outfit_textures(rider, outfit, archive));
    Ok(textures)
}

/// Only the outfit slots' replacement textures, by the rider mesh's
/// material names (outfitTextures; the garage dresses the menu rider).
pub fn outfit_textures(
    rider: &Mesh,
    outfit: &atv_core::race_roster::Outfit,
    archive: &Resource,
) -> HashMap<String, MaterialTexture> {
    let mut textures = HashMap::new();
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
        return textures;
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
    textures
}

/// Where one material's texture comes from, read from its PSM: everything
/// about it but the pixels.
struct MaterialPlan {
    fog: bool,
    blend: bool,
    source: Source,
    /// The mud map's PST member.
    mud: Option<usize>,
}

enum Source {
    /// A reflection pass the port draws untextured.
    Unsupported,
    Atm(usize),
    Tga(usize),
    Pst(usize),
}

fn material_plan(material: &str, archive: &Resource) -> Result<Option<MaterialPlan>> {
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
        return Ok(Some(MaterialPlan {
            fog,
            blend: false,
            source: Source::Unsupported,
            mud: None,
        }));
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
    let source = if let Some(index) = atm {
        Source::Atm(index)
    } else if let Some(index) = tga {
        Source::Tga(index)
    } else if let Some(index) = pst {
        Source::Pst(index)
    } else {
        if psm.is_none() {
            return Ok(None);
        }
        return Err(format!("original material texture is missing: {binding}"));
    };
    // MX MudAnimator maps (the second), or the ATV2 PixelAnimation dirt pass.
    let mud_name = if described.mud_maps.len() > 1 {
        Some(described.mud_maps[1].clone())
    } else {
        described.dirt_map.clone()
    };
    let mud = mud_name.and_then(|name| basename_index(archive, &format!("{name}.pst")));
    let blend = described.pass.as_ref().is_some_and(|pass| pass.blend);
    Ok(Some(MaterialPlan {
        fog,
        blend,
        source,
        mud,
    }))
}

/// Finishes a plan; `pst` gives the image of a PST member.
fn material_finish(
    plan: MaterialPlan,
    archive: &Resource,
    pst: &mut dyn FnMut(usize) -> Result<Image>,
) -> Result<MaterialTexture> {
    let decoded = |texture: Result<texture::Texture>| {
        texture.map(|t| Image {
            width: t.info.width,
            height: t.info.height,
            rgba: t.rgba,
        })
    };
    let image = match plan.source {
        Source::Unsupported => None,
        Source::Atm(index) => decoded(
            archive
                .extract(index)
                .and_then(|bytes| texture::decode_atm(&bytes, 0).map(|(_, t)| t)),
        )
        .ok(),
        Source::Tga(index) => decoded(
            archive
                .extract(index)
                .and_then(|bytes| texture::decode_tga(&bytes)),
        )
        .ok(),
        // The browser draws a material whose image fails to decode untextured.
        Source::Pst(index) => pst(index).ok(),
    };
    let mud = plan.mud.map(&mut *pst).transpose()?;
    Ok(MaterialTexture {
        image,
        fog: plan.fog,
        mud,
        blend: plan.blend,
    })
}

fn material_texture(material: &str, archive: &Resource) -> Result<Option<MaterialTexture>> {
    let Some(plan) = material_plan(material, archive)? else {
        return Ok(None);
    };
    let mut pst = |index: usize| {
        let texture = texture::decode_pst(&archive.extract(index)?)?;
        Ok(Image {
            width: texture.info.width,
            height: texture.info.height,
            rgba: texture.rgba,
        })
    };
    material_finish(plan, archive, &mut pst).map(Some)
}
