//! Reads everything a Freeride needs from the original archives: the player's
//! quad and rider, their motions and textures, scene collision, water and the
//! follow-camera presets. Mirrors terrainFromContent/startSimulation in
//! web/game.js and the matching atv-web operations (2/3, 11, 20-22, 26-27,
//! 44-45, 48-49, 51-53, 64-65, 89, 140, 145).
use super::{Model, QuadRig, Ride, Seed, Simulation, textures};
use crate::{Result, launch::course_resource, venue::Venue};
use atv_content::Content;
use atv_core::math::Det;
use atv_core::{
    animation, camera, collision, ecosystem, materials, mesh, motion_catalog::MotionCatalog,
    obstacle::StaticObstacle, physics_data::PhysicsData, player_models, quad_pose::QuadPose,
    resource::Resource, rider_pose, stunt, stunt_score, terrain, water_animation, wreck_animation,
};
use std::rc::Rc;
use textures::basename_index;

const IDENTITY_COL: [f32; 12] = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];
const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

pub(crate) fn member<'a>(resource: &Resource<'a>, name: &str, what: &str) -> Result<Vec<u8>> {
    resource.extract(
        basename_index(resource, name)
            .ok_or_else(|| format!("original {what} is missing ({name})"))?,
    )
}

/// Member whose path ends with `suffix` (the rider loader's matching rule).
pub(crate) fn member_suffix<'a>(resource: &Resource<'a>, suffix: &str) -> Result<Vec<u8>> {
    let suffix = suffix.to_ascii_lowercase();
    let index = resource
        .members
        .iter()
        .position(|m| {
            m.name
                .replace('\\', "/")
                .to_ascii_lowercase()
                .ends_with(&suffix)
        })
        .ok_or_else(|| format!("original rider file is missing: {suffix}"))?;
    resource.extract(index)
}

/// Member by full lower-case path such as `geometry/name.col`.
fn member_path(resource: &Resource, path: &str) -> Option<usize> {
    resource
        .members
        .iter()
        .position(|m| m.name.replace('\\', "/").eq_ignore_ascii_case(path))
}

/// A float as the browser sees it after Rust's JSON output (shortest decimal
/// that round-trips binary32, read back as binary64).
fn js(value: f32) -> f64 {
    value.to_string().parse().unwrap_or(f64::from(value))
}

/// Normal, stunt/hold, wreck and landing clips in the order op 51 selects them.
pub(crate) fn riding_motions(
    catalog: &MotionCatalog,
) -> Result<Vec<atv_core::motion_catalog::Motion>> {
    let prefix = if catalog.motions.iter().any(|m| m.name.starts_with("F_")) {
        "F"
    } else {
        "M"
    };
    let find = |name: String, what: &str| {
        catalog
            .motions
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(&name))
            .cloned()
            .ok_or_else(|| format!("missing {what} motion {name}"))
    };
    let mut selected = Vec::new();
    for clip in rider_pose::Clip::ALL {
        selected.push(find(format!("{prefix}_{}", clip.suffix()), "rider")?);
    }
    for kind in ["Stunt", "Hold"] {
        for suffix in stunt::SUFFIXES {
            selected.push(find(format!("{prefix}_{kind}{suffix}"), "rider")?);
        }
    }
    for suffix in wreck_animation::SUFFIXES {
        selected.push(find(format!("{prefix}_{suffix}"), "wreck")?);
    }
    for suffix in rider_pose::Landing::SUFFIXES {
        selected.push(find(format!("{prefix}_{suffix}"), "landing")?);
    }
    Ok(selected)
}

/// Loads the ride for `quad` (1-26) and `rider` (0 male, 1 female) on a
/// venue already read by [`crate::venue::load`].
pub async fn load(
    content: &Content,
    venue: &Venue,
    quad: u16,
    rider: u16,
    progress: impl FnMut(&str),
) -> Result<Ride> {
    load_with(content, venue, quad, rider, None, progress).await
}

/// A Freeride, or with `race` a race on the venue's `*_NAT.RES` course
/// (the venue must have been loaded for racing).
pub async fn load_with(
    content: &Content,
    venue: &Venue,
    quad: u16,
    rider: u16,
    race: Option<super::race::Settings>,
    mut progress: impl FnMut(&str),
) -> Result<Ride> {
    let environment = venue.environment.as_str();
    let course = course_resource(environment, race.is_some())?;
    let models = player_models::selection(quad, rider)?;
    let course_bytes = content.file(&course.archive).await?;
    let archive = Resource::parse(&course_bytes)?;
    let (seed, vehicle_model, rider_model) = if models.motorcycle {
        progress("Reading original motorcycle…");
        // Op 100: the MX archive holds the bike, its rider, motions and
        // textures (atv_core::motorcycle::Model).
        let bytes = content.file(&models.quad).await?;
        let model = atv_core::motorcycle::Model::load(&bytes, quad)?;
        let archive = Resource::parse(&bytes)?;
        // Drawn as atv-web op 102 returns them: the material-remapped bike
        // mesh and the MX rider, textured from the same archive.
        let vehicle = Model {
            textures: textures::load(&model.mesh, &archive)?,
            mesh: model.mesh.clone(),
        };
        let rider = Model {
            textures: textures::load(&model.rider, &archive)?,
            mesh: model.rider.clone(),
        };
        (Seed::Bike(Box::new(model)), vehicle, rider)
    } else {
        load_quad(content, &models, &mut progress).await?
    };

    // Terrain materials: the course's <venue>.toy, else the Endless archive's.
    let toy = match basename_index(&archive, &format!("{environment}.toy")) {
        Some(index) => archive.extract(index)?,
        None => {
            let path = course
                .placements
                .as_deref()
                .ok_or("original terrain materials are missing")?;
            let bytes = content.file(path).await?;
            member(
                &Resource::parse(&bytes)?,
                &format!("{environment}.toy"),
                "terrain materials",
            )?
        }
    };

    progress("Reading original scene collisions…");
    let terrain =
        terrain::decode_tr2(&member(&archive, &format!("{environment}.tr2"), "terrain")?)?;
    let scene = &venue.scene;
    let water = match &scene.water_file {
        None => None,
        Some(file) => {
            let index =
                basename_index(&archive, file).ok_or("original water definition is missing")?;
            let name = &archive.members[index].name;
            let (directory, _) = name
                .rsplit_once('/')
                .ok_or("water resource has no directory")?;
            let definition = water_animation::Definition::parse(&archive.extract(index)?)?;
            Some(water_animation::LiveField::new(
                definition,
                &format!("{directory}/Sigs"),
                |path| {
                    archive.extract(
                        archive
                            .members
                            .iter()
                            .position(|m| m.name.eq_ignore_ascii_case(path))
                            .ok_or_else(|| format!("missing water height resource {path}"))?,
                    )
                },
            )?)
        }
    };
    let ecosystem = basename_index(&archive, &format!("{environment}.esb"))
        .map(|index| archive.extract(index).and_then(|b| ecosystem::decode(&b)))
        .transpose()?;
    // 00206e90 ignores missing COL files; authored non-solid overlays have none.
    let mut meshes: Vec<(String, Rc<collision::CollisionMesh>)> = Vec::new();
    let mut obstacles = Vec::new();
    let mut mud_obstacles = Vec::new();
    let mud_prefix = format!("{}mud", environment.to_ascii_lowercase());
    for placement in &scene.collision_models {
        let name = placement.name.to_ascii_lowercase();
        let Some(index) = member_path(&archive, &format!("geometry/{name}.col")) else {
            continue;
        };
        let mesh = match meshes.iter().find(|(n, _)| *n == name) {
            Some((_, mesh)) => mesh.clone(),
            None => {
                let mesh = collision::decode_col(&archive.extract(index)?)?;
                if mesh.transform != IDENTITY_COL {
                    return Err("unsupported static collision header transform".into());
                }
                let mesh = Rc::new(mesh);
                meshes.push((name.clone(), mesh.clone()));
                mesh
            }
        };
        let stem = name.rsplit(['/', '\\']).next().unwrap_or(&name);
        mud_obstacles.push(stem.starts_with(&mud_prefix));
        obstacles.push(StaticObstacle {
            mesh,
            world_matrix: placement.world_matrix,
        });
    }
    let presets = camera::presets(
        &member(&archive, "followcampresets.pb", "camera presets")?,
        0,
    )?;
    // 00319c10 -> 003007b0 supplies 70 MPH and the original ft/s conversion.
    let follow = camera::FollowCamera::new(presets, 70. * f32::from_bits(0x3fbbbb84))?;

    let race = match race {
        Some(settings) => {
            progress("Reading original race…");
            let mut setup =
                race_setup(content, &archive, venue, &toy, quad, rider, settings).await?;
            // MX vs. ATV fields: a motorcycle player's CPUs keep their quads
            // and the ATV rider (game.js loads it for them).
            if models.motorcycle {
                let atv = player_models::selection(1, rider)?;
                let (mesh, _, animator, wreck) =
                    load_atv_rider(content, &atv, &mut progress).await?;
                setup.cpu_rider = Some(Box::new(super::CpuRider {
                    animator,
                    wreck,
                    mesh,
                }));
            }
            Some((setup, settings))
        }
        None => None,
    };
    // Op 140: scene starts keep their authored height (single player, no
    // slot); a race places the grid behind the original TDF start gate, 2.5ft
    // above the contact plane.
    let (position, yaw) = match &race {
        Some((setup, _)) => {
            let (position, yaw) = setup.course.start();
            (position.map(js), js(yaw))
        }
        None => {
            let direction = scene.start_direction;
            (
                scene.start_position.map(js),
                js(direction[0]).atan2_d(js(direction[2])),
            )
        }
    };
    let origin = atv_core::spawn::start_origin(&terrain, position, yaw, race.is_some(), None)?;
    let start = (origin.map(|v| v as f32), yaw as f32);

    let simulation = Simulation {
        materials: materials::terrain_materials(&toy)?,
        terrain,
        obstacles,
        mud_obstacles,
        scene_partition: scene.collision_partition,
        ecosystem,
        boundary: scene.boundary_launch,
        extended_boundary: scene.crowd_present || environment.eq_ignore_ascii_case("training"),
        water,
        camera: follow,
        // Freeride only: race venues load no train.
        train: if race.is_some() {
            None
        } else {
            venue.train.clone()
        },
    };
    Ride::start(
        simulation,
        super::Selection { quad, rider },
        start,
        seed,
        vehicle_model,
        rider_model,
        race,
    )
}

/// The ATV, its rider rig and motions (ops 20-22, 44, 48, 51-53).
async fn load_quad(
    content: &Content,
    models: &player_models::Models,
    progress: &mut impl FnMut(&str),
) -> Result<(Seed, Model, Model)> {
    progress("Reading original ATV…");
    let quad_bytes = content.file(&models.quad).await?;
    let quad_archive = Resource::parse(&quad_bytes)?;
    let mesh_bytes = member(&quad_archive, &models.quad_mesh, "ATV mesh")?;
    let quad_mesh = mesh::decode_psg(&mesh_bytes)?;
    let physics = PhysicsData::parse(&member(&quad_archive, &models.physics, "ATV physics")?)?;
    let chassis = collision::decode_ray_tree(&member(
        &quad_archive,
        &models.chassis,
        "chassis collision",
    )?)?;
    let actor = collision::decode_ray_tree(&member(
        &quad_archive,
        &models.actor,
        "actor collision rays",
    )?)?;
    if actor.transform != IDENTITY {
        return Err("unsupported actor collision header transform".into());
    }
    let material_bytes = content.file(&models.quad_materials).await?;
    let quad_textures = textures::load(&quad_mesh, &Resource::parse(&material_bytes)?)?;
    let (rider_mesh, rider_textures, animator, wreck) =
        load_atv_rider(content, models, progress).await?;
    let rig = QuadRig {
        physics,
        quad_pose: Rc::new(QuadPose::new(&quad_mesh)?),
        chassis_points: chassis.chassis_points()?,
        wreck_rays: Rc::new(chassis),
        obstacle_rays: Rc::new(actor),
        animator,
        wreck,
    };
    Ok((
        Seed::Quad(Box::new(rig)),
        Model {
            mesh: quad_mesh,
            textures: quad_textures,
        },
        Model {
            mesh: rider_mesh,
            textures: rider_textures,
        },
    ))
}

/// The ATV2 rider rig and motions (op 27 and ops 51-53, `loadAtvRider`): the
/// player's on an ATV, or race CPUs' beside a motorcycle player.
async fn load_atv_rider(
    content: &Content,
    models: &player_models::Models,
    progress: &mut impl FnMut(&str),
) -> Result<(
    mesh::Mesh,
    std::collections::HashMap<String, textures::MaterialTexture>,
    rider_pose::Animator,
    wreck_animation::Animator,
)> {
    progress("Reading original rider…");
    let rider_bytes = content.file(&models.rider).await?;
    let rider_archive = Resource::parse(&rider_bytes)?;
    let rider_mesh = mesh::decode_psg(&member_suffix(
        &rider_archive,
        &format!("/{}", models.rider_mesh),
    )?)?;
    let catalog = MotionCatalog::parse(&member_suffix(
        &rider_archive,
        &format!("/{}", models.rider_catalog),
    )?)?;
    let selected = riding_motions(&catalog)?;
    let motion_bytes = content.file("/ANIMSRDR.RES").await?;
    let motion_archive = Resource::parse(&motion_bytes)?;
    let mut motions = Vec::with_capacity(selected.len());
    for definition in &selected {
        let file = definition
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&definition.path);
        motions.push(rider_pose::Motion {
            animation: animation::decode_mot(&member_suffix(
                &motion_archive,
                &format!("/{file}"),
            )?)?,
            sampling: animation::Sampling {
                translation_interpolation: definition.interpolate_position,
                rotation_interpolation: definition.interpolate_rotation,
            },
        });
    }
    let rider_material_bytes = content.file("/RIDER/RIDER_MT.RES").await?;
    let rider_textures = textures::load(&rider_mesh, &Resource::parse(&rider_material_bytes)?)?;

    // Op 53: normal, stunt (14..62), wreck (62..76) and landing (76..78) clips.
    if motions.len() != 78 {
        return Err("load all normal, stunt, wreck and landing motions first".into());
    }
    let landings = motions.split_off(76);
    let wrecks = motions.split_off(62);
    let stunts = motions.split_off(14);
    let wreck = wreck_animation::Animator::new(
        wrecks,
        selected[62..76].iter().map(|m| m.looping).collect(),
        &rider_mesh.nodes,
    )?;
    let mut animator = rider_pose::Animator::new(motions, &rider_mesh.nodes)?;
    animator.set_landing_motions(landings)?;
    // 002de690 identifies mode 11 as Freeride/Freestyle.
    animator.set_scoring_mode(Some(stunt_score::FREESTYLE_MODE))?;
    animator.set_stunts(stunts, selected[14..62].iter().map(|m| m.looping).collect())?;
    Ok((rider_mesh, rider_textures, animator, wreck))
}

/// The race course, intro shots, trackside cameras and the CPU roster with
/// each CPU's own quad (terrainFromContent's race branch and op 106).
async fn race_setup(
    content: &Content,
    archive: &Resource<'_>,
    venue: &Venue,
    common_toy: &[u8],
    quad: u16,
    rider: u16,
    settings: super::race::Settings,
) -> Result<super::race::Setup> {
    let environment = venue.environment.as_str();
    let course = atv_core::race::Course::parse(&member(
        archive,
        &format!("{environment}nate.tdf"),
        "National course",
    )?)?;
    // After the finish a director cuts between these trackside cameras.
    let tv = basename_index(archive, &format!("{environment}nat.tv"))
        .map(|index| {
            let bytes = archive.extract(index)?;
            atv_core::race_director::parse_tv(
                std::str::from_utf8(&bytes).map_err(|_| "TV cameras must be text")?,
            )
        })
        .transpose()?;
    let mode = basename_index(archive, &format!("{environment}nat.toy"))
        .map(|index| archive.extract(index))
        .transpose()?
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    // Original roster (00210d20/0021f680): CPU quads from the course or the
    // player's ATV group, and AI riders with their names and outfits.
    let riders = member(archive, "airiders.pb", "CPU roster")?;
    let roster = match basename_index(archive, "atvgroups.pb") {
        Some(index) => {
            let champ = mode
                .as_deref()
                .and_then(|text| {
                    text.lines().find_map(|line| {
                        let (key, value) = line.split_once('=')?;
                        key.trim()
                            .eq_ignore_ascii_case("ChampQuadGroup")
                            .then(|| {
                                let value = value.trim_start();
                                let end = value
                                    .char_indices()
                                    .find(|(i, c)| !(c.is_ascii_digit() || (*i == 0 && *c == '-')))
                                    .map_or(value.len(), |(i, _)| i);
                                value[..end].parse::<i64>().ok()
                            })
                            .flatten()
                    })
                })
                .unwrap_or(-1);
            atv_core::race_roster::select(
                &archive.extract(index)?,
                &riders,
                atv_core::motorcycle::roster_quad(u32::from(quad)),
                rider == 1,
                champ,
                4,
                settings.seed,
            )?
        }
        None => Vec::new(),
    };
    if roster.len() < settings.cpu {
        return Err("original CPU roster is incomplete".into());
    }
    let mut models = Vec::with_capacity(roster.len());
    let mut draw = Vec::with_capacity(roster.len());
    let rider_materials = content.file("/RIDER/RIDER_MT.RES").await?;
    let rider_materials = Resource::parse(&rider_materials)?;
    for entry in &roster {
        let selection = player_models::selection(entry.quad as u16, rider)?;
        let bytes = content.file(&selection.quad).await?;
        let quad_archive = Resource::parse(&bytes)?;
        let part = |name: &str| member(&quad_archive, name, "CPU quad resource");
        let mesh_bytes = part(&selection.quad_mesh)?;
        models.push(super::cpu::CpuModel::new(
            &part(&selection.physics)?,
            &mesh_bytes,
            &part(&selection.chassis)?,
            &part(&selection.actor)?,
        )?);
        // Drawn with the quad's own textures and the player's rider rig
        // in the roster outfit (remoteAssets + outfitTextures in game.js).
        let quad_mesh = mesh::decode_psg(&mesh_bytes)?;
        let material_bytes = content.file(&selection.quad_materials).await?;
        let quad_textures = textures::load(&quad_mesh, &Resource::parse(&material_bytes)?)?;
        let rider_bytes = content.file(&selection.rider).await?;
        let rider_mesh = mesh::decode_psg(&member_suffix(
            &Resource::parse(&rider_bytes)?,
            &format!("/{}", selection.rider_mesh),
        )?)?;
        let rider_textures = textures::outfit(&rider_mesh, &entry.outfit, &rider_materials)?;
        draw.push((
            Model {
                mesh: quad_mesh,
                textures: quad_textures,
            },
            Model {
                mesh: rider_mesh,
                textures: rider_textures,
            },
        ));
    }
    // Grid RiderSpacing from the race mode TOY or, for Supercross, the scene TOY.
    let spacing_sources = mode
        .into_iter()
        .chain(std::iter::once(
            String::from_utf8_lossy(common_toy).into_owned(),
        ))
        .collect();
    // CrowdPresent stadiums: crowd.aud reactions (op 115), the Pro crowd on
    // Tracks.dat Pro Supercross courses (002cb9b0).
    let crowd = if venue.scene.crowd_present {
        match basename_index(archive, "crowd.aud") {
            Some(index) => {
                let prefix = if pro_supercross(content, environment).await? {
                    "Pro"
                } else {
                    "Am"
                };
                Some(atv_core::race_crowd::parse_crowd(
                    &String::from_utf8_lossy(&archive.extract(index)?),
                    prefix,
                )?)
            }
            None => None,
        }
    } else {
        None
    };
    // Supercross fireworks are the ambient mode TOY's emitter groups.
    let fireworks = match venue.ambient.as_ref().and_then(|a| a.borrow().mode.clone()) {
        Some(mode) => atv_core::race_fireworks::parse(&mode)?,
        None => Default::default(),
    };
    Ok(super::race::Setup {
        course,
        shots: venue.intro_shots.clone(),
        tv,
        roster,
        models,
        draw,
        spacing_sources,
        cpu_rider: None,
        stage: super::race::Stage {
            crowd,
            gate: venue.drop_gate.clone(),
            fireworks,
        },
    })
}

/// Tracks.dat lists the venue under a Pro Supercross (`prosxN`) section
/// (frontend.js `proSupercross`).
async fn pro_supercross(content: &Content, environment: &str) -> Result<bool> {
    let Ok(bytes) = content.file("/STARTAPP.RES").await else {
        return Ok(false);
    };
    let archive = Resource::parse(&bytes)?;
    let Some(index) = basename_index(&archive, "tracks.dat") else {
        return Ok(false);
    };
    let document = atv_core::ui::Document::parse(&archive.extract(index)?)?;
    let mut sections: Vec<(String, Option<String>)> = Vec::new();
    for section in &document.sections {
        let name = section.name.to_ascii_lowercase();
        // sections(): the first value of each key; a later duplicate
        // section replaces the earlier one's fields in its slot.
        let scn = section
            .fields
            .iter()
            .find(|f| f.key.eq_ignore_ascii_case("scn"))
            .map(|f| f.value.clone());
        match sections.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = scn,
            None => sections.push((name, scn)),
        }
    }
    Ok(sections.iter().any(|(name, scn)| {
        name.strip_prefix("prosx")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
            && scn.as_deref() == Some(environment)
    }))
}
