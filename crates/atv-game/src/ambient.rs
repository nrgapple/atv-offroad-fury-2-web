//! Placed TOY ambient effects (loadAmbientEffects in web/ambient-effects.js):
//! atv_core::ambient_effects simulates the original emitters; this module
//! gathers their sources and atlases from the course archive.
use crate::{Result, materials::last_component, venue::Image};
use atv_core::{
    ambient_effects::{Effects, Frame, Metadata, Source, plan},
    resource::Resource,
    texture,
};

pub struct Ambient {
    pub effects: Effects,
    pub metadata: Metadata,
    /// CPU sprite atlas (Particles.prt TextureMap).
    pub atlas: Option<Image>,
    /// 128x256 GPU fountain atlas.
    pub gpu_atlas: Option<Image>,
    /// The mode TOY the emitters came from (race fireworks read it).
    pub mode: Option<String>,
}

impl Ambient {
    /// One 60 Hz step; moving actors (the ENV08 locomotive's Smoke.slt)
    /// publish their poses first.
    pub fn step(&mut self, actors: &[(String, [f32; 16])]) -> Result<()> {
        if !actors.is_empty() {
            self.effects
                .set_actor_poses(actors.iter().map(|(m, matrix)| (m.as_str(), matrix)));
        }
        self.effects.step(1. / 60.).map(|_| ())
    }

    pub fn frame(&self) -> &Frame {
        self.effects.frame()
    }
}

fn named(archive: &Resource, name: &str) -> Option<usize> {
    archive
        .members
        .iter()
        .position(|m| last_component(&m.name).eq_ignore_ascii_case(name))
}

fn text(archive: &Resource, index: usize) -> Result<String> {
    Ok(String::from_utf8_lossy(&archive.extract(index)?).into_owned())
}

pub(crate) fn image(archive: &Resource, name: &str) -> Result<Image> {
    let index = named(archive, name)
        .ok_or_else(|| format!("Original ambient resource is missing: {name}"))?;
    let decoded = texture::decode_pst(&archive.extract(index)?)?;
    Ok(Image {
        width: decoded.info.width,
        height: decoded.info.height,
        rgba: decoded.rgba,
    })
}

/// The scene's GPU particle texture: TextureMap_1 of particle_gpu.psm or
/// envNNparticles_gpu.psm.
pub(crate) fn gpu_atlas(archive: &Resource) -> Result<Image> {
    let material = archive
        .members
        .iter()
        .position(|m| {
            let name = last_component(&m.name).to_ascii_lowercase();
            name == "particle_gpu.psm"
                || name
                    .strip_prefix("env")
                    .and_then(|rest| rest.strip_suffix("particles_gpu.psm"))
                    .is_some_and(|digits| {
                        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                    })
        })
        .ok_or("Original fountain GPU material is missing.")?;
    let definition = text(archive, material)?;
    let binding = definition
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim().eq_ignore_ascii_case("TextureMap_1")
                && !value.trim().is_empty()
                && value
                    .trim()
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            .then(|| value.trim().to_string())
        })
        .ok_or("Original fountain GPU atlas binding is missing.")?;
    image(archive, &format!("{binding}.pst"))
}

pub fn load(
    archive: &Resource,
    environment: &str,
    scene: &[u8],
    common: Option<&[u8]>,
    toy_index: Option<usize>,
    mode: Option<&[u8]>,
    mode_index: Option<usize>,
) -> Result<Option<Ambient>> {
    let suffix = if environment == "game01" { "so" } else { "fr" };
    let ambient_mode = named(archive, &format!("{environment}{suffix}.toy"));
    let mode = match ambient_mode {
        Some(index) if Some(index) != toy_index => Some(text(archive, index)?),
        _ if mode_index != toy_index => mode.map(|m| String::from_utf8_lossy(m).into_owned()),
        _ => None,
    };
    let mode_text = mode.clone();
    let mut source = Source {
        scene: String::from_utf8_lossy(scene).into_owned(),
        common: common
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .unwrap_or_default(),
        mode,
        prt: String::new(),
        emts: Default::default(),
    };
    let layout = plan(&source)?;
    if layout.declared == 0 {
        return Ok(None);
    }
    for placement in &layout.placements {
        if [0, 2, 8, 11, 12].contains(&placement.type_id)
            && !source.emts.contains_key(&placement.filename)
            && let Some(index) = named(archive, &placement.filename)
        {
            source
                .emts
                .insert(placement.filename.clone(), text(archive, index)?);
        }
    }
    if let Some(index) = archive
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with("particles.prt"))
    {
        source.prt = text(archive, index)?;
    }
    let (effects, metadata) = Effects::from_source(&source)?;
    if metadata.active == 0 {
        return Ok(None);
    }
    let atlas = match &metadata.atlas {
        Some(atlas) => {
            let stem = atlas
                .texture
                .strip_suffix(".tga")
                .or_else(|| atlas.texture.strip_suffix(".TGA"))
                .unwrap_or(&atlas.texture);
            let image = image(archive, &format!("{stem}.pst"))?;
            if atlas
                .frames
                .iter()
                .any(|f| f.rect[2] > image.width as f32 || f.rect[3] > image.height as f32)
            {
                return Err("Original ambient rectangle exceeds atlas.".into());
            }
            Some(image)
        }
        None => None,
    };
    let gpu_atlas = if metadata.gpu_active > 0 {
        let image = gpu_atlas(archive)?;
        if image.width != 128 || image.height != 256 {
            return Err("Unsupported original fountain atlas.".into());
        }
        Some(image)
    } else {
        None
    };
    Ok(Some(Ambient {
        effects,
        metadata,
        atlas,
        gpu_atlas,
        mode: mode_text,
    }))
}
