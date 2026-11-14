//! Quick chat (research/QUICK_CHAT.md): preset lines, emotes and a horn,
//! sent as ids only. No free text travels, so nothing can be spelled out.
//! Port addition; the PS2 game had no chat of this kind.
use crate::{PeerId, Result, valid_peer};
use serde::{Deserialize, Serialize};

/// [`Chat::kind`]: one of [`LINES`].
pub const LINE: u8 = 0;
/// [`Chat::kind`]: one of [`EMOTES`], drawn as an icon bubble.
pub const EMOTE: u8 = 1;
/// [`Chat::kind`]: the horn (id 0), heard only by riders nearby.
pub const HORN: u8 = 2;

/// Lines on the in-ride wheel: ids `0..RIDE_LINES`.
pub const RIDE_LINES: u8 = 8;
/// Every line, by id. The in-ride wheel holds the first [`RIDE_LINES`];
/// the rest are the post-race set.
pub const LINES: [&str; 14] = [
    "Nice!",
    "GG",
    "Wow!",
    "Sorry!",
    "Follow me",
    "Wait up",
    "Race?",
    "Thanks",
    "Rematch?",
    "Close one!",
    "Well raced",
    "Next time!",
    "So fast!",
    "Fun!",
];
/// Post-race lines, by id: GG and Thanks from the ride set, then the rest.
pub const POST_LINES: [u8; 8] = [1, 8, 9, 10, 11, 12, 13, 7];
/// Emote names, by id (the client draws each as an icon).
pub const EMOTES: [&str; 8] = [
    "thumbs", "laugh", "wow", "cool", "heart", "star", "fire", "trophy",
];

/// The quick-chat window: at most this many lines in [`LINE_WINDOW_MS`],
/// then nothing for [`LINE_LOCKOUT_MS`].
pub const LINE_BURST: usize = 3;
pub const LINE_WINDOW_MS: u64 = 5000;
pub const LINE_LOCKOUT_MS: u64 = 5000;
/// One emote this often.
pub const EMOTE_MS: u64 = 2000;
/// One horn this often.
pub const HORN_MS: u64 = 1000;
/// A horn reaches riders this close (ft): the server's 60-300 ft interest
/// tier, where riders see each other's every second pose.
pub const HEARING_FT: f32 = 300.;

/// One quick-chat event. From a client `peer` is ignored (the server stamps
/// the sender's slot); from the server it is the sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    pub peer: PeerId,
    pub kind: u8,
    pub id: u8,
}

/// Whether `kind` and `id` name a preset.
pub fn valid(kind: u8, id: u8) -> bool {
    match kind {
        LINE => usize::from(id) < LINES.len(),
        EMOTE => usize::from(id) < EMOTES.len(),
        HORN => id == 0,
        _ => false,
    }
}

/// A line's text.
pub fn line(id: u8) -> Option<&'static str> {
    LINES.get(usize::from(id)).copied()
}

impl Chat {
    pub fn validate(&self) -> Result<()> {
        if valid_peer(self.peer) && valid(self.kind, self.id) {
            Ok(())
        } else {
            Err(crate::Error::InvalidValue)
        }
    }
}

/// One sender's quick-chat budget, as the server enforces it and the client
/// mirrors it to grey out its wheel. Times are milliseconds on any clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// The latest accepted lines' times, newest last.
    lines: [u64; LINE_BURST],
    count: usize,
    locked_until: u64,
    emote_at: Option<u64>,
    horn_at: Option<u64>,
}

impl Limits {
    /// Whether a `kind` sent at `now` would pass, without taking it.
    pub fn ready(&self, kind: u8, now: u64) -> bool {
        match kind {
            LINE => {
                now >= self.locked_until
                    && (self.count < LINE_BURST
                        || now.saturating_sub(self.lines[0]) >= LINE_WINDOW_MS)
            }
            EMOTE => self
                .emote_at
                .is_none_or(|t| now.saturating_sub(t) >= EMOTE_MS),
            HORN => self
                .horn_at
                .is_none_or(|t| now.saturating_sub(t) >= HORN_MS),
            _ => false,
        }
    }

    /// Takes a `kind` sent at `now` if it passes. A line refused inside the
    /// window starts the lockout.
    pub fn take(&mut self, kind: u8, now: u64) -> bool {
        if !self.ready(kind, now) {
            if kind == LINE && now >= self.locked_until {
                self.locked_until = now + LINE_LOCKOUT_MS;
                self.count = 0;
            }
            return false;
        }
        match kind {
            LINE => {
                if self.count == LINE_BURST {
                    self.lines.rotate_left(1);
                    self.lines[LINE_BURST - 1] = now;
                } else {
                    self.lines[self.count] = now;
                    self.count += 1;
                }
            }
            EMOTE => self.emote_at = Some(now),
            _ => self.horn_at = Some(now),
        }
        true
    }

    /// Until when lines are locked out (0 when they never were).
    pub fn locked_until(&self) -> u64 {
        self.locked_until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_presets_are_valid() {
        assert!(valid(LINE, 0) && valid(LINE, 13) && !valid(LINE, 14));
        assert!(valid(EMOTE, 7) && !valid(EMOTE, 8));
        assert!(valid(HORN, 0) && !valid(HORN, 1));
        assert!(!valid(3, 0));
        assert!(POST_LINES.iter().all(|&id| valid(LINE, id)));
        assert!(
            Chat {
                peer: 20,
                kind: LINE,
                id: 0
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn three_lines_in_five_seconds_then_a_lockout() {
        let mut l = Limits::default();
        assert!(l.take(LINE, 0) && l.take(LINE, 1000) && l.take(LINE, 2000));
        // The fourth inside the window locks lines out for 5 s.
        assert!(!l.take(LINE, 3000));
        assert!(!l.take(LINE, 7999));
        assert!(l.take(LINE, 8000));
        // Emotes and the horn keep their own timers.
        assert!(l.take(EMOTE, 8000) && !l.take(EMOTE, 9999) && l.take(EMOTE, 10_000));
        assert!(l.take(HORN, 8000) && !l.take(HORN, 8999) && l.take(HORN, 9000));
    }

    #[test]
    fn lines_spread_out_never_lock() {
        let mut l = Limits::default();
        for i in 0..20 {
            assert!(l.take(LINE, i * 1700), "{i}");
        }
        assert_eq!(l.locked_until(), 0);
    }
}
