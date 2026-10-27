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
use std::collections::BTreeMap;

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

/// One course's record in the profile (the 0x94-byte course record that
/// `001f1e30` fills and `00322080` stores): races and victories, the best
/// lap and best full-length race time, and victories per ATV. Times are
/// whole milliseconds so the record compares exactly.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CourseRecord {
    pub races: u32,
    pub wins: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_lap_ms: Option<u32>,
    /// Best total time of a full-length race (3 laps Enduro, 5 otherwise:
    /// `001f1e30` records times only at the default lap count).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_total_ms: Option<u32>,
    /// Victories per vehicle id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub atv_wins: BTreeMap<u16, u32>,
}

impl CourseRecord {
    pub fn most_winning_atv(&self) -> Option<u16> {
        most_wins(&self.atv_wins)
    }
}

/// The vehicle with the most victories (the lowest id on a tie).
fn most_wins(wins: &BTreeMap<u16, u32>) -> Option<u16> {
    wins.iter()
        .filter(|(_, n)| **n > 0)
        .fold(None, |best: Option<(u16, u32)>, (id, n)| match best {
            Some((_, b)) if b >= *n => best,
            _ => Some((*id, *n)),
        })
        .map(|(id, _)| id)
}

/// A finished race as the profile records it (`001f1e30` → `00322080`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaceResult {
    /// The race course id (`env05`, `sx01`, `env05ends`...).
    pub course: String,
    /// The player's vehicle id.
    pub quad: u16,
    /// Finishing place, 1-based.
    pub place: usize,
    /// The player's best lap and total time, in seconds.
    pub best_lap: Option<f32>,
    pub total: Option<f32>,
    /// The race ran the series' default laps (3 Enduro, 5 otherwise).
    pub full_length: bool,
}

/// Longest name a profile takes: the name entry's eight cells
/// (`Profile_NameEntry` "XXXXXXXX" / "________").
pub const NAME_MAX: usize = 8;
/// The name entry's default (the "PLAYER1" string beside the Profile
/// Editor page name, `0x4347b8`).
pub const DEFAULT_NAME: &str = "PLAYER1";
/// Profiles one memory card holds ("Player Profile save file 1"–"4",
/// manual p. 25: "up to four profiles on one memory card").
pub const SLOTS: usize = 4;

/// The player's profile: its name, lifetime Profile Points and the race
/// record kept by `00322080` (races, victories, victories per ATV and the
/// per-course records). The host stores it as JSON; a missing or unreadable
/// record reads as a new profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// The save file name (at most [`NAME_MAX`] characters). Saves from
    /// before profile names read empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Profile Points available to spend.
    pub points: i64,
    /// Profile Points earned over the profile's life (the Overview's
    /// "Profile Points Earned"); older saves read 0 and show `points`.
    pub earned: i64,
    pub races: u32,
    pub wins: u32,
    /// Best single-race stunt score.
    pub best_stunt: i64,
    /// Championship rounds, standings, trophies and opened series.
    pub championships: crate::championship::Championships,
    /// Profile Store purchases, earned unlocks and the points spent
    /// (`store`, research/PROFILE_STORE.md).
    pub store: crate::store::Unlocks,
    /// Victories per vehicle id (profile +0x2c, one counter per ATV).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub atv_wins: BTreeMap<u16, u32>,
    /// Per-course records by course id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub courses: BTreeMap<String, CourseRecord>,
}

impl Profile {
    /// A new profile with this name (`00320db0`).
    pub fn named(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Adds earned Profile Points (`00323440`): race awards and trophy
    /// rewards.
    pub fn credit(&mut self, points: i64) {
        self.earned = self.earned_points().saturating_add(points);
        self.points = self.points.saturating_add(points);
    }

    /// Lifetime Profile Points (never below the points still held).
    pub fn earned_points(&self) -> i64 {
        self.earned.max(self.points)
    }

    /// Credits one finished race (`00323440` points, `00322080` record).
    pub fn record_race(&mut self, place: usize, awarded: i32, stunt_score: f32) {
        self.credit(i64::from(awarded));
        self.races = self.races.saturating_add(1);
        if place == 1 {
            self.wins = self.wins.saturating_add(1);
        }
        self.best_stunt = self.best_stunt.max(stunt_score.max(0.) as i64);
    }

    /// Records a championship round and its course unlocks: an advanced
    /// round opens the series' next course for other modes and a gold or
    /// silver trophy the first course beyond the Normal series (`003222b0`,
    /// `00322650` → `00323640`; the games' lists the port lacks are not
    /// cascaded).
    pub fn record_championship(
        &mut self,
        series: &crate::championship::Series,
        expert: bool,
        places: &[usize],
    ) -> crate::championship::Outcome {
        use crate::championship::Trophy;
        let outcome = self.championships.record(series, expert, places);
        let mut open = Vec::new();
        if outcome.advanced && outcome.finished.is_none() {
            open.push(outcome.round as u16 + 1);
        }
        if let Some((Some(Trophy::Gold | Trophy::Silver), ..)) = outcome.finished {
            open.push(series.rounds[0] as u16);
        }
        if let Some(key) = crate::store::EVENT_LISTS
            .get(usize::from(series.kind))
            .filter(|_| !open.is_empty())
        {
            let list = self.store.courses.entry(key.to_string()).or_default();
            for index in open {
                if !list.contains(&index) {
                    list.push(index);
                }
            }
        }
        outcome
    }

    /// Records the race on its course and vehicle (`001f1e30`'s course
    /// record and `00322080`'s per-ATV victories; best lap and total time
    /// as `00324b40` keeps them).
    pub fn record_course(&mut self, result: &RaceResult) {
        let won = result.place == 1;
        if won {
            *self.atv_wins.entry(result.quad).or_default() += 1;
        }
        let course = self.courses.entry(result.course.clone()).or_default();
        course.races = course.races.saturating_add(1);
        if won {
            course.wins = course.wins.saturating_add(1);
            *course.atv_wins.entry(result.quad).or_default() += 1;
        }
        let ms = |t: f32| (t.is_finite() && t > 0.).then(|| (t * 1000.).round() as u32);
        let better = |old: Option<u32>, new: Option<u32>| match (old, new) {
            (Some(o), Some(n)) => Some(o.min(n)),
            (o, n) => o.or(n),
        };
        course.best_lap_ms = better(course.best_lap_ms, result.best_lap.and_then(ms));
        if result.full_length {
            course.best_total_ms = better(course.best_total_ms, result.total.and_then(ms));
        }
    }

    /// The ATV with the most victories ("Most Winning ATV").
    pub fn most_winning_atv(&self) -> Option<u16> {
        most_wins(&self.atv_wins)
    }

    /// Victories over races completed, in percent ("Win Percentage").
    pub fn win_percentage(&self) -> f32 {
        if self.races == 0 {
            0.
        } else {
            self.wins as f32 * 100. / self.races as f32
        }
    }

    /// "Percent Complete" (`00320db0` sets the total at +0x6c0 to 201;
    /// +0x6c1 counts each item unlocked, `00323a00`, and the trophy score,
    /// gold 3, silver 2, bronze 1). Unlocks the port records one by one
    /// count; the unlock cheats, which the port keeps as flags, do not.
    pub fn percent_complete(&self) -> f32 {
        use crate::championship::{SERIES, Trophy};
        let mut score = self.store.unlocked_count();
        for series in &SERIES {
            for expert in [false, true] {
                score += match self.championships.progress(series, expert).trophy {
                    Some(Trophy::Gold) => 3,
                    Some(Trophy::Silver) => 2,
                    Some(Trophy::Bronze) => 1,
                    None => 0,
                };
            }
        }
        (score as f32 * 100. / COMPLETE_TOTAL as f32).min(100.)
    }
}

/// `00320db0`: the Percent Complete total (+0x6c0 = 0xc9).
pub const COMPLETE_TOTAL: usize = 201;

/// Why a profile name or book change was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookError {
    /// No characters after trimming.
    EmptyName,
    /// Another profile already has this name.
    Duplicate,
    /// All [`SLOTS`] save files are in use.
    Full,
    /// No profile at that index.
    Missing,
}

impl BookError {
    pub fn message(self) -> &'static str {
        match self {
            Self::EmptyName => "Enter a profile name.",
            Self::Duplicate => "A profile with that name already exists.",
            Self::Full => "All four profile save files are in use. Delete a profile first.",
            Self::Missing => "That profile no longer exists.",
        }
    }
}

/// Whether the name entry offers this character: letters (upper case),
/// digits, space and a few marks. Authored: the original's
/// `namenumber_entry` glyph set is not recovered.
pub fn name_char(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || " -_.!".contains(c)
}

/// The characters the controller's letter row steps through.
pub const NAME_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.! ";

/// A typed name as the entry keeps it: upper case, offered characters
/// only, at most [`NAME_MAX`] characters (leading spaces dropped).
pub fn clean_name(text: &str) -> String {
    text.chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| name_char(*c))
        .skip_while(|c| *c == ' ')
        .take(NAME_MAX)
        .collect()
}

/// The saved profiles (the memory card's save files) and the loaded one.
/// The original keeps one loaded profile in memory and autosaves it to its
/// card; the port keeps the book in local storage (PROFILE.md).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileBook {
    /// The loaded profile's index, if any.
    pub current: Option<usize>,
    pub profiles: Vec<Profile>,
}

impl ProfileBook {
    /// The book for a save from before profile books: the implicit profile
    /// becomes the loaded "PLAYER1" (a new one when there was none).
    pub fn migrate(old: Option<Profile>) -> Self {
        let mut profile = old.unwrap_or_default();
        if profile.name.is_empty() {
            profile.name = DEFAULT_NAME.into();
        }
        Self {
            current: Some(0),
            profiles: vec![profile],
        }
    }

    /// Drops an out-of-range selection and blank names (a hand-edited or
    /// damaged save).
    pub fn normalize(&mut self) {
        self.profiles.truncate(SLOTS);
        for (i, profile) in self.profiles.iter_mut().enumerate() {
            profile.name = clean_name(&profile.name).trim_end().to_string();
            if profile.name.is_empty() {
                profile.name = format!("PLAYER{}", i + 1);
            }
        }
        if self.current.is_some_and(|i| i >= self.profiles.len()) {
            self.current = None;
        }
    }

    pub fn current_profile(&self) -> Option<&Profile> {
        self.profiles.get(self.current?)
    }

    pub fn current_mut(&mut self) -> Option<&mut Profile> {
        self.profiles.get_mut(self.current?)
    }

    pub fn is_full(&self) -> bool {
        self.profiles.len() >= SLOTS
    }

    /// A valid, unused name (trimmed, cleaned).
    pub fn check_name(&self, name: &str, except: Option<usize>) -> Result<String, BookError> {
        let name = clean_name(name).trim_end().to_string();
        if name.is_empty() {
            return Err(BookError::EmptyName);
        }
        if self
            .profiles
            .iter()
            .enumerate()
            .any(|(i, p)| Some(i) != except && p.name.eq_ignore_ascii_case(&name))
        {
            return Err(BookError::Duplicate);
        }
        Ok(name)
    }

    /// The name entry's starting text: "PLAYER1", or the next unused
    /// "PLAYERn".
    pub fn suggested_name(&self) -> String {
        (1..)
            .map(|n| format!("PLAYER{n}"))
            .find(|name| self.check_name(name, None).is_ok())
            .unwrap_or_else(|| DEFAULT_NAME.into())
    }

    /// Create Profile: a new profile, saved and loaded.
    pub fn create(&mut self, name: &str) -> Result<usize, BookError> {
        if self.is_full() {
            return Err(BookError::Full);
        }
        let name = self.check_name(name, None)?;
        self.profiles.push(Profile::named(&name));
        let index = self.profiles.len() - 1;
        self.current = Some(index);
        Ok(index)
    }

    /// Load Profile.
    pub fn load(&mut self, index: usize) -> Result<(), BookError> {
        if index >= self.profiles.len() {
            return Err(BookError::Missing);
        }
        self.current = Some(index);
        Ok(())
    }

    /// Copy Profile: a copy saved under a new name; the loaded profile
    /// stays loaded.
    pub fn copy(&mut self, index: usize, name: &str) -> Result<usize, BookError> {
        let source = self.profiles.get(index).ok_or(BookError::Missing)?.clone();
        if self.is_full() {
            return Err(BookError::Full);
        }
        let name = self.check_name(name, None)?;
        self.profiles.push(Profile { name, ..source });
        Ok(self.profiles.len() - 1)
    }

    /// Delete Profile. Deleting the loaded profile leaves none loaded.
    pub fn delete(&mut self, index: usize) -> Result<Profile, BookError> {
        if index >= self.profiles.len() {
            return Err(BookError::Missing);
        }
        let removed = self.profiles.remove(index);
        self.current = match self.current {
            Some(c) if c == index => None,
            Some(c) if c > index => Some(c - 1),
            other => other,
        };
        Ok(removed)
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
    fn championship_rounds_unlock_courses() {
        let series = crate::championship::series("unspnat").unwrap();
        let mut p = Profile::default();
        // A replayed round unlocks nothing; an advanced one the next course.
        p.record_championship(series, false, &[5, 1, 2, 3, 4]);
        assert!(p.store.courses.is_empty());
        p.record_championship(series, false, &[1, 2, 3, 4, 5]);
        assert_eq!(p.store.courses["unspnat"], [1]);
        for _ in 0..4 {
            p.record_championship(series, false, &[1, 2, 3, 4, 5]);
        }
        // Gold also opens the course after the five Normal rounds.
        assert_eq!(p.store.courses["unspnat"], [1, 2, 3, 4, 5]);
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

    #[test]
    fn course_records_keep_bests_and_atv_victories() {
        let mut p = Profile::default();
        let race = |place, quad, lap, total, full| RaceResult {
            course: "sx01".into(),
            quad,
            place,
            best_lap: Some(lap),
            total: Some(total),
            full_length: full,
        };
        p.record_course(&race(1, 4, 41.25, 210.5, true));
        p.record_course(&race(2, 4, 40.0, 205.0, false));
        p.record_course(&race(1, 7, 42.0, 220.0, true));
        p.record_course(&race(1, 7, 43.0, 230.0, true));
        let c = &p.courses["sx01"];
        assert_eq!((c.races, c.wins), (4, 3));
        assert_eq!(c.best_lap_ms, Some(40_000));
        // The short race's time is not a full-length record.
        assert_eq!(c.best_total_ms, Some(210_500));
        assert_eq!(c.most_winning_atv(), Some(7));
        assert_eq!(p.most_winning_atv(), Some(7));
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Profile>(&json).unwrap(), p);
    }

    #[test]
    fn earned_points_survive_spending_and_old_saves() {
        let mut p: Profile = serde_json::from_str("{\"points\":120,\"races\":3}").unwrap();
        assert_eq!((p.name.as_str(), p.earned_points()), ("", 120));
        p.credit(30);
        assert_eq!((p.points, p.earned_points()), (150, 150));
        p.points -= 100;
        assert_eq!((p.points, p.earned_points()), (50, 150));
        p.record_race(4, 0, 0.);
        assert_eq!(p.races, 4);
        assert!((p.win_percentage() - 0.).abs() < 1e-6);
    }

    #[test]
    fn names_are_cleaned_and_unique() {
        assert_eq!(clean_name("  joe's rider 99"), "JOES RID");
        assert_eq!(clean_name("ab-c_d.!x"), "AB-C_D.!");
        let mut book = ProfileBook::default();
        assert_eq!(book.suggested_name(), "PLAYER1");
        assert_eq!(book.create("  "), Err(BookError::EmptyName));
        assert_eq!(book.create("ace"), Ok(0));
        assert_eq!(book.create("ACE"), Err(BookError::Duplicate));
        assert_eq!(book.current_profile().unwrap().name, "ACE");
        assert_eq!(book.suggested_name(), "PLAYER1");
        book.create("player1").unwrap();
        assert_eq!(book.suggested_name(), "PLAYER2");
    }

    #[test]
    fn book_creates_loads_copies_and_deletes() {
        let mut book = ProfileBook::default();
        book.create("ONE").unwrap();
        book.current_mut().unwrap().credit(50);
        book.create("TWO").unwrap();
        assert_eq!(book.current, Some(1));
        book.load(0).unwrap();
        assert_eq!(book.current_profile().unwrap().points, 50);
        // A copy keeps the record under its own name; ONE stays loaded.
        assert_eq!(book.copy(0, "THREE"), Ok(2));
        assert_eq!(book.profiles[2].points, 50);
        assert_eq!(book.current, Some(0));
        book.create("FOUR").unwrap();
        assert_eq!(book.create("FIVE"), Err(BookError::Full));
        assert_eq!(book.copy(1, "FIVE"), Err(BookError::Full));
        // Deleting before the loaded one keeps it loaded.
        book.load(3).unwrap();
        book.delete(1).unwrap();
        assert_eq!(book.current_profile().unwrap().name, "FOUR");
        // Deleting the loaded profile leaves none loaded.
        book.delete(2).unwrap();
        assert_eq!(book.current, None);
        assert_eq!(book.load(5), Err(BookError::Missing));
        assert_eq!(book.delete(5), Err(BookError::Missing));
    }

    #[test]
    fn an_old_save_migrates_into_the_book() {
        let old: Profile = serde_json::from_str("{\"points\":7,\"wins\":2}").unwrap();
        let book = ProfileBook::migrate(Some(old));
        let p = book.current_profile().unwrap();
        assert_eq!((p.name.as_str(), p.points, p.wins), ("PLAYER1", 7, 2));
        assert_eq!(ProfileBook::migrate(None).profiles[0].points, 0);
        let mut odd: ProfileBook =
            serde_json::from_str("{\"current\":9,\"profiles\":[{\"name\":\"\"}]}").unwrap();
        odd.normalize();
        assert_eq!(
            (odd.current, odd.profiles[0].name.as_str()),
            (None, "PLAYER1")
        );
    }
}
