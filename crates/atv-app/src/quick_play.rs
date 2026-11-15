//! Quick Play on the client (research/QUICK_PLAY.md): the server-run room
//! loop drawn as the Quick Play panel, this rider's votes, and the rounds
//! this rider starts when the server makes it the round's driver.
//!
//! The round itself is the existing room race / room game / room Freestyle
//! (research/MULTIPLAYER.md): the driver starts it exactly as a private
//! room's host does, with the server's setup, and the others follow it as
//! guests.
use atv_game::multiplayer::quickplay::{
    NO_CHOICE, RANDOM, Reward, Rewards, RoomLoop, Round, Setup, Vote, loop_phase, playlist,
    rematch, result_flag, reward_flag, seat_flag,
};
use atv_game::multiplayer::{CPU_PEER, game_kind};
use atv_ui::hud::quickplay::{self as panel, CardView, LevelView, Phase, RowView, SeatView, View};

/// Votes are resent this often (they carry the rider's ping).
const RESEND_MS: f64 = 2_000.;
/// The podium's values count up over this long (ms).
const COUNT_UP_MS: f64 = 1_200.;
/// The win flash (ms).
const FLASH_MS: f64 = 700.;
/// The level-up beat: its flash and confetti (ms).
const BEAT_MS: f64 = 1_600.;
/// The weekly league tiers (research/ONLINE_IDENTITY.md).
const TIER_NAMES: [&str; 6] = [
    "Bronze", "Silver", "Gold", "Platinum", "Diamond", "Champion",
];

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
    /// The round whose podium rewards came, and when.
    rewards_round: u32,
    rewards_at: f64,
    /// The countdown's last beeped second.
    beeped: Option<u32>,
    /// One-shots for the ride's mixer (the countdown's beeps).
    pub sounds: Vec<&'static str>,
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

/// This rider's level bar for a reward `gain` (0-1) into its count-up: a
/// level-up fills the new level from empty; otherwise the bar grows from
/// the XP before the round.
fn level_view(w: &Reward, gain: f64, beat: Option<f64>) -> LevelView {
    let span = f64::from(w.next_xp.saturating_sub(w.level_xp).max(1));
    let after = f64::from(w.xp.saturating_sub(w.level_xp)) / span;
    let up = w.flags & reward_flag::LEVEL_UP != 0;
    let before = if up {
        0.
    } else {
        f64::from(
            w.xp.saturating_sub(u32::from(w.points))
                .saturating_sub(w.level_xp),
        ) / span
    };
    LevelView {
        level: w.level,
        fill: (before + (after - before) * gain).clamp(0., 1.) as f32,
        gain: format!("+{} XP", (f64::from(w.points) * gain).round() as u32),
        tier: TIER_NAMES
            .get(usize::from(w.tier))
            .copied()
            .unwrap_or("Bronze")
            .into(),
        tier_up: w.flags & reward_flag::TIER_UP != 0 && gain >= 1.,
        level_up: up && gain >= 1.,
        beat: beat.map_or(0., |t| t as f32),
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
    /// seat, `course` a venue; `rewards` are the latest podium rewards.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        state: &RoomLoop,
        rewards: Option<&Rewards>,
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
            self.beeped = None;
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
        // The countdown card beeps its last three seconds with the start
        // lamps' beep; the round's own lamps play GO.
        if state.phase == loop_phase::COUNTDOWN
            && let Some(t) = server_now.filter(|_| state.deadline_ms > 0)
        {
            let left = ((state.deadline_ms as f64 - t) / 1000.).ceil().max(0.) as u32;
            if (1..=3).contains(&left) && self.beeped.is_none_or(|b| left < b) {
                self.beeped = Some(left);
                self.sounds.push("beep01.vag");
            }
        }
        let mine = rewards.filter(|r| r.round == state.setup.number && r.round > 0);
        if state.phase == loop_phase::RESULTS
            && let Some(r) = mine
            && r.round != self.rewards_round
        {
            self.rewards_round = r.round;
            self.rewards_at = now;
        }
        let rewards = mine.filter(|_| state.phase == loop_phase::RESULTS);
        let ping = ping.map_or(0, |p| p.clamp(1, 9_999) as u16);
        let vote = self.vote(state, ping);
        let send = (self.sent != Some(vote) || now - self.sent_at >= RESEND_MS) && me.is_some();
        let vote = send.then(|| {
            self.sent = Some(vote);
            self.sent_at = now;
            vote
        });
        let start = self.start(state, me, environment, loaded);
        self.view = self.build(state, rewards, me, now, server_now, name, course, narrow);
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
        rewards: Option<&Rewards>,
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
        // Rewards count up from their arrival, then a level-up beats.
        let earned = now - self.rewards_at;
        let gain = (earned / COUNT_UP_MS).clamp(0., 1.);
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
                    points: rewards
                        .and_then(|w| w.of(r.peer))
                        .map(|w| format!("+{}", (f64::from(w.points) * gain).round() as u32)),
                }
            })
            .collect();
        let reward = rewards.zip(me).and_then(|(w, me)| w.of(me)).copied();
        let beat = reward
            .filter(|w| w.flags & reward_flag::LEVEL_UP != 0)
            .map(|_| (earned - COUNT_UP_MS) / BEAT_MS)
            .filter(|t| (0. ..1.).contains(t));
        let level = reward.map(|w| level_view(&w, gain, beat));
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
                beat.map_or(0., |t| ((1. - t * 2.).max(0.) * 0.8) as f32)
            },
            level,
            confetti: beat.map(|t| t as f32),
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
        }
    }

    fn frame(
        q: &mut QuickPlay,
        s: &RoomLoop,
        now: f64,
        env: &str,
    ) -> (Option<Vote>, Option<Start>) {
        frame_with(q, s, None, now, Some(4_000.), env)
    }

    fn frame_with(
        q: &mut QuickPlay,
        s: &RoomLoop,
        rewards: Option<&Rewards>,
        now: f64,
        server_now: Option<f64>,
        env: &str,
    ) -> (Option<Vote>, Option<Start>) {
        q.frame(
            s,
            rewards,
            Some(0),
            now,
            server_now,
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

    #[test]
    fn podium_rewards_count_up_and_a_level_up_beats() {
        let mut q = QuickPlay::new(playlist::RACE, "QPR00001".into());
        let results = state(loop_phase::RESULTS);
        let reward = Reward {
            peer: 0,
            flags: reward_flag::LEVEL_UP | reward_flag::TIER_UP,
            points: 45,
            xp: 120,
            level: 2,
            level_xp: 100,
            next_xp: 300,
            tier: 1,
            week_points: 120,
        };
        let rewards = Rewards {
            round: 3,
            rewards: vec![reward],
        };
        // Another round's rewards are not shown.
        let old = Rewards {
            round: 2,
            ..rewards.clone()
        };
        frame_with(&mut q, &results, Some(&old), 0., None, "nat01");
        assert!(q.view.level.is_none() && q.view.rows[0].points.is_none());
        frame_with(&mut q, &results, Some(&rewards), 100., None, "nat01");
        assert_eq!(q.view.rows[0].points.as_deref(), Some("+0"));
        // CPUs earn nothing.
        assert_eq!(q.view.rows[1].points, None);
        let level = q.view.level.clone().unwrap();
        assert_eq!((level.level, level.fill, level.level_up), (2, 0., false));
        frame_with(
            &mut q,
            &results,
            Some(&rewards),
            100. + COUNT_UP_MS,
            None,
            "nat01",
        );
        assert_eq!(q.view.rows[0].points.as_deref(), Some("+45"));
        let level = q.view.level.clone().unwrap();
        assert_eq!(level.gain, "+45 XP");
        assert!((level.fill - 0.1).abs() < 1e-6);
        assert!(level.level_up && level.tier_up);
        assert_eq!(level.tier, "Silver");
        // The beat: flash and confetti, then calm.
        frame_with(
            &mut q,
            &results,
            Some(&rewards),
            200. + COUNT_UP_MS,
            None,
            "nat01",
        );
        assert!(q.view.confetti.is_some() && q.view.flash > 0.);
        frame_with(
            &mut q,
            &results,
            Some(&rewards),
            200. + COUNT_UP_MS + BEAT_MS,
            None,
            "nat01",
        );
        assert!(q.view.confetti.is_none());
        // Without a level-up the bar grows from the XP before the round.
        let plain = Reward {
            flags: 0,
            xp: 250,
            ..reward
        };
        let view = level_view(&plain, 0., None);
        assert!((view.fill - 105. / 200.).abs() < 1e-6);
        assert!((level_view(&plain, 1., None).fill - 0.75).abs() < 1e-6);
    }

    #[test]
    fn the_countdown_beeps_its_last_three_seconds() {
        let mut q = QuickPlay::new(playlist::RACE, "QPR00001".into());
        let countdown = RoomLoop {
            deadline_ms: 10_000,
            ..state(loop_phase::COUNTDOWN)
        };
        // 4 s left: quiet; then one beep each for 3, 2 and 1.
        for t in [6_000., 6_500., 7_100., 7_200., 8_100., 9_100., 9_900.] {
            frame_with(&mut q, &countdown, None, t, Some(t), "env05");
        }
        assert_eq!(q.sounds, ["beep01.vag"; 3]);
    }
}
