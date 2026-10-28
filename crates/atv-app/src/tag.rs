//! Tag in a multiplayer room (research/GAMES.md "Tag", "Online"): the room
//! host runs `atv_core::tag` and the ball for the roster and broadcasts
//! `GameState`; guests mirror it. The original plays Tag split screen on
//! its `*_TG` courses; the port plays it on the room's venue (an authored
//! adaptation: the TG archives and `tagball.psg` are not hosted, so the
//! ball shows as a screen tag).
use atv_core::math::Det;
use atv_core::tag::{Ball, DropReason, Event, Game, Rider};
use atv_game::multiplayer::{GameState, NO_PEER};
use atv_game::ride::Ride;

/// Two ATVs closer than this (centre to centre) touch (authored).
pub const CONTACT: f32 = 7.0;
/// A rider within this of the ball touches it (authored).
pub const BALL_REACH: f32 = 5.0;
const BEEP: &str = "tagbeep.vag";

pub struct TagSession {
    pub game: Game,
    pub ball: Ball,
    roster: Vec<u8>,
    me: usize,
    steps: u64,
    serial: u32,
    sent_step: u64,
    carrier_seen: Option<usize>,
    pub finished: bool,
    /// A banner and its seconds left ("You are it!", "%s has the ball!").
    banner: Option<(String, f32)>,
    sounds: Vec<&'static str>,
    names: Vec<String>,
    log: Vec<String>,
    host: bool,
    /// The carrier slot last broadcast.
    sent_carrier: u8,
    /// Tag contact distance (`tagReach=N` widens it for tests).
    reach: f32,
}

/// `tagTest=fast`: a 30 s clock for browser tests.
fn time_limit() -> f32 {
    if crate::url_flag("tagTest=fast") {
        30.
    } else {
        atv_core::hockey::DEFAULT_TIME_LIMIT
    }
}

impl TagSession {
    pub fn new(host: bool) -> Self {
        Self {
            game: Game::new(0, time_limit()),
            ball: Ball {
                position: [0.; 3],
                velocity: [0.; 3],
            },
            roster: Vec::new(),
            me: 0,
            steps: 0,
            serial: 0,
            sent_step: 0,
            carrier_seen: None,
            finished: false,
            banner: None,
            sounds: Vec::new(),
            names: Vec::new(),
            log: Vec::new(),
            host,
            sent_carrier: NO_PEER,
            reach: (10..=200)
                .step_by(10)
                .find(|r| crate::url_flag(&format!("tagReach={r}")))
                .map_or(CONTACT, |r| r as f32),
        }
    }

    fn name(&self, slot: usize) -> String {
        self.names
            .get(slot)
            .cloned()
            .unwrap_or_else(|| format!("Rider {}", slot + 1))
    }

    fn announce(&mut self, carrier: Option<usize>) {
        if carrier == self.carrier_seen {
            return;
        }
        self.carrier_seen = carrier;
        if let Some(c) = carrier {
            // `00246310`: "You are it!" (1.6 s) / "%s has the ball!" (2 s).
            self.banner = Some(if c == self.me {
                ("You are it!".into(), atv_core::tag::YOU_ARE_IT_BANNER)
            } else {
                (
                    format!("{} has the ball!", self.name(c)),
                    atv_core::tag::HAS_BALL_BANNER,
                )
            });
            self.sounds.push(BEEP);
        }
    }

    /// Advances the game; true once, when it ends.
    pub fn update(&mut self, ride: &Ride, net: &mut crate::multiplayer::Net) -> bool {
        let terrain = ride.terrain();
        let (me, roster) = net.roster();
        let Some(me) = me.and_then(|id| roster.iter().position(|r| *r == id)) else {
            return false;
        };
        self.names = net.names(&roster);
        if roster != self.roster {
            self.roster = roster;
            self.me = me;
            self.game = Game::new(self.roster.len(), self.game.time_limit);
            self.finished = false;
            let o = ride.frame().origin;
            self.ball = Ball::toss(o, [0.; 3]);
        }
        let steps = ride.steps();
        let n = steps.saturating_sub(self.steps);
        self.steps = steps;
        let dt = atv_game::ride::STEP;
        if let Some((_, t)) = &mut self.banner {
            *t -= dt * n as f32;
            if *t <= 0. {
                self.banner = None;
            }
        }
        if !net.is_host() {
            self.host = false;
            return self.follow(net);
        }
        self.host = true;
        if n == 0 || self.finished {
            return false;
        }
        let ground = |x: f32, z: f32| {
            terrain
                .contact(x, z)
                .ok()
                .flatten()
                .map_or(0., |c| c.height)
        };
        let frame = ride.frame();
        let own = (
            frame.origin,
            frame.linear_velocity,
            frame.wreck.phase != atv_core::wreck::Phase::Riding,
        );
        let riders: Vec<([f32; 3], [f32; 3], bool)> = self
            .roster
            .iter()
            .enumerate()
            .map(|(i, id)| {
                if i == self.me {
                    own
                } else {
                    net.rider_motion(*id)
                        .map_or(([0., -1000., 0.], [0.; 3], true), |(p, v)| (p, v, false))
                }
            })
            .collect();
        let mut ended = false;
        let reach = self.reach;
        for _ in 0..n {
            let carrier = self.game.carrier;
            let caught = self.ball.step(
                dt,
                ground,
                carrier.map(|c| (riders[c].0, riders[c].1)),
                self.game.attached,
            );
            if caught {
                self.game.catch();
            }
            let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot_d(a[2] - b[2]);
            let input: Vec<Rider> = riders
                .iter()
                .map(|(p, _, wrecked)| Rider {
                    wrecked: *wrecked,
                    touches_ball: carrier.is_none()
                        && flat(*p, self.ball.position) < BALL_REACH
                        && (p[1] - self.ball.position[1]).abs() < 8.,
                    touches_carrier: carrier.is_some_and(|c| {
                        flat(*p, riders[c].0) < reach && flat(*p, riders[c].0) > 0.
                    }),
                })
                .collect();
            let resting = self.ball.resting(ground);
            for event in self.game.tick(dt, &input, resting) {
                match event {
                    Event::Gain { rider } => {
                        let (p, v, _) = riders[rider];
                        self.ball = Ball::toss(p, v);
                        self.log(format!("Gain {rider}"));
                    }
                    Event::Drop { rider, reason } => {
                        self.log(format!("Drop {rider} {reason:?}"));
                        if reason == DropReason::Wreck || reason == DropReason::Lost {
                            self.banner =
                                Some((format!("{} dropped the ball!", self.name(rider)), 2.));
                        }
                    }
                    Event::Respawn => {
                        let o = riders[self.me].0;
                        self.ball = Ball::throw_in(self.ball.position, o);
                    }
                    Event::Over => {
                        self.finished = true;
                        ended = true;
                    }
                    Event::Points { .. } => {}
                }
            }
        }
        self.announce(self.game.carrier);
        // 20 Hz and on every change of carrier.
        let changed = self.game.carrier.map_or(NO_PEER, |c| c as u8) != self.sent_carrier;
        if changed || ended || steps >= self.sent_step + 3 {
            self.sent_step = steps;
            if changed || ended {
                self.serial = self.serial.wrapping_add(1);
            }
            let mut scores = [0; 4];
            for (s, r) in scores.iter_mut().zip(&self.game.records) {
                *s = r.score;
            }
            net.send_game(&GameState {
                game: 2,
                serial: self.serial,
                phase: u8::from(self.game.over),
                timer: 0.,
                clock: self.game.clock,
                scores,
                banner: 0,
                banner_time: 0.,
                holder: self.game.carrier.map_or(NO_PEER, |c| c as u8),
                ball: self.ball.position,
                ball_velocity: self.ball.velocity,
                ball_yaw: 0.,
                race: Default::default(),
            });
            self.sent_carrier = self.game.carrier.map_or(NO_PEER, |c| c as u8);
        }
        ended
    }

    fn log(&mut self, line: String) {
        if self.log.len() == 32 {
            self.log.remove(0);
        }
        self.log.push(line);
    }

    /// Guest: mirror the host's game.
    fn follow(&mut self, net: &crate::multiplayer::Net) -> bool {
        let Some(g) = net.game_state().filter(|g| g.game == 2).cloned() else {
            return false;
        };
        self.game.clock = g.clock;
        for (r, s) in self.game.records.iter_mut().zip(g.scores) {
            r.score = s;
        }
        let carrier = (g.holder != NO_PEER).then_some(usize::from(g.holder));
        self.game.carrier = carrier.filter(|c| *c < self.roster.len());
        self.ball.position = g.ball;
        self.ball.velocity = g.ball_velocity;
        self.announce(self.game.carrier);
        if g.phase == 1 && !self.finished {
            self.finished = true;
            self.game.over = true;
            return true;
        }
        false
    }

    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    fn clock(&self) -> String {
        let s = self.game.clock.ceil() as u32;
        format!("{}:{:02}", s / 60, s % 60)
    }

    pub fn view(&self) -> atv_ui::hud::training::View {
        let mine = self.game.records.get(self.me).map_or(0, |r| r.score);
        atv_ui::hud::training::View {
            title: format!("Tag   {}   You {mine}", self.clock()),
            caption: Some(match self.game.carrier {
                Some(c) if c == self.me => "You have the ball: keep it away!".into(),
                Some(c) => format!("{} has the ball: tag them!", self.name(c)),
                None => "Grab the ball!".into(),
            }),
            banner: self.banner.as_ref().map(|(t, _)| t.clone()),
            distance: None,
            hint: None,
        }
    }

    /// The ball as a screen tag (no `tagball.psg` on the port's venues).
    pub fn markers(&self) -> Vec<atv_ui::hud::peers::Peer> {
        let [x, y, z] = self.ball.position;
        vec![atv_ui::hud::peers::Peer {
            slot: 2,
            name: "Ball".into(),
            position: [x, y - 6., z],
            stale: false,
            pill: None,
        }]
    }

    /// The TagOver page (`00247160`: places by score).
    pub fn summary(&self) -> String {
        self.game
            .standings()
            .iter()
            .enumerate()
            .map(|(place, slot)| {
                format!(
                    "{}. {} {}",
                    place + 1,
                    self.name(*slot),
                    self.game.records[*slot].score
                )
            })
            .collect::<Vec<_>>()
            .join("   ")
    }

    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({
            "host": self.host,
            "me": self.me,
            "roster": self.roster,
            "carrier": self.game.carrier,
            "scores": self.game.records.iter().map(|r| r.score).collect::<Vec<_>>(),
            "clock": self.game.clock,
            "ball": self.ball.position,
            "banner": self.banner.as_ref().map(|(t, _)| t.clone()),
            "finished": self.finished,
            "log": self.log,
            "summary": self.summary(),
        })
    }
}
