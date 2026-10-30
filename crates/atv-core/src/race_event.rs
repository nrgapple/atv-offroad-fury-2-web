//! The original Track Select event types and their cycler
//! (research/RACING.md "Practice").
//!
//! Exact-build evidence (USA v2.00 `SCUS_972.11`, private exports):
//!
//! - `0021add0` names the seven event types 0-6 at profile +0x15f0 + 0x40 n:
//!   Single Race, Practice, Lap Attack, Objectives, Classic, Duel, Freeride.
//! - Tracks.dat gives every race course list (Nat, SX, Enduro, Short Track)
//!   `RaceTypeMask=103` (bits 0, 1, 2, 5, 6: Single Race, Practice, Lap
//!   Attack, Duel, Freeride) and the stunt lists 88 (Objectives, Classic,
//!   Freeride).
//! - The cycler `0026d010` (also run on page entry by `0026a720`) steps to
//!   the next allowed type, skips Duel unless the multiplayer flag
//!   (profile +0x1a08) is set, and online (+0x7d8) also skips Objectives
//!   and Lap Attack.
//!
//! The port builds only some types; [`EventType::built`] lists them, so Duel
//! and Lap Attack slot in by adding them there.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub enum EventType {
    #[default]
    SingleRace,
    Practice,
    LapAttack,
    Objectives,
    Classic,
    Duel,
    Freeride,
}

/// Tracks.dat `*RaceTypeMask` of every race course list.
pub const RACE_TRACK_MASK: u32 = 103;

/// The types in the original's order (`0021add0`).
pub const ALL: [EventType; 7] = [
    EventType::SingleRace,
    EventType::Practice,
    EventType::LapAttack,
    EventType::Objectives,
    EventType::Classic,
    EventType::Duel,
    EventType::Freeride,
];

impl EventType {
    /// The original type number (`0021add0`, the cycler's index).
    pub fn index(self) -> u32 {
        ALL.iter().position(|t| *t == self).unwrap_or(0) as u32
    }
    /// The original name (`0021add0`: 0x42c080 "Single Race", 0x42c070
    /// "Practice", ...).
    pub fn name(self) -> &'static str {
        match self {
            Self::SingleRace => "Single Race",
            Self::Practice => "Practice",
            Self::LapAttack => "Lap Attack",
            Self::Objectives => "Objectives",
            Self::Classic => "Classic",
            Self::Duel => "Duel",
            Self::Freeride => "Freeride",
        }
    }
    /// A saved or linked key.
    pub fn key(self) -> &'static str {
        match self {
            Self::SingleRace => "race",
            Self::Practice => "practice",
            Self::LapAttack => "lapattack",
            Self::Objectives => "objectives",
            Self::Classic => "classic",
            Self::Duel => "duel",
            Self::Freeride => "freeride",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        ALL.into_iter().find(|t| t.key() == key)
    }
    /// The race page's types the port has built (Duel and Lap Attack come
    /// later; Freeride on a race course is not offered there).
    pub fn built(self) -> bool {
        matches!(self, Self::SingleRace | Self::Practice)
    }
    /// The race-session mode the Start handler `0026b7a0` writes to profile
    /// +0: 0 for Single Race, 1 for Practice, 2 for Lap Attack, 5 for Duel.
    /// Race-over and results (`001f1990`) run only for modes 0 and 5.
    pub fn has_finish(self) -> bool {
        matches!(self, Self::SingleRace | Self::Duel)
    }
    /// The Laps row (`002f5e60`): Practice, Lap Attack and Freeride have no
    /// lap count (0, shown "N/A" by `0026df60`).
    pub fn has_laps(self) -> bool {
        !matches!(self, Self::Practice | Self::LapAttack | Self::Freeride)
    }
}

/// Whether `mask` (a Tracks.dat `RaceTypeMask`) and the play mode allow
/// `kind` (`0026d010`).
pub fn allowed(kind: EventType, mask: u32, multiplayer: bool, online: bool) -> bool {
    if mask & (1 << kind.index()) == 0 {
        return false;
    }
    if kind == EventType::Duel && !multiplayer {
        return false;
    }
    !(online
        && matches!(
            kind,
            EventType::Objectives | EventType::LapAttack | EventType::Duel
        ))
}

/// The race page's types for this mode, in the original order.
pub fn offered(mask: u32, multiplayer: bool, online: bool) -> Vec<EventType> {
    ALL.into_iter()
        .filter(|t| t.built() && allowed(*t, mask, multiplayer, online))
        .collect()
}

/// Steps `current` through the offered types (wrapping, either way); a type
/// no longer offered falls back to the first one.
pub fn step(current: EventType, direction: i32, offered: &[EventType]) -> EventType {
    if offered.is_empty() {
        return current;
    }
    match offered.iter().position(|t| *t == current) {
        Some(at) => {
            offered[(at as i32 + direction.signum()).rem_euclid(offered.len() as i32) as usize]
        }
        None => offered[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_tracks_offer_single_race_and_practice() {
        assert_eq!(
            offered(RACE_TRACK_MASK, false, false),
            vec![EventType::SingleRace, EventType::Practice]
        );
        assert_eq!(
            offered(RACE_TRACK_MASK, true, false),
            vec![EventType::SingleRace, EventType::Practice]
        );
        // Stunt lists (88) have neither.
        assert!(offered(88, false, false).is_empty());
    }

    #[test]
    fn cycler_skips_duel_offline_single_player_and_online_types() {
        assert!(!allowed(EventType::Duel, RACE_TRACK_MASK, false, false));
        assert!(allowed(EventType::Duel, RACE_TRACK_MASK, true, false));
        assert!(!allowed(EventType::LapAttack, RACE_TRACK_MASK, false, true));
        assert!(allowed(EventType::LapAttack, RACE_TRACK_MASK, false, false));
        assert!(allowed(EventType::Practice, RACE_TRACK_MASK, false, true));
        assert!(!allowed(EventType::Classic, RACE_TRACK_MASK, false, false));
    }

    #[test]
    fn step_wraps_both_ways() {
        let o = offered(RACE_TRACK_MASK, false, false);
        assert_eq!(step(EventType::SingleRace, 1, &o), EventType::Practice);
        assert_eq!(step(EventType::Practice, 1, &o), EventType::SingleRace);
        assert_eq!(step(EventType::SingleRace, -1, &o), EventType::Practice);
        assert_eq!(step(EventType::Duel, 1, &o), EventType::SingleRace);
    }

    #[test]
    fn practice_has_no_laps_or_finish() {
        assert!(!EventType::Practice.has_laps());
        assert!(!EventType::Practice.has_finish());
        assert!(EventType::SingleRace.has_finish());
        assert_eq!(EventType::Practice.index(), 1);
        assert_eq!(EventType::from_key("practice"), Some(EventType::Practice));
    }
}
