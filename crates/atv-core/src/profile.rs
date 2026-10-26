//! Race Profile Points and the player's profile record.
//!
//! Exact-build evidence (private exports, bounded reading; research/PROFILE.md):
//!
//! - `0020a8d0` reads the race TOY `[ChampionshipInfo]` section: the
//!   `ChampQuadGroup` and, for places 1–5, `AmNormalPoints%d`,
//!   `ProNormalPoints%d`, `AmHardPoints%d` and `ProHardPoints%d` (missing
//!   keys read 0).
//! - `001f1e30` awards a human racer's Profile Points at the finish, only in
//!   single player (settings +0x10 = 1): outside a championship 1st, 2nd and
//!   3rd earn 10, 5 and 1; in a championship the course table indexed by Pro
//!   (`0021db00`) and Hard (+0x1890 = 2) for places 1–3. It then adds the
//!   race stunt score / 1000 (truncated), unless the rider has no profile or
//!   the place is 5, which earn 0. The total goes to the profile
//!   (`00323440`) and its race record (`00322080`: races, wins).
use serde::{Deserialize, Serialize};

/// Course Profile Points by finishing place 1–5 (`0020a8d0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CourseAwards {
    pub am_normal: [i32; 5],
    pub pro_normal: [i32; 5],
    pub am_hard: [i32; 5],
    pub pro_hard: [i32; 5],
}

impl CourseAwards {
    /// The race TOY `[ChampionshipInfo]` keys; None without the section.
    pub fn parse(toy: &str) -> Option<Self> {
        let mut awards = Self::default();
        let mut inside = false;
        let mut found = false;
        for line in toy.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                if inside {
                    break;
                }
                inside = name.trim().eq_ignore_ascii_case("ChampionshipInfo");
                found |= inside;
                continue;
            }
            if !inside {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            for (prefix, table) in [
                ("amnormalpoints", &mut awards.am_normal),
                ("pronormalpoints", &mut awards.pro_normal),
                ("amhardpoints", &mut awards.am_hard),
                ("prohardpoints", &mut awards.pro_hard),
            ] {
                if let Some(place) = key
                    .strip_prefix(prefix)
                    .and_then(|n| n.parse::<usize>().ok())
                    .filter(|n| (1..=5).contains(n))
                {
                    table[place - 1] = leading_int(value);
                }
            }
        }
        found.then_some(awards)
    }

    /// The table a championship uses: Pro and Hard pick the column.
    pub fn table(&self, pro: bool, hard: bool) -> &[i32; 5] {
        match (pro, hard) {
            (false, false) => &self.am_normal,
            (true, false) => &self.pro_normal,
            (false, true) => &self.am_hard,
            (true, true) => &self.pro_hard,
        }
    }
}

/// `atoi`: optional sign, then digits; anything else reads 0.
fn leading_int(value: &str) -> i32 {
    let value = value.trim_start();
    let end = value
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || (*i == 0 && matches!(c, '-' | '+'))))
        .map_or(value.len(), |(i, _)| i);
    value[..end].parse().unwrap_or(0)
}

/// How a race awards its place points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// A single race: 10, 5 and 1 for places 1–3.
    Single,
    /// A championship round: the course table for the series and difficulty.
    Championship {
        awards: &'a CourseAwards,
        pro: bool,
        hard: bool,
    },
}

/// Single-race place points (`001f1e30`).
pub const SINGLE_RACE_POINTS: [i32; 3] = [10, 5, 1];

/// `001f1e30`: a human's Profile Points for finishing `place` (1-based)
/// with `stunt_score` race stunt points.
pub fn race_award(event: Event, place: usize, stunt_score: f32) -> i32 {
    if place == 5 || place == 0 {
        return 0;
    }
    let placed = match event {
        Event::Single => SINGLE_RACE_POINTS.get(place - 1).copied().unwrap_or(0),
        Event::Championship { awards, pro, hard } if place <= 3 => {
            awards.table(pro, hard)[place - 1]
        }
        Event::Championship { .. } => 0,
    };
    // (int)((float)score / 1000.0): truncation toward zero.
    placed.saturating_add((stunt_score / 1000.) as i32)
}

/// The player's profile: lifetime Profile Points and the race record kept by
/// `00322080` (races and wins). The host stores it as JSON; a missing or
/// unreadable record reads as a new profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub points: i64,
    pub races: u32,
    pub wins: u32,
    /// Best single-race stunt score.
    pub best_stunt: i64,
}

impl Profile {
    /// Credits one finished race (`00323440` points, `00322080` record).
    pub fn record_race(&mut self, place: usize, awarded: i32, stunt_score: f32) {
        self.points = self.points.saturating_add(i64::from(awarded));
        self.races = self.races.saturating_add(1);
        if place == 1 {
            self.wins = self.wins.saturating_add(1);
        }
        self.best_stunt = self.best_stunt.max(stunt_score.max(0.) as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOY: &str = "[SceneInfo]\r\nRiderSpacing=10.0 \r\n[ChampionshipInfo]\r\n\
        ;Profile points awarded according to race order\r\nChampQuadGroup=1\r\n\
        AmNormalPoints1=20\r\nAmNormalPoints2=10\r\nAmNormalPoints3=5\r\n\
        AmHardPoints1=60\r\nAmHardPoints2=30 ; note\r\nAmHardPoints3=15\r\n\
        ProNormalPoints1=40\r\n[Other]\r\nProHardPoints1=99\r\n";

    #[test]
    fn reads_the_course_tables() {
        let a = CourseAwards::parse(TOY).unwrap();
        assert_eq!(a.am_normal, [20, 10, 5, 0, 0]);
        assert_eq!(a.am_hard, [60, 30, 15, 0, 0]);
        assert_eq!(a.pro_normal, [40, 0, 0, 0, 0]);
        // Keys outside the section are not read.
        assert_eq!(a.pro_hard, [0; 5]);
        assert_eq!(a.table(false, true), &a.am_hard);
        assert_eq!(CourseAwards::parse("[SceneInfo]\nRiderSpacing=8"), None);
    }

    #[test]
    fn single_race_award_follows_001f1e30() {
        assert_eq!(race_award(Event::Single, 1, 0.), 10);
        assert_eq!(race_award(Event::Single, 2, 1999.), 6);
        assert_eq!(race_award(Event::Single, 3, 4500.), 5);
        // Fourth earns only the stunt share; fifth earns nothing.
        assert_eq!(race_award(Event::Single, 4, 2500.), 2);
        assert_eq!(race_award(Event::Single, 5, 9000.), 0);
    }

    #[test]
    fn championship_award_uses_the_course_table() {
        let a = CourseAwards::parse(TOY).unwrap();
        let hard = Event::Championship {
            awards: &a,
            pro: false,
            hard: true,
        };
        assert_eq!(race_award(hard, 1, 0.), 60);
        assert_eq!(race_award(hard, 3, 1000.), 16);
        assert_eq!(race_award(hard, 4, 1000.), 1);
    }

    #[test]
    fn profile_records_races() {
        let mut p = Profile {
            points: 5,
            ..Default::default()
        };
        p.record_race(1, 12, 2400.);
        p.record_race(3, 1, 100.);
        assert_eq!((p.points, p.races, p.wins, p.best_stunt), (18, 2, 1, 2400));
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Profile>(&json).unwrap(), p);
        assert_eq!(
            serde_json::from_str::<Profile>("{\"points\":7}")
                .unwrap()
                .points,
            7
        );
    }
}
