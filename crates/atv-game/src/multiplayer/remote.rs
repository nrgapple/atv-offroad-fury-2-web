//! Remote riders: each one's original quad and rider, posed locally from the
//! replicated recipe (atv-web multiplayer_animation.rs). Receiving never
//! advances the player's own solver.
use crate::Result;
use crate::ride::{Model, Pose, gear, load, textures};
use atv_content::Content;
use atv_core::{
    animation, mesh, player_models,
    quad_pose::QuadPose,
    resource::Resource,
    rider_pose::{Animator, Motion, Recipe},
};
use atv_net::{RiderState, Snapshot};
use std::rc::Rc;

/// A remote rider's look: quad (1-26), rider (0 male, 1 female) and the
/// room's appearance bytes (seven gear indices and the quad paint).
pub type Key = (u16, u16, [u8; atv_net::APPEARANCE_BYTES]);

/// A loaded remote model: its drawables and the rig poses are sampled from.
pub struct RemoteModel {
    pub key: Key,
    pub quad: Model,
    pub rider: Model,
    /// Quad paint (mode, hue), `[0, 0]` for the original livery.
    pub paint: [f32; 2],
    pub rig: Rc<Rig>,
}

pub struct Rig {
    quad: QuadPose,
    nodes: Vec<mesh::Node>,
    rider: Animator,
    wrecks: Vec<Motion>,
}

/// Reads a remote rider's original quad, rider mesh, the 78 riding, stunt,
/// wreck and landing motions and both models' textures (game.js
/// `remoteAssets`, op 115's rig bundle), dressed in the rider's gear with
/// the quad's paint (remote appearance in game.js).
pub async fn load_model(content: &Content, key: Key) -> Result<RemoteModel> {
    let (quad, rider, look) = key;
    if !(1..=26).contains(&quad) || rider > 1 || !atv_net::valid_appearance(&look) {
        return Err("unsupported remote player model".into());
    }
    let appearance = gear::Appearance {
        gear: look[..7].try_into().expect("seven gear slots"),
        paint: look[7],
    };
    let models = player_models::selection(quad, rider)?;
    let quad_bytes = content.file(&models.quad).await?;
    let quad_archive = Resource::parse(&quad_bytes)?;
    let quad_mesh = mesh::decode_psg(&load::member(
        &quad_archive,
        &models.quad_mesh,
        "remote ATV mesh",
    )?)?;
    let quad_materials = content.file(&models.quad_materials).await?;
    let quad_textures = textures::load(&quad_mesh, &Resource::parse(&quad_materials)?)?;
    let rider_bytes = content.file(&models.rider).await?;
    let rider_archive = Resource::parse(&rider_bytes)?;
    let rider_mesh = mesh::decode_psg(&load::member_suffix(
        &rider_archive,
        &format!("/{}", models.rider_mesh),
    )?)?;
    let catalog = atv_core::motion_catalog::MotionCatalog::parse(&load::member_suffix(
        &rider_archive,
        &format!("/{}", models.rider_catalog),
    )?)?;
    let selected = load::riding_motions(&catalog)?;
    let motion_bytes = content.file("/ANIMSRDR.RES").await?;
    let motion_archive = Resource::parse(&motion_bytes)?;
    let mut motions = Vec::with_capacity(selected.len());
    for definition in &selected {
        let file = definition
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&definition.path);
        motions.push(Motion {
            animation: animation::decode_mot(&load::member_suffix(
                &motion_archive,
                &format!("/{file}"),
            )?)?,
            sampling: animation::Sampling {
                translation_interpolation: definition.interpolate_position,
                rotation_interpolation: definition.interpolate_rotation,
            },
        });
    }
    if motions.len() != 78 {
        return Err("remote rider motions are incomplete".into());
    }
    // riding_motions order: normal (0..14), stunt/hold (14..62), wreck
    // (62..76), landing (76..78).
    let landings = motions.split_off(76);
    let wrecks = motions.split_off(62);
    let stunts = motions.split_off(14);
    let mut animator = Animator::new(motions, &rider_mesh.nodes)?;
    animator.set_stunts(stunts, selected[14..62].iter().map(|m| m.looping).collect())?;
    animator.set_landing_motions(landings)?;
    let rider_materials = content.file("/RIDER/RIDER_MT.RES").await?;
    let rider_materials = Resource::parse(&rider_materials)?;
    let catalog = if appearance.gear.iter().any(|g| *g != 0) {
        gear::catalog(content).await?
    } else {
        None
    };
    let rider_textures = match catalog {
        Some(catalog) => {
            gear::rider_textures(&rider_mesh, &appearance, &catalog, &rider_materials)?
        }
        None => textures::load(&rider_mesh, &rider_materials)?,
    };
    let rig = Rig {
        quad: QuadPose::new(&quad_mesh)?,
        nodes: rider_mesh.nodes.clone(),
        rider: animator,
        wrecks,
    };
    Ok(RemoteModel {
        key,
        quad: Model {
            mesh: quad_mesh,
            textures: quad_textures,
        },
        rider: Model {
            mesh: rider_mesh,
            textures: rider_textures,
        },
        paint: gear::PAINT_PALETTE
            .get(usize::from(appearance.paint))
            .copied()
            .unwrap_or([0.; 2]),
        rig: Rc::new(rig),
    })
}

/// One remote rider's local animation clocks.
pub struct Local {
    rig: Rc<Rig>,
    rider: Animator,
    crash: u32,
    wreck_time: f32,
    initialized: bool,
}

impl Local {
    pub fn new(rig: Rc<Rig>) -> Result<Self> {
        Ok(Self {
            rider: rig.rider.fork_racer(&rig.nodes)?,
            rig,
            crash: 0,
            wreck_time: 0.,
            initialized: false,
        })
    }

    pub fn key_matches(&self, rig: &Rc<Rig>) -> bool {
        Rc::ptr_eq(&self.rig, rig)
    }

    /// The drawn pose of a presented snapshot `dt` seconds after the last.
    pub fn pose(&mut self, s: &Snapshot, dt: f32, stale: bool) -> Result<Pose> {
        let rig = &self.rig;
        let a = s.animation.as_ref().ok_or("remote animation missing")?;
        let quad_nodes = rig.quad.sample(a.steering, a.wheels, a.compression)?;
        let step = if a.paused || stale { 0. } else { dt };
        let blend_dt = if self.initialized { dt } else { 0.25 };
        self.initialized = true;
        let quad_world = s.atv.to_column_major();
        let (rider_world, rider_nodes) = if s.rider_state == RiderState::Wreck {
            if self.crash != s.crash_serial {
                self.crash = s.crash_serial;
                self.wreck_time = 0.;
            }
            self.wreck_time += step;
            // The source air/bounce motions are clocked on this receiver;
            // only the detached root and recovery are replicated.
            let settled = s.rider_velocity.iter().map(|v| v * v).sum::<f32>() < 4.;
            let m = &rig.wrecks[if settled {
                self.crash as usize % 4
            } else {
                4 + self.crash as usize % 4
            }];
            let time = self.wreck_time.rem_euclid(m.animation.duration.max(0.001));
            (
                s.rider.to_column_major(),
                m.animation
                    .sample_pose(&rig.nodes, time, m.sampling)?
                    .world_matrices,
            )
        } else {
            self.wreck_time = 0.;
            (
                quad_world,
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
                    .world_matrices,
            )
        };
        Ok(Pose {
            quad_world,
            quad_nodes,
            rider_world,
            rider_nodes,
        })
    }
}
