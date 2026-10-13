//! Authored browser multiplayer wire format and remote presentation smoothing.
//!
//! Transport independent: both the WASM client and Rust relay validate the same
//! bounded messages. World coordinates retain the game's feet convention, time
//! is milliseconds, and all quaternions use the core's W,X,Y,Z component order.
//! This is a new networking baseline, not recovered PS2 network code.

use serde::{Deserialize, Serialize};

mod codec;
mod smoothing;
mod transform;

pub use codec::{decode, encode};
pub use smoothing::{PoseBuffer, SampledPose, SmoothingConfig};
pub use transform::Transform;

pub const VERSION: u8 = 2;
pub const MAX_PLAYERS: usize = 4;
/// One-based lobby IDs reference this authored catalog; source resource keys
/// remain stable even when their browser display names change.
pub const MAP_ENVIRONMENTS: [&str; 9] = [
    "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free05",
];
pub const MAX_MAP_ID: u16 = MAP_ENVIRONMENTS.len() as u16;
pub const MAX_NODES: usize = 64;
pub const MAX_PACKET_BYTES: usize = 8192;
pub const MAX_NAME_BYTES: usize = 32;
pub const ROOM_CODE_BYTES: usize = 8;
pub type PeerId = u8;
pub type RoomCode = [u8; ROOM_CODE_BYTES];

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

/// Eight uppercase base-32 characters; ambiguous I/O/0/1 are excluded.
pub fn valid_room_code(code: &RoomCode) -> bool {
    code.iter()
        .all(|b| b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(b))
}
/// User-entered codes accept surrounding whitespace and ASCII lowercase. The
/// on-wire and URL form remains exactly eight uppercase base-32 characters.
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
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lobby {
    pub host: PeerId,
    /// Changes when host changes; clients reject stale host commands.
    pub epoch: u32,
    pub map: u16,
    pub started: bool,
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
    },
    Lobby(Lobby),
    SetLobby {
        epoch: u32,
        map: u16,
        started: bool,
    },
    Snapshot(Snapshot),
    Leave {
        peer: PeerId,
    },
    Ping {
        nonce: u32,
    },
    Pong {
        nonce: u32,
    },
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
        self.atv.validate()?;
        self.rider.validate()?;
        for node in self.atv_nodes.iter().chain(&self.rider_nodes) {
            node.validate()?;
        }
        Ok(())
    }
}
