//! Original frontend rider meshes and MOT clips, independent of ride simulation.
//! Clip sampling is recovered; the ordered breathing/idle playlist is an authored
//! menu adaptation until the native selection/delay callback is reconstructed.
use crate::{
    Result,
    animation::{self, Animation, Pose, Sampling},
    mesh::{self, Mesh},
    resource::Resource,
};

pub struct Rider {
    pub mesh: Mesh,
    clips: Vec<(String, Animation)>,
    duration: f32,
}
impl Rider {
    pub fn from_resource(bytes: &[u8], female: bool) -> Result<Self> {
        let archive = Resource::parse(bytes)?;
        let stem = if female { "frider" } else { "mrider" };
        let extract = |name: &str| -> Result<Vec<u8>> {
            let matches: Vec<_> = archive
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    m.name
                        .rsplit('/')
                        .next()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
                .collect();
            if matches.len() != 1 {
                return Err(format!("menu rider member missing or duplicated: {name}"));
            }
            archive.extract(matches[0].0)
        };
        let mesh = mesh::decode_psg(&extract(&format!("{stem}_ui.psg"))?)?;
        if mesh.nodes.len() > 32 {
            return Err("menu rig exceeds GPU pose capacity".into());
        }
        let names = std::iter::once(format!("{stem}_body_breathe.mot"))
            .chain((1..=if female { 5 } else { 7 }).map(|n| format!("{stem}_body_idle{n:02}.mot")));
        let mut clips = vec![];
        for name in names {
            let clip = animation::decode_mot(&extract(&name)?)?;
            if clip.duration <= 0. {
                return Err("menu clip has no duration".into());
            }
            clip.sample_pose(&mesh.nodes, 0., Sampling::default())?;
            clips.push((name, clip));
        }
        Self::from_clips(mesh, clips)
    }
    /// Shared menu playback for a recovered rig and its own original clips.
    pub fn from_clips(mesh: Mesh, clips: Vec<(String, Animation)>) -> Result<Self> {
        if clips.is_empty() || mesh.nodes.len() > 32 {
            return Err("invalid menu rider playlist".into());
        }
        for (_, clip) in &clips {
            if clip.duration <= 0. || !clip.duration.is_finite() {
                return Err("menu clip has no duration".into());
            }
            clip.sample_pose(&mesh.nodes, 0., Sampling::default())?;
        }
        let duration = clips.iter().map(|(_, clip)| clip.duration).sum();
        Ok(Self {
            mesh,
            clips,
            duration,
        })
    }
    pub fn pose(&self, time: f32) -> Result<(String, Pose)> {
        if !time.is_finite() || time < 0. {
            return Err("invalid menu animation time".into());
        }
        let mut cursor = time % self.duration;
        for (name, clip) in &self.clips {
            if cursor < clip.duration {
                return Ok((
                    name.clone(),
                    clip.sample_pose(&self.mesh.nodes, cursor, Sampling::default())?,
                ));
            }
            cursor -= clip.duration;
        }
        Err("menu animation cursor outside playlist".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Track, TranslationKey};
    #[test]
    fn playlist_wraps_source_time_and_rejects_invalid_clocks() {
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let clip = |from, to| Animation {
            version: 3,
            frame_count: 30,
            duration: 1.,
            bone_names: vec!["pelvis".into()],
            translation_kind: 0,
            translations: vec![Track {
                bone: 0,
                keys: vec![
                    TranslationKey {
                        time: 0.,
                        value: [from, 0., 0.],
                        tangents: None,
                    },
                    TranslationKey {
                        time: 1.,
                        value: [to, 0., 0.],
                        tangents: None,
                    },
                ],
            }],
            rotations: vec![],
        };
        let rider = Rider {
            mesh: Mesh {
                nodes: vec![mesh::Node {
                    name: "pelvis".into(),
                    parent: -1,
                    local_matrix: identity,
                    world_matrix: identity,
                    bounds: [0.; 7],
                }],
                materials: vec![],
                lods: vec![],
                trailing_bytes: 0,
            },
            clips: vec![
                ("breathe".into(), clip(0., 2.)),
                ("idle".into(), clip(10., 12.)),
            ],
            duration: 2.,
        };
        for (time, name, x) in [
            (0.5, "breathe", 1.),
            (1.5, "idle", 11.),
            (2.5, "breathe", 1.),
        ] {
            let (sample, pose) = rider.pose(time).unwrap();
            assert_eq!(sample, name);
            assert_eq!(pose.world_matrices[0][12], x);
        }
        for time in [-1., f32::NAN, f32::INFINITY] {
            assert!(rider.pose(time).is_err());
        }
    }
}
