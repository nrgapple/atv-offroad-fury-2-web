//! Freestyle objective events and the Amateur/Pro Freestyle championships
//! (research/CHAMPIONSHIPS.md "Freestyle objectives").
//!
//! Exact-build evidence (private exports and the course `<scene>.obj`
//! files of the OB archives, bounded reading; no runtime traces):
//!
//! - A Freestyle course's objectives come from `[Objectives]`
//!   `NumObjectives` / `[Objective%d]` `Type` (`002fca60`: `CherryPick`,
//!   `Stunt`, `ComboMambo`). Every championship course ships four: Hit Every
//!   Green Target, Hit Every Red Target (CherryPick: `NumberOfTargets`,
//!   `BonusPointsPerTarget`, `BonusTimePerTargetInSec`, `002fa840`), Bust 10
//!   Different Combos (`CombosToWin`, `002fbe30`) and Score Stunt Points
//!   (`NumPointLevels`, `Points%d`). Six completion bits per course
//!   (`00321ff0`, counted by `0025a100` "Completed Course Objectives: %d of
//!   6"): the two target runs, the combos and the three point levels.
//! - The event runs against the 120 s limit `00289d50` stores at +0x57c.
//! - Stunt (`002fb720`): each point level the run's stunt score passes
//!   shows "%d Point Level Reached"; reaching the last completes it.
//! - ComboMambo (`002fc050`): a successful chain of two or more stunts that
//!   differs from every combo already counted adds one ("Repeated Combos Do
//!   Not Count" otherwise); `CombosToWin` completes it.
//! - CherryPick: one-time trigger spheres from the objective's TOY file;
//!   each hit adds the bonus points and bonus seconds; all targets complete
//!   it.
//! - Results (`002ddc50`, `002dde00`): "Congratulations, %s" / "Sorry, %s"
//!   and "You have completed the objective!" / "You have failed the
//!   objective.".
//! - Championship: Freestyle rounds always advance (`00322c60`); the trophy
//!   (`00322440`) counts the series' completed objectives (profile +0x1e0 +
//!   type*0x3c): Amateur 24/30/36 and Pro 18/24/30 for bronze/silver/gold,
//!   else "You have completed %d/36 objectives." (`002ddee0`). Rewards
//!   (`00322650`) Amateur 1000/500/250, Pro 2000/1000/500 on both
//!   difficulties; a new profile (`00320db0`) opens Amateur Freestyle on
//!   Normal and Expert.
use crate::Result;
use crate::championship::{Championships, Series, Trophy, save_key};

/// `00289d50` +0x57c: the Freestyle event's time limit, seconds.
pub const TIME_LIMIT: f32 = 120.0;

/// Objectives per course counted by the championship (`0025a100`).
pub const BITS_PER_COURSE: u32 = 6;

/// The two Freestyle championships, in the Championship menu's order.
pub const SERIES: [Series; 2] = [
    Series {
        kind: 3,
        key: "unspstunt",
        name: "Amateur Freestyle",
        pro: false,
        rounds: [6, 6],
        laps: 0,
        courses: &["env05", "env02", "env08", "env06", "env04", "env01"],
        opened_by: None,
        rewards: [[1000, 500, 250], [1000, 500, 250]],
    },
    Series {
        kind: 2,
        key: "spstunt",
        name: "Pro Freestyle",
        pro: true,
        rounds: [5, 5],
        laps: 0,
        courses: &["free01", "free04", "free02", "free06", "free03"],
        opened_by: Some("unspstunt"),
        rewards: [[2000, 1000, 500], [2000, 1000, 500]],
    },
];

pub fn series(key: &str) -> Option<&'static Series> {
    SERIES.iter().find(|s| s.key == key)
}

/// `00322440`: objectives completed for bronze, silver and gold.
pub fn trophy_thresholds(series: &Series) -> [u32; 3] {
    if series.pro {
        [18, 24, 30]
    } else {
        [24, 30, 36]
    }
}

/// All objectives a series offers (36 Amateur, 30 Pro).
pub fn total_objectives(series: &Series) -> u32 {
    series.rounds[0] * BITS_PER_COURSE
}

/// What an objective asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Hit every target of the objective's TOY file.
    CherryPick {
        targets: u32,
        bonus_points: f32,
        bonus_seconds: f32,
        toy: String,
    },
    /// Land this many different combos.
    ComboMambo { combos: u32 },
    /// Pass each stunt point level.
    Stunt { levels: Vec<f32> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub description: String,
    pub kind: Kind,
}

impl Objective {
    /// Completion bits this objective can set (`00321ff0` mask): the
    /// objective's own bit, or one per point level.
    pub fn bits(&self, index: usize) -> Vec<u32> {
        match &self.kind {
            Kind::Stunt { levels } => (0..levels.len() as u32).map(|l| 3 + l).collect(),
            _ => vec![index as u32],
        }
    }

    /// The port loads no target TOYs yet (they ship in the OB archives).
    pub fn playable(&self) -> bool {
        !matches!(self.kind, Kind::CherryPick { .. })
    }
}

/// Parses a course `.obj` file (`[Objectives]` and `[Objective%d]`).
pub fn parse_objectives(text: &str) -> Result<Vec<Objective>> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name.trim().to_ascii_lowercase(), Vec::new()));
        } else if let Some((k, v)) = line.split_once('=')
            && let Some((_, keys)) = sections.last_mut()
        {
            keys.push((
                k.trim().to_ascii_lowercase(),
                v.trim().trim_matches('"').to_string(),
            ));
        }
    }
    let get = |section: &str, key: &str| -> Option<String> {
        sections
            .iter()
            .find(|(n, _)| n == section)?
            .1
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let num = |section: &str, key: &str| -> Result<f32> {
        get(section, key)
            .ok_or_else(|| format!("[{section}] {key} missing"))?
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or_else(|| format!("[{section}] {key} invalid"))
    };
    let count = num("objectives", "numobjectives")? as usize;
    if count > 16 {
        return Err("too many objectives".into());
    }
    let mut objectives = Vec::with_capacity(count);
    for i in 0..count {
        let s = format!("objective{i}");
        let kind = match get(&s, "type")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "cherrypick" => Kind::CherryPick {
                targets: num(&s, "numberoftargets")? as u32,
                bonus_points: num(&s, "bonuspointspertarget").unwrap_or(1000.),
                bonus_seconds: num(&s, "bonustimepertargetinsec").unwrap_or(2.),
                toy: get(&s, "toyfile").unwrap_or_default(),
            },
            "combomambo" => Kind::ComboMambo {
                combos: num(&s, "combostowin").unwrap_or(10.) as u32,
            },
            "stunt" => {
                let n = num(&s, "numpointlevels")? as usize;
                if n == 0 || n > 3 {
                    return Err(format!("[{s}] point levels out of range"));
                }
                Kind::Stunt {
                    levels: (1..=n)
                        .map(|l| num(&s, &format!("points{l}")))
                        .collect::<Result<_>>()?,
                }
            }
            other => return Err(format!("[{s}] unknown objective type {other:?}")),
        };
        objectives.push(Objective {
            description: get(&s, "description").unwrap_or_default(),
            kind,
        });
    }
    Ok(objectives)
}

/// The championship courses' objectives as the OB archives' `.obj` files
/// define them (numbers recovered from the shipped files; the target
/// positions stay in the archives). Every course: green targets, red
/// targets, 10 combos and three point levels.
pub fn course_objectives(course: &str) -> Option<Vec<Objective>> {
    // (green targets, red targets, point levels)
    let (green, red, levels): (u32, u32, [f32; 3]) = match course {
        "env01" => (12, 8, [10000., 25000., 50000.]),
        "env02" => (18, 9, [10000., 25000., 50000.]),
        "env04" => (17, 8, [10000., 25000., 50000.]),
        "env05" => (13, 8, [10000., 25000., 50000.]),
        "env06" => (15, 8, [10000., 25000., 50000.]),
        "env08" => (19, 10, [10000., 25000., 50000.]),
        "free01" => (8, 8, [10000., 25000., 50000.]),
        "free02" => (13, 13, [10000., 25000., 50000.]),
        "free03" => (8, 8, [10000., 25000., 50000.]),
        "free04" => (31, 9, [10000., 25000., 100000.]),
        "free06" => (12, 6, [25000., 50000., 100000.]),
        _ => return None,
    };
    Some(vec![
        Objective {
            description: "Hit Every Green Target".into(),
            kind: Kind::CherryPick {
                targets: green,
                bonus_points: 500.,
                bonus_seconds: 10.,
                toy: format!("{course}gcp"),
            },
        },
        Objective {
            description: "Hit Every Red Target".into(),
            kind: Kind::CherryPick {
                targets: red,
                bonus_points: 1000.,
                bonus_seconds: 5.,
                toy: format!("{course}rcp"),
            },
        },
        Objective {
            description: "Bust 10 Different Combos".into(),
            kind: Kind::ComboMambo { combos: 10 },
        },
        Objective {
            description: "Score Stunt Points".into(),
            kind: Kind::Stunt {
                levels: levels.to_vec(),
            },
        },
    ])
}

/// How a finished objective ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Failed,
}

/// One objective attempt.
#[derive(Clone, Debug)]
pub struct Run {
    pub objective: Objective,
    pub index: usize,
    /// Seconds left (CherryPick hits add time).
    pub time_left: f32,
    /// Committed stunt points when the run started.
    baseline: f32,
    /// Stunt points scored in this run, plus target bonuses.
    pub score: f32,
    bonus: f32,
    /// Combos counted (stunt id sequences).
    combos: Vec<Vec<usize>>,
    /// Point levels passed.
    pub levels: u32,
    /// Targets hit.
    pub hits: Vec<bool>,
    pub outcome: Option<Outcome>,
    /// Messages for the HUD's centre line, newest last.
    pub messages: Vec<String>,
}

impl Run {
    pub fn new(objective: Objective, index: usize, committed: f32) -> Self {
        let targets = match &objective.kind {
            Kind::CherryPick { targets, .. } => *targets as usize,
            _ => 0,
        };
        Self {
            objective,
            index,
            time_left: TIME_LIMIT,
            baseline: committed,
            score: 0.,
            bonus: 0.,
            combos: Vec::new(),
            levels: 0,
            hits: vec![false; targets],
            outcome: None,
            messages: Vec::new(),
        }
    }

    pub fn finished(&self) -> bool {
        self.outcome.is_some()
    }

    fn complete(&mut self) {
        if self.outcome.is_none() {
            self.outcome = Some(Outcome::Completed);
        }
    }

    /// Advances the clock with the rider's committed stunt points; the
    /// run fails when time runs out short of the goal.
    pub fn tick(&mut self, dt: f32, committed: f32) {
        if self.finished() {
            return;
        }
        self.score = (committed - self.baseline).max(0.) + self.bonus;
        if let Kind::Stunt { levels } = &self.objective.kind {
            let levels = levels.clone();
            while (self.levels as usize) < levels.len()
                && self.score >= levels[self.levels as usize]
            {
                self.messages.push(format!(
                    "{} Point Level Reached",
                    levels[self.levels as usize] as i64
                ));
                self.levels += 1;
            }
            if self.levels as usize == levels.len() {
                self.complete();
                return;
            }
        }
        self.time_left = (self.time_left - dt.max(0.)).max(0.);
        if self.time_left <= 0. {
            self.outcome = Some(Outcome::Failed);
        }
    }

    /// A finished stunt chain: its success and stunt ids in order
    /// (`002fc050` counts chains of two or more, once per sequence).
    pub fn chain(&mut self, success: bool, ids: &[usize]) {
        if self.finished() || !success || ids.len() < 2 {
            return;
        }
        let Kind::ComboMambo { combos } = self.objective.kind else {
            return;
        };
        if self.combos.iter().any(|c| c == ids) {
            self.messages.push("Repeated Combos Do Not Count".into());
            return;
        }
        self.combos.push(ids.to_vec());
        if self.combos.len() as u32 >= combos {
            self.complete();
        }
    }

    /// A target was hit: its bonus points and seconds.
    pub fn hit(&mut self, target: usize) {
        if self.finished() {
            return;
        }
        let Kind::CherryPick {
            bonus_points,
            bonus_seconds,
            ..
        } = self.objective.kind
        else {
            return;
        };
        let Some(hit) = self.hits.get_mut(target).filter(|h| !**h) else {
            return;
        };
        *hit = true;
        self.bonus += bonus_points;
        self.score += bonus_points;
        self.time_left += bonus_seconds;
        if self.hits.iter().all(|h| *h) {
            self.complete();
        }
    }

    /// Combos counted so far.
    pub fn combos(&self) -> u32 {
        self.combos.len() as u32
    }

    /// The HUD's progress line.
    pub fn progress(&self) -> String {
        match &self.objective.kind {
            Kind::Stunt { levels } => match levels.get(self.levels as usize) {
                Some(next) => format!(
                    "Level {} of {}: {} pts",
                    self.levels + 1,
                    levels.len(),
                    *next as i64
                ),
                None => format!("All {} Levels Reached", levels.len()),
            },
            Kind::ComboMambo { combos } => format!("Combos {} of {combos}", self.combos()),
            Kind::CherryPick { targets, .. } => format!(
                "Targets {} of {targets}",
                self.hits.iter().filter(|h| **h).count()
            ),
        }
    }

    /// The completion bits this run earned (point levels count even when
    /// the run fails short of the last).
    pub fn earned_bits(&self) -> u8 {
        let bits = match &self.objective.kind {
            Kind::Stunt { .. } => (0..self.levels).map(|l| 3 + l).collect::<Vec<_>>(),
            _ if self.outcome == Some(Outcome::Completed) => vec![self.index as u32],
            _ => Vec::new(),
        };
        bits.into_iter()
            .filter(|b| *b < BITS_PER_COURSE)
            .fold(0, |m, b| m | (1 << b))
    }

    /// `002dde00`: the result line.
    pub fn result_line(&self) -> &'static str {
        match self.outcome {
            Some(Outcome::Completed) => "You have completed the objective!",
            _ => "You have failed the objective.",
        }
    }
}

/// The save key of a course's completion bits.
fn course_key(series: &Series, expert: bool, course: &str) -> String {
    format!("{}:{course}", save_key(series, expert))
}

/// What a finished Freestyle round did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundOutcome {
    /// The round finished (0-based) and the series' round count.
    pub round: u32,
    pub rounds: u32,
    /// Objectives completed in the series after the round.
    pub completed: u32,
    /// The championship ended: the trophy, its first-time reward and the
    /// series opened.
    pub finished: Option<(Option<Trophy>, i64, Vec<String>)>,
}

impl Championships {
    /// `00320db0`: Amateur Freestyle starts open on Normal and Expert.
    pub fn freestyle_open(&self, series: &Series, expert: bool) -> bool {
        !series.pro || self.opened.contains(&save_key(series, expert))
    }

    /// A course's completion bits on a difficulty.
    pub fn course_mask(&self, series: &Series, expert: bool, course: &str) -> u8 {
        self.objectives
            .get(&course_key(series, expert, course))
            .copied()
            .unwrap_or(0)
    }

    /// Adds an objective run's bits; returns the course's new mask.
    pub fn record_objective(
        &mut self,
        series: &Series,
        expert: bool,
        course: &str,
        bits: u8,
    ) -> u8 {
        let mask = self
            .objectives
            .entry(course_key(series, expert, course))
            .or_default();
        *mask |= bits & 0x3f;
        *mask
    }

    /// Objectives completed over the series' courses.
    pub fn completed_objectives(&self, series: &Series, expert: bool) -> u32 {
        let rounds = series.rounds[usize::from(expert)] as usize;
        series.courses[..rounds]
            .iter()
            .map(|c| self.course_mask(series, expert, c).count_ones())
            .sum()
    }

    /// The course of the series' next round.
    pub fn next_freestyle_course(&self, series: &Series, expert: bool) -> &'static str {
        self.next_course(series, expert)
    }

    /// Finishes the current round: Freestyle always advances; after the
    /// last round the trophy comes from the objectives completed.
    pub fn finish_freestyle_round(&mut self, series: &Series, expert: bool) -> RoundOutcome {
        let rounds = series.rounds[usize::from(expert)];
        let completed = self.completed_objectives(series, expert);
        let progress = self.progress.entry(save_key(series, expert)).or_default();
        let round = progress.round.min(rounds - 1);
        if round + 1 < rounds {
            progress.round = round + 1;
            return RoundOutcome {
                round,
                rounds,
                completed,
                finished: None,
            };
        }
        let [bronze, silver, gold] = trophy_thresholds(series);
        let won = if completed >= gold {
            Some(Trophy::Gold)
        } else if completed >= silver {
            Some(Trophy::Silver)
        } else if completed >= bronze {
            Some(Trophy::Bronze)
        } else {
            None
        };
        let reward = match won {
            Some(t) if progress.trophy.is_none_or(|best| t < best) => {
                progress.trophy = Some(t);
                series.rewards[usize::from(expert)][t as usize]
            }
            _ => 0,
        };
        *progress = crate::championship::Progress {
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
        opened.retain(|k| !self.opened.contains(k) && !(k.ends_with("-expert") && !series.pro));
        self.opened.extend(opened.iter().cloned());
        RoundOutcome {
            round,
            rounds,
            completed,
            finished: Some((won, reward, opened)),
        }
    }
}

/// `002ddee0`: the championship-over line.
pub fn trophy_line(series: &Series, outcome: &RoundOutcome) -> String {
    match &outcome.finished {
        Some((Some(t), reward, _)) => {
            let mut line = format!("You have won a {} trophy!", t.name().to_ascii_lowercase());
            if *reward > 0 {
                line += &format!(" and {reward} Profile Points!");
            }
            line
        }
        _ => format!(
            "You have completed {}/{} objectives.",
            outcome.completed,
            total_objectives(series)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJ: &str = "[Objectives]\r\nNumObjectives = 4\r\n\r\n;---\r\n; Hit\r\n[Objective0]\r\n\
Type=CherryPick\r\nNumberOfTargets=13\r\nBonusPointsPerTarget=500\r\nBonusTimePerTargetInSec=10\r\n\
Description=\"Hit Every Green Target\"\r\nTOYFile=env05gcp\r\n[Objective1]\r\nType=CherryPick\r\n\
NumberOfTargets=8\r\nBonusPointsPerTarget=1000\r\nBonusTimePerTargetInSec=5\r\n\
Description=\"Hit Every Red Target\"\r\nTOYFile=env05rcp\r\n[Objective2]\r\nType=ComboMambo\r\n\
CombosToWin=10\r\nDescription=\"Bust 10 Different Combos\"\r\nTOYFile=\r\n[Objective3]\r\n\
Type=Stunt\r\nNumPointLevels=3\r\nPoints1=10000.0\r\nPoints2=25000.0\r\nPoints3=50000.0\r\n\
Description=\"Score Stunt Points\"\r\nTOYFile=\r\n";

    #[test]
    fn parses_the_obj_layout_and_matches_the_course_table() {
        let parsed = parse_objectives(OBJ).unwrap();
        assert_eq!(parsed, course_objectives("env05").unwrap());
        assert!(
            parse_objectives("[Objectives]\nNumObjectives=1\n[Objective0]\nType=Foo\n").is_err()
        );
        for s in &SERIES {
            assert_eq!(s.courses.len(), s.rounds[0] as usize);
            for c in s.courses {
                let o = course_objectives(c).unwrap();
                let bits: Vec<u32> = o.iter().enumerate().flat_map(|(i, o)| o.bits(i)).collect();
                assert_eq!(bits, [0, 1, 2, 3, 4, 5], "{c}");
            }
        }
        assert_eq!(total_objectives(&SERIES[0]), 36);
        assert_eq!(total_objectives(&SERIES[1]), 30);
    }

    #[test]
    fn stunt_levels_count_as_reached_and_time_runs_out() {
        let o = course_objectives("free06").unwrap().remove(3);
        let mut run = Run::new(o, 3, 5000.);
        run.tick(1., 5000. + 30000.);
        assert_eq!(run.levels, 1);
        assert_eq!(run.messages, ["25000 Point Level Reached"]);
        assert_eq!(run.progress(), "Level 2 of 3: 50000 pts");
        run.tick(200., 35000.);
        assert_eq!(run.outcome, Some(Outcome::Failed));
        assert_eq!(run.earned_bits(), 0b1000);
        let mut run = Run::new(course_objectives("env05").unwrap().remove(3), 3, 0.);
        run.tick(TIME_LIMIT - 1., 50000.);
        assert_eq!(run.outcome, Some(Outcome::Completed));
        assert_eq!(run.earned_bits(), 0b111000);
        assert!(run.time_left > 0.);
    }

    #[test]
    fn combos_count_once_per_sequence() {
        let mut run = Run::new(course_objectives("env01").unwrap().remove(2), 2, 0.);
        run.chain(true, &[1]);
        run.chain(false, &[1, 2]);
        run.chain(true, &[1, 2]);
        run.chain(true, &[1, 2]);
        assert_eq!(run.combos(), 1);
        assert_eq!(run.messages.last().unwrap(), "Repeated Combos Do Not Count");
        run.chain(true, &[2, 1]);
        for i in 3..11 {
            run.chain(true, &[i, i]);
        }
        assert_eq!(run.combos(), 10);
        assert_eq!(run.outcome, Some(Outcome::Completed));
        assert_eq!(run.earned_bits(), 0b100);
        assert_eq!(run.progress(), "Combos 10 of 10");
    }

    #[test]
    fn targets_add_points_and_time() {
        let mut run = Run::new(course_objectives("free06").unwrap().remove(1), 1, 0.);
        run.tick(10., 0.);
        run.hit(0);
        run.hit(0);
        assert_eq!(run.time_left, TIME_LIMIT - 10. + 5.);
        assert_eq!(run.score, 1000.);
        for t in 1..6 {
            run.hit(t);
        }
        assert_eq!(run.outcome, Some(Outcome::Completed));
        assert_eq!(run.earned_bits(), 0b10);
    }

    #[test]
    fn rounds_always_advance_and_the_trophy_counts_objectives() {
        let am = series("unspstunt").unwrap();
        let pro = series("spstunt").unwrap();
        let mut c = Championships::default();
        assert!(c.freestyle_open(am, false) && c.freestyle_open(am, true));
        assert!(!c.freestyle_open(pro, false));
        // Five courses with all six objectives, the sixth with none.
        for (i, course) in am.courses[..5].iter().enumerate() {
            assert_eq!(c.next_freestyle_course(am, false), *course);
            assert_eq!(c.record_objective(am, false, course, 0x3f), 0x3f);
            let o = c.finish_freestyle_round(am, false);
            assert_eq!((o.round, o.finished.is_none()), (i as u32, true));
        }
        let o = c.finish_freestyle_round(am, false);
        assert_eq!(o.completed, 30);
        let (trophy, reward, opened) = o.finished.clone().unwrap();
        assert_eq!((trophy, reward), (Some(Trophy::Silver), 500));
        assert_eq!(opened, ["spstunt-normal"]);
        assert!(c.freestyle_open(pro, false));
        assert_eq!(
            trophy_line(am, &o),
            "You have won a silver trophy! and 500 Profile Points!"
        );
        // The bits stay: the next run starts over at round 1 with them.
        assert_eq!(c.progress(am, false).round, 0);
        c.record_objective(am, false, "env01", 0x3f);
        for _ in 0..5 {
            c.finish_freestyle_round(am, false);
        }
        let o = c.finish_freestyle_round(am, false);
        assert_eq!(o.finished.unwrap().0, Some(Trophy::Gold));
        // Pro: 17 objectives earn nothing.
        c.record_objective(pro, false, "free01", 0x3f);
        c.record_objective(pro, false, "free04", 0x3f);
        c.record_objective(pro, false, "free02", 0x1f);
        for _ in 0..4 {
            c.finish_freestyle_round(pro, false);
        }
        let o = c.finish_freestyle_round(pro, false);
        assert_eq!(o.finished.clone().unwrap().0, None);
        assert_eq!(trophy_line(pro, &o), "You have completed 17/30 objectives.");
    }
}
