//! Original static-scenery pass metadata and the observed translation-only UV
//! animator (001be080, 001be5d0). General rotation/shear and pixel animation are
//! reported as unsupported; they must not silently become a fixed texture.
use crate::{
    Result,
    authored_text::{
        assignment, first_digits, js_number, line_assignment, line_starts, lines, occurrences,
        skip_space, space, starts_with_key, trim, uncommented,
    },
    scene,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Pass {
    pub shader: String,
    pub prelit: bool,
    /// Original local-normal reflection projection: 1=XY, 2=XZ, 3=YZ.
    pub reflection_plane: u8,
    pub fog: bool,
    pub blend: bool,
    /// GS (Cs-0)*As+Cd; source-alpha additive rather than opaque fallback.
    pub additive: bool,
    pub depth_write: bool,
    pub uv_scroll: Option<Scroll>,
    pub unsupported_animation: Option<String>,
    pub texture_animation: Option<TextureAnimation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TextureAnimation {
    pub fps: f32,
    pub initial: f32,
    pub looping: bool,
    pub loop_start: i32,
}

/// Forward ATM frame clock from 001bd8e0/001bdb60. Original elapsed time is
/// unsigned milliseconds; the first preparation has zero elapsed time.
pub struct TextureClock {
    config: TextureAnimation,
    frames: usize,
    phase: f32,
    previous_ms: Option<u32>,
}
impl TextureClock {
    pub fn phase(&self) -> f32 {
        self.phase
    }
    pub fn new(config: TextureAnimation, frames: usize) -> Result<Self> {
        if frames == 0
            || frames > 512
            || !config.initial.is_finite()
            || !config.fps.is_finite()
            || !(0.0..=1000.0).contains(&config.fps)
            || config.initial < 0.
            || config.initial >= frames as f32
            || config.loop_start < -1
            || config.loop_start >= frames as i32
        {
            return Err("invalid scenery ATM frame range".into());
        }
        Ok(Self {
            phase: config.initial,
            config,
            frames,
            previous_ms: None,
        })
    }
    pub fn tick(&mut self, milliseconds: u32) -> usize {
        let elapsed = self
            .previous_ms
            .filter(|previous| *previous != u32::MAX)
            .map(|previous| milliseconds.wrapping_sub(previous) as f32 * 0.001)
            .unwrap_or(0.);
        self.previous_ms = Some(milliseconds);
        self.phase += elapsed * self.config.fps;
        if self.phase >= self.frames as f32 {
            let remainder = self.phase % self.frames as f32;
            self.phase = if !self.config.looping {
                (self.frames - 1) as f32
            } else if self.config.loop_start < 0 {
                remainder
            } else {
                self.config.loop_start as f32
                    + remainder % (self.frames as f32 - self.config.loop_start as f32)
            };
        }
        self.phase as usize
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Scroll {
    pub velocity: [f32; 2],
    pub offset: [f32; 2],
}

impl Scroll {
    pub fn step(&mut self, dt: f32) -> Result<[f32; 2]> {
        if !dt.is_finite() || !(0.0..=1.0).contains(&dt) {
            return Err("invalid scenery animation interval".into());
        }
        // Native fmodf preserves negative remainders. The output texture matrix
        // subtracts the accumulated center; positive authored V scroll moves
        // the image down rather than sampling ahead.
        for i in 0..2 {
            self.offset[i] = (self.offset[i] + dt * self.velocity[i]) % 1.0;
        }
        Ok(self.offset.map(|v| -v))
    }
}

pub fn parse(data: &[u8]) -> Result<Pass> {
    if data.len() > 1024 * 1024 || data.contains(&0) {
        return Err("invalid scenery material extent".into());
    }
    let text = std::str::from_utf8(data).map_err(|_| "scenery material is not UTF-8")?;
    // The original parser treats blank authoring-template values as absent.
    let clean = text
        .lines()
        .filter(|line| {
            !line
                .split(';')
                .next()
                .unwrap_or("")
                .split_once('=')
                .is_some_and(|(_, value)| value.trim().is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n");
    let data = scene::document(clean.as_bytes())?;
    let get = |key: &str| {
        data.entries
            .iter()
            .find(|entry| {
                entry.section.eq_ignore_ascii_case("ShaderPass_1")
                    && entry.key.eq_ignore_ascii_case(key)
            })
            .map(|entry| entry.value.as_str())
    };
    let number = |key: &str, default: f32| -> Result<f32> {
        let value = get(key)
            .map(|v| v.parse::<f32>())
            .transpose()
            .map_err(|_| format!("invalid scenery material {key}"))?
            .unwrap_or(default);
        if !value.is_finite() {
            return Err(format!("nonfinite scenery material {key}"));
        }
        Ok(value)
    };
    let shader = get("microProgramName").unwrap_or("").to_owned();
    let reflection_plane = match shader.to_ascii_lowercase().as_str() {
        "prelitreflxyenvmapcompressed" => 1,
        "prelitreflxzenvmapcompressed" => 2,
        "prelitreflyzenvmapcompressed" => 3,
        _ => 0,
    };
    let prelit = shader.eq_ignore_ascii_case("PrelitCompressed")
        || shader.eq_ignore_ascii_case("TexTransformPrelitCompressed")
        || shader.eq_ignore_ascii_case("prelit")
        || reflection_plane != 0;
    let enabled = number("alphaBlendingOnOff", 0.)? != 0.;
    let blend_fields = if enabled {
        ["inputColorA", "inputColorB", "inputAlphaC", "inputAlphaD"]
            .into_iter()
            .zip([0., 1., 0., 1.])
            .map(|(key, default)| number(key, default))
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![0., 1., 0., 1.]
    };
    let additive = enabled && blend_fields == [0., 2., 0., 1.];
    let blend = enabled && (additive || blend_fields == [0., 1., 0., 1.]);
    // Legacy authored 'o' is accepted as the engine's strtol zero.
    let depth_write =
        !get("ZWriteDisabled").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let animate = number("uvs_animate_1", number("uvs_animate", 0.)?)? != 0.;
    let mut unsupported_animation = None;
    let texture_animation = if get("TextureAnimation")
        .is_some_and(|v| v.eq_ignore_ascii_case("unique") || v.eq_ignore_ascii_case("shared"))
    {
        let field = |key: &str, default: f32| number(&format!("{key}_1"), number(key, default)?);
        if field("animatedMapReverseLoopOnEnd", 0.)? != 0. {
            unsupported_animation = Some("reverse ATM playback".into());
            None
        } else {
            let fps = field("animatedMapFramesPerSecond", 1.)?;
            if !(0.0..=1000.0).contains(&fps) {
                return Err("invalid scenery ATM rate".into());
            }
            Some(TextureAnimation {
                fps,
                initial: field("animatedMapInitialFrame", 0.)?,
                looping: field("animatedMapLooping", 1.)? != 0.,
                loop_start: field("animatedMapLoopOnEndToFrame", -1.)? as i32,
            })
        }
    } else {
        None
    };
    let uv_scroll = if animate {
        let observed = shader.eq_ignore_ascii_case("TexTransformPrelitCompressed");
        let defaults = [
            ("angle", 0.),
            ("uv_angular_rate", 0.),
            ("u_center", 0.),
            ("v_center", 0.),
            ("u_scale", 1.),
            ("v_scale", 1.),
            ("u_shear", 0.),
            ("v_shear", 0.),
            ("uv_frames", 0.),
            ("uv_Looping", 0.),
            ("uv_ReverseLoopOnEnd", 0.),
        ];
        let mut translation_only = observed;
        for (key, default) in defaults {
            let overridden = number(&format!("{key}_1"), number(key, default)?)?;
            translation_only &= overridden == default;
        }
        if translation_only {
            Some(Scroll {
                velocity: [
                    number("u_velocity_1", number("u_velocity", 0.)?)?,
                    number("v_velocity_1", number("v_velocity", 0.)?)?,
                ],
                offset: [0.; 2],
            })
        } else {
            unsupported_animation = Some("UV rotation, shear, frame sequence or shader".into());
            None
        }
    } else {
        None
    };
    Ok(Pass {
        shader,
        prelit,
        reflection_plane,
        // DefaultMT constructor001b9810 enables fog; explicit PSM passes
        // override its GS PRIM bit through001b51d0.
        fog: number("foggingOnOff", 1.)? != 0.,
        blend,
        additive,
        depth_write,
        uv_scroll,
        unsupported_animation,
        texture_animation,
    })
}

/// One ordered PSM pass as the browser binds it (formerly regex parsing in
/// game.js). Text semantics follow the former JavaScript exactly, including
/// unanchored key searches; `pass` is parsed only for a bound authored pass.
#[derive(Debug, Serialize)]
pub struct MaterialPass {
    /// Shared declarations plus this pass's fields under ShaderPass_1.
    pub source: String,
    /// 001bf430 base TextureMap_1 when it uses coordinate set 0.
    pub binding: Option<String>,
    pub shader: Option<String>,
    /// Reflection/environment-map microprogram name.
    pub reflective: bool,
    /// Reflective program without a supported local reflection plane.
    pub unsupported_reflection: bool,
    pub pass: Option<Pass>,
    pub alpha_test: f64,
    /// Explicit GEQUAL cutout (alphaTestMethod 5, failure 0/1) values.
    pub scene_alpha_failure: f64,
    pub scene_alpha_cutoff: f64,
    /// MudAnimator TextureMap_1 entries in authored order.
    pub mud_maps: Vec<String>,
}

/// Splits ordered ShaderPass_N sections; each result carries the shared
/// declarations and its own fields as ShaderPass_1. Prologue/epilogue
/// execution is separate.
pub fn pass_sources(text: &str) -> Result<Vec<String>> {
    if text.encode_utf16().count() > 1024 * 1024 {
        return Err("Original material exceeds extent limit.".into());
    }
    let mut sections: Vec<(&str, Vec<&str>)> = vec![("", Vec::new())];
    for line in lines(text) {
        let clean = uncommented(line);
        match clean.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            Some(header) if !header.is_empty() && !header.contains(']') => {
                sections.push((header, Vec::new()))
            }
            _ => sections.last_mut().unwrap().1.push(line),
        }
    }
    let total = |line: &str| {
        let line = line.trim_start_matches(space);
        starts_with_key(line, 0, "TotalShaderPasses")
            && assignment(line, 0, "TotalShaderPasses").is_some()
    };
    let count = sections
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case("General"))
        .flat_map(|(_, lines)| lines.iter().map(|line| uncommented(line)))
        .find(|line| starts_with_key(line, 0, "TotalShaderPasses") && total(line))
        .map_or(1., |line| js_number(line.split('=').nth(1).unwrap_or("")));
    if count.fract() != 0. || !(1. ..=8.).contains(&count) {
        return Err("Unsupported original material pass count.".into());
    }
    let numbered = |header: &str| {
        header.len() > 11
            && starts_with_key(header, 0, "ShaderPass_")
            && header[11..].bytes().all(|b| b.is_ascii_digit())
    };
    let shared = sections
        .iter()
        .filter(|(header, _)| !numbered(header))
        .map(|(header, lines)| {
            let body = lines
                .iter()
                .map(|line| {
                    if total(line) {
                        "TotalShaderPasses=1"
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if header.is_empty() {
                body
            } else {
                format!("[{header}]\n{body}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if trim(text).is_empty() {
        return Ok(vec![String::new()]);
    }
    (1..=count as usize)
        .map(|index| {
            let name = format!("ShaderPass_{index}");
            let mut matches = sections
                .iter()
                .filter(|(header, _)| header.eq_ignore_ascii_case(&name));
            match (matches.next(), matches.next()) {
                (Some((_, lines)), None) => {
                    Ok(format!("{shared}\n[ShaderPass_1]\n{}", lines.join("\n")))
                }
                _ => Err(format!(
                    "Original material pass{index} is missing or duplicated."
                )),
            }
        })
        .collect()
}

/// TextureMap_1 of [ShaderPass_1] when TextureCoordinateIndex_1 is zero.
pub fn texture_binding(text: &str) -> Option<String> {
    let (mut base, mut texture, mut coordinates) = (false, None, 0.);
    for line in lines(text).map(uncommented) {
        if line.starts_with('[') {
            base = line.eq_ignore_ascii_case("[shaderpass_1]");
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| base) else {
            continue;
        };
        let (key, value) = (trim(key), trim(value));
        if key.eq_ignore_ascii_case("TextureMap_1") {
            texture = Some(value);
        }
        if key.eq_ignore_ascii_case("TextureCoordinateIndex_1") {
            coordinates = js_number(value);
        }
    }
    texture
        .filter(|t| coordinates == 0. && !t.is_empty())
        .map(str::to_owned)
}

fn describe_pass(source: String, fallback: Option<&str>) -> Result<MaterialPass> {
    let text = source.as_str();
    let shader = lines(text)
        .map(uncommented)
        .find(|line| {
            starts_with_key(line, 0, "microProgramName")
                && assignment(line, 0, "microProgramName").is_some()
        })
        .map(|line| trim(line.split('=').nth(1).unwrap_or("")).to_owned());
    let binding = fallback
        .map(str::to_owned)
        .or_else(|| texture_binding(text));
    let pass = if binding.is_some() && fallback.is_none() {
        Some(parse(text.as_bytes())?)
    } else {
        None
    };
    let reflective = shader.as_deref().is_some_and(|shader| {
        let shader = shader.to_ascii_lowercase();
        shader.contains("refl") || shader.contains("envmap")
    });
    let reference = js_number(first_digits(text, "alphaReferenceValue").unwrap_or("0")) / 255.;
    let alpha_off = occurrences(text, "alphaTestOnOff").any(|at| {
        assignment(text, at, "alphaTestOnOff").is_some_and(|value| text[value..].starts_with('0'))
    });
    let cutout = line_assignment(text, "alphaTestOnOff", &['1'])
        && line_assignment(text, "alphaTestMethod", &['5'])
        && line_assignment(text, "processingMethodWhenFailedInAlphaTest", &['0', '1']);
    let mut mud_maps = Vec::new();
    if occurrences(text, "MudAnimator").next().is_some() {
        // /^\s*TextureMap_1\s*=\s*([^\r\n;]+)/gmi, including its backtracking.
        let value_char = |c: char| !matches!(c, '\r' | '\n' | ';');
        let mut from = 0;
        for start in line_starts(text) {
            if start < from {
                continue;
            }
            let key = skip_space(text, start);
            let Some(value) = starts_with_key(text, key, "TextureMap_1")
                .then(|| assignment(text, key, "TextureMap_1"))
                .flatten()
            else {
                continue;
            };
            let open = text[..value].rfind('=').unwrap() + 1;
            let first = if text[value..].chars().next().is_some_and(value_char) {
                Some(value)
            } else {
                text[open..value]
                    .char_indices()
                    .rev()
                    .find(|(_, c)| !matches!(c, '\r' | '\n'))
                    .map(|(at, _)| open + at)
            };
            let Some(first) = first else {
                continue;
            };
            let end = first
                + text[first..]
                    .find(|c: char| !value_char(c))
                    .unwrap_or(text.len() - first);
            mud_maps.push(trim(&text[first..end]).replace('"', ""));
            from = end;
        }
    }
    Ok(MaterialPass {
        unsupported_reflection: reflective
            && pass.as_ref().is_none_or(|pass| pass.reflection_plane == 0),
        binding,
        shader,
        reflective,
        pass,
        alpha_test: if alpha_off { 0. } else { reference },
        scene_alpha_failure: if cutout {
            js_number(first_digits(text, "processingMethodWhenFailedInAlphaTest").unwrap_or("0"))
        } else {
            0.
        },
        scene_alpha_cutoff: if cutout { reference } else { 0. },
        mud_maps,
        source,
    })
}

/// Ordered browser passes for a PSM. Without an authored PSM (`fallback`),
/// 001bf430 → 001b5080 binds the material's own texture stem with set 0.
pub fn describe(text: &str, multi_pass: bool, fallback: Option<&str>) -> Result<Vec<MaterialPass>> {
    let sources = if multi_pass {
        pass_sources(text)?
    } else {
        vec![text.to_owned()]
    };
    sources
        .into_iter()
        .map(|source| describe_pass(source, fallback))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_lamp_additive_alpha_is_not_opaque() {
        let source=b"[ShaderPass_1]\nmicroProgramName=PreLitCompressed\nalphaBlendingOnOff=1\ninputColorA=0\ninputColorB=2\ninputAlphaC=0\ninputAlphaD=1";
        let pass = parse(source).unwrap();
        assert!(pass.prelit && pass.blend && pass.additive);
        assert!(
            !parse(b"[ShaderPass_1]\nalphaBlendingOnOff=0\ninputColorB=2")
                .unwrap()
                .additive
        );
    }
    #[test]
    fn reflection_planes_preserve_prelit_blend_and_disabled_depth() {
        for (shader, plane) in [
            ("PrelitReflXyEnvmapCompressed", 1),
            ("PrelitReflXZEnvmapCompressed", 2),
            ("PrelitReflYZEnvmapCompressed", 3),
        ] {
            let source = format!(
                "[ShaderPass_1]\nmicroProgramName={shader}\nalphaBlendingOnOff=1\nZWriteDisabled=1\n"
            );
            let pass = parse(source.as_bytes()).unwrap();
            assert_eq!(pass.reflection_plane, plane);
            assert!(pass.prelit && pass.blend && !pass.depth_write);
            assert!(pass.fog);
        }
        let unsupported =
            parse(b"[ShaderPass_1]\nmicroProgramName=EnvironmentMapCompressed\n").unwrap();
        assert_eq!(unsupported.reflection_plane, 0);
        assert!(!unsupported.prelit);
        let unfogged = parse(b"[ShaderPass_1]\nfoggingOnOff=0\n").unwrap();
        assert!(!unfogged.fog);
    }
    #[test]
    fn scroll_uses_negative_sampling_offset_and_signed_wrap() {
        let mut scroll = Scroll {
            velocity: [2., -3.],
            offset: [0.; 2],
        };
        assert_eq!(scroll.step(0.25).unwrap(), [-0.5, 0.75]);
        assert_eq!(scroll.step(0.25).unwrap(), [-0., 0.5]);
        let unchanged = scroll.offset;
        assert!(scroll.step(f32::NAN).is_err());
        assert_eq!(scroll.offset, unchanged);
    }
    #[test]
    fn original_blank_keys_and_indexed_overrides() {
        let pass = parse(b"[General]\nTotalShaderPasses=1\n[ShaderPass_1]\nmicroProgramName=TexTransformPrelitCompressed\nuvs_animate=1\nu_velocity=0\nv_velocity=2\nv_velocity_1=3\nangle=\nZWriteDisabled=o\nalphaBlendingOnOff=1\n").unwrap();
        assert!(pass.prelit && pass.blend && pass.depth_write);
        assert_eq!(pass.uv_scroll.unwrap().velocity, [0., 3.]);
        let pass = parse(b"[ShaderPass_1]\nmicroProgramName=TexTransformPrelitCompressed\nuvs_animate=1\nu_scale=2\n").unwrap();
        assert!(pass.unsupported_animation.is_some());
        assert!(pass.uv_scroll.is_none());
        assert!(parse(b"[ShaderPass_1]\nv_velocity=NaN\nuvs_animate=1\nmicroProgramName=TexTransformPrelitCompressed\n").is_err());
    }
    // Expected values below were produced by the former game.js regex reading
    // (textureBinding, microProgramName, alpha and MudAnimator expressions).
    fn single(text: &str) -> MaterialPass {
        let mut passes = describe(text, false, None).unwrap();
        assert_eq!(passes.len(), 1);
        passes.remove(0)
    }
    #[test]
    fn binding_uses_base_pass_set_zero_with_javascript_number_rules() {
        let binding = |text| texture_binding(text);
        assert_eq!(
            binding("[ShaderPass_1]\nTextureMap_1=a\nTextureCoordinateIndex_1=0x0").as_deref(),
            Some("a")
        );
        assert_eq!(
            binding("[ShaderPass_1]\nTextureMap_1=a\nTextureCoordinateIndex_1=-0.0e3").as_deref(),
            Some("a")
        );
        assert_eq!(
            binding("[ShaderPass_1]\nTextureMap_1=a\nTextureCoordinateIndex_1=inf"),
            None
        );
        assert_eq!(
            binding("[ShaderPass_1]\nTextureMap_1=a\nTextureCoordinateIndex_1=1"),
            None
        );
        assert_eq!(binding("[ShaderPass_1]\nTextureMap_1=\n"), None);
        assert_eq!(
            binding("[SHADERPASS_1] ;c\n texturemap_1 = b ; comment\n").as_deref(),
            Some("b")
        );
        assert_eq!(binding("[ShaderPass_2]\nTextureMap_1=a"), None);
        let fallback = describe("", true, Some("stem")).unwrap();
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].binding.as_deref(), Some("stem"));
        assert!(fallback[0].pass.is_none() && fallback[0].shader.is_none());
    }
    #[test]
    fn shader_alpha_and_cutout_follow_the_former_expressions() {
        assert_eq!(
            single("microProgramName = A=B\n").shader.as_deref(),
            Some("A")
        );
        assert_eq!(
            single("x microProgramName=no\nmicroProgramName=yes")
                .shader
                .as_deref(),
            Some("yes")
        );
        assert_eq!(
            single("XalphaReferenceValue=7 alphaReferenceValue=9").alpha_test,
            7. / 255.
        );
        assert_eq!(
            single("alphaTestOnOff=\n0\nalphaReferenceValue=9").alpha_test,
            0.
        );
        let cutout = single(
            "  alphaTestOnOff = 1 \nalphaTestMethod=5\nprocessingMethodWhenFailedInAlphaTest=1\nalphaReferenceValue=64",
        );
        assert_eq!(
            (
                cutout.alpha_test,
                cutout.scene_alpha_failure,
                cutout.scene_alpha_cutoff
            ),
            (64. / 255., 1., 64. / 255.)
        );
        for text in [
            "a alphaTestOnOff=1\nalphaTestMethod=5\nprocessingMethodWhenFailedInAlphaTest=0",
            "alphaTestOnOff\n=\n1\n\nalphaTestMethod=5\u{2028}processingMethodWhenFailedInAlphaTest=01",
            "alphaTestOnOff=11\nalphaTestMethod=5\nprocessingMethodWhenFailedInAlphaTest=0",
        ] {
            let pass = single(text);
            assert_eq!(
                (pass.scene_alpha_failure, pass.scene_alpha_cutoff),
                (0., 0.)
            );
        }
        let reflective = describe(
            "[ShaderPass_1]\nmicroProgramName=DiffuseEnvmapCompressed\nTextureMap_1=s",
            false,
            None,
        )
        .unwrap();
        assert!(reflective[0].reflective && reflective[0].unsupported_reflection);
        let plane = describe(
            "[ShaderPass_1]\nmicroProgramName=PrelitReflXZEnvmapCompressed\nTextureMap_1=s",
            false,
            None,
        )
        .unwrap();
        assert!(plane[0].reflective && !plane[0].unsupported_reflection);
    }
    #[test]
    fn mud_maps_preserve_multiline_regex_backtracking() {
        let maps = |text| single(text).mud_maps;
        assert_eq!(
            maps(
                "[ShaderPass_1]\nMudAnimator=1\nTextureMap_1=dirt\n[ShaderPass_2]\nTextureMap_1 = \"mud\" ;c\n"
            ),
            ["dirt", "mud"]
        );
        assert_eq!(
            maps("MudAnimator\nTextureMap_1=  ;x\nTextureMap_1=\n\n  b  "),
            ["", "b"]
        );
        assert_eq!(
            maps(
                "mudanimator\n  TextureMap_1=a TextureMap_1=b\u{2028}TextureMap_1=c\nTextureMap_1 =\t\"d\""
            ),
            ["a TextureMap_1=b\u{2028}TextureMap_1=c", "d"]
        );
        assert!(maps("MudAnimator\nTextureMap_1=\r\n").is_empty());
        assert!(maps("TextureMap_1=a\nTextureMap_1=b").is_empty());
    }
    #[test]
    fn ordered_passes_keep_shared_declarations_and_reject_bad_counts() {
        let passes = pass_sources("[General]\nTotalShaderPasses = 2 ; two\n[ShaderPass_1]\nTextureMap_1=a\n[ShaderPass_2]\nTextureMap_1=b\n[Epilogue]\nx=1").unwrap();
        assert_eq!(
            passes,
            [
                "\n[General]\nTotalShaderPasses=1\n[Epilogue]\nx=1\n[ShaderPass_1]\nTextureMap_1=a",
                "\n[General]\nTotalShaderPasses=1\n[Epilogue]\nx=1\n[ShaderPass_1]\nTextureMap_1=b",
            ]
        );
        assert_eq!(
            pass_sources("[General]\nTotalShaderPasses=0x2\n[ShaderPass_1]\n[ShaderPass_2]")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            pass_sources(
                "[General]\nTotalShaderPasses=1\n[ShaderPass_01]\n[ShaderPass_1]\nTextureMap_1=a"
            )
            .unwrap(),
            ["\n[General]\nTotalShaderPasses=1\n[ShaderPass_1]\nTextureMap_1=a"]
        );
        assert_eq!(pass_sources(" \n\t").unwrap(), [""]);
        for (text, error) in [
            (
                "[General]\nTotalShaderPasses=9",
                "Unsupported original material pass count.",
            ),
            (
                "[General]\nTotalShaderPasses=1.5",
                "Unsupported original material pass count.",
            ),
            (
                "[General]\nTotalShaderPasses=NaN",
                "Unsupported original material pass count.",
            ),
            (
                "[General]\nTotalShaderPasses=2\n[ShaderPass_1]",
                "Original material pass2 is missing or duplicated.",
            ),
            (
                "[ShaderPass_1]\n[ShaderPass_1]",
                "Original material pass1 is missing or duplicated.",
            ),
        ] {
            assert_eq!(pass_sources(text).unwrap_err(), error);
        }
        let reflection = "[General]\nTotalShaderPasses=2\nEnableMultiPassCull=0\n[ShaderPass_1]\nmicroProgramName=PrelitReflXZEnvmapCompressed\nTextureMap_1=sky\n[ShaderPass_2];source overlay\nmicroProgramName=diffuseCompressed\nTextureMap_1=facade\nalphaBlendingOnOff=1\n[Epilogue]\nDummy=1";
        let passes = describe(reflection, true, None).unwrap();
        assert_eq!(
            passes
                .iter()
                .map(|p| p.binding.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["sky", "facade"]
        );
        assert_eq!(passes[0].pass.as_ref().unwrap().reflection_plane, 2);
        assert!(passes[1].pass.as_ref().unwrap().blend && !passes[1].source.contains("PrelitRefl"));
        assert!(
            passes.iter().all(
                |p| p.source.contains("TotalShaderPasses=1") && p.source.contains("[Epilogue]")
            )
        );
    }
    #[test]
    fn atm_clock_retains_first_frame_uses_unsigned_ms_and_loop_start() {
        let config = TextureAnimation {
            fps: 2.,
            initial: 0.,
            looping: true,
            loop_start: 0,
        };
        let mut clock = TextureClock::new(config.clone(), 4).unwrap();
        assert_eq!(clock.tick(u32::MAX - 249), 0);
        assert_eq!(clock.tick(250), 1);
        assert_eq!(clock.tick(1750), 0);
        let mut clock = TextureClock::new(
            TextureAnimation {
                looping: false,
                ..config
            },
            4,
        )
        .unwrap();
        assert_eq!(clock.tick(0), 0);
        assert_eq!(clock.tick(3000), 3);
    }
}
