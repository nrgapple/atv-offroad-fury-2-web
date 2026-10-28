//! The original mini-games ("Games": King of the Hills, Tag, Hockey and
//! Treasure Hunt; research/GAMES.md).
//!
//! Exact-build evidence (private exports, bounded reading; no runtime
//! traces):
//!
//! - Event types 9-12 (`0021add0`: King of the Hills, Tag, Hockey, Treasure
//!   Hunt); their course lists are Tracks.dat `koth`, `tag`, `hoc` and `th`
//!   (`0021cca0`), every one with `RaceTypeMask=1`. Each Environment and
//!   Freestyle venue ships one archive per game (`E05_KH.RES`,
//!   `E05_TG.RES`, `E05_TH.RES`); Hockey rides Coonce Arena (`G01_SO.RES`).
//!   A game archive's mode TOY (`env05kh.toy`, `env05th.toy`) holds the
//!   game's triggers and its start positions.
//! - The Games entry is Multiplayer → Split Screen → Games (page
//!   `MultiPlayerSelect`), and the setup page is the shared Track Select
//!   page in mode 3 ("Games Prep", `0026a720`): Game, the course and a
//!   time limit. The limit (`002f60b0`/`002f6190`) steps 2..20 minutes and
//!   wraps; Games Prep starts it at 5 (`002f6060`); the event reads it as
//!   minutes x 60 (`0026b7a0`). Games have no CPU riders: no game archive
//!   carries a racing line or AI roster.
//! - Triggers (`0023ff80`, `0023fd00`, `00171f90`): `Position`, `Box`
//!   (the trigger's box extents about its position), `OneTimeOnly`,
//!   `NumberOfActions` and `[Trigger%dAction%d]` `ActionType`. A rider
//!   touching a trigger calls its actions' callbacks with 1 (`00172bc0`);
//!   leaving it calls them with 0 (`001728e0`).
//! - Ranking (`00220e70`, `002d0f20`): riders sort by the game's count,
//!   ties by stunt score (`002fa120`).
//!
//! The rules live in [`king_of_the_hill`] and [`treasure_hunt`]; Tag and
//! Hockey plug into the same [`Rules`] trait and [`Session`].
use serde::Serialize;
use std::collections::BTreeMap;

pub mod king_of_the_hill;
pub mod treasure_hunt;

/// The four original games, in the Profile Store's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum GameKind {
    KingOfTheHills,
    Tag,
    Hockey,
    TreasureHunt,
}

impl GameKind {
    pub const ALL: [GameKind; 4] = [
        GameKind::KingOfTheHills,
        GameKind::Tag,
        GameKind::Hockey,
        GameKind::TreasureHunt,
    ];

    /// The original event type (settings +8; `0021add0`).
    pub fn event_type(self) -> u8 {
        match self {
            GameKind::KingOfTheHills => 9,
            GameKind::Tag => 10,
            GameKind::Hockey => 11,
            GameKind::TreasureHunt => 12,
        }
    }

    pub fn from_event_type(kind: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.event_type() == kind)
    }

    /// The name the menus show (`0021add0`).
    pub fn name(self) -> &'static str {
        match self {
            GameKind::KingOfTheHills => "King of the Hills",
            GameKind::Tag => "Tag",
            GameKind::Hockey => "Hockey",
            GameKind::TreasureHunt => "Treasure Hunt",
        }
    }

    /// The port's id (ride links, menus, tests).
    pub fn key(self) -> &'static str {
        match self {
            GameKind::KingOfTheHills => "koth",
            GameKind::Tag => "tag",
            GameKind::Hockey => "hockey",
            GameKind::TreasureHunt => "treasure",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.key() == key)
    }

    /// The Tracks.dat course list (`crate::store::EVENT_LISTS`).
    pub fn list_key(self) -> &'static str {
        match self {
            GameKind::KingOfTheHills => "koth",
            GameKind::Tag => "tag",
            GameKind::Hockey => "hoc",
            GameKind::TreasureHunt => "th",
        }
    }

    /// The game archive's file suffix (`E05_KH.RES`).
    pub fn archive_suffix(self) -> &'static str {
        match self {
            GameKind::KingOfTheHills => "KH",
            GameKind::Tag => "TG",
            GameKind::Hockey => "SO",
            GameKind::TreasureHunt => "TH",
        }
    }

    /// The mode TOY's suffix after the venue id (`env05kh.toy`).
    pub fn toy_suffix(self) -> &'static str {
        match self {
            GameKind::KingOfTheHills => "kh",
            GameKind::Tag => "tg",
            GameKind::Hockey => "",
            GameKind::TreasureHunt => "th",
        }
    }

    /// The port plays this game (Hockey through `crate::hockey`, Tag in a
    /// hosted multiplayer room through `crate::tag`).
    pub fn playable(self) -> bool {
        true
    }

    /// The results column (Overlays.pb `[TreasureOver Menu]` "Tokens
    /// Collected", `[HillKingOver Menu]` "Hills").
    pub fn score_label(self) -> &'static str {
        match self {
            GameKind::KingOfTheHills => "Hills",
            GameKind::TreasureHunt => "Tokens Collected",
            GameKind::Tag => "It Time",
            GameKind::Hockey => "Goals",
        }
    }
}

/// The game archive of `venue` (`/TRACKS/TRACKSA/ENV05/E05_TH.RES`).
pub fn game_archive(kind: GameKind, venue: &str) -> Option<String> {
    let family = venue.trim_end_matches(|c: char| c.is_ascii_digit());
    let number = venue.get(family.len()..).filter(|n| n.len() == 2)?;
    let prefix = match family {
        "env" => "E",
        "free" => "F",
        "game" => "G",
        _ => return None,
    };
    Some(format!(
        "/TRACKS/TRACKSA/{}/{prefix}{number}_{}.RES",
        venue.to_ascii_uppercase(),
        kind.archive_suffix()
    ))
}

/// The mode TOY member of a game archive (`env05th.toy`).
pub fn mode_toy(kind: GameKind, venue: &str) -> String {
    format!("{venue}{}.toy", kind.toy_suffix())
}

/// Games Prep's time limit (`002f6060`), minutes.
pub const DEFAULT_MINUTES: u32 = 5;
/// `002f60b0` / `002f6190`: the limit steps 2..20 minutes and wraps.
pub const MIN_MINUTES: u32 = 2;
pub const MAX_MINUTES: u32 = 20;

pub fn step_minutes(minutes: u32, direction: i32) -> u32 {
    let span = (MAX_MINUTES - MIN_MINUTES + 1) as i32;
    let at = minutes.clamp(MIN_MINUTES, MAX_MINUTES) as i32 - MIN_MINUTES as i32;
    (at + direction).rem_euclid(span) as u32 + MIN_MINUTES
}

/// A TOY document: sections of key/value pairs, keys case-insensitive.
#[derive(Clone, Debug, Default)]
pub struct Toy {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Toy {
    pub fn parse(text: &str) -> Self {
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current: Option<String> = None;
        for raw in text.lines() {
            let line = raw.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let name = name.trim().to_ascii_lowercase();
                sections.entry(name.clone()).or_default();
                current = Some(name);
            } else if let (Some(section), Some((key, value))) = (&current, line.split_once('=')) {
                sections
                    .entry(section.clone())
                    .or_default()
                    .entry(key.trim().to_ascii_lowercase())
                    .or_insert_with(|| value.trim().trim_matches('"').to_string());
            }
        }
        Self { sections }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_ascii_lowercase())?
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn int(&self, section: &str, key: &str) -> Option<i64> {
        self.get(section, key)?.trim().parse().ok()
    }

    pub fn vec3(&self, section: &str, key: &str) -> Option<[f32; 3]> {
        vec3(self.get(section, key)?)
    }

    pub fn has(&self, section: &str) -> bool {
        self.sections.contains_key(&section.to_ascii_lowercase())
    }
}

fn vec3(text: &str) -> Option<[f32; 3]> {
    let mut parts = text.split(',').map(|p| p.trim().parse::<f32>());
    let v = [
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ];
    v.iter().all(|c| c.is_finite()).then_some(v)
}

/// One `[TriggerN]` and its actions.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Trigger {
    pub name: String,
    pub position: [f32; 3],
    /// `Box`: the extents about `position` (read as half sizes).
    pub extents: [f32; 3],
    /// `Radius`: a sphere trigger (Freestyle targets), 0 for a box.
    pub radius: f32,
    pub one_time: bool,
    /// Each action's `ActionType` and its other keys (lowercase).
    pub actions: Vec<(String, BTreeMap<String, String>)>,
}

impl Trigger {
    /// The action of this type, if any.
    pub fn action(&self, kind: &str) -> Option<&BTreeMap<String, String>> {
        self.actions
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(kind))
            .map(|(_, v)| v)
    }

    /// Whether a rider at `point` touches the box. The original tests the
    /// ATV's collision body against it; `margin` grows the box by the
    /// body's horizontal reach (an approximation, research/GAMES.md).
    pub fn touches(&self, point: [f32; 3], margin: f32) -> bool {
        (0..3).all(|i| {
            let grow = if i == 1 { margin * 0.5 } else { margin };
            (point[i] - self.position[i]).abs() <= self.extents[i] + grow
        })
    }
}

/// `[Triggers] NumberOfTriggers`, `[Trigger%d]` and `[Trigger%dAction%d]`
/// up to each trigger's `NumberOfActions` (`0023ff80`).
pub fn parse_triggers(toy: &Toy) -> Vec<Trigger> {
    let count = toy
        .int("Triggers", "NumberOfTriggers")
        .unwrap_or(0)
        .clamp(0, 512);
    let mut out = Vec::new();
    for n in 1..=count {
        let section = format!("Trigger{n}");
        let Some(position) = toy.vec3(&section, "Position") else {
            continue;
        };
        let extents = toy.vec3(&section, "Box").unwrap_or([0.; 3]);
        let actions = toy
            .int(&section, "NumberOfActions")
            .unwrap_or(0)
            .clamp(0, 16);
        let actions = (1..=actions)
            .filter_map(|a| {
                let name = format!("Trigger{n}Action{a}").to_ascii_lowercase();
                let fields = toy.sections.get(&name)?;
                let kind = fields.get("actiontype")?.clone();
                Some((kind, fields.clone()))
            })
            .collect();
        out.push(Trigger {
            name: toy.get(&section, "Name").unwrap_or_default().to_string(),
            position,
            extents,
            radius: toy
                .get(&section, "Radius")
                .and_then(|r| r.trim().parse::<f32>().ok())
                .filter(|r| r.is_finite() && *r > 0.)
                .unwrap_or(0.),
            one_time: toy
                .get(&section, "OneTimeOnly")
                .is_some_and(|v| v.eq_ignore_ascii_case("t")),
            actions,
        });
    }
    out
}

/// A `[SceneInfo]` start: `StartN` and `DirectionN`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Start {
    pub position: [f32; 3],
    pub direction: [f32; 3],
}

impl Start {
    /// Heading about +Y from the direction (0 faces +Z).
    pub fn yaw(&self) -> f32 {
        use crate::math::Det;
        self.direction[0].atan2_d(self.direction[2])
    }
}

/// `[SceneInfo] NumStartPositions` / `Start%d` / `Direction%d` (one per
/// player slot). Courses without them keep the venue's own start.
pub fn parse_starts(toy: &Toy) -> Vec<Start> {
    let count = toy
        .int("SceneInfo", "NumStartPositions")
        .unwrap_or(0)
        .clamp(0, 8);
    (1..=count)
        .filter_map(|n| {
            Some(Start {
                position: toy.vec3("SceneInfo", &format!("Start{n}"))?,
                direction: toy
                    .vec3("SceneInfo", &format!("Direction{n}"))
                    .unwrap_or([0., 0., 1.]),
            })
        })
        .collect()
}

/// One rider's state for a game step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rider {
    pub position: [f32; 3],
    /// Any wheel on the ground (+0x908).
    pub grounded: bool,
    /// A stunt chain is open (status +8 & 0x20).
    pub stunting: bool,
    pub wrecked: bool,
    /// A stunt chain ended this step: landed, and its points.
    pub chain: Option<(bool, f32)>,
}

/// A world marker the HUD draws (screen tag and minimap icon).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Marker {
    pub position: [f32; 3],
    pub label: String,
    /// The rider holding it (King of the Hills flag colour).
    pub owner: Option<usize>,
    /// The score to beat (King of the Hills).
    pub score: f32,
}

/// A timed sign (Overlays.pb signage, `002e9510`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Notice {
    pub text: String,
    pub seconds: f32,
}

/// A game's rules over its riders (0 is the local player).
pub trait Rules {
    fn kind(&self) -> GameKind;
    /// Seeds the rules' random choices (the original's `rand`).
    fn seed(&mut self, _seed: u32) {}
    /// The event started (after the start countdown).
    fn begin(&mut self);
    /// One step; returns signs to show.
    fn update(&mut self, dt: f32, riders: &[Rider]) -> Vec<Notice>;
    /// The rules ended the game (Treasure Hunt: all collected).
    fn complete(&self) -> bool;
    /// Each rider's count (hills held, treasures collected).
    fn scores(&self) -> Vec<i32>;
    /// The HUD's count readout for a rider ("2 / 5").
    fn progress(&self, rider: usize) -> String;
    /// The extra readout (Treasure Hunt's "More Treasures in").
    fn countdown(&self) -> Option<f32> {
        None
    }
    fn markers(&self) -> Vec<Marker>;
    /// Restart Game (`00220d10`, `002d1450`).
    fn restart(&mut self);
    /// Diagnostics (name, value) for tests.
    fn diagnostics(&self) -> Vec<(&'static str, f32)>;
    /// The TOY triggers riders entered since the last call whose other
    /// actions (`HideAnimation`, `Particles`) the presentation runs:
    /// Treasure Hunt's collected treasures (`00172bc0` calls a trigger's
    /// action callbacks in order; the `Treasure` action counts it).
    fn take_triggered(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// Why the game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum End {
    /// The time limit ran out.
    TimeUp,
    /// The rules ended it (all treasures collected).
    Complete,
}

/// A results row (`[TreasureOver Menu]` / `[HillKingOver Menu]`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Standing {
    pub rider: usize,
    pub score: i32,
    pub stunt: f32,
}

/// Seconds the "All Treasures Collected" sign shows before the results
/// (`002d0a20`: 5 s).
pub const FINISH_HOLD: f32 = 5.;

/// The shared game shell: the event clock, signs, ranking and the end.
pub struct Session {
    pub rules: Box<dyn Rules>,
    pub minutes: u32,
    pub time_left: f32,
    pub started: bool,
    pub end: Option<End>,
    /// Seconds after the rules completed before the results open.
    hold: f32,
    pub notice: Option<Notice>,
    /// Each rider's committed stunt score (results' Stunt Score column).
    pub stunt: Vec<f32>,
    pub riders: usize,
}

impl Session {
    pub fn new(rules: Box<dyn Rules>, minutes: u32, riders: usize) -> Self {
        let minutes = minutes.clamp(MIN_MINUTES, MAX_MINUTES);
        Self {
            rules,
            minutes,
            time_left: minutes as f32 * 60.,
            started: false,
            end: None,
            hold: 0.,
            notice: None,
            stunt: vec![0.; riders.max(1)],
            riders: riders.max(1),
        }
    }

    pub fn kind(&self) -> GameKind {
        self.rules.kind()
    }

    /// The start countdown ended.
    pub fn begin(&mut self) {
        if !self.started {
            self.started = true;
            self.rules.begin();
        }
    }

    /// The results are open.
    pub fn over(&self) -> bool {
        self.end.is_some() && self.hold <= 0.
    }

    /// One step: the rules, the clock (the event's time limit, `0026b7a0`)
    /// and the signs. `stunt` is each rider's committed stunt score.
    pub fn tick(&mut self, dt: f32, riders: &[Rider], stunt: &[f32]) {
        for (i, s) in stunt.iter().enumerate().take(self.riders) {
            self.stunt[i] = *s;
        }
        if let Some(n) = &mut self.notice {
            n.seconds -= dt;
            if n.seconds <= 0. {
                self.notice = None;
            }
        }
        if self.end.is_some() {
            self.hold -= dt;
            return;
        }
        if !self.started {
            return;
        }
        for notice in self.rules.update(dt, riders) {
            self.notice = Some(notice);
        }
        if self.rules.complete() {
            self.end = Some(End::Complete);
            self.hold = FINISH_HOLD;
            return;
        }
        self.time_left = (self.time_left - dt).max(0.);
        if self.time_left <= 0. {
            self.end = Some(End::TimeUp);
            self.hold = 0.;
        }
    }

    /// Riders by count, ties by stunt score (`00220e70`, `002fa120`).
    pub fn standings(&self) -> Vec<Standing> {
        let scores = self.rules.scores();
        let mut rows: Vec<Standing> = (0..self.riders)
            .map(|rider| Standing {
                rider,
                score: scores.get(rider).copied().unwrap_or(0),
                stunt: self.stunt.get(rider).copied().unwrap_or(0.),
            })
            .collect();
        rows.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(b.stunt.total_cmp(&a.stunt))
                .then(a.rider.cmp(&b.rider))
        });
        rows
    }

    /// The player's place (1-based).
    pub fn place(&self, rider: usize) -> usize {
        self.standings()
            .iter()
            .position(|s| s.rider == rider)
            .map_or(1, |p| p + 1)
    }

    /// Restart Game: the rules, the clock and the signs start over.
    pub fn restart(&mut self) {
        self.rules.restart();
        self.time_left = self.minutes as f32 * 60.;
        self.started = false;
        self.end = None;
        self.hold = 0.;
        self.notice = None;
        self.stunt.iter_mut().for_each(|s| *s = 0.);
    }

    /// "M:SS" of the time left (EventTimeRemainingValueCallback).
    pub fn clock(&self) -> String {
        let s = self.time_left.ceil().max(0.) as u32;
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The game's rules from its mode TOY text, for one player.
pub fn rules(kind: GameKind, toy: &str) -> crate::Result<Box<dyn Rules>> {
    rules_for(kind, toy, 1)
}

/// The game's rules for `riders` players (split screen: one per view; the
/// original's rider records hold up to four).
pub fn rules_for(kind: GameKind, toy: &str, riders: usize) -> crate::Result<Box<dyn Rules>> {
    let toy = Toy::parse(toy);
    match kind {
        GameKind::KingOfTheHills => Ok(Box::new(king_of_the_hill::KingOfTheHill::with_riders(
            &toy, riders,
        )?)),
        GameKind::TreasureHunt => Ok(Box::new(treasure_hunt::TreasureHunt::new(&toy, riders)?)),
        _ => Err(format!(
            "{} is not available in this port yet.",
            kind.name()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_screen_rules_keep_a_record_per_player() {
        let koth = "[KingOfTheHill]\nNumberOfHills=1\n[Triggers]\nNumberOfTriggers=1\n\
             [Trigger1]\nName=Hill1\nNumberOfActions=1\nPosition=0,0,0\nBox=20,20,20\n\
             [Trigger1Action1]\nActionType=Hilltop\nHillNumber=1\n";
        let split = rules_for(GameKind::KingOfTheHills, koth, 2).unwrap();
        assert_eq!(split.scores(), vec![0, 0]);
        assert_eq!(split.progress(1), "0 / 1");
        assert_eq!(
            rules(GameKind::KingOfTheHills, koth).unwrap().scores(),
            vec![0]
        );
    }

    #[test]
    fn kinds_match_the_store_and_tracks_dat() {
        for (kind, (event, name)) in GameKind::ALL.iter().zip(crate::store::GAMES) {
            assert_eq!(kind.event_type(), event);
            assert_eq!(kind.name(), name);
            assert!(crate::store::EVENT_LISTS.contains(&kind.list_key()));
        }
        assert_eq!(
            game_archive(GameKind::TreasureHunt, "env05").as_deref(),
            Some("/TRACKS/TRACKSA/ENV05/E05_TH.RES")
        );
        assert_eq!(
            game_archive(GameKind::KingOfTheHills, "free01").as_deref(),
            Some("/TRACKS/TRACKSA/FREE01/F01_KH.RES")
        );
        assert_eq!(mode_toy(GameKind::KingOfTheHills, "env07"), "env07kh.toy");
        assert_eq!(game_archive(GameKind::TreasureHunt, "nat01"), None);
    }

    /// Private sweep: every course of both game lists loads its mode TOY
    /// from the original archive and its hills/treasures resolve. Skips
    /// without the ignored extracted disc files.
    #[test]
    fn every_original_game_course_resolves() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/extracted");
        if !root.join("TRACKS/TRACKSA/ENV05/E05_TH.RES").exists() {
            eprintln!("skipped: private game archives are not extracted");
            return;
        }
        let venues = [
            "env05", "env02", "env08", "env06", "env04", "env01", "env03", "free01", "free04",
            "free02", "free06", "free03", "free05", "env07",
        ];
        for kind in [GameKind::KingOfTheHills, GameKind::TreasureHunt] {
            for venue in venues {
                let path = game_archive(kind, venue).unwrap();
                let bytes = std::fs::read(root.join(&path[1..])).unwrap();
                let archive = crate::resource::Resource::parse(&bytes).unwrap();
                let name = mode_toy(kind, venue);
                let index = archive
                    .members
                    .iter()
                    .position(|m| m.name.to_ascii_lowercase().ends_with(&name))
                    .unwrap_or_else(|| panic!("{path} lacks {name}"));
                let text = String::from_utf8_lossy(&archive.extract(index).unwrap()).into_owned();
                let toy = Toy::parse(&text);
                match kind {
                    GameKind::KingOfTheHills => {
                        let g = king_of_the_hill::KingOfTheHill::new(&toy).unwrap();
                        assert_eq!(
                            g.hills.len() as i64,
                            toy.int("KingOfTheHill", "NumberOfHills").unwrap(),
                            "{venue}"
                        );
                    }
                    _ => {
                        let g = treasure_hunt::TreasureHunt::new(&toy, 1).unwrap();
                        assert_eq!(g.treasures.len(), g.targets, "{venue}");
                    }
                }
            }
        }
    }

    /// Private probe: the King of the Hills scenes add only the `kohbase`
    /// and `kohflag` models, and no game archive ships a collision mesh for
    /// them (the original's static collision skips models without a COL,
    /// `00206e90`); every COL a game archive carries is the Freeride one.
    /// Private probe: every Treasure Hunt treasure trigger hides a `bomb`
    /// animation whose PSG and the `bomb.MOT` loop ship in the archive.
    #[test]
    fn every_treasure_hides_a_bomb_model() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/extracted");
        if !root.join("TRACKS/TRACKSA/ENV05/E05_TH.RES").exists() {
            eprintln!("skipped: private archives are not extracted");
            return;
        }
        for venue in [
            "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free01",
            "free02", "free03", "free04", "free05", "free06",
        ] {
            let path = game_archive(GameKind::TreasureHunt, venue).unwrap();
            let bytes = std::fs::read(root.join(&path[1..])).unwrap();
            let archive = crate::resource::Resource::parse(&bytes).unwrap();
            let member = |name: &str| {
                archive.members.iter().position(|m| {
                    m.name
                        .replace('\\', "/")
                        .to_ascii_lowercase()
                        .ends_with(name)
                })
            };
            let toy = archive
                .extract(member(&format!("{venue}th.toy")).unwrap())
                .unwrap();
            let text = String::from_utf8_lossy(&toy);
            let bombs = crate::freestyle::parse_animated_triggers(&text, "Treasure");
            let expected = Toy::parse(&text)
                .int("TreasureHunt", "NumberOfTargets")
                .unwrap();
            assert_eq!(bombs.len() as i64, expected, "{venue}");
            assert!(bombs.iter().all(|b| b.model == "bomb"), "{venue}");
            assert!(
                member("bomb.psg").is_some() && member("bomb.mot").is_some(),
                "{venue}"
            );
        }
    }

    /// Private sweep: every Treasure Hunt treasure and every Freestyle
    /// target (green and red) on every course starts its explosion: each
    /// `Particles` action names an emitter of its own TOY whose EMT, atlas
    /// group and (for bombs) `bomb.mcf` animation instance resolve, and the
    /// started one-shot draws CPU sprites. Skips without the disc files.
    #[test]
    fn every_treasure_and_target_explosion_resolves() {
        use crate::ambient_effects::{Effects, Source, Toy};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/extracted");
        if !root.join("TRACKS/TRACKSA/ENV05/E05_TH.RES").exists() {
            eprintln!("skipped: private archives are not extracted");
            return;
        }
        let venues = [
            "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free01",
            "free02", "free03", "free04", "free05", "free06",
        ];
        let mut checked = 0;
        let mut runs = Vec::new();
        for venue in venues {
            runs.push((
                game_archive(GameKind::TreasureHunt, venue).unwrap(),
                format!("{venue}th"),
                Toy::Mode,
            ));
            if let Some(ob) = crate::freestyle::objective_archive(venue) {
                for colour in ["gcp", "rcp"] {
                    runs.push((ob.clone(), format!("{venue}{colour}"), Toy::Targets));
                }
            }
        }
        for (path, stem, toy) in runs {
            let bytes = std::fs::read(root.join(&path[1..])).unwrap();
            let archive = crate::resource::Resource::parse(&bytes).unwrap();
            let member = |name: &str| {
                archive
                    .members
                    .iter()
                    .position(|m| {
                        m.name
                            .replace('\\', "/")
                            .rsplit('/')
                            .next()
                            .unwrap()
                            .eq_ignore_ascii_case(name)
                    })
                    .map(|i| String::from_utf8_lossy(&archive.extract(i).unwrap()).into_owned())
            };
            let Some(text) = member(&format!("{stem}.toy")) else {
                panic!("{path} lacks {stem}.toy");
            };
            let targets = if toy == Toy::Mode {
                crate::freestyle::parse_animated_triggers(&text, "Treasure")
            } else {
                crate::freestyle::parse_targets(&text)
            };
            assert!(!targets.is_empty(), "{stem}");
            let prt = archive
                .members
                .iter()
                .position(|m| m.name.to_ascii_lowercase().ends_with("particles.prt"))
                .map(|i| String::from_utf8_lossy(&archive.extract(i).unwrap()).into_owned())
                .unwrap();
            let mut source = Source {
                scene: "[StaticModels]\nNumberOfStaticModels=0\n".into(),
                common: String::new(),
                mode: (toy == Toy::Mode).then(|| text.clone()),
                targets: (toy == Toy::Targets).then(|| text.clone()),
                prt,
                emts: Default::default(),
            };
            for p in crate::ambient_effects::plan(&source).unwrap().placements {
                if let Some(emt) = member(&p.filename) {
                    source.emts.insert(p.filename.clone(), emt);
                }
            }
            let (mut effects, metadata) = Effects::from_source(&source).unwrap();
            assert!(
                metadata
                    .unsupported
                    .iter()
                    .all(|u| u.starts_with("locosmoke")),
                "{stem}: {:?}",
                metadata.unsupported
            );
            for (i, target) in targets.iter().enumerate() {
                assert_eq!(target.particles.len(), 1, "{stem} {}", target.name);
                let action = target.particles[0];
                assert!(action.on);
                // Let earlier bursts (StartOn one-shots, the last target) end.
                for _ in 0..70 {
                    effects.step(1. / 60.).unwrap();
                }
                assert!(effects.trigger(toy, action, None), "{stem} {}", target.name);
                effects.step(1. / 60.).unwrap();
                let frame = effects.frame();
                assert!(frame.emitted > 0, "{stem} {}", target.name);
                // The burst starts at the trigger (the env04 green target 12
                // is authored 2 ft from its emitter).
                let near = frame
                    .particles
                    .iter()
                    .any(|p| (0..3).all(|a| (p.position[a] - target.position[a]).abs() < 3.));
                assert!(near, "{stem} {} #{i}", target.name);
                checked += 1;
            }
        }
        // 14 Treasure Hunts (25 or 15 treasures) and 13 objective courses.
        assert_eq!(checked, 581);
    }

    #[test]
    fn hill_scene_objects_have_no_collision_meshes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/extracted");
        if !root.join("TRACKS/TRACKSA/ENV05/E05_FR.RES").exists() {
            eprintln!("skipped: private archives are not extracted");
            return;
        }
        for venue in [
            "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free01",
            "free02", "free03", "free04", "free05", "free06",
        ] {
            let path = game_archive(GameKind::KingOfTheHills, venue).unwrap();
            let bytes = std::fs::read(root.join(&path[1..])).unwrap();
            let archive = crate::resource::Resource::parse(&bytes).unwrap();
            for name in ["geometry/kohbase.col", "geometry/kohflag.col"] {
                assert!(
                    !archive
                        .members
                        .iter()
                        .any(|m| m.name.eq_ignore_ascii_case(name)),
                    "{venue} {name}"
                );
            }
        }
        let read = |p: &str| std::fs::read(root.join(p)).unwrap();
        let (kh, fr) = (
            read("TRACKS/TRACKSA/ENV05/E05_KH.RES"),
            read("TRACKS/TRACKSA/ENV05/E05_FR.RES"),
        );
        let (kh, fr) = (
            crate::resource::Resource::parse(&kh).unwrap(),
            crate::resource::Resource::parse(&fr).unwrap(),
        );
        for (i, m) in kh.members.iter().enumerate() {
            if !m.name.to_ascii_lowercase().ends_with(".col") {
                continue;
            }
            let j = fr
                .members
                .iter()
                .position(|f| f.name.eq_ignore_ascii_case(&m.name))
                .unwrap_or_else(|| panic!("{} only in the game archive", m.name));
            assert_eq!(kh.extract(i).unwrap(), fr.extract(j).unwrap(), "{}", m.name);
        }
    }

    #[test]
    fn time_limit_steps_two_to_twenty_and_wraps() {
        assert_eq!(step_minutes(DEFAULT_MINUTES, 1), 6);
        assert_eq!(step_minutes(20, 1), 2);
        assert_eq!(step_minutes(2, -1), 20);
    }

    #[test]
    fn triggers_and_starts_parse() {
        let toy = Toy::parse(
            "[SceneInfo]\r\nNumStartPositions=1\r\nStart1=1,2,3\r\nDirection1=1,0,0\r\n\
             [Triggers]\r\nNumberOfTriggers=1\r\n[Trigger1]\r\nName=Hill1\r\nNumberOfActions=1\r\n\
             Position=10,20,30\r\nBox=20,20,20\r\nOneTimeOnly=T\r\n[Trigger1Action1]\r\n\
             ActionType=Hilltop\r\nHillNumber=1 ; comment\r\n[Trigger1Action2]\r\nActionType=Ignored\r\n",
        );
        let triggers = parse_triggers(&toy);
        assert_eq!(triggers.len(), 1);
        let t = &triggers[0];
        assert_eq!(
            (t.name.as_str(), t.one_time, t.actions.len()),
            ("Hill1", true, 1)
        );
        assert_eq!(
            t.action("hilltop")
                .and_then(|a| a.get("hillnumber"))
                .map(String::as_str),
            Some("1")
        );
        assert!(t.touches([29., 20., 49.], 0.));
        assert!(!t.touches([31., 20., 30.], 0.));
        let starts = parse_starts(&toy);
        assert_eq!(starts[0].position, [1., 2., 3.]);
        assert!((starts[0].yaw() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
