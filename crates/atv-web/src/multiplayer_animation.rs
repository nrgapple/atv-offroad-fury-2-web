//! Remote presentation reconstructed locally; never advances the player's solver.
use atv_core::{
    Result, animation, mesh,
    quad_pose::QuadPose,
    resource::Resource,
    rider_pose::{Animator, Motion, Recipe},
};
use atv_net::{RiderState, Snapshot};
use std::rc::Rc;
type Matrices = Vec<[f32; 16]>;
type Pose = (Matrices, Matrices);
pub struct Rig {
    pub quad: Rc<QuadPose>,
    pub nodes: Vec<mesh::Node>,
    pub rider: Animator,
    pub wrecks: Vec<Motion>,
}
pub struct Local {
    rider: Animator,
    crash: u32,
    wreck_time: f32,
    initialized: bool,
}
fn member(res: &Resource<'_>, name: &str) -> Result<Vec<u8>> {
    let i = res
        .members
        .iter()
        .position(|m| {
            m.name
                .split('/')
                .next_back()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .ok_or_else(|| format!("remote resource member missing: {name}"))?;
    res.extract(i)
}
pub fn load(bytes: &[u8], key: usize) -> Result<Rig> {
    if !(2..=53).contains(&key) || bytes.len() > 128 * 1024 * 1024 {
        return Err("invalid remote rig bundle".into());
    }
    let selected = atv_core::player_models::selection((key / 2) as u16, (key % 2) as u16)?;
    let mut at: usize = 12;
    let mut parts = Vec::new();
    for i in 0..3 {
        let size = u32::from_le_bytes(
            bytes
                .get(i * 4..i * 4 + 4)
                .ok_or("truncated rig header")?
                .try_into()
                .map_err(|_| "rig header")?,
        ) as usize;
        let end = at.checked_add(size).ok_or("rig size overflow")?;
        parts.push(bytes.get(at..end).ok_or("truncated rig bundle")?);
        at = end;
    }
    if at != bytes.len() {
        return Err("remote rig bundle has trailing bytes".into());
    }
    let quad = Resource::parse(parts[0])?;
    let rider = Resource::parse(parts[1])?;
    let animations = Resource::parse(parts[2])?;
    let quad = Rc::new(QuadPose::new(&mesh::decode_psg(&member(
        &quad,
        &selected.quad_mesh,
    )?)?)?);
    let nodes = mesh::decode_psg(&member(&rider, &selected.rider_mesh)?)?.nodes;
    let catalog =
        atv_core::motion_catalog::MotionCatalog::parse(&member(&rider, &selected.rider_catalog)?)?;
    let prefix = if key % 2 == 1 { "F" } else { "M" };
    let mut suffixes = atv_core::rider_pose::Clip::ALL
        .iter()
        .map(|c| c.suffix().to_string())
        .collect::<Vec<_>>();
    for kind in ["Stunt", "Hold"] {
        for suffix in atv_core::stunt::SUFFIXES {
            suffixes.push(format!("{kind}{suffix}"));
        }
    }
    suffixes.extend(
        atv_core::rider_pose::Landing::SUFFIXES
            .iter()
            .map(|s| s.to_string()),
    );
    suffixes.extend(
        atv_core::wreck_animation::SUFFIXES
            .iter()
            .map(|s| s.to_string()),
    );
    let mut looping = Vec::new();
    let mut motions = Vec::new();
    for suffix in suffixes {
        let name = format!("{prefix}_{suffix}");
        let m = catalog
            .motions
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("missing remote motion {name}"))?;
        looping.push(m.looping);
        motions.push(Motion {
            animation: animation::decode_mot(&member(
                &animations,
                m.path
                    .split(['\\', '/'])
                    .next_back()
                    .ok_or("invalid remote motion path")?,
            )?)?,
            sampling: animation::Sampling {
                translation_interpolation: m.interpolate_position,
                rotation_interpolation: m.interpolate_rotation,
            },
        });
    }
    let wrecks = motions.split_off(64);
    let landings = motions.split_off(62);
    let stunts = motions.split_off(14);
    let mut rider = Animator::new(motions, &nodes)?;
    rider.set_stunts(stunts, looping[14..62].to_vec())?;
    rider.set_landing_motions(landings)?;
    Ok(Rig {
        quad,
        nodes,
        rider,
        wrecks,
    })
}
impl Local {
    pub fn new(rig: &Rig) -> Result<Self> {
        Ok(Self {
            rider: rig.rider.fork_racer(&rig.nodes)?,
            crash: 0,
            wreck_time: 0.,
            initialized: false,
        })
    }
    pub fn pose(&mut self, rig: &Rig, s: &Snapshot, dt: f32, stale: bool) -> Result<Pose> {
        let a = s.animation.as_ref().ok_or("remote animation missing")?;
        let quad = rig.quad.sample(a.steering, a.wheels, a.compression)?;
        let step = if a.paused || stale { 0. } else { dt };
        let blend_dt = if self.initialized { dt } else { 0.25 };
        self.initialized = true;
        let rider = if s.rider_state == RiderState::Wreck {
            if self.crash != s.crash_serial {
                self.crash = s.crash_serial;
                self.wreck_time = 0.;
            }
            self.wreck_time += step;
            // The source air/bounce motions are intentionally clocked on this
            // receiver. Only detached root position and recovery are replicated.
            let settled = s.rider_velocity.iter().map(|v| v * v).sum::<f32>() < 4.;
            let m = &rig.wrecks[if settled {
                self.crash as usize % 4
            } else {
                4 + self.crash as usize % 4
            }];
            let time = self.wreck_time.rem_euclid(m.animation.duration.max(0.001));
            m.animation
                .sample_pose(&rig.nodes, time, m.sampling)?
                .world_matrices
        } else {
            self.wreck_time = 0.;
            self.rider
                .present(
                    &rig.nodes,
                    Recipe {
                        clips: a.clips,
                        count: a.count,
                        phase: a.phase,
                        first_weight: a.first_weight,
                        pair_weight: a.pair_weight,
                        look_phase: a.look_phase,
                    },
                    blend_dt,
                )?
                .world_matrices
        };
        Ok((quad, rider))
    }
}
