//! The Hall of Fame: each race course's best lap and race times, longest
//! jump, most combos and highest stunt score with the names that set them,
//! and the in-race "New …" record messages (research/PROFILE.md, "Hall of
//! Fame").
//!
//! Exact-build evidence (private exports, bounded reading):
//!
//! - `00331400` builds the table: sixteen event types, each with a list of
//!   course records sized from the event's course count (world +0xe80 +
//!   0x7c × type); racing types (0, 1, 4–8) keep two per course (Normal,
//!   then Expert, picked by `00331970` from difficulty +0x1890 = 2),
//!   Freestyle types (2, 3) one. `00331750` clears a racing record (0xa4
//!   bytes): three fastest laps (+0x0, empty = FLT_MAX) with 9-byte names
//!   (+0xc) and vehicles (+0x28), three fastest race times (+0x34, names
//!   +0x40, vehicles +0x5c), longest jump (+0x68, name +0x6c, vehicle
//!   +0x78), most combos (+0x7c, +0x80, +0x8c) and highest stunt score
//!   (+0x90, +0x94, +0xa0). No default holders are written.
//! - Race hooks hand each insert the placeholder name `"@%d"` (`0x4289f8`,
//!   the player index) and the player's vehicle: the lap hook `001f24b0` →
//!   `003323d0` (each counted lap), the finish `001f1e30` → `003325d0`
//!   (race time, only at the default 3 Enduro / 5 other laps), the jump
//!   hook `001f2610` → `00332810` (whenever the race's best jump grows) and
//!   a landed chain `001f22a0` → `00332b20` (points, (int)(sum × count))
//!   and `003329e0` (the chain's event count; a count of 1 is not a
//!   combo). Times insert when strictly faster than a kept time, shifting
//!   the slower ones down; the others replace a strictly smaller value.
//! - A Hall of Fame record shows its infotext for 1.5 s (`002e2cf0`):
//!   "New %sFastest Lap Time!", "New %sFastest 3 Lap Race Time!" (Enduro,
//!   race types 0 and 1) or "…5 Lap…", "New Longest Jump %d %s!", "New Most
//!   Combos!", "New Highest Stunt Score!" (`%s` from `003337a0`: "", "2nd
//!   ", "3rd "). Only when it set none does the profile's course record
//!   show its own: "New Personal %sFastest Lap Time!" (`003249c0`, the
//!   record's three laps +0x70), "New Personal %sFastest 5 Lap Race Time!"
//!   (`00324b40`, three race times +0x7c), "New Personal Longest Jump %d
//!   %s!" (`00324cc0`, over 40 ft), "New Personal Most Combos!" and "New
//!   Personal Highest Stunt Score!".
//! - After the race (`001f1740` → `00217e40`) a player whose placeholder
//!   holds a record gets the `[NameEntry Menu]` overlay ("Congratulations",
//!   "You Have Broken These Records:", the records listed by `00333360` in
//!   the order lap, race, combos, jump, stunt — "Fastest Lap Time", "2nd
//!   Fastest 5 Lap Race Time"… — and "Enter your name for the Hall of
//!   Fame:", nine bytes), and `00331f50` replaces the placeholder with the
//!   entered name.
//! - Freestyle types (2 Pro, 3 Amateur) keep one 0x90-byte record per
//!   course: longest jump (+0x0, name +0x4, vehicle +0x10), the three
//!   highest objective scores (+0x14, names +0x20, vehicles +0x3c), most
//!   combos (+0x48) and the three highest stunt scores (+0x5c, one landed
//!   chain's (int)(sum × count), names +0x68, vehicles +0x84). The
//!   Freestyle jump hook `002fd7c0` → `00332e30` ("New Longest Jump"), the
//!   chain hook `002fd350` → `00333130` ("New %sHighest Stunt Score!") and
//!   `00333000` ("New Most Combos!"), and the objective's end `002fce00` →
//!   `00332c50` ("New %sHighest Objective Score!", only for the 120 s
//!   event) insert as the race ones do (larger values win). The profile's
//!   Freestyle record (0x28 bytes) keeps its own: longest jump (`003251b0`,
//!   no 40 ft minimum), three objective scores (`00325030`), most combos
//!   (`00325330`) and three stunt scores (`00325410`), with "New Personal
//!   %sHighest Objective Score!" and the rest; `00333360` lists Freestyle
//!   records as objective scores, stunt scores, most combos, longest jump.
//!   The objective's end also credits the run's stunt score / 1000 plus
//!   the combo points to the profile (`002fce00` → `00323440`).
//! - Not ported: the per-vehicle exclusions (+0x514 of 1 or 0x15) and the
//!   overall records the inserts also keep at the table's head.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `0x4289f8` "@%d" for player 0: the name a record holds until the name
/// entry replaces it.
pub const PLACEHOLDER: &str = "@0";
/// Longest name the Hall of Fame keeps (9-byte fields).
pub const NAME_MAX: usize = 8;
/// `002e2cf0`'s duration for a record message.
pub const MESSAGE_SECONDS: f32 = 1.5;
/// `00324cc0`: a personal jump record must exceed 40 ft.
pub const PERSONAL_MIN_JUMP_FEET: f32 = 40.;

/// One holder: the value, the name and the vehicle ridden.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry<T> {
    pub value: T,
    pub name: String,
    pub vehicle: u16,
}

/// One course's Hall of Fame on one difficulty (the 0xa4-byte record).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CourseFame {
    /// Fastest laps, fastest first (at most three), seconds.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub laps: Vec<Entry<f32>>,
    /// Fastest full-length race times, fastest first (at most three).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub races: Vec<Entry<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_jump: Option<Entry<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_combos: Option<Entry<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_stunt: Option<Entry<i64>>,
}

/// Inserts `time` into a fastest-first top three when strictly faster than
/// a kept time (or a free place remains); the 1-based rank it took.
fn insert_time(list: &mut Vec<Entry<f32>>, time: f32, name: &str, vehicle: u16) -> Option<usize> {
    if !time.is_finite() || time <= 0. {
        return None;
    }
    let at = list
        .iter()
        .position(|e| time < e.value)
        .unwrap_or(list.len());
    if at >= 3 {
        return None;
    }
    list.insert(
        at,
        Entry {
            value: time,
            name: name.into(),
            vehicle,
        },
    );
    list.truncate(3);
    Some(at + 1)
}

/// Replaces a holder whose value is strictly smaller.
fn beat<T: PartialOrd + Copy + Default>(
    slot: &mut Option<Entry<T>>,
    value: T,
    name: &str,
    vehicle: u16,
) -> bool {
    let record = slot.as_ref().map_or(T::default(), |e| e.value);
    if record < value {
        *slot = Some(Entry {
            value,
            name: name.into(),
            vehicle,
        });
        true
    } else {
        false
    }
}

/// `003337a0`: "", "2nd ", "3rd ".
fn ordinal(rank: usize) -> &'static str {
    match rank {
        2 => "2nd ",
        3 => "3rd ",
        _ => "",
    }
}

/// "Fastest Lap Time", "2nd Fastest 5 Lap Race Time"… (`00333360`).
fn ranked(rank: usize, what: &str) -> String {
    format!("{}{what}", ordinal(rank))
}

impl CourseFame {
    fn names_mut(&mut self) -> impl Iterator<Item = &mut String> {
        let laps = self.laps.iter_mut().map(|e| &mut e.name);
        let races = self.races.iter_mut().map(|e| &mut e.name);
        laps.chain(races)
            .chain(self.longest_jump.iter_mut().map(|e| &mut e.name))
            .chain(self.most_combos.iter_mut().map(|e| &mut e.name))
            .chain(self.highest_stunt.iter_mut().map(|e| &mut e.name))
    }

    /// `00333360`: the records `name` holds, as the name entry lists them.
    pub fn held_by(&self, name: &str, enduro: bool) -> Vec<String> {
        let race = if enduro {
            "Fastest 3 Lap Race Time"
        } else {
            "Fastest 5 Lap Race Time"
        };
        let mut out = Vec::new();
        if let Some(i) = self.laps.iter().position(|e| e.name == name) {
            out.push(ranked(i + 1, "Fastest Lap Time"));
        }
        if let Some(i) = self.races.iter().position(|e| e.name == name) {
            out.push(ranked(i + 1, race));
        }
        let holds = |e: Option<&String>| e.is_some_and(|n| n == name);
        if holds(self.most_combos.as_ref().map(|e| &e.name)) {
            out.push("Most Combos".into());
        }
        if holds(self.longest_jump.as_ref().map(|e| &e.name)) {
            out.push("Longest Jump".into());
        }
        if holds(self.highest_stunt.as_ref().map(|e| &e.name)) {
            out.push("Highest Stunt Score".into());
        }
        out
    }

    /// `00331f50`: the placeholder becomes the entered name.
    pub fn rename(&mut self, from: &str, to: &str) {
        for name in self.names_mut() {
            if name == from {
                *name = to.into();
            }
        }
    }
}

/// One Freestyle course's Hall of Fame (the 0x90-byte record).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FreestyleFame {
    /// Highest objective scores, highest first (at most three).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objectives: Vec<Entry<i64>>,
    /// Highest stunt scores (one landed chain), highest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stunts: Vec<Entry<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_jump: Option<Entry<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_combos: Option<Entry<u32>>,
}

/// Inserts `score` into a highest-first top three when strictly higher
/// than a kept score (or a free place remains); the 1-based rank.
fn insert_score(list: &mut Vec<Entry<i64>>, score: i64, name: &str, vehicle: u16) -> Option<usize> {
    let at = list
        .iter()
        .position(|e| score > e.value)
        .unwrap_or(list.len());
    if at >= 3 || (at == list.len() && score <= 0) {
        return None;
    }
    list.insert(
        at,
        Entry {
            value: score,
            name: name.into(),
            vehicle,
        },
    );
    list.truncate(3);
    Some(at + 1)
}

fn insert_plain_score(list: &mut Vec<i64>, score: i64) -> Option<usize> {
    let at = list.iter().position(|s| score > *s).unwrap_or(list.len());
    if at >= 3 || (at == list.len() && score <= 0) {
        return None;
    }
    list.insert(at, score);
    list.truncate(3);
    Some(at + 1)
}

impl FreestyleFame {
    /// `00333360` (type 2): the records `name` holds.
    pub fn held_by(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(i) = self.objectives.iter().position(|e| e.name == name) {
            out.push(ranked(i + 1, "Highest Objective Score"));
        }
        if let Some(i) = self.stunts.iter().position(|e| e.name == name) {
            out.push(ranked(i + 1, "Highest Stunt Score"));
        }
        if self.most_combos.as_ref().is_some_and(|e| e.name == name) {
            out.push("Most Combos".into());
        }
        if self.longest_jump.as_ref().is_some_and(|e| e.name == name) {
            out.push("Longest Jump".into());
        }
        out
    }
}

/// The Hall of Fame: every course's records on Normal and Expert, and the
/// Freestyle courses'. The host stores it apart from the profiles (it is
/// not any one profile's).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HallOfFame {
    /// By [`HallOfFame::key`].
    pub courses: BTreeMap<String, CourseFame>,
    /// By [`HallOfFame::freestyle_key`].
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub freestyle: BTreeMap<String, FreestyleFame>,
}

impl HallOfFame {
    /// A course's key: its race course id, with `:expert` for Expert
    /// (`00331970` indexes the Expert records past the Normal ones).
    pub fn key(course: &str, expert: bool) -> String {
        if expert {
            format!("{course}:expert")
        } else {
            course.to_string()
        }
    }

    pub fn course(&self, course: &str, expert: bool) -> Option<&CourseFame> {
        self.courses.get(&Self::key(course, expert))
    }

    /// A Freestyle course's key: the championship series (Amateur and Pro
    /// Freestyle are separate event types) and the course.
    pub fn freestyle_key(series: &str, course: &str) -> String {
        format!("{series}:{course}")
    }

    pub fn freestyle_course(&self, series: &str, course: &str) -> Option<&FreestyleFame> {
        self.freestyle.get(&Self::freestyle_key(series, course))
    }

    /// Stores a Freestyle run's records under `name`, as [`HallOfFame::commit`].
    pub fn commit_freestyle(&mut self, watch: &FreestyleWatch, name: &str) {
        let name = clean_name(name);
        let fame = self.freestyle.entry(watch.key.clone()).or_default();
        for e in watch
            .fame
            .objectives
            .iter()
            .filter(|e| e.name == PLACEHOLDER)
        {
            insert_score(&mut fame.objectives, e.value, &name, e.vehicle);
        }
        for e in watch.fame.stunts.iter().filter(|e| e.name == PLACEHOLDER) {
            insert_score(&mut fame.stunts, e.value, &name, e.vehicle);
        }
        if let Some(e) = watch
            .fame
            .longest_jump
            .as_ref()
            .filter(|e| e.name == PLACEHOLDER)
        {
            beat(&mut fame.longest_jump, e.value, &name, e.vehicle);
        }
        if let Some(e) = watch
            .fame
            .most_combos
            .as_ref()
            .filter(|e| e.name == PLACEHOLDER)
        {
            beat(&mut fame.most_combos, e.value, &name, e.vehicle);
        }
    }

    /// The name entry renames what [`HallOfFame::commit_freestyle`] stored.
    pub fn rename_freestyle(&mut self, watch: &FreestyleWatch, from: &str, to: &str) {
        let (from, to) = (clean_name(from), clean_name(to));
        let Some(fame) = self.freestyle.get_mut(&watch.key) else {
            return;
        };
        let run = &watch.fame;
        let placed = |list: &[Entry<i64>], e: &Entry<i64>| {
            list.iter()
                .any(|r| r.name == PLACEHOLDER && r.value == e.value && r.vehicle == e.vehicle)
        };
        for e in fame.objectives.iter_mut().filter(|e| e.name == from) {
            if placed(&run.objectives, e) {
                e.name = to.clone();
            }
        }
        for e in fame.stunts.iter_mut().filter(|e| e.name == from) {
            if placed(&run.stunts, e) {
                e.name = to.clone();
            }
        }
        if let (Some(e), Some(r)) = (&mut fame.longest_jump, &run.longest_jump)
            && e.name == from
            && r.name == PLACEHOLDER
            && e.value == r.value
        {
            e.name = to.clone();
        }
        if let (Some(e), Some(r)) = (&mut fame.most_combos, &run.most_combos)
            && e.name == from
            && r.name == PLACEHOLDER
            && e.value == r.value
        {
            e.name = to;
        }
    }

    /// Stores a finished run's records under `name`.
    pub fn commit_run(&mut self, watch: &Watch, name: &str) {
        match watch {
            Watch::Race(w) => self.commit(w, name),
            Watch::Freestyle(w) => self.commit_freestyle(w, name),
        }
    }

    /// Renames a run's records from `from` to `to`.
    pub fn rename_run(&mut self, watch: &Watch, from: &str, to: &str) {
        match watch {
            Watch::Race(w) => self.rename(w, from, to),
            Watch::Freestyle(w) => self.rename_freestyle(w, from, to),
        }
    }

    /// Stores a finished race's records under `name` (`00331f50` names
    /// the placeholder). The race's placeholder entries enter the stored
    /// records by the insert rules, so records stored since the race began
    /// are kept.
    pub fn commit(&mut self, watch: &RecordWatch, name: &str) {
        let name = clean_name(name);
        let fame = self.courses.entry(watch.key.clone()).or_default();
        let mine = |e: &&Entry<f32>| e.name == PLACEHOLDER;
        for e in watch.fame.laps.iter().filter(mine) {
            insert_time(&mut fame.laps, e.value, &name, e.vehicle);
        }
        for e in watch.fame.races.iter().filter(mine) {
            insert_time(&mut fame.races, e.value, &name, e.vehicle);
        }
        if let Some(e) = watch.fame.longest_jump.as_ref().filter(mine) {
            beat(&mut fame.longest_jump, e.value, &name, e.vehicle);
        }
        if let Some(e) = watch
            .fame
            .most_combos
            .as_ref()
            .filter(|e| e.name == PLACEHOLDER)
        {
            beat(&mut fame.most_combos, e.value, &name, e.vehicle);
        }
        if let Some(e) = watch
            .fame
            .highest_stunt
            .as_ref()
            .filter(|e| e.name == PLACEHOLDER)
        {
            beat(&mut fame.highest_stunt, e.value, &name, e.vehicle);
        }
    }

    /// The name entry renames what [`HallOfFame::commit`] stored under
    /// `from` for this race.
    pub fn rename(&mut self, watch: &RecordWatch, from: &str, to: &str) {
        let to = clean_name(to);
        let Some(fame) = self.courses.get_mut(&watch.key) else {
            return;
        };
        let from = clean_name(from);
        let race = &watch.fame;
        let same = |a: &Entry<f32>, b: &Entry<f32>| a.value == b.value && a.vehicle == b.vehicle;
        let placed = |list: &[Entry<f32>], e: &Entry<f32>| {
            list.iter().any(|r| r.name == PLACEHOLDER && same(r, e))
        };
        for e in fame.laps.iter_mut().filter(|e| e.name == from) {
            if placed(&race.laps, e) {
                e.name = to.clone();
            }
        }
        for e in fame.races.iter_mut().filter(|e| e.name == from) {
            if placed(&race.races, e) {
                e.name = to.clone();
            }
        }
        if let (Some(e), Some(r)) = (&mut fame.longest_jump, &race.longest_jump)
            && e.name == from
            && r.name == PLACEHOLDER
            && same(e, r)
        {
            e.name = to.clone();
        }
        if let (Some(e), Some(r)) = (&mut fame.most_combos, &race.most_combos)
            && e.name == from
            && r.name == PLACEHOLDER
            && e.value == r.value
        {
            e.name = to.clone();
        }
        if let (Some(e), Some(r)) = (&mut fame.highest_stunt, &race.highest_stunt)
            && e.name == from
            && r.name == PLACEHOLDER
            && e.value == r.value
        {
            e.name = to;
        }
    }
}

/// The race loading screen's Hall of Fame (`002585c0`): the first fastest
/// lap, the first fastest race, the highest stunt score and the longest
/// jump with their holders ("<Nobody>" when empty), and the difficulty.
pub fn briefing(fame: Option<&CourseFame>, course: &str, expert: bool) -> Vec<String> {
    let fame = fame.cloned().unwrap_or_default();
    let who = |name: Option<&String>| {
        name.filter(|n| !n.is_empty())
            .map_or("<Nobody>".to_string(), |n| n.clone())
    };
    let time = |e: Option<&Entry<f32>>| match e.map(|e| e.value) {
        Some(t) if t > 5999.99 => "99:99.99".to_string(),
        Some(t) if t > 0. => {
            let h = (t * 100. + 0.5) as i64;
            format!("{:02}:{:02}.{:02}", h / 6000, h / 100 % 60, h % 100)
        }
        _ => "--:--.--".into(),
    };
    let laps = if course.ends_with("ends") || course.ends_with("endl") {
        3
    } else {
        5
    };
    let lap = fame.laps.first();
    let race = fame.races.first();
    let stunt = fame.highest_stunt.as_ref();
    let jump = fame.longest_jump.as_ref();
    vec![
        format!("Fastest Lap: {} {}", who(lap.map(|e| &e.name)), time(lap)),
        format!(
            "Fastest {laps}-Lap Race: {} {}",
            who(race.map(|e| &e.name)),
            time(race)
        ),
        format!(
            "Highest Stunt Score: {} {}",
            who(stunt.map(|e| &e.name)),
            stunt.map_or(0, |e| e.value)
        ),
        format!(
            "Longest Jump: {} {} Feet",
            who(jump.map(|e| &e.name)),
            jump.map_or(0, |e| e.value as i64)
        ),
        format!("{} Difficulty", if expert { "Expert" } else { "Normal" }),
    ]
}

/// A name as the Hall of Fame keeps it: trimmed, at most [`NAME_MAX`]
/// characters, never the placeholder or empty.
pub fn clean_name(name: &str) -> String {
    let name: String = name.trim().chars().take(NAME_MAX).collect();
    let name = name.trim_end().to_string();
    if name.is_empty() || name.starts_with('@') {
        crate::profile::DEFAULT_NAME.into()
    } else {
        name
    }
}

/// A finished run whose records the name entry names.
#[derive(Clone, Debug, PartialEq)]
pub enum Watch {
    Race(RecordWatch),
    Freestyle(FreestyleWatch),
}

impl Watch {
    /// The records the placeholder holds: what the name entry lists.
    pub fn broken(&self) -> Vec<String> {
        match self {
            Self::Race(w) => w.broken(),
            Self::Freestyle(w) => w.fame.held_by(PLACEHOLDER),
        }
    }
}

/// The profile's record for one Freestyle course (the 0x28-byte record):
/// what the "New Personal …" messages compare against.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FreestyleRecord {
    /// Longest jump in hundredths of a foot (finite, comparable).
    pub longest_jump_cft: u32,
    /// Highest objective scores and stunt scores, highest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objectives: Vec<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stunts: Vec<i64>,
    pub most_combos: u32,
}

impl FreestyleRecord {
    pub fn longest_jump(&self) -> f32 {
        self.longest_jump_cft as f32 / 100.
    }
}

/// One Freestyle objective run's record keeping for the player.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FreestyleWatch {
    /// The [`HallOfFame::freestyle_key`].
    pub key: String,
    pub vehicle: u16,
    pub fame: FreestyleFame,
    /// The loaded profile's record; `None` without a profile.
    pub personal: Option<FreestyleRecord>,
    /// Profile Points from combos: `n - 1` per landed chain of `n` > 1
    /// (`002fd350` adds them to the award accumulator).
    pub combo_points: i32,
    run_best: f32,
    #[serde(skip)]
    messages: Vec<String>,
}

impl FreestyleWatch {
    pub fn new(
        series: &str,
        course: &str,
        vehicle: u16,
        hall: Option<&HallOfFame>,
        personal: Option<FreestyleRecord>,
    ) -> Self {
        Self {
            key: HallOfFame::freestyle_key(series, course),
            vehicle,
            fame: hall
                .and_then(|h| h.freestyle_course(series, course))
                .cloned()
                .unwrap_or_default(),
            personal,
            ..Default::default()
        }
    }

    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// `002fd7c0`: a settled jump.
    pub fn jump(&mut self, feet: f32) {
        if !feet.is_finite() || feet <= self.run_best {
            return;
        }
        self.run_best = feet;
        let fame = beat(&mut self.fame.longest_jump, feet, PLACEHOLDER, self.vehicle);
        if fame {
            self.messages
                .push(format!("New Longest Jump {} Feet!", feet as i64));
        }
        if let Some(personal) = &mut self.personal
            && feet > personal.longest_jump()
        {
            personal.longest_jump_cft = (feet * 100.) as u32;
            if !fame {
                self.messages
                    .push(format!("New Personal Longest Jump {} Feet!", feet as i64));
            }
        }
    }

    /// `002fd350`: a finished chain; only a landed one counts.
    pub fn chain(&mut self, success: bool, count: usize, sum: f32) {
        if !success || count == 0 {
            return;
        }
        let points = (sum * count as f32) as i64;
        let count = count.min(u32::MAX as usize) as u32;
        let stunt = insert_score(&mut self.fame.stunts, points, PLACEHOLDER, self.vehicle);
        if let Some(rank) = stunt {
            self.messages
                .push(format!("New {}Highest Stunt Score!", ordinal(rank)));
        }
        if let Some(personal) = &mut self.personal
            && let Some(rank) = insert_plain_score(&mut personal.stunts, points)
            && stunt.is_none()
        {
            self.messages.push(format!(
                "New Personal {}Highest Stunt Score!",
                ordinal(rank)
            ));
        }
        if count == 1 {
            return;
        }
        self.combo_points = self.combo_points.saturating_add(count as i32 - 1);
        let combos = beat(&mut self.fame.most_combos, count, PLACEHOLDER, self.vehicle);
        if combos {
            self.messages.push("New Most Combos!".into());
        }
        if let Some(personal) = &mut self.personal
            && count > personal.most_combos
        {
            personal.most_combos = count;
            if !combos {
                self.messages.push("New Personal Most Combos!".into());
            }
        }
    }

    /// `002fce00`: the 120 s objective ended with `score`; the Profile
    /// Points it earns (score / 1000 plus the combo points).
    pub fn finish(&mut self, score: f32) -> i64 {
        let score = score.max(0.) as i64;
        let fame = insert_score(&mut self.fame.objectives, score, PLACEHOLDER, self.vehicle);
        if let Some(rank) = fame {
            self.messages
                .push(format!("New {}Highest Objective Score!", ordinal(rank)));
        }
        if let Some(personal) = &mut self.personal
            && let Some(rank) = insert_plain_score(&mut personal.objectives, score)
            && fame.is_none()
        {
            self.messages.push(format!(
                "New Personal {}Highest Objective Score!",
                ordinal(rank)
            ));
        }
        score / 1000 + i64::from(self.combo_points)
    }
}

/// The loaded profile's course record as the race starts: what the
/// "New Personal …" messages compare against (`003249c0`, `00324b40`,
/// `00324cc0`, `00324e60`, `00324f50`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Personal {
    /// Fastest laps and full-length race times, fastest first, seconds.
    pub laps: Vec<f32>,
    pub races: Vec<f32>,
    pub longest_jump: f32,
    pub most_combos: u32,
    pub most_points: i64,
}

fn insert_plain(list: &mut Vec<f32>, time: f32) -> Option<usize> {
    if !time.is_finite() || time <= 0. {
        return None;
    }
    let at = list.iter().position(|t| time < *t).unwrap_or(list.len());
    if at >= 3 {
        return None;
    }
    list.insert(at, time);
    list.truncate(3);
    Some(at + 1)
}

/// One race's record keeping for the player: the course's Hall of Fame as
/// it stood at the start, updated by the race hooks, and the messages they
/// raise.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RecordWatch {
    /// The [`HallOfFame::key`] of the course and difficulty.
    pub key: String,
    pub vehicle: u16,
    /// An Enduro race (3-lap race times).
    pub enduro: bool,
    /// The race runs the default laps (`001f1e30`: race times only then).
    pub full_length: bool,
    pub fame: CourseFame,
    /// The loaded profile's record; `None` without a profile or with more
    /// than one human (+0x10 = 1 gates the personal records).
    pub personal: Option<Personal>,
    /// The race's best jump so far (racer +0x118): the jump hook runs only
    /// when it grows.
    race_best: f32,
    #[serde(skip)]
    messages: Vec<String>,
}

impl RecordWatch {
    pub fn new(
        course: &str,
        expert: bool,
        vehicle: u16,
        full_length: bool,
        fame: Option<&HallOfFame>,
        personal: Option<Personal>,
    ) -> Self {
        let enduro = course.ends_with("ends") || course.ends_with("endl");
        Self {
            key: HallOfFame::key(course, expert),
            vehicle,
            enduro,
            full_length,
            fame: fame
                .and_then(|h| h.course(course, expert))
                .cloned()
                .unwrap_or_default(),
            personal,
            ..Default::default()
        }
    }

    /// Messages raised since the last call, oldest first.
    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// The records the placeholder holds: what the name entry lists.
    pub fn broken(&self) -> Vec<String> {
        self.fame.held_by(PLACEHOLDER, self.enduro)
    }

    /// `001f24b0`: a counted lap.
    pub fn lap(&mut self, time: f32) {
        let fame = insert_time(&mut self.fame.laps, time, PLACEHOLDER, self.vehicle);
        if let Some(rank) = fame {
            self.messages
                .push(format!("New {}Fastest Lap Time!", ordinal(rank)));
        }
        if let Some(personal) = &mut self.personal
            && let Some(rank) = insert_plain(&mut personal.laps, time)
            && fame.is_none()
        {
            self.messages
                .push(format!("New Personal {}Fastest Lap Time!", ordinal(rank)));
        }
    }

    /// `001f1e30`: the player finished in `total` seconds.
    pub fn finish(&mut self, total: f32) {
        if !self.full_length {
            return;
        }
        let laps = if self.enduro { 3 } else { 5 };
        let fame = insert_time(&mut self.fame.races, total, PLACEHOLDER, self.vehicle);
        if let Some(rank) = fame {
            self.messages.push(format!(
                "New {}Fastest {laps} Lap Race Time!",
                ordinal(rank)
            ));
        }
        if let Some(personal) = &mut self.personal
            && let Some(rank) = insert_plain(&mut personal.races, total)
            && fame.is_none()
        {
            // 0x445450 reads "5 Lap" for every event.
            self.messages.push(format!(
                "New Personal {}Fastest 5 Lap Race Time!",
                ordinal(rank)
            ));
        }
    }

    /// `0024df70` → `001f2610`: a settled jump of `feet`.
    pub fn jump(&mut self, feet: f32) {
        if !feet.is_finite() || feet <= self.race_best {
            return;
        }
        self.race_best = feet;
        let fame = beat(&mut self.fame.longest_jump, feet, PLACEHOLDER, self.vehicle);
        if fame {
            self.messages
                .push(format!("New Longest Jump {} Feet!", feet as i64));
        }
        if let Some(personal) = &mut self.personal
            && feet > PERSONAL_MIN_JUMP_FEET
            && feet > personal.longest_jump
        {
            personal.longest_jump = feet;
            if !fame {
                self.messages
                    .push(format!("New Personal Longest Jump {} Feet!", feet as i64));
            }
        }
    }

    /// `001f22a0`: a finished chain of `count` events whose points sum to
    /// `sum`; only a landed chain counts.
    pub fn chain(&mut self, success: bool, count: usize, sum: f32) {
        if !success || count == 0 {
            return;
        }
        let points = (sum * count as f32) as i64;
        let count = count.min(u32::MAX as usize) as u32;
        let stunt = beat(
            &mut self.fame.highest_stunt,
            points,
            PLACEHOLDER,
            self.vehicle,
        );
        if stunt {
            self.messages.push("New Highest Stunt Score!".into());
        }
        if let Some(personal) = &mut self.personal
            && points > personal.most_points
        {
            personal.most_points = points;
            if !stunt {
                self.messages
                    .push("New Personal Highest Stunt Score!".into());
            }
        }
        if count == 1 {
            return;
        }
        let combos = beat(&mut self.fame.most_combos, count, PLACEHOLDER, self.vehicle);
        if combos {
            self.messages.push("New Most Combos!".into());
        }
        if let Some(personal) = &mut self.personal
            && count > personal.most_combos
        {
            personal.most_combos = count;
            if !combos {
                self.messages.push("New Personal Most Combos!".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(personal: Option<Personal>) -> RecordWatch {
        RecordWatch::new("sx01", false, 7, true, None, personal)
    }

    #[test]
    fn laps_keep_the_fastest_three_and_shift_slower_ones() {
        let mut w = watch(None);
        w.lap(60.);
        w.lap(58.);
        w.lap(59.);
        w.lap(61.);
        let laps: Vec<f32> = w.fame.laps.iter().map(|e| e.value).collect();
        assert_eq!(laps, [58., 59., 60.]);
        assert_eq!(
            w.take_messages(),
            [
                "New Fastest Lap Time!",
                "New Fastest Lap Time!",
                "New 2nd Fastest Lap Time!"
            ]
        );
        // A tie is not faster.
        w.lap(60.);
        assert!(w.take_messages().is_empty());
        assert_eq!(w.fame.laps[0].vehicle, 7);
        assert_eq!(w.fame.laps[0].name, PLACEHOLDER);
    }

    #[test]
    fn personal_messages_only_when_no_hall_of_fame_record_is_set() {
        let mut hall = HallOfFame::default();
        let mut first = watch(None);
        first.lap(50.);
        first.jump(80.);
        hall.commit(&first, "ACE");
        let personal = Personal {
            laps: vec![55.],
            longest_jump: 45.,
            ..Default::default()
        };
        let mut w = RecordWatch::new("sx01", false, 3, true, Some(&hall), Some(personal));
        // Slower than the Hall of Fame's 50 s, faster than the personal 55 s.
        w.lap(52.);
        assert_eq!(
            w.take_messages(),
            ["New 2nd Fastest Lap Time!"],
            "a Hall of Fame record suppresses the personal one"
        );
        w.lap(53.);
        assert_eq!(w.take_messages(), ["New 3rd Fastest Lap Time!"]);
        w.lap(54.);
        assert_eq!(w.take_messages(), ["New Personal 3rd Fastest Lap Time!"]);
        // Jumps: under 40 ft is never personal; 60 ft beats 45 ft personal
        // but not the 80 ft Hall of Fame.
        w.jump(39.);
        w.jump(60.);
        assert_eq!(w.take_messages(), ["New Personal Longest Jump 60 Feet!"]);
        // Only a jump beyond the race's best is judged.
        w.jump(59.);
        w.jump(90.5);
        assert_eq!(w.take_messages(), ["New Longest Jump 90 Feet!"]);
    }

    #[test]
    fn chains_set_stunt_score_and_combos() {
        let mut w = watch(Some(Personal::default()));
        w.chain(true, 1, 500.);
        assert_eq!(w.take_messages(), ["New Highest Stunt Score!"]);
        w.chain(false, 6, 9000.);
        assert!(w.take_messages().is_empty());
        w.chain(true, 3, 100.);
        assert_eq!(w.take_messages(), ["New Most Combos!"]);
        assert_eq!(w.fame.highest_stunt.as_ref().unwrap().value, 500);
        w.chain(true, 2, 400.);
        assert_eq!(w.take_messages(), ["New Highest Stunt Score!"]);
        assert_eq!(w.fame.most_combos.as_ref().unwrap().value, 3);
    }

    #[test]
    fn race_times_only_at_full_length_with_the_event_lap_count() {
        let mut w = RecordWatch::new("env05ends", true, 1, true, None, Some(Personal::default()));
        assert!(w.enduro);
        w.finish(300.);
        assert_eq!(w.take_messages(), ["New Fastest 3 Lap Race Time!"]);
        let mut short = RecordWatch::new("sx01", false, 1, false, None, None);
        short.finish(100.);
        assert!(short.take_messages().is_empty() && short.fame.races.is_empty());
    }

    #[test]
    fn commit_names_the_placeholder_and_lists_broken_records() {
        let mut w = watch(None);
        w.lap(40.);
        w.finish(220.);
        w.chain(true, 4, 250.);
        w.jump(70.);
        assert_eq!(
            w.broken(),
            [
                "Fastest Lap Time",
                "Fastest 5 Lap Race Time",
                "Most Combos",
                "Longest Jump",
                "Highest Stunt Score"
            ]
        );
        let mut hall = HallOfFame::default();
        hall.commit(&w, "  rider-long-name ");
        let fame = hall.course("sx01", false).unwrap();
        assert_eq!(fame.laps[0].name, "rider-lo");
        assert!(fame.held_by(PLACEHOLDER, false).is_empty());
        assert!(hall.course("sx01", true).is_none());
        // The next race sees the record and ranks below it.
        let mut next = RecordWatch::new("sx01", false, 2, true, Some(&hall), None);
        next.lap(41.);
        assert_eq!(next.broken(), ["2nd Fastest Lap Time"]);
        assert_eq!(clean_name("@0"), crate::profile::DEFAULT_NAME);
        // Renaming touches only this race's entries.
        let mut other = watch(None);
        other.lap(39.);
        hall.commit(&other, "PLAYER1");
        hall.rename(&other, "PLAYER1", "ZED");
        let laps = &hall.course("sx01", false).unwrap().laps;
        assert_eq!(
            laps.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["ZED", "rider-lo"]
        );
        // A stale watch (started before another race stored a faster lap)
        // still merges by the insert rules.
        let mut stale = watch(None);
        stale.lap(39.5);
        hall.commit(&stale, "OLD");
        let laps = &hall.course("sx01", false).unwrap().laps;
        assert_eq!(
            laps.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["ZED", "OLD", "rider-lo"]
        );
    }

    #[test]
    fn freestyle_runs_keep_objective_and_stunt_scores() {
        let mut hall = HallOfFame::default();
        let mut w = FreestyleWatch::new("unspstunt", "env05", 2, Some(&hall), None);
        w.jump(55.);
        w.chain(true, 1, 800.);
        w.chain(true, 3, 300.);
        w.chain(false, 4, 900.);
        assert_eq!(
            w.take_messages(),
            [
                "New Longest Jump 55 Feet!",
                "New Highest Stunt Score!",
                "New Highest Stunt Score!",
                "New Most Combos!"
            ]
        );
        assert_eq!(w.combo_points, 2);
        // 12,345 / 1000 + 2 combo points.
        assert_eq!(w.finish(12_345.), 14);
        assert_eq!(w.take_messages(), ["New Highest Objective Score!"]);
        let watch = Watch::Freestyle(w.clone());
        assert_eq!(
            watch.broken(),
            [
                "Highest Objective Score",
                // `00333360` lists a table's first rank held only.
                "Highest Stunt Score",
                "Most Combos",
                "Longest Jump"
            ]
        );
        hall.commit_run(&watch, "PLAYER1");
        hall.rename_run(&watch, "PLAYER1", "ACE");
        let fame = hall.freestyle_course("unspstunt", "env05").unwrap();
        assert_eq!(fame.stunts[0].value, 900);
        assert_eq!(fame.stunts[1].value, 800);
        assert_eq!(fame.objectives[0].name, "ACE");
        assert!(hall.freestyle_course("spstunt", "env05").is_none());
        // A later run: below the Hall of Fame, above the personal record.
        let personal = FreestyleRecord {
            objectives: vec![5_000],
            ..Default::default()
        };
        let mut next = FreestyleWatch::new("unspstunt", "env05", 2, Some(&hall), Some(personal));
        next.finish(9_000.);
        assert_eq!(
            next.take_messages(),
            ["New 2nd Highest Objective Score!"],
            "the Hall of Fame suppresses the personal message"
        );
        let mut low = FreestyleWatch::new(
            "unspstunt",
            "env05",
            2,
            Some(&hall),
            Some(FreestyleRecord::default()),
        );
        low.fame.objectives = (0..3)
            .map(|i| Entry {
                value: 50_000 - i,
                name: "X".into(),
                vehicle: 0,
            })
            .collect();
        low.jump(30.);
        assert_eq!(low.finish(100.), 0);
        assert_eq!(
            low.take_messages(),
            [
                "New Personal Longest Jump 30 Feet!",
                "New Personal Highest Objective Score!"
            ]
        );
        assert_eq!(low.personal.unwrap().longest_jump_cft, 3000);
    }

    #[test]
    fn loading_briefing_names_the_first_holders() {
        assert_eq!(
            briefing(None, "env05ends", true),
            [
                "Fastest Lap: <Nobody> --:--.--",
                "Fastest 3-Lap Race: <Nobody> --:--.--",
                "Highest Stunt Score: <Nobody> 0",
                "Longest Jump: <Nobody> 0 Feet",
                "Expert Difficulty"
            ]
        );
        let mut w = watch(None);
        w.lap(65.004);
        w.jump(71.9);
        let mut hall = HallOfFame::default();
        hall.commit(&w, "ACE");
        let lines = briefing(hall.course("sx01", false), "sx01", false);
        assert_eq!(lines[0], "Fastest Lap: ACE 01:05.00");
        assert_eq!(lines[1], "Fastest 5-Lap Race: <Nobody> --:--.--");
        assert_eq!(lines[3], "Longest Jump: ACE 71 Feet");
    }
}
