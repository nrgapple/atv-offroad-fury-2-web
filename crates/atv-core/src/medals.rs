//! Medals, ghost picks and the weekly challenge (research/GHOST_CHALLENGES.md;
//! a port addition, issue #637): each measured race course's author time and
//! its Bronze/Silver/Gold/Author medal times, the rider's best lap per course
//! and weekly results, the ghost a picker choice races, and the ISO-week-seeded
//! weekly pick shared with `cloudflare/weekly.mjs` (computable offline).
use crate::world_board::{BoardEntry, DAILY_POOL, VehicleClass, civil_date};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A course medal, worst first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Medal {
    Bronze,
    Silver,
    Gold,
    Author,
}

impl Medal {
    /// Worst first.
    pub const ALL: [Medal; 4] = [Medal::Bronze, Medal::Silver, Medal::Gold, Medal::Author];

    /// The medal's time as thousandths of the author time (judgment:
    /// GHOST_CHALLENGES.md "Medals").
    pub fn per_mille(self) -> u64 {
        match self {
            Self::Bronze => 1500,
            Self::Silver => 1200,
            Self::Gold => 1060,
            Self::Author => 1000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bronze => "Bronze",
            Self::Silver => "Silver",
            Self::Gold => "Gold",
            Self::Author => "Author",
        }
    }

    /// The medal's colour, `#rrggbb`.
    pub fn color(self) -> &'static str {
        match self {
            Self::Bronze => "#c8793a",
            Self::Silver => "#c9d2dc",
            Self::Gold => "#f2c832",
            Self::Author => "#36c37a",
        }
    }

    /// The next better medal.
    pub fn next(self) -> Option<Medal> {
        Self::ALL.get(self as usize + 1).copied()
    }
}

/// Each measured race course's author time, milliseconds: the fastest
/// flying lap the game's own race path driver rode there (`MEASURED_LAP` in
/// `cloudflare/board_seeds.mjs`; a test keeps the two equal). Sorted by id.
pub const AUTHOR: [(&str, u32); 51] = [
    ("env01", 102_308),
    ("env01endl", 204_917),
    ("env01ends", 105_408),
    ("env01st", 11_250),
    ("env02", 31_240),
    ("env02endl", 197_852),
    ("env02ends", 82_331),
    ("env03", 57_872),
    ("env03endl", 93_161),
    ("env03ends", 83_014),
    ("env04", 62_713),
    ("env04endl", 210_615),
    ("env04ends", 120_887),
    ("env05", 21_367),
    ("env05endl", 131_301),
    ("env05ends", 69_043),
    ("env06", 86_776),
    ("env06endl", 129_085),
    ("env06ends", 99_842),
    ("env06st", 15_683),
    ("env07", 96_743),
    ("env07endl", 203_334),
    ("env07ends", 155_645),
    ("env07st", 27_823),
    ("env08", 61_768),
    ("env08endl", 118_006),
    ("env08ends", 87_912),
    ("nat01", 43_138),
    ("nat02", 77_681),
    ("nat03", 79_547),
    ("nat04", 154_712),
    ("nat05", 95_977),
    ("nat06", 127_969),
    ("nat07", 148_747),
    ("nat08", 109_757),
    ("sx01", 69_983),
    ("sx02", 65_384),
    ("sx03", 62_030),
    ("sx04", 57_257),
    ("sx05", 58_486),
    ("sx06", 50_493),
    ("sx07", 66_930),
    ("sx08", 46_689),
    ("sx09", 57_027),
    ("sx10", 45_039),
    ("sx11", 68_111),
    ("sx12", 40_574),
    ("sx13", 55_612),
    ("sx14", 70_286),
    ("sx15", 77_614),
    ("sx16", 58_236),
];

/// The course's author time.
pub fn author_ms(course: &str) -> Option<u32> {
    AUTHOR
        .binary_search_by(|(id, _)| (*id).cmp(course))
        .ok()
        .map(|i| AUTHOR[i].1)
}

/// The course's time for `medal`, milliseconds (rounded down).
pub fn medal_ms(course: &str, medal: Medal) -> Option<u32> {
    author_ms(course).map(|ms| (u64::from(ms) * medal.per_mille() / 1000) as u32)
}

/// The best medal a lap of `lap_ms` earns on `course`.
pub fn earned(course: &str, lap_ms: u32) -> Option<Medal> {
    Medal::ALL
        .iter()
        .rev()
        .copied()
        .find(|m| medal_ms(course, *m).is_some_and(|ms| lap_ms <= ms))
}

/// The medal to chase next: the worst one `best` has not earned (Author
/// once every medal is in).
pub fn next_medal(course: &str, best: Option<u32>) -> Option<Medal> {
    author_ms(course)?;
    Some(match best.and_then(|ms| earned(course, ms)) {
        None => Medal::Bronze,
        Some(m) => m.next().unwrap_or(Medal::Author),
    })
}

/// A weekly challenge badge, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Badge {
    Top1,
    Top10,
    Top25,
    Top50,
    Finished,
}

impl Badge {
    /// The cut-earning bands with their percent, best first.
    pub const BANDS: [(Badge, u32); 4] = [
        (Badge::Top1, 1),
        (Badge::Top10, 10),
        (Badge::Top25, 25),
        (Badge::Top50, 50),
    ];
    pub const ALL: [Badge; 5] = [
        Badge::Top1,
        Badge::Top10,
        Badge::Top25,
        Badge::Top50,
        Badge::Finished,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top1 => "Top 1%",
            Self::Top10 => "Top 10%",
            Self::Top25 => "Top 25%",
            Self::Top50 => "Top 50%",
            Self::Finished => "Finished",
        }
    }

    /// The badge a weekly time earns against a board's cuts (`{"1": ms,
    /// "10": ms, ...}`): the best band it is inside, else Finished
    /// (`weeklyBadge` in `cloudflare/weekly.mjs`).
    pub fn of(value: i64, cuts: &BTreeMap<String, i64>) -> Badge {
        Self::BANDS
            .iter()
            .find(|(_, percent)| {
                cuts.get(&percent.to_string())
                    .is_some_and(|cut| value <= *cut)
            })
            .map_or(Badge::Finished, |(badge, _)| *badge)
    }
}

/// A week's result on this device: the rider's best time and, once the
/// week's cuts are final, the badge it earned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekResult {
    pub value: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<Badge>,
}

/// The rider's medal record on this device: the best medal-earning lap per
/// course and each weekly challenge entered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MedalBook {
    pub laps: BTreeMap<String, u32>,
    pub weeks: BTreeMap<String, WeekResult>,
    /// The lap time of each course's personal best ghost kept on this
    /// device (its bytes are stored beside the book).
    pub ghosts: BTreeMap<String, u32>,
}

impl MedalBook {
    /// A lap of `lap_ms` on `course`: kept when it is the course's best.
    /// Returns the medal it earned and whether that medal is new (better
    /// than the course's medal before).
    pub fn record_lap(&mut self, course: &str, lap_ms: u32) -> Option<(Medal, bool)> {
        let medal = earned(course, lap_ms)?;
        let before = self.medal(course);
        let best = self.laps.entry(course.to_string()).or_insert(lap_ms);
        *best = (*best).min(lap_ms);
        Some((medal, before.is_none_or(|b| medal > b)))
    }

    /// The course's best lap, milliseconds.
    pub fn best(&self, course: &str) -> Option<u32> {
        self.laps.get(course).copied()
    }

    /// The course's medal.
    pub fn medal(&self, course: &str) -> Option<Medal> {
        earned(course, self.best(course)?)
    }

    /// Courses holding each medal, Bronze first.
    pub fn counts(&self) -> [u32; 4] {
        let mut counts = [0; 4];
        for (course, ms) in &self.laps {
            if let Some(m) = earned(course, *ms) {
                counts[m as usize] += 1;
            }
        }
        counts
    }

    /// A weekly race of `value` ms in `week`: kept when it is the week's best.
    pub fn record_week(&mut self, week: &str, value: u32) -> bool {
        let entry = self
            .weeks
            .entry(week.to_string())
            .or_insert(WeekResult { value, badge: None });
        let better = value <= entry.value;
        entry.value = entry.value.min(value);
        better
    }

    /// Badges earned, best first.
    pub fn badge_counts(&self) -> [u32; 5] {
        let mut counts = [0; 5];
        for week in self.weeks.values() {
            if let Some(b) = week.badge {
                counts[Badge::ALL.iter().position(|x| *x == b).unwrap_or(4)] += 1;
            }
        }
        counts
    }

    /// Past weeks without a badge yet, which need their final board.
    pub fn unbadged(&self, current: &str) -> Vec<String> {
        self.weeks
            .iter()
            .filter(|(id, w)| w.badge.is_none() && id.as_str() < current)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The ISO week (Monday to Sunday, UTC) of a day number, as its index from
/// the week of 1970-01-01.
pub fn week_index(day: i64) -> i64 {
    (day + 3).div_euclid(7)
}

/// A week index's `YYYY-Www` id.
pub fn week_id(index: i64) -> String {
    let thursday = index * 7;
    let year: i64 = civil_date(thursday)[..4].parse().unwrap_or(1970);
    let week = (thursday - days_from_civil(year, 1, 1)) / 7 + 1;
    format!("{year:04}-W{week:02}")
}

/// A `YYYY-Www` id's week index, when that week exists.
pub fn parse_week(id: &str) -> Option<i64> {
    let (year, week) = id.split_once("-W")?;
    if year.len() != 4 || week.len() != 2 {
        return None;
    }
    let (year, week): (i64, i64) = (year.parse().ok()?, week.parse().ok()?);
    let index = week_index(days_from_civil(year, 1, 4)) + week - 1;
    (week >= 1 && week_id(index) == id).then_some(index)
}

/// The week's end (the next Monday 00:00 UTC), Unix milliseconds.
pub fn week_end_ms(index: i64) -> i64 {
    (index * 7 + 4) * 86_400_000
}

/// One week's challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeeklyPick {
    /// `YYYY-Www`.
    pub week: String,
    pub course: &'static str,
    pub class: VehicleClass,
    /// The week's end, Unix milliseconds.
    pub ends_ms: i64,
}

impl WeeklyPick {
    pub fn board(&self) -> String {
        format!("weekly:{}", self.week)
    }
    pub fn laps(&self) -> u32 {
        crate::world_board::default_laps(self.course)
    }
}

fn mix(index: i64) -> u32 {
    let mut x = ((index as i32) ^ 0x5bd1_e995_u32 as i32) as u32;
    x = x.wrapping_mul(0x9e37_79b1);
    x = (x ^ (x >> 15)).wrapping_mul(0x2c1b_3c6d);
    x ^ (x >> 13)
}

/// The pick for week `index` (`weeklyPick` in `cloudflare/weekly.mjs`).
pub fn weekly(index: i64) -> WeeklyPick {
    let hash = mix(index);
    WeeklyPick {
        week: week_id(index),
        course: DAILY_POOL[hash as usize % DAILY_POOL.len()],
        class: if (hash >> 9) % 4 == 3 {
            VehicleClass::Motorcycle
        } else {
            VehicleClass::Atv
        },
        ends_ms: week_end_ms(index),
    }
}

/// How near under a medal time a ghost must be to stand for the medal
/// exactly (else its choice reads "~Gold"), thousandths of the medal time.
pub const MEDAL_GHOST_SLACK: u64 = 10;

/// A medal's ghost among a lap board's rows: the stored ghost nearest at or
/// under `target_ms`, else the nearest either side; with whether it is only
/// approximate (not within [`MEDAL_GHOST_SLACK`] under the medal time).
pub fn medal_ghost(entries: &[BoardEntry], target_ms: u32) -> Option<(&BoardEntry, bool)> {
    let target = i64::from(target_ms);
    let ghosts = entries.iter().filter(|e| e.ghost && !e.house);
    let under = ghosts
        .clone()
        .filter(|e| e.value <= target)
        .max_by_key(|e| e.value);
    let chosen = under.or_else(|| ghosts.min_by_key(|e| (e.value - target).abs()))?;
    let slack = (target * MEDAL_GHOST_SLACK as i64) / 1000;
    let exact = chosen.value <= target && target - chosen.value <= slack;
    Some((chosen, !exact))
}

/// The rival: the slowest stored ghost quicker than `mine` (the rider's
/// best), or the slowest stored ghost without one.
pub fn rival_ghost(entries: &[BoardEntry], mine: Option<i64>) -> Option<&BoardEntry> {
    entries
        .iter()
        .filter(|e| e.ghost && !e.house && mine.is_none_or(|m| e.value < m))
        .max_by_key(|e| e.value)
}

/// The board's best stored ghost.
pub fn top_ghost(entries: &[BoardEntry]) -> Option<&BoardEntry> {
    entries
        .iter()
        .filter(|e| e.ghost && !e.house)
        .min_by_key(|e| e.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_author_table_is_the_server_s_measured_laps() {
        let js = include_str!("../../../cloudflare/board_seeds.mjs");
        let start = js.find("export const MEASURED_LAP = {").expect("table");
        let end = start + js[start..].find("};").expect("table end");
        let rows: Vec<(String, u32)> = js[start..end]
            .lines()
            .skip(1)
            .filter_map(|line| {
                let (id, ms) = line.trim().trim_end_matches(',').split_once(':')?;
                Some((id.trim().to_string(), ms.trim().parse().ok()?))
            })
            .collect();
        let mine: Vec<(String, u32)> = AUTHOR
            .iter()
            .map(|(id, ms)| (id.to_string(), *ms))
            .collect();
        let mut sorted = rows.clone();
        sorted.sort();
        assert_eq!(mine, sorted);
        assert!(
            AUTHOR.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted for the search"
        );
    }

    #[test]
    fn medal_times_scale_the_author_time() {
        assert_eq!(author_ms("nat01"), Some(43_138));
        assert_eq!(medal_ms("nat01", Medal::Gold), Some(45_726));
        assert_eq!(medal_ms("nat01", Medal::Silver), Some(51_765));
        assert_eq!(medal_ms("nat01", Medal::Bronze), Some(64_707));
        assert_eq!(earned("nat01", 43_138), Some(Medal::Author));
        assert_eq!(earned("nat01", 43_139), Some(Medal::Gold));
        assert_eq!(earned("nat01", 51_765), Some(Medal::Silver));
        assert_eq!(earned("nat01", 64_708), None);
        assert_eq!(earned("nat09", 1), None, "no measurement, no medals");
        assert_eq!(next_medal("nat01", None), Some(Medal::Bronze));
        assert_eq!(next_medal("nat01", Some(50_000)), Some(Medal::Gold));
        assert_eq!(next_medal("nat01", Some(40_000)), Some(Medal::Author));
        assert_eq!(next_medal("nat09", None), None);
    }

    #[test]
    fn the_book_keeps_each_course_s_best_and_counts_medals() {
        let mut book = MedalBook::default();
        assert_eq!(
            book.record_lap("nat01", 60_000),
            Some((Medal::Bronze, true))
        );
        assert_eq!(
            book.record_lap("nat01", 62_000),
            Some((Medal::Bronze, false))
        );
        assert_eq!(book.best("nat01"), Some(60_000));
        assert_eq!(book.record_lap("nat01", 45_000), Some((Medal::Gold, true)));
        assert_eq!(book.record_lap("nat01", 99_000), None);
        assert_eq!(book.record_lap("sx01", 70_000), Some((Medal::Gold, true)));
        assert_eq!(book.counts(), [0, 0, 2, 0]);
        assert!(book.record_week("2026-W46", 300_000));
        assert!(!book.record_week("2026-W46", 301_000));
        assert_eq!(book.weeks["2026-W46"].value, 300_000);
        assert_eq!(book.unbadged("2026-W47"), ["2026-W46"]);
        assert!(book.unbadged("2026-W46").is_empty(), "the week is not over");
        let text = serde_json::to_string(&book).unwrap();
        assert_eq!(serde_json::from_str::<MedalBook>(&text).unwrap(), book);
        assert_eq!(
            serde_json::from_str::<MedalBook>("{}").unwrap(),
            MedalBook::default()
        );
    }

    #[test]
    fn iso_weeks_match_the_server() {
        // cloudflare/weekly.test.mjs pins the same weeks.
        let day = |y, m, d| days_from_civil(y, m, d);
        assert_eq!(civil_date(day(2026, 11, 14)), "2026-11-14");
        assert_eq!(week_id(week_index(day(2026, 11, 14))), "2026-W46");
        assert_eq!(week_id(week_index(day(2027, 1, 1))), "2026-W53");
        assert_eq!(week_id(week_index(day(2024, 12, 30))), "2025-W01");
        assert_eq!(week_id(week_index(0)), "1970-W01");
        assert_eq!(week_id(week_index(day(2026, 11, 15))), "2026-W46");
        assert_eq!(week_id(week_index(day(2026, 11, 16))), "2026-W47");
        assert_eq!(parse_week("2021-W53"), None);
        assert_eq!(parse_week("2026-W00"), None);
        assert_eq!(parse_week("2026-W46"), Some(week_index(day(2026, 11, 14))));
        assert_eq!(
            week_end_ms(parse_week("2026-W46").unwrap()),
            day(2026, 11, 16) * 86_400_000
        );
        let picks: Vec<String> = ["2026-W46", "2026-W47", "2027-W01"]
            .iter()
            .map(|id| {
                let p = weekly(parse_week(id).unwrap());
                assert_eq!(p.week, *id);
                format!("{}/{}", p.course, p.class.label().to_lowercase())
            })
            .collect();
        assert_eq!(picks, ["sx08/atv", "sx12/atv", "env06ends/atv"]);
    }

    #[test]
    fn badges_take_the_best_band_inside() {
        let cuts: BTreeMap<String, i64> = [("1", 100), ("10", 200), ("25", 300), ("50", 400)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let got: Vec<Badge> = [50, 100, 150, 300, 399, 401]
            .into_iter()
            .map(|v| Badge::of(v, &cuts))
            .collect();
        use Badge::*;
        assert_eq!(got, [Top1, Top1, Top10, Top25, Top50, Finished]);
        assert_eq!(Badge::of(1, &BTreeMap::new()), Finished);
    }

    fn row(rank: u32, value: i64, ghost: bool) -> BoardEntry {
        BoardEntry {
            rank,
            name: format!("R{rank}"),
            value,
            vehicle: 1,
            ghost,
            ..Default::default()
        }
    }

    #[test]
    fn ghost_picks_find_the_medal_rival_and_top_ghosts() {
        let rows = vec![
            row(1, 44_000, true),
            row(2, 45_500, false),
            row(3, 45_700, true),
            row(4, 50_000, true),
            row(5, 60_000, true),
            BoardEntry {
                house: true,
                ..row(6, 61_000, true)
            },
        ];
        // Gold 45,726: R3's 45,700 is within 1 % under it.
        let (gold, approx) = medal_ghost(&rows, 45_726).unwrap();
        assert_eq!((gold.name.as_str(), approx), ("R3", false));
        // Silver 51,765: R4's 50,000 is 3.4 % under, so "~Silver".
        let (silver, approx) = medal_ghost(&rows, 51_765).unwrap();
        assert_eq!((silver.name.as_str(), approx), ("R4", true));
        // Author 43,138: nothing under it; the nearest stands in.
        let (author, approx) = medal_ghost(&rows, 43_138).unwrap();
        assert_eq!((author.name.as_str(), approx), ("R1", true));
        assert_eq!(rival_ghost(&rows, Some(55_000)).unwrap().name, "R4");
        assert_eq!(rival_ghost(&rows, Some(45_700)).unwrap().name, "R1");
        assert_eq!(rival_ghost(&rows, Some(44_000)), None, "nobody ahead");
        assert_eq!(
            rival_ghost(&rows, None).unwrap().name,
            "R5",
            "house rows never"
        );
        assert_eq!(top_ghost(&rows).unwrap().name, "R1");
        assert_eq!(top_ghost(&rows[1..2]), None);
    }
}
