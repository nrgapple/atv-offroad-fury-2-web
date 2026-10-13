//! Source WTT/PSM water render inputs, shared with the live physics surface.
//! Portable rasterization belongs to the browser; PS2 VU shading is not claimed.
use crate::{
    Result,
    physics_data::PhysicsData,
    texture,
    water_animation::{Definition, LiveField},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Material {
    pub shader: String,
    pub environment_texture: String,
    pub froth_texture: String,
    /// Original GS RGB modulation uses 128 as unity.
    pub environment_color: [f32; 3],
    pub froth_color: [f32; 3],
    pub polygon_scale: f32,
    pub additional_fov: f32,
    pub environment_blend: bool,
    pub environment_depth_write: bool,
    pub environment_fog: bool,
    pub froth_alpha_reference: f32,
    pub froth_depth_write: bool,
}
fn text(data: &[u8], omit_empty: bool) -> Result<PhysicsData> {
    if data.len() > 1024 * 1024 {
        return Err("water render text exceeds limit".into());
    }
    let source = std::str::from_utf8(data).map_err(|_| "water render text is not UTF-8")?;
    let cleaned = source
        .lines()
        .map(|s| s.split("//").next().unwrap_or(""))
        .filter(|s| {
            !omit_empty
                || !s
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .split_once('=')
                    .is_some_and(|(_, value)| value.trim().is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n");
    PhysicsData::parse(cleaned.as_bytes())
}
fn scalar(data: &PhysicsData, section: &str, key: &str, default: f32) -> Result<f32> {
    if data
        .entries
        .iter()
        .any(|entry| entry.section == section && entry.key.eq_ignore_ascii_case(key))
    {
        data.first_scalar(section, key)
    } else {
        Ok(default)
    }
}
fn name(data: &PhysicsData, section: &str, key: &str) -> Result<String> {
    let name = data.first(section, key)?.value.trim_matches('"');
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.contains("..") {
        return Err("invalid water render resource name".into());
    }
    Ok(name.to_owned())
}
impl Material {
    pub fn shader_name(wtt: &[u8]) -> Result<String> {
        name(&text(wtt, false)?, "General Information", "ShaderName")
    }
    pub fn parse(wtt: &[u8], psm: &[u8]) -> Result<Self> {
        let wtt = text(wtt, false)?;
        let psm = text(psm, true)?;
        let general = "General Information";
        let pass = "ShaderPass_1";
        if scalar(&wtt, general, "RenderInfinitePlane", 1.)? == 0.
            || scalar(&wtt, general, "NumPools", 0.)? != 0.
        {
            return Err("finite water pools require original render-object integration".into());
        }
        if !psm
            .first(pass, "microProgramName")?
            .value
            .eq_ignore_ascii_case("Water")
            || scalar(&psm, pass, "totalTextureMaps", 0.)? != 2.
        {
            return Err("unsupported original water material program".into());
        }
        let mut environment_color = [0.; 3];
        let mut froth_color = [0.; 3];
        for (i, axis) in ["Red", "Green", "Blue"].iter().enumerate() {
            environment_color[i] = scalar(&wtt, general, &format!("EnvColor{axis}"), 80.)? / 128.;
            froth_color[i] = scalar(&wtt, general, &format!("FrothColor{axis}"), 80.)? / 128.;
        }
        let result = Self {
            shader: name(&wtt, general, "ShaderName")?,
            environment_texture: name(&psm, pass, "TextureMap_1")?,
            froth_texture: name(&psm, pass, "TextureMap_2")?,
            environment_color,
            froth_color,
            polygon_scale: scalar(&wtt, general, "PolygonScale", 1.)?,
            additional_fov: scalar(&wtt, general, "AdditionalFov", 4.)?,
            environment_blend: scalar(&psm, pass, "alphaBlendingOnOff_1", 0.)? != 0.,
            environment_depth_write: scalar(&psm, pass, "zWriteDisabled_1", 0.)? == 0.,
            environment_fog: scalar(
                &psm,
                pass,
                "foggingOnOff_1",
                scalar(&psm, pass, "foggingOnOff", 1.)?,
            )? != 0.,
            froth_alpha_reference: scalar(&psm, pass, "alphaReferenceValue_2", 0.)? / 128.,
            froth_depth_write: scalar(&psm, pass, "zWriteDisabled_2", 0.)? == 0.,
        };
        if !(0. ..=1.).contains(&result.polygon_scale)
            || result.polygon_scale == 0.
            || result
                .environment_color
                .iter()
                .chain(&result.froth_color)
                .any(|c| !(0. ..=255. / 128.).contains(c))
        {
            return Err("invalid original water render parameters".into());
        }
        Ok(result)
    }
}
#[derive(Debug, Serialize)]
pub struct HeightTexture {
    pub dimension: usize,
    pub pixels: Vec<u8>,
}
#[derive(Debug, Serialize)]
pub struct HeightChannel {
    pub width: [f32; 2],
    pub inverse_width: [f32; 2],
    pub dimension: usize,
    pub tiled: bool,
    pub scale: f32,
    pub positive: bool,
    pub layer: usize,
    pub layers: Vec<usize>,
}
#[derive(Debug, Serialize)]
pub struct HeightAtlas {
    pub textures: Vec<HeightTexture>,
    pub channels: Vec<HeightChannel>,
}

#[derive(Debug, Serialize)]
pub struct GradientTexture {
    pub dimension: usize,
    pub pixels: Vec<u8>,
}
#[derive(Debug, Serialize)]
pub struct GradientWave {
    pub origin_channel: usize,
    pub layer: usize,
    pub layers: Vec<usize>,
    pub scale: f32,
    pub fixed_alpha: i32,
}
#[derive(Debug, Serialize)]
pub struct GradientAtlas {
    pub textures: Vec<GradientTexture>,
    pub waves: Vec<GradientWave>,
}
impl GradientAtlas {
    /// 00198ca0 loads DxPos/DxNeg/DzPos/DzNeg with authored BumpScale.
    /// Render sampling uses display intensities, independently of raw physics indices.
    pub fn new(
        wtt: &[u8],
        directory: &str,
        mut load: impl FnMut(&str) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let data = text(wtt, false)?;
        let definition = Definition::parse(wtt)?;
        let mut textures: Vec<GradientTexture> = vec![];
        let mut waves = vec![];
        for (index, wave) in definition
            .waves
            .iter()
            .enumerate()
            .filter(|(_, wave)| wave.height_channel && !wave.bump_only)
        {
            let mut layers = Vec::with_capacity(wave.frames);
            for frame in 0..wave.frames {
                let mut pixels = vec![];
                let mut dimension = 0;
                for (component, suffix) in ["DxPos", "DxNeg", "DzPos", "DzNeg"].iter().enumerate() {
                    let image = texture::decode_pst(&load(&format!(
                        "{directory}/{}{:04}{suffix}.pst",
                        wave.filename, frame
                    ))?)?;
                    if image.info.width != image.info.height {
                        return Err("nonsquare water gradient signature".into());
                    }
                    if component == 0 {
                        dimension = image.info.width as usize;
                        pixels.resize(dimension * dimension * 4, 0);
                    }
                    if image.info.width as usize != dimension {
                        return Err("water gradient channel dimensions differ".into());
                    }
                    for (pixel, rgba) in image.rgba.as_chunks::<4>().0.iter().enumerate() {
                        pixels[pixel * 4 + component] = rgba[0];
                    }
                }
                let layer = match textures
                    .iter()
                    .position(|texture| texture.dimension == dimension && texture.pixels == pixels)
                {
                    Some(i) => i,
                    None => {
                        textures.push(GradientTexture { dimension, pixels });
                        textures.len() - 1
                    }
                };
                layers.push(layer);
            }
            let scale = scalar(
                &data,
                &format!("Signature Wave {}", index + 1),
                "BumpScale",
                1.,
            )?;
            waves.push(GradientWave {
                origin_channel: waves.len() * 2,
                layer: layers[0],
                layers,
                scale,
                // 00197980, single-frame weight=1: GS FIX rounds before blending.
                fixed_alpha: (128. * scale + 0.5) as i32,
            });
        }
        if textures.is_empty() {
            return Err("water gradient atlas is empty".into());
        }
        Ok(Self { textures, waves })
    }
}
impl HeightAtlas {
    pub fn new(field: &LiveField) -> Result<Self> {
        let mut textures: Vec<HeightTexture> = vec![];
        let mut channels = vec![];
        if field.waves.len() > 8 {
            return Err("portable water supports at most eight height channels".into());
        }
        for (index, wave) in field.waves.iter().enumerate() {
            if wave.pixel_stride != 1 || wave.row_stride != wave.dimension {
                return Err("portable water requires packed height maps".into());
            }
            let fallback = vec![wave.first_frame.clone()];
            let frames = field.frames.get(index).unwrap_or(&fallback);
            let mut layers = vec![];
            for pixels in frames {
                let layer = match textures.iter().position(|texture| {
                    texture.dimension == wave.dimension && texture.pixels == *pixels
                }) {
                    Some(i) => i,
                    None => {
                        textures.push(HeightTexture {
                            dimension: wave.dimension,
                            pixels: pixels.clone(),
                        });
                        textures.len() - 1
                    }
                };
                layers.push(layer);
            }
            channels.push(HeightChannel {
                width: wave.width,
                inverse_width: wave.inverse_width,
                dimension: wave.dimension,
                tiled: wave.tiled,
                scale: wave.scale,
                positive: wave.positive,
                layer: layers[0],
                layers,
            });
        }

        Ok(Self { textures, channels })
    }
}
#[derive(Debug, Serialize)]
pub struct Frame {
    pub base_height: f32,
    pub origins: Vec<[f32; 2]>,
    pub signature_frames: Vec<[usize; 2]>,
    pub first_frame_weights: Vec<f32>,
}
impl Frame {
    /// Completed render-preparation state, with no second visual clock or swell.
    pub fn new(field: &LiveField) -> Self {
        Self {
            base_height: field.definition.surface.base_height,
            origins: field.waves.iter().map(|wave| wave.origin).collect(),
            signature_frames: field
                .states
                .iter()
                .map(|state| [state.first_frame, state.second_frame])
                .collect(),
            first_frame_weights: field
                .states
                .iter()
                .map(|state| state.first_frame_weight)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derivative_atlas_uses_original_pairs_and_active_wave_order() {
        let wtt = b"[General Information]\nWakeScale=0\nWaterHeightRange=0\nNumSignatureWaves=2\n[Signature Wave 1]\nfilename=wave\nwidthX=15\nwidthZ=15\nnumFrames=1\nBumpScale=.2\n[Signature Wave 2]\nfilename=wave\nwidthX=30\nwidthZ=30\nnumFrames=1\nBumpScale=.5\n[Signature Wave 3]\nfilename=unused\n";
        let mut paths = vec![];
        let atlas = GradientAtlas::new(wtt, "Sigs", |path| {
            paths.push(path.to_owned());
            let intensity = if path.ends_with("DxPos.pst") {
                10
            } else if path.ends_with("DxNeg.pst") {
                20
            } else if path.ends_with("DzPos.pst") {
                30
            } else {
                40
            };
            let mut pst = vec![];
            for word in [1u32, 1, 1] {
                pst.extend_from_slice(&word.to_le_bytes());
            }
            let mut palette = [0u8; 1024];
            palette[4..8].copy_from_slice(&[intensity, intensity, intensity, 128]);
            pst.extend_from_slice(&palette);
            for word in [2u32, 2, 0, 0] {
                pst.extend_from_slice(&word.to_le_bytes());
            }
            pst.extend_from_slice(&[1; 4]);
            Ok(pst)
        })
        .unwrap();
        assert_eq!(atlas.textures.len(), 1);
        assert_eq!(atlas.textures[0].pixels, [10, 20, 30, 40].repeat(4));
        assert_eq!(atlas.waves[0].origin_channel, 0);
        assert_eq!(atlas.waves[1].origin_channel, 2);
        assert_eq!(atlas.waves[0].scale, 0.2);
        assert_eq!(atlas.waves[1].scale, 0.5);
        assert_eq!(atlas.waves[0].fixed_alpha, 26);
        assert_eq!(atlas.waves[1].fixed_alpha, 64);
        assert_eq!(paths.len(), 8);
        assert!(!paths.iter().any(|path| path.contains("unused")));
    }
    #[test]
    fn material_selects_authored_maps_and_gs_units() {
        let wtt=b"[General Information]\nShaderName=\"Stream\"\nEnvColorRed=75\nEnvColorGreen=75\nEnvColorBlue=75\nRenderInfinitePlane=1\nPolygonScale=.99995\n";
        let psm=b"[ShaderPass_1]\nmicroProgramName=Water\ntotalTextureMaps=2\nTextureMap_1=StreamEnvironment\nTextureMap_2=StreamFroth\nzWriteDisabled_1=1\ndepthTestMethod_1=\nalphaBlendingOnOff_1=1\n";
        let result = Material::parse(wtt, psm).unwrap();
        assert_eq!(result.environment_texture, "StreamEnvironment");
        assert_eq!(result.environment_color, [75. / 128.; 3]);
        assert!(!result.environment_depth_write);
        assert!(result.environment_blend);
        let finite = wtt
            .iter()
            .copied()
            .chain(b"RenderInfinitePlane=0\n".iter().copied())
            .collect::<Vec<_>>();
        //The source first-key rule retains the earlier value; never choose last.
        assert!(Material::parse(&finite, psm).is_ok());
        assert!(
            Material::parse(
                b"[General Information]\nShaderName=Stream\nRenderInfinitePlane=0",
                psm
            )
            .is_err()
        );
    }
    #[test]
    fn snapshot_and_atlas_keep_prepared_origins_and_deduplicate_pixels() {
        let definition = crate::water_animation::Definition::parse(
            b"[General Information]\nWaterBaseHeight=10\nWakeScale=0\nWaterHeightRange=0\n",
        )
        .unwrap();
        let wave = crate::water_surface::SignatureWave {
            origin: [-3., 2.],
            width: [15., 30.],
            inverse_width: [1. / 15., 1. / 30.],
            dimension: 2,
            row_stride: 2,
            pixel_stride: 1,
            tiled: true,
            height_channel: true,
            positive: true,
            scale: 16.,
            first_frame: vec![1, 2, 3, 4],
            second_frame: vec![1, 2, 3, 4],
        };
        let field = LiveField {
            definition,
            swell: Default::default(),
            states: vec![],
            frames: vec![vec![wave.first_frame.clone()]; 2],
            waves: vec![wave.clone(), wave],
        };
        let atlas = HeightAtlas::new(&field).unwrap();
        assert_eq!(atlas.textures.len(), 1);
        assert_eq!(atlas.channels.len(), 2);
        assert_eq!(atlas.channels[1].layer, 0);
        assert_eq!(Frame::new(&field).origins, vec![[-3., 2.]; 2]);
        assert_eq!(Frame::new(&field).base_height, 10.);
    }
}
