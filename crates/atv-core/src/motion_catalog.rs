//! Runtime motion catalog loaded by001cb6a0 ->001cabb0.
//! MOT key times remain authoritative; MCF FPS only participates in the
//! pre-load default transition time on this path.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Motion {
    pub name: String,
    pub path: String,
    pub looping: bool,
    pub interpolate_position: bool,
    pub interpolate_rotation: bool,
    pub fps: u32,
    pub declared_frame_count: u32,
    pub transition_time: f32,
}

#[derive(Debug, Serialize)]
pub struct MotionCatalog {
    pub motions: Vec<Motion>,
}

impl MotionCatalog {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let data = PhysicsData::parse(bytes)?;
        let text = |section: &str, key: &str| {
            data.entries
                .iter()
                .find(|entry| {
                    entry.section.eq_ignore_ascii_case(section)
                        && entry.key.eq_ignore_ascii_case(key)
                })
                .map(|entry| entry.value.trim_matches('"'))
        };
        let integer = |section: &str, key: &str, default: u32| -> Result<u32> {
            text(section, key)
                .map(str::parse::<u32>)
                .transpose()
                .map_err(|_| format!("invalid motion catalog {key}"))
                .map(|value| value.unwrap_or(default))
        };
        let count = integer("General info", "NumberOfMotions", 0)?;
        if count == 0 || count > 512 {
            return Err("invalid motion catalog count".into());
        }
        let directory = text("General info", "ContentDirectory").unwrap_or("");
        let mut motions = Vec::with_capacity(count as usize);
        for index in 1..=count {
            let section = format!("MOTION {index}");
            let boolean = |key: &str, default: bool| -> Result<bool> {
                match text(&section, key).map(str::to_ascii_lowercase).as_deref() {
                    None => Ok(default),
                    Some("yes" | "true" | "1") => Ok(true),
                    Some("no" | "false" | "0") => Ok(false),
                    _ => Err(format!("invalid motion catalog {key}")),
                }
            };
            let name = text(&section, "NAME")
                .ok_or("missing motion name")?
                .to_owned();
            if name.is_empty() || name.len() > 255 {
                return Err("invalid motion name".into());
            }
            let file = text(&section, "VUE").ok_or("missing motion file")?;
            let mut path = format!("{directory}{file}").replace('\\', "/");
            if path.len() > 1024
                || path.starts_with('/')
                || path.contains(':')
                || path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err("invalid motion resource path".into());
            }
            // The source forcibly substitutes.MOT even when VUE names a.vue.
            if let Some(dot) = path
                .rfind('.')
                .filter(|dot| *dot > path.rfind('/').unwrap_or(0))
            {
                path.truncate(dot);
            }
            path.push_str(".MOT");
            let fps = integer(&section, "FPS", 15)?;
            let declared_frame_count = integer(&section, "FrameCount", 0)?;
            if fps == 0 || fps > 1000 || declared_frame_count > 1_000_000 {
                return Err("invalid motion timing metadata".into());
            }
            let transition_time = text(&section, "TransitionTime")
                .map(str::parse::<f32>)
                .transpose()
                .map_err(|_| "invalid motion transition time")?
                .unwrap_or(((declared_frame_count as f32 / fps as f32) / 4.).min(0.5));
            if !transition_time.is_finite() || transition_time < 0. {
                return Err("invalid motion transition time".into());
            }
            motions.push(Motion {
                name,
                path,
                looping: boolean("Looping", false)?,
                interpolate_position: boolean("InterpolatePosition", true)?,
                interpolate_rotation: boolean("InterpolateRotation", true)?,
                fps,
                declared_frame_count,
                transition_time,
            });
        }
        Ok(Self { motions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn substitutes_resource_extension_and_uses_preload_transition_metadata() {
        let catalog = MotionCatalog::parse(b"[General info]\nNumberOfMotions=3\nContentDirectory=Anim\\Rider\\\n[MOTION 1]\nName=turn\nVUE=turn.vue\nFrameCount=12\nFPS=30\nLooping=yes\n[MOTION 2]\nName=land\nVUE=land\nFrameCount=90\nTransitionTime=0.15\nInterpolatePosition=no\n[MOTION 3]\nName=look\nVUE=look.mot\n").unwrap();
        assert_eq!(catalog.motions[0].path, "Anim/Rider/turn.MOT");
        assert_eq!(catalog.motions[0].transition_time, 0.1);
        assert!(catalog.motions[0].looping);
        assert_eq!(catalog.motions[1].transition_time, 0.15);
        assert!(!catalog.motions[1].interpolate_position);
        assert_eq!(catalog.motions[2].transition_time, 0.);
        assert_eq!(catalog.motions[2].fps, 15);
    }
    #[test]
    fn rejects_unsafe_paths_and_nonfinite_timing() {
        for suffix in [
            "VUE=../other.vue",
            "VUE=turn.vue\nTransitionTime=NaN",
            "VUE=turn.vue\nFPS=0",
        ] {
            let data =
                format!("[General info]\nNumberOfMotions=1\n[MOTION 1]\nName=turn\n{suffix}\n");
            assert!(MotionCatalog::parse(data.as_bytes()).is_err());
        }
    }
}
