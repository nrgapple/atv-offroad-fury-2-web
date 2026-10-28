//! Hockey on Coonce Arena (research/GAMES.md "Hockey"): samples the ride
//! for `atv_core::hockey`, moves the puck, places the rider at each
//! face-off and holds it there until the drop.
//!
//! Temporary entry (Main Menu → Hockey) until the shared Games menu lands;
//! the original is split-screen only with no CPU riders, so a single human
//! plays Red against an empty Blue net (an authored practice adaptation).
use atv_core::hockey::{Arena, Banner, Event, Game, Puck, State};
use atv_core::math::Det;
use atv_game::ride::Ride;

/// `00335590`: the event's sounds and `Crowd\hockey.aud`'s four events
/// (bank names; research/GAMES.md).
const SIREN: &str = "siren02.vag";
const GAME_OVER: &str = "gameover.vag";
const PUCK: [&str; 2] = ["puck1.vag", "puck2.vag"];
const CROWD: [&str; 4] = [
    "crowd/hockey/hcheer01.vag",
    "crowd/hockey/hcheer04.vag",
    "crowd/hockey/hockywin.vag",
    "crowd/hockey/hockyend.vag",
];

/// Samples a Coonce Arena ride loads (none elsewhere).
pub fn samples(environment: &str) -> Vec<String> {
    if environment != "game01" {
        return Vec::new();
    }
    [SIREN, GAME_OVER]
        .into_iter()
        .chain(PUCK)
        .chain(CROWD)
        .map(String::from)
        .collect()
}

pub struct HockeySession {
    /// One-shots raised since the host last took them.
    sounds: Vec<&'static str>,
    /// Hat-trick group starts (true) and stops since last taken.
    hats: Vec<bool>,
    pub game: Game,
    pub puck: Puck,
    steps: u64,
    placed: bool,
    /// The game ended (the HockeyOver menu).
    pub finished: bool,
    /// Sounds and messages of the latest tick (diagnostics).
    log: Vec<String>,
    /// `hockeyTest=fast`: a 20 s clock for browser tests.
    fast: bool,
    /// `hockeyGoal=1`: the first drop tees the puck up for the human;
    /// `hockeyGoal=3`: the first three (a hat trick) (tests).
    tee: u8,
    /// The human's rider slot (roster order online).
    me: usize,
    /// Online: the room roster the game was built for, the phase serial
    /// sent (host) or last applied (guest) and the last send step.
    roster: Vec<u8>,
    serial: u32,
    phase: u8,
    sent_step: u64,
    /// Split screen: player 2 skates for Blue on this console
    /// (research/SPLIT_SCREEN.md "Slice 3").
    pub split: bool,
}

fn phase_code(state: State) -> (u8, f32) {
    match state {
        State::FaceOff { countdown } => (0, countdown),
        State::Play => (1, 0.),
        State::SuddenDeath => (2, 0.),
        State::Pause { remaining } => (3, remaining),
        State::Over => (4, 0.),
    }
}

fn phase_state(code: u8, timer: f32) -> State {
    match code {
        0 => State::FaceOff { countdown: timer },
        1 => State::Play,
        2 => State::SuddenDeath,
        3 => State::Pause { remaining: timer },
        _ => State::Over,
    }
}

fn banner_code(banner: Option<(Banner, f32)>) -> (u8, f32) {
    match banner {
        None => (0, 0.),
        Some((Banner::Goal { team }, t)) => (1 + team.min(1) as u8, t),
        Some((Banner::HatTrick, t)) => (3, t),
        Some((Banner::SuddenDeath, t)) => (4, t),
        Some((Banner::GameOver, t)) => (5, t),
    }
}

fn banner_of(code: u8, t: f32) -> Option<(Banner, f32)> {
    let b = match code {
        1 => Banner::Goal { team: 0 },
        2 => Banner::Goal { team: 1 },
        3 => Banner::HatTrick,
        4 => Banner::SuddenDeath,
        5 => Banner::GameOver,
        _ => return None,
    };
    Some((b, t))
}

/// The drops `hockeyGoal=N` tees up for the human (test tooling).
fn tee_goals() -> u8 {
    if crate::url_flag("hockeyGoal=3") {
        3
    } else {
        u8::from(crate::url_flag("hockeyGoal=1"))
    }
}

/// "You are" alone; split screen names the player.
fn you(split: bool, me: usize) -> String {
    if split {
        format!("Player {} is", me + 1)
    } else {
        "You are".into()
    }
}

/// Rider slot of the human (team Red).
const PLAYER: usize = 0;

impl HockeySession {
    pub fn new() -> Self {
        let fast = crate::url_flag("hockeyTest=fast");
        let limit = if fast {
            20.
        } else {
            atv_core::hockey::DEFAULT_TIME_LIMIT
        };
        let arena = Arena::COONCE;
        Self {
            game: Game::new(arena, vec![0], limit),
            puck: Puck::held(&arena),
            steps: 0,
            placed: false,
            finished: false,
            log: Vec::new(),
            sounds: Vec::new(),
            hats: Vec::new(),
            fast,
            tee: tee_goals(),
            me: PLAYER,
            roster: Vec::new(),
            serial: 0,
            phase: 0,
            sent_step: 0,
            split: false,
        }
    }

    /// Split screen: player 1 skates for Red and player 2 for Blue (the
    /// rooms' alternating teams; the original Hockey Team Select page was
    /// not traced, so this is a port choice).
    pub fn new_split() -> Self {
        let mut session = Self::new();
        session.game = Game::new(session.game.arena, vec![0, 1], session.game.time_limit);
        session.split = true;
        session
    }

    fn face_off(&mut self, ride: &mut Ride, second: Option<&mut Ride>) {
        // The pause's end (`00336200`), setup and Restart Game stop the
        // hat-trick group before the riders line up.
        self.hats.push(false);
        let (origin, yaw) = self.game.face_off(self.me);
        if let Err(error) = ride.place_beside(origin, yaw, [0.; 3]) {
            crate::status(&format!("Hockey face-off placement failed: {error}"));
        }
        // Split screen: player 2 lines up at rider slot 1's spot.
        if self.split
            && let Some(ride2) = second
        {
            let (origin, yaw) = self.game.face_off(1);
            if let Err(error) = ride2.place_beside(origin, yaw, [0.; 3]) {
                crate::status(&format!("Player 2 face-off placement failed: {error}"));
            }
        }
        self.puck = Puck::held(&self.game.arena);
    }

    /// Pause → Restart (`003377f0`).
    pub fn restart(&mut self, ride: &mut Ride, second: Option<&mut Ride>) {
        self.game.restart();
        self.finished = false;
        self.face_off(ride, second);
        self.steps = ride.steps();
    }

    /// Test hook: slides the puck into goal 1 off the human's stick.
    fn tee_up(&mut self) {
        let goal = self.game.arena.goals[0];
        self.puck.position = [goal[0], self.game.arena.ice + 0.2, goal[2] - 14.];
        self.puck.velocity = [0., 0., 25.];
        self.game.touch(self.me);
    }

    /// Advances the game with the ride's clock. True once, when the game
    /// ends.
    pub fn update(
        &mut self,
        ride: &mut Ride,
        net: Option<&mut crate::multiplayer::Net>,
        mut second: Option<&mut Ride>,
    ) -> bool {
        let mut net = net;
        if let Some(net) = net.as_deref_mut() {
            let (me, roster) = net.roster();
            let me = me.and_then(|id| roster.iter().position(|r| *r == id));
            if let Some(me) = me {
                if !net.is_host() {
                    return self.follow(ride, net, me);
                }
                // The host runs the game for the room's roster: alternate
                // teams in peer order (the original Team Select is not
                // reproduced). A changed roster starts a new game.
                if roster != self.roster {
                    let teams = (0..roster.len()).map(|i| i % 2).collect();
                    self.game = Game::new(self.game.arena, teams, self.game.time_limit);
                    self.roster = roster;
                    self.me = me;
                    self.placed = false;
                    self.finished = false;
                    self.tee = tee_goals();
                }
            }
        }
        if !self.placed {
            self.placed = true;
            self.face_off(ride, second.as_deref_mut());
            self.steps = ride.steps();
        }
        let steps = ride.steps();
        let n = steps.saturating_sub(self.steps);
        self.steps = steps;
        if n == 0 || self.finished {
            return false;
        }
        // A room's game waits at the face-off for a second rider.
        if self.roster.len() == 1 {
            self.game.state = State::FaceOff {
                countdown: atv_core::hockey::FACE_OFF_SECONDS,
            };
            if let Some(net) = net {
                self.broadcast(net, steps);
            }
            return false;
        }
        let dt = atv_game::ride::STEP;
        let mut ended = false;
        let frame = ride.frame();
        let own = (frame.origin, frame.linear_velocity);
        // Every rider in slot order: the human's own ATV, the others' latest
        // relayed poses (out of reach while unknown).
        let riders: Vec<([f32; 3], [f32; 3])> = if self.roster.is_empty() {
            // Split screen: player 2's ATV is rider slot 1.
            let mut riders = vec![own];
            if self.split
                && let Some(ride2) = second.as_deref()
            {
                let f = ride2.frame();
                riders.push((f.origin, f.linear_velocity));
            }
            riders
        } else {
            let (me, _) = net.as_deref().map(|n| n.roster()).unwrap_or_default();
            self.roster
                .iter()
                .map(|id| {
                    if Some(*id) == me {
                        own
                    } else {
                        net.as_deref()
                            .and_then(|n| n.rider_motion(*id))
                            .unwrap_or(([0., -1000., 0.], [0.; 3]))
                    }
                })
                .collect()
        };
        for _ in 0..n {
            let mut touched = false;
            match self.game.state {
                State::FaceOff { .. } => self.puck = Puck::held(&self.game.arena),
                _ => {
                    // A wrecked rider's ATV still pushes the puck; the
                    // detached rider does not (authored).
                    let touches = self.puck.step(&self.game.arena, &riders, dt);
                    touched = !touches.is_empty();
                    for rider in touches {
                        self.game.touch(rider);
                    }
                }
            }
            let mut events = self.game.tick(dt, self.puck.speed(), touched);
            if self.game.live() && self.game.arena.goal_at(self.puck.position).is_some() {
                events.extend(
                    self.game
                        .goal(self.puck.position, crate::random::unit() < 0.5),
                );
            }
            for event in events {
                if let Event::Goal {
                    hat_trick: true, ..
                } = event
                {
                    self.hats.push(true);
                }
                match event {
                    Event::FaceOff => self.face_off(ride, second.as_deref_mut()),
                    Event::Drop => {
                        self.puck = Puck::drop(&self.game.arena, || crate::random::unit() as f32);
                        if self.tee > 0 {
                            self.tee -= 1;
                            self.tee_up();
                        }
                    }
                    Event::Over => {
                        self.finished = true;
                        ended = true;
                    }
                    other => {
                        if let Some(name) = match other {
                            Event::Siren => Some(SIREN),
                            Event::GameOverSound => Some(GAME_OVER),
                            Event::PuckHit => Some(PUCK[crate::random::below(2)]),
                            Event::Crowd(n) => CROWD.get(usize::from(n).wrapping_sub(1)).copied(),
                            _ => None,
                        } {
                            self.sounds.push(name);
                        }
                        if self.log.len() == 32 {
                            self.log.remove(0);
                        }
                        self.log.push(format!("{other:?}"));
                    }
                }
            }
        }
        if let Some(net) = net
            && !self.roster.is_empty()
        {
            self.broadcast(net, steps);
        }
        ended
    }

    /// Host: the state for the room at most every third step (20 Hz) and
    /// on every phase change.
    fn broadcast(&mut self, net: &mut crate::multiplayer::Net, steps: u64) {
        let (phase, timer) = phase_code(self.game.state);
        let changed = phase != self.phase;
        if changed {
            self.phase = phase;
            self.serial = self.serial.wrapping_add(1);
        }
        if !changed && steps < self.sent_step + 3 {
            return;
        }
        self.sent_step = steps;
        let (banner, banner_time) = banner_code(self.game.banner);
        let [red, blue] = self.game.scores;
        net.send_game(&atv_game::multiplayer::GameState {
            game: 1,
            serial: self.serial,
            phase,
            timer,
            clock: self.game.clock,
            scores: [red, blue, 0, 0],
            banner,
            banner_time,
            holder: atv_game::multiplayer::NO_PEER,
            ball: self.puck.position,
            ball_velocity: self.puck.velocity,
            ball_yaw: self.puck.yaw,
        });
    }

    /// Guest: mirror the host's game; face off at its own spot.
    fn follow(&mut self, ride: &mut Ride, net: &mut crate::multiplayer::Net, me: usize) -> bool {
        let (_, roster) = net.roster();
        if roster != self.roster {
            let teams = (0..roster.len()).map(|i| i % 2).collect();
            self.game = Game::new(self.game.arena, teams, self.game.time_limit);
            self.roster = roster;
            self.me = me;
            self.serial = u32::MAX;
        }
        self.steps = ride.steps();
        let Some(g) = net.game_state().cloned() else {
            return false;
        };
        let state = phase_state(g.phase, g.timer);
        let fresh = g.serial != self.serial;
        self.serial = g.serial;
        let before = self.game.scores;
        self.game.state = state;
        self.game.clock = g.clock;
        self.game.scores = [g.scores[0], g.scores[1]];
        self.game.banner = banner_of(g.banner, g.banner_time);
        self.puck.position = g.ball;
        self.puck.velocity = g.ball_velocity;
        self.puck.yaw = g.ball_yaw;
        if !fresh {
            return false;
        }
        match g.phase {
            0 => {
                self.hats.push(false);
                let (origin, yaw) = self.game.face_off(self.me);
                if let Err(error) = ride.place_beside(origin, yaw, [0.; 3]) {
                    crate::status(&format!("Hockey face-off placement failed: {error}"));
                }
                self.finished = false;
            }
            3 if self.game.scores != before => {
                if g.banner == 3 {
                    self.hats.push(true);
                }
                self.sounds.push(SIREN);
                self.sounds
                    .push(if g.banner == 3 { CROWD[2] } else { CROWD[0] });
            }
            3 if g.banner == 5 => {
                self.sounds.push(GAME_OVER);
                self.sounds.push(CROWD[3]);
            }
            4 if !self.finished => {
                self.finished = true;
                return true;
            }
            _ => {}
        }
        false
    }

    /// The puck model's world matrix (yaw spin about +Y).
    pub fn puck_matrix(&self) -> [f32; 16] {
        let (s, c) = self.puck.yaw.sin_cos_d();
        let [x, y, z] = self.puck.position;
        [c, 0., -s, 0., 0., 1., 0., 0., s, 0., c, 0., x, y, z, 1.]
    }

    /// The one-shots to play this frame.
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// Hat-trick group starts/stops to apply to the arena's emitters.
    pub fn take_hats(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.hats)
    }

    pub fn held(&self) -> bool {
        self.game.held()
    }

    fn clock(&self) -> String {
        let s = self.game.clock.ceil() as u32;
        format!("{}:{:02}", s / 60, s % 60)
    }

    /// The HUD panel (the Training panel's layout).
    pub fn view(&self, origin: [f32; 3]) -> atv_ui::hud::training::View {
        self.view_for(origin, self.me)
    }

    /// A local player's panel (split screen: player 2 is rider 1).
    pub fn view_for(&self, origin: [f32; 3], me: usize) -> atv_ui::hud::training::View {
        let g = &self.game;
        let [red, blue] = g.scores;
        let banner = g.banner.map(|(b, _)| b.text()).or(match g.state {
            State::FaceOff { .. } if self.roster.len() == 1 => {
                Some("Waiting for riders".to_string())
            }
            State::FaceOff { countdown } => Some(format!("Face-off {}", countdown.ceil() as u32)),
            _ => None,
        });
        let p = self.puck.position;
        atv_ui::hud::training::View {
            title: format!("Red {red} – {blue} Blue   {}", self.clock()),
            caption: Some({
                let team = atv_core::hockey::TEAMS[g.teams.get(me).copied().unwrap_or(0)];
                match g.state {
                    State::SuddenDeath => {
                        format!(
                            "{} {team}. Sudden death: the next goal wins.",
                            you(self.split, me)
                        )
                    }
                    _ => format!(
                        "{} {team}. Push the puck into the far goal.",
                        you(self.split, me)
                    ),
                }
            }),
            banner,
            distance: None,
            hint: Some(format!(
                "PUCK {:.0} FT",
                (p[0] - origin[0]).hypot_d(p[2] - origin[2])
            )),
        }
    }

    /// The puck's screen tag and map marker.
    pub fn markers(&self) -> Vec<atv_ui::hud::peers::Peer> {
        let [x, y, z] = self.puck.position;
        vec![atv_ui::hud::peers::Peer {
            slot: 1,
            name: "Puck".into(),
            // screen_marker lifts tags 7 ft.
            position: [x, y - 5., z],
            stale: false,
            pill: None,
            map_only: false,
        }]
    }

    /// Each local player's goals (split screen lists both).
    fn goal_line(&self) -> String {
        let goals = |i: usize| self.game.goals.get(i).copied().unwrap_or(0);
        if self.split {
            format!(
                "{} (Red) goals: {}, {} (Blue) goals: {}",
                crate::split::PLAYER_1,
                goals(0),
                crate::split::PLAYER_2,
                goals(1)
            )
        } else {
            format!("Your goals: {}", goals(self.me))
        }
    }

    /// The HockeyOver page's lines: the score, the winner and the goals.
    pub fn summary(&self) -> String {
        let [red, blue] = self.game.scores;
        let outcome = match self.game.winner() {
            Some(0) => "Red wins!",
            Some(_) => "Blue wins!",
            None => "Tied.",
        };
        format!("Red {red} – {blue} Blue. {outcome} {}.", self.goal_line())
    }

    /// The HockeyOver result line.
    pub fn result(&self) -> String {
        let [red, blue] = self.game.scores;
        let outcome = match self.game.winner() {
            Some(0) => "Red wins!",
            Some(_) => "Blue wins!",
            None => "Tied.",
        };
        format!(
            "Hockey: Red {red} – {blue} Blue. {outcome} {}.",
            self.goal_line()
        )
    }

    /// Diagnostics for tools/test_engine_hockey_tag.mjs.
    pub fn state(&self, origin: [f32; 3]) -> serde_json::Value {
        let g = &self.game;
        serde_json::json!({
            "origin": origin,
            "state": format!("{:?}", g.state),
            "scores": g.scores,
            "goals": g.goals,
            "clock": g.clock,
            "banner": g.banner.map(|(b, _)| b.text()),
            "puck": self.puck.position,
            "puckSpeed": self.puck.speed(),
            "held": g.held(),
            "finished": self.finished,
            "fast": self.fast,
            "log": self.log,
            "result": self.result(),
            "me": self.me,
            "roster": self.roster,
            "serial": self.serial,
            "teams": g.teams,
            "split": self.split,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_state_codes_round_trip() {
        for state in [
            State::FaceOff { countdown: 1.5 },
            State::Play,
            State::SuddenDeath,
            State::Pause { remaining: 3.25 },
            State::Over,
        ] {
            let (code, timer) = phase_code(state);
            assert_eq!(phase_state(code, timer), state);
        }
        for banner in [
            Banner::Goal { team: 0 },
            Banner::Goal { team: 1 },
            Banner::HatTrick,
            Banner::SuddenDeath,
            Banner::GameOver,
        ] {
            let (code, t) = banner_code(Some((banner, 2.)));
            assert_eq!(banner_of(code, t), Some((banner, 2.)));
        }
        assert_eq!(banner_of(0, 0.), None);
    }

    #[test]
    fn arena_loads_hockey_samples_only() {
        assert_eq!(samples("game01").len(), 8);
        assert!(samples("env01").is_empty());
    }
}
