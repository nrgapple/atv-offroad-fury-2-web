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
use atv_core::{resource::Resource, scene::Scene, terrain, texture};

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
    /// Each layer's box-filtered mip levels below it, computed with the
    /// decode (offload.rs); empty when not computed (the renderer filters).
    pub layer_mips: Vec<Vec<Vec<u8>>>,
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
    /// The race a race load read (National for free ride).
    pub race: crate::launch::RaceVariant,
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
    /// The keyframed actors' loop sounds (`Actors::sounds`), `car` indexing
    /// `Actors::sound_emitters`.
    pub actor_sounds: Vec<atv_core::spatial_audio::Definition>,
    /// Keyframed TOY actors (crows, fish, windmills, blimps...), posed by
    /// their looping motions on the 60 Hz scene clock.
    pub actors: Option<std::rc::Rc<std::cell::RefCell<crate::actors::Actors>>>,
    /// Placed TOY particle effects (smoke, fountains, fog), stepped at 60 Hz.
    pub ambient: Option<std::rc::Rc<std::cell::RefCell<crate::ambient::Ambient>>>,
    /// A race's National drop gate (drawn as the last scene model).
    pub drop_gate: Option<atv_core::race_gate::DropGate>,
    /// An Enduro race's gates in order, the finish gate last (`0022a340`).
    pub enduro_gates: Vec<atv_core::race::Gate>,
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
    /// A Freestyle objective's course file and targets (the OB archive).
    pub objective: Option<Objective>,
    /// Treasure Hunt's treasures from the game's mode TOY: their `bomb`
    /// models and loop (`bomb.MOT`), posed and hidden by the game.
    pub treasures: Option<Objective>,
    /// The Training Course's two `arrow.slt` markers (`00224a80`): the
    /// goal's and Steering's first circle's, posed and hidden by the
    /// course (research/TRAINING.md).
    pub training_arrows: Option<[usize; 2]>,
    /// Tag's `tagball.psg` (a `*_TG.RES` ride): the posed model the Tag
    /// session moves each frame through the drop gate's path.
    pub tag_ball: bool,
    /// Zone references of the scene and TOYs (StartingZone/Zone/InitialZone)
    /// against the scene's zones (research/VISIBILITY.md).
    pub zones: atv_core::zones::Census,
}

/// A Freestyle objective's data from the course's OB archive
/// (research/CHAMPIONSHIPS.md "Freestyle objectives").
pub struct Objective {
    /// The course `.obj` (`[Objectives]`), parsed.
    pub objectives: Vec<atv_core::freestyle::Objective>,
    /// The target TOY's targets (empty for Combo and Stunt runs) and each
    /// one's cherry: its scene model index (appended after the scene's).
    pub targets: Vec<atv_core::freestyle::Target>,
    pub models: Vec<Option<usize>>,
    /// The cherry loop (`cherry.MOT`, `cherryg.mcf` Looping=yes) and the
    /// cherry PSG's nodes it poses.
    pub motion: Option<(atv_core::animation::Animation, Vec<atv_core::mesh::Node>)>,
}

impl Objective {
    /// A cherry's world matrix at `time` seconds: the TOY offset and the
    /// looping motion's root pose (a turn about +Y and a 2 ft bob).
    pub fn cherry_matrix(&self, target: usize, time: f32) -> Option<[f32; 16]> {
        let t = self.targets.get(target)?;
        let mut matrix = match &self.motion {
            Some((mot, nodes)) if mot.duration > 0. => mot
                .sample_pose(nodes, time.rem_euclid(mot.duration), Default::default())
                .ok()
                .and_then(|p| p.world_matrices.first().copied())
                .unwrap_or(IDENTITY),
            _ => IDENTITY,
        };
        for a in 0..3 {
            matrix[12 + a] += t.offset[a];
        }
        Some(matrix)
    }
}

type AnimatedModels = (
    Vec<Option<usize>>,
    Option<(atv_core::animation::Animation, Vec<atv_core::mesh::Node>)>,
);
/// Appends each animated target's model to the scene at its TOY offset and
/// reads the looping motion (`mot`) with the model's PSG nodes it poses:
/// Freestyle's cherries and Treasure Hunt's bombs.
fn animated_models(
    archive: &Resource,
    scene: &mut atv_core::scene::Scene,
    targets: &[atv_core::freestyle::Target],
    mot: &str,
) -> Result<AnimatedModels> {
    let mut models = Vec::with_capacity(targets.len());
    for t in targets {
        if t.model.is_empty() {
            models.push(None);
            continue;
        }
        let [x, y, z] = t.offset;
        let mut matrix = IDENTITY;
        matrix[12..15].copy_from_slice(&[x, y, z]);
        scene.models.push(atv_core::scene::StaticModel {
            name: t.model.clone(),
            position: t.offset,
            look: [0., 0., 1.],
            up: Some([0., 1., 0.]),
            use_lighting: true,
            dynamic_lighting: false,
            world_matrix: matrix,
        });
        models.push(Some(scene.models.len() - 1));
    }
    let motion = match (
        member_named(archive, mot),
        targets
            .first()
            .and_then(|t| member_named(archive, &format!("{}.psg", t.model))),
    ) {
        (Some(mot), Some(psg)) => Some((
            atv_core::animation::decode_mot(&archive.extract(mot)?)?,
            atv_core::mesh::decode_psg(&archive.extract(psg)?)?.nodes,
        )),
        _ => None,
    };
    Ok((models, motion))
}

const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

/// Presentation choices made at load time.
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// Weather RNG sample (0..2^31), or a forced outcome for parity captures.
    pub weather: WeatherChoice,
    /// Decode the Enhanced/High smoothed sky tiles as well.
    pub sky_smooth: bool,
    /// Which of the venue's races a race load reads.
    pub race: crate::launch::RaceVariant,
    /// A mini-game's archive and scene stem (`E05_KH.RES`, `env05kh`):
    /// a Freeride load reads that scene and mode TOY over the venue's
    /// Freeride archive (research/GAMES.md).
    pub game_scene: Option<(String, String)>,
    /// A Freestyle objective: the course's OB archive (read over the
    /// Freeride archive) and the objective's target TOY stem, if any.
    pub objective: Option<(String, Option<String>)>,
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
    let mut freeride_scene = None;
    let course = if racing {
        crate::launch::race_resource(environment, options.race)?
    } else {
        let mut course = course_resource(environment, false)?;
        if let Some((archive, stem)) = &options.game_scene {
            freeride_scene = Some(course.scene_stem.clone());
            course.overlay = Some(archive.clone());
            course.scene_stem = stem.clone();
            course.mode_stem = stem.clone();
        } else if let Some((archive, _)) = &options.objective {
            course.overlay = Some(archive.clone());
        }
        course
    };
    let bytes = crate::ride::load::course_archive(content, &course, environment).await?;
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
    // Treasure Hunt ships no scene of its own (`env05th.toy` but no
    // `env05th.scn`): it rides the Freeride scene with its mode TOY.
    let scene_stem = match &freeride_scene {
        Some(freeride)
            if member_named(&archive, &format!("{}.scn", course.scene_stem)).is_none() =>
        {
            freeride.clone()
        }
        _ => course.scene_stem.clone(),
    };
    let scene_text = extract(&format!("{scene_stem}.scn"), "start position")?;
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
    // Coonce Arena's puck (`00337be0`, `hockeypuck.psg`): a free-moving
    // model the Hockey session poses each frame through the posed-model
    // path the drop gate uses (research/GAMES.md). It rests at the TOY's
    // `[Ball] StartPosition` until posed.
    let gate_model = gate_model.or_else(|| {
        (environment == "game01" && !racing && member_named(&archive, "hockeypuck.psg").is_some())
            .then(|| {
                let [x, y, z] = atv_core::hockey::Arena::COONCE.puck_start;
                let mut matrix = [0.; 16];
                for i in [0, 5, 10, 15] {
                    matrix[i] = 1.;
                }
                matrix[12..15].copy_from_slice(&[x, y, z]);
                scene.models.push(atv_core::scene::StaticModel {
                    name: "hockeypuck".into(),
                    position: [x, y, z],
                    look: [0., 0., 1.],
                    up: Some([0., 1., 0.]),
                    use_lighting: true,
                    dynamic_lighting: false,
                    world_matrix: matrix,
                });
                scene.models.len() - 1
            })
    });
    // Tag's ball (`00244d20` builds the `tagball.psg` body): a TG ride's
    // posed model, held at the mode TOY's start until the session poses it.
    let tag_ball = !racing
        && options
            .game_scene
            .as_ref()
            .is_some_and(|(_, stem)| stem.ends_with("tg"))
        && member_named(&archive, "tagball.psg").is_some();
    let gate_model = gate_model.or_else(|| {
        tag_ball.then(|| {
            let [x, y, z] = scene.start_position;
            let mut matrix = [0.; 16];
            for i in [0, 5, 10, 15] {
                matrix[i] = 1.;
            }
            matrix[12..15].copy_from_slice(&[x, y + 4., z]);
            scene.models.push(atv_core::scene::StaticModel {
                name: "tagball".into(),
                position: [x, y + 4., z],
                look: [0., 0., 1.],
                up: Some([0., 1., 0.]),
                use_lighting: true,
                dynamic_lighting: false,
                world_matrix: matrix,
            });
            scene.models.len() - 1
        })
    });
    // The Training Course's arrows (`00224a80` builds two `arrow.slt`
    // pointer objects); hidden until the course places them.
    let training_arrows =
        (environment == "training" && !racing && member_named(&archive, "arrow.psg").is_some())
            .then(|| {
                std::array::from_fn(|_| {
                    scene.models.push(atv_core::scene::StaticModel {
                        name: "arrow".into(),
                        position: [0.; 3],
                        look: [0., 0., 1.],
                        up: Some([0., 1., 0.]),
                        use_lighting: true,
                        dynamic_lighting: false,
                        world_matrix: IDENTITY,
                    });
                    scene.models.len() - 1
                })
            });
    // A Freestyle objective (`<course>.obj`) and its targets' cherries,
    // posed by the run (they spin and bob, and hide when hit).
    // The objective's target TOY: its cherry explosions are appended
    // emitters (`002427b0`) that its `Particles` actions start.
    let mut target_toy = None;
    let objective = match &options.objective {
        Some((_, toy)) if !racing => {
            let text = String::from_utf8_lossy(&extract(
                &format!("{environment}.obj"),
                "objective course file",
            )?)
            .into_owned();
            let objectives = atv_core::freestyle::parse_objectives(&text)?;
            let targets = match toy {
                Some(stem) => {
                    let bytes = extract(&format!("{stem}.toy"), "objective targets")?;
                    let targets =
                        atv_core::freestyle::parse_targets(&String::from_utf8_lossy(&bytes));
                    target_toy = Some(bytes);
                    targets
                }
                None => Vec::new(),
            };
            let (models, motion) = animated_models(&archive, &mut scene, &targets, "cherry.mot")?;
            Some(Objective {
                objectives,
                targets,
                models,
                motion,
            })
        }
        _ => None,
    };
    // Treasure Hunt (`002d0750`): each `Treasure` trigger hides its
    // `bomb.mcf` animation when collected.
    let treasures = match (&options.game_scene, &mode) {
        (Some(_), Some(mode)) if !racing => {
            let targets = atv_core::freestyle::parse_animated_triggers(
                &String::from_utf8_lossy(mode),
                "Treasure",
            );
            if targets.is_empty() {
                None
            } else {
                let (models, motion) = animated_models(&archive, &mut scene, &targets, "bomb.mot")?;
                Some(Objective {
                    objectives: Vec::new(),
                    targets,
                    models,
                    motion,
                })
            }
        }
        _ => None,
    };
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
    // The layers decode in parallel off the page's thread (offload.rs).
    let mut sources = Vec::with_capacity(layout.records.len());
    for record in &layout.records {
        let mip = record.mips.last().ok_or("terrain texture has no mip")?;
        sources.push(
            tr2.get(mip.offset..mip.offset + mip.bytes)
                .ok_or("terrain texture exceeds archive")?,
        );
    }
    let mut layers = Vec::with_capacity(layout.records.len());
    let mut layer_mips = Vec::with_capacity(layout.records.len());
    for (record, image) in layout
        .records
        .iter()
        .zip(crate::offload::ipums_with(&sources, false, true).await?)
    {
        layer_mips.push(image.mips);
        if record.width != 256 || record.height != 256 || image.rgba.len() != 256 * 256 * 4 {
            return Err("unsupported terrain texture image".into());
        }
        layers.push(Image {
            width: 256,
            height: 256,
            rgba: image.rgba,
        });
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
    let sky = Some(crate::sky::load(&archive.extract(sky_index)?, options.sky_smooth).await?);
    crate::yield_frame().await;
    // Enduro gates (`0022a340`): a `<venue>endwgate` at every waypoint but
    // the last, which holds the `<venue>endfgate` finish gate; both collide.
    let enduro_gates = if racing && options.race.is_enduro() {
        let stem = &course.mode_stem;
        let waypoints =
            atv_core::enduro::parse_wpt(&extract(&format!("{stem}e.wpt"), "Enduro gates")?)?;
        let model = |kind: &str| format!("{environment}end{kind}gate");
        // The race box spans the gate model's stored half extent across it
        // (PSG node bounds, model +0x110).
        let mesh =
            atv_core::mesh::decode_psg(&extract(&format!("{}.psg", model("w")), "Enduro gate")?)?;
        let half_width = mesh.nodes.first().map_or(0., |n| n.bounds[3]);
        let gates = atv_core::enduro::gates(&waypoints, half_width, |x, z| {
            ground.height(x as f64, z as f64) as f32
        })?;
        for (i, gate) in gates.iter().enumerate() {
            let name = model(if i + 1 == gates.len() { "f" } else { "w" });
            let matrix = atv_core::enduro::model_matrix(gate);
            scene.models.push(atv_core::scene::StaticModel {
                name: name.clone(),
                position: [matrix[12], matrix[13], matrix[14]],
                look: [matrix[8], matrix[9], matrix[10]],
                up: Some([0., 1., 0.]),
                use_lighting: true,
                dynamic_lighting: false,
                world_matrix: matrix,
            });
            scene
                .collision_models
                .push(atv_core::scene::CollisionModel {
                    name,
                    world_matrix: matrix,
                    material: 0,
                });
        }
        gates
    } else {
        Vec::new()
    };
    // Gate banners point at the next gate; the gates are the last scene models.
    let first_gate = scene.models.len() - enduro_gates.len();
    let enduro_dress: Vec<_> = (0..enduro_gates.len())
        .map(|i| {
            let (front, back) = atv_core::enduro::arrow_textures(&enduro_gates, i);
            crate::scenery::EnduroDress {
                model: first_gate + i,
                front: format!("{environment}{front}"),
                back: format!("{environment}{back}"),
            }
        })
        .collect();
    let scenery =
        crate::scenery::load(&archive, &scene, toy.as_deref(), gate_model, &enduro_dress)?;
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
        crate::ambient::Sources {
            mode: mode.as_deref(),
            mode_index,
            game: options.game_scene.is_some() && !racing,
            targets: target_toy.as_deref(),
        },
    )
    .unwrap_or_else(|error| {
        // The browser skips effects it cannot configure.
        eprintln!("ambient effects: {error}");
        None
    })
    .map(|ambient| std::rc::Rc::new(std::cell::RefCell::new(ambient)));
    // Keyframed actors of the common and mode TOYs. Treasure Hunt's bombs
    // and the locomotive wheels are drawn by their own systems.
    let treasure_refs: Vec<String> = treasures
        .iter()
        .flat_map(|t| t.targets.iter().map(|t| t.animation.to_ascii_lowercase()))
        .collect();
    let has_train = environment == "env08";
    // A mode TOY that is the common TOY itself (Freeride on National and
    // Supercross venues) declares its actors once.
    let mode_toy = mode.as_deref().filter(|_| mode_index != toy_index);
    let toys: Vec<&[u8]> = toy.as_deref().into_iter().chain(mode_toy).collect();
    let mut zones = atv_core::zones::Census::default();
    zones.add(&String::from_utf8_lossy(&scene_text), scene.zones);
    for text in &toys {
        zones.add(&String::from_utf8_lossy(text), scene.zones);
    }
    let actors = crate::actors::load(&archive, &toys, &|d| {
        (!d.reference_name.is_empty()
            && treasure_refs.contains(&d.reference_name.to_ascii_lowercase()))
            || (has_train && d.mcf == "loco_wheels.mcf")
    })
    .unwrap_or_else(|error| {
        eprintln!("toy actors: {error}");
        Default::default()
    });
    for (name, error) in &actors.unresolved {
        eprintln!("toy actor {name}: {error}");
    }
    let actor_sounds = actors.sounds.iter().map(|s| s.definition.clone()).collect();
    let actors = (!actors.is_empty()).then(|| std::rc::Rc::new(std::cell::RefCell::new(actors)));
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
        race: if racing {
            options.race
        } else {
            Default::default()
        },
        sky,
        scenery,
        vegetation,
        water,
        train,
        train_sounds,
        actor_sounds,
        actors,
        ambient,
        snow,
        dust_zones: toy
            .as_deref()
            .map(|toy| crate::lingering_dust::dust_zones(&String::from_utf8_lossy(toy)))
            .unwrap_or_default(),
        drop_gate,
        enduro_gates,
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
        objective,
        treasures,
        training_arrows,
        tag_ball,
        zones,
        scene,
        terrain: Terrain {
            width,
            depth,
            spacing: decoded.spacing,
            heights,
            min,
            max,
            layers,
            layer_mips,
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
