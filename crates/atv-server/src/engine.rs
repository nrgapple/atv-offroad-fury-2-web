//! One room as a pure, synchronous state machine (research/GAME_SERVER.md).
//!
//! The room behavior is the retired Cloudflare Durable Object relay's
//! (research/MULTIPLAYER.md): slots and rejoin keys, the 30-second hold, host
//! promotion, the lobby, host-only games and CPUs, race progress, claims,
//! pings with server time, rate limits and expiry. Poses are new: each is
//! decoded once and kept as encoded bytes, builds that said
//! [`signal::FRAMES`] get one frame per tick from [`Room::tick`], and the
//! others get each pose as it arrives, as from the relay. There are no
//! direct links, couriers or batches on the server.
//!
//! No I/O, clock or randomness of its own: the caller passes the time and an
//! [`Entropy`] source and delivers the [`Out`] actions it returns.

use atv_net::{
    Lobby, MAX_PLAYERS, PRIVATE_CAPACITY, PUBLIC_CAPACITY, PUBLIC_MAP, Packet, Participant,
    RoomCode, Snapshot, signal,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::interest::{self, Pose, Sent};

pub mod chat;

/// A dropped rider keeps their slot (and color) this long for reconnection.
pub const GRACE_MS: u64 = 30_000;
/// A connection that sends nothing for this long is closed (recoverably),
/// and a room empty this long ends.
pub const IDLE_MS: u64 = 60_000;
/// Private rooms end after this long; shared rooms live while anyone rides.
pub const MAX_AGE_MS: u64 = 12 * 60 * 60 * 1000;
/// Per-connection budget: 120 messages and 1 MiB a second on average, over
/// a 3 s window, so a client whose packets queued during a stall and arrive
/// together is not closed for the burst.
const RATE_WINDOW_MS: u64 = 3000;
const RATE_MESSAGES: u16 = 360;
const RATE_BYTES: usize = 3 * 1024 * 1024;
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Why a pose came through the server, as the relay counted it
/// ([`Counters::poses`] index). Without direct links only three apply: the
/// sender rode alone, its build predates links, or it has none.
pub const POSE_KINDS: [&str; 7] = [
    "alone",
    "presence",
    "senderOld",
    "senderNoLinks",
    "forOld",
    "forNoLinks",
    "forUnlinked",
];

/// One connection's handle, chosen by the caller.
pub type ConnId = u64;

/// What the caller must do after a call.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Send this packet.
    Send(ConnId, Bytes),
    /// Send this pose or frame; a slow connection may drop it for a newer one.
    Pose(ConnId, Bytes),
    /// Close the socket with this code and reason. The room has already
    /// forgotten the connection.
    Close(ConnId, u16, &'static str),
}

/// The random source for codes and keys.
pub trait Entropy {
    fn fill(&mut self, out: &mut [u8]);
}

/// A rejoin key: 32 lowercase hex digits.
pub fn token(rng: &mut dyn Entropy) -> String {
    let mut bytes = [0; 16];
    rng.fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh private room code without ambiguous glyphs.
pub fn room_code(rng: &mut dyn Entropy) -> RoomCode {
    let mut bytes = [0; atv_net::ROOM_CODE_BYTES];
    rng.fill(&mut bytes);
    bytes.map(|b| ALPHABET[usize::from(b & 31)])
}

/// A rejoin key as the 16 bytes of a `Welcome`.
pub fn key_bytes(key: &str) -> [u8; 16] {
    let mut out = [0; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = key
            .get(i * 2..i * 2 + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .unwrap_or(0);
    }
    out
}

/// A close the rider did not choose: its slot is held for it.
pub fn recoverable_close(code: u16, reason: &str) -> bool {
    matches!(code, 1001 | 1005 | 1006 | 1011 | 1012 | 1013)
        || (code == 1000 && reason == "Connection timed out")
}

/// A slot held for reconnection. A creator's or claimant's slot is held
/// without a player until its first connection arrives.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Reserved {
    pub peer: u8,
    #[serde(default)]
    pub player: Option<Participant>,
    pub key: String,
    pub since_ms: u64,
}

/// Lowest slot that is neither connected nor held for a reconnecting rider.
fn free_slot(taken: impl Iterator<Item = u8> + Clone, capacity: usize) -> Option<u8> {
    (0..capacity as u8).find(|id| taken.clone().all(|t| t != *id))
}

/// What a room has counted since the caller last took the counts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    pub messages: u64,
    pub bytes_in: u64,
    /// Accepted poses by [`POSE_KINDS`].
    pub poses: [u64; 7],
    /// Pose batches from older builds.
    pub batches: u64,
    pub frames: u64,
    /// Poses carried in frames.
    pub frame_poses: u64,
    /// Poses sent on their own (builds without frames, full snapshots,
    /// arrivals' catch-up).
    pub single_poses: u64,
}

/// A connected socket.
struct Conn {
    id: ConnId,
    peer: u8,
    key: String,
    player: Option<Participant>,
    /// Map this rider has loaded. Poses go only to and from the room's map.
    ready_map: u16,
    sequence: Option<u32>,
    /// Delta baseline: the newest accepted pose, decoded.
    baseline: Option<Snapshot>,
    pose: Option<Pose>,
    /// Announced room races (a serial-0 `Progress`).
    races: bool,
    /// Announced room King of the Hills / Treasure Hunt (a serial-0 `Claim`).
    games: bool,
    /// Announced room Freestyle (a serial-0 `Claim` of kind 2).
    freestyle: bool,
    /// Said HELLO.
    signals: bool,
    /// Decodes CPU sound cues.
    cpu_sounds: bool,
    /// Decodes frames.
    frames: bool,
    /// Takes Free Ride hub packets (shared rooms only).
    hub: bool,
    /// Decodes motorcycle poses ([`signal::MOTORCYCLES`]); builds without
    /// it are never sent one (they would reject the whole frame).
    motorcycles: bool,
    /// Takes quick chat, and its quick-chat budget.
    chat_on: bool,
    chat: atv_net::chat::Limits,
    last_seen_ms: u64,
    window_ms: u64,
    messages: u16,
    bytes: usize,
    /// What this rider last got of each other rider in frames, by slot.
    sent: [Option<Sent>; MAX_PLAYERS],
}

impl Conn {
    fn account(&mut self, now: u64, bytes: usize) -> bool {
        if now.saturating_sub(self.window_ms) >= RATE_WINDOW_MS {
            self.window_ms = now;
            self.messages = 0;
            self.bytes = 0;
        }
        self.messages = self.messages.saturating_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        self.last_seen_ms = now;
        self.messages <= RATE_MESSAGES && self.bytes <= RATE_BYTES
    }
    fn models(&self) -> Option<(u16, u16)> {
        self.player.as_ref().map(|p| (p.atv_model, p.rider_model))
    }
    /// Whether this build decodes `pose` (motorcycles need the capability).
    fn takes(&self, pose: &Pose) -> bool {
        self.motorcycles || !atv_net::is_motorcycle(pose.models.0)
    }
    /// The newest pose, when it shows the rider's current vehicle.
    fn current_pose(&self) -> Option<&Pose> {
        self.pose
            .as_ref()
            .filter(|p| Some(p.models) == self.models())
    }
}

/// Why a connection was refused (an HTTP status and message).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub message: &'static str,
}

/// A room as saved at shutdown: everything a rider needs to rejoin.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SavedRoom {
    pub code: String,
    pub public: bool,
    pub created_ms: u64,
    pub host: u8,
    pub epoch: u32,
    pub map: u16,
    pub started: bool,
    /// Connected riders and held slots, all held again on restore.
    pub reserved: Vec<Reserved>,
    /// Slots left out of the public counts.
    #[serde(default)]
    pub uncounted: Vec<u8>,
}

pub struct Room {
    code: RoomCode,
    public: bool,
    created_ms: u64,
    /// Current map authority. Passes to another rider when the host leaves.
    host: u8,
    epoch: u32,
    map: u16,
    started: bool,
    reserved: Vec<Reserved>,
    empty_since_ms: Option<u64>,
    closed: bool,
    conns: Vec<Conn>,
    counters: Counters,
    /// Slots whose riders connected from an uncounted address
    /// (`FURY_UNCOUNTED_IPS`): they play normally but stay out of the
    /// public rider counts.
    uncounted: Vec<u8>,
    /// Shared rooms' challenges, events and rotation (engine/hub.rs).
    hub: Option<hub::Hub>,
    /// Recent quick chat, for reports.
    chat_ring: chat::Ring,
}

/// A shared room's hub, seeded from its code and opening time.
fn new_hub(code: &RoomCode, public: bool, now: u64) -> Option<hub::Hub> {
    public.then(|| {
        let seed = code
            .iter()
            .fold(now, |s, &b| s.rotate_left(8) ^ u64::from(b));
        hub::Hub::new(seed, now)
    })
}

impl Room {
    /// A new room whose first slot is held for its creator (or first
    /// claimant) under `key`. Shared rooms ride Arlington from the start.
    pub fn new(code: RoomCode, key: String, public: bool, now: u64) -> Self {
        Room {
            code,
            public,
            created_ms: now,
            host: 0,
            epoch: 1,
            map: if public { PUBLIC_MAP } else { 1 },
            started: public,
            reserved: vec![Reserved {
                peer: 0,
                player: None,
                key,
                since_ms: now,
            }],
            empty_since_ms: None,
            closed: false,
            conns: Vec::new(),
            counters: Counters::default(),
            uncounted: Vec::new(),
            hub: new_hub(&code, public, now),
            chat_ring: Default::default(),
        }
    }

    pub fn code(&self) -> RoomCode {
        self.code
    }
    pub fn public(&self) -> bool {
        self.public
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn capacity(&self) -> usize {
        if self.public {
            PUBLIC_CAPACITY
        } else {
            PRIVATE_CAPACITY
        }
    }
    /// Sockets, joined or not.
    pub fn connections(&self) -> usize {
        self.conns.len()
    }
    /// Riders in the roster: joined, or held for reconnection.
    pub fn riders(&self) -> usize {
        self.lobby().players.len()
    }
    /// Marks the rider now in `peer`'s slot as counted or not; set on every
    /// admission, so a slot taken over by someone else follows its new rider.
    pub fn set_counted(&mut self, peer: u8, counted: bool) {
        self.uncounted.retain(|&p| p != peer);
        if !counted {
            self.uncounted.push(peer);
        }
    }
    fn counted(&self, peer: u8) -> bool {
        !self.uncounted.contains(&peer)
    }
    /// [`Self::riders`] without uncounted riders.
    pub fn counted_riders(&self) -> usize {
        self.lobby()
            .players
            .iter()
            .filter(|p| self.counted(p.id))
            .count()
    }
    /// The room's counted riders right now, by build: counts only.
    pub fn rider_counts(&self) -> std::collections::BTreeMap<String, u64> {
        let mut counts = std::collections::BTreeMap::new();
        let mut add = |key: &str, n: u64| *counts.entry(key.to_string()).or_insert(0) += n;
        let joined: Vec<&Conn> = self
            .conns
            .iter()
            .filter(|c| c.player.is_some() && self.counted(c.peer))
            .collect();
        add("riders", joined.len() as u64);
        for c in joined {
            add("oldBuild", u64::from(!c.signals));
            add("frames", u64::from(c.frames));
        }
        add(
            "held",
            self.reserved
                .iter()
                .filter(|r| r.player.is_some() && self.counted(r.peer))
                .count() as u64,
        );
        counts
    }
    /// Counts since the last call.
    pub fn take_counters(&mut self) -> Counters {
        std::mem::take(&mut self.counters)
    }

    fn index(&self, id: ConnId) -> Option<usize> {
        self.conns.iter().position(|c| c.id == id)
    }

    /// The rider with map authority: the room's host while it is in the
    /// roster, else the lowest slot in it.
    fn current_host(&self) -> u8 {
        let roster = self
            .conns
            .iter()
            .filter(|c| c.player.is_some())
            .map(|c| c.peer)
            .chain(
                self.reserved
                    .iter()
                    .filter(|r| r.player.is_some())
                    .map(|r| r.peer),
            );
        if roster.clone().any(|p| p == self.host) {
            self.host
        } else {
            roster.min().unwrap_or(self.host)
        }
    }

    /// Connected riders plus riders whose slots are held for reconnection.
    pub fn lobby(&self) -> Lobby {
        let mut players: Vec<Participant> =
            self.conns.iter().filter_map(|c| c.player.clone()).collect();
        for held in &self.reserved {
            if let Some(player) = &held.player
                && !players.iter().any(|p| p.id == held.peer)
            {
                players.push(player.clone());
            }
        }
        players.sort_by_key(|player| player.id);
        Lobby {
            host: self.current_host(),
            epoch: self.epoch,
            map: self.map,
            started: self.started,
            public: self.public,
            players,
        }
    }

    /// Reserves a slot in a shared room for a Free Ride claimant: its key,
    /// or None when the room is full. Unused claims expire with the hold.
    pub fn claim(&mut self, now: u64, rng: &mut dyn Entropy) -> Option<String> {
        if !self.public || self.closed {
            return None;
        }
        let taken = self
            .conns
            .iter()
            .map(|c| c.peer)
            .chain(self.reserved.iter().map(|r| r.peer));
        let peer = free_slot(taken, self.capacity())?;
        let key = token(rng);
        self.reserved.push(Reserved {
            peer,
            player: None,
            key: key.clone(),
            since_ms: now,
        });
        self.empty_since_ms = None;
        Some(key)
    }

    /// A socket asks to enter with an optional rejoin key (`?key=` or the
    /// creator's `?host=`). A key replaces its own older connection or
    /// takes back its held slot; otherwise the lowest free slot is used.
    pub fn admit(
        &mut self,
        id: ConnId,
        key: Option<&str>,
        now: u64,
        rng: &mut dyn Entropy,
        out: &mut Vec<Out>,
    ) -> Result<u8, Refused> {
        if self.closed {
            return Err(Refused {
                status: 410,
                message: "Room ended",
            });
        }
        let key = key.filter(|key| key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit()));
        let mut reclaimed = None;
        if let Some(key) = key {
            // A rider's key replaces its own broken connection, even if the
            // old transport has not reported its close yet.
            while let Some(i) = self.conns.iter().position(|c| c.key == key) {
                let old = self.conns.remove(i);
                reclaimed = Some(old.peer);
                out.push(Out::Close(old.id, 1000, "Connection replaced"));
            }
            if let Some(index) = self.reserved.iter().position(|r| r.key == key) {
                reclaimed.get_or_insert(self.reserved.remove(index).peer);
            }
        }
        let peer = match reclaimed {
            Some(peer) => peer,
            None => {
                let taken = self
                    .conns
                    .iter()
                    .map(|c| c.peer)
                    .chain(self.reserved.iter().map(|r| r.peer));
                free_slot(taken, self.capacity()).ok_or(Refused {
                    status: 409,
                    message: "Game is full!",
                })?
            }
        };
        let key = match (reclaimed, key) {
            (Some(_), Some(key)) => key.to_string(),
            _ => token(rng),
        };
        self.empty_since_ms = None;
        self.conns.push(Conn {
            id,
            peer,
            key,
            player: None,
            ready_map: 0,
            sequence: None,
            baseline: None,
            pose: None,
            races: false,
            games: false,
            freestyle: false,
            signals: false,
            cpu_sounds: false,
            frames: false,
            hub: false,
            motorcycles: false,
            chat_on: false,
            chat: Default::default(),
            last_seen_ms: now,
            window_ms: now,
            messages: 0,
            bytes: 0,
            sent: [None; MAX_PLAYERS],
        });
        // Nobody else has this slot's sequence any more.
        self.forget_sent(peer);
        Ok(peer)
    }

    /// The socket closed (or failed) with this code and reason.
    pub fn disconnected(
        &mut self,
        id: ConnId,
        code: u16,
        reason: &str,
        now: u64,
        out: &mut Vec<Out>,
    ) {
        if let Some(i) = self.index(id) {
            self.departed(i, recoverable_close(code, reason), now, out);
        }
    }

    fn send(&self, to: ConnId, packet: &Packet, out: &mut Vec<Out>) {
        if let Ok(bytes) = atv_net::encode(packet) {
            out.push(Out::Send(to, Bytes::from(bytes)));
        }
    }

    /// To every joined rider but `except`.
    fn broadcast(&self, packet: &Packet, except: Option<ConnId>, out: &mut Vec<Out>) {
        let Ok(bytes) = atv_net::encode(packet) else {
            return;
        };
        let bytes = Bytes::from(bytes);
        for c in &self.conns {
            if Some(c.id) != except && c.player.is_some() {
                out.push(Out::Send(c.id, bytes.clone()));
            }
        }
    }

    fn broadcast_lobby(&self, out: &mut Vec<Out>) {
        let lobby = self.lobby();
        if !lobby.players.is_empty() {
            self.broadcast(&Packet::Lobby(lobby), None, out);
        }
    }

    fn forget_sent(&mut self, peer: u8) {
        for c in &mut self.conns {
            c.sent[usize::from(peer)] = None;
        }
    }

    /// Closes connection `i` for a reason of the server's own.
    fn close(&mut self, i: usize, code: u16, reason: &'static str, now: u64, out: &mut Vec<Out>) {
        out.push(Out::Close(self.conns[i].id, code, reason));
        self.departed(i, recoverable_close(code, reason), now, out);
    }

    /// Pass map authority to the lowest connected rider, else a held slot.
    fn promote(&mut self) {
        let connected = self
            .conns
            .iter()
            .filter(|c| c.player.is_some())
            .map(|c| c.peer)
            .min();
        if let Some(peer) = connected.or_else(|| {
            self.reserved
                .iter()
                .filter(|r| r.player.is_some())
                .map(|r| r.peer)
                .min()
        }) {
            self.host = peer;
        }
    }

    fn departed(&mut self, i: usize, recoverable: bool, now: u64, out: &mut Vec<Out>) {
        let conn = self.conns.remove(i);
        self.forget_sent(conn.peer);
        if self.closed {
            return;
        }
        let held = recoverable && conn.player.is_some();
        self.reserved.retain(|r| r.peer != conn.peer);
        if held {
            // Keep the rider's slot, color and roster entry while it reconnects.
            self.reserved.push(Reserved {
                peer: conn.peer,
                player: conn.player.clone(),
                key: conn.key.clone(),
                since_ms: now,
            });
        } else {
            self.broadcast(&Packet::Leave { peer: conn.peer }, None, out);
            if self.host == conn.peer {
                self.promote();
            }
        }
        if self.conns.is_empty() {
            self.empty_since_ms.get_or_insert(now);
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.broadcast_lobby(out);
    }

    /// One message from connection `id`.
    pub fn receive(&mut self, id: ConnId, bytes: Bytes, now: u64, out: &mut Vec<Out>) {
        self.counters.messages += 1;
        self.counters.bytes_in += bytes.len() as u64;
        let Some(i) = self.index(id) else {
            return;
        };
        if self.closed {
            return self.close(i, 1000, "Room ended", now, out);
        }
        if !self.conns[i].account(now, bytes.len()) {
            return self.close(i, 1008, "Message rate exceeded", now, out);
        }
        let packet = match atv_net::decode(&bytes) {
            Ok(packet) => packet,
            Err(atv_net::Error::UnsupportedVersion) => {
                return self.close(
                    i,
                    1008,
                    "Game updated. Reload the page to join multiplayer.",
                    now,
                    out,
                );
            }
            Err(_) => return self.close(i, 1008, "Invalid multiplayer packet", now, out),
        };
        let joined = self.conns[i].player.is_some();
        let host = self.current_host();
        let on_map = self.started && self.conns[i].ready_map == self.map;
        match packet {
            Packet::Join { code, mut player } if !joined && code == self.code => {
                if !atv_net::valid_actor(player.atv_model, player.rider_model) {
                    return self.close(i, 1008, "Unknown actor model", now, out);
                }
                let peer = self.conns[i].peer;
                player.id = peer;
                if self.public {
                    // Strangers see this name: apply the shared-room filter.
                    player.name = atv_net::public_name(&player.name, player.id);
                }
                self.conns[i].player = Some(player);
                let present = self
                    .conns
                    .iter()
                    .any(|c| c.id != id && c.player.is_some() && c.peer == self.host)
                    || self
                        .reserved
                        .iter()
                        .any(|r| r.player.is_some() && r.peer == self.host);
                if !present {
                    self.host = peer;
                }
                self.epoch = self.epoch.wrapping_add(1);
                // A reclaimed or reused slot starts a new pose sequence; clear
                // observers' old buffers before its new snapshots arrive.
                self.broadcast(&Packet::Leave { peer }, Some(id), out);
                let lobby = self.lobby();
                self.send(
                    id,
                    &Packet::Welcome {
                        peer,
                        code,
                        lobby: lobby.clone(),
                        key: key_bytes(&self.conns[i].key),
                    },
                    out,
                );
                // Everyone else learns the full roster, including the arrival.
                self.broadcast(&Packet::Lobby(lobby), Some(id), out);
            }
            Packet::Snapshot(snapshot) if joined => {
                self.pose(i, *snapshot, Some(bytes), out);
            }
            Packet::Delta(mut delta) if joined => {
                delta.snapshot.peer = self.conns[i].peer;
                // A delta can race a map/vehicle change. Wait for the sender's
                // next full snapshot rather than ending a healthy session.
                let Some(base) = self.conns[i].baseline.as_ref() else {
                    return;
                };
                match atv_net::apply_delta(&delta, base) {
                    Ok(snapshot) => self.pose(i, snapshot, None, out),
                    Err(_) => self.close(i, 1008, "Invalid snapshot baseline", now, out),
                }
            }
            // An older build's pose batch: its newer poses, in order.
            Packet::Batch { poses, .. } if joined => self.batch(i, poses, out),
            Packet::Ready { map } if joined => {
                if map == self.map {
                    self.conns[i].ready_map = map;
                    self.conns[i].sent = [None; MAX_PLAYERS];
                    // Give an arriving rider everyone's current state, even if
                    // all peers are momentarily idle.
                    let mut catch_up = Vec::new();
                    for other in &self.conns {
                        if other.id != id
                            && other.ready_map == map
                            && let Some(pose) = other.current_pose()
                            && self.conns[i].takes(pose)
                        {
                            catch_up.push((other.peer, pose.clone()));
                        }
                    }
                    for (peer, pose) in catch_up {
                        out.push(Out::Pose(id, pose.bytes.clone()));
                        self.counters.single_poses += 1;
                        self.conns[i].sent[usize::from(peer)] = Some(Sent::of(&pose, now));
                    }
                }
            }
            Packet::Signal(message) if joined => {
                if message.kind == signal::HELLO {
                    let flags = message.data[0];
                    let conn = &mut self.conns[i];
                    conn.signals = true;
                    conn.cpu_sounds = flags & signal::CPU_SOUNDS != 0;
                    conn.frames = flags & signal::FRAMES != 0;
                    conn.hub = self.public && flags & signal::HUB != 0;
                    conn.motorcycles = flags & signal::MOTORCYCLES != 0;
                    conn.chat_on = chat::said_chat(flags);
                }
                // Session descriptions and link reports: no links here.
            }
            // Direct links and couriers do not exist on the server.
            Packet::Links { .. } | Packet::Bundle { .. } if joined => {}
            Packet::Profile(mut player) if joined => {
                if !atv_net::valid_actor(player.atv_model, player.rider_model) {
                    return self.close(i, 1008, "Unknown actor model", now, out);
                }
                let peer = self.conns[i].peer;
                player.id = peer;
                if self.public {
                    player.name = atv_net::public_name(&player.name, player.id);
                }
                let conn = &mut self.conns[i];
                conn.player = Some(player);
                // The rider reloads with its new vehicle and reports Ready again.
                conn.ready_map = 0;
                conn.baseline = None;
                conn.pose = None;
                self.forget_sent(peer);
                self.epoch = self.epoch.wrapping_add(1);
                self.broadcast_lobby(out);
            }
            Packet::Ping { nonce } if joined => self.send(
                id,
                &Packet::Pong {
                    nonce,
                    server_ms: now,
                },
                out,
            ),
            Packet::SetLobby {
                epoch,
                map,
                started,
            } if joined && !self.public && host == self.conns[i].peer => {
                if !atv_net::valid_map(map) {
                    return self.close(i, 1008, "Unknown environment", now, out);
                }
                if epoch != self.epoch || (self.started && !started) {
                    return self.send(id, &Packet::Lobby(self.lobby()), out);
                }
                self.map = map;
                self.started = started;
                self.epoch = self.epoch.wrapping_add(1);
                self.broadcast(&Packet::Lobby(self.lobby()), None, out);
            }
            // A room mini-game's state (research/GAMES.md "Online"): only the
            // host of a started private room runs it; riders on the map get it.
            Packet::Game(state)
                if joined && !self.public && on_map && host == self.conns[i].peer =>
            {
                let race = state.game == atv_net::game_kind::RACE;
                let game = state.game >= atv_net::game_kind::KING_OF_THE_HILLS;
                let freestyle = state.game == atv_net::game_kind::FREESTYLE;
                let packet = Packet::Game(state);
                let Ok(bytes) = atv_net::encode(&packet) else {
                    return;
                };
                let bytes = Bytes::from(bytes);
                for c in &self.conns {
                    if c.id != id
                        && c.ready_map == self.map
                        && (!race || c.races)
                        && (!game || c.games)
                        && (!freestyle || c.freestyle)
                    {
                        out.push(Out::Send(c.id, bytes.clone()));
                    }
                }
            }
            // The room-race host's CPU riders: race-aware riders on the map
            // get them; builds that cannot decode sound cues get them without.
            Packet::Cpus(motion)
                if joined && !self.public && on_map && host == self.conns[i].peer =>
            {
                let plain = without_cues(&motion)
                    .and_then(|m| atv_net::encode(&Packet::Cpus(m)).ok())
                    .map(Bytes::from);
                let Ok(full) = atv_net::encode(&Packet::Cpus(motion)) else {
                    return;
                };
                let full = Bytes::from(full);
                for c in &self.conns {
                    if c.id != id && c.ready_map == self.map && c.races {
                        let bytes = match &plain {
                            Some(plain) if !c.cpu_sounds => plain.clone(),
                            _ => full.clone(),
                        };
                        out.push(Out::Send(c.id, bytes));
                    }
                }
            }
            // A rider's room-race progress: serial 0 only announces support;
            // the server stamps the sender and forwards it to race-aware
            // riders on the map.
            Packet::Progress(mut progress) if joined => {
                self.conns[i].races = true;
                if progress.serial != 0 && !self.public && on_map {
                    progress.peer = self.conns[i].peer;
                    let Ok(bytes) = atv_net::encode(&Packet::Progress(progress)) else {
                        return;
                    };
                    let bytes = Bytes::from(bytes);
                    for c in &self.conns {
                        if c.id != id && c.ready_map == self.map && c.races {
                            out.push(Out::Send(c.id, bytes.clone()));
                        }
                    }
                }
            }
            // A rider's room game report: serial 0 only announces support;
            // the server stamps the sender and passes it to the host alone.
            Packet::Claim(mut claim) if joined => {
                let freestyle = claim.kind == atv_net::claim_kind::FREESTYLE;
                let peer = self.conns[i].peer;
                self.conns[i].games = true;
                self.conns[i].freestyle |= freestyle;
                if claim.serial != 0 && !self.public && on_map && host != peer {
                    claim.peer = peer;
                    for c in &self.conns {
                        if c.peer == host
                            && c.player.is_some()
                            && c.ready_map == self.map
                            && c.games
                            && (!freestyle || c.freestyle)
                        {
                            self.send(c.id, &Packet::Claim(claim), out);
                        }
                    }
                }
            }
            // The Free Ride hub (engine/hub.rs); ignored elsewhere.
            Packet::Hub(message) if joined => {
                let peer = self.conns[i].peer;
                if self.conns[i].hub {
                    let riders = self.hub_riders();
                    if let Some(hub) = &mut self.hub {
                        hub.message(peer, *message, &riders, now, out);
                    }
                }
            }
            Packet::Challenge(message) if joined => {
                let peer = self.conns[i].peer;
                if self.conns[i].hub {
                    let riders = self.hub_riders();
                    if let Some(hub) = &mut self.hub {
                        hub.challenge(peer, message, &riders, now, out);
                    }
                }
            }
            Packet::Chat(message) if joined => self.chat(i, message, now, out),
            Packet::Leave { .. } => self.close(i, 1000, "Left room", now, out),
            // A former host's map command can race a handoff; resynchronize it.
            Packet::SetLobby { .. } if joined => self.send(id, &Packet::Lobby(self.lobby()), out),
            _ => self.close(i, 1008, "Message not permitted", now, out),
        }
    }

    /// Accepts a rider's pose if it is newer than its last, keeps it as the
    /// delta baseline and as encoded bytes, and passes it at once to joined
    /// riders whose builds take no frames (as the relay did). `raw` is the
    /// packet as it arrived, reused when already stamped with the sender's
    /// slot.
    fn pose(&mut self, i: usize, mut snapshot: Snapshot, raw: Option<Bytes>, out: &mut Vec<Out>) {
        let conn = &mut self.conns[i];
        if conn
            .sequence
            .is_some_and(|old| !atv_net::sequence_newer(snapshot.sequence, old))
        {
            return;
        }
        conn.sequence = Some(snapshot.sequence);
        let stamped = snapshot.peer == conn.peer;
        snapshot.peer = conn.peer;
        let bytes = match raw {
            Some(raw) if stamped => raw,
            _ => match atv_net::encode(&Packet::Snapshot(Box::new(snapshot.clone()))) {
                Ok(bytes) => Bytes::from(bytes),
                Err(_) => return,
            },
        };
        conn.pose = Some(Pose::new(&snapshot, bytes.clone()));
        conn.baseline = Some(snapshot);
        self.forward(i, std::slice::from_ref(&bytes), out);
    }

    fn batch(&mut self, i: usize, poses: Vec<Vec<u8>>, out: &mut Vec<Out>) {
        let conn = &mut self.conns[i];
        let models = conn.models().unwrap_or_default();
        let (accepted, newest) = batch_poses(poses, conn.peer, conn.sequence);
        let Some(newest) = newest else {
            return;
        };
        let accepted: Vec<(Bytes, (u16, u16))> = accepted
            .into_iter()
            .map(|(bytes, models)| (Bytes::from(bytes), models))
            .collect();
        let current = accepted.iter().all(|(_, m)| *m == models);
        conn.sequence = Some(newest.sequence);
        let last = accepted.last().map(|(b, _)| b.clone()).unwrap_or_default();
        conn.pose = Some(Pose::new(&newest, last));
        conn.baseline = Some(newest);
        self.counters.batches += 1;
        if current {
            let bytes: Vec<Bytes> = accepted.into_iter().map(|(b, _)| b).collect();
            self.forward(i, &bytes, out);
        } else {
            // Poses from a previous vehicle are never relayed.
            self.conns[i].pose = None;
        }
    }

    /// Counts a sender's accepted pose and sends it to the joined riders
    /// without frames, when the sender is riding the started room's map on
    /// its current vehicle.
    fn forward(&mut self, i: usize, poses: &[Bytes], out: &mut Vec<Out>) {
        let sender = &self.conns[i];
        if !(self.started && sender.ready_map == self.map && sender.current_pose().is_some()) {
            return;
        }
        let alone = !self
            .conns
            .iter()
            .any(|c| c.id != sender.id && c.player.is_some() && c.ready_map == self.map);
        let kind = if alone {
            0
        } else if sender.signals {
            3
        } else {
            2
        };
        self.counters.poses[kind] += 1;
        let bike = atv_net::is_motorcycle(sender.models().unwrap_or_default().0);
        for c in &self.conns {
            if c.id != sender.id && c.player.is_some() && !c.frames && (c.motorcycles || !bike) {
                for pose in poses {
                    out.push(Out::Pose(c.id, pose.clone()));
                    self.counters.single_poses += 1;
                }
            }
        }
    }

    /// The room tick: one frame (or a few, for a crowded room) to each rider
    /// on the map whose build takes frames, with the poses due for it.
    pub fn tick(&mut self, now: u64, out: &mut Vec<Out>) {
        if !self.started || self.closed {
            return;
        }
        self.step_hub(now, out);
        let map = self.map;
        let senders: Vec<(u8, Pose)> = self
            .conns
            .iter()
            .filter(|c| c.ready_map == map)
            .filter_map(|c| c.current_pose().map(|p| (c.peer, p.clone())))
            .collect();
        if senders.is_empty() {
            return;
        }
        let mut due = Vec::with_capacity(senders.len());
        for conn in &mut self.conns {
            if !conn.frames || conn.player.is_none() || conn.ready_map != map {
                continue;
            }
            let origin = conn.pose.as_ref().map(|p| p.position);
            due.clear();
            for (peer, pose) in &senders {
                if *peer == conn.peer || !conn.takes(pose) {
                    continue;
                }
                let slot = &mut conn.sent[usize::from(*peer)];
                let distance = origin.map_or(0., |o| interest::distance(o, pose.position));
                if !interest::due(slot.as_ref(), pose, distance, now) {
                    continue;
                }
                *slot = Some(Sent::of(pose, now));
                if pose.compact {
                    due.push(pose.bytes.clone());
                } else {
                    out.push(Out::Pose(conn.id, pose.bytes.clone()));
                    self.counters.single_poses += 1;
                }
            }
            if due.is_empty() {
                continue;
            }
            self.counters.frame_poses += due.len() as u64;
            for frame in interest::frames(&due) {
                self.counters.frames += 1;
                out.push(Out::Pose(conn.id, frame));
            }
        }
    }

    /// Joined riders as the hub sees them.
    fn hub_riders(&self) -> Vec<hub::Rider> {
        self.conns
            .iter()
            .filter(|c| c.player.is_some())
            .map(|c| {
                let pose = c.current_pose().filter(|_| c.ready_map == self.map);
                hub::Rider {
                    peer: c.peer,
                    conn: c.id,
                    hub: c.hub,
                    position: pose.map(|p| p.position),
                    moving: pose.is_some_and(|p| p.moving),
                }
            })
            .collect()
    }

    /// The hub's step; a finished rotation vote moves the room, older
    /// builds with it (they follow the lobby's map like a host's change).
    fn step_hub(&mut self, now: u64, out: &mut Vec<Out>) {
        if !self.hub.as_ref().is_some_and(|hub| hub.due(now)) {
            return;
        }
        let riders = self.hub_riders();
        let map = self.map;
        let moved = self
            .hub
            .as_mut()
            .map(|hub| hub.step(now, map, &riders, out))
            .unwrap_or_default();
        if let Some(map) = moved
            .map
            .filter(|&m| m != self.map && atv_net::valid_map(m))
        {
            self.map = map;
            self.epoch = self.epoch.wrapping_add(1);
            self.broadcast_lobby(out);
        }
    }

    /// The room's map (shared rooms move with the hub's rotation).
    pub fn map(&self) -> u16 {
        self.map
    }

    /// Once a second: idle sockets, expired holds, room expiry. Returns why
    /// the room ended, after closing every socket, when it did.
    pub fn housekeep(&mut self, now: u64, out: &mut Vec<Out>) -> Option<&'static str> {
        if self.closed {
            return Some("Room ended");
        }
        if !self.public && now.saturating_sub(self.created_ms) > MAX_AGE_MS {
            self.finish("Room expired", out);
            return Some("Room expired");
        }
        while let Some(i) = self
            .conns
            .iter()
            .position(|c| now.saturating_sub(c.last_seen_ms) > IDLE_MS)
        {
            self.close(i, 1000, "Connection timed out", now, out);
        }
        let gone: Vec<Reserved> = self
            .reserved
            .iter()
            .filter(|r| now.saturating_sub(r.since_ms) >= GRACE_MS)
            .cloned()
            .collect();
        if !gone.is_empty() {
            self.reserved.retain(|r| !gone.contains(r));
            for held in gone.iter().filter(|r| r.player.is_some()) {
                self.broadcast(&Packet::Leave { peer: held.peer }, None, out);
            }
            if gone.iter().any(|r| r.peer == self.host) {
                self.promote();
            }
            self.epoch = self.epoch.wrapping_add(1);
            self.broadcast_lobby(out);
        }
        if self.conns.is_empty() && self.reserved.is_empty() {
            self.empty_since_ms.get_or_insert(now);
        } else {
            self.empty_since_ms = None;
        }
        if self
            .empty_since_ms
            .is_some_and(|since| now.saturating_sub(since) > IDLE_MS)
        {
            self.finish("Empty room expired", out);
            return Some("Empty room expired");
        }
        None
    }

    /// Ends the room: every socket closes with `reason`.
    pub fn finish(&mut self, reason: &'static str, out: &mut Vec<Out>) {
        self.closed = true;
        for c in self.conns.drain(..) {
            out.push(Out::Close(c.id, 1000, reason));
        }
        self.reserved.clear();
    }

    /// Shutdown: the room as it must come back, with every connected rider
    /// held, and every socket closed with 1012 so clients reconnect.
    pub fn shut_down(&mut self, now: u64, out: &mut Vec<Out>) -> SavedRoom {
        let mut reserved: Vec<Reserved> = self
            .conns
            .iter()
            .map(|c| Reserved {
                peer: c.peer,
                player: c.player.clone(),
                key: c.key.clone(),
                since_ms: now,
            })
            .collect();
        for held in &self.reserved {
            if !reserved.iter().any(|r| r.peer == held.peer) {
                reserved.push(held.clone());
            }
        }
        reserved.sort_by_key(|r| r.peer);
        for c in self.conns.drain(..) {
            out.push(Out::Close(c.id, 1012, "Server restarting"));
        }
        self.closed = true;
        SavedRoom {
            code: String::from_utf8_lossy(&self.code).into_owned(),
            public: self.public,
            created_ms: self.created_ms,
            host: self.host,
            epoch: self.epoch,
            map: self.map,
            started: self.started,
            reserved,
            uncounted: self.uncounted.clone(),
        }
    }

    /// A room saved at shutdown, its riders' slots held from `now` so they
    /// can rejoin within the hold.
    pub fn restore(saved: SavedRoom, now: u64) -> Option<Self> {
        let code = atv_net::parse_room_code(&saved.code).ok()?;
        let capacity = if saved.public {
            PUBLIC_CAPACITY
        } else {
            PRIVATE_CAPACITY
        };
        let mut reserved = saved.reserved;
        reserved.retain(|r| usize::from(r.peer) < capacity);
        for r in &mut reserved {
            r.since_ms = now;
        }
        Some(Room {
            code,
            public: saved.public,
            created_ms: saved.created_ms,
            host: saved.host,
            epoch: saved.epoch.wrapping_add(1),
            map: if atv_net::valid_map(saved.map) {
                saved.map
            } else {
                1
            },
            started: saved.started,
            reserved,
            empty_since_ms: None,
            closed: false,
            conns: Vec::new(),
            counters: Counters::default(),
            hub: new_hub(&code, saved.public, now),
            uncounted: saved.uncounted,
            chat_ring: Default::default(),
        })
    }
}

/// CPU motion for builds that cannot decode its sound cues (None when it
/// carries none).
fn without_cues(motion: &atv_net::CpuMotion) -> Option<Box<atv_net::CpuMotion>> {
    motion.riders.iter().any(|r| r.sound.is_some()).then(|| {
        let mut m = Box::new(motion.clone());
        for r in &mut m.riders {
            r.sound = None;
        }
        m
    })
}

/// The poses of a rider's batch the server accepts, each with the models
/// it shows, and the newest of them. A pose is accepted when it is stamped
/// with the sender's own slot and is newer than the last pose it sent; the
/// codec has already checked that they are whole compact poses in order.
#[allow(clippy::type_complexity)]
fn batch_poses(
    poses: Vec<Vec<u8>>,
    peer: u8,
    mut sequence: Option<u32>,
) -> (Vec<(Vec<u8>, (u16, u16))>, Option<Snapshot>) {
    let mut newest = None;
    let mut accepted = Vec::with_capacity(poses.len());
    for bytes in poses {
        let Ok(Packet::Snapshot(pose)) = atv_net::decode(&bytes) else {
            continue;
        };
        if pose.peer != peer
            || sequence.is_some_and(|old| !atv_net::sequence_newer(pose.sequence, old))
        {
            continue;
        }
        sequence = Some(pose.sequence);
        accepted.push((bytes, (pose.atv_model, pose.rider_model)));
        newest = Some(*pose);
    }
    (accepted, newest)
}

pub mod hub;

#[cfg(test)]
mod tests;
