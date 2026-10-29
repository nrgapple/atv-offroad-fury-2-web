//! King of the Hills and Treasure Hunt in a multiplayer room
//! (research/MULTIPLAYER.md "Room games"). The room host runs the
//! recovered rules (`atv_core::games`) for every room slot and broadcasts
//! `GameState` (game 4 or 5) with the hills' owners and scores or the
//! treasures' collectors; guests mirror it.
//!
//! - Treasure Hunt pickups are the host's: it tests every rider's relayed
//!   position against the shown group's treasures.
//! - A King of the Hills capture needs the rider's own wheel contact, open
//!   stunt and landed chain, which poses do not carry: each guest runs the
//!   arming rules for itself against the mirrored hills and reports a
//!   capture (`GameClaim`); the host checks it (current game, rider in the
//!   room and near the hill, bounded points, rate) and applies it only
//!   when it beats the hill's score, as `00220b50` does.
//! - Slots are peer ids (a private room holds four); a rider who leaves is
//!   dropped from the results and frees their hills.
use crate::games::{GameSession, sample};
use atv_core::games::{GameKind, Rider};
pub use atv_game::multiplayer::MAX_GAME_POINTS;
use atv_game::multiplayer::{
    GameClaim, GameMarks, GameState, MAX_MARKS, NO_PEER, claim_kind, game_kind,
};
use atv_game::ride::Ride;

/// Room slots (a private room's capacity; peer ids 0-3).
pub const SLOTS: usize = atv_game::multiplayer::RACE_RIDERS;
/// A capture claim must come from within this of the hill (ft): the
/// claimant's relayed position lags its own by up to a few hundred ms.
pub const CLAIM_REACH: f32 = 400.;
/// The fewest seconds between one rider's accepted capture claims.
pub const CLAIM_INTERVAL: f32 = 0.25;
/// Seconds between a guest's stunt-score reports.
const STUNT_REPORT: f32 = 1.;
/// Where an absent slot's rider is placed for the rules (far below).
const NOWHERE: [f32; 3] = [0., -100_000., 0.];

/// The room code of a game (GameState::game).
pub fn code(kind: GameKind) -> Option<u8> {
    match kind {
        GameKind::KingOfTheHills => Some(game_kind::KING_OF_THE_HILLS),
        GameKind::TreasureHunt => Some(game_kind::TREASURE_HUNT),
        _ => None,
    }
}

pub fn kind(code: u8) -> Option<GameKind> {
    match code {
        game_kind::KING_OF_THE_HILLS => Some(GameKind::KingOfTheHills),
        game_kind::TREASURE_HUNT => Some(GameKind::TreasureHunt),
        _ => None,
    }
}

/// The archive venues room games play on, by wire code.
const VENUES: [&str; 14] = [
    "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free01", "free02",
    "free03", "free04", "free05", "free06",
];

pub fn venue_code(venue: &str) -> Option<u8> {
    VENUES.iter().position(|v| *v == venue).map(|v| v as u8)
}

/// The room's link to its game.
pub struct RoomLink {
    pub host: bool,
    /// The game (GameState serial); each Play Again is a new one.
    pub serial: u32,
    pub me: usize,
    pub present: [bool; SLOTS],
    pub names: [String; SLOTS],
    /// Slots present when the game started or since (leavers are dropped).
    seen: [bool; SLOTS],
    sent_step: u64,
    sent_phase: u8,
    /// Host: each slot's reported stunt score and last accepted claim.
    stunts: [f32; SLOTS],
    last_claim: [f32; SLOTS],
    clock: f32,
    report: f32,
    /// Claims accepted / refused (diagnostics).
    pub accepted: u32,
    pub refused: u32,
    /// Guest: the claim last sent (hill, score), so it is sent once.
    sent_claim: Option<(usize, u32)>,
    /// Host: slots that reported in this game (loaded the course).
    joined: [bool; SLOTS],
    /// Host: seconds the start has waited for riders to load.
    waited: f32,
}

/// The longest the host's start waits for riders still loading (s).
pub const LOAD_WAIT: f32 = 30.;

impl RoomLink {
    pub fn new(host: bool, serial: u32) -> Self {
        Self {
            host,
            serial,
            me: 0,
            present: [false; SLOTS],
            names: Default::default(),
            seen: [false; SLOTS],
            sent_step: 0,
            sent_phase: u8::MAX,
            stunts: [0.; SLOTS],
            last_claim: [-1.; SLOTS],
            clock: 0.,
            report: STUNT_REPORT,
            accepted: 0,
            refused: 0,
            sent_claim: None,
            joined: [false; SLOTS],
            waited: 0.,
        }
    }
}

/// Host-side check of a capture claim; returns why it is refused.
pub fn check_claim(
    claim: &GameClaim,
    serial: u32,
    present: &[bool; SLOTS],
    hills: &[[f32; 3]],
    position: Option<[f32; 3]>,
    since_last: f32,
) -> Result<(), &'static str> {
    let slot = claim.peer as usize;
    if claim.serial != serial {
        return Err("another game");
    }
    if slot >= SLOTS || !present[slot] {
        return Err("not in the game");
    }
    if claim.kind != claim_kind::CAPTURE {
        return Ok(());
    }
    let index = claim.index as usize;
    let Some(hill) = index.checked_sub(1).and_then(|i| hills.get(i)) else {
        return Err("no such hill");
    };
    if !(claim.points > 0. && claim.points <= MAX_GAME_POINTS) {
        return Err("points out of range");
    }
    if since_last < CLAIM_INTERVAL {
        return Err("too soon");
    }
    let Some(p) = position else {
        return Err("no position");
    };
    let d = ((p[0] - hill[0]).powi(2) + (p[2] - hill[2]).powi(2)).sqrt();
    if d > CLAIM_REACH {
        return Err("too far from the hill");
    }
    Ok(())
}

impl GameSession {
    /// A room's game (host or guest).
    pub fn for_room(kind: GameKind, venue: String, minutes: u32, host: bool, serial: u32) -> Self {
        let course = venue.clone();
        let mut game = GameSession::new(kind, venue, course, minutes, SLOTS);
        game.room = Some(RoomLink::new(host, serial));
        game
    }

    /// Room placement: slot `me` takes the TOY's start of that slot, else
    /// a spot beside start 1 (10 ft per slot).
    fn place_room(&mut self, ride: &mut Ride, me: usize) {
        if self.starts.is_empty() && self.venue_start.is_none() {
            let f = ride.frame();
            let [w, x, y, z] = f.orientation;
            use atv_core::math::Det;
            let yaw = Det::atan2_d(2. * (w * y + x * z), 1. - 2. * (y * y + x * x));
            self.venue_start = Some((f.origin, yaw));
        }
        let first = self
            .starts
            .first()
            .map(|s| (s.position, s.yaw()))
            .or(self.venue_start);
        let spot = match (self.starts.get(me), first) {
            (Some(s), _) => Some((s.position, s.yaw())),
            (None, Some((at, yaw))) => {
                use atv_core::math::Det;
                let (sin, cos) = yaw.sin_cos_d();
                let k = me as f32 * 10.;
                Some(([at[0] + cos * k, at[1], at[2] - sin * k], yaw))
            }
            _ => None,
        };
        if let Some((at, yaw)) = spot
            && let Err(error) = ride.place_beside(at, yaw, [0.; 3])
        {
            crate::status(&format!("Game placement failed: {error}"));
        }
        ride.take_chains();
        ride.take_free_records();
        self.steps = ride.steps();
        self.committed[me] = ride.committed_points();
        self.countdown = crate::games::COUNTDOWN;
        self.go = 0.;
    }

    /// Play Again (host): a new game for the room.
    pub fn room_restart(&mut self, ride: &mut Ride, serial: u32) {
        if let Some(room) = &mut self.room {
            room.serial = serial;
            room.sent_phase = u8::MAX;
            room.stunts = [0.; SLOTS];
            room.seen = room.present;
            room.sent_claim = None;
            room.joined = [false; SLOTS];
            room.waited = 0.;
            room.report = STUNT_REPORT;
        }
        if let Some(session) = &mut self.session {
            session.restart();
        }
        self.reported = false;
        self.placed = false;
        let me = self.room.as_ref().map_or(0, |r| r.me);
        self.placed = true;
        self.place_room(ride, me);
    }

    /// Advances a room game: the host's rules and broadcast, or the
    /// guest's mirror and own reports.
    pub fn room_update(&mut self, ride: &mut Ride, net: &mut crate::multiplayer::Net) {
        let Some(code) = code(self.kind) else { return };
        if self.session.is_none() || self.room.is_none() {
            return;
        }
        let (me, roster) = net.roster();
        let Some(me) = me.map(usize::from).filter(|m| *m < SLOTS) else {
            return;
        };
        let names = net.names(&roster);
        let host = net.is_host();
        {
            let room = self.room.as_mut().unwrap();
            room.me = me;
            room.host = host;
            let mut present = [false; SLOTS];
            for (id, name) in roster.iter().zip(names) {
                if (*id as usize) < SLOTS {
                    present[*id as usize] = true;
                    room.names[*id as usize] = name;
                }
            }
            // Leavers drop out of the game (their hills free up).
            for slot in 0..SLOTS {
                if room.present[slot] && !present[slot] {
                    if let Some(session) = &mut self.session {
                        session.rules.release(slot);
                    }
                }
            }
            room.present = present;
            for slot in 0..SLOTS {
                room.seen[slot] |= present[slot];
            }
        }
        if !self.placed {
            self.placed = true;
            self.place_room(ride, me);
        }
        let steps = ride.steps();
        let dt = steps.saturating_sub(self.steps) as f32 * atv_game::ride::STEP;
        self.steps = steps;
        if dt <= 0. {
            return;
        }
        if host {
            self.host_step(ride, net, me, dt, steps, code);
        } else {
            self.guest_step(ride, net, me, dt, code);
        }
        self.room_test_hooks(ride, net, me, dt);
    }

    /// Test tooling (browser tests only): `gameTest=visit` places the ATV
    /// on the next standing treasure each second; `gameTest=hill` sets it
    /// on hill 1 and, 2 s later, reports a 1500 + 100 x slot point capture
    /// of it (the arming rules are the single-player ones; this exercises
    /// the room's claim, check and broadcast).
    fn room_test_hooks(
        &mut self,
        ride: &mut Ride,
        net: &mut crate::multiplayer::Net,
        me: usize,
        dt: f32,
    ) {
        let Some(session) = &mut self.session else {
            return;
        };
        if !session.started || session.end.is_some() {
            return;
        }
        self.test_clock += dt;
        let markers = session.rules.markers();
        if self.visit && self.test_clock >= 1. {
            // Odd slots take the last standing treasure, so riders differ.
            let next = if me % 2 == 1 {
                markers.last()
            } else {
                markers.first()
            };
            if let Some(m) = next {
                self.test_clock = 0.;
                self.visits += 1;
                let _ = ride.place_beside(m.position, 0., [0.; 3]);
            }
            return;
        }
        let Some(hill) = markers.first().map(|m| m.position) else {
            return;
        };
        let raise = atv_core::games::king_of_the_hill::MARKER_RAISE;
        match self.hill {
            1 => {
                let _ = ride.place_beside([hill[0], hill[1] - raise, hill[2]], 0., [0.; 3]);
                self.hill = 2;
                self.test_clock = 0.;
            }
            2 if self.test_clock > 2. => {
                self.hill = 3;
                let points = 1500. + 100. * me as f32;
                let Some(room) = &self.room else { return };
                if room.host {
                    if let Some(notice) = session.rules.claim(me, 1, points) {
                        session.notice = Some(notice);
                    }
                } else {
                    net.send_claim(GameClaim {
                        peer: me as u8,
                        serial: room.serial,
                        kind: claim_kind::CAPTURE,
                        index: 1,
                        points,
                        stunt: 0.,
                    });
                }
            }
            _ => {}
        }
    }

    fn host_step(
        &mut self,
        ride: &mut Ride,
        net: &mut crate::multiplayer::Net,
        me: usize,
        dt: f32,
        steps: u64,
        code: u8,
    ) {
        let session = self.session.as_mut().unwrap();
        let room = self.room.as_mut().unwrap();
        room.clock += dt;
        // The start waits (up to LOAD_WAIT) for every rider to load the
        // course and report in, so all share the 3-2-1.
        room.joined[me] = true;
        let loading = (0..SLOTS).any(|s| room.present[s] && !room.joined[s]);
        if !session.started && loading && room.waited < LOAD_WAIT {
            room.waited += dt;
        } else if session.started {
            self.go = (self.go - dt).max(0.);
        } else {
            if self.countdown >= crate::games::COUNTDOWN {
                self.sounds.push("beep01.vag");
            }
            let before = self.countdown.ceil();
            self.countdown -= dt;
            if self.countdown <= 0. {
                self.countdown = 0.;
                session.begin();
                self.go = 1.;
                self.sounds.push("go.vag");
            } else if self.countdown.ceil() < before {
                self.sounds.push("beep01.vag");
            }
        }
        // Riders' reports: stunt scores and King of the Hills captures.
        let hills: Vec<[f32; 3]> = if self.kind == GameKind::KingOfTheHills {
            session.rules.markers().iter().map(|m| m.position).collect()
        } else {
            Vec::new()
        };
        for claim in net.take_claims() {
            let slot = claim.peer as usize;
            let since = room.clock - room.last_claim.get(slot).copied().unwrap_or(0.);
            let position = net.rider_motion(claim.peer).map(|(p, _)| p);
            match check_claim(&claim, room.serial, &room.present, &hills, position, since) {
                Ok(()) => {
                    room.joined[slot] = true;
                    room.stunts[slot] = claim.stunt;
                    if claim.kind == claim_kind::CAPTURE && session.started && session.end.is_none()
                    {
                        room.last_claim[slot] = room.clock;
                        if let Some(notice) =
                            session
                                .rules
                                .claim(slot, claim.index as usize, claim.points)
                        {
                            room.accepted += 1;
                            session.notice = Some(notice);
                        }
                    }
                }
                Err(_) => room.refused += 1,
            }
        }
        let collected: i32 = session.rules.scores().iter().sum();
        let mut riders = vec![
            Rider {
                position: NOWHERE,
                grounded: true,
                ..Default::default()
            };
            SLOTS
        ];
        let mut stunts = room.stunts;
        let (own, _) = sample(ride, &mut self.committed[me], None);
        riders[me] = own;
        stunts[me] = self.committed[me];
        room.stunts[me] = self.committed[me];
        for slot in 0..SLOTS {
            if slot != me
                && room.present[slot]
                && let Some((position, _)) = net.rider_motion(slot as u8)
            {
                riders[slot].position = position;
            }
        }
        session.tick(dt, &riders, &stunts);
        if self.kind == GameKind::TreasureHunt
            && session.rules.scores().iter().sum::<i32>() > collected
        {
            self.sounds.push("iconhit.vag");
        }
        // 10 Hz, and at once on every phase change or score.
        let phase = session.phase();
        let scored = session.rules.scores().iter().sum::<i32>() != collected;
        if phase != room.sent_phase || scored || steps >= room.sent_step + 6 {
            room.sent_step = steps;
            room.sent_phase = phase;
            let mut scores = [0u32; SLOTS];
            for (s, v) in scores.iter_mut().zip(session.rules.scores()) {
                *s = v.clamp(0, MAX_MARKS as i32) as u32;
            }
            let marks: Vec<(u8, f32)> = session
                .rules
                .marks_state()
                .into_iter()
                .take(MAX_MARKS)
                .map(|(owner, v)| {
                    (
                        owner.filter(|o| *o < SLOTS).map_or(NO_PEER, |o| o as u8),
                        v.clamp(0., MAX_GAME_POINTS),
                    )
                })
                .collect();
            let mut stunt_out = [0.; SLOTS];
            for (o, s) in stunt_out.iter_mut().zip(session.stunt.iter()) {
                *o = s.clamp(0., MAX_GAME_POINTS);
            }
            net.send_game(&GameState {
                game: code,
                serial: room.serial,
                phase,
                timer: if session.started {
                    session.rules.countdown().unwrap_or(0.)
                } else {
                    self.countdown.max(0.)
                },
                clock: session.time_left,
                scores,
                marks: GameMarks {
                    venue: venue_code(&self.venue).unwrap_or(0),
                    stunts: stunt_out,
                    marks,
                },
                ..Default::default()
            });
        }
    }

    fn guest_step(
        &mut self,
        ride: &mut Ride,
        net: &mut crate::multiplayer::Net,
        me: usize,
        dt: f32,
        code: u8,
    ) {
        let session = self.session.as_mut().unwrap();
        let room = self.room.as_mut().unwrap();
        let state = net
            .game_state()
            .filter(|g| g.game == code && g.serial == room.serial)
            .cloned();
        let (own, _) = sample(ride, &mut self.committed[me], None);
        if let Some(g) = &state {
            let was_started = session.started;
            if !session.started && g.phase == 0 {
                let before = self.countdown.ceil();
                if g.timer.ceil() < before || self.countdown >= crate::games::COUNTDOWN {
                    self.sounds.push("beep01.vag");
                }
            }
            self.countdown = if g.phase == 0 {
                g.timer.clamp(0., 10.)
            } else {
                0.
            };
            session.mirror(g.phase, g.clock);
            if !was_started && session.started {
                self.go = 1.;
                self.sounds.push("go.vag");
            }
            let before: i32 = session.rules.scores().iter().sum();
            let owners: Vec<Option<usize>> =
                session.rules.marks_state().iter().map(|m| m.0).collect();
            let marks: Vec<(Option<usize>, f32)> = g
                .marks
                .marks
                .iter()
                .map(|(o, v)| ((*o != NO_PEER).then_some(*o as usize), *v))
                .collect();
            let scores: Vec<i32> = g.scores.iter().map(|s| *s as i32).collect();
            session
                .rules
                .apply_marks(&marks, &scores, if g.phase == 1 { g.timer } else { 0. });
            if self.kind == GameKind::TreasureHunt
                && session.rules.scores().iter().sum::<i32>() > before
            {
                self.sounds.push("iconhit.vag");
            }
            // The capture sign for a hill that changed hands at the host.
            if self.kind == GameKind::KingOfTheHills {
                for (i, (owner, _)) in marks.iter().enumerate() {
                    if let Some(o) = owner
                        && owners.get(i).copied().flatten() != Some(*o)
                    {
                        session.notice = Some(atv_core::games::Notice {
                            text: format!("ATV {} captured hill {}", o + 1, i + 1),
                            seconds: atv_core::games::king_of_the_hill::CAPTURE_SIGN,
                        });
                    }
                }
            }
            for (slot, s) in g.marks.stunts.iter().enumerate() {
                if slot != me && slot < session.stunt.len() {
                    session.stunt[slot] = *s;
                }
            }
        }
        if session.started {
            self.go = (self.go - dt).max(0.);
        }
        if me < session.stunt.len() {
            session.stunt[me] = self.committed[me];
        }
        // King of the Hills: this rider's own arming against the mirrored
        // hills; a capture is reported for the host to confirm.
        if self.kind == GameKind::KingOfTheHills && session.started && session.end.is_none() {
            let mut riders = vec![
                Rider {
                    position: NOWHERE,
                    grounded: true,
                    ..Default::default()
                };
                SLOTS
            ];
            riders[me] = own;
            let before = session.rules.marks_state();
            session.rules.update(dt, &riders);
            let after = session.rules.marks_state();
            for (i, (b, a)) in before.iter().zip(&after).enumerate() {
                if a.0 == Some(me) && (b.0 != Some(me) || a.1 != b.1) {
                    let key = (i + 1, a.1.to_bits());
                    if room.sent_claim != Some(key) {
                        room.sent_claim = Some(key);
                        net.send_claim(GameClaim {
                            peer: me as u8,
                            serial: room.serial,
                            kind: claim_kind::CAPTURE,
                            index: (i + 1) as u16,
                            points: a.1.clamp(0., MAX_GAME_POINTS),
                            stunt: self.committed[me].clamp(0., MAX_GAME_POINTS),
                        });
                    }
                }
            }
        }
        room.report += dt;
        if room.report >= STUNT_REPORT && state.is_some() {
            room.report = 0.;
            net.send_claim(GameClaim {
                peer: me as u8,
                serial: room.serial,
                kind: claim_kind::STUNT,
                index: 0,
                points: 0.,
                stunt: self.committed[me].clamp(0., MAX_GAME_POINTS),
            });
        }
    }

    /// The room's results (present riders only, by count then stunt).
    pub fn room_results(&mut self) -> Option<String> {
        let session = self.session.as_ref()?;
        let room = self.room.as_ref()?;
        if self.reported || !session.over() {
            return None;
        }
        self.reported = true;
        let end = match session.end {
            Some(atv_core::games::End::Complete) => "All Treasures Collected",
            _ => "Time Up",
        };
        let rows: Vec<String> = self
            .room_standings()
            .iter()
            .enumerate()
            .map(|(place, s)| {
                format!(
                    "{}. {}: {} {}, Stunt Score {}.",
                    place + 1,
                    room.names[s.rider],
                    self.kind.score_label(),
                    s.score,
                    atv_ui::hud::objective::grouped(s.stunt as i64)
                )
            })
            .collect();
        let summary = format!(
            "{} on {}: {end}. {}",
            self.kind.name(),
            self.course,
            rows.join(" ")
        );
        self.summary = Some(summary.clone());
        Some(summary)
    }

    /// Standings over the room's present riders.
    pub fn room_standings(&self) -> Vec<atv_core::games::Standing> {
        let (Some(session), Some(room)) = (&self.session, &self.room) else {
            return Vec::new();
        };
        session
            .standings()
            .into_iter()
            .filter(|s| room.present.get(s.rider).copied().unwrap_or(false))
            .collect()
    }

    /// The room diagnostics for browser tests.
    pub fn room_state(&self) -> serde_json::Value {
        let Some(room) = &self.room else {
            return serde_json::Value::Null;
        };
        serde_json::json!({
            "host": room.host,
            "serial": room.serial,
            "me": room.me,
            "present": room.present,
            "accepted": room.accepted,
            "refused": room.refused,
            "standings": self.room_standings(),
            "phase": self.session.as_ref().map(atv_core::games::Session::phase),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(peer: u8, index: u16, points: f32) -> GameClaim {
        GameClaim {
            peer,
            serial: 7,
            kind: claim_kind::CAPTURE,
            index,
            points,
            stunt: 0.,
        }
    }

    #[test]
    fn capture_claims_are_checked_against_the_game_room_and_hill() {
        let present = [true, true, false, false];
        let hills = [[0., 0., 0.], [1000., 0., 0.]];
        let near = Some([10., 0., 10.]);
        assert!(check_claim(&claim(1, 1, 500.), 7, &present, &hills, near, 1.).is_ok());
        assert_eq!(
            check_claim(
                &GameClaim {
                    serial: 6,
                    ..claim(1, 1, 500.)
                },
                7,
                &present,
                &hills,
                near,
                1.
            ),
            Err("another game")
        );
        assert_eq!(
            check_claim(&claim(2, 1, 500.), 7, &present, &hills, near, 1.),
            Err("not in the game")
        );
        assert_eq!(
            check_claim(&claim(1, 0, 500.), 7, &present, &hills, near, 1.),
            Err("no such hill")
        );
        assert_eq!(
            check_claim(&claim(1, 3, 500.), 7, &present, &hills, near, 1.),
            Err("no such hill")
        );
        assert_eq!(
            check_claim(&claim(1, 2, 500.), 7, &present, &hills, near, 1.),
            Err("too far from the hill")
        );
        assert_eq!(
            check_claim(&claim(1, 1, 0.), 7, &present, &hills, near, 1.),
            Err("points out of range")
        );
        assert_eq!(
            check_claim(&claim(1, 1, 500.), 7, &present, &hills, near, 0.1),
            Err("too soon")
        );
        assert_eq!(
            check_claim(&claim(1, 1, 500.), 7, &present, &hills, None, 1.),
            Err("no position")
        );
        // A stunt report needs only the game and the room.
        let report = GameClaim {
            kind: claim_kind::STUNT,
            index: 0,
            points: 0.,
            ..claim(1, 0, 0.)
        };
        assert!(check_claim(&report, 7, &present, &hills, None, 0.).is_ok());
    }

    #[test]
    fn venue_codes_cover_the_game_course_lists() {
        for v in VENUES {
            assert_eq!(VENUES[venue_code(v).unwrap() as usize], v);
        }
        assert_eq!(venue_code("nat01"), None);
        assert_eq!(code(GameKind::Tag), None);
        assert_eq!(
            kind(code(GameKind::TreasureHunt).unwrap()),
            Some(GameKind::TreasureHunt)
        );
    }
}
