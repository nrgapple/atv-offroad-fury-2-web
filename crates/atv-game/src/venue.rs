//! Loads one venue's terrain presentation from its original course archive.
//! Mirrors terrainFromContent/uploadTerrainTextures in web/game.js.
use crate::{
    Result,
    launch::course_resource,
    materials::{last_component, member_named},
    scenery::Scenery,
    sky::Sky,
    vegetation::Vegetation,
};
use atv_content::Content;
use atv_core::{ipu, resource::Resource, scene::Scene, terrain, texture};

#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Terrain {
    /// Vertex counts along X and Z (cells + 1).
    pub width: usize,
    pub depth: usize,
    pub spacing: f32,
    /// Row-major Z then X.
    pub heights: Vec<f32>,
    pub min: f32,
    pub max: f32,
    /// 256x256 base layers, one per original texture record.
    pub layers: Vec<Image>,
    /// Layer per 64-cell tile, row-major, `mapping_width` x `mapping_height`.
    pub mapping: Vec<u16>,
    pub mapping_width: u32,
    pub mapping_height: u32,
    pub detail: Image,
}

impl Terrain {
    pub fn size_x(&self) -> f32 {
        (self.width - 1) as f32 * self.spacing
    }
    pub fn size_z(&self) -> f32 {
        (self.depth - 1) as f32 * self.spacing
    }
    pub fn height(&self, x: usize, z: usize) -> f32 {
        self.heights[z * self.width + x]
    }
}

pub struct Venue {
    pub environment: String,
    pub scene: Scene,
    pub terrain: Terrain,
    pub sky: Option<Sky>,
    pub scenery: Scenery,
    pub vegetation: Option<Vegetation>,
    /// Shared live field: the renderer and physics read the same surface.
    pub water: Option<std::rc::Rc<std::cell::RefCell<crate::water::Water>>>,
    /// ENV08's train, shared like the water field (physics collides with it).
    pub train: Option<std::rc::Rc<std::cell::RefCell<crate::train::Train>>>,
    /// The train cars' sound sources (the mode TOY's procedural animations).
    pub train_sounds: Vec<atv_core::spatial_audio::Definition>,
    /// Placed TOY particle effects (smoke, fountains, fog), stepped at 60 Hz.
    pub ambient: Option<std::rc::Rc<std::cell::RefCell<crate::ambient::Ambient>>>,
    /// A race's National drop gate (drawn as the last scene model).
    pub drop_gate: Option<atv_core::race_gate::DropGate>,
    /// The dense render-grid ground (renderCore.terrainHeight): cameras and
    /// presentation follow the visible terrain; gameplay uses contact().
    pub visible: std::rc::Rc<atv_core::terrain::Heightfield>,
    /// Pre-race camera shots of a race-course load (empty in free ride).
    pub intro_shots: Vec<Vec<atv_core::race_intro::ShotFrame>>,
    /// Active Snow weather (ring emitters around the camera).
    pub snow: Option<std::rc::Rc<std::cell::RefCell<crate::snow::Weather>>>,
    /// `[TerrainZoneN] GenerateDust` flags of the venue TOY (lingering dust).
    pub dust_zones: [bool; 8],
    /// The venue TOY's terrain-zone presentation flags (tire tracks).
    pub terrain_zones: [crate::tire_tracks::TerrainZone; 8],
    /// Source Weather selection for this load (00208cc0).
    pub weather: Option<atv_core::weather::Selection>,
}

/// Presentation choices made at load time.
#[derive(Clone, Copy, Debug, Default)]
pub struct LoadOptions {
    /// Weather RNG sample (0..2^31), or a forced outcome for parity captures.
    pub weather: WeatherChoice,
    /// Decode the Enhanced/High smoothed sky tiles as well.
    pub sky_smooth: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum WeatherChoice {
    Random(u32),
    Active(bool),
}

impl Default for WeatherChoice {
    fn default() -> Self {
        Self::Random(0)
    }
}

pub async fn load(content: &Content, environment: &str, racing: bool) -> Result<Venue> {
    load_with(content, environment, racing, LoadOptions::default()).await
}

pub async fn load_with(
    content: &Content,
    environment: &str,
    racing: bool,
    options: LoadOptions,
) -> Result<Venue> {
    let course = course_resource(environment, racing)?;
    let bytes = crate::ride::load::course_archive(content, &course.archive, environment).await?;
    let archive = Resource::parse(&bytes)?;
    // The decode steps below are long: each yields to the host's loop
    // (desktop polls loads between events; the web resumes at once).
    crate::yield_frame().await;
    let extract = |name: &str, what: &str| -> Result<Vec<u8>> {
        archive.extract(
            member_named(&archive, name)
                .ok_or_else(|| crate::ride::load::missing_member(what, name))?,
        )
    };
    let scene_text = extract(&format!("{}.scn", course.scene_stem), "start position")?;
    let mode_index = member_named(&archive, &format!("{}.toy", course.mode_stem));
    let mode = mode_index.map(|i| archive.extract(i)).transpose()?;
    let mut scene = atv_core::scene::parse_with_start(&scene_text, mode.as_deref())?;
    // 00208cc0 reads [Weather] from the common/mode TOYs; an inactive
    // weather removes its model from the scene.
    // The browser compares this index with the ambient mode TOY's course
    // index even when it came from the placements archive; keep that.
    let (toy, toy_index) = match member_named(&archive, &format!("{environment}.toy")) {
        Some(index) => (Some(archive.extract(index)?), Some(index)),
        None => match &course.placements {
            Some(path) => {
                let bytes = content.file(path).await?;
                let placements = Resource::parse(&bytes)?;
                match member_named(&placements, &format!("{environment}.toy")) {
                    Some(index) => (Some(placements.extract(index)?), Some(index)),
                    None => (None, None),
                }
            }
            None => (None, None),
        },
    };
    let weather = match &toy {
        Some(common) => {
            let definition =
                atv_core::weather::Definition::parse(common, mode.as_deref().unwrap_or(&[]))?;
            let random = match options.weather {
                WeatherChoice::Random(sample) => sample & 0x7fff_ffff,
                WeatherChoice::Active(true) => 0,
                WeatherChoice::Active(false) => 0x7fff_ffff,
            };
            Some(atv_core::weather::select(definition, random, 1)?)
        }
        None => None,
    };
    if let Some(removed) = weather.as_ref().and_then(|w| w.removed_model.as_deref()) {
        scene
            .models
            .retain(|model| !model.name.eq_ignore_ascii_case(removed));
    }
    // National drop gate: its own model from the race TOY, not a scene
    // model (00222cd0, op 116); the race poses it after GO.
    let drop_gate = match (&mode, racing) {
        (Some(mode), true) => atv_core::race_gate::parse(&String::from_utf8_lossy(mode))?,
        _ => None,
    };
    let gate_model = drop_gate.as_ref().map(|gate| {
        let matrix = gate.matrix(0.);
        scene.models.push(atv_core::scene::StaticModel {
            name: gate.model.clone(),
            position: gate.position,
            look: [matrix[8], matrix[9], matrix[10]],
            up: Some([matrix[4], matrix[5], matrix[6]]),
            use_lighting: true,
            dynamic_lighting: false,
            world_matrix: matrix,
        });
        scene.models.len() - 1
    });
    let tr2 = extract(&format!("{environment}.tr2"), "terrain")?;
    let decoded = terrain::decode_tr2(&tr2)?;
    crate::yield_frame().await;
    let heights = decoded.heightfield()?;
    let ground = decoded.render_heights()?;
    crate::yield_frame().await;
    let (width, depth) = (
        decoded.width_cells as usize + 1,
        decoded.depth_cells as usize + 1,
    );
    if width < 2
        || depth < 2
        || !(decoded.spacing.is_finite() && decoded.spacing > 0.)
        || heights.len() != width * depth
    {
        return Err("invalid terrain dimensions".into());
    }
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for &h in &heights {
        if !h.is_finite() {
            return Err("nonfinite terrain height".into());
        }
        min = min.min(h);
        max = max.max(h);
    }

    let layout = terrain::texture_layout(&tr2)?;
    let mut layers = Vec::with_capacity(layout.records.len());
    for record in &layout.records {
        let mip = record.mips.last().ok_or("terrain texture has no mip")?;
        let image = ipu::decode_ipum(
            tr2.get(mip.offset..mip.offset + mip.bytes)
                .ok_or("terrain texture exceeds archive")?,
        )?;
        if record.width != 256 || record.height != 256 || image.rgba.len() != 256 * 256 * 4 {
            return Err("unsupported terrain texture image".into());
        }
        layers.push(Image {
            width: 256,
            height: 256,
            rgba: image.rgba,
        });
        crate::yield_frame().await;
    }
    let mapping_width = (width - 1).div_ceil(64) as u32;
    let mapping_height = (depth - 1).div_ceil(64) as u32;
    let mut mapping = vec![u16::MAX; (mapping_width * mapping_height) as usize];
    for region in &layout.regions {
        if region.span_cells != 256 || region.x % 256 != 0 || region.z % 256 != 0 {
            return Err("unsupported terrain texture region".into());
        }
        for (index, layer) in region.subtextures.iter().enumerate() {
            let layer = layer.ok_or("unsupported terrain texture region")?;
            let x = region.x / 64 + index as u32 % 4;
            let z = region.z / 64 + index as u32 / 4;
            let slot = (z * mapping_width + x) as usize;
            if x >= mapping_width
                || z >= mapping_height
                || layer as usize >= layers.len()
                || mapping[slot] != u16::MAX
            {
                return Err("invalid terrain texture region coverage".into());
            }
            mapping[slot] = layer;
        }
    }
    if mapping.contains(&u16::MAX) {
        return Err("terrain texture regions do not cover the terrain".into());
    }
    // 002098b0 derives the original detail resource from the terrain basename.
    let detail = texture::decode_pst(&extract(
        &format!("{environment}_d.pst"),
        "ground detail texture",
    )?)?;
    let sky_index = archive
        .members
        .iter()
        .position(|m| last_component(&m.name).eq_ignore_ascii_case(&scene.cube_file))
        .ok_or("Original scene sky is missing.")?;
    let sky = Some(crate::sky::load(
        &archive.extract(sky_index)?,
        options.sky_smooth,
    )?);
    crate::yield_frame().await;
    let scenery = crate::scenery::load(&archive, &scene, toy.as_deref(), gate_model)?;
    crate::yield_frame().await;
    let vegetation = crate::vegetation::load(&archive, environment, &scene, &ground)?;
    crate::yield_frame().await;
    let water = crate::water::load(&archive, &scene)?
        .map(|water| std::rc::Rc::new(std::cell::RefCell::new(water)));
    let ambient = crate::ambient::load(
        &archive,
        environment,
        &scene_text,
        toy.as_deref(),
        toy_index,
        mode.as_deref(),
        mode_index,
    )
    .unwrap_or_else(|error| {
        // The browser skips effects it cannot configure.
        eprintln!("ambient effects: {error}");
        None
    })
    .map(|ambient| std::rc::Rc::new(std::cell::RefCell::new(ambient)));
    let snow = crate::snow::load(&archive, weather.as_ref())?
        .map(|snow| std::rc::Rc::new(std::cell::RefCell::new(snow)));
    let train = if environment == "env08" {
        crate::train::load(&archive, mode_index)?
            .map(|train| std::rc::Rc::new(std::cell::RefCell::new(train)))
    } else {
        None
    };
    // GameAudio.loadSpatial: the mode TOY's procedural train animations name
    // each car's looping sound.
    let train_sounds = match (&train, mode.as_deref()) {
        (Some(_), Some(mode)) => {
            atv_core::spatial_audio::train_definitions(mode).unwrap_or_else(|error| {
                // A missing train sample must leave driving usable.
                eprintln!("train sound: {error}");
                Vec::new()
            })
        }
        _ => Vec::new(),
    };
    Ok(Venue {
        environment: environment.to_string(),
        sky,
        scenery,
        vegetation,
        water,
        train,
        train_sounds,
        ambient,
        snow,
        dust_zones: toy
            .as_deref()
            .map(|toy| crate::lingering_dust::dust_zones(&String::from_utf8_lossy(toy)))
            .unwrap_or_default(),
        drop_gate,
        visible: std::rc::Rc::new(ground),
        terrain_zones: toy
            .as_deref()
            .map(|toy| crate::tire_tracks::terrain_zones(&String::from_utf8_lossy(toy)))
            .unwrap_or_default(),
        intro_shots: if racing {
            crate::intro::shots(&archive, environment)?
        } else {
            Vec::new()
        },
        weather,
        scene,
        terrain: Terrain {
            width,
            depth,
            spacing: decoded.spacing,
            heights,
            min,
            max,
            layers,
            mapping,
            mapping_width,
            mapping_height,
            detail: Image {
                width: detail.info.width,
                height: detail.info.height,
                rgba: detail.rgba,
            },
        },
    })
}
