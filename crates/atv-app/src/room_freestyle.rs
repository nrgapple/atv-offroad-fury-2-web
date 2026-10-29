//! Freestyle in a multiplayer room (research/MULTIPLAYER.md "Room
//! Freestyle"): a timed stunt session on the room's course for every room
//! rider, with a host-owned score table.
//!
//! - Each rider scores its own stunts with the ride's recovered scoring
//!   (the committed stunt points the Freestyle HUD shows, and its finished
//!   chains): the session score is the stunt points committed since GO,
//!   the best combo the most one landed chain added, and the combos the
//!   different chains of two or more stunts (`002fc050`'s ComboMambo rule).
//! - Riders report their row to the host (`GameClaim` kind 2); the host
//!   checks each report (this session, a rider in the room and playing,
//!   bounded, never lower than the last) and broadcasts the table
//!   (`GameState` game 6) with the clock and phase. Every page shows the
//!   host's table, so the results agree.
//! - Slots are peer ids (a private room holds four). The players are fixed
//!   at GO; a rider who joins later spectates and rides the next session.
//!   A rider who leaves drops out of the table. If the host leaves, the
//!   relay's next host runs on from its mirror of the table.
use atv_game::multiplayer::{
    GameClaim, GameMarks, GameState, MAX_GAME_POINTS, MAX_MARKS, NO_PEER, claim_kind, game_kind,
    game_phase,
};
use atv_game::ride::Ride;

/// Room slots (a private room's capacity; peer ids 0-3).
pub const SLOTS: usize = atv_game::multiplayer::RACE_RIDERS;
/// The session's time limit: the Freestyle event clock (`00289d50`
/// settings +0x57c, 120 s on a new profile).
pub const TIME_LIMIT: f32 = atv_core::freestyle::TIME_LIMIT;
/// The longest the host's start waits for riders still loading (s).
pub const LOAD_WAIT: f32 = 30.;
/// Seconds the host keeps "Time Up" up for riders' last reports before the
/// table is final (a chain landed at the buzzer reaches it a relay delay
/// later).
pub const FINAL_WAIT: f32 = 2.;
/// Seconds between a rider's reports (and at once on every change).
const REPORT: f32 = 0.5;

/// The host's session length: the Freestyle clock (`freestyleTest=fast`:
/// 25 s for tests).
pub fn session_seconds() -> f32 {
    if crate::url_flag("freestyleTest=fast") {
        25.
    } else {
        TIME_LIMIT
    }
}

/// One rider's session line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Row {
    /// Stunt points committed since GO.
    pub score: f32,
    /// The most one landed chain added.
    pub best: f32,
    /// Different combos (chains of two or more stunts) landed.
    pub combos: u32,
}

impl Row {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"score": self.score, "best": self.best, "combos": self.combos})
    }

    fn bounded(&self) -> bool {
        self.score.is_finite()
            && self.best.is_finite()
            && (0. ..=MAX_GAME_POINTS).contains(&self.score)
            && (0. ..=self.score).contains(&self.best)
            && self.combos as usize <= MAX_MARKS
    }
}

/// The slots in `present` as a player mask.
pub fn mask(present: &[bool; SLOTS]) -> u8 {
    present
        .iter()
        .enumerate()
        .filter(|(_, p)| **p)
        .fold(0, |m, (slot, _)| m | 1 << slot)
}

/// What a host tick changed (for the start lamps' sounds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tick {
    Beep,
    Go,
    TimeUp,
    Over,
}

/// The session's rules and score table: the host's, mirrored by guests.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub serial: u32,
    /// `game_phase`: countdown, play, complete (Time Up, final reports),
    /// over (results).
    pub phase: u8,
    /// Seconds to GO (countdown), or left of the final wait (complete).
    pub timer: f32,
    /// Seconds left on the session clock.
    pub clock: f32,
    /// The slots playing this session (bit N slot N).
    pub players: u8,
    pub present: [bool; SLOTS],
    /// Host: slots that reported in (loaded the course) this session.
    pub joined: [bool; SLOTS],
    /// Host: seconds the start has waited for riders still loading.
    pub waited: f32,
    /// Host: hold the countdown until another rider has loaded (a session
    /// hosted from the menus, so invitees play rather than spectate).
    pub hold: bool,
    pub rows: [Row; SLOTS],
}

impl Table {
    pub fn new(serial: u32, limit: f32) -> Self {
        Self {
            serial,
            phase: game_phase::COUNTDOWN,
            timer: crate::games::COUNTDOWN,
            clock: limit,
            players: 0,
            present: [false; SLOTS],
            joined: [false; SLOTS],
            waited: 0.,
            hold: false,
            rows: [Row::default(); SLOTS],
        }
    }

    pub fn playing(&self, slot: usize) -> bool {
        slot < SLOTS && self.players & (1 << slot) != 0
    }

    /// The room's riders now. Before GO everyone present plays (host);
    /// leavers stop playing and drop out of the table.
    pub fn set_present(&mut self, present: [bool; SLOTS], host: bool) {
        if host && self.phase == game_phase::COUNTDOWN {
            self.players = mask(&present);
        }
        self.players &= mask(&present);
        for (slot, (was, now)) in self.present.iter().zip(present).enumerate() {
            if *was && !now {
                self.rows[slot] = Row::default();
                self.joined[slot] = false;
            }
        }
        self.present = present;
    }

    /// A rider's report (host): refused unless it is for this session,
    /// from a rider in the room, bounded and not lower than the last.
    /// Any report from a present rider marks it loaded; rows count only
    /// from players during play and the final wait.
    pub fn report(&mut self, claim: &GameClaim) -> Result<(), &'static str> {
        let slot = claim.peer as usize;
        if claim.kind != claim_kind::FREESTYLE {
            return Err("not a Freestyle report");
        }
        if claim.serial != self.serial {
            return Err("another session");
        }
        if slot >= SLOTS || !self.present[slot] {
            return Err("not in the room");
        }
        self.joined[slot] = true;
        let row = Row {
            score: claim.stunt,
            best: claim.points,
            combos: u32::from(claim.index),
        };
        if !row.bounded() {
            return Err("out of range");
        }
        if !matches!(self.phase, game_phase::PLAY | game_phase::COMPLETE) {
            // Before GO a report only says the rider has loaded; once the
            // table is final nothing changes it.
            return if row == Row::default() || self.phase == game_phase::OVER {
                Ok(())
            } else {
                Err("not in play")
            };
        }
        if !self.playing(slot) {
            return Err("spectating");
        }
        let old = self.rows[slot];
        if row.score < old.score || row.best < old.best || row.combos < old.combos {
            return Err("score went back");
        }
        self.rows[slot] = row;
        Ok(())
    }

    /// The host's own line.
    pub fn set_own(&mut self, slot: usize, row: Row) {
        if slot < SLOTS
            && self.playing(slot)
            && matches!(self.phase, game_phase::PLAY | game_phase::COMPLETE)
            && row.bounded()
        {
            self.rows[slot] = row;
        }
    }

    /// Host: advances the start, the clock and the end; `me` is the host's
    /// slot (always loaded).
    pub fn tick(&mut self, dt: f32, me: usize) -> Option<Tick> {
        if me < SLOTS {
            self.joined[me] = true;
        }
        match self.phase {
            game_phase::COUNTDOWN => {
                let others = (0..SLOTS).any(|s| s != me && self.present[s] && self.joined[s]);
                if self.hold && others {
                    self.hold = false;
                }
                let loading = (0..SLOTS).any(|s| self.present[s] && !self.joined[s]);
                if self.hold {
                    return None;
                }
                if loading && self.waited < LOAD_WAIT {
                    self.waited += dt;
                    return None;
                }
                let before = self.timer.ceil();
                let first = self.timer >= crate::games::COUNTDOWN;
                self.timer -= dt;
                if self.timer <= 0. {
                    self.timer = 0.;
                    // The players are fixed at GO: riders still loading
                    // watch this session.
                    self.players = mask(&self.present) & mask(&self.joined);
                    self.phase = game_phase::PLAY;
                    Some(Tick::Go)
                } else if first || self.timer.ceil() < before {
                    Some(Tick::Beep)
                } else {
                    None
                }
            }
            game_phase::PLAY => {
                self.clock = (self.clock - dt).max(0.);
                if self.clock <= 0. || self.players == 0 {
                    self.phase = game_phase::COMPLETE;
                    self.timer = FINAL_WAIT;
                    Some(Tick::TimeUp)
                } else {
                    None
                }
            }
            game_phase::COMPLETE => {
                self.timer -= dt;
                if self.timer <= 0. {
                    self.timer = 0.;
                    self.phase = game_phase::OVER;
                    Some(Tick::Over)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The table's riders by score, then best combo, then combos (players
    /// still in the room only).
    pub fn standings(&self) -> Vec<(usize, Row)> {
        let mut rows: Vec<(usize, Row)> = (0..SLOTS)
            .filter(|s| self.present[*s] && self.playing(*s))
            .map(|s| (s, self.rows[s]))
            .collect();
        rows.sort_by(|(a, x), (b, y)| {
            y.score
                .total_cmp(&x.score)
                .then(y.best.total_cmp(&x.best))
                .then(y.combos.cmp(&x.combos))
                .then(a.cmp(b))
        });
        rows
    }

    /// This rider's place among the players (1 when not playing).
    pub fn place(&self, slot: usize) -> usize {
        self.standings()
            .iter()
            .position(|(s, _)| *s == slot)
            .map_or(1, |p| p + 1)
    }

    /// The host's broadcast.
    pub fn state(&self) -> GameState {
        let mut stunts = [0.; SLOTS];
        let mut scores = [0; SLOTS];
        let mut marks = Vec::with_capacity(SLOTS);
        for slot in 0..SLOTS {
            let row = self.rows[slot];
            stunts[slot] = row.score;
            scores[slot] = row.combos;
            marks.push((
                if self.playing(slot) {
                    slot as u8
                } else {
                    NO_PEER
                },
                row.best.min(row.score),
            ));
        }
        GameState {
            game: game_kind::FREESTYLE,
            serial: self.serial,
            phase: self.phase,
            timer: self.timer.max(0.),
            clock: self.clock.max(0.),
            scores,
            holder: NO_PEER,
            marks: GameMarks {
                venue: 0,
                players: self.players,
                stunts,
                marks,
            },
            ..Default::default()
        }
    }

    /// A guest's mirror of the host's table.
    pub fn mirror(&mut self, g: &GameState) {
        if g.game != game_kind::FREESTYLE {
            return;
        }
        self.serial = g.serial;
        self.phase = g.phase;
        self.timer = g.timer;
        self.clock = g.clock;
        self.players = g.marks.players & mask(&self.present);
        for slot in 0..SLOTS {
            self.rows[slot] = Row {
                score: g.marks.stunts[slot],
                best: g.marks.marks.get(slot).map_or(0., |m| m.1),
                combos: g.scores[slot],
            };
        }
    }
}

/// A rider's own session score from its ride.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tracker {
    pub row: Row,
    /// Committed points at the last sample.
    last: f32,
    /// The different combos landed (stunt ids in order).
    combos: Vec<Vec<usize>>,
}

impl Tracker {
    /// GO: count from the ride's committed points now.
    pub fn begin(&mut self, committed: f32) {
        *self = Self {
            last: committed,
            ..Default::default()
        };
    }

    /// The committed points now and the chains finished since the last
    /// sample (success, stunt ids). Points only add: a reset that clears
    /// the ride's score frame takes nothing back.
    pub fn sample(&mut self, committed: f32, chains: &[(bool, Vec<usize>)]) {
        let gain = if committed.is_finite() {
            (committed - self.last).max(0.)
        } else {
            0.
        };
        if committed.is_finite() {
            self.last = committed;
        }
        self.row.score = (self.row.score + gain).min(MAX_GAME_POINTS);
        if chains.iter().any(|(landed, _)| *landed) {
            self.row.best = self.row.best.max(gain);
        }
        for (landed, ids) in chains {
            if *landed
                && ids.len() >= 2
                && !self.combos.contains(ids)
                && self.combos.len() < MAX_MARKS
            {
                self.combos.push(ids.clone());
            }
        }
        self.row.combos = self.combos.len() as u32;
        self.row.best = self.row.best.min(self.row.score);
    }
}

/// The room's Freestyle session on this page.
pub struct RoomFreestyle {
    pub table: Table,
    pub host: bool,
    pub me: usize,
    pub names: [String; SLOTS],
    /// The room's course (results line).
    pub course: String,
    pub tracker: Tracker,
    /// Placed at the start (or joined under way: not placed).
    placed: bool,
    /// This page saw the countdown (else it joined under way).
    seen_countdown: bool,
    /// This rider's GO was taken (its tracker started).
    began: bool,
    /// The ride's step count at the last frame (None: not seen yet).
    steps: Option<u64>,
    report: f32,
    sent: Option<(u32, u8, Row)>,
    sent_step: u64,
    sent_phase: u8,
    /// The green lamp's second after GO.
    go: f32,
    /// One-shot sounds for the ride's mixer.
    pub sounds: Vec<&'static str>,
    /// A sign and its seconds left.
    notice: Option<(String, f32)>,
    /// The results were shown.
    pub reported: bool,
    pub summary: Option<String>,
    /// Reports accepted / refused (host diagnostics).
    pub accepted: u32,
    pub refused: u32,
    /// Test tooling: `freestyleTest=score` credits this rider a landed
    /// two-stunt chain worth 1000 + 100 x slot each second of play (the
    /// scoring itself is the ride's; this exercises reports and table).
    test_score: bool,
    test_clock: f32,
    test_chain: usize,
}

impl RoomFreestyle {
    pub fn new(host: bool, serial: u32, limit: f32, course: String) -> Self {
        Self {
            table: Table::new(serial, limit),
            host,
            me: 0,
            names: Default::default(),
            course,
            tracker: Tracker::default(),
            placed: false,
            seen_countdown: false,
            began: false,
            steps: None,
            report: REPORT,
            sent: None,
            sent_step: 0,
            sent_phase: u8::MAX,
            go: 0.,
            sounds: Vec::new(),
            notice: None,
            reported: false,
            summary: None,
            accepted: 0,
            refused: 0,
            test_score: crate::url_flag("freestyleTest=score"),
            test_clock: 0.,
            test_chain: 0,
        }
    }

    /// Play Again (host): a new session for the room, in place.
    pub fn restart(&mut self, serial: u32) {
        let limit = self.limit();
        let mut next = Self::new(self.host, serial, limit, self.course.clone());
        next.me = self.me;
        next.names = self.names.clone();
        next.table.present = self.table.present;
        next.test_score = self.test_score;
        *self = next;
    }

    /// The session's length (a guest takes the host's clock at the start).
    fn limit(&self) -> f32 {
        session_seconds()
    }

    pub fn spectating(&self) -> bool {
        !self.table.playing(self.me)
    }

    /// The ATV is held through the start countdown (riders placed at the
    /// start) and, for players, after Time Up.
    pub fn locked(&self) -> bool {
        match self.table.phase {
            game_phase::COUNTDOWN => self.seen_countdown,
            game_phase::PLAY => false,
            _ => !self.spectating(),
        }
    }

    /// Room placement: slot `me` ten feet to the side of the scene start
    /// per slot (split screen's Freeride starts).
    fn place(&mut self, ride: &mut Ride) {
        let (at, yaw) = ride.start_pose();
        let (at, yaw) = crate::split::freeride_start_for(at, yaw, self.me as u8 + 1);
        if let Err(error) = ride.place_beside(at, yaw, [0.; 3]) {
            crate::status(&format!("Freestyle placement failed: {error}"));
        }
        ride.take_chains();
        ride.take_free_records();
        self.tracker.begin(ride.committed_points());
    }

    /// Advances the session; returns the results summary once, when the
    /// table is final.
    pub fn frame(&mut self, ride: &mut Ride, net: &mut crate::multiplayer::Net) -> Option<String> {
        let (me, roster) = net.roster();
        let me = me.map(usize::from).filter(|m| *m < SLOTS)?;
        let names = net.names(&roster);
        let host = net.is_host();
        // Host handoff: the new host takes over its mirror; riders in the
        // room have loaded the course already.
        if host && !self.host && self.table.phase == game_phase::COUNTDOWN {
            self.table.joined = self.table.present;
        }
        self.host = host;
        self.me = me;
        let mut present = [false; SLOTS];
        for (id, name) in roster.iter().zip(names) {
            if (*id as usize) < SLOTS {
                present[*id as usize] = true;
                self.names[*id as usize] = name;
            }
        }
        self.table.set_present(present, host);
        // The first frame (or a reloaded ride) only takes the step count.
        let steps = ride.steps();
        let dt = self
            .steps
            .filter(|s| *s <= steps)
            .map_or(0., |s| (steps - s) as f32 * atv_game::ride::STEP);
        self.steps = Some(steps);
        if !host {
            let state = net
                .game_state()
                .filter(|g| g.game == game_kind::FREESTYLE && g.serial == self.table.serial)
                .cloned();
            if let Some(g) = &state {
                let before = self.table.phase;
                let timer = self.table.timer;
                self.table.mirror(g);
                if g.phase == game_phase::COUNTDOWN {
                    if !self.seen_countdown || g.timer.ceil() < timer.ceil() {
                        self.sounds.push("beep01.vag");
                    }
                    self.seen_countdown = true;
                }
                if before == game_phase::COUNTDOWN
                    && g.phase == game_phase::PLAY
                    && self.seen_countdown
                {
                    self.go = 1.;
                    self.sounds.push("go.vag");
                }
                if before != game_phase::COMPLETE && g.phase == game_phase::COMPLETE {
                    self.notice = Some(("Time Up".into(), FINAL_WAIT));
                }
            }
        }
        // Placement at the start; a rider who joined under way watches.
        if !self.placed {
            self.placed = true;
            if self.table.phase == game_phase::COUNTDOWN {
                self.seen_countdown = true;
                self.place(ride);
            } else if self.spectating() {
                self.notice = Some(("Spectating: you ride the next session".into(), 5.));
            }
        }
        if dt <= 0. {
            return self.results();
        }
        if self.notice.as_ref().is_some_and(|(_, s)| *s <= 0.) {
            self.notice = None;
        }
        if let Some((_, seconds)) = &mut self.notice {
            *seconds -= dt;
        }
        self.go = (self.go - dt).max(0.);
        if host {
            for claim in net.take_claims() {
                if claim.kind != claim_kind::FREESTYLE {
                    continue;
                }
                match self.table.report(&claim) {
                    Ok(()) => self.accepted += 1,
                    Err(_) => self.refused += 1,
                }
            }
            let started = self.table.phase == game_phase::COUNTDOWN;
            match self.table.tick(dt, me) {
                Some(Tick::Beep) => self.sounds.push("beep01.vag"),
                Some(Tick::Go) => {
                    self.go = 1.;
                    self.sounds.push("go.vag");
                }
                Some(Tick::TimeUp) => self.notice = Some(("Time Up".into(), FINAL_WAIT)),
                _ => {}
            }
            if started && self.table.hold {
                self.notice = Some((
                    "Waiting for riders: share the room invite (Pause, Restart Game starts now)"
                        .into(),
                    1.,
                ));
            }
        }
        // GO: this rider's own count starts.
        let playing = !self.spectating();
        if self.table.phase == game_phase::PLAY && !self.began {
            self.began = true;
            ride.take_chains();
            self.tracker.begin(ride.committed_points());
        }
        if self.table.phase == game_phase::PLAY && playing {
            let chains = ride.take_chains();
            self.tracker.sample(ride.committed_points(), &chains);
            self.test_hook(dt);
        } else if self.table.phase == game_phase::COUNTDOWN {
            ride.take_chains();
        }
        if playing && self.began {
            if host {
                self.table.set_own(me, self.tracker.row);
            } else if matches!(self.table.phase, game_phase::PLAY | game_phase::COMPLETE) {
                // This page shows its own line live; the host's copy lags
                // a relay delay.
                self.table.rows[me] = self.tracker.row;
            }
        }
        if host {
            let phase = self.table.phase;
            if phase != self.sent_phase || steps >= self.sent_step + 6 || steps < self.sent_step {
                self.sent_step = steps;
                self.sent_phase = phase;
                net.send_game(&self.table.state());
            }
        } else {
            // Reports: on every change, at least every REPORT seconds
            // (a countdown report says this rider has loaded).
            self.report += dt;
            let row = if playing && self.began {
                self.tracker.row
            } else {
                Row::default()
            };
            if self.table.phase != game_phase::OVER
                && (self.report >= REPORT || self.sent.map(|s| s.2) != Some(row))
            {
                self.report = 0.;
                self.sent = Some((self.table.serial, self.table.phase, row));
                net.send_claim(GameClaim {
                    peer: me as u8,
                    serial: self.table.serial,
                    kind: claim_kind::FREESTYLE,
                    index: row.combos.min(MAX_MARKS as u32) as u16,
                    points: row.best.min(row.score),
                    stunt: row.score.clamp(0., MAX_GAME_POINTS),
                });
            }
        }
        self.results()
    }

    /// `freestyleTest=score`: each second, one landed two-stunt chain worth
    /// 1000 + 100 x slot (a different combo each time), through the same
    /// tracker the ride's chains feed.
    fn test_hook(&mut self, dt: f32) {
        if !self.test_score {
            return;
        }
        self.test_clock += dt;
        if self.test_clock >= 1. {
            self.test_clock = 0.;
            self.test_chain += 1;
            let committed = self.tracker.last + 1000. + 100. * self.me as f32;
            self.tracker.sample(
                committed,
                &[(true, vec![self.test_chain, self.test_chain + 1])],
            );
        }
    }

    fn results(&mut self) -> Option<String> {
        if self.reported || self.table.phase != game_phase::OVER {
            return None;
        }
        self.reported = true;
        let grouped = |v: f32| atv_ui::hud::objective::grouped(v as i64);
        let rows: Vec<String> = self
            .table
            .standings()
            .iter()
            .enumerate()
            .map(|(place, (slot, row))| {
                format!(
                    "{}. {}: Score {}, Best Combo {}, Combos {}.",
                    place + 1,
                    self.names[*slot],
                    grouped(row.score),
                    grouped(row.best),
                    row.combos
                )
            })
            .collect();
        let summary = format!("Freestyle on {}: Time Up. {}", self.course, rows.join(" "));
        self.summary = Some(summary.clone());
        Some(summary)
    }

    /// The HUD panel (the games panel: clock, combos, place, score, and
    /// the leader or a sign on its last line).
    pub fn view(&self) -> atv_ui::hud::games::View {
        let row = self.table.rows[self.me.min(SLOTS - 1)];
        let countdown = self.table.phase == game_phase::COUNTDOWN;
        let leader = self
            .table
            .standings()
            .first()
            .filter(|(slot, r)| *slot != self.me && r.score > 0.)
            .map(|(slot, r)| {
                format!(
                    "LEADER {} {}",
                    self.names[*slot],
                    atv_ui::hud::objective::grouped(r.score as i64)
                )
            });
        atv_ui::hud::games::View {
            title: "Freestyle".into(),
            clock: atv_ui::hud::objective::clock(self.table.clock),
            low: self.table.phase == game_phase::PLAY && self.table.clock <= 10.,
            count_label: "COMBOS".into(),
            count: row.combos.to_string(),
            score: row.score as i64,
            place: self.table.place(self.me),
            start: (countdown && !self.table.hold).then(|| (self.table.timer.ceil() as u32).max(1)),
            lights: if countdown && self.seen_countdown && !self.table.hold {
                crate::games::lamp_step(false, self.table.timer, 0.)
            } else if self.go > 0. {
                Some(4)
            } else {
                None
            },
            sign: self
                .notice
                .as_ref()
                .map(|(t, _)| t.clone())
                .or_else(|| {
                    self.spectating()
                        .then(|| "Spectating: you ride the next session".into())
                })
                .or(leader),
        }
    }

    /// Diagnostics for browser tests (`#ui-state` ride.roomFreestyle).
    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({
            "host": self.host,
            "serial": self.table.serial,
            "me": self.me,
            "phase": self.table.phase,
            "clock": self.table.clock,
            "timer": self.table.timer,
            "present": self.table.present,
            "players": self.table.players,
            "spectating": self.spectating(),
            "hold": self.table.hold,
            "own": self.tracker.row.json(),
            "rows": self.table.rows.iter().map(Row::json).collect::<Vec<_>>(),
            "standings": self.table.standings().iter().map(|(slot, row)| {
                serde_json::json!({"rider": slot, "name": self.names[*slot], "score": row.score, "best": row.best, "combos": row.combos})
            }).collect::<Vec<_>>(),
            "accepted": self.accepted,
            "refused": self.refused,
            "reported": self.reported,
            "summary": self.summary,
            "locked": self.locked(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(peer: u8, serial: u32, score: f32, best: f32, combos: u16) -> GameClaim {
        GameClaim {
            peer,
            serial,
            kind: claim_kind::FREESTYLE,
            index: combos,
            points: best,
            stunt: score,
        }
    }

    /// A host table for slots 0 and 1, past its countdown.
    fn playing() -> Table {
        let mut t = Table::new(7, 120.);
        t.set_present([true, true, false, false], true);
        t.report(&report(1, 7, 0., 0., 0)).unwrap();
        for _ in 0..40 {
            t.tick(0.1, 0);
        }
        assert_eq!(t.phase, game_phase::PLAY);
        t
    }

    #[test]
    fn the_start_waits_for_riders_then_counts_down_to_go() {
        let mut t = Table::new(1, 120.);
        t.set_present([true, true, false, false], true);
        assert_eq!(t.players, 0b11);
        // Slot 1 has not loaded: the start waits (up to LOAD_WAIT).
        assert_eq!(t.tick(1., 0), None);
        assert_eq!(t.timer, crate::games::COUNTDOWN);
        assert_eq!(t.waited, 1.);
        // Its first report (zeros) says it has loaded.
        assert!(t.report(&report(1, 1, 0., 0., 0)).is_ok());
        assert_eq!(t.tick(0.1, 0), Some(Tick::Beep));
        let mut ticks = Vec::new();
        for _ in 0..40 {
            if let Some(tick) = t.tick(0.1, 0) {
                ticks.push(tick);
            }
        }
        assert_eq!(ticks.iter().filter(|t| **t == Tick::Beep).count(), 2);
        assert_eq!(ticks.last(), Some(&Tick::Go));
        assert_eq!(t.phase, game_phase::PLAY);
        assert_eq!(t.players, 0b11);
        // A non-zero row before GO is refused.
        let mut early = Table::new(2, 120.);
        early.set_present([true, true, false, false], true);
        assert_eq!(early.report(&report(1, 2, 10., 0., 0)), Err("not in play"));
    }

    #[test]
    fn a_rider_still_loading_after_the_load_wait_spectates() {
        let mut t = Table::new(1, 120.);
        t.set_present([true, false, true, false], true);
        let mut go = false;
        for _ in 0..400 {
            go |= t.tick(0.1, 0) == Some(Tick::Go);
        }
        assert!(go);
        assert_eq!(t.players, 0b1, "slot 2 never loaded");
        assert_eq!(t.report(&report(2, 1, 10., 0., 0)), Err("spectating"));
    }

    #[test]
    fn a_held_start_waits_until_another_rider_loads() {
        let mut t = Table::new(1, 120.);
        t.hold = true;
        t.set_present([true, false, false, false], true);
        for _ in 0..600 {
            assert_eq!(t.tick(0.1, 0), None);
        }
        t.set_present([true, true, false, false], true);
        t.report(&report(1, 1, 0., 0., 0)).unwrap();
        assert_eq!(t.tick(0.1, 0), Some(Tick::Beep));
        assert!(!t.hold);
    }

    #[test]
    fn reports_are_checked_against_the_session_room_and_last_row() {
        let mut t = playing();
        assert!(t.report(&report(1, 7, 500., 300., 1)).is_ok());
        assert_eq!(
            t.rows[1],
            Row {
                score: 500.,
                best: 300.,
                combos: 1
            }
        );
        assert_eq!(
            t.report(&report(1, 6, 900., 300., 1)),
            Err("another session")
        );
        assert_eq!(
            t.report(&report(2, 7, 900., 300., 1)),
            Err("not in the room")
        );
        assert_eq!(
            t.report(&report(9, 7, 900., 300., 1)),
            Err("not in the room")
        );
        assert_eq!(
            t.report(&report(1, 7, 400., 300., 1)),
            Err("score went back")
        );
        assert_eq!(
            t.report(&report(1, 7, 900., 200., 1)),
            Err("score went back")
        );
        assert_eq!(
            t.report(&report(1, 7, 900., 300., 0)),
            Err("score went back")
        );
        assert_eq!(t.report(&report(1, 7, 900., 1000., 1)), Err("out of range"));
        assert_eq!(
            t.report(&report(1, 7, f32::NAN, 0., 1)),
            Err("out of range")
        );
        assert_eq!(
            t.report(&report(1, 7, MAX_GAME_POINTS * 2., 0., 1)),
            Err("out of range")
        );
        assert_eq!(
            t.report(&GameClaim {
                kind: claim_kind::STUNT,
                ..report(1, 7, 900., 300., 1)
            }),
            Err("not a Freestyle report")
        );
        // Refusals leave the row as it was.
        assert_eq!(t.rows[1].score, 500.);
    }

    #[test]
    fn time_up_waits_for_last_reports_then_freezes_the_table() {
        let mut t = playing();
        t.set_own(
            0,
            Row {
                score: 100.,
                best: 100.,
                combos: 0,
            },
        );
        let mut ticks = Vec::new();
        for _ in 0..1300 {
            if let Some(tick) = t.tick(0.1, 0) {
                ticks.push(tick);
            }
            if t.phase != game_phase::PLAY {
                break;
            }
        }
        assert_eq!(ticks, [Tick::TimeUp]);
        assert_eq!(t.phase, game_phase::COMPLETE);
        // A chain landed at the buzzer still counts.
        assert!(t.report(&report(1, 7, 2000., 2000., 1)).is_ok());
        for _ in 0..30 {
            t.tick(0.1, 0);
        }
        assert_eq!(t.phase, game_phase::OVER);
        // Final: later reports change nothing.
        assert!(t.report(&report(1, 7, 9000., 2000., 1)).is_ok());
        assert_eq!(t.rows[1].score, 2000.);
        t.set_own(
            0,
            Row {
                score: 9000.,
                best: 100.,
                combos: 0,
            },
        );
        assert_eq!(t.rows[0].score, 100.);
        let standings: Vec<usize> = t.standings().iter().map(|(s, _)| *s).collect();
        assert_eq!(standings, [1, 0]);
    }

    #[test]
    fn standings_rank_score_then_best_combo_then_combos() {
        let mut t = Table::new(1, 120.);
        t.set_present([true; SLOTS], true);
        t.phase = game_phase::PLAY;
        t.rows = [
            Row {
                score: 100.,
                best: 50.,
                combos: 1,
            },
            Row {
                score: 300.,
                best: 50.,
                combos: 0,
            },
            Row {
                score: 100.,
                best: 80.,
                combos: 0,
            },
            Row {
                score: 100.,
                best: 50.,
                combos: 2,
            },
        ];
        let order: Vec<usize> = t.standings().iter().map(|(s, _)| *s).collect();
        assert_eq!(order, [1, 2, 3, 0]);
        assert_eq!(t.place(3), 3);
    }

    #[test]
    fn leavers_drop_out_and_late_joiners_spectate() {
        let mut t = Table::new(1, 120.);
        t.set_present([true, true, true, false], true);
        for s in 1..3 {
            t.report(&report(s, 1, 0., 0., 0)).unwrap();
        }
        while t.phase == game_phase::COUNTDOWN {
            t.tick(0.1, 0);
        }
        assert_eq!(t.players, 0b111);
        t.report(&report(2, 1, 700., 700., 0)).unwrap();
        // Slot 2 leaves: out of the table; a late rider takes slot 3.
        t.set_present([true, true, false, true], true);
        assert_eq!(t.players, 0b011);
        assert_eq!(t.rows[2], Row::default());
        assert_eq!(t.report(&report(3, 1, 10., 0., 0)), Err("spectating"));
        // A late joiner reusing the leaver's slot is not credited either.
        t.set_present([true, true, true, true], true);
        assert_eq!(t.players, 0b011);
        assert_eq!(t.report(&report(2, 1, 900., 0., 0)), Err("spectating"));
        assert_eq!(t.standings().len(), 2);
        // Everyone else leaving ends the session.
        t.set_present([true, false, false, false], true);
        t.set_present([false, false, false, false], true);
        assert_eq!(t.tick(0.1, 5), Some(Tick::TimeUp));
    }

    #[test]
    fn three_and_four_riders_share_one_table() {
        let mut t = Table::new(3, 120.);
        t.set_present([true; SLOTS], true);
        for s in 1..4 {
            t.report(&report(s, 3, 0., 0., 0)).unwrap();
        }
        while t.phase == game_phase::COUNTDOWN {
            t.tick(0.1, 0);
        }
        assert_eq!(t.players, 0b1111);
        for s in 1..4u8 {
            let score = 1000. * f32::from(s);
            t.report(&report(s, 3, score, score / 2., u16::from(s)))
                .unwrap();
        }
        t.set_own(
            0,
            Row {
                score: 2500.,
                best: 2500.,
                combos: 0,
            },
        );
        let order: Vec<usize> = t.standings().iter().map(|(s, _)| *s).collect();
        assert_eq!(order, [3, 0, 2, 1]);
    }

    #[test]
    fn a_guest_mirror_round_trips_the_host_table() {
        let mut host = playing();
        host.report(&report(1, 7, 1234., 1000., 3)).unwrap();
        host.set_own(
            0,
            Row {
                score: 50.,
                best: 50.,
                combos: 0,
            },
        );
        let state = host.state();
        assert!(state.validate().is_ok(), "{state:?}");
        let mut guest = Table::new(7, 120.);
        guest.set_present([true, true, false, false], false);
        guest.mirror(&state);
        assert_eq!(guest.rows, host.rows);
        assert_eq!(guest.players, host.players);
        assert_eq!(guest.phase, host.phase);
        assert_eq!(guest.standings(), host.standings());
        // Host handoff: the mirror runs on as the host table.
        guest.set_present([false, true, false, false], true);
        let mut over = false;
        for _ in 0..1400 {
            over |= guest.tick(0.1, 1) == Some(Tick::Over);
        }
        assert!(over);
        assert_eq!(guest.standings().len(), 1);
        assert_eq!(guest.standings()[0].0, 1);
        // Another game's state is ignored.
        let mut other = Table::new(7, 120.);
        other.mirror(&GameState {
            game: game_kind::TAG,
            ..Default::default()
        });
        assert_eq!(other.phase, game_phase::COUNTDOWN);
    }

    #[test]
    fn the_tracker_counts_committed_points_best_chain_and_different_combos() {
        let mut t = Tracker::default();
        t.begin(5000.);
        // Nothing landed yet.
        t.sample(5000., &[]);
        assert_eq!(t.row, Row::default());
        // A two-stunt chain adds 800.
        t.sample(5800., &[(true, vec![1, 2])]);
        assert_eq!(
            t.row,
            Row {
                score: 800.,
                best: 800.,
                combos: 1
            }
        );
        // The same combo again scores but does not count as a new combo.
        t.sample(6100., &[(true, vec![1, 2])]);
        assert_eq!(t.row.combos, 1);
        assert_eq!(t.row.score, 1100.);
        assert_eq!(t.row.best, 800.);
        // A single stunt is not a combo; a crashed chain is not landed.
        t.sample(6200., &[(true, vec![3])]);
        t.sample(6200., &[(false, vec![4, 5])]);
        assert_eq!(t.row.combos, 1);
        // A reset clearing the score frame takes nothing back.
        t.sample(0., &[]);
        t.sample(2000., &[(true, vec![2, 1])]);
        assert_eq!(
            t.row,
            Row {
                score: 3200.,
                best: 2000.,
                combos: 2
            }
        );
        t.sample(f32::NAN, &[]);
        assert_eq!(t.row.score, 3200.);
    }
}
