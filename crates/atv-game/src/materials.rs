//! Original PSM material passes and their PST/ATM/TGA textures
//! (baseTextures in web/game.js). Rust reads ordered passes, bindings,
//! shader, alpha test and scene cutouts (atv_core::scenery_material).
use crate::{Result, venue::Image};
use atv_core::{
    resource::Resource,
    scenery_material::{self, Pass},
    texture,
};
use std::{collections::HashMap, rc::Rc};

/// One decoded material texture; ATM animations keep every frame.
pub struct MaterialImage {
    pub image: Image,
    /// Every ATM frame (the first equals `image`) for authored texture clocks.
    pub frames: Vec<Vec<u8>>,
}

pub struct MaterialPass {
    pub shader: Option<String>,
    pub reflective: bool,
    pub pass: Option<Pass>,
    /// Shared declarations plus this pass, for UV scroll/ATM clocks.
    pub source: String,
    pub alpha_test: f32,
    pub scene_alpha_failure: f32,
    pub scene_alpha_cutoff: f32,
    /// None when the authored texture failed to decode or the pass is an
    /// unsupported reflection program (skipped by the scene builder).
    pub texture: Option<Rc<MaterialImage>>,
    pub unsupported_reflection: bool,
}

impl MaterialPass {
    /// A material without any bound pass: drawn untextured, lit and fogged.
    fn untextured() -> Self {
        Self {
            shader: None,
            reflective: false,
            pass: None,
            source: String::new(),
            alpha_test: 0.,
            scene_alpha_failure: 0.,
            scene_alpha_cutoff: 0.,
            texture: None,
            unsupported_reflection: false,
        }
    }
    pub fn fog(&self) -> bool {
        self.pass.as_ref().is_none_or(|p| p.fog)
    }
}

/// A material's passes in authored order. The browser keys pass N>0 as
/// `name#passN+1` and only lists them when the base pass is bound; an
/// unbound base pass leaves one untextured draw.
pub struct Material {
    pub passes: Vec<MaterialPass>,
    /// `[General] EnableMultiPassCull` (`001bf430`, default on): beyond
    /// 130 ft only the first pass draws (research/VISIBILITY.md).
    pub multipass_cull: bool,
}

pub fn last_component(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

pub fn member_named(resource: &Resource, name: &str) -> Option<usize> {
    resource
        .members
        .iter()
        .position(|member| last_component(&member.name).eq_ignore_ascii_case(name))
}

/// Member at an exact archive path (`geometry/x.psg`), case-insensitive.
pub fn member_at(resource: &Resource, path: &str) -> Option<usize> {
    resource
        .members
        .iter()
        .position(|member| member.name.replace('\\', "/").eq_ignore_ascii_case(path))
}

/// Per-archive material cache, shared by every model of one family.
#[derive(Default)]
pub struct Materials {
    cache: HashMap<String, Rc<Material>>,
}

impl Materials {
    pub fn get(
        &mut self,
        archive: &Resource,
        name: &str,
        multi_pass: bool,
    ) -> Result<Rc<Material>> {
        if let Some(material) = self.cache.get(name) {
            return Ok(material.clone());
        }
        let material = Rc::new(load(archive, name, multi_pass)?);
        self.cache.insert(name.to_string(), material.clone());
        Ok(material)
    }
}

fn load(archive: &Resource, name: &str, multi_pass: bool) -> Result<Material> {
    let psm = member_named(archive, &format!("{name}.psm"));
    let text = match psm {
        Some(index) => String::from_utf8_lossy(&archive.extract(index)?).into_owned(),
        None => String::new(),
    };
    // Without an authored PSM, 001bf430 -> 001b5080 binds the material's own stem.
    let described = scenery_material::describe(&text, multi_pass, psm.is_none().then_some(name))?;
    let mut passes: Vec<Option<MaterialPass>> = Vec::with_capacity(described.len());
    for described in described {
        let Some(binding) = described.binding.clone() else {
            passes.push(None);
            continue;
        };
        let mut pass = MaterialPass {
            shader: described.shader,
            reflective: described.reflective,
            source: described.source,
            alpha_test: described.alpha_test as f32,
            scene_alpha_failure: described.scene_alpha_failure as f32,
            scene_alpha_cutoff: described.scene_alpha_cutoff as f32,
            texture: None,
            unsupported_reflection: described.unsupported_reflection,
            pass: described.pass,
        };
        if pass.unsupported_reflection {
            passes.push(Some(pass));
            continue;
        }
        let animated = pass
            .pass
            .as_ref()
            .is_some_and(|p| p.texture_animation.is_some());
        let pst = member_named(archive, &format!("{binding}.pst"));
        let atm = if animated || pst.is_none() {
            member_named(archive, &format!("{binding}.atm"))
        } else {
            None
        };
        let tga = if pst.is_none() && atm.is_none() {
            member_named(archive, &format!("{binding}.tga"))
        } else {
            None
        };
        let Some(index) = atm.or(tga).or(pst) else {
            if psm.is_none() {
                passes.push(None);
                continue;
            }
            return Err(format!("Original material texture is missing: {binding}"));
        };
        let bytes = archive.extract(index)?;
        match decode(&bytes, atm.is_some(), tga.is_some(), animated) {
            Ok(image) => pass.texture = Some(Rc::new(image)),
            // An undecodable texture keeps only its shader name: the browser
            // draws the pass untextured with default pass state.
            Err(_) => {
                pass.pass = None;
                pass.alpha_test = 0.;
                pass.scene_alpha_failure = 0.;
                pass.scene_alpha_cutoff = 0.;
            }
        }
        passes.push(Some(pass));
    }
    let passes = match passes.first() {
        Some(Some(_)) => passes.into_iter().flatten().collect(),
        _ => vec![MaterialPass::untextured()],
    };
    Ok(Material {
        passes,
        multipass_cull: atv_core::model_lod::multipass_cull(&text),
    })
}

fn decode(bytes: &[u8], atm: bool, tga: bool, animated: bool) -> Result<MaterialImage> {
    let (first, frames) = if atm {
        let (count, first) = texture::decode_atm(bytes, 0)?;
        let mut frames = vec![first.rgba.clone()];
        if animated {
            for frame in 1..count {
                frames.push(texture::decode_atm(bytes, frame)?.1.rgba);
            }
        }
        (first, frames)
    } else if tga {
        let t = texture::decode_tga(bytes)?;
        let frames = vec![t.rgba.clone()];
        (t, frames)
    } else {
        let t = texture::decode_pst(bytes)?;
        let frames = vec![t.rgba.clone()];
        (t, frames)
    };
    let (width, height) = (first.info.width, first.info.height);
    if width == 0 || height == 0 || first.rgba.len() != (width * height * 4) as usize {
        return Err("invalid material texture dimensions".into());
    }
    Ok(MaterialImage {
        image: Image {
            width,
            height,
            rgba: first.rgba,
        },
        frames: if animated { frames } else { Vec::new() },
    })
}
