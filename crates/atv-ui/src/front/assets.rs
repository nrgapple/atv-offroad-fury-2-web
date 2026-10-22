//! Original frontend resources (frontend.js `init`, `trackPhoto`,
//! `courseShots`, `loadTips` and the uiQuad.txt stats), read from the same
//! archives in the browser game's order.
use super::pixels::Rgba;
use atv_content::Content;
use atv_core::{texture, ui};
use atv_game::{archive::Archive, launch};
use std::collections::HashMap;

/// One selectable venue (frontend.js `unlockedCourses`).
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub name: String,
    /// TP photo stem (`<image>a.pst` + `<image>b.pst`).
    pub image: String,
}

/// Course photo archives, in their registration order.
pub const COURSE_PREVIEWS: [&str; 6] = [
    "/UI/RESOURCE/TP/TP_ANAT.RES",
    "/UI/RESOURCE/TP/TP_PSTNT.RES",
    "/UI/RESOURCE/TP/TP_PNAT.RES",
    "/UI/RESOURCE/TP/TP_ASX.RES",
    "/UI/RESOURCE/TP/TP_PSX.RES",
    "/UI/RESOURCE/TP/TP_HOCK.RES",
];

/// Original STARTEVT etips.txt entries about riding technique.
const RIDING_TIPS: [u32; 14] = [5, 9, 10, 11, 14, 16, 17, 18, 19, 23, 24, 25, 26, 27];

fn unquote(value: &str) -> String {
    let v = value.strip_prefix('"').unwrap_or(value);
    v.strip_suffix('"').unwrap_or(v).to_string()
}

/// Lower-cased section name and its fields.
pub type Sections = Vec<(String, HashMap<String, String>)>;

/// A UI document as frontend.js `sections()` reads it: section names
/// lower-cased (a later duplicate replaces the value in the first slot) and
/// the first value of each lower-cased key.
pub fn sections(bytes: &[u8]) -> Result<Sections, String> {
    let document = ui::Document::parse(bytes)?;
    let mut out: Sections = Vec::new();
    for section in document.sections {
        let mut fields = HashMap::new();
        for field in section.fields.iter().rev() {
            fields.insert(field.key.to_ascii_lowercase(), field.value.clone());
        }
        let name = section.name.to_ascii_lowercase();
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = fields,
            None => out.push((name, fields)),
        }
    }
    Ok(out)
}

/// frontend.js `unlockedCourses`: one entry per environment, named by its
/// first single-player/championship section, when its archive is published.
pub fn unlocked_courses(
    tracks: &[(String, HashMap<String, String>)],
    published: &dyn Fn(&str) -> bool,
) -> Vec<Track> {
    let pattern = |name: &str| {
        let digits = |rest: &str| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
        ["unspnat", "spnat", "amsx", "prosx", "spstunt"]
            .iter()
            .any(|p| name.strip_prefix(p).is_some_and(digits))
            || name == "train1"
            || name == "hoc1"
    };
    let environments = launch::environments();
    let mut source: HashMap<String, &HashMap<String, String>> = HashMap::new();
    for (section, fields) in tracks {
        if !pattern(section) {
            continue;
        }
        if let Some(scn) = fields.get("scn")
            && environments.contains(scn)
        {
            source.entry(scn.clone()).or_insert(fields);
        }
    }
    environments
        .iter()
        .filter(|id| {
            source.contains_key(*id)
                && launch::course_resource(id, false).is_ok_and(|c| published(&c.archive))
        })
        .map(|id| {
            let fields = source[id];
            Track {
                id: id.clone(),
                name: unquote(fields.get("name").map_or("", String::as_str)),
                image: if id == "training" {
                    String::new()
                } else {
                    unquote(fields.get("image").map_or("", String::as_str))
                },
            }
        })
        .collect()
}

fn pst(bytes: &[u8]) -> Result<Rgba, String> {
    let decoded = texture::decode_pst(bytes)?;
    Ok(Rgba::new(
        decoded.info.width,
        decoded.info.height,
        decoded.rgba,
    ))
}

/// What the title needs: the venue list and PAGE01's red scenic backdrop.
pub struct Boot {
    pub tracks: Vec<Track>,
    pub backdrop: Option<Rgba>,
}

pub async fn boot(content: &Content) -> Result<Boot, String> {
    let startapp = Archive::open(content, "/STARTAPP.RES").await?;
    let index = startapp
        .named("tracks.dat")
        .ok_or("original tracks.dat is missing")?;
    let tracks = sections(&startapp.member(index).await?)?;
    let tracks = unlocked_courses(&tracks, &|path| content.manifest.record(path).is_some());
    let mut backdrop = None;
    if content
        .manifest
        .record("/UI/RESOURCE/PAGEA/PAGE01.RES")
        .is_some()
    {
        let page = Archive::open(content, "/UI/RESOURCE/PAGEA/PAGE01.RES").await?;
        if let Some(index) = page.named("bk_mainmenu.pst") {
            backdrop = pst(&page.member(index).await?).ok();
        }
    }
    Ok(Boot { tracks, backdrop })
}

/// Every course photograph: the two halves joined and trimmed. Archives
/// register in order, so a name in two archives resolves to the earlier.
pub async fn photos(content: &Content, tracks: &[Track]) -> Result<Vec<(String, Rgba)>, String> {
    let mut archives = Vec::new();
    for path in COURSE_PREVIEWS {
        if content.manifest.record(path).is_some() {
            archives.push(Archive::open(content, path).await?);
        }
    }
    let find = |name: &str| {
        archives
            .iter()
            .find_map(|a| a.named(name).map(|index| (a, index)))
    };
    let mut out = Vec::new();
    for track in tracks.iter().filter(|t| !t.image.is_empty()) {
        let (Some((a, ia)), Some((b, ib))) = (
            find(&format!("{}a.pst", track.image)),
            find(&format!("{}b.pst", track.image)),
        ) else {
            continue;
        };
        let (Ok(left), Ok(right)) = (pst(&a.member(ia).await?), pst(&b.member(ib).await?)) else {
            continue;
        };
        out.push((track.id.clone(), Rgba::join(&left, &right).trim_dark()));
    }
    Ok(out)
}

/// The loader's riding tips, each a list of lines.
pub async fn tips(content: &Content) -> Result<Vec<Vec<String>>, String> {
    let archive = Archive::open(content, "/STARTEVT.RES").await?;
    let index = archive.named("etips.txt").ok_or("etips.txt is missing")?;
    let document = sections(&archive.member(index).await?)?;
    Ok(RIDING_TIPS
        .iter()
        .filter_map(|id| {
            let (_, fields) = document.iter().find(|(n, _)| *n == format!("tip{id}"))?;
            let mut lines: Vec<(u32, String)> = fields
                .iter()
                .filter_map(|(k, v)| {
                    let n = k.strip_prefix("line")?.strip_suffix("_1")?;
                    Some((n.parse().ok()?, unquote(v)))
                })
                .collect();
            lines.sort_by_key(|(n, _)| *n);
            Some(lines.into_iter().map(|(_, v)| v).collect())
        })
        .collect())
}

/// The original loader's six action shots for a course (frames 02-07).
pub async fn shots(content: &Content, environment: &str) -> Result<Vec<Rgba>, String> {
    let Some(path) = launch::loading_shots_resource(environment) else {
        return Ok(Vec::new());
    };
    if content.manifest.record(&path).is_none() {
        return Ok(Vec::new());
    }
    let archive = Archive::open(content, &path).await?;
    let mut out = Vec::new();
    for frame in 2..=7 {
        if let Some(index) = archive.named(&format!("frame0{frame}.pst"))
            && let Ok(image) = pst(&archive.member(index).await?)
        {
            out.push(image.trim_dark());
        }
    }
    Ok(out)
}

/// Original ATV select page stats (uiQuad.txt) for one IngameID.
#[derive(Clone, Debug, PartialEq)]
pub struct QuadInfo {
    pub logo: String,
    pub engine: f32,
    pub traction: f32,
    pub suspension: f32,
}

pub struct QuadCatalog {
    pub info: HashMap<u16, QuadInfo>,
    pub logos: HashMap<String, Rgba>,
}

pub async fn quad_catalog(content: &Content) -> Result<QuadCatalog, String> {
    let archive = Archive::open(content, "/UI/RESOURCE/QUADINFO.RES").await?;
    let index = archive.named("uiQuad.txt").ok_or("uiQuad.txt is missing")?;
    let mut info = HashMap::new();
    for (name, fields) in sections(&archive.member(index).await?)? {
        let Some(n) = name.strip_prefix("quad") else {
            continue;
        };
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Some(id) = fields
            .get("ingameid")
            .and_then(|v| v.trim().parse::<u16>().ok())
        else {
            continue;
        };
        let number = |key: &str| {
            fields
                .get(key)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(0.)
                .clamp(0., 1.)
        };
        info.insert(
            id,
            QuadInfo {
                logo: fields
                    .get("image")
                    .map_or(String::new(), |v| v.replace('"', "").trim().to_string()),
                engine: number("engine_range"),
                traction: number("traction_range"),
                suspension: number("suspension_range"),
            },
        );
    }
    let mut logos = HashMap::new();
    for quad in info.values() {
        let stem = quad.logo.trim_end_matches(".tga").trim_end_matches(".pst");
        if stem.is_empty() || logos.contains_key(stem) {
            continue;
        }
        if let Some(index) = archive.named(&format!("{stem}.pst"))
            && let Ok(image) = pst(&archive.member(index).await?)
        {
            logos.insert(stem.to_string(), image);
        }
    }
    Ok(QuadCatalog { info, logos })
}

/// The authored physics top speed (mph) of an ATV.
pub async fn top_speed(content: &Content, quad: u16) -> Result<f32, String> {
    let models = atv_core::player_models::selection(quad, 0)?;
    let archive = Archive::open(content, &models.quad).await?;
    let index = archive
        .named(&models.physics)
        .ok_or("ATV physics is missing")?;
    let physics = atv_core::physics_data::PhysicsData::parse(&archive.member(index).await?)?;
    Ok(atv_core::vehicle_stats::from_physics(&physics)?.top_speed)
}

/// A material texture of a preview model. `cutout` discards low alpha.
#[derive(Clone)]
pub struct ModelTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub cutout: bool,
}

/// A mesh for the menu model previews, its textures by material name and
/// node world matrices (None: the bind pose).
pub struct PreviewModel {
    pub mesh: atv_core::mesh::Mesh,
    pub textures: HashMap<String, ModelTexture>,
    pub pose: Option<Vec<[f32; 16]>>,
}

pub fn ride_textures(
    textures: HashMap<String, atv_game::ride::textures::MaterialTexture>,
) -> HashMap<String, ModelTexture> {
    textures
        .into_iter()
        .filter_map(|(name, t)| {
            let image = t.image?;
            Some((
                name,
                ModelTexture {
                    width: image.width,
                    height: image.height,
                    rgba: image.rgba,
                    cutout: true,
                },
            ))
        })
        .collect()
}

/// The garage vehicle and, for a motorcycle, its MX rider with the original
/// selection clips (selectionAssets and the MX menuRiderAssets).
pub async fn vehicle_models(
    content: &Content,
    quad: u16,
) -> Result<(PreviewModel, Option<MenuRider>), String> {
    use atv_core::resource::Resource;
    let models = atv_core::player_models::selection(quad, 0)?;
    let bytes = content.file(&models.quad).await?;
    if models.motorcycle {
        let model = atv_core::motorcycle::Model::load(&bytes, quad)?;
        let archive = Resource::parse(&bytes)?;
        let vehicle = PreviewModel {
            textures: ride_textures(atv_game::ride::textures::load(&model.mesh, &archive)?),
            mesh: model.mesh.clone(),
            pose: Some(model.pose(0., [0., 0.], [0., 0.]).to_vec()),
        };
        let textures = ride_textures(atv_game::ride::textures::load(&model.rider, &archive)?);
        // atv-web `motorcycles::menu`: breathing between each selection idle.
        let mut clips = Vec::new();
        for idle in 0..=3 {
            let breathe = "ui_rider_select_breathing";
            clips.push((
                format!("{breathe}.mot"),
                model
                    .motions
                    .get(breathe)
                    .ok_or("missing MX menu breathing")?
                    .clone(),
            ));
            if idle > 0 {
                let name = format!("ui_rider_select_idle{idle}");
                clips.push((
                    format!("{name}.mot"),
                    model
                        .motions
                        .get(&name)
                        .ok_or("missing MX menu idle")?
                        .clone(),
                ));
            }
        }
        let rider = atv_core::menu_preview::Rider::from_clips(model.rider.clone(), clips)?;
        // MX selection clips face the opposite way to the ATV menu rig.
        let yaw = -(std::f32::consts::PI - 0.6);
        return Ok((
            vehicle,
            Some(MenuRider {
                rider,
                textures,
                yaw,
            }),
        ));
    }
    let archive = Resource::parse(&bytes)?;
    let index = archive
        .members
        .iter()
        .position(|m| {
            m.name
                .rsplit('/')
                .next()
                .is_some_and(|n| n.eq_ignore_ascii_case(&models.quad_mesh))
        })
        .ok_or("ATV mesh is missing")?;
    let mesh = atv_core::mesh::decode_psg(&archive.extract(index)?)?;
    let materials = content.file(&models.quad_materials).await?;
    let textures = ride_textures(atv_game::ride::textures::load(
        &mesh,
        &Resource::parse(&materials)?,
    )?);
    Ok((
        PreviewModel {
            mesh,
            textures,
            pose: None,
        },
        None,
    ))
}

/// The original frontend rider (PAGE07: mesh, breathing/idle clips and its
/// opaque UI textures) for an ATV garage (menuRiderAssets).
pub struct MenuRider {
    pub rider: atv_core::menu_preview::Rider,
    pub textures: HashMap<String, ModelTexture>,
    /// Preview turn (the shader's yaw).
    pub yaw: f32,
}

pub async fn menu_rider(content: &Content, female: bool) -> Result<MenuRider, String> {
    use atv_core::resource::Resource;
    let bytes = content.file("/UI/RESOURCE/PAGEA/PAGE07.RES").await?;
    let rider = atv_core::menu_preview::Rider::from_resource(&bytes, female)?;
    let archive = Resource::parse(&bytes)?;
    let mut textures = HashMap::new();
    for (index, member) in archive.members.iter().enumerate() {
        let name = member
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&member.name)
            .to_lowercase();
        let Some(stem) = name.strip_suffix(".pst") else {
            continue;
        };
        if let Ok(image) = pst(&archive.extract(index)?) {
            textures.insert(
                stem.to_string(),
                ModelTexture {
                    width: image.width,
                    height: image.height,
                    rgba: image.data,
                    cutout: false,
                },
            );
        }
    }
    if let Some(shirt) = textures.get("atv2_shirt_ui").cloned() {
        textures.insert("atv2_shirt_ui_export".into(), shirt);
    }
    Ok(MenuRider {
        rider,
        textures,
        yaw: 0.6,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn courses_follow_the_first_matching_section() {
        let doc = b"[spnat1]\r\nname = \"Tioga Valley\"\r\nscn = env02\r\nimage = tp_e02\r\n[unspnat2]\r\nname = \"Other\"\r\nscn = env02\r\n[spnat3]\r\nname = \"Caribou Peak\"\r\nscn = env01\r\nimage = tp_e01\r\n[career1]\r\nscn = env03\r\n";
        let tracks = sections(doc).unwrap();
        let list = unlocked_courses(&tracks, &|_| true);
        assert_eq!(
            list.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["env01", "env02"]
        );
        assert_eq!(list[1].name, "Tioga Valley");
        assert_eq!(list[0].image, "tp_e01");
    }
}
