//! Tag in a multiplayer room (research/GAMES.md "Tag", "Online"): the room
//! host runs `atv_core::tag` and the ball for the roster and broadcasts
//! `GameState`; guests mirror it. The original plays Tag split screen on
//! its `*_TG` courses; the port reloads the room's venue from its TG
//! archive (the mode TOY's start, `tagball.psg`) when a game starts there,
//! and draws the ball as that model (a screen tag on other venues).
//!
//! Split screen (research/SPLIT_SCREEN.md "Slice 4"): the same session runs
//! the rules for two to four local riders with no room (`new_local`): this
//! console is the host, player N rider slot N - 1.
use atv_core::math::Det;
use atv_core::tag::{Ball, Bounds, DropReason, Event, Game, Rider};
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
    /// The own ride's open-chain length last step (a new trick starts the
    /// knock-loose draw, `00244ef0`).
    chain_seen: usize,
    /// The ball is drawn as `tagball.psg` (a TG ride): no screen tag.
    pub model: bool,
    /// Boundary throw-ins so far (diagnostics).
    throw_ins: u32,
    knocks: u32,
    /// Games Prep's minutes this game was set up with.
    minutes: u32,
    /// `tagTest=throw`: the first ball starts past the boundary; a second
    /// after the throw-in it is tossed back to the host (test hook).
    throw_test: Option<f32>,
    /// The course start (`00246500`'s fallback when no ray hits).
    pub start: [f32; 3],
    /// The venue's `CrowdPresent` (world +0x658): no boundary margin.
    pub crowd: bool,
    /// Ground respawns so far (diagnostics: ray throws, start returns).
    respawns: (u32, u32),
    /// Split screen: two to four local players and no room.
    pub local: bool,
    /// Players 2-4's open-chain lengths last step (split screen).
    chain_seen_others: Vec<usize>,
    /// Split screen: the first ball was tossed.
    tossed: bool,
    /// Host: each remote rider's last fresh pose and the step it was seen,
    /// held through short network gaps (`remote_motion`).
    held: Vec<Option<Held>>,
    /// Host: rider steps ridden on a held pose so far (diagnostics).
    holds: u32,
}

/// A remote rider's last fresh position, velocity and step.
type Held = ([f32; 3], [f32; 3], u64);

/// A remote rider missing from the relay for longer than this (seconds)
/// is absent (it cannot touch or carry the ball); a shorter gap is held.
pub const REMOTE_HOLD: f32 = 3.0;

/// The host's view of a remote rider for the rules: its fresh pose, else
/// (a late or throttled peer) its last fresh position at rest for up to
/// [`REMOTE_HOLD`], else absent. A network gap is not a wreck: the
/// original's riders are all local, so a missing pose must not drop the
/// carrier's ball (research/GAMES.md "Online").
fn remote_motion(
    held: &mut Option<Held>,
    fresh: Option<([f32; 3], [f32; 3])>,
    step: u64,
) -> ([f32; 3], [f32; 3], bool) {
    const ABSENT: ([f32; 3], [f32; 3], bool) = ([0., -1000., 0.], [0.; 3], true);
    if let Some((p, v)) = fresh {
        *held = Some((p, v, step));
        return (p, v, false);
    }
    let limit = (REMOTE_HOLD / atv_game::ride::STEP) as u64;
    match *held {
        Some((p, _, seen)) if step.saturating_sub(seen) <= limit => (p, [0.; 3], false),
        _ => ABSENT,
    }
}

/// The game's clock: Games Prep's minutes (`0026b7a0`, 2-20, default 5);
/// `tagTest=fast` a 30 s clock for browser tests.
fn time_limit(minutes: u32) -> f32 {
    if crate::url_flag("tagTest=fast") {
        30.
    } else {
        minutes.clamp(atv_core::games::MIN_MINUTES, atv_core::games::MAX_MINUTES) as f32 * 60.
    }
}

impl TagSession {
    pub fn new(host: bool, minutes: u32) -> Self {
        Self {
            game: Game::new(0, time_limit(minutes)),
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
            chain_seen: 0,
            model: false,
            throw_ins: 0,
            knocks: 0,
            minutes,
            throw_test: None,
            start: [0.; 3],
            respawns: (0, 0),
            crowd: false,
            local: false,
            chain_seen_others: Vec::new(),
            tossed: false,
            held: Vec::new(),
            holds: 0,
        }
    }

    /// Split screen: a two-player game on this console (no room).
    pub fn new_local(minutes: u32, players: usize) -> Self {
        let players = players.clamp(2, 4);
        let mut session = Self::new(true, minutes);
        session.local = true;
        session.roster = (0..players as u8).collect();
        session.names = (1..=players as u8)
            .map(|n| crate::split::player_name(n).to_string())
            .collect();
        session.game = Game::new(players, session.game.time_limit);
        session.chain_seen_others = vec![0; players - 1];
        session
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
            // Split screen names the carrier in both views.
            self.banner = Some(if c == self.me && !self.local {
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
    /// `net`: the room (None in split screen); `others`: split screen's
    /// players 2-4.
    pub fn update(
        &mut self,
        ride: &mut Ride,
        net: Option<&mut crate::multiplayer::Net>,
        others: &mut [&mut Ride],
    ) -> bool {
        let shared: Vec<&Ride> = others.iter().map(|r| &**r).collect();
        let ended = self.advance(ride, net, &shared);
        // `00246310`/`00246000`: the carrier's ATV runs handicapped; each
        // rider owns its ATV, so each applies its own.
        let handicap = |ride: &mut Ride, slot: usize, game: &Game, any: bool| {
            let carrying = !game.over && game.carrier == Some(slot) && any;
            if carrying != (ride.engine_effect() != 0.) {
                ride.set_carrier_handicap(carrying);
            }
        };
        let any = !self.roster.is_empty();
        handicap(ride, self.me, &self.game, any);
        if self.local {
            for (index, ride2) in others.iter_mut().enumerate() {
                handicap(ride2, index + 1, &self.game, any);
            }
        }
        ended
    }

    /// Split screen Restart Game: a new game for both players.
    pub fn restart_local(&mut self) {
        let mut fresh = Self::new_local(self.minutes, self.roster.len());
        fresh.model = self.model;
        fresh.start = self.start;
        fresh.crowd = self.crowd;
        *self = fresh;
    }

    /// The game ended or was left: no handicap remains.
    pub fn release(ride: &mut Ride) {
        ride.set_carrier_handicap(false);
    }

    fn advance(
        &mut self,
        ride: &Ride,
        net: Option<&mut crate::multiplayer::Net>,
        others: &[&Ride],
    ) -> bool {
        let terrain = ride.terrain();
        let bounds = Bounds::from_cells(terrain.width_cells, terrain.depth_cells, terrain.spacing);
        let host = self.local || net.as_deref().is_some_and(|n| n.is_host());
        let fresh = if self.local {
            // The first update (and Restart Game) tosses the first ball.
            let fresh = !self.tossed;
            if fresh {
                self.tossed = true;
                self.finished = false;
                // The clock starts now, not at the ride's first step.
                self.steps = ride.steps();
            }
            fresh
        } else {
            let Some(net) = net.as_deref() else {
                return false;
            };
            let (me, roster) = net.roster();
            let Some(me) = me.and_then(|id| roster.iter().position(|r| *r == id)) else {
                return false;
            };
            self.names = net.names(&roster);
            let changed = roster != self.roster;
            if changed {
                self.roster = roster;
                self.me = me;
                self.game = Game::new(self.roster.len(), self.game.time_limit);
                self.finished = false;
                self.held.clear();
            }
            changed
        };
        if fresh {
            let o = ride.frame().origin;
            self.ball = Ball::toss(o, [0.; 3]);
            if crate::url_flag("tagTest=throw") && host {
                // Past the west margin (`0020aec0`), level with the host.
                self.ball.position = [bounds.cell[0] * 40., o[1] + 20., o[2]];
                self.ball.velocity = [0.; 3];
                self.throw_test = Some(0.);
            }
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
        if !host {
            self.host = false;
            return net.as_deref().is_some_and(|net| self.follow(net));
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
        let motion = |ride: &Ride| {
            let f = ride.frame();
            (
                f.origin,
                f.linear_velocity,
                f.wreck.phase != atv_core::wreck::Phase::Riding,
            )
        };
        self.held.resize(self.roster.len(), None);
        let riders: Vec<([f32; 3], [f32; 3], bool)> = self
            .roster
            .iter()
            .enumerate()
            .map(|(i, id)| {
                if i == self.me {
                    own
                } else if self.local {
                    // Split screen: that player's own ride.
                    others
                        .get(i.wrapping_sub(1))
                        .map_or(([0., -1000., 0.], [0.; 3], true), |r| motion(r))
                } else {
                    let fresh = net.as_deref().and_then(|net| net.rider_motion(*id));
                    if fresh.is_none() && self.held[i].is_some() {
                        self.holds += 1;
                    }
                    remote_motion(&mut self.held[i], fresh, steps)
                }
            })
            .collect();
        let mut ended = false;
        let reach = self.reach;
        // A trick begun since the last update (the own rider's; remote
        // riders' tricks are not relayed, research/GAMES.md).
        let chain = ride.chain_len();
        let own_trick = chain > self.chain_seen;
        self.chain_seen = chain;
        // Split screen: the other players' tricks are local too.
        let mut other_tricks = vec![false; others.len()];
        if self.local {
            self.chain_seen_others.resize(others.len(), 0);
            for (index, r) in others.iter().enumerate() {
                let chain = r.chain_len();
                other_tricks[index] = chain > self.chain_seen_others[index];
                self.chain_seen_others[index] = chain;
            }
        }
        if let Some(t) = &mut self.throw_test
            && self.throw_ins > 0
        {
            *t += n as f32 * dt;
            if *t > 1. {
                self.throw_test = None;
                self.ball = Ball::toss(ride.frame().origin, [0.; 3]);
            }
        }
        for step in 0..n {
            // `00244ef0` → `0020aec0`/`00246ec0`: a ball past the boundary
            // margin is thrown toward the map centre.
            if !self.game.attached
                && bounds.outside(
                    self.ball.position,
                    atv_core::tag::boundary_margin(self.crowd),
                )
            {
                self.ball = Ball::throw_in(self.ball.position, bounds.centre());
                self.throw_ins += 1;
                if self.throw_ins == 1 || self.throw_ins.is_multiple_of(30) {
                    self.log(format!("ThrowIn {}", self.throw_ins));
                }
            }
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
                .enumerate()
                .map(|(i, (p, _, wrecked))| Rider {
                    wrecked: *wrecked,
                    stunt_started: step == 0
                        && ((i == self.me && own_trick)
                            || (self.local
                                && i > 0
                                && other_tricks.get(i - 1).copied().unwrap_or(false))),
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
                        // `00246500`: eighteen level rays from a random
                        // heading against the world (the port casts the
                        // heightfield); the nearest hit sets the throw,
                        // with none the ball restarts at the course start.
                        let from = self.ball.position;
                        let heading = self.game.draw() * 360.;
                        let cast = |a: [f32; 3], b: [f32; 3]| terrain_ray(ground, a, b);
                        self.ball = match atv_core::tag::respawn_search(from, heading, cast) {
                            Some(hit) => {
                                let draws = [self.game.draw(), self.game.draw(), self.game.draw()];
                                self.respawns.0 += 1;
                                Ball::respawn_throw(from, hit, draws)
                            }
                            None => {
                                self.respawns.1 += 1;
                                Ball::respawn_start(
                                    self.start,
                                    ground(self.start[0], self.start[2]),
                                )
                            }
                        };
                        self.log(format!("Respawn {} {}", self.respawns.0, self.respawns.1));
                    }
                    Event::KnockLoose { rider } => {
                        let (p, v, _) = riders[rider];
                        self.ball = Ball::knock(p, v);
                        self.knocks += 1;
                        self.log(format!("KnockLoose {rider}"));
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
        let Some(net) = net.filter(|_| !self.local) else {
            return ended;
        };
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
        self.view_for(self.me)
    }

    /// A local player's panel (split screen: player N is slot N - 1).
    pub fn view_for(&self, me: usize) -> atv_ui::hud::training::View {
        let mine = self.game.records.get(me).map_or(0, |r| r.score);
        atv_ui::hud::training::View {
            title: format!("Tag   {}   You {mine}", self.clock()),
            caption: Some(match self.game.carrier {
                Some(c) if c == me => "You have the ball: keep it away!".into(),
                Some(c) => format!("{} has the ball: tag them!", self.name(c)),
                None => "Grab the ball!".into(),
            }),
            banner: self.banner.as_ref().map(|(t, _)| t.clone()),
            distance: None,
            hint: None,
        }
    }

    /// The ball's minimap marker; a screen tag too unless the
    /// `tagball.psg` model is drawn.
    pub fn markers(&self) -> Vec<atv_ui::hud::peers::Peer> {
        let [x, y, z] = self.ball.position;
        vec![atv_ui::hud::peers::Peer {
            slot: 2,
            name: "Ball".into(),
            position: [x, y - 6., z],
            stale: false,
            pill: None,
            map_only: self.model,
        }]
    }

    /// The ball model's world matrix (`tagball.psg`; the ball's centre).
    pub fn ball_matrix(&self) -> [f32; 16] {
        let [x, y, z] = self.ball.position;
        [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, y, z, 1.]
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
            "timeLimit": self.game.time_limit,
            "minutes": self.minutes,
            "ballVelocity": self.ball.velocity,
            "model": self.model,
            "throwIns": self.throw_ins,
            "knocks": self.knocks,
            "respawns": [self.respawns.0, self.respawns.1],
            "attached": self.game.attached,
            "local": self.local,
            "holds": self.holds,
        })
    }
}

/// The first point along the level segment `a`-`b` at or under the
/// heightfield (1-unit steps), the port's stand-in for the world collision
/// cast `00169b60` (static objects are not tested).
fn terrain_ray(ground: impl Fn(f32, f32) -> f32, a: [f32; 3], b: [f32; 3]) -> Option<[f32; 3]> {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (d[0] * d[0] + d[2] * d[2]).sqrt();
    let steps = len.ceil().max(1.) as usize;
    (1..=steps).find_map(|k| {
        let t = k as f32 / steps as f32;
        let p = [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t];
        (ground(p[0], p[2]) >= p[1]).then_some(p)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_network_gap_is_held_not_a_wreck() {
        let step = |s: f32| (s / atv_game::ride::STEP) as u64;
        let mut held = None;
        // Never seen: absent.
        assert!(remote_motion(&mut held, None, 0).2);
        let p = [10., 2., 30.];
        let fresh = remote_motion(&mut held, Some((p, [5., 0., 0.])), 100);
        assert_eq!(fresh, (p, [5., 0., 0.], false));
        // A throttled guest's poses stop for a second: it still rides
        // (and keeps carrying) where it was last seen, at rest.
        assert_eq!(
            remote_motion(&mut held, None, 100 + step(1.)),
            (p, [0.; 3], false)
        );
        // Long gone: absent, so a carrier drops the ball.
        assert!(remote_motion(&mut held, None, 100 + step(REMOTE_HOLD) + 2).2);
        // Back again.
        assert!(!remote_motion(&mut held, Some((p, [0.; 3])), 900).2);
    }

    #[test]
    fn a_held_carrier_keeps_scoring() {
        // The guest carries; its poses go stale for 1.5 s. The rules see a
        // riding carrier, so its possession (and score) continues.
        let dt = atv_game::ride::STEP;
        let mut game = Game::new(2, 30.);
        let mut held = None;
        let p = [0., 0., 0.];
        remote_motion(&mut held, Some((p, [0.; 3])), 0);
        let mut riders = [Rider::default(), Rider {
            touches_ball: true,
            ..Rider::default()
        }];
        game.tick(dt, &riders, false);
        assert_eq!(game.carrier, Some(1));
        riders[1].touches_ball = false;
        for step in 1..=(1.5 / dt) as u64 {
            riders[1].wrecked = remote_motion(&mut held, None, step).2;
            game.tick(dt, &riders, false);
        }
        assert_eq!(game.carrier, Some(1));
        assert!(game.records[1].score >= atv_core::tag::POINTS_PER_SECOND);
    }
}
