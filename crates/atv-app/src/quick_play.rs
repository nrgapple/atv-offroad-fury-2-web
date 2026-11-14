//! Quick Play on the client (research/QUICK_PLAY.md): the server-run room
//! loop drawn as the Quick Play panel, this rider's votes, and the rounds
//! this rider starts when the server makes it the round's driver.
//!
//! The round itself is the existing room race / room game / room Freestyle
//! (research/MULTIPLAYER.md): the driver starts it exactly as a private
//! room's host does, with the server's setup, and the others follow it as
//! guests.
//!
//! A private room's host-run loop (research/QUICK_PLAY.md "Private rooms")
//! is the same panel: the room's host is the driver and also sends its
//! choices (Start, the mode, CPU fill, Skip); Leave leaves the room. A
//! rider watching a round it joined late follows a racer with a camera.
use atv_core::math::Det;
use atv_game::multiplayer::quickplay::{
    Hosting, NO_CHOICE, RANDOM, RoomLoop, Round, Setup, Vote, host_flag, loop_phase, playlist,
    rematch, result_flag, seat_flag,
};
use atv_game::multiplayer::{CPU_PEER, game_kind};
use atv_ui::hud::quickplay::{self as panel, CardView, Phase, RowView, SeatView, View};

/// Votes are resent this often (they carry the rider's ping).
const RESEND_MS: f64 = 2_000.;
/// The podium's values count up over this long (ms).
const COUNT_UP_MS: f64 = 1_200.;
/// The win flash (ms).
const FLASH_MS: f64 = 700.;
/// The modes a private room's host cycles through.
const MODES: [u8; 4] = [
    game_kind::RACE,
    game_kind::KING_OF_THE_HILLS,
    game_kind::TREASURE_HUNT,
    game_kind::FREESTYLE,
];
/// The follow camera: behind and above the watched rider (ft), easing in
/// at this rate (per second).
const FOLLOW_BACK: f32 = 26.;
const FOLLOW_UP: f32 = 10.;
const FOLLOW_EASE: f32 = 4.;

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
    /// A private room's loop (Leave leaves the room).
    pub private: bool,
    /// A private room host's choices: the mode, CPU fill, and Start / Skip
    /// until the phase changes. Taken from the loop when first seen.
    mode: u8,
    cpus: bool,
    start: bool,
    skip: bool,
    hosting_seen: bool,
    /// The host's Race laps and game minutes (its menus), set each frame.
    pub laps: u8,
    pub minutes: u8,
    /// Spectating: the rider the follow camera watches, and its pick.
    pub watch: Option<u8>,
    watch_pick: usize,
    follow: Option<Follow>,
}

/// The follow camera's eased eye, target and heading.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Follow {
    eye: [f32; 3],
    target: [f32; 3],
    heading: [f32; 2],
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
            mode: game_kind::RACE,
            laps: 3,
            minutes: 5,
            ..Default::default()
        }
    }

    /// This rider hosts the room's private loop.
    fn hosts(state: &RoomLoop, me: Option<u8>) -> bool {
        state.hosted.is_some() && me.is_some() && Some(state.driver) == me
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
            hosting: None,
        }
    }

    /// A private room host's choices for the loop as it is now.
    fn hosting(&self, state: &RoomLoop) -> Hosting {
        let mut flags = if self.cpus { host_flag::CPUS } else { 0 };
        if self.start && state.phase == loop_phase::LOBBY {
            flags |= host_flag::START;
        }
        if self.skip && state.phase == loop_phase::VOTE {
            flags |= host_flag::SKIP;
        }
        Hosting {
            mode: self.mode,
            flags,
            laps: self.laps.clamp(1, 5),
            minutes: self.minutes.clamp(1, 20),
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
        self.private = state.hosted.is_some();
        let host = Self::hosts(state, me);
        if let Some(hosted) = state.hosted.filter(|_| host && !self.hosting_seen) {
            self.hosting_seen = true;
            self.mode = hosted.mode;
            self.cpus = hosted.flags & host_flag::CPUS != 0;
        }
        if state.phase != self.phase {
            let previous = self.phase;
            self.phase = state.phase;
            self.phase_at = now;
            self.focus = 0;
            self.start = false;
            self.skip = false;
            match state.phase {
                loop_phase::VOTE => self.choice = NO_CHOICE,
                loop_phase::RESULTS => self.rematch = rematch::UNDECIDED,
                loop_phase::LOBBY => self.ready = false,
                // A round this rider does not drive from its start (or
                // first sees under way) it follows; if the room passes to
                // it mid-round, it runs on from the mirror, never anew. A
                // private room's host that started the round itself (no
                // countdown) is already running it.
                loop_phase::PLAY
                    if Some(state.driver) != me
                        || (self.private && previous != loop_phase::COUNTDOWN) =>
                {
                    self.started = self.started.max(state.setup.number);
                }
                _ => {}
            }
        }
        let ping = ping.map_or(0, |p| p.clamp(1, 9_999) as u16);
        let mut vote = self.vote(state, ping);
        if host {
            vote.hosting = Some(self.hosting(state));
        }
        let send = (self.sent != Some(vote) || now - self.sent_at >= RESEND_MS) && me.is_some();
        let vote = send.then(|| {
            self.sent = Some(vote);
            self.sent_at = now;
            vote
        });
        let start = self.start(state, me, environment, loaded);
        self.watch = self.watched(state, me);
        self.view = self.build(state, me, now, server_now, name, course, narrow);
        (vote, start)
    }

    /// Spectating: the racer to follow (the pick cycles through them).
    fn watched(&mut self, state: &RoomLoop, me: Option<u8>) -> Option<u8> {
        let spectating = state.phase == loop_phase::PLAY
            && me
                .and_then(|m| state.seat(m))
                .is_some_and(|s| s.flags & seat_flag::SPECTATING != 0);
        if !spectating {
            self.follow = None;
            return None;
        }
        let racers: Vec<u8> = state
            .seats
            .iter()
            .filter(|s| {
                Some(s.peer) != me && s.flags & (seat_flag::CPU | seat_flag::SPECTATING) == 0
            })
            .map(|s| s.peer)
            .collect();
        (!racers.is_empty()).then(|| racers[self.watch_pick % racers.len()])
    }

    /// The follow camera's (eye, target) for the watched rider at
    /// `position` moving at `velocity`: behind it along its motion (its
    /// last heading when it stops), above the ground, eased toward each new
    /// spot so a switch glides.
    pub fn follow(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        dt: f32,
        ground: &dyn Fn(f32, f32) -> f32,
    ) -> ([f32; 3], [f32; 3]) {
        let speed = velocity[0].hypot_d(velocity[2]);
        let heading = match self.follow {
            _ if speed > 3. => [velocity[0] / speed, velocity[2] / speed],
            Some(f) => f.heading,
            None => [0., 1.],
        };
        let target = [position[0], position[1] + 3., position[2]];
        let mut eye = [
            position[0] - heading[0] * FOLLOW_BACK,
            position[1] + FOLLOW_UP,
            position[2] - heading[1] * FOLLOW_BACK,
        ];
        eye[1] = eye[1].max(ground(eye[0], eye[2]) + 4.);
        let (eye, target) = match self.follow {
            Some(f) => {
                let k = (dt * FOLLOW_EASE).clamp(0., 1.);
                let ease = |a: [f32; 3], b: [f32; 3]| {
                    [
                        a[0] + (b[0] - a[0]) * k,
                        a[1] + (b[1] - a[1]) * k,
                        a[2] + (b[2] - a[2]) * k,
                    ]
                };
                (ease(f.eye, eye), ease(f.target, target))
            }
            None => (eye, target),
        };
        self.follow = Some(Follow {
            eye,
            target,
            heading,
        });
        (eye, target)
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
                    host: state.hosted.is_some() && s.peer == state.driver,
                }
            })
            .collect();
        // Stunts rooms show their open seats; a race's are CPUs.
        let open = if seats.iter().any(|s| s.cpu) {
            0
        } else {
            atv_game::multiplayer::quickplay::QUICKPLAY_HUMANS.saturating_sub(seats.len())
        };
        let card = |round: &Round, votes: u8, chosen: bool| {
            let mut track = round.environment().map_or_else(String::new, course);
            if round.mode == game_kind::RACE && round.variant > 0 {
                track.push_str(match round.variant {
                    1 => " Long",
                    2 => " Short",
                    _ => " ST",
                });
            }
            CardView {
                mode: mode_word(round.mode).into(),
                track,
                votes,
                chosen,
            }
        };
        let mut cards: Vec<CardView> = state
            .options
            .iter()
            .enumerate()
            .map(|(i, r)| card(r, state.votes[i], self.choice == i as u8))
            .collect();
        if phase == Phase::Vote {
            cards.push(CardView {
                mode: "Random".into(),
                track: "?".into(),
                votes: state.votes[usize::from(RANDOM)],
                chosen: self.choice == RANDOM,
            });
        }
        let picked = (phase == Phase::Countdown).then(|| card(&state.setup.round, 0, false));
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
        let host = Self::hosts(state, me);
        // A private room shows its mode (the host's own choice at once).
        let (mode, cpus) = match state.hosted {
            Some(_) if host => (Some(self.mode), self.cpus),
            Some(h) => (Some(h.mode), h.flags & host_flag::CPUS != 0),
            None => (None, false),
        };
        View {
            phase,
            playlist: match mode {
                Some(mode) => mode_word(mode).into(),
                None if state.playlist == playlist::STUNTS => "Stunts".into(),
                None => "Race".into(),
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
            private: state.hosted.is_some(),
            host,
            cpus,
            watching: self.watch.map(name),
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
            panel::START => self.start = true,
            panel::SKIP => self.skip = true,
            panel::CPUS => self.cpus = !self.cpus,
            panel::MODE => {
                let at = MODES.iter().position(|m| *m == self.mode).unwrap_or(0);
                self.mode = MODES[(at + 1) % MODES.len()];
            }
            panel::WATCH => self.watch_pick = self.watch_pick.wrapping_add(1),
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
    use atv_game::multiplayer::quickplay::{Placing, Seat};

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
            hosted: None,
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

    fn hosted(phase: u8) -> RoomLoop {
        RoomLoop {
            hosted: Some(atv_game::multiplayer::quickplay::Hosted {
                mode: game_kind::RACE,
                flags: 0,
            }),
            ..state(phase)
        }
    }

    #[test]
    fn a_private_room_host_sends_its_choices_until_the_phase_moves() {
        let mut q = QuickPlay::new(playlist::RACE, "ROOM0001".into());
        q.laps = 4;
        let lobby = hosted(loop_phase::LOBBY);
        let (sent, _) = frame(&mut q, &lobby, 0., "nat01");
        let h = sent.unwrap().hosting.unwrap();
        assert_eq!((h.mode, h.flags, h.laps), (game_kind::RACE, 0, 4));
        assert!(q.view.private && q.view.host);
        assert!(q.view.seats[0].host);
        assert_eq!(
            panel::buttons(&q.view),
            [panel::START, panel::MODE, panel::CPUS]
        );
        q.press(panel::CPUS);
        q.press(panel::MODE);
        q.press(panel::START);
        let h = frame(&mut q, &lobby, 10., "nat01")
            .0
            .unwrap()
            .hosting
            .unwrap();
        assert_eq!(h.mode, game_kind::KING_OF_THE_HILLS);
        assert_eq!(h.flags, host_flag::CPUS | host_flag::START);
        assert_eq!(q.view.playlist, "Hills");
        // The vote: Start is gone; Skip is the host's.
        let vote = hosted(loop_phase::VOTE);
        let h = frame(&mut q, &vote, 20., "nat01")
            .0
            .unwrap()
            .hosting
            .unwrap();
        assert_eq!(h.flags, host_flag::CPUS);
        q.press(panel::SKIP);
        let h = frame(&mut q, &vote, 30., "nat01")
            .0
            .unwrap()
            .hosting
            .unwrap();
        assert_eq!(h.flags, host_flag::CPUS | host_flag::SKIP);
        // A guest sends no host choices and gets Ready.
        let mut g = QuickPlay::new(playlist::RACE, "ROOM0001".into());
        let guest = RoomLoop {
            driver: 1,
            ..hosted(loop_phase::LOBBY)
        };
        assert_eq!(frame(&mut g, &guest, 0., "nat01").0.unwrap().hosting, None);
        assert_eq!(panel::buttons(&g.view), [panel::READY]);
        assert!(!g.view.host && g.view.seats[1].host);
    }

    #[test]
    fn a_host_never_restarts_a_round_it_started_from_pause() {
        let mut q = QuickPlay::new(playlist::RACE, "ROOM0001".into());
        frame(&mut q, &hosted(loop_phase::LOBBY), 0., "nat01");
        // Straight from the lobby to play: the host's own Pause race.
        assert_eq!(
            frame(&mut q, &hosted(loop_phase::PLAY), 10., "nat01").1,
            None
        );
        // A loop round, after its countdown, is the host's to start.
        let mut next = hosted(loop_phase::COUNTDOWN);
        next.setup.number = 4;
        frame(&mut q, &next, 20., "nat01");
        next.phase = loop_phase::PLAY;
        assert!(matches!(
            frame(&mut q, &next, 30., "nat01").1,
            Some(Start::Race(_))
        ));
    }

    #[test]
    fn a_late_joiner_follows_the_racers_with_a_camera() {
        let mut q = QuickPlay::new(playlist::RACE, "ROOM0001".into());
        let mut play = hosted(loop_phase::PLAY);
        play.seats.push(Seat {
            peer: 2,
            flags: 0,
            ping_ms: 50,
        });
        play.seats[0].flags |= seat_flag::SPECTATING;
        frame(&mut q, &play, 0., "nat01");
        assert!(q.view.spectating);
        assert_eq!(q.watch, Some(1));
        assert_eq!(q.view.watching.as_deref(), Some("R1"));
        assert_eq!(panel::buttons(&q.view), [panel::WATCH]);
        q.press(panel::WATCH);
        frame(&mut q, &play, 10., "nat01");
        assert_eq!(q.watch, Some(2));
        // Behind the rider along its motion, then eased toward the next spot.
        let flat = |_: f32, _: f32| 0.;
        let (eye, target) = q.follow([0., 0., 0.], [0., 0., 30.], 0.016, &flat);
        assert_eq!(eye, [0., FOLLOW_UP, -FOLLOW_BACK]);
        assert_eq!(target, [0., 3., 0.]);
        let (eye, _) = q.follow([0., 0., 100.], [0., 0., 30.], 0.1, &flat);
        assert!(eye[2] > -FOLLOW_BACK && eye[2] < 100. - FOLLOW_BACK);
        // Seated again: no camera.
        frame(&mut q, &hosted(loop_phase::VOTE), 20., "nat01");
        assert_eq!(q.watch, None);
    }
}
