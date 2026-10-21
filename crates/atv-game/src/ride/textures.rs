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
