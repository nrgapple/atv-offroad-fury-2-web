//! Native0033e4b0/0033e350 candidate-cache policy, independent of spatial
//! registration, category eligibility and shape-specific distance providers.
use crate::{
    Result,
    scene_partition::Query,
    wheel::{Vec3, dot, sub},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeClass {
    CollisionObject,
    Vegetation,
    Other,
}
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub index: usize,
    pub distance: f32,
    pub class: NodeClass,
    /// CollisionObject eligibility from0011eeb0. Vegetation uses its own path.
    pub eligible: bool,
    /// Native mode bitbf.3 (ordinary static placements set this bit).
    pub static_mode: bool,
}
#[derive(Debug, Clone)]
pub struct CandidateCache {
    min_radius: f32,
    max_radius: f32,
    center: Vec3,
    radius: f32,
    valid: bool,
    pub ignore_vegetation: bool,
    entries: Vec<(usize, f32)>,
}
impl CandidateCache {
    pub fn new(min_radius: f32, max_radius: f32) -> Result<Self> {
        if !min_radius.is_finite()
            || !max_radius.is_finite()
            || min_radius < 0.
            || max_radius < min_radius
        {
            return Err("invalid native scene cache radii".into());
        }
        Ok(Self {
            min_radius,
            max_radius,
            center: [0.; 3],
            radius: 0.,
            valid: false,
            ignore_vegetation: false,
            entries: Vec::new(),
        })
    }
    pub fn entries(&self) -> &[(usize, f32)] {
        &self.entries
    }
    pub fn is_valid(&self) -> bool {
        self.valid
    }
    pub fn radius(&self) -> f32 {
        self.radius
    }
    /// Evaluate before querying the partition; native equality refreshes.
    pub fn needs_refresh(&self, center: Vec3, query_radius: f32) -> Result<bool> {
        if !center.iter().all(|v| v.is_finite()) || !query_radius.is_finite() || query_radius < 0. {
            return Err("invalid native scene cache query".into());
        }
        let delta = sub(center, self.center);
        Ok(!self.valid || self.radius <= dot(delta, delta).sqrt() + query_radius)
    }
    /// The supplied cell bounds and candidate sequence must come from the
    /// original partition query at center with half-extents=min_radius.
    /// Shape distance and category eligibility remain caller-owned.
    pub fn rebuild(
        &mut self,
        center: Vec3,
        partition_query: &Query,
        candidates: impl IntoIterator<Item = Candidate>,
    ) -> Result<()> {
        if !center
            .iter()
            .chain(partition_query.cell_min.iter())
            .chain(partition_query.cell_max.iter())
            .all(|v| v.is_finite())
        {
            return Err("invalid scene cache partition bounds".into());
        }
        let lo = partition_query.cell_min;
        let hi = partition_query.cell_max;
        let radius = (center[0] - lo[0])
            .min(center[2] - lo[2])
            .min(hi[0] - center[0])
            .min(hi[2] - center[2])
            .min(self.max_radius)
            .max(self.min_radius);
        let mut entries = Vec::new();
        for candidate in candidates {
            if entries.len() == 256 {
                break;
            }
            if !candidate.distance.is_finite() || candidate.distance < 0. {
                return Err("invalid scene candidate distance".into());
            }
            let keep = match candidate.class {
                NodeClass::CollisionObject => {
                    candidate.eligible && (!candidate.static_mode || candidate.distance < radius)
                }
                NodeClass::Vegetation => !self.ignore_vegetation && candidate.distance < radius,
                NodeClass::Other => false,
            };
            if keep {
                entries.push((candidate.index, candidate.distance));
            }
        }
        self.center = center;
        self.radius = radius;
        self.entries = entries;
        self.valid = true;
        Ok(())
    }
    /// Reuse preserves membership/order, even when eligibility changes.
    /// Native0033e350 only recomputes shape distance for the retained nodes.
    pub fn update_distances(
        &mut self,
        mut distance: impl FnMut(usize) -> Result<f32>,
    ) -> Result<()> {
        for (index, value) in &mut self.entries {
            let next = distance(*index)?;
            if !next.is_finite() || next < 0. {
                return Err("invalid reused scene candidate distance".into());
            }
            *value = next;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bounds() -> Query {
        Query {
            indices: Vec::new(),
            cell_min: [-100.; 3],
            cell_max: [100.; 3],
        }
    }
    #[test]
    fn native_strict_static_radius_and_equality_refresh_keep_order() {
        let mut cache = CandidateCache::new(40., 50.).unwrap();
        cache
            .rebuild(
                [0.; 3],
                &bounds(),
                [
                    Candidate {
                        index: 9,
                        distance: 50.,
                        class: NodeClass::CollisionObject,
                        eligible: true,
                        static_mode: true,
                    },
                    Candidate {
                        index: 4,
                        distance: 100.,
                        class: NodeClass::CollisionObject,
                        eligible: true,
                        static_mode: false,
                    },
                    Candidate {
                        index: 2,
                        distance: 0.,
                        class: NodeClass::Other,
                        eligible: true,
                        static_mode: false,
                    },
                    Candidate {
                        index: 8,
                        distance: 49.,
                        class: NodeClass::CollisionObject,
                        eligible: true,
                        static_mode: true,
                    },
                ],
            )
            .unwrap();
        assert_eq!(cache.entries(), [(4, 100.), (8, 49.)]);
        assert!(!cache.needs_refresh([46., 0., 0.], 3.).unwrap());
        assert!(cache.needs_refresh([47., 0., 0.], 3.).unwrap());
        cache.update_distances(|id| Ok(id as f32)).unwrap();
        assert_eq!(cache.entries(), [(4, 4.), (8, 8.)]);
    }
    #[test]
    fn vegetation_consumes_capacity_and_uses_separate_eligibility_path() {
        let mut cache = CandidateCache::new(40., 50.).unwrap();
        let candidates = || {
            (0..260).map(|index| Candidate {
                index,
                distance: 0.,
                class: if index == 0 {
                    NodeClass::Vegetation
                } else {
                    NodeClass::CollisionObject
                },
                eligible: index != 0,
                static_mode: true,
            })
        };
        cache.rebuild([0.; 3], &bounds(), candidates()).unwrap();
        assert_eq!(cache.entries().len(), 256);
        assert_eq!(cache.entries()[0], (0, 0.));
        assert_eq!(cache.entries()[255], (255, 0.));
        cache.ignore_vegetation = true;
        cache.rebuild([0.; 3], &bounds(), candidates()).unwrap();
        assert_eq!(cache.entries()[0], (1, 0.));
        assert_eq!(cache.entries()[255], (256, 0.));
    }
}
