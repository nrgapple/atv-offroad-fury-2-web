//! Server-run room loop (research/QUICK_PLAY.md): Quick Play rooms, and
//! later private rooms' ready-up, vote and rematch (slice C).
//!
//! Authored for the port; the original's online lobbies are not used.
//! [`RoomLoop`] (kind 22) goes from the game server to builds that said
//! [`crate::signal::ROOM_LOOP`]; [`Vote`] (kind 23) goes from a client to the
//! server only. Both are bounded and validated like every other packet.

use crate::{
    CPU_PEER, Error, MAP_ENVIRONMENTS, NO_PEER, PeerId, RACE_CPUS, RACE_RIDERS, Result, RoomCode,
    game_kind, valid_map,
};
use serde::{Deserialize, Serialize};

/// Room code prefix of Quick Play rooms (`QPR00001` race, `QPS00001`
/// stunts). Reserved: private rooms cannot take it.
pub const QUICKPLAY_PREFIX: &[u8; 2] = b"QP";

pub fn is_quickplay_room_code(code: &RoomCode) -> bool {
    code.starts_with(QUICKPLAY_PREFIX)
}

/// Human seats in a Quick Play room: a room race's or game's slots (peer
/// ids 0-3). CPUs take the rest of a race field.
pub const QUICKPLAY_HUMANS: usize = RACE_RIDERS;
/// Most seats a [`RoomLoop`] lists: the humans and a race's CPUs.
pub const MAX_SEATS: usize = RACE_RIDERS + RACE_CPUS;
/// Map cards in a vote; the fourth card is Random.
pub const VOTE_OPTIONS: usize = 3;
/// [`Vote::choice`] for the Random card.
pub const RANDOM: u8 = VOTE_OPTIONS as u8;
/// [`Vote::choice`] before choosing.
pub const NO_CHOICE: u8 = 255;

/// [`RoomLoop::phase`].
pub mod loop_phase {
    /// Seats fill; `deadline_ms` is the latest start.
    pub const LOBBY: u8 = 0;
    /// Map cards; `deadline_ms` ends the vote.
    pub const VOTE: u8 = 1;
    /// The pick is shown and the map loads; `round` is set.
    pub const COUNTDOWN: u8 = 2;
    /// The round runs (its own grid and 3-2-1-GO).
    pub const PLAY: u8 = 3;
    /// Podium: `results` are final; Rematch is the default.
    pub const RESULTS: u8 = 4;
}

/// [`RoomLoop::playlist`].
pub mod playlist {
    /// Races and King of the Hills.
    pub const RACE: u8 = 0;
    /// Freestyle and Treasure Hunt.
    pub const STUNTS: u8 = 1;
    pub fn name(playlist: u8) -> Option<&'static str> {
        match playlist {
            RACE => Some("race"),
            STUNTS => Some("stunts"),
            _ => None,
        }
    }
    pub fn parse(name: &str) -> Option<u8> {
        match name {
            "race" => Some(RACE),
            "stunts" => Some(STUNTS),
            _ => None,
        }
    }
}

/// [`Seat::flags`].
pub mod seat_flag {
    pub const CPU: u8 = 1;
    pub const VOTED: u8 = 2;
    pub const READY: u8 = 4;
    /// Joined after the round started: plays the next one.
    pub const SPECTATING: u8 = 8;
    /// Chose Leave on the podium.
    pub const LEAVING: u8 = 16;
    /// Runs this round's CPUs and game rules for the server.
    pub const DRIVER: u8 = 32;
    pub const ALL: u8 = 63;
}

/// [`Placing::flags`].
pub mod result_flag {
    pub const CPU: u8 = 1;
    pub const DNF: u8 = 2;
    /// Left the room during the round.
    pub const LEFT: u8 = 4;
    /// The server's checks refused this rider's result (ranked DNF).
    pub const REJECTED: u8 = 8;
    pub const ALL: u8 = 15;
}

/// One map card, or the round that was picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Round {
    /// [`game_kind`]: race, King of the Hills, Treasure Hunt or Freestyle.
    pub mode: u8,
    /// Lobby map id (1-based [`MAP_ENVIRONMENTS`]).
    pub map: u16,
    /// Race only: 0 National/Supercross, 1 Enduro long, 2 Enduro short,
    /// 3 Short Track.
    pub variant: u8,
}

impl Round {
    pub fn environment(&self) -> Option<&'static str> {
        crate::map_environment(self.map)
    }
    fn validate(&self) -> Result<()> {
        let mode_ok = matches!(
            self.mode,
            game_kind::RACE
                | game_kind::KING_OF_THE_HILLS
                | game_kind::TREASURE_HUNT
                | game_kind::FREESTYLE
        );
        if !mode_ok || !valid_map(self.map) || self.variant > 3 {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
}

/// The picked round's settings, set from [`loop_phase::COUNTDOWN`] on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Setup {
    pub round: Round,
    /// The round's number (from 1); the driver's game serial follows it.
    pub number: u32,
    /// Race laps (1-5).
    pub laps: u8,
    /// Games and Freestyle: the time limit in minutes (1-20).
    pub minutes: u8,
    /// Race: fill the field to this many riders with CPUs (0 none).
    pub cpu_field: u8,
    /// CPU difficulty: 1 Normal, 2 Expert.
    pub difficulty: u8,
    /// The grid shuffle's seed.
    pub seed: u32,
}

/// A seat in the room: a human rider (peer id) or a CPU
/// ([`CPU_PEER`] + index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Seat {
    pub peer: PeerId,
    /// [`seat_flag`] bits.
    pub flags: u8,
    /// The rider's own reported round trip to the server (ms; 0 unknown).
    pub ping_ms: u16,
}

/// One podium row, in place order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Placing {
    pub peer: PeerId,
    /// [`result_flag`] bits.
    pub flags: u8,
    /// Race: finish ms. Stunts and points: the score.
    pub value: u32,
}

/// Kind 22: the server's room loop.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoomLoop {
    pub phase: u8,
    pub playlist: u8,
    /// When the phase ends, on the server clock (`Pong::server_ms`); 0 when
    /// it ends on an event (the round finishing).
    pub deadline_ms: u64,
    /// The vote's map cards.
    pub options: Vec<Round>,
    /// Votes per card, Random last.
    pub votes: [u8; VOTE_OPTIONS + 1],
    /// The card the roulette landed on ([`RANDOM`] for Random), or
    /// [`NO_CHOICE`].
    pub pick: u8,
    /// The round being played or just played.
    pub setup: Setup,
    /// Who runs the round's CPUs and game rules ([`NO_PEER`] none).
    pub driver: PeerId,
    pub seats: Vec<Seat>,
    pub results: Vec<Placing>,
    /// The server could check the results it shows.
    pub verified: bool,
}

/// [`Vote::rematch`].
pub mod rematch {
    pub const UNDECIDED: u8 = 0;
    pub const STAY: u8 = 1;
    pub const LEAVE: u8 = 2;
}

/// Kind 23: a rider's choices for the loop, and its own round trip (for the
/// others' ping bars). Sent on every change and every few seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Vote {
    /// The setup number (or, during a vote, the coming one) this refers to.
    pub round: u32,
    /// A card index, [`RANDOM`] or [`NO_CHOICE`].
    pub choice: u8,
    /// [`rematch`] on the podium.
    pub rematch: u8,
    /// Ready to start now (Quick Play's lobby; private rooms' ready-up).
    pub ready: bool,
    pub ping_ms: u16,
}

fn peer_ok(peer: PeerId) -> bool {
    usize::from(peer) < QUICKPLAY_HUMANS || (CPU_PEER..CPU_PEER + RACE_CPUS as u8).contains(&peer)
}

impl RoomLoop {
    pub fn validate(&self) -> Result<()> {
        let bad = self.phase > loop_phase::RESULTS
            || playlist::name(self.playlist).is_none()
            || self.options.len() > VOTE_OPTIONS
            || self.pick > RANDOM && self.pick != NO_CHOICE
            || (self.pick != NO_CHOICE
                && self.pick != RANDOM
                && usize::from(self.pick) >= self.options.len())
            || (self.driver != NO_PEER && usize::from(self.driver) >= QUICKPLAY_HUMANS)
            || self.seats.len() > MAX_SEATS
            || self.results.len() > MAX_SEATS
            || self.setup.laps > 5
            || self.setup.minutes > 20
            || usize::from(self.setup.cpu_field) > MAX_SEATS
            || self.setup.difficulty > 2;
        if bad {
            return Err(Error::InvalidValue);
        }
        for round in &self.options {
            round.validate()?;
        }
        if self.setup.number > 0 {
            self.setup.round.validate()?;
        }
        let mut seen = 0u16;
        for seat in &self.seats {
            if !peer_ok(seat.peer)
                || seen & (1 << seat.peer) != 0
                || seat.flags & !seat_flag::ALL != 0
                || (seat.flags & seat_flag::CPU != 0) != (seat.peer >= CPU_PEER)
            {
                return Err(Error::InvalidValue);
            }
            seen |= 1 << seat.peer;
        }
        let mut seen = 0u16;
        for row in &self.results {
            if !peer_ok(row.peer)
                || seen & (1 << row.peer) != 0
                || row.flags & !result_flag::ALL != 0
                || (row.flags & result_flag::CPU != 0) != (row.peer >= CPU_PEER)
            {
                return Err(Error::InvalidValue);
            }
            seen |= 1 << row.peer;
        }
        Ok(())
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        out.push(self.phase);
        out.push(self.playlist);
        out.extend_from_slice(&self.deadline_ms.to_le_bytes());
        out.push(self.options.len() as u8);
        for round in &self.options {
            round_bytes(round, out);
        }
        out.extend_from_slice(&self.votes);
        out.push(self.pick);
        round_bytes(&self.setup.round, out);
        out.extend_from_slice(&self.setup.number.to_le_bytes());
        out.extend_from_slice(&[
            self.setup.laps,
            self.setup.minutes,
            self.setup.cpu_field,
            self.setup.difficulty,
        ]);
        out.extend_from_slice(&self.setup.seed.to_le_bytes());
        out.push(self.driver);
        out.push(self.seats.len() as u8);
        for seat in &self.seats {
            out.extend_from_slice(&[seat.peer, seat.flags]);
            out.extend_from_slice(&seat.ping_ms.to_le_bytes());
        }
        out.push(self.results.len() as u8);
        for row in &self.results {
            out.extend_from_slice(&[row.peer, row.flags]);
            out.extend_from_slice(&row.value.to_le_bytes());
        }
        out.push(u8::from(self.verified));
        Ok(())
    }

    pub(crate) fn decode(body: &[u8]) -> Result<Self> {
        let mut r = Cursor(body);
        let phase = r.byte()?;
        let playlist = r.byte()?;
        let deadline_ms = r.u64()?;
        let count = usize::from(r.byte()?);
        if count > VOTE_OPTIONS {
            return Err(Error::InvalidValue);
        }
        let options = (0..count).map(|_| r.round()).collect::<Result<Vec<_>>>()?;
        let votes = [r.byte()?, r.byte()?, r.byte()?, r.byte()?];
        let pick = r.byte()?;
        let setup = Setup {
            round: r.round()?,
            number: r.u32()?,
            laps: r.byte()?,
            minutes: r.byte()?,
            cpu_field: r.byte()?,
            difficulty: r.byte()?,
            seed: r.u32()?,
        };
        let driver = r.byte()?;
        let count = usize::from(r.byte()?);
        if count > MAX_SEATS {
            return Err(Error::InvalidValue);
        }
        let seats = (0..count)
            .map(|_| {
                Ok(Seat {
                    peer: r.byte()?,
                    flags: r.byte()?,
                    ping_ms: r.u16()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let count = usize::from(r.byte()?);
        if count > MAX_SEATS {
            return Err(Error::InvalidValue);
        }
        let results = (0..count)
            .map(|_| {
                Ok(Placing {
                    peer: r.byte()?,
                    flags: r.byte()?,
                    value: r.u32()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let verified = match r.byte()? {
            0 => false,
            1 => true,
            _ => return Err(Error::InvalidValue),
        };
        if !r.0.is_empty() {
            return Err(Error::InvalidLength);
        }
        let value = RoomLoop {
            phase,
            playlist,
            deadline_ms,
            options,
            votes,
            pick,
            setup,
            driver,
            seats,
            results,
            verified,
        };
        value.validate()?;
        Ok(value)
    }

    /// This rider's seat.
    pub fn seat(&self, peer: PeerId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.peer == peer)
    }
}

impl Vote {
    pub fn validate(&self) -> Result<()> {
        if (self.choice > RANDOM && self.choice != NO_CHOICE) || self.rematch > rematch::LEAVE {
            return Err(Error::InvalidValue);
        }
        Ok(())
    }
    pub(crate) fn encode(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        out.extend_from_slice(&self.round.to_le_bytes());
        out.extend_from_slice(&[self.choice, self.rematch, u8::from(self.ready)]);
        out.extend_from_slice(&self.ping_ms.to_le_bytes());
        Ok(())
    }
    pub(crate) fn decode(body: &[u8]) -> Result<Self> {
        let mut r = Cursor(body);
        let vote = Vote {
            round: r.u32()?,
            choice: r.byte()?,
            rematch: r.byte()?,
            ready: match r.byte()? {
                0 => false,
                1 => true,
                _ => return Err(Error::InvalidValue),
            },
            ping_ms: r.u16()?,
        };
        if !r.0.is_empty() {
            return Err(Error::InvalidLength);
        }
        vote.validate()?;
        Ok(vote)
    }
}

fn round_bytes(round: &Round, out: &mut Vec<u8>) {
    out.push(round.mode);
    out.extend_from_slice(&round.map.to_le_bytes());
    out.push(round.variant);
}

struct Cursor<'a>(&'a [u8]);
impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.0.len() < N {
            return Err(Error::Truncated);
        }
        let (head, rest) = self.0.split_at(N);
        self.0 = rest;
        Ok(head.try_into().expect("split length"))
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
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    /// A round as sent: unset (all zero) or valid.
    fn round(&mut self) -> Result<Round> {
        let round = Round {
            mode: self.byte()?,
            map: self.u16()?,
            variant: self.byte()?,
        };
        if round != Round::default() {
            round.validate()?;
        }
        Ok(round)
    }
}

/// The map id of an environment key (`nat03` → 17).
pub fn map_id(environment: &str) -> Option<u16> {
    MAP_ENVIRONMENTS
        .iter()
        .position(|e| *e == environment)
        .map(|i| i as u16 + 1)
}

/// Venues with King of the Hills and Treasure Hunt archives
/// (`E0n_KH.RES`/`_TH.RES`, `F0n_…`), as `atv-app` `room_game` lists them.
pub const GAME_VENUES: [&str; 14] = [
    "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free01", "free02",
    "free03", "free04", "free05", "free06",
];

/// Every round a playlist can pick (the Random card draws from all of
/// them): races on every race course and race (`RaceVariant::exists`),
/// King of the Hills on its venues; Freestyle on the Freestyle and
/// Environment venues, Treasure Hunt on its venues.
pub fn pool(playlist: u8) -> Vec<Round> {
    let mut out = Vec::new();
    let mut add = |mode: u8, environment: &str, variant: u8| {
        if let Some(map) = map_id(environment) {
            out.push(Round { mode, map, variant });
        }
    };
    match playlist {
        playlist::RACE => {
            for environment in MAP_ENVIRONMENTS {
                let family = environment.trim_end_matches(|c: char| c.is_ascii_digit());
                if !matches!(family, "env" | "nat" | "sx") || environment.len() != family.len() + 2
                {
                    continue;
                }
                add(game_kind::RACE, environment, 0);
                if family == "env" {
                    add(game_kind::RACE, environment, 1);
                    add(game_kind::RACE, environment, 2);
                    if matches!(environment, "env01" | "env06" | "env07") {
                        add(game_kind::RACE, environment, 3);
                    }
                }
            }
            for venue in GAME_VENUES {
                add(game_kind::KING_OF_THE_HILLS, venue, 0);
            }
        }
        playlist::STUNTS => {
            for venue in GAME_VENUES {
                add(game_kind::FREESTYLE, venue, 0);
                add(game_kind::TREASURE_HUNT, venue, 0);
            }
        }
        _ => {}
    }
    out
}

/// No rider: [`RoomLoop::driver`] before anyone can drive.
pub const NO_DRIVER: PeerId = NO_PEER;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Packet, decode, encode};

    fn sample() -> RoomLoop {
        RoomLoop {
            phase: loop_phase::RESULTS,
            playlist: playlist::RACE,
            deadline_ms: 1_763_130_000_000,
            options: pool(playlist::RACE)[..3].to_vec(),
            votes: [1, 0, 2, 1],
            pick: RANDOM,
            setup: Setup {
                round: pool(playlist::RACE)[5],
                number: 7,
                laps: 2,
                minutes: 0,
                cpu_field: 6,
                difficulty: 1,
                seed: 99,
            },
            driver: 1,
            seats: vec![
                Seat {
                    peer: 0,
                    flags: seat_flag::VOTED,
                    ping_ms: 40,
                },
                Seat {
                    peer: 1,
                    flags: seat_flag::DRIVER,
                    ping_ms: 120,
                },
                Seat {
                    peer: CPU_PEER,
                    flags: seat_flag::CPU,
                    ping_ms: 0,
                },
            ],
            results: vec![
                Placing {
                    peer: CPU_PEER,
                    flags: result_flag::CPU,
                    value: 61_000,
                },
                Placing {
                    peer: 0,
                    flags: 0,
                    value: 62_500,
                },
                Placing {
                    peer: 1,
                    flags: result_flag::DNF | result_flag::LEFT,
                    value: 0,
                },
            ],
            verified: true,
        }
    }

    #[test]
    fn room_loop_and_vote_round_trip() {
        let state = sample();
        let bytes = encode(&Packet::RoomLoop(Box::new(state.clone()))).unwrap();
        assert_eq!(decode(&bytes).unwrap(), Packet::RoomLoop(Box::new(state)));
        let vote = Vote {
            round: 3,
            choice: RANDOM,
            rematch: rematch::STAY,
            ready: true,
            ping_ms: 85,
        };
        let bytes = encode(&Packet::Vote(vote)).unwrap();
        assert_eq!(decode(&bytes).unwrap(), Packet::Vote(vote));
        // An empty lobby state is valid too.
        let lobby = RoomLoop {
            pick: NO_CHOICE,
            driver: NO_DRIVER,
            ..Default::default()
        };
        let bytes = encode(&Packet::RoomLoop(Box::new(lobby.clone()))).unwrap();
        assert_eq!(decode(&bytes).unwrap(), Packet::RoomLoop(Box::new(lobby)));
    }

    #[test]
    fn refuses_out_of_range_values() {
        let bad = [
            RoomLoop {
                phase: 5,
                ..sample()
            },
            RoomLoop {
                playlist: 2,
                ..sample()
            },
            RoomLoop {
                pick: 4,
                ..sample()
            },
            RoomLoop {
                driver: 4,
                ..sample()
            },
            RoomLoop {
                seats: vec![Seat::default(); MAX_SEATS + 1],
                ..sample()
            },
            RoomLoop {
                // A CPU flag on a human seat.
                seats: vec![Seat {
                    peer: 0,
                    flags: seat_flag::CPU,
                    ping_ms: 0,
                }],
                ..sample()
            },
            RoomLoop {
                // Twice the same rider.
                results: vec![Placing::default(), Placing::default()],
                ..sample()
            },
            RoomLoop {
                options: vec![Round {
                    mode: 1,
                    map: 1,
                    variant: 0,
                }],
                pick: NO_CHOICE,
                ..sample()
            },
        ];
        for state in bad {
            assert!(encode(&Packet::RoomLoop(Box::new(state))).is_err());
        }
        assert!(
            encode(&Packet::Vote(Vote {
                choice: 4,
                ..Default::default()
            }))
            .is_err()
        );
        // Truncated and padded bodies.
        let bytes = encode(&Packet::RoomLoop(Box::new(sample()))).unwrap();
        let mut short = bytes[..bytes.len() - 1].to_vec();
        let len = (short.len() - 8) as u16;
        short[6..8].copy_from_slice(&len.to_le_bytes());
        assert!(decode(&short).is_err());
        let mut long = bytes.clone();
        long.push(0);
        let len = (long.len() - 8) as u16;
        long[6..8].copy_from_slice(&len.to_le_bytes());
        assert!(decode(&long).is_err());
    }

    #[test]
    fn pools_hold_only_valid_rounds() {
        let race = pool(playlist::RACE);
        let stunts = pool(playlist::STUNTS);
        // 32 National/Supercross/Environment races, 16 Enduro, 3 Short
        // Track and 14 King of the Hills venues.
        assert_eq!(race.len(), 32 + 16 + 3 + 14);
        assert_eq!(stunts.len(), 28);
        for r in race.iter().chain(&stunts) {
            r.validate().unwrap();
        }
        assert!(is_quickplay_room_code(b"QPR00001"));
        assert!(!is_quickplay_room_code(b"FREERD01"));
    }
}
