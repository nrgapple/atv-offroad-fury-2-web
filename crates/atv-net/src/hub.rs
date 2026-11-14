//! Free Ride hub packets (research/FREERIDE_HUB.md): pop-up events, the map
//! rotation vote ([`Hub`], kind 26) and challenges between two riders
//! ([`Challenge`], kind 27). A game server sends them only to builds whose
//! HELLO said [`crate::signal::HUB`], in shared rooms whose claim answered
//! `"hub": true`. Authored for the port; the original has no Free Ride hub.
use crate::{Error, MAX_PLAYERS, NO_PEER, PeerId, Result, valid_map, valid_peer};
use serde::{Deserialize, Serialize};

/// [`Hub::op`] values.
pub mod hub_op {
    /// Server: `mask` names the riders whose builds take hub packets (the
    /// riders a Challenge can go to).
    pub const RIDERS: u8 = 0;
    /// Server: a pop-up event's state.
    pub const EVENT: u8 = 1;
    /// Server: the map rotation vote (`maps`, `votes`, `timer_ms`) or, with
    /// no `maps`, the end of one.
    pub const VOTE: u8 = 2;
    /// Client: the sender's stunt-off score (`value`) for event `serial`.
    pub const SCORE: u8 = 8;
    /// Client: the sender votes for option `choice` of vote `serial`.
    pub const CAST: u8 = 9;
}

/// [`Hub::kind`] of an event.
pub mod event_kind {
    /// First to the target (`target`) wins.
    pub const SPRINT: u8 = 0;
    /// Most seconds within the hill's radius (`target`).
    pub const HILL: u8 = 1;
    /// Most stunt points in the event.
    pub const STUNTS: u8 = 2;
}

/// [`Hub::phase`] of an event.
pub mod event_phase {
    /// The join circle is open (`point`, `radius`); `mask` has joined.
    pub const JOIN: u8 = 0;
    /// The event runs; `rows` are the live standings.
    pub const RUN: u8 = 1;
    /// Final standings in `rows` (`points` awarded).
    pub const RESULTS: u8 = 2;
    /// Nobody joined, or the room moved on.
    pub const CANCELLED: u8 = 3;
}

/// Bit of [`Hub::choice`] in an event: one rider joined, so it ran as a
/// solo score challenge.
pub const SOLO: u8 = 1;
/// No choice (a vote option not yet cast).
pub const NO_CHOICE: u8 = 255;
/// Options in a rotation vote.
pub const MAX_VOTE_MAPS: usize = 3;
/// Longest phase a hub packet may announce (ms).
pub const MAX_HUB_TIMER_MS: u32 = 30 * 60 * 1000;
/// Coordinates beyond this many feet are refused.
const MAX_COORDINATE: f32 = 1.0e6;
/// Largest event value (seconds, points or feet).
pub const MAX_HUB_VALUE: f32 = 1.0e8;

/// One rider's line in an event's standings.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct HubRow {
    pub peer: PeerId,
    /// Sprint: seconds to the target (0 when it did not finish); Hill:
    /// seconds held; Stunts: points.
    pub value: f32,
    /// Hub points awarded (results only).
    pub points: u16,
    /// Sprint: reached the target.
    pub finished: bool,
}

/// A hub message (kind 26). Fields an op does not use are zero or empty.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Hub {
    pub op: u8,
    /// The event or vote this belongs to.
    pub serial: u32,
    pub kind: u8,
    pub phase: u8,
    /// Time left in the phase (ms).
    pub timer_ms: u32,
    /// The join circle's centre.
    pub point: [f32; 3],
    /// The sprint target or the hill.
    pub target: [f32; 3],
    /// The join circle's radius (ft).
    pub radius: f32,
    /// Event: joined riders; RIDERS: hub riders.
    pub mask: u32,
    /// SCORE: the sender's points.
    pub value: f32,
    /// Event: [`SOLO`] flags; CAST: the option; VOTE: the winning option
    /// when the vote is over, else [`NO_CHOICE`].
    pub choice: u8,
    /// VOTE: the venues (map ids) on offer.
    pub maps: Vec<u16>,
    /// VOTE: votes per option, in `maps` order.
    pub votes: Vec<u8>,
    /// Event standings, best first.
    pub rows: Vec<HubRow>,
}

fn finite(values: &[f32], limit: f32) -> bool {
    values.iter().all(|v| v.is_finite() && v.abs() <= limit)
}

impl Hub {
    pub fn validate(&self) -> Result<()> {
        let peers = (1u64 << MAX_PLAYERS) - 1;
        let mut seen = 0u32;
        let rows = self.rows.len() <= MAX_PLAYERS
            && self.rows.iter().all(|r| {
                let fresh = valid_peer(r.peer) && seen & (1 << r.peer) == 0;
                if valid_peer(r.peer) {
                    seen |= 1 << r.peer;
                }
                fresh && r.value.is_finite() && (0. ..=MAX_HUB_VALUE).contains(&r.value)
            });
        let valid = matches!(
            self.op,
            hub_op::RIDERS | hub_op::EVENT | hub_op::VOTE | hub_op::SCORE | hub_op::CAST
        ) && self.kind <= event_kind::STUNTS
            && self.phase <= event_phase::CANCELLED
            && self.timer_ms <= MAX_HUB_TIMER_MS
            && finite(&self.point, MAX_COORDINATE)
            && finite(&self.target, MAX_COORDINATE)
            && self.radius.is_finite()
            && (0. ..=1000.).contains(&self.radius)
            && u64::from(self.mask) <= peers
            && self.value.is_finite()
            && (0. ..=MAX_HUB_VALUE).contains(&self.value)
            && (self.choice < MAX_VOTE_MAPS as u8
                || self.choice == NO_CHOICE
                || self.op == hub_op::EVENT)
            && self.maps.len() <= MAX_VOTE_MAPS
            && self.maps.iter().all(|&m| valid_map(m))
            && self.votes.len() == self.maps.len()
            && usize::from(self.votes.iter().map(|&v| u16::from(v)).sum::<u16>()) <= MAX_PLAYERS
            && rows;
        if valid {
            Ok(())
        } else {
            Err(Error::InvalidValue)
        }
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        out.push(self.op);
        out.extend_from_slice(&self.serial.to_le_bytes());
        out.push(self.kind);
        out.push(self.phase);
        out.extend_from_slice(&self.timer_ms.to_le_bytes());
        for v in self.point.iter().chain(&self.target).chain([&self.radius]) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.mask.to_le_bytes());
        out.extend_from_slice(&self.value.to_le_bytes());
        out.push(self.choice);
        out.push(self.maps.len() as u8);
        for (map, votes) in self.maps.iter().zip(&self.votes) {
            out.extend_from_slice(&map.to_le_bytes());
            out.push(*votes);
        }
        out.push(self.rows.len() as u8);
        for row in &self.rows {
            out.push(row.peer);
            out.extend_from_slice(&row.value.to_le_bytes());
            out.extend_from_slice(&row.points.to_le_bytes());
            out.push(u8::from(row.finished));
        }
        Ok(())
    }

    /// The whole payload after the packet header.
    pub(crate) fn read(bytes: &[u8]) -> Result<Self> {
        let mut r = Bytes { bytes, at: 0 };
        let mut hub = Hub {
            op: r.byte()?,
            serial: r.u32()?,
            kind: r.byte()?,
            phase: r.byte()?,
            timer_ms: r.u32()?,
            point: [r.f32()?, r.f32()?, r.f32()?],
            target: [r.f32()?, r.f32()?, r.f32()?],
            radius: r.f32()?,
            mask: r.u32()?,
            value: r.f32()?,
            choice: r.byte()?,
            ..Default::default()
        };
        let maps = usize::from(r.byte()?);
        if maps > MAX_VOTE_MAPS {
            return Err(Error::InvalidValue);
        }
        for _ in 0..maps {
            hub.maps.push(r.u16()?);
            hub.votes.push(r.byte()?);
        }
        let rows = usize::from(r.byte()?);
        if rows > MAX_PLAYERS {
            return Err(Error::InvalidValue);
        }
        for _ in 0..rows {
            hub.rows.push(HubRow {
                peer: r.byte()?,
                value: r.f32()?,
                points: r.u16()?,
                finished: r.boolean()?,
            });
        }
        r.end()?;
        hub.validate()?;
        Ok(hub)
    }
}

/// [`Challenge::op`] values.
pub mod challenge_op {
    /// Client: challenge rider `peer`.
    pub const ASK: u8 = 0;
    /// Client: accept challenge `serial`.
    pub const ACCEPT: u8 = 1;
    /// Client: decline challenge `serial` (also sent at once for a blocked
    /// or muted challenger).
    pub const DECLINE: u8 = 2;
    /// Server to the challenged rider: `peer` challenges you; answer within
    /// `timer_ms`.
    pub const OFFER: u8 = 3;
    /// Server to the challenger: waiting for `peer` (`timer_ms` left).
    pub const WAIT: u8 = 4;
    /// Server to both: race `peer` to `target` within `timer_ms`.
    pub const START: u8 = 5;
    /// Server to both: `detail` won (or [`crate::NO_PEER`], a draw).
    pub const RESULT: u8 = 6;
    /// Server: the challenge ended without a race (`detail` says why).
    pub const CLOSED: u8 = 7;
}

/// [`Challenge::detail`] of a [`challenge_op::CLOSED`].
pub mod challenge_end {
    pub const DECLINED: u8 = 0;
    pub const EXPIRED: u8 = 1;
    /// One of the two is already in a challenge or an event.
    pub const BUSY: u8 = 2;
    /// The pair raced or asked less than a minute ago.
    pub const COOLDOWN: u8 = 3;
    /// One of the two left the room or the course.
    pub const LEFT: u8 = 4;
    /// Too far apart, or the other build has no hub.
    pub const UNAVAILABLE: u8 = 5;
}

/// A challenge message (kind 27).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Challenge {
    pub op: u8,
    pub serial: u32,
    /// The other rider.
    pub peer: PeerId,
    pub timer_ms: u32,
    /// START: the waypoint.
    pub target: [f32; 3],
    /// RESULT: the winner; CLOSED: a [`challenge_end`] reason.
    pub detail: u8,
}

impl Challenge {
    pub fn validate(&self) -> Result<()> {
        let detail = match self.op {
            challenge_op::RESULT => valid_peer(self.detail) || self.detail == NO_PEER,
            challenge_op::CLOSED => self.detail <= challenge_end::UNAVAILABLE,
            _ => self.detail == 0,
        };
        if self.op <= challenge_op::CLOSED
            && valid_peer(self.peer)
            && self.timer_ms <= MAX_HUB_TIMER_MS
            && finite(&self.target, MAX_COORDINATE)
            && detail
        {
            Ok(())
        } else {
            Err(Error::InvalidValue)
        }
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        out.push(self.op);
        out.extend_from_slice(&self.serial.to_le_bytes());
        out.push(self.peer);
        out.extend_from_slice(&self.timer_ms.to_le_bytes());
        for v in self.target {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.push(self.detail);
        Ok(())
    }

    pub(crate) fn read(bytes: &[u8]) -> Result<Self> {
        let mut r = Bytes { bytes, at: 0 };
        let challenge = Challenge {
            op: r.byte()?,
            serial: r.u32()?,
            peer: r.byte()?,
            timer_ms: r.u32()?,
            target: [r.f32()?, r.f32()?, r.f32()?],
            detail: r.byte()?,
        };
        r.end()?;
        challenge.validate()?;
        Ok(challenge)
    }
}

struct Bytes<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Bytes<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self.at + N;
        let slice = self.bytes.get(self.at..end).ok_or(Error::Truncated)?;
        self.at = end;
        Ok(slice.try_into().expect("length checked"))
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take()?))
    }
    fn boolean(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidValue),
        }
    }
    fn end(&self) -> Result<()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(Error::InvalidLength)
        }
    }
}
