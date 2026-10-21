//! Active ring-emitter Snow weather (loadSnow in web/weather.js): the source
//! SnowEmitter (atv_core::weather) and the scene's GPU particle texture.
use crate::{Result, materials::last_component, venue::Image};
use atv_core::{
    resource::Resource,
    texture,
    weather::{Selection, Snow, SnowConfig},
};

pub struct Weather {
    pub snow: Snow,
    pub texture: Image,
}

pub fn load(archive: &Resource, selection: Option<&Selection>) -> Result<Option<Weather>> {
    let Some(definition) = selection
        .filter(|s| s.active)
        .and_then(|s| s.definition.as_ref())
        .filter(|d| d.kind == "snow" && d.ring_emitters)
    else {
        return Ok(None);
    };
    let named = |name: &str| {
        archive
            .members
            .iter()
            .position(|m| last_component(&m.name).eq_ignore_ascii_case(name))
    };
    let emitter = named("snow.emt").ok_or("Original Snow emitter is missing.")?;
    let config = SnowConfig::parse(definition, &archive.extract(emitter)?)?;
    // Snow uses the scene's GPU material binding, as the ATV scene manager does.
    let material = archive
        .members
        .iter()
        .position(|m| {
            let name = last_component(&m.name).to_ascii_lowercase();
            name == "particle_gpu.psm"
                || name
                    .strip_prefix("env")
                    .and_then(|rest| rest.strip_suffix("particles_gpu.psm"))
                    .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .ok_or("Original Snow GPU material is missing.")?;
    let text = String::from_utf8_lossy(&archive.extract(material)?).into_owned();
    let binding = text
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            (key.trim().eq_ignore_ascii_case("TextureMap_1")
                && !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            .then(|| value.to_string())
        })
        .and_then(|binding| named(&format!("{binding}.pst")))
        .ok_or("Original Snow GPU particle texture is missing.")?;
    let decoded = texture::decode_pst(&archive.extract(binding)?)?;
    Ok(Some(Weather {
        snow: Snow::new(config),
        texture: Image {
            width: decoded.info.width,
            height: decoded.info.height,
            rgba: decoded.rgba,
        },
    }))
}
