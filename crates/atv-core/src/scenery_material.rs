//! Original static-scenery pass metadata and the observed translation-only UV
//! animator (001be080, 001be5d0). General rotation/shear and pixel animation are
//! reported as unsupported; they must not silently become a fixed texture.
use crate::{Result, scene};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Pass {
    pub shader: String,
    pub prelit: bool,
    /// Original local-normal reflection projection: 1=XY, 2=XZ, 3=YZ.
    pub reflection_plane: u8,
    pub fog: bool,
    pub blend: bool,
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
    let blend = number("alphaBlendingOnOff", 0.)? != 0.
        && ["inputColorA", "inputColorB", "inputAlphaC", "inputAlphaD"]
            .into_iter()
            .zip([0., 1., 0., 1.])
            .map(|(k, v)| number(k, v).map(|n| n == v))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .all(|v| v);
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
        depth_write,
        uv_scroll,
        unsupported_animation,
        texture_animation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
