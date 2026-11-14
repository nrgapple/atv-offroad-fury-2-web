//! Multiplayer rooms for the Rust engine: the relay session (join, roster,
//! clock, adaptive pose cadence with compact deltas), direct links between
//! riders ([`links`]) and remote riders drawn with their own quad and rider
//! models.
//!
//! Transport-independent: the host moves bytes over a WebSocket (web-sys in
//! the browser, a native client on desktop) and, in the browser, over one
//! WebRTC data channel per linked rider, and calls [`Session`] with them.
//! This ports the protocol half of atv-web's networking adapter
//! (crates/atv-web/src/multiplayer.rs ops 80-85, 109-111, 118) and the remote
//! presentation of multiplayer_animation.rs; web/multiplayer.js stays the
//! JavaScript game's socket glue. The wire format, codec, deltas and
//! smoothing are atv-net's, shared with the JavaScript game and the relay.
pub mod links;
pub mod remote;
pub use atv_net::quickplay;
pub use atv_net::{RiderState, Snapshot};
pub use links::Command as LinkCommand;
pub mod spawn;

use crate::Result;
use crate::ride::Ride;
pub use atv_net::{
    CPU_PEER, CpuMotion, CpuRider, GameClaim, GameMarks, GameState, MAX_GAME_POINTS, MAX_MARKS,
    NO_PEER, RACE_CPUS, RACE_DNF, RACE_MAX_MS, RACE_RIDERS, RaceInfo, RaceProgress, claim_kind,
    game_kind, game_phase, race_phase,
};
use atv_net::{Lobby, Packet, Participant, PoseBuffer, SampledPose};
use std::collections::BTreeMap;

/// The wire protocol version (atv-net), shared with the JavaScript game.
pub const PROTOCOL: u8 = atv_net::VERSION;

/// The reserved Arlington Free Ride rooms `FREERD01`-`FREERD25`.
pub fn is_shared_room(code: &str) -> bool {
    atv_net::parse_room_code(code).is_ok_and(|c| atv_net::is_public_room_code(&c))
}

/// Free Ride's course (Arlington Stream).
pub const FREERIDE_ENVIRONMENT: &str = "env08";

/// Private room codes: exactly eight letters or digits.
pub fn room_code(text: &str) -> Result<atv_net::RoomCode> {
    atv_net::parse_room_code(text)
        .map_err(|_| "Enter the 8-letter or number lobby code.".to_string())
}

/// The player as the room lists them.
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    /// The Garage choice: ATV 1-26 or motorcycle 27-46.
    pub quad: u16,
    /// 0 male, 1 female.
    pub rider: u16,
    /// Gear indices and quad paint (0 = original), as the JavaScript sends.
    pub appearance: [u8; atv_net::APPEARANCE_BYTES],
}

impl Profile {
    /// An unknown vehicle falls back to ATV01. Motorcycles ride rooms too
    /// (research/MULTIPLAYER.md "Motorcycles"); the JavaScript game fell
    /// back to ATV01.
    pub fn new(
        name: &str,
        quad: u16,
        rider: u16,
        appearance: crate::ride::gear::Appearance,
    ) -> Self {
        let mut look = [0; atv_net::APPEARANCE_BYTES];
        if appearance.valid() {
            look[..7].copy_from_slice(&appearance.gear);
            look[7] = appearance.paint;
        }
        let name = name.trim();
        Self {
            name: if name.is_empty() {
                "Rider".into()
            } else {
                name.chars().take(atv_net::MAX_NAME_BYTES).collect()
            },
            quad: if atv_net::valid_actor(quad, 0) {
                quad
            } else {
                1
            },
            rider: rider.min(1),
            appearance: look,
        }
    }

    fn participant(&self) -> Participant {
        Participant {
            id: 0,
            name: self.name.clone(),
            atv_model: self.quad,
            rider_model: self.rider,
            appearance: self.appearance,
        }
    }
}

/// Counters for status lines and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counters {
    pub sent_bytes: u64,
    pub received_bytes: u64,
    pub messages: u64,
    pub full: u64,
    pub delta: u64,
    pub suppressed: u64,
    pub last_packet_bytes: usize,
    /// Poses sent and received over direct links (the host counts sends).
    pub direct_sent: u64,
    pub direct_sent_bytes: u64,
    pub direct_received: u64,
    pub direct_received_bytes: u64,
    /// Bundles sent as the room's courier.
    pub bundles: u64,
    /// Pose batches sent to the relay, and received from it.
    pub batches: u64,
    pub batches_received: u64,
    /// Frames received from a game server.
    pub frames_received: u64,
}

/// What a room's claim (create, open or Free Ride) says its server does
/// (research/GAME_SERVER.md): the relay introduces riders for direct links
/// and takes pose batches; the game server does neither and sends frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Serves {
    /// `links`: introduces riders on a course for direct links.
    pub links: bool,
    /// `batches`: takes pose batches in shared rooms.
    pub batches: bool,
    /// `frames`: sends builds that say [`atv_net::signal::FRAMES`] one
    /// [`Packet::Frame`] per tick.
    pub frames: bool,
    /// `motorcycles`: takes [`atv_net::signal::MOTORCYCLES`] and sends
    /// motorcycle poses only to builds that say it.
    pub motorcycles: bool,
    /// `quickplay`: a server-run Quick Play room (research/QUICK_PLAY.md):
    /// builds say [`atv_net::signal::ROOM_LOOP`] and follow its loop.
    pub quickplay: bool,
}

impl Serves {
    /// The flags of a room claim's JSON; absent ones are false.
    pub fn from_claim(claim: &serde_json::Value) -> Serves {
        Serves {
            links: claim["links"].as_bool() == Some(true),
            batches: claim["batches"].as_bool() == Some(true),
            frames: claim["frames"].as_bool() == Some(true),
            motorcycles: claim["motorcycles"].as_bool() == Some(true),
            quickplay: claim["quickplay"].as_bool() == Some(true),
        }
    }

    /// Whether this server hears a [`Session::hello`].
    pub fn hello(&self) -> bool {
        self.links || self.frames
    }
}

/// One 50 ms tick's poses ([`Session::poses`]).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Poses {
    /// For the relay, ahead of `relay`: the last tick's pose, when this
    /// tick's is in another state and that one was not sent.
    pub before: Option<Vec<u8>>,
    /// For the relay; confirm an actual send with [`Session::sent`].
    pub relay: Option<Vec<u8>>,
    /// A full pose and the linked riders it is due for.
    pub direct: Option<(Vec<u8>, Vec<u8>)>,
}

/// One relay session (atv-web's `Client`, without the browser audio).
#[derive(Default)]
pub struct Session {
    profile: Option<Profile>,
    pub peer: Option<u8>,
    pub code: String,
    /// Owner-only slot secret from Welcome (hex), used to reclaim the slot.
    pub key: String,
    pub lobby: Option<Lobby>,
    baseline: Option<Snapshot>,
    pending: Option<Snapshot>,
    last_sent_ms: u32,
    last_full_ms: u32,
    force_full: bool,
    sequence: u32,
    crash_serial: u32,
    wreck: bool,
    buffers: BTreeMap<u8, PoseBuffer>,
    pub counters: Counters,
    pub rtt_ms: Option<u32>,
    /// The latest relay round trips (ms), oldest first.
    pub rtt_history: std::collections::VecDeque<u32>,
    /// Relay wall clock minus local monotonic clock, from the best ping.
    clock_offset_ms: Option<f64>,
    clock_rtt_ms: u32,
    /// Peer contacts stay off until this local time after an arrival.
    ghost_until: u64,
    /// The host's latest mini-game state (`Packet::Game`) and when it came.
    pub game: Option<(atv_net::GameState, u64)>,
    /// Other riders' latest room-race progress by peer (`Packet::Progress`).
    pub progress: BTreeMap<u8, atv_net::RaceProgress>,
    /// Riders' room-game reports the host has not taken yet (the relay
    /// forwards them to the host only; bounded).
    pub claims: Vec<atv_net::GameClaim>,
    /// The server-run room loop (`Packet::RoomLoop`) and when it came.
    pub room_loop: Option<(atv_net::quickplay::RoomLoop, u64)>,
    /// The room-race host's CPU riders (`Packet::Cpus`): the race serial,
    /// each CPU's pose history by index and its latest progress.
    pub cpu_serial: u32,
    pub cpu_buffers: BTreeMap<u8, PoseBuffer>,
    pub cpu_progress: BTreeMap<u8, atv_net::RaceProgress>,
    /// Each CPU's sound cues not yet reached by its presented pose, with the
    /// motion's sequence (research/AUDIO.md "CPU riders").
    cpu_cues: BTreeMap<u8, Vec<(u32, atv_net::CpuSoundCues)>>,
    /// Direct links to the other riders on this course.
    links: links::Links,
    /// Riders (bits) who got this rider's last pose through the relay.
    relayed: u32,
    /// A rider newly depends on the relay: its next pose goes at once.
    relay_due: bool,
    /// The room's courier (research/MULTIPLAYER.md "Courier"), when one
    /// carries poses.
    pub courier: Option<u8>,
    /// As the courier: the latest pose from each linked rider, not yet
    /// bundled.
    carry: BTreeMap<u8, Vec<u8>>,
    /// The last tick's pose, in a shared room.
    previous: Option<Snapshot>,
    /// As the courier: when the last bundle went.
    last_bundle_ms: u32,
    /// The relay takes pose batches and this build sends them
    /// (research/MULTIPLAYER.md "Free Ride pose batches").
    batching: bool,
    /// Poses kept for the next batch, encoded, oldest first.
    batch: Vec<Vec<u8>>,
    /// The newest pose kept for a batch (sent or not).
    kept: Option<Snapshot>,
    /// The newest batch sent showed the rider at rest.
    sent_resting: bool,
    /// Where the rider came to rest, while it stays there.
    rest: Option<[f32; 3]>,
    /// The relay's pace factor in tenths (research/MULTIPLAYER.md "Free
    /// Ride pace"); 0 before it sends one.
    pub pace: u8,
}

fn encode(packet: &Packet) -> Result<Vec<u8>> {
    atv_net::encode(packet).map_err(|e| e.to_string())
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f32>()
        .sqrt()
}

fn vehicle_world(frame: &atv_core::vehicle::VehicleFrame) -> [f32; 16] {
    let b = atv_core::dynamics::basis(frame.orientation);
    let p = frame.origin;
    [
        b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1], b[2][2],
        0., p[0], p[1], p[2], 1.,
    ]
}

impl Session {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile: Some(profile),
            ..Default::default()
        }
    }

    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// A fresh session for a new socket (reconnect), keeping the profile
    /// (atv-web op 84 then op 80).
    pub fn restart(&mut self) {
        *self = Session {
            profile: self.profile.take(),
            ..Default::default()
        };
    }

    /// The first packet on a new socket.
    pub fn join(&self, code: &str) -> Result<Vec<u8>> {
        let profile = self.profile.as_ref().ok_or("no multiplayer profile")?;
        encode(&Packet::Join {
            code: room_code(code)?,
            player: profile.participant(),
        })
    }

    /// Keeps the socket warm and refines the shared world clock; the nonce is
    /// the local clock so the reply measures the round trip.
    pub fn ping(&self, now_ms: u64) -> Result<Vec<u8>> {
        encode(&Packet::Ping {
            nonce: now_ms as u32,
        })
    }

    /// The host starts the room on `environment` (SetLobby).
    pub fn start_room(&self, environment: &str) -> Result<Vec<u8>> {
        let lobby = self.lobby.as_ref().ok_or("lobby admission is pending")?;
        if self.peer != Some(lobby.host) {
            return Err("Only the lobby host can set up a ride.".into());
        }
        let map = map_id(environment)?;
        encode(&Packet::SetLobby {
            epoch: lobby.epoch,
            map,
            started: true,
        })
    }

    /// Reports the room's map loaded; the relay relays poses only from
    /// riders whose Ready matches the current map. Direct links start over:
    /// the relay answers with who rides the course now.
    pub fn ready(&mut self) -> Result<Vec<u8>> {
        let map = self.lobby.as_ref().ok_or("no lobby")?.map;
        self.links.reset();
        encode(&Packet::Ready { map })
    }

    /// Tells a server that takes one ([`Serves::hello`]) what this build
    /// understands. Send once per socket, after Welcome.
    ///
    /// With `links`, the build follows direct-link signalling, and with
    /// `direct` it can open links itself. With `batches` it sends pose
    /// batches in shared rooms and accepts them from other riders. With
    /// `frames` it decodes [`Packet::Frame`]s. A server without `links`
    /// introduces no riders, so the session keeps sending its poses for
    /// every rider in the room, as without a hello.
    pub fn hello(&mut self, serves: Serves, direct: bool) -> Result<Vec<u8>> {
        let peer = self.peer.ok_or("join multiplayer first")?;
        let direct = direct && serves.links;
        if serves.links {
            self.links.announce(direct);
        }
        // A new socket: the relay names the current courier and pace again.
        self.courier = None;
        self.carry.clear();
        self.batching = serves.batches;
        self.pace = 0;
        let mut flags = if direct {
            atv_net::signal::DIRECT | atv_net::signal::CARRIES
        } else {
            0
        };
        // Every build hears the host's CPU sound cues (research/AUDIO.md
        // "CPU riders"), links or not.
        flags |= atv_net::signal::CPU_SOUNDS;
        if serves.batches {
            flags |= atv_net::signal::BATCHES;
        }
        if serves.frames {
            flags |= atv_net::signal::FRAMES;
        }
        // Every build draws other riders' motorcycles; a server before
        // them refuses the bit.
        if serves.motorcycles {
            flags |= atv_net::signal::MOTORCYCLES;
        }
        encode(&Packet::Signal(atv_net::Signal {
            peer,
            kind: atv_net::signal::HELLO,
            data: if serves.quickplay {
                // The first flags byte is full: Quick Play's bit is in a second.
                vec![flags, atv_net::signal::ROOM_LOOP]
            } else {
                vec![flags]
            },
        }))
    }

    /// This client's session description for `peer`'s link (an offer, or
    /// the answer to theirs), for the relay to pass on.
    pub fn signal(&mut self, peer: u8, answer: bool, description: &[u8]) -> Result<Vec<u8>> {
        self.links.described(description);
        encode(&Packet::Signal(atv_net::Signal {
            peer,
            kind: if answer {
                atv_net::signal::ANSWER
            } else {
                atv_net::signal::OFFER
            },
            data: description.to_vec(),
        }))
    }

    /// What this client's links did, for the relay's accounting, at most
    /// once a minute and only when it changed.
    pub fn link_report_packet(&mut self, now_ms: u64) -> Option<Vec<u8>> {
        let report = self.links.link_report(now_ms)?;
        encode(&Packet::Signal(atv_net::Signal {
            peer: self.peer?,
            kind: atv_net::signal::REPORT,
            data: report.to_bytes(),
        }))
        .ok()
    }

    /// What this client's links did so far.
    pub fn link_counts(&self) -> atv_net::LinkReport {
        self.links.counts
    }

    /// What the host does with its direct connections now. Call every tick,
    /// after the tick's messages.
    pub fn link_commands(&mut self, now_ms: u64) -> Vec<LinkCommand> {
        self.links.commands(now_ms)
    }

    /// The host's connection to `peer` opened.
    pub fn link_opened(&mut self, peer: u8, now_ms: u64) {
        self.links.opened(peer, now_ms);
    }

    /// The host's connection to `peer` closed or could not be made.
    pub fn link_closed(&mut self, peer: u8, now_ms: u64) {
        self.links.closed(peer, now_ms);
    }

    /// The open links for the relay, when they changed: it stops relaying
    /// poses between two riders who both report their link.
    pub fn links_packet(&mut self, now_ms: u64) -> Option<Vec<u8>> {
        let mask = self.links.report(now_ms)?;
        encode(&Packet::Links { mask }).ok()
    }

    /// Each rider on this course and how its poses travel (status, tests).
    pub fn link_states(&self) -> Vec<(u8, &'static str)> {
        self.links.states()
    }

    /// Whether the relay reports the riders on this course.
    pub fn signalling(&self) -> bool {
        self.links.announced()
    }

    /// Change ATV and Rider in place: a Profile packet for the room; the next
    /// pose is an independent full snapshot.
    pub fn change_profile(&mut self, quad: u16, rider: u16) -> Result<Vec<u8>> {
        let profile = self.profile.as_mut().ok_or("no multiplayer profile")?;
        profile.quad = if (1..=26).contains(&quad) { quad } else { 1 };
        profile.rider = rider.min(1);
        let participant = profile.participant();
        self.force_full = true;
        self.baseline = None;
        self.pending = None;
        self.batch.clear();
        self.kept = None;
        encode(&Packet::Profile(participant))
    }

    /// The room's current course, once the host has started it.
    pub fn environment(&self) -> Option<&'static str> {
        self.lobby
            .as_ref()
            .filter(|l| l.started)
            .and_then(|l| atv_net::map_environment(l.map))
    }

    pub fn is_host(&self) -> bool {
        self.lobby
            .as_ref()
            .is_some_and(|l| self.peer == Some(l.host))
    }

    /// Shared world time (ms), or None before the first relay ping.
    pub fn server_now(&self, now_ms: u64) -> Option<f64> {
        self.clock_offset_ms.map(|offset| now_ms as f64 + offset)
    }

    /// The other riders in the roster.
    pub fn others(&self) -> impl Iterator<Item = &Participant> {
        let peer = self.peer;
        self.lobby
            .iter()
            .flat_map(|l| l.players.iter())
            .filter(move |p| Some(p.id) != peer)
    }

    fn set_lobby(&mut self, value: Lobby) {
        if self.lobby.as_ref() != Some(&value) {
            self.force_full = true;
            self.baseline = None;
        }
        // Joins, departures and host handoff change the epoch; only a new
        // map or start state invalidates other riders' motion.
        if self
            .lobby
            .as_ref()
            .is_some_and(|old| old.map != value.map || old.started != value.started)
        {
            self.buffers.clear();
            self.links.reset();
        }
        self.links
            .retain(|peer| value.players.iter().any(|p| p.id == peer));
        // A rider who changed vehicle restarts its pose history.
        let old = self.lobby.take();
        self.buffers.retain(|peer, _| {
            value.players.iter().any(|p| {
                p.id == *peer
                    && old.as_ref().is_none_or(|old| {
                        old.players.iter().any(|o| {
                            o.id == p.id
                                && o.atv_model == p.atv_model
                                && o.rider_model == p.rider_model
                        })
                    })
            })
        });
        self.lobby = Some(value);
    }

    /// Applies one relay message received at `now_ms` (local clock).
    pub fn receive(&mut self, bytes: &[u8], now_ms: u64) -> Result<()> {
        let packet = atv_net::decode(bytes).map_err(|e| e.to_string())?;
        self.counters.received_bytes += bytes.len() as u64;
        match packet {
            Packet::Welcome {
                peer,
                code,
                lobby,
                key,
            } => {
                self.peer = Some(peer);
                self.code = String::from_utf8(code.to_vec()).map_err(|e| e.to_string())?;
                self.key = key.iter().map(|b| format!("{b:02x}")).collect();
                self.set_lobby(lobby);
            }
            Packet::Lobby(lobby) => self.set_lobby(lobby),
            Packet::Snapshot(s) => self.pose(*s, now_ms, None)?,
            // Only sent to a client that said it takes batches. Each pose
            // is timed as if it had come alone, as old as it is in the
            // batch.
            Packet::Batch { interval_ms, poses } => {
                self.counters.batches_received += 1;
                let mut samples = Vec::with_capacity(poses.len());
                for bytes in &poses {
                    if let Ok(Packet::Snapshot(s)) = atv_net::decode(bytes) {
                        samples.push(*s);
                    }
                }
                let newest = samples.last().map_or(0, |s| s.time_ms);
                for s in samples {
                    let age = u64::from(newest.wrapping_sub(s.time_ms));
                    self.pose(s, now_ms.saturating_sub(age), Some(interval_ms.into()))?;
                }
            }
            // Only sent to a client that said frames (research/GAME_SERVER.md):
            // each pose as if it had come alone.
            Packet::Frame { poses } => {
                self.counters.frames_received += 1;
                for bytes in &poses {
                    match atv_net::decode(bytes) {
                        Ok(Packet::Snapshot(s)) => self.pose(*s, now_ms, None)?,
                        Ok(Packet::Delta(d)) => self.frame_delta(*d, now_ms)?,
                        _ => {}
                    }
                }
            }
            Packet::Signal(message) if message.kind == atv_net::signal::PACE => {
                self.pace = message.data[0];
            }
            Packet::Leave { peer } => {
                self.buffers.remove(&peer);
                self.progress.remove(&peer);
                self.links.forget(peer);
            }
            // Only sent to a client that said [`Session::hello`].
            Packet::Signal(message) if message.kind == atv_net::signal::COURIER => {
                if message.data[0] == 1 {
                    self.courier = Some(message.peer);
                } else if self.courier == Some(message.peer) {
                    self.courier = None;
                }
                if self.courier.is_none() || self.courier != self.peer {
                    self.carry.clear();
                }
            }
            Packet::Signal(message) => self.links.signal(message, now_ms),
            Packet::Pong { nonce, server_ms } => {
                let rtt = (now_ms as u32).wrapping_sub(nonce);
                self.rtt_ms = Some(rtt);
                if self.rtt_history.len() >= 64 {
                    self.rtt_history.pop_front();
                }
                self.rtt_history.push_back(rtt);
                // Half the round trip is the best one-way estimate over TCP;
                // prefer low-latency samples.
                if rtt < 5000 {
                    let offset = server_ms as f64 + f64::from(rtt) * 0.5 - now_ms as f64;
                    match self.clock_offset_ms {
                        Some(old) if rtt > self.clock_rtt_ms.saturating_add(20) => {
                            self.clock_offset_ms = Some(old + (offset - old) * 0.05);
                        }
                        Some(old) if (offset - old).abs() < 250. => {
                            self.clock_offset_ms = Some(old + (offset - old) * 0.3);
                            self.clock_rtt_ms = self.clock_rtt_ms.min(rtt);
                        }
                        _ => {
                            self.clock_offset_ms = Some(offset);
                            self.clock_rtt_ms = rtt;
                        }
                    }
                }
            }
            // The relay forwards it only from the host.
            Packet::Game(state) => self.game = Some((*state, now_ms)),
            // The relay stamps the sender.
            Packet::Progress(progress) => {
                self.progress.insert(progress.peer, progress);
            }
            // The host's CPU riders (the relay forwards them only from the
            // host): a new race restarts their histories.
            Packet::Cpus(motion) => {
                if motion.serial != self.cpu_serial {
                    self.cpu_serial = motion.serial;
                    self.cpu_buffers.clear();
                    self.cpu_progress.clear();
                    self.cpu_cues.clear();
                }
                for r in motion.riders {
                    let id = atv_net::CPU_PEER + r.index;
                    self.cpu_progress.insert(
                        r.index,
                        atv_net::RaceProgress {
                            peer: id,
                            serial: motion.serial,
                            lap: r.lap,
                            distance: r.distance,
                            finish_ms: r.finish_ms,
                        },
                    );
                    if let Some(cues) = r.sound {
                        let queue = self.cpu_cues.entry(r.index).or_default();
                        // A pose that stops presenting (stale) keeps no
                        // backlog: the oldest cues go.
                        if queue.len() >= CPU_CUE_BACKLOG {
                            queue.remove(0);
                        }
                        queue.push((r.pose.sequence, cues));
                    }
                    let buffer = self.cpu_buffers.entry(r.index).or_default();
                    if buffer.push(r.pose.clone(), now_ms).is_err() {
                        buffer.clear();
                        buffer.push(r.pose, now_ms).map_err(|e| e.to_string())?;
                    }
                }
            }
            // Room King of the Hills / Treasure Hunt reports: kept for the
            // host's game, which validates them (a bounded queue).
            Packet::Claim(claim) => {
                if self.claims.len() < 64 {
                    self.claims.push(claim);
                }
            }
            Packet::RoomLoop(state) => self.room_loop = Some((*state, now_ms)),
            _ => return Err("unexpected relay packet".into()),
        }
        Ok(())
    }

    /// A pose from another rider, by the relay or their link; `batch` is
    /// the interval of the batch it came in.
    fn pose(&mut self, s: Snapshot, now_ms: u64, batch: Option<u32>) -> Result<()> {
        let allowed = self.lobby.as_ref().is_some_and(|l| {
            l.started
                && l.players.iter().any(|p| {
                    p.id == s.peer && p.atv_model == s.atv_model && p.rider_model == s.rider_model
                })
        });
        if allowed && Some(s.peer) != self.peer {
            let buffer = self.buffers.entry(s.peer).or_default();
            // A link's pose can be handled before an older relay message
            // that was stamped later; arrival times only move forward.
            let now_ms = buffer.latest().map_or(now_ms, |(_, at)| now_ms.max(at));
            if buffer.push(s.clone(), now_ms).is_err() {
                // One rider's pose history (not the wire packet, which
                // decoded and validated) disagrees with this pose:
                // restart that rider's timeline rather than the room.
                buffer.clear();
                buffer.push(s, now_ms).map_err(|e| e.to_string())?;
            }
            buffer.batched(batch);
        }
        Ok(())
    }

    /// A baseline delta in a frame, against the rider's latest pose here. A
    /// delta on another baseline (a pose this client never got) waits for
    /// the rider's next full pose, as the relay does with a sender's.
    fn frame_delta(&mut self, d: atv_net::Delta, now_ms: u64) -> Result<()> {
        let Some((base, _)) = self.buffers.get(&d.snapshot.peer).and_then(|b| b.latest()) else {
            return Ok(());
        };
        match atv_net::apply_delta(&d, base) {
            Ok(s) => self.pose(s, now_ms, None),
            Err(_) => Ok(()),
        }
    }

    /// How late this client now draws `peer` (ms), for status and tests.
    pub fn delay_ms(&self, peer: u8) -> Option<f32> {
        self.buffers.get(&peer).map(PoseBuffer::presented_delay_ms)
    }

    /// Applies one message from `peer`'s direct link: only that rider's own
    /// full pose is accepted. An error means the link (not the room) is
    /// misbehaving and should be closed.
    pub fn receive_direct(&mut self, peer: u8, bytes: &[u8], now_ms: u64) -> Result<()> {
        let Packet::Snapshot(s) = atv_net::decode(bytes).map_err(|e| e.to_string())? else {
            return Err("unexpected direct packet".into());
        };
        if s.peer != peer {
            return Err("direct pose for another rider".into());
        }
        // A link this client already gave up on.
        if !self.links.heard(peer, now_ms) {
            return Ok(());
        }
        self.counters.direct_received += 1;
        self.counters.direct_received_bytes += bytes.len() as u64;
        // Bundles carry compact poses only (what links send).
        if self.courier.is_some() && self.courier == self.peer && s.animation.is_some() {
            self.carry.insert(peer, bytes.to_vec());
        }
        self.pose(*s, now_ms, None)
    }

    /// The player's poses for this tick: one for the relay and one for the
    /// linked riders it is due for, either absent when the adaptive cadence
    /// suppresses it (atv-web op 81). Call every 50 ms; after the relay
    /// bytes are actually sent, confirm them with [`Session::sent`] so
    /// deltas only reference delivered baselines.
    pub fn poses(&mut self, ride: &Ride, now_ms: u64, paused: bool) -> Result<Poses> {
        let s = self.build(ride, now_ms, paused)?;
        self.emit(s, paused)
    }

    fn build(&mut self, ride: &Ride, now_ms: u64, paused: bool) -> Result<Snapshot> {
        let now = now_ms as u32;
        let peer = self.peer.ok_or("join multiplayer first")?;
        let profile = self.profile.as_ref().ok_or("no multiplayer profile")?;
        let frame = ride.frame();
        let bike = ride.motorcycle();
        // A motorcycle's rider is detached off Riding (its own wreck body).
        let wreck = match bike {
            Some(bike) => ride
                .detached_rider_world()
                .map(|world| (world, bike.rider_velocity())),
            None => ride.rider_wreck(),
        };
        if wreck.is_some() && !self.wreck {
            self.crash_serial = self.crash_serial.wrapping_add(1);
        }
        self.wreck = wreck.is_some();
        let transform =
            |m: [f32; 16]| atv_net::Transform::from_column_major(m).map_err(|e| e.to_string());
        let atv = transform(vehicle_world(frame))?;
        let (rider, velocity, state) = match wreck {
            Some((world, velocity)) => (transform(world)?, velocity, RiderState::Wreck),
            None => (
                atv,
                frame.linear_velocity,
                if frame.wreck.phase == atv_core::wreck::Phase::Recovering {
                    RiderState::Recovering
                } else if frame.grounded.iter().any(|v| *v) {
                    RiderState::Riding
                } else {
                    RiderState::Airborne
                },
            ),
        };
        let basis = atv_core::dynamics::basis(frame.orientation);
        let angular_velocity = std::array::from_fn(|i| {
            (0..3)
                .map(|j| basis[j][i] * frame.local_angular_velocity[j])
                .sum()
        });
        let zero = |v: [f32; 3]| if paused { [0.; 3] } else { v };
        // An ATV sends its rider's clip recipe; a motorcycle its linkage and
        // the MX rider's bones (research/MULTIPLAYER.md "Motorcycles").
        let (animation, rider_nodes) = match bike {
            Some(bike) => {
                let (steering, wheels, compression) = bike.linkage();
                let limit = atv_net::STEERING_LIMIT;
                let bones = bike
                    .rider_matrices()?
                    .into_iter()
                    .map(transform)
                    .collect::<Result<Vec<_>>>()?;
                let animation = atv_net::AnimationState {
                    steering: steering.clamp(-limit, limit),
                    wheels: [wheels[0], wheels[1], 0., 0.],
                    compression: [compression[0], compression[1], 0., 0.],
                    paused,
                    ..Default::default()
                };
                (animation, bones)
            }
            None => {
                let recipe = ride.rider_recipe().ok_or("no rider animation")?;
                let animation = atv_net::AnimationState {
                    steering: frame.visual_steering_angle,
                    wheels: frame.wheel_angles,
                    compression: frame.wheel_compression,
                    clips: recipe.clips,
                    count: recipe.count,
                    phase: recipe.phase,
                    first_weight: recipe.first_weight,
                    pair_weight: recipe.pair_weight,
                    look_phase: recipe.look_phase,
                    paused,
                };
                (animation, Vec::new())
            }
        };
        Ok(Snapshot {
            peer,
            sequence: self.sequence.wrapping_add(1),
            time_ms: now,
            atv_model: profile.quad,
            rider_model: profile.rider,
            atv,
            velocity: zero(frame.linear_velocity),
            angular_velocity: zero(angular_velocity),
            animation: Some(Box::new(animation)),
            atv_nodes: Vec::new(),
            rider_nodes,
            rider_state: state,
            rider,
            rider_velocity: zero(velocity),
            rpm: frame.rpm,
            throttle: if paused { 0. } else { ride.applied().throttle },
            speed: if paused { 0. } else { frame.speed_magnitude },
            crash_serial: self.crash_serial,
            train_time: ride.train_time(),
            train_running: !paused,
            platform: ride.train_platform().and_then(|(car, m)| {
                Some(Box::new(atv_net::Platform {
                    car: u8::try_from(car).ok()?,
                    transform: atv_net::Transform::from_column_major(m).ok()?,
                }))
            }),
        })
    }

    /// Decides who needs pose `s` now and encodes it for them.
    fn emit(&mut self, mut s: Snapshot, paused: bool) -> Result<Poses> {
        let now = s.time_ms;
        let state = s.rider_state;
        let moving = s
            .velocity
            .iter()
            .chain(s.rider_velocity.iter())
            .map(|v| v * v)
            .sum::<f32>()
            > 0.25;
        let buffers = &self.buffers;
        let feet = |peer: u8| {
            buffers
                .get(&peer)
                .and_then(|b| b.latest())
                .map(|(p, _)| distance(p.atv.position, s.atv.position))
        };
        // Shared Free Ride rooms are nearly all of the relay's traffic: their
        // relay path waits longer between poses. Links are free and keep
        // their rates.
        let shared = self.lobby.as_ref().is_some_and(|l| l.public);
        let every = |feet: f32, direct: bool| {
            if shared && !direct {
                links::freeride_interval(paused, moving, state, s.speed, feet)
            } else {
                links::interval(paused, moving, state, s.speed, feet, direct)
            }
        };
        // The courier carries this rider's poses to riders without a link to
        // it: linked with the courier, this rider leaves them off the relay
        // and gives the courier the rate the nearest of them needs.
        let carrying = self.courier.is_some() && self.courier == Some(s.peer);
        let courier = self
            .courier
            .filter(|c| *c != s.peer && self.links.is_open(*c));
        let unlinked = self
            .links
            .relayed()
            .map(|peer| feet(peer).unwrap_or(links::UNKNOWN_FT))
            .fold(f32::INFINITY, f32::min);
        // Linked riders: each at the rate its distance needs, and a change
        // of state at once.
        let car = s.platform.as_ref().map(|p| p.car);
        let direct = self
            .links
            .due(now, (state, s.crash_serial, paused, car), |peer| {
                let own = every(feet(peer).unwrap_or(links::UNKNOWN_FT), true);
                if Some(peer) == courier && unlinked.is_finite() {
                    own.min(every(unlinked, false))
                } else {
                    own
                }
            });
        // The relay: at the rate the nearest rider who depends on it needs.
        // With none (alone, or every rider on the course linked) a presence
        // pose keeps the relay's copy current, which it hands the next
        // arrival on Ready. Without it a host whose tab is in the background
        // (throttled timers) has no pose to meet until long after a guest
        // has loaded, who then starts on top of it.
        let mut nearest = f32::INFINITY;
        let (interval, presence) = if self.links.announced() {
            let mut riders = 0;
            if courier.is_none() {
                for peer in self.links.relayed() {
                    riders |= 1 << peer;
                    nearest = nearest.min(feet(peer).unwrap_or(links::UNKNOWN_FT));
                }
            }
            // An arrival, or a rider whose link just failed, sees this
            // rider where it is now.
            self.relay_due |= riders & !self.relayed != 0;
            self.relayed = riders;
            if riders == 0 {
                let idle = paused || !moving;
                (
                    if idle {
                        links::PRESENCE_IDLE_MS
                    } else {
                        links::PRESENCE_MS
                    },
                    true,
                )
            } else {
                (every(nearest, false), false)
            }
        } else {
            // A relay or build without signalling: every rider in the room
            // may be on the course (atv-web sends nothing when alone).
            let solo = self.lobby.as_ref().is_some_and(|l| l.players.len() < 2);
            let nearest = buffers
                .values()
                .filter_map(|b| b.latest())
                .map(|(p, _)| distance(p.atv.position, s.atv.position))
                .fold(f32::INFINITY, f32::min);
            let interval = every(nearest, false);
            (if solo { interval.max(1000) } else { interval }, solo)
        };
        // In a shared room a rider someone needs through the relay sends
        // its poses in batches, when the relay takes them. The courier's
        // own poses stay in its bundles and presence poses stay single.
        if shared && self.batching && !carrying && !presence {
            let plain = self.links.relayed_plain();
            return self.emit_batch(s, paused, moving, nearest, plain, direct);
        }
        self.batch.clear();
        self.kept = None;
        self.rest = None;
        // Slower tiers allow proportionally more travel between poses.
        let reach = 16. * (interval as f32 / 100.).max(1.);
        // In a shared room a rider at rest waits two seconds between poses:
        // its first resting pose goes at once, or one stopped dead would be
        // drawn rolling on from its last moving pose until then.
        let stopped = shared && !moving && state == RiderState::Riding;
        let transition = self.baseline.as_ref().is_none_or(|b| {
            b.rider_state != state
                || b.crash_serial != s.crash_serial
                || b.animation.as_ref().is_some_and(|a| a.paused != paused)
                || b.platform.as_ref().map(|p| p.car) != car
                || distance(b.atv.position, s.atv.position) > reach
                || (stopped && b.velocity.iter().map(|v| v * v).sum::<f32>() > 0.25)
        });
        let elapsed = now.wrapping_sub(self.last_sent_ms);
        // In a shared room the courier bundles at most ten times a second:
        // it is one more stream of relay messages, and the riders it
        // carries for are sent no more often themselves.
        let held = carrying && shared && now.wrapping_sub(self.last_bundle_ms) < 100;
        let relay = !held
            && (self.force_full
                || self.relay_due
                || (!presence && transition)
                || elapsed >= interval);
        // A receiver does not blend a pose with one in another state (taking
        // off, landing, a wreck): it holds the earlier pose until the later
        // one's time. With a shared room's longer intervals that hold would
        // be seen, so the last tick's pose goes first and it lasts one tick.
        let previous = if shared {
            self.previous.replace(s.clone())
        } else {
            None
        };
        let before = previous.filter(|p| {
            relay && !carrying && p.rider_state != state && p.time_ms != self.last_sent_ms
        });
        let bundling = carrying && !held && !self.carry.is_empty();
        if !relay {
            self.counters.suppressed += 1;
            if direct.is_empty() && !bundling {
                return Ok(Poses::default());
            }
        }
        let mut poses = Poses::default();
        if let Some(mut pose) = before {
            pose.sequence = s.sequence;
            s.sequence = s.sequence.wrapping_add(1);
            let output = encode(&Packet::Snapshot(Box::new(pose)))?;
            self.counters.full += 1;
            self.counters.messages += 1;
            self.counters.sent_bytes += output.len() as u64;
            poses.before = Some(output);
        }
        self.sequence = s.sequence;
        let mut whole = None;
        if held {
            // The courier's next bundle is not due: what it carries waits.
        } else if carrying {
            // As the courier, every relayed pose goes in a bundle: this
            // rider's own (whole, no deltas) and the latest from each linked
            // rider, as many as fit; the rest go in the next one.
            let mut poses_out = Vec::new();
            let mut bytes = 9;
            if relay {
                let own = encode(&Packet::Snapshot(Box::new(s.clone())))?;
                bytes += 2 + own.len();
                whole = Some(own.clone());
                poses_out.push(own);
            }
            for (peer, pose) in std::mem::take(&mut self.carry) {
                if bytes + 2 + pose.len() > atv_net::MAX_PACKET_BYTES
                    || poses_out.len() == atv_net::MAX_BUNDLE
                {
                    self.carry.insert(peer, pose);
                } else {
                    bytes += 2 + pose.len();
                    poses_out.push(pose);
                }
            }
            if !poses_out.is_empty() {
                poses.relay = Some(encode(&Packet::Bundle { poses: poses_out })?);
                self.last_bundle_ms = now;
            }
        } else if relay {
            let full = self.force_full
                || transition
                || now.wrapping_sub(self.last_full_ms) >= 2000
                || self.baseline.is_none();
            let patch = match (&self.baseline, full) {
                (Some(baseline), false) => {
                    // An all-block patch is no smaller than independent
                    // full state; spare the relay the baseline
                    // reconstruction.
                    Some(atv_net::delta(&s, baseline).map_err(|e| e.to_string())?)
                        .filter(|patch| patch.mask != 15)
                }
                _ => None,
            };
            let output = match patch {
                Some(patch) => encode(&Packet::Delta(Box::new(patch)))?,
                None => {
                    let output = encode(&Packet::Snapshot(Box::new(s.clone())))?;
                    whole = Some(output.clone());
                    output
                }
            };
            self.counters.last_packet_bytes = output.len();
            poses.relay = Some(output);
        }
        if !direct.is_empty() {
            let output = match whole {
                Some(output) => output,
                None => encode(&Packet::Snapshot(Box::new(s.clone())))?,
            };
            poses.direct = Some((output, direct));
        }
        if relay {
            self.pending = Some(s);
        }
        Ok(poses)
    }

    /// Keeps `pose` for the next batch under the next sequence number.
    fn keep(&mut self, mut pose: Snapshot) -> Result<()> {
        self.sequence = self.sequence.wrapping_add(1);
        pose.sequence = self.sequence;
        self.batch
            .push(encode(&Packet::Snapshot(Box::new(pose.clone())))?);
        self.kept = Some(pose);
        Ok(())
    }

    /// [`Session::emit`] for a rider who batches its relay poses (research/
    /// MULTIPLAYER.md "Free Ride pose batches"): keeps this tick's pose when
    /// the tier's spacing has passed and sends what was kept when its batch
    /// interval has. `nearest` is the distance to the nearest rider who gets
    /// them through the relay; `plain` says one of those is on a build
    /// without batches.
    fn emit_batch(
        &mut self,
        mut s: Snapshot,
        paused: bool,
        moving: bool,
        nearest: f32,
        plain: bool,
        direct: Vec<u8>,
    ) -> Result<Poses> {
        let now = s.time_ms;
        let state = s.rider_state;
        let wrecked = state == RiderState::Wreck;
        // A rider is at rest from the tick it stands still until it has
        // moved 1.5 ft from there, goes 3 ft/s or wrecks: one parked on a
        // slope, creeping or with its wheels leaving the ground and touching
        // again, stays at rest and is sent as standing (no velocity), so
        // that every receiver keeps it fresh as a resting rider.
        let here = s.atv.position;
        self.rest = self
            .rest
            .filter(|at| !wrecked && s.speed.abs() < 3. && distance(*at, here) <= 1.5)
            .or((!moving && state == RiderState::Riding).then_some(here));
        let resting = self.rest.is_some();
        if resting {
            s.velocity = [0.; 3];
            s.rider_velocity = [0.; 3];
            s.angular_velocity = [0.; 3];
            s.speed = 0.;
        }
        let tier_state = if resting { RiderState::Riding } else { state };
        let (every, spacing) =
            links::batch_tier(paused, !resting, tier_state, s.speed, nearest, plain);
        // The relay's pace stretches the wait between batches, not the
        // poses kept in them.
        let pace = u32::from(
            self.pace
                .clamp(atv_net::signal::PACE_MIN, atv_net::signal::PACE_MAX),
        );
        let every = (every * pace / 10).min(u32::from(atv_net::MAX_BATCH_INTERVAL_MS));
        let car = s.platform.as_ref().map(|p| p.car);
        let since_sent = now.wrapping_sub(self.last_sent_ms);
        // A change that matters goes at once: a wreck or its end, a pause,
        // a train car boarded or left, a reset (farther than a ride goes in
        // the time), setting off from rest or coming to it, and a new rider
        // who needs a moving rider's place (the relay's copy of a resting
        // one is right).
        let last = self.kept.as_ref().or(self.baseline.as_ref());
        let kept_ms = last.map(|b| b.time_ms);
        let changed = last.is_none_or(|b| {
            (b.rider_state == RiderState::Wreck) != wrecked
                || b.crash_serial != s.crash_serial
                || b.animation.as_ref().is_some_and(|a| a.paused != paused)
                || b.platform.as_ref().map(|p| p.car) != car
                || distance(b.atv.position, s.atv.position)
                    > 16. * (now.wrapping_sub(b.time_ms) as f32 / 100.).max(1.)
        });
        // (Five times a second at most.)
        let turned = resting != self.sent_resting && since_sent >= 200;
        let arrival = self.relay_due && !resting;
        self.relay_due = false;
        let flush = changed
            || turned
            || arrival
            || since_sent >= every
            || self.batch.len() + 2 >= atv_net::MAX_BATCH;
        // A pose in another state than the one before it is not blended
        // with it by the receiver, which holds the earlier pose: keep both
        // ticks, so that hold lasts 50 ms whatever the spacing. (A rider at
        // rest is one pose.)
        let previous = self.previous.take();
        let stepped = !resting && previous.as_ref().is_some_and(|p| p.rider_state != state);
        if stepped
            && let Some(p) = previous
            && kept_ms != Some(p.time_ms)
        {
            self.keep(p)?;
        }
        let since_kept = kept_ms.map_or(u32::MAX, |at| now.wrapping_sub(at));
        let keeping = flush || stepped || since_kept >= spacing;
        if keeping {
            self.keep(s.clone())?;
            s.sequence = self.sequence;
        }
        let mut poses = Poses::default();
        if !direct.is_empty() {
            let output = match (keeping, self.batch.last()) {
                (true, Some(output)) => output.clone(),
                _ => {
                    self.sequence = self.sequence.wrapping_add(1);
                    s.sequence = self.sequence;
                    encode(&Packet::Snapshot(Box::new(s.clone())))?
                }
            };
            poses.direct = Some((output, direct));
        }
        if flush {
            let output = encode(&Packet::Batch {
                interval_ms: every as u16,
                poses: std::mem::take(&mut self.batch),
            })?;
            self.counters.last_packet_bytes = output.len();
            poses.relay = Some(output);
            self.pending = self.kept.clone();
            self.sent_resting = resting;
        } else {
            self.counters.suppressed += 1;
        }
        self.previous = Some(s);
        Ok(poses)
    }

    /// Send receipt for [`Session::snapshot`] bytes: the baseline advances
    /// only after an actual send (dropped frames never become a reference).
    pub fn sent(&mut self, bytes: &[u8]) -> Result<()> {
        let kind = bytes.get(5).copied().unwrap_or(0);
        if kind == 20 {
            // A batch of whole poses; the newest is the relay's copy.
            self.counters.batches += 1;
            self.counters.messages += 1;
            self.counters.sent_bytes += bytes.len() as u64;
            if let Some(s) = self.pending.take() {
                self.last_sent_ms = s.time_ms;
                self.last_full_ms = s.time_ms;
                self.force_full = false;
                self.baseline = Some(s);
            }
            return Ok(());
        }
        if kind == 19 {
            // A courier's bundle; this rider's own pose in it is whole.
            self.counters.bundles += 1;
            self.counters.messages += 1;
            self.counters.sent_bytes += bytes.len() as u64;
            if let Some(s) = self.pending.take() {
                self.last_sent_ms = s.time_ms;
                self.last_full_ms = s.time_ms;
                self.counters.full += 1;
                self.force_full = false;
                self.relay_due = false;
                self.baseline = Some(s);
            }
            return Ok(());
        }
        let Some(s) = self.pending.take() else {
            return Ok(());
        };
        if !matches!(kind, 9 | 10) || bytes.len() != self.counters.last_packet_bytes {
            return Err("invalid snapshot send receipt".into());
        }
        self.last_sent_ms = s.time_ms;
        if kind == 9 {
            self.last_full_ms = s.time_ms;
            self.counters.full += 1;
        } else {
            self.counters.delta += 1;
        }
        self.force_full = false;
        self.relay_due = false;
        self.baseline = Some(s);
        self.counters.sent_bytes += bytes.len() as u64;
        self.counters.messages += 1;
        Ok(())
    }

    /// Contacts stay off for 2.5 s after any arrival so riders separate.
    pub fn ghost(&mut self, now_ms: u64) {
        self.ghost_until = now_ms + 2500;
    }

    /// Other riders' current (unbuffered) bodies for contacts, unless ghosted
    /// (atv-web `collide`).
    pub fn contact_bodies(&self, now_ms: u64, cars: Cars) -> Vec<PeerBody> {
        let Some(own) = self.peer else {
            return Vec::new();
        };
        if now_ms < self.ghost_until {
            return Vec::new();
        }
        self.buffers
            .iter()
            .filter_map(|(&peer, buffer)| {
                let mut pose = buffer.sample_current(now_ms).filter(|p| !p.stale)?;
                rebase(&mut pose.snapshot, cars);
                Some(PeerBody {
                    position: pose.snapshot.atv.position,
                    velocity: pose.snapshot.velocity,
                    orientation: pose.snapshot.atv.rotation,
                    tie: if own < peer { -1. } else { 1. },
                })
            })
            .collect()
    }

    /// Whether any listed rider's pose has arrived (a meet has something
    /// to try).
    pub fn heard_from_others(&self) -> bool {
        self.others().any(|p| {
            self.buffers
                .get(&p.id)
                .is_some_and(|b| b.latest().is_some())
        })
    }

    /// Riders to meet on arrival: fresh and Riding, the host first in a
    /// private room (shared rooms spread arrivals; `order` shuffles them).
    pub fn meet_candidates(&self, now_ms: u64, order: u64, cars: Cars) -> Vec<Snapshot> {
        let host = self.lobby.as_ref().filter(|l| !l.public).map(|l| l.host);
        let mut riding: Vec<Snapshot> = self
            .buffers
            .values()
            .filter_map(|b| b.sample_current(now_ms).filter(|p| !p.stale))
            .map(|mut p| {
                rebase(&mut p.snapshot, cars);
                p.snapshot
            })
            .filter(|s| s.rider_state == RiderState::Riding)
            .collect();
        riding.sort_by_key(|s| match host {
            Some(host) => (u64::from(s.peer != host), u64::from(s.peer)),
            None => (0, (u64::from(s.peer) ^ order).wrapping_mul(0x9e37_79b9)),
        });
        riding
    }

    /// One rider's current (unbuffered) pose, if fresh.
    pub fn current(&self, peer: u8, now_ms: u64, cars: Cars) -> Option<Snapshot> {
        let mut pose = self.buffers.get(&peer)?.sample_current(now_ms)?;
        if pose.stale {
            return None;
        }
        rebase(&mut pose.snapshot, cars);
        Some(pose.snapshot)
    }

    /// The rider's own room-race progress as a packet.
    pub fn progress_packet(&self, progress: atv_net::RaceProgress) -> Result<Vec<u8>> {
        let peer = self.peer.ok_or("join multiplayer first")?;
        encode(&Packet::Progress(atv_net::RaceProgress {
            peer,
            ..progress
        }))
    }

    /// The host's CPU riders as a packet.
    pub fn cpu_packet(&self, motion: atv_net::CpuMotion) -> Result<Vec<u8>> {
        encode(&Packet::Cpus(Box::new(motion)))
    }

    /// Each CPU rider's presented pose (the remote riders' smoothing).
    pub fn present_cpus(&mut self, now_ms: u64) -> Vec<(u8, SampledPose)> {
        self.cpu_buffers
            .iter_mut()
            .filter_map(|(&index, buffer)| buffer.present(now_ms, 0.).map(|pose| (index, pose)))
            .collect()
    }

    /// CPU `index`'s sound cues that its presented pose (`sequence`) has
    /// reached, merged; the cues of motion `n` play while the pose is
    /// presented between motions `n - 1` and `n`, the interval they cover.
    pub fn take_cpu_cues(&mut self, index: u8, sequence: u32) -> Option<atv_net::CpuSoundCues> {
        let queue = self.cpu_cues.get_mut(&index)?;
        let due = queue
            .iter()
            .take_while(|(s, _)| s.wrapping_sub(sequence) as i32 <= 1)
            .count();
        let mut merged: Option<atv_net::CpuSoundCues> = None;
        for (_, cues) in queue.drain(..due) {
            merged = Some(match merged {
                Some(old) => merge_cues(old, cues),
                None => cues,
            });
        }
        merged
    }

    /// The rider's own room-game report as a packet.
    pub fn claim_packet(&self, claim: atv_net::GameClaim) -> Result<Vec<u8>> {
        let peer = self.peer.ok_or("join multiplayer first")?;
        encode(&Packet::Claim(atv_net::GameClaim { peer, ..claim }))
    }

    /// This rider's room-loop choices as a packet.
    pub fn vote_packet(&self, vote: atv_net::quickplay::Vote) -> Result<Vec<u8>> {
        encode(&Packet::Vote(vote))
    }

    /// The host's mini-game state as a packet.
    pub fn game_packet(&self, state: &atv_net::GameState) -> Result<Vec<u8>> {
        encode(&Packet::Game(Box::new(state.clone())))
    }

    /// Roster order (peer ids, ascending): mini-game slots and teams.
    pub fn roster(&self) -> Vec<u8> {
        let mut ids: Vec<u8> = self
            .lobby
            .iter()
            .flat_map(|l| l.players.iter().map(|p| p.id))
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Every other rider's current position (spawn clearance).
    pub fn positions(&self, now_ms: u64, cars: Cars) -> Vec<[f32; 3]> {
        self.buffers
            .values()
            .filter_map(|b| b.sample_current(now_ms).filter(|p| !p.stale))
            .map(|mut p| {
                rebase(&mut p.snapshot, cars);
                p.snapshot.atv.position
            })
            .collect()
    }

    /// Each remote rider's presented pose (PoseBuffer jitter buffer, Hermite
    /// arcs and decaying corrections). Riders near `own` are presented
    /// closer to their current estimate so contacts line up.
    pub fn present(
        &mut self,
        now_ms: u64,
        own: Option<[f32; 3]>,
        cars: Cars,
    ) -> Vec<(u8, SampledPose)> {
        self.buffers
            .iter_mut()
            .filter_map(|(&id, buffer)| {
                let near = own
                    .zip(buffer.latest().map(|(s, _)| s.atv.position))
                    .map_or(0., |(a, b)| ((40. - distance(a, b)) / 30.).clamp(0., 1.));
                buffer.present(now_ms, near).map(|mut pose| {
                    rebase(&mut pose.snapshot, cars);
                    (id, pose)
                })
            })
            .collect()
    }
}

/// Sound cues of a CPU's motions presented together: every event and the
/// strongest impacts, the later motion's levels.
fn merge_cues(old: atv_net::CpuSoundCues, new: atv_net::CpuSoundCues) -> atv_net::CpuSoundCues {
    use atv_net::cpu_cue::{AIRBORNE, RIDER_WATER};
    let levels = AIRBORNE | RIDER_WATER;
    atv_net::CpuSoundCues {
        flags: (old.flags & !levels) | new.flags,
        quad_contact: old.quad_contact.max(new.quad_contact),
        wreck_impact: old.wreck_impact.max(new.wreck_impact),
        impact: old.impact.max(new.impact),
        rider_impact: old.rider_impact.max(new.rider_impact),
        terrain: new.terrain,
    }
}

/// Unplayed sound cues kept per CPU (two seconds of motion at 10 Hz).
const CPU_CUE_BACKLOG: usize = 20;

/// A room race host's CPU riders as one relay message (research/
/// MULTIPLAYER.md "Room race CPUs"): each CPU's progress and a compact pose
/// built as a rider's own snapshot is, under id [`CPU_PEER`] + index.
pub fn cpu_motion(
    serial: u32,
    sequence: u32,
    time_ms: u32,
    rider_model: u16,
    frames: &[crate::ride::race::RoomCpuFrame],
) -> Result<atv_net::CpuMotion> {
    let transform =
        |m: [f32; 16]| atv_net::Transform::from_column_major(m).map_err(|e| e.to_string());
    let riders = frames
        .iter()
        .take(atv_net::RACE_CPUS)
        .map(|f| {
            let frame = &f.frame;
            let atv = transform(vehicle_world(frame))?;
            let (rider, rider_velocity, rider_state) = match f.wreck {
                Some((world, velocity)) => (transform(world)?, velocity, RiderState::Wreck),
                None => (
                    atv,
                    frame.linear_velocity,
                    if frame.wreck.phase == atv_core::wreck::Phase::Recovering {
                        RiderState::Recovering
                    } else if frame.grounded.iter().any(|v| *v) {
                        RiderState::Riding
                    } else {
                        RiderState::Airborne
                    },
                ),
            };
            let basis = atv_core::dynamics::basis(frame.orientation);
            let angular_velocity = std::array::from_fn(|i| {
                (0..3)
                    .map(|j| basis[j][i] * frame.local_angular_velocity[j])
                    .sum()
            });
            let index = f.index as u8;
            Ok(atv_net::CpuRider {
                index,
                lap: f.lap.min(10) as u8,
                distance: f.distance.clamp(-10_000., 1_000_000.),
                finish_ms: f
                    .finish
                    .map_or(0, |t| ((t * 1000.) as u32).clamp(1, RACE_MAX_MS)),
                pose: Snapshot {
                    peer: atv_net::CPU_PEER + index,
                    sequence,
                    time_ms,
                    atv_model: f.quad,
                    rider_model,
                    atv,
                    velocity: frame.linear_velocity,
                    angular_velocity,
                    animation: Some(Box::new(atv_net::AnimationState {
                        steering: frame.visual_steering_angle,
                        wheels: frame.wheel_angles,
                        compression: frame.wheel_compression,
                        clips: f.recipe.clips,
                        count: f.recipe.count,
                        phase: f.recipe.phase,
                        first_weight: f.recipe.first_weight,
                        pair_weight: f.recipe.pair_weight,
                        look_phase: f.recipe.look_phase,
                        paused: false,
                    })),
                    rider_state,
                    rider,
                    rider_velocity,
                    rpm: frame.rpm,
                    // The guests' engine sound inputs (research/AUDIO.md
                    // "CPU riders"): the snapshot's existing fields.
                    throttle: f.throttle.clamp(-1., 1.),
                    speed: frame.speed_magnitude,
                    // The wreck count stands in for a crash serial.
                    crash_serial: frame.reset_count,
                    ..Default::default()
                },
                sound: Some(f.sound),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let motion = atv_net::CpuMotion { serial, riders };
    motion.validate().map_err(|e| e.to_string())?;
    Ok(motion)
}

/// This client's train car matrices, to place riders standing on a car.
pub type Cars<'a> = &'a dyn Fn(usize) -> Option<[f32; 16]>;

/// No train here (tests, other venues).
pub fn no_cars(_: usize) -> Option<[f32; 16]> {
    None
}

fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}

/// A rider standing on a train car is drawn relative to this client's car
/// (atv-web multiplayer_world.rs `rebase`); a detached rider keeps its world
/// pose.
pub fn rebase(snapshot: &mut Snapshot, cars: Cars) {
    let Some(platform) = &snapshot.platform else {
        return;
    };
    let Some(current) = cars(usize::from(platform.car)) else {
        return;
    };
    let sender = platform.transform.to_column_major();
    let relative = |m: [f32; 16]| {
        atv_net::Transform::from_column_major(multiply(
            current,
            atv_core::obstacle::relative_transform(&m, &sender),
        ))
    };
    if let Ok(atv) = relative(snapshot.atv.to_column_major()) {
        snapshot.atv = atv;
    }
    if snapshot.rider_state != RiderState::Wreck
        && let Ok(rider) = relative(snapshot.rider.to_column_major())
    {
        snapshot.rider = rider;
    }
}

/// Another rider's body for owner-local contacts on one step.
#[derive(Clone, Copy, Debug)]
pub struct PeerBody {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub orientation: [f32; 4],
    /// -1 when this rider's slot is lower, else 1 (equal-mass tie break).
    pub tie: f32,
}

/// The relay's stable map id for a venue.
pub fn map_id(environment: &str) -> Result<u16> {
    atv_net::MAP_ENVIRONMENTS
        .iter()
        .position(|e| e.eq_ignore_ascii_case(environment))
        .map(|i| i as u16 + 1)
        .ok_or_else(|| format!("{environment} is not a multiplayer course"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_net::Participant;

    /// The relay's claims: links and batches.
    const LINKS: Serves = Serves {
        links: true,
        batches: false,
        frames: false,
        motorcycles: false,
        quickplay: false,
    };

    fn lobby(players: &[(u8, u16)], started: bool) -> Lobby {
        Lobby {
            host: 0,
            epoch: 1,
            map: 1,
            started,
            public: false,
            players: players
                .iter()
                .map(|&(id, quad)| Participant {
                    id,
                    name: format!("R{id}"),
                    atv_model: quad,
                    rider_model: 0,
                    appearance: [0; 8],
                })
                .collect(),
        }
    }

    fn welcome(session: &mut Session, peer: u8, lobby: Lobby) {
        let bytes = atv_net::encode(&Packet::Welcome {
            peer,
            code: *b"ABCD1234",
            lobby,
            key: [0xab; 16],
        })
        .unwrap();
        session.receive(&bytes, 0).unwrap();
    }

    #[test]
    fn cpu_sound_cues_play_when_the_presented_pose_reaches_them() {
        use atv_net::cpu_cue::{AIRBORNE, HARD_LANDING, WRECK};
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 1), (1, 1)], true));
        let send = |s: &mut Session, sequence: u32, cues: atv_net::CpuSoundCues| {
            let motion = atv_net::CpuMotion {
                serial: 3,
                riders: vec![atv_net::CpuRider {
                    index: 0,
                    lap: 0,
                    distance: 0.,
                    finish_ms: 0,
                    pose: Snapshot {
                        peer: atv_net::CPU_PEER,
                        sequence,
                        time_ms: sequence * 100,
                        animation: Some(Box::default()),
                        ..Default::default()
                    },
                    sound: Some(cues),
                }],
            };
            let bytes = encode(&Packet::Cpus(Box::new(motion))).unwrap();
            s.receive(&bytes, u64::from(sequence) * 100).unwrap();
        };
        let cue = |flags, impact, terrain| atv_net::CpuSoundCues {
            flags,
            impact,
            terrain,
            ..Default::default()
        };
        send(&mut s, 5, cue(AIRBORNE, 0., [-1, -1]));
        send(&mut s, 6, cue(HARD_LANDING, 20., [3, 3]));
        // Motion 5's cues cover the interval presented from motion 4 on.
        assert_eq!(s.take_cpu_cues(0, 3), None);
        assert_eq!(s.take_cpu_cues(0, 4).map(|c| c.flags), Some(AIRBORNE));
        assert_eq!(s.take_cpu_cues(0, 4), None);
        send(&mut s, 7, cue(WRECK, 5., [4, 4]));
        // Presented late: both play at once, the later levels win.
        let both = s.take_cpu_cues(0, 9).unwrap();
        assert_eq!(both.flags, HARD_LANDING | WRECK);
        assert_eq!((both.impact, both.terrain), (20., [4, 4]));
        assert_eq!(s.take_cpu_cues(1, 9), None);
    }

    #[test]
    fn profile_keeps_garage_vehicles_and_motorcycles() {
        let p = Profile::new("  ", 30, 3, Default::default());
        assert_eq!((p.name.as_str(), p.quad, p.rider), ("Rider", 30, 1));
        assert_eq!(Profile::new("Ann", 14, 0, Default::default()).quad, 14);
        assert_eq!(Profile::new("Ann", 46, 0, Default::default()).quad, 46);
        for unknown in [0, 47] {
            assert_eq!(Profile::new("Ann", unknown, 0, Default::default()).quad, 1);
        }
    }

    #[test]
    fn welcome_sets_identity_and_room() {
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 1), (1, 1)], true));
        assert_eq!(s.peer, Some(1));
        assert_eq!(s.code, "ABCD1234");
        assert_eq!(s.key, "ab".repeat(16));
        assert_eq!(s.environment(), Some("env01"));
        assert!(!s.is_host());
        assert_eq!(s.others().map(|p| p.id).collect::<Vec<_>>(), [0]);
        assert!(s.start_room("env02").is_err());
    }

    #[test]
    fn join_and_set_lobby_round_trip() {
        let mut s = Session::new(Profile::new("Host", 3, 1, Default::default()));
        let join = atv_net::decode(&s.join("abcd1234").unwrap()).unwrap();
        let Packet::Join { code, player } = join else {
            panic!("join packet")
        };
        assert_eq!(&code, b"ABCD1234");
        assert_eq!((player.atv_model, player.rider_model), (3, 1));
        assert!(s.join("short").is_err());
        welcome(&mut s, 0, lobby(&[(0, 3)], false));
        assert_eq!(s.environment(), None);
        let Packet::SetLobby { map, started, .. } =
            atv_net::decode(&s.start_room("env08").unwrap()).unwrap()
        else {
            panic!("set lobby")
        };
        assert_eq!((map, started), (8, true));
    }

    #[test]
    fn snapshots_only_from_rostered_riders_with_matching_models() {
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 5), (1, 1)], true));
        let mut pose = Snapshot {
            peer: 0,
            sequence: 1,
            atv_model: 5,
            rider_model: 0,
            ..Default::default()
        };
        let bytes = atv_net::encode(&Packet::Snapshot(Box::new(pose.clone()))).unwrap();
        s.receive(&bytes, 10).unwrap();
        assert_eq!(s.present(100, None, &no_cars).len(), 1);
        // A model the roster does not list is ignored.
        pose.atv_model = 6;
        pose.sequence = 2;
        let mut other = Session::new(Profile::new("B", 1, 0, Default::default()));
        welcome(&mut other, 1, lobby(&[(0, 5), (1, 1)], true));
        let bytes = atv_net::encode(&Packet::Snapshot(Box::new(pose))).unwrap();
        other.receive(&bytes, 10).unwrap();
        assert!(other.present(100, None, &no_cars).is_empty());
        // Leave drops the rider's history.
        let leave = atv_net::encode(&Packet::Leave { peer: 0 }).unwrap();
        s.receive(&leave, 20).unwrap();
        assert!(s.present(100, None, &no_cars).is_empty());
    }

    fn signal(session: &mut Session, peer: u8, kind: u8, data: &[u8], now_ms: u64) {
        let bytes = atv_net::encode(&Packet::Signal(atv_net::Signal {
            peer,
            kind,
            data: data.to_vec(),
        }))
        .unwrap();
        session.receive(&bytes, now_ms).unwrap();
    }

    /// This rider's pose at `x` ft along the course, moving at `speed`.
    fn riding(session: &Session, time_ms: u32, x: f32, speed: f32) -> Snapshot {
        let mut s = Snapshot {
            peer: session.peer.unwrap(),
            sequence: session.sequence.wrapping_add(1),
            time_ms,
            atv_model: 1,
            rider_model: 0,
            velocity: [speed, 0., 0.],
            speed,
            animation: Some(Default::default()),
            ..Default::default()
        };
        s.atv.position[0] = x;
        s.rider.position[0] = x;
        s
    }

    /// Rides from `from` to `until` (ms) at 20 ft/s in 50 ms ticks and
    /// counts the poses for the relay and for linked riders.
    fn ride(session: &mut Session, from: u32, until: u32) -> (usize, usize) {
        let (mut relay, mut direct) = (0, 0);
        for time in (from..until).step_by(50) {
            let pose = riding(session, time, time as f32 * 0.02, 20.);
            let poses = session.emit(pose, false).unwrap();
            if let Some(bytes) = poses.relay {
                session.sent(&bytes).unwrap();
                relay += 1;
            }
            direct += poses.direct.map_or(0, |(_, peers)| peers.len());
        }
        (relay, direct)
    }

    fn pose_from(peer: u8, sequence: u32, time_ms: u32, x: f32) -> Vec<u8> {
        let mut s = Snapshot {
            peer,
            sequence,
            time_ms,
            atv_model: 1,
            rider_model: 0,
            ..Default::default()
        };
        s.atv.position[0] = x;
        atv_net::encode(&Packet::Snapshot(Box::new(s))).unwrap()
    }

    #[test]
    fn the_courier_carries_linked_riders_poses_to_riders_without_links() {
        use atv_net::signal::{ANSWER, CARRIES, COURIER, CPU_SOUNDS, DIRECT, PRESENT};
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 1), (1, 1), (2, 1)], true));
        let hello = atv_net::decode(&s.hello(LINKS, true).unwrap()).unwrap();
        assert!(matches!(hello, Packet::Signal(m) if m.data == [DIRECT | CARRIES | CPU_SOUNDS]));
        s.ready().unwrap();
        // Rider 0 links with this one; rider 2 (a desktop build) cannot.
        signal(&mut s, 0, PRESENT, &[DIRECT], 0);
        signal(&mut s, 2, PRESENT, &[0], 0);
        s.link_commands(0);
        signal(&mut s, 0, ANSWER, b"v=0", 0);
        s.link_commands(0);
        s.link_opened(0, 0);
        assert_eq!(ride(&mut s, 0, 1000), (10, 10));
        // Rider 0 is the courier: rider 2 gets this rider's poses through
        // it, so the relay only hears a presence pose.
        signal(&mut s, 0, COURIER, &[1], 1000);
        assert_eq!(s.courier, Some(0));
        assert_eq!(ride(&mut s, 1000, 4000), (1, 30));
        // The courier stops: back to the relay at once.
        signal(&mut s, 0, COURIER, &[0], 4000);
        assert_eq!(ride(&mut s, 4000, 5000), (10, 10));
        // This rider is the courier: rider 0's poses ride along with its own.
        signal(&mut s, 1, COURIER, &[1], 5000);
        let mut compact = atv_net::decode(&pose_from(0, 1, 5000, 3.)).unwrap();
        if let Packet::Snapshot(p) = &mut compact {
            p.animation = Some(Default::default());
        }
        s.receive_direct(0, &atv_net::encode(&compact).unwrap(), 5000)
            .unwrap();
        let pose = riding(&s, 5050, 101., 20.);
        let bundle = s.emit(pose, false).unwrap().relay.unwrap();
        s.sent(&bundle).unwrap();
        let Packet::Bundle { poses } = atv_net::decode(&bundle).unwrap() else {
            panic!("a bundle");
        };
        let peers: Vec<u8> = poses
            .iter()
            .map(|p| match atv_net::decode(p).unwrap() {
                Packet::Snapshot(s) => s.peer,
                _ => panic!("a pose"),
            })
            .collect();
        assert_eq!(peers, [1, 0]);
        assert_eq!(s.counters.bundles, 1);
        // Nothing new from rider 0: the next ticks carry this rider alone,
        // at its own rate, still bundled.
        let (relay, _) = ride(&mut s, 5100, 6100);
        assert_eq!((relay, s.counters.bundles), (10, 11));
    }

    #[test]
    fn a_shared_room_courier_bundles_ten_times_a_second_at_most() {
        use atv_net::signal::{ANSWER, COURIER, DIRECT, PRESENT};
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        let room = Lobby {
            public: true,
            ..lobby(&[(0, 1), (1, 1), (2, 1)], true)
        };
        welcome(&mut s, 1, room);
        s.hello(LINKS, true).unwrap();
        s.ready().unwrap();
        // Rider 0 is linked with this rider, the courier; rider 2 cannot link.
        signal(&mut s, 0, PRESENT, &[DIRECT], 0);
        signal(&mut s, 2, PRESENT, &[0], 0);
        s.link_commands(0);
        signal(&mut s, 0, ANSWER, b"v=0", 0);
        s.link_commands(0);
        s.link_opened(0, 0);
        signal(&mut s, 1, COURIER, &[1], 0);
        // Rider 0's pose arrives on every 50 ms tick for two seconds.
        let mut bundles = 0;
        for time in (1000..3000).step_by(50) {
            let mut carried = atv_net::decode(&pose_from(0, time / 50, time, 3.)).unwrap();
            if let Packet::Snapshot(p) = &mut carried {
                p.animation = Some(Default::default());
            }
            s.receive_direct(0, &atv_net::encode(&carried).unwrap(), time.into())
                .unwrap();
            let pose = riding(&s, time, time as f32 * 0.02, 20.);
            if let Some(bytes) = s.emit(pose, false).unwrap().relay {
                assert!(matches!(
                    atv_net::decode(&bytes).unwrap(),
                    Packet::Bundle { .. }
                ));
                s.sent(&bytes).unwrap();
                bundles += 1;
            }
        }
        assert_eq!((bundles, s.counters.bundles), (20, 20));
    }

    #[test]
    fn linked_riders_get_poses_directly_and_the_relay_only_presence() {
        use atv_net::signal::{ANSWER, ARRIVED, DIRECT, DROPPED, PRESENT};
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 1), (1, 1), (2, 1)], true));
        assert!(!s.signalling());
        s.hello(LINKS, true).unwrap();
        s.ready().unwrap();
        // Rider 0 can link; rider 2 is on a build that cannot.
        signal(&mut s, 0, PRESENT, &[DIRECT], 0);
        signal(&mut s, 2, PRESENT, &[0], 0);
        assert_eq!(s.link_commands(0), [LinkCommand::Offer(0)]);
        // Both still depend on the relay: the ordinary 10 Hz, nothing direct.
        assert_eq!(ride(&mut s, 1000, 2000), (10, 0));
        signal(&mut s, 0, ANSWER, b"v=0", 2000);
        assert_eq!(
            s.link_commands(2000),
            [LinkCommand::Answer(0, b"v=0".to_vec())]
        );
        s.link_opened(0, 2000);
        let links = atv_net::decode(&s.links_packet(2600).unwrap()).unwrap();
        assert_eq!(links, Packet::Links { mask: 1 });
        assert!(s.links_packet(4000).is_none());
        // The linked rider gets every pose directly; rider 2 keeps the relay
        // at its rate.
        assert_eq!(ride(&mut s, 2000, 3000), (10, 10));
        // Rider 2 leaves: nobody needs the relay, which gets one presence
        // pose every three seconds.
        let leave = atv_net::encode(&Packet::Leave { peer: 2 }).unwrap();
        s.receive(&leave, 3000).unwrap();
        let roster = atv_net::encode(&Packet::Lobby(Lobby {
            epoch: 2,
            ..lobby(&[(0, 1), (1, 1)], true)
        }))
        .unwrap();
        s.receive(&roster, 3000).unwrap();
        // (The roster change itself sends one full pose.)
        assert_eq!(ride(&mut s, 3000, 3050), (1, 1));
        // The linked rider is far away: about 3 Hz is enough for it.
        s.receive_direct(0, &pose_from(0, 1, 500, 5000.), 3050)
            .unwrap();
        assert_eq!(ride(&mut s, 3050, 9050), (2, 20));
        assert_eq!(s.present(9050, None, &no_cars).len(), 1);
        assert_eq!(s.counters.direct_received, 1);
        // A stationary rider idles on both paths; the link still proves
        // itself alive once a second.
        let (mut relay, mut direct) = (0, 0);
        for time in (9050..19_050).step_by(50) {
            let pose = riding(&s, time, 180., 0.);
            let poses = s.emit(pose, false).unwrap();
            if let Some(bytes) = poses.relay {
                s.sent(&bytes).unwrap();
                relay += 1;
            }
            direct += usize::from(poses.direct.is_some());
        }
        assert_eq!((relay, direct), (2, 10));
        // An arrival sees this rider where it is now, not where the last
        // presence pose left it, and rides on the relay until its link opens.
        signal(&mut s, 3, ARRIVED, &[DIRECT], 19_050);
        assert_eq!(ride(&mut s, 19_050, 19_100).0, 1);
        assert_eq!(ride(&mut s, 19_100, 20_100).0, 10);
        assert_eq!(s.link_states(), [(0, "direct"), (3, "waiting")]);
        // The linked rider gives up its end: the relay carries poses again
        // at once.
        signal(&mut s, 3, atv_net::signal::GONE, &[0], 20_100);
        ride(&mut s, 20_100, 20_200);
        signal(&mut s, 0, DROPPED, &[0], 20_200);
        assert_eq!(s.link_commands(20_200), [LinkCommand::Close(0)]);
        assert_eq!(ride(&mut s, 20_200, 20_250), (1, 0));
        assert_eq!(
            atv_net::decode(&s.links_packet(21_000).unwrap()).unwrap(),
            Packet::Links { mask: 0 }
        );
    }

    #[test]
    fn a_link_carries_only_its_own_riders_poses() {
        use atv_net::signal::{ARRIVED, DIRECT, OFFER};
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 1), (1, 1), (2, 1)], true));
        s.hello(LINKS, true).unwrap();
        signal(&mut s, 0, ARRIVED, &[DIRECT], 0);
        signal(&mut s, 0, OFFER, b"v=0", 0);
        assert_eq!(
            s.link_commands(0),
            [LinkCommand::Accept(0, b"v=0".to_vec())]
        );
        // Nothing is taken from a link that is not open.
        s.receive_direct(0, &pose_from(0, 1, 0, 10.), 10).unwrap();
        assert!(s.present(100, None, &no_cars).is_empty());
        s.link_opened(0, 20);
        s.receive_direct(0, &pose_from(0, 2, 50, 10.), 30).unwrap();
        assert_eq!(s.present(300, None, &no_cars).len(), 1);
        // Another rider's pose, or anything but a pose, is the link's fault.
        assert!(s.receive_direct(0, &pose_from(2, 1, 0, 10.), 40).is_err());
        let leave = atv_net::encode(&Packet::Leave { peer: 2 }).unwrap();
        assert!(s.receive_direct(0, &leave, 40).is_err());
        assert!(s.receive_direct(0, b"junk", 40).is_err());
        assert_eq!(s.present(310, None, &no_cars).len(), 1);
        // The relay's copy of an older pose arriving late is not an error,
        // and a relay message stamped before the link's is still ordered.
        s.receive(&pose_from(0, 1, 0, 10.), 25).unwrap();
        s.receive(&pose_from(0, 3, 100, 12.), 25).unwrap();
        s.receive_direct(0, &pose_from(0, 4, 150, 14.), 90).unwrap();
        assert_eq!(s.counters.direct_received, 2);
        // Reloading the course drops every link.
        s.ready().unwrap();
        assert_eq!(s.link_commands(400), [LinkCommand::Close(0)]);
        assert!(s.link_states().is_empty());
    }

    #[test]
    fn without_signalling_the_relay_pose_rate_follows_the_nearest_rider() {
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(&mut s, 1, lobby(&[(0, 1), (1, 1)], true));
        // Nobody heard yet: the far rate.
        assert_eq!(ride(&mut s, 1000, 3100).0, 6);
        // A rider 60-80 ft away: 10 Hz. (This rider moves 20 ft a second.)
        s.receive(&pose_from(0, 1, 0, 142.), 3100).unwrap();
        assert_eq!(ride(&mut s, 3100, 4100), (10, 0));
        // 2000 ft away: about 3 Hz, however fast this rider goes.
        s.receive(&pose_from(0, 2, 50, 2100.), 4100).unwrap();
        let mut relay = 0;
        for time in (4100..6200).step_by(50) {
            let mut pose = riding(&s, time, time as f32 * 0.02, 90.);
            pose.rider_state = RiderState::Airborne;
            if let Some(bytes) = s.emit(pose, false).unwrap().relay {
                s.sent(&bytes).unwrap();
                relay += 1;
            }
        }
        assert_eq!(relay, 6);
        // Alone in the room: a presence pose once a second.
        let roster = atv_net::encode(&Packet::Lobby(Lobby {
            host: 1,
            epoch: 2,
            ..lobby(&[(1, 1)], true)
        }))
        .unwrap();
        s.receive(&roster, 6200).unwrap();
        assert_eq!(ride(&mut s, 6200, 9200), (3, 0));
    }

    #[test]
    fn shared_free_ride_rooms_send_the_relay_fewer_poses() {
        let room = |public| Lobby {
            public,
            ..lobby(&[(0, 1), (1, 1)], true)
        };
        // Rides on from `x` or rests there for `ms`; counts the relay's poses.
        fn poses(s: &mut Session, x: &mut f32, from: u32, ms: u32, speed: f32) -> usize {
            let paused = speed < 0.;
            let speed = speed.max(0.);
            let mut relay = 0;
            for time in (from..from + ms).step_by(50) {
                *x += speed * 0.05;
                let mut pose = riding(s, time, *x, speed);
                pose.animation.as_mut().unwrap().paused = paused;
                if let Some(bytes) = s.emit(pose, paused).unwrap().relay {
                    s.sent(&bytes).unwrap();
                    relay += 1;
                }
            }
            relay
        }
        let mut counts = Vec::new();
        for public in [false, true] {
            let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
            welcome(&mut s, 1, room(public));
            // The other rider is 130-200 ft away, then (below) 2000 ft.
            s.receive(&pose_from(0, 1, 0, 200.), 0).unwrap();
            let mut x = 0.;
            let moving = poses(&mut s, &mut x, 0, 3000, 20.);
            let resting = poses(&mut s, &mut x, 3000, 10_000, 0.);
            // Setting off again is sent at once.
            let off = poses(&mut s, &mut x, 13_000, 50, 20.);
            let paused = poses(&mut s, &mut x, 13_050, 30_000, -1.);
            s.receive(&pose_from(0, 2, 50, 2400.), 43_050).unwrap();
            let far = poses(&mut s, &mut x, 43_050, 2100, 20.);
            counts.push([moving, resting, off, paused, far]);
        }
        // A private room keeps its rates: 10 Hz, 1 Hz, 0.2 Hz, about 3 Hz far.
        assert_eq!(counts[0], [30, 10, 1, 6, 6]);
        // Free Ride: about 7 Hz, 0.5 Hz, 0.1 Hz; the far rate is unchanged.
        assert_eq!(counts[1], [20, 5, 1, 3, 6]);
    }

    #[test]
    fn a_shared_room_sends_the_tick_before_a_change_of_state_too() {
        let mut sent = Vec::new();
        for public in [false, true] {
            let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
            welcome(
                &mut s,
                1,
                Lobby {
                    public,
                    ..lobby(&[(0, 1), (1, 1)], true)
                },
            );
            s.receive(&pose_from(0, 1, 0, 200.), 0).unwrap();
            // Riding at 20 ft/s, airborne from 1150 ms to 1450 ms.
            let mut poses = Vec::new();
            for time in (0..2000).step_by(50) {
                let mut pose = riding(&s, time, time as f32 * 0.02, 20.);
                if (1150..1450).contains(&time) {
                    pose.rider_state = RiderState::Airborne;
                }
                let out = s.emit(pose, false).unwrap();
                for bytes in out.before.iter().chain(&out.relay) {
                    let pose = match atv_net::decode(bytes).unwrap() {
                        Packet::Snapshot(s) => *s,
                        Packet::Delta(d) => d.snapshot,
                        _ => panic!("a pose"),
                    };
                    poses.push((pose.time_ms, pose.sequence, pose.rider_state));
                }
                if let Some(bytes) = out.relay {
                    s.sent(&bytes).unwrap();
                }
            }
            sent.push(poses);
        }
        let around = |poses: &[(u32, u32, RiderState)], from: u32, until: u32| -> Vec<u32> {
            poses
                .iter()
                .filter(|p| (from..=until).contains(&p.0))
                .map(|p| p.0)
                .collect()
        };
        // A private room sends every 100 ms while riding and every 50 ms in
        // the air, and each change at once: 50 ms after the pose before it.
        assert_eq!(
            around(&sent[0], 900, 1500),
            [900, 1000, 1100, 1150, 1200, 1250, 1300, 1350, 1400, 1450]
        );
        // Free Ride sends every 150 ms (100 ms in the air). The poses before
        // take-off (1050) and landing (1350) would be 100 ms old: the ticks
        // at 1100 and 1400 go with the changes.
        assert_eq!(
            around(&sent[1], 900, 1500),
            [900, 1050, 1100, 1150, 1250, 1350, 1400, 1450]
        );
        // Sequence numbers still rise with time.
        for poses in &sent {
            assert!(
                poses
                    .windows(2)
                    .all(|w| w[1].0 > w[0].0 && w[1].1 == w[0].1 + 1)
            );
        }
    }

    /// Rides on from `x` for `ms` and collects the batches for the relay:
    /// the interval each announces and its poses.
    fn batches(
        s: &mut Session,
        x: &mut f32,
        from: u32,
        ms: u32,
        speed: f32,
        state: RiderState,
    ) -> Vec<(u16, Vec<Snapshot>)> {
        let mut out = Vec::new();
        for time in (from..from + ms).step_by(50) {
            *x += speed * 0.05;
            let mut pose = riding(s, time, *x, speed);
            pose.rider_state = state;
            let Some(bytes) = s.emit(pose, false).unwrap().relay else {
                continue;
            };
            s.sent(&bytes).unwrap();
            let Packet::Batch { interval_ms, poses } = atv_net::decode(&bytes).unwrap() else {
                panic!("a batch");
            };
            let poses = poses
                .iter()
                .map(|pose| match atv_net::decode(pose).unwrap() {
                    Packet::Snapshot(s) => *s,
                    _ => panic!("a pose"),
                })
                .collect();
            out.push((interval_ms, poses));
        }
        out
    }

    #[test]
    fn a_shared_room_rider_batches_its_relay_poses_by_distance() {
        use RiderState::{Airborne, Riding, Wreck};
        use atv_net::signal::{BATCHES, CPU_SOUNDS, PACE, PRESENT};
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        let room = Lobby {
            public: true,
            ..lobby(&[(0, 1), (1, 1), (2, 1)], true)
        };
        welcome(&mut s, 1, room);
        let hello = atv_net::decode(
            &s.hello(
                Serves {
                    links: true,
                    batches: true,
                    ..Default::default()
                },
                false,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(hello, Packet::Signal(m) if m.data == [CPU_SOUNDS | BATCHES]));
        s.ready().unwrap();
        // Rider 0, whose build takes batches, is 20-40 ft away.
        signal(&mut s, 0, PRESENT, &[BATCHES], 0);
        s.receive(&pose_from(0, 1, 0, 40.), 0).unwrap();
        let mut all = Vec::new();
        let mut x = 0.;
        let times = |batch: &(u16, Vec<Snapshot>)| -> Vec<u32> {
            batch.1.iter().map(|p| p.time_ms).collect()
        };
        // Near: five batches a second, poses 100 ms apart (the first pose
        // goes at once).
        let near = batches(&mut s, &mut x, 1000, 1000, 20., Riding);
        assert_eq!(near.len(), 5);
        assert!(near.iter().all(|b| b.0 == 200));
        assert_eq!(times(&near[0]), [1000]);
        assert_eq!(times(&near[1]), [1100, 1200]);
        assert_eq!(times(&near[4]), [1700, 1800]);
        all.extend(near);
        // Fast: still five batches, poses 50 ms apart.
        let fast = batches(&mut s, &mut x, 2000, 1000, 60., Riding);
        assert_eq!(fast.len(), 5);
        assert_eq!(times(&fast[0]), [1900, 2000]);
        assert_eq!(times(&fast[1]), [2050, 2100, 2150, 2200]);
        all.extend(fast);
        // Rider 0 is nearly a mile off: a batch every 1.5 s, poses 500 ms
        // apart.
        s.receive(&pose_from(0, 2, 50, 5000.), 3000).unwrap();
        let far = batches(&mut s, &mut x, 3000, 6000, 20., Riding);
        assert_eq!(far.len(), 4);
        assert!(far.iter().all(|b| b.0 == 1500));
        assert_eq!(times(&far[1]), [4800, 5300, 5800]);
        assert_eq!(times(&far[3]), [7800, 8300, 8800]);
        all.extend(far);
        // A jump is not sent at once, but the ticks on both sides of each
        // change of state are kept, so the receiver holds a pose for 50 ms.
        let jump = batches(&mut s, &mut x, 9000, 500, 20., Airborne);
        assert!(jump.is_empty());
        let landed = batches(&mut s, &mut x, 9500, 1000, 20., Riding);
        assert_eq!(landed.len(), 1);
        assert_eq!(times(&landed[0]), [8950, 9000, 9450, 9500, 10_000, 10_300]);
        assert_eq!(
            landed[0]
                .1
                .iter()
                .map(|p| p.rider_state)
                .collect::<Vec<_>>(),
            [Riding, Airborne, Airborne, Riding, Riding, Riding]
        );
        all.extend(landed);
        // A wreck goes at once, with the tick before it.
        let wreck = batches(&mut s, &mut x, 10_500, 50, 20., Wreck);
        assert_eq!(wreck.len(), 1);
        assert_eq!(times(&wreck[0]), [10_450, 10_500]);
        all.extend(wreck);
        // At rest: one pose every five seconds, the first at once.
        let rest = batches(&mut s, &mut x, 10_550, 11_000, 0., Riding);
        assert_eq!(rest.len(), 3);
        assert!(rest.iter().all(|b| b.0 == 5000 && b.1.len() == 1));
        assert_eq!(times(&rest[2]), [20_550]);
        all.extend(rest);
        // Parked on a slope it creeps, and its wheels leave the ground and
        // touch again: still at rest, sent as standing, until it has slid
        // 1.5 ft from where it stopped.
        let mut creep = Vec::new();
        for (tick, time) in (21_550..31_550).step_by(50).enumerate() {
            let mut pose = riding(&s, time, x + 0.3 * (tick % 3) as f32, 0.8);
            if tick % 7 == 0 {
                pose.rider_state = Airborne;
            }
            if let Some(bytes) = s.emit(pose, false).unwrap().relay {
                s.sent(&bytes).unwrap();
                let Packet::Batch { poses, .. } = atv_net::decode(&bytes).unwrap() else {
                    panic!("a batch");
                };
                let Packet::Snapshot(pose) = atv_net::decode(&poses[0]).unwrap() else {
                    panic!("a pose");
                };
                assert_eq!((poses.len(), pose.velocity), (1, [0.; 3]));
                creep.push(time);
            }
        }
        assert_eq!(creep, [25_550, 30_550]);
        x += 2.;
        let slid = batches(&mut s, &mut x, 31_550, 50, 0.8, Riding);
        assert_eq!((slid.len(), slid[0].0), (1, 1500));
        // (Velocities are quantized on the wire.)
        assert!((slid[0].1[0].velocity[0] - 0.8).abs() < 0.1);
        all.extend(slid);
        // It stops again, then sets off: both sent at once (a stop within
        // 200 ms of the last batch waits that long).
        let rest = batches(&mut s, &mut x, 31_600, 4950, 0., Riding);
        assert_eq!(rest.len(), 1);
        assert_eq!((rest[0].0, times(&rest[0])), (5000, vec![31_750]));
        all.extend(rest);
        let off = batches(&mut s, &mut x, 36_550, 100, 20., Riding);
        assert_eq!((off.len(), off[0].0), (1, 1500));
        all.extend(off);
        // Rider 2 arrives on a build without batches, as far away: it gets
        // this rider's place at once, and then nothing waits longer than
        // that build smooths.
        signal(&mut s, 2, PRESENT, &[0], 36_650);
        s.receive(&pose_from(2, 1, 0, 5000.), 36_650).unwrap();
        let plain = batches(&mut s, &mut x, 36_650, 1400, 20., Riding);
        assert_eq!(plain.len(), 4);
        assert!(plain.iter().all(|b| b.0 == 350 && b.1.len() == 1));
        assert_eq!(times(&plain[0]), [36_650]);
        all.extend(plain);
        // The relay asks for twice the wait.
        signal(&mut s, 1, PACE, &[20], 38_050);
        let paced = batches(&mut s, &mut x, 38_050, 2100, 20., Riding);
        assert_eq!(paced.len(), 3);
        assert!(paced.iter().all(|b| b.0 == 700 && b.1.len() == 2));
        all.extend(paced);
        // Every pose is newer than the one before it, as the relay demands.
        let poses: Vec<&Snapshot> = all.iter().flat_map(|b| &b.1).collect();
        assert!(poses.windows(2).all(|w| {
            atv_net::sequence_newer(w[1].sequence, w[0].sequence) && w[1].time_ms > w[0].time_ms
        }));
        assert_eq!(s.counters.batches as usize, all.len() + creep.len());
        // A relay that does not take batches gets single poses.
        let mut old = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(
            &mut old,
            1,
            Lobby {
                public: true,
                ..lobby(&[(0, 1), (1, 1)], true)
            },
        );
        old.hello(LINKS, false).unwrap();
        old.ready().unwrap();
        signal(&mut old, 0, PRESENT, &[0], 0);
        assert_eq!(ride(&mut old, 0, 3000), (20, 0));
        assert_eq!(old.counters.batches, 0);
    }

    #[test]
    fn batches_from_other_riders_are_shown_one_interval_late() {
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        welcome(
            &mut s,
            1,
            Lobby {
                public: true,
                ..lobby(&[(0, 1), (1, 1)], true)
            },
        );
        let pose = |sequence: u32| {
            let mut p = Snapshot {
                peer: 0,
                sequence,
                time_ms: sequence * 100,
                atv_model: 1,
                rider_model: 0,
                velocity: [30., 0., 0.],
                animation: Some(Default::default()),
                ..Default::default()
            };
            p.atv.position[0] = sequence as f32 * 3.;
            atv_net::encode(&Packet::Snapshot(Box::new(p))).unwrap()
        };
        // Rider 0's poses of the last 300 ms arrive together.
        let batch = atv_net::encode(&Packet::Batch {
            interval_ms: 300,
            poses: vec![pose(1), pose(2), pose(3)],
        })
        .unwrap();
        s.receive(&batch, 1000).unwrap();
        assert_eq!(s.counters.batches_received, 1);
        assert_eq!(s.present(1000, None, &no_cars).len(), 1);
        assert!((s.delay_ms(0).unwrap() - 308.).abs() < 1.);
        // Shown where it was 308 ms before the newest pose: at the first.
        let shown = s.present(1016, None, &no_cars)[0].1.snapshot.atv.position[0];
        assert!(shown < 6., "{shown}");
        // Single poses again: the ordinary delay, within a few seconds.
        s.receive(&pose(4), 1100).unwrap();
        for now in (1116..4000).step_by(16) {
            s.present(now, None, &no_cars);
        }
        assert!(s.delay_ms(0).unwrap() < 130.);
    }

    #[test]
    fn hello_flags_follow_the_room_claim() {
        use atv_net::signal::{BATCHES, CPU_SOUNDS, DIRECT, FRAMES, MOTORCYCLES, PRESENT};
        let relay = serde_json::json!({"code": "ABCD1234", "links": true, "batches": true});
        let server = serde_json::json!({"code": "ABCD1234", "links": false, "batches": false, "frames": true, "motorcycles": true});
        let relay = Serves::from_claim(&relay);
        let server = Serves::from_claim(&server);
        assert!(relay.links && relay.batches && !relay.frames && relay.hello());
        assert!(!server.links && !server.batches && server.frames && server.hello());
        assert!(server.motorcycles && !relay.motorcycles);
        assert!(!Serves::from_claim(&serde_json::json!({"code": "X"})).hello());
        let flags = |serves: Serves, direct: bool| {
            let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
            welcome(&mut s, 1, lobby(&[(0, 1), (1, 1)], true));
            let Packet::Signal(m) = atv_net::decode(&s.hello(serves, direct).unwrap()).unwrap()
            else {
                panic!("hello")
            };
            (m.data[0], s.signalling(), s)
        };
        // The relay never hears FRAMES.
        let (bits, signalling, _) = flags(relay, false);
        assert_eq!((bits, signalling), (CPU_SOUNDS | BATCHES, true));
        assert_eq!(flags(relay, true).0 & FRAMES, 0);
        // The game server: frames, no links even where the browser could
        // open them, and no signalling, so poses keep going to every rider.
        let (bits, signalling, mut s) = flags(server, true);
        assert_eq!(
            (bits, signalling),
            (CPU_SOUNDS | FRAMES | MOTORCYCLES, false)
        );
        s.ready().unwrap();
        signal(&mut s, 0, PRESENT, &[DIRECT], 0);
        assert!(s.link_commands(10_000).is_empty());
    }

    #[test]
    fn a_server_without_links_or_batches_gets_single_poses() {
        let serves = Serves {
            frames: true,
            ..Default::default()
        };
        // In Free Ride, where the relay takes batches: the same poses as a
        // build that never said hello.
        let mut plain = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        let mut framed = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        for s in [&mut plain, &mut framed] {
            welcome(
                s,
                1,
                Lobby {
                    public: true,
                    ..lobby(&[(0, 1), (1, 1)], true)
                },
            );
        }
        framed.hello(serves, true).unwrap();
        for s in [&mut plain, &mut framed] {
            s.ready().unwrap();
            s.receive(&compact_pose(0, 1, 50, 0.), 0).unwrap();
        }
        let sent = ride(&mut framed, 0, 3000);
        assert_eq!(sent, ride(&mut plain, 0, 3000));
        assert!(sent.0 > 0 && sent.1 == 0, "{sent:?}");
        assert_eq!(framed.counters.batches, 0);
        assert!(framed.link_commands(10_000).is_empty());
    }

    /// Rider `peer`'s compact pose (what frames carry).
    fn compact_pose(peer: u8, sequence: u32, time_ms: u32, x: f32) -> Vec<u8> {
        atv_net::encode(&Packet::Snapshot(Box::new(compact(
            peer, sequence, time_ms, x,
        ))))
        .unwrap()
    }

    fn compact(peer: u8, sequence: u32, time_ms: u32, x: f32) -> Snapshot {
        let mut p = Snapshot {
            peer,
            sequence,
            time_ms,
            atv_model: 1,
            rider_model: 0,
            velocity: [20., 0., 0.],
            animation: Some(Default::default()),
            ..Default::default()
        };
        p.atv.position[0] = x;
        p.rider.position[0] = x;
        p
    }

    #[test]
    fn frames_feed_each_pose_as_if_it_had_come_alone() {
        let room = || {
            let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
            welcome(&mut s, 1, lobby(&[(0, 1), (1, 1), (2, 1)], true));
            s
        };
        let (mut single, mut framed) = (room(), room());
        for tick in 1..=20u32 {
            let now = u64::from(tick) * 50;
            let poses: Vec<Vec<u8>> = [0, 2]
                .iter()
                .map(|&peer| compact_pose(peer, tick, tick * 50, tick as f32 + f32::from(peer)))
                .collect();
            for pose in &poses {
                single.receive(pose, now).unwrap();
            }
            let frame = atv_net::encode(&Packet::Frame { poses }).unwrap();
            framed.receive(&frame, now).unwrap();
        }
        assert_eq!(framed.counters.frames_received, 20);
        for now in (1000..1400).step_by(16) {
            let a = single.present(now, None, &no_cars);
            let b = framed.present(now, None, &no_cars);
            assert_eq!(a.len(), 2);
            assert_eq!(
                a.iter()
                    .map(|(p, r)| (*p, r.snapshot.atv.position))
                    .collect::<Vec<_>>(),
                b.iter()
                    .map(|(p, r)| (*p, r.snapshot.atv.position))
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(single.delay_ms(0), framed.delay_ms(0));
        // A delta on the rider's latest pose here applies; one on a pose
        // this client never got waits for the next full one. This rider's
        // own pose and a stranger's are ignored, as from the relay.
        let base = compact(0, 20, 1000, 20.);
        let delta = |base: &Snapshot, sequence: u32, x: f32| {
            let next = compact(0, sequence, sequence * 50, x);
            atv_net::encode(&Packet::Delta(Box::new(
                atv_net::delta(&next, base).unwrap(),
            )))
            .unwrap()
        };
        let mut missed = base.clone();
        missed.sequence = 19;
        let frame = atv_net::encode(&Packet::Frame {
            poses: vec![
                delta(&missed, 23, 99.),
                delta(&base, 21, 21.5),
                compact_pose(1, 50, 2000, 7.),
                compact_pose(9, 50, 2000, 7.),
            ],
        })
        .unwrap();
        framed.receive(&frame, 1050).unwrap();
        let latest = framed.buffers[&0].latest().unwrap().0;
        assert_eq!(latest.sequence, 21);
        assert!((latest.atv.position[0] - 21.5).abs() < 0.01);
        assert!(!framed.buffers.contains_key(&1) && !framed.buffers.contains_key(&9));
    }

    #[test]
    fn pong_sets_round_trip_and_clock() {
        let mut s = Session::new(Profile::new("Ann", 1, 0, Default::default()));
        let pong = atv_net::encode(&Packet::Pong {
            nonce: 1000,
            server_ms: 50_000,
        })
        .unwrap();
        s.receive(&pong, 1040).unwrap();
        assert_eq!(s.rtt_ms, Some(40));
        assert_eq!(s.server_now(1040), Some(50_020.));
    }

    #[test]
    fn riders_on_a_car_are_drawn_relative_to_this_clients_car() {
        let at = |x: f32| {
            let mut m = [0.; 16];
            m[0] = 1.;
            m[5] = 1.;
            m[10] = 1.;
            m[15] = 1.;
            m[12] = x;
            m
        };
        let mut s = Snapshot {
            atv: atv_net::Transform::from_column_major(at(105.)).unwrap(),
            rider: atv_net::Transform::from_column_major(at(105.)).unwrap(),
            platform: Some(Box::new(atv_net::Platform {
                car: 3,
                transform: atv_net::Transform::from_column_major(at(100.)).unwrap(),
            })),
            ..Default::default()
        };
        // The sender's car is 2 ft behind this client's: the rider moves along.
        rebase(&mut s, &|car| (car == 3).then(|| at(102.)));
        assert!((s.atv.position[0] - 107.).abs() < 1e-4);
        assert!((s.rider.position[0] - 107.).abs() < 1e-4);
        // Another car (or no train) leaves the pose alone.
        rebase(&mut s, &no_cars);
        assert!((s.atv.position[0] - 107.).abs() < 1e-4);
    }

    #[test]
    fn map_ids_follow_the_relay_catalog() {
        assert_eq!(map_id("env01").unwrap(), 1);
        assert_eq!(map_id("FREE05").unwrap(), 9);
        assert!(map_id("nowhere").is_err());
    }
}
