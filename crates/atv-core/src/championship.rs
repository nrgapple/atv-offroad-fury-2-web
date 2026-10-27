//! Single-player racing championships (research/CHAMPIONSHIPS.md).
//!
//! Exact-build evidence (private exports, bounded reading):
//!
//! - Series are race event types (settings +8); their course lists come from
//!   `Tracks.dat` in file order (`0021cca0`) and their round counts, Normal
//!   and Expert, from a table read by `00322c20`. `00289d50` sets five laps
//!   for Nationals and Supercross, the championship flag (+0x36d) and the
//!   CPU difficulty (+0x610: 3 on Expert, else 2). Round N races list
//!   entry N (inferred from the course index = rounds completed).
//! - Championship points by place (`001f1db0`): 7, 4, 2, 1, 0.
//! - Advancing (`00322c60`, threshold `00322ed0`): after round r (0-based)
//!   the player needs 4r + 2 points in total; the final round always
//!   passes. Short of it, the race's points are dropped and the round is
//!   replayed; otherwise every rider's points are added (`003222b0`).
//! - Trophy (`00322440`): the player's rank among the five totals, ties to
//!   the player: gold, silver, bronze or none. Rewards (`00322650`, only the
//!   first time a trophy is won) from a per-series table, and unlocks
//!   (`003238a0`, Normal only): gold opens the series' Expert, any trophy in
//!   an Amateur series opens its Pro series. A new profile (`00320db0`)
//!   starts with the Amateur series open on Normal.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A racing championship series (an original event type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Series {
    /// Original event type (settings +8).
    pub kind: u8,
    /// `Tracks.dat` list key, also the save key.
    pub key: &'static str,
    pub name: &'static str,
    pub pro: bool,
    /// Rounds on Normal and Expert.
    pub rounds: [u32; 2],
    pub laps: u32,
    /// Course ids in `Tracks.dat` order (Expert plays one more).
    pub courses: &'static [&'static str],
    /// The Amateur series whose trophy opens this Pro series.
    pub opened_by: Option<&'static str>,
    /// First-trophy Profile Points: Normal gold/silver/bronze, then Expert.
    pub rewards: [[i64; 3]; 2],
}

/// The racing series the port can run, in event-type order. Amateur Enduro
/// races each venue's short Enduro and Pro Enduro its long one (`002f65e0`,
/// `0021d8b0`), three laps. Short Tracks opens only with the GOLDCUPS or
/// IGIVEUP cheat (`003238a0` is called for type 8 by nothing else found);
/// the Freestyle championships (objectives) are not offered.
pub const SERIES: [Series; 7] = [
    Series {
        kind: 0,
        key: "send",
        name: "Amateur Enduro",
        pro: false,
        rounds: [5, 6],
        laps: 3,
        courses: &[
            "env05ends",
            "env02ends",
            "env08ends",
            "env06ends",
            "env04ends",
            "env01ends",
        ],
        opened_by: None,
        rewards: [[500, 250, 125], [1500, 750, 375]],
    },
    Series {
        kind: 1,
        key: "lend",
        name: "Pro Enduro",
        pro: true,
        rounds: [5, 6],
        laps: 3,
        courses: &[
            "env05endl",
            "env02endl",
            "env08endl",
            "env06endl",
            "env04endl",
            "env01endl",
        ],
        opened_by: Some("send"),
        rewards: [[1000, 500, 250], [2000, 1000, 500]],
    },
    Series {
        kind: 5,
        key: "unspnat",
        name: "Amateur Nationals",
        pro: false,
        rounds: [5, 6],
        laps: 5,
        courses: &["env05", "env02", "env08", "env06", "env04", "env01"],
        opened_by: None,
        rewards: [[500, 250, 125], [1500, 750, 375]],
    },
    Series {
        kind: 4,
        key: "spnat",
        name: "Pro Nationals",
        pro: true,
        rounds: [6, 7],
        laps: 5,
        courses: &[
            "nat01", "nat02", "nat03", "nat04", "nat05", "nat06", "nat07",
        ],
        opened_by: Some("unspnat"),
        rewards: [[1000, 500, 250], [2000, 1000, 500]],
    },
    Series {
        kind: 6,
        key: "amsx",
        name: "Amateur Supercross",
        pro: false,
        rounds: [6, 7],
        laps: 5,
        courses: &["sx08", "sx12", "sx01", "sx13", "sx02", "sx06", "sx05"],
        opened_by: None,
        rewards: [[500, 250, 125], [1500, 750, 375]],
    },
    Series {
        kind: 7,
        key: "prosx",
        name: "Pro Supercross",
        pro: true,
        rounds: [6, 7],
        laps: 5,
        courses: &["sx03", "sx07", "sx16", "sx04", "sx10", "sx15", "sx14"],
        opened_by: Some("amsx"),
        rewards: [[1000, 500, 250], [2000, 1000, 500]],
    },
    Series {
        kind: 8,
        key: "st",
        name: "Short Tracks",
        pro: false,
        rounds: [3, 3],
        laps: 5,
        courses: &["env06st", "env01st", "env07st"],
        opened_by: None,
        rewards: [[0, 0, 0], [0, 0, 0]],
    },
];

pub fn series(key: &str) -> Option<&'static Series> {
    SERIES.iter().find(|s| s.key == key)
}

/// A championship round's context on the race frame: the series, the
/// difficulty and the course's Profile Points table for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Round {
    pub series: &'static str,
    pub expert: bool,
    pub awards: [i32; 5],
}

/// Championship points by finishing place 1–5 (`001f1db0`).
pub const PLACE_POINTS: [i32; 5] = [7, 4, 2, 1, 0];

pub fn place_points(place: usize) -> i32 {
    place
        .checked_sub(1)
        .and_then(|i| PLACE_POINTS.get(i))
        .copied()
        .unwrap_or(0)
}

/// `00322ed0`: the total needed after round `round` (0-based).
pub fn required(round: u32) -> i32 {
    4 * round as i32 + 2
}

/// `00322d80`: the place a round needs, from the points still missing;
/// None is "N/A" (eight or more missing, or the final round).
pub fn required_place(points: i32, round: u32, rounds: u32) -> Option<usize> {
    if round + 1 >= rounds {
        return None;
    }
    match (required(round) - points).max(0) {
        0 => Some(5),
        1 => Some(4),
        2 => Some(3),
        3..=4 => Some(2),
        5..=7 => Some(1),
        _ => None,
    }
}

/// The championship loading screen (`002591f0`): the round, the points and
/// what the round needs.
pub fn briefing(series: &Series, progress: &Progress, expert: bool) -> Vec<String> {
    let rounds = series.rounds[usize::from(expert)];
    let round = progress.round.min(rounds - 1);
    let points = progress.points[0];
    let place = match required_place(points, round, rounds) {
        Some(place) => format!(
            "{place}{}",
            match place {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            }
        ),
        None => "N/A".into(),
    };
    vec![
        format!(
            "{} ({})",
            series.name,
            if expert { "Expert" } else { "Normal" }
        ),
        format!("You Are Currently On Track: {} of {rounds}", round + 1),
        format!("Current Championship Points: {points}"),
        format!(
            "Points Required to Advance: {}",
            (required(round) - points).max(0)
        ),
        format!("Place Required to Advance: {place}"),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Trophy {
    Gold,
    Silver,
    Bronze,
}

impl Trophy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gold => "Gold",
            Self::Silver => "Silver",
            Self::Bronze => "Bronze",
        }
    }
}

/// `00322440`: riders with strictly more points rank above the player.
pub fn trophy(points: &[i32; 5]) -> Option<Trophy> {
    match points[1..].iter().filter(|p| **p > points[0]).count() {
        0 => Some(Trophy::Gold),
        1 => Some(Trophy::Silver),
        2 => Some(Trophy::Bronze),
        _ => None,
    }
}

/// One series and difficulty: the rounds cleared and the standings (the
/// player, then the four CPU slots).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub round: u32,
    pub points: [i32; 5],
    /// The best trophy won so far.
    pub trophy: Option<Trophy>,
}

/// What a finished round did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Championship points this race, per slot.
    pub race_points: [i32; 5],
    /// The round counted (else it is replayed).
    pub advanced: bool,
    /// The round raced (0-based) and the series' round count.
    pub round: u32,
    pub rounds: u32,
    /// The player's total after the round (with the race when advanced).
    pub total: i32,
    /// The total the round needed.
    pub required: i32,
    /// The championship ended: the trophy, its first-time reward and the
    /// series opened.
    pub finished: Option<(Option<Trophy>, i64, Vec<String>)>,
}

/// The save key of a series on a difficulty.
pub fn save_key(series: &Series, expert: bool) -> String {
    format!(
        "{}-{}",
        series.key,
        if expert { "expert" } else { "normal" }
    )
}

/// Championship progress of the player's profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Championships {
    pub progress: BTreeMap<String, Progress>,
    /// Series opened beyond a new profile's (`save_key`s).
    pub opened: Vec<String>,
    /// Freestyle objective bits per `save_key:course` (`00321ff0`).
    pub objectives: BTreeMap<String, u8>,
}

impl Championships {
    /// `00320db0`: the Amateur series start open on Normal (Short Tracks
    /// does not).
    pub fn is_open(&self, series: &Series, expert: bool) -> bool {
        (!series.pro && !expert && series.kind != 8)
            || self.opened.contains(&save_key(series, expert))
    }

    pub fn progress(&self, series: &Series, expert: bool) -> Progress {
        self.progress
            .get(&save_key(series, expert))
            .cloned()
            .unwrap_or_default()
    }

    /// The course of the series' next round.
    pub fn next_course(&self, series: &Series, expert: bool) -> &'static str {
        let round = self.progress(series, expert).round as usize;
        series.courses[round.min(series.courses.len() - 1)]
    }

    /// Records a finished round from each slot's place (the player, then
    /// the CPUs; missing CPU slots score nothing).
    pub fn record(&mut self, series: &Series, expert: bool, places: &[usize]) -> Outcome {
        let key = save_key(series, expert);
        let rounds = series.rounds[usize::from(expert)];
        let progress = self.progress.entry(key).or_default();
        let round = progress.round.min(rounds - 1);
        let mut race_points = [0; 5];
        for (slot, place) in places.iter().take(5).enumerate() {
            race_points[slot] = place_points(*place);
        }
        let last = round + 1 == rounds;
        let required = required(round);
        let advanced = last || progress.points[0] + race_points[0] >= required;
        if advanced {
            for (total, add) in progress.points.iter_mut().zip(race_points) {
                *total += add;
            }
        }
        let total = progress.points[0];
        let mut finished = None;
        if advanced && last {
            let won = trophy(&progress.points);
            let reward = match won {
                Some(t) if progress.trophy.is_none_or(|best| t < best) => {
                    let table = series.rewards[usize::from(expert)];
                    // A better trophy pays its own reward once.
                    progress.trophy = Some(t);
                    table[t as usize]
                }
                _ => 0,
            };
            *progress = Progress {
                trophy: progress.trophy,
                ..Default::default()
            };
            let mut opened = Vec::new();
            if !expert && won.is_some() {
                if won == Some(Trophy::Gold) {
                    opened.push(save_key(series, true));
                }
                if let Some(pro) = SERIES.iter().find(|s| s.opened_by == Some(series.key)) {
                    opened.push(save_key(pro, false));
                }
            }
            opened.retain(|k| !self.opened.contains(k));
            self.opened.extend(opened.iter().cloned());
            finished = Some((won, reward, opened));
        } else if advanced {
            progress.round = round + 1;
        }
        Outcome {
            race_points,
            advanced,
            round,
            rounds,
            total,
            required,
            finished,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_and_courses_follow_the_original_lists() {
        for s in &SERIES {
            assert!(s.courses.len() >= s.rounds[1] as usize, "{}", s.key);
            assert!(
                s.courses
                    .iter()
                    .all(|c| ["env", "nat", "sx"].iter().any(|f| c.starts_with(f)))
            );
        }
        assert_eq!(series("amsx").unwrap().courses[2], "sx01");
        assert_eq!(series("send").unwrap().courses[0], "env05ends");
        assert_eq!(series("lend").unwrap().opened_by, Some("send"));
        assert_eq!(series("lend").unwrap().laps, 3);
        assert_eq!(
            (1..=5).map(place_points).collect::<Vec<_>>(),
            [7, 4, 2, 1, 0]
        );
        assert_eq!(
            (0..6).map(required).collect::<Vec<_>>(),
            [2, 6, 10, 14, 18, 22]
        );
    }

    #[test]
    fn a_short_round_is_replayed_without_points() {
        let s = series("unspnat").unwrap();
        let mut c = Championships::default();
        // Fifth earns 0 of the 2 needed: nothing counts.
        let o = c.record(s, false, &[5, 1, 2, 3, 4]);
        assert!(!o.advanced);
        assert_eq!((o.round, o.total, o.required), (0, 0, 2));
        assert_eq!(c.progress(s, false), Progress::default());
        assert_eq!(c.next_course(s, false), "env05");
        // Third (2 points) clears round 1; every slot scores.
        let o = c.record(s, false, &[3, 1, 2, 4, 5]);
        assert!(o.advanced);
        assert_eq!(c.progress(s, false).points, [2, 7, 4, 1, 0]);
        assert_eq!(c.next_course(s, false), "env02");
    }

    #[test]
    fn the_final_round_awards_the_trophy_once_and_opens_series() {
        let s = series("amsx").unwrap();
        let mut c = Championships::default();
        assert!(c.is_open(s, false) && !c.is_open(s, true));
        assert!(!c.is_open(series("prosx").unwrap(), false));
        for _ in 0..5 {
            assert!(c.record(s, false, &[1, 2, 3, 4, 5]).advanced);
        }
        let o = c.record(s, false, &[5, 1, 2, 3, 4]);
        let (trophy, reward, opened) = o.finished.unwrap();
        assert_eq!((trophy, reward), (Some(Trophy::Gold), 500));
        assert_eq!(opened, ["amsx-expert", "prosx-normal"]);
        assert!(c.is_open(s, true) && c.is_open(series("prosx").unwrap(), false));
        // The series starts over; a second gold pays nothing.
        assert_eq!(c.progress(s, false).round, 0);
        for _ in 0..5 {
            c.record(s, false, &[1, 2, 3, 4, 5]);
        }
        let (_, reward, opened) = c.record(s, false, &[1, 2, 3, 4, 5]).finished.unwrap();
        assert_eq!((reward, opened.len()), (0, 0));
    }

    #[test]
    fn briefing_follows_the_loading_screen() {
        let s = series("amsx").unwrap();
        let p = Progress {
            round: 2,
            points: [7, 0, 0, 0, 0],
            trophy: None,
        };
        // Round 3 needs 10: 3 missing is 2nd place.
        assert_eq!(
            briefing(s, &p, false)[1..],
            [
                "You Are Currently On Track: 3 of 6",
                "Current Championship Points: 7",
                "Points Required to Advance: 3",
                "Place Required to Advance: 2nd",
            ]
        );
        assert_eq!(required_place(10, 2, 6), Some(5));
        assert_eq!(required_place(9, 2, 6), Some(4));
        assert_eq!(required_place(5, 2, 6), Some(1));
        assert_eq!(required_place(2, 2, 6), None);
        assert_eq!(required_place(0, 5, 6), None);
    }

    #[test]
    fn trophy_ranks_ties_to_the_player() {
        assert_eq!(trophy(&[10, 10, 3, 2, 1]), Some(Trophy::Gold));
        assert_eq!(trophy(&[10, 12, 11, 2, 1]), Some(Trophy::Bronze));
        assert_eq!(trophy(&[1, 12, 11, 2, 1]), None);
    }
}
