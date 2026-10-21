//! ENV08's original train (web/train.js): the Rust route/MOT runtime
//! (atv_core::train, 0011c840) and its car and wheel meshes in bind pose.
use crate::{
    Result,
    materials::{Material, Materials, member_at},
};
use atv_core::{mesh, resource::Resource, train::Runtime};
use std::rc::Rc;

pub struct Chunk {
    /// Bone-local position, bone-local normal, u, v, node per vertex.
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub material: Rc<Material>,
}

pub struct Model {
    /// Archive path (`geometry/<car>.psg`, `anim/loco_wheels.psg`).
    pub name: String,
    pub chunks: Vec<Chunk>,
    /// Node world matrices of the stored bind pose.
    pub bind: Vec<[f32; 16]>,
}

pub struct Train {
    pub runtime: Runtime,
    pub models: Vec<Model>,
}

pub const WHEELS: &str = "anim/loco_wheels.psg";

/// `geometry/<model>.psg` for a car's SLT name.
pub fn mesh_name(model: &str) -> String {
    let name = model.replace('\\', "/").to_ascii_lowercase();
    format!(
        "geometry/{}",
        name.strip_suffix(".slt")
            .map_or(name.clone(), |stem| format!("{stem}.psg"))
    )
}

/// The inverse-transpose direction of a bind matrix (inverseDirection in game.js).
fn inverse_direction(m: &[f32; 16], v: [f32; 3]) -> Result<[f32; 3]> {
    let (a, b, c) = ([m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]);
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let determinant = dot(a, cross(b, c));
    if determinant.abs() < 1e-10 {
        return Err("Train bind matrix is singular.".into());
    }
    Ok([cross(b, c), cross(c, a), cross(a, b)].map(|row| dot(row, v) / determinant))
}

pub fn load(archive: &Resource, mode_index: Option<usize>) -> Result<Option<Train>> {
    let Some(index) = mode_index else {
        return Ok(None);
    };
    let Some(runtime) = Runtime::from_resource(archive, index)? else {
        return Ok(None);
    };
    let mut names: Vec<String> = Vec::new();
    for car in runtime.frame()?.cars {
        let name = mesh_name(&car.model);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names.push(WHEELS.into());
    let mut materials = Materials::default();
    let mut models = Vec::with_capacity(names.len());
    for name in names {
        let member = member_at(archive, &name)
            .ok_or_else(|| format!("Original train mesh is missing: {name}"))?;
        let mesh = mesh::decode_psg(&archive.extract(member)?)?;
        if mesh.nodes.len() > 32 {
            return Err("Train model exceeds original pose capacity.".into());
        }
        let lod = mesh.lods.first().ok_or("train PSG has no LOD")?;
        let mut chunks = Vec::new();
        for group in &lod.groups {
            let material_name = mesh
                .materials
                .get(group.material as usize)
                .ok_or("train material is missing")?;
            let material = materials.get(archive, material_name, false)?;
            if material.passes[0].reflective {
                continue;
            }
            let mut vertices = Vec::with_capacity(group.vertices.len() * 9);
            for v in &group.vertices {
                let node = mesh
                    .nodes
                    .get(usize::from(v.node))
                    .ok_or("Train mesh bone is missing.")?;
                vertices.extend(v.local_position);
                vertices.extend(inverse_direction(&node.world_matrix, v.normal)?);
                vertices.extend(v.uv);
                vertices.push(f32::from(v.node));
            }
            chunks.push(Chunk {
                vertices,
                indices: group.indices.clone(),
                material,
            });
        }
        models.push(Model {
            name,
            chunks,
            bind: mesh.nodes.iter().map(|n| n.world_matrix).collect(),
        });
    }
    Ok(Some(Train { runtime, models }))
}
