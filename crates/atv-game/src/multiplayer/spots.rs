//! **Port addition, not recovered behavior** (issue #710): Free Ride
//! arrivals start all over the venue instead of at its one start point
//! (research/FREERIDE_HUB.md "Spawn spots").
//!
//! Candidates come from the venue's long Enduro racing line
//! (`<venue>endle.tdf` in `E0n_ENDL.RES`): the original's AI path through
//! that course, authored ground that riders ride across the whole map, with
//! a race direction to face. A Free Ride load already reads that archive for
//! the venue's terrain materials, so the spots cost no extra download. The
//! line is sampled every [`SPACING`] feet (and [`OFFSET`] feet to each side
//! where the line itself fails), and each candidate must pass [`check`].
//! Nothing is committed: the list is derived from the content at load time.
use crate::Result;
use crate::ride::Ride;
use atv_core::math::Det;
use atv_core::obstacle::StaticObstacle;
use atv_core::scene_partition::Bounds;

/// Feet between candidates along the racing line.
pub const SPACING: f32 = 80.;
/// A candidate that fails on the line is tried this far to each side (ft).
pub const OFFSET: f32 = 9.;
/// Kept spots lie at least this far apart (ft).
pub const MIN_GAP: f32 = 60.;
/// The rideable area keeps this far inside the boundary launch margin (ft).
pub const INSIDE: f32 = 60.;
/// Footprint radius tested for ground, slope and water (ft): an ATV is
/// 4 × 6.4 ft (`atv_core::peer_contact`).
pub const FOOTPRINT: f32 = 6.;
/// Steepest footprint slope (rise over run, about 17°): a parked ATV holds.
pub const SLOPE: f32 = 0.3;
/// Radius of the cliff test (ft) and the steepest rise or drop over it
/// (about 31°): no ledge or wall beside the spot.
pub const CLIFF_RADIUS: f32 = 20.;
pub const CLIFF: f32 = 0.6;
/// Props and trees keep clear of this radius around the spot (ft).
pub const CLEAR: f32 = 7.;
/// Nothing solid within this height over the spot (ft): not under a
/// bridge, roof or canopy.
pub const SKY: f32 = 60.;
/// Eye height of the view rays (ft) and their length.
pub const EYE: f32 = 4.;
pub const VIEW: f32 = 50.;
/// Of the eight view directions, at least this many must be open.
pub const OPEN_VIEWS: usize = 5;
/// The ground rising faster than this along a view ray blocks it.
pub const VIEW_RISE: f32 = 0.5;
/// An arrival keeps this far from other riders (ft).
pub const RIDER_CLEAR: f32 = 40.;
/// A Reset ATV spot lies at least this far from where the rider is (ft).
pub const RESET_MOVE: f32 = 15.;
/// Spawn height over the ground, as [`super::spawn::beside`] places.
pub const LIFT: f32 = 2.5;

/// A validated spawn: the ATV origin and the heading it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub origin: [f32; 3],
    pub yaw: f32,
}

/// A candidate on the ground plane: where, and the course direction there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub at: [f32; 2],
    /// The line's heading there (unit, x and z).
    pub heading: [f32; 2],
}

/// Why a candidate failed (counted for diagnostics and the content test).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reject {
    Outside,
    OffTerrain,
    OnProp,
    Water,
    Steep,
    Cliff,
    Blocked,
    Covered,
    BoxedIn,
    Crowded,
}

/// What validation reads of a venue. [`RideWorld`] answers from a loaded
/// ride; the unit tests build small worlds of their own.
pub trait World {
    /// The rideable area's corners on the ground plane.
    fn area(&self) -> ([f32; 2], [f32; 2]);
    /// Contact-plane ground height; None off the terrain.
    fn ground(&self, x: f32, z: f32) -> Option<f32>;
    /// Still water's depth over the ground (negative where the ground is
    /// higher); None where there is none.
    fn water_depth(&self, x: f32, z: f32) -> Option<f32>;
    /// Narrows the prop queries below to those within `radius` of `at`.
    fn focus(&mut self, _at: [f32; 2], _radius: f32) {}
    /// The highest prop surface under `point` within `depth` feet.
    fn prop_top(&self, point: [f32; 3], depth: f32) -> Option<f32>;
    /// Whether the segment `a`-`b` touches a prop or tree.
    fn prop_between(&self, a: [f32; 3], b: [f32; 3]) -> bool;
    /// Whether a compact prop or a tree reaches into the upright cylinder
    /// of `radius` round `at` between heights `low` and `high`.
    fn prop_near(&self, at: [f32; 2], radius: f32, low: f32, high: f32) -> bool;
}

fn flat(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot_d(a[1] - b[1])
}

fn yaw_of(h: [f32; 2]) -> f32 {
    h[0].atan2_d(h[1])
}

/// The eight compass directions, `first` first, then alternating sides.
fn directions(first: [f32; 2]) -> [[f32; 2]; 8] {
    let base = yaw_of(first);
    let order = [0., 1., -1., 2., -2., 3., -3., 4.];
    order.map(|k: f32| {
        let (s, c) = (base + k * std::f32::consts::FRAC_PI_4).sin_cos_d();
        [s, c]
    })
}

/// Candidates every `spacing` feet along `line` (x, y, z points in riding
/// order), each with the line's direction there.
pub fn line_candidates(line: &[[f32; 3]], spacing: f32) -> Vec<Candidate> {
    let mut out = Vec::new();
    if !(spacing > 0.) {
        return out;
    }
    // The first candidate sits half a spacing in, off the start grid.
    let mut next = spacing * 0.5;
    let mut walked = 0.;
    for pair in line.windows(2) {
        let (a, b) = ([pair[0][0], pair[0][2]], [pair[1][0], pair[1][2]]);
        let length = flat(a, b);
        if !(length > 1e-3) {
            continue;
        }
        let heading = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
        while next <= walked + length {
            let f = (next - walked) / length;
            out.push(Candidate {
                at: [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f],
                heading,
            });
            next += spacing;
        }
        walked += length;
    }
    out
}

/// Whether `p` lies inside the rideable area.
fn inside(area: ([f32; 2], [f32; 2]), p: [f32; 2]) -> bool {
    p[0] >= area.0[0] && p[0] <= area.1[0] && p[1] >= area.0[1] && p[1] <= area.1[1]
}

/// Validates one spot at `at` facing along `heading` (or against it, or
/// the open direction nearest it). See the constants for each rule.
pub fn check(
    world: &mut dyn World,
    at: [f32; 2],
    heading: [f32; 2],
) -> std::result::Result<Spot, Reject> {
    let area = world.area();
    // In bounds, with the whole cliff ring inside the area too.
    if !inside(area, at) {
        return Err(Reject::Outside);
    }
    let ground = world.ground(at[0], at[1]).ok_or(Reject::OffTerrain)?;
    world.focus(at, VIEW + CLEAR);
    let world: &dyn World = world;
    // On the ground: no deck or roof underneath (the column from the eye
    // down past the ground).
    if world
        .prop_top([at[0], ground + EYE + 2., at[1]], EYE + 4.)
        .is_some_and(|top| top > ground + 0.75)
    {
        return Err(Reject::OnProp);
    }
    let ring =
        |radius: f32| directions([0., 1.]).map(|d| [at[0] + d[0] * radius, at[1] + d[1] * radius]);
    // Dry ground under the footprint and a little beyond it.
    let dry =
        |world: &dyn World, p: [f32; 2]| world.water_depth(p[0], p[1]).is_none_or(|d| d < -0.25);
    if !dry(world, at) || ring(FOOTPRINT + 2.).iter().any(|p| !dry(world, *p)) {
        return Err(Reject::Water);
    }
    // A gentle footprint.
    for p in ring(FOOTPRINT) {
        let h = world.ground(p[0], p[1]).ok_or(Reject::OffTerrain)?;
        if (h - ground).abs() > SLOPE * FOOTPRINT {
            return Err(Reject::Steep);
        }
    }
    // No ledge, drop or wall close by.
    for p in ring(CLIFF_RADIUS) {
        if !inside(area, p) {
            return Err(Reject::Outside);
        }
        let h = world.ground(p[0], p[1]).ok_or(Reject::OffTerrain)?;
        if (h - ground).abs() > CLIFF * CLIFF_RADIUS {
            return Err(Reject::Cliff);
        }
    }
    // Clear of props and trees: their bounds (compact ones) and rays at
    // two heights out to the clearance radius (large meshes).
    if world.prop_near(at, CLEAR, ground + 0.75, ground + 7.) {
        return Err(Reject::Blocked);
    }
    for height in [1.5, EYE] {
        let from = [at[0], ground + height, at[1]];
        for k in 0..16 {
            let (s, c) = (k as f32 * std::f32::consts::PI / 8.).sin_cos_d();
            let to = [at[0] + s * CLEAR, ground + height, at[1] + c * CLEAR];
            if world.prop_between(from, to) {
                return Err(Reject::Blocked);
            }
        }
    }
    // Open sky overhead.
    if world.prop_between([at[0], ground + 1.5, at[1]], [at[0], ground + SKY, at[1]]) {
        return Err(Reject::Covered);
    }
    // A view out: most directions open, facing one of them.
    let eye = ground + EYE;
    let open = |world: &dyn World, d: [f32; 2]| {
        let to = [at[0] + d[0] * VIEW, eye, at[1] + d[1] * VIEW];
        if world.prop_between([at[0], eye, at[1]], to) {
            return false;
        }
        (1..=10).all(|i| {
            let r = VIEW * i as f32 / 10.;
            world
                .ground(at[0] + d[0] * r, at[1] + d[1] * r)
                .is_some_and(|h| h <= eye + VIEW_RISE * r)
        })
    };
    let views = directions(heading);
    let opens = views.map(|d| open(world, d));
    if opens.iter().filter(|o| **o).count() < OPEN_VIEWS {
        return Err(Reject::BoxedIn);
    }
    // The course direction when open, else its reverse, else the open
    // direction nearest it.
    let facing = if opens[0] {
        heading
    } else if opens[7] {
        views[7]
    } else {
        views[opens.iter().position(|o| *o).unwrap_or(0)]
    };
    Ok(Spot {
        origin: [at[0], ground + LIFT, at[1]],
        yaw: yaw_of(facing),
    })
}

/// A venue's validated Free Ride spots, kept for the session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spots {
    pub spots: Vec<Spot>,
    /// Candidates tried, and each failure reason's count.
    pub tried: usize,
    pub rejected: Vec<(Reject, usize)>,
}

impl Spots {
    /// Validates `candidates` (each on the line, then [`OFFSET`] to either
    /// side), keeping spots at least [`MIN_GAP`] apart.
    pub fn validate(world: &mut dyn World, candidates: &[Candidate]) -> Self {
        let mut out = Spots::default();
        let mut reasons = std::collections::BTreeMap::new();
        for c in candidates {
            out.tried += 1;
            let side = [c.heading[1], -c.heading[0]];
            let mut first = None;
            let mut found = None;
            for offset in [0., OFFSET, -OFFSET] {
                let at = [c.at[0] + side[0] * offset, c.at[1] + side[1] * offset];
                match check(world, at, c.heading) {
                    Ok(spot) => {
                        found = Some(spot);
                        break;
                    }
                    Err(reason) => {
                        first.get_or_insert(reason);
                    }
                }
            }
            let reason = match found {
                Some(spot) if out.spots.iter().all(|s| gap(s, &spot) >= MIN_GAP) => {
                    out.spots.push(spot);
                    continue;
                }
                Some(_) => Reject::Crowded,
                None => first.unwrap_or(Reject::OffTerrain),
            };
            *reasons.entry(reason).or_insert(0) += 1;
        }
        out.rejected = reasons.into_iter().collect();
        out
    }

    pub fn is_empty(&self) -> bool {
        self.spots.is_empty()
    }

    /// An arrival's spot: from `start` (an index; a random number for a
    /// fresh arrival) on, the first clear of every rider in `riders` by
    /// [`RIDER_CLEAR`], else the one farthest from them.
    pub fn pick(&self, start: usize, riders: &[[f32; 3]]) -> Option<(Spot, usize)> {
        let n = self.spots.len();
        if n == 0 {
            return None;
        }
        let order = (0..n).map(|i| ((start + i) % n, self.spots[(start + i) % n]));
        let clearance = |s: &Spot| {
            riders
                .iter()
                .map(|r| flat([s.origin[0], s.origin[2]], [r[0], r[2]]))
                .fold(f32::MAX, f32::min)
        };
        order
            .clone()
            .find(|(_, s)| clearance(s) >= RIDER_CLEAR)
            .or_else(|| order.max_by(|a, b| clearance(&a.1).total_cmp(&clearance(&b.1))))
            .map(|(i, s)| (s, i))
    }

    /// Where slot `slot` of a room that moved together starts: distinct
    /// spots for up to 20 riders (or as many as there are spots), spread
    /// over the list, the same on every client for the same `seed`.
    pub fn slot_start(&self, slot: u8, seed: u32) -> usize {
        let n = self.spots.len().max(1);
        let step = (n / 20).max(1);
        (seed as usize % n + usize::from(slot) * step) % n
    }

    /// Reset ATV in Free Ride: the nearest spot at least [`RESET_MOVE`] from
    /// `from` and clear of `riders`, else the nearest such spot at all.
    pub fn nearest(&self, from: [f32; 3], riders: &[[f32; 3]]) -> Option<Spot> {
        let here = [from[0], from[2]];
        let distance = |s: &Spot| flat(here, [s.origin[0], s.origin[2]]);
        let away = || self.spots.iter().filter(|s| distance(s) >= RESET_MOVE);
        let clear = |s: &&Spot| {
            riders
                .iter()
                .all(|r| flat([s.origin[0], s.origin[2]], [r[0], r[2]]) >= RIDER_CLEAR)
        };
        away()
            .filter(clear)
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            .or_else(|| away().min_by(|a, b| distance(a).total_cmp(&distance(b))))
            .copied()
    }

    /// How many of a `cells` × `cells` grid over `area` hold a spot (the
    /// spread metric of the content test).
    pub fn cells_covered(&self, area: ([f32; 2], [f32; 2]), cells: usize) -> usize {
        let mut used = vec![false; cells * cells];
        for s in &self.spots {
            let f = |i: usize, v: f32| {
                ((v - area.0[i]) / (area.1[i] - area.0[i]) * cells as f32) as usize
            };
            let (i, j) = (
                f(0, s.origin[0]).min(cells - 1),
                f(1, s.origin[2]).min(cells - 1),
            );
            used[j * cells + i] = true;
        }
        used.into_iter().filter(|u| *u).count()
    }
}

fn gap(a: &Spot, b: &Spot) -> f32 {
    flat([a.origin[0], a.origin[2]], [b.origin[0], b.origin[2]])
}

/// The rideable area of `ride`'s venue: the terrain inside the boundary
/// launch margin (`0020aec0`, `atv_core::tag::boundary_margin`), less
/// `inside` feet. Roaming CPUs keep to the same area (40 ft inside).
pub fn rideable_area(ride: &Ride, crowd_present: bool, inside: f32) -> ([f32; 2], [f32; 2]) {
    let terrain = ride.terrain();
    let bounds = atv_core::tag::Bounds::from_cells(
        terrain.width_cells,
        terrain.depth_cells,
        terrain.spacing,
    );
    let margin = atv_core::tag::boundary_margin(crowd_present);
    (
        [
            margin * bounds.cell[0] + inside,
            margin * bounds.cell[1] + inside,
        ],
        [
            bounds.cell[0] * (bounds.tiles[0] * 256. - margin) - inside,
            bounds.cell[1] * (bounds.tiles[1] * 256. - margin) - inside,
        ],
    )
}

/// [`World`] over a loaded ride: its contact terrain, still water, the
/// scene's collision props (static and the train's cars where they stand)
/// and registered vegetation.
pub struct RideWorld<'a> {
    ride: &'a Ride,
    area: ([f32; 2], [f32; 2]),
    props: Vec<(Bounds, &'a StaticObstacle)>,
    trees: Vec<Bounds>,
    near: Vec<usize>,
    near_trees: Vec<usize>,
}

/// Props whose footprint is no larger than this half extent (ft) are tested
/// by their bounds as well as by rays.
const COMPACT: f32 = 25.;

impl<'a> RideWorld<'a> {
    pub fn new(ride: &'a Ride, crowd_present: bool) -> Result<Self> {
        let mut props = Vec::new();
        let mut trees = Vec::new();
        if let Some(scene) = ride.collision_scene() {
            for obstacle in scene
                .obstacles()
                .iter()
                .chain(scene.dynamic().iter().map(|d| &d.obstacle))
            {
                props.push((obstacle.world_bounds()?, obstacle));
            }
            trees.extend(scene.vegetation().iter().map(|v| v.bounds));
        } else {
            for obstacle in ride.static_obstacles() {
                props.push((obstacle.world_bounds()?, obstacle));
            }
        }
        Ok(Self {
            ride,
            area: rideable_area(ride, crowd_present, INSIDE),
            props,
            trees,
            near: Vec::new(),
            near_trees: Vec::new(),
        })
    }
}

/// Whether the segment `a`-`b`, widened by `pad`, meets the box `b0`.
fn segment_meets(bounds: &Bounds, a: [f32; 3], b: [f32; 3], pad: f32) -> bool {
    let (mut t0, mut t1) = (0f32, 1f32);
    for i in 0..3 {
        let (lo, hi) = (
            bounds.center[i] - bounds.half[i] - pad,
            bounds.center[i] + bounds.half[i] + pad,
        );
        let d = b[i] - a[i];
        if d.abs() < 1e-6 {
            if a[i] < lo || a[i] > hi {
                return false;
            }
            continue;
        }
        let (mut u, mut v) = ((lo - a[i]) / d, (hi - a[i]) / d);
        if u > v {
            std::mem::swap(&mut u, &mut v);
        }
        t0 = t0.max(u);
        t1 = t1.min(v);
        if t0 > t1 {
            return false;
        }
    }
    true
}

impl World for RideWorld<'_> {
    fn area(&self) -> ([f32; 2], [f32; 2]) {
        self.area
    }

    fn ground(&self, x: f32, z: f32) -> Option<f32> {
        self.ride.ground(x, z)
    }

    fn water_depth(&self, x: f32, z: f32) -> Option<f32> {
        self.ride.water_depth(x, z)
    }

    fn focus(&mut self, at: [f32; 2], radius: f32) {
        let close = |b: &Bounds| {
            (at[0] - b.center[0]).abs() <= b.half[0] + radius
                && (at[1] - b.center[2]).abs() <= b.half[2] + radius
        };
        self.near.clear();
        self.near.extend(
            self.props
                .iter()
                .enumerate()
                .filter(|(_, (b, _))| close(b))
                .map(|(i, _)| i),
        );
        self.near_trees.clear();
        self.near_trees.extend(
            self.trees
                .iter()
                .enumerate()
                .filter(|(_, b)| close(b))
                .map(|(i, _)| i),
        );
    }

    fn prop_top(&self, point: [f32; 3], depth: f32) -> Option<f32> {
        let below = [point[0], point[1] - depth, point[2]];
        self.near
            .iter()
            .filter_map(|i| {
                let (bounds, obstacle) = &self.props[*i];
                if !segment_meets(bounds, point, below, 0.5) {
                    return None;
                }
                super::spawn::surface(&obstacle.mesh, &obstacle.world_matrix, point, depth)
                    .ok()
                    .flatten()
            })
            .max_by(f32::total_cmp)
    }

    fn prop_between(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        self.near_trees
            .iter()
            .any(|i| segment_meets(&self.trees[*i], a, b, 0.))
            || self.near.iter().any(|i| {
                let (bounds, obstacle) = &self.props[*i];
                segment_meets(bounds, a, b, 0.5)
                    && super::spawn::ray_hits(&obstacle.mesh, &obstacle.world_matrix, a, b)
                        .is_ok_and(|hit| hit.is_some())
            })
    }

    fn prop_near(&self, at: [f32; 2], radius: f32, low: f32, high: f32) -> bool {
        let meets = |b: &Bounds| {
            (at[0] - b.center[0]).abs() < b.half[0] + radius
                && (at[1] - b.center[2]).abs() < b.half[2] + radius
                && b.center[1] + b.half[1] > low
                && b.center[1] - b.half[1] < high
        };
        self.near_trees.iter().any(|i| meets(&self.trees[*i]))
            || self.near.iter().any(|i| {
                let b = &self.props[*i].0;
                b.half[0] <= COMPACT && b.half[2] <= COMPACT && meets(b)
            })
    }
}

/// The venue's long Enduro racing line (`<venue>endle.tdf` in its
/// `E0n_ENDL.RES`), x, y, z in riding order; None where it has none.
pub async fn enduro_line(
    content: &atv_content::Content,
    environment: &str,
) -> Option<Vec<[f32; 3]>> {
    let path = crate::launch::course_resource(environment, false)
        .ok()?
        .placements?;
    if !content.has(&path) {
        return None;
    }
    let archive = crate::archive::Archive::open(content, &path).await.ok()?;
    let index = archive.named(&format!("{environment}endle.tdf"))?;
    let course = atv_core::race::Course::parse(&archive.member(index).await.ok()?).ok()?;
    (course.points.len() > 1).then_some(course.points)
}

/// A venue's Free Ride spots for `ride` (loaded on it): the long Enduro
/// line's candidates, validated. Empty where the venue has no such line.
pub async fn free_ride_spots(
    content: &atv_content::Content,
    environment: &str,
    ride: &Ride,
    crowd_present: bool,
) -> Result<Spots> {
    let Some(line) = enduro_line(content, environment).await else {
        return Ok(Spots::default());
    };
    let mut world = RideWorld::new(ride, crowd_present)?;
    Ok(Spots::validate(
        &mut world,
        &line_candidates(&line, SPACING),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2000 ft square of flat ground at height 10 with a hill, a lake, a
    /// wall, a roof, a post and a cliff where the tests put them.
    struct Test {
        lake: Option<([f32; 2], f32)>,
        /// A ramp rising `rise` per foot east of x = 500.
        rise: f32,
        /// A step up of `step` feet east of x = 1500.
        step: f32,
        /// Axis boxes: solid props.
        boxes: Vec<Bounds>,
        /// Compact props tested by their bounds.
        posts: Vec<Bounds>,
    }

    impl Default for Test {
        fn default() -> Self {
            Self {
                lake: None,
                rise: 0.,
                step: 0.,
                boxes: vec![],
                posts: vec![],
            }
        }
    }

    impl World for Test {
        fn area(&self) -> ([f32; 2], [f32; 2]) {
            ([100., 100.], [1900., 1900.])
        }
        fn ground(&self, x: f32, z: f32) -> Option<f32> {
            if !(0. ..=2000.).contains(&x) || !(0. ..=2000.).contains(&z) {
                return None;
            }
            let ramp = if x > 500. && x < 1000. {
                (x - 500.) * self.rise
            } else {
                0.
            };
            let step = if x > 1500. { self.step } else { 0. };
            Some(10. + ramp + step)
        }
        fn water_depth(&self, x: f32, z: f32) -> Option<f32> {
            let (centre, radius) = self.lake?;
            let ground = self.ground(x, z)?;
            (flat(centre, [x, z]) < radius).then_some(12. - ground)
        }
        fn prop_top(&self, point: [f32; 3], depth: f32) -> Option<f32> {
            self.boxes
                .iter()
                .filter(|b| segment_meets(b, point, [point[0], point[1] - depth, point[2]], 0.))
                .map(|b| b.center[1] + b.half[1])
                .filter(|top| *top <= point[1])
                .max_by(f32::total_cmp)
        }
        fn prop_between(&self, a: [f32; 3], b: [f32; 3]) -> bool {
            self.boxes.iter().any(|x| segment_meets(x, a, b, 0.))
        }
        fn prop_near(&self, at: [f32; 2], radius: f32, low: f32, high: f32) -> bool {
            self.posts.iter().any(|b| {
                (at[0] - b.center[0]).abs() < b.half[0] + radius
                    && (at[1] - b.center[2]).abs() < b.half[2] + radius
                    && b.center[1] + b.half[1] > low
                    && b.center[1] - b.half[1] < high
            })
        }
    }

    fn boxed(center: [f32; 3], half: [f32; 3]) -> Bounds {
        Bounds { center, half }
    }

    const NORTH: [f32; 2] = [0., 1.];

    #[test]
    fn open_flat_ground_passes_facing_the_course() {
        let spot = check(&mut Test::default(), [300., 300.], [1., 0.]).unwrap();
        assert_eq!(spot.origin, [300., 10. + LIFT, 300.]);
        assert!((spot.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn each_rule_rejects_its_hazard() {
        let mut world = Test::default();
        assert_eq!(check(&mut world, [50., 300.], NORTH), Err(Reject::Outside));
        // The cliff ring may not leave the area either.
        assert_eq!(check(&mut world, [110., 300.], NORTH), Err(Reject::Outside));

        let mut lake = Test {
            lake: Some(([400., 400.], 30.)),
            ..Test::default()
        };
        assert_eq!(check(&mut lake, [400., 400.], NORTH), Err(Reject::Water));
        // At the shore: dry under the ATV, wet within the margin.
        assert_eq!(check(&mut lake, [400., 435.], NORTH), Err(Reject::Water));
        assert!(check(&mut lake, [400., 450.], NORTH).is_ok());

        let mut ramp = Test {
            rise: 0.4,
            ..Test::default()
        };
        assert_eq!(check(&mut ramp, [700., 300.], NORTH), Err(Reject::Steep));
        let mut gentle = Test {
            rise: 0.2,
            ..Test::default()
        };
        assert!(check(&mut gentle, [700., 300.], NORTH).is_ok());

        let mut ledge = Test {
            step: 15.,
            ..Test::default()
        };
        assert_eq!(check(&mut ledge, [1490., 300.], NORTH), Err(Reject::Cliff));
        assert!(check(&mut ledge, [1450., 300.], NORTH).is_ok());

        let mut post = Test {
            posts: vec![boxed([305., 12., 300.], [1., 3., 1.])],
            ..Test::default()
        };
        assert_eq!(check(&mut post, [300., 300.], NORTH), Err(Reject::Blocked));

        // A long wall beside the spot (a large mesh: rays find it).
        let mut wall = Test {
            boxes: vec![boxed([305., 15., 300.], [0.5, 5., 200.])],
            ..Test::default()
        };
        assert_eq!(check(&mut wall, [300., 300.], NORTH), Err(Reject::Blocked));

        let mut roof = Test {
            boxes: vec![boxed([300., 40., 300.], [30., 1., 30.])],
            ..Test::default()
        };
        assert_eq!(check(&mut roof, [300., 300.], NORTH), Err(Reject::Covered));

        let mut deck = Test {
            boxes: vec![boxed([300., 12., 300.], [30., 1., 30.])],
            ..Test::default()
        };
        assert_eq!(check(&mut deck, [300., 300.], NORTH), Err(Reject::OnProp));
    }

    #[test]
    fn a_spot_walled_in_is_boxed_in_and_a_blocked_course_turns_round() {
        // Walls 20 ft off on three sides (north, east, west): boxed in.
        let walls = |sides: &[[f32; 2]]| Test {
            boxes: sides
                .iter()
                .map(|d| {
                    let half = if d[0] == 0. {
                        [40., 5., 0.5]
                    } else {
                        [0.5, 5., 40.]
                    };
                    boxed([300. + d[0] * 20., 15., 300. + d[1] * 20.], half)
                })
                .collect(),
            ..Test::default()
        };
        assert_eq!(
            check(
                &mut walls(&[[0., 1.], [1., 0.], [-1., 0.]]),
                [300., 300.],
                NORTH
            ),
            Err(Reject::BoxedIn)
        );
        // One wall ahead: still open, but the rider faces away from it.
        let spot = check(&mut walls(&[[0., 1.]]), [300., 300.], NORTH).unwrap();
        assert!((spot.yaw.abs() - std::f32::consts::PI).abs() < 1e-3);
        // A rise steeper than the view rule ahead (40 ft up a step 30 ft
        // east) blocks the views that way: the rider faces west.
        let mut bank = Test {
            step: 40.,
            ..Test::default()
        };
        let spot = check(&mut bank, [1470., 300.], [1., 0.]).unwrap();
        assert!((spot.yaw + std::f32::consts::FRAC_PI_2).abs() < 1e-3);
    }

    #[test]
    fn candidates_follow_the_line_at_the_spacing() {
        let line = [[0., 0., 0.], [100., 0., 0.], [100., 0., 300.]];
        let c = line_candidates(&line, 80.);
        let want = [
            [40., 0.],
            [100., 20.],
            [100., 100.],
            [100., 180.],
            [100., 260.],
        ];
        assert_eq!(c.len(), want.len());
        for (c, want) in c.iter().zip(want) {
            assert!(flat(c.at, want) < 1e-3, "{:?} != {want:?}", c.at);
        }
        assert_eq!(c[0].heading, [1., 0.]);
        assert_eq!(c[1].heading, [0., 1.]);
        assert!(line_candidates(&line, 0.).is_empty());
    }

    #[test]
    fn validation_spreads_spots_and_tries_beside_the_line() {
        // A line along z = 300 through a post on the line itself.
        let mut world = Test {
            posts: vec![boxed([500., 12., 300.], [1., 3., 1.])],
            ..Test::default()
        };
        let line: Vec<[f32; 3]> = vec![[200., 0., 300.], [1200., 0., 300.]];
        let spots = Spots::validate(&mut world, &line_candidates(&line, 20.));
        assert_eq!(spots.tried, 50);
        for pair in spots.spots.windows(2) {
            assert!(gap(&pair[0], &pair[1]) >= MIN_GAP);
        }
        assert!(spots.spots.len() >= 15);
        assert!(spots.rejected.iter().any(|(r, _)| *r == Reject::Crowded));
        // The candidate on the post moved beside the line.
        let line: Vec<[f32; 3]> = vec![[460., 0., 300.], [540., 0., 300.]];
        let spots = Spots::validate(&mut world, &line_candidates(&line, 80.));
        assert_eq!(spots.spots.len(), 1);
        assert!((spots.spots[0].origin[2] - 300.).abs() > OFFSET - 0.01);
    }

    fn spots(points: &[[f32; 2]]) -> Spots {
        Spots {
            spots: points
                .iter()
                .map(|p| Spot {
                    origin: [p[0], 12.5, p[1]],
                    yaw: 0.,
                })
                .collect(),
            ..Spots::default()
        }
    }

    #[test]
    fn arrivals_avoid_riders_and_rooms_spread_their_slots() {
        let list = spots(&[[0., 0.], [100., 0.], [200., 0.], [300., 0.]]);
        assert_eq!(list.pick(1, &[]).unwrap().1, 1);
        // A rider on the chosen spot: the next clear one.
        assert_eq!(list.pick(1, &[[105., 12., 0.]]).unwrap().0.origin[0], 200.);
        // Riders on every spot: the one farthest from them.
        let crowd = [[0., 0., 0.], [100., 0., 0.], [200., 0., 0.], [290., 0., 0.]];
        assert_eq!(list.pick(0, &crowd).unwrap().1, 3);
        assert_eq!(Spots::default().pick(0, &[]), None);
        // Slots of a room that moved together never share a spot.
        let many = spots(&(0..50).map(|i| [i as f32 * 100., 0.]).collect::<Vec<_>>());
        let starts: std::collections::BTreeSet<usize> =
            (0..20).map(|s| many.slot_start(s, 7)).collect();
        assert_eq!(starts.len(), 20);
        let few: std::collections::BTreeSet<usize> =
            (0..4).map(|s| list.slot_start(s, 9)).collect();
        assert_eq!(few.len(), 4);
    }

    #[test]
    fn reset_takes_the_nearest_clear_spot_that_moves_the_rider() {
        let list = spots(&[[0., 0.], [100., 0.], [200., 0.], [300., 0.]]);
        // Standing on a spot: the next nearest, not the same one.
        assert_eq!(list.nearest([90., 12., 5.], &[]).unwrap().origin[0], 0.);
        assert_eq!(list.nearest([160., 12., 0.], &[]).unwrap().origin[0], 200.);
        // A rider there: the nearest clear one.
        assert_eq!(
            list.nearest([160., 12., 0.], &[[200., 12., 0.]])
                .unwrap()
                .origin[0],
            100.
        );
        assert_eq!(Spots::default().nearest([0.; 3], &[]), None);
    }

    #[test]
    fn coverage_counts_grid_cells_with_a_spot() {
        let list = spots(&[[150., 150.], [160., 170.], [1850., 1850.], [1000., 150.]]);
        let area = ([100., 100.], [1900., 1900.]);
        assert_eq!(list.cells_covered(area, 4), 3);
    }
}
