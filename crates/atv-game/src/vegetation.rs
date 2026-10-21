//! Original ESB vegetation: species meshes, billboard atlas and placements
//! (terrainFromContent/buildEcosystem in web/game.js). Per-frame selection
//! and instance floats come from atv_core::vegetation_render.
use crate::{
    Result,
    materials::{MaterialPass, Materials, member_at, member_named},
};
use atv_core::vegetation_render::{Config, SpeciesBounds, Vegetation as Selection};
use atv_core::{ecosystem, mesh, resource::Resource, scene::Scene, terrain::Heightfield};
use std::{cell::RefCell, rc::Rc};

pub struct Chunk {
    /// x y z, nx ny nz, u v per vertex.
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub material: Rc<crate::materials::Material>,
}

impl Chunk {
    pub fn pass(&self) -> &MaterialPass {
        &self.material.passes[0]
    }
}

pub struct Group {
    pub chunks: Vec<Chunk>,
}

pub struct Billboard {
    pub material: Rc<crate::materials::Material>,
}

pub struct Vegetation {
    /// One group per ESB species, in species order.
    pub groups: Vec<Group>,
    pub billboard: Option<Billboard>,
    /// Shared with the renderer, which selects every frame.
    pub selection: Rc<RefCell<Selection>>,
    pub lighting: ecosystem::Lighting,
}

pub fn load(
    archive: &Resource,
    environment: &str,
    scene: &Scene,
    ground: &Heightfield,
) -> Result<Option<Vegetation>> {
    let Some(index) = member_named(archive, &format!("{environment}.esb")) else {
        return Ok(None);
    };
    let bytes = archive.extract(index)?;
    let data = ecosystem::decode(&bytes)?;
    let mut materials = Materials::default();
    let mut groups = Vec::with_capacity(data.species.len());
    let mut bounds = Vec::with_capacity(data.species.len());
    let mut billboard: Option<(Billboard, String)> = None;
    for species in &data.species {
        let path = format!("eco/{}.psg", species.model);
        let member = member_at(archive, &path)
            .ok_or_else(|| format!("Original vegetation mesh is missing: {}", species.model))?;
        let mesh = mesh::decode_psg(&archive.extract(member)?)?;
        let bind = mesh.bind_bounds()?;
        bounds.push(SpeciesBounds {
            slot: species.slot,
            center_y: bind.center[1],
            half_extents: [bind.half_extents[0], bind.half_extents[1]],
        });
        let lod = mesh.lods.first().ok_or("vegetation PSG has no LOD")?;
        let mut chunks = Vec::new();
        for group in &lod.groups {
            if group.vertices.len() > 65536 {
                return Err("Vegetation mesh exceeds index bounds.".into());
            }
            let name = mesh
                .materials
                .get(group.material as usize)
                .ok_or("vegetation material is missing")?;
            let material = materials.get(archive, name, false)?;
            let mut vertices = Vec::with_capacity(group.vertices.len() * 8);
            for v in &group.vertices {
                vertices.extend(v.position);
                vertices.extend(v.normal);
                vertices.extend(v.uv);
            }
            chunks.push(Chunk {
                vertices,
                indices: group.indices.clone(),
                material,
            });
        }
        let stem = species
            .billboard
            .rsplit_once('.')
            .map_or(species.billboard.as_str(), |(stem, _)| stem)
            .to_string();
        let atlas_material = materials.get(archive, &stem, false)?;
        if species.atlas.is_some() {
            match &billboard {
                None => {
                    let pass = &atlas_material.passes[0];
                    if pass.texture.is_none() || pass.shader.as_deref() != Some("EcoSystemCode2D") {
                        return Err(format!("Original vegetation atlas is unavailable: {stem}"));
                    }
                    billboard = Some((
                        Billboard {
                            material: atlas_material,
                        },
                        stem,
                    ));
                }
                Some((_, first)) if !first.eq_ignore_ascii_case(&stem) => {
                    return Err(
                        "Mixed vegetation atlas families require separate ordered batches.".into(),
                    );
                }
                Some(_) => {}
            }
        }
        groups.push(Group { chunks });
    }
    // Rust registers placements (00129b70 resampling) and owns per-frame selection.
    let config = Config {
        far: scene.fog.as_ref().map_or(768., |f| f.end),
        distances: scene.ecosystem_distances,
        species: bounds,
    };
    let selection = Selection::new(data, ground, &config)?;
    Ok(Some(Vegetation {
        groups,
        billboard: billboard.map(|(b, _)| b),
        selection: Rc::new(RefCell::new(selection)),
        lighting: ecosystem::Lighting {
            ambient: scene.ecosystem_lighting.ambient,
            sunlight: scene.ecosystem_lighting.sunlight,
            direction: scene.ecosystem_lighting.direction,
            sun_present: scene.ecosystem_lighting.sun_present,
        },
    }))
}
