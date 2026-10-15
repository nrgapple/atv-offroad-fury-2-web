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
use crate::{Result, slice, u32le};
use serde::Serialize;

/// Sections flagged 0x100 are the parallel alternate line at a fork.
const ALTERNATE_SECTION: u32 = 0x100;
/// Native grid fallback in 001f8410 sets riders 4.66ft behind the start node.
const GRID_SETBACK: f32 = 4.66;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Gate {
    pub center: [f32; 3],
    pub forward: [f32; 3],
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
    pub lane_limit: f32,
    pub leader_policy: [f32; 4],
    pub loop_start: usize,
    /// Checkpoint just past the finish gate; passing it completes a lap.
    pub finish: usize,
    /// The grid sits just behind the finish gate, so its first crossing
    /// begins the race instead of completing a lap.
    pub finish_grace: bool,
    /// Ordered (fork end, rejoin) checkpoint pairs. A rider on the alternate
    /// line may reach the rejoin point without the main line's checkpoints.
    pub branches: Vec<(usize, usize)>,
    pub start_gate: Option<Gate>,
    pub finish_gate: Option<Gate>,
    pub rider_spacing: f32,
    pub sections: usize,
}
impl Default for Course {
    fn default() -> Self {
        Self {
            points: vec![],
            widths: vec![],
            flags: vec![],
            speeds: vec![],
            airtimes: vec![],
            lane_limit: 3.,
            leader_policy: [150., 0.9, 1.5, 0.5],
            loop_start: 0,
            finish: 0,
            finish_grace: false,
            branches: vec![],
            start_gate: None,
            finish_gate: None,
            rider_spacing: 8.,
            sections: 1,
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
    point_flags: Vec<u32>,
    speeds: Vec<f32>,
    airtimes: Vec<f32>,
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
        let mut leader_policy = [150., 0.9, 1.5, 0.5];
        let mut gates = [None; 2];
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
                sections[current.unwrap()]
                    .widths
                    .push(distance(edges[0], edges[1]));
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
                    let forward =
                        normalize_flat(vector(at + 12)?).ok_or("invalid TDF gate direction")?;
                    let half_width = transform(offset, scale, vector(at + 24)?, false)?;
                    let width = float(bytes, at + 36)? * scale.unwrap_or(1.);
                    if !(0. ..=2048.).contains(&width) {
                        return Err("invalid TDF gate".into());
                    }
                    if tag != 25 {
                        gates[(tag - 15) as usize] = Some(Gate {
                            center,
                            forward,
                            half_width,
                            width,
                        });
                    }
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
                19..=22 => {
                    in_tail = true;
                    leader_policy[(tag - 19) as usize] = float(bytes, at)?;
                }
                40 => {
                    in_tail = true;
                    lane_limit = float(bytes, at)?;
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
        Self::linearize(&sections, lane_limit, leader_policy, gates)
    }

    fn linearize(
        sections: &[Section],
        lane_limit: f32,
        leader_policy: [f32; 4],
        gates: [Option<Gate>; 2],
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
        let speeds = field(&|s, l| s.speeds[l]);
        let airtimes = field(&|s, l| s.airtimes[l]);
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
            lane_limit,
            leader_policy,
            loop_start,
            finish,
            finish_grace: false,
            branches: vec![],
            start_gate: gates[0],
            finish_gate: gates[1],
            rider_spacing: 8.,
            sections: sections.len(),
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
                }
            }
        }
        course.branches.sort_unstable();
        course.branches.dedup();
        // Checkpoint 0 is the grid itself and is first passed after a lap.
        course.finish_grace = course.finish != 0
            && course.route_length(0, course.finish) < 0.25 * course.cycle_length();
        Ok(course)
    }
    /// The checkpoint after `index` on the main racing line.
    pub fn next(&self, index: usize) -> usize {
        if index + 1 >= self.points.len() {
            self.loop_start
        } else {
            index + 1
        }
    }
    fn steps(&self, from: usize, to: usize) -> usize {
        let mut i = from;
        for n in 0..=self.points.len() {
            if i == to {
                return n;
            }
            i = self.next(i);
        }
        usize::MAX
    }
    fn segment(&self, index: usize) -> f32 {
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
    /// Grid origin and heading: behind the original start gate when present.
    pub fn start(&self) -> ([f32; 3], f32) {
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
                    g.forward[0].atan2(g.forward[2]),
                )
            }
            None => (a, (b[0] - a[0]).atan2(b[2] - a[2])),
        }
    }
}
fn normalize_flat(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = (v[0] * v[0] + v[2] * v[2]).sqrt();
    (length > 0.000001 && length.is_finite()).then(|| [v[0] / length, 0., v[2] / length])
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
    next: usize,
    completed: u32,
    last: [f32; 3],
    finish: Option<f32>,
    lap_start: f32,
    best: Option<f32>,
    last_lap: f32,
    /// Times the lap circuit was re-entered from its closing segment.
    wraps: u32,
    entered: bool,
    armed: bool,
}
impl Progress {
    fn new(start: [f32; 3], course: &Course) -> Self {
        Self {
            next: course.next(0),
            completed: 0,
            last: start,
            finish: None,
            lap_start: 0.,
            best: None,
            last_lap: 0.,
            wraps: 0,
            entered: course.loop_start == 0,
            armed: !course.finish_grace,
        }
    }
    fn reaches(&self, p: [f32; 3], course: &Course, index: usize) -> bool {
        let target = course.points[index];
        let radius = (course.widths[index] * 0.6).clamp(16., 65.);
        let dx = p[0] - self.last[0];
        let dz = p[2] - self.last[2];
        let t = (((target[0] - self.last[0]) * dx + (target[2] - self.last[2]) * dz)
            / (dx * dx + dz * dz).max(0.0001))
        .clamp(0., 1.);
        let closest = [self.last[0] + dx * t, p[1], self.last[2] + dz * t];
        distance(closest, target) <= radius
    }
    fn pass(&mut self, course: &Course, time: f32, laps: u32) {
        let passed = self.next;
        if passed == course.loop_start {
            if self.entered {
                self.wraps += 1;
            }
            self.entered = true;
        }
        if passed == course.finish {
            if self.armed {
                self.completed += 1;
                let lap = time - self.lap_start;
                self.last_lap = lap;
                self.lap_start = time;
                self.best = Some(self.best.map_or(lap, |v| v.min(lap)));
                if self.completed >= laps {
                    self.finish = Some(time);
                }
            }
            self.armed = true;
        }
        self.next = course.next(passed);
    }
    fn update(&mut self, p: [f32; 3], course: &Course, time: f32, laps: u32) {
        if self.finish.is_some() {
            return;
        }
        let dx = p[0] - self.last[0];
        let dz = p[2] - self.last[2];
        // A reset/teleport cannot award a checkpoint along its swept path.
        if dx * dx + dz * dz > 0.0001 && distance(p, self.last) < 200. {
            if self.reaches(p, course, self.next) {
                self.pass(course, time, laps);
            } else if let Some(&(_, to)) = course.branches.iter().find(|(from, to)| {
                *to != self.next
                    && course.steps(*from, self.next) < course.steps(*from, *to)
                    && self.reaches(p, course, *to)
            }) {
                // Alternate line rejoined: the skipped main-line checkpoints
                // still count in order, including any finish or lap wrap.
                while self.finish.is_none() && self.next != to {
                    self.pass(course, time, laps);
                }
                if self.finish.is_none() {
                    self.pass(course, time, laps);
                }
            }
        }
        self.last = p;
    }
    // Restrict interpolation to the ordered checkpoint segment. Global nearest
    // projection wraps a rider just behind the start to the end of the lap and
    // can also award apparent distance across neighboring sections of a course.
    fn distance(&self, course: &Course) -> f32 {
        let previous = if self.next == course.loop_start && self.entered {
            course.points.len() - 1
        } else {
            self.next - 1
        };
        let a = course.points[previous];
        let b = course.points[self.next];
        let dx = b[0] - a[0];
        let dz = b[2] - a[2];
        let segment = distance(a, b);
        let fraction = (((self.last[0] - a[0]) * dx + (self.last[2] - a[2]) * dz)
            / (segment * segment).max(0.0001))
        .clamp(0., 1.);
        let prefix: f32 = (0..previous).map(|i| course.segment(i)).sum();
        self.wraps as f32 * course.cycle_length() + prefix + fraction * segment
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
}
#[derive(Serialize)]
pub struct Frame {
    pub phase: &'static str,
    pub countdown: u32,
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
    pub wrong_way: bool,
    pub opponents: Vec<Opponent>,
    pub standings: Vec<Standing>,
}
pub struct Race {
    pub course: Course,
    pub laps: u32,
    pub countdown: f32,
    pub elapsed: f32,
    player: Progress,
    opponents: Vec<Opponent>,
    progress: Vec<Progress>,
    preview_speeds: Vec<f32>,
}
impl Race {
    pub fn new(course: Course, laps: u32, names: Vec<String>, difficulty: u32) -> Result<Self> {
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
        let (start, yaw) = course.start();
        // Course RiderSpacing from the mode or scene TOY (8–15ft originals).
        let spacing = course.rider_spacing;
        let opponents: Vec<_> = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Opponent {
                name,
                origin: [
                    start[0] + yaw.cos() * [-2., -1., 1., 2.][i] * spacing,
                    start[1],
                    start[2] - yaw.sin() * [-2., -1., 1., 2.][i] * spacing,
                ],
                yaw,
                distance: 0.,
                finished: false,
                physical: None,
            })
            .collect();
        let preview_speeds = (0..opponents.len())
            .map(|i| [32., 46., 60.][difficulty as usize] + i as f32 * 1.7)
            .collect();
        let progress = opponents
            .iter()
            .map(|o| Progress::new(o.origin, &course))
            .collect();
        let player = Progress::new(start, &course);
        Ok(Self {
            course,
            laps,
            countdown: 3.,
            elapsed: 0.,
            player,
            opponents,
            progress,
            preview_speeds,
        })
    }
    pub fn locked(&self) -> bool {
        self.countdown > 0. || self.player.finish.is_some()
    }
    pub fn step(&mut self, p: [f32; 3], dt: f32) {
        if self.countdown > 0. {
            self.countdown = (self.countdown - dt).max(0.);
            self.player.last = p;
            return;
        }
        if self.player.finish.is_some() {
            return;
        }
        self.elapsed += dt;
        self.player.update(p, &self.course, self.elapsed, self.laps);
    }
    /// Preserve the separate authored motorcycle race preview. Original ATV
    /// racers use observe_cpu after independent Vehicle steps, never this path.
    pub fn step_motorcycle_preview_cpus(&mut self, dt: f32) {
        if !self.running() {
            return;
        }
        for ((o, progress), speed) in self
            .opponents
            .iter_mut()
            .zip(&mut self.progress)
            .zip(&self.preview_speeds)
        {
            if progress.finish.is_some() {
                continue;
            }
            let target = self.course.points[progress.next];
            let delta = distance(o.origin, target);
            let travel = (speed * dt).min(delta);
            if delta > 0.001 {
                o.yaw = (target[0] - o.origin[0]).atan2(target[2] - o.origin[2]);
                o.origin[0] += o.yaw.sin() * travel;
                o.origin[2] += o.yaw.cos() * travel;
                o.distance += travel;
            }
            if delta <= travel + 1. {
                progress.update(o.origin, &self.course, self.elapsed, self.laps);
            }
            o.finished = progress.finish.is_some();
        }
    }
    pub fn opponents(&self) -> &[Opponent] {
        &self.opponents
    }
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
        if p.finish.is_some() {
            return;
        }
        o.distance += distance(o.origin, origin);
        o.origin = origin;
        o.yaw = yaw;
        o.physical = physical;
        p.update(origin, &self.course, self.elapsed, self.laps);
        o.finished = p.finish.is_some();
    }
    pub fn frame(&self, yaw: f32) -> Frame {
        let mut order: Vec<_> = std::iter::once(("YOU".to_string(), &self.player))
            .chain(
                self.opponents
                    .iter()
                    .zip(&self.progress)
                    .map(|(o, p)| (o.name.clone(), p)),
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
            .position(|(_, p)| std::ptr::eq(*p, &self.player))
            .unwrap()
            + 1;
        let target = self.course.points[self.player.next];
        let bearing = (target[0] - self.player.last[0]).atan2(target[2] - self.player.last[2]);
        Frame {
            phase: if self.countdown > 0. {
                "countdown"
            } else if self.player.finish.is_some() {
                "finished"
            } else {
                "racing"
            },
            countdown: self.countdown.ceil() as u32,
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
            checkpoint: self.player.next,
            checkpoints: self.course.points.len(),
            wrong_way: self.countdown == 0.
                && self.player.finish.is_none()
                && (bearing - yaw).cos() < -0.4,
            opponents: self.opponents.clone(),
            standings: order
                .into_iter()
                .enumerate()
                .map(|(i, (name, p))| Standing {
                    name,
                    position: i + 1,
                    laps: p.completed,
                    time: p.finish,
                })
                .collect(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
            let first = if course.finish == 0 {
                course.cycle_length()
            } else {
                course.route_length(0, course.finish)
            };
            let expected =
                if course.finish_grace { 2. } else { 1. } * course.cycle_length() + first;
            // Checkpoints are awarded within their 16–65ft radius, so closely spaced
            // points can complete the finish slightly before its center.
            assert!(
                (travelled - expected).abs() < 150.,
                "{travelled} vs {expected}"
            );
        }
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
    #[test]
    fn countdown_and_finish_are_locked() {
        let mut r = Race::new(course(), 1, vec![], 1).unwrap();
        assert!(r.locked());
        for _ in 0..181 {
            r.step([0.; 3], 1. / 60.);
        }
        assert!(!r.locked());
        for p in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.], [0.; 3]] {
            r.step(p, 1.);
        }
        assert!(r.locked());
        assert_eq!(r.frame(0.).phase, "finished");
        assert_eq!(r.frame(0.).best, Some(4.));
    }
    #[test]
    fn shortcuts_reverse_and_reset_do_not_finish() {
        let mut r = Race::new(course(), 1, vec![], 1).unwrap();
        r.countdown = 0.;
        for p in [[100., 0., 0.], [0.; 3], [5000.; 3], [0.; 3]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 0);
        assert_eq!(r.player.next, 1);
    }
    #[test]
    fn leader_gap_uses_ordered_distance_at_start_and_lap_wrap() {
        let mut r = Race::new(course(), 2, vec!["CPU".into()], 1).unwrap();
        r.countdown = 0.;
        // A rider who drifts backwards over the start must not appear almost
        // one lap ahead, disabling the original CPU speed limiting policy.
        r.step([5., 0., 0.], 1.);
        r.observe_cpu(0, [0., 0., 50.], 0., None);
        assert_eq!(r.progress_difference(0), 50.);
        for p in [[0., 0., 100.], [100., 0., 100.], [100., 0., 0.]] {
            r.step([5., 0., 0.], 1.);
            r.observe_cpu(0, p, 0., None);
        }
        r.observe_cpu(0, [30., 0., 0.], 0., None);
        assert_eq!(r.progress_difference(0), 370.);
        r.observe_cpu(0, [0.; 3], 0., None);
        assert_eq!(r.progress_difference(0), 400.);
        r.observe_cpu(0, [0., 0., 10.], 0., None);
        assert_eq!(r.progress_difference(0), 410.);
        // A reset to another segment cannot create a lap or course-length gap.
        r.step([100., 0., 0.], 1.);
        assert_eq!(r.player.completed, 0);
        assert_eq!(r.player.distance(&r.course), 0.);
    }
    #[test]
    fn physical_cpus_require_observation_and_motorcycle_preview_is_explicit() {
        let mut r = Race::new(course(), 1, vec!["CPU".into()], 1).unwrap();
        let start = r.opponents()[0].origin;
        r.step_motorcycle_preview_cpus(1. / 60.);
        assert_eq!(r.opponents()[0].origin, start);
        r.countdown = 0.;
        r.step([0.; 3], 1. / 60.);
        assert_eq!(r.opponents()[0].origin, start);
        assert_eq!(r.opponents()[0].distance, 0.);
        r.step_motorcycle_preview_cpus(1. / 60.);
        assert_ne!(r.opponents()[0].origin, start);
        assert!(r.opponents()[0].distance > 0.);
        assert!(r.opponents()[0].physical.is_none());
    }
    #[test]
    fn wreck_reposition_preserves_progress_without_a_swept_or_radius_award() {
        let mut r = Race::new(course(), 1, vec!["CPU".into()], 0).unwrap();
        r.countdown = 0.;
        r.step([0.; 3], 1.);
        let prior_distance = r.opponents[0].distance;
        r.reposition_cpu(0, [0., 0., 100.], 1., None);
        assert_eq!(r.progress[0].next, 1);
        assert_eq!(r.progress[0].completed, 0);
        assert_eq!(r.opponents[0].distance, prior_distance);
        // Ordinary driving after the recovery can earn this checkpoint.
        r.observe_cpu(0, [0., 0., 101.], 1., None);
        assert_eq!(r.progress[0].next, 2);
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
            for p in *points {
                data.push(8);
                field(&mut data, 23, &[int(0)]);
                field(&mut data, 9, &[p[0], 0., p[1]]);
                field(&mut data, 10, &[p[0] - 15., 0., p[1]]);
                field(&mut data, 11, &[p[0] + 15., 0., p[1]]);
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
        // The gate/policy tail is optional; the route graph is not.
        for length in 0..data.len() - 2 * 41 - 5 * 5 {
            assert!(Course::parse(&data[..length]).is_err());
        }
        let mut c = c;
        c.rider_spacing = 10.;
        let mut r = Race::new(c, 2, vec!["CPU".into()], 1).unwrap();
        assert_eq!(r.opponents()[0].origin[0], -20.);
        r.countdown = 0.;
        // Start straight, then the finish gate completes the short first lap.
        for p in [[0., 0., -50.], [0., 0., 0.], [0., 0., 100.]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.completed, 1);
        assert_eq!(r.player.wraps, 0);
        let before = r.player.distance(&r.course);
        // The alternate line never visits (50, 250) but still rejoins in order.
        for p in [[0., 0., 200.], [50., 0., 150.], [100., 0., 200.]] {
            r.step(p, 1.);
        }
        assert_eq!(r.player.next, 7);
        assert!(r.player.distance(&r.course) > before);
        for p in [
            [100., 0., 100.],
            [100., 0., 0.],
            [50., 0., -20.],
            [0., 0., 0.],
        ] {
            r.step(p, 1.);
        }
        assert!(r.player.finish.is_none());
        r.step([0., 0., 100.], 1.);
        assert_eq!(r.player.completed, 2);
        assert_eq!(r.player.wraps, 1);
        assert_eq!(r.frame(0.).phase, "finished");
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
