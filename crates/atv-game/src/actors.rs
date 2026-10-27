//! Keyframed TOY actors (research/AMBIENT_ACTORS.md): the venue's crows,
//! fish, windmills, blimps, twisters and other animated props from the
//! common and mode TOYs, each an `anim/` PSG posed by its looping MOT.
//! Repeated models and motions are decoded once and shared.
use crate::{
    Result,
    materials::{Material, Materials, member_at},
};
use atv_core::{
    animation::{Animation, Sampling},
    mesh::{self, Node},
    resource::Resource,
    toy_actors::{self, Definition, Mcf},
};
use std::rc::Rc;

/// Rigid-node skinning keeps the train renderer's 32-matrix limit.
pub const MAX_NODES: usize = 32;

/// One material pass of one PSG group, in node-local coordinates.
pub struct Chunk {
    /// Node-local position, node-local normal, u, v, node per vertex.
    pub vertices: Vec<f32>,
    /// Authored vertex RGBA for prelit passes, 128 otherwise.
    pub colors: Vec<[u8; 4]>,
    pub indices: Vec<u32>,
    pub material: Rc<Material>,
    pub pass_index: usize,
}

impl Chunk {
    pub fn pass(&self) -> &crate::materials::MaterialPass {
        &self.material.passes[self.pass_index]
    }
}

pub struct Model {
    /// PSG stem (`crow`, `blimp_06`).
    pub name: String,
    pub nodes: Vec<Node>,
    pub chunks: Vec<Chunk>,
    /// Per node: node-local vertex bounds (None for nodes without geometry).
    pub node_bounds: Vec<Option<([f32; 3], [f32; 3])>>,
}

pub struct Clip {
    pub name: String,
    pub animation: Animation,
    pub looping: bool,
}

pub struct Actor {
    pub definition: Definition,
    pub model: usize,
    /// The playing motion (None: the model holds its bind pose).
    pub clip: Option<usize>,
    /// `Offset`/`Look`/`Up` placement.
    pub placement: [f32; 16],
    /// Whether the starting sequence plays (`SequenceActive`); otherwise
    /// the actor holds its motion's t=0 pose.
    pub playing: bool,
    /// The motion repeats: an MCF `Looping` motion, or a one-shot motion
    /// whose sequence loops (`0020cba0` restarts it when it stops).
    pub looping: bool,
    /// Seconds into the motion at scene time zero.
    pub phase: f64,
}

/// A keyframed actor's looping sound (the blimp, airliner, plane and
/// helicopter engines). `definition.car` is this sound's index in
/// `Actors::sounds` and in `Actors::sound_emitters`.
#[derive(Clone, Debug)]
pub struct ActorSound {
    pub actor: usize,
    /// The `AttachToObject` PSG node; None when the model has no node of
    /// that name (the sound then follows the actor root).
    pub node: Option<usize>,
    pub definition: atv_core::spatial_audio::Definition,
}

/// A resolved actor's pose for drawing.
pub struct Pose {
    pub placement: [f32; 16],
    /// Model-space node matrices (bind or MOT-posed).
    pub nodes: Rc<Vec<[f32; 16]>>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Default)]
pub struct Actors {
    pub models: Vec<Model>,
    pub clips: Vec<Clip>,
    pub actors: Vec<Actor>,
    /// Records that could not be resolved: `[AnimationN]` MCF and reason.
    pub unresolved: Vec<(String, String)>,
    /// Every actor's `[AnimationNLoopSoundM]`, in actor order.
    pub sounds: Vec<ActorSound>,
    /// Scene clock: whole 60 Hz steps.
    steps: u64,
    /// Poses of the current step, shared by actors on the same model,
    /// motion and time (`(model, clip, time bits)` → node matrices).
    cache: std::cell::RefCell<Vec<(PoseKey, NodePose)>>,
}

/// Model, motion and motion-time bits of a cached pose.
type PoseKey = (usize, Option<usize>, u32);
type NodePose = Rc<Vec<[f32; 16]>>;

const STEP: f64 = 1. / 60.;

fn find_member(archive: &Resource, name: &str) -> Option<usize> {
    member_at(archive, &format!("anim/{name}"))
        .or_else(|| member_at(archive, name))
        .or_else(|| crate::materials::member_named(archive, name))
}

impl Actors {
    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    /// Scene time in seconds.
    pub fn time(&self) -> f64 {
        self.steps as f64 * STEP
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// One 60 Hz simulation step.
    pub fn step(&mut self) {
        self.steps += 1;
        self.cache.get_mut().clear();
    }

    /// The motion time of `actor` at the current step.
    pub fn motion_time(&self, actor: usize) -> f32 {
        let a = &self.actors[actor];
        match a.clip.map(|c| &self.clips[c]) {
            Some(clip) if a.playing => {
                toy_actors::motion_time(self.time() + a.phase, clip.animation.duration, a.looping)
            }
            _ => 0.,
        }
    }

    /// Each sound's emitter world matrix at this step and one step before
    /// (the attached node posed by the actor's motion, then placed).
    pub fn sound_emitters(&self) -> Vec<([f32; 16], [f32; 16])> {
        self.sounds
            .iter()
            .map(|sound| {
                let at = |steps: u64| self.emitter(sound, steps);
                (at(self.steps), at(self.steps.saturating_sub(1)))
            })
            .collect()
    }

    fn emitter(&self, sound: &ActorSound, steps: u64) -> [f32; 16] {
        let a = &self.actors[sound.actor];
        let Some(node) = sound.node else {
            return a.placement;
        };
        let model = &self.models[a.model];
        let time = match a.clip.map(|c| &self.clips[c]) {
            Some(clip) if a.playing => toy_actors::motion_time(
                steps as f64 * STEP + a.phase,
                clip.animation.duration,
                a.looping,
            ),
            _ => 0.,
        };
        let local = a
            .clip
            .and_then(|c| {
                self.clips[c]
                    .animation
                    .sample_pose(&model.nodes, time, Sampling::default())
                    .ok()
            })
            .and_then(|p| p.world_matrices.get(node).copied())
            .unwrap_or(model.nodes[node].world_matrix);
        multiply(&a.placement, &local)
    }

    /// `actor`'s placement, node matrices and world bounds at this step.
    pub fn pose(&self, actor: usize) -> Option<Pose> {
        let a = self.actors.get(actor)?;
        let model = &self.models[a.model];
        let time = self.motion_time(actor);
        let key = (a.model, a.clip, time.to_bits());
        let cached = self
            .cache
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, p)| p.clone());
        let nodes = match cached {
            Some(nodes) => nodes,
            None => {
                let nodes = match a.clip.map(|c| &self.clips[c]) {
                    Some(clip) => {
                        clip.animation
                            .sample_pose(&model.nodes, time, Sampling::default())
                            .ok()?
                            .world_matrices
                    }
                    None => model.nodes.iter().map(|n| n.world_matrix).collect(),
                };
                let nodes = Rc::new(nodes);
                self.cache.borrow_mut().push((key, nodes.clone()));
                nodes
            }
        };
        let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for (node, bounds) in nodes.iter().zip(&model.node_bounds) {
            let Some((lo, hi)) = bounds else { continue };
            let world = multiply(&a.placement, node);
            let (l, h) = crate::scenery::world_bounds(*lo, *hi, &world);
            for i in 0..3 {
                min[i] = min[i].min(l[i]);
                max[i] = max[i].max(h[i]);
            }
        }
        Some(Pose {
            placement: a.placement,
            nodes,
            min,
            max,
        })
    }
}

/// Column-major `a * b`.
pub fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| {
        let (column, row) = (i / 4, i % 4);
        (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum()
    })
}

/// Reads the keyframed actors of the venue's common TOY and the mode TOY.
/// `skip` leaves records other systems own (Treasure Hunt's bombs, the
/// locomotive wheels) out.
pub fn load(
    archive: &Resource,
    toys: &[&[u8]],
    skip: &dyn Fn(&Definition) -> bool,
) -> Result<Actors> {
    let mut actors = Actors::default();
    let mut materials = Materials::default();
    let mut mcfs: Vec<(String, Option<Mcf>)> = Vec::new();
    for toy in toys {
        for definition in toy_actors::parse(&String::from_utf8_lossy(toy)) {
            if skip(&definition) {
                continue;
            }
            let label = format!("Animation{} {}", definition.index, definition.mcf);
            let mcf = match mcfs.iter().find(|(n, _)| *n == definition.mcf) {
                Some((_, m)) => m.clone(),
                None => {
                    let m = find_member(archive, &definition.mcf)
                        .and_then(|i| archive.extract(i).ok())
                        .and_then(|b| Mcf::parse(&String::from_utf8_lossy(&b)).ok());
                    mcfs.push((definition.mcf.clone(), m.clone()));
                    m
                }
            };
            let Some(mcf) = mcf else {
                actors
                    .unresolved
                    .push((label, "MCF missing or invalid".into()));
                continue;
            };
            let model = match actors.models.iter().position(|m| m.name == mcf.model) {
                Some(m) => m,
                None => match load_model(archive, &mcf.model, &mut materials) {
                    Ok(model) => {
                        actors.models.push(model);
                        actors.models.len() - 1
                    }
                    Err(error) => {
                        actors.unresolved.push((label, error));
                        continue;
                    }
                },
            };
            // The starting sequence's first motion (`MotionToPlay` index).
            let start = definition.start_sequence();
            let (playing, sequence_loops) = start.map_or((false, false), |(s, p)| (p, s.looping));
            let motion = start
                .and_then(|(s, _)| s.motions.first())
                .and_then(|&n| usize::try_from(n - 1).ok())
                .and_then(|n| definition.motions.get(n));
            let clip = match motion {
                None => None,
                Some(name) => {
                    match load_clip(
                        archive,
                        &mcf,
                        name,
                        &actors.models[model],
                        &mut actors.clips,
                    ) {
                        Ok(clip) => Some(clip),
                        Err(error) => {
                            actors.unresolved.push((label, error));
                            continue;
                        }
                    }
                }
            };
            let placement = match definition.matrix() {
                Ok(m) => m,
                Err(error) => {
                    actors.unresolved.push((label, error));
                    continue;
                }
            };
            let looping = clip.is_some_and(|c| actors.clips[c].looping) || sequence_loops;
            for sound in &definition.loop_sounds {
                let node = actors.models[model]
                    .nodes
                    .iter()
                    .position(|n| n.name.eq_ignore_ascii_case(&sound.attach));
                actors.sounds.push(ActorSound {
                    actor: actors.actors.len(),
                    node,
                    definition: atv_core::spatial_audio::Definition {
                        car: actors.sounds.len(),
                        sample: sound.sample.clone(),
                        min_distance: sound.min_distance,
                        max_distance: sound.max_distance,
                        looping: sound.looping,
                        start_disabled: sound.start_disabled,
                        position: None,
                    },
                });
            }
            actors.actors.push(Actor {
                definition,
                model,
                clip,
                placement,
                playing,
                looping,
                // `0023cdc0` starts every motion at t=0: no random phase.
                phase: 0.,
            });
        }
    }
    Ok(actors)
}

fn load_clip(
    archive: &Resource,
    mcf: &Mcf,
    name: &str,
    model: &Model,
    clips: &mut Vec<Clip>,
) -> Result<usize> {
    let name = name.to_ascii_lowercase();
    let looping = mcf.motion(&name).is_none_or(|m| m.looping);
    if let Some(at) = clips
        .iter()
        .position(|c| c.name == name && c.looping == looping)
    {
        // A shared clip must also bind to this model's nodes.
        clips[at]
            .animation
            .sample_pose(&model.nodes, 0., Sampling::default())?;
        return Ok(at);
    }
    let member = find_member(archive, &format!("{name}.mot"))
        .ok_or_else(|| format!("motion {name}.mot missing"))?;
    let animation = atv_core::animation::decode_mot(&archive.extract(member)?)?;
    animation.sample_pose(&model.nodes, 0., Sampling::default())?;
    clips.push(Clip {
        name,
        animation,
        looping,
    });
    Ok(clips.len() - 1)
}

fn load_model(archive: &Resource, name: &str, materials: &mut Materials) -> Result<Model> {
    let member = find_member(archive, &format!("{name}.psg"))
        .ok_or_else(|| format!("model {name}.psg missing"))?;
    let mesh = mesh::decode_psg(&archive.extract(member)?)?;
    if mesh.nodes.is_empty() || mesh.nodes.len() > MAX_NODES {
        return Err(format!(
            "{name}: {} nodes exceed the pose capacity",
            mesh.nodes.len()
        ));
    }
    let lod = mesh.lods.first().ok_or("actor PSG has no LOD")?;
    let mut chunks = Vec::new();
    let mut node_bounds: Vec<Option<([f32; 3], [f32; 3])>> = vec![None; mesh.nodes.len()];
    for group in &lod.groups {
        if group.vertices.is_empty() || group.indices.is_empty() {
            continue;
        }
        if group
            .indices
            .iter()
            .any(|&i| i as usize >= group.vertices.len())
        {
            return Err("actor PSG index exceeds group".into());
        }
        let material_name = mesh
            .materials
            .get(group.material as usize)
            .ok_or("actor material is missing")?;
        let material = materials.get(archive, material_name, true)?;
        for v in &group.vertices {
            let node = usize::from(v.node);
            let bounds = node_bounds
                .get_mut(node)
                .ok_or("actor mesh node is missing")?;
            let (lo, hi) = bounds.get_or_insert(([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]));
            for a in 0..3 {
                lo[a] = lo[a].min(v.local_position[a]);
                hi[a] = hi[a].max(v.local_position[a]);
            }
        }
        for (pass_index, pass) in material.passes.iter().enumerate() {
            // Reflection programs (local-normal projections) are scenery
            // features the actor pass does not draw.
            if pass.reflective || pass.unsupported_reflection {
                continue;
            }
            let prelit = pass.pass.as_ref().is_some_and(|p| p.prelit);
            let mut vertices = Vec::with_capacity(group.vertices.len() * 9);
            let mut colors = Vec::with_capacity(group.vertices.len());
            for v in &group.vertices {
                vertices.extend(v.local_position);
                vertices.extend(v.local_normal);
                vertices.extend(v.uv);
                vertices.push(f32::from(v.node));
                colors.push(if prelit {
                    v.color.unwrap_or([128; 4])
                } else {
                    [128; 4]
                });
            }
            chunks.push(Chunk {
                vertices,
                colors,
                indices: group.indices.clone(),
                material: material.clone(),
                pass_index,
            });
        }
    }
    Ok(Model {
        name: name.to_string(),
        nodes: mesh.nodes,
        chunks,
        node_bounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_major_products_compose_translation_after_rotation() {
        // 90 degrees about Y, then a translation.
        let rotate = [
            0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 0., 0., 0., 1.,
        ];
        let mut translate = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        translate[12..15].copy_from_slice(&[1., 2., 3.]);
        let m = multiply(&translate, &rotate);
        // The rotated +X axis, then the offset.
        assert_eq!(&m[0..3], &[0., 0., -1.]);
        assert_eq!(&m[12..15], &[1., 2., 3.]);
        let n = multiply(&rotate, &translate);
        assert_eq!(&n[12..15], &[3., 2., -1.]);
    }
}
