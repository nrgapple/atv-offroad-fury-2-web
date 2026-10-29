//! Split Screen → Players: the original's MultiATVRiderSelect page (archive
//! PAGE08, `QuadzillaUIPageMultiATVRiderSelect`, vtable `0045c810`), where
//! every local player claims a seat with its own controller and dresses
//! its rider before a split event (research/SPLIT_SCREEN.md "Players").
//!
//! Recovered (USA v2.00): the page handler `0027a0a0` reads each pad's
//! buttons for its own player (`0025bb10`); a seat that has not joined
//! takes only START, which joins it (`0027e780`). A joined player's rows
//! are ATV, Rider, Gear, Helmet, Goggles, Gloves, Boots (up/down move,
//! left/right change) and START marks it ready (`0027eb70`) or, when ready,
//! takes it back. Triangle on player 1's pad leaves the page. The update
//! (`0027d300`) shows Start/Force Start once two players have joined, and
//! starts the event (`0027e360`) once player 1 and every joined player are
//! ready (`0027e230`), seating the joined players in pad order.
//!
//! Port adaptations: the page opens with the last session's players
//! joined (the Split Screen page's Players count) instead of player 1 only;
//! a joined player 2-4 can leave again (Circle/Triangle when not ready);
//! the Gear row steps the jersey line with its sleeves and pants, and a
//! Paint row follows Boots. Keyboard, mouse and touch reach every seat
//! through the page's rows, and **Start Event** (the original's Force
//! Start label) starts with the joined players.
use super::{
    Event, Frontend, SPLIT_PLAYERS_KEY, split_look_key, split_look_value, split_player_key,
};
use crate::{hud::Viewport, settings::Settings};
use atv_core::rider_gear::{Appearance, PAINTS};

/// A seat's value rows in the original's order, then the port's Paint.
pub const SEAT_ROWS: [(&str, &str); 8] = [
    ("quad", "ATV"),
    ("rider", "Rider"),
    ("jersey", "Gear"),
    ("helmet", "Helmet"),
    ("goggle", "Goggles"),
    ("glove", "Gloves"),
    ("boot", "Boots"),
    ("paint", "Paint"),
];

/// The seated players: count, players 2-4's ATV/rider and look in seat
/// order.
pub type Launch = (u8, [(u16, u16); 3], [Appearance; 3]);

/// The Players page's working state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seats {
    pub joined: [bool; 4],
    pub ready: [bool; 4],
    /// Each pad's row among [`SEAT_ROWS`] (players 2-4).
    pub cursor: [usize; 4],
    /// Each pad's buttons last frame (edge detection), `u32::MAX` until
    /// released after the page opened.
    held: [u32; 4],
    /// The event the page holds until the players are seated.
    pub pending: Option<Event>,
    /// Let the next event through (the page released it).
    pub release: bool,
    /// The seated players: count, players 2-4's ATV/rider and look in
    /// seat order (the joined players compacted, as `0027e360` copies them).
    pub launch: Option<Launch>,
}

impl Seats {
    pub fn count(&self) -> usize {
        self.joined.iter().filter(|j| **j).count()
    }

    /// `0027e230`: every joined player ready, player 1 among them, and at
    /// least two seated.
    pub fn all_ready(&self) -> bool {
        self.joined[0]
            && self.count() >= 2
            && self.joined.iter().zip(self.ready).all(|(j, r)| !*j || r)
    }
}

/// Events a split session starts with (the app seats them).
fn split_event(event: &Event) -> bool {
    matches!(
        event,
        Event::Ride { .. }
            | Event::Race { .. }
            | Event::Game { .. }
            | Event::Hockey { .. }
            | Event::TagHost { .. }
            | Event::Objective { .. }
    )
}

impl Frontend {
    /// Holds a split event on the Players page until the players are
    /// seated (the original's TrackSelect → MultiATVRiderSelect → event).
    pub(super) fn seat_gate(&mut self, event: Option<Event>, view: &Viewport) -> Option<Event> {
        let event = event?;
        if std::mem::take(&mut self.seats.release) || !self.split || !split_event(&event) {
            return Some(event);
        }
        let players = usize::from(self.split_players.clamp(2, 4));
        self.seats = Seats {
            joined: std::array::from_fn(|i| i < players),
            held: [u32::MAX; 4],
            pending: Some(event),
            ..Seats::default()
        };
        self.show("splitseats", view);
        None
    }

    /// The seated players for the app (`split::Request`): the page's
    /// launch, else the Split Screen page's count and seats.
    pub fn split_launch(&self) -> Launch {
        self.seats
            .launch
            .unwrap_or((self.split_players, self.split_seconds, self.split_looks))
    }

    /// The page's rows (action, label), the order `choices` keeps.
    pub(super) fn seat_choices(&self) -> Vec<(String, String)> {
        let s = &self.seats;
        let mut rows = Vec::new();
        if s.count() >= 2 {
            rows.push((
                "splitgo".to_string(),
                format!("Start Event · {} Players", s.count()),
            ));
        }
        for i in 0..4 {
            let n = i + 1;
            if !s.joined[i] {
                rows.push((
                    format!("seat{n}join"),
                    format!("Player {n}: Press START to Join"),
                ));
                continue;
            }
            rows.push((
                format!("seat{n}ready"),
                if s.ready[i] {
                    format!("Player {n}: Ready")
                } else {
                    format!("Player {n}: Press START when Ready")
                },
            ));
            if i == 0 {
                continue;
            }
            for (k, (row, label)) in SEAT_ROWS.iter().enumerate() {
                let marker = if s.cursor[i] == k && !s.ready[i] {
                    "> "
                } else {
                    ""
                };
                rows.push((
                    format!("seat{n}{row}"),
                    format!("{marker}P{n} {label}: {}", self.seat_value(i, row)),
                ));
            }
            rows.push((format!("seat{n}leave"), format!("Player {n}: Leave")));
        }
        rows
    }

    fn seat_value(&self, seat: usize, row: &str) -> String {
        let (quad, rider) = self.split_seconds[seat - 1];
        let look = self.split_looks[seat - 1];
        match row {
            "quad" => self.quad_label(quad),
            "rider" => if rider == 1 { "Female" } else { "Male" }.into(),
            "jersey" => self.jersey_label(look),
            "paint" => PAINTS[usize::from(look.paint).min(PAINTS.len() - 1)].into(),
            slot => self.gear_name(look, slot),
        }
    }

    /// Steps one of seat `seat`'s (1-3: players 2-4) rows and saves it.
    fn step_seat(&mut self, seat: usize, row: &str, direction: i32, settings: &mut Settings) {
        let n = seat as u8 + 1;
        match row {
            "quad" | "rider" => {
                let (mut quad, mut rider) = self.split_seconds[seat - 1];
                if row == "rider" {
                    rider = 1 - rider.min(1);
                } else {
                    let count = i32::from(super::SPLIT_QUADS);
                    for _ in 0..count {
                        quad = ((i32::from(quad) - 1 + direction).rem_euclid(count) + 1) as u16;
                        if self.quad_owned(quad) {
                            break;
                        }
                    }
                }
                self.split_seconds[seat - 1] = (quad, rider);
                settings.set(&split_player_key(n), &format!("{quad},{rider}"));
            }
            _ => {
                let mut look = self.split_looks[seat - 1];
                match row {
                    "paint" => {
                        look.paint = (i32::from(look.paint) + direction)
                            .rem_euclid(PAINTS.len() as i32)
                            as u8;
                    }
                    "jersey" => look = self.step_split_gear(look),
                    slot => {
                        // Rider Gear's own stepping (owned items only).
                        let own = self.appearance;
                        self.appearance = look;
                        self.step_gear(slot, direction);
                        look = self.appearance;
                        self.appearance = own;
                    }
                }
                self.split_looks[seat - 1] = look;
                settings.set(&split_look_key(n), &split_look_value(&look));
            }
        }
    }

    /// START on a seat: join it, or toggle ready (`0027e780` / `0027eb70`).
    fn seat_start(&mut self, seat: usize) {
        if !self.seats.joined[seat] {
            self.seats.joined[seat] = true;
            self.seats.ready[seat] = false;
        } else {
            self.seats.ready[seat] = !self.seats.ready[seat];
        }
    }

    fn seat_leave(&mut self, seat: usize) {
        if seat > 0 {
            self.seats.joined[seat] = false;
            self.seats.ready[seat] = false;
            self.seats.cursor[seat] = 0;
        }
    }

    /// Starts the held event with the joined players (Force Start, or
    /// `0027e360` once everyone is ready).
    fn seat_launch(&mut self, settings: &mut Settings) -> Option<Event> {
        let s = &self.seats;
        if s.count() < 2 || !s.joined[0] {
            return None;
        }
        let mut seconds = self.split_seconds;
        let mut looks = self.split_looks;
        let joined: Vec<usize> = (1..4).filter(|i| s.joined[*i]).collect();
        for (to, from) in joined.iter().enumerate() {
            seconds[to] = self.split_seconds[from - 1];
            looks[to] = self.split_looks[from - 1];
        }
        let players = s.count() as u8;
        self.split_players = players;
        settings.set(SPLIT_PLAYERS_KEY, &players.to_string());
        self.seats.launch = Some((players, seconds, looks));
        self.seats.release = true;
        self.seats.pending.clone()
    }

    fn seat_changed(&mut self, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        if self.seats.all_ready() {
            return self.seat_launch(settings);
        }
        self.refresh(view);
        None
    }

    /// A row of the Players page (keyboard, mouse, touch or player 1's
    /// pad). Returns `Some(None)` when handled without an event.
    pub(super) fn seat_action(
        &mut self,
        action: &str,
        direction: i32,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Option<Event>> {
        if action == "splitgo" {
            return Some(self.seat_launch(settings));
        }
        let rest = action.strip_prefix("seat")?;
        let n = usize::from(rest.as_bytes().first()?.wrapping_sub(b'0'));
        if !(1..=4).contains(&n) {
            return None;
        }
        let (seat, row) = (n - 1, &rest[1..]);
        match row {
            "join" | "ready" => self.seat_start(seat),
            "leave" => self.seat_leave(seat),
            row if SEAT_ROWS.iter().any(|(r, _)| *r == row) && seat > 0 => {
                if self.seats.ready[seat] {
                    return Some(None);
                }
                self.seats.cursor[seat] = SEAT_ROWS.iter().position(|(r, _)| *r == row)?;
                self.step_seat(seat, row, direction, settings);
            }
            _ => return None,
        }
        Some(self.seat_changed(view, settings))
    }

    /// Player `slot + 1`'s own controller on the Players page (pads 2-4;
    /// pad 1 drives the page like any menu). START (or Cross) joins and
    /// marks ready, Circle/Triangle takes ready back or leaves, the D-pad
    /// or left stick moves its row and changes its value.
    pub fn seat_pad(
        &mut self,
        slot: usize,
        pad: Option<(&[f32], &[f32])>,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Event> {
        if self.page != "splitseats" || !(1..4).contains(&slot) {
            return None;
        }
        let mut held = 0u32;
        if let Some((buttons, axes)) = pad {
            let b = |i: usize| buttons.get(i).copied().unwrap_or(0.) > 0.5;
            let a = |i: usize| axes.get(i).copied().unwrap_or(0.);
            for (bit, on) in [
                (0, b(9) || b(0)),
                (1, b(1) || b(3)),
                (2, b(12) || a(1) < -0.55),
                (3, b(13) || a(1) > 0.55),
                (4, b(14) || a(0) < -0.55),
                (5, b(15) || a(0) > 0.55),
            ] {
                if on {
                    held |= 1 << bit;
                }
            }
        }
        let last = self.seats.held[slot];
        self.seats.held[slot] = if last == u32::MAX && held != 0 {
            u32::MAX
        } else {
            held
        };
        let fresh = if last == u32::MAX { 0 } else { held & !last };
        if fresh == 0 {
            return None;
        }
        let seat = slot;
        let (joined, ready) = (self.seats.joined[seat], self.seats.ready[seat]);
        if fresh & 1 != 0 {
            self.seat_start(seat);
        } else if !joined {
            return None;
        } else if fresh & 2 != 0 {
            if ready {
                self.seats.ready[seat] = false;
            } else {
                self.seat_leave(seat);
            }
        } else if ready {
            return None;
        } else if fresh & 0b1100 != 0 {
            let step = if fresh & 4 != 0 { -1 } else { 1 };
            self.seats.cursor[seat] =
                (self.seats.cursor[seat] as i32 + step).rem_euclid(SEAT_ROWS.len() as i32) as usize;
        } else {
            let direction = if fresh & 16 != 0 { -1 } else { 1 };
            let row = SEAT_ROWS[self.seats.cursor[seat]].0;
            self.step_seat(seat, row, direction, settings);
        }
        let event = self.seat_changed(view, settings);
        self.seat_gate(event, view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyone_joined_and_ready_starts_the_event() {
        let mut s = Seats {
            joined: [true, false, true, false],
            ..Seats::default()
        };
        assert_eq!(s.count(), 2);
        assert!(!s.all_ready());
        s.ready[0] = true;
        assert!(!s.all_ready(), "player 3 is not ready");
        s.ready[2] = true;
        assert!(s.all_ready());
        // Player 1 alone never starts a split event.
        let alone = Seats {
            joined: [true, false, false, false],
            ready: [true; 4],
            ..Seats::default()
        };
        assert!(!alone.all_ready());
    }
}
