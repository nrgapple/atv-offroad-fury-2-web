//! Original static-scenery pass metadata and the observed translation-only UV
//! animator (001be080, 001be5d0). General rotation/shear and pixel animation are
//! reported as unsupported; they must not silently become a fixed texture.
use crate::{Result, scene};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Pass {
    pub shader: String,
    pub prelit: bool,
    pub blend: bool,
    pub depth_write: bool,
    pub uv_scroll: Option<Scroll>,
    pub unsupported_animation: Option<String>,
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
    let prelit = shader.eq_ignore_ascii_case("PrelitCompressed")
        || shader.eq_ignore_ascii_case("TexTransformPrelitCompressed")
        || shader.eq_ignore_ascii_case("prelit");
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
        blend,
        depth_write,
        uv_scroll,
        unsupported_animation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
