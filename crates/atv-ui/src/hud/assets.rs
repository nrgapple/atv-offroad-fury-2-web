//! Original HUD resources (web/hud.js `init`): the OverlayObject-5 speed and
//! preload meter, the OverlayObject-12 stunt text, their CEL font metrics and
//! icon textures, read from the same archives in the browser game's order.
use atv_content::Content;
use atv_core::{texture, ui};
use atv_game::archive::Archive;
use atv_game::launch::RaceVariant;
use std::collections::HashMap;

pub const FONT_DEFAULT: &str = "tradegothic_boldcondensed_22s";

/// One `[OverlayObject-N]` section's fields (keys lower-cased).
#[derive(Clone, Debug, Default)]
pub struct Section {
    pub fields: HashMap<String, String>,
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }
    pub fn text(&self, key: &str) -> Option<String> {
        self.get(key)
            .map(|v| v.trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
    }
    pub fn number(&self, key: &str) -> Option<f32> {
        self.get(key)?.trim().parse().ok()
    }
    pub fn pair(&self, key: &str) -> Option<[f32; 2]> {
        let mut it = self
            .get(key)?
            .split(',')
            .map(|v| v.trim().parse::<f32>().ok());
        Some([it.next()??, it.next()??])
    }
}

/// CEL font: per-code advance (metrics[0] + metrics[2]) and glyph height.
#[derive(Clone, Debug, Default)]
pub struct CelFont {
    pub advances: HashMap<u32, f32>,
    pub heights: HashMap<u32, f32>,
}

impl CelFont {
    /// hud.js `text`: summed original advances and the tallest glyph.
    pub fn measure(&self, text: &str) -> (f32, f32) {
        let mut width = 0.;
        let mut height = 1f32;
        let mut any = false;
        for c in text.chars() {
            if let Some(a) = self.advances.get(&(c as u32)) {
                width += a;
                height = height.max(self.heights.get(&(c as u32)).copied().unwrap_or(25.));
                any = true;
            }
        }
        (width, if any { height } else { 1. })
    }
}

pub struct HudImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct OverlayAssets {
    pub speed: Section,
    pub stunt: Section,
    pub fonts: HashMap<String, CelFont>,
    pub images: HashMap<String, HudImage>,
    /// Race start 321 LIGHTS art (`321ron`, `321roff`, `321gon`, `321goff`):
    /// race and Freeride course archives carry it; Training does not.
    pub lamps: bool,
    /// OverlaySet1 single-player race overlays (races only).
    pub race: Option<RaceAssets>,
    /// The split-screen layouts (`OverlaySet<n>-ScreenLayout-<layout>`)
    /// by overlay set and layout name.
    pub split: HashMap<(u32, &'static str), SplitLayout>,
}

/// The overlay sets whose split layouts the port draws: 1 Race, 2 Stunt
/// (Freeride and Freestyle), 6 Treasure Hunt, 7 King of the Hills, 8 Tag,
/// 9 Hockey and 11 Enduro.
pub const SPLIT_SETS: [u32; 7] = [1, 2, 6, 7, 8, 9, 11];
/// The split layouts' section names (`003029a0`'s layout enum 1-3 and the
/// three-player variant of 3).
pub const SPLIT_LAYOUTS: [&str; 4] = ["2Player-Horiz", "2Player-Vertical", "3Player", "4Player"];
/// The objects one split layout places per player: the multiplayer lap
/// time, lap, score, infotext and stunt text, both speed meters, the
/// compass and the place number (the signage, object 35, is listed too).
pub const SPLIT_OBJECTS: [u32; 12] = [5, 12, 19, 20, 21, 23, 24, 31, 33, 35, 41, 10];

/// One overlay of a split layout placed for one player: the object, its
/// position on the 640x448 frame and the object's section with the set's
/// texts and that player's justification overrides applied.
#[derive(Clone, Debug)]
pub struct Placed {
    pub object: u32,
    pub at: [f32; 2],
    pub section: Section,
}

/// `OverlaySet<n>-ScreenLayout-<layout>`: each player's overlays (in the
/// layout's player order) and the single overlays the whole frame shares.
#[derive(Clone, Debug, Default)]
pub struct SplitLayout {
    pub players: Vec<Vec<Placed>>,
    pub shared: Vec<Placed>,
}

/// Every `[Section]` of a document, sliced once (keys lower-cased).
pub fn sections(bytes: &[u8]) -> HashMap<String, Section> {
    let text: String = bytes.iter().map(|&b| char::from(b)).collect();
    let mut starts = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']).trim_end();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed[1..].contains('[') {
            starts.push((at, trimmed[1..trimmed.len() - 1].to_ascii_lowercase()));
        }
        at += line.len();
    }
    let mut out = HashMap::new();
    for (i, (begin, name)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map_or(bytes.len(), |s| s.0);
        let Ok(document) = ui::Document::parse(&bytes[*begin..end]) else {
            continue;
        };
        let Some(first) = document.sections.first() else {
            continue;
        };
        let fields = first
            .fields
            .iter()
            .rev()
            .map(|f| (f.key.to_ascii_lowercase(), f.value.clone()))
            .collect();
        out.entry(name.clone()).or_insert(Section { fields });
    }
    out
}

/// The split layouts of [`SPLIT_SETS`] (`002edf00` reads
/// `NumOverlays-ScreenLayout-<layout>` and each `Overlay<n>` section's
/// `OverlayObject`, `NumOverlay` positions and `Overlay<k>Text<j>
/// Justification` overrides under `AlterJustification`).
pub fn split_layouts(all: &HashMap<String, Section>) -> HashMap<(u32, &'static str), SplitLayout> {
    let mut out = HashMap::new();
    for set in SPLIT_SETS {
        let Some(head) = all.get(&format!("overlayset{set}")) else {
            continue;
        };
        for name in SPLIT_LAYOUTS {
            let count = head
                .number(&format!("numoverlays-screenlayout-{name}"))
                .unwrap_or(0.) as usize;
            let mut layout = SplitLayout::default();
            for i in 1..=count {
                let key =
                    format!("overlayset{set}-screenlayout-{name}-overlay{i}").to_ascii_lowercase();
                let Some(overlay) = all.get(&key) else {
                    continue;
                };
                let Some(object) = overlay.number("overlayobject").map(|n| n as u32) else {
                    continue;
                };
                if !SPLIT_OBJECTS.contains(&object) {
                    continue;
                }
                let Some(base) = all.get(&format!("overlayobject-{object}")) else {
                    continue;
                };
                let n = overlay.number("numoverlay").unwrap_or(0.) as usize;
                for k in 1..=n {
                    let Some(at) = overlay.pair(&format!("overlay{k}")) else {
                        continue;
                    };
                    let mut section = base.clone();
                    let own = format!("overlay{k}");
                    for (field, value) in &overlay.fields {
                        if let Some(rest) = field.strip_prefix(&own)
                            && rest.starts_with("text")
                        {
                            section.fields.insert(rest.to_string(), value.clone());
                        } else if field.starts_with("text") {
                            section.fields.insert(field.clone(), value.clone());
                        }
                    }
                    let placed = Placed {
                        object,
                        at,
                        section,
                    };
                    if object == 10 {
                        layout.shared.push(placed);
                    } else {
                        if layout.players.len() < k {
                            layout.players.resize(k, Vec::new());
                        }
                        layout.players[k - 1].push(placed);
                    }
                }
            }
            out.insert((set, name), layout);
        }
    }
    out
}

/// web/hud.js `initRace`: the race layouts, sign icons, the radar map from
/// the race TOY and the radar racer icon.
pub struct RaceAssets {
    /// `OverlayObject-N` for N in [`RACE_OBJECTS`].
    pub layout: HashMap<u32, Section>,
    /// `Single-Sign-N` by sign id.
    pub signs: HashMap<u8, Section>,
    /// `Multi-Sign-N` by sign id: the split layouts' signage (object 35,
    /// `SignageName=Multi-`).
    pub multi_signs: HashMap<u8, Section>,
    /// Radar map image name and its world bounds (left, top, right, bottom);
    /// none on Enduro courses, whose mode TOYs have no [Radar].
    pub radar: Option<(String, [f32; 4])>,
}

/// The race overlay objects hud.js draws.
/// Object 40 is the Enduro gate indicator (Overlay Set 11).
pub const RACE_OBJECTS: [u32; 9] = [1, 2, 3, 4, 7, 11, 14, 40, 41];
/// Sign ids and their sections (hud.js `SIGNS`).
pub const SIGNS: [(u8, &str); 7] = [
    (1, "Single-Sign-1"),
    (2, "Single-Sign-2"),
    (3, "Single-Sign-3"),
    (12, "Single-Sign-12"),
    (13, "Single-Sign-13"),
    (14, "Single-Sign-14"),
    (15, "Single-Sign-15"),
];
pub const LAMPS: [&str; 4] = ["321ron", "321roff", "321gon", "321goff"];
/// The Enduro compass (Overlay Set 11 object 31, `QuadzillaCompass`): its
/// face, the player's arrow and the next-gate icon.
pub const COMPASS: [&str; 3] = ["compassbkg", "compassarrow", "radaricon_gate"];
/// The radar racer icon; the player's copy is tinted red (`tinted`).
pub const RACER: &str = "radaricon_racer";
pub const RACER_PLAYER: &str = "radaricon_racer#255,0,0";
/// King of the Hills' hill score pill (`002eac20`): only the KH game
/// archives carry it.
pub const PILL: &str = "koh_pillbkg";
/// The compass icon each mode passes `002e68e0` (`002edf00`): a Freestyle
/// target run's untouched cherries, King of the Hills' hills and Treasure
/// Hunt's live treasures. Only that mode's archive carries its icon.
pub const RADAR_TARGET: &str = "radaricon_target";
pub const RADAR_HILL: &str = "radaricon_hill";
pub const RADAR_FLAG: &str = "radaricon_flag";

/// `<icon>#<slot>`: an icon tinted with a participant colour (`002e76e0`
/// colours each hill by its owner's slot).
pub fn tinted_name(icon: &str, slot: u8) -> String {
    format!("{icon}#{slot}")
}

/// hud.js `tinted`: the icon multiplied by `color`, keeping its alpha.
pub fn tinted(image: &HudImage, color: crate::color::Color) -> HudImage {
    let mut rgba = image.rgba.clone();
    for px in rgba.as_chunks_mut::<4>().0 {
        px[0] = (f32::from(px[0]) * color.r).round() as u8;
        px[1] = (f32::from(px[1]) * color.g).round() as u8;
        px[2] = (f32::from(px[2]) * color.b).round() as u8;
    }
    HudImage {
        width: image.width,
        height: image.height,
        rgba,
    }
}

/// The section's original bytes (hud.js/frontend.js `document(name, section)`):
/// Overlays.pb has a stray authoring note outside the sections.
pub fn section(bytes: &[u8], name: &str) -> Option<Section> {
    let text: String = bytes.iter().map(|&b| char::from(b)).collect();
    let mut starts = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']).trim_end();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed[1..].contains('[') {
            starts.push((at, trimmed[1..trimmed.len() - 1].to_string()));
        }
        at += line.len();
    }
    let index = starts
        .iter()
        .position(|(_, n)| n.eq_ignore_ascii_case(name))?;
    let begin = starts[index].0;
    let end = starts.get(index + 1).map_or(bytes.len(), |s| s.0);
    let document = ui::Document::parse(&bytes[begin..end]).ok()?;
    let fields = document
        .sections
        .first()?
        .fields
        .iter()
        .rev()
        .map(|f| (f.key.to_ascii_lowercase(), f.value.clone()))
        .collect();
    Some(Section { fields })
}

/// Archives in the order the browser registers member names; the first
/// archive holding a name wins. A race adds its course archive last; a
/// mini-game adds its game archive (`E05_KH.RES`) after the overlays.
fn archives(environment: &str, race: Option<RaceVariant>, game: Option<&str>) -> Vec<String> {
    let mut list = vec![
        "/STARTAPP.RES".to_string(),
        "/UI/RESOURCE/PAGEA/PAGE01.RES".to_string(),
        "/STARTEVT.RES".to_string(),
    ];
    if let Ok(course) = atv_game::launch::course_resource(environment, false) {
        list.push(course.archive);
    }
    list.push("/OVERLAYS.RES".to_string());
    list.extend(game.map(str::to_string));
    if let Some(variant) = race
        && let Ok(course) = atv_game::launch::race_resource(environment, variant)
    {
        list.push(course.archive);
        list.extend(course.overlay);
    }
    list
}

struct Opened<'c> {
    archives: Vec<Archive<'c>>,
}

impl Opened<'_> {
    fn find(&self, name: &str) -> Option<(usize, usize)> {
        self.archives
            .iter()
            .enumerate()
            .find_map(|(a, archive)| archive.named(name).map(|m| (a, m)))
    }

    async fn member(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        match self.find(name) {
            Some((a, m)) => self.archives[a].member(m).await.map(Some),
            None => Ok(None),
        }
    }

    async fn image(
        &self,
        images: &mut HashMap<String, HudImage>,
        name: &str,
    ) -> Result<bool, String> {
        let stem = name
            .trim_end_matches(".tga")
            .trim_end_matches(".pst")
            .to_string();
        if images.contains_key(&stem) {
            return Ok(true);
        }
        let Some(bytes) = self.member(&format!("{stem}.pst")).await? else {
            return Ok(false);
        };
        let decoded = texture::decode_pst(&bytes)?;
        images.insert(
            stem,
            HudImage {
                width: decoded.info.width,
                height: decoded.info.height,
                rgba: decoded.rgba,
            },
        );
        Ok(true)
    }

    /// The layout's CEL fonts and icon textures.
    async fn section_assets(
        &self,
        layout: &Section,
        fonts: &mut HashMap<String, CelFont>,
        images: &mut HashMap<String, HudImage>,
    ) -> Result<(), String> {
        for i in 1..=layout.number("numtext").unwrap_or(0.) as usize {
            let name = layout
                .text(&format!("text{i}font"))
                .unwrap_or_else(|| FONT_DEFAULT.into())
                .to_ascii_lowercase();
            if fonts.contains_key(&name) {
                continue;
            }
            let Some(bytes) = self.member(&format!("{name}.cel")).await? else {
                continue;
            };
            let cel = ui::Font::decode(&bytes)?;
            let mut font = CelFont::default();
            for g in &cel.glyphs {
                if let Some(metrics) = g.metrics {
                    font.advances
                        .insert(g.code, (metrics[0] + metrics[2]) as f32);
                    font.heights.insert(g.code, metrics[3] as f32);
                }
            }
            fonts.insert(name, font);
        }
        for i in 1..=layout.number("numicons").unwrap_or(0.) as usize {
            if let Some(name) = layout.text(&format!("icon{i}texture")) {
                self.image(images, &name).await?;
            }
        }
        Ok(())
    }
}

pub async fn load(
    content: &Content,
    environment: &str,
    race: Option<RaceVariant>,
    game: Option<&str>,
) -> Result<OverlayAssets, String> {
    let mut opened = Opened {
        archives: Vec::new(),
    };
    for path in archives(environment, race, game) {
        if content.manifest.record(&path).is_some() {
            opened.archives.push(Archive::open(content, &path).await?);
        }
    }
    let overlays = opened
        .member("Overlays.pb")
        .await?
        .ok_or("original Overlays.pb is missing")?;
    let all = sections(&overlays);
    let speed = all
        .get("overlayobject-5")
        .cloned()
        .ok_or("OverlayObject-5 missing")?;
    let stunt = all
        .get("overlayobject-12")
        .cloned()
        .ok_or("OverlayObject-12 missing")?;
    let split = split_layouts(&all);
    let mut fonts = HashMap::new();
    let mut images = HashMap::new();
    for layout in [&speed, &stunt] {
        opened
            .section_assets(layout, &mut fonts, &mut images)
            .await?;
    }
    // The split layouts' multiplayer objects (fonts and meter icons).
    for n in [19, 20, 21, 23, 24, 33] {
        if let Some(layout) = all.get(&format!("overlayobject-{n}")) {
            opened
                .section_assets(layout, &mut fonts, &mut images)
                .await?;
        }
    }
    let mut lamps = true;
    for name in LAMPS {
        lamps &= opened.image(&mut images, name).await?;
    }
    opened.image(&mut images, PILL).await?;
    // Target runs and mini-games show the compass (Overlay Sets 5-7).
    if game.is_some() {
        for name in COMPASS
            .into_iter()
            .chain([RACER, RADAR_TARGET, RADAR_HILL, RADAR_FLAG])
        {
            opened.image(&mut images, name).await?;
        }
        if let Some(hill) = images.get(RADAR_HILL) {
            let copies: Vec<_> = (0..4)
                .map(|slot| {
                    (
                        tinted_name(RADAR_HILL, slot),
                        tinted(hill, super::peers::color(slot)),
                    )
                })
                .collect();
            images.extend(copies);
        }
    }
    let race = if let Some(variant) = race {
        match race_assets(
            &opened,
            &overlays,
            environment,
            variant,
            &mut fonts,
            &mut images,
        )
        .await
        {
            Ok(race) => Some(race),
            Err(error) => {
                // The browser keeps riding without the race overlays.
                eprintln!("race overlays: {error}");
                None
            }
        }
    } else {
        None
    };
    Ok(OverlayAssets {
        speed,
        stunt,
        fonts,
        images,
        lamps,
        race,
        split,
    })
}

async fn race_assets(
    opened: &Opened<'_>,
    overlays: &[u8],
    environment: &str,
    variant: RaceVariant,
    fonts: &mut HashMap<String, CelFont>,
    images: &mut HashMap<String, HudImage>,
) -> Result<RaceAssets, String> {
    let mut layout = HashMap::new();
    for n in RACE_OBJECTS {
        let name = format!("OverlayObject-{n}");
        let found = section(overlays, &name).ok_or(format!("{name} missing"))?;
        opened.section_assets(&found, fonts, images).await?;
        layout.insert(n, found);
    }
    let mut signs = HashMap::new();
    let mut multi_signs = HashMap::new();
    for (id, name) in SIGNS {
        let found = section(overlays, name).ok_or(format!("{name} missing"))?;
        opened.section_assets(&found, fonts, images).await?;
        signs.insert(id, found);
        let multi = name.replace("Single-", "Multi-");
        if let Some(found) = section(overlays, &multi) {
            opened.section_assets(&found, fonts, images).await?;
            multi_signs.insert(id, found);
        }
    }
    let toy = opened
        .member(&format!("{environment}{}.toy", variant.stem()))
        .await?
        .ok_or("race TOY missing")?;
    if variant.is_enduro() {
        for name in COMPASS.into_iter().chain([RACER]) {
            opened.image(images, name).await?;
        }
    }
    let Some(radar) = section(&toy, "Radar").filter(|_| !variant.is_enduro()) else {
        return Ok(RaceAssets {
            layout,
            signs,
            multi_signs,
            radar: None,
        });
    };
    let texture = radar.text("texture").ok_or("Radar texture missing")?;
    let stem = texture
        .trim_end_matches(".tga")
        .trim_end_matches(".pst")
        .to_string();
    if !opened.image(images, &stem).await? {
        return Err(format!("radar map {stem} missing"));
    }
    if !opened.image(images, RACER).await? {
        return Err("radar racer icon missing".into());
    }
    // hud.js `tinted`: multiply by the colour, keep the icon's alpha.
    let racer = &images[RACER];
    let mut rgba = racer.rgba.clone();
    for px in rgba.as_chunks_mut::<4>().0 {
        px[1] = 0;
        px[2] = 0;
    }
    images.insert(
        RACER_PLAYER.into(),
        HudImage {
            width: racer.width,
            height: racer.height,
            rgba,
        },
    );
    let bound = |key: &str| radar.number(key).ok_or(format!("Radar {key} missing"));
    Ok(RaceAssets {
        layout,
        signs,
        multi_signs,
        radar: Some((
            stem,
            [
                bound("left")?,
                bound("top")?,
                bound("right")?,
                bound("bottom")?,
            ],
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sections_slice_their_own_bytes() {
        let bytes =
            b"[A]\nx=1\nstray note\n[OverlayObject-5]\nNumText=2\nText1Font=\"f\"\n[B]\ny=2\n";
        let s = section(bytes, "overlayobject-5").unwrap();
        assert_eq!(s.number("numtext"), Some(2.));
        assert_eq!(s.text("Text1Font").as_deref(), Some("f"));
        assert!(section(bytes, "missing").is_none());
    }

    /// Prints the private layouts: `FURY_CONTENT=… cargo test -p atv-ui -- --ignored dump`.
    #[test]
    #[ignore]
    fn dump_overlay_layouts() {
        let dir = std::env::var("FURY_CONTENT").expect("FURY_CONTENT");
        let content =
            pollster::block_on(Content::open(atv_content::Store::Directory(dir.into()))).unwrap();
        let assets =
            pollster::block_on(load(&content, "env01", Some(RaceVariant::National), None)).unwrap();
        let mut keys: Vec<_> = assets.speed.fields.iter().collect();
        keys.sort();
        println!("speed {keys:#?}");
        let mut keys: Vec<_> = assets.stunt.fields.iter().collect();
        keys.sort();
        println!("stunt {keys:#?}");
        for (name, image) in &assets.images {
            println!("image {name} {}x{}", image.width, image.height);
        }
        for (name, font) in &assets.fonts {
            println!("font {name} {:?}", font.measure("00 Mph"));
        }
    }
}
