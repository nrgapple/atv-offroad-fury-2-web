//! Original ordinary-static spatial partition registration and query order.
//! USA v2.00 00161d50/00162060/00162b80/00162ed0/00163120. Runtime
//! portals, dynamic insertion/removal, visibility and category filters are separate.
use crate::{Result, wheel::Vec3};

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub center: Vec3,
    pub half: Vec3,
}
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub cell_size: f32,
}
#[derive(Debug, Clone, Default)]
struct Node {
    children: [Option<usize>; 4],
    entries: Vec<usize>,
}
#[derive(Debug, Clone)]
pub struct StaticPartition {
    config: Config,
    scale: f32,
    world_per_unit: f32,
    sizes: Vec<i32>,
    nodes: Vec<Node>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// Native iterator order, with one occurrence per registered object.
    pub indices: Vec<usize>,
    /// Native quantized query-cell bounds; Y is unbounded. Coordinates are
    /// relative to the partition minimum, as returned by00162b80.
    pub cell_min: Vec3,
    pub cell_max: Vec3,
}
impl StaticPartition {
    /// Register the supplied bounds in their original loader order.
    pub fn new(config: Config, objects: &[Bounds]) -> Result<Self> {
        let values = [
            config.min_x,
            config.max_x,
            config.min_z,
            config.max_z,
            config.cell_size,
        ];
        if !values.iter().all(|v| v.is_finite())
            || config.max_x <= config.min_x
            || config.max_z <= config.min_z
            || config.cell_size <= 0.
        {
            return Err("invalid native spatial partition dimensions".into());
        }
        let scale = 32768. / (config.max_x - config.min_x).max(config.max_z - config.min_z);
        if !scale.is_finite() || scale <= 0. {
            return Err("spatial partition scale is not representable".into());
        }
        let mut sizes = Vec::new();
        let mut size = 32768_i32;
        loop {
            size >>= 1;
            sizes.push(size);
            if sizes.len() == 12 || (config.cell_size * scale) as i32 >= size {
                break;
            }
        }
        let mut partition = Self {
            config,
            scale,
            world_per_unit: 1. / scale,
            sizes,
            nodes: vec![Node::default()],
        };
        for (id, bounds) in objects.iter().enumerate() {
            if !bounds
                .center
                .iter()
                .chain(bounds.half.iter())
                .all(|v| v.is_finite())
                || bounds.half.iter().any(|&v| v < 0.)
            {
                return Err("invalid spatial object bounds".into());
            }
            let key = partition.registration_key(*bounds);
            if key != 0 {
                partition.insert(id, key);
            }
        }
        Ok(partition)
    }
    /// Native packed insertion key. The hierarchy uses only X/Z bounds.
    pub fn registration_key(&self, bounds: Bounds) -> u32 {
        let c = self.config;
        let min_x = (bounds.center[0] - bounds.half[0]).max(c.min_x);
        let max_x = (bounds.center[0] + bounds.half[0]).min(c.max_x);
        let min_z = (bounds.center[2] - bounds.half[2]).max(c.min_z);
        let max_z = (bounds.center[2] + bounds.half[2]).min(c.max_z);
        if max_x - c.min_x < 0.
            || c.max_x - c.min_x < min_x - c.min_x
            || max_z - c.min_z < 0.
            || c.max_z - c.min_z < min_z - c.min_z
        {
            return 0;
        }
        let mut x0 = ((min_x - c.min_x) * self.scale) as i32;
        let mut z0 = ((min_z - c.min_z) * self.scale) as i32;
        let mut x1 = ((max_x - c.min_x) * self.scale) as i32 + 1;
        let mut z1 = ((max_z - c.min_z) * self.scale) as i32 + 1;
        let mut level = 0;
        while level < self.sizes.len() && x1 - x0 < self.sizes[level] && z1 - z0 < self.sizes[level]
        {
            level += 1;
        }
        level = level.min(self.sizes.len() - 1);
        let size = self.sizes[level];
        let mask = !(size - 1);
        x0 &= mask;
        x1 &= mask;
        z0 &= mask;
        z1 &= mask;
        if (x1 - x0 > size || z1 - z0 > size) && level != 0 {
            let mask = !(size * 2 - 1);
            level -= 1;
            x0 &= mask;
            x1 &= mask;
            z0 &= mask;
            z1 &= mask;
        }
        let mut flags = 0x10_u32;
        if z1 != z0 {
            flags = 0x30;
        }
        if x1 != x0 {
            flags |= 0x40;
        }
        if flags & 0x60 == 0x60 {
            flags |= 0x80;
        }
        level as u32 | flags | ((x0.min(0x7fff) as u32) << 17) | ((z0.min(0x7fff) as u32) << 5)
    }
    fn insert(&mut self, id: usize, key: u32) {
        let level = (key & 0xf) as usize;
        let size = self.sizes[level];
        for quadrant in 0..4 {
            if key & (0x10 << quadrant) == 0 {
                continue;
            }
            let x = ((key & 0xfff00000) >> 17) as i32 + if quadrant & 2 != 0 { size } else { 0 };
            let z = ((key & 0xfff00) >> 5) as i32 + if quadrant & 1 != 0 { size } else { 0 };
            let mut node = 0;
            let mut bit = 0x4000;
            for _ in 0..=level {
                let child = usize::from(x & bit != 0) * 2 + usize::from(z & bit != 0);
                let next = if let Some(next) = self.nodes[node].children[child] {
                    next
                } else {
                    let next = self.nodes.len();
                    self.nodes.push(Node::default());
                    self.nodes[node].children[child] = Some(next);
                    next
                };
                node = next;
                bit >>= 1;
            }
            self.nodes[node].entries.push(id);
        }
    }
    pub fn query(&self, bounds: Bounds) -> Result<Option<Query>> {
        if !bounds
            .center
            .iter()
            .chain(bounds.half.iter())
            .all(|v| v.is_finite())
            || bounds.half.iter().any(|&v| v < 0.)
        {
            return Err("invalid spatial query bounds".into());
        }
        let c = self.config;
        let low_x = (bounds.center[0] - bounds.half[0]) - c.min_x;
        let high_x = (bounds.center[0] + bounds.half[0]) - c.min_x;
        let low_z = (bounds.center[2] - bounds.half[2]) - c.min_z;
        let high_z = (bounds.center[2] + bounds.half[2]) - c.min_z;
        if high_x < 0. || low_x > c.max_x - c.min_x || high_z < 0. || low_z > c.max_z - c.min_z {
            return Ok(None);
        }
        let size = *self.sizes.last().unwrap();
        let mask = !(size - 1);
        let rectangle = [
            ((low_x * self.scale) as i32) & mask,
            ((low_z * self.scale) as i32) & mask,
            size - 1 + (((high_x * self.scale) as i32) & mask),
            size - 1 + (((high_z * self.scale) as i32) & mask),
        ];
        let mut cells = Vec::new();
        self.visit(0, 0, 0, 0x4000, rectangle, &mut cells);
        let mut indices = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for cell in cells.into_iter().rev() {
            for &id in &self.nodes[cell].entries {
                if seen.insert(id) {
                    indices.push(id);
                }
            }
        }
        Ok(Some(Query {
            indices,
            cell_min: [
                rectangle[0] as f32 * self.world_per_unit,
                -f32::MAX,
                rectangle[1] as f32 * self.world_per_unit,
            ],
            cell_max: [
                rectangle[2] as f32 * self.world_per_unit,
                f32::MAX,
                rectangle[3] as f32 * self.world_per_unit,
            ],
        }))
    }
    fn all_cells(&self, node: usize, cells: &mut Vec<usize>) {
        for child in self.nodes[node].children.into_iter().flatten() {
            self.all_cells(child, cells);
        }
        if !self.nodes[node].entries.is_empty() {
            cells.push(node);
        }
    }
    fn visit(&self, node: usize, x: i32, z: i32, size: i32, q: [i32; 4], cells: &mut Vec<usize>) {
        if *self.sizes.last().unwrap() <= size {
            for quadrant in 0..4 {
                let Some(child) = self.nodes[node].children[quadrant] else {
                    continue;
                };
                let x = x + if quadrant & 2 != 0 { size } else { 0 };
                let z = z + if quadrant & 1 != 0 { size } else { 0 };
                if q[0] <= x + size && x <= q[2] && q[1] <= z + size && z <= q[3] {
                    if x < q[0] || q[2] < x + size || z < q[1] || q[3] < z + size {
                        self.visit(child, x, z, size >> 1, q, cells);
                    } else {
                        self.all_cells(child, cells);
                    }
                }
            }
        }
        if !self.nodes[node].entries.is_empty() {
            cells.push(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_order_crosses_cells_and_deduplicates_spanning_objects() {
        // Synthetic original-instruction checkpoint; translated/non-square
        // region and overlapping footprints. Returned order is not loader order.
        let objects = [
            Bounds {
                center: [
                    f32::from_bits(1152306791),
                    f32::from_bits(1092616192),
                    f32::from_bits(1156062472),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152723658),
                    f32::from_bits(1092616192),
                    f32::from_bits(1155851454),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152893269),
                    f32::from_bits(1092616192),
                    f32::from_bits(1155742003),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152786992),
                    f32::from_bits(1092616192),
                    f32::from_bits(1155998998),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152101814),
                    f32::from_bits(1092616192),
                    f32::from_bits(1155833925),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152301303),
                    f32::from_bits(1092616192),
                    f32::from_bits(1156398336),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152941773),
                    f32::from_bits(1092616192),
                    f32::from_bits(1156173920),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
            Bounds {
                center: [
                    f32::from_bits(1152397714),
                    f32::from_bits(1092616192),
                    f32::from_bits(1156147547),
                ],
                half: [
                    f32::from_bits(1117782016),
                    f32::from_bits(1084227584),
                    f32::from_bits(1117782016),
                ],
            },
        ];
        let tree = StaticPartition::new(
            Config {
                min_x: f32::from_bits(3291644137),
                min_z: f32::from_bits(3286575507),
                cell_size: f32::from_bits(1098907648),
                max_x: f32::from_bits(1163824494),
                max_z: f32::from_bits(1166239642),
            },
            &objects,
        )
        .unwrap();
        let query = tree
            .query(Bounds {
                center: [
                    f32::from_bits(1152511284),
                    f32::from_bits(0),
                    f32::from_bits(1156196866),
                ],
                half: [
                    f32::from_bits(1109393408),
                    f32::from_bits(1109393408),
                    f32::from_bits(1109393408),
                ],
            })
            .unwrap()
            .unwrap();
        assert_eq!(query.indices, [0, 5, 1, 2, 3, 4, 6, 7]);
        assert_eq!(tree.registration_key(objects[0]), 1611071731);
    }
    #[test]
    fn outside_bounds_do_not_register_or_query_and_y_does_not_partition() {
        let config = Config {
            min_x: 0.,
            max_x: 4096.,
            min_z: 0.,
            max_z: 4096.,
            cell_size: 64.,
        };
        let objects = [
            Bounds {
                center: [-100., 0., -100.],
                half: [1.; 3],
            },
            Bounds {
                center: [1024., 5000., 1024.],
                half: [10.; 3],
            },
        ];
        let tree = StaticPartition::new(config, &objects).unwrap();
        assert_eq!(tree.registration_key(objects[0]), 0);
        assert!(tree.query(objects[0]).unwrap().is_none());
        let q = tree
            .query(Bounds {
                center: [1024., -5000., 1024.],
                half: [1.; 3],
            })
            .unwrap()
            .unwrap();
        assert_eq!(q.indices, [1]);
    }
}
