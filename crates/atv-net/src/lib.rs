//! Authored browser multiplayer wire format and remote presentation smoothing.
//!
//! Transport independent: both the WASM client and Rust relay validate the same
//! bounded messages. World coordinates retain the game's feet convention, time
//! is milliseconds, and all quaternions use the core's W,X,Y,Z component order.
//! This is a new networking baseline, not recovered PS2 network code.
// Presentation only: not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]

use serde::{Deserialize, Serialize};

mod codec;
mod compact;
mod names;
pub mod quickplay;
pub use compact::{AnimationState, Delta, STEERING_LIMIT, apply_delta, delta};
pub use names::{offensive_name, public_name};
mod smoothing;
mod transform;

pub use codec::{decode, encode};
pub use smoothing::{PoseBuffer, SENDER_JUMP_MS, SampledPose, SmoothingConfig};
pub use transform::Transform;

pub const VERSION: u8 = 6;
/// Protocol bound on participant slots. Private rooms keep the original four
/// riders ([`PRIVATE_CAPACITY`]); shared Arlington Free Ride rooms hold more.
pub const MAX_PLAYERS: usize = 20;
pub const PRIVATE_CAPACITY: usize = 4;
pub const PUBLIC_CAPACITY: usize = 20;
/// Shared Free Ride rooms ride Arlington Stream (`env08`) only.
pub const PUBLIC_MAP: u16 = 8;
/// Shared rooms are numbered from 1; a full room opens the next one.
pub const PUBLIC_ROOMS: u8 = 25;
const PUBLIC_PREFIX: &[u8; 6] = b"FREERD";
/// One-based lobby IDs reference this authored catalog; source resource keys
/// remain stable even when their browser display names change.
pub const MAP_ENVIRONMENTS: [&str; 40] = [
    "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free05", "free01",
    "free02", "free03", "free04", "free06", "nat01", "nat02", "nat03", "nat04", "nat05", "nat06",
    "nat07", "nat08", "sx01", "sx02", "sx03", "sx04", "sx05", "sx06", "sx07", "sx08", "sx09",
    "sx10", "sx11", "sx12", "sx13", "sx14", "sx15", "sx16", "training", "game01",
];
pub const MAX_MAP_ID: u16 = MAP_ENVIRONMENTS.len() as u16;
pub const MAX_NODES: usize = 64;
pub const MAX_PACKET_BYTES: usize = 8192;
pub const MAX_NAME_BYTES: usize = 32;
pub const ROOM_CODE_BYTES: usize = 8;
pub type PeerId = u8;
pub type RoomCode = [u8; ROOM_CODE_BYTES];
/// Per-participant secret returned only to its owner. Presenting it again
/// within the reconnect grace reclaims the same slot (and color).
pub type RejoinKey = [u8; 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Truncated,
    InvalidHeader,
    UnsupportedVersion,
    UnknownKind,
    InvalidLength,
    InvalidValue,
    InvalidTransform,
    InvalidRoomCode,
    InvalidUtf8,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "multiplayer packet: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// Eight uppercase letters or digits. Generated codes omit ambiguous glyphs.
pub fn valid_room_code(code: &RoomCode) -> bool {
    code.iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// User-entered codes accept surrounding whitespace and ASCII lowercase. The
/// on-wire and URL form remains exactly eight uppercase letters or digits.
pub fn parse_room_code(value: &str) -> Result<RoomCode> {
    let uppercase = value.trim().to_ascii_uppercase();
    let code: RoomCode = uppercase
        .as_bytes()
        .try_into()
        .map_err(|_| Error::InvalidRoomCode)?;
    if !valid_room_code(&code) {
        return Err(Error::InvalidRoomCode);
    }
    Ok(code)
}

/// Code of shared Free Ride room `number` (1-based), for example `FREERD01`.
pub fn public_room_code(number: u8) -> Option<RoomCode> {
    (1..=PUBLIC_ROOMS).contains(&number).then(|| {
        let mut code = [0; ROOM_CODE_BYTES];
        code[..6].copy_from_slice(PUBLIC_PREFIX);
        code[6] = b'0' + number / 10;
        code[7] = b'0' + number % 10;
        code
    })
}
/// Shared room codes are reserved: they cannot be hosted or reopened privately.
pub fn is_public_room_code(code: &RoomCode) -> bool {
    code.starts_with(PUBLIC_PREFIX)
}

pub fn sequence_newer(candidate: u32, previous: u32) -> bool {
    let delta = candidate.wrapping_sub(previous);
    delta != 0 && delta < 0x8000_0000
}
pub fn map_environment(id: u16) -> Option<&'static str> {
    id.checked_sub(1)
        .and_then(|index| MAP_ENVIRONMENTS.get(usize::from(index)).copied())
}
pub fn next_map(id: u16) -> Option<u16> {
    valid_map(id).then(|| if id == MAX_MAP_ID { 1 } else { id + 1 })
}
pub fn valid_map(id: u16) -> bool {
    (1..=MAX_MAP_ID).contains(&id)
}

pub fn valid_peer(id: PeerId) -> bool {
    (id as usize) < MAX_PLAYERS
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub id: PeerId,
    pub name: String,
    pub atv_model: u16,
    pub rider_model: u16,
    /// Rider gear catalog indices (helmet, glove, boot, shirt, sleeve, pant,
    /// goggle; 0 keeps the original) followed by the quad paint index.
    #[serde(default)]
    pub appearance: [u8; APPEARANCE_BYTES],
}
pub const APPEARANCE_BYTES: usize = 8;
pub fn valid_appearance(a: &[u8; APPEARANCE_BYTES]) -> bool {
    a[..7].iter().all(|g| *g <= 64) && a[7] < 16
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lobby {
    pub host: PeerId,
    /// Changes when host changes; clients reject stale host commands.
    pub epoch: u32,
    pub map: u16,
    pub started: bool,
    /// Shared Free Ride room: no host controls, fixed map, larger capacity.
    #[serde(default)]
    pub public: bool,
    pub players: Vec<Participant>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum RiderState {
    #[default]
    Riding = 0,
    Airborne = 1,
    Wreck = 2,
    Recovering = 3,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Platform {
    pub car: u8,
    /// Sender's platform transform at this snapshot, used to preserve relative pose.
    pub transform: Transform,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub animation: Option<Box<AnimationState>>,
    pub peer: PeerId,
    pub sequence: u32,
    pub time_ms: u32,
    pub atv_model: u16,
    pub rider_model: u16,
    pub atv: Transform,
    pub velocity: [f32; 3],
    /// World angular velocity in radians per second.
    pub angular_velocity: [f32; 3],
    /// Model-space node poses; never interpolate these as hierarchical locals.
    pub atv_nodes: Vec<Transform>,
    pub rider_nodes: Vec<Transform>,
    pub rider_state: RiderState,
    pub rider: Transform,
    pub rider_velocity: [f32; 3],
    pub rpm: f32,
    pub throttle: f32,
    pub speed: f32,
    /// Increment once per crash; receivers deduplicate sound/particle events.
    pub crash_serial: u32,
    #[serde(default)]
    pub train_time: Option<f32>,
    #[serde(default)]
    pub train_running: bool,
    #[serde(default)]
    pub platform: Option<Box<Platform>>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            animation: None,
            peer: 0,
            sequence: 0,
            time_ms: 0,
            atv_model: 1,
            rider_model: 1,
            atv: Transform::default(),
            velocity: [0.; 3],
            angular_velocity: [0.; 3],
            atv_nodes: Vec::new(),
            rider_nodes: Vec::new(),
            rider_state: RiderState::Riding,
            rider: Transform::default(),
            rider_velocity: [0.; 3],
            rpm: 0.,
            throttle: 0.,
            speed: 0.,
            crash_serial: 0,
            train_time: None,
            train_running: false,
            platform: None,
        }
    }
}
/// A room mini-game's host-owned state (research/GAMES.md "Online"): the
/// host runs the rules and the puck/ball and broadcasts this; riders still
/// own their own ATVs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameState {
    /// 1 Hockey, 2 Tag, 3 Race (research/MULTIPLAYER.md "Room races").
    pub game: u8,
    /// Increments each change of `phase` (face-offs, goals).
    pub serial: u32,
    /// The game's phase code (Hockey: 0 face-off, 1 play, 2 sudden death,
    /// 3 pause, 4 over).
    pub phase: u8,
    /// Phase countdown (face-off or pause seconds left).
    pub timer: f32,
    /// Game clock seconds left.
    pub clock: f32,
    /// Hockey: Red and Blue scores; Tag: points by roster slot.
    pub scores: [u32; 4],
    /// Banner code (0 none) and its seconds left.
    pub banner: u8,
    pub banner_time: f32,
    /// Hockey: the scoring rider; Tag: the carrier (`NO_PEER` for none).
    pub holder: PeerId,
    /// The puck or ball.
    pub ball: [f32; 3],
    pub ball_velocity: [f32; 3],
    pub ball_yaw: f32,
    /// Race only (game 3); encoded after the shared fields.
    #[serde(default)]
    pub race: RaceInfo,
    /// King of the Hills (game 4) and Treasure Hunt (game 5) only; encoded
    /// after the shared fields (research/MULTIPLAYER.md "Room games").
    #[serde(default)]
    pub marks: GameMarks,
}
/// Room King of the Hills / Treasure Hunt state beyond the shared fields.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameMarks {
    /// The course's archive venue (`env05`): 0-7 env01-08, 8-13 free01-06.
    pub venue: u8,
    /// The slots playing this game (bit N for slot N): riders present at
    /// GO, less leavers. A rider who joins after GO spectates (no pickups,
    /// no captures, not in the results) until the next game.
    pub players: u8,
    /// Each slot's committed stunt score (the results' tie-break).
    pub stunts: [f32; RACE_RIDERS],
    /// King of the Hills: one per hill (owner slot, score to beat).
    /// Treasure Hunt: one per treasure (collector slot, 1 when in the shown
    /// group else 0). `NO_PEER` is no owner.
    pub marks: Vec<(PeerId, f32)>,
}
/// Most hills or treasures a room game carries (the TOY parser caps a
/// course at 64 hills and 64 groups of triggers).
pub const MAX_MARKS: usize = 512;
/// Highest stunt or hill score a room game accepts.
pub const MAX_GAME_POINTS: f32 = 100_000_000.;
/// Room game codes (GameState::game).
pub mod game_kind {
    pub const HOCKEY: u8 = 1;
    pub const TAG: u8 = 2;
    pub const RACE: u8 = 3;
    pub const KING_OF_THE_HILLS: u8 = 4;
    pub const TREASURE_HUNT: u8 = 5;
    /// Room Freestyle (research/MULTIPLAYER.md "Room Freestyle"): the host's
    /// score table. `GameMarks` carries one mark per slot (slot, best
    /// combo points), `stunts` each slot's session stunt score and `scores`
    /// each slot's different combos; `venue` is unused (0).
    pub const FREESTYLE: u8 = 6;
}
/// Most slots a room Freestyle table carries (one mark per room slot).
pub const FREESTYLE_MARKS: usize = RACE_RIDERS;
/// Room King of the Hills / Treasure Hunt phases (GameState::phase).
pub mod game_phase {
    /// Riders load the course; `timer` is the start countdown.
    pub const COUNTDOWN: u8 = 0;
    pub const PLAY: u8 = 1;
    /// The rules completed (all treasures): the finish sign holds.
    pub const COMPLETE: u8 = 2;
    /// Results: `scores` are final.
    pub const OVER: u8 = 3;
}
/// A rider's room-game report to the host (any rider; the relay stamps
/// `peer` and forwards it to the host only).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct GameClaim {
    pub peer: PeerId,
    /// The game (GameState serial) it belongs to; 0 announces that the
    /// sender understands room games 4 and 5 (kind 2: and room Freestyle).
    pub serial: u32,
    /// 0 a stunt-score report; 1 a King of the Hills capture of hill
    /// `index` (its 1-based number) with `points`; 2 a room Freestyle
    /// report: `stunt` the session stunt score, `points` the best combo,
    /// `index` the different combos landed.
    pub kind: u8,
    pub index: u16,
    pub points: f32,
    /// The sender's committed stunt score.
    pub stunt: f32,
}
pub mod claim_kind {
    pub const STUNT: u8 = 0;
    pub const CAPTURE: u8 = 1;
    pub const FREESTYLE: u8 = 2;
}
impl GameClaim {
    pub fn validate(&self) -> Result<()> {
        if !valid_peer(self.peer)
            || self.kind > claim_kind::FREESTYLE
            || self.index as usize > MAX_MARKS
            || (self.kind == claim_kind::FREESTYLE && self.points > self.stunt)
            || !self.points.is_finite()
            || !(0. ..=MAX_GAME_POINTS).contains(&self.points)
            || !self.stunt.is_finite()
            || !(0. ..=MAX_GAME_POINTS).contains(&self.stunt)
        {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
}
/// A room race's host-chosen setup (game 3): the course variant on the
/// room's map, the lap count and each private-room slot's start slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RaceInfo {
    /// 0 National/Supercross, 1 Enduro long, 2 Enduro short, 3 Short Track.
    pub variant: u8,
    pub laps: u8,
    /// Start slot (1…riders) by peer id; 0 = not in this race.
    pub grid: [u8; RACE_RIDERS],
    /// CPU riders the host simulates (research/MULTIPLAYER.md "Room race
    /// CPUs"); 0 for a human-only race. Encoded only when nonzero.
    pub cpus: u8,
    /// Start slot of each CPU (index < `cpus`), 0 beyond.
    pub cpu_grid: [u8; RACE_CPUS],
    /// The host's roster draw: its ATV (1-26, picks the CPU quad group),
    /// rider (0 male, 1 female: the CPU riders' names and outfits) and
    /// difficulty (1 Normal, 2 Expert), so every peer draws the same field.
    pub roster_quad: u8,
    pub roster_rider: u8,
    pub difficulty: u8,
    /// Results for each CPU, as `GameState::scores` for the riders.
    pub cpu_scores: [u32; RACE_CPUS],
}
/// Riders in a room race: a private room's capacity (peer ids 0…3).
pub const RACE_RIDERS: usize = 4;
/// Most CPU riders in a room race (the original race's four CPUs).
pub const RACE_CPUS: usize = 4;
/// A room-race CPU's id in standings and motion: `CPU_PEER + index`, after
/// the four room slots (never a room participant's id in a private room).
pub const CPU_PEER: PeerId = RACE_RIDERS as PeerId;
/// One CPU rider's motion and progress in [`CpuMotion`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpuRider {
    /// CPU index (0 … `RaceInfo::cpus` - 1).
    pub index: u8,
    pub lap: u8,
    /// Race distance (ft), as [`RaceProgress::distance`].
    pub distance: f32,
    /// Finish time (ms of race clock), 0 while racing.
    pub finish_ms: u32,
    /// Compact pose (`peer` = [`CPU_PEER`] + index, with animation).
    pub pose: Snapshot,
    /// The CPU's sound cues since the host's previous [`CpuMotion`]; None
    /// from an older host, and stripped by the relay for riders that did not
    /// say [`signal::CPU_SOUNDS`].
    #[serde(default)]
    pub sound: Option<CpuSoundCues>,
}
/// A room-race CPU's sound events over one [`CpuMotion`] interval
/// (research/AUDIO.md "CPU riders"): what a guest cannot infer from the
/// pose. Magnitudes are ft/s and travel in sixteenths (0 … 4095.9).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct CpuSoundCues {
    /// [`cpu_cue`] bits.
    pub flags: u8,
    /// Strongest quad-to-quad contact (0 = none).
    pub quad_contact: f32,
    /// The wreck entry's local impact (with [`cpu_cue::WRECK`]).
    pub wreck_impact: f32,
    /// Strongest sampled world impact of the quad (+d0).
    pub impact: f32,
    /// Strongest detached rider body contact delta magnitude.
    pub rider_impact: f32,
    /// The two side drive-on emitters' terrain bytes at the interval's end
    /// (-1 airborne, 8 under water).
    pub terrain: [i8; 2],
}
/// [`CpuSoundCues::flags`] bits.
pub mod cpu_cue {
    /// A hard landing (the rider's Jostle vocal).
    pub const HARD_LANDING: u8 = 1;
    /// The quad wrecked (entry impact in `wreck_impact`).
    pub const WRECK: u8 = 2;
    /// That wreck was a fall (30 ft drop or boundary launch).
    pub const FALL: u8 = 4;
    /// More than 2 ft above the ground at the interval's end.
    pub const AIRBORNE: u8 = 8;
    /// The chassis collider touched something.
    pub const CHASSIS_CONTACT: u8 = 16;
    /// The detached rider body touches water at the interval's end.
    pub const RIDER_WATER: u8 = 32;
    pub const ALL: u8 = 63;
}
/// Largest cue magnitude the wire carries (u16 sixteenths).
pub const CUE_MAX: f32 = 65_535. / 16.;
impl CpuSoundCues {
    pub fn validate(&self) -> Result<()> {
        let magnitudes = [
            self.quad_contact,
            self.wreck_impact,
            self.impact,
            self.rider_impact,
        ];
        if self.flags & !cpu_cue::ALL != 0
            || magnitudes
                .iter()
                .any(|m| !m.is_finite() || !(0. ..=CUE_MAX).contains(m))
            || self.terrain.iter().any(|t| *t < -1)
        {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
}
/// The room-race host's CPU riders (kind 16; relayed only from the host of
/// a started private room to race-aware riders).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CpuMotion {
    /// The race (GameState serial).
    pub serial: u32,
    pub riders: Vec<CpuRider>,
}
impl CpuMotion {
    pub fn validate(&self) -> Result<()> {
        let mut seen = 0u8;
        if self.serial == 0 || self.riders.len() > RACE_CPUS {
            return Err(Error::InvalidValue);
        }
        for r in &self.riders {
            if r.index as usize >= RACE_CPUS
                || seen & (1 << r.index) != 0
                || r.pose.peer != CPU_PEER + r.index
                || r.pose.animation.is_none()
                || r.lap > 10
                || !r.distance.is_finite()
                || !(-10_000. ..=1_000_000.).contains(&r.distance)
                || r.finish_ms > RACE_MAX_MS
            {
                return Err(Error::InvalidValue);
            }
            seen |= 1 << r.index;
            r.pose.validate()?;
            if let Some(cues) = &r.sound {
                cues.validate()?;
            }
        }
        Ok(())
    }
}
/// Room race phases (GameState::phase for game 3).
pub mod race_phase {
    /// Riders load the course and wait on the grid.
    pub const GRID: u8 = 0;
    /// 3-2-1-GO: `timer` is the seconds left to GO.
    pub const COUNTDOWN: u8 = 1;
    pub const RACING: u8 = 2;
    /// Final: `scores` hold finish times (ms) by peer id; riders who did
    /// not finish hold `RACE_DNF + n`, the host's order among them; 0 is a
    /// slot outside the race. Ascending scores are the agreed finish order.
    pub const RESULTS: u8 = 3;
}
/// Base of a room-race result for a rider who did not finish.
pub const RACE_DNF: u32 = u32::MAX - 16;
/// Finish times above this (ms) are invalid (two hours).
pub const RACE_MAX_MS: u32 = 7_200_000;
/// A rider's own race progress (any rider; the relay stamps `peer`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RaceProgress {
    pub peer: PeerId,
    /// The race (GameState serial) this progress belongs to; 0 announces
    /// that the sender understands room races.
    pub serial: u32,
    /// Laps completed.
    pub lap: u8,
    /// Race distance from the grid along the racing line (ft).
    pub distance: f32,
    /// Finish time (ms of race clock), 0 while racing.
    pub finish_ms: u32,
}
impl RaceProgress {
    pub fn validate(&self) -> Result<()> {
        if !valid_peer(self.peer)
            || self.lap > 10
            || !self.distance.is_finite()
            || !(-10_000. ..=1_000_000.).contains(&self.distance)
            || self.finish_ms > RACE_MAX_MS
        {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
}
pub const NO_PEER: PeerId = 255;
impl GameState {
    pub fn validate(&self) -> Result<()> {
        if self.game == 3 {
            let r = &self.race;
            let cpus = usize::from(r.cpus);
            let field = RACE_RIDERS + cpus;
            if r.variant > 3
                || !(1..=5).contains(&r.laps)
                || cpus > RACE_CPUS
                || r.grid.iter().any(|&g| g as usize > field)
                || r.cpu_grid
                    .iter()
                    .enumerate()
                    .any(|(i, &g)| (i < cpus) != (g > 0) || g as usize > field)
                || (cpus > 0
                    && (!(1..=26).contains(&r.roster_quad)
                        || r.roster_rider > 1
                        || !(1..=2).contains(&r.difficulty)))
                || (cpus == 0 && (r.roster_quad != 0 || r.roster_rider != 0 || r.difficulty != 0))
                || r.cpu_scores
                    .iter()
                    .enumerate()
                    .any(|(i, &s)| (s > RACE_MAX_MS && s < RACE_DNF) || (i >= cpus && s != 0))
                || self.phase > race_phase::RESULTS
                || self.scores.iter().any(|&s| s > RACE_MAX_MS && s < RACE_DNF)
            {
                return Err(Error::InvalidValue);
            }
        }
        if matches!(
            self.game,
            game_kind::KING_OF_THE_HILLS | game_kind::TREASURE_HUNT
        ) && (self.phase > game_phase::OVER
            || self.marks.venue > 13
            || self.marks.players >> RACE_RIDERS != 0
            || self.marks.marks.len() > MAX_MARKS
            || self
                .marks
                .stunts
                .iter()
                .any(|s| !s.is_finite() || !(0. ..=MAX_GAME_POINTS).contains(s))
            || self.marks.marks.iter().any(|(owner, v)| {
                (*owner != NO_PEER && *owner as usize >= RACE_RIDERS)
                    || !v.is_finite()
                    || !(0. ..=MAX_GAME_POINTS).contains(v)
            })
            || self.scores.iter().any(|&s| s as usize > MAX_MARKS)
            || (self.holder != NO_PEER && self.holder as usize >= RACE_RIDERS))
        {
            return Err(Error::InvalidValue);
        }
        // Room Freestyle: one mark per slot, owned by that slot (or none),
        // its best combo no more than its session score.
        if self.game == game_kind::FREESTYLE
            && (self.phase > game_phase::OVER
                || self.marks.venue != 0
                || self.marks.players >> RACE_RIDERS != 0
                || self.marks.marks.len() > FREESTYLE_MARKS
                || self
                    .marks
                    .stunts
                    .iter()
                    .any(|s| !s.is_finite() || !(0. ..=MAX_GAME_POINTS).contains(s))
                || self
                    .marks
                    .marks
                    .iter()
                    .enumerate()
                    .any(|(slot, (owner, v))| {
                        (*owner != NO_PEER && *owner as usize != slot)
                            || !v.is_finite()
                            || !(0. ..=MAX_GAME_POINTS).contains(v)
                            || *v > self.marks.stunts[slot]
                    })
                || self.scores.iter().any(|&s| s as usize > MAX_MARKS)
                || self.holder != NO_PEER)
        {
            return Err(Error::InvalidValue);
        }
        if !(1..=6).contains(&self.game)
            || self.phase > 8
            || self.banner > 16
            || (self.holder != NO_PEER && !valid_peer(self.holder))
            || !bounded(&[self.timer, self.clock, self.banner_time], 100_000.)
            || !bounded(&self.ball, 1_000_000.)
            || !bounded(&self.ball_velocity, 100_000.)
            || !bounded(&[self.ball_yaw], 1_000_000.)
        {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
}

/// Direct-link signalling (research/MULTIPLAYER.md "Direct links").
/// `Signal::kind` values; riders exchange session descriptions through the
/// relay and then send poses to each other without it.
pub mod signal {
    /// Client to relay: this build understands signalling. `data` is one
    /// flags byte ([`DIRECT`] when the browser can open direct links).
    pub const HELLO: u8 = 0;
    /// A session description offer for `peer`.
    pub const OFFER: u8 = 1;
    /// The answer to `peer`'s offer.
    pub const ANSWER: u8 = 2;
    /// Relay to an arriving rider: `peer` is already riding this course.
    /// `data` is that rider's flags byte.
    pub const PRESENT: u8 = 3;
    /// Relay to riders on a course: `peer` just arrived. `data` is the
    /// arrival's flags byte.
    pub const ARRIVED: u8 = 4;
    /// Relay to riders: `peer` lost its connection and stopped riding while
    /// its roster slot is held. `data` is a zero flags byte.
    pub const GONE: u8 = 5;
    /// Relay to a rider: `peer` no longer reports its link to you, so the
    /// relay carries your poses to it again. `data` is a zero flags byte.
    pub const DROPPED: u8 = 6;
    /// Client to relay, at most once a minute: what this client's links did
    /// ([`super::LinkReport`] bytes), so the relay can count why riders stay
    /// on it. `peer` is the sender.
    pub const REPORT: u8 = 7;
    /// Relay to riders that said [`CARRIES`]: `peer` is the room's courier,
    /// and with `data[0]` = 1 it carries the poses of the riders linked with
    /// it to riders without those links in [`super::Packet::Bundle`]s
    /// (research/MULTIPLAYER.md "Courier"); 0 ends that.
    pub const COURIER: u8 = 8;
    /// Flags bit: the rider can open direct links.
    pub const DIRECT: u8 = 1;
    /// Hello flags bit: the build can be a courier and follows
    /// [`COURIER`]. Never forwarded in other riders' flags, which older
    /// builds accept only as [`DIRECT`].
    pub const CARRIES: u8 = 2;
    /// Hello flags bit: the build decodes [`super::CpuSoundCues`] in
    /// [`super::CpuMotion`]; the relay strips them for riders without it.
    /// Never forwarded in other riders' flags.
    pub const CPU_SOUNDS: u8 = 4;
    /// Flags bit: the build decodes [`super::Packet::Batch`] and follows
    /// [`PACE`] (research/MULTIPLAYER.md "Free Ride pose batches"). Sent
    /// only to a relay whose room claim says it accepts batches, and
    /// forwarded in other riders' flags only to builds that said it.
    pub const BATCHES: u8 = 8;
    /// Relay to riders that said [`BATCHES`], in a shared room: stretch
    /// batch intervals by `data[0]` tenths ([`PACE_MIN`]..=[`PACE_MAX`];
    /// 10 is the ordinary pace). `peer` is the rider it is sent to.
    pub const PACE: u8 = 9;
    pub const PACE_MIN: u8 = 10;
    pub const PACE_MAX: u8 = 30;
    /// Hello flags bit: the build decodes [`super::Packet::Frame`]. A game
    /// server (research/GAME_SERVER.md) sends such builds one frame per tick
    /// instead of each rider's pose separately. Never forwarded in other
    /// riders' flags.
    pub const FRAMES: u8 = 16;
    /// Hello flags bit: the build follows the server-run room loop
    /// ([`super::Packet::RoomLoop`], research/QUICK_PLAY.md) and may send
    /// [`super::Packet::Vote`]. Never forwarded in other riders' flags.
    pub const ROOM_LOOP: u8 = 32;
}
/// Most poses in one [`Packet::Bundle`]: every rider in a full room.
pub const MAX_BUNDLE: usize = MAX_PLAYERS;
/// Most samples in one [`Packet::Batch`].
pub const MAX_BATCH: usize = 16;
/// Most poses in one [`Packet::Frame`] (a frame is also bounded by
/// [`MAX_PACKET_BYTES`]; a server splits larger rooms over several frames).
pub const MAX_FRAME: usize = 64;
/// Longest interval a [`Packet::Batch`] may announce (ms).
pub const MAX_BATCH_INTERVAL_MS: u16 = 60_000;

/// What one client's direct links did since it joined the room: counts only,
/// for the relay's accounting (research/MULTIPLAYER.md "Relay accounting").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkReport {
    /// Handshakes started (offers made or accepted).
    pub attempts: u16,
    /// Links that opened.
    pub opened: u16,
    /// Handshakes that had not opened by the connect timeout.
    pub timeouts: u16,
    /// Handshakes the browser reported failed.
    pub failed: u16,
    /// Open links that went silent.
    pub silent: u16,
    /// Open links the browser closed.
    pub lost: u16,
    /// Links the other end gave up.
    pub dropped: u16,
    /// Session descriptions this client sent.
    pub described: u16,
    /// Of those, ones carrying a public address from STUN (none: the
    /// network blocks STUN).
    pub public: u16,
    /// Ones carrying a local address hidden behind an mDNS name.
    pub hidden: u16,
    /// Ones carrying a plain local address.
    pub local: u16,
}
pub const LINK_REPORT_BYTES: usize = 22;
impl LinkReport {
    /// Counts, in [`Self::NAMES`] and byte order.
    pub fn values(&self) -> [u16; 11] {
        [
            self.attempts,
            self.opened,
            self.timeouts,
            self.failed,
            self.silent,
            self.lost,
            self.dropped,
            self.described,
            self.public,
            self.hidden,
            self.local,
        ]
    }
    /// Field names, in byte order.
    pub const NAMES: [&'static str; 11] = [
        "attempts",
        "opened",
        "timeouts",
        "failed",
        "silent",
        "lost",
        "dropped",
        "described",
        "public",
        "hidden",
        "local",
    ];
    pub fn to_bytes(&self) -> Vec<u8> {
        self.values().iter().flat_map(|v| v.to_le_bytes()).collect()
    }
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != LINK_REPORT_BYTES {
            return None;
        }
        let v = |i: usize| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]);
        Some(LinkReport {
            attempts: v(0),
            opened: v(1),
            timeouts: v(2),
            failed: v(3),
            silent: v(4),
            lost: v(5),
            dropped: v(6),
            described: v(7),
            public: v(8),
            hidden: v(9),
            local: v(10),
        })
    }
}
/// Largest relayed session description.
pub const MAX_SIGNAL_BYTES: usize = 6000;
/// One direct-link signalling message. From a client `peer` is the rider it
/// is for; from the relay it is the rider it came from or describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    pub peer: PeerId,
    pub kind: u8,
    pub data: Vec<u8>,
}
impl Signal {
    pub fn validate(&self) -> Result<()> {
        let valid = valid_peer(self.peer)
            && match self.kind {
                signal::HELLO => {
                    self.data.len() == 1
                        && self.data[0]
                            <= signal::DIRECT
                                | signal::CARRIES
                                | signal::CPU_SOUNDS
                                | signal::BATCHES
                                | signal::FRAMES
                }
                signal::PRESENT | signal::ARRIVED => {
                    self.data.len() == 1 && self.data[0] & !(signal::DIRECT | signal::BATCHES) == 0
                }
                signal::GONE | signal::DROPPED => {
                    self.data.len() == 1 && self.data[0] <= signal::DIRECT
                }
                signal::COURIER => self.data.len() == 1 && self.data[0] <= 1,
                signal::PACE => {
                    self.data.len() == 1
                        && (signal::PACE_MIN..=signal::PACE_MAX).contains(&self.data[0])
                }
                signal::OFFER | signal::ANSWER => {
                    !self.data.is_empty() && self.data.len() <= MAX_SIGNAL_BYTES
                }
                signal::REPORT => self.data.len() == LINK_REPORT_BYTES,
                _ => false,
            };
        if valid {
            Ok(())
        } else {
            Err(Error::InvalidValue)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Packet {
    Join {
        code: RoomCode,
        player: Participant,
    },
    Welcome {
        peer: PeerId,
        code: RoomCode,
        lobby: Lobby,
        key: RejoinKey,
    },
    Lobby(Lobby),
    SetLobby {
        epoch: u32,
        map: u16,
        started: bool,
    },
    Snapshot(Box<Snapshot>),
    Delta(Box<Delta>),
    Leave {
        peer: PeerId,
    },
    Ping {
        nonce: u32,
    },
    Pong {
        nonce: u32,
        /// Relay wall clock in milliseconds; riders share world clocks from it.
        server_ms: u64,
    },
    /// Change this rider's vehicle/rider/name without leaving the room.
    Profile(Participant),
    /// The sender has finished loading this map and is riding on it.
    Ready {
        map: u16,
    },
    /// The host's mini-game state (relayed only from the host).
    Game(Box<GameState>),
    /// A rider's room-race progress (research/MULTIPLAYER.md "Room races").
    Progress(RaceProgress),
    /// A rider's room King of the Hills / Treasure Hunt report.
    Claim(GameClaim),
    /// The room-race host's CPU riders (research/MULTIPLAYER.md "Room race
    /// CPUs").
    Cpus(Box<CpuMotion>),
    /// Direct-link signalling; only sent to builds that announced it.
    Signal(Signal),
    /// The riders this client currently reaches over an open direct link (one
    /// bit per peer id). The relay stops forwarding poses between two riders
    /// once each reports the other.
    Links {
        mask: u32,
    },
    /// Courier to relay: the latest pose (an encoded compact `Snapshot`
    /// packet) of each rider linked with it, and its own, for the relay to
    /// pass to riders without those links. Never sent to clients.
    Bundle {
        poses: Vec<Vec<u8>>,
    },
    /// One rider's poses since its last batch, oldest first (encoded compact
    /// `Snapshot` packets, each with its own sequence and time), and how
    /// long until its next batch at this rate (research/MULTIPLAYER.md
    /// "Free Ride pose batches"). To the relay only when its room claim
    /// accepts batches; from the relay only to builds that said
    /// [`signal::BATCHES`] (it sends the others each pose as a `Snapshot`).
    Batch {
        interval_ms: u16,
        poses: Vec<Vec<u8>>,
    },
    /// Game server to client, once per server tick: the latest pose of each
    /// other rider due for this client (encoded compact `Snapshot` or `Delta`
    /// packets, at most one per rider). Sent only to builds that said
    /// [`signal::FRAMES`]; the others get each pose separately.
    Frame {
        poses: Vec<Vec<u8>>,
    },
    /// Game server to builds that said [`signal::ROOM_LOOP`]: the
    /// server-run room loop (research/QUICK_PLAY.md).
    RoomLoop(Box<quickplay::RoomLoop>),
    /// Client to game server: a rider's vote, rematch and ready choices.
    Vote(quickplay::Vote),
}

pub(crate) fn bounded(values: &[f32], limit: f32) -> bool {
    values.iter().all(|v| v.is_finite() && v.abs() <= limit)
}
impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        if !valid_peer(self.peer)
            || self.atv_nodes.len() > MAX_NODES
            || self.rider_nodes.len() > MAX_NODES
            || !bounded(&self.velocity, 100_000.)
            || !bounded(&self.rider_velocity, 100_000.)
            || !bounded(&self.angular_velocity, 1000.)
            || !self.rpm.is_finite()
            || !(0. ..=100_000.).contains(&self.rpm)
            || !self.throttle.is_finite()
            || !(-1. ..=1.).contains(&self.throttle)
            || !self.speed.is_finite()
            || self.speed.abs() > 100_000.
        {
            return Err(Error::InvalidValue);
        }
        if self
            .train_time
            .is_some_and(|t| !t.is_finite() || !(0. ..=86400.).contains(&t))
        {
            return Err(Error::InvalidValue);
        }
        if let Some(platform) = &self.platform {
            if platform.car >= 32 || self.train_time.is_none() {
                return Err(Error::InvalidValue);
            }
            platform.transform.validate()?;
        }
        if let Some(a) = &self.animation {
            a.validate()?;
        }
        self.atv.validate()?;
        self.rider.validate()?;
        for node in self.atv_nodes.iter().chain(&self.rider_nodes) {
            node.validate()?;
        }
        Ok(())
    }
}
