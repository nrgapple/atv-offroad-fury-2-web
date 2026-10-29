//! Multiplayer rooms for the Rust engine: the relay session (join, roster,
//! clock, adaptive pose cadence with compact deltas) and remote riders drawn
//! with their own quad and rider models.
//!
//! Transport-independent: the host moves bytes over a WebSocket (web-sys in
//! the browser, a native client on desktop) and calls [`Session`] with them.
//! This ports the protocol half of atv-web's networking adapter
//! (crates/atv-web/src/multiplayer.rs ops 80-85, 109-111, 118) and the remote
//! presentation of multiplayer_animation.rs; web/multiplayer.js stays the
//! JavaScript game's socket glue. The wire format, codec, deltas and
//! smoothing are atv-net's, shared with the JavaScript game and the relay.
pub mod remote;
pub use atv_net::RiderState;
pub mod spawn;

use crate::Result;
use crate::ride::Ride;
pub use atv_net::{
    GameClaim, GameMarks, GameState, MAX_GAME_POINTS, MAX_MARKS, NO_PEER, RACE_DNF, RACE_MAX_MS,
    RACE_RIDERS, RaceInfo, RaceProgress, claim_kind, game_kind, game_phase, race_phase,
};
use atv_net::{Lobby, Packet, Participant, PoseBuffer, SampledPose, Snapshot};
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
    /// ATV 1-26 (rooms carry no motorcycles; see [`Profile::new`]).
    pub quad: u16,
    /// 0 male, 1 female.
    pub rider: u16,
    /// Gear indices and quad paint (0 = original), as the JavaScript sends.
    pub appearance: [u8; atv_net::APPEARANCE_BYTES],
}

impl Profile {
    /// Motorcycles fall back to ATV01 in rooms, as in the JavaScript game
    /// (remote riders are drawn on quads only).
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
            quad: if (1..=26).contains(&quad) { quad } else { 1 },
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
    /// riders whose Ready matches the current map.
    pub fn ready(&self) -> Result<Vec<u8>> {
        let map = self.lobby.as_ref().ok_or("no lobby")?.map;
        encode(&Packet::Ready { map })
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
        }
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
            Packet::Snapshot(s) => {
                let allowed = self.lobby.as_ref().is_some_and(|l| {
                    l.started
                        && l.players.iter().any(|p| {
                            p.id == s.peer
                                && p.atv_model == s.atv_model
                                && p.rider_model == s.rider_model
                        })
                });
                if allowed && Some(s.peer) != self.peer {
                    let buffer = self.buffers.entry(s.peer).or_default();
                    if buffer.push((*s).clone(), now_ms).is_err() {
                        // One rider's pose history (not the wire packet, which
                        // decoded and validated) disagrees with this pose:
                        // restart that rider's timeline rather than the room.
                        buffer.clear();
                        buffer.push(*s, now_ms).map_err(|e| e.to_string())?;
                    }
                }
            }
            Packet::Leave { peer } => {
                self.buffers.remove(&peer);
                self.progress.remove(&peer);
            }
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
            // Room King of the Hills / Treasure Hunt reports: kept for the
            // host's game, which validates them (a bounded queue).
            Packet::Claim(claim) => {
                if self.claims.len() < 64 {
                    self.claims.push(claim);
                }
            }
            _ => return Err("unexpected relay packet".into()),
        }
        Ok(())
    }

    /// The player's pose for the room, or None when the adaptive cadence
    /// suppresses it (atv-web op 81). Call every 50 ms; after the bytes are
    /// actually sent, confirm them with [`Session::sent`] so deltas only
    /// reference delivered baselines.
    pub fn snapshot(&mut self, ride: &Ride, now_ms: u64, paused: bool) -> Result<Option<Vec<u8>>> {
        let now = now_ms as u32;
        let peer = self.peer.ok_or("join multiplayer first")?;
        let profile = self.profile.as_ref().ok_or("no multiplayer profile")?;
        // Alone in the room (atv-web sends nothing): a presence pose at most
        // once a second (paused: every 5 s) keeps the relay's copy current,
        // which it hands the next arrival on Ready. Without it a host whose
        // tab is in the background (throttled timers) has no pose to meet
        // until long after a guest has loaded, who then starts on top of it.
        let solo = self.lobby.as_ref().is_some_and(|l| l.players.len() < 2);
        let frame = ride.frame();
        let recipe = ride
            .rider_recipe()
            .ok_or("multiplayer rides an ATV (motorcycles fall back to ATV01)")?;
        let wreck = ride.rider_wreck();
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
        let s = Snapshot {
            peer,
            sequence: self.sequence.wrapping_add(1),
            time_ms: now,
            atv_model: profile.quad,
            rider_model: profile.rider,
            atv,
            velocity: zero(frame.linear_velocity),
            angular_velocity: zero(angular_velocity),
            animation: Some(Box::new(atv_net::AnimationState {
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
            })),
            atv_nodes: Vec::new(),
            rider_nodes: Vec::new(),
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
        };
        let nearest = self
            .buffers
            .values()
            .filter_map(|b| b.latest())
            .map(|(p, _)| distance(p.atv.position, s.atv.position))
            .fold(f32::INFINITY, f32::min);
        let transition = self.baseline.as_ref().is_none_or(|b| {
            b.rider_state != s.rider_state
                || b.crash_serial != s.crash_serial
                || b.animation.as_ref().is_some_and(|a| a.paused != paused)
                || b.platform.as_ref().map(|p| p.car) != s.platform.as_ref().map(|p| p.car)
                || distance(b.atv.position, s.atv.position) > 16.
        });
        let elapsed = now.wrapping_sub(self.last_sent_ms);
        let moving = s
            .velocity
            .iter()
            .chain(s.rider_velocity.iter())
            .map(|v| v * v)
            .sum::<f32>()
            > 0.25;
        // Up to 20 Hz near, fast, airborne or wrecked; 10 Hz moving; 5 Hz
        // beyond 300 ft; 1 Hz stationary; 0.2 Hz paused.
        let interval = if paused {
            5000
        } else if !moving && state == RiderState::Riding {
            1000
        } else if state == RiderState::Wreck
            || state == RiderState::Airborne
            || s.speed > 35.
            || nearest < 40.
        {
            50
        } else if nearest > 300. {
            200
        } else {
            100
        };
        let interval = if solo { interval.max(1000) } else { interval };
        if !self.force_full && (solo || !transition) && elapsed < interval {
            self.counters.suppressed += 1;
            return Ok(None);
        }
        self.sequence = s.sequence;
        let full = self.force_full
            || transition
            || now.wrapping_sub(self.last_full_ms) >= 2000
            || self.baseline.is_none();
        let packet = match (&self.baseline, full) {
            (Some(baseline), false) => {
                let patch = atv_net::delta(&s, baseline).map_err(|e| e.to_string())?;
                // An all-block patch is no smaller than independent full
                // state; spare the relay the baseline reconstruction.
                if patch.mask == 15 {
                    Packet::Snapshot(Box::new(s.clone()))
                } else {
                    Packet::Delta(Box::new(patch))
                }
            }
            _ => Packet::Snapshot(Box::new(s.clone())),
        };
        let output = encode(&packet)?;
        self.pending = Some(s);
        self.counters.last_packet_bytes = output.len();
        Ok(Some(output))
    }

    /// Send receipt for [`Session::snapshot`] bytes: the baseline advances
    /// only after an actual send (dropped frames never become a reference).
    pub fn sent(&mut self, bytes: &[u8]) -> Result<()> {
        let Some(s) = self.pending.take() else {
            return Ok(());
        };
        let kind = bytes.get(5).copied().unwrap_or(0);
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

    /// The rider's own room-game report as a packet.
    pub fn claim_packet(&self, claim: atv_net::GameClaim) -> Result<Vec<u8>> {
        let peer = self.peer.ok_or("join multiplayer first")?;
        encode(&Packet::Claim(atv_net::GameClaim { peer, ..claim }))
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
    fn profile_keeps_rooms_on_quads() {
        let p = Profile::new("  ", 30, 3, Default::default());
        assert_eq!((p.name.as_str(), p.quad, p.rider), ("Rider", 1, 1));
        assert_eq!(Profile::new("Ann", 14, 0, Default::default()).quad, 14);
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
