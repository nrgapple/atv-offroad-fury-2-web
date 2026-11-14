//! The Quick Play room loop (research/QUICK_PLAY.md): a pure state machine
//! the room runs with no player host. Lobby fill, a map vote, the round,
//! the podium, then the next vote.
//!
//! The server owns the loop, picks every round and checks results. The
//! round itself runs on the clients' existing room race and room game code
//! (research/MULTIPLAYER.md "Room races", "Room games", "Room Freestyle"):
//! one connected rider, the driver, simulates the round's CPUs and runs the
//! game rules that need course data the server does not have, exactly as a
//! private room's host does. The server forwards the driver's state, watches
//! every rider's progress, and decides the podium itself.
//!
//! No I/O, clock or randomness of its own: the room passes the time and a
//! seed, and sends what [`Loop::state`] returns.

use std::collections::BTreeMap;

use atv_net::quickplay::{
    self as qp, NO_CHOICE, Placing, QUICKPLAY_HUMANS, RANDOM, RoomLoop, Round, Seat, Setup,
    VOTE_OPTIONS, Vote, loop_phase, playlist, rematch, result_flag, seat_flag,
};
use atv_net::{
    CPU_PEER, GameClaim, GameState, NO_PEER, RACE_CPUS, RACE_DNF, RaceProgress, claim_kind,
    game_kind, game_phase, race_phase,
};
use serde::Serialize;

/// The longest lobby wait from the first rider (ms).
pub const LOBBY_MS: u64 = 20_000;
/// The vote (ms).
pub const VOTE_MS: u64 = 12_000;
/// The pick is shown and the map loads (ms).
pub const COUNTDOWN_MS: u64 = 4_000;
/// The podium (ms).
pub const RESULTS_MS: u64 = 8_000;
/// The podium holds at least this long, even when everyone chose Rematch.
pub const RESULTS_MIN_MS: u64 = 3_000;
/// A driver that has not started the round by then has failed it (ms):
/// the room votes again.
pub const START_MS: u64 = 45_000;
/// A race round ends by this long after it starts, whatever the driver says.
pub const RACE_MAX_MS: u64 = 12 * 60_000;
/// Race laps (authored: short rounds).
pub const LAPS: u8 = 2;
/// Games and Freestyle time limit (minutes; Games Prep's minimum).
pub const MINUTES: u8 = 2;
/// A race field fills to this many riders with CPUs (at most four CPUs).
pub const RACE_FIELD: u8 = 6;
/// Fastest plausible race speed along the racing line (ft/s): an ATV's top
/// speed is about 100 ft/s; jumps and slopes add little along the line.
pub const MAX_SPEED_FPS: f32 = 180.;
/// Distance allowance per speed check for resets and report jitter (ft).
const SPEED_SLACK_FT: f32 = 200.;
/// A finish time may differ from the server's clock since GO by this share
/// of the race (clock-slowing cheats run slow by more).
pub const DRIFT: f64 = 0.03;
/// Plus this much for report cadence and network latency (ms).
pub const DRIFT_SLACK_MS: f64 = 1_500.;

/// A connected rider, as the room sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Human {
    pub peer: u8,
    pub name: String,
    /// The socket's `pid` (unverified until slice D).
    pub pid: Option<String>,
    /// Can run the round as its driver (every rider today: only new
    /// builds reach a Quick Play room).
    pub can_drive: bool,
}

/// What the room does after a step.
#[derive(Debug, Default, PartialEq)]
pub struct Effects {
    /// Move the room to this map.
    pub map: Option<u16>,
    /// Make this rider the room's host (the driver).
    pub driver: Option<u8>,
    /// The loop changed: send it now.
    pub changed: bool,
}

/// One rider's watched race progress.
#[derive(Clone, Debug, Default)]
struct Watch {
    lap: u8,
    distance: f32,
    /// Speed check anchor: (server ms, distance).
    anchor: Option<(u64, f32)>,
    finish_ms: u32,
    finish_at: u64,
    rejected: bool,
    heard: bool,
}

/// The round being played.
#[derive(Clone, Debug, Default)]
struct Watched {
    /// Riders seated at the start (peer ids).
    racers: Vec<u8>,
    started_at: u64,
    /// The driver's game serial for this round, once seen.
    serial: Option<u32>,
    /// GO on the server clock.
    go_at: Option<u64>,
    progress: BTreeMap<u8, Watch>,
    /// Freestyle: each rider's last forwarded report (score).
    claims: BTreeMap<u8, f32>,
    /// The driver's final state.
    last: Option<GameState>,
    names: BTreeMap<u8, (String, Option<String>)>,
}

/// One rider in a match record.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct RecordRider {
    pub pid: Option<String>,
    pub name: String,
    pub cpu: bool,
    pub place: u32,
    pub value: u32,
    pub dnf: bool,
    pub left: bool,
}

/// A finished public match (`FURY_STATE_DIR/matches.jsonl`, the contract in
/// research/MULTIPLAYER_EXPERIENCE.md "Match records").
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Record {
    pub t: u64,
    pub room: String,
    pub playlist: &'static str,
    pub mode: u8,
    pub map: &'static str,
    pub verified: bool,
    pub riders: Vec<RecordRider>,
}

/// The room's loop.
#[derive(Clone, Debug)]
pub struct Loop {
    playlist: u8,
    phase: u8,
    /// When the phase ends (0: on an event).
    deadline: u64,
    phase_at: u64,
    first_at: Option<u64>,
    options: Vec<Round>,
    votes: BTreeMap<u8, u8>,
    pick: u8,
    setup: Setup,
    last_round: Option<Round>,
    driver: u8,
    ready: BTreeMap<u8, bool>,
    rematch: BTreeMap<u8, u8>,
    pings: BTreeMap<u8, u16>,
    watched: Watched,
    results: Vec<Placing>,
    verified: bool,
    records: Vec<Record>,
    random: u64,
}

impl Loop {
    pub fn new(playlist: u8, seed: u64) -> Self {
        Loop {
            playlist,
            phase: loop_phase::LOBBY,
            deadline: 0,
            phase_at: 0,
            first_at: None,
            options: Vec::new(),
            votes: BTreeMap::new(),
            pick: NO_CHOICE,
            setup: Setup::default(),
            last_round: None,
            driver: NO_PEER,
            ready: BTreeMap::new(),
            rematch: BTreeMap::new(),
            pings: BTreeMap::new(),
            watched: Watched::default(),
            results: Vec::new(),
            verified: false,
            records: Vec::new(),
            random: seed | 1,
        }
    }

    pub fn playlist(&self) -> u8 {
        self.playlist
    }
    pub fn phase(&self) -> u8 {
        self.phase
    }
    pub fn driver(&self) -> u8 {
        self.driver
    }
    /// The round being played, from the countdown on.
    pub fn setup(&self) -> Option<Setup> {
        (self.phase >= loop_phase::COUNTDOWN && self.setup.number > 0).then_some(self.setup)
    }
    /// Finished matches since the last call.
    pub fn take_records(&mut self) -> Vec<Record> {
        std::mem::take(&mut self.records)
    }
    /// A newcomer can take a seat for the next round without waiting long.
    pub fn joinable(&self) -> bool {
        self.phase <= loop_phase::VOTE || self.phase == loop_phase::RESULTS
    }

    fn next_random(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.random;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.random = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_random() % n.max(1) as u64) as usize
    }

    fn enter(&mut self, phase: u8, now: u64, length: u64) {
        self.phase = phase;
        self.phase_at = now;
        self.deadline = if length == 0 { 0 } else { now + length };
    }

    /// The map to wait on before the first round: any of the playlist's.
    pub fn first_map(&mut self) -> u16 {
        let pool = qp::pool(self.playlist);
        let i = self.below(pool.len());
        pool.get(i).map_or(1, |r| r.map)
    }

    /// Three different cards, not the round just played when the pool
    /// allows: one of each of the playlist's modes first, so every vote
    /// offers a race and a game (or Freestyle and Treasure Hunt).
    fn deal(&mut self) {
        let mut pool = qp::pool(self.playlist);
        if let Some(last) = self.last_round {
            pool.retain(|r| *r != last);
        }
        self.options.clear();
        let mut modes: Vec<u8> = Vec::new();
        for r in &pool {
            if !modes.contains(&r.mode) {
                modes.push(r.mode);
            }
        }
        for mode in modes {
            let of: Vec<usize> = (0..pool.len()).filter(|i| pool[*i].mode == mode).collect();
            if self.options.len() < VOTE_OPTIONS && !of.is_empty() {
                let i = of[self.below(of.len())];
                self.options.push(pool.swap_remove(i));
            }
        }
        while self.options.len() < VOTE_OPTIONS && !pool.is_empty() {
            let i = self.below(pool.len());
            self.options.push(pool.swap_remove(i));
        }
    }

    /// The vote-weighted roulette: each vote is a ticket; no votes, every
    /// card one ticket. Random draws from the whole pool.
    fn spin(&mut self) -> (u8, Round) {
        let mut tickets = [0u32; VOTE_OPTIONS + 1];
        for choice in self.votes.values() {
            if usize::from(*choice) < tickets.len()
                && (*choice == RANDOM || usize::from(*choice) < self.options.len())
            {
                tickets[usize::from(*choice)] += 1;
            }
        }
        if tickets.iter().all(|t| *t == 0) {
            for (i, t) in tickets.iter_mut().enumerate() {
                *t = u32::from(i == usize::from(RANDOM) || i < self.options.len());
            }
        }
        let total: u32 = tickets.iter().sum();
        let mut ticket = self.below(total as usize) as u32;
        let mut pick = RANDOM;
        for (i, t) in tickets.iter().enumerate() {
            if ticket < *t {
                pick = i as u8;
                break;
            }
            ticket -= t;
        }
        let round = if pick == RANDOM {
            let pool = qp::pool(self.playlist);
            let i = self.below(pool.len());
            pool[i]
        } else {
            self.options[usize::from(pick)]
        };
        (pick, round)
    }

    fn votes(&self) -> [u8; VOTE_OPTIONS + 1] {
        let mut out = [0u8; VOTE_OPTIONS + 1];
        for choice in self.votes.values() {
            if let Some(n) = out.get_mut(usize::from(*choice)) {
                *n = n.saturating_add(1);
            }
        }
        out
    }

    /// A rider's vote packet. Returns true when the loop changed.
    pub fn vote(&mut self, peer: u8, vote: Vote) -> bool {
        let mut changed = false;
        if vote.ping_ms > 0 && self.pings.insert(peer, vote.ping_ms) != Some(vote.ping_ms) {
            changed = true;
        }
        match self.phase {
            loop_phase::LOBBY => {
                changed |= self.ready.insert(peer, vote.ready) != Some(vote.ready);
            }
            loop_phase::VOTE if vote.round == self.setup.number + 1 => {
                let valid = vote.choice == RANDOM || usize::from(vote.choice) < self.options.len();
                if valid {
                    changed |= self.votes.insert(peer, vote.choice) != Some(vote.choice);
                } else if vote.choice == NO_CHOICE {
                    changed |= self.votes.remove(&peer).is_some();
                }
            }
            loop_phase::RESULTS if vote.round == self.setup.number => {
                if vote.rematch != rematch::UNDECIDED {
                    changed |= self.rematch.insert(peer, vote.rematch) != Some(vote.rematch);
                }
            }
            _ => {}
        }
        changed
    }

    /// The driver's state for the round (the room forwards it as a host's).
    pub fn game(&mut self, state: &GameState, now: u64) {
        if self.phase != loop_phase::PLAY || state.game != self.setup.round.mode {
            return;
        }
        // The driver's first state names the round's serial; a new
        // driver's handoff keeps it.
        let serial = *self.watched.serial.get_or_insert(state.serial);
        if state.serial != serial {
            return;
        }
        if state.game == game_kind::RACE {
            match state.phase {
                race_phase::COUNTDOWN => {
                    self.watched.go_at = Some(now + (f64::from(state.timer) * 1000.) as u64);
                }
                race_phase::RACING if self.watched.go_at.is_none() => {
                    self.watched.go_at = Some(now);
                }
                _ => {}
            }
        } else if state.phase == game_phase::PLAY && self.watched.go_at.is_none() {
            self.watched.go_at = Some(now);
        }
        self.watched.last = Some(state.clone());
    }

    /// A rider's forwarded race progress: the plausibility checks.
    pub fn progress(&mut self, peer: u8, p: &RaceProgress, now: u64) {
        if self.phase != loop_phase::PLAY
            || self.setup.round.mode != game_kind::RACE
            || Some(p.serial) != self.watched.serial
            || !self.watched.racers.contains(&peer)
        {
            return;
        }
        let go_at = self.watched.go_at;
        let w = self.watched.progress.entry(peer).or_default();
        w.heard = true;
        if w.rejected {
            return;
        }
        // Laps never go back, and a finish time once reported stands.
        if p.lap < w.lap || (w.finish_ms > 0 && p.finish_ms != w.finish_ms) {
            w.rejected = true;
            return;
        }
        w.lap = p.lap;
        w.distance = p.distance;
        match w.anchor {
            None => w.anchor = Some((now, p.distance)),
            Some((t, d)) if now >= t + 1000 => {
                let limit = MAX_SPEED_FPS * (now - t) as f32 / 1000. + SPEED_SLACK_FT;
                if p.distance - d > limit {
                    w.rejected = true;
                    return;
                }
                w.anchor = Some((now, p.distance));
            }
            Some(_) => {}
        }
        if w.finish_ms == 0 && p.finish_ms > 0 {
            w.finish_ms = p.finish_ms;
            w.finish_at = now;
            // A finish faster than the line allows.
            let floor = f64::from(p.distance.max(0.) / MAX_SPEED_FPS) * 1000.;
            let mut bad = f64::from(p.finish_ms) < floor;
            // The server's clock since GO: a time that runs slow or ahead
            // of it by more than the allowance is refused.
            if let Some(go) = go_at {
                let elapsed = now.saturating_sub(go) as f64;
                let finish = f64::from(p.finish_ms);
                bad |= elapsed > finish * (1. + DRIFT) + DRIFT_SLACK_MS
                    || finish > elapsed * (1. + DRIFT) + DRIFT_SLACK_MS;
            }
            w.rejected = bad;
        }
    }

    /// A rider's forwarded Freestyle report.
    pub fn claim(&mut self, peer: u8, claim: &GameClaim) {
        if self.phase == loop_phase::PLAY
            && self.setup.round.mode == game_kind::FREESTYLE
            && claim.kind == claim_kind::FREESTYLE
            && Some(claim.serial) == self.watched.serial
        {
            let best = self.watched.claims.entry(peer).or_insert(0.);
            *best = best.max(claim.stunt);
        }
    }

    /// Advances the loop for the connected `humans` at `now`.
    pub fn step(&mut self, now: u64, humans: &[Human], room: &str) -> Effects {
        let mut fx = Effects::default();
        let before = self.phase;
        // Forget riders who left.
        let here = |peer: &u8| humans.iter().any(|h| h.peer == *peer);
        self.votes.retain(|p, _| here(p));
        self.ready.retain(|p, _| here(p));
        self.rematch.retain(|p, _| here(p));
        self.pings.retain(|p, _| here(p));
        for h in humans {
            self.watched
                .names
                .entry(h.peer)
                .or_insert_with(|| (h.name.clone(), h.pid.clone()));
        }
        if humans.is_empty() && self.phase != loop_phase::LOBBY {
            // Everyone went: wait for the next rider.
            self.first_at = None;
            self.enter(loop_phase::LOBBY, now, 0);
            self.watched = Watched::default();
            self.results.clear();
        }
        match self.phase {
            loop_phase::LOBBY => {
                if let Some(first) = self.first_at.filter(|_| !humans.is_empty()) {
                    let full = humans.len() >= QUICKPLAY_HUMANS;
                    let ready = humans
                        .iter()
                        .all(|h| self.ready.get(&h.peer).copied().unwrap_or(false));
                    if full || ready || now >= first + LOBBY_MS {
                        self.start_vote(now);
                    }
                } else if !humans.is_empty() {
                    self.first_at = Some(now);
                    self.deadline = now + LOBBY_MS;
                    fx.changed = true;
                } else {
                    self.deadline = 0;
                }
            }
            loop_phase::VOTE => {
                let all = humans.iter().all(|h| self.votes.contains_key(&h.peer));
                if all || now >= self.deadline {
                    let (pick, round) = self.spin();
                    self.pick = pick;
                    self.last_round = Some(round);
                    let seed = self.next_random() as u32;
                    self.setup = Setup {
                        round,
                        number: self.setup.number + 1,
                        laps: if round.mode == game_kind::RACE {
                            LAPS
                        } else {
                            0
                        },
                        minutes: if round.mode == game_kind::RACE {
                            0
                        } else {
                            MINUTES
                        },
                        cpu_field: if round.mode == game_kind::RACE {
                            RACE_FIELD
                        } else {
                            0
                        },
                        difficulty: if round.mode == game_kind::RACE { 1 } else { 0 },
                        seed: seed.max(1),
                    };
                    fx.map = Some(round.map);
                    self.enter(loop_phase::COUNTDOWN, now, COUNTDOWN_MS);
                }
            }
            loop_phase::COUNTDOWN => {
                if now >= self.deadline {
                    self.watched = Watched {
                        racers: humans.iter().map(|h| h.peer).collect(),
                        started_at: now,
                        names: std::mem::take(&mut self.watched.names),
                        ..Default::default()
                    };
                    self.results.clear();
                    self.enter(loop_phase::PLAY, now, 0);
                }
            }
            loop_phase::PLAY => {
                let over = self.watched.last.as_ref().is_some_and(|s| {
                    (s.game == game_kind::RACE && s.phase == race_phase::RESULTS)
                        || (s.game != game_kind::RACE && s.phase == game_phase::OVER)
                });
                let limit = if self.setup.round.mode == game_kind::RACE {
                    RACE_MAX_MS
                } else {
                    u64::from(self.setup.minutes) * 60_000 + 90_000
                };
                let stalled =
                    self.watched.serial.is_none() && now >= self.watched.started_at + START_MS;
                let racing = self.watched.racers.iter().any(|p| here(p));
                if over || stalled || !racing || now >= self.watched.started_at + limit {
                    self.finish(now, humans, room, !stalled && racing);
                    self.rematch.clear();
                    self.enter(loop_phase::RESULTS, now, RESULTS_MS);
                }
            }
            loop_phase::RESULTS => {
                let stay = humans.iter().all(|h| {
                    self.rematch.get(&h.peer).copied() == Some(rematch::STAY)
                        || self.rematch.get(&h.peer).copied() == Some(rematch::LEAVE)
                });
                if now >= self.deadline || (stay && now >= self.phase_at + RESULTS_MIN_MS) {
                    self.start_vote(now);
                }
            }
            _ => {}
        }
        // The driver: the current one while connected, else the lowest
        // connected rider that can drive.
        let driver_here = humans.iter().any(|h| h.peer == self.driver && h.can_drive);
        if !driver_here {
            let next = humans
                .iter()
                .filter(|h| h.can_drive)
                .map(|h| h.peer)
                .min()
                .unwrap_or(NO_PEER);
            if next != self.driver {
                self.driver = next;
                fx.changed = true;
                if next != NO_PEER {
                    fx.driver = Some(next);
                }
            }
        }
        fx.changed |= self.phase != before;
        fx
    }

    fn start_vote(&mut self, now: u64) {
        self.deal();
        self.votes.clear();
        self.pick = NO_CHOICE;
        self.ready.clear();
        self.enter(loop_phase::VOTE, now, VOTE_MS);
    }

    /// The round ended: the podium and the match record.
    fn finish(&mut self, now: u64, humans: &[Human], room: &str, played: bool) {
        let here = |peer: u8| humans.iter().any(|h| h.peer == peer);
        let w = std::mem::take(&mut self.watched);
        let mut rows: Vec<(Placing, (u32, i64))> = Vec::new();
        let last = w.last.clone().unwrap_or_default();
        let mode = self.setup.round.mode;
        let mut verified = played && w.go_at.is_some();
        if mode == game_kind::RACE {
            for &peer in &w.racers {
                let p = w.progress.get(&peer).cloned().unwrap_or_default();
                let left = !here(peer);
                verified &= p.heard || left;
                let mut flags = 0;
                if left {
                    flags |= result_flag::LEFT;
                }
                if p.rejected {
                    flags |= result_flag::REJECTED;
                }
                let finished = p.finish_ms > 0 && !p.rejected;
                if !finished {
                    flags |= result_flag::DNF;
                }
                let key = if finished {
                    (0, i64::from(p.finish_ms))
                } else if p.rejected {
                    (2, 0)
                } else {
                    (
                        1,
                        -(i64::from(p.lap) * 10_000_000 + (p.distance.max(0.) * 10.) as i64),
                    )
                };
                let value = if finished { p.finish_ms } else { 0 };
                rows.push((Placing { peer, flags, value }, key));
            }
            // CPUs: the driver simulated them; its frozen results stand.
            let cpus = usize::from(last.race.cpus).min(RACE_CPUS);
            for (i, &score) in last.race.cpu_scores.iter().take(cpus).enumerate() {
                let finished = score > 0 && score < RACE_DNF;
                let key = if finished {
                    (0, i64::from(score))
                } else {
                    (
                        1,
                        i64::from(score.saturating_sub(RACE_DNF)) - 1_000_000_000_000,
                    )
                };
                rows.push((
                    Placing {
                        peer: CPU_PEER + i as u8,
                        flags: result_flag::CPU | if finished { 0 } else { result_flag::DNF },
                        value: if finished { score } else { 0 },
                    },
                    key,
                ));
            }
        } else {
            let freestyle = mode == game_kind::FREESTYLE;
            // King of the Hills and Treasure Hunt run on the driver's
            // course data: the server cannot check them.
            verified &= freestyle;
            for &peer in &w.racers {
                let slot = usize::from(peer);
                let playing = last.marks.players & (1 << peer) != 0;
                let left = !here(peer);
                let mut value = if freestyle {
                    last.marks.stunts.get(slot).copied().unwrap_or(0.)
                } else {
                    last.scores.get(slot).copied().unwrap_or(0) as f32
                };
                if freestyle {
                    // The driver cannot score a rider above its own reports.
                    let claimed = w.claims.get(&peer).copied().unwrap_or(0.);
                    if value > claimed + 0.5 {
                        verified = false;
                        value = claimed;
                    }
                }
                let tie = last.marks.stunts.get(slot).copied().unwrap_or(0.);
                let mut flags = 0;
                if left {
                    flags |= result_flag::LEFT;
                }
                if !playing {
                    flags |= result_flag::DNF;
                }
                let value = value.max(0.).round() as u32;
                rows.push((
                    Placing { peer, flags, value },
                    (
                        u32::from(!playing),
                        -(i64::from(value) * 100_000 + (tie.max(0.) as i64).min(99_999)),
                    ),
                ));
            }
        }
        rows.sort_by_key(|(p, key)| (*key, p.peer));
        self.results = rows.into_iter().map(|(p, _)| p).collect();
        self.verified = verified;
        if !played || self.results.is_empty() {
            return;
        }
        let riders = self
            .results
            .iter()
            .enumerate()
            .map(|(place, p)| {
                let cpu = p.flags & result_flag::CPU != 0;
                let (name, pid) = if cpu {
                    (format!("CPU {}", p.peer - CPU_PEER + 1), None)
                } else {
                    w.names
                        .get(&p.peer)
                        .cloned()
                        .unwrap_or_else(|| (format!("Rider {}", p.peer + 1), None))
                };
                RecordRider {
                    pid,
                    name,
                    cpu,
                    place: place as u32 + 1,
                    value: p.value,
                    dnf: p.flags & result_flag::DNF != 0,
                    left: p.flags & result_flag::LEFT != 0,
                }
            })
            .collect();
        self.records.push(Record {
            t: now,
            room: room.to_string(),
            playlist: playlist::name(self.playlist).unwrap_or("race"),
            mode,
            map: self.setup.round.environment().unwrap_or(""),
            verified,
            riders,
        });
    }

    /// The loop as sent to riders.
    pub fn state(&self, humans: &[Human]) -> RoomLoop {
        let mut seats: Vec<Seat> = humans
            .iter()
            .map(|h| {
                let mut flags = 0;
                if self.votes.contains_key(&h.peer) {
                    flags |= seat_flag::VOTED;
                }
                if self.ready.get(&h.peer).copied().unwrap_or(false) {
                    flags |= seat_flag::READY;
                }
                if self.phase == loop_phase::PLAY && !self.watched.racers.contains(&h.peer) {
                    flags |= seat_flag::SPECTATING;
                }
                if self.rematch.get(&h.peer).copied() == Some(rematch::LEAVE) {
                    flags |= seat_flag::LEAVING;
                }
                if h.peer == self.driver {
                    flags |= seat_flag::DRIVER;
                }
                Seat {
                    peer: h.peer,
                    flags,
                    ping_ms: self.pings.get(&h.peer).copied().unwrap_or(0),
                }
            })
            .collect();
        seats.truncate(QUICKPLAY_HUMANS);
        seats.sort_by_key(|s| s.peer);
        // CPU seats: a race field's fill (the coming round's while waiting).
        let racing = match self.phase {
            loop_phase::LOBBY | loop_phase::VOTE => self.playlist == playlist::RACE,
            _ => self.setup.round.mode == game_kind::RACE,
        };
        if racing {
            let humans = if self.phase == loop_phase::PLAY {
                self.watched.racers.len()
            } else {
                seats.len()
            };
            let cpus = usize::from(RACE_FIELD)
                .saturating_sub(humans.max(1))
                .min(RACE_CPUS);
            for i in 0..cpus {
                seats.push(Seat {
                    peer: CPU_PEER + i as u8,
                    flags: seat_flag::CPU,
                    ping_ms: 0,
                });
            }
        }
        RoomLoop {
            phase: self.phase,
            playlist: self.playlist,
            deadline_ms: self.deadline,
            options: if self.phase == loop_phase::VOTE || self.phase == loop_phase::COUNTDOWN {
                self.options.clone()
            } else {
                Vec::new()
            },
            votes: self.votes(),
            pick: if self.phase == loop_phase::COUNTDOWN {
                self.pick
            } else {
                NO_CHOICE
            },
            setup: if self.setup.number > 0 {
                self.setup
            } else {
                Setup::default()
            },
            driver: self.driver,
            seats,
            results: if self.phase == loop_phase::RESULTS {
                self.results.clone()
            } else {
                Vec::new()
            },
            verified: self.phase == loop_phase::RESULTS && self.verified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn human(peer: u8) -> Human {
        Human {
            peer,
            name: format!("R{peer}"),
            pid: Some(format!("pid{peer}")),
            can_drive: true,
        }
    }

    fn vote(round: u32, choice: u8) -> Vote {
        Vote {
            round,
            choice,
            rematch: rematch::UNDECIDED,
            ready: false,
            ping_ms: 0,
        }
    }

    fn race_state(serial: u32, phase: u8, timer: f32) -> GameState {
        GameState {
            game: game_kind::RACE,
            serial,
            phase,
            timer,
            ..Default::default()
        }
    }

    /// Runs a race loop to PLAY with `humans` and returns the loop and the
    /// time.
    fn to_play(humans: &[Human]) -> (Loop, u64) {
        let mut l = Loop::new(playlist::RACE, 7);
        let mut now = 1_000;
        l.step(now, humans, "QPR00001");
        now += LOBBY_MS;
        l.step(now, humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
        // Everyone votes card 0: the roulette lands on it.
        for h in humans {
            l.vote(h.peer, vote(1, 0));
        }
        let fx = l.step(now, humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::COUNTDOWN);
        assert_eq!(l.pick, 0);
        assert_eq!(fx.map, Some(l.options[0].map));
        // Force a race round for the race tests.
        l.setup.round = Round {
            mode: game_kind::RACE,
            map: 15,
            variant: 0,
        };
        now += COUNTDOWN_MS;
        l.step(now, humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::PLAY);
        (l, now)
    }

    fn progress(serial: u32, lap: u8, distance: f32, finish_ms: u32) -> RaceProgress {
        RaceProgress {
            peer: 0,
            serial,
            lap,
            distance,
            finish_ms,
        }
    }

    #[test]
    fn a_lone_rider_waits_at_most_the_lobby_time_and_full_rooms_start_at_once() {
        let mut l = Loop::new(playlist::RACE, 1);
        let one = [human(0)];
        let fx = l.step(1_000, &one, "QPR00001");
        assert!(fx.changed);
        assert_eq!(fx.driver, Some(0));
        assert_eq!(l.state(&one).deadline_ms, 1_000 + LOBBY_MS);
        l.step(1_000 + LOBBY_MS - 1, &one, "QPR00001");
        assert_eq!(l.phase(), loop_phase::LOBBY);
        l.step(1_000 + LOBBY_MS, &one, "QPR00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
        // Four riders start without waiting.
        let mut l = Loop::new(playlist::RACE, 1);
        let four: Vec<Human> = (0..4).map(human).collect();
        l.step(1_000, &four, "QPR00001");
        l.step(1_050, &four, "QPR00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
        // A lone rider's Ready skips the wait.
        let mut l = Loop::new(playlist::STUNTS, 1);
        l.step(1_000, &one, "QPS00001");
        l.vote(
            0,
            Vote {
                ready: true,
                ..vote(0, NO_CHOICE)
            },
        );
        l.step(1_100, &one, "QPS00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
    }

    #[test]
    fn the_vote_deals_three_cards_ends_early_and_counts_live() {
        let mut l = Loop::new(playlist::RACE, 3);
        let two = [human(0), human(1)];
        l.step(0, &two, "QPR00001");
        l.step(LOBBY_MS, &two, "QPR00001");
        let state = l.state(&two);
        assert_eq!(state.phase, loop_phase::VOTE);
        assert_eq!(state.options.len(), 3);
        assert_ne!(state.options[0], state.options[1]);
        // A race and King of the Hills on every ballot.
        assert!(state.options.iter().any(|o| o.mode == game_kind::RACE));
        assert!(
            state
                .options
                .iter()
                .any(|o| o.mode == game_kind::KING_OF_THE_HILLS)
        );
        assert_eq!(state.deadline_ms, LOBBY_MS + VOTE_MS);
        // CPU seats fill the race field to six.
        assert_eq!(state.seats.len(), 6);
        assert_eq!(
            state
                .seats
                .iter()
                .filter(|s| s.flags & seat_flag::CPU != 0)
                .count(),
            4
        );
        // A vote for another round, or a card that does not exist, counts
        // for nothing.
        assert!(!l.vote(0, vote(5, 1)));
        assert!(!l.vote(0, vote(1, 7)));
        assert!(l.vote(0, vote(1, 2)));
        assert_eq!(l.state(&two).votes, [0, 0, 1, 0]);
        assert!(l.state(&two).seats[0].flags & seat_flag::VOTED != 0);
        l.step(LOBBY_MS + 10, &two, "QPR00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
        l.vote(1, vote(1, RANDOM));
        l.step(LOBBY_MS + 20, &two, "QPR00001");
        assert_eq!(l.phase(), loop_phase::COUNTDOWN);
        let state = l.state(&two);
        assert!(state.pick == 2 || state.pick == RANDOM);
        assert_eq!(state.setup.number, 1);
        assert_eq!(
            state.setup.laps,
            if state.setup.round.mode == 3 { 2 } else { 0 }
        );
    }

    #[test]
    fn the_roulette_follows_the_votes_and_random_is_uniform_over_the_pool() {
        let mut counts = [0u32; 4];
        let mut maps = std::collections::BTreeSet::new();
        for seed in 0..2_000 {
            let mut l = Loop::new(playlist::STUNTS, seed);
            l.deal();
            l.votes.insert(0, 0);
            l.votes.insert(1, 0);
            l.votes.insert(2, 0);
            l.votes.insert(3, RANDOM);
            let (pick, round) = l.spin();
            counts[usize::from(pick)] += 1;
            if pick == RANDOM {
                maps.insert((round.mode, round.map));
            }
        }
        // Three tickets of four on card 0, one on Random.
        assert!((1_350..1_650).contains(&counts[0]), "{counts:?}");
        assert_eq!(counts[1] + counts[2], 0);
        // Random reached most of the 28 Stunts rounds.
        assert!(maps.len() >= 25, "{}", maps.len());
        // No votes: every card one ticket.
        let mut counts = [0u32; 4];
        for seed in 0..2_000 {
            let mut l = Loop::new(playlist::RACE, seed);
            l.deal();
            counts[usize::from(l.spin().0)] += 1;
        }
        assert!(counts.iter().all(|c| (380..620).contains(c)), "{counts:?}");
    }

    #[test]
    fn a_checked_race_ranks_by_server_checked_times_and_writes_a_record() {
        let humans = [human(0), human(1), human(2)];
        let (mut l, mut now) = to_play(&humans);
        // The driver's countdown: GO in 4 s.
        l.game(&race_state(9, race_phase::GRID, 4.), now);
        l.game(&race_state(9, race_phase::COUNTDOWN, 4.), now);
        let go = now + 4_000;
        assert_eq!(l.watched.go_at, Some(go));
        now = go;
        l.game(&race_state(9, race_phase::RACING, 0.), now);
        // Rider 0 rides honestly at 90 ft/s and finishes in 60 s.
        // Rider 1 reports a clock-slowed time (50 s for 60 s of server time).
        // Rider 2 teleports far ahead.
        for s in 0..=60u64 {
            let t = go + s * 1000;
            let d = s as f32 * 90.;
            let done = s == 60;
            l.progress(0, &progress(9, 0, d, if done { 60_000 } else { 0 }), t);
            l.progress(1, &progress(9, 0, d, if done { 50_000 } else { 0 }), t);
            let jump = if s == 10 { 5_000. } else { 0. };
            l.progress(2, &progress(9, 0, d + jump, 0), t);
        }
        now = go + 61_000;
        let mut last = race_state(9, race_phase::RESULTS, 0.);
        last.race.cpus = 2;
        last.race.cpu_scores = [58_000, RACE_DNF + 1, 0, 0];
        l.game(&last, now);
        l.step(now, &humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::RESULTS);
        let state = l.state(&humans);
        let order: Vec<u8> = state.results.iter().map(|p| p.peer).collect();
        // CPU 0 (58 s), rider 0 (60 s), then DNFs: CPU 1, the refused riders.
        assert_eq!(order[..3], [CPU_PEER, 0, CPU_PEER + 1]);
        assert!(
            state.results[3..]
                .iter()
                .all(|p| p.flags & (result_flag::REJECTED | result_flag::DNF)
                    == result_flag::REJECTED | result_flag::DNF)
        );
        assert!(state.verified);
        let records = l.take_records();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!((r.playlist, r.mode, r.map), ("race", 3, "nat01"));
        assert_eq!(r.riders[1].pid.as_deref(), Some("pid0"));
        assert_eq!(r.riders[1].value, 60_000);
        assert_eq!(r.riders[0].name, "CPU 1");
        let line = serde_json::to_string(r).unwrap();
        assert!(line.starts_with(r#"{"t":"#), "{line}");
        assert!(line.contains(r#""riders":[{"pid":null,"name":"CPU 1","cpu":true,"place":1,"value":58000,"dnf":false,"left":false}"#), "{line}");
    }

    #[test]
    fn a_finish_time_that_changes_or_laps_that_go_back_are_refused() {
        let humans = [human(0)];
        let (mut l, now) = to_play(&humans);
        l.game(&race_state(4, race_phase::RACING, 0.), now);
        l.progress(0, &progress(4, 1, 1_000., 0), now + 20_000);
        l.progress(0, &progress(4, 0, 1_100., 0), now + 21_000);
        assert!(l.watched.progress[&0].rejected);
        // A report for another race is ignored.
        let (mut l, now) = to_play(&humans);
        l.game(&race_state(4, race_phase::RACING, 0.), now);
        l.progress(0, &progress(5, 9, 1_000_000., 1), now);
        assert!(!l.watched.progress.contains_key(&0));
    }

    #[test]
    fn the_driver_hands_over_and_a_room_that_empties_waits_again() {
        let humans = [human(0), human(1)];
        let (mut l, now) = to_play(&humans);
        assert_eq!(l.driver(), 0);
        let fx = l.step(now + 10, &humans[1..], "QPR00001");
        assert_eq!(fx.driver, Some(1));
        assert_eq!(l.driver(), 1);
        // A late joiner spectates the round.
        let late = [human(1), human(3)];
        let state = l.state(&late);
        assert!(state.seat(3).unwrap().flags & seat_flag::SPECTATING != 0);
        assert!(state.seat(1).unwrap().flags & seat_flag::DRIVER != 0);
        // Everyone leaves: the loop waits in the lobby.
        let fx = l.step(now + 20, &[], "QPR00001");
        assert_eq!(l.phase(), loop_phase::LOBBY);
        assert!(fx.changed);
        assert_eq!(l.driver(), NO_PEER);
    }

    #[test]
    fn a_round_the_driver_never_starts_goes_back_to_the_vote_without_a_record() {
        let humans = [human(0)];
        let (mut l, now) = to_play(&humans);
        l.step(now + START_MS, &humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::RESULTS);
        assert!(l.take_records().is_empty());
        assert!(!l.state(&humans).verified);
        l.step(now + START_MS + RESULTS_MS, &humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
    }

    #[test]
    fn rematch_from_everyone_shortens_the_podium() {
        let humans = [human(0), human(1)];
        let (mut l, now) = to_play(&humans);
        l.game(&race_state(2, race_phase::RESULTS, 0.), now + 1);
        l.step(now + 1, &humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::RESULTS);
        let stay = Vote {
            rematch: rematch::STAY,
            ..vote(1, NO_CHOICE)
        };
        l.vote(0, stay);
        l.vote(
            1,
            Vote {
                rematch: rematch::LEAVE,
                ..stay
            },
        );
        assert!(l.state(&humans).seat(1).unwrap().flags & seat_flag::LEAVING != 0);
        l.step(now + 1_000, &humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::RESULTS);
        l.step(now + 1 + RESULTS_MIN_MS, &humans, "QPR00001");
        assert_eq!(l.phase(), loop_phase::VOTE);
        // The card just played is not dealt again.
        assert!(!l.options.contains(&l.last_round.unwrap()));
    }

    #[test]
    fn freestyle_scores_never_exceed_the_riders_own_reports() {
        let mut l = Loop::new(playlist::STUNTS, 5);
        let humans = [human(0), human(1)];
        l.step(0, &humans, "QPS00001");
        l.step(LOBBY_MS, &humans, "QPS00001");
        l.step(LOBBY_MS + VOTE_MS, &humans, "QPS00001");
        l.setup.round = Round {
            mode: game_kind::FREESTYLE,
            map: 10,
            variant: 0,
        };
        l.step(LOBBY_MS + VOTE_MS + COUNTDOWN_MS, &humans, "QPS00001");
        assert_eq!(l.phase(), loop_phase::PLAY);
        let mut state = GameState {
            game: game_kind::FREESTYLE,
            serial: 3,
            phase: game_phase::PLAY,
            ..Default::default()
        };
        let now = LOBBY_MS + VOTE_MS + COUNTDOWN_MS;
        l.game(&state, now);
        let claim = |stunt: f32| GameClaim {
            peer: 0,
            serial: 3,
            kind: claim_kind::FREESTYLE,
            index: 1,
            points: 0.,
            stunt,
        };
        l.claim(0, &claim(1_500.));
        l.claim(1, &claim(900.));
        state.phase = game_phase::OVER;
        state.marks.players = 0b11;
        state.marks.stunts = [1_500., 4_000., 0., 0.];
        l.game(&state, now + 120_000);
        l.step(now + 120_000, &humans, "QPS00001");
        let s = l.state(&humans);
        assert_eq!(
            s.results
                .iter()
                .map(|p| (p.peer, p.value))
                .collect::<Vec<_>>(),
            [(0, 1_500), (1, 900)]
        );
        // The driver's table disagreed with rider 1's own reports.
        assert!(!s.verified);
    }
}
