//! Original WTT water surface: the live height field shared with physics
//! (atv_core::water_animation) and its render inputs (atv_core::water_render),
//! as startSimulation and WaterRenderer read them (ops 64/66 in atv-web).
use crate::{Result, materials::last_component, venue::Image};
use atv_core::{
    resource::Resource,
    scene::Scene,
    texture,
    water_animation::{Definition, LiveField},
    water_render::{Frame, GradientAtlas, HeightAtlas, Material},
};

pub struct Water {
    /// Live field; physics samples the same surface.
    pub field: LiveField,
    pub material: Material,
    pub height: HeightAtlas,
    pub gradients: GradientAtlas,
    pub environment: Image,
}

impl Water {
    /// One 60 Hz simulation step: update clocks, then render preparation
    /// (the order of the browser's batch around the vehicle step).
    pub fn step(&mut self) -> Result<()> {
        self.field.advance(1. / 60.)?;
        self.field.prepare_surface()
    }

    pub fn frame(&self) -> Frame {
        Frame::new(&self.field)
    }
}

pub fn load(archive: &Resource, scene: &Scene) -> Result<Option<Water>> {
    let Some(file) = &scene.water_file else {
        return Ok(None);
    };
    let index = archive
        .members
        .iter()
        .position(|m| last_component(&m.name).eq_ignore_ascii_case(file))
        .ok_or("Original water definition is missing.")?;
    let (directory, filename) = archive.members[index]
        .name
        .rsplit_once('/')
        .ok_or("water resource has no directory")?;
    if !filename.to_ascii_lowercase().ends_with(".wtt") {
        return Err("water resource must be WTT".into());
    }
    let signatures = format!("{directory}/Sigs");
    let load = |path: &str| -> Result<Vec<u8>> {
        let member = archive
            .members
            .iter()
            .position(|member| member.name.eq_ignore_ascii_case(path))
            .ok_or_else(|| format!("missing water resource {path}"))?;
        archive.extract(member)
    };
    let wtt = archive.extract(index)?;
    let field = LiveField::new(Definition::parse(&wtt)?, &signatures, load)?;
    let shader = Material::shader_name(&wtt)?;
    let material = Material::parse(&wtt, &load(&format!("textures/{shader}.psm"))?)?;
    let environment = texture::decode_pst(&load(&format!(
        "textures/{}.pst",
        material.environment_texture
    ))?)?;
    let gradients = GradientAtlas::new(&wtt, &signatures, load)?;
    for wave in &gradients.waves {
        if field
            .waves
            .get(wave.origin_channel)
            .is_none_or(|channel| channel.dimension != gradients.textures[wave.layer].dimension)
        {
            return Err(
                "portable water requires matching height/derivative scroll dimensions".into(),
            );
        }
    }
    let height = HeightAtlas::new(&field)?;
    Ok(Some(Water {
        field,
        material,
        height,
        gradients,
        environment: Image {
            width: environment.info.width,
            height: environment.info.height,
            rgba: environment.rgba,
        },
    }))
}
