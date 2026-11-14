//! Remote riders: each one's original quad and rider, posed locally from the
//! replicated recipe (atv-web multiplayer_animation.rs), or a motorcycle and
//! its MX rider, posed from the replicated linkage and bones
//! (research/MULTIPLAYER.md "Motorcycles"). Receiving never advances the
//! player's own solver.
use crate::Result;
use crate::ride::{Model, Pose, gear, load, textures};
use atv_content::Content;
use atv_core::{
    animation, mesh, motorcycle, player_models,
    quad_pose::QuadPose,
    resource::Resource,
    rider_pose::{Animator, Motion, Recipe},
};
use atv_net::{RiderState, Snapshot};
use std::rc::Rc;

/// One rider sex's mesh, motion looping flags and decoded motions.
struct RiderShared {
    mesh: mesh::Mesh,
    looping: Vec<bool>,
    motions: Vec<Motion>,
}

type Outfit = std::collections::HashMap<String, textures::MaterialTexture>;
type Outfits = std::collections::HashMap<(u16, [u8; 7]), std::rc::Rc<Outfit>>;

thread_local! {
    /// Rider rigs by sex (0 male, 1 female), read once per session.
    static RIDERS: std::cell::RefCell<std::collections::HashMap<u16, std::rc::Rc<RiderShared>>> =
        Default::default();
    /// Decoded rider textures by sex and gear.
    static OUTFITS: std::cell::RefCell<Outfits> =
        Default::default();
}

/// A remote rider's look: vehicle (ATV 1-26, motorcycle 27-46), rider (0
/// male, 1 female) and the room's appearance bytes (seven gear indices and
/// the quad paint; a motorcycle keeps its original look).
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

pub enum Rig {
    Quad(QuadRig),
    /// The bike's linkage solver (its meshes, motions and tuning come along).
    Bike(Box<motorcycle::Model>),
}

pub struct QuadRig {
    quad: QuadPose,
    nodes: Vec<mesh::Node>,
    rider: Animator,
    wrecks: Vec<Motion>,
}

/// A remote motorcycle: the bike and its MX rider from the MX archive, drawn
/// as the player's own bike is (ride/load.rs).
async fn load_bike(content: &Content, key: Key) -> Result<RemoteModel> {
    let (vehicle, rider, _) = key;
    let models = player_models::selection(vehicle, rider)?;
    let bytes = content.file(&models.quad).await?;
    crate::yield_frame().await;
    let model = motorcycle::Model::load(&bytes, vehicle)?;
    crate::yield_frame().await;
    let archive = Resource::parse(&bytes)?;
    let quad = Model {
        textures: textures::load(&model.mesh, &archive)?,
        mesh: model.mesh.clone(),
    };
    crate::yield_frame().await;
    let rider = Model {
        textures: textures::load(&model.rider, &archive)?,
        mesh: model.rider.clone(),
    };
    Ok(RemoteModel {
        key,
        quad,
        rider,
        paint: [0.; 2],
        rig: Rc::new(Rig::Bike(Box::new(model))),
    })
}

/// Reads a remote rider's original quad, rider mesh, the 78 riding, stunt,
/// wreck and landing motions and both models' textures (game.js
/// `remoteAssets`, op 115's rig bundle), dressed in the rider's gear with
/// the quad's paint (remote appearance in game.js).
pub async fn load_model(content: &Content, key: Key) -> Result<RemoteModel> {
    let (quad, rider, look) = key;
    if !atv_net::valid_actor(quad, rider) || !atv_net::valid_appearance(&look) {
        return Err("unsupported remote player model".into());
    }
    if atv_net::is_motorcycle(quad) {
        return load_bike(content, key).await;
    }
    let appearance = gear::Appearance {
        gear: look[..7].try_into().expect("seven gear slots"),
        paint: look[7],
    };
    let models = player_models::selection(quad, rider)?;
    let quad_bytes = content.file(&models.quad).await?;
    let quad_archive = Resource::parse(&quad_bytes)?;
    let quad_member = load::member(&quad_archive, &models.quad_mesh, "remote ATV mesh")?;
    crate::yield_frame().await;
    let quad_mesh = mesh::decode_psg(&quad_member)?;
    crate::yield_frame().await;
    let quad_materials = content.file(&models.quad_materials).await?;
    let quad_textures =
        textures::load_spread(&quad_mesh, &Resource::parse(&quad_materials)?).await?;
    crate::yield_frame().await;
    // Every remote rider of one sex shares the rider mesh, motion catalog
    // and 78 motions: read them once per session.
    let cached = RIDERS.with_borrow(|m| m.get(&rider).cloned());
    let shared = match cached {
        Some(shared) => shared,
        None => {
            let rider_bytes = content.file(&models.rider).await?;
            let rider_archive = Resource::parse(&rider_bytes)?;
            let mesh = mesh::decode_psg(&load::member_suffix(
                &rider_archive,
                &format!("/{}", models.rider_mesh),
            )?)?;
            let catalog = atv_core::motion_catalog::MotionCatalog::parse(&load::member_suffix(
                &rider_archive,
                &format!("/{}", models.rider_catalog),
            )?)?;
            let selected = load::riding_motions(&catalog)?;
            crate::yield_frame().await;
            let motion_bytes = content.file("/ANIMSRDR.RES").await?;
            let motion_archive = Resource::parse(&motion_bytes)?;
            let mut motions = Vec::with_capacity(selected.len());
            for (index, definition) in selected.iter().enumerate() {
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
                if index % 8 == 7 {
                    crate::yield_frame().await;
                }
            }
            let shared = std::rc::Rc::new(RiderShared {
                mesh,
                looping: selected.iter().map(|m| m.looping).collect(),
                motions,
            });
            RIDERS.with_borrow_mut(|m| m.insert(rider, shared.clone()));
            shared
        }
    };
    let rider_mesh = shared.mesh.clone();
    crate::yield_frame().await;
    let mut motions = shared.motions.clone();
    if motions.len() != 78 {
        return Err("remote rider motions are incomplete".into());
    }
    // riding_motions order: normal (0..14), stunt/hold (14..62), wreck
    // (62..76), landing (76..78).
    let landings = motions.split_off(76);
    let wrecks = motions.split_off(62);
    let stunts = motions.split_off(14);
    let mut animator = Animator::new(motions, &rider_mesh.nodes)?;
    animator.set_stunts(stunts, shared.looping[14..62].to_vec())?;
    animator.set_landing_motions(landings)?;
    crate::yield_frame().await;
    // Riders of one sex in the same gear share their decoded textures.
    let look_key = (rider, appearance.gear);
    let cached = OUTFITS.with_borrow(|m| m.get(&look_key).cloned());
    let rider_textures = match cached {
        Some(textures) => (*textures).clone(),
        None => {
            let rider_materials = content.file("/RIDER/RIDER_MT.RES").await?;
            let rider_materials = Resource::parse(&rider_materials)?;
            let catalog = if appearance.gear.iter().any(|g| *g != 0) {
                gear::catalog(content).await?
            } else {
                None
            };
            let textures = match catalog {
                Some(catalog) => {
                    gear::rider_textures(&rider_mesh, &appearance, &catalog, &rider_materials)?
                }
                None => textures::load_spread(&rider_mesh, &rider_materials).await?,
            };
            OUTFITS.with_borrow_mut(|m| m.insert(look_key, std::rc::Rc::new(textures.clone())));
            textures
        }
    };
    let rig = Rig::Quad(QuadRig {
        quad: QuadPose::new(&quad_mesh)?,
        nodes: rider_mesh.nodes.clone(),
        rider: animator,
        wrecks,
    });
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
    /// The ATV rider's local animator (a motorcycle's rider is sent posed).
    rider: Option<Animator>,
    crash: u32,
    wreck_time: f32,
    initialized: bool,
}

impl Local {
    pub fn new(rig: Rc<Rig>) -> Result<Self> {
        Ok(Self {
            rider: match &*rig {
                Rig::Quad(quad) => Some(quad.rider.fork_racer(&quad.nodes)?),
                Rig::Bike(_) => None,
            },
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
        let a = s.animation.as_ref().ok_or("remote animation missing")?;
        let rig = match &*self.rig {
            Rig::Quad(rig) => rig,
            Rig::Bike(bike) => {
                // The sender's linkage and rider bones, posed as its own
                // bike draws them; the rider leaves the bike in a wreck.
                if !atv_net::is_motorcycle(s.atv_model)
                    || s.rider_nodes.len() != bike.rider.nodes.len()
                {
                    return Err("remote motorcycle pose does not fit its rider".into());
                }
                let quad_world = s.atv.to_column_major();
                return Ok(Pose {
                    quad_world,
                    quad_nodes: bike.pose(
                        a.steering,
                        [a.wheels[0], a.wheels[1]],
                        [a.compression[0], a.compression[1]],
                    ),
                    rider_world: if s.rider_state == RiderState::Wreck {
                        s.rider.to_column_major()
                    } else {
                        quad_world
                    },
                    rider_nodes: s.rider_nodes.iter().map(|t| t.to_column_major()).collect(),
                });
            }
        };
        let rider = self.rider.as_mut().ok_or("remote rider animator missing")?;
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
                rider
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
