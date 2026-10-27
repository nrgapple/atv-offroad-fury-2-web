//! Race bookkeeping around recovered TDF geometry and independent CPU vehicles.
//! Race event/progression rules remain provisional; source-correlated driver
//! decisions live in race_ai and feed the recovered vehicle solver.
//!
//! Exact-build Track loader 00179680 reads a TDF as a graph: tag 1 is the
//! section count, tag 5 opens a section (6 flags, 7 length, 24 extra), tag 8
//! opens a route point, tag 13 gives a section's link count and each tag 14 is
//! a (slot, target section) link. Tags 15, 16 and 25 construct oriented boxes
//! at Track +0x54/+0x58/+0x5c. Every original course places tag 15 at its
//! start (matching the scene DefaultStartPosition and the Supercross start-gate
//! firework trigger) and tag 16 on the lap circuit (matching the
//! RaceOrderMatters finish trigger), so they are used as start and finish gates.
use crate::math::Det;
use crate::{Result, slice, u32le};
use serde::Serialize;

/// Sections flagged 0x100 are the parallel alternate line at a fork.
const ALTERNATE_SECTION: u32 = 0x100;
/// Native grid fallback in 001f8410 sets riders 4.66ft behind the start node.
const GRID_SETBACK: f32 = 4.66;
const COUNTDOWN: f32 = 4.;
/// Enduro race over this long after the first finisher (RaceStatus +0x94).
const ENDURO_FINISH_TIMEOUT: f32 = 30.;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Gate {
    pub center: [f32; 3],
    pub forward: [f32; 3],
    /// The authored direction as stored (box +0x50); `001a95b0` takes its
    /// unnormalized dot product with the rider's motion.
    pub facing: [f32; 3],
    pub half_width: [f32; 3],
    pub width: f32,
}

#[derive(Clone, Serialize)]
pub struct Course {
    /// Main racing line: an optional start straight followed by the lap
    /// circuit. The point after the last one is `loop_start`.
    pub points: Vec<[f32; 3]>,
    pub widths: Vec<f32>,
    pub flags: Vec<u32>,
    pub speeds: Vec<f32>,
    pub airtimes: Vec<f32>,
    /// TDF tag 27 (node +0x4c): scale on the AI jump preload (`0024ebb0`).
    #[serde(skip)]
    pub preload_scales: Vec<f32>,
    pub lane_limit: f32,
    pub leader_policy: [f32; 4],
    pub loop_start: usize,
    /// Checkpoint just past the finish gate. On courses without a finish
    /// gate, passing it completes a lap.
    pub finish: usize,
    /// The grid sits just behind the finish checkpoint, so its first pass
    /// begins the race instead of completing a lap (courses without a gate).
    pub finish_grace: bool,
    /// Ordered (fork end, rejoin) checkpoint pairs. A rider on the alternate
    /// line may reach the rejoin point without the main line's checkpoints.
    pub branches: Vec<(usize, usize)>,
    pub start_gate: Option<Gate>,
    pub finish_gate: Option<Gate>,
    /// Enduro gates in race order (`crate::enduro`); the last is the finish.
    /// Empty on TDF-gated races.
    pub enduro: Vec<Gate>,
    /// Race distance from the grid to where the main racing line first
    /// crosses the finish gate line (the box's track position, `0017da40`).
    pub finish_at: Option<f32>,
    /// TDF tag 25, the third oriented box (Track +0x5c).
    pub tag25_gate: Option<Gate>,
    pub rider_spacing: f32,
    /// TDF tag 41 (Track +0x30). Bit 4: RaceStatus reset `00247900` sets rider
    /// flag 0x10000, so the first finish-gate crossing only clears it instead
    /// of completing a lap. Bit 8 (rider flag 0x20000): the first counted lap
    /// is not a best-lap candidate.
    pub track_flags: u32,
    /// `001f8410` side of the start row, from section 0's first turn.
    pub grid_sign: f32,
    pub sections: usize,
    /// Authored tag 10/11 edge points; consecutive pairs bound the track
    /// polygon tested by the native on-track query (0017bb50).
    pub edges: Vec<[[f32; 3]; 2]>,
    pub alternates: Vec<Alternate>,
}
/// A 0x100 alternate line from main checkpoint `from` back to `to`.
#[derive(Clone, Serialize)]
pub struct Alternate {
    pub from: usize,
    pub to: usize,
    pub points: Vec<[f32; 3]>,
    pub edges: Vec<[[f32; 3]; 2]>,
}
/// Position on the main racing line: segment start checkpoint and fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    pub segment: usize,
    pub fraction: f32,
}
/// Result of the windowed on-track query.
#[derive(Clone, Copy, Debug)]
pub struct Located {
    pub station: Station,
    /// Signed route distance from the previous station.
    pub offset: f32,
    pub entered: bool,
}
/// Route window searched around a rider's current station: the forward and
/// backward limits RaceStatus `00248b20` passes to native 0017be80 from Track
/// +0x34/+0x38. The Track constructor `00179330` sets both to 200ft, then the
/// race track loader `00215900` sets +0x34 = 125ft and +0x38 = 75ft. +0x34
/// also bounds how near the finish a rider must be for a finish-gate
/// crossing to count.
pub const TRACK_AHEAD: f32 = 125.;
pub const TRACK_BEHIND: f32 = 75.;
/// Box +0x74, set to 150ft for the three Track boxes after loading: a crossing
/// higher than this above the gate center does not count.
const GATE_CEILING: f32 = 150.;
/// Result of the native gate test `001a95b0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crossing {
    None,
    /// Through the gate in its direction, under its ceiling.
    Through,
    /// Across the gate's line beside the gate.
    Beside,
}
impl Gate {
    /// Native `001a95b0` for a Track box (flags 3, no radius limit): the
    /// rider's motion from `previous` to `current` against the gate line, the
    /// box diagonal from center − half extents to center + half extents in
    /// x/z (`0012b6d0`). A zero previous position never crosses.
    pub fn crossing(&self, current: [f32; 3], previous: [f32; 3]) -> Crossing {
        self.crossing_with(current, previous, true)
    }
    /// `001a95b0` with box flag 4 clear (`directional`) or set: an Enduro
    /// gate (flags 5) counts a crossing in either direction.
    pub fn crossing_with(
        &self,
        current: [f32; 3],
        previous: [f32; 3],
        directional: bool,
    ) -> Crossing {
        if previous == [0.; 3] {
            return Crossing::None;
        }
        let (dx, dz) = (current[0] - previous[0], current[2] - previous[2]);
        let (ax, az) = (
            self.center[0] - self.half_width[0],
            self.center[2] - self.half_width[2],
        );
        let (gx, gz) = (2. * self.half_width[0], 2. * self.half_width[2]);
        let denominator = dz * gx - gz * dx;
        if denominator == 0. {
            return Crossing::None;
        }
        let t = (gz * (previous[0] - ax) + gx * (az - previous[2])) / denominator;
        let s = if gx == 0. {
            (previous[2] + dz * t - az) / gz
        } else {
            (previous[0] + dx * t - ax) / gx
        };
        if !(0. ..=1.).contains(&t) {
            return Crossing::None;
        }
        if !(0. ..=1.).contains(&s) {
            return Crossing::Beside;
        }
        let height = previous[1] + t * (current[1] - previous[1]) - self.center[1];
        let along =
            self.facing[0] * dx + self.facing[1] * (current[1] - previous[1]) + self.facing[2] * dz;
        if height <= GATE_CEILING && (along >= 0. || !directional) {
            Crossing::Through
        } else {
            Crossing::None
        }
    }
}
/// Track apron behind the first node of a start straight (grid setback 4.66ft).
const START_APRON: f32 = 30.;
impl Default for Course {
    fn default() -> Self {
        Self {
            points: vec![],
            widths: vec![],
            flags: vec![],
            speeds: vec![],
            airtimes: vec![],
            preload_scales: vec![],
            lane_limit: 3.,
            leader_policy: [150., 0.9, 1.5, 0.5],
            loop_start: 0,
            finish: 0,
            finish_grace: false,
            branches: vec![],
            start_gate: None,
            finish_gate: None,
            enduro: vec![],
            finish_at: None,
            tag25_gate: None,
            rider_spacing: 8.,
            track_flags: 0,
            grid_sign: -1.,
            sections: 1,
            edges: vec![],
            alternates: vec![],
        }
    }
}
fn float(bytes: &[u8], at: usize) -> Result<f32> {
    let v = f32::from_le_bytes(slice(bytes, at, 4)?.try_into().unwrap());
    if !v.is_finite() {
        return Err("nonfinite TDF field".into());
    }
    Ok(v)
}
#[derive(Default)]
struct Section {
    defined: bool,
    flags: u32,
    points: Vec<[f32; 3]>,
    widths: Vec<f32>,
    edges: Vec<[[f32; 3]; 2]>,
    point_flags: Vec<u32>,
    speeds: Vec<f32>,
    airtimes: Vec<f32>,
    preload_scales: Vec<f32>,
    links: Vec<Option<usize>>,
    closed: bool,
}
struct RoutePoint {
    section: usize,
    local: usize,
}
impl Course {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 256 * 1024 {
            return Err("TDF exceeds limit".into());
        }
        let mut at = 0;
        let mut offset = [None; 2];
        let mut scale = None;
        let mut lane_limit = 3.;
        let mut track_flags = 0;
        let mut leader_policy = [150., 0.9, 1.5, 0.5];
        let mut gates = [None; 3];
        let mut sections: Vec<Section> = Vec::new();
        let mut current: Option<usize> = None;
        let mut edges = Vec::new();
        let mut in_point = false;
        let mut in_tail = false;
        let mut point_field = 0;
        let transform = |offset: [Option<f32>; 2], scale: Option<f32>, v: [f32; 3], absolute| {
            let x = offset[0].ok_or("missing TDF x offset")?;
            let z = offset[1].ok_or("missing TDF z offset")?;
            let s = scale
                .filter(|s| *s > 0. && *s <= 1024.)
                .ok_or("invalid TDF scale")?;
            let point = if absolute {
                [x + v[0] * s, v[1] * s, z + v[2] * s]
            } else {
                v.map(|n| n * s)
            };
            if !point.iter().all(|v| v.is_finite() && v.abs() < 1_000_000.) {
                return Err::<[f32; 3], String>("invalid TDF coordinate".into());
            }
            Ok(point)
        };
        let vector = |at: usize| -> Result<[f32; 3]> {
            Ok([
                float(bytes, at)?,
                float(bytes, at + 4)?,
                float(bytes, at + 8)?,
            ])
        };
        while at < bytes.len() {
            let tag = bytes[at];
            at += 1;
            let size = match tag {
                8 => 0,
                9..=11 => 12,
                14 => 8,
                15..=16 | 25 => 40,
                1..=7 | 12 | 13 | 18..=24 | 26..=27 | 40..=41 => 4,
                _ => return Err(format!("unsupported TDF tag {tag}")),
            };
            slice(bytes, at, size)?;
            if in_point && tag != 8 && tag != 13 && tag != 5 {
                if [23, 9, 10, 11, 12, 18, 26, 27].get(point_field) != Some(&tag) {
                    return Err("invalid TDF route field order".into());
                }
                point_field += 1;
            }
            let complete_point = |sections: &mut Vec<Section>,
                                  current: Option<usize>,
                                  edges: &Vec<[f32; 3]>,
                                  point_field: usize|
             -> Result<()> {
                if edges.len() != 2 || point_field != 8 {
                    return Err("invalid TDF point order".into());
                }
                let section = &mut sections[current.unwrap()];
                section.widths.push(distance(edges[0], edges[1]));
                section.edges.push([edges[0], edges[1]]);
                Ok(())
            };
            match tag {
                // Native 00179680 allocates the section table from tag 1.
                1 => {
                    let count = u32le(bytes, at)? as usize;
                    if !sections.is_empty() || !(1..=64).contains(&count) {
                        return Err("invalid TDF section count".into());
                    }
                    sections.resize_with(count, Section::default);
                }
                2..=3 => {
                    offset[(tag - 2) as usize] = Some(float(bytes, at)?);
                }
                4 => {
                    scale = Some(float(bytes, at)?);
                }
                5 => {
                    let id = u32le(bytes, at)? as usize;
                    if in_tail || in_point || id >= sections.len() || sections[id].defined {
                        return Err("invalid TDF section".into());
                    }
                    if current.is_some_and(|previous| !sections[previous].closed) {
                        return Err("unterminated TDF section".into());
                    }
                    sections[id].defined = true;
                    current = Some(id);
                }
                6 => {
                    let id = current.ok_or("TDF section flags outside section")?;
                    sections[id].flags = u32le(bytes, at)?;
                }
                8 => {
                    if sections.is_empty() {
                        return Err("missing TDF section count".into());
                    }
                    // Single-section authoring may omit the tag 5 header.
                    if current.is_none() && sections.len() == 1 {
                        sections[0].defined = true;
                        current = Some(0);
                    }
                    let id = current.ok_or("TDF point outside section")?;
                    if in_tail || sections[id].closed {
                        return Err("invalid TDF point order".into());
                    }
                    if in_point {
                        complete_point(&mut sections, current, &edges, point_field)?;
                    }
                    if sections[id].points.len() > 4096 {
                        return Err("TDF section exceeds limit".into());
                    }
                    edges.clear();
                    point_field = 0;
                    in_point = true;
                }
                9..=11 => {
                    if !in_point || in_tail {
                        return Err("TDF coordinate outside route".into());
                    }
                    let point = transform(offset, scale, vector(at)?, true)?;
                    let section = &mut sections[current.unwrap()];
                    if tag == 9 {
                        if !edges.is_empty() || section.points.len() != section.widths.len() {
                            return Err("invalid TDF centers".into());
                        }
                        section.points.push(point);
                    } else {
                        edges.push(point);
                    }
                }
                13 => {
                    if !in_point || in_tail {
                        return Err("invalid TDF section end".into());
                    }
                    complete_point(&mut sections, current, &edges, point_field)?;
                    let count = u32le(bytes, at)? as usize;
                    if count > 8 {
                        return Err("invalid TDF link count".into());
                    }
                    let section = &mut sections[current.unwrap()];
                    section.links = vec![None; count];
                    section.closed = true;
                    in_point = false;
                }
                14 => {
                    let id = current.ok_or("TDF link outside section")?;
                    let slot = u32le(bytes, at)? as usize;
                    let target = u32le(bytes, at + 4)? as usize;
                    let count = sections.len();
                    let section = &mut sections[id];
                    if in_tail
                        || !section.closed
                        || target >= count
                        || section.links.get(slot) != Some(&None)
                    {
                        return Err("invalid TDF link".into());
                    }
                    section.links[slot] = Some(target);
                }
                15 | 16 | 25 => {
                    if in_point {
                        return Err("TDF gate inside route point".into());
                    }
                    in_tail = true;
                    let center = transform(offset, scale, vector(at)?, true)?;
                    let facing = vector(at + 12)?;
                    let forward = normalize_flat(facing).ok_or("invalid TDF gate direction")?;
                    let half_width = transform(offset, scale, vector(at + 24)?, false)?;
                    let width = float(bytes, at + 36)? * scale.unwrap_or(1.);
                    if !(0. ..=2048.).contains(&width) {
                        return Err("invalid TDF gate".into());
                    }
                    gates[if tag == 25 { 2 } else { (tag - 15) as usize }] = Some(Gate {
                        center,
                        forward,
                        facing,
                        half_width,
                        width,
                    });
                }
                23 if in_point => {
                    sections[current.unwrap()]
                        .point_flags
                        .push(u32le(bytes, at)?);
                }
                18 if in_point => {
                    let speed = float(bytes, at)?;
                    if speed != -1. && !(0. ..=1000.).contains(&speed) {
                        return Err("invalid TDF speed".into());
                    }
                    sections[current.unwrap()].speeds.push(speed);
                }
                26 if in_point => {
                    sections[current.unwrap()].airtimes.push(float(bytes, at)?);
                }
                27 if in_point => {
                    sections[current.unwrap()]
                        .preload_scales
                        .push(float(bytes, at)?);
                }
                19..=22 => {
                    in_tail = true;
                    leader_policy[(tag - 19) as usize] = float(bytes, at)?;
                }
                40 => {
                    in_tail = true;
                    lane_limit = float(bytes, at)?;
                }
                41 => {
                    track_flags = u32le(bytes, at)?;
                }
                7 | 12 | 26..=27 => {
                    float(bytes, at)?;
                }
                _ => {}
            }
            at += size;
        }
        if sections.is_empty()
            || in_point
            || sections
                .iter()
                .any(|s| !s.defined || !s.closed || s.points.len() < 2)
        {
            return Err("invalid or open TDF course".into());
        }
        if sections.iter().any(|s| {
            s.point_flags.len() != s.points.len()
                || s.speeds.len() != s.points.len()
                || s.airtimes.len() != s.points.len()
                || s.preload_scales.len() != s.points.len()
                || s.links.iter().any(Option::is_none)
        }) || !(0. ..=30.).contains(&lane_limit)
            || leader_policy[0] <= 0.
            || leader_policy[1..].iter().any(|v| *v < 0. || *v > 3.)
        {
            return Err("invalid TDF AI configuration".into());
        }
        // An unlinked single section is the closed Caribou-style loop.
        if sections.len() == 1 && sections[0].links.is_empty() {
            sections[0].links = vec![Some(0)];
        }
        let mut course = Self::linearize(&sections, lane_limit, leader_policy, gates)?;
        course.track_flags = track_flags;
        Ok(course)
    }

    fn linearize(
        sections: &[Section],
        lane_limit: f32,
        leader_policy: [f32; 4],
        gates: [Option<Gate>; 3],
    ) -> Result<Self> {
        let successor = |id: usize| -> Option<usize> {
            let links: Vec<usize> = sections[id].links.iter().flatten().copied().collect();
            links
                .iter()
                .copied()
                .find(|t| sections[*t].flags & ALTERNATE_SECTION == 0)
                .or(links.first().copied())
        };
        // Follow the main line from section 0 until it closes into the lap.
        let mut order = Vec::new();
        let mut id = 0;
        while !order.contains(&id) {
            order.push(id);
            id = successor(id).ok_or("TDF section has no successor")?;
        }
        let entry = order.iter().position(|s| *s == id).unwrap();
        let mut route: Vec<RoutePoint> = Vec::new();
        let mut prelude = 0;
        for (n, &section) in order.iter().enumerate() {
            if n == entry {
                prelude = route.len();
            }
            for local in 0..sections[section].points.len() {
                let p = sections[section].points[local];
                // Consecutive sections may repeat their junction point.
                if route
                    .last()
                    .is_some_and(|r| distance(sections[r.section].points[r.local], p) < 0.5)
                {
                    continue;
                }
                route.push(RoutePoint { section, local });
            }
        }
        let at = |r: &RoutePoint| sections[r.section].points[r.local];
        if route.len() > prelude + 1
            && distance(at(&route[prelude]), at(route.last().unwrap())) < 1.
        {
            route.pop();
        }
        let cycle = route.len() - prelude;
        if cycle < 3 || route.len() < 4 || route.len() > 4096 {
            return Err("invalid TDF route dimensions".into());
        }
        let next = |i: usize| if i + 1 == route.len() { prelude } else { i + 1 };
        // Nearest route segment in the gate's direction.
        let nearest = |gate: &Gate, range: std::ops::Range<usize>| {
            range
                .map(|i| {
                    let (a, b) = (at(&route[i]), at(&route[next(i)]));
                    let tangent = normalize_flat([b[0] - a[0], 0., b[2] - a[2]]).unwrap_or([0.; 3]);
                    let along = tangent[0] * gate.forward[0] + tangent[2] * gate.forward[2];
                    let penalty = if along < 0. { 10_000. } else { 0. };
                    (segment_distance(gate.center, a, b) + penalty, i)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, i)| i)
                .unwrap()
        };
        let start = gates[0].map_or(0, |g| nearest(&g, 0..route.len()));
        let finish_segment = gates[1].map(|g| nearest(&g, prelude..route.len()));
        // Reorder so the start segment is index 0 and the lap circuit follows.
        let mut ordered: Vec<usize> = (start..route.len()).collect();
        let loop_start = if start < prelude {
            prelude - start
        } else {
            ordered.extend(prelude..start);
            0
        };
        let position = |old: usize| ordered.iter().position(|o| *o == old).unwrap();
        let finish = finish_segment.map_or(loop_start, |i| position(next(i)));
        let points: Vec<[f32; 3]> = ordered.iter().map(|i| at(&route[*i])).collect();
        let field = |f: &dyn Fn(&Section, usize) -> f32| -> Vec<f32> {
            ordered
                .iter()
                .map(|i| f(&sections[route[*i].section], route[*i].local))
                .collect()
        };
        let widths = field(&|s, l| s.widths[l]);
        let edges = ordered
            .iter()
            .map(|i| sections[route[*i].section].edges[route[*i].local])
            .collect();
        let speeds = field(&|s, l| s.speeds[l]);
        let airtimes = field(&|s, l| s.airtimes[l]);
        let preload_scales = field(&|s, l| s.preload_scales[l]);
        let flags = ordered
            .iter()
            .map(|i| sections[route[*i].section].point_flags[route[*i].local])
            .collect();
        if widths
            .iter()
            .any(|w| !w.is_finite() || *w < 1. || *w > 2048.)
            || (0..points.len()).any(|i| {
                let n = if i + 1 == points.len() {
                    loop_start
                } else {
                    i + 1
                };
                distance(points[i], points[n]) < 0.01
            })
        {
            return Err("invalid TDF route dimensions".into());
        }
        let mut course = Self {
            points,
            widths,
            flags,
            speeds,
            airtimes,
            preload_scales,
            lane_limit,
            leader_policy,
            loop_start,
            finish,
            finish_grace: false,
            branches: vec![],
            start_gate: gates[0],
            finish_gate: gates[1],
            enduro: vec![],
            finish_at: None,
            tag25_gate: gates[2],
            rider_spacing: 8.,
            track_flags: 0,
            grid_sign: grid_sign(&sections[0].points),
            sections: sections.len(),
            edges,
            alternates: vec![],
        };
        // Alternate lines leave a fork section and rejoin the main line later.
        let main: Vec<usize> = order.clone();
        for &fork in &order {
            let primary = successor(fork);
            for &alternate in sections[fork].links.iter().flatten() {
                if Some(alternate) == primary {
                    continue;
                }
                let mut id = alternate;
                let mut steps = 0;
                while !main.contains(&id) && steps < sections.len() {
                    id = successor(id).unwrap_or(id);
                    steps += 1;
                }
                let end = course.points.len();
                let fork_end = (0..end).rev().find(|i| {
                    let r = &route[ordered[*i]];
                    r.section == fork && r.local + 1 == sections[fork].points.len()
                });
                // The rejoin section's first point may have been merged into
                // the preceding junction point.
                let rejoin = (0..end)
                    .find(|i| {
                        let r = &route[ordered[*i]];
                        r.section == id && r.local == 0
                    })
                    .or_else(|| {
                        (0..end)
                            .find(|i| {
                                let r = &route[ordered[*i]];
                                r.section == id && r.local == 1
                            })
                            .map(|i| {
                                if i == course.loop_start {
                                    end - 1
                                } else {
                                    i - 1
                                }
                            })
                    });
                if let (Some(from), Some(to), true) = (fork_end, rejoin, main.contains(&id))
                    && course.steps(from, to) * 2 < course.points.len()
                {
                    course.branches.push((from, to));
                    // Alternate polyline: fork end, the off-line sections, rejoin.
                    let mut points = vec![course.points[from]];
                    let mut edges = vec![course.edges[from]];
                    let mut line = alternate;
                    for _ in 0..sections.len() {
                        if main.contains(&line) {
                            break;
                        }
                        for (p, e) in sections[line].points.iter().zip(&sections[line].edges) {
                            if distance(*points.last().unwrap(), *p) >= 0.5 {
                                points.push(*p);
                                edges.push(*e);
                            }
                        }
                        line = successor(line).unwrap_or(line);
                    }
                    if distance(*points.last().unwrap(), course.points[to]) >= 0.5 {
                        points.push(course.points[to]);
                        edges.push(course.edges[to]);
                    }
                    if points.len() >= 3 {
                        course.alternates.push(Alternate {
                            from,
                            to,
                            points,
                            edges,
                        });
                    }
                }
            }
        }
        course.branches.sort_unstable();
        course.branches.dedup();
        course.finish_at = course.finish_gate.and_then(|g| course.gate_distance(&g));
        // Checkpoint 0 is the grid itself and is first passed after a lap.
        course.finish_grace = course.finish != 0
            && course.route_length(0, course.finish) < 0.25 * course.cycle_length();
        Ok(course)
    }
    /// The race track loader `00215900` rebuilds the finish box from the
    /// scene's finish gate object (`natfgate`, `stfgate` on Short Tracks):
    /// centered on the model, across its right axis by `half_width`, facing
    /// its look vector. Re-places it on the route (`0017b290`); a box the
    /// main racing line does not cross keeps the TDF box.
    pub fn set_finish_gate(&mut self, matrix: &[f32; 16], half_width: f32) -> bool {
        let right = [matrix[0], matrix[1], matrix[2]];
        let facing = [matrix[8], matrix[9], matrix[10]];
        let (Some(forward), true) = (
            normalize_flat(facing),
            half_width.is_finite() && (1. ..=200.).contains(&half_width),
        ) else {
            return false;
        };
        let gate = Gate {
            center: [matrix[12], matrix[13], matrix[14]],
            forward,
            facing,
            half_width: right.map(|v| v * half_width),
            width: half_width,
        };
        let Some(at) = self.gate_distance(&gate) else {
            return false;
        };
        self.finish_gate = Some(gate);
        self.finish_at = Some(at);
        true
    }
    /// Race distance from the grid to the first point where the main racing
    /// line crosses `gate`'s line within the gate, like the box track position
    /// `0017da40` finds by intersecting the box diagonal with route segments.
    pub fn gate_distance(&self, gate: &Gate) -> Option<f32> {
        let mut total = 0.;
        let mut segment = 0;
        for _ in 0..self.points.len() + self.loop_start {
            let (a, b) = (self.points[segment], self.points[self.next(segment)]);
            let (ax, az) = (
                gate.center[0] - gate.half_width[0],
                gate.center[2] - gate.half_width[2],
            );
            let (gx, gz) = (2. * gate.half_width[0], 2. * gate.half_width[2]);
            let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
            let denominator = dz * gx - gz * dx;
            if denominator != 0. {
                let t = (gz * (a[0] - ax) + gx * (az - a[2])) / denominator;
                let s = if gx == 0. {
                    (a[2] + dz * t - az) / gz
                } else {
                    (a[0] + dx * t - ax) / gx
                };
                if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s) {
                    return Some(total + t * self.segment(segment));
                }
            }
            total += self.segment(segment);
            segment = self.next(segment);
        }
        None
    }
    /// Route distance from the grid (checkpoint 0) to a station.
    pub fn station_distance(&self, s: Station) -> f32 {
        self.route_length(0, s.segment) + s.fraction * self.segment(s.segment)
    }
    /// Whether a station lies just past the finish gate, within the Track
    /// +0x38 window. On env07 and sx13 the start row stands about 20ft past
    /// the gate on a course with Track flag 4. Read literally, the skipped
    /// first crossing would then come only after a full circuit and make the
    /// first lap two circuits long. The port assumes the original consumes
    /// the skip during start placement on those courses; this is not
    /// established from the exact build.
    pub fn grid_past_finish(&self, s: Station) -> bool {
        self.finish_at.is_some_and(|at| {
            (self.station_distance(s) - at).rem_euclid(self.cycle_length()) < TRACK_BEHIND
        })
    }
    /// Edge pair at a main checkpoint; synthesized from its width when a
    /// hand-built course has no authored edges.
    pub fn edge(&self, index: usize) -> [[f32; 3]; 2] {
        if let Some(e) = self.edges.get(index) {
            return *e;
        }
        let p = self.points[index];
        let a = self.points[self.previous(index, true).unwrap_or(index)];
        let b = self.points[self.next(index)];
        let t = normalize_flat([b[0] - a[0], 0., b[2] - a[2]]).unwrap_or([0., 0., 1.]);
        let h = self.widths[index] * 0.5;
        [
            [p[0] - t[2] * h, p[1], p[2] + t[0] * h],
            [p[0] + t[2] * h, p[1], p[2] - t[0] * h],
        ]
    }
    /// The checkpoint before `index`; across the lap seam inside the circuit.
    pub fn previous(&self, index: usize, in_cycle: bool) -> Option<usize> {
        if index == self.loop_start && in_cycle {
            Some(self.points.len() - 1)
        } else {
            index.checked_sub(1)
        }
    }
    pub fn point_at(&self, s: Station) -> [f32; 3] {
        let a = self.points[s.segment];
        let b = self.points[self.next(s.segment)];
        std::array::from_fn(|n| a[n] + s.fraction * (b[n] - a[n]))
    }
    /// Heading (yaw about +Y, atan2(x, z)) of the racing line at a station.
    pub fn heading_at(&self, s: Station) -> f32 {
        let a = self.points[s.segment];
        let b = self.points[self.next(s.segment)];
        (b[0] - a[0]).atan2_d(b[2] - a[2])
    }
    /// Station `distance` feet further along the racing line.
    pub fn advance(&self, mut s: Station, mut distance: f32) -> Station {
        for _ in 0..=self.points.len() {
            let length = self.segment(s.segment).max(0.0001);
            let left = (1. - s.fraction) * length;
            if distance <= left {
                s.fraction += distance / length;
                return s;
            }
            distance -= left;
            s = Station {
                segment: self.next(s.segment),
                fraction: 0.,
            };
        }
        s
    }
    /// Race distance of the first lap: grid to the checkpoint after the finish
    /// gate, plus one lap when the grid sits just behind that gate.
    pub fn first_lap(&self) -> f32 {
        let first = if self.finish == 0 {
            self.cycle_length()
        } else {
            self.route_length(0, self.finish)
        };
        if self.finish_grace {
            first + self.cycle_length()
        } else {
            first
        }
    }
    /// Windowed on-track query after native 0017be80: the rider must lie inside
    /// an authored edge polygon within the route window around its station.
    /// Alternate lines map onto the main line between their fork and rejoin.
    pub fn locate(&self, p: [f32; 3], from: Station, entered: bool) -> Option<Located> {
        let start = self.segment(from.segment) * from.fraction;
        let mut best: Option<(f32, Located)> = None;
        let mut consider = |segment: usize, offset_to_start: f32, entered: bool, fraction: f32| {
            let offset = offset_to_start + fraction * self.segment(segment);
            let located = Located {
                station: Station { segment, fraction },
                offset,
                entered,
            };
            if best.is_none_or(|(o, _)| offset.abs() < o) {
                best = Some((offset.abs(), located));
            }
        };
        // Forward along the main line.
        let mut segment = from.segment;
        let mut offset = -start;
        let mut in_cycle = entered;
        let mut visited = Vec::new();
        for _ in 0..self.points.len() {
            if offset > TRACK_AHEAD {
                break;
            }
            visited.push((segment, offset, in_cycle));
            if let Some(f) = self.inside(p, segment) {
                consider(segment, offset, in_cycle, f);
            }
            offset += self.segment(segment);
            segment = self.next(segment);
            in_cycle |= segment == self.loop_start;
        }
        // Backward along the main line.
        let mut segment = from.segment;
        let mut offset = -start;
        for _ in 0..self.points.len() {
            let Some(previous) = self.previous(segment, entered) else {
                break;
            };
            offset -= self.segment(previous);
            if offset + self.segment(previous) < -TRACK_BEHIND {
                break;
            }
            segment = previous;
            if let Some(f) = self.inside(p, segment) {
                consider(segment, offset, entered, f);
            }
        }
        // Alternate lines whose fork lies in the forward window, or which the
        // rider is already between.
        for alt in &self.alternates {
            let Some((_, fork_offset, fork_cycle)) =
                visited
                    .iter()
                    .copied()
                    .find(|(s, _, _)| *s == alt.from)
                    .or_else(|| {
                        (self.steps(alt.from, from.segment) < self.steps(alt.from, alt.to))
                            .then_some((alt.from, f32::NAN, entered))
                    })
            else {
                continue;
            };
            let fork_offset = if fork_offset.is_nan() {
                -start - self.route_length(alt.from, from.segment)
            } else {
                fork_offset
            };
            let span = self.route_length(alt.from, alt.to);
            let total: f32 = alt
                .points
                .windows(2)
                .map(|w| distance(w[0], w[1]))
                .sum::<f32>()
                .max(0.0001);
            let mut along = 0.;
            for i in 0..alt.points.len() - 1 {
                let length = distance(alt.points[i], alt.points[i + 1]);
                if let Some(f) = quad(
                    p,
                    alt.edges[i],
                    alt.edges[i + 1],
                    alt.points[i],
                    alt.points[i + 1],
                ) {
                    let mapped = (along + f * length) / total * span;
                    let station = self.advance(
                        Station {
                            segment: alt.from,
                            fraction: 0.,
                        },
                        mapped,
                    );
                    let crosses = self.steps(alt.from, station.segment)
                        >= self.steps(alt.from, self.loop_start)
                        && self.loop_start != alt.from;
                    let located = Located {
                        station,
                        offset: fork_offset + mapped,
                        entered: fork_cycle || crosses,
                    };
                    // The native query only searches the route window, on
                    // alternates as on the main line. Without this an
                    // alternate that crosses a later part of the circuit
                    // (sx14's 12->57 line) maps a rider there back to near
                    // its fork, about 1,000ft behind.
                    let in_window = (-TRACK_BEHIND..=TRACK_AHEAD).contains(&located.offset);
                    if in_window && best.is_none_or(|(o, _)| located.offset.abs() < o) {
                        best = Some((located.offset.abs(), located));
                    }
                }
                along += length;
            }
        }
        best.map(|(_, l)| l)
    }
    /// Whether `p` lies inside the authored edge polygon of `segment` alone
    /// (`0017bb50`), without the start-apron allowance of the race query.
    pub fn within(&self, p: [f32; 3], segment: usize) -> bool {
        let n = self.next(segment);
        quad(
            p,
            self.edge(segment),
            self.edge(n),
            self.points[segment],
            self.points[n],
        )
        .is_some()
    }
    /// Fraction along `segment` when `p` lies inside its edge polygon.
    fn inside(&self, p: [f32; 3], segment: usize) -> Option<f32> {
        let n = self.next(segment);
        let on = quad(
            p,
            self.edge(segment),
            self.edge(n),
            self.points[segment],
            self.points[n],
        );
        // The grid sits behind the start gate, before the first node of a
        // start straight; a short apron behind it counts as on track.
        if on.is_some() || segment != 0 || self.loop_start == 0 {
            return on;
        }
        let a = self.points[0];
        let b = self.points[n];
        let t = normalize_flat([b[0] - a[0], 0., b[2] - a[2]])?;
        let e = self.edge(0);
        let back = e.map(|v| [v[0] - t[0] * START_APRON, v[1], v[2] - t[2] * START_APRON]);
        let behind = [a[0] - t[0] * START_APRON, a[1], a[2] - t[2] * START_APRON];
        quad(p, back, e, behind, a).map(|_| 0.)
    }
    /// The checkpoint after `index` on the main racing line.
    pub fn next(&self, index: usize) -> usize {
        if index + 1 >= self.points.len() {
            self.loop_start
        } else {
            index + 1
        }
    }
    pub fn steps(&self, from: usize, to: usize) -> usize {
        let mut i = from;
        for n in 0..=self.points.len() {
            if i == to {
                return n;
            }
            i = self.next(i);
        }
        usize::MAX
    }
    pub fn segment(&self, index: usize) -> f32 {
        distance(self.points[index], self.points[self.next(index)])
    }
    /// Ordered route length from `from` forward to `to`.
    pub fn route_length(&self, from: usize, to: usize) -> f32 {
        let mut i = from;
        let mut length = 0.;
        for _ in 0..self.points.len() {
            if i == to {
                break;
            }
            length += self.segment(i);
            i = self.next(i);
        }
        length
    }
    pub fn cycle_length(&self) -> f32 {
        (self.loop_start..self.points.len())
            .map(|i| self.segment(i))
            .sum()
    }
    /// Make this an Enduro waypoint race (EnduroRaceEvent): the gates
    /// replace the TDF start and finish boxes, and `001f8410` lines the grid
    /// up in one row past the last gate, slots across its direction.
    pub fn set_enduro(&mut self, gates: Vec<Gate>) -> Result<()> {
        if !(2..=crate::enduro::MAX_WAYPOINTS).contains(&gates.len()) {
            return Err("invalid Enduro gates".into());
        }
        self.enduro = gates;
        self.start_gate = None;
        self.finish_gate = None;
        self.finish_at = None;
        self.track_flags = 0;
        self.grid_sign = 1.;
        Ok(())
    }
    pub fn is_enduro(&self) -> bool {
        !self.enduro.is_empty()
    }
    /// Grid origin and heading: behind the original start gate when present.
    pub fn start(&self) -> ([f32; 3], f32) {
        if let Some(last) = self.enduro.last() {
            let [dx, _, dz] = last.forward;
            let [x, y, z] = last.center;
            let past = crate::enduro::GRID_PAST_FINISH;
            return ([x + past * dx, y, z + past * dz], dx.atan2_d(dz));
        }
        let a = self.points[0];
        let b = self.points[self.next(0)];
        match self.start_gate {
            Some(g) => {
                // Route centers carry terrain height; the gate is authored flat.
                let dx = b[0] - a[0];
                let dz = b[2] - a[2];
                let t = (((g.center[0] - a[0]) * dx + (g.center[2] - a[2]) * dz)
                    / (dx * dx + dz * dz).max(0.0001))
                .clamp(0., 1.);
                (
                    [
                        g.center[0] - g.forward[0] * GRID_SETBACK,
                        a[1] + t * (b[1] - a[1]),
                        g.center[2] - g.forward[2] * GRID_SETBACK,
                    ],
                    g.forward[0].atan2_d(g.forward[2]),
                )
            }
            None => (a, (b[0] - a[0]).atan2_d(b[2] - a[2])),
        }
    }
    /// Race grid helper `001f8410` (race modes 3–10): rider slot `slot` of
    /// `riders` stands `sign × (slot − riders/2) × RiderSpacing` across the
    /// start, along (fz, 0, −fx).
    pub fn grid(&self, slot: u32, riders: usize) -> ([f32; 3], f32) {
        let (start, yaw) = self.start();
        let across = self.grid_sign * (slot as f32 - riders as f32 * 0.5) * self.rider_spacing;
        (
            [
                start[0] + yaw.cos_d() * across,
                start[1],
                start[2] - yaw.sin_d() * across,
            ],
            yaw,
        )
    }
}
/// `001f8410` row side: −1 when section 0's first two node directions
/// (`0017d980`, normalized node-to-next vectors) turn with a positive
/// cross-product y, otherwise 1; −1 without three nodes.
fn grid_sign(points: &[[f32; 3]]) -> f32 {
    let [p0, p1, p2] = match points {
        [a, b, c, ..] => [*a, *b, *c],
        _ => return -1.,
    };
    let d0 = [p1[0] - p0[0], p1[2] - p0[2]];
    let d1 = [p2[0] - p1[0], p2[2] - p1[2]];
    // Normalization does not change the sign of the y component.
    if d0[1] * d1[0] - d0[0] * d1[1] > 0. {
        -1.
    } else {
        1.
    }
}
/// Start slot table `001f6350`: slots 1…n, then for each pair i < j a swap
/// whenever rand × 2⁻³¹ > 0.5 (skipped when settings +0x7d8 is set). Rider k
/// (the human first, then the CPUs) starts in slot `table[k]`.
pub fn start_slots(riders: usize, random: &mut crate::engine_audio::OriginalRandom) -> Vec<u32> {
    let mut table: Vec<u32> = (1..=riders as u32).collect();
    for i in 0..riders.saturating_sub(1) {
        for j in i + 1..riders {
            if random.next_u31() as f32 * 4.656613e-10 > 0.5 {
                table.swap(i, j);
            }
        }
    }
    table
}
fn normalize_flat(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = (v[0] * v[0] + v[2] * v[2]).sqrt();
    (length > 0.000001 && length.is_finite()).then(|| [v[0] / length, 0., v[2] / length])
}
/// Point-in-quad test for the polygon between two authored edge pairs,
/// returning the centerline fraction of the projection.
fn quad(
    p: [f32; 3],
    e0: [[f32; 3]; 2],
    e1: [[f32; 3]; 2],
    a: [f32; 3],
    b: [f32; 3],
) -> Option<f32> {
    let corners = [e0[0], e1[0], e1[1], e0[1]];
    let mut inside = false;
    for i in 0..4 {
        let (c, d) = (corners[i], corners[(i + 3) % 4]);
        if (c[2] > p[2]) != (d[2] > p[2])
            && p[0] < (d[0] - c[0]) * (p[2] - c[2]) / (d[2] - c[2]) + c[0]
        {
            inside = !inside;
        }
    }
    if !inside {
        return None;
    }
    let dx = b[0] - a[0];
    let dz = b[2] - a[2];
    Some(
        (((p[0] - a[0]) * dx + (p[2] - a[2]) * dz) / (dx * dx + dz * dz).max(0.0001)).clamp(0., 1.),
    )
}
fn segment_distance(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dz = b[2] - a[2];
    let t =
        (((p[0] - a[0]) * dx + (p[2] - a[2]) * dz) / (dx * dx + dz * dz).max(0.0001)).clamp(0., 1.);
    distance(p, [a[0] + dx * t, p[1], a[2] + dz * t])
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
#[derive(Clone)]
struct Progress {
    /// Last on-track station on the main racing line.
    station: Station,
    entered: bool,
    /// Race distance along the racing line from the grid, and its maximum.
    distance: f32,
    furthest: f32,
    completed: u32,
    last: [f32; 3],
    finish: Option<f32>,
    lap_start: f32,
    best: Option<f32>,
    last_lap: f32,
    /// Completed lap times, oldest first.
    lap_times: Vec<f32>,
    /// Seconds since the rider last lay inside the track polygon.
    off_track: f32,
    /// Rider flag 0x10000: the next finish-gate crossing only clears it.
    skip_crossing: bool,
    /// Rider flag 0x20000: the next counted lap is not a best-lap candidate.
    skip_best: bool,
    /// Stunt points scored since GO, frozen at the finish.
    stunt: f32,
    /// Enduro (`0024a490`): the next gate (+0xb0/+0xb4), gates crossed
    /// (+0x34) and the gate-to-gate distance credited so far (+0x20).
    next_gate: usize,
    gates_passed: u32,
    gate_distance: f32,
}
fn gate_gap(a: &Gate, b: &Gate) -> f32 {
    let d = [0, 1, 2].map(|i| a.center[i] - b.center[i]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}
impl Progress {
    fn new(start: [f32; 3], course: &Course) -> Self {
        let entered = course.loop_start == 0;
        // An Enduro grid stands past the finish gate, anywhere on the TDF
        // route: start the station search at the nearest route segment.
        let segment = if course.is_enduro() {
            (0..course.points.len())
                .min_by(|&a, &b| {
                    let d = |i: usize| {
                        segment_distance(start, course.points[i], course.points[course.next(i)])
                    };
                    d(a).total_cmp(&d(b))
                })
                .unwrap_or(0)
        } else {
            0
        };
        let origin = Station {
            segment,
            fraction: 0.,
        };
        let station = course
            .locate(start, origin, entered)
            .filter(|l| l.offset >= 0.)
            .map_or(origin, |l| l.station);
        let distance = course.segment(0) * station.fraction * f32::from(station.segment == 0);
        Self {
            station,
            entered,
            distance,
            furthest: distance,
            completed: 0,
            last: start,
            finish: None,
            lap_start: 0.,
            best: None,
            last_lap: 0.,
            lap_times: Vec::new(),
            off_track: 0.,
            skip_crossing: course.track_flags & 4 != 0 && !course.grid_past_finish(station),
            skip_best: course.track_flags & 8 != 0,
            stunt: 0.,
            next_gate: 0,
            gates_passed: 0,
            // `00247900`: the distance from the finish gate to gate 0.
            gate_distance: match (course.enduro.first(), course.enduro.last()) {
                (Some(first), Some(last)) => gate_gap(first, last),
                _ => 0.,
            },
        }
        .with_enduro_distance(start, course)
    }
    fn with_enduro_distance(mut self, p: [f32; 3], course: &Course) -> Self {
        if let Some(next) = course.enduro.get(self.next_gate) {
            self.distance = self.gate_distance - distance(next.center, p);
            self.furthest = self.distance;
        }
        self
    }
    /// RaceStatus type 1 (`0024a490`): only the next gate is tested; a
    /// crossing either way advances to the following gate, and crossing the
    /// last gate counts a lap and wraps to gate 0. Passing beside the gate
    /// does nothing, so a missed gate must be ridden back to. Race distance
    /// is the credited gate distance less the distance to the next gate.
    fn update_enduro(
        &mut self,
        p: [f32; 3],
        previous: [f32; 3],
        course: &Course,
        time: f32,
        laps: u32,
    ) {
        let gates = &course.enduro;
        let next = self.next_gate;
        if previous[0] >= 0. && gates[next].crossing_with(p, previous, false) == Crossing::Through {
            let following = (next + 1) % gates.len();
            self.gate_distance += gate_gap(&gates[following], &gates[next]);
            self.next_gate = following;
            if following == 0 {
                self.complete_lap(time, laps);
            }
            self.gates_passed += 1;
        }
        self.distance = self.gate_distance - distance(gates[self.next_gate].center, p);
        self.furthest = self.furthest.max(self.distance);
    }
    fn update(&mut self, p: [f32; 3], course: &Course, time: f32, laps: u32, dt: f32) {
        // Progress only moves inside the track polygon and the route window, so
        // cutting across the infield or a teleport cannot skip the course.
        // After the finish the station still follows the rider, who keeps
        // driving under CPU control, but the race result is settled.
        let previous_distance = self.distance;
        let previous = self.last;
        if course.is_enduro() {
            // Type 1 runs no on-track query (no Off Track or route window);
            // the station only follows the rider for the CPU drivers.
            if let Some(located) = course.locate(p, self.station, self.entered) {
                self.station = located.station;
                self.entered = located.entered;
            }
            self.last = p;
            if self.finish.is_none() {
                self.update_enduro(p, previous, course, time, laps);
            }
            return;
        }
        match course.locate(p, self.station, self.entered) {
            Some(located) => {
                self.station = located.station;
                self.entered = located.entered;
                if self.finish.is_none() {
                    self.distance += located.offset;
                }
                self.off_track = 0.;
            }
            None => self.off_track += dt,
        }
        self.last = p;
        if self.finish.is_some() {
            return;
        }
        self.furthest = self.furthest.max(self.distance);
        if let (Some(gate), Some(at)) = (course.finish_gate, course.finish_at) {
            // RaceStatus `00248b20`: a lap counts when the rider's motion
            // crosses the finish gate (`001a95b0`) while its previous station
            // lies less than Track +0x34 before the gate on the route.
            //
            // `at` is where the main racing line meets the gate line. A rider
            // off the line crosses a skewed gate a little later on the route
            // (sx03: 0.6ft past `at`, env01 and sx13 similar), and its
            // accumulated distance can lead the station slightly, so once a
            // rider has reached the gate's distance, a previous distance up
            // to Track +0x38 past it also counts (authored; read literally
            // the crossing would wrap to a whole lap ahead and never count).
            // A grid just past the gate (env01, 4ft) has not reached it.
            let cycle = course.cycle_length();
            let near = if previous_distance <= at {
                at - previous_distance < TRACK_AHEAD
            } else {
                let past = (previous_distance - at).rem_euclid(cycle);
                cycle - past < TRACK_AHEAD || past < TRACK_BEHIND
            };
            if near && gate.crossing(p, previous) == Crossing::Through {
                if self.skip_crossing {
                    self.skip_crossing = false;
                } else {
                    self.complete_lap(time, laps);
                }
            }
            return;
        }
        // Courses without a finish gate: a lap completes when the furthest
        // progress passes the finish checkpoint.
        while self.completed < laps
            && self.furthest >= course.first_lap() + self.completed as f32 * course.cycle_length()
        {
            self.complete_lap(time, laps);
        }
    }
    /// Lap bookkeeping after a counted crossing (`00248b20`): a negative lap
    /// time is clamped to zero and skips the best-lap update, as does the
    /// first lap on a course with Track flag 8.
    fn complete_lap(&mut self, time: f32, laps: u32) {
        self.completed += 1;
        let lap = time - self.lap_start;
        self.last_lap = lap.max(0.);
        self.lap_times.push(self.last_lap);
        self.lap_start = time;
        if lap >= 0. && !self.skip_best && self.best.is_none_or(|b| lap < b) {
            self.best = Some(lap);
        }
        self.skip_best = false;
        if self.completed >= laps {
            self.finish = Some(time);
        }
    }
    fn distance(&self, _course: &Course) -> f32 {
        self.distance
    }
}
#[derive(Serialize, Clone)]
pub struct Opponent {
    pub name: String,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub distance: f32,
    pub finished: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical: Option<CpuPose>,
}
#[derive(Serialize, Clone)]
pub struct CpuPose {
    pub orientation: [f32; 4],
    pub speed: f32,
    pub grounded: [bool; 4],
    pub wheel_compression: [f32; 4],
    pub wheel_angles: [f32; 4],
    pub quad_matrices: Vec<[f32; 16]>,
    pub rider_matrices: Vec<[f32; 16]>,
    pub rider_world_matrix: Option<[f32; 16]>,
    pub wreck: crate::wreck::State,
    pub rider_wreck: Option<crate::wreck_animation::Frame>,
    pub stunt: Option<crate::stunt::Frame>,
    pub reset_count: u32,
    pub decisions: crate::race_ai::Decisions,
}
#[derive(Serialize)]
pub struct Standing {
    pub name: String,
    pub position: usize,
    pub laps: u32,
    pub time: Option<f32>,
    pub best: Option<f32>,
    /// Race-over estimate for a rider who had not crossed the line.
    pub estimated: bool,
    /// Race distance along the racing line from the grid.
    pub progress: f32,
    /// Race stunt score (`001f1e30` reads rider +0x104; the port scores
    /// only the player's tricks, CPUs hold 0).
    pub stunt: f32,
    /// The human rider's row.
    pub player: bool,
}
#[derive(Serialize)]
pub struct Frame {
    pub phase: &'static str,
    pub countdown: u32,
    /// QuadzillaCountdownOverlay step (`00217590`): 0 all lamps off, 1–3 red
    /// lamps lit one per second, 4 green at GO, shown for 1s (`002e81f0`).
    pub lights: Option<u8>,
    pub elapsed: f32,
    pub lap: u32,
    pub laps: u32,
    pub lap_time: f32,
    pub best: Option<f32>,
    pub position: usize,
    pub field: usize,
    pub target: [f32; 3],
    pub checkpoint: usize,
    pub checkpoints: usize,
    /// Enduro "GATE %d of %d" (`002df0b0`): the player's next gate (1-based)
    /// and the gate count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enduro_gate: Option<[usize; 2]>,
    /// The next Enduro gate's position and direction (x, z): the compass
    /// shows it (`002e6e80`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enduro_next: Option<([f32; 3], [f32; 2])>,
    /// The player's heading (radians, atan2 of forward x and z).
    pub yaw: f32,
    /// Race distance from the grid to the finish gate and the TDF tag 41
    /// Track flags that shape lap counting.
    pub finish_at: Option<f32>,
    pub track_flags: u32,
    pub wrong_way: bool,
    /// Player outside the track polygon, with the on-track return point.
    pub off_track: bool,
    pub off_track_time: f32,
    pub track_point: [f32; 3],
    pub track_heading: f32,
    /// Rider-to-rider contacts resolved so far this race.
    pub contacts: u32,
    pub opponents: Vec<Opponent>,
    pub standings: Vec<Standing>,
    /// The player's completed lap times and the best one's index.
    pub lap_times: Vec<f32>,
    pub best_lap: Option<usize>,
    /// Original LEAD (`00248b20` +0xec): positive seconds ahead of the next
    /// rider when leading, negative seconds behind the leader otherwise.
    pub lead: f32,
    /// Rank of each rider slot: the player, then each CPU.
    pub ranks: Vec<usize>,
    /// Original start slot (1…n, `001f6350`) of the player, then each CPU.
    pub grid_slots: Vec<u32>,
    /// Original race overlays, attached by the simulation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hud: Option<crate::race_hud::Frame>,
    /// Stadium crowd cue, attached by the simulation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crowd: Option<crate::race_crowd::CrowdFrame>,
    /// National drop gate world matrix, attached by the simulation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<[f32; 16]>,
    /// A championship round's context, attached by the simulation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub championship: Option<crate::championship::Round>,
}
pub struct Race {
    pub course: Course,
    pub laps: u32,
    pub countdown: f32,
    /// The pre-race camera holds the countdown until it requests it.
    intro: bool,
    pub elapsed: f32,
    player: Progress,
    /// The player's grid origin and heading in its start slot.
    player_grid: ([f32; 3], f32),
    grid_slots: Vec<u32>,
    opponents: Vec<Opponent>,
    progress: Vec<Progress>,
    pub contacts: u32,
    /// Riders whose finish time was estimated at race over.
    estimated: Vec<bool>,
    random: crate::engine_audio::OriginalRandom,
    /// Finish-line crossings since the last `take_finishes` (`00216190`):
    /// (is the player, finishing place). Race-over estimates do not cross.
    finishes: Vec<(bool, usize)>,
    /// The player's committed stunt points when the race went live.
    stunt_base: f32,
}
impl Race {
    /// A race with the riders in slot order (player in slot 1), as when the
    /// original skips the shuffle (settings +0x7d8).
    pub fn new(course: Course, laps: u32, names: Vec<String>, difficulty: u32) -> Result<Self> {
        let slots = (1..=names.len() as u32 + 1).collect();
        Self::with_slots(course, laps, names, difficulty, slots)
    }
    /// A race whose start slots are drawn by the original shuffle from a
    /// per-race seed.
    pub fn seeded(
        course: Course,
        laps: u32,
        names: Vec<String>,
        difficulty: u32,
        seed: u64,
    ) -> Result<Self> {
        let mut random = crate::engine_audio::OriginalRandom::with_seed(seed);
        let slots = start_slots(names.len() + 1, &mut random);
        Self::with_slots(course, laps, names, difficulty, slots)
    }
    /// `slots[0]` is the player's start slot, then one per CPU: a
    /// permutation of 1…n.
    pub fn with_slots(
        course: Course,
        laps: u32,
        names: Vec<String>,
        difficulty: u32,
        slots: Vec<u32>,
    ) -> Result<Self> {
        if course.points.len() < 4
            || course.points.len() > 4096
            || course.widths.len() != course.points.len()
            || course.points.iter().flatten().any(|v| !v.is_finite())
            || course.widths.iter().any(|w| !w.is_finite() || *w < 1.)
            || course.loop_start + 3 > course.points.len()
            || course.finish >= course.points.len()
            || !(1. ..=40.).contains(&course.rider_spacing)
        {
            return Err("invalid race course".into());
        }
        if !(1..=5).contains(&laps)
            || names.len() > 4
            || difficulty > 2
            || names.iter().any(|n| n.len() > 40)
        {
            return Err("invalid race setup".into());
        }
        let riders = names.len() + 1;
        let mut sorted = slots.clone();
        sorted.sort_unstable();
        if sorted != (1..=riders as u32).collect::<Vec<_>>() {
            return Err("invalid race start slots".into());
        }
        let player_grid = course.grid(slots[0], riders);
        let opponents: Vec<_> = names
            .into_iter()
            .zip(&slots[1..])
            .map(|(name, &slot)| {
                let (origin, yaw) = course.grid(slot, riders);
                Opponent {
                    name,
                    origin,
                    yaw,
                    distance: 0.,
                    finished: false,
                    physical: None,
                }
            })
            .collect();
        let progress: Vec<_> = opponents
            .iter()
            .map(|o| Progress::new(o.origin, &course))
            .collect();
        let player = Progress::new(player_grid.0, &course);
        Ok(Self {
            course,
            laps,
            // `0020f780`: 4.0s, a step each second, live at the end.
            countdown: COUNTDOWN,
            intro: false,
            elapsed: 0.,
            player,
            player_grid,
            grid_slots: slots,
            opponents,
            estimated: vec![false; progress.len() + 1],
            progress,
            contacts: 0,
            stunt_base: 0.,
            random: crate::engine_audio::OriginalRandom::with_seed(97211 + 5 * 47),
            finishes: Vec::new(),
        })
    }
    /// The player's grid origin and heading in its start slot.
    pub fn player_grid(&self) -> ([f32; 3], f32) {
        self.player_grid
    }
    /// Start slot of the player, then each CPU.
    pub fn grid_slots(&self) -> &[u32] {
        &self.grid_slots
    }
    /// The intro and countdown hold every racer on the grid; physics still
    /// settles.
    pub fn holding(&self) -> bool {
        self.intro || self.countdown > 0.
    }
    pub fn in_intro(&self) -> bool {
        self.intro
    }
    /// Hold the countdown for the pre-race camera.
    pub fn begin_intro(&mut self) {
        if self.countdown == COUNTDOWN && self.elapsed == 0. {
            self.intro = true;
        }
    }
    pub fn start_countdown(&mut self) {
        self.intro = false;
    }
    /// The player has finished; their quad continues under CPU control.
    /// The player's committed stunt points this step: the race stunt score
    /// counts what was scored after GO and stops at the finish.
    pub fn record_player_stunt(&mut self, committed: f32) {
        if self.intro || self.countdown > 0. {
            self.stunt_base = committed;
        } else if self.player.finish.is_none() {
            self.player.stunt = (committed - self.stunt_base).max(0.);
        }
    }
    pub fn player_finished(&self) -> bool {
        self.player.finish.is_some()
    }
    pub fn step(&mut self, p: [f32; 3], dt: f32) {
        if self.intro {
            self.player.last = p;
            return;
        }
        if self.countdown > 0. {
            self.countdown = (self.countdown - dt).max(0.);
            self.player.last = p;
            return;
        }
        if self.player.finish.is_some() {
            self.player
                .update(p, &self.course, self.elapsed, self.laps, dt);
            return;
        }
        self.elapsed += dt;
        self.player
            .update(p, &self.course, self.elapsed, self.laps, dt);
        if self.player.finish.is_some() {
            self.finishes.push((true, self.finished_count()));
            self.race_over();
        } else if self.course.is_enduro()
            && self
                .progress
                .iter()
                .filter_map(|p| p.finish)
                .any(|first| self.elapsed - first >= ENDURO_FINISH_TIMEOUT)
        {
            // Enduro RaceStatus (`0020f780` +0x94): the race ends for everyone
            // 30s after the first rider finishes.
            self.race_over();
        }
    }
    /// RaceStatus `00248b20` race over, once every human has finished: the
    /// game does not wait for the CPUs. Each unfinished rider's gap behind
    /// the leader is its elapsed time less the leader's time scaled by its
    /// share of the leader's distance (+0xec). Its total becomes the leader's
    /// time plus that gap, its last lap (gap + lap start) / laps remaining,
    /// its best lap no slower than that or than total / laps. Times are then
    /// made increasing down the running order with a random under 1s.
    fn race_over(&mut self) {
        let elapsed = self.elapsed;
        let laps = self.laps;
        let mut order: Vec<usize> = (0..=self.progress.len()).collect();
        order.sort_by(|&a, &b| {
            let (a, b) = (self.racer(a), self.racer(b));
            match (a.finish, b.finish) {
                (Some(a), Some(b)) => a.total_cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => b.distance.total_cmp(&a.distance),
            }
        });
        let leader = self.racer(order[0]);
        let (leader_time, leader_distance) = (leader.finish.unwrap_or(elapsed), leader.distance);
        let pace = if leader_distance > 0. {
            leader_time / leader_distance
        } else {
            0.
        };
        for &i in &order {
            let p = self.racer_mut(i);
            if p.finish.is_some() {
                continue;
            }
            let gap = (elapsed - pace * p.distance).max(0.);
            let total = leader_time + gap;
            let last = (gap + p.lap_start) / laps.saturating_sub(p.completed).max(1) as f32;
            let mut best = p
                .best
                .map_or(last, |b| if last < b || b == 0. { last } else { b });
            if total < laps as f32 * best {
                best = total / laps as f32;
            }
            p.last_lap = last;
            p.best = Some(best);
            p.completed = laps;
            p.finish = Some(total);
            self.estimated[i] = true;
        }
        for k in 1..order.len() {
            let previous = self.racer(order[k - 1]).finish.unwrap_or(elapsed);
            let jitter = self.random.next_u31() as f32 * 4.656613e-10;
            let p = self.racer_mut(order[k]);
            if p.finish.is_some_and(|t| t < previous) {
                p.finish = Some(previous + jitter);
            }
        }
        for (o, p) in self.opponents.iter_mut().zip(&self.progress) {
            o.finished = p.finish.is_some();
        }
    }
    pub fn opponents(&self) -> &[Opponent] {
        &self.opponents
    }
    /// RaceStatus gaps (`00248b20`): riders ordered by race distance; each
    /// unfinished rider behind the leader is its elapsed time less the
    /// leader's time scaled by its share of the leader's distance (≥ 0); the
    /// leader's is minus the second rider's. LEAD shows minus the player's.
    fn lead(&self) -> f32 {
        let mut order: Vec<usize> = (0..=self.progress.len()).collect();
        order.sort_by(|&a, &b| self.racer(b).distance.total_cmp(&self.racer(a).distance));
        let leader = self.racer(order[0]);
        let pace = if leader.distance > 0. {
            leader.finish.unwrap_or(self.elapsed) / leader.distance
        } else {
            0.
        };
        let gap = |i: usize| {
            let r = self.racer(i);
            (self.elapsed - pace * r.distance).max(0.)
        };
        let player = if order[0] == 0 {
            -order.get(1).map_or(0., |&i| gap(i))
        } else {
            gap(0)
        };
        -player
    }
    fn finished_count(&self) -> usize {
        std::iter::once(&self.player)
            .chain(&self.progress)
            .filter(|p| p.finish.is_some())
            .count()
    }
    pub fn laps_completed(&self, cpu: Option<usize>) -> u32 {
        self.rider(cpu).completed
    }
    pub fn take_finishes(&mut self) -> Vec<(bool, usize)> {
        std::mem::take(&mut self.finishes)
    }
    /// Racer 0 is the player, then each CPU.
    fn racer(&self, i: usize) -> &Progress {
        if i == 0 {
            &self.player
        } else {
            &self.progress[i - 1]
        }
    }
    fn racer_mut(&mut self, i: usize) -> &mut Progress {
        if i == 0 {
            &mut self.player
        } else {
            &mut self.progress[i - 1]
        }
    }
    /// The race clock runs between GO and the player's finish.
    pub fn running(&self) -> bool {
        self.countdown == 0. && self.elapsed > 0. && self.player.finish.is_none()
    }
    pub fn progress_difference(&self, index: usize) -> f32 {
        self.progress[index].distance(&self.course) - self.player.distance(&self.course)
    }
    /// Reposition cannot count traveled distance or award a swept checkpoint.
    pub fn reposition_cpu(
        &mut self,
        index: usize,
        origin: [f32; 3],
        yaw: f32,
        physical: Option<CpuPose>,
    ) {
        let opponent = &mut self.opponents[index];
        opponent.origin = origin;
        opponent.yaw = yaw;
        opponent.physical = physical;
        self.progress[index].last = origin;
    }
    pub fn observe_cpu(
        &mut self,
        index: usize,
        origin: [f32; 3],
        yaw: f32,
        physical: Option<CpuPose>,
    ) {
        let o = &mut self.opponents[index];
        let p = &mut self.progress[index];
        if p.finish.is_none() {
            o.distance += distance(o.origin, origin);
        }
        o.origin = origin;
        o.yaw = yaw;
        o.physical = physical;
        let was = p.finish.is_some();
        p.update(origin, &self.course, self.elapsed, self.laps, 1. / 60.);
        o.finished = p.finish.is_some();
        if !was && o.finished {
            let place = self.finished_count();
            self.finishes.push((false, place));
        }
    }
    fn rider(&self, cpu: Option<usize>) -> &Progress {
        cpu.map_or(&self.player, |i| &self.progress[i])
    }
    /// Last on-track station of the player (None) or a CPU.
    pub fn station(&self, cpu: Option<usize>) -> Station {
        self.rider(cpu).station
    }
    pub fn off_track_time(&self, cpu: Option<usize>) -> f32 {
        self.rider(cpu).off_track
    }
    /// Race reset pose on the racing line, facing the race direction.
    pub fn checkpoint(&self, cpu: Option<usize>) -> ([f32; 3], f32) {
        let s = self.rider(cpu).station;
        (self.course.point_at(s), self.course.heading_at(s))
    }
    pub fn frame(&self, yaw: f32) -> Frame {
        let mut order: Vec<_> =
            std::iter::once(("YOU".to_string(), &self.player, self.estimated[0]))
                .chain(
                    self.opponents
                        .iter()
                        .zip(&self.progress)
                        .zip(&self.estimated[1..])
                        .map(|((o, p), e)| (o.name.clone(), p, *e)),
                )
                .collect();
        order.sort_by(|a, b| match (a.1.finish, b.1.finish) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => {
                b.1.distance(&self.course)
                    .total_cmp(&a.1.distance(&self.course))
            }
        });
        let position = order
            .iter()
            .position(|(_, p, _)| std::ptr::eq(*p, &self.player))
            .unwrap()
            + 1;
        let station = self.player.station;
        let target = match self.course.enduro.get(self.player.next_gate) {
            Some(gate) => gate.center,
            None => self.course.point_at(self.course.advance(station, 60.)),
        };
        let heading = self.course.heading_at(station);
        Frame {
            phase: if self.intro {
                "intro"
            } else if self.countdown > 0. {
                "countdown"
            } else if self.player.finish.is_some() {
                "finished"
            } else {
                "racing"
            },
            countdown: self.countdown.ceil() as u32,
            lights: if self.intro {
                None
            } else if self.countdown > 0. {
                Some(((COUNTDOWN - self.countdown) as u8).min(3))
            } else if self.elapsed < 1. {
                Some(4)
            } else {
                None
            },
            elapsed: self.elapsed,
            lap: (self.player.completed + 1).min(self.laps),
            laps: self.laps,
            lap_time: if self.player.finish.is_some() {
                self.player.last_lap
            } else {
                self.elapsed - self.player.lap_start
            },
            best: self.player.best,
            position,
            field: order.len(),
            target,
            checkpoint: station.segment,
            checkpoints: self.course.points.len(),
            enduro_gate: self
                .course
                .is_enduro()
                .then(|| [self.player.next_gate + 1, self.course.enduro.len()]),
            enduro_next: self
                .course
                .enduro
                .get(self.player.next_gate)
                .map(|g| (g.center, [g.forward[0], g.forward[2]])),
            yaw,
            finish_at: self.course.finish_at,
            track_flags: self.course.track_flags,
            wrong_way: !self.course.is_enduro()
                && self.countdown == 0.
                && self.player.finish.is_none()
                && self.player.off_track < 0.5
                && (heading - yaw).cos_d() < -0.4,
            off_track: !self.course.is_enduro() && self.player.off_track > 0.5,
            off_track_time: self.player.off_track,
            track_point: self.course.point_at(station),
            track_heading: heading,
            contacts: self.contacts,
            lap_times: self.player.lap_times.clone(),
            best_lap: self
                .player
                .best
                .and_then(|b| self.player.lap_times.iter().position(|t| *t == b)),
            lead: self.lead(),
            ranks: std::iter::once(&self.player)
                .chain(&self.progress)
                .map(|p| {
                    order
                        .iter()
                        .position(|(_, q, _)| std::ptr::eq(*q, p))
                        .map_or(0, |i| i + 1)
                })
                .collect(),
            grid_slots: self.grid_slots.clone(),
            hud: None,
            crowd: None,
            gate: None,
            championship: None,
            opponents: self.opponents.clone(),
            standings: order
                .into_iter()
                .enumerate()
                .map(|(i, (name, p, estimated))| Standing {
                    name,
                    position: i + 1,
                    laps: p.completed,
                    time: p.finish,
                    best: p.best,
                    estimated,
                    progress: p.distance,
                    stunt: p.stunt,
                    player: std::ptr::eq(p, &self.player),
                })
                .collect(),
        }
    }
}
/// Original grid RiderSpacing: the first of the race mode TOY, then (for
/// Supercross) the scene TOY, whose `RiderSpacing=` value lies in 1–40 ft.
/// Reads /^\s*RiderSpacing\s*=\s*([\d.]+)/im as the former browser did.
pub fn rider_spacing<'a>(sources: impl IntoIterator<Item = &'a str>) -> Option<f32> {
    use crate::authored_text::{assignment, js_number, line_starts, skip_space, starts_with_key};
    sources.into_iter().find_map(|text| {
        let value = line_starts(text).find_map(|start| {
            let key = skip_space(text, start);
            let value = starts_with_key(text, key, "RiderSpacing")
                .then(|| assignment(text, key, "RiderSpacing"))
                .flatten()?;
            let end = value
                + text[value..]
                    .find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(text.len() - value);
            (end > value).then(|| js_number(&text[value..end]))
        })?;
        (value.is_finite() && (1. ..=40.).contains(&value)).then_some(value as f32)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_slots_shuffle_pairs_like_001f6350() {
        use crate::engine_audio::OriginalRandom;
        assert_eq!(start_slots(1, &mut OriginalRandom::with_seed(3)), vec![1]);
        let mut player = [0; 5];
        for seed in 0..400 {
            let slots = start_slots(5, &mut OriginalRandom::with_seed(seed));
            // Reference: the same draw order, one draw per pair i < j.
            let mut random = OriginalRandom::with_seed(seed);
            let mut table = [1, 2, 3, 4, 5];
            for i in 0..4 {
                for j in i + 1..5 {
                    if random.next_u31() as f32 / 2_147_483_648. > 0.5 {
                        table.swap(i, j);
                    }
                }
            }
            assert_eq!(slots, table);
            let mut sorted = slots.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, [1, 2, 3, 4, 5]);
            player[slots[0] as usize - 1] += 1;
        }
        // The human can start in any slot.
        assert!(player.iter().all(|&n| n > 20), "{player:?}");
    }
    #[test]
    fn grid_rows_follow_001f8410() {
        // Moving +z then turning toward +x: positive cross y, sign −1.
        assert_eq!(grid_sign(&[[0.; 3], [0., 0., 10.], [10., 0., 20.]]), -1.);
        assert_eq!(grid_sign(&[[0.; 3], [0., 0., 10.], [-10., 0., 20.]]), 1.);
        assert_eq!(grid_sign(&[[0.; 3], [0., 0., 10.], [0., 0., 20.]]), 1.);
        assert_eq!(grid_sign(&[[0.; 3], [0., 0., 10.]]), -1.);
        let mut c = Course::parse(&graph_course()).unwrap();
        c.rider_spacing = 10.;
        let (start, yaw) = c.start();
        for sign in [-1., 1.] {
            c.grid_sign = sign;
            // Five riders: slot − 2.5 spacings across, one row behind the gate.
            for slot in 1..=5 {
                let (p, y) = c.grid(slot, 5);
                assert_eq!(y, yaw);
                assert_eq!(p[2], start[2]);
                assert!((p[0] - (start[0] + sign * (slot as f32 - 2.5) * 10.)).abs() < 1e-4);
            }
        }
        let race =
            Race::with_slots(c.clone(), 1, vec!["A".into(), "B".into()], 1, vec![3, 1, 2]).unwrap();
        assert_eq!(race.player_grid(), c.grid(3, 3));
        assert_eq!(race.opponents()[0].origin, c.grid(1, 3).0);
        assert_eq!(race.opponents()[1].origin, c.grid(2, 3).0);
        assert_eq!(race.frame(0.).grid_slots, vec![3, 1, 2]);
        for bad in [vec![1, 1, 2], vec![1, 2], vec![0, 1, 2], vec![1, 2, 4]] {
            assert!(Race::with_slots(c.clone(), 1, vec!["A".into(), "B".into()], 1, bad).is_err());
        }
        let seeded = Race::seeded(c, 1, vec!["A".into(), "B".into()], 1, 9).unwrap();
        let expected = start_slots(3, &mut crate::engine_audio::OriginalRandom::with_seed(9));
        assert_eq!(seeded.grid_slots(), expected.as_slice());
    }
    #[test]
    fn rider_spacing_reads_the_first_valid_authored_toy() {
        // Former browser expression results for the same text.
        assert_eq!(
            rider_spacing(["[Race]\n  RiderSpacing = 12.5 ;ft", "RiderSpacing=8"]),
            Some(12.5)
        );
        assert_eq!(rider_spacing(["", "riderspacing=\n9"]), Some(9.));
        assert_eq!(
            rider_spacing(["RiderSpacing=8.5.1", "RiderSpacing=41", "x RiderSpacing=3"]),
            None
        );
        assert_eq!(
            rider_spacing(["RiderSpacing=0.5\nRiderSpacing=10", "RiderSpacing=15"]),
            Some(15.)
        );
    }
    #[test]
    #[ignore = "requires privately extracted original TDF courses"]
    fn every_original_race_course_parses() {
        let dir = std::env::var("ATV_RACE_COURSE_DIR").expect("set ATV_RACE_COURSE_DIR");
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with("nate.tdf"))
            .collect();
        names.sort();
        assert_eq!(names.len(), 32);
        for path in names {
            let course = Course::parse(&std::fs::read(&path).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let (start, _) = course.start();
            let gate = course.start_gate.expect("original start gate");
            eprintln!(
                "{} sections={} points={} loop={} finish={} grace={} branches={:?} lap={:.0} first={:.0} start_offset={:.1}",
                path.file_name().unwrap().to_string_lossy(),
                course.sections,
                course.points.len(),
                course.loop_start,
                course.finish,
                course.finish_grace,
                course.branches,
                course.cycle_length(),
                course.route_length(0, course.finish),
                distance(start, gate.center),
            );
            // 001f8410 rows of five at 8ft and 15ft RiderSpacing: slots
            // inside the track polygon (or the start apron).
            let on_track = |spacing: f32| {
                let mut c = course.clone();
                c.rider_spacing = spacing;
                let origin = Station {
                    segment: 0,
                    fraction: 0.,
                };
                (1..=5)
                    .filter(|&slot| c.locate(c.grid(slot, 5).0, origin, false).is_some())
                    .count()
            };
            eprintln!(
                "  track_flags={:#x} finish_at={:?} grid_at={:.1}",
                course.track_flags,
                course.finish_at,
                course.station_distance(Progress::new(start, &course).station)
            );
            eprintln!(
                "  grid_sign={} on_track(8ft)={}/5 on_track(15ft)={}/5",
                course.grid_sign,
                on_track(8.),
                on_track(15.)
            );
            assert!(course.finish_gate.is_some());
            assert!(course.loop_start + 3 <= course.points.len());
            assert!(course.cycle_length() > 300.);
            // Drive the main racing line in 20ft steps for a two-lap race.
            let mut race = Race::new(course.clone(), 2, vec!["CPU".into()], 1).unwrap();
            race.countdown = 0.;
            let mut at = start;
            let mut index = 0;
            let mut travelled = 0.;
            while race.player.finish.is_none() && travelled < 4. * course.cycle_length() + 1000. {
                let target = course.points[course.next(index)];
                let gap = distance(at, target);
                if gap <= 20. {
                    at = target;
                    index = course.next(index);
                    travelled += gap;
                } else {
                    at = std::array::from_fn(|n| at[n] + (target[n] - at[n]) * 20. / gap);
                    travelled += 20.;
                }
                race.step(at, 0.1);
            }
            assert!(
                race.player.finish.is_some(),
                "{} did not finish",
                path.display()
            );
            // Two laps end at the second counted finish-gate crossing ahead of
            // the grid; Track flag 4 skips the first crossing.
            let at = course
                .finish_at
                .expect("racing line crosses the finish gate");
            let grid = Progress::new(start, &course).station;
            let first = at - course.station_distance(grid);
            let first = if first < 0. {
                first + course.cycle_length()
            } else {
                first
            };
            let skipped = Progress::new(start, &course).skip_crossing;
            let expected = first + f32::from(1 + u8::from(skipped)) * course.cycle_length();
            // Lap-to-lap crossings move a little where a start straight joins
            // the circuit.
            assert!(
                (travelled - expected).abs() < 150.,
                "{} {travelled} vs {expected}",
                path.display()
            );
            // Every alternate line stays on the track and rejoins in order.
            let label = path.file_name().unwrap().to_string_lossy().to_string();
            for alt in &course.alternates {
                let mut race = Race::new(course.clone(), 3, vec![], 1).unwrap();
                race.countdown = 0.;
                let mut route: Vec<[f32; 3]> = vec![];
                let mut i = 0;
                while i != alt.from {
                    i = course.next(i);
                    route.push(course.points[i]);
                }
                route.extend(alt.points.iter().skip(1));
                let mut at = start;
                for target in route {
                    loop {
                        let gap = distance(at, target);
                        at = if gap <= 10. {
                            target
                        } else {
                            std::array::from_fn(|n| at[n] + (target[n] - at[n]) * 10. / gap)
                        };
                        race.step(at, 0.05);
                        assert!(
                            race.player.off_track < 0.3,
                            "{label} alternate {}->{} left the track at {at:?}",
                            alt.from,
                            alt.to
                        );
                        if gap <= 10. {
                            break;
                        }
                    }
                }
                let reached = race.player.station.segment;
                assert!(
                    distance(course.point_at(race.player.station), course.points[alt.to]) < 40.,
                    "{label} alternate {}->{} ended at {reached}",
                    alt.from,
                    alt.to
                );
            }
            eprintln!("{label}: {} alternates driven", course.alternates.len());
        }
    }
    #[test]
    fn alternate_overlap_outside_the_route_window_is_off_track() {
        // A square circuit with an alternate across the middle from
        // checkpoint 1 to 5, like sx14's 12->57 line crossing a later part
        // of the course.
        let mut c = Course {
            points: vec![
                [0., 0., 0.],
                [0., 0., 100.],
                [0., 0., 200.],
                [100., 0., 200.],
                [200., 0., 200.],
                [200., 0., 100.],
                [200., 0., 0.],
                [100., 0., 0.],
            ],
            widths: vec![20.; 8],
            ..Course::default()
        };
        let across = |x: f32| [[x, 0., 110.], [x, 0., 90.]];
        c.alternates.push(Alternate {
            from: 1,
            to: 5,
            points: vec![[0., 0., 100.], [100., 0., 100.], [200., 0., 100.]],
            edges: vec![across(0.), across(100.), across(200.)],
        });
        let near_rejoin = Station {
            segment: 4,
            fraction: 0.8,
        };
        // Near the rejoin the alternate is inside the window.
        let late = c.locate([170., 0., 105.], near_rejoin, true).unwrap();
        assert!(late.offset > -75. && late.offset < 0., "{late:?}");
        // Its start lies about 320ft behind the rider's station: not found.
        assert!(c.locate([30., 0., 105.], near_rejoin, true).is_none());
    }
    #[test]
    fn rider_off_the_line_counts_a_skewed_finish_gate() {
        // Gate on the west side, turned 30° from square: the racing line
        // (x = 0) meets it 50ft from the grid, a rider at x = -10 about
        // 5.8ft later, when its race distance is already past the gate.
        let mut c = course();
        let (sin, cos) = (0.5, 0.866_025_4);
        let matrix = [
            cos, 0., -sin, 0., 0., 1., 0., 0., sin, 0., cos, 0., 0., 0., 50., 1.,
        ];
        assert!(c.set_finish_gate(&matrix, 15.));
        assert!((c.finish_at.unwrap() - 50.).abs() < 0.01);
        let mut r = Race::new(c, 1, vec![], 0).unwrap();
        r.countdown = 0.;
        for z in 0..=70 {
            r.step([-10., 0., z as f32], 0.1);
        }
        assert!(r.player_finished(), "{}", r.player.distance);
    }
    fn course() -> Course {
        Course {
            points: vec![
                [0., 0., 0.],
                [0., 0., 100.],
                [100., 0., 100.],
                [100., 0., 0.],
            ],
            widths: vec![30.; 4],
            ..Course::default()
        }
    }
    /// Square course with gates at the middle of three sides; the finish
    /// gate is on the west side heading south.
    fn enduro_course() -> Course {
        let mut c = course();
        let w = |x: f32, z: f32, dx: f32, dz: f32| crate::enduro::Waypoint {
            x,
            z,
            direction: [dx, dz],
        };
        let gates = crate::enduro::gates(
            &[
                w(0., 50., 0., 1.),
                w(50., 100., 1., 0.),
                w(100., 50., 0., -1.),
            ],
            15.,
            |_, _| 0.,
        )
        .unwrap();
        c.set_enduro(gates).unwrap();
        c
    }
    #[test]
    fn enduro_gates_count_in_order_and_last_gate_laps() {
        let c = enduro_course();
        // `001f8410`: 10ft past the finish gate, facing its direction.
        let (start, yaw) = c.start();
        assert_eq!(start, [100., 0., 40.]);
        assert!((yaw.abs() - std::f32::consts::PI).abs() < 1e-3);
        let mut r = Race::new(c, 2, vec!["CPU".into()], 1).unwrap();
        r.countdown = 0.;
        let step = |r: &mut Race, p: [f32; 3]| r.step(p, 1.);
        step(&mut r, [100., 0., 40.]);
        assert_eq!(r.frame(0.).enduro_gate, Some([1, 3]));
        // Missing gate 1 (passing beside it) and crossing gate 2 do nothing.
        step(&mut r, [-30., 0., 45.]);
        step(&mut r, [-30., 0., 55.]);
        step(&mut r, [55., 0., 95.]);
        step(&mut r, [45., 0., 105.]);
        assert_eq!(r.frame(0.).enduro_gate, Some([1, 3]));
        // Gate 1 counts against its direction too (flags 5).
        step(&mut r, [0., 0., 55.]);
        step(&mut r, [0., 0., 45.]);
        assert_eq!(r.frame(0.).enduro_gate, Some([2, 3]));
        let before = r.frame(0.).standings[0].progress;
        step(&mut r, [45., 0., 100.]);
        step(&mut r, [55., 0., 100.]);
        step(&mut r, [100., 0., 55.]);
        assert_eq!(r.laps_completed(None), 0);
        step(&mut r, [100., 0., 45.]);
        let f = r.frame(0.);
        assert_eq!(r.laps_completed(None), 1);
        assert_eq!(f.enduro_gate, Some([1, 3]));
        assert_eq!(f.lap_times, [11.]);
        assert!(!f.wrong_way && !f.off_track);
        let player = f.standings.iter().find(|s| s.player).unwrap();
        assert!(player.progress > before, "race distance follows the gates");
        assert_eq!(player.position, 1);
        for p in [
            [0., 0., 55.],
            [0., 0., 45.],
            [45., 0., 100.],
            [55., 0., 100.],
            [100., 0., 55.],
            [100., 0., 45.],
        ] {
            step(&mut r, p);
        }
        assert!(r.player_finished());
    }
    #[test]
    fn enduro_race_ends_thirty_seconds_after_the_first_finisher() {
        let mut r = Race::new(enduro_course(), 1, vec!["CPU".into()], 1).unwrap();
        r.countdown = 0.;
        r.step([100., 0., 40.], 1.);
        for p in [
            [0., 0., 55.],
            [0., 0., 45.],
            [45., 0., 100.],
            [55., 0., 100.],
            [100., 0., 55.],
            [100., 0., 45.],
        ] {
            r.step([100., 0., 40.], 1.);
            r.observe_cpu(0, p, 0., None);
        }
        assert!(r.opponents()[0].finished && !r.player_finished());
        for _ in 0..29 {
            r.step([100., 0., 40.], 1.);
        }
        assert!(!r.player_finished());
        r.step([100., 0., 40.], 1.);
        assert!(r.player_finished(), "race over 30s after the CPU finished");
        let f = r.frame(0.);
        assert!(f.standings.iter().find(|s| s.player).unwrap().estimated);
    }
    #[test]
    fn scene_finish_gate_replaces_the_tdf_box_on_the_route() {
        let mut c = course();
        // A gate model across the first side, facing +z (00215900).
        let mut m = [0f32; 16];
        (m[0], m[5], m[10], m[15]) = (1., 1., 1., 1.);
        (m[12], m[13], m[14]) = (0., 0., 50.);
        assert!(c.set_finish_gate(&m, 15.));
        assert_eq!(c.finish_at, Some(50.));
        let g = c.finish_gate.unwrap();
        assert_eq!((g.half_width, g.forward), ([15., 0., 0.], [0., 0., 1.]));
        // A model the racing line does not cross keeps the box.
        m[12] = 500.;
        assert!(!c.set_finish_gate(&m, 15.));
        assert_eq!(c.finish_at, Some(50.));
        assert!(!c.set_finish_gate(&m, f32::NAN));
    }
    #[test]
    fn countdown_then_finish_hand_over_the_player() {
        let mut r = Race::new(course(), 1, vec![], 1).unwrap();
        r.begin_intro();
        assert!(r.holding() && r.in_intro());
        r.step([0.; 3], 1.);
        assert_eq!(r.countdown, 4., "the intro holds the countdown");
        assert_eq!(r.frame(0.).phase, "intro");
        assert_eq!(r.frame(0.).lights, None);
        r.start_countdown();
        assert_eq!(r.frame(0.).lights, Some(0));
        r.step([0.; 3], 1.5);
        assert_eq!(r.frame(0.).lights, Some(1));
        r.countdown = 4.;
        assert!(r.holding() && !r.player_finished());
        for _ in 0..241 {
            r.step([0.; 3], 1. / 60.);
        }
        assert!(!r.holding() && !r.player_finished());
        assert_eq!(r.frame(0.).lights, Some(4), "green at GO");
        r.step([0.; 3], 1.);
        assert_eq!(r.frame(0.).lights, None, "lamps hide after 1s");
        r.elapsed = 0.;
        for p in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]] {
            r.step(p, 1.);
        }
        assert!(r.player_finished() && !r.running());
        assert_eq!(r.frame(0.).phase, "finished");
        assert_eq!(r.frame(0.).best, Some(4.));
        // After the finish the station follows the CPU-driven player.
        r.step([0., 0., 60.], 1.);
        assert_eq!(r.station(None).segment, 0);
        assert!(r.station(None).fraction > 0.5);
        assert_eq!(r.elapsed, 4.);
    }
    #[test]
    fn race_stunt_score_counts_from_go_to_the_finish() {
        let mut r = Race::new(course(), 1, vec!["Cpu".into()], 1).unwrap();
        // Points held before GO are not race points.
        r.record_player_stunt(500.);
        r.countdown = 0.;
        r.record_player_stunt(2500.);
        for p in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]] {
            r.step(p, 1.);
        }
        assert!(r.player_finished());
        r.record_player_stunt(9000.);
        let f = r.frame(0.);
        let you = f.standings.iter().find(|s| s.player).unwrap();
        assert_eq!((you.name.as_str(), you.stunt), ("YOU", 2000.));
        assert!(
            f.standings
                .iter()
                .filter(|s| !s.player)
                .all(|s| s.stunt == 0.)
        );
    }
    #[test]
    fn race_over_estimates_unfinished_cpus_from_the_leader_pace() {
        let mut r = Race::new(course(), 1, vec!["Half".into(), "Quarter".into()], 1).unwrap();
        r.countdown = 0.;
        // The CPUs reach 200ft and 100ft as the player completes 400ft in 4s.
        let half = [
            [0., 0., 100.],
            [100., 0., 100.],
            [100., 0., 100.],
            [100., 0., 100.],
        ];
        let quarter = [
            [0., 0., 50.],
            [0., 0., 100.],
            [0., 0., 100.],
            [0., 0., 100.],
        ];
        for (i, p) in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]]
            .into_iter()
            .enumerate()
        {
            r.observe_cpu(0, half[i], 0., None);
            r.observe_cpu(1, quarter[i], 0., None);
            r.step(p, 1.);
        }
        assert!(r.player_finished());
        let f = r.frame(0.);
        assert!(f.opponents.iter().all(|o| o.finished));
        let names: Vec<_> = f.standings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["YOU", "Half", "Quarter"]);
        // Gap = elapsed − leader time × distance share; total = leader + gap.
        let half = &f.standings[1];
        assert!(half.estimated && !f.standings[0].estimated);
        assert!((half.time.unwrap() - 6.).abs() < 1e-4, "{:?}", half.time);
        assert!((f.standings[2].time.unwrap() - 7.).abs() < 1e-4);
        // Last lap (gap + lap start) / laps remaining; best is no slower.
        assert!((half.best.unwrap() - 2.).abs() < 1e-4);
        assert_eq!(f.standings[2].laps, 1);
        // The results no longer change while everyone keeps driving.
        r.observe_cpu(1, [100., 0., 100.], 0., None);
        assert!((r.frame(0.).standings[2].time.unwrap() - 7.).abs() < 1e-4);
    }
    #[test]
    fn lead_is_the_time_gap_at_the_leaders_pace() {
        let mut r = Race::new(course(), 2, vec!["CPU".into()], 1).unwrap();
        r.countdown = 0.;
        r.observe_cpu(0, [0., 0., 50.], 0., None);
        r.step([0., 0., 100.], 1.);
        r.observe_cpu(0, [0., 0., 100.], 0., None);
        r.step([100., 0., 100.], 1.);
        // Player leads at 200ft in 2s; the CPU at 100ft is 1s behind.
        let f = r.frame(0.);
        assert!((f.lead - 1.).abs() < 1e-4, "{}", f.lead);
        assert_eq!(f.ranks, vec![1, 2]);
        r.observe_cpu(0, [100., 0., 100.], 0., None);
        r.observe_cpu(0, [100., 0., 0.], 0., None);
        // Now the CPU leads at 300ft; the player at 200ft trails.
        let f = r.frame(0.);
        assert!(
            (f.lead - -(2. - 2. / 300. * 200.)).abs() < 1e-4,
            "{}",
            f.lead
        );
        assert_eq!(f.ranks, vec![2, 1]);
        r.step([100., 0., 0.], 1.);
        r.step([0.; 3], 1.);
        let f = r.frame(0.);
        assert_eq!(f.lap_times, vec![4.]);
        assert_eq!(f.best_lap, Some(0));
    }
    #[test]
    fn race_over_keeps_estimates_increasing_down_the_order() {
        let mut r = Race::new(course(), 1, vec!["A".into(), "B".into()], 1).unwrap();
        r.countdown = 0.;
        // B finished first by itself; A is close behind the player's line.
        for (i, p) in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]]
            .into_iter()
            .enumerate()
        {
            r.observe_cpu(1, p, 0., None);
            r.observe_cpu(
                0,
                [[0., 0., 99.], [99., 0., 100.], [100., 0., 2.], [2., 0., 0.]][i],
                0.,
                None,
            );
            r.step(
                [
                    [0., 0., 50.],
                    [0., 0., 100.],
                    [100., 0., 100.],
                    [100., 0., 0.],
                ][i],
                1.,
            );
        }
        r.step([0.; 3], 1.);
        let times: Vec<_> = r
            .frame(0.)
            .standings
            .iter()
            .map(|s| s.time.unwrap())
            .collect();
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
    }
    #[test]
    fn shortcuts_reverse_and_reset_do_not_finish() {
        let mut r = Race::new(course(), 1, vec![], 1).unwrap();
        r.countdown = 0.;
        for p in [[100., 0., 0.], [0.; 3], [5000.; 3], [0.; 3]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 0);
        assert!(r.player.furthest <= 0.);
    }
    #[test]
    fn leader_gap_uses_ordered_distance_at_start_and_lap_wrap() {
        let mut r = Race::new(course(), 2, vec!["CPU".into()], 1).unwrap();
        r.countdown = 0.;
        // A rider who drifts backwards over the start must not appear almost
        // one lap ahead, disabling the original CPU speed limiting policy.
        // (5, 0, 0) lies 5ft before the start on the closing segment.
        r.step([5., 0., 0.], 1.);
        assert_eq!(r.player.distance(&r.course), -5.);
        r.observe_cpu(0, [0., 0., 50.], 0., None);
        assert_eq!(r.progress_difference(0), 55.);
        for p in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.]] {
            r.step([5., 0., 0.], 1.);
            r.observe_cpu(0, p, 0., None);
        }
        r.observe_cpu(0, [30., 0., 0.], 0., None);
        assert_eq!(r.progress_difference(0), 375.);
        r.observe_cpu(0, [0.; 3], 0., None);
        assert_eq!(r.progress_difference(0), 405.);
        r.observe_cpu(0, [0., 0., 10.], 0., None);
        assert_eq!(r.progress_difference(0), 415.);
        // A reset to another segment cannot create a lap or course-length gap.
        r.step([100., 0., 0.], 1.);
        assert_eq!(r.player.completed, 0);
        assert!(r.player.distance(&r.course) <= 0.);
    }
    #[test]
    fn physical_cpus_require_observation() {
        let mut r = Race::new(course(), 1, vec!["CPU".into()], 1).unwrap();
        let start = r.opponents()[0].origin;
        r.countdown = 0.;
        r.step([0.; 3], 1. / 60.);
        assert_eq!(r.opponents()[0].origin, start);
        assert_eq!(r.opponents()[0].distance, 0.);
        assert!(r.opponents()[0].physical.is_none());
    }
    #[test]
    fn wreck_reposition_preserves_progress_without_a_swept_or_radius_award() {
        let mut r = Race::new(course(), 1, vec!["CPU".into()], 0).unwrap();
        r.countdown = 0.;
        r.step([0.; 3], 1.);
        let prior_distance = r.opponents[0].distance;
        let station = r.progress[0].station;
        r.reposition_cpu(0, [0., 0., 100.], 1., None);
        assert_eq!(r.progress[0].station, station);
        assert_eq!(r.progress[0].completed, 0);
        assert_eq!(r.opponents[0].distance, prior_distance);
        // Ordinary driving after the recovery earns progress on the track.
        r.observe_cpu(0, [0., 0., 101.], 1., None);
        assert!((r.progress[0].distance - 101.).abs() < 2.);
        assert_eq!(r.opponents[0].distance, prior_distance + 1.);
    }
    #[test]
    fn opponents_complete_laps_and_rank_by_finish_time() {
        let mut r = Race::new(course(), 1, vec!["Slow".into(), "Fast".into()], 2).unwrap();
        r.countdown = 0.;
        for (n, p) in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]]
            .into_iter()
            .enumerate()
        {
            r.step([0.; 3], 1.);
            r.observe_cpu(1, p, 0., None);
            if n > 0 {
                r.observe_cpu(
                    0,
                    [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.]][n - 1],
                    0.,
                    None,
                );
            }
        }
        r.step([0.; 3], 1.);
        r.observe_cpu(0, [0.; 3], 0., None);
        let f = r.frame(0.);
        assert!(f.opponents.iter().all(|o| o.finished));
        assert_eq!(f.position, 3);
        assert!(f.standings[0].time.unwrap() < f.standings[1].time.unwrap());
    }
    #[test]
    fn parses_closed_tagged_course_and_rejects_truncation_or_duplicate_fields() {
        fn field(out: &mut Vec<u8>, tag: u8, values: &[f32]) {
            out.push(tag);
            for v in values {
                out.extend(v.to_le_bytes());
            }
        }
        let mut data = vec![1, 1, 0, 0, 0];
        field(&mut data, 2, &[2304.]);
        field(&mut data, 3, &[2304.]);
        field(&mut data, 4, &[4.5]);
        let points = [
            [0., 0., 0.],
            [0., 0., 20.],
            [0., 0., 40.],
            [20., 0., 40.],
            [40., 0., 40.],
            [40., 0., 20.],
            [40., 0., 0.],
            [20., 0., 0.],
            [0., 0., 0.],
        ];
        for p in points {
            data.push(8);
            field(&mut data, 23, &[0.]);
            field(&mut data, 9, &p);
            field(&mut data, 10, &[p[0] - 3., 0., p[2]]);
            field(&mut data, 11, &[p[0] + 3., 0., p[2]]);
            for tag in [12, 18, 26, 27] {
                field(&mut data, tag, &[1.]);
            }
        }
        field(&mut data, 13, &[f32::from_bits(1)]);
        // Caribou's single section links back to itself.
        field(&mut data, 14, &[f32::from_bits(0), f32::from_bits(0)]);
        let c = Course::parse(&data).unwrap();
        assert_eq!((c.loop_start, c.finish, c.sections), (0, 0, 1));
        assert_eq!(c.points.len(), 8);
        assert_eq!(c.points[1], [2304., 0., 2394.]);
        assert_eq!(c.widths[0], 27.);
        assert_eq!(c.airtimes, vec![1.; 8]);
        assert_eq!(c.preload_scales, vec![1.; 8]);
        for length in 0..data.len() {
            assert!(Course::parse(&data[..length]).is_err());
        }
        data[26] = 10;
        assert!(Course::parse(&data).is_err());
    }
    /// Start straight (section 0) into a lap with a fork: section 2 is the
    /// main line, section 3 the 0x100 alternate; both rejoin at section 4.
    fn graph_course() -> Vec<u8> {
        fn field(out: &mut Vec<u8>, tag: u8, values: &[f32]) {
            out.push(tag);
            for v in values {
                out.extend(v.to_le_bytes());
            }
        }
        let int = f32::from_bits;
        type Section<'a> = (u32, &'a [[f32; 2]], &'a [u32]);
        let sections: [Section; 5] = [
            (1, &[[0., -100.], [0., -50.], [0., 0.]], &[1]),
            (1, &[[0., 0.], [0., 100.], [0., 200.]], &[2, 3]),
            (1, &[[0., 200.], [50., 250.], [100., 200.]], &[4]),
            (0x101, &[[0., 200.], [50., 150.], [100., 200.]], &[4]),
            (
                1,
                &[
                    [100., 200.],
                    [100., 100.],
                    [100., 0.],
                    [50., -20.],
                    [0., 0.],
                ],
                &[1],
            ),
        ];
        let mut data = Vec::new();
        field(&mut data, 1, &[int(5)]);
        field(&mut data, 2, &[0.]);
        field(&mut data, 3, &[0.]);
        field(&mut data, 4, &[1.]);
        for (id, (flags, points, links)) in sections.iter().enumerate() {
            field(&mut data, 5, &[int(id as u32)]);
            field(&mut data, 6, &[int(*flags)]);
            field(&mut data, 7, &[100.]);
            field(&mut data, 24, &[0.]);
            for (i, p) in points.iter().enumerate() {
                // Edges 15ft either side, perpendicular to the section.
                let (a, b) = if i + 1 < points.len() {
                    (points[i], points[i + 1])
                } else {
                    (points[i - 1], points[i])
                };
                let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
                let l = (dx * dx + dz * dz).sqrt();
                let (nx, nz) = (dz / l * 15., -dx / l * 15.);
                data.push(8);
                field(&mut data, 23, &[int(0)]);
                field(&mut data, 9, &[p[0], 0., p[1]]);
                field(&mut data, 10, &[p[0] - nx, 0., p[1] - nz]);
                field(&mut data, 11, &[p[0] + nx, 0., p[1] + nz]);
                for tag in [12, 18, 26, 27] {
                    field(&mut data, tag, &[1.]);
                }
            }
            field(&mut data, 13, &[int(links.len() as u32)]);
            for (slot, target) in links.iter().enumerate() {
                field(&mut data, 14, &[int(slot as u32), int(*target)]);
            }
        }
        field(&mut data, 15, &[0., 0., -90., 0., 0., 1., 30., 0., 0., 60.]);
        field(&mut data, 16, &[0., 0., 50., 0., 0., 1., 20., 0., 0., 40.]);
        for tag in [19, 20, 21, 22] {
            field(&mut data, tag, &[1.]);
        }
        field(&mut data, 40, &[3.]);
        data
    }
    #[test]
    fn graph_course_has_start_straight_gates_and_alternate_line() {
        let data = graph_course();
        let c = Course::parse(&data).unwrap();
        assert_eq!(c.sections, 5);
        // Prelude keeps its junction; the main line takes section 2, not 3.
        assert_eq!(c.loop_start, 3);
        assert_eq!(c.points[3], [0., 0., 100.]);
        assert_eq!(c.points[5], [50., 0., 250.]);
        assert_eq!(c.points[c.points.len() - 1], [0., 0., 0.]);
        assert_eq!(c.finish, c.loop_start);
        assert!(!c.finish_grace);
        assert_eq!(c.branches, vec![(4, 6)]);
        let (start, yaw) = c.start();
        assert!((start[2] + 94.66).abs() < 0.001 && yaw.abs() < 0.001);
        // A grid slightly behind the first node stays on the track apron.
        let origin = Station {
            segment: 0,
            fraction: 0.,
        };
        assert!(c.locate([5., 0., -110.], origin, false).is_some());
        assert!(c.locate([5., 0., -140.], origin, false).is_none());
        // The gate/policy tail is optional; the route graph is not.
        for length in 0..data.len() - 2 * 41 - 5 * 5 {
            assert!(Course::parse(&data[..length]).is_err());
        }
        let mut c = c;
        c.rider_spacing = 10.;
        let sign = c.grid_sign;
        let mut r = Race::new(c, 2, vec!["CPU".into()], 1).unwrap();
        // Unshuffled: the player in slot 1 (centered), the CPU one spacing over.
        assert_eq!(r.grid_slots(), &[1, 2]);
        assert_eq!(r.player_grid().0[0], start[0]);
        assert_eq!(r.opponents()[0].origin[0], start[0] + sign * 10.);
        r.countdown = 0.;
        // (The native gate test ignores a previous position at the origin.)
        for p in [[0., 0., -50.], [1., 0., 0.], [0., 0., 100.]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 1);
        let before = r.player.distance(&r.course);
        // The alternate line never visits (50, 250) but maps onto the main
        // line between its fork and rejoin.
        r.step([0., 0., 200.], 1.);
        r.step([50., 0., 150.], 1.);
        assert!(r.player.off_track == 0., "alternate line is on track");
        r.step([100., 0., 200.], 1.);
        assert_eq!(r.player.off_track, 0.);
        assert!(r.player.distance(&r.course) >= before + 200.);
        for p in [
            [100., 0., 100.],
            [100., 0., 0.],
            [50., 0., -20.],
            [1., 0., 0.],
        ] {
            r.step(p, 1.);
        }
        assert!(r.player.finish.is_none());
        r.step([0., 0., 100.], 1.);
        assert_eq!(r.player.completed, 2);
        assert_eq!(r.frame(0.).phase, "finished");
    }
    #[test]
    fn finish_gate_crossing_follows_001a95b0() {
        let gate = Gate {
            center: [0., 2., 50.],
            forward: [0., 0., 1.],
            facing: [0., 0., 1.],
            half_width: [20., 0., 0.],
            width: 40.,
        };
        assert_eq!(
            gate.crossing([5., 0., 60.], [5., 0., 40.]),
            Crossing::Through
        );
        // Backward through the gate, beside it, short of it, too high, and
        // from a zero previous position.
        assert_eq!(gate.crossing([5., 0., 40.], [5., 0., 60.]), Crossing::None);
        assert_eq!(
            gate.crossing([30., 0., 60.], [30., 0., 40.]),
            Crossing::Beside
        );
        assert_eq!(gate.crossing([5., 0., 45.], [5., 0., 40.]), Crossing::None);
        assert_eq!(
            gate.crossing([5., 160., 60.], [5., 160., 40.]),
            Crossing::None
        );
        assert_eq!(gate.crossing([0., 0., 60.], [0.; 3]), Crossing::None);
        // The gate line is the box diagonal, center ± half extents.
        let slanted = Gate {
            half_width: [20., 0., 10.],
            ..gate
        };
        assert_eq!(
            slanted.crossing([-15., 0., 44.], [-15., 0., 41.]),
            Crossing::Through
        );
        assert_eq!(
            slanted.crossing([-15., 0., 51.], [-15., 0., 49.]),
            Crossing::None
        );
        // Track +0x34/+0x38 as the race track loader 00215900 sets them.
        assert_eq!((TRACK_AHEAD, TRACK_BEHIND), (125., 75.));
    }
    #[test]
    fn track_flags_skip_the_first_crossing_and_first_best_lap() {
        let mut data = graph_course();
        data.push(41);
        data.extend(0xc_u32.to_le_bytes());
        let c = Course::parse(&data).unwrap();
        assert_eq!(c.track_flags, 0xc);
        assert_eq!(c.finish_at, Some(150.));
        let mut r = Race::new(c, 2, vec![], 1).unwrap();
        r.countdown = 0.;
        let lap = [
            [0., 0., 200.],
            [50., 0., 250.],
            [100., 0., 200.],
            [100., 0., 100.],
            [100., 0., 0.],
            [50., 0., -20.],
            [1., 0., 0.],
            [0., 0., 100.],
        ];
        // Flag 4: the first pass through the gate only arms the rider.
        for p in [[0., 0., -50.], [1., 0., 0.], [0., 0., 100.]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 0);
        // Flag 8: the first counted lap is not a best-lap candidate.
        for p in lap {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 1);
        assert_eq!(r.player.best, None);
        for p in lap {
            r.step(p, 2.);
        }
        assert_eq!(r.player.completed, 2);
        assert_eq!(r.player.best, Some(16.));
        assert_eq!(r.frame(0.).best_lap, Some(1));
    }
    #[test]
    fn passing_beside_the_finish_gate_counts_no_lap() {
        let c = Course::parse(&graph_course()).unwrap();
        let mut r = Race::new(c, 1, vec![], 1).unwrap();
        r.countdown = 0.;
        // Just outside the 40ft gate (and this narrow test track).
        let gate = r.course.finish_gate.unwrap();
        assert_eq!(
            gate.crossing([21., 0., 100.], [21., 0., 0.]),
            Crossing::Beside
        );
        for p in [[0., 0., -50.], [21., 0., 0.], [21., 0., 100.]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 0);
    }
    #[test]
    fn finish_just_ahead_of_grid_starts_the_race_without_a_lap() {
        let mut c = course();
        c.finish = 1;
        c.finish_grace = true;
        let mut r = Race::new(c, 1, vec![], 1).unwrap();
        r.countdown = 0.;
        r.step([0., 0., 100.], 1.);
        assert_eq!(r.player.completed, 0);
        for p in [[100., 0., 100.], [100., 0., 0.], [0.; 3]] {
            r.step(p, 1.);
        }
        assert!(r.player.finish.is_none());
        r.step([0., 0., 100.], 1.);
        assert_eq!(r.player.completed, 1);
    }
    #[test]
    fn multi_lap_best_time_and_finish_order_survive_final_lap() {
        let mut r = Race::new(course(), 2, vec![], 0).unwrap();
        r.countdown = 0.;
        for dt in [1., 2.] {
            for p in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]] {
                r.step(p, dt);
            }
        }
        let f = r.frame(0.);
        assert_eq!(f.elapsed, 12.);
        assert_eq!(f.best, Some(4.));
        assert_eq!(f.lap_time, 8.);
        assert_eq!(f.lap, 2);
        assert_eq!(f.standings[0].laps, 2);
        r.step([0.; 3], 1.);
        assert_eq!(r.elapsed, 12.);
    }
    #[test]
    fn rejects_malformed_setup_and_course() {
        assert!(Course::parse(&[]).is_err());
        assert!(Course::parse(&[99]).is_err());
        assert!(Race::new(course(), 0, vec![], 0).is_err());
    }
}
