//! Combined static/ESB partition and shared quad/recovery candidate cache.
//! Shipped cylinders expand in native mixed-node order for quad, wheel and active
//! wreck-rider consumers. Dynamic nodes and exceptional actor filters remain open.
use crate::{
    Result,
    ecosystem::Ecosystem,
    obstacle::{StaticObstacle, sphere_separation, transform_point},
    scene_candidates::{Candidate, CandidateCache, NodeClass},
    scene_partition::{Bounds, Config, Query, StaticPartition},
    terrain::Terrain,
    vegetation_collision,
    wheel::{Vec3, dot},
};
use std::rc::Rc;

/// Index within its original loader stage, never a distance-sorted index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneNode {
    StaticObstacle(usize),
    Vegetation(usize),
}

#[derive(Debug, Clone, Copy)]
pub enum PhysicalTrim {
    Supplied,
    CachedRider,
}

///Geometry instances in original mixed-node and per-species proxy order.
///Unsupported definitions retain their slots without becoming a guessed shape.
pub struct PhysicalMesh {
    pub node: SceneNode,
    pub proxy_index: Option<usize>,
    pub obstacle: StaticObstacle,
}
#[derive(Default)]
pub struct PhysicalSelection {
    pub meshes: Vec<PhysicalMesh>,
    pub static_candidates: usize,
    pub vegetation_candidates: usize,
    pub unsupported_vegetation_proxies: usize,
}
impl PhysicalSelection {
    pub fn vegetation_proxies(&self) -> usize {
        self.meshes
            .iter()
            .filter(|mesh| mesh.proxy_index.is_some())
            .count()
    }
}

pub struct SceneContacts {
    pub contacts: Vec<crate::obstacle::ObstacleContact>,
    pub vegetation_proxies: usize,
    pub vegetation_callbacks: usize,
    pub unsupported_vegetation_proxies: usize,
}

/// Direct type-3 camera query, independent of the retained quad/rider cache.
pub struct DirectCameraQuery<'a> {
    pub bounds: Bounds,
    pub radius: f32,
    pub obstacles: Vec<&'a StaticObstacle>,
}

#[derive(Debug, Clone)]
pub struct RegisteredVegetation {
    pub slot: u8,
    /// Final packed coordinates, including tagged/forced height resampling.
    pub coordinates: [u16; 3],
    pub height: f32,
    pub radius: f32,
    /// Configured species count; supported geometry is expanded by each consumer.
    pub proxy_count: usize,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Copy)]
pub struct RetainedCandidate {
    pub node: SceneNode,
    /// Native shape separation with provider query radius zero.
    pub distance: f32,
}

#[derive(Debug, Clone)]
pub struct CachedQuery {
    pub center: Vec3,
    pub query_radius: f32,
    pub cache_radius: f32,
    pub refreshed: bool,
    /// All retained cache entries, before the consumer's sphere-radius trim.
    pub candidates: Vec<RetainedCandidate>,
}
impl CachedQuery {
    ///0011fd30 includes exact-radius nodes, unlike the quad/recovery trim.
    pub fn inclusively_within(&self) -> impl Iterator<Item = &RetainedCandidate> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.distance <= self.query_radius)
    }
    ///001dd830 and001ea050 use a strict consumer gate. Cached rider0011fd30
    ///instead uses inclusive distance and its own eligibility/proxy callbacks.
    pub fn strictly_within(&self) -> impl Iterator<Item = &RetainedCandidate> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.distance < self.query_radius)
    }
}

#[derive(Debug, Clone, Copy)]
struct StaticSphere {
    center: Vec3,
    radius: f32,
}

#[derive(Debug, Clone)]
pub struct SceneCollision {
    obstacles: Vec<StaticObstacle>,
    static_spheres: Vec<StaticSphere>,
    vegetation: Vec<RegisteredVegetation>,
    vegetation_meshes: [Vec<Option<Rc<crate::collision::CollisionMesh>>>; 256],
    coordinate_unit: f32,
    coordinate_reciprocal: f32,
    partition: StaticPartition,
    cache: CandidateCache,
}
impl SceneCollision {
    ///00214b80 loads ordinary obstacles first, then every ESB placement in
    ///file order, including species without proxies. The sampler receives the
    ///packed XYZ converted with extent/65536, before Y is requantized using
    ///the independently rounded65536/extent. Ordinary eligibility is supplied
    ///per query, rather than baked into this scene's membership.
    pub fn new(
        config: Config,
        obstacles: Vec<StaticObstacle>,
        ecosystem: Option<&Ecosystem>,
        force_resample_height: bool,
        mut sample_height: impl FnMut(Vec3) -> Result<f32>,
    ) -> Result<Self> {
        let extent = (config.max_x - config.min_x).max(config.max_z - config.min_z);
        let coordinate_unit = extent / 65536.;
        let coordinate_reciprocal = 65536. / extent;
        if !extent.is_finite()
            || extent <= 0.
            || !coordinate_unit.is_finite()
            || coordinate_unit <= 0.
            || !coordinate_reciprocal.is_finite()
        {
            return Err("invalid combined collision partition extent".into());
        }
        let mut bounds = Vec::with_capacity(
            obstacles.len() + ecosystem.map_or(0, |ecosystem| ecosystem.placements.len()),
        );
        let mut static_spheres = Vec::with_capacity(obstacles.len());
        for obstacle in &obstacles {
            bounds.push(obstacle.world_bounds()?);
            let root = obstacle.mesh.nodes.first().ok_or("missing obstacle root")?;
            let sphere = StaticSphere {
                center: transform_point(&obstacle.world_matrix, root.center),
                //003302b0 retains the local root diagonal even under scaled
                //transforms; transformed AABB extents are registration only.
                radius: dot(root.half_extents, root.half_extents).sqrt(),
            };
            if !sphere.radius.is_finite() {
                return Err("nonfinite ordinary collision sphere".into());
            }
            static_spheres.push(sphere);
        }
        let mut vegetation = Vec::new();
        let mut vegetation_meshes = std::array::from_fn(|_| Vec::new());
        if let Some(ecosystem) = ecosystem {
            let mut proxy_counts = [None; 256];
            for species in &ecosystem.species {
                if proxy_counts[usize::from(species.slot)].is_some() {
                    return Err("duplicate collision ecosystem species slot".into());
                }
                proxy_counts[usize::from(species.slot)] = Some(species.proxies.len());
                for words in &species.proxies {
                    let mesh = match crate::vegetation_proxy::Record::parse(words)? {
                        crate::vegetation_proxy::Record::Cylinder(cylinder) => Some(Rc::new(
                            crate::vegetation_ray::cylinder_collision(&cylinder.mesh()?)?,
                        )),
                        //All eight shipped Freestyle ecosystems use cylinders.
                        //Other definitions need their own verified resource or
                        //dispatch contracts; preserve count/order meanwhile.
                        _ => None,
                    };
                    vegetation_meshes[usize::from(species.slot)].push(mesh);
                }
            }
            vegetation.reserve(ecosystem.placements.len());
            for placement in &ecosystem.placements {
                let proxy_count = proxy_counts[usize::from(placement.slot)]
                    .ok_or("collision placement references absent species")?;
                let mut coordinates = placement.coordinates;
                if force_resample_height || placement.resample_height {
                    let position =
                        vegetation_collision::placement_position(coordinates, coordinate_unit)?;
                    coordinates = vegetation_collision::resampled_coordinates(
                        coordinates,
                        sample_height(position)?,
                        coordinate_reciprocal,
                    )?;
                }
                let plant_bounds = vegetation_collision::placement_bounds(
                    coordinates,
                    coordinate_unit,
                    placement.height,
                    placement.radius,
                )?;
                bounds.push(plant_bounds);
                vegetation.push(RegisteredVegetation {
                    slot: placement.slot,
                    coordinates,
                    height: placement.height,
                    radius: placement.radius,
                    proxy_count,
                    bounds: plant_bounds,
                });
            }
        }
        let partition = StaticPartition::new(config, &bounds)?;
        Ok(Self {
            obstacles,
            static_spheres,
            vegetation,
            vegetation_meshes,
            coordinate_unit,
            coordinate_reciprocal,
            partition,
            cache: CandidateCache::new(40., 50.)?,
        })
    }

    ///The main scene loader00200900 passes force-resample false; ESB tag bits
    ///still resample. Terrain0016ac10 leaves input Y unchanged without a cell.
    pub fn from_terrain(
        config: Config,
        obstacles: Vec<StaticObstacle>,
        ecosystem: Option<&Ecosystem>,
        terrain: &Terrain,
        force_resample_height: bool,
    ) -> Result<Self> {
        Self::new(
            config,
            obstacles,
            ecosystem,
            force_resample_height,
            |point| {
                Ok(terrain
                    .contact(point[0], point[2])?
                    .map_or(point[1], |hit| hit.height))
            },
        )
    }

    pub fn obstacles(&self) -> &[StaticObstacle] {
        &self.obstacles
    }
    pub fn vegetation(&self) -> &[RegisteredVegetation] {
        &self.vegetation
    }
    pub fn coordinate_unit(&self) -> f32 {
        self.coordinate_unit
    }
    pub fn coordinate_reciprocal(&self) -> f32 {
        self.coordinate_reciprocal
    }
    pub fn cache(&self) -> &CandidateCache {
        &self.cache
    }
    ///Collector field+82c differs from actor query bitbf.0, which controls
    ///physical vegetation expansion. Changing this flag does not force a
    ///refresh: existing cache membership survives until native invalidation.
    pub fn set_ignore_vegetation(&mut self, ignore: bool) {
        self.cache.ignore_vegetation = ignore;
    }
    fn node(&self, index: usize) -> SceneNode {
        if index < self.obstacles.len() {
            SceneNode::StaticObstacle(index)
        } else {
            SceneNode::Vegetation(index - self.obstacles.len())
        }
    }
    fn distance(&self, center: Vec3, index: usize) -> Result<f32> {
        match self.node(index) {
            SceneNode::StaticObstacle(index) => {
                let sphere = self.static_spheres[index];
                Ok(sphere_separation(center, 0., sphere.center, sphere.radius))
            }
            SceneNode::Vegetation(index) => {
                let plant = &self.vegetation[index];
                vegetation_collision::horizontal_separation(
                    center,
                    0.,
                    vegetation_collision::placement_position(
                        plant.coordinates,
                        self.coordinate_unit,
                    )?,
                    plant.radius,
                )
            }
        }
    }
    ///The quad's radius is the fixed bind-bound half-extent diagonal configured
    ///by001e1fd0; its saved local center is transformed per frame. Recovery uses
    ///radius20. The adapter never derives either radius from animated bounds.
    pub fn cached_query(
        &mut self,
        center: Vec3,
        query_radius: f32,
        mut ordinary_eligible: impl FnMut(usize, &StaticObstacle) -> bool,
    ) -> Result<CachedQuery> {
        let refreshed = self.cache.needs_refresh(center, query_radius)?;
        if refreshed {
            //00162b80 leaves caller-initialized zero cell bounds untouched
            //when its search is wholly outside the partition. Empty membership
            //still becomes valid with native minimum cache radius forty.
            let query = self
                .partition
                .query(Bounds {
                    center,
                    half: [40.; 3],
                })?
                .unwrap_or_else(|| Query {
                    indices: Vec::new(),
                    cell_min: [0.; 3],
                    cell_max: [0.; 3],
                });
            let candidates = query
                .indices
                .iter()
                .map(|&index| {
                    let node = self.node(index);
                    Ok(Candidate {
                        index,
                        distance: self.distance(center, index)?,
                        class: match node {
                            SceneNode::StaticObstacle(_) => NodeClass::CollisionObject,
                            SceneNode::Vegetation(_) => NodeClass::Vegetation,
                        },
                        eligible: match node {
                            SceneNode::StaticObstacle(index) => {
                                ordinary_eligible(index, &self.obstacles[index])
                            }
                            SceneNode::Vegetation(_) => false,
                        },
                        static_mode: true,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            self.cache.rebuild(center, &query, candidates)?;
        } else {
            let distances = self
                .cache
                .entries()
                .iter()
                .map(|&(index, _)| {
                    self.distance(center, index)
                        .map(|distance| (index, distance))
                })
                .collect::<Result<Vec<_>>>()?;
            let mut values = distances.into_iter();
            self.cache.update_distances(|index| {
                let (stored_index, distance) =
                    values.next().ok_or("missing retained scene distance")?;
                debug_assert_eq!(stored_index, index);
                Ok(distance)
            })?;
        }
        Ok(CachedQuery {
            center,
            query_radius,
            cache_radius: self.cache.radius(),
            refreshed,
            candidates: self
                .cache
                .entries()
                .iter()
                .map(|&(index, distance)| RetainedCandidate {
                    node: self.node(index),
                    distance,
                })
                .collect(),
        })
    }
    ///Ordinary-only view after the caller's strict sphere-radius trim. Plants
    ///remain in CachedQuery and consume the same shared256-entry capacity.
    pub fn static_selection<'a>(&'a self, query: &CachedQuery) -> Vec<&'a StaticObstacle> {
        query
            .strictly_within()
            .filter_map(|candidate| match candidate.node {
                SceneNode::StaticObstacle(index) => Some(&self.obstacles[index]),
                SceneNode::Vegetation(_) => None,
            })
            .collect()
    }
    ///Cached rider0011fd30 rechecks current ordinary eligibility even on cache
    ///reuse. A refresh uses that same actor's eligibility for cache membership.
    pub fn inclusive_static_selection<'a>(
        &'a self,
        query: &CachedQuery,
        mut ordinary_eligible: impl FnMut(usize, &StaticObstacle) -> bool,
    ) -> Vec<&'a StaticObstacle> {
        query
            .inclusively_within()
            .filter_map(|candidate| match candidate.node {
                SceneNode::StaticObstacle(index)
                    if ordinary_eligible(index, &self.obstacles[index]) =>
                {
                    Some(&self.obstacles[index])
                }
                _ => None,
            })
            .collect()
    }

    ///0011f9a0 supplied consumers have a separate inclusive horizontal plant
    ///sphere gate. 0011fd30 cached riders use their inclusive retained distance
    ///instead. Actor bf.0 suppresses expansion without changing membership.
    pub fn physical_selection(
        &self,
        query: &CachedQuery,
        trim: PhysicalTrim,
        actor_center: Vec3,
        actor_radius: f32,
        skip_vegetation: bool,
        mut ordinary_eligible: impl FnMut(usize, &StaticObstacle) -> bool,
    ) -> Result<PhysicalSelection> {
        let mut selected = PhysicalSelection::default();
        for candidate in &query.candidates {
            let included = match trim {
                PhysicalTrim::Supplied => candidate.distance < query.query_radius,
                PhysicalTrim::CachedRider => candidate.distance <= query.query_radius,
            };
            if !included {
                continue;
            }
            match candidate.node {
                SceneNode::StaticObstacle(index)
                    if ordinary_eligible(index, &self.obstacles[index]) =>
                {
                    selected.static_candidates += 1;
                    selected.meshes.push(PhysicalMesh {
                        node: candidate.node,
                        proxy_index: None,
                        obstacle: self.obstacles[index].clone(),
                    });
                }
                SceneNode::Vegetation(index) => {
                    selected.vegetation_candidates += 1;
                    if skip_vegetation {
                        continue;
                    }
                    let plant = &self.vegetation[index];
                    let position = vegetation_collision::placement_position(
                        plant.coordinates,
                        self.coordinate_unit,
                    )?;
                    if matches!(trim, PhysicalTrim::Supplied)
                        && vegetation_collision::horizontal_separation(
                            actor_center,
                            actor_radius,
                            position,
                            plant.radius,
                        )? != 0.
                    {
                        continue;
                    }
                    //001280b0 copies the runtime0 template basis and replaces
                    //its packed XYZ translation; visual size ratios are unused.
                    let world_matrix = [
                        1.,
                        0.,
                        0.,
                        0.,
                        0.,
                        1.,
                        0.,
                        0.,
                        0.,
                        0.,
                        1.,
                        0.,
                        position[0],
                        position[1],
                        position[2],
                        1.,
                    ];
                    for (proxy_index, mesh) in self.vegetation_meshes[usize::from(plant.slot)]
                        .iter()
                        .enumerate()
                    {
                        if let Some(mesh) = mesh {
                            selected.meshes.push(PhysicalMesh {
                                node: candidate.node,
                                proxy_index: Some(proxy_index),
                                obstacle: StaticObstacle {
                                    mesh: mesh.clone(),
                                    world_matrix,
                                },
                            });
                        } else {
                            selected.unsupported_vegetation_proxies += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(selected)
    }

    ///Quad AR/AO type2 geometry callbacks retain one nearest-hit accumulator
    ///across ordinary meshes and every expanded plant proxy in native order.
    pub fn quad_contacts(
        &self,
        query: &CachedQuery,
        rays: &crate::collision::CollisionRayTree,
        world: &[f32; 16],
        skip_vegetation: bool,
    ) -> Result<SceneContacts> {
        let mut geometry = crate::obstacle::SceneContactQuery::new(rays, world)?;
        let selected = self.physical_selection(
            query,
            PhysicalTrim::Supplied,
            geometry.center,
            geometry.radius,
            skip_vegetation,
            |_, _| true,
        )?;
        let vegetation_proxies = selected.vegetation_proxies();
        let mut vegetation_callbacks = 0;
        for mesh in &selected.meshes {
            let plant = mesh.proxy_index.is_some();
            if geometry.visit_mesh(&mesh.obstacle, !plant)? && plant {
                vegetation_callbacks += 1;
            }
        }
        Ok(SceneContacts {
            contacts: geometry.finish(),
            vegetation_proxies,
            vegetation_callbacks,
            unsupported_vegetation_proxies: selected.unsupported_vegetation_proxies,
        })
    }
    ///001ea050's plant exclusion uses retained cache distances, configured
    ///proxy count and an inclusive twenty-foot horizontal circle, not geometry.
    pub fn vegetation_blocks_checkpoint(&self, query: &CachedQuery) -> Result<bool> {
        for candidate in &query.candidates {
            if let SceneNode::Vegetation(index) = candidate.node {
                let plant = &self.vegetation[index];
                if vegetation_collision::blocks_checkpoint(
                    query.center,
                    plant.coordinates,
                    self.coordinate_unit,
                    plant.proxy_count,
                    candidate.distance,
                )? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    ///Separate spatial iterator; does not update or trim the quad/recovery
    ///cache. Actor bounds/sphere overlap and physical proxy expansion belong
    ///to direct-query consumers, including camera. Cached wreck-rider uses its
    ///own inclusive/eligibility contract and must not substitute this path.
    pub fn direct_query(
        &self,
        bounds: Bounds,
        ignore_vegetation: bool,
        mut ordinary_eligible: impl FnMut(usize, &StaticObstacle) -> bool,
    ) -> Result<Vec<SceneNode>> {
        let Some(query) = self.partition.query(bounds)? else {
            return Ok(Vec::new());
        };
        Ok(query
            .indices
            .into_iter()
            .filter_map(|index| {
                let node = self.node(index);
                match node {
                    SceneNode::StaticObstacle(index)
                        if ordinary_eligible(index, &self.obstacles[index]) =>
                    {
                        Some(node)
                    }
                    SceneNode::Vegetation(_) if !ignore_vegetation => Some(node),
                    _ => None,
                }
            })
            .collect())
    }

    ///0031d190/0011f2c0 use segment bounds to traverse the scene partition,
    ///then require inclusive sphere overlap and current ordinary eligibility.
    ///The camera's query bitbf.0 disables physical vegetation expansion. This
    ///path does not reuse, refresh, sort or capacity-trim the vehicle cache.
    pub fn direct_camera_candidates(
        &self,
        ray: &crate::collision::CollisionRay,
        ordinary_eligible: impl FnMut(usize, &StaticObstacle) -> bool,
    ) -> Result<DirectCameraQuery<'_>> {
        if !ray
            .start
            .iter()
            .chain(ray.end.iter())
            .all(|v| v.is_finite())
        {
            return Err("nonfinite camera collision ray".into());
        }
        let low: Vec3 = std::array::from_fn(|i| ray.start[i].min(ray.end[i]));
        let high: Vec3 = std::array::from_fn(|i| ray.start[i].max(ray.end[i]));
        //00327360 prepares these bounds/sphere before the direct iterator.
        let bounds = Bounds {
            center: std::array::from_fn(|i| (high[i] + low[i]) * 0.5),
            half: std::array::from_fn(|i| (high[i] - low[i]) * 0.5),
        };
        let radius = dot(bounds.half, bounds.half).sqrt();
        let obstacles = self
            .direct_query(bounds, true, ordinary_eligible)?
            .into_iter()
            .filter_map(|node| match node {
                SceneNode::StaticObstacle(index) => {
                    let sphere = self.static_spheres[index];
                    (sphere_separation(bounds.center, radius, sphere.center, sphere.radius) == 0.)
                        .then_some(&self.obstacles[index])
                }
                SceneNode::Vegetation(_) => None,
            })
            .collect();
        Ok(DirectCameraQuery {
            bounds,
            radius,
            obstacles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision::{CollisionMesh, RayTreeNode},
        ecosystem::{Placement, Species},
    };
    use std::rc::Rc;
    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    fn config() -> Config {
        Config {
            min_x: 0.,
            max_x: 256.,
            min_z: 0.,
            max_z: 256.,
            cell_size: 32.,
        }
    }
    fn obstacle(center: Vec3, half: Vec3, matrix: [f32; 16]) -> StaticObstacle {
        StaticObstacle {
            mesh: Rc::new(CollisionMesh {
                transform: [0.; 12],
                vertices: Vec::new(),
                vertex_normal_bits: None,
                triangles: Vec::new(),
                broad_phase_nodes: 1,
                flags: 0,
                nodes: vec![RayTreeNode {
                    bound_measure: 0.,
                    center,
                    half_extents: half,
                    children: [0x8000; 2],
                }],
            }),
            world_matrix: matrix,
        }
    }
    fn ecosystem(placements: Vec<Placement>, proxies: &[usize]) -> Ecosystem {
        Ecosystem {
            species: proxies
                .iter()
                .enumerate()
                .map(|(slot, &count)| Species {
                    slot: slot as u8,
                    model: String::new(),
                    billboard: String::new(),
                    parameters: [0; 23],
                    optional_name: None,
                    proxies: vec![[0; 9]; count],
                })
                .collect(),
            placements,
            global_name: None,
            global_word: 0,
            placement_texture: None,
        }
    }
    fn plant(x: u16, z: u16, slot: u8) -> Placement {
        Placement {
            slot,
            resample_height: false,
            coordinates: [x * 256, 0, z * 256],
            height: 20.,
            radius: 2.,
        }
    }

    fn cylinder_plants() -> Ecosystem {
        let mut eco = ecosystem(vec![plant(128, 128, 0), plant(129, 128, 0)], &[2]);
        for placement in &mut eco.placements {
            placement.radius = 0.25;
        }
        eco.species[0].proxies = vec![
            [1, 0, 0, 0, 0, 0, 0, 1f32.to_bits(), 5f32.to_bits()],
            [1, 0, 0, 0, 0, 0, 0, 0, 0],
        ];
        eco
    }

    #[test]
    fn physical_expansion_keeps_mixed_order_zero_proxy_and_distinct_actor_gates() {
        let eco = cylinder_plants();
        let objects = vec![
            obstacle([128., 1., 128.], [1.; 3], IDENTITY),
            obstacle([129., 1., 128.], [1.; 3], IDENTITY),
        ];
        let scene = SceneCollision::new(config(), objects, Some(&eco), false, |_| Ok(0.)).unwrap();
        let query = CachedQuery {
            center: [128., 0., 128.],
            query_radius: 3.,
            cache_radius: 40.,
            refreshed: false,
            candidates: vec![
                RetainedCandidate {
                    node: SceneNode::StaticObstacle(0),
                    distance: 0.,
                },
                RetainedCandidate {
                    node: SceneNode::Vegetation(0),
                    distance: 0.,
                },
                RetainedCandidate {
                    node: SceneNode::StaticObstacle(1),
                    distance: 0.,
                },
                RetainedCandidate {
                    node: SceneNode::Vegetation(1),
                    distance: 0.75,
                },
            ],
        };
        let cached = scene
            .physical_selection(
                &query,
                PhysicalTrim::CachedRider,
                query.center,
                query.query_radius,
                false,
                |_, _| true,
            )
            .unwrap();
        let order: Vec<_> = cached
            .meshes
            .iter()
            .map(|m| (m.node, m.proxy_index))
            .collect();
        assert_eq!(
            order,
            vec![
                (SceneNode::StaticObstacle(0), None),
                (SceneNode::Vegetation(0), Some(0)),
                (SceneNode::Vegetation(0), Some(1)),
                (SceneNode::StaticObstacle(1), None),
                (SceneNode::Vegetation(1), Some(0)),
                (SceneNode::Vegetation(1), Some(1)),
            ]
        );
        assert_eq!(cached.unsupported_vegetation_proxies, 0);
        assert_eq!(cached.meshes[2].obstacle.mesh.triangles.len(), 32);
        assert!(
            cached.meshes[2]
                .obstacle
                .mesh
                .vertices
                .iter()
                .all(|p| *p == [0.; 3])
        );
        let wheel = scene
            .physical_selection(
                &query,
                PhysicalTrim::Supplied,
                [129., 0., 128.],
                0.5,
                false,
                |_, _| true,
            )
            .unwrap();
        assert_eq!(
            wheel
                .meshes
                .iter()
                .filter(|m| m.proxy_index.is_some())
                .map(|m| m.node)
                .collect::<Vec<_>>(),
            vec![SceneNode::Vegetation(1); 2]
        );
        let skip = scene
            .physical_selection(
                &query,
                PhysicalTrim::CachedRider,
                query.center,
                query.query_radius,
                true,
                |i, _| i == 1,
            )
            .unwrap();
        assert_eq!(skip.vegetation_candidates, 2);
        assert_eq!(skip.meshes.len(), 1);
        assert_eq!(skip.meshes[0].node, SceneNode::StaticObstacle(1));
    }

    #[test]
    fn cylinder_type2_contacts_keep_callbacks_across_proxy_order_and_query_bit() {
        use crate::collision::{CollisionRay, CollisionRayTree};
        let mut eco = cylinder_plants();
        eco.placements.truncate(1);
        //Two actual cylinders in one plant make two source callbacks; the
        //second re-appends the accumulated nearest row before the zero proxy.
        let duplicate = eco.species[0].proxies[0];
        eco.species[0].proxies.insert(1, duplicate);
        let mut scene =
            SceneCollision::new(config(), vec![], Some(&eco), false, |_| Ok(0.)).unwrap();
        let mut world = IDENTITY;
        world[12] = 128.;
        world[14] = 128.;
        let rays = CollisionRayTree {
            transform: IDENTITY,
            query_capacity: 1,
            rays: vec![CollisionRay {
                start: [2., 2., 0.2],
                end: [0., 2., 0.2],
            }],
            nodes: vec![RayTreeNode {
                center: [1., 2., 0.2],
                half_extents: [1., 0.1, 0.1],
                bound_measure: 4.04,
                children: [0x8000; 2],
            }],
        };
        let query = scene
            .cached_query([128., 2., 128.], 3., |_, _| true)
            .unwrap();
        let hit = scene.quad_contacts(&query, &rays, &world, false).unwrap();
        assert_eq!(hit.vegetation_proxies, 3);
        assert_eq!(hit.unsupported_vegetation_proxies, 0);
        assert_eq!(hit.vegetation_callbacks, 2);
        assert_eq!(hit.contacts.len(), 2);
        assert_eq!(hit.contacts[0].world_point, hit.contacts[1].world_point);
        assert!(hit.contacts[0].world_normal[0] > 0.9);
        assert!(hit.contacts[0].world_point[0] > 128.8 && hit.contacts[0].world_point[0] < 129.);
        let before = scene.cache().entries().to_vec();
        let skipped = scene.quad_contacts(&query, &rays, &world, true).unwrap();
        assert!(skipped.contacts.is_empty());
        assert_eq!(skipped.vegetation_proxies, 0);
        assert_eq!(scene.cache().entries(), before);
        //Cylinder has no top cap, even when the plant gate admits this ray.
        let vertical = CollisionRayTree {
            rays: vec![CollisionRay {
                start: [0.2, 6., 0.1],
                end: [0.2, 4., 0.1],
            }],
            nodes: vec![RayTreeNode {
                center: [0.2, 5., 0.1],
                half_extents: [0.1, 1., 0.1],
                bound_measure: 4.04,
                children: [0x8000; 2],
            }],
            ..rays
        };
        let cap = scene
            .quad_contacts(&query, &vertical, &world, false)
            .unwrap();
        assert!(cap.contacts.is_empty());
    }

    #[test]
    fn camera_direct_query_preserves_cache_and_uses_inclusive_sphere_without_capacity_trim() {
        let mut obstacles = (0..300)
            .map(|_| obstacle([128., 0., 128.], [2.; 3], IDENTITY))
            .collect::<Vec<_>>();
        obstacles.push(obstacle([134., 0., 128.], [1., 0., 0.], IDENTITY));
        obstacles.push(obstacle([135., 0., 128.], [1., 0., 0.], IDENTITY));
        let plants = ecosystem(vec![plant(128, 128, 0)], &[1]);
        let mut scene =
            SceneCollision::new(config(), obstacles, Some(&plants), false, |_| Ok(0.)).unwrap();
        scene
            .cached_query([128., 0., 128.], 3., |_, _| true)
            .unwrap();
        let previous = scene.cache().entries().to_vec();
        assert_eq!(previous.len(), 256);
        let query = scene
            .direct_camera_candidates(
                &crate::collision::CollisionRay {
                    start: [123., 0., 128.],
                    end: [133., 0., 128.],
                },
                |index, _| index != 1,
            )
            .unwrap();
        assert_eq!(query.bounds.center, [128., 0., 128.]);
        assert_eq!(query.radius, 5.);
        assert_eq!(query.obstacles.len(), 300);
        assert!(
            query
                .obstacles
                .iter()
                .any(|o| std::ptr::eq(*o, &scene.obstacles()[300]))
        );
        assert!(
            !query
                .obstacles
                .iter()
                .any(|o| std::ptr::eq(*o, &scene.obstacles()[301]))
        );
        assert!(
            !query
                .obstacles
                .iter()
                .any(|o| std::ptr::eq(*o, &scene.obstacles()[1]))
        );
        assert_eq!(scene.cache().entries(), previous);
        assert!(
            scene
                .direct_camera_candidates(
                    &crate::collision::CollisionRay {
                        start: [f32::NAN, 0., 0.],
                        end: [0.; 3],
                    },
                    |_, _| true,
                )
                .is_err()
        );
    }
    #[test]
    fn shared_quad_recovery_quad_query_preserves_membership_and_own_trim() {
        let eco = ecosystem(vec![plant(100, 100, 0)], &[1]);
        let mut scene = SceneCollision::new(
            config(),
            vec![
                obstacle([110., 0., 100.], [1.; 3], IDENTITY),
                obstacle([121., 0., 100.], [1.; 3], IDENTITY),
            ],
            Some(&eco),
            false,
            |_| panic!("untagged plants must not sample"),
        )
        .unwrap();
        let quad = scene
            .cached_query([100., 0., 100.], 3., |_, _| true)
            .unwrap();
        assert!(quad.refreshed);
        assert_eq!(quad.candidates.len(), 3);
        assert!(scene.static_selection(&quad).is_empty());
        let recovery = scene
            .cached_query([101., 0., 100.], 20., |_, _| {
                panic!("reuse must not reevaluate eligibility")
            })
            .unwrap();
        assert!(!recovery.refreshed);
        assert_eq!(scene.static_selection(&recovery).len(), 2);
        assert!(scene.vegetation_blocks_checkpoint(&recovery).unwrap());
        let quad_again = scene
            .cached_query([102., 0., 100.], 3., |_, _| false)
            .unwrap();
        assert!(!quad_again.refreshed);
        assert_eq!(
            quad.candidates.iter().map(|c| c.node).collect::<Vec<_>>(),
            quad_again
                .candidates
                .iter()
                .map(|c| c.node)
                .collect::<Vec<_>>()
        );
        assert!(scene.static_selection(&quad_again).is_empty());
        assert_ne!(
            quad.candidates[0].distance,
            quad_again.candidates[0].distance
        );
        let equality = scene
            .cached_query([100. + quad.cache_radius - 3., 0., 100.], 3., |_, _| false)
            .unwrap();
        assert!(equality.refreshed);
    }
    #[test]
    fn source_order_resampling_registers_proxyless_plants_with_quantized_bounds() {
        let mut placements = vec![plant(80, 80, 0), plant(90, 90, 1), plant(100, 100, 0)];
        placements[0].resample_height = true;
        placements[2].resample_height = true;
        placements[1].coordinates[1] = 777;
        let eco = ecosystem(placements, &[0, 2]);
        let mut calls = Vec::new();
        let scene = SceneCollision::new(
            config(),
            vec![obstacle([80., 0., 80.], [1.; 3], IDENTITY)],
            Some(&eco),
            false,
            |p| {
                calls.push(p);
                Ok(if p[0] == 80. { -2. } else { 1.125 })
            },
        )
        .unwrap();
        assert_eq!(calls, [[80., 0., 80.], [100., 0., 100.]]);
        assert_eq!(scene.vegetation()[0].coordinates, [20480, 65024, 20480]);
        assert_eq!(scene.vegetation()[0].proxy_count, 0);
        assert_eq!(scene.vegetation()[0].bounds.center, [80., 264., 80.]);
        assert_eq!(scene.vegetation()[1].coordinates[1], 777);
        assert_eq!(scene.vegetation()[2].coordinates[1], 288);
        let nodes = scene
            .direct_query(
                Bounds {
                    center: [90., -1000., 90.],
                    half: [120.; 3],
                },
                false,
                |_, _| true,
            )
            .unwrap();
        assert!(nodes.contains(&SceneNode::Vegetation(0)));
        assert!(nodes.contains(&SceneNode::Vegetation(1)));
        assert!(nodes.contains(&SceneNode::Vegetation(2)));
        assert_eq!(scene.coordinate_unit(), 256. / 65536.);
        assert_eq!(scene.coordinate_reciprocal(), 65536. / 256.);
    }
    #[test]
    fn force_resampling_visits_every_placement_in_source_order() {
        let eco = ecosystem(vec![plant(10, 10, 0), plant(20, 20, 0)], &[0]);
        let mut visited = Vec::new();
        let scene = SceneCollision::new(config(), vec![], Some(&eco), true, |point| {
            visited.push(point[0]);
            Ok(point[0])
        })
        .unwrap();
        assert_eq!(visited, [10., 20.]);
        assert_eq!(scene.vegetation()[0].coordinates[1], 2560);
        assert_eq!(scene.vegetation()[1].coordinates[1], 5120);
    }
    #[test]
    fn ordinary_distance_keeps_root_diagonal_after_scale_and_plant_ignores_altitude() {
        let mut scaled = IDENTITY;
        scaled[0] = 10.;
        scaled[12] = 100.;
        scaled[14] = 100.;
        let eco = ecosystem(vec![plant(100, 100, 0)], &[0]);
        let mut scene = SceneCollision::new(
            config(),
            vec![obstacle([0.; 3], [2., 3., 4.], scaled)],
            Some(&eco),
            false,
            |_| Ok(0.),
        )
        .unwrap();
        let query = scene
            .cached_query([110., 1000., 100.], 3., |_, _| true)
            .unwrap();
        //Altitude excludes the ordinary sphere from cache, but not the plant.
        assert_eq!(query.candidates.len(), 1);
        assert_eq!(query.candidates[0].node, SceneNode::Vegetation(0));
        assert_eq!(query.candidates[0].distance, 8.);
        let query = scene
            .cached_query([110., 0., 100.], 3., |_, _| true)
            .unwrap();
        let ordinary = query
            .candidates
            .iter()
            .find(|c| matches!(c.node, SceneNode::StaticObstacle(_)))
            .unwrap();
        assert_eq!(ordinary.distance, 10. - 29f32.sqrt());
        assert!(scene.static_selection(&query).is_empty());
    }
    #[test]
    fn cache_ignore_changes_only_on_refresh_and_direct_query_is_independent() {
        let eco = ecosystem(vec![plant(100, 100, 0)], &[1]);
        let mut scene =
            SceneCollision::new(config(), vec![], Some(&eco), false, |_| Ok(0.)).unwrap();
        let first = scene
            .cached_query([100., 0., 100.], 3., |_, _| true)
            .unwrap();
        assert_eq!(first.candidates.len(), 1);
        scene.set_ignore_vegetation(true);
        let reused = scene
            .cached_query([101., 0., 100.], 20., |_, _| true)
            .unwrap();
        assert!(!reused.refreshed);
        assert_eq!(reused.candidates.len(), 1);
        let before = scene.cache().entries().to_vec();
        let direct = scene
            .direct_query(
                Bounds {
                    center: [100., 0., 100.],
                    half: [5.; 3],
                },
                true,
                |_, _| true,
            )
            .unwrap();
        assert!(direct.is_empty());
        assert_eq!(scene.cache().entries(), before);
        let refreshed = scene
            .cached_query([130., 0., 100.], 20., |_, _| true)
            .unwrap();
        assert!(refreshed.refreshed);
        assert!(refreshed.candidates.is_empty());
        scene.set_ignore_vegetation(false);
        let reused = scene
            .cached_query([131., 0., 100.], 3., |_, _| true)
            .unwrap();
        assert!(!reused.refreshed);
        assert!(reused.candidates.is_empty());
    }
    #[test]
    fn vegetation_and_ordinary_candidates_share_capacity() {
        let eco = ecosystem((0..260).map(|_| plant(100, 100, 0)).collect(), &[0]);
        let mut scene = SceneCollision::new(
            config(),
            vec![obstacle([100., 0., 100.], [1.; 3], IDENTITY)],
            Some(&eco),
            false,
            |_| Ok(0.),
        )
        .unwrap();
        let query = scene
            .cached_query([100., 0., 100.], 3., |_, _| true)
            .unwrap();
        assert_eq!(query.candidates.len(), 256);
        assert_eq!(query.candidates[0].node, SceneNode::StaticObstacle(0));
        assert_eq!(query.candidates[255].node, SceneNode::Vegetation(254));
        assert_eq!(scene.static_selection(&query).len(), 1);
        assert!(!scene.vegetation_blocks_checkpoint(&query).unwrap());
    }

    #[test]
    fn original_mixed_checkpoint_interleaves_ordinary_and_vegetation_nodes() {
        //Nearby mixed original-instruction fixture, first source frame. Omitted
        //far nodes are outside the collector radius; both retained classes must
        //follow one tree iterator rather than concatenated separate queries.
        let config = Config {
            min_x: 0.,
            min_z: 0.,
            cell_size: 64.,
            max_x: f32::from_bits(0x45b7307a),
            max_z: f32::from_bits(0x4545fb2d),
        };
        let centers = [
            [0x45398fd7, 0x41200000, 0x44c9b625],
            [0x45344dc3, 0x41200000, 0x44c30267],
            [0x4539d0b5, 0x41200000, 0x44bfa675],
            [0x4536a365, 0x41200000, 0x44bec0ae],
            [0x45386c8d, 0x41200000, 0x44cd5bda],
            [0x45399052, 0x41200000, 0x44c05be1],
            [0x453633a5, 0x41200000, 0x44beb8b4],
            [0x45361ed9, 0x41200000, 0x44c9a456],
        ];
        let obstacles = centers
            .into_iter()
            .map(|center| obstacle(center.map(f32::from_bits), [1., 5., 1.], IDENTITY))
            .collect();
        let eco = ecosystem(
            vec![
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32969, 111, 17980],
                    height: 20.,
                    radius: f32::from_bits(0x40040000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [33053, 111, 17859],
                    height: 20.,
                    radius: f32::from_bits(0x3fb00000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32720, 111, 17416],
                    height: 20.,
                    radius: f32::from_bits(0x40198000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32581, 111, 17385],
                    height: 20.,
                    radius: f32::from_bits(0x3fc20000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32476, 111, 17426],
                    height: 20.,
                    radius: f32::from_bits(0x3f920000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32400, 111, 17762],
                    height: 20.,
                    radius: f32::from_bits(0x3fb60000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [33032, 111, 18085],
                    height: 20.,
                    radius: f32::from_bits(0x402c8000),
                },
                Placement {
                    slot: 0,
                    resample_height: false,
                    coordinates: [32399, 111, 17653],
                    height: 20.,
                    radius: f32::from_bits(0x40028000),
                },
            ],
            &[0],
        );
        let mut scene =
            SceneCollision::new(config, obstacles, Some(&eco), false, |_| Ok(0.)).unwrap();
        let query = scene
            .cached_query(
                [0x4537307a, 0x00000000, 0x44c5fb2d].map(f32::from_bits),
                3.,
                |_, _| true,
            )
            .unwrap();
        use SceneNode::{StaticObstacle as S, Vegetation as V};
        assert_eq!(
            query
                .candidates
                .iter()
                .map(|candidate| candidate.node)
                .collect::<Vec<_>>(),
            [S(0), V(0), V(6), V(1), S(7), V(2), V(4), V(5), V(7), V(3)]
        );
        assert_eq!(query.cache_radius.to_bits(), 0x42367940);
    }
    #[test]
    fn terrain_resampling_preserves_input_y_when_native_cell_is_missing() {
        let terrain = Terrain {
            spacing: 10.,
            tiles: vec![crate::terrain::TerrainTile {
                tag: 0,
                heights: [4; 289],
                attributes: [u32::from_le_bytes([0, 127, 0, 0]); 289],
            }],
            leaves: vec![crate::terrain::TerrainLeaf {
                x: 0,
                z: 0,
                tile: 0,
                height_morph: 1.,
                height_scale: 1.,
                minimum_height: 40.,
                maximum_height: 40.,
            }],
            hierarchy_nodes: 1,
            width_cells: 16,
            depth_cells: 16,
        };
        let mut placements = vec![plant(100, 100, 0), plant(200, 200, 0)];
        for placement in &mut placements {
            placement.resample_height = true;
            placement.coordinates[1] = 777;
        }
        let eco = ecosystem(placements, &[0]);
        let scene =
            SceneCollision::from_terrain(config(), vec![], Some(&eco), &terrain, false).unwrap();
        assert_eq!(scene.vegetation()[0].coordinates[1], 10240);
        assert_eq!(scene.vegetation()[0].bounds.center[1], 50.);
        assert_eq!(scene.vegetation()[1].coordinates[1], 777);
    }
    #[test]
    fn recovery_plant_circle_is_inclusive_but_cache_separation_is_strict() {
        let eco = ecosystem(vec![plant(100, 100, 0)], &[1]);
        let mut scene =
            SceneCollision::new(config(), vec![], Some(&eco), false, |_| Ok(0.)).unwrap();
        let edge = scene
            .cached_query([120., 1000., 100.], 20., |_, _| true)
            .unwrap();
        assert_eq!(edge.candidates[0].distance, 18.);
        assert!(scene.vegetation_blocks_checkpoint(&edge).unwrap());
        let width_edge = scene
            .cached_query([122., 1000., 100.], 20., |_, _| true)
            .unwrap();
        assert_eq!(width_edge.candidates[0].distance, 20.);
        assert!(!scene.vegetation_blocks_checkpoint(&width_edge).unwrap());
        assert_eq!(width_edge.strictly_within().count(), 0);
    }
}
