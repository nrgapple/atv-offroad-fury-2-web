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
pub use compact::{AnimationState, Delta, apply_delta, delta};
pub use names::{offensive_name, public_name};
mod smoothing;
mod transform;

pub use codec::{decode, encode};
pub use smoothing::{PoseBuffer, SampledPose, SmoothingConfig};
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
    /// 1 Hockey, 2 Tag.
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
}
pub const NO_PEER: PeerId = 255;
impl GameState {
    pub fn validate(&self) -> Result<()> {
        if !(1..=2).contains(&self.game)
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
