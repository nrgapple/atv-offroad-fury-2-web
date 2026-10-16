//! Synchronous render-side helpers. The page instantiates a second instance of
//! this module on its main thread; it holds only presentation state (terrain
//! heights and vegetation queues) and never runs the simulation. Per-frame
//! exports take numbers and write instance floats into this instance's memory.
use atv_core::{Result, terrain::Heightfield, vegetation_render};
use std::cell::RefCell;

#[derive(Default)]
struct Render {
    /// Dense heights of the visible terrain mesh.
    ground: Option<Heightfield>,
    vegetation: Option<vegetation_render::Vegetation>,
    /// visible, scanned, far, transition, near, groups, then per-group counts.
    counts: Vec<u32>,
    /// Enhanced/High wheel re-projection: the player's quad linkage and its
    /// original suspension travel, sampled again with presentation-only
    /// compression. Never connected to the simulation.
    quad: Option<(atv_core::quad_pose::QuadPose, f32)>,
    quad_matrices: Vec<f32>,
}
thread_local! { static RENDER: RefCell<Render> = RefCell::default(); }

pub fn dispatch(input: &[u8], op: u32) -> Result<Vec<u8>> {
    RENDER.with_borrow_mut(|render| match op {
        141 => {
            render.ground = None;
            let terrain = atv_core::terrain::decode_tr2(input)?;
            let result = serde_json::json!({"width_cells": terrain.width_cells, "depth_cells": terrain.depth_cells, "spacing": terrain.spacing});
            render.vegetation = None;
            render.ground = Some(terrain.render_heights()?);
            serde_json::to_vec(&result).map_err(|e| e.to_string())
        }
        142 => {
            // u32 configuration length, configuration JSON, then original ESB bytes.
            render.vegetation = None;
            let length = input
                .get(..4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
                .filter(|length| *length <= input.len() - 4)
                .ok_or("invalid vegetation configuration framing")?;
            let config: vegetation_render::Config =
                serde_json::from_slice(&input[4..4 + length]).map_err(|e| e.to_string())?;
            let ecosystem = atv_core::ecosystem::decode(&input[4 + length..])?;
            let ground = render.ground.as_ref().ok_or("load render terrain first")?;
            let vegetation = vegetation_render::Vegetation::new(ecosystem, ground, &config)?;
            let result = serde_json::json!({
                "placements": vegetation.len(), "groups": vegetation.groups, "max_radius": vegetation.max_radius,
                "capacity": vegetation_render::CAPACITY, "billboard_floats": vegetation_render::BILLBOARD_FLOATS,
                "mesh_floats": vegetation_render::MESH_FLOATS, "mesh_offset": vegetation_render::MESH_OFFSET,
            });
            render.counts = vec![0; 6 + vegetation.groups.len()];
            render.vegetation = Some(vegetation);
            serde_json::to_vec(&result).map_err(|e| e.to_string())
        }
        // u32 PSG length, the player's original quad PSG, then its physics text.
        144 => {
            render.quad = None;
            let length = input
                .get(..4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
                .filter(|length| *length <= input.len() - 4)
                .ok_or("invalid quad linkage framing")?;
            let mesh = atv_core::mesh::decode_psg(&input[4..4 + length])?;
            let physics = atv_core::physics_data::PhysicsData::parse(&input[4 + length..])?;
            let travel = physics.first_scalar("Suspension", "Compression")?;
            if !(travel.is_finite() && travel > 0.) {
                return Err("invalid suspension travel".into());
            }
            let pose = atv_core::quad_pose::QuadPose::new(&mesh)?;
            let nodes = mesh.nodes.len();
            render.quad = Some((pose, travel));
            render.quad_matrices = vec![0.; nodes * 16];
            serde_json::to_vec(&serde_json::json!({"nodes": nodes, "travel": travel}))
                .map_err(|e| e.to_string())
        }
        // Verification only: every render placement and the last selection.
        143 => {
            let vegetation = render.vegetation.as_ref().ok_or("vegetation is not loaded")?;
            let placements: Vec<_> = (0..vegetation.len())
                .filter_map(|i| vegetation.placement(i))
                .map(|(slot, position, radius)| serde_json::json!({"slot": slot, "position": position, "radius": radius}))
                .collect();
            let selection: Vec<_> = vegetation
                .selection()
                .into_iter()
                .map(|(index, depth)| serde_json::json!({"index": index, "depth": depth}))
                .collect();
            serde_json::to_vec(&serde_json::json!({"placements": placements, "selection": selection}))
                .map_err(|e| e.to_string())
        }
        _ => Err("unknown render operation".into()),
    })
}

/// Visible (dense render grid) ground height for cameras and presentation;
/// NaN before terrain loads or for NaN input.
#[unsafe(no_mangle)]
pub extern "C" fn render_terrain_height(x: f64, z: f64) -> f64 {
    RENDER.with_borrow(|render| {
        render
            .ground
            .as_ref()
            .map_or(f64::NAN, |ground| ground.height(x, z))
    })
}

/// Selects this frame's vegetation; returns 0 when no vegetation is loaded.
#[unsafe(no_mangle)]
pub extern "C" fn render_vegetation_select(
    camera_x: f64,
    camera_z: f64,
    forward_x: f64,
    forward_z: f64,
    frame: f64,
) -> u32 {
    RENDER.with_borrow_mut(|render| {
        let Some(vegetation) = render.vegetation.as_mut() else {
            return 0;
        };
        let counts = vegetation.select([camera_x, camera_z], [forward_x, forward_z], frame);
        render.counts.clear();
        render.counts.extend([
            counts.visible,
            counts.scanned,
            counts.far,
            counts.transition,
            counts.near,
            vegetation.groups.len() as u32,
        ]);
        render.counts.extend_from_slice(&vegetation.group_counts);
        1
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn render_vegetation_counts() -> *const u32 {
    RENDER.with_borrow(|render| render.counts.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn render_vegetation_instances() -> *const f32 {
    RENDER.with_borrow(|render| {
        render
            .vegetation
            .as_ref()
            .map_or(std::ptr::null(), |v| v.output.as_ptr())
    })
}

/// Samples the loaded quad linkage with presentation compression, each value
/// clamped to [0, travel]. Returns the node count (0 when unavailable); the
/// column-major matrices follow at `render_quad_pose_matrices`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn render_quad_pose(
    steering: f64,
    angle_0: f64,
    angle_1: f64,
    angle_2: f64,
    angle_3: f64,
    compression_0: f64,
    compression_1: f64,
    compression_2: f64,
    compression_3: f64,
) -> u32 {
    RENDER.with_borrow_mut(|render| {
        let Some((pose, travel)) = render.quad.as_ref() else {
            return 0;
        };
        let clamp = |c: f64| (c as f32).clamp(0., *travel);
        let compression = [compression_0, compression_1, compression_2, compression_3].map(clamp);
        let angles = [angle_0, angle_1, angle_2, angle_3].map(|a| a as f32);
        let Ok(matrices) = pose.sample(steering as f32, angles, compression) else {
            return 0;
        };
        render.quad_matrices.clear();
        render.quad_matrices.extend(matrices.iter().flatten());
        matrices.len() as u32
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn render_quad_pose_matrices() -> *const f32 {
    RENDER.with_borrow(|render| render.quad_matrices.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_state_rejects_bad_input_before_drawing() {
        assert!(render_terrain_height(1., 1.).is_nan());
        assert_eq!(render_vegetation_select(0., 0., 0., 1., 1.), 0);
        assert!(dispatch(b"not a terrain", 141).is_err());
        assert!(dispatch(&[1, 0], 142).unwrap_err().contains("framing"));
        assert!(
            dispatch(&[200, 0, 0, 0, b'{'], 142)
                .unwrap_err()
                .contains("framing")
        );
        let config = br#"{"far":768,"distances":[120,180],"species":[]}"#;
        let mut framed = (config.len() as u32).to_le_bytes().to_vec();
        framed.extend_from_slice(config);
        framed.extend_from_slice(&2u32.to_le_bytes());
        framed.extend(std::iter::repeat_n(0, 256));
        framed.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            dispatch(&framed, 142).unwrap_err(),
            "load render terrain first"
        );
        assert!(dispatch(&[], 143).is_err());
        assert_eq!(render_quad_pose(0., 0., 0., 0., 0., 0., 0., 0., 0.), 0);
        assert!(
            dispatch(&[9, 0, 0, 0, 1], 144)
                .unwrap_err()
                .contains("framing")
        );
        assert!(dispatch(&[1, 0, 0, 0, 1, b'x'], 144).is_err());
    }
}
