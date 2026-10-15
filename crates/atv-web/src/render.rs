//! Synchronous render-side helpers. The page instantiates a second instance of
//! this module on its main thread; it holds only presentation state (terrain
//! heights and vegetation queues) and never runs the simulation. Per-frame
//! exports take numbers and write instance floats into this instance's memory.
use atv_core::{Result, terrain::Terrain, vegetation_render};
use std::cell::RefCell;

#[derive(Default)]
struct Render {
    terrain: Option<Terrain>,
    grid: Option<atv_core::terrain::LeafGrid>,
    vegetation: Option<vegetation_render::Vegetation>,
    /// visible, scanned, far, transition, near, groups, then per-group counts.
    counts: Vec<u32>,
}
thread_local! { static RENDER: RefCell<Render> = RefCell::default(); }

pub fn dispatch(input: &[u8], op: u32) -> Result<Vec<u8>> {
    RENDER.with_borrow_mut(|render| match op {
        141 => {
            let terrain = atv_core::terrain::decode_tr2(input)?;
            let result = serde_json::json!({"width_cells": terrain.width_cells, "depth_cells": terrain.depth_cells, "spacing": terrain.spacing});
            render.vegetation = None;
            render.grid = terrain.leaf_grid();
            render.terrain = Some(terrain);
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
            let terrain = render.terrain.as_ref().ok_or("load render terrain first")?;
            let vegetation = vegetation_render::Vegetation::new(ecosystem, terrain, &config)?;
            let result = serde_json::json!({
                "placements": vegetation.len(), "groups": vegetation.groups, "max_radius": vegetation.max_radius,
                "capacity": vegetation_render::CAPACITY, "billboard_floats": vegetation_render::BILLBOARD_FLOATS,
                "mesh_floats": vegetation_render::MESH_FLOATS, "mesh_offset": vegetation_render::MESH_OFFSET,
            });
            render.counts = vec![0; 6 + vegetation.groups.len()];
            render.vegetation = Some(vegetation);
            serde_json::to_vec(&result).map_err(|e| e.to_string())
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

/// Clamped terrain contact height; NaN before terrain loads or for NaN input.
#[unsafe(no_mangle)]
pub extern "C" fn render_terrain_height(x: f64, z: f64) -> f64 {
    RENDER.with_borrow(|render| {
        render
            .terrain
            .as_ref()
            .and_then(|terrain| {
                terrain
                    .clamped_height(render.grid.as_ref(), x as f32, z as f32)
                    .ok()
                    .flatten()
            })
            .map_or(f64::NAN, f64::from)
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
    }
}
