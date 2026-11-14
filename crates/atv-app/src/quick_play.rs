//! Quick Play on the client (research/QUICK_PLAY.md): the server-run room
//! loop drawn as the Quick Play panel, this rider's votes, and the rounds
//! this rider starts when the server makes it the round's driver.
//!
//! The round itself is the existing room race / room game / room Freestyle
//! (research/MULTIPLAYER.md): the driver starts it exactly as a private
//! room's host does, with the server's setup, and the others follow it as
//! guests.
use atv_game::multiplayer::quickplay::{
    NO_CHOICE, RANDOM, RoomLoop, Setup, Vote, loop_phase, playlist, rematch, result_flag, seat_flag,
};
use atv_game::multiplayer::{CPU_PEER, game_kind};
use atv_ui::hud::quickplay::{self as panel, CardView, Phase, RowView, SeatView, View};

/// Votes are resent this often (they carry the rider's ping).
const RESEND_MS: f64 = 2_000.;
/// The podium's values count up over this long (ms).
const COUNT_UP_MS: f64 = 1_200.;
/// The win flash (ms).
const FLASH_MS: f64 = 700.;

/// What the driver must start now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Start {
    Race(Setup),
    Freestyle(Setup),
    Game(atv_core::games::GameKind, Setup),
}

/// This rider's side of the loop.
#[derive(Debug, Default)]
pub struct QuickPlay {
    pub playlist: u8,
    /// The room, avoided when Leave queues again.
    pub code: String,
    choice: u8,
    rematch: u8,
    ready: bool,
    sent: Option<Vote>,
    sent_at: f64,
    /// The last round number this rider started as driver.
    started: u32,
    phase: u8,
    phase_at: f64,
    focus: usize,
    focus_visible: bool,
    /// Leave was pressed: queue again elsewhere.
    pub leave: bool,
    pad_held: [bool; 3],
    /// The last panel built (tests and diagnostics).
    pub view: View,
}

/// A round's mode word on its card.
pub fn mode_word(mode: u8) -> &'static str {
    match mode {
        game_kind::RACE => "Race",
        game_kind::KING_OF_THE_HILLS => "Hills",
        game_kind::TREASURE_HUNT => "Treasure",
        game_kind::FREESTYLE => "Stunts",
        _ => "Random",
    }
}

/// `m:ss.cc`, or points.
fn value_text(mode: u8, value: f64, dnf: bool) -> String {
    if dnf {
        return "DNF".into();
    }
    if mode == game_kind::RACE {
        crate::room_race::clock((value / 1000.) as f32)
    } else {
        format!("{}", value.round() as u64)
    }
}

impl QuickPlay {
    pub fn new(playlist: u8, code: String) -> Self {
        QuickPlay {
            playlist,
            code,
            choice: NO_CHOICE,
            ..Default::default()
        }
    }

    /// The vote packet for the loop as it is now.
    fn vote(&self, state: &RoomLoop, ping: u16) -> Vote {
        let round = if state.phase == loop_phase::VOTE {
            state.setup.number + 1
        } else {
            state.setup.number
        };
        Vote {
            round,
            choice: if state.phase == loop_phase::VOTE {
                self.choice
            } else {
                NO_CHOICE
            },
            rematch: if state.phase == loop_phase::RESULTS {
                self.rematch
            } else {
                rematch::UNDECIDED
            },
            ready: state.phase == loop_phase::LOBBY && self.ready,
            ping_ms: ping,
        }
    }

    /// One frame: follow the loop, send this rider's choices, and say what
    /// to start when this rider drives a round that has not started.
    /// `environment` is the course this client has loaded; `name` names a
    /// seat, `course` a venue.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        state: &RoomLoop,
        me: Option<u8>,
        now: f64,
        server_now: Option<f64>,
        ping: Option<u32>,
        environment: &str,
        loaded: bool,
        name: &dyn Fn(u8) -> String,
        course: &dyn Fn(&str) -> String,
        narrow: bool,
    ) -> (Option<Vote>, Option<Start>) {
        if state.phase != self.phase {
            self.phase = state.phase;
            self.phase_at = now;
            self.focus = 0;
            match state.phase {
                loop_phase::VOTE => self.choice = NO_CHOICE,
                loop_phase::RESULTS => self.rematch = rematch::UNDECIDED,
                loop_phase::LOBBY => self.ready = false,
                // A round this rider does not drive from its start (or
                // first sees under way) it follows; if the room passes to
                // it mid-round, it runs on from the mirror, never anew.
                loop_phase::PLAY if Some(state.driver) != me => {
                    self.started = self.started.max(state.setup.number);
                }
                _ => {}
            }
        }
        let ping = ping.map_or(0, |p| p.clamp(1, 9_999) as u16);
        let vote = self.vote(state, ping);
        let send = (self.sent != Some(vote) || now - self.sent_at >= RESEND_MS) && me.is_some();
        let vote = send.then(|| {
            self.sent = Some(vote);
            self.sent_at = now;
            vote
        });
        let start = self.start(state, me, environment, loaded);
        self.view = self.build(state, me, now, server_now, name, course, narrow);
        (vote, start)
    }

    /// The driver starts each round once, on the round's course.
    fn start(
        &mut self,
        state: &RoomLoop,
        me: Option<u8>,
        environment: &str,
        loaded: bool,
    ) -> Option<Start> {
        let setup = state.setup;
        if state.phase != loop_phase::PLAY
            || me.is_none()
            || Some(state.driver) != me
            || setup.number <= self.started
            || !loaded
            || setup.round.environment() != Some(environment)
        {
            return None;
        }
        self.started = setup.number;
        match setup.round.mode {
            game_kind::RACE => Some(Start::Race(setup)),
            game_kind::FREESTYLE => Some(Start::Freestyle(setup)),
            mode => crate::room_game::kind(mode).map(|kind| Start::Game(kind, setup)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        &self,
        state: &RoomLoop,
        me: Option<u8>,
        now: f64,
        server_now: Option<f64>,
        name: &dyn Fn(u8) -> String,
        course: &dyn Fn(&str) -> String,
        narrow: bool,
    ) -> View {
        let phase = match state.phase {
            loop_phase::LOBBY => Phase::Lobby,
            loop_phase::VOTE => Phase::Vote,
            loop_phase::COUNTDOWN => Phase::Countdown,
            loop_phase::PLAY => Phase::Play,
            _ => Phase::Results,
        };
        let seconds = server_now
            .filter(|_| state.deadline_ms > 0)
            .map(|t| ((state.deadline_ms as f64 - t) / 1000.).ceil().max(0.) as u32);
        let seats: Vec<SeatView> = state
            .seats
            .iter()
            .map(|s| {
                let cpu = s.flags & seat_flag::CPU != 0;
                let mine = Some(s.peer) == me;
                SeatView {
                    name: name(s.peer),
                    cpu,
                    me: mine,
                    ping: (!cpu && !mine && s.ping_ms > 0).then_some(s.ping_ms),
                    ready: s.flags & seat_flag::READY != 0,
                    voted: s.flags & seat_flag::VOTED != 0,
                }
            })
            .collect();
        // Stunts rooms show their open seats; a race's are CPUs.
        let open = if seats.iter().any(|s| s.cpu) {
            0
        } else {
            atv_game::multiplayer::quickplay::QUICKPLAY_HUMANS.saturating_sub(seats.len())
        };
        let card = |mode: u8, map: u16, votes: u8, chosen: bool| CardView {
            mode: mode_word(mode).into(),
            track: atv_game::multiplayer::quickplay::Round {
                mode,
                map,
                variant: 0,
            }
            .environment()
            .map_or_else(String::new, course),
            votes,
            chosen,
        };
        let mut cards: Vec<CardView> = state
            .options
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut c = card(r.mode, r.map, state.votes[i], self.choice == i as u8);
                if r.mode == game_kind::RACE && r.variant > 0 {
                    c.track.push_str(match r.variant {
                        1 => " Long",
                        2 => " Short",
                        _ => " ST",
                    });
                }
                c
            })
            .collect();
        if phase == Phase::Vote {
            cards.push(CardView {
                mode: "Random".into(),
                track: "?".into(),
                votes: state.votes[usize::from(RANDOM)],
                chosen: self.choice == RANDOM,
            });
        }
        let picked = (phase == Phase::Countdown)
            .then(|| card(state.setup.round.mode, state.setup.round.map, 0, false));
        let since = now - self.phase_at;
        let grow = (since / COUNT_UP_MS).clamp(0., 1.);
        let rows = state
            .results
            .iter()
            .map(|r| {
                let dnf = r.flags & result_flag::DNF != 0;
                RowView {
                    name: name(r.peer),
                    value: value_text(state.setup.round.mode, f64::from(r.value) * grow, dnf),
                    cpu: r.flags & result_flag::CPU != 0,
                    me: Some(r.peer) == me,
                    dnf,
                }
            })
            .collect();
        let won =
            phase == Phase::Results && state.results.first().is_some_and(|r| Some(r.peer) == me);
        let me_seat = me.and_then(|m| state.seat(m));
        View {
            phase,
            playlist: if state.playlist == playlist::STUNTS {
                "Stunts".into()
            } else {
                "Race".into()
            },
            seconds,
            seats,
            open,
            cards,
            picked,
            rows,
            ready: self.ready,
            stay: self.rematch == rematch::STAY,
            leaving: self.rematch == rematch::LEAVE,
            focus: self.focus,
            focus_visible: self.focus_visible,
            flash: if won && since < FLASH_MS {
                (1. - since / FLASH_MS) as f32
            } else {
                0.
            },
            spectating: phase == Phase::Play
                && me_seat.is_some_and(|s| s.flags & seat_flag::SPECTATING != 0),
            narrow,
        }
    }

    /// A panel button was pressed (tap, Enter or a controller's A).
    pub fn press(&mut self, id: u32) {
        match id {
            panel::READY => self.ready = !self.ready,
            panel::REMATCH => self.rematch = rematch::STAY,
            panel::LEAVE => {
                self.rematch = rematch::LEAVE;
                self.leave = true;
            }
            id if (panel::CARD..panel::CARD + 4).contains(&id) => {
                self.choice = (id - panel::CARD) as u8;
            }
            _ => {}
        }
    }

    /// A key while the panel takes input: true when taken.
    pub fn key(&mut self, code: &str) -> bool {
        let mut view = self.view.clone();
        view.focus = self.focus;
        let (taken, pressed) = panel::key(&mut view, code);
        self.focus = view.focus;
        self.focus_visible |= view.focus_visible;
        if let Some(id) = pressed {
            self.press(id);
        }
        taken
    }

    /// The controller beside the ride (D-pad and A, not taken from it).
    pub fn pad(&mut self, buttons: &[f32]) {
        let mut view = self.view.clone();
        view.focus = self.focus;
        let pressed = panel::pad(&mut view, buttons, &mut self.pad_held);
        self.focus = view.focus;
        self.focus_visible |= view.focus_visible;
        if let Some(id) = pressed {
            self.press(id);
        }
    }

    /// A seat's or podium row's name: the room's riders, the race's CPU
    /// roster names, else "CPU n".
    pub fn cpu_name(peer: u8) -> String {
        format!("CPU {}", peer.saturating_sub(CPU_PEER) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_game::multiplayer::quickplay::{Placing, Round, Seat};

    fn names(peer: u8) -> String {
        if peer >= CPU_PEER {
            QuickPlay::cpu_name(peer)
        } else {
            format!("R{peer}")
        }
    }

    fn state(phase: u8) -> RoomLoop {
        RoomLoop {
            phase,
            playlist: playlist::RACE,
            deadline_ms: 10_000,
            options: vec![
                Round {
                    mode: game_kind::RACE,
                    map: 15,
                    variant: 0,
                },
                Round {
                    mode: game_kind::KING_OF_THE_HILLS,
                    map: 5,
                    variant: 0,
                },
                Round {
                    mode: game_kind::RACE,
                    map: 1,
                    variant: 1,
                },
            ],
            votes: [0, 1, 0, 0],
            pick: NO_CHOICE,
            setup: Setup {
                round: Round {
                    mode: game_kind::RACE,
                    map: 15,
                    variant: 0,
                },
                number: 3,
                laps: 2,
                minutes: 0,
                cpu_field: 6,
                difficulty: 1,
                seed: 5,
            },
            driver: 0,
            seats: vec![
                Seat {
                    peer: 0,
                    flags: seat_flag::DRIVER,
                    ping_ms: 30,
                },
                Seat {
                    peer: 1,
                    flags: 0,
                    ping_ms: 170,
                },
                Seat {
                    peer: CPU_PEER,
                    flags: seat_flag::CPU,
                    ping_ms: 0,
                },
            ],
            results: vec![
                Placing {
                    peer: 0,
                    flags: 0,
                    value: 61_000,
                },
                Placing {
                    peer: CPU_PEER,
                    flags: result_flag::CPU | result_flag::DNF,
                    value: 0,
                },
            ],
            verified: true,
        }
    }

    fn frame(
        q: &mut QuickPlay,
        s: &RoomLoop,
        now: f64,
        env: &str,
    ) -> (Option<Vote>, Option<Start>) {
        q.frame(
            s,
            Some(0),
            now,
            Some(4_000.),
            Some(40),
            env,
            true,
            &names,
            &|e: &str| e.to_uppercase(),
            true,
        )
    }

    #[test]
    fn votes_follow_the_cards_and_are_resent_with_the_ping() {
        let mut q = QuickPlay::new(playlist::RACE, "QPR00001".into());
        let vote = state(loop_phase::VOTE);
        let (sent, _) = frame(&mut q, &vote, 0., "nat01");
        assert_eq!(sent.unwrap().choice, NO_CHOICE);
        assert_eq!(sent.unwrap().round, 4);
        assert_eq!(sent.unwrap().ping_ms, 40);
        // Unchanged: nothing until the resend.
        assert_eq!(frame(&mut q, &vote, 100., "nat01").0, None);
        q.press(panel::CARD + 1);
        assert_eq!(frame(&mut q, &vote, 200., "nat01").0.unwrap().choice, 1);
        assert!(frame(&mut q, &vote, 2_300., "nat01").0.is_some());
        // The panel: 3 cards and Random, live counts, the countdown.
        let v = &q.view;
        assert_eq!(v.cards.len(), 4);
        assert_eq!(v.cards[1].mode, "Hills");
        assert_eq!(v.cards[1].votes, 1);
        assert!(v.cards[1].chosen);
        assert_eq!(v.cards[2].track, "ENV01 Long");
        assert_eq!(v.seconds, Some(6));
        // Ping bars only for others, none for CPUs.
        assert_eq!(v.seats[0].ping, None);
        assert_eq!(v.seats[1].ping, Some(170));
        assert!(v.seats[2].cpu && v.seats[2].ping.is_none());
    }

    #[test]
    fn the_driver_starts_each_round_once_on_its_course() {
        let mut q = QuickPlay::new(playlist::RACE, "QPR00001".into());
        let play = state(loop_phase::PLAY);
        // Still on the previous course: wait for the reload.
        assert_eq!(frame(&mut q, &play, 0., "env05").1, None);
        let (_, start) = frame(&mut q, &play, 10., "nat01");
        assert_eq!(start, Some(Start::Race(play.setup)));
        assert_eq!(frame(&mut q, &play, 20., "nat01").1, None);
        // Not the driver: nothing to start.
        let mut q = QuickPlay::new(playlist::RACE, "QPR00001".into());
        let other = RoomLoop {
            driver: 1,
            ..state(loop_phase::PLAY)
        };
        assert_eq!(frame(&mut q, &other, 0., "nat01").1, None);
        // A rider handed the room mid-round does not restart the round.
        assert_eq!(frame(&mut q, &play, 10., "nat01").1, None);
    }

    #[test]
    fn the_podium_counts_up_and_flashes_for_a_win() {
        let mut q = QuickPlay::new(playlist::RACE, "QPR00001".into());
        let results = state(loop_phase::RESULTS);
        frame(&mut q, &results, 1_000., "nat01");
        assert_eq!(q.view.rows[0].value, "0:00.00");
        assert!(q.view.flash > 0.9);
        frame(&mut q, &results, 1_000. + COUNT_UP_MS, "nat01");
        assert_eq!(q.view.rows[0].value, "1:01.00");
        assert_eq!(q.view.rows[1].value, "DNF");
        assert_eq!(q.view.rows[1].name, "CPU 1");
        assert_eq!(q.view.flash, 0.);
        // Leave asks to queue again; Rematch is sent as the choice.
        q.press(panel::REMATCH);
        let (sent, _) = frame(&mut q, &results, 3_000., "nat01");
        assert_eq!(sent.unwrap().rematch, rematch::STAY);
        q.press(panel::LEAVE);
        assert!(q.leave);
    }
}
