//! Static scene models: SCN placements of root-relative PSG meshes and their
//! ordered material passes (buildMesh's terrain.world branch in web/game.js).
use crate::{
    Result,
    materials::{Material, Materials, member_at},
};
use atv_core::{
    mesh,
    model_lod::ModelLod,
    resource::Resource,
    scene::Scene,
    scenery_material::{Scroll, TextureClock},
};
use std::rc::Rc;

/// One LOD group drawn with one material pass.
pub struct Chunk {
    /// The PSG level of detail (0 finest) this group belongs to.
    pub lod: u8,
    /// x y z, nx ny nz, u v per vertex.
    pub vertices: Vec<f32>,
    /// Authored vertex RGBA (prelit passes), 128 per channel otherwise.
    pub colors: Vec<[u8; 4]>,
    /// Reflection passes: local position, local normal and node per vertex.
    pub reflection: Vec<f32>,
    pub indices: Vec<u32>,
    pub material: Rc<Material>,
    /// Base material name: UV scroll and ATM clocks are keyed by pass name.
    pub material_name: String,
    pub pass_index: usize,
    /// Node world matrices for reflection passes.
    pub nodes: Vec<[f32; 16]>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Chunk {
    pub fn pass(&self) -> &crate::materials::MaterialPass {
        &self.material.passes[self.pass_index]
    }
    /// Texture clock key (materialPassKey in web/scenery-materials.js).
    pub fn pass_key(&self) -> String {
        if self.pass_index == 0 {
            self.material_name.clone()
        } else {
            format!("{}#pass{}", self.material_name, self.pass_index + 1)
        }
    }
}

pub struct Draw {
    pub chunk: usize,
    /// The scene model this draw belongs to (index into `Scene::models`).
    pub model: usize,
    pub matrix: [f32; 16],
    pub unlit: bool,
    /// Stadium camera-flash pool index (hidden until lit), or None.
    pub flash: Option<usize>,
    /// Part of the race's drop gate (posed each step after GO).
    pub gate: bool,
    /// Part of Enduro gate N (faded unless it is the rider's next gate).
    pub enduro_gate: Option<usize>,
    /// The chunk's level of detail (only the model's selected level draws).
    pub lod: u8,
    /// A later pass of a material with `EnableMultiPassCull`: skipped while
    /// the model is beyond 130 ft (`001b2390` -> `001b90b0`).
    pub far_cull: bool,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Default)]
pub struct Scenery {
    pub chunks: Vec<Chunk>,
    /// Scene order: models as listed, then their groups and passes.
    pub draws: Vec<Draw>,
    pub unsupported: Vec<(String, String)>,
    pub skipped_groups: usize,
    /// Stadium `[CameraFlash]` settings and the cameraflash pool size.
    pub flashes: Option<(atv_core::race_crowd::FlashConfig, usize)>,
    /// Per scene model: its LOD thresholds and bounds (`001b3a90`), None
    /// when its mesh is unsupported.
    pub lods: Vec<Option<ModelLod>>,
}

/// An Enduro gate's scene model and the banner textures that replace its
/// `frontarrow`/`backarrow` placeholders (`00229670`).
pub struct EnduroDress {
    pub model: usize,
    pub front: String,
    pub back: String,
}

/// `common` is the venue TOY, read for `[CameraFlash]`; `gate` is the scene
/// model index of a race's drop gate; `enduro` dresses the Enduro gates.
pub fn load(
    archive: &Resource,
    scene: &Scene,
    common: Option<&[u8]>,
    gate: Option<usize>,
    enduro: &[EnduroDress],
) -> Result<Scenery> {
    let mut scenery = Scenery::default();
    let mut materials = Materials::default();
    let mut geometry: Vec<(String, Vec<usize>, Option<ModelLod>)> = Vec::new();
    for model in &scene.models {
        if geometry.iter().any(|(name, ..)| *name == model.name) {
            continue;
        }
        let path = format!("geometry/{}.psg", model.name);
        // Animated TOY models (the Freestyle targets' cherries) ship under
        // `anim/`.
        let index = member_at(archive, &path)
            .or_else(|| member_at(archive, &format!("anim/{}.psg", model.name)))
            .ok_or_else(|| format!("Original scene mesh is missing: {}", model.name))?;
        match model_chunks(archive, index, &mut materials, &mut scenery) {
            Ok((chunks, lod)) => geometry.push((model.name.clone(), chunks, Some(lod))),
            Err(error) => {
                scenery.unsupported.push((model.name.clone(), error));
                geometry.push((model.name.clone(), Vec::new(), None));
            }
        }
    }
    // Stadium cameraflash.slt models are hidden and lit in random batches
    // (0020f2c0); each keeps its pool index in scene order.
    let mut flashes = 0usize;
    for (index, model) in scene.models.iter().enumerate() {
        let flash =
            (scene.crowd_present && model.name.eq_ignore_ascii_case("cameraflash")).then(|| {
                flashes += 1;
                flashes - 1
            });
        let Some((_, chunks, lod)) = geometry.iter().find(|(name, ..)| *name == model.name) else {
            scenery.lods.push(None);
            continue;
        };
        scenery.lods.push(lod.clone());
        let dress = enduro.iter().position(|d| d.model == index);
        for &chunk in chunks {
            // A gate banner draws its own arrow texture: a copy of the
            // placeholder chunk with the chosen material.
            let chunk = match dress.map(|d| &enduro[d]) {
                Some(d) => {
                    let banner = match scenery.chunks[chunk]
                        .material_name
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "frontarrow" => Some(&d.front),
                        "backarrow" => Some(&d.back),
                        _ => None,
                    };
                    match banner {
                        Some(name) => {
                            let material = materials.get(archive, name, true)?;
                            let c = &scenery.chunks[chunk];
                            if material.passes.len() <= c.pass_index {
                                return Err(format!(
                                    "Enduro banner {name} has no pass {}",
                                    c.pass_index
                                ));
                            }
                            let copy = Chunk {
                                lod: c.lod,
                                vertices: c.vertices.clone(),
                                colors: c.colors.clone(),
                                reflection: c.reflection.clone(),
                                indices: c.indices.clone(),
                                material,
                                material_name: name.clone(),
                                pass_index: c.pass_index,
                                nodes: c.nodes.clone(),
                                min: c.min,
                                max: c.max,
                            };
                            scenery.chunks.push(copy);
                            scenery.chunks.len() - 1
                        }
                        None => chunk,
                    }
                }
                None => chunk,
            };
            let c = &scenery.chunks[chunk];
            let prelit = c.pass().pass.as_ref().is_some_and(|p| p.prelit);
            let far_cull = c.pass_index > 0 && c.material.multipass_cull;
            let (min, max) = world_bounds(c.min, c.max, &model.world_matrix);
            scenery.draws.push(Draw {
                chunk,
                model: index,
                matrix: model.world_matrix,
                unlit: prelit || !model.use_lighting,
                flash,
                gate: gate == Some(index),
                enduro_gate: dress,
                lod: c.lod,
                far_cull,
                min,
                max,
            });
        }
    }
    if flashes > 0 {
        let config = common
            .and_then(|toy| atv_core::race_crowd::parse_flash_config(&String::from_utf8_lossy(toy)))
            .unwrap_or_default();
        scenery.flashes = Some((config, flashes));
    }
    Ok(scenery)
}

fn model_chunks(
    archive: &Resource,
    index: usize,
    materials: &mut Materials,
    scenery: &mut Scenery,
) -> Result<(Vec<usize>, ModelLod)> {
    // Static scene setters replace the PSG root; upload root-relative geometry.
    let mesh = mesh::decode_psg_for_scene(&archive.extract(index)?)?;
    if mesh.lods.is_empty() {
        return Err("PSG has no LOD".into());
    }
    let model_lod = ModelLod::from_mesh(&mesh)?;
    // Every stored level loads (001b2730); 001b3a90 picks one per frame.
    let mut resolved = Vec::new();
    for (level, lod) in mesh.lods.iter().enumerate() {
        for group in &lod.groups {
            let name = mesh
                .materials
                .get(group.material as usize)
                .ok_or("PSG group material is missing")?;
            resolved.push((
                level as u8,
                group,
                name,
                materials.get(archive, name, true)?,
            ));
        }
    }
    let mut chunks = Vec::new();
    for (level, group, name, material) in resolved {
        for (pass_index, pass) in material.passes.iter().enumerate() {
            let reflection = pass.pass.as_ref().map_or(0, |p| p.reflection_plane);
            if pass.reflective && reflection == 0 {
                scenery.skipped_groups += 1;
                continue;
            }
            if group.vertices.is_empty() || group.indices.is_empty() {
                continue;
            }
            if reflection != 0 && mesh.nodes.len() > 32 {
                return Err("Reflection mesh exceeds original rigid-node GPU capacity.".into());
            }
            let prelit = pass.pass.as_ref().is_some_and(|p| p.prelit);
            let mut vertices = Vec::with_capacity(group.vertices.len() * 8);
            let mut colors = Vec::with_capacity(group.vertices.len());
            let mut local = Vec::new();
            let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for v in &group.vertices {
                vertices.extend(v.position);
                vertices.extend(v.normal);
                vertices.extend(v.uv);
                colors.push(if prelit {
                    v.color.unwrap_or([128; 4])
                } else {
                    [128; 4]
                });
                if reflection != 0 {
                    local.extend(v.local_position);
                    local.extend(v.local_normal);
                    local.push(f32::from(v.node));
                }
                for a in 0..3 {
                    min[a] = min[a].min(v.position[a]);
                    max[a] = max[a].max(v.position[a]);
                }
            }
            if group
                .indices
                .iter()
                .any(|&i| i as usize >= group.vertices.len())
            {
                return Err("PSG index exceeds group".into());
            }
            scenery.chunks.push(Chunk {
                lod: level,
                vertices,
                colors,
                reflection: local,
                indices: group.indices.clone(),
                material: material.clone(),
                material_name: name.clone(),
                pass_index,
                nodes: if reflection != 0 {
                    mesh.nodes.iter().map(|n| n.world_matrix).collect()
                } else {
                    Vec::new()
                },
                min,
                max,
            });
            chunks.push(scenery.chunks.len() - 1);
        }
    }
    Ok((chunks, model_lod))
}

/// Axis-aligned bounds of a transformed box (worldBounds in render-support.js).
pub fn world_bounds(min: [f32; 3], max: [f32; 3], m: &[f32; 16]) -> ([f32; 3], [f32; 3]) {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for corner in 0..8 {
        let p: [f32; 3] = std::array::from_fn(|a| {
            if corner & (1 << a) != 0 {
                max[a]
            } else {
                min[a]
            }
        });
        for a in 0..3 {
            let v = m[a] * p[0] + m[4 + a] * p[1] + m[8 + a] * p[2] + m[12 + a];
            lo[a] = lo[a].min(v);
            hi[a] = hi[a].max(v);
        }
    }
    (lo, hi)
}

/// Scenery UV scroll and ATM frame clocks, advanced once per 60 Hz
/// simulation step as the browser's batch does (001be080, 001bd8e0).
#[derive(Default)]
pub struct Clocks {
    scrolls: Vec<(Scroll, [f32; 2])>,
    textures: Vec<(TextureClock, usize)>,
    /// Per chunk: scroll and texture clock indices.
    chunk_scroll: Vec<Option<usize>>,
    chunk_texture: Vec<Option<usize>>,
    tick: u64,
}

impl Clocks {
    pub fn new(scenery: &Scenery) -> Self {
        let mut clocks = Self::default();
        let mut scroll_keys: Vec<String> = Vec::new();
        let mut texture_keys: Vec<String> = Vec::new();
        for chunk in &scenery.chunks {
            let pass = chunk.pass();
            let key = chunk.pass_key();
            let scroll = pass.pass.as_ref().and_then(|p| p.uv_scroll.clone());
            clocks.chunk_scroll.push(scroll.map(|scroll| {
                scroll_keys
                    .iter()
                    .position(|k| *k == key)
                    .unwrap_or_else(|| {
                        scroll_keys.push(key.clone());
                        clocks.scrolls.push((scroll, [0.; 2]));
                        clocks.scrolls.len() - 1
                    })
            }));
            let frames = pass.texture.as_ref().map_or(0, |t| t.frames.len());
            let animation = pass
                .pass
                .as_ref()
                .and_then(|p| p.texture_animation.clone())
                .filter(|_| frames > 0);
            clocks.chunk_texture.push(animation.and_then(|config| {
                if let Some(at) = texture_keys.iter().position(|k| *k == key) {
                    return Some(at);
                }
                let clock = TextureClock::new(config, frames).ok()?;
                texture_keys.push(key.clone());
                clocks.textures.push((clock, 0));
                Some(clocks.textures.len() - 1)
            }));
        }
        clocks
    }

    /// One 1/60 s simulation step.
    pub fn step(&mut self) {
        for (scroll, offset) in &mut self.scrolls {
            if let Ok(next) = scroll.step(1. / 60.) {
                *offset = next;
            }
        }
        let milliseconds = (self.tick * 1000 / 60) as u32;
        self.tick += 1;
        for (clock, frame) in &mut self.textures {
            *frame = clock.tick(milliseconds);
        }
    }

    pub fn steps(&self) -> u64 {
        self.tick
    }

    pub fn uv_offset(&self, chunk: usize) -> [f32; 2] {
        self.chunk_scroll
            .get(chunk)
            .copied()
            .flatten()
            .map_or([0.; 2], |i| self.scrolls[i].1)
    }

    pub fn animated(&self, chunk: usize) -> bool {
        self.chunk_scroll.get(chunk).is_some_and(Option::is_some)
    }

    pub fn texture_frame(&self, chunk: usize) -> usize {
        self.chunk_texture
            .get(chunk)
            .copied()
            .flatten()
            .map_or(0, |i| self.textures[i].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_bounds_cover_rotated_and_translated_boxes() {
        // 90 degrees about Y, then translated.
        let m = [
            0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 10., 20., 30., 1.,
        ];
        let (lo, hi) = world_bounds([0., 0., 0.], [2., 1., 4.], &m);
        assert_eq!(lo, [10., 20., 28.]);
        assert_eq!(hi, [14., 21., 30.]);
    }
}
