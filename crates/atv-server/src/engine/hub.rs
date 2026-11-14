//! The Free Ride hub of one shared room (research/FREERIDE_HUB.md):
//! challenges between two nearby riders, pop-up events and the map
//! rotation vote. Authored for the port; nothing here is recovered behavior.
//!
//! Only riders whose HELLO said [`signal::HUB`] take part or receive hub
//! packets; older builds ride on unchanged (a rotation moves them with the
//! room through the ordinary `Lobby`). The server owns every timer and
//! judges sprints and hills from the riders' own poses at server time.
//! Waypoints are spots riders of this room actually rode through, so they
//! lie on the course without the server knowing its terrain.
//!
//! Pure like the room: the caller passes the time and the riders, and
//! delivers the packets pushed to `out`.

use std::sync::OnceLock;

use atv_net::{
    Challenge, Hub as HubPacket, HubRow, MAX_PLAYERS, NO_PEER, Packet, challenge_end, challenge_op,
    event_kind, event_phase, hub_op,
};
use bytes::Bytes;

use super::{ConnId, Out};

/// The challenge button shows within this distance (ft) of another rider.
pub const CHALLENGE_RANGE: f32 = 100.;
/// The server accepts an ask this far apart: the riders' poses lag.
const ASK_RANGE: f32 = 160.;
/// The challenged rider answers within this long.
pub const OFFER_MS: u64 = 8_000;
/// A pair waits this long after a challenge before another.
pub const COOLDOWN_MS: u64 = 60_000;
/// A sprint ends within this many feet of its waypoint (horizontal).
pub const REACH: f32 = 30.;
/// Sprint waypoints lie this far from the riders' midpoint, preferred.
const SPRINT_NEAR: f32 = 300.;
const SPRINT_FAR: f32 = 900.;
/// A pose implying more than this speed (ft/s) since the last is not
/// believed for a finish (a teleport or a forged position).
const MAX_SPEED: f32 = 250.;
/// The join circle (ft).
pub const JOIN_RADIUS: f32 = 40.;
/// The hill's radius (ft) and height band.
pub const HILL_RADIUS: f32 = 35.;
const HILL_BAND: f32 = 30.;
/// Each event runs this long at most.
pub const EVENT_MS: u64 = 60_000;
/// How long the vote stays open before the room moves.
pub const VOTE_MS: u64 = 60_000;
/// No event starts this close before a rotation.
const EVENT_QUIET_MS: u64 = 120_000;
/// Hub work runs at most this often (the room ticks at 20 Hz).
const STEP_MS: u64 = 100;
/// Live standings go out this often while an event runs.
const STANDINGS_MS: u64 = 1_000;
/// Spots kept per room, and how often riders are sampled for them.
const SPOTS: usize = 512;
const SPOT_MS: u64 = 2_000;
/// Free roam venues the rotation offers: the eight Environment venues
/// (`env01`-`env08`, map ids 1-8; research/FREERIDE_HUB.md "Venues").
pub const ROTATION_MAPS: [u16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
/// Stunt-off reports above this are refused.
const MAX_STUNT_POINTS: f32 = 10_000_000.;
/// Results points by place, and for taking part.
const PLACE_POINTS: [u16; 3] = [100, 60, 30];
const PLAYED_POINTS: u16 = 10;

/// Hub timers (`FURY_HUB_*`, research/FREERIDE_HUB.md "Settings").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HubConfig {
    /// A pop-up event every this many ms (inclusive range); None: none.
    pub events: Option<(u64, u64)>,
    /// The event join window.
    pub join_ms: u64,
    /// The room moves to a voted venue every this many ms; None: never.
    pub rotation: Option<(u64, u64)>,
}

impl Default for HubConfig {
    fn default() -> Self {
        HubConfig {
            events: Some((4 * 60_000, 6 * 60_000)),
            join_ms: 45_000,
            rotation: Some((20 * 60_000, 25 * 60_000)),
        }
    }
}

static CONFIG: OnceLock<HubConfig> = OnceLock::new();

/// Sets the server's hub timers once, at start.
pub fn configure(config: HubConfig) {
    let _ = CONFIG.set(config);
}

fn config() -> HubConfig {
    CONFIG.get().copied().unwrap_or_default()
}

/// One joined rider as the hub sees it this step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rider {
    pub peer: u8,
    pub conn: ConnId,
    /// Its build takes hub packets.
    pub hub: bool,
    /// Its newest pose on the room's map, when it has one.
    pub position: Option<[f32; 3]>,
    /// Moving (not at rest), for spot sampling.
    pub moving: bool,
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

fn bit(peer: u8) -> u32 {
    1 << peer
}

/// A rider's finish tracking: the last believed position and when.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Track {
    at: [f32; 3],
    ms: u64,
}

impl Track {
    /// Takes a new position; false when the rider could not have got
    /// there from the last believed one at [`MAX_SPEED`], which then stays
    /// the reference (a jump counts only once that speed could cover it).
    fn advance(&mut self, position: [f32; 3], now: u64) -> bool {
        let dt = now.saturating_sub(self.ms).max(1) as f32 / 1000.;
        let d = (0..3)
            .map(|i| (position[i] - self.at[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        let believed = d / dt <= MAX_SPEED || d < REACH;
        if believed {
            self.at = position;
            self.ms = now;
        }
        believed
    }
}

#[derive(Clone, Debug, PartialEq)]
enum DuelPhase {
    Offer {
        until: u64,
    },
    Race {
        target: [f32; 3],
        until: u64,
        tracks: [Option<Track>; 2],
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Duel {
    serial: u32,
    from: u8,
    to: u8,
    phase: DuelPhase,
}

impl Duel {
    fn has(&self, peer: u8) -> bool {
        self.from == peer || self.to == peer
    }
}

/// One rider's standing in an event.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Entry {
    peer: u8,
    /// Sprint: seconds at the finish; Hill: seconds held; Stunts: points.
    value: f32,
    finished: bool,
    /// Sprint: the nearest it came to the target (ft), for unfinished order.
    best: f32,
    track: Option<Track>,
}

#[derive(Clone, Debug, PartialEq)]
struct Event {
    serial: u32,
    kind: u8,
    phase: u8,
    point: [f32; 3],
    target: [f32; 3],
    joined: u32,
    /// End of the current phase.
    until: u64,
    started: u64,
    solo: bool,
    entries: Vec<Entry>,
    standings_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct Vote {
    serial: u32,
    maps: Vec<u16>,
    /// Each slot's choice.
    choices: [u8; MAX_PLAYERS],
    until: u64,
}

pub struct Hub {
    rng: u64,
    serial: u32,
    last_step: u64,
    spots: Vec<[f32; 3]>,
    spot_at: u64,
    duels: Vec<Duel>,
    /// Pairs (lower slot, higher slot) and when they may duel again.
    cooldowns: Vec<(u8, u8, u64)>,
    event: Option<Event>,
    next_event: u64,
    next_kind: u8,
    vote: Option<Vote>,
    next_rotation: Option<u64>,
    /// Hub riders as last announced, and riders sent the current state.
    riders: u32,
    greeted: u32,
}

/// A step's result for the room.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    pub map: Option<u16>,
}

impl Hub {
    pub fn new(seed: u64, now: u64) -> Self {
        let config = config();
        let mut hub = Hub {
            rng: seed | 1,
            serial: 0,
            last_step: 0,
            spots: Vec::new(),
            spot_at: 0,
            duels: Vec::new(),
            cooldowns: Vec::new(),
            event: None,
            next_event: u64::MAX,
            next_kind: 0,
            vote: None,
            next_rotation: None,
            riders: 0,
            greeted: 0,
        };
        hub.next_kind = (hub.random() % 3) as u8;
        if let Some(range) = config.events {
            hub.next_event = now + hub.between(range);
        }
        hub.next_rotation = config.rotation.map(|range| now + hub.between(range));
        hub
    }

    /// Tests: the next rotation is due at `at`.
    #[cfg(test)]
    pub(crate) fn rotate_at(&mut self, at: u64) {
        self.next_rotation = Some(at);
    }

    fn random(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn between(&mut self, (low, high): (u64, u64)) -> u64 {
        low + self.random() % (high.saturating_sub(low) + 1)
    }

    fn next_serial(&mut self) -> u32 {
        self.serial = self.serial.wrapping_add(1).max(1);
        self.serial
    }

    fn send(conn: ConnId, packet: &Packet, out: &mut Vec<Out>) {
        if let Ok(bytes) = atv_net::encode(packet) {
            out.push(Out::Send(conn, Bytes::from(bytes)));
        }
    }

    fn to_hub(riders: &[Rider], packet: &Packet, out: &mut Vec<Out>) {
        let Ok(bytes) = atv_net::encode(packet) else {
            return;
        };
        let bytes = Bytes::from(bytes);
        for r in riders.iter().filter(|r| r.hub) {
            out.push(Out::Send(r.conn, bytes.clone()));
        }
    }

    fn rider(riders: &[Rider], peer: u8) -> Option<&Rider> {
        riders.iter().find(|r| r.peer == peer && r.hub)
    }

    /// In a duel or an event.
    fn busy(&self, peer: u8) -> bool {
        self.duels.iter().any(|d| d.has(peer))
            || self
                .event
                .as_ref()
                .is_some_and(|e| e.phase == event_phase::RUN && e.joined & bit(peer) != 0)
    }

    fn cooling(&self, a: u8, b: u8, now: u64) -> bool {
        let pair = (a.min(b), a.max(b));
        self.cooldowns
            .iter()
            .any(|&(x, y, until)| (x, y) == pair && now < until)
    }

    fn cool(&mut self, a: u8, b: u8, now: u64) {
        let pair = (a.min(b), a.max(b));
        self.cooldowns
            .retain(|&(x, y, until)| (x, y) != pair && now < until);
        self.cooldowns.push((pair.0, pair.1, now + COOLDOWN_MS));
    }

    /// A rider's challenge message.
    pub fn challenge(
        &mut self,
        from: u8,
        message: Challenge,
        riders: &[Rider],
        now: u64,
        out: &mut Vec<Out>,
    ) {
        let Some(me) = Self::rider(riders, from).copied() else {
            return;
        };
        let closed = |serial, peer, detail| {
            Packet::Challenge(Challenge {
                op: challenge_op::CLOSED,
                serial,
                peer,
                detail,
                ..Default::default()
            })
        };
        match message.op {
            challenge_op::ASK => {
                let to = message.peer;
                let target = Self::rider(riders, to).copied();
                let near = match (me.position, target.and_then(|t| t.position)) {
                    (Some(a), Some(b)) => flat(a, b) <= ASK_RANGE,
                    _ => false,
                };
                let refusal = if to == from || target.is_none() || !near {
                    Some(challenge_end::UNAVAILABLE)
                } else if self.busy(from) || self.busy(to) || self.vote.is_some() {
                    Some(challenge_end::BUSY)
                } else if self.cooling(from, to, now) {
                    Some(challenge_end::COOLDOWN)
                } else {
                    None
                };
                if let Some(detail) = refusal {
                    return Self::send(me.conn, &closed(0, to, detail), out);
                }
                let serial = self.next_serial();
                self.duels.push(Duel {
                    serial,
                    from,
                    to,
                    phase: DuelPhase::Offer {
                        until: now + OFFER_MS,
                    },
                });
                // Asked or not, the pair cools down: no repeated asking.
                self.cool(from, to, now);
                let offer = |op, peer| {
                    Packet::Challenge(Challenge {
                        op,
                        serial,
                        peer,
                        timer_ms: OFFER_MS as u32,
                        ..Default::default()
                    })
                };
                Self::send(me.conn, &offer(challenge_op::WAIT, to), out);
                if let Some(t) = target {
                    Self::send(t.conn, &offer(challenge_op::OFFER, from), out);
                }
            }
            challenge_op::ACCEPT | challenge_op::DECLINE => {
                let Some(i) = self.duels.iter().position(|d| {
                    d.serial == message.serial
                        && d.to == from
                        && matches!(d.phase, DuelPhase::Offer { .. })
                }) else {
                    return;
                };
                if message.op == challenge_op::DECLINE {
                    let duel = self.duels.remove(i);
                    self.cool(duel.from, duel.to, now);
                    for (peer, other) in [(duel.from, duel.to), (duel.to, duel.from)] {
                        if let Some(r) = Self::rider(riders, peer) {
                            Self::send(
                                r.conn,
                                &closed(duel.serial, other, challenge_end::DECLINED),
                                out,
                            );
                        }
                    }
                    return;
                }
                self.start_duel(i, riders, now, out);
            }
            _ => {}
        }
    }

    fn start_duel(&mut self, i: usize, riders: &[Rider], now: u64, out: &mut Vec<Out>) {
        let (from, to) = (self.duels[i].from, self.duels[i].to);
        let positions = (
            Self::rider(riders, from).and_then(|r| r.position),
            Self::rider(riders, to).and_then(|r| r.position),
        );
        let (Some(a), Some(b)) = positions else {
            return self.end_duel(i, riders, None, out);
        };
        let middle = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2., (a[2] + b[2]) / 2.];
        let target = self.waypoint(middle, SPRINT_NEAR, SPRINT_FAR);
        let timer = sprint_ms(flat(middle, target));
        self.duels[i].phase = DuelPhase::Race {
            target,
            until: now + timer,
            tracks: [
                Some(Track { at: a, ms: now }),
                Some(Track { at: b, ms: now }),
            ],
        };
        let serial = self.duels[i].serial;
        for (peer, other) in [(from, to), (to, from)] {
            if let Some(r) = Self::rider(riders, peer) {
                Self::send(
                    r.conn,
                    &Packet::Challenge(Challenge {
                        op: challenge_op::START,
                        serial,
                        peer: other,
                        timer_ms: timer as u32,
                        target,
                        detail: 0,
                    }),
                    out,
                );
            }
        }
    }

    /// Ends duel `i`: `winner` Some(peer or NO_PEER) sends a result, None
    /// a close because one rider left.
    fn end_duel(&mut self, i: usize, riders: &[Rider], winner: Option<u8>, out: &mut Vec<Out>) {
        let duel = self.duels.remove(i);
        let raced = matches!(duel.phase, DuelPhase::Race { .. });
        for (peer, other) in [(duel.from, duel.to), (duel.to, duel.from)] {
            let Some(r) = Self::rider(riders, peer) else {
                continue;
            };
            let message = match winner {
                Some(winner) if raced => Challenge {
                    op: challenge_op::RESULT,
                    serial: duel.serial,
                    peer: other,
                    detail: winner,
                    ..Default::default()
                },
                Some(_) => Challenge {
                    op: challenge_op::CLOSED,
                    serial: duel.serial,
                    peer: other,
                    detail: challenge_end::EXPIRED,
                    ..Default::default()
                },
                None => Challenge {
                    op: challenge_op::CLOSED,
                    serial: duel.serial,
                    peer: other,
                    detail: challenge_end::LEFT,
                    ..Default::default()
                },
            };
            Self::send(r.conn, &Packet::Challenge(message), out);
        }
    }

    /// A spot `near`-`far` ft from `from` (horizontally) that a rider of
    /// this room rode through, else the farthest one at least half `near`
    /// away, else a point `near` ft off in a random direction.
    fn waypoint(&mut self, from: [f32; 3], near: f32, far: f32) -> [f32; 3] {
        let start = self.random() as usize;
        let n = self.spots.len();
        let mut farthest: Option<([f32; 3], f32)> = None;
        for k in 0..n {
            let spot = self.spots[(start + k) % n];
            let d = flat(spot, from);
            if (near..=far).contains(&d) {
                return spot;
            }
            if d >= near / 2. && d <= far && farthest.is_none_or(|(_, best)| d > best) {
                farthest = Some((spot, d));
            }
        }
        if let Some((spot, _)) = farthest {
            return spot;
        }
        let angle = (self.random() % 3600) as f32 / 3600. * std::f32::consts::TAU;
        [
            from[0] + near * angle.cos(),
            from[1],
            from[2] + near * angle.sin(),
        ]
    }

    /// The highest spot within `range` ft of `from`, else `from`.
    fn hill(&self, from: [f32; 3], range: f32) -> [f32; 3] {
        self.spots
            .iter()
            .filter(|s| flat(**s, from) <= range)
            .copied()
            .fold(from, |best, s| if s[1] > best[1] { s } else { best })
    }

    /// A rider's hub message.
    pub fn message(
        &mut self,
        from: u8,
        message: HubPacket,
        riders: &[Rider],
        now: u64,
        out: &mut Vec<Out>,
    ) {
        match message.op {
            hub_op::SCORE => {
                let Some(event) = &mut self.event else {
                    return;
                };
                if event.serial != message.serial
                    || event.kind != event_kind::STUNTS
                    || event.phase != event_phase::RUN
                    || message.value > MAX_STUNT_POINTS
                {
                    return;
                }
                if let Some(entry) = event.entries.iter_mut().find(|e| e.peer == from) {
                    // Points only add.
                    entry.value = entry.value.max(message.value);
                }
            }
            hub_op::CAST => {
                let Some(vote) = &mut self.vote else {
                    return;
                };
                if vote.serial != message.serial || usize::from(message.choice) >= vote.maps.len() {
                    return;
                }
                vote.choices[usize::from(from)] = message.choice;
                let packet = self.vote_packet(now, atv_net::hub::NO_CHOICE);
                Self::to_hub(riders, &packet, out);
            }
            _ => {}
        }
    }

    fn vote_packet(&self, now: u64, choice: u8) -> Packet {
        let Some(vote) = &self.vote else {
            return Packet::Hub(Box::new(HubPacket {
                op: hub_op::VOTE,
                choice,
                ..Default::default()
            }));
        };
        let votes = (0..vote.maps.len() as u8)
            .map(|i| vote.choices.iter().filter(|&&c| c == i).count() as u8)
            .collect();
        Packet::Hub(Box::new(HubPacket {
            op: hub_op::VOTE,
            serial: vote.serial,
            timer_ms: vote.until.saturating_sub(now) as u32,
            choice,
            maps: vote.maps.clone(),
            votes,
            ..Default::default()
        }))
    }

    fn event_packet(event: &Event, now: u64) -> Packet {
        let mut rows: Vec<HubRow> = event
            .entries
            .iter()
            .map(|e| HubRow {
                peer: e.peer,
                value: e.value.clamp(0., atv_net::hub::MAX_HUB_VALUE),
                points: 0,
                finished: e.finished,
            })
            .collect();
        rank(event.kind, &event.entries, &mut rows);
        if event.phase == event_phase::RESULTS {
            award(event.solo, event.kind, &mut rows);
        }
        Packet::Hub(Box::new(HubPacket {
            op: hub_op::EVENT,
            serial: event.serial,
            kind: event.kind,
            phase: event.phase,
            timer_ms: event.until.saturating_sub(now) as u32,
            point: event.point,
            target: event.target,
            radius: if event.kind == event_kind::HILL && event.phase == event_phase::RUN {
                HILL_RADIUS
            } else {
                JOIN_RADIUS
            },
            mask: event.joined,
            choice: if event.solo { atv_net::hub::SOLO } else { 0 },
            rows,
            ..Default::default()
        }))
    }

    /// Whether [`Hub::step`] would do anything now.
    pub fn due(&self, now: u64) -> bool {
        now.saturating_sub(self.last_step) >= STEP_MS
    }

    /// The hub's periodic work, at most every [`STEP_MS`]. Returns the map
    /// the room moves to when a rotation vote ends.
    pub fn step(&mut self, now: u64, map: u16, riders: &[Rider], out: &mut Vec<Out>) -> Moved {
        if now.saturating_sub(self.last_step) < STEP_MS {
            return Moved::default();
        }
        let dt = now.saturating_sub(self.last_step).min(1000);
        self.last_step = now;
        self.greet(riders, now, out);
        self.sample(riders, now);
        self.step_duels(riders, now, out);
        self.step_event(riders, now, dt, out);
        self.step_rotation(map, riders, now, out)
    }

    /// Hub riders learn who else takes challenges, and a newcomer the
    /// event or vote in progress.
    fn greet(&mut self, riders: &[Rider], now: u64, out: &mut Vec<Out>) {
        let hub = riders
            .iter()
            .filter(|r| r.hub)
            .fold(0, |mask, r| mask | bit(r.peer));
        if hub != self.riders {
            self.riders = hub;
            Self::to_hub(
                riders,
                &Packet::Hub(Box::new(HubPacket {
                    op: hub_op::RIDERS,
                    mask: hub,
                    ..Default::default()
                })),
                out,
            );
        }
        let newcomers = hub & !self.greeted;
        self.greeted = hub;
        for r in riders
            .iter()
            .filter(|r| r.hub && newcomers & bit(r.peer) != 0)
        {
            if let Some(event) = &self.event {
                Self::send(r.conn, &Self::event_packet(event, now), out);
            }
            if self.vote.is_some() {
                Self::send(r.conn, &self.vote_packet(now, atv_net::hub::NO_CHOICE), out);
            }
        }
    }

    fn sample(&mut self, riders: &[Rider], now: u64) {
        if now.saturating_sub(self.spot_at) < SPOT_MS {
            return;
        }
        self.spot_at = now;
        for r in riders.iter().filter(|r| r.moving) {
            let Some(p) = r.position else {
                continue;
            };
            if self.spots.len() < SPOTS {
                self.spots.push(p);
            } else {
                let i = self.random() as usize % SPOTS;
                self.spots[i] = p;
            }
        }
    }

    fn step_duels(&mut self, riders: &[Rider], now: u64, out: &mut Vec<Out>) {
        let mut i = 0;
        while i < self.duels.len() {
            let duel = &mut self.duels[i];
            let at = |peer| Self::rider(riders, peer).and_then(|r| r.position);
            let (Some(a), Some(b)) = (at(duel.from), at(duel.to)) else {
                self.end_duel(i, riders, None, out);
                continue;
            };
            // Some(winner or NO_PEER) ends the duel.
            let outcome = match &mut duel.phase {
                DuelPhase::Offer { until } => (now >= *until).then_some(NO_PEER),
                DuelPhase::Race {
                    target,
                    until,
                    tracks,
                } => {
                    let mut reached = [false; 2];
                    let mut gap = [0f32; 2];
                    for (k, p) in [a, b].into_iter().enumerate() {
                        let believed = tracks[k].as_mut().is_none_or(|t| t.advance(p, now));
                        gap[k] = flat(p, *target);
                        reached[k] = believed && gap[k] <= REACH;
                    }
                    match reached {
                        [true, false] => Some(duel.from),
                        [false, true] => Some(duel.to),
                        // Both in one step: the nearer one.
                        [true, true] => Some(if gap[0] <= gap[1] { duel.from } else { duel.to }),
                        [false, false] if now >= *until => Some(NO_PEER),
                        _ => None,
                    }
                }
            };
            if let Some(winner) = outcome {
                let (from, to) = (duel.from, duel.to);
                self.cool(from, to, now);
                self.end_duel(i, riders, Some(winner), out);
                continue;
            }
            i += 1;
        }
    }

    fn step_event(&mut self, riders: &[Rider], now: u64, dt: u64, out: &mut Vec<Out>) {
        let config = config();
        let Some(mut event) = self.event.take() else {
            let quiet = self
                .next_rotation
                .is_some_and(|at| now + EVENT_QUIET_MS + config.join_ms >= at);
            if now >= self.next_event && self.vote.is_none() && !quiet {
                self.announce(riders, now, out);
            } else if now >= self.next_event {
                // Postponed past the move.
                self.next_event = now + 30_000;
            }
            return;
        };
        let mut changed = false;
        match event.phase {
            event_phase::JOIN => {
                for r in riders.iter().filter(|r| r.hub) {
                    let inside = r
                        .position
                        .is_some_and(|p| flat(p, event.point) <= JOIN_RADIUS);
                    if inside
                        && event.joined & bit(r.peer) == 0
                        && !self.duels.iter().any(|d| d.has(r.peer))
                    {
                        event.joined |= bit(r.peer);
                        changed = true;
                    }
                }
                if now >= event.until {
                    let joined: Vec<&Rider> = riders
                        .iter()
                        .filter(|r| r.hub && event.joined & bit(r.peer) != 0)
                        .collect();
                    if joined.is_empty() {
                        event.phase = event_phase::CANCELLED;
                    } else {
                        event.phase = event_phase::RUN;
                        event.solo = joined.len() == 1;
                        event.started = now;
                        event.until = now + EVENT_MS;
                        event.entries = joined
                            .iter()
                            .map(|r| Entry {
                                peer: r.peer,
                                best: f32::INFINITY,
                                track: r.position.map(|at| Track { at, ms: now }),
                                ..Default::default()
                            })
                            .collect();
                        event.joined = joined.iter().fold(0, |m, r| m | bit(r.peer));
                        event.target = match event.kind {
                            event_kind::SPRINT => self.waypoint(event.point, 400., 1000.),
                            event_kind::HILL => self.hill(event.point, 500.),
                            _ => event.point,
                        };
                        if event.kind == event_kind::SPRINT {
                            event.until = now + sprint_ms(flat(event.point, event.target));
                        }
                        event.standings_at = now;
                    }
                    changed = true;
                }
            }
            event_phase::RUN => {
                // Riders who left drop out of the standings.
                let before = event.entries.len();
                event
                    .entries
                    .retain(|e| Self::rider(riders, e.peer).is_some_and(|r| r.position.is_some()));
                changed |= event.entries.len() != before;
                let seconds = (now.saturating_sub(event.started)) as f32 / 1000.;
                for entry in &mut event.entries {
                    let Some(p) = Self::rider(riders, entry.peer).and_then(|r| r.position) else {
                        continue;
                    };
                    let believed = entry.track.as_mut().is_none_or(|t| t.advance(p, now));
                    match event.kind {
                        event_kind::SPRINT if !entry.finished => {
                            let gap = flat(p, event.target);
                            entry.best = entry.best.min(gap);
                            if believed && gap <= REACH {
                                entry.finished = true;
                                entry.value = seconds;
                                changed = true;
                            }
                        }
                        event_kind::HILL => {
                            if believed
                                && flat(p, event.target) <= HILL_RADIUS
                                && (p[1] - event.target[1]).abs() <= HILL_BAND
                            {
                                entry.value += dt as f32 / 1000.;
                            }
                        }
                        _ => {}
                    }
                }
                let all_in =
                    event.kind == event_kind::SPRINT && event.entries.iter().all(|e| e.finished);
                if now >= event.until || all_in || event.entries.is_empty() {
                    event.phase = event_phase::RESULTS;
                    event.until = now;
                    changed = true;
                } else if now.saturating_sub(event.standings_at) >= STANDINGS_MS {
                    changed = true;
                }
            }
            _ => {}
        }
        if changed {
            event.standings_at = now;
            Self::to_hub(riders, &Self::event_packet(&event, now), out);
        }
        if event.phase >= event_phase::RESULTS {
            if let Some(range) = config.events {
                self.next_event = now + self.between(range);
            }
        } else {
            self.event = Some(event);
        }
    }

    /// Opens a join circle near a random hub rider.
    fn announce(&mut self, riders: &[Rider], now: u64, out: &mut Vec<Out>) {
        let placed: Vec<[f32; 3]> = riders
            .iter()
            .filter(|r| r.hub)
            .filter_map(|r| r.position)
            .collect();
        if placed.is_empty() {
            // Nobody to see it: try again shortly.
            self.next_event = now + 30_000;
            return;
        }
        let near = placed[self.random() as usize % placed.len()];
        let point = self.waypoint(near, 60., 250.);
        let kind = self.next_kind;
        self.next_kind = (self.next_kind + 1) % 3;
        let event = Event {
            serial: self.next_serial(),
            kind,
            phase: event_phase::JOIN,
            point,
            target: point,
            joined: 0,
            until: now + config().join_ms,
            started: now,
            solo: false,
            entries: Vec::new(),
            standings_at: now,
        };
        Self::to_hub(riders, &Self::event_packet(&event, now), out);
        self.event = Some(event);
    }

    fn step_rotation(&mut self, map: u16, riders: &[Rider], now: u64, out: &mut Vec<Out>) -> Moved {
        let Some(at) = self.next_rotation else {
            return Moved::default();
        };
        if self.vote.is_none() && now + VOTE_MS >= at {
            // Three venues other than this one.
            let mut pool: Vec<u16> = ROTATION_MAPS
                .iter()
                .copied()
                .filter(|&m| m != map)
                .collect();
            let mut maps = Vec::with_capacity(3);
            while maps.len() < atv_net::hub::MAX_VOTE_MAPS && !pool.is_empty() {
                let i = self.random() as usize % pool.len();
                maps.push(pool.swap_remove(i));
            }
            self.vote = Some(Vote {
                serial: self.next_serial(),
                maps,
                choices: [atv_net::hub::NO_CHOICE; MAX_PLAYERS],
                until: at.max(now + 1),
            });
            let packet = self.vote_packet(now, atv_net::hub::NO_CHOICE);
            Self::to_hub(riders, &packet, out);
            return Moved::default();
        }
        let Some(vote) = &self.vote else {
            return Moved::default();
        };
        if now < vote.until {
            return Moved::default();
        }
        // Votes of riders still here; ties and no votes pick at random.
        let present = riders.iter().fold(0u32, |m, r| m | bit(r.peer));
        let counts: Vec<usize> = (0..vote.maps.len() as u8)
            .map(|i| {
                (0..MAX_PLAYERS as u8)
                    .filter(|&p| present & bit(p) != 0 && vote.choices[usize::from(p)] == i)
                    .count()
            })
            .collect();
        let top = counts.iter().copied().max().unwrap_or(0);
        let tied: Vec<u8> = (0..counts.len() as u8)
            .filter(|&i| counts[usize::from(i)] == top)
            .collect();
        let maps = vote.maps.clone();
        let choice = tied[self.random() as usize % tied.len()];
        let destination = maps[usize::from(choice)];
        let packet = self.vote_packet(now, choice);
        Self::to_hub(riders, &packet, out);
        self.vote = None;
        self.next_rotation = config().rotation.map(|range| now + self.between(range));
        // The room moves together: everything in progress ends.
        if let Some(mut event) = self.event.take() {
            event.phase = event_phase::CANCELLED;
            Self::to_hub(riders, &Self::event_packet(&event, now), out);
        }
        while !self.duels.is_empty() {
            self.end_duel(0, riders, None, out);
        }
        self.spots.clear();
        if let Some(range) = config().events {
            self.next_event = now + self.between(range);
        }
        Moved {
            map: Some(destination),
        }
    }
}

/// A sprint's time limit for a waypoint `distance` ft away: 15 s plus
/// 35 ft/s, within 30-60 s.
fn sprint_ms(distance: f32) -> u64 {
    let seconds = (15. + distance / 35.).clamp(30., 60.);
    (seconds * 1000.) as u64
}

/// Orders `rows` best first: finished sprinters by time, then the rest by
/// how near they came; hills and stunts by value.
fn rank(kind: u8, entries: &[Entry], rows: &mut [HubRow]) {
    let best = |peer: u8| {
        entries
            .iter()
            .find(|e| e.peer == peer)
            .map_or(f32::INFINITY, |e| e.best)
    };
    rows.sort_by(|a, b| {
        if kind == event_kind::SPRINT {
            b.finished
                .cmp(&a.finished)
                .then_with(|| {
                    if a.finished {
                        a.value.total_cmp(&b.value)
                    } else {
                        best(a.peer).total_cmp(&best(b.peer))
                    }
                })
                .then(a.peer.cmp(&b.peer))
        } else {
            b.value.total_cmp(&a.value).then(a.peer.cmp(&b.peer))
        }
    });
}

/// Hub points: 100/60/30 for the podium and 10 for taking part; a solo run
/// scores against itself.
fn award(solo: bool, kind: u8, rows: &mut [HubRow]) {
    for (place, row) in rows.iter_mut().enumerate() {
        row.points = if solo {
            match kind {
                event_kind::SPRINT if row.finished => 50,
                event_kind::SPRINT => PLAYED_POINTS,
                event_kind::HILL => (row.value as u16).clamp(PLAYED_POINTS, 60),
                _ => ((row.value / 1000.) as u16).clamp(PLAYED_POINTS, 100),
            }
        } else {
            PLACE_POINTS.get(place).copied().unwrap_or(PLAYED_POINTS)
        };
    }
}

#[cfg(test)]
mod tests;
