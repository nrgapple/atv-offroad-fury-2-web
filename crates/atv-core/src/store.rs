//! The original Profile Store ("Unlock Items") and the profile's unlocks
//! (research/PROFILE_STORE.md).
//!
//! Exact-build evidence (private exports, bounded reading; nothing traced):
//!
//! - Catalogs: ATVs from `ui/frontend/uiQuad.txt` (`Name`, `Access`,
//!   `IngameID`, `Points`), rider gear from `uiRider.txt` (`Access_n`,
//!   `Points_n`) and courses from `Tracks.dat` (`access=`, `points=`, one list
//!   per event type). Access 0 starts locked; 1 (and 3, hidden) start owned
//!   (`00320db0` copies `access != 0` into the new profile).
//! - Purchase (`0029c4d0` ATV, `0029daa0` gear, `00296ca0` course,
//!   `0029b3b0` game): an owned item does nothing; otherwise `00323480`
//!   subtracts the cost from the remaining points only when the result stays
//!   at or above zero, marks the item owned and shows the new remaining
//!   points; short of points it plays the error sound and changes nothing.
//!   Earned points (`00323440`) add to both the lifetime (`earned`) and the
//!   remaining (`points`) totals; the store spends `points` only and keeps
//!   the total spent.
//! - Not for sale (`002f73d0`, `002f8510`): the four coloured Ravage Talons
//!   and the matching ATV2 helmets, jerseys, sleeves and pants, the Training
//!   Course reward (`0021f440`).
//! - A Jersey purchase also unlocks the sleeves and pants of its index.
//! - Event types (`0021add0`): every type costs 500 to open and starts open
//!   except King of the Hills (9) and Treasure Hunt (12), the two locked
//!   games.
//! - Cheats (`0033ede0`, entered on Unlock Items → Cheats): GIMMEPTS adds
//!   1,000 points (`00323440`); SHOWROOM unlocks every ATV but the last
//!   (G Ride); TRLBLAZR every course of types 0-8 but San Jacinto Isles (the
//!   last entry of Amateur/Pro Enduro, Amateur Freestyle, Amateur Nationals
//!   and Short Tracks); THREADS every gear item but the last (G-Ride);
//!   GAMEON the four games; GOLDCUPS opens every championship on Normal and
//!   Expert (`003238a0` for all 16 types, Short Tracks included); GABRIEL
//!   San Jacinto Isles; IGIVEUP all of SHOWROOM, TRLBLAZR, THREADS, GAMEON
//!   and GOLDCUPS. The G-Ride items were the reward for an ATV Offroad Fury
//!   save on the memory card.
use crate::profile::Profile;
use crate::rider_gear::Category;
use crate::ui::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Tracks.dat list keys by original event type (settings +8).
pub const EVENT_LISTS: [&str; 15] = [
    "send",
    "lend",
    "spstunt",
    "unspstunt",
    "spnat",
    "unspnat",
    "amsx",
    "prosx",
    "st",
    "koth",
    "tag",
    "hoc",
    "th",
    "train",
    "wpe",
];

/// The store's Tracks pages ("Switch Event" order) and their event types.
pub const TRACK_PAGES: [(u8, &str); 9] = [
    (5, "Amateur Nationals"),
    (4, "Pro Nationals"),
    (0, "Amateur Enduro"),
    (1, "Pro Enduro"),
    (6, "Amateur Supercross"),
    (7, "Pro Supercross"),
    (3, "Amateur Freestyle"),
    (2, "Pro Freestyle"),
    (8, "Short Tracks"),
];

/// The store's games: event type and name; each opens for 500 points.
pub const GAMES: [(u8, &str); 4] = [
    (9, "King of the Hills"),
    (10, "Tag"),
    (11, "Hockey"),
    (12, "Treasure Hunt"),
];
pub const EVENT_COST: i64 = 500;

/// The Rider Equipment pages: (page, gear slots it unlocks).
pub const GEAR_PAGES: [(&str, &[&str]); 5] = [
    ("Helmet", &["helmet"]),
    ("Jersey", &["shirt", "sleeve", "pant"]),
    ("Gloves", &["glove"]),
    ("Goggles", &["goggle"]),
    ("Boots", &["boot"]),
];

/// One ATV of the uiQuad.txt catalog, in store order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Atv {
    pub id: u16,
    pub name: String,
    /// 0 locked, 1 owned, 3 owned but hidden.
    pub access: u8,
    pub cost: i64,
}

/// One course of a Tracks.dat list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Course {
    pub scene: String,
    pub name: String,
    pub open: bool,
    pub cost: i64,
}

/// A Tracks.dat list: its event type and courses in file order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CourseList {
    pub kind: u8,
    pub courses: Vec<Course>,
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn int(value: Option<&str>) -> i64 {
    value
        .and_then(|v| v.split(';').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

/// `uiQuad.txt`: the ATVs in `[QuadN]` order.
pub fn parse_atvs(bytes: &[u8]) -> crate::Result<Vec<Atv>> {
    let doc = Document::parse(bytes)?;
    let count = int(doc.value("General", "NumQuads"));
    if !(1..=64).contains(&count) {
        return Err("uiQuad.txt has no NumQuads".into());
    }
    let mut out = Vec::new();
    for n in 1..=count {
        let section = format!("Quad{n}");
        let Some(id) = doc
            .value(&section, "IngameID")
            .and_then(|v| v.trim().parse().ok())
        else {
            continue;
        };
        out.push(Atv {
            id,
            name: unquote(doc.value(&section, "Name").unwrap_or_default()),
            access: int(doc.value(&section, "Access")).clamp(0, 3) as u8,
            cost: int(doc.value(&section, "Points")),
        });
    }
    Ok(out)
}

/// `Tracks.dat`: every event type's list (`0021cca0`), `<key><n>` sections.
pub fn parse_courses(bytes: &[u8]) -> crate::Result<Vec<CourseList>> {
    let doc = Document::parse(bytes)?;
    let mut out = Vec::new();
    for (kind, key) in EVENT_LISTS.iter().enumerate() {
        let mut courses = Vec::new();
        for n in 1.. {
            let section = format!("{key}{n}");
            let Some(scene) = doc.value(&section, "scn") else {
                break;
            };
            courses.push(Course {
                scene: scene.trim().to_ascii_lowercase(),
                name: unquote(doc.value(&section, "name").unwrap_or_default()),
                open: int(doc.value(&section, "access")) != 0,
                cost: int(doc.value(&section, "points")),
            });
        }
        out.push(CourseList {
            kind: kind as u8,
            courses,
        });
    }
    if out.iter().all(|l| l.courses.is_empty()) {
        return Err("Tracks.dat lists no courses".into());
    }
    Ok(out)
}

/// The profile's unlocks beyond a new profile's, and the points spent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Unlocks {
    /// Profile Points spent in the store (a record; `points` is what remains).
    pub spent: i64,
    /// ATVs bought or earned, by in-game id.
    pub atvs: Vec<u16>,
    /// Gear by slot (`rider_gear::SLOTS`), 0-based catalog indices.
    pub gear: BTreeMap<String, Vec<u16>>,
    /// Courses by Tracks.dat list key, 0-based indices.
    pub courses: BTreeMap<String, Vec<u16>>,
    /// Event types opened (games), by list key.
    pub events: Vec<String>,
    /// Unlock cheats entered (SHOWROOM, TRLBLAZR, THREADS, GAMEON,
    /// GOLDCUPS, GABRIEL).
    pub cheats: Vec<String>,
}

impl Unlocks {
    /// Items unlocked one by one (each `00323a00` step of +0x6c1).
    pub fn unlocked_count(&self) -> usize {
        self.atvs.len()
            + self.gear.values().map(Vec::len).sum::<usize>()
            + self.courses.values().map(Vec::len).sum::<usize>()
            + self.events.len()
    }

    fn cheat(&self, code: &str) -> bool {
        self.cheats.iter().any(|c| c == code)
    }
}

/// Profile Points left to spend (the original's "remaining" total).
pub fn remaining(profile: &Profile) -> i64 {
    profile.points
}

/// `002f73d0`: store indices 8-11, the coloured Talons from Training.
pub fn atv_for_sale(index: usize) -> bool {
    !(8..=11).contains(&index)
}

/// Whether an ATV (in-game id) may be ridden. Ids outside the catalog
/// (ATV06 and the port's motorcycles) were never sold and stay open.
pub fn atv_owned(catalog: &[Atv], id: u16, profile: &Profile) -> bool {
    let Some(index) = catalog.iter().position(|a| a.id == id) else {
        return true;
    };
    let u = &profile.store;
    catalog[index].access != 0
        || u.atvs.contains(&id)
        || (u.cheat("SHOWROOM") && index + 1 < catalog.len())
}

/// `002f8510`: the ATV2 Training gear is not sold (0-based indices).
pub fn gear_for_sale(slot: &str, index: usize) -> bool {
    match slot {
        "helmet" => !(19..=22).contains(&index),
        "shirt" | "sleeve" | "pant" => !(18..=21).contains(&index),
        _ => true,
    }
}

/// Whether a gear item (0-based index in its slot) may be worn.
pub fn gear_owned(category: &Category, index: usize, profile: &Profile) -> bool {
    let Some(item) = category.items.get(index) else {
        return false;
    };
    let u = &profile.store;
    !item.locked
        || u.gear
            .get(category.slot)
            .is_some_and(|l| l.contains(&(index as u16)))
        || (u.cheat("THREADS") && index + 1 < category.items.len())
}

/// Types whose last course is San Jacinto Isles (TRLBLAZR skips it).
fn last_is_bonus(kind: u8) -> bool {
    matches!(kind, 0 | 1 | 3 | 5 | 8)
}

/// Whether a course (0-based index in its event type's list) is unlocked.
pub fn course_owned(lists: &[CourseList], kind: u8, index: usize, profile: &Profile) -> bool {
    let Some(list) = lists.iter().find(|l| l.kind == kind) else {
        return true;
    };
    let Some(course) = list.courses.get(index) else {
        return false;
    };
    let u = &profile.store;
    let last = index + 1 == list.courses.len();
    let key = EVENT_LISTS[usize::from(kind)];
    course.open
        || u.courses
            .get(key)
            .is_some_and(|l| l.contains(&(index as u16)))
        || (kind <= 8 && u.cheat("TRLBLAZR") && !(last && last_is_bonus(kind)))
        || ((9..=12).contains(&kind) && u.cheat("GAMEON") && !(last && kind != 11))
        || (u.cheat("GABRIEL") && last && last_is_bonus(kind))
}

/// `0021add0`: event types start open but King of the Hills and Treasure
/// Hunt.
pub fn event_open(kind: u8, profile: &Profile) -> bool {
    let key = EVENT_LISTS.get(usize::from(kind)).copied().unwrap_or("");
    !matches!(kind, 9 | 12)
        || profile.store.events.iter().any(|e| e == key)
        || profile.store.cheat("GAMEON")
}

/// The original race course id's list: (event type, index), from its
/// venue and race variant (`env05ends` → Amateur Enduro, `sx03` → Pro
/// Supercross).
pub fn race_course_entry(lists: &[CourseList], id: &str) -> Option<(u8, usize)> {
    let (scene, kinds): (&str, &[u8]) = if let Some(v) = id.strip_suffix("ends") {
        (v, &[0])
    } else if let Some(v) = id.strip_suffix("endl") {
        (v, &[1])
    } else if let Some(v) = id.strip_suffix("st") {
        (v, &[8])
    } else {
        (id, &[5, 4, 6, 7])
    };
    lookup(lists, scene, kinds)
}

/// A Freestyle course (Environment or Pro Freestyle venue).
pub fn freestyle_entry(lists: &[CourseList], id: &str) -> Option<(u8, usize)> {
    lookup(lists, id, &[3, 2])
}

fn lookup(lists: &[CourseList], scene: &str, kinds: &[u8]) -> Option<(u8, usize)> {
    kinds.iter().find_map(|kind| {
        let list = lists.iter().find(|l| l.kind == *kind)?;
        let index = list.courses.iter().position(|c| c.scene == scene)?;
        Some((*kind, index))
    })
}

/// What Purchase Item did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purchase {
    Bought {
        cost: i64,
    },
    AlreadyOwned,
    /// Short of points: the original plays its error sound.
    Insufficient {
        cost: i64,
    },
    NotForSale,
}

/// `00323480`: spend `cost` when enough remains.
fn spend(profile: &mut Profile, cost: i64) -> bool {
    let cost = cost.max(0);
    if remaining(profile) - cost < 0 {
        return false;
    }
    profile.points -= cost;
    profile.store.spent += cost;
    true
}

fn add<T: PartialEq>(list: &mut Vec<T>, value: T) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// `0029c4d0`: buy the ATV at a store index.
pub fn buy_atv(profile: &mut Profile, catalog: &[Atv], index: usize) -> Purchase {
    let Some(atv) = catalog.get(index) else {
        return Purchase::NotForSale;
    };
    if atv_owned(catalog, atv.id, profile) {
        return Purchase::AlreadyOwned;
    }
    if !atv_for_sale(index) {
        return Purchase::NotForSale;
    }
    if !spend(profile, atv.cost) {
        return Purchase::Insufficient { cost: atv.cost };
    }
    add(&mut profile.store.atvs, atv.id);
    Purchase::Bought { cost: atv.cost }
}

/// `0029daa0`: buy a gear page's item; the Jersey also unlocks the sleeves
/// and pants of its index. `categories` is the rider_gear catalog.
pub fn buy_gear(
    profile: &mut Profile,
    categories: &[Category],
    page: usize,
    index: usize,
) -> Purchase {
    let Some((_, slots)) = GEAR_PAGES.get(page) else {
        return Purchase::NotForSale;
    };
    let Some(first) = categories.iter().find(|c| c.slot == slots[0]) else {
        return Purchase::NotForSale;
    };
    let Some(item) = first.items.get(index) else {
        return Purchase::NotForSale;
    };
    if gear_owned(first, index, profile) {
        return Purchase::AlreadyOwned;
    }
    if !gear_for_sale(first.slot, index) {
        return Purchase::NotForSale;
    }
    let cost = i64::from(item.points);
    if !spend(profile, cost) {
        return Purchase::Insufficient { cost };
    }
    for slot in *slots {
        if categories
            .iter()
            .any(|c| c.slot == *slot && index < c.items.len())
        {
            add(
                profile.store.gear.entry(slot.to_string()).or_default(),
                index as u16,
            );
        }
    }
    Purchase::Bought { cost }
}

/// `00323640`: unlock a course and the same venue in the games' lists.
pub fn unlock_course(profile: &mut Profile, lists: &[CourseList], kind: u8, index: usize) {
    let len = lists
        .iter()
        .find(|l| l.kind == kind)
        .map_or(0, |l| l.courses.len());
    if index >= len {
        return;
    }
    let last = index + 1 == len;
    let mut cascade: Vec<(u8, usize)> = Vec::new();
    match kind {
        0 | 1 | 3 | 5 if !last => {
            cascade.extend([14, 9, 10, 12].map(|k| (k, index)));
        }
        2 => cascade.extend([9, 10, 12].map(|k| (k, index + 7))),
        8 if !last => {
            let at = if index == 0 { 3 } else { 5 };
            cascade.extend([9, 10, 12].map(|k| (k, at)));
        }
        _ => {}
    }
    for (k, i) in cascade {
        unlock_course(profile, lists, k, i);
    }
    if !course_owned(lists, kind, index, profile) {
        let key = EVENT_LISTS[usize::from(kind)].to_string();
        add(profile.store.courses.entry(key).or_default(), index as u16);
    }
}

/// `00296ca0`: buy a course.
pub fn buy_course(profile: &mut Profile, lists: &[CourseList], kind: u8, index: usize) -> Purchase {
    let Some(course) = lists
        .iter()
        .find(|l| l.kind == kind)
        .and_then(|l| l.courses.get(index))
    else {
        return Purchase::NotForSale;
    };
    if course_owned(lists, kind, index, profile) {
        return Purchase::AlreadyOwned;
    }
    let cost = course.cost;
    if !spend(profile, cost) {
        return Purchase::Insufficient { cost };
    }
    unlock_course(profile, lists, kind, index);
    Purchase::Bought { cost }
}

/// `0029b3b0`: open a game's event type.
pub fn buy_event(profile: &mut Profile, kind: u8) -> Purchase {
    if event_open(kind, profile) {
        return Purchase::AlreadyOwned;
    }
    if !spend(profile, EVENT_COST) {
        return Purchase::Insufficient { cost: EVENT_COST };
    }
    add(
        &mut profile.store.events,
        EVENT_LISTS[usize::from(kind)].to_string(),
    );
    Purchase::Bought { cost: EVENT_COST }
}

/// The Training Course reward (`0021f440`): the four coloured Talons and
/// their ATV2 helmet, jersey, sleeves and pants.
pub fn training_reward(profile: &mut Profile, catalog: &[Atv]) {
    for index in 8..=11 {
        if let Some(atv) = catalog.get(index) {
            add(&mut profile.store.atvs, atv.id);
        }
    }
    for i in 19..=22u16 {
        add(profile.store.gear.entry("helmet".into()).or_default(), i);
    }
    for slot in ["shirt", "sleeve", "pant"] {
        for i in 18..=21u16 {
            add(profile.store.gear.entry(slot.into()).or_default(), i);
        }
    }
}

/// The unlock cheats IGIVEUP applies.
const EVERYTHING: [&str; 5] = ["SHOWROOM", "TRLBLAZR", "THREADS", "GAMEON", "GOLDCUPS"];

/// `0033ede0`: an entered cheat's message, None for "Invalid cheat!".
/// Profile cheats change `profile`; the race cheats (EATDIRT, BRINGIT) and
/// the other session switches are reported to the caller by name.
pub fn enter_cheat(profile: &mut Profile, code: &str) -> Option<(&'static str, &'static str)> {
    let code = code.trim().to_ascii_uppercase();
    let (code, message): (&'static str, &'static str) = match code.as_str() {
        "GIMMEPTS" => ("GIMMEPTS", "Added 1,000 Profile Points!"),
        "IGIVEUP" => ("IGIVEUP", "Unlocked Everything!"),
        "SHOWROOM" => ("SHOWROOM", "Unlocked All ATVs!"),
        "TRLBLAZR" => ("TRLBLAZR", "Unlocked All Tracks!"),
        "THREADS" => ("THREADS", "Unlocked All Equipment!"),
        "GAMEON" => ("GAMEON", "Unlocked All Games!"),
        "GOLDCUPS" => ("GOLDCUPS", "Unlocked All Championship Events!"),
        "GABRIEL" => ("GABRIEL", "Unlocked San Jacinto Isles!"),
        "WIDESCRN" => ("WIDESCRN", "Switched Aspect Ratio!"),
        "FLYPAPER" => ("FLYPAPER", "Wrecks Disabled!"),
        "EATDIRT" => ("EATDIRT", "Aggressive AI Enabled!"),
        "BRINGIT" => ("BRINGIT", "All Out AI Enabled!"),
        "GETONUP" => ("GETONUP", "Wheelie For Points Enabled!"),
        "STOPCAM" => ("STOPCAM", "Stop Action Replay Cam Enabled!"),
        "SYNC30HZ" => ("SYNC30HZ", "Frame Rate Locked To 30Hz!"),
        _ => return None,
    };
    match code {
        // 00323440 adds to the lifetime and the remaining totals.
        "GIMMEPTS" => profile.credit(1000),
        "IGIVEUP" => {
            for c in EVERYTHING {
                add(&mut profile.store.cheats, c.to_string());
            }
            open_championships(profile);
        }
        "SHOWROOM" | "TRLBLAZR" | "THREADS" | "GAMEON" | "GABRIEL" => {
            add(&mut profile.store.cheats, code.to_string());
        }
        "GOLDCUPS" => {
            add(&mut profile.store.cheats, code.to_string());
            open_championships(profile);
        }
        _ => {}
    }
    Some((code, message))
}

/// GOLDCUPS (`0033eca0`): every championship on Normal and Expert.
fn open_championships(profile: &mut Profile) {
    use crate::championship::{SERIES, save_key};
    for series in &SERIES {
        for expert in [false, true] {
            add(&mut profile.championships.opened, save_key(series, expert));
        }
    }
}

/// "1,000"-style thousands separators, as the store prints costs.
pub fn grouped(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if value < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rider_gear::Item;

    const QUADS: &str = "[General]\nNumQuads = 4\n[Quad1]\nName = \"Honda 400EX Yellow\"\nAccess = 1\nIngameID = 22\nPoints = 1000\n\
        [Quad2]\nName = \"Honda 300EX\"\nAccess = 3\nIngameID = 5\nPoints = 1000\n\
        [Quad3]\nName = \"Patriot\"\nAccess = 0\nIngameID = 8\nPoints = 250\n\
        [Quad4]\nName = \"G Ride\"\nAccess = 0\nIngameID = 19\nPoints = 1000\n";

    const TRACKS: &str = "[General]\nNumShortTracks=3\n[unspnat1]\nscn=env05\nname=\"Prospect Springs\"\naccess=1\npoints=0\n\
        [unspnat2]\nscn=env08\nname=\"Arlington Stream\"\naccess=0\npoints=200\n\
        [unspnat3]\nscn=env07\nname=\"San Jacinto Isles\"\naccess=0\npoints=500\n\
        [amsx1]\nscn=sx08\nname=\"Greenfield\"\naccess=1\npoints=0\n\
        [prosx1]\nscn=sx03\nname=\"Battle Creek\"\naccess=1\npoints=0\n\
        [send1]\nscn=env05\naccess=1\npoints=0\n[send2]\nscn=env08\naccess=0\npoints=200\n\
        [st1]\nscn=env06\naccess=0\npoints=100\n[st2]\nscn=env01\naccess=0\npoints=100\n[st3]\nscn=env07\naccess=0\npoints=100\n\
        [koth1]\nscn=env05\naccess=1\npoints=0\n[koth2]\nscn=env08\naccess=0\npoints=0\n[koth3]\nscn=env06\naccess=0\npoints=0\n\
        [koth4]\nscn=env01\naccess=0\npoints=0\n[koth5]\nscn=env04\naccess=0\npoints=0\n[koth6]\nscn=env03\naccess=0\npoints=0\n";

    fn profile(points: i64) -> Profile {
        Profile {
            points,
            ..Default::default()
        }
    }

    fn helmets() -> Vec<Category> {
        let item = |locked| Item {
            name: "x".into(),
            materials: vec![],
            locked,
            points: 50,
            logo: String::new(),
        };
        let mut out = Vec::new();
        for slot in crate::rider_gear::SLOTS {
            out.push(Category {
                slot,
                items: (0..26).map(|i| item(i % 2 == 1)).collect(),
            });
        }
        out
    }

    #[test]
    fn parses_the_catalogs() {
        let atvs = parse_atvs(QUADS.as_bytes()).unwrap();
        assert_eq!(atvs.len(), 4);
        assert_eq!((atvs[2].id, atvs[2].cost, atvs[2].access), (8, 250, 0));
        assert_eq!(atvs[1].access, 3);
        let lists = parse_courses(TRACKS.as_bytes()).unwrap();
        let nat = &lists[5];
        assert_eq!(nat.courses.len(), 3);
        assert_eq!(nat.courses[1].name, "Arlington Stream");
        assert!(nat.courses[0].open && !nat.courses[1].open);
        assert_eq!(lists[8].courses[2].cost, 100);
        assert_eq!(race_course_entry(&lists, "env08"), Some((5, 1)));
        assert_eq!(race_course_entry(&lists, "sx03"), Some((7, 0)));
        assert_eq!(race_course_entry(&lists, "env08ends"), Some((0, 1)));
        assert_eq!(race_course_entry(&lists, "env07st"), Some((8, 2)));
    }

    #[test]
    fn new_profile_owns_access_items() {
        let atvs = parse_atvs(QUADS.as_bytes()).unwrap();
        let p = profile(0);
        assert!(atv_owned(&atvs, 22, &p));
        // Access 3 (hidden) starts owned too.
        assert!(atv_owned(&atvs, 5, &p));
        assert!(!atv_owned(&atvs, 8, &p));
        // Not in the catalog: never sold, never locked.
        assert!(atv_owned(&atvs, 6, &p) && atv_owned(&atvs, 30, &p));
        assert!(event_open(10, &p) && event_open(11, &p) && event_open(8, &p));
        assert!(!event_open(9, &p) && !event_open(12, &p));
    }

    #[test]
    fn purchase_spends_remaining_points_only_when_enough() {
        let atvs = parse_atvs(QUADS.as_bytes()).unwrap();
        let mut p = profile(200);
        assert_eq!(
            buy_atv(&mut p, &atvs, 2),
            Purchase::Insufficient { cost: 250 }
        );
        assert_eq!(remaining(&p), 200);
        p.points += 50;
        assert_eq!(buy_atv(&mut p, &atvs, 2), Purchase::Bought { cost: 250 });
        assert_eq!((remaining(&p), p.store.spent), (0, 250));
        assert!(atv_owned(&atvs, 8, &p));
        assert_eq!(buy_atv(&mut p, &atvs, 2), Purchase::AlreadyOwned);
        assert_eq!(buy_atv(&mut p, &atvs, 0), Purchase::AlreadyOwned);
        // Saved and read back, with old saves reading as no unlocks.
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Profile>(&json).unwrap(), p);
        let old: Profile = serde_json::from_str("{\"points\":7}").unwrap();
        assert_eq!(old.store, Unlocks::default());
    }

    #[test]
    fn training_items_are_not_sold() {
        let mut atvs: Vec<Atv> = (0..13)
            .map(|i| Atv {
                id: 100 + i,
                name: String::new(),
                access: 0,
                cost: 10,
            })
            .collect();
        atvs[0].access = 1;
        let mut p = profile(1000);
        assert_eq!(buy_atv(&mut p, &atvs, 9), Purchase::NotForSale);
        assert_eq!(buy_atv(&mut p, &atvs, 12), Purchase::Bought { cost: 10 });
        let gear = helmets();
        // Helmet 20 (index 19) is the ATV2 Yellow Training helmet.
        assert_eq!(buy_gear(&mut p, &gear, 0, 19), Purchase::NotForSale);
        training_reward(&mut p, &atvs);
        assert!(atv_owned(&atvs, 109, &p));
        assert!(gear_owned(&gear[0], 19, &p));
    }

    #[test]
    fn jersey_unlocks_its_sleeves_and_pants() {
        let gear = helmets();
        let mut p = profile(100);
        let shirt = gear.iter().position(|c| c.slot == "shirt").unwrap();
        let pant = gear.iter().position(|c| c.slot == "pant").unwrap();
        assert!(!gear_owned(&gear[shirt], 3, &p));
        assert_eq!(buy_gear(&mut p, &gear, 1, 3), Purchase::Bought { cost: 50 });
        assert!(gear_owned(&gear[shirt], 3, &p) && gear_owned(&gear[pant], 3, &p));
        assert_eq!(remaining(&p), 50);
    }

    #[test]
    fn courses_and_games() {
        let lists = parse_courses(TRACKS.as_bytes()).unwrap();
        let mut p = profile(600);
        assert!(!course_owned(&lists, 5, 1, &p));
        assert_eq!(
            buy_course(&mut p, &lists, 5, 1),
            Purchase::Bought { cost: 200 }
        );
        assert!(course_owned(&lists, 5, 1, &p));
        // 00323640 opens the venue in the games' lists too.
        assert!(course_owned(&lists, 9, 1, &p));
        assert_eq!(
            buy_course(&mut p, &lists, 8, 0),
            Purchase::Bought { cost: 100 }
        );
        assert!(course_owned(&lists, 9, 3, &p));
        assert_eq!(buy_event(&mut p, 9), Purchase::Insufficient { cost: 500 });
        p.credit(1000);
        assert_eq!(buy_event(&mut p, 9), Purchase::Bought { cost: 500 });
        assert!(event_open(9, &p));
        assert_eq!(buy_event(&mut p, 10), Purchase::AlreadyOwned);
    }

    #[test]
    fn cheats_follow_0033ede0() {
        let atvs = parse_atvs(QUADS.as_bytes()).unwrap();
        let lists = parse_courses(TRACKS.as_bytes()).unwrap();
        let mut p = profile(0);
        assert_eq!(enter_cheat(&mut p, "nope"), None);
        assert_eq!(
            enter_cheat(&mut p, "gimmepts"),
            Some(("GIMMEPTS", "Added 1,000 Profile Points!"))
        );
        assert_eq!(remaining(&p), 1000);
        enter_cheat(&mut p, "SHOWROOM");
        // Every ATV but the last (G Ride).
        assert!(atv_owned(&atvs, 8, &p) && !atv_owned(&atvs, 19, &p));
        enter_cheat(&mut p, "TRLBLAZR");
        assert!(course_owned(&lists, 5, 1, &p));
        // San Jacinto Isles needs GABRIEL.
        assert!(!course_owned(&lists, 5, 2, &p) && !course_owned(&lists, 8, 2, &p));
        enter_cheat(&mut p, "GABRIEL");
        assert!(course_owned(&lists, 5, 2, &p) && course_owned(&lists, 8, 2, &p));
        let mut q = profile(0);
        enter_cheat(&mut q, "IGIVEUP");
        let st = crate::championship::series("st").unwrap();
        assert!(q.championships.is_open(st, false) && q.championships.is_open(st, true));
        assert!(event_open(12, &q));
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(10000000), "10,000,000");
        assert_eq!(grouped(25), "25");
    }
}
