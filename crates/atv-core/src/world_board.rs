//! World Hall of Fame boards and the daily event (research/WORLD_BOARDS.md;
//! a port addition, issue #543): the run's 1 Hz position trace the server
//! checks, the submission body, the boards' JSON, and the UTC-date-seeded
//! daily pick (shared with `cloudflare/boards.mjs`, computable offline).
use serde::{Deserialize, Serialize};

/// Board entries the server keeps.
pub const TOP: usize = 50;
/// The trace keeps one point a second, at most this many (30 minutes).
pub const MAX_POINTS: usize = 1800;
/// Rider resets the trace marks; later ones are not marked (the server
/// refuses more).
pub const MAX_RESETS: usize = 30;
/// Vehicles from this id on are motorcycles (`player_models::selection`).
pub const FIRST_MOTORCYCLE: u16 = 27;

/// The daily event's courses: the championship race courses in event-type
/// order (Amateur and Pro Enduro, Amateur and Pro Nationals, Amateur and Pro
/// Supercross). Keep in step with `DAILY_POOL` in `cloudflare/boards.mjs`.
pub const DAILY_POOL: [&str; 39] = [
    "env05ends",
    "env02ends",
    "env08ends",
    "env06ends",
    "env04ends",
    "env01ends",
    "env05endl",
    "env02endl",
    "env08endl",
    "env06endl",
    "env04endl",
    "env01endl",
    "env05",
    "env02",
    "env08",
    "env06",
    "env04",
    "env01",
    "nat01",
    "nat02",
    "nat03",
    "nat04",
    "nat05",
    "nat06",
    "nat07",
    "sx08",
    "sx12",
    "sx01",
    "sx13",
    "sx02",
    "sx06",
    "sx05",
    "sx03",
    "sx07",
    "sx16",
    "sx04",
    "sx10",
    "sx15",
    "sx14",
];

/// The daily event's vehicle class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VehicleClass {
    Atv,
    Motorcycle,
}

impl VehicleClass {
    pub fn admits(self, vehicle: u16) -> bool {
        (vehicle >= FIRST_MOTORCYCLE) == (self == Self::Motorcycle)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Atv => "ATV",
            Self::Motorcycle => "Motorcycle",
        }
    }
}

/// One day's event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyPick {
    /// `YYYY-MM-DD`, UTC.
    pub date: String,
    pub course: &'static str,
    pub class: VehicleClass,
}

impl DailyPick {
    /// The day's board id.
    pub fn board(&self) -> String {
        format!("daily:{}", self.date)
    }
    /// The race's laps: three for Enduro, five otherwise.
    pub fn laps(&self) -> u32 {
        default_laps(self.course)
    }
}

pub fn default_laps(course: &str) -> u32 {
    if course.ends_with("ends") || course.ends_with("endl") {
        3
    } else {
        5
    }
}

fn mix(day: i64) -> u32 {
    let mut x = (day as u32).wrapping_mul(0x9e37_79b1);
    x = (x ^ (x >> 16)).wrapping_mul(0x045d_9f3b);
    x ^ (x >> 16)
}

/// The pick for UTC day `day` (days since 1970-01-01).
pub fn daily(day: i64) -> DailyPick {
    let hash = mix(day);
    DailyPick {
        date: civil_date(day),
        course: DAILY_POOL[hash as usize % DAILY_POOL.len()],
        class: if (hash >> 8) % 4 == 3 {
            VehicleClass::Motorcycle
        } else {
            VehicleClass::Atv
        },
    }
}

/// The UTC day number of a Unix time in milliseconds.
pub fn day_of(unix_ms: f64) -> i64 {
    (unix_ms / 86_400_000.).floor() as i64
}

/// `YYYY-MM-DD` of a day number (proleptic Gregorian, UTC).
pub fn civil_date(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The run's position trace from GO: one `[x, z]` point (integer world
/// feet) a second, each lap's clock time and the points a rider reset moved
/// the vehicle to. Fixed capacity: recording allocates nothing per step.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Trace {
    pub points: Vec<[i32; 2]>,
    pub marks: Vec<u32>,
    pub resets: Vec<usize>,
    #[serde(skip)]
    reset_pending: bool,
}

impl Default for Trace {
    fn default() -> Self {
        Self {
            points: Vec::with_capacity(MAX_POINTS),
            marks: Vec::with_capacity(5),
            resets: Vec::with_capacity(MAX_RESETS),
            reset_pending: false,
        }
    }
}

impl Trace {
    /// A race step at clock `elapsed` seconds with the vehicle at `origin`;
    /// `laps` completed so far.
    pub fn step(&mut self, elapsed: f32, origin: [f32; 3], laps: u32) {
        while (self.marks.len() as u32) < laps && self.marks.len() < 5 {
            self.marks.push((elapsed * 1000.).round() as u32);
        }
        // The point for second n is the first step at or after n seconds.
        let due = self.points.len();
        if due < MAX_POINTS && elapsed >= due as f32 {
            if std::mem::take(&mut self.reset_pending) && due > 0 && self.resets.len() < MAX_RESETS
            {
                self.resets.push(due);
            }
            self.points
                .push([origin[0].round() as i32, origin[2].round() as i32]);
        }
    }

    /// A rider reset moved the vehicle: the next point is not ridden to.
    pub fn reset(&mut self) {
        self.reset_pending = true;
    }
}

/// What a finished race sends (`POST /api/boards/submit`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceSubmission<'a> {
    pub name: &'a str,
    pub vehicle: u16,
    pub course: &'a str,
    pub expert: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily: Option<&'a str>,
    pub trace: &'a Trace,
}

/// What a finished Freestyle objective sends.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FreestyleSubmission<'a> {
    pub name: &'a str,
    pub vehicle: u16,
    pub series: &'a str,
    pub course: &'a str,
    pub score: i64,
    pub seconds: u32,
    pub trace: &'a Trace,
}

/// Seconds to the milliseconds the boards keep: the lap's mark difference,
/// so the server can match it.
pub fn millis(seconds: f32) -> u32 {
    (seconds * 1000.).round() as u32
}

/// The board ids: race boards per course and difficulty, Freestyle per
/// series and course.
pub fn lap_board(course: &str, expert: bool) -> String {
    format!("lap:{course}:{}", if expert { 'x' } else { 'n' })
}
pub fn race_board(course: &str, expert: bool) -> String {
    format!("race:{course}:{}", if expert { 'x' } else { 'n' })
}
pub fn score_board(series: &str, course: &str) -> String {
    format!("score:{series}:{course}")
}

/// One board row.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct BoardEntry {
    pub rank: u32,
    pub name: String,
    /// Milliseconds, or the Freestyle score.
    pub value: i64,
    pub vehicle: u16,
    pub day: String,
}

/// Yesterday's daily winner.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Winner {
    pub date: String,
    pub name: Option<String>,
    pub value: Option<i64>,
    pub vehicle: Option<u16>,
}

/// `GET /api/boards/<board>`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Board {
    pub board: String,
    pub entries: Vec<BoardEntry>,
    pub yesterday: Option<Winner>,
}

/// One board's placing in a submission's answer.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Placing {
    pub rank: Option<u32>,
    pub placed: bool,
}

/// `POST /api/boards/submit`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Placings {
    pub name: String,
    pub boards: std::collections::BTreeMap<String, Placing>,
}

impl Placings {
    /// The results line for the boards this run entered, best first
    /// ("World #3 Fastest Lap").
    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<(u32, String)> = self
            .boards
            .iter()
            .filter(|(_, p)| p.placed)
            .filter_map(|(id, p)| {
                let what = match id.split(':').next()? {
                    "lap" => "Fastest Lap",
                    "race" => "Fastest Race",
                    "score" => "Objective Score",
                    "daily" => "Daily Event",
                    _ => return None,
                };
                let rank = p.rank?;
                Some((rank, format!("World #{rank} {what}!")))
            })
            .collect();
        lines.sort();
        lines.into_iter().map(|(_, l)| l).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_pick_matches_the_server() {
        // cloudflare/boards.test.mjs pins the same dates.
        let day = |date: &str| {
            (0..30_000)
                .find(|d| civil_date(*d) == date)
                .expect("date in range")
        };
        let picks: Vec<String> = ["2026-11-11", "2026-11-12", "2027-01-01"]
            .iter()
            .map(|d| {
                let p = daily(day(d));
                assert_eq!(p.date, *d);
                format!("{}/{}", p.course, p.class.label().to_lowercase())
            })
            .collect();
        assert_eq!(picks, ["env01endl/atv", "env05/atv", "env02/atv"]);
        assert_eq!(daily(day("2026-11-14")).class, VehicleClass::Motorcycle);
        assert_eq!(daily(day("2026-11-14")).course, "env04");
    }

    #[test]
    fn daily_pool_matches_the_server() {
        let js = include_str!("../../../cloudflare/boards.mjs");
        let start = js.find("export const DAILY_POOL = [").expect("pool");
        let end = start + js[start..].find("];").expect("pool end");
        let ids: Vec<&str> = js[start..end].split('\'').skip(1).step_by(2).collect();
        assert_eq!(ids, DAILY_POOL);
    }

    #[test]
    fn civil_dates_and_days() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_403), "2025-11-11");
        assert_eq!(day_of(1_762_819_200_000.), 20_403);
        assert_eq!(day_of(1_762_819_199_999.), 20_402);
        assert_eq!(civil_date(-1), "1969-12-31");
        assert_eq!(civil_date(11_016), "2000-02-29");
    }

    #[test]
    fn the_trace_keeps_one_point_a_second_laps_and_resets() {
        let mut trace = Trace::default();
        let mut laps = 0;
        for i in 0..(60 * 125) {
            let t = i as f32 / 60.;
            if i == 60 * 61 {
                laps = 1;
            }
            if i == 60 * 30 + 10 {
                trace.reset();
            }
            trace.step(t, [t * 50., 3., -t], laps);
        }
        assert!((125..=126).contains(&trace.points.len()));
        assert_eq!(trace.points[2], [100, -2]);
        assert_eq!(trace.marks, [61_000]);
        assert_eq!(trace.resets, [31]);
        let capacity = trace.points.capacity();
        assert_eq!(capacity, MAX_POINTS, "no growth while recording");
    }

    #[test]
    fn a_submission_serializes_as_the_server_reads_it() {
        let mut trace = Trace::default();
        trace.step(0., [1.4, 0., 2.6], 0);
        let body = serde_json::to_value(RaceSubmission {
            name: "ADA",
            vehicle: 3,
            course: "nat01",
            expert: false,
            lap_ms: Some(millis(61.2345)),
            race_ms: None,
            daily: None,
            trace: &trace,
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"name": "ADA", "vehicle": 3, "course": "nat01", "expert": false,
                "lapMs": 61235, "trace": {"points": [[1, 3]], "marks": [], "resets": []}})
        );
    }

    #[test]
    fn placings_name_the_boards_entered() {
        let answer: Placings = serde_json::from_str(
            r#"{"name":"ADA","boards":{"lap:nat01:n":{"rank":3,"placed":true},"race:nat01:n":{"rank":9,"placed":false},"daily:2026-11-11":{"rank":1,"placed":true}}}"#,
        )
        .unwrap();
        assert_eq!(
            answer.lines(),
            ["World #1 Daily Event!", "World #3 Fastest Lap!"]
        );
        let board: Board = serde_json::from_str(
            r#"{"board":"daily:2026-11-11","entries":[{"rank":1,"name":"BOB","value":61000,"vehicle":3,"day":"2026-11-11"}],"yesterday":{"date":"2026-11-10"}}"#,
        )
        .unwrap();
        assert_eq!(board.entries[0].name, "BOB");
        assert_eq!(board.yesterday.unwrap().name, None);
    }
}
