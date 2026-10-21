//! Browser vegetation queues for the original ESB placements. Distance phases,
//! near-entry signed-short orientation and the stale-visibility reset follow
//!0012a1c0; queue order (distant, depth-disabled transition, near meshes)
//! follows0012af80. The 64² cell scan and the bounded 1,500-plant nearest-depth
//! heap are browser acceleration: native spatial callbacks, depth-bucket cutoff
//! and clipping/cache order are not reproduced. Arithmetic mirrors the former
//! JavaScript port: binary64 except where the original binary32 rounding applies.
use crate::{Result, ecosystem::Ecosystem, terrain::Heightfield};
use serde::{Deserialize, Serialize};

pub const CAPACITY: usize = 1500;
/// Near mesh instance: position3, scale2, rotation2, phase.
pub const MESH_FLOATS: usize = 8;
/// Atlas billboard instance: position3, size2, rotation2, phase, atlas columns
/// (left/right offset, left/right U), center U, V top/bottom, base height.
pub const BILLBOARD_FLOATS: usize = 16;
/// Near mesh instances follow every billboard slot in the shared output.
pub const MESH_OFFSET: usize = CAPACITY * BILLBOARD_FLOATS;

#[derive(Debug, Deserialize)]
pub struct SpeciesBounds {
    pub slot: u8,
    /// Bind-pose center Y and half extents (X, Y) of the species LOD0 mesh.
    pub center_y: f32,
    pub half_extents: [f32; 2],
}
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Scene fog end, or 768 without fog: the browser visibility envelope.
    pub far: f32,
    /// EcoDistance3D/EcoDistance2D from 00202bd0.
    pub distances: [f32; 2],
    pub species: Vec<SpeciesBounds>,
}

#[derive(Clone, Debug)]
struct Placement {
    group: usize,
    coordinates: [u16; 3],
    position: [f64; 3],
    scale: [f64; 2],
    dimensions: [f32; 2],
    atlas: Option<[f32; 7]>,
    base_height: f64,
    radius: f64,
    near: bool,
    rotation: [f64; 2],
    phase: f64,
    seen: f64,
    depth: f64,
}

#[derive(Debug, Default, Serialize, Clone, Copy, PartialEq)]
pub struct Counts {
    pub visible: u32,
    pub scanned: u32,
    pub far: u32,
    pub transition: u32,
    pub near: u32,
}

pub struct Vegetation {
    placements: Vec<Placement>,
    offsets: Vec<u32>,
    indices: Vec<u32>,
    /// Species slots in group (ESB species) order.
    pub groups: Vec<u8>,
    pub max_radius: f64,
    extent: f64,
    far: f64,
    distances: [f64; 2],
    /// Selected (projected depth, placement) pairs: the heap, then the
    /// sorted selection. Keeping the depth beside the index avoids a random
    /// placement read per heap and sort comparison; the order is unchanged.
    selected: Vec<(f64, u32)>,
    /// Instances: billboards (distant then transition), then near meshes by group.
    pub output: Vec<f32>,
    /// Near mesh instance count per group, after the last selection.
    pub group_counts: Vec<u32>,
    pub counts: Counts,
    next: Vec<usize>,
}

impl Vegetation {
    /// Registers every ESB placement in source order, with the terrain extent
    /// as the packed-coordinate range. Height-resampled placements quantize
    /// the visible dense-grid height back to unsigned16 (00129b70), as the
    /// former browser did. Collision registration samples the contact plane;
    /// which plane the original sampler reads is not verified.
    pub fn new(ecosystem: Ecosystem, ground: &Heightfield, config: &Config) -> Result<Self> {
        let extent = ground.extent();
        if !extent.is_finite() || extent <= 0. {
            return Err("invalid vegetation terrain extent".into());
        }
        if !config.far.is_finite()
            || config.far < 0.
            || !config.distances.iter().all(|v| v.is_finite() && *v >= 0.)
        {
            return Err("invalid vegetation distances".into());
        }
        let scale = extent / 65536.;
        let mut groups = Vec::with_capacity(ecosystem.species.len());
        let mut group_of = [None; 256];
        let mut bounds = Vec::with_capacity(ecosystem.species.len());
        for species in &ecosystem.species {
            let source = config
                .species
                .iter()
                .find(|bounds| bounds.slot == species.slot)
                .ok_or("missing vegetation species bounds")?;
            let [half_x, half_y] = source.half_extents;
            if !(half_x > 0. && half_y > 0.) || !half_x.is_finite() || !half_y.is_finite() {
                return Err("Invalid vegetation mesh bounds.".into());
            }
            let atlas = species.atlas.as_ref().map(|a| {
                [
                    a.horizontal[0],
                    a.horizontal[2],
                    a.u[0],
                    a.u[2],
                    a.u[1],
                    a.v[0],
                    a.v[1],
                ]
            });
            group_of[usize::from(species.slot)] = Some(groups.len());
            groups.push(species.slot);
            let (center, half_x, half_y) = (
                f64::from(source.center_y),
                f64::from(half_x),
                f64::from(half_y),
            );
            bounds.push((half_x, half_y, (center - half_y) * 0.5 / half_y, atlas));
        }
        let mut placements = Vec::with_capacity(ecosystem.placements.len());
        let mut max_radius = 0f64;
        for p in &ecosystem.placements {
            let group =
                group_of[usize::from(p.slot)].ok_or("placement references absent species")?;
            let (half_x, half_y, base_height, atlas) = bounds[group];
            let mut position = p.coordinates.map(|c| f64::from(c) * scale);
            if p.resample_height {
                // Math.trunc(height / scale) & 65535: ToInt32 keeps the low 16 bits.
                let packed = (ground.height(position[0], position[2]) / scale).trunc();
                position[1] = if packed.is_finite() {
                    packed.rem_euclid(65536.)
                } else {
                    0.
                } * scale;
            }
            let (radius, height) = (f64::from(p.radius), f64::from(p.height));
            let placement = Placement {
                group,
                coordinates: p.coordinates,
                position,
                scale: [radius / half_x, height * 0.5 / half_y],
                dimensions: [p.radius, p.height],
                atlas,
                base_height,
                radius: radius.max(height),
                near: false,
                rotation: [1., 0.],
                phase: 64.,
                seen: -100.,
                depth: 0.,
            };
            max_radius = max_radius.max(placement.radius);
            placements.push(placement);
        }
        let index = ecosystem.visibility_index;
        if index.offsets.len() != 4097
            || index
                .placements
                .iter()
                .any(|i| *i as usize >= placements.len())
        {
            return Err("invalid vegetation visibility index".into());
        }
        Ok(Self {
            group_counts: vec![0; groups.len()],
            groups,
            placements,
            offsets: index.offsets,
            indices: index.placements,
            max_radius,
            extent,
            far: f64::from(config.far),
            distances: config.distances.map(f64::from),
            selected: Vec::with_capacity(CAPACITY),
            output: vec![0.; CAPACITY * (BILLBOARD_FLOATS + MESH_FLOATS)],
            counts: Counts::default(),
            next: Vec::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.placements.len()
    }
    pub fn is_empty(&self) -> bool {
        self.placements.is_empty()
    }
    /// Source placement index, slot, render position and visibility radius.
    pub fn placement(&self, index: usize) -> Option<(u8, [f64; 3], f64)> {
        self.placements
            .get(index)
            .map(|p| (self.groups[p.group], p.position, p.radius))
    }
    /// Source placement indices and projected depths of the last selection,
    /// in submission (descending depth) order.
    pub fn selection(&self) -> Vec<(u32, f64)> {
        self.selected.iter().map(|&(depth, i)| (i, depth)).collect()
    }
    /// Offset in floats of a group's near mesh instances within `output`.
    pub fn group_offset(&self, group: usize) -> usize {
        MESH_OFFSET + self.group_counts[..group].iter().sum::<u32>() as usize * MESH_FLOATS
    }

    // Reusable max heap of the nearest projected depths, capped at CAPACITY.
    fn push(&mut self, index: u32) {
        let depth = self.placements[index as usize].depth;
        let heap = &mut self.selected;
        if heap.len() == CAPACITY {
            if depth >= heap[0].0 {
                return;
            }
            let mut i = 0;
            while i * 2 + 1 < heap.len() {
                let mut child = i * 2 + 1;
                if child + 1 < heap.len() && heap[child + 1].0 > heap[child].0 {
                    child += 1;
                }
                if heap[child].0 <= depth {
                    break;
                }
                heap[i] = heap[child];
                i = child;
            }
            heap[i] = (depth, index);
        } else {
            let mut i = heap.len();
            heap.push((depth, index));
            while i > 0 {
                let parent = (i - 1) >> 1;
                if heap[parent].0 >= depth {
                    break;
                }
                heap[i] = heap[parent];
                i = parent;
            }
            heap[i] = (depth, index);
        }
    }

    /// Selects and writes this render frame's instances. `forward` is the
    /// camera's horizontal (sin yaw, cos yaw); `frame` is the browser frame
    /// serial used for the stale-visibility reset.
    pub fn select(&mut self, camera: [f64; 2], forward: [f64; 2], frame: f64) -> Counts {
        let [forward_x, forward_z] = forward;
        let inverse = 65536. / self.extent;
        let cell_size = self.extent / 64.;
        let far = self.far;
        let range = far + self.max_radius;
        self.selected.clear();
        self.group_counts.iter_mut().for_each(|count| *count = 0);
        self.counts = Counts::default();
        if !camera.iter().chain(forward.iter()).all(|v| v.is_finite()) {
            return self.counts;
        }
        let cell = |v: f64| (v / cell_size).floor().clamp(0., 63.) as usize;
        let [min_x, max_x] = [cell(camera[0] - range), cell(camera[0] + range)];
        let [min_z, max_z] = [cell(camera[1] - range), cell(camera[1] + range)];
        let eye_x = f64::from(((camera[0] * inverse) as f32).trunc());
        let eye_z = f64::from(((camera[1] * inverse) as f32).trunc());
        let quantize = |v: f64| f64::from(((v * inverse) as f32).trunc());
        let inner = quantize(self.distances[0]);
        let outer = quantize(self.distances[1]);
        let factor = (64. / (outer - inner)) as f32;
        let mut scanned = 0;
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let cell = z * 64 + x;
                for i in self.offsets[cell]..self.offsets[cell + 1] {
                    let index = self.indices[i as usize];
                    let p = &mut self.placements[index as usize];
                    scanned += 1;
                    let dx = p.position[0] - camera[0];
                    let dz = p.position[2] - camera[1];
                    p.depth = dx * forward_x + dz * forward_z;
                    let reach = far + p.radius;
                    if dx * dx + dz * dz > reach * reach || p.depth <= -p.radius {
                        continue;
                    }
                    let qx = eye_x - f64::from(p.coordinates[0]);
                    let qz = eye_z - f64::from(p.coordinates[2]);
                    let distance = qx * qx + qz * qz;
                    let near = distance <= outer * outer;
                    let mut phase = if !near {
                        64.
                    } else if distance < inner * inner {
                        0.
                    } else {
                        let offset = (distance.sqrt() as f32 as f64 - inner) as f32;
                        f64::from((factor * offset).trunc())
                    };
                    if near && !p.near {
                        //0012a1c0 captures signed-short orientation only on distant→near entry.
                        p.rotation = if forward_z == 0. {
                            [1., 0.]
                        } else {
                            [
                                f64::from(((forward_z * 32767.) as f32).trunc()) / 32767.,
                                f64::from(((-forward_x * 32767.) as f32).trunc()) / 32767.,
                            ]
                        };
                    }
                    p.near = near;
                    if near && phase != 0. && frame - p.seen > 3. {
                        phase = 0.;
                    }
                    p.phase = phase;
                    self.push(index);
                }
            }
        }
        // Descending projected depth; stable, as the former browser sort.
        self.selected
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for &(_, i) in &self.selected {
            let p = &mut self.placements[i as usize];
            p.seen = frame;
            if p.phase != 64. {
                self.group_counts[p.group] += 1;
            }
        }
        // Running near-mesh write offset per group (reused, no per-frame allocation).
        let mut next = std::mem::take(&mut self.next);
        next.clear();
        next.extend((0..self.groups.len()).map(|g| self.group_offset(g)));
        let mut billboard = [0usize; 2];
        for transition in [false, true] {
            for &(_, i) in &self.selected {
                let p = &self.placements[i as usize];
                if let Some(atlas) = p.atlas
                    && p.phase != 0.
                    && (p.phase < 49.) == transition
                {
                    let at = (billboard[0] + billboard[1]) * BILLBOARD_FLOATS;
                    let data = &mut self.output[at..at + BILLBOARD_FLOATS];
                    data[..3].copy_from_slice(&p.position.map(|v| v as f32));
                    data[3..5].copy_from_slice(&p.dimensions);
                    data[5..7].copy_from_slice(&p.rotation.map(|v| v as f32));
                    data[7] = p.phase as f32;
                    data[8..15].copy_from_slice(&atlas);
                    data[15] = p.base_height as f32;
                    billboard[usize::from(transition)] += 1;
                }
            }
        }
        for &(_, i) in &self.selected {
            let p = &self.placements[i as usize];
            if p.phase == 64. {
                continue;
            }
            let at = next[p.group];
            next[p.group] += MESH_FLOATS;
            let data = &mut self.output[at..at + MESH_FLOATS];
            data[..3].copy_from_slice(&p.position.map(|v| v as f32));
            data[3..5].copy_from_slice(&p.scale.map(|v| v as f32));
            data[5..7].copy_from_slice(&p.rotation.map(|v| v as f32));
            data[7] = p.phase as f32;
        }
        self.next = next;
        self.counts = Counts {
            visible: self.selected.len() as u32,
            scanned,
            far: billboard[0] as u32,
            transition: billboard[1] as u32,
            near: self.group_counts.iter().sum(),
        };
        self.counts
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::terrain::{Terrain, TerrainLeaf, TerrainTile};

    /// A 16-cell, 64 ft plane: height = 3x + z (cells), exact in binary32.
    pub(crate) fn plane() -> Terrain {
        Terrain {
            spacing: 64.,
            tiles: vec![TerrainTile {
                tag: 0,
                heights: std::array::from_fn(|i| ((i % 17) * 3 + i / 17) as u16),
                attributes: [u32::from_le_bytes([0, 127, 0, 0]); 289],
            }],
            leaves: vec![TerrainLeaf {
                x: 0,
                z: 0,
                tile: 0,
                height_morph: 1.,
                height_scale: 1. / 64.,
                minimum_height: 0.,
                maximum_height: 100.,
            }],
            hierarchy_nodes: 1,
            width_cells: 16,
            depth_cells: 16,
        }
    }
    type Planted = (u8, bool, [u16; 3], i8, i8);
    fn esb(species: &[(u8, &[f32])], placements: &[Planted]) -> Vec<u8> {
        let mut b = 2u32.to_le_bytes().to_vec();
        for slot in 0..=255u8 {
            let Some((_, parameters)) = species.iter().find(|(s, _)| *s == slot) else {
                b.push(0);
                continue;
            };
            b.extend_from_slice(&[1, 2, b'm', 0, 10]);
            b.extend_from_slice(b"atlas.pst\0");
            for i in 0..23 {
                b.extend_from_slice(&parameters.get(i).copied().unwrap_or(0.).to_le_bytes());
            }
            b.push(0);
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.push(0);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.push(0);
        b.extend_from_slice(&(placements.len() as u32).to_le_bytes());
        for (slot, resample, coordinates, height, width) in placements {
            b.push(slot | if *resample { 128 } else { 0 });
            for c in coordinates {
                b.extend_from_slice(&c.to_le_bytes());
            }
            b.extend_from_slice(&[*height as u8, *width as u8]);
        }
        b
    }
    fn fixture() -> Vegetation {
        let ring: [[f64; 2]; 14] = [
            [50., 0.],
            [-40., 60.],
            [150., 20.],
            [130., -90.],
            [170., 0.],
            [175., 40.],
            [300., 10.],
            [-200., 250.],
            [0., -60.],
            [20., -300.],
            [600., 600.],
            [380., 100.],
            [140., 30.],
            [0., 155.],
        ];
        let placements: Vec<Planted> = ring
            .iter()
            .enumerate()
            .map(|(i, [dx, dz])| {
                let wrap = |v: f64| (v * 64.) as u32 as u16;
                let coordinates = [wrap(500. + dx), 640 + i as u16 * 7, wrap(500. + dz)];
                let signed = |n: usize| ((n % 255) as i32 - 127) as i8;
                (
                    if i % 3 == 2 { 3 } else { 1 },
                    i % 4 == 1,
                    coordinates,
                    signed(i * 37),
                    signed(i * 53),
                )
            })
            .collect();
        let tree = [10., 8., 12., 2., 1., 3., 0.25, 0.75, 0.9, 0.1, 0.375];
        let fern = [4., 3., 5., 1.5, 1., 2.];
        let ecosystem =
            crate::ecosystem::decode(&esb(&[(1, &tree), (3, &fern)], &placements)).unwrap();
        let config = Config {
            far: 400.,
            distances: [120., 180.],
            species: vec![
                SpeciesBounds {
                    slot: 1,
                    center_y: 1.5,
                    half_extents: [2., 3.],
                },
                SpeciesBounds {
                    slot: 3,
                    center_y: 0.5,
                    half_extents: [0.5, 1.25],
                },
            ],
        };
        Vegetation::new(ecosystem, &plane().render_heights().unwrap(), &config).unwrap()
    }

    /// Expected values were produced by the former game.js buildEcosystem and
    /// drawEcosystem on this fixture: resampling, distant/transition/near
    /// queues, the stale-visibility reset (frame 7) and near-entry orientation.
    #[test]
    fn selection_matches_the_former_browser_queues() {
        let mut vegetation = fixture();
        let positions: [[f32; 3]; 14] = [
            [550., 10., 500.],
            [460., 30.3125, 560.],
            [650., 10.21875, 520.],
            [630., 10.328125, 410.],
            [670., 10.4375, 500.],
            [675., 40.078125, 540.],
            [800., 10.65625, 510.],
            [300., 10.765625, 750.],
            [500., 10.875, 440.],
            [520., 27.5, 200.],
            [76., 11.09375, 76.],
            [880., 11.203125, 600.],
            [640., 11.3125, 530.],
            [500., 33.671875, 655.],
        ];
        #[allow(clippy::type_complexity)]
        let frames: [([f64; 5], [u32; 5], &[f32], [&[f32]; 2]); 5] = [
            (
                [500., 500., 0., 1., 1.],
                [10, 13, 2, 0, 7],
                &[
                    300., 10.765625, 750., 1.9140625, 8.078125, 1., 0., 64., -0.5, 1.5, 0.25, 0.75,
                    0.375, 0.1, 0.9, -0.25, 800., 10.65625, 510., 1.5, 11.484375, 1., 0., 64.,
                    -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25,
                ],
                [
                    &[
                        500., 33.671875, 655., 1.203125, 1.9244791, 1., 0., 0., 460., 30.3125,
                        560., 0.7109375, 1.4322916, 1., 0., 0., 640., 11.3125, 530., 0.99609375,
                        1.828125, 1., 0., 0., 550., 10., 500., 0.50390625, 1.3359375, 1., 0., 0.,
                        670., 10.4375, 500., 1.3320313, 1.7213541, 1., 0., 0.,
                    ],
                    &[
                        675., 40.078125, 540., 2.0859375, 1.78125, 1., 0., 0., 650., 10.21875,
                        520., 2.8359375, 1.434375, 1., 0., 0.,
                    ],
                ],
            ),
            (
                [500., 500., 0., 1., 2.],
                [10, 13, 3, 2, 7],
                &[
                    300., 10.765625, 750., 1.9140625, 8.078125, 1., 0., 64., -0.5, 1.5, 0.25, 0.75,
                    0.375, 0.1, 0.9, -0.25, 800., 10.65625, 510., 1.5, 11.484375, 1., 0., 64.,
                    -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25, 670., 10.4375, 500., 2.6640625,
                    10.328125, 1., 0., 53., -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25, 500.,
                    33.671875, 655., 2.40625, 11.546875, 1., 0., 37., -0.5, 1.5, 0.25, 0.75, 0.375,
                    0.1, 0.9, -0.25, 640., 11.3125, 530., 1.9921875, 10.96875, 1., 0., 24., -0.5,
                    1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25,
                ],
                [
                    &[
                        500., 33.671875, 655., 1.203125, 1.9244791, 1., 0., 37., 460., 30.3125,
                        560., 0.7109375, 1.4322916, 1., 0., 0., 640., 11.3125, 530., 0.99609375,
                        1.828125, 1., 0., 24., 550., 10., 500., 0.50390625, 1.3359375, 1., 0., 0.,
                        670., 10.4375, 500., 1.3320313, 1.7213541, 1., 0., 53.,
                    ],
                    &[
                        675., 40.078125, 540., 2.0859375, 1.78125, 1., 0., 63., 650., 10.21875,
                        520., 2.8359375, 1.434375, 1., 0., 33.,
                    ],
                ],
            ),
            (
                [510., 495., 0.3894183423086505, 0.9210609940028851, 7.],
                [10, 13, 2, 0, 7],
                &[
                    300., 10.765625, 750., 1.9140625, 8.078125, 1., 0., 64., -0.5, 1.5, 0.25, 0.75,
                    0.375, 0.1, 0.9, -0.25, 800., 10.65625, 510., 1.5, 11.484375, 1., 0., 64.,
                    -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25,
                ],
                [
                    &[
                        500., 33.671875, 655., 1.203125, 1.9244791, 1., 0., 0., 640., 11.3125,
                        530., 0.99609375, 1.828125, 1., 0., 0., 670., 10.4375, 500., 1.3320313,
                        1.7213541, 1., 0., 0., 460., 30.3125, 560., 0.7109375, 1.4322916, 1., 0.,
                        0., 550., 10., 500., 0.50390625, 1.3359375, 1., 0., 0.,
                    ],
                    &[
                        675., 40.078125, 540., 2.0859375, 1.78125, 1., 0., 0., 650., 10.21875,
                        520., 2.8359375, 1.434375, 1., 0., 0.,
                    ],
                ],
            ),
            (
                [505., 520., 0.5984721441039564, -0.8011436155469337, 8.],
                [10, 13, 3, 1, 7],
                &[
                    520., 27.5, 200., 2.7421875, 9.234375, 1., 0., 64., -0.5, 1.5, 0.25, 0.75,
                    0.375, 0.1, 0.9, -0.25, 800., 10.65625, 510., 1.5, 11.484375, 1., 0., 64.,
                    -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25, 670., 10.4375, 500., 2.6640625,
                    10.328125, 1., 0., 49., -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25, 640.,
                    11.3125, 530., 1.9921875, 10.96875, 1., 0., 16., -0.5, 1.5, 0.25, 0.75, 0.375,
                    0.1, 0.9, -0.25,
                ],
                [
                    &[
                        630.,
                        10.328125,
                        410.,
                        1.125,
                        1.625,
                        -0.8011414,
                        -0.59846795,
                        0.,
                        670.,
                        10.4375,
                        500.,
                        1.3320313,
                        1.7213541,
                        1.,
                        0.,
                        49.,
                        640.,
                        11.3125,
                        530.,
                        0.99609375,
                        1.828125,
                        1.,
                        0.,
                        16.,
                        550.,
                        10.,
                        500.,
                        0.50390625,
                        1.3359375,
                        1.,
                        0.,
                        0.,
                    ],
                    &[
                        650.,
                        10.21875,
                        520.,
                        2.8359375,
                        1.434375,
                        1.,
                        0.,
                        26.,
                        675.,
                        40.078125,
                        540.,
                        2.0859375,
                        1.78125,
                        1.,
                        0.,
                        54.,
                        500.,
                        10.875,
                        440.,
                        3.328125,
                        1.33125,
                        -0.8011414,
                        -0.59846795,
                        0.,
                    ],
                ],
            ),
            (
                [500., 500., 0., 1., 9.],
                [10, 13, 3, 2, 7],
                &[
                    300., 10.765625, 750., 1.9140625, 8.078125, 1., 0., 64., -0.5, 1.5, 0.25, 0.75,
                    0.375, 0.1, 0.9, -0.25, 800., 10.65625, 510., 1.5, 11.484375, 1., 0., 64.,
                    -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25, 670., 10.4375, 500., 2.6640625,
                    10.328125, 1., 0., 53., -0.5, 1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25, 500.,
                    33.671875, 655., 2.40625, 11.546875, 1., 0., 37., -0.5, 1.5, 0.25, 0.75, 0.375,
                    0.1, 0.9, -0.25, 640., 11.3125, 530., 1.9921875, 10.96875, 1., 0., 24., -0.5,
                    1.5, 0.25, 0.75, 0.375, 0.1, 0.9, -0.25,
                ],
                [
                    &[
                        500., 33.671875, 655., 1.203125, 1.9244791, 1., 0., 37., 460., 30.3125,
                        560., 0.7109375, 1.4322916, 1., 0., 0., 640., 11.3125, 530., 0.99609375,
                        1.828125, 1., 0., 24., 550., 10., 500., 0.50390625, 1.3359375, 1., 0., 0.,
                        670., 10.4375, 500., 1.3320313, 1.7213541, 1., 0., 53.,
                    ],
                    &[
                        675., 40.078125, 540., 2.0859375, 1.78125, 1., 0., 63., 650., 10.21875,
                        520., 2.8359375, 1.434375, 1., 0., 33.,
                    ],
                ],
            ),
        ];
        for (index, position) in positions.iter().enumerate() {
            assert_eq!(
                vegetation.placement(index).unwrap().1.map(|v| v as f32),
                *position
            );
        }
        for ([x, z, forward_x, forward_z, frame], counts, billboards, groups) in frames {
            let actual = vegetation.select([x, z], [forward_x, forward_z], frame);
            assert_eq!(
                [
                    actual.visible,
                    actual.scanned,
                    actual.far,
                    actual.transition,
                    actual.near
                ],
                counts
            );
            assert_eq!(&vegetation.output[..billboards.len()], billboards);
            for (group, expected) in groups.iter().enumerate() {
                assert_eq!(
                    vegetation.group_counts[group] as usize * MESH_FLOATS,
                    expected.len()
                );
                let at = vegetation.group_offset(group);
                assert_eq!(&vegetation.output[at..at + expected.len()], *expected);
            }
        }
    }

    #[test]
    fn heap_keeps_the_nearest_depths_and_rejects_invalid_cameras() {
        let mut vegetation = fixture();
        let template = vegetation.placements[0].clone();
        vegetation.placements = (0..CAPACITY + 500)
            .map(|i| Placement {
                position: [500. + i as f64 * 0.01, 0., 501.],
                coordinates: [32000, 0, 32064],
                ..template.clone()
            })
            .collect();
        vegetation.offsets = vec![0; 4097];
        vegetation.offsets[7 * 64 + 7 + 1..].fill((CAPACITY + 500) as u32);
        vegetation.indices = (0..(CAPACITY + 500) as u32).rev().collect();
        let counts = vegetation.select([500., 500.], [1., 0.], 1.);
        assert_eq!(counts.visible as usize, CAPACITY);
        let selection = vegetation.selection();
        assert!(selection.windows(2).all(|w| w[0].1 >= w[1].1));
        assert_eq!(selection.last().unwrap().0, 0);
        assert_eq!(selection[0].0 as usize, CAPACITY - 1);
        assert_eq!(
            vegetation.select([f64::NAN, 500.], [1., 0.], 2.),
            Counts::default()
        );
    }
}
