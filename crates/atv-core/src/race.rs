//! Race bookkeeping around recovered TDF geometry and independent CPU vehicles.
//! Race event/progression rules remain provisional; source-correlated driver
//! decisions live in race_ai and feed the recovered vehicle solver.
use crate::{Result, slice, u32le};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Course {
    pub points: Vec<[f32; 3]>,
    pub widths: Vec<f32>,
    pub flags: Vec<u32>,
    pub speeds: Vec<f32>,
    pub lane_limit: f32,
    pub leader_policy: [f32; 4],
}
impl Default for Course {
    fn default() -> Self {
        Self {
            points: vec![],
            widths: vec![],
            flags: vec![],
            speeds: vec![],
            lane_limit: 3.,
            leader_policy: [150., 0.9, 1.5, 0.5],
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
impl Course {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 256 * 1024 {
            return Err("TDF exceeds limit".into());
        }
        let mut at = 0;
        let mut offset = [None; 2];
        let mut scale = None;
        let mut points = Vec::new();
        let mut widths = Vec::new();
        let mut flags = Vec::new();
        let mut speeds = Vec::new();
        let mut lane_limit = 3.;
        let mut leader_policy = [150., 0.9, 1.5, 0.5];
        let mut edges = Vec::new();
        let mut in_point = false;
        let mut in_tail = false;
        let mut version = false;
        let mut point_field = 0;
        while at < bytes.len() {
            let tag = bytes[at];
            at += 1;
            let size = match tag {
                8 => 0,
                9..=11 => 12,
                14 => 8,
                15..=16 => 40,
                1..=7 | 12 | 13 | 18..=24 | 26..=27 | 40..=41 => 4,
                _ => return Err(format!("unsupported TDF tag {tag}")),
            };
            slice(bytes, at, size)?;
            if in_point && !in_tail && tag != 8 && tag != 13 {
                if [23, 9, 10, 11, 12, 18, 26, 27].get(point_field) != Some(&tag) {
                    return Err("invalid TDF route field order".into());
                }
                point_field += 1;
            }
            match tag {
                1 => {
                    if version || u32le(bytes, at)? != 1 {
                        return Err("invalid TDF version".into());
                    }
                    version = true;
                }
                2..=3 => {
                    offset[(tag - 2) as usize] = Some(float(bytes, at)?);
                }
                4 => {
                    scale = Some(float(bytes, at)?);
                }
                8 => {
                    if in_tail || (in_point && (edges.len() != 2 || point_field != 8)) {
                        return Err("invalid TDF point order".into());
                    }
                    if in_point {
                        widths.push(distance(edges[0], edges[1]));
                    }
                    edges.clear();
                    point_field = 0;
                    in_point = true;
                }
                9..=11 => {
                    if !in_point || in_tail {
                        return Err("TDF coordinate outside route".into());
                    }
                    let x = offset[0].ok_or("missing TDF x offset")?;
                    let z = offset[1].ok_or("missing TDF z offset")?;
                    let s = scale
                        .filter(|s| *s > 0. && *s <= 1024.)
                        .ok_or("invalid TDF scale")?;
                    let point = [
                        x + float(bytes, at)? * s,
                        float(bytes, at + 4)? * s,
                        z + float(bytes, at + 8)? * s,
                    ];
                    if !point.iter().all(|v| v.is_finite() && v.abs() < 1_000_000.) {
                        return Err("invalid TDF coordinate".into());
                    }
                    if tag == 9 {
                        if points.len() > 4096 || (!points.is_empty() && !edges.is_empty()) {
                            return Err("invalid TDF centers".into());
                        }
                        points.push(point);
                    } else {
                        edges.push(point);
                    }
                }
                13 => {
                    if !in_point || edges.len() != 2 || point_field != 8 || in_tail {
                        return Err("invalid TDF tail".into());
                    }
                    widths.push(distance(edges[0], edges[1]));
                    in_tail = true;
                }
                23 if in_point && !in_tail => {
                    flags.push(u32le(bytes, at)?);
                }
                18 if in_point && !in_tail => {
                    let speed = float(bytes, at)?;
                    if speed != -1. && !(0. ..=1000.).contains(&speed) {
                        return Err("invalid TDF speed".into());
                    }
                    speeds.push(speed);
                }
                19..=22 => {
                    leader_policy[(tag - 19) as usize] = float(bytes, at)?;
                }
                40 => {
                    lane_limit = float(bytes, at)?;
                }
                7 | 12 | 26..=27 => {
                    float(bytes, at)?;
                }
                _ => {}
            }
            at += size;
        }
        if !version
            || !in_tail
            || points.len() < 8
            || points.len() != widths.len()
            || distance(points[0], *points.last().unwrap()) > 1.
        {
            return Err("invalid or open TDF course".into());
        }
        if flags.len() != points.len()
            || speeds.len() != points.len()
            || !(0. ..=30.).contains(&lane_limit)
            || leader_policy[0] <= 0.
            || leader_policy[1..].iter().any(|v| *v < 0. || *v > 3.)
        {
            return Err("invalid TDF AI configuration".into());
        }
        points.pop();
        widths.pop();
        flags.pop();
        speeds.pop();
        if widths
            .iter()
            .any(|w| !w.is_finite() || *w < 1. || *w > 2048.)
            || points.windows(2).any(|p| distance(p[0], p[1]) < 0.01)
        {
            return Err("invalid TDF route dimensions".into());
        }
        Ok(Self {
            points,
            widths,
            flags,
            speeds,
            lane_limit,
            leader_policy,
        })
    }
    pub fn start(&self) -> ([f32; 3], f32) {
        let a = self.points[0];
        let b = self.points[1];
        (a, (b[0] - a[0]).atan2(b[2] - a[2]))
    }
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
}
impl Progress {
    fn new(start: [f32; 3]) -> Self {
        Self {
            next: 1,
            completed: 0,
            last: start,
            finish: None,
            lap_start: 0.,
            best: None,
            last_lap: 0.,
        }
    }
    fn update(&mut self, p: [f32; 3], course: &Course, time: f32, laps: u32) {
        if self.finish.is_some() {
            return;
        }
        let target = course.points[self.next];
        let radius = (course.widths[self.next] * 0.6).clamp(16., 65.);
        let dx = p[0] - self.last[0];
        let dz = p[2] - self.last[2];
        let t = (((target[0] - self.last[0]) * dx + (target[2] - self.last[2]) * dz)
            / (dx * dx + dz * dz).max(0.0001))
        .clamp(0., 1.);
        let closest = [self.last[0] + dx * t, p[1], self.last[2] + dz * t];
        // A reset/teleport cannot award a checkpoint along its swept path.
        if dx * dx + dz * dz > 0.0001
            && distance(p, self.last) < 200.
            && distance(closest, target) <= radius
        {
            self.next = (self.next + 1) % course.points.len();
            if self.next == 1 {
                self.completed += 1;
                let lap = time - self.lap_start;
                self.last_lap = lap;
                self.lap_start = time;
                self.best = Some(self.best.map_or(lap, |v| v.min(lap)));
                if self.completed >= laps {
                    self.finish = Some(time);
                }
            }
        }
        self.last = p;
    }
    // Restrict interpolation to the ordered checkpoint segment. Global nearest
    // projection wraps a rider just behind the start to the end of the lap and
    // can also award apparent distance across neighboring sections of a course.
    fn distance(&self, course: &Course) -> f32 {
        let previous = (self.next + course.points.len() - 1) % course.points.len();
        let a = course.points[previous];
        let b = course.points[self.next];
        let dx = b[0] - a[0];
        let dz = b[2] - a[2];
        let segment = distance(a, b);
        let fraction = (((self.last[0] - a[0]) * dx + (self.last[2] - a[2]) * dz)
            / (segment * segment).max(0.0001))
        .clamp(0., 1.);
        let mut length = 0.;
        let mut prefix = 0.;
        for (i, point) in course.points.iter().enumerate() {
            let segment = distance(*point, course.points[(i + 1) % course.points.len()]);
            length += segment;
            if i < previous {
                prefix += segment;
            }
        }
        self.completed as f32 * length + prefix + fraction * segment
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
}
impl Race {
    pub fn new(course: Course, laps: u32, names: Vec<String>, difficulty: u32) -> Result<Self> {
        if course.points.len() < 4
            || course.points.len() > 4096
            || course.widths.len() != course.points.len()
            || course.points.iter().flatten().any(|v| !v.is_finite())
            || course.widths.iter().any(|w| !w.is_finite() || *w < 1.)
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
        let opponents: Vec<_> = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Opponent {
                name,
                origin: [
                    start[0] + yaw.cos() * [-2., -1., 1., 2.][i] * 8.,
                    start[1],
                    start[2] - yaw.sin() * [-2., -1., 1., 2.][i] * 8.,
                ],
                yaw,
                distance: 0.,
                finished: false,
                physical: None,
            })
            .collect();
        let progress = opponents.iter().map(|o| Progress::new(o.origin)).collect();
        Ok(Self {
            course,
            laps,
            countdown: 3.,
            elapsed: 0.,
            player: Progress::new(start),
            opponents,
            progress,
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
    pub fn opponents(&self) -> &[Opponent] {
        &self.opponents
    }
    pub fn running(&self) -> bool {
        self.countdown == 0. && self.elapsed > 0. && self.player.finish.is_none()
    }
    pub fn progress_difference(&self, index: usize) -> f32 {
        self.progress[index].distance(&self.course) - self.player.distance(&self.course)
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
        let c = Course::parse(&data).unwrap();
        assert_eq!(c.points.len(), 8);
        assert_eq!(c.points[1], [2304., 0., 2394.]);
        assert_eq!(c.widths[0], 27.);
        for length in 0..data.len() {
            assert!(Course::parse(&data[..length]).is_err());
        }
        data[26] = 10;
        assert!(Course::parse(&data).is_err());
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
