//! Online identity: rider tokens, friend codes, mutual friends, recent
//! riders, presence, online stats, the hidden rating and the weekly league
//! (research/ONLINE_IDENTITY.md). HTTP routes only, no packet kinds.
//!
//! Other server code reads the verified player id of a socket with
//! [`Players::pid`], a rider's hidden rating with [`Players::rating`] and
//! blocks with [`Players::blocks`].

pub mod codes;
pub mod league;
pub mod rating;
mod routes;

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use ring::hmac;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::{ConnId, Entropy};
pub use rating::Rating;
pub use routes::routes;

/// Riders kept at most (well under 1 KiB each typically, a few KiB at the
/// list caps): bounds memory and the state file on the 1 GB host. New
/// riders are refused past it.
pub const MAX_RIDERS: usize = 50_000;
pub const MAX_FRIENDS: usize = 100;
/// Pending requests kept per rider, each way; the oldest goes first.
pub const MAX_REQUESTS: usize = 50;
pub const MAX_BLOCKED: usize = 200;
/// Recent riders: the last 20 humans met within 7 days.
pub const RECENT: usize = 20;
pub const RECENT_MS: u64 = 7 * 86_400_000;
/// A rider who called the API this recently counts as online in the menus.
pub const ONLINE_MS: u64 = 90_000;
/// Best results kept per rider (one per mode and map).
const MAX_BEST: usize = 64;
/// When the file loads, riders with no friends and no races unseen this
/// long are dropped, and every rider unseen for a year.
const IDLE_DROP_MS: u64 = 30 * 86_400_000;
const IDLE_DROP_ANY_MS: u64 = 365 * 86_400_000;
/// Match lines read per pass, and the longest line accepted.
const TAIL_BYTES: u64 = 4 << 20;
const MAX_LINE: usize = 16 << 10;

pub const PLAYERS_FILE: &str = "players.json";
pub const KEY_FILE: &str = "players.key";
pub const MATCHES_FILE: &str = "matches.jsonl";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rider {
    pub code: String,
    pub name: String,
    pub created_ms: u64,
    pub seen_ms: u64,
    /// Appear offline.
    pub hidden: bool,
    pub friends: Vec<String>,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
    pub blocked: Vec<String>,
    pub recent: Vec<Met>,
    pub stats: Stats,
    pub rating: Rating,
    pub xp: u64,
    pub week: u32,
    pub points: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Met {
    pub pid: String,
    pub ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub races: u32,
    pub wins: u32,
    pub podiums: u32,
    pub best: Vec<Best>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Best {
    pub mode: u8,
    pub map: String,
    pub value: u64,
}

/// One line of `matches.jsonl` (research/MULTIPLAYER_EXPERIENCE.md "Match
/// records"), written by Quick Play.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchRecord {
    pub t: u64,
    pub room: String,
    pub playlist: String,
    pub mode: u8,
    pub map: String,
    pub verified: bool,
    pub riders: Vec<MatchRider>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchRider {
    pub pid: Option<String>,
    pub name: String,
    pub cpu: bool,
    pub place: u32,
    pub value: u64,
    pub dnf: bool,
    pub left: bool,
}

/// Lower values are better in timed races; King of the Hills (mode 4) and
/// the Stunts playlist count points or scores.
pub fn lower_is_better(playlist: &str, mode: u8) -> bool {
    playlist == "race" && mode != 4
}

/// The presence kind of a room code: Free Ride shards and private rooms can
/// be joined from a friend's row; Quick Play (`QP…`) joins come later.
pub fn room_kind(code: &atv_net::RoomCode) -> &'static str {
    if atv_net::is_public_room_code(code) {
        "freeride"
    } else if code.starts_with(b"QP") {
        "quickplay"
    } else {
        "private"
    }
}

/// A refusal: HTTP status and a short message.
pub type Refusal = (u16, &'static str);

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Saved {
    v: u32,
    matches_offset: u64,
    riders: HashMap<String, Rider>,
}

struct Socket {
    pid: String,
    room: atv_net::RoomCode,
}

struct State {
    key: hmac::Key,
    riders: HashMap<String, Rider>,
    /// Friend code → player id.
    codes: HashMap<String, String>,
    matches_offset: u64,
    sockets: HashMap<ConnId, Socket>,
    /// Player id → last API call (ms), for menu presence.
    menus: HashMap<String, u64>,
    dirty: bool,
}

pub struct Players {
    state: Mutex<State>,
}

impl Default for Players {
    fn default() -> Self {
        let mut key = [0u8; 32];
        crate::rooms::OsEntropy.fill(&mut key);
        Players::with_key(&key)
    }
}

/// Names others see: the shared-room filter, at most 16 characters.
fn public_name(name: &str) -> String {
    let name: String = name.trim().chars().take(16).collect();
    if name.is_empty() || atv_net::offensive_name(&name) {
        "Rider".into()
    } else {
        name
    }
}

fn remove(list: &mut Vec<String>, pid: &str) -> bool {
    let before = list.len();
    list.retain(|p| p != pid);
    list.len() != before
}

impl State {
    fn rider(&self, pid: &str) -> Result<&Rider, Refusal> {
        self.riders.get(pid).ok_or((401, "Register again"))
    }

    fn rider_mut(&mut self, pid: &str) -> Result<&mut Rider, Refusal> {
        self.dirty = true;
        self.riders.get_mut(pid).ok_or((401, "Register again"))
    }

    fn blocks(&self, a: &str, b: &str) -> bool {
        let has = |x: &str, y: &str| {
            self.riders
                .get(x)
                .is_some_and(|r| r.blocked.iter().any(|p| p == y))
        };
        has(a, b) || has(b, a)
    }

    fn new_code(&mut self, rng: &mut dyn Entropy) -> String {
        loop {
            let code = codes::friend_code(rng);
            if !self.codes.contains_key(&code) {
                return code;
            }
        }
    }

    /// Drops a pending request from `from` to `to`, both sides.
    fn unrequest(&mut self, from: &str, to: &str) {
        if let Some(r) = self.riders.get_mut(from) {
            remove(&mut r.outgoing, to);
        }
        if let Some(r) = self.riders.get_mut(to) {
            remove(&mut r.incoming, from);
        }
    }

    fn befriend(&mut self, a: &str, b: &str) {
        self.unrequest(a, b);
        self.unrequest(b, a);
        for (x, y) in [(a, b), (b, a)] {
            if let Some(r) = self.riders.get_mut(x)
                && !r.friends.iter().any(|p| p == y)
            {
                r.friends.push(y.to_string());
            }
        }
        self.dirty = true;
    }

    fn unfriend(&mut self, a: &str, b: &str) {
        for (x, y) in [(a, b), (b, a)] {
            if let Some(r) = self.riders.get_mut(x) {
                remove(&mut r.friends, y);
            }
        }
        self.dirty = true;
    }

    /// `me` asks `target` to be friends; returns the new state.
    fn request(&mut self, me: &str, target: &str) -> Result<&'static str, Refusal> {
        if me == target {
            return Err((400, "That's you"));
        }
        let mine = self.rider(me)?;
        if mine.blocked.iter().any(|p| p == target) {
            return Err((409, "Blocked"));
        }
        if mine.friends.iter().any(|p| p == target) {
            return Ok("friends");
        }
        let Some(theirs) = self.riders.get(target) else {
            return Err((404, "Unknown code"));
        };
        // A rider who blocked you never learns of the request, and you
        // never learn of the block.
        if theirs.blocked.iter().any(|p| p == me) {
            return Ok("sent");
        }
        if mine.incoming.iter().any(|p| p == target) {
            if mine.friends.len() >= MAX_FRIENDS || theirs.friends.len() >= MAX_FRIENDS {
                return Err((409, "Friends full"));
            }
            self.befriend(me, target);
            return Ok("friends");
        }
        if mine.friends.len() >= MAX_FRIENDS {
            return Err((409, "Friends full"));
        }
        if mine.outgoing.iter().any(|p| p == target) {
            return Ok("sent");
        }
        // Make room on both sides, dropping the oldest request.
        if mine.outgoing.len() >= MAX_REQUESTS {
            let oldest = mine.outgoing[0].clone();
            self.unrequest(me, &oldest);
        }
        if self.riders[target].incoming.len() >= MAX_REQUESTS {
            let oldest = self.riders[target].incoming[0].clone();
            self.unrequest(&oldest, target);
        }
        self.rider_mut(me)?.outgoing.push(target.to_string());
        self.rider_mut(target)?.incoming.push(me.to_string());
        Ok("sent")
    }

    /// Everyone `me` has a reference for.
    fn known(&self, me: &Rider) -> impl Iterator<Item = String> {
        me.friends
            .iter()
            .chain(&me.incoming)
            .chain(&me.outgoing)
            .chain(&me.blocked)
            .chain(me.recent.iter().map(|m| &m.pid))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn resolve(&self, me: &str, reference: &str) -> Result<String, Refusal> {
        let rider = self.rider(me)?;
        self.known(rider)
            .find(|pid| codes::reference(&self.key, me, pid) == reference)
            .ok_or((404, "Unknown rider"))
    }

    fn meet(&mut self, a: &str, b: &str, now: u64) {
        if a == b || self.blocks(a, b) {
            return;
        }
        for (x, y) in [(a, b), (b, a)] {
            if let Some(r) = self.riders.get_mut(x) {
                r.recent
                    .retain(|m| m.pid != y && now.saturating_sub(m.ms) < RECENT_MS);
                r.recent.insert(
                    0,
                    Met {
                        pid: y.to_string(),
                        ms: now,
                    },
                );
                r.recent.truncate(RECENT);
            }
        }
        self.dirty = true;
    }

    fn socket_of(&self, pid: &str) -> Option<&Socket> {
        self.sockets.values().find(|s| s.pid == pid)
    }

    fn online(&self, pid: &str, now: u64) -> bool {
        let Some(r) = self.riders.get(pid) else {
            return false;
        };
        !r.hidden
            && (self.socket_of(pid).is_some()
                || self
                    .menus
                    .get(pid)
                    .is_some_and(|&ms| now.saturating_sub(ms) < ONLINE_MS))
    }

    fn touch(&mut self, pid: &str, now: u64) {
        self.menus.insert(pid.to_string(), now);
        if let Some(r) = self.riders.get_mut(pid) {
            // The saved last-seen time moves at most once a minute.
            if now.saturating_sub(r.seen_ms) > 60_000 {
                r.seen_ms = now;
                self.dirty = true;
            }
        }
    }

    /// The rider's points for the current week (0 once the week turned).
    fn week_points(rider: &Rider, now: u64) -> u32 {
        if rider.week == league::week(now) {
            rider.points
        } else {
            0
        }
    }

    fn badge(rider: &Rider, now: u64) -> Value {
        let points = Self::week_points(rider, now);
        json!({
            "level": league::level(rider.xp),
            "tier": league::TIERS[league::tier(points)].0,
        })
    }
}

impl Players {
    pub fn with_key(key: &[u8]) -> Self {
        Players {
            state: Mutex::new(State {
                key: hmac::Key::new(hmac::HMAC_SHA256, key),
                riders: HashMap::new(),
                codes: HashMap::new(),
                matches_offset: 0,
                sockets: HashMap::new(),
                menus: HashMap::new(),
                dirty: false,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("players lock")
    }

    /// Registers (or signs in) the rider holding `secret`: a token for the
    /// API and sockets, and the rider's friend code.
    pub fn register(
        &self,
        secret: &str,
        name: &str,
        now: u64,
        rng: &mut dyn Entropy,
    ) -> Result<Value, Refusal> {
        if !codes::valid_secret(secret) {
            return Err((400, "Bad secret"));
        }
        let pid = codes::pid(secret);
        let mut s = self.lock();
        let created = !s.riders.contains_key(&pid);
        if created {
            if s.riders.len() >= MAX_RIDERS {
                return Err((503, "Online full"));
            }
            let code = s.new_code(rng);
            s.codes.insert(code.clone(), pid.clone());
            s.riders.insert(
                pid.clone(),
                Rider {
                    code,
                    created_ms: now,
                    ..Rider::default()
                },
            );
        }
        let name = public_name(name);
        let rider = s.rider_mut(&pid)?;
        rider.name = name;
        rider.seen_ms = now;
        let code = codes::format_code(&rider.code);
        s.touch(&pid, now);
        let token = codes::token(&s.key, &pid, now / 1000);
        Ok(json!({
            "token": token,
            "code": code,
            "created": created,
            "expires": now / 1000 + codes::TOKEN_TTL_S,
        }))
    }

    /// The registered rider a token belongs to.
    pub fn verify(&self, token: &str, now: u64) -> Option<String> {
        let s = self.lock();
        codes::verify(&s.key, token, now / 1000).filter(|pid| s.riders.contains_key(pid))
    }

    /// The verified player id of a room socket, if it presented a token.
    pub fn pid(&self, conn: ConnId) -> Option<String> {
        self.lock().sockets.get(&conn).map(|s| s.pid.clone())
    }

    /// A rider's hidden rating (new riders: the default, high uncertainty).
    pub fn rating(&self, pid: &str) -> Option<Rating> {
        self.lock().riders.get(pid).map(|r| r.rating)
    }

    /// True when either rider blocked the other.
    pub fn blocks(&self, a: &str, b: &str) -> bool {
        self.lock().blocks(a, b)
    }

    /// A verified socket entered `room`: presence, and everyone else
    /// verified in the room becomes a recent rider.
    pub fn connected(&self, conn: ConnId, pid: &str, room: atv_net::RoomCode, now: u64) {
        let mut s = self.lock();
        let others: Vec<String> = s
            .sockets
            .values()
            .filter(|o| o.room == room && o.pid != pid)
            .map(|o| o.pid.clone())
            .collect();
        for other in others {
            s.meet(pid, &other, now);
        }
        s.sockets.insert(
            conn,
            Socket {
                pid: pid.to_string(),
                room,
            },
        );
        s.touch(pid, now);
    }

    pub fn disconnected(&self, conn: ConnId, now: u64) {
        let mut s = self.lock();
        if let Some(socket) = s.sockets.remove(&conn) {
            s.touch(&socket.pid, now);
        }
    }

    /// The rider's own online card: code, level, league and stats.
    pub fn me(&self, pid: &str, now: u64) -> Result<Value, Refusal> {
        let mut s = self.lock();
        s.touch(pid, now);
        let r = s.rider(pid)?;
        let points = State::week_points(r, now);
        let tier = league::tier(points);
        let level = league::level(r.xp);
        let week = league::week(now);
        Ok(json!({
            "code": codes::format_code(&r.code),
            "name": r.name,
            "hidden": r.hidden,
            "level": level,
            "xp": r.xp,
            "levelXp": league::level_xp(level),
            "nextXp": league::level_xp(level + 1),
            "league": {
                "tier": league::TIERS[tier].0,
                "tierIndex": tier,
                "points": points,
                "next": league::next_tier_at(points),
                "endsMs": league::week_start_ms(week + 1),
            },
            "stats": {
                "races": r.stats.races,
                "wins": r.stats.wins,
                "podiums": r.stats.podiums,
                "best": r.stats.best,
            },
            "friends": r.friends.len(),
            "requests": r.incoming.len(),
        }))
    }

    /// Appear offline and the shown name.
    pub fn settings(
        &self,
        pid: &str,
        hidden: Option<bool>,
        name: Option<&str>,
    ) -> Result<(), Refusal> {
        let mut s = self.lock();
        let r = s.rider_mut(pid)?;
        if let Some(hidden) = hidden {
            r.hidden = hidden;
        }
        if let Some(name) = name {
            r.name = public_name(name);
        }
        Ok(())
    }

    /// A new friend code; the old one stops working. Friends stay.
    pub fn new_code(&self, pid: &str, rng: &mut dyn Entropy) -> Result<String, Refusal> {
        let mut s = self.lock();
        s.rider(pid)?;
        let code = s.new_code(rng);
        let old = std::mem::replace(&mut s.rider_mut(pid)?.code, code.clone());
        s.codes.remove(&old);
        s.codes.insert(code.clone(), pid.to_string());
        Ok(codes::format_code(&code))
    }

    /// Friends with presence, requests both ways, recent riders and blocks.
    pub fn friends(&self, pid: &str, now: u64) -> Result<Value, Refusal> {
        let mut s = self.lock();
        s.touch(pid, now);
        let me = s.rider(pid)?.clone();
        let reference = |other: &str| codes::reference(&s.key, pid, other);
        let name = |other: &str| s.riders.get(other).map_or("Rider", |r| r.name.as_str());
        let mut friends: Vec<(bool, String, Value)> = me
            .friends
            .iter()
            .filter_map(|f| {
                let r = s.riders.get(f)?;
                let online = s.online(f, now);
                let room = online.then(|| s.socket_of(f)).flatten().map(|socket| {
                    let kind = room_kind(&socket.room);
                    let code = String::from_utf8_lossy(&socket.room).into_owned();
                    let join = matches!(kind, "freeride" | "private");
                    json!({"kind": kind, "code": join.then_some(code)})
                });
                let mut row = State::badge(r, now);
                row["id"] = reference(f).into();
                row["name"] = r.name.clone().into();
                row["online"] = online.into();
                row["room"] = room.unwrap_or(Value::Null);
                Some((online, r.name.to_lowercase(), row))
            })
            .collect();
        friends.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let requests: Vec<Value> = me
            .incoming
            .iter()
            .rev()
            .map(|p| {
                let mut row = s.riders.get(p).map_or(json!({}), |r| State::badge(r, now));
                row["id"] = reference(p).into();
                row["name"] = name(p).into();
                row
            })
            .collect();
        let simple = |list: &[String]| -> Vec<Value> {
            list.iter()
                .rev()
                .map(|p| json!({"id": reference(p), "name": name(p)}))
                .collect()
        };
        let recent: Vec<Value> = me
            .recent
            .iter()
            .filter(|m| now.saturating_sub(m.ms) < RECENT_MS && s.riders.contains_key(&m.pid))
            .filter(|m| !me.blocked.contains(&m.pid))
            .map(|m| {
                let state = if me.friends.contains(&m.pid) {
                    "friend"
                } else if me.outgoing.contains(&m.pid) {
                    "sent"
                } else if me.incoming.contains(&m.pid) {
                    "asked"
                } else {
                    "add"
                };
                json!({
                    "id": reference(&m.pid),
                    "name": name(&m.pid),
                    "ago": now.saturating_sub(m.ms) / 1000,
                    "state": state,
                })
            })
            .collect();
        Ok(json!({
            "friends": friends.into_iter().map(|f| f.2).collect::<Vec<_>>(),
            "requests": requests,
            "sent": simple(&me.outgoing),
            "recent": recent,
            "blocked": simple(&me.blocked),
            "max": MAX_FRIENDS,
        }))
    }

    /// Adds by friend code or by a reference from this rider's lists.
    pub fn add(
        &self,
        pid: &str,
        code: Option<&str>,
        reference: Option<&str>,
    ) -> Result<&'static str, Refusal> {
        let mut s = self.lock();
        let target = match (code, reference) {
            (Some(code), _) => {
                let code = codes::parse_code(code).ok_or((400, "Bad code"))?;
                s.codes.get(&code).cloned().ok_or((404, "Unknown code"))?
            }
            (None, Some(reference)) => s.resolve(pid, reference)?,
            (None, None) => return Err((400, "Code needed")),
        };
        s.request(pid, &target)
    }

    /// Accept, decline, remove, block or unblock the rider behind a
    /// reference.
    pub fn act(&self, pid: &str, action: &str, reference: &str) -> Result<&'static str, Refusal> {
        let mut s = self.lock();
        let other = s.resolve(pid, reference)?;
        match action {
            "accept" => {
                if !s.rider(pid)?.incoming.contains(&other) {
                    return Err((404, "No request"));
                }
                s.request(pid, &other)
            }
            "decline" => {
                s.unrequest(&other, pid);
                s.dirty = true;
                Ok("declined")
            }
            "cancel" => {
                s.unrequest(pid, &other);
                s.dirty = true;
                Ok("cancelled")
            }
            "remove" => {
                s.unfriend(pid, &other);
                Ok("removed")
            }
            "block" => {
                s.unfriend(pid, &other);
                s.unrequest(pid, &other);
                s.unrequest(&other, pid);
                let me = s.rider_mut(pid)?;
                me.recent.retain(|m| m.pid != other);
                if !me.blocked.contains(&other) {
                    if me.blocked.len() >= MAX_BLOCKED {
                        me.blocked.remove(0);
                    }
                    me.blocked.push(other);
                }
                Ok("blocked")
            }
            "unblock" => {
                remove(&mut s.rider_mut(pid)?.blocked, &other);
                Ok("unblocked")
            }
            _ => Err((404, "Unknown action")),
        }
    }

    /// This week's top 20 and, with a rider, their place and percentile.
    pub fn league(&self, pid: Option<&str>, now: u64) -> Value {
        let s = self.lock();
        let week = league::week(now);
        let mut board: Vec<(&String, &Rider)> = s
            .riders
            .iter()
            .filter(|(_, r)| r.week == week && r.points > 0)
            .collect();
        board.sort_by(|a, b| {
            b.1.points
                .cmp(&a.1.points)
                .then_with(|| b.1.xp.cmp(&a.1.xp))
                .then_with(|| a.0.cmp(b.0))
        });
        let top: Vec<Value> = board
            .iter()
            .take(20)
            .enumerate()
            .map(|(i, (p, r))| {
                let mut row = State::badge(r, now);
                row["rank"] = (i + 1).into();
                row["name"] = r.name.clone().into();
                row["points"] = r.points.into();
                row["me"] = (Some(p.as_str()) == pid).into();
                row
            })
            .collect();
        let me = pid.and_then(|pid| {
            let rank = board.iter().position(|(p, _)| p.as_str() == pid)? + 1;
            // Top share, rounded up to a whole percent (1 = top 1%).
            let top = (rank * 100).div_ceil(board.len());
            Some(json!({"rank": rank, "top": top}))
        });
        json!({
            "week": week,
            "endsMs": league::week_start_ms(week + 1),
            "riders": board.len(),
            "top": top,
            "me": me,
        })
    }

    /// Counts one match record: stats, rating, league points and XP for
    /// every registered human rider; their co-riders become recent riders.
    /// Unverified records count nothing.
    pub fn ingest(&self, record: &MatchRecord) -> usize {
        if !record.verified || record.riders.is_empty() {
            return 0;
        }
        let mut s = self.lock();
        let riders = record.riders.len() as u32;
        let lower = lower_is_better(&record.playlist, record.mode);
        let week = league::week(record.t);
        // Humans with a registered id, once each.
        let mut humans: Vec<&MatchRider> = Vec::new();
        for r in &record.riders {
            if let Some(pid) = &r.pid
                && !r.cpu
                && s.riders.contains_key(pid)
                && !humans.iter().any(|h| h.pid == r.pid)
            {
                humans.push(r);
            }
        }
        for h in &humans {
            let pid = h.pid.as_deref().expect("human id");
            let rider = s.riders.get_mut(pid).expect("registered");
            let finished = !h.left && !h.dnf && h.place > 0;
            rider.stats.races += 1;
            if finished && h.place == 1 && riders > 1 {
                rider.stats.wins += 1;
            }
            if finished && h.place <= 3 && riders > 1 {
                rider.stats.podiums += 1;
            }
            if finished && h.value > 0 && !record.map.is_empty() {
                let map: String = record.map.chars().take(16).collect();
                let best = &mut rider.stats.best;
                match best
                    .iter()
                    .position(|b| b.mode == record.mode && b.map == map)
                {
                    Some(i) => {
                        let b = &mut best[i];
                        if (lower && h.value < b.value) || (!lower && h.value > b.value) {
                            b.value = h.value;
                        }
                    }
                    None if best.len() < MAX_BEST => best.push(Best {
                        mode: record.mode,
                        map,
                        value: h.value,
                    }),
                    None => {}
                }
            }
            let points = league::match_points(h.place, riders, h.left);
            if rider.week != week {
                // A record from an older week never reopens it.
                if week < rider.week {
                    rider.xp += u64::from(points);
                    continue;
                }
                rider.week = week;
                rider.points = 0;
            }
            rider.points = rider.points.saturating_add(points);
            rider.xp += u64::from(points);
        }
        // Rating: finishers by place, then riders who did not finish, then
        // leavers; CPUs never take part.
        if humans.len() >= 2 {
            let rank = |h: &MatchRider| {
                if h.left {
                    u32::MAX
                } else if h.dnf || h.place == 0 {
                    u32::MAX - 1
                } else {
                    h.place
                }
            };
            let ranks: Vec<u32> = humans.iter().map(|h| rank(h)).collect();
            let mut ratings: Vec<Rating> = humans
                .iter()
                .map(|h| s.riders[h.pid.as_deref().expect("human id")].rating)
                .collect();
            rating::rate(&mut ratings, &ranks);
            for (h, r) in humans.iter().zip(ratings) {
                if let Some(rider) = s.riders.get_mut(h.pid.as_deref().expect("human id")) {
                    rider.rating = r;
                }
            }
        }
        let ids: Vec<String> = humans.iter().filter_map(|h| h.pid.clone()).collect();
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                s.meet(a, b, record.t);
            }
        }
        s.dirty = true;
        humans.len()
    }

    /// Reads match lines appended to `path` since the last pass. A file
    /// that shrank (rotated) is read from its start.
    pub fn tail(&self, path: &Path) -> usize {
        let Ok(mut file) = std::fs::File::open(path) else {
            return 0;
        };
        let len = file.metadata().map_or(0, |m| m.len());
        let mut offset = self.lock().matches_offset;
        if len < offset {
            offset = 0;
        }
        if len == offset || file.seek(SeekFrom::Start(offset)).is_err() {
            return 0;
        }
        let mut bytes = Vec::new();
        if file.take(TAIL_BYTES).read_to_end(&mut bytes).is_err() {
            return 0;
        }
        // Only whole lines; a partial last line waits for the next pass.
        let Some(end) = bytes.iter().rposition(|&b| b == b'\n') else {
            if bytes.len() as u64 >= TAIL_BYTES {
                // One line longer than a pass: skip it.
                self.lock().matches_offset = offset + bytes.len() as u64;
            }
            return 0;
        };
        let mut counted = 0;
        for line in bytes[..end].split(|&b| b == b'\n') {
            if line.is_empty() || line.len() > MAX_LINE {
                continue;
            }
            match serde_json::from_slice::<MatchRecord>(line) {
                Ok(record) => counted += usize::from(self.ingest(&record) > 0),
                Err(e) => tracing::warn!(event = "match_unreadable", error = %e),
            }
        }
        let mut s = self.lock();
        s.matches_offset = offset + end as u64 + 1;
        s.dirty = true;
        counted
    }

    /// The state file when anything changed since the last save; menu
    /// presence older than it counts is dropped at the same time.
    pub fn snapshot(&self, now: u64) -> Option<Vec<u8>> {
        let mut s = self.lock();
        s.menus
            .retain(|_, &mut ms| now.saturating_sub(ms) < ONLINE_MS);
        if !s.dirty {
            return None;
        }
        s.dirty = false;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Out<'a> {
            v: u32,
            matches_offset: u64,
            riders: &'a HashMap<String, Rider>,
        }
        serde_json::to_vec(&Out {
            v: 1,
            matches_offset: s.matches_offset,
            riders: &s.riders,
        })
        .ok()
    }

    /// Loads a state file: riders, codes and the match offset. Long-idle
    /// riders without friends are dropped; lists are cut to their caps.
    pub fn load(&self, bytes: &[u8], now: u64) -> Result<usize, serde_json::Error> {
        let saved: Saved = serde_json::from_slice(bytes)?;
        let mut s = self.lock();
        s.matches_offset = saved.matches_offset;
        s.riders = saved
            .riders
            .into_iter()
            .filter(|(pid, r)| {
                codes::valid_pid(pid)
                    && codes::parse_code(&r.code).is_some()
                    && now.saturating_sub(r.seen_ms) < IDLE_DROP_ANY_MS
                    && (!r.friends.is_empty()
                        || r.stats.races > 0
                        || now.saturating_sub(r.seen_ms) < IDLE_DROP_MS)
            })
            .take(MAX_RIDERS)
            .collect();
        let mut codes = HashMap::new();
        let mut taken = Vec::new();
        for (pid, r) in &mut s.riders {
            r.friends.truncate(MAX_FRIENDS);
            r.incoming.truncate(MAX_REQUESTS);
            r.outgoing.truncate(MAX_REQUESTS);
            r.blocked.truncate(MAX_BLOCKED);
            r.recent.truncate(RECENT);
            r.stats.best.truncate(MAX_BEST);
            if codes.insert(r.code.clone(), pid.clone()).is_some() {
                taken.push(pid.clone());
            }
        }
        s.codes = codes;
        // Two riders with one code (a hand-edited file): fresh codes.
        for pid in taken {
            let code = s.new_code(&mut crate::rooms::OsEntropy);
            s.codes.insert(code.clone(), pid.clone());
            s.riders.get_mut(&pid).expect("rider").code = code;
        }
        // Drop links to riders that are gone.
        let ids: std::collections::HashSet<String> = s.riders.keys().cloned().collect();
        for r in s.riders.values_mut() {
            for list in [
                &mut r.friends,
                &mut r.incoming,
                &mut r.outgoing,
                &mut r.blocked,
            ] {
                list.retain(|p| ids.contains(p));
            }
            r.recent.retain(|m| ids.contains(&m.pid));
        }
        s.dirty = false;
        Ok(s.riders.len())
    }

    /// Uses the signing key in `dir`, writing a new one when there is none.
    pub fn load_key(&self, dir: &Path) -> std::io::Result<()> {
        let path = dir.join(KEY_FILE);
        let key = match std::fs::read_to_string(&path) {
            Ok(text) if text.trim().len() == 64 => (0..32)
                .map(|i| u8::from_str_radix(&text.trim()[i * 2..i * 2 + 2], 16))
                .collect::<Result<Vec<u8>, _>>()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            _ => {
                let mut key = [0u8; 32];
                crate::rooms::OsEntropy.fill(&mut key);
                crate::write_atomic(&path, codes::hex(&key).as_bytes())?;
                key.to_vec()
            }
        };
        self.lock().key = hmac::Key::new(hmac::HMAC_SHA256, &key);
        Ok(())
    }

    /// Riders registered, for metrics.
    pub fn count(&self) -> usize {
        self.lock().riders.len()
    }
}

#[cfg(test)]
mod tests;
