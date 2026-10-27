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

/// `2fd980`: score per whole second left when an objective completes.
pub const TIME_BONUS_PER_SECOND: f32 = 100.;

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
    /// `DefaultStartPosition` / `DefaultStartDirection`: where the
    /// objective places the rider (absent from the built-in table).
    pub start: Option<crate::games::Start>,
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

    /// Whether the objective rides on the hosted package: the target runs
    /// need the course's OB archive (`has_archive`).
    pub fn playable(&self, has_archive: bool) -> bool {
        has_archive || !matches!(self.kind, Kind::CherryPick { .. })
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
        // Three leading components, as a "%f,%f,%f" read takes them:
        // env04's green start ships "2428.03,149,4460,47".
        let vector = |key: &str| -> Option<[f32; 3]> {
            let v: Vec<f32> = get(&s, key)?
                .split(',')
                .take(3)
                .map(|c| c.trim().parse::<f32>())
                .collect::<std::result::Result<_, _>>()
                .ok()?;
            (v.len() == 3 && v.iter().all(|c| c.is_finite())).then(|| [v[0], v[1], v[2]])
        };
        objectives.push(Objective {
            description: get(&s, "description").unwrap_or_default(),
            kind,
            start: vector("defaultstartposition").map(|position| crate::games::Start {
                position,
                direction: vector("defaultstartdirection").unwrap_or([0., 0., 1.]),
            }),
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
            start: None,
        },
        Objective {
            description: "Hit Every Red Target".into(),
            kind: Kind::CherryPick {
                targets: red,
                bonus_points: 1000.,
                bonus_seconds: 5.,
                toy: format!("{course}rcp"),
            },
            start: None,
        },
        Objective {
            description: "Bust 10 Different Combos".into(),
            kind: Kind::ComboMambo { combos: 10 },
            start: None,
        },
        Objective {
            description: "Score Stunt Points".into(),
            kind: Kind::Stunt {
                levels: levels.to_vec(),
            },
            start: None,
        },
    ])
}

/// A championship course's objective archive (`TRACKS/*/<x>_OB.RES`):
/// the course `.obj`, the green/red target TOYs and the cherry models.
pub fn objective_archive(course: &str) -> Option<String> {
    course_objectives(course)?;
    let (folder, stem) = if let Some(n) = course.strip_prefix("env") {
        (format!("ENV{n}"), format!("E{n}"))
    } else {
        let n = course.strip_prefix("free")?;
        (format!("FREE{n}"), format!("F{n}"))
    };
    Some(format!("/TRACKS/TRACKSA/{folder}/{stem}_OB.RES"))
}

/// A collision body registered with the objective's triggers, in world
/// space. `002fae60` (CherryPick reset) registers, per player, the quad's
/// four wheel bodies (+0x168..+0x174), its chassis rigid body (+0x18c),
/// the `ao.col` chassis hull (+0x97c), the `ar.col` actor rays (+0x980)
/// and the passive `f.col` actor (+0x984) with each trigger (`00172440`
/// clears their mutual exclusions); the rider's own bodies are not
/// registered. The wheels are spheres and the ray bundles segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriggerBody {
    Sphere { center: [f32; 3], radius: f32 },
    Segment { start: [f32; 3], end: [f32; 3] },
}

impl TriggerBody {
    /// The body touches the trigger sphere at `center` with `radius`.
    pub fn touches(&self, center: [f32; 3], radius: f32) -> bool {
        let d2 = |p: [f32; 3]| -> f32 { (0..3).map(|a| (p[a] - center[a]).powi(2)).sum() };
        match *self {
            Self::Sphere {
                center: c,
                radius: r,
            } => d2(c) <= (radius + r).powi(2),
            Self::Segment { start, end } => {
                let d: [f32; 3] = std::array::from_fn(|a| end[a] - start[a]);
                let len2: f32 = d.iter().map(|v| v * v).sum();
                let t = if len2 > 0. {
                    ((0..3).map(|a| (center[a] - start[a]) * d[a]).sum::<f32>() / len2)
                        .clamp(0., 1.)
                } else {
                    0.
                };
                d2(std::array::from_fn(|a| start[a] + d[a] * t)) <= radius * radius
            }
        }
    }
}

/// One target of a CherryPick TOY: a one-time trigger sphere whose
/// actions hide its cherry animation, run the `Cherry` action and start
/// its explosion emitter.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Target {
    pub name: String,
    /// The trigger sphere (`Position`, `Radius`).
    pub position: [f32; 3],
    pub radius: f32,
    /// The hidden animation (`AnimationNameRef`) and its `[AnimationN]`
    /// model (`MCF`, the cherry PSG stem) at `Offset`.
    pub animation: String,
    pub model: String,
    pub offset: [f32; 3],
    /// `Particles` action: `EmitterNumber` (1-based).
    pub emitter: Option<u32>,
    /// Every `Particles` action within `NumberOfActions`, in order (the
    /// target's explosion; `0023f3c0`).
    pub particles: Vec<crate::ambient_effects::ParticleAction>,
}

/// The targets of a CherryPick TOY (`[Triggers]` with a `Cherry` action;
/// `0023ff80`), in trigger order.
pub fn parse_targets(text: &str) -> Vec<Target> {
    parse_animated_triggers(text, "Cherry")
}

/// A TOY's triggers carrying `action` (Freestyle's `Cherry`, Treasure
/// Hunt's `Treasure`) with the animation their `HideAnimation` hides: its
/// `MCF` stem (the PSG/MOT pair) and `Offset`.
pub fn parse_animated_triggers(text: &str, action: &str) -> Vec<Target> {
    use crate::games::{Toy, parse_triggers};
    let toy = Toy::parse(text);
    let animations = toy
        .int("Animations", "NumberOfAnimations")
        .unwrap_or(0)
        .clamp(0, 512);
    let animation = |reference: &str| {
        (1..=animations).find_map(|n| {
            let section = format!("Animation{n}");
            toy.get(&section, "ReferenceName")
                .filter(|r| r.eq_ignore_ascii_case(reference))?;
            let mcf = toy.get(&section, "MCF")?;
            let stem = mcf.rsplit_once('.').map_or(mcf, |(stem, _)| stem);
            Some((
                stem.to_ascii_lowercase(),
                toy.vec3(&section, "Offset").unwrap_or([0.; 3]),
            ))
        })
    };
    parse_triggers(&toy)
        .into_iter()
        .filter(|t| t.action(action).is_some())
        .map(|t| {
            let reference = t
                .action("HideAnimation")
                .and_then(|a| a.get("animationnameref"))
                .cloned()
                .unwrap_or_default();
            let (model, offset) = animation(&reference).unwrap_or_default();
            Target {
                name: t.name.clone(),
                position: t.position,
                radius: if t.radius > 0. { t.radius } else { 5. },
                animation: reference,
                model,
                offset,
                emitter: t
                    .action("Particles")
                    .and_then(|a| a.get("emitternumber"))
                    .and_then(|n| n.trim().parse().ok()),
                particles: t
                    .actions
                    .iter()
                    .filter(|(kind, _)| kind.eq_ignore_ascii_case("Particles"))
                    .filter_map(|(_, fields)| crate::ambient_effects::ParticleAction::parse(fields))
                    .collect(),
            }
        })
        .collect()
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
    /// A timed sign (`002e9510`): the target signs show for 1 s, the last
    /// for 5 s; seconds left.
    pub sign: Option<(String, f32)>,
}

/// `002fab40`: "Target Collected" and "All Targets Collected" seconds.
pub const TARGET_SIGN: f32 = 1.;
pub const ALL_TARGETS_SIGN: f32 = 5.;

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
            sign: None,
        }
    }

    /// Restart: the same objective from its start (timer, score, combos,
    /// levels and targets), on the rider's current committed points.
    pub fn restart(&mut self, committed: f32) {
        *self = Self::new(self.objective.clone(), self.index, committed);
    }

    pub fn finished(&self) -> bool {
        self.outcome.is_some()
    }

    /// Completes the run: `2fd980` adds the whole seconds left x 100 to
    /// the score of a completed objective (every type).
    fn complete(&mut self) {
        if self.outcome.is_none() {
            self.outcome = Some(Outcome::Completed);
            let bonus = self.time_left.max(0.).floor() * TIME_BONUS_PER_SECOND;
            self.bonus += bonus;
            self.score += bonus;
        }
    }

    /// Advances the clock with the rider's committed stunt points; the
    /// run fails when time runs out short of the goal.
    pub fn tick(&mut self, dt: f32, committed: f32) {
        if let Some((_, left)) = &mut self.sign {
            *left -= dt.max(0.);
            if *left <= 0. {
                self.sign = None;
            }
        }
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
        // `2fab40` signs (Overlays.pb `[Single-Sign-8/9]`, `-10/11]`).
        if self.hits.iter().all(|h| *h) {
            self.sign = Some(("All Targets Collected".into(), ALL_TARGETS_SIGN));
            self.complete();
        } else {
            self.sign = Some(("Target Collected".into(), TARGET_SIGN));
        }
    }

    /// A point touching the targets' spheres: each untouched one is hit
    /// once. Returns the new hits.
    pub fn touch(&mut self, targets: &[Target], point: [f32; 3]) -> Vec<usize> {
        self.touch_bodies(
            targets,
            &[TriggerBody::Sphere {
                center: point,
                radius: 0.,
            }],
        )
    }

    /// Any of the player's registered bodies (`002fae60`) touching an
    /// untouched target's sphere hits it once (`00172bc0`, `OneTimeOnly`).
    /// Returns the new hits.
    pub fn touch_bodies(&mut self, targets: &[Target], bodies: &[TriggerBody]) -> Vec<usize> {
        let mut hit = Vec::new();
        for (i, t) in targets.iter().enumerate() {
            if self.finished() || self.hits.get(i).is_none_or(|h| *h) {
                continue;
            }
            if bodies.iter().any(|b| b.touches(t.position, t.radius)) {
                self.hit(i);
                hit.push(i);
            }
        }
        hit
    }

    /// Targets hit so far.
    pub fn targets_hit(&self) -> usize {
        self.hits.iter().filter(|h| **h).count()
    }

    /// The HUD's notice: a showing sign, else the latest message.
    pub fn notice(&self) -> Option<&str> {
        self.sign
            .as_ref()
            .map(|(text, _)| text.as_str())
            .or(self.messages.last().map(String::as_str))
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

/// A trophy `003230f0` awarded mid-championship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Award {
    pub trophy: Trophy,
    /// Profile Points paid (`00323440`; one reward per level reached).
    pub reward: i64,
    /// Objectives completed in the series.
    pub completed: u32,
    /// Series opened (save keys).
    pub opened: Vec<String>,
}

impl Award {
    /// The Objective Championship Results line (`002ddee0`).
    pub fn line(&self) -> String {
        let mut line = format!(
            "You have won a {} trophy! (+{} Profile Points)",
            self.trophy.name().to_ascii_lowercase(),
            self.reward
        );
        for key in &self.opened {
            if let Some(s) = key.strip_suffix("-normal").and_then(series) {
                line += &format!(" {} opened.", s.name);
            }
            if let Some(s) = key.strip_suffix("-expert").and_then(series) {
                line += &format!(" {} Expert opened.", s.name);
            }
        }
        line
    }
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

    /// A championship objective's bits (`003230f0`): each newly set bit
    /// adds one to the series' count, and the count reaching a trophy
    /// threshold awards that trophy at once, pays its reward (each level
    /// reached pays) and opens what a trophy opens. Returns the course's
    /// new mask and the award, if any.
    pub fn award_objective(
        &mut self,
        series: &Series,
        expert: bool,
        course: &str,
        bits: u8,
    ) -> (u8, Option<Award>) {
        let before = self.completed_objectives(series, expert);
        let mask = self.record_objective(series, expert, course, bits);
        let after = self.completed_objectives(series, expert);
        let thresholds = trophy_thresholds(series);
        let levels = [Trophy::Bronze, Trophy::Silver, Trophy::Gold];
        let mut award: Option<Award> = None;
        for (level, threshold) in levels.into_iter().zip(thresholds) {
            if before < threshold && after >= threshold {
                let reward = series.rewards[usize::from(expert)][level as usize];
                let a = award.get_or_insert(Award {
                    trophy: level,
                    reward: 0,
                    completed: after,
                    opened: Vec::new(),
                });
                a.trophy = level;
                a.reward += reward;
            }
        }
        let Some(mut award) = award else {
            return (mask, None);
        };
        let progress = self.progress.entry(save_key(series, expert)).or_default();
        if progress.trophy.is_none_or(|best| award.trophy < best) {
            progress.trophy = Some(award.trophy);
        }
        let mut opened = Vec::new();
        if !expert {
            if award.trophy == Trophy::Gold {
                opened.push(save_key(series, true));
            }
            if let Some(pro) = SERIES.iter().find(|s| s.opened_by == Some(series.key)) {
                opened.push(save_key(pro, false));
            }
        }
        opened.retain(|k| !self.opened.contains(k) && !(k.ends_with("-expert") && !series.pro));
        self.opened.extend(opened.iter().cloned());
        award.opened = opened;
        (mask, Some(award))
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
                line += &format!(" (+{reward} Profile Points)");
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
Description=\"Hit Every Green Target\"\r\nTOYFile=env05gcp\r\nDefaultStartPosition=2256,137,3269.44,7\r\n\
DefaultStartDirection=.996,0,-.088\r\n[Objective1]\r\nType=CherryPick\r\n\
NumberOfTargets=8\r\nBonusPointsPerTarget=1000\r\nBonusTimePerTargetInSec=5\r\n\
Description=\"Hit Every Red Target\"\r\nTOYFile=env05rcp\r\n[Objective2]\r\nType=ComboMambo\r\n\
CombosToWin=10\r\nDescription=\"Bust 10 Different Combos\"\r\nTOYFile=\r\n[Objective3]\r\n\
Type=Stunt\r\nNumPointLevels=3\r\nPoints1=10000.0\r\nPoints2=25000.0\r\nPoints3=50000.0\r\n\
Description=\"Score Stunt Points\"\r\nTOYFile=\r\n";

    #[test]
    fn parses_the_obj_layout_and_matches_the_course_table() {
        let mut parsed = parse_objectives(OBJ).unwrap();
        assert_eq!(
            parsed[0].start,
            Some(crate::games::Start {
                position: [2256., 137., 3269.44],
                direction: [0.996, 0., -0.088]
            })
        );
        assert_eq!(parsed[1].start, None);
        for o in &mut parsed {
            o.start = None;
        }
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
    fn restart_starts_the_run_over() {
        let mut run = Run::new(course_objectives("env01").unwrap().remove(2), 2, 100.);
        run.tick(50., 2600.);
        run.chain(true, &[1, 2]);
        run.chain(true, &[1, 2]);
        assert_eq!((run.combos(), run.score), (1, 2500.));
        run.restart(2600.);
        assert_eq!(run.time_left, TIME_LIMIT);
        assert_eq!((run.combos(), run.score, run.levels), (0, 0., 0));
        assert!(run.messages.is_empty() && run.outcome.is_none());
        // The counted combo counts again.
        run.chain(true, &[1, 2]);
        assert_eq!(run.combos(), 1);
        let mut run = Run::new(course_objectives("env01").unwrap().remove(3), 3, 0.);
        run.tick(1., 30000.);
        assert_eq!(run.levels, 2);
        run.restart(30000.);
        run.tick(1., 30000.);
        assert_eq!((run.levels, run.earned_bits()), (0, 0));
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

    /// The layout of the OB archives' target TOYs (`env01gcp.toy`).
    const TARGETS: &str = "[Triggers]\r\nNumberOfTriggers=2\r\n[Trigger1]\r\nName=Cherry1\r\n\
NumberOfActions=3\r\nZone=1\r\nOneTimeOnly=T\r\nPosition=100,20,300\r\nRadius=5.0\r\n\
[Trigger1Action1]\r\nActionType=HideAnimation\r\nAnimationNameRef=cherry1r\r\n\
[Trigger1Action2]\r\nActionType=Cherry\r\n[Trigger1Action3]\r\nActionType=Particles\r\n\
EmitterNumber=1\r\nInstanceNumber=1\r\nTurnOn=T\r\n[Trigger2]\r\nName=Cherry2\r\n\
NumberOfActions=2\r\nOneTimeOnly=T\r\nPosition=200,20,300\r\nRadius=5.0\r\n\
[Trigger2Action1]\r\nActionType=HideAnimation\r\nAnimationNameRef=cherry2r\r\n\
[Trigger2Action2]\r\nActionType=Cherry\r\n[Animations]\r\nNumberOfAnimations=2\r\n\
[Animation1]\r\nMCF=cherryr.mcf\r\nIsGlobal=T\r\nOffset=100,20,300\r\nReferenceName=cherry1r\r\n\
[Animation2]\r\nMCF=cherryr.mcf\r\nOffset=200,20,300\r\nReferenceName=cherry2r\r\n";

    #[test]
    fn registered_bodies_touch_the_trigger_sphere() {
        let c = [0., 0., 0.];
        // Wheel spheres: centre distance within both radii.
        let wheel = |x| TriggerBody::Sphere {
            center: [x, 0., 0.],
            radius: 1.,
        };
        assert!(wheel(6.).touches(c, 5.));
        assert!(!wheel(6.01).touches(c, 5.));
        // Ray segments: any point of the ray inside the sphere, including a
        // ray passing straight through it with both ends outside.
        let ray = |a: [f32; 3], b: [f32; 3]| TriggerBody::Segment { start: a, end: b };
        assert!(ray([-9., 4., 0.], [9., 4., 0.]).touches(c, 5.));
        assert!(!ray([-9., 5.1, 0.], [9., 5.1, 0.]).touches(c, 5.));
        assert!(!ray([6., 0., 0.], [9., 0., 0.]).touches(c, 5.));
        assert!(ray([4., 0., 0.], [4., 0., 0.]).touches(c, 5.));
    }

    #[test]
    fn targets_come_from_the_toy_and_hit_once() {
        let targets = parse_targets(TARGETS);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].model, "cherryr");
        assert_eq!(targets[0].animation, "cherry1r");
        assert_eq!(targets[0].emitter, Some(1));
        assert_eq!(
            (targets[1].offset, targets[1].radius),
            ([200., 20., 300.], 5.)
        );
        assert_eq!(
            objective_archive("free06").as_deref(),
            Some("/TRACKS/TRACKSA/FREE06/F06_OB.RES")
        );
        assert_eq!(
            objective_archive("env01").as_deref(),
            Some("/TRACKS/TRACKSA/ENV01/E01_OB.RES")
        );
        assert_eq!(objective_archive("env03"), None);
        let mut objective = course_objectives("free06").unwrap().remove(1);
        objective.kind = Kind::CherryPick {
            targets: 2,
            bonus_points: 1000.,
            bonus_seconds: 5.,
            toy: "x".into(),
        };
        let mut run = Run::new(objective, 1, 0.);
        assert!(run.touch(&targets, [106., 20., 300.]).is_empty());
        assert_eq!(run.touch(&targets, [104., 21., 301.]), [0]);
        // One-time: touching it again counts nothing.
        assert!(run.touch(&targets, [100., 20., 300.]).is_empty());
        assert_eq!((run.targets_hit(), run.score), (1, 1000.));
        assert_eq!(run.progress(), "Targets 1 of 2");
        assert_eq!(run.notice(), Some("Target Collected"));
        // The sign shows for one second.
        run.tick(0.5, 0.);
        assert_eq!(run.notice(), Some("Target Collected"));
        run.tick(0.6, 0.);
        assert_eq!(run.notice(), None);
        assert!(run.messages.is_empty());
        // A wheel sphere reaching the trigger's edge hits it; the ATV
        // origin far outside does not matter.
        let wheel = TriggerBody::Sphere {
            center: [207., 20., 300.],
            radius: 2.5,
        };
        assert!(run.touch(&targets, [230., 20., 300.]).is_empty());
        assert_eq!(run.touch_bodies(&targets, &[wheel]), [1]);
        assert_eq!(run.outcome, Some(Outcome::Completed));
        assert_eq!(run.notice(), Some("All Targets Collected"));
        run.tick(4.9, 0.);
        assert_eq!(run.notice(), Some("All Targets Collected"));
        run.tick(0.2, 0.);
        assert_eq!(run.notice(), None);
        // `2fd980`: the 128 whole seconds left (120 - 1.1 + 2 x 5) x 100.
        assert_eq!(run.score, 2000. + 12800.);
        assert_eq!(run.earned_bits(), 0b10);
        assert!(objective_playable_rules());
    }

    fn objective_playable_rules() -> bool {
        let o = course_objectives("env01").unwrap();
        !o[0].playable(false) && o[0].playable(true) && o[2].playable(false)
    }

    #[test]
    fn reaching_a_threshold_awards_the_trophy_at_once() {
        let am = series("unspstunt").unwrap();
        let mut c = Championships::default();
        for course in &am.courses[..3] {
            assert_eq!(c.award_objective(am, false, course, 0x3f).1, None);
        }
        // 18 -> 23: nothing; the stunt run's last level is the 24th.
        assert_eq!(c.award_objective(am, false, "env04", 0b11111).1, None);
        let (mask, award) = c.award_objective(am, false, "env04", 0b111000);
        assert_eq!(mask, 0x3f);
        let award = award.unwrap();
        assert_eq!(
            (award.trophy, award.reward, award.completed),
            (Trophy::Bronze, 250, 24)
        );
        assert_eq!(award.opened, ["spstunt-normal"]);
        assert_eq!(
            award.line(),
            "You have won a bronze trophy! (+250 Profile Points) Pro Freestyle opened."
        );
        assert_eq!(c.progress(am, false).trophy, Some(Trophy::Bronze));
        // A bit already set counts nothing.
        assert_eq!(c.award_objective(am, false, "env04", 0x3f).1, None);
        // 28, then 30: silver; 36: gold.
        assert_eq!(c.award_objective(am, false, "env01", 0x0f).1, None);
        let award = c.award_objective(am, false, "env01", 0x30).1.unwrap();
        assert_eq!((award.trophy, award.reward), (Trophy::Silver, 500));
        assert!(award.opened.is_empty());
        let award = c.award_objective(am, false, "env06", 0x3f).1.unwrap();
        assert_eq!((award.trophy, award.reward), (Trophy::Gold, 1000));
        // Amateur Freestyle Expert is open from the start (`00320db0`).
        assert!(award.opened.is_empty());
        assert_eq!(c.completed_objectives(am, false), 36);
        let mut d = Championships::default();
        for course in am.courses {
            d.record_objective(am, false, course, 0x3f);
        }
        d.objectives.insert("unspstunt-normal:env01".into(), 0x1f);
        let award = d.award_objective(am, false, "env01", 0x20).1.unwrap();
        assert_eq!((award.trophy, award.reward), (Trophy::Gold, 1000));
        assert_eq!(award.opened, ["spstunt-normal"]);
        // Finishing the series pays nothing more for the same trophy.
        for _ in 0..6 {
            d.finish_freestyle_round(am, false);
        }
        assert_eq!(d.progress(am, false).trophy, Some(Trophy::Gold));
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
            "You have won a silver trophy! (+500 Profile Points)"
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
