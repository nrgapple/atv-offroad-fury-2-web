//! Hockey on Coonce Arena (research/GAMES.md "Hockey"): samples the ride
//! for `atv_core::hockey`, moves the puck, places the rider at each
//! face-off and holds it there until the drop.
//!
//! Temporary entry (Main Menu → Hockey) until the shared Games menu lands;
//! the original is split-screen only with no CPU riders, so a single human
//! plays Red against an empty Blue net (an authored practice adaptation).
use atv_core::hockey::{Arena, Event, Game, Puck, State};
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
    /// `hockeyGoal=1`: the first drop tees the puck up for the human (tests).
    tee: bool,
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
            fast,
            tee: crate::url_flag("hockeyGoal=1"),
        }
    }

    fn face_off(&mut self, ride: &mut Ride) {
        let (origin, yaw) = self.game.face_off(PLAYER);
        if let Err(error) = ride.place_beside(origin, yaw, [0.; 3]) {
            crate::status(&format!("Hockey face-off placement failed: {error}"));
        }
        self.puck = Puck::held(&self.game.arena);
    }

    /// Pause → Restart (`003377f0`).
    pub fn restart(&mut self, ride: &mut Ride) {
        self.game.restart();
        self.finished = false;
        self.face_off(ride);
        self.steps = ride.steps();
    }

    /// Test hook: slides the puck into goal 1 off the human's stick.
    fn tee_up(&mut self) {
        let goal = self.game.arena.goals[0];
        self.puck.position = [goal[0], self.game.arena.ice + 0.2, goal[2] - 14.];
        self.puck.velocity = [0., 0., 25.];
        self.game.touch(PLAYER);
    }

    /// Advances the game with the ride's clock. True once, when the game
    /// ends.
    pub fn update(&mut self, ride: &mut Ride) -> bool {
        if !self.placed {
            self.placed = true;
            self.face_off(ride);
            self.steps = ride.steps();
        }
        let steps = ride.steps();
        let n = steps.saturating_sub(self.steps);
        self.steps = steps;
        if n == 0 || self.finished {
            return false;
        }
        let dt = atv_game::ride::STEP;
        let mut ended = false;
        let frame = ride.frame();
        let rider = (frame.origin, frame.linear_velocity);
        for _ in 0..n {
            let mut touched = false;
            match self.game.state {
                State::FaceOff { .. } => self.puck = Puck::held(&self.game.arena),
                _ => {
                    // A wrecked rider's ATV still pushes the puck; the
                    // detached rider does not (authored).
                    let riders = [rider];
                    touched = !self.puck.step(&self.game.arena, &riders, dt).is_empty();
                    if touched {
                        self.game.touch(PLAYER);
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
                match event {
                    Event::FaceOff => self.face_off(ride),
                    Event::Drop => {
                        self.puck = Puck::drop(&self.game.arena, || crate::random::unit() as f32);
                        if std::mem::take(&mut self.tee) {
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
        ended
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

    pub fn held(&self) -> bool {
        self.game.held()
    }

    fn clock(&self) -> String {
        let s = self.game.clock.ceil() as u32;
        format!("{}:{:02}", s / 60, s % 60)
    }

    /// The HUD panel (the Training panel's layout).
    pub fn view(&self, origin: [f32; 3]) -> atv_ui::hud::training::View {
        let g = &self.game;
        let [red, blue] = g.scores;
        let banner = g.banner.map(|(b, _)| b.text()).or(match g.state {
            State::FaceOff { countdown } => Some(format!("Face-off {}", countdown.ceil() as u32)),
            _ => None,
        });
        let p = self.puck.position;
        atv_ui::hud::training::View {
            title: format!("Red {red} – {blue} Blue   {}", self.clock()),
            caption: Some(match g.state {
                State::SuddenDeath => "Sudden death: the next goal wins.".into(),
                _ => "Push the puck into the far goal.".into(),
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
        }]
    }

    /// The HockeyOver page's lines: the score, the winner and the goals.
    pub fn summary(&self) -> String {
        let [red, blue] = self.game.scores;
        let outcome = match self.game.winner() {
            Some(0) => "Red wins!",
            Some(_) => "Blue wins!",
            None => "Tied.",
        };
        format!(
            "Red {red} – {blue} Blue. {outcome} Your goals: {}.",
            self.game.goals[PLAYER]
        )
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
            "Hockey: Red {red} – {blue} Blue. {outcome} Your goals: {}.",
            self.game.goals[PLAYER]
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
        })
    }
}
