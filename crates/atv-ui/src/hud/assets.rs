//! Original HUD resources (web/hud.js `init`): the OverlayObject-5 speed and
//! preload meter, the OverlayObject-12 stunt text, their CEL font metrics and
//! icon textures, read from the same archives in the browser game's order.
use atv_content::Content;
use atv_core::{texture, ui};
use atv_game::archive::Archive;
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
/// archive holding a name wins.
fn archives(environment: &str) -> Vec<String> {
    let mut list = vec![
        "/STARTAPP.RES".to_string(),
        "/UI/RESOURCE/PAGEA/PAGE01.RES".to_string(),
        "/STARTEVT.RES".to_string(),
    ];
    if let Ok(course) = atv_game::launch::course_resource(environment, false) {
        list.push(course.archive);
    }
    list.push("/OVERLAYS.RES".to_string());
    list
}

pub async fn load(content: &Content, environment: &str) -> Result<OverlayAssets, String> {
    let mut opened = Vec::new();
    for path in archives(environment) {
        if content.manifest.record(&path).is_some() {
            opened.push(Archive::open(content, &path).await?);
        }
    }
    let find = |name: &str| -> Option<(usize, usize)> {
        opened
            .iter()
            .enumerate()
            .find_map(|(a, archive)| archive.named(name).map(|m| (a, m)))
    };
    let (a, m) = find("Overlays.pb").ok_or("original Overlays.pb is missing")?;
    let overlays = opened[a].member(m).await?;
    let speed = section(&overlays, "OverlayObject-5").ok_or("OverlayObject-5 missing")?;
    let stunt = section(&overlays, "OverlayObject-12").ok_or("OverlayObject-12 missing")?;
    let mut fonts = HashMap::new();
    let mut images = HashMap::new();
    for layout in [&speed, &stunt] {
        for i in 1..=layout.number("numtext").unwrap_or(0.) as usize {
            let name = layout
                .text(&format!("text{i}font"))
                .unwrap_or_else(|| FONT_DEFAULT.into())
                .to_ascii_lowercase();
            if fonts.contains_key(&name) {
                continue;
            }
            let Some((a, m)) = find(&format!("{name}.cel")) else {
                continue;
            };
            let cel = ui::Font::decode(&opened[a].member(m).await?)?;
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
            let Some(name) = layout.text(&format!("icon{i}texture")) else {
                continue;
            };
            let stem = name
                .trim_end_matches(".tga")
                .trim_end_matches(".pst")
                .to_string();
            if images.contains_key(&stem) {
                continue;
            }
            let Some((a, m)) = find(&format!("{stem}.pst")) else {
                continue;
            };
            let decoded = texture::decode_pst(&opened[a].member(m).await?)?;
            images.insert(
                stem,
                HudImage {
                    width: decoded.info.width,
                    height: decoded.info.height,
                    rgba: decoded.rgba,
                },
            );
        }
    }
    Ok(OverlayAssets {
        speed,
        stunt,
        fonts,
        images,
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
        let assets = pollster::block_on(load(&content, "env01")).unwrap();
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
