//! Authored sun flares: the TOY `[SunFlares]` list (ATV2 001fe180, MX
//! 00216650) and each flare material's occlusion settings. ATV2 reads its
//! PSM `[Prologue]` (001b8150); MX reads the TOY when
//! `useToyFlareSettings` is set, else the PSM `OcclusionFlare` stream
//! (001dbcb0). Rendering is in atv-render's flare pass (research/SUN_FLARES.md).
use crate::materials::member_named;
use crate::venue::Image;
use atv_core::Result;
use atv_core::resource::Resource;

/// The most flares a scene keeps (the authored maximum is one).
pub const MAX_FLARES: usize = 4;

#[derive(Clone)]
pub struct SunFlare {
    /// Unit vector from the camera toward the sun: Position - TargetPosition
    /// (or the negated LightDirection).
    pub toward_sun: [f32; 3],
    pub image: Image,
    /// Sprite width and height in 640x448 PS2 screen pixels.
    pub size: f32,
    /// Occlusion sample square in PS2 screen pixels.
    pub sample_size: f32,
    /// GS vertex RGB (128 = 1) and alpha (128 = 1) of the sprite.
    pub color: [f32; 3],
    pub alpha: f32,
}

/// `key = value` pairs by `[section]`, `;` comments removed, keys and
/// sections compared ASCII case-insensitively by the readers below.
fn entries(text: &str) -> Vec<(String, String, String)> {
    let mut section = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[') {
            section = name.split(']').next().unwrap_or("").trim().to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            out.push((
                section.clone(),
                key.trim().to_string(),
                value.trim().to_string(),
            ));
        }
    }
    out
}

fn value<'a>(entries: &'a [(String, String, String)], section: &str, key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|(s, k, _)| s.eq_ignore_ascii_case(section) && k.eq_ignore_ascii_case(key))
        .map(|(_, _, v)| v.as_str())
}

fn number(text: Option<&str>) -> Option<f32> {
    text?.parse::<f32>().ok().filter(|v| v.is_finite())
}

fn vector<const N: usize>(text: Option<&str>) -> Option<[f32; N]> {
    let parts: Vec<f32> = text?
        .split(',')
        .map(|v| v.trim().parse::<f32>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()?;
    parts.get(..N)?.try_into().ok()
}

fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (length > 1e-6 && length.is_finite()).then(|| v.map(|c| c / length))
}

/// One authored flare before its material and image are resolved.
#[derive(Debug, PartialEq)]
pub struct Authored {
    pub texture: String,
    pub toward_sun: [f32; 3],
    /// MX TOY settings, used when `useToyFlareSettings` is set:
    /// sample size, FlareRGBA, FlareAlpha, FlareScreenSize.
    pub toy_settings: Option<(f32, [f32; 3], f32, f32)>,
}

/// The `[SunFlareN]` sections a TOY declares, in order.
pub fn authored(toy: &str) -> Vec<Authored> {
    let entries = entries(toy);
    let count = number(value(&entries, "SunFlares", "NumberOfSunFlares"))
        .map_or(0, |n| n.max(0.) as usize)
        .min(MAX_FLARES);
    (1..=count)
        .filter_map(|i| {
            let section = format!("SunFlare{i}");
            let texture = value(&entries, &section, "TextureMap")?;
            let stem = texture.rsplit(['/', '\\']).next()?;
            let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_string();
            // 00216650: LightDirection, else TargetPosition - Position; the
            // flare stands at the camera minus that direction.
            let direction =
                vector::<3>(value(&entries, &section, "LightDirection")).or_else(|| {
                    let from = vector::<3>(value(&entries, &section, "Position"))?;
                    let to = vector::<3>(value(&entries, &section, "TargetPosition"))?;
                    Some([to[0] - from[0], to[1] - from[1], to[2] - from[2]])
                })?;
            let toward_sun = normalize(direction)?.map(|c| -c);
            let toy =
                number(value(&entries, &section, "useToyFlareSettings")).is_some_and(|v| v != 0.);
            // 00216650 defaults: sample 16, alpha 40, screen size 350.
            let toy_settings = toy.then(|| {
                (
                    number(value(&entries, &section, "OcclusionSampleSize")).unwrap_or(16.),
                    vector::<3>(value(&entries, &section, "FlareRGBA")).unwrap_or([128.; 3]),
                    number(value(&entries, &section, "FlareAlpha")).unwrap_or(40.),
                    number(value(&entries, &section, "FlareScreenSize")).unwrap_or(350.),
                )
            });
            Some(Authored {
                texture: stem,
                toward_sun,
                toy_settings,
            })
        })
        .collect()
}

/// Sprite settings from the flare material.
#[derive(Debug, PartialEq)]
pub struct MaterialSettings {
    pub sample_size: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub size: f32,
    pub texture: Option<String>,
}

/// ATV2 `[Prologue]` (001b8150 defaults: sample 32, scale 3.25, alpha 128,
/// 128-pixel render target). 002cf8c0 draws renderTargetWidth x
/// flareMaxScale pixels at vertex alpha max(flareMaxAlpha, 80) >> 2.
/// MX `OcclusionFlare` stream (001dbcb0 defaults: sample 16, alpha 80,
/// screen size 400) draws flareScreenSize pixels at the authored flareAlpha
/// as GS vertex alpha (an adaptation: MX's render-target sprite filter path
/// is not recovered; its flare art is opaque, so the vertex alpha alone
/// sets the glow).
pub fn material_settings(psm: &str) -> MaterialSettings {
    let entries = entries(psm);
    let texture = value(&entries, "ShaderPass_1", "TextureMap_1").map(|t| {
        let t = t.rsplit(['/', '\\']).next().unwrap_or(t);
        t.rsplit_once('.').map_or(t, |(s, _)| s).to_string()
    });
    let stream = entries
        .iter()
        .find(|(_, k, v)| {
            k.eq_ignore_ascii_case("filterAlgorithm") && v.eq_ignore_ascii_case("OcclusionFlare")
        })
        .map(|(s, _, _)| s.clone());
    if let Some(stream) = stream {
        let get = |key| number(value(&entries, &stream, key));
        MaterialSettings {
            sample_size: get("OcclusionSampleSize").unwrap_or(16.),
            color: vector::<3>(value(&entries, &stream, "FlareRGBA")).unwrap_or([128.; 3]),
            alpha: get("FlareAlpha").unwrap_or(80.),
            size: get("FlareScreenSize").unwrap_or(400.),
            texture,
        }
    } else {
        let get = |key| number(value(&entries, "Prologue", key));
        let width = get("renderTargetWidth").unwrap_or(128.);
        MaterialSettings {
            sample_size: get("OcclusionSampleSize").unwrap_or(32.),
            color: vector::<3>(value(&entries, "Prologue", "flareColor")).unwrap_or([128.; 3]),
            alpha: (get("FlareMaxAlpha").unwrap_or(128.).clamp(80., 255.) as u32 >> 2) as f32,
            size: width * get("FlareMaxScale").unwrap_or(3.25),
            texture,
        }
    }
}

/// The scene's flares with their images; flares whose material or image is
/// missing are skipped (the original tolerates absent objects).
pub fn load(archive: &Resource, toy: &[u8]) -> Result<Vec<SunFlare>> {
    let toy = String::from_utf8_lossy(toy);
    let mut flares = Vec::new();
    for flare in authored(&toy) {
        let Some(psm) = member_named(archive, &format!("{}.psm", flare.texture)) else {
            eprintln!("sun flare: material {} is missing", flare.texture);
            continue;
        };
        let psm = archive.extract(psm)?;
        let mut settings = material_settings(&String::from_utf8_lossy(&psm));
        if let Some((sample_size, color, alpha, size)) = flare.toy_settings {
            settings = MaterialSettings {
                sample_size,
                color,
                alpha,
                size,
                texture: settings.texture,
            };
        }
        let stem = settings.texture.as_deref().unwrap_or(&flare.texture);
        let Some(pst) = member_named(archive, &format!("{stem}.pst")) else {
            eprintln!("sun flare: texture {stem} is missing");
            continue;
        };
        let texture = atv_core::texture::decode_pst(&archive.extract(pst)?)?;
        flares.push(SunFlare {
            toward_sun: flare.toward_sun,
            image: Image {
                width: texture.info.width,
                height: texture.info.height,
                rgba: texture.rgba,
            },
            size: settings.size,
            sample_size: settings.sample_size.clamp(1., 64.),
            color: settings.color,
            alpha: settings.alpha,
        });
    }
    Ok(flares)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atv2_toy_and_prologue() {
        let toy = "[SunFlares]\nNumberOfSunFlares=1 \n\n[SunFlare1]\nTextureMap=env02sun.tga\n\
            MinScale=0.5\nMaxScale=3\nPosition=5661.866,880.399,2880.0\nTargetPosition=2880.0,135.0,2880.0\n";
        let flares = authored(toy);
        assert_eq!(flares.len(), 1);
        assert_eq!(flares[0].texture, "env02sun");
        assert!(flares[0].toy_settings.is_none());
        let [x, y, z] = flares[0].toward_sun;
        assert!(x > 0.9 && y > 0.2 && z.abs() < 1e-6);
        let psm = "[Prologue] \nsetOcclusionAlpha = 1\nrenderTargetWidth\t= 128\nFlareMaxScale = 3.0\n\
            OcclusionSampleSize = 32\nFlareMaxAlpha=40\n[ShaderPass_1]  ;---\nTextureMap_1 = env02sun\n";
        let settings = material_settings(psm);
        assert_eq!(settings.size, 384.);
        assert_eq!(settings.sample_size, 32.);
        // 002cf8c0: max(40, 80) >> 2.
        assert_eq!(settings.alpha, 20.);
        assert_eq!(settings.texture.as_deref(), Some("env02sun"));
    }

    #[test]
    fn mx_toy_settings_and_stream() {
        let toy = "[SunFlares]\nNumberOfSunFlares=1\n[SunFlare1]\nTextureMap=SunFlare48.tga\n\
            Position=1188.3,675.5,-263.2\nTargetPosition=448,90,448\n\
            useToyFlareSettings \t= 1\nOcclusionSampleSize = 26\nFlareRGBA = 128.0, 128.0, 128.0, 0\n\
            FlareAlpha = 40\nFlareScreenSize=250\n";
        let flares = authored(toy);
        assert_eq!(flares[0].toy_settings, Some((26., [128.; 3], 40., 250.)));
        let psm = "[FilterStream_3]\nfilterAlgorithm = OcclusionFlare\nOcclusionSampleSize = 26\n\
            FlareRGBA = 128.0, 128.0, 128.0, 0\nFlareAlpha = 60\nFlareScreenSize = 200\n\
            [ShaderPass_1]  ;---\nTextureMap_1 = sunflare33.pst\n";
        let settings = material_settings(psm);
        assert_eq!(
            (settings.size, settings.alpha, settings.sample_size),
            (200., 60., 26.)
        );
        assert_eq!(settings.texture.as_deref(), Some("sunflare33"));
    }

    #[test]
    fn missing_or_degenerate_flares_are_skipped() {
        assert!(authored("").is_empty());
        let same = "[SunFlares]\nNumberOfSunFlares=1\n[SunFlare1]\nTextureMap=a.tga\nPosition=1,1,1\nTargetPosition=1,1,1\n";
        assert!(authored(same).is_empty());
    }
}
