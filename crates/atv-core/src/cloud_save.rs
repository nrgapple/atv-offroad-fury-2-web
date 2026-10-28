//! Pass Key cloud saves: the key and share-code formats, the save payload
//! that travels between devices, and the rules that merge two saves without
//! dropping progress (research/CLOUD_SAVE.md, client section).
//!
//! Authored for the port: the original kept profiles on a memory card. A
//! Pass Key (`FURY-XXXX-XXXX-XXXX-XXXX`) is 80 random bits in Crockford
//! base 32, made by the client; whoever holds it continues that save. A
//! share code (`SHARE-…`) names a server snapshot that others copy from.
use crate::championship::{Championships, Progress};
use crate::hall_of_fame::{CourseFame, Entry, FreestyleFame, FreestyleRecord, HallOfFame};
use crate::profile::{CourseRecord, DEFAULT_NAME, Profile, ProfileBook};
use crate::store::Unlocks;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Crockford base 32: no I, L, O or U.
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Symbols in a key or share code (80 bits).
pub const SYMBOLS: usize = 16;
pub const KEY_PREFIX: &str = "FURY";
pub const SHARE_PREFIX: &str = "SHARE";
/// The payload version this build writes.
pub const VERSION: u32 = 1;
/// The server's cap on the serialized save.
pub const MAX_BYTES: usize = 64 * 1024;
/// Saves whose JSON is longer travel deflated ([`Save::to_wire`]).
pub const COMPRESS_OVER: usize = 48 * 1024;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut n, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let v = BASE64.iter().position(|b| *b == c)? as u32;
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The 16 symbols of 80 random bits (big-endian, five bits each).
pub fn symbols(bytes: [u8; 10]) -> String {
    let mut bits: u128 = 0;
    for b in bytes {
        bits = bits << 8 | u128::from(b);
    }
    (0..SYMBOLS)
        .map(|i| ALPHABET[((bits >> (5 * (SYMBOLS - 1 - i))) & 31) as usize] as char)
        .collect()
}

/// `PREFIX-XXXX-XXXX-XXXX-XXXX`.
pub fn format(prefix: &str, symbols: &str) -> String {
    let groups: Vec<&str> = symbols
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    format!("{prefix}-{}", groups.join("-"))
}

/// A new Pass Key from 10 bytes of a cryptographic random source.
pub fn pass_key(random: [u8; 10]) -> String {
    format(KEY_PREFIX, &symbols(random))
}

/// The typed symbols of a key or code as the entry keeps them: upper case,
/// O read as 0, I and L as 1, other characters dropped, at most 16 (a
/// pasted prefix or link is removed first).
pub fn clean_symbols(text: &str, prefix: &str) -> String {
    // A pasted link: `…#key=FURY-…` or `…#share=SHARE-…`.
    let text = text.rsplit('=').next().unwrap_or(text);
    let raw: String = text
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    // The prefix goes only when a full code follows it (SHARE's letters
    // are symbols too; FURY's U is not).
    let raw = match raw.strip_prefix(prefix) {
        Some(rest) if rest.len() >= SYMBOLS || prefix == KEY_PREFIX => rest.to_string(),
        _ => raw,
    };
    raw.chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .filter(|c| ALPHABET.contains(&(*c as u8)))
        .take(SYMBOLS)
        .collect()
}

/// The contract's normalization: upper case, spaces and dashes and the
/// prefix removed, O → 0, I and L → 1; anything else, or not exactly 16
/// symbols, is rejected. Returns the canonical `PREFIX-XXXX-…`.
pub fn normalize(text: &str, prefix: &str) -> Option<String> {
    let mut raw: String = text
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '\t'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if raw.len() == prefix.len() + SYMBOLS && raw.starts_with(prefix) {
        raw.drain(..prefix.len());
    }
    if raw.len() != SYMBOLS {
        return None;
    }
    let mut out = String::with_capacity(SYMBOLS);
    for c in raw.chars() {
        let c = match c {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        if !ALPHABET.contains(&(c as u8)) {
            return None;
        }
        out.push(c);
    }
    Some(format(prefix, &out))
}

pub fn normalize_key(text: &str) -> Option<String> {
    normalize(text, KEY_PREFIX)
}

pub fn normalize_share(text: &str) -> Option<String> {
    normalize(text, SHARE_PREFIX)
}

/// The progress that travels with a Pass Key: every profile (points,
/// purchases and unlocks, championships and trophies, records) and the Hall
/// of Fame. Device settings (graphics, controls, audio, garage choice) stay
/// on the device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Save {
    /// Payload version ([`VERSION`]).
    pub v: u32,
    /// The profile book: the profiles and which one is loaded.
    pub profiles: ProfileBook,
    pub hall_of_fame: HallOfFame,
}

impl Default for Save {
    fn default() -> Self {
        Self {
            v: VERSION,
            profiles: ProfileBook::default(),
            hall_of_fame: HallOfFame::default(),
        }
    }
}

impl Save {
    pub fn new(profiles: ProfileBook, hall_of_fame: HallOfFame) -> Self {
        Self {
            v: VERSION,
            profiles,
            hall_of_fame,
        }
    }

    /// Reads a payload; a newer version than this build's is refused (it
    /// may hold progress this build would drop).
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("Not a save file ({e})."))?;
        Self::from_value(value)
    }

    pub fn from_value(value: serde_json::Value) -> Result<Self, String> {
        if !value.is_object() {
            return Err("Not a save file.".into());
        }
        if let Some(packed) = value.get("z").and_then(serde_json::Value::as_str) {
            let bytes = base64_decode(packed).ok_or("Not a save file (bad encoding).")?;
            let text = miniz_oxide::inflate::decompress_to_vec_with_limit(&bytes, 8 << 20)
                .map_err(|_| "Not a save file (bad encoding).")?;
            let inner: serde_json::Value =
                serde_json::from_slice(&text).map_err(|e| format!("Not a save file ({e})."))?;
            if inner.get("z").is_some() {
                return Err("Not a save file.".into());
            }
            return Self::from_value(inner);
        }
        let version = value["v"].as_u64().unwrap_or(0);
        if version == 0 {
            return Err("Not a save file.".into());
        }
        if version > u64::from(VERSION) {
            return Err("This save is from a newer version of the game. Update to load it.".into());
        }
        let mut save: Save =
            serde_json::from_value(value).map_err(|e| format!("Not a save file ({e})."))?;
        save.v = VERSION;
        save.profiles.normalize();
        Ok(save)
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The save as it travels to the server: the plain JSON object, or,
    /// when that is over [`COMPRESS_OVER`] bytes, `{"v":1,"z":…}` with the
    /// JSON deflated (raw DEFLATE) and in standard base 64. A completed
    /// save with a full Hall of Fame runs past the 64 KiB cap as plain JSON.
    pub fn to_wire(&self) -> serde_json::Value {
        let text = self.to_json();
        if text.len() <= COMPRESS_OVER {
            return self.to_value();
        }
        let packed = miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 9);
        serde_json::json!({"v": VERSION, "z": base64_encode(&packed)})
    }

    /// Whether the save holds anything beyond a new game's single empty
    /// PLAYER1: the point at which a Pass Key is made.
    pub fn has_progress(&self) -> bool {
        let book = &self.profiles;
        !hall_is_empty(&self.hall_of_fame)
            || book.profiles.len() != 1
            || book
                .profiles
                .iter()
                .any(|p| p.name != DEFAULT_NAME || *p != Profile::named(&p.name))
    }

    /// The loaded profile, else the first.
    pub fn main_profile(&self) -> Option<&Profile> {
        self.profiles
            .current_profile()
            .or_else(|| self.profiles.profiles.first())
    }

    /// Hall of Fame holders kept (each name in a top three or a record).
    pub fn fame_records(&self) -> usize {
        let h = &self.hall_of_fame;
        h.courses
            .values()
            .map(|c| {
                c.laps.len()
                    + c.races.len()
                    + usize::from(c.longest_jump.is_some())
                    + usize::from(c.most_combos.is_some())
                    + usize::from(c.highest_stunt.is_some())
            })
            .sum::<usize>()
            + h.freestyle
                .values()
                .map(|f| {
                    f.objectives.len()
                        + f.stunts.len()
                        + usize::from(f.longest_jump.is_some())
                        + usize::from(f.most_combos.is_some())
                })
                .sum::<usize>()
    }
}

fn hall_is_empty(hall: &HallOfFame) -> bool {
    hall.courses.values().all(|c| *c == CourseFame::default())
        && hall
            .freestyle
            .values()
            .all(|f| *f == FreestyleFame::default())
}

/// Most profiles a merge keeps. A merge never drops a profile, so two
/// devices that each filled their four save files may hold more than the
/// original's four; the Profile Editor then refuses new ones until some
/// are deleted.
pub const MERGE_SLOTS: usize = 8;

/// Merges two saves without dropping progress.
///
/// With `base` (the save both sides last agreed on: a 409 conflict or a
/// pull), `local`'s changes since `base` are replayed onto `remote`:
/// counters add both sides' gains (`remote + local − base`), Profile Points
/// included (never below 0), so points earned on two devices both count
/// and points spent on two devices are both spent. Without `base` (loading
/// a Pass Key, share code or file onto a device that has its own progress)
/// the merge is idempotent: counters keep the larger side, so merging the
/// same save twice changes nothing and no points are minted.
///
/// Either way: records and Hall of Fame top threes keep the best entries,
/// trophies, purchases, unlocks, opened series and objectives are unions,
/// a championship keeps the side that changed (or the further round), and
/// profiles match by name. A profile deleted on one side is dropped only
/// when the other side did not change it since `base`.
pub fn merge(local: &Save, remote: &Save, base: Option<&Save>) -> Save {
    let mode = match base {
        Some(b) => Mode::Rebase(b),
        None => Mode::Union,
    };
    Save {
        v: VERSION,
        profiles: merge_books(&local.profiles, &remote.profiles, mode),
        hall_of_fame: merge_hall(&local.hall_of_fame, &remote.hall_of_fame),
    }
}

#[derive(Clone, Copy)]
enum Mode<'a> {
    Rebase(&'a Save),
    Union,
}

fn same_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn find<'p>(book: &'p ProfileBook, name: &str) -> Option<&'p Profile> {
    book.profiles.iter().find(|p| same_name(&p.name, name))
}

fn merge_books(local: &ProfileBook, remote: &ProfileBook, mode: Mode) -> ProfileBook {
    let base_book = match mode {
        Mode::Rebase(b) => Some(&b.profiles),
        Mode::Union => None,
    };
    let mut names: Vec<&str> = remote.profiles.iter().map(|p| p.name.as_str()).collect();
    for p in &local.profiles {
        if !names.iter().any(|n| same_name(n, &p.name)) {
            names.push(&p.name);
        }
    }
    let mut profiles = Vec::new();
    for name in names {
        let l = find(local, name);
        let r = find(remote, name);
        let b = base_book.and_then(|b| find(b, name));
        let merged = match (l, r) {
            (Some(l), Some(r)) => Some(match mode {
                Mode::Rebase(_) => merge_profile(l, r, Some(b.unwrap_or(&Profile::named(name)))),
                Mode::Union => merge_profile(l, r, None),
            }),
            // Deleted on one side: kept when the other side progressed since
            // the base (or there is no base to tell).
            (Some(only), None) | (None, Some(only)) => match (base_book, b) {
                (Some(_), Some(b)) if b == only => None,
                _ => Some(only.clone()),
            },
            (None, None) => None,
        };
        profiles.extend(merged);
    }
    profiles.truncate(MERGE_SLOTS);
    // The loaded profile stays this device's choice where it survives.
    let pick = |book: &ProfileBook| {
        book.current_profile()
            .and_then(|c| profiles.iter().position(|p| same_name(&p.name, &c.name)))
    };
    let current = pick(local).or_else(|| pick(remote));
    ProfileBook { current, profiles }
}

/// `remote + local − base` (Rebase) or the larger (Union), never below 0.
fn counter_i64(l: i64, r: i64, b: Option<i64>) -> i64 {
    match b {
        Some(b) => r.saturating_add(l.saturating_sub(b)).max(0),
        None => l.max(r),
    }
}

fn counter_u32(l: u32, r: u32, b: Option<u32>) -> u32 {
    counter_i64(i64::from(l), i64::from(r), b.map(i64::from)).clamp(0, i64::from(u32::MAX)) as u32
}

fn counters<K: Ord + Clone>(
    l: &BTreeMap<K, u32>,
    r: &BTreeMap<K, u32>,
    b: Option<&BTreeMap<K, u32>>,
) -> BTreeMap<K, u32> {
    let mut out = BTreeMap::new();
    for key in l.keys().chain(r.keys()) {
        let get = |m: &BTreeMap<K, u32>| m.get(key).copied().unwrap_or(0);
        let value = counter_u32(get(l), get(r), b.map(get));
        if value > 0 {
            out.insert(key.clone(), value);
        }
    }
    out
}

fn union<T: PartialEq + Clone>(r: &[T], l: &[T]) -> Vec<T> {
    let mut out = r.to_vec();
    for item in l {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }
    out
}

fn union_map<T: PartialEq + Clone>(
    r: &BTreeMap<String, Vec<T>>,
    l: &BTreeMap<String, Vec<T>>,
) -> BTreeMap<String, Vec<T>> {
    let mut out = r.clone();
    for (key, list) in l {
        let merged = union(out.get(key).map_or(&[][..], Vec::as_slice), list);
        out.insert(key.clone(), merged);
    }
    out
}

/// A fastest-first (or highest-first) top three of both lists.
fn top_three<T: PartialOrd + Copy>(a: &[T], b: &[T], highest: bool) -> Vec<T> {
    let mut all: Vec<T> = Vec::new();
    for v in a.iter().chain(b) {
        if !all.contains(v) {
            all.push(*v);
        }
    }
    all.sort_by(|x, y| {
        let order = x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal);
        if highest { order.reverse() } else { order }
    });
    all.truncate(3);
    all
}

fn min_time(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn merge_profile(l: &Profile, r: &Profile, b: Option<&Profile>) -> Profile {
    let mut out = r.clone();
    out.points = counter_i64(l.points, r.points, b.map(|b| b.points));
    out.earned = counter_i64(
        l.earned_points(),
        r.earned_points(),
        b.map(Profile::earned_points),
    );
    out.races = counter_u32(l.races, r.races, b.map(|b| b.races));
    out.wins = counter_u32(l.wins, r.wins, b.map(|b| b.wins));
    out.best_stunt = l.best_stunt.max(r.best_stunt);
    out.stunts.merge(&l.stunts);
    out.championships = merge_championships(
        &l.championships,
        &r.championships,
        b.map(|b| &b.championships),
    );
    out.store = merge_store(&l.store, &r.store, b.map(|b| &b.store));
    out.atv_wins = counters(&l.atv_wins, &r.atv_wins, b.map(|b| &b.atv_wins));
    let empty = CourseRecord::default();
    for (key, lc) in &l.courses {
        let base = b.map(|b| b.courses.get(key).unwrap_or(&empty));
        let merged = match r.courses.get(key) {
            Some(rc) => merge_course(lc, rc, base),
            None => merge_course(lc, &empty, base.map(|_| &empty)),
        };
        out.courses.insert(key.clone(), merged);
    }
    for (key, lf) in &l.freestyle {
        let merged = match r.freestyle.get(key) {
            Some(rf) => merge_freestyle(lf, rf),
            None => lf.clone(),
        };
        out.freestyle.insert(key.clone(), merged);
    }
    out
}

fn merge_course(l: &CourseRecord, r: &CourseRecord, b: Option<&CourseRecord>) -> CourseRecord {
    let mut stunts = r.stunts;
    stunts.merge(&l.stunts);
    CourseRecord {
        races: counter_u32(l.races, r.races, b.map(|b| b.races)),
        wins: counter_u32(l.wins, r.wins, b.map(|b| b.wins)),
        best_lap_ms: min_time(l.best_lap_ms, r.best_lap_ms),
        best_total_ms: min_time(l.best_total_ms, r.best_total_ms),
        atv_wins: counters(&l.atv_wins, &r.atv_wins, b.map(|b| &b.atv_wins)),
        top_laps_ms: top_three(&l.top_laps_ms, &r.top_laps_ms, false),
        top_races_ms: top_three(&l.top_races_ms, &r.top_races_ms, false),
        stunts,
    }
}

fn merge_freestyle(l: &FreestyleRecord, r: &FreestyleRecord) -> FreestyleRecord {
    FreestyleRecord {
        longest_jump_cft: l.longest_jump_cft.max(r.longest_jump_cft),
        objectives: top_three(&l.objectives, &r.objectives, true),
        stunts: top_three(&l.stunts, &r.stunts, true),
        most_combos: l.most_combos.max(r.most_combos),
    }
}

fn merge_store(l: &Unlocks, r: &Unlocks, b: Option<&Unlocks>) -> Unlocks {
    Unlocks {
        spent: counter_i64(l.spent, r.spent, b.map(|b| b.spent)),
        atvs: union(&r.atvs, &l.atvs),
        gear: union_map(&r.gear, &l.gear),
        courses: union_map(&r.courses, &l.courses),
        events: union(&r.events, &l.events),
        cheats: union(&r.cheats, &l.cheats),
    }
}

/// The better trophy (Gold sorts first).
fn best_trophy(
    a: Option<crate::championship::Trophy>,
    b: Option<crate::championship::Trophy>,
) -> Option<crate::championship::Trophy> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn merge_progress(l: &Progress, r: &Progress, b: Option<&Progress>) -> Progress {
    let further = |x: &Progress, y: &Progress| (x.round, x.points[0]) > (y.round, y.points[0]);
    let mut out = match b {
        Some(b) if l == b => r.clone(),
        Some(b) if r == b => l.clone(),
        _ if further(l, r) => l.clone(),
        _ => r.clone(),
    };
    out.trophy = best_trophy(l.trophy, r.trophy);
    out
}

fn merge_championships(
    l: &Championships,
    r: &Championships,
    b: Option<&Championships>,
) -> Championships {
    let mut out = r.clone();
    let empty = Progress::default();
    for (key, lp) in &l.progress {
        let base = b.map(|b| b.progress.get(key).unwrap_or(&empty));
        let merged = merge_progress(lp, r.progress.get(key).unwrap_or(&empty), base);
        out.progress.insert(key.clone(), merged);
    }
    out.opened = union(&r.opened, &l.opened);
    for (key, bits) in &l.objectives {
        *out.objectives.entry(key.clone()).or_default() |= bits;
    }
    out
}

/// Hall of Fame entries: both sides' top threes, best first, each holder
/// once.
fn fame_list<T: PartialOrd + Copy>(a: &[Entry<T>], b: &[Entry<T>], highest: bool) -> Vec<Entry<T>>
where
    Entry<T>: PartialEq + Clone,
{
    let mut all: Vec<Entry<T>> = Vec::new();
    for e in b.iter().chain(a) {
        if !all.contains(e) {
            all.push(e.clone());
        }
    }
    // Stable: on a tie the remote's (then the earlier) entry stays ahead.
    all.sort_by(|x, y| {
        let order = x
            .value
            .partial_cmp(&y.value)
            .unwrap_or(std::cmp::Ordering::Equal);
        if highest { order.reverse() } else { order }
    });
    all.truncate(3);
    all
}

fn fame_best<T: PartialOrd + Copy>(a: &Option<Entry<T>>, b: &Option<Entry<T>>) -> Option<Entry<T>>
where
    Entry<T>: Clone,
{
    match (a, b) {
        (Some(x), Some(y)) if x.value > y.value => Some(x.clone()),
        (_, Some(y)) => Some(y.clone()),
        (x, None) => x.clone(),
    }
}

fn merge_hall(l: &HallOfFame, r: &HallOfFame) -> HallOfFame {
    let mut out = r.clone();
    for (key, lc) in &l.courses {
        let rc = r.courses.get(key).cloned().unwrap_or_default();
        out.courses.insert(
            key.clone(),
            CourseFame {
                laps: fame_list(&lc.laps, &rc.laps, false),
                races: fame_list(&lc.races, &rc.races, false),
                longest_jump: fame_best(&lc.longest_jump, &rc.longest_jump),
                most_combos: fame_best(&lc.most_combos, &rc.most_combos),
                highest_stunt: fame_best(&lc.highest_stunt, &rc.highest_stunt),
            },
        );
    }
    for (key, lf) in &l.freestyle {
        let rf = r.freestyle.get(key).cloned().unwrap_or_default();
        out.freestyle.insert(
            key.clone(),
            FreestyleFame {
                objectives: fame_list(&lf.objectives, &rf.objectives, true),
                stunts: fame_list(&lf.stunts, &rf.stunts, true),
                longest_jump: fame_best(&lf.longest_jump, &rf.longest_jump),
                most_combos: fame_best(&lf.most_combos, &rf.most_combos),
            },
        );
    }
    out
}

/// Adds `profile` to `book` as a new profile (a share code's "Add As New
/// Profile"): a free name is chosen ("ACE", "ACE2"…) and it is loaded.
pub fn add_profile(
    book: &mut ProfileBook,
    profile: &Profile,
) -> Result<usize, crate::profile::BookError> {
    if book.is_full() {
        return Err(crate::profile::BookError::Full);
    }
    let base = crate::profile::clean_name(&profile.name);
    let base = if base.trim().is_empty() {
        DEFAULT_NAME.to_string()
    } else {
        base.trim_end().to_string()
    };
    let name = (1..)
        .map(|n: usize| {
            if n == 1 {
                base.clone()
            } else {
                let suffix = n.to_string();
                let keep = crate::profile::NAME_MAX.saturating_sub(suffix.len());
                format!("{}{suffix}", base.chars().take(keep).collect::<String>())
            }
        })
        .find(|n| book.check_name(n, None).is_ok())
        .unwrap_or_else(|| book.suggested_name());
    book.profiles.push(Profile {
        name,
        ..profile.clone()
    });
    let index = book.profiles.len() - 1;
    book.current = Some(index);
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::championship::Trophy;

    #[test]
    fn keys_are_sixteen_crockford_symbols() {
        assert_eq!(pass_key([0; 10]), "FURY-0000-0000-0000-0000");
        assert_eq!(pass_key([0xff; 10]), "FURY-ZZZZ-ZZZZ-ZZZZ-ZZZZ");
        let key = pass_key([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x0f, 0xed]);
        assert_eq!(key.len(), 24);
        assert_eq!(key, "FURY-28T5-CY4T-QKFF-03ZD");
        assert!(key[5..].bytes().all(|b| b == b'-' || ALPHABET.contains(&b)));
        // Every bit lands in exactly one symbol.
        for bit in 0..80 {
            let mut bytes = [0u8; 10];
            bytes[bit / 8] = 0x80 >> (bit % 8);
            let s = symbols(bytes);
            assert_eq!(s.chars().filter(|c| *c != '0').count(), 1, "bit {bit}");
        }
    }

    #[test]
    fn keys_normalize_as_the_contract_says() {
        let key = "FURY-28T5-CY4T-QKFF-03ZD";
        assert_eq!(normalize_key(key).as_deref(), Some(key));
        assert_eq!(
            normalize_key("fury 28t5 cy4t qkff o3zd").as_deref(),
            Some(key)
        );
        assert_eq!(normalize_key("28T5CY4TQKFF03ZD").as_deref(), Some(key));
        assert_eq!(
            normalize_key("2815-CY4T-QKFF-03ZD").as_deref(),
            normalize_key("28L5-cy4t-qkff-03zd").as_deref()
        );
        assert_eq!(
            normalize_key("2I15CY4TQKFF03ZD").as_deref(),
            Some("FURY-2115-CY4T-QKFF-03ZD")
        );
        // U is not a symbol; wrong lengths are refused.
        assert_eq!(normalize_key("FURY-28T5-CY4T-QKFF-03ZU"), None);
        assert_eq!(normalize_key("FURY-28T5-CY4T-QKFF-03Z"), None);
        assert_eq!(normalize_key("FURY-28T5-CY4T-QKFF-03ZDD"), None);
        assert_eq!(normalize_key("FURY_28T5_CY4T_QKFF_03ZD"), None);
        assert_eq!(
            normalize_share("share-abcd-efgh-jkmn-pqrs").as_deref(),
            Some("SHARE-ABCD-EFGH-JKMN-PQRS")
        );
        // A share code may itself start with S-H-A-R-E.
        assert_eq!(
            normalize_share("SHARE0000000000Z").as_deref(),
            Some("SHARE-SHAR-E000-0000-000Z")
        );
    }

    #[test]
    fn typed_symbols_are_cleaned_for_the_entry() {
        assert_eq!(clean_symbols("fury-28t5-cy4t", KEY_PREFIX), "28T5CY4T");
        assert_eq!(
            clean_symbols(
                "https://furyremastered.com/play.html#key=FURY-28T5-CY4T-QKFF-O3ZD",
                KEY_PREFIX
            ),
            "28T5CY4TQKFF03ZD"
        );
        assert_eq!(
            clean_symbols("SHARE-ABCD-EFGH-JKMN-PQRS", SHARE_PREFIX),
            "ABCDEFGHJKMNPQRS"
        );
        assert_eq!(clean_symbols("SHARE", SHARE_PREFIX), "SHARE");
        assert_eq!(clean_symbols("u!u", KEY_PREFIX), "");
    }

    fn sample() -> Save {
        let mut book = ProfileBook::migrate(None);
        let p = book.current_mut().unwrap();
        p.credit(120);
        p.races = 3;
        p.wins = 1;
        p.store.atvs = vec![4];
        p.record_course(&crate::profile::RaceResult {
            course: "sx01".into(),
            quad: 3,
            place: 1,
            best_lap: Some(41.5),
            total: Some(212.25),
            full_length: true,
            stunts: Default::default(),
            personal: Some(crate::hall_of_fame::Personal {
                laps: vec![41.5, 42.],
                races: vec![212.25],
                ..Default::default()
            }),
        });
        let mut hall = HallOfFame::default();
        let mut watch = crate::hall_of_fame::RecordWatch::new("sx01", false, 3, true, None, None);
        watch.lap(41.5);
        watch.finish(212.25);
        hall.commit(&watch, "ACE");
        Save::new(book, hall)
    }

    #[test]
    fn the_payload_round_trips() {
        let save = sample();
        let text = save.to_json();
        assert!(text.starts_with("{\"v\":1,"));
        assert!(text.contains("\"hallOfFame\""));
        assert_eq!(Save::parse(&text).unwrap(), save);
        // Unknown and newer payloads.
        assert!(Save::parse("[]").is_err());
        assert!(Save::parse("{\"profiles\":{}}").is_err());
        assert!(
            Save::parse("{\"v\":2}")
                .unwrap_err()
                .contains("newer version")
        );
        assert!(save.has_progress());
        assert!(!Save::new(ProfileBook::migrate(None), HallOfFame::default()).has_progress());
        assert_eq!(save.fame_records(), 2);
        assert!(text.len() < MAX_BYTES);
    }

    #[test]
    fn a_rebase_adds_both_sides_gains_and_spends() {
        let base = sample();
        let mut local = base.clone();
        let mut remote = base.clone();
        {
            let l = local.profiles.current_mut().unwrap();
            l.credit(30);
            l.races += 2;
            // Bought an ATV for 100.
            l.points -= 100;
            l.store.spent += 100;
            l.store.atvs.push(7);
        }
        {
            let r = remote.profiles.current_mut().unwrap();
            r.credit(50);
            r.races += 1;
            r.wins += 1;
            r.store.atvs.push(9);
        }
        let merged = merge(&local, &remote, Some(&base));
        let p = merged.profiles.current_profile().unwrap();
        assert_eq!(p.earned, 120 + 30 + 50);
        assert_eq!(p.points, 120 + 30 + 50 - 100);
        assert_eq!(p.store.spent, 100);
        assert_eq!(p.store.atvs, [4, 9, 7]);
        assert_eq!((p.races, p.wins), (6, 2));
        // Commutes (apart from list order).
        let other = merge(&remote, &local, Some(&base));
        let q = other.profiles.current_profile().unwrap();
        assert_eq!((q.points, q.earned, q.races, q.wins), (100, 200, 6, 2));
        // Nothing changed locally: the remote wins as it is.
        assert_eq!(merge(&base, &remote, Some(&base)), remote);
        assert_eq!(merge(&local, &base, Some(&base)), local);
    }

    #[test]
    fn a_double_spend_never_goes_negative() {
        let base = sample();
        let mut local = base.clone();
        let mut remote = base.clone();
        for (save, atv) in [(&mut local, 7), (&mut remote, 9)] {
            let p = save.profiles.current_mut().unwrap();
            p.points -= 100;
            p.store.spent += 100;
            p.store.atvs.push(atv);
        }
        let merged = merge(&local, &remote, Some(&base));
        let p = merged.profiles.current_profile().unwrap();
        assert_eq!(p.points, 0);
        assert_eq!(p.store.spent, 200);
        assert!(p.store.atvs.contains(&7) && p.store.atvs.contains(&9));
    }

    #[test]
    fn a_union_merge_is_idempotent_and_keeps_the_best() {
        let shared = sample();
        let mut mine = Save::new(ProfileBook::migrate(None), HallOfFame::default());
        {
            let p = mine.profiles.current_mut().unwrap();
            p.credit(500);
            p.races = 1;
            p.championships.progress.insert(
                "amsx-normal".into(),
                Progress {
                    round: 2,
                    points: [14, 0, 0, 0, 0],
                    trophy: Some(Trophy::Bronze),
                },
            );
            p.championships.objectives.insert("a:env05".into(), 0b0101);
            p.record_course(&crate::profile::RaceResult {
                course: "sx01".into(),
                quad: 5,
                place: 2,
                best_lap: Some(40.),
                total: None,
                full_length: true,
                stunts: crate::profile::StuntRecords {
                    longest_jump: 99.,
                    most_combos: 2,
                    most_points: 10,
                },
                personal: Some(crate::hall_of_fame::Personal {
                    laps: vec![40., 41.5],
                    ..Default::default()
                }),
            });
        }
        let mut theirs = shared.clone();
        {
            let p = theirs.profiles.current_mut().unwrap();
            p.championships.progress.insert(
                "amsx-normal".into(),
                Progress {
                    round: 0,
                    points: [0; 5],
                    trophy: Some(Trophy::Gold),
                },
            );
            p.championships.objectives.insert("a:env05".into(), 0b0010);
        }
        let once = merge(&mine, &theirs, None);
        let twice = merge(&once, &theirs, None);
        assert_eq!(once, twice, "merging the same save again changes nothing");
        let p = once.profiles.current_profile().unwrap();
        assert_eq!(p.points, 500);
        assert_eq!(p.earned, 500);
        assert_eq!(p.races, 3);
        let progress = &p.championships.progress["amsx-normal"];
        assert_eq!((progress.round, progress.trophy), (2, Some(Trophy::Gold)));
        assert_eq!(p.championships.objectives["a:env05"], 0b0111);
        let course = &p.courses["sx01"];
        assert_eq!(course.best_lap_ms, Some(40_000));
        assert_eq!(course.top_laps_ms, [40_000, 41_500, 42_000]);
        assert_eq!(course.best_total_ms, Some(212_250));
        assert_eq!(course.stunts.longest_jump, 99.);
        assert_eq!(p.store.atvs, [4]);
        // Hall of Fame: the shared holder survives.
        assert_eq!(once.hall_of_fame.courses["sx01"].laps[0].name, "ACE");
    }

    #[test]
    fn hall_of_fame_merges_keep_the_best_three_once_each() {
        let entry = |value: f32, name: &str| Entry {
            value,
            name: name.into(),
            vehicle: 1,
        };
        let mut a = HallOfFame::default();
        let mut b = HallOfFame::default();
        a.courses.insert(
            "sx01".into(),
            CourseFame {
                laps: vec![entry(40., "A"), entry(42., "A"), entry(44., "A")],
                longest_jump: Some(Entry {
                    value: 120.,
                    name: "A".into(),
                    vehicle: 2,
                }),
                ..Default::default()
            },
        );
        b.courses.insert(
            "sx01".into(),
            CourseFame {
                laps: vec![entry(40., "A"), entry(41., "B")],
                longest_jump: Some(Entry {
                    value: 90.,
                    name: "B".into(),
                    vehicle: 2,
                }),
                ..Default::default()
            },
        );
        b.courses.insert("env01".into(), CourseFame::default());
        let merged = merge_hall(&a, &b);
        let laps: Vec<(f32, &str)> = merged.courses["sx01"]
            .laps
            .iter()
            .map(|e| (e.value, e.name.as_str()))
            .collect();
        assert_eq!(laps, [(40., "A"), (41., "B"), (42., "A")]);
        assert_eq!(
            merged.courses["sx01"].longest_jump.as_ref().unwrap().value,
            120.
        );
        assert!(merged.courses.contains_key("env01"));
    }

    #[test]
    fn profiles_match_by_name_and_deletions_lose_to_progress() {
        let base = sample();
        // Local deletes PLAYER1 and creates ACE; remote adds points to
        // PLAYER1 and creates ZED.
        let mut local = base.clone();
        local.profiles.delete(0).unwrap();
        local.profiles.create("ACE").unwrap();
        let mut remote = base.clone();
        remote.profiles.current_mut().unwrap().credit(5);
        remote.profiles.create("ZED").unwrap();
        let merged = merge(&local, &remote, Some(&base));
        let names: Vec<&str> = merged
            .profiles
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["PLAYER1", "ZED", "ACE"]);
        assert_eq!(
            merged.profiles.current_profile().map(|p| p.name.as_str()),
            Some("ACE")
        );
        // An unchanged profile deleted locally stays deleted.
        let remote = base.clone();
        let merged = merge(&local, &remote, Some(&base));
        let names: Vec<&str> = merged
            .profiles
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["ACE"]);
        // Profiles created on both sides with one name add up from zero.
        let mut l = Save::default();
        let mut r = Save::default();
        for (s, pts) in [(&mut l, 10), (&mut r, 20)] {
            s.profiles.create("NEW").unwrap();
            s.profiles.current_mut().unwrap().credit(pts);
        }
        let merged = merge(&l, &r, Some(&Save::default()));
        assert_eq!(merged.profiles.profiles[0].points, 30);
        assert_eq!(merge(&l, &r, None).profiles.profiles[0].points, 20);
    }

    #[test]
    fn a_shared_profile_joins_under_a_free_name() {
        let mut book = ProfileBook::migrate(None);
        let shared = Profile {
            name: "PLAYER1".into(),
            points: 900,
            ..Default::default()
        };
        let i = add_profile(&mut book, &shared).unwrap();
        assert_eq!(book.profiles[i].name, "PLAYER12");
        assert_eq!(book.current, Some(i));
        assert_eq!(book.profiles[i].points, 900);
        add_profile(&mut book, &Profile::named("ACE")).unwrap();
        add_profile(&mut book, &Profile::named("ACE")).unwrap();
        assert_eq!(book.profiles[3].name, "ACE2");
        assert_eq!(
            add_profile(&mut book, &shared),
            Err(crate::profile::BookError::Full)
        );
    }

    /// Four completed profiles and a full Hall of Fame: far past 64 KiB as
    /// plain JSON, well under it deflated.
    fn complete_save() -> Save {
        let mut book = ProfileBook::default();
        for n in 0..4 {
            book.create(&format!("RIDER{n}")).unwrap();
            let p = book.current_mut().unwrap();
            p.credit(123_456);
            p.races = 999;
            p.wins = 500;
            for id in 1..=26u16 {
                p.atv_wins.insert(id, 12);
                p.store.atvs.push(id);
            }
            for slot in [
                "helmet", "shirt", "sleeve", "pant", "glove", "boot", "goggle",
            ] {
                p.store.gear.insert(slot.into(), (0..24).collect());
            }
            for k in 0..13 {
                p.store
                    .courses
                    .insert(format!("list{k}"), (0..10).collect());
            }
            for c in 0..60 {
                let mut r = CourseRecord {
                    races: 40,
                    wins: 20,
                    best_lap_ms: Some(41_234 + c),
                    best_total_ms: Some(212_345 + c),
                    top_laps_ms: vec![41_234 + c, 41_500, 42_000],
                    top_races_ms: vec![212_345 + c, 213_000, 214_000],
                    ..Default::default()
                };
                for id in 1..=5u16 {
                    r.atv_wins.insert(id, 4);
                }
                r.stunts = crate::profile::StuntRecords {
                    longest_jump: 123.456,
                    most_combos: 7,
                    most_points: 98_765,
                };
                p.courses.insert(format!("course{c:02}"), r);
            }
            for f in 0..20 {
                p.freestyle.insert(
                    format!("unspstunt:env{f:02}"),
                    FreestyleRecord {
                        longest_jump_cft: 12_345,
                        objectives: vec![123_456, 120_000, 110_000],
                        stunts: vec![98_765, 87_654, 76_543],
                        most_combos: 9,
                    },
                );
            }
        }
        let mut hall = HallOfFame::default();
        let e = |value: f32| Entry {
            value,
            name: "RIDER123".into(),
            vehicle: 26,
        };
        for c in 0..120 {
            hall.courses.insert(
                format!("course{c:02}:expert"),
                CourseFame {
                    laps: vec![e(41.234 + c as f32), e(41.5), e(42.)],
                    races: vec![e(212.345), e(213.), e(214.)],
                    longest_jump: Some(e(123.456)),
                    most_combos: None,
                    highest_stunt: None,
                },
            );
        }
        Save::new(book, hall)
    }

    #[test]
    fn a_complete_save_travels_deflated_under_the_cap() {
        let save = complete_save();
        assert!(save.to_json().len() > MAX_BYTES);
        let wire = save.to_wire();
        let text = wire.to_string();
        assert!(text.len() < MAX_BYTES / 2, "{} bytes", text.len());
        assert!(wire["z"].is_string());
        assert_eq!(Save::from_value(wire).unwrap(), save);
        // Small saves stay plain.
        let small = sample();
        assert_eq!(small.to_wire(), small.to_value());
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", b"\xff\x00\x10"] {
            assert_eq!(base64_decode(&base64_encode(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert!(Save::from_value(serde_json::json!({"v": 1, "z": "@@"})).is_err());
    }
}
