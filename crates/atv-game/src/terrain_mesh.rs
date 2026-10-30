//! Presentation terrain grid (buildTerrainMesh in web/game.js).
//!
//! Original samples are kept within 96 cells of the rider and a bounded coarse
//! far field covers the rest. This LOD is provisional and never alters contacts.
use crate::venue::Terrain;

/// Interleaved position (3) and normal (3).
pub const FLOATS_PER_VERTEX: usize = 6;
const PATCH_CELLS: f64 = 96.;
const CHUNK: usize = 64;

pub struct Chunk {
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patch {
    pub x0: usize,
    pub x1: usize,
    pub z0: usize,
    pub z1: usize,
}

pub fn far_step(terrain: &Terrain) -> usize {
    (((terrain.width - 1) as f64 * (terrain.depth - 1) as f64 / 100000.)
        .sqrt()
        .ceil() as usize)
        .max(1)
}

/// Fine-patch bounds around a world-space XZ centre, snapped to the far grid.
pub fn patch(terrain: &Terrain, center: [f32; 2]) -> Patch {
    let step = far_step(terrain) as f64;
    let spacing = terrain.spacing as f64;
    let (w, d) = ((terrain.width - 1) as f64, (terrain.depth - 1) as f64);
    let cx = (center[0] as f64 / spacing).clamp(0., w);
    let cz = (center[1] as f64 / spacing).clamp(0., d);
    Patch {
        x0: (((cx - PATCH_CELLS) / step).floor() * step).max(0.) as usize,
        x1: (((cx + PATCH_CELLS) / step).ceil() * step).min(w) as usize,
        z0: (((cz - PATCH_CELLS) / step).floor() * step).max(0.) as usize,
        z1: (((cz + PATCH_CELLS) / step).ceil() * step).min(d) as usize,
    }
}

impl Patch {
    /// Whether two patches overlap or share an edge (no coarse cell between).
    fn touches(&self, other: &Patch) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.z0 <= other.z1 && other.z0 <= self.z1
    }

    fn contains(&self, x0: usize, x1: usize, z0: usize, z1: usize) -> bool {
        x0 >= self.x0 && x1 <= self.x1 && z0 >= self.z0 && z1 <= self.z1
    }
}

/// Split screen: one fine patch per local player (research/SPLIT_SCREEN.md).
/// Patches that overlap or touch merge into their bounding box, so every
/// remaining patch has coarse cells around it to stitch to. A port choice:
/// the original's split-screen terrain LOD was not traced.
pub fn disjoint(mut patches: Vec<Patch>) -> Vec<Patch> {
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for a in 0..patches.len() {
            for b in a + 1..patches.len() {
                if patches[a].touches(&patches[b]) {
                    let other = patches.swap_remove(b);
                    let p = &mut patches[a];
                    *p = Patch {
                        x0: p.x0.min(other.x0),
                        x1: p.x1.max(other.x1),
                        z0: p.z0.min(other.z0),
                        z1: p.z1.max(other.z1),
                    };
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    patches
}

pub fn build(terrain: &Terrain, patch: Option<Patch>) -> Vec<Chunk> {
    build_patches(terrain, patch.as_slice())
}

/// The grid with a fine patch at each of `patches` (disjoint, see
/// [`disjoint`]).
pub fn build_patches(terrain: &Terrain, patches: &[Patch]) -> Vec<Chunk> {
    let step = far_step(terrain);
    let mut xs: Vec<usize> = (0..terrain.width - 1).step_by(step).collect();
    let mut zs: Vec<usize> = (0..terrain.depth - 1).step_by(step).collect();
    xs.push(terrain.width - 1);
    zs.push(terrain.depth - 1);
    let mut chunks = Vec::new();
    for z0 in (0..zs.len() - 1).step_by(CHUNK) {
        for x0 in (0..xs.len() - 1).step_by(CHUNK) {
            let gx = &xs[x0..(x0 + CHUNK + 1).min(xs.len())];
            let gz = &zs[z0..(z0 + CHUNK + 1).min(zs.len())];
            chunks.push(grid(terrain, gx, gz, step, patches, None));
        }
    }
    for p in patches {
        let fx: Vec<usize> = (p.x0..=p.x1).collect();
        let fz: Vec<usize> = (p.z0..=p.z1).collect();
        for z0 in (0..fz.len().saturating_sub(1)).step_by(CHUNK) {
            for x0 in (0..fx.len().saturating_sub(1)).step_by(CHUNK) {
                let gx = &fx[x0..(x0 + CHUNK + 1).min(fx.len())];
                let gz = &fz[z0..(z0 + CHUNK + 1).min(fz.len())];
                chunks.push(grid(terrain, gx, gz, step, patches, Some(*p)));
            }
        }
    }
    chunks.retain(|chunk| !chunk.indices.is_empty());
    chunks
}

fn grid(
    t: &Terrain,
    gx: &[usize],
    gz: &[usize],
    step: usize,
    patches: &[Patch],
    // The fine patch this chunk belongs to; none for the coarse grid.
    fine: Option<Patch>,
) -> Chunk {
    let (nx, nz) = (gx.len(), gz.len());
    let h = |x: usize, z: usize| t.height(x, z) as f64;
    let spacing = t.spacing as f64;
    let mut vertices = Vec::with_capacity(nx * nz * FLOATS_PER_VERTEX);
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for &z in gz {
        for &x in gx {
            let (left, right) = (x.saturating_sub(1), (x + 1).min(t.width - 1));
            let (back, front) = (z.saturating_sub(1), (z + 1).min(t.depth - 1));
            let dx = (h(right, z) - h(left, z)) / ((right - left) as f64 * spacing);
            let dz = (h(x, front) - h(x, back)) / ((front - back) as f64 * spacing);
            let length = (dx * dx + 1. + dz * dz).sqrt();
            let mut y = h(x, z);
            // Stitch the fine patch boundary to the surrounding coarse grid.
            if let Some(p) = fine {
                if x == p.x0 || x == p.x1 {
                    let za = z / step * step;
                    let zb = (za + step).min(t.depth - 1);
                    y = if za == zb {
                        h(x, za)
                    } else {
                        h(x, za) + (h(x, zb) - h(x, za)) * (z - za) as f64 / (zb - za) as f64
                    };
                } else if z == p.z0 || z == p.z1 {
                    let xa = x / step * step;
                    let xb = (xa + step).min(t.width - 1);
                    y = if xa == xb {
                        h(xa, z)
                    } else {
                        h(xa, z) + (h(xb, z) - h(xa, z)) * (x - xa) as f64 / (xb - xa) as f64
                    };
                }
            }
            let position = [
                (x as f64 * spacing) as f32,
                y as f32,
                (z as f64 * spacing) as f32,
            ];
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
            vertices.extend_from_slice(&position);
            vertices.extend_from_slice(&[
                (-dx / length) as f32,
                (1. / length) as f32,
                (-dz / length) as f32,
            ]);
        }
    }
    let mut indices = Vec::with_capacity((nx - 1) * (nz - 1) * 6);
    for z in 0..nz - 1 {
        for x in 0..nx - 1 {
            if fine.is_none()
                && patches
                    .iter()
                    .any(|p| p.contains(gx[x], gx[x + 1], gz[z], gz[z + 1]))
            {
                continue;
            }
            let a = (z * nx + x) as u32;
            let (b, c) = (a + 1, a + nx as u32);
            indices.extend_from_slice(&[a, c, b, b, c, c + 1]);
        }
    }
    Chunk {
        vertices,
        indices,
        min,
        max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::venue::Image;

    fn flat(width: usize, depth: usize) -> Terrain {
        let heights = (0..width * depth)
            .map(|i| (i % width) as f32 * 0.5)
            .collect();
        let image = || Image {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        Terrain {
            width,
            depth,
            spacing: 2.,
            heights,
            min: 0.,
            max: 0.,
            layers: vec![],
            layer_mips: vec![],
            mapping: vec![],
            mapping_width: 0,
            mapping_height: 0,
            detail: image(),
        }
    }

    #[test]
    fn coarse_grid_covers_terrain_and_patch_cuts_a_hole() {
        let t = flat(1025, 1025);
        assert_eq!(far_step(&t), 4);
        let full = build(&t, None);
        let cells: usize = full.iter().map(|c| c.indices.len() / 6).sum();
        assert_eq!(cells, 256 * 256);
        let p = patch(&t, [1000., 1000.]);
        assert_eq!(
            p,
            Patch {
                x0: 404,
                x1: 596,
                z0: 404,
                z1: 596
            }
        );
        let split = build(&t, Some(p));
        let fine: usize =
            split.iter().map(|c| c.indices.len() / 6).sum::<usize>() - (cells - 48 * 48);
        assert_eq!(fine, 192 * 192);
    }

    #[test]
    fn split_screen_patches_stay_apart_or_merge() {
        let t = flat(1025, 1025);
        let cells = 256 * 256;
        // Far apart: two full fine patches, each cutting its own hole.
        let (a, b) = (patch(&t, [400., 400.]), patch(&t, [1600., 1600.]));
        let patches = disjoint(vec![a, b]);
        assert_eq!(patches, [a, b]);
        let built = build_patches(&t, &patches);
        let total: usize = built.iter().map(|c| c.indices.len() / 6).sum();
        assert_eq!(total, cells - 2 * 48 * 48 + 2 * 192 * 192);
        // Overlapping: one bounding patch, so no cell is drawn twice.
        let c = patch(&t, [600., 500.]);
        let merged = disjoint(vec![a, c]);
        assert_eq!(merged.len(), 1);
        let m = merged[0];
        assert_eq!((m.x0, m.x1, m.z0, m.z1), (a.x0, c.x1, a.z0, c.z1));
        let total: usize = build_patches(&t, &merged)
            .iter()
            .map(|c| c.indices.len() / 6)
            .sum();
        let (w, d) = (m.x1 - m.x0, m.z1 - m.z0);
        assert_eq!(total, cells - (w / 4) * (d / 4) + w * d);
    }

    #[test]
    fn normals_follow_slope() {
        let t = flat(5, 5);
        let chunk = &build(&t, None)[0];
        let normal = &chunk.vertices[3..6];
        assert!((normal[0] + 0.25 / (1.0625f32).sqrt()).abs() < 1e-6 && normal[2] == 0.);
    }
}
