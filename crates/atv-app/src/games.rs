//! An original mini-game on the current ride (research/GAMES.md): loads
//! the game archive's mode TOY, places the ATV at the game's start, runs
//! the start countdown and samples the ride for `atv_core::games`.
//! Tag and Hockey plug in through `atv_core::games::rules`.
use atv_core::games::{GameKind, Rider, Session, Start, Toy};
use atv_game::ride::Ride;

/// The start lights' seconds before a game begins (Overlays.pb 321 lights).
pub const COUNTDOWN: f32 = 3.;

/// The 321 LIGHTS step a game's start shows (`002e79a0`, `002e81f0`): one
/// more red lamp each counted second (with its beep), green at GO for a
/// second. The port's 3 s hold starts on the first red lamp, so the
/// original's all-off step is not shown.
fn lamp_step(started: bool, countdown: f32, go: f32) -> Option<u8> {
    if !started {
        Some((4 - countdown.ceil().clamp(1., 3.) as i32) as u8)
    } else if go > 0. {
        Some(4)
    } else {
        None
    }
}

pub struct GameSession {
    pub kind: GameKind,
    pub venue: String,
    /// The course's Tracks.dat name.
    pub course: String,
    pub minutes: u32,
    /// The rules, once the mode TOY loaded.
    pub session: Option<Session>,
    /// Why the game could not load.
    pub error: Option<String>,
    starts: Vec<Start>,
    /// Seconds left on the start countdown.
    pub countdown: f32,
    /// The green lamp's second after GO (`002e81f0` hides it 1 s later).
    go: f32,
    placed: bool,
    steps: u64,
    /// Each player's committed stunt score at the last sample.
    committed: [f32; 4],
    /// Local players: 1, or 2-4 in split screen (player N is rider N - 1,
    /// research/SPLIT_SCREEN.md "Slice 3", "Three and four players").
    pub players: usize,
    /// The results were shown.
    pub reported: bool,
    /// The last results summary (diagnostics).
    summary: Option<String>,
    /// Test tooling: `gameTest=fast` ends the clock after 20 s;
    /// `gameTest=visit` places the ATV on the next standing treasure each
    /// second; `gameTest=hill` sets the ATV on hill 1, then lifts it off
    /// the hill (armed) for a stunt.
    fast: bool,
    visit: bool,
    hill: u8,
    /// `gameTest=hillride`: instead of lifting the ATV, launch it across
    /// the hilltop at 40 ft/s with a 12 ft/s lip (direction N of four, the next on each restart)
    /// so it rides off the hill for a real stunt.
    ride_off: Option<u8>,
    test_clock: f32,
    /// `gameTest=visit` placements so far.
    visits: u32,
    /// A mode TOY without `Start%d` keeps the venue's ride start: player
    /// 1's first pose, so Restart Game returns both players there.
    venue_start: Option<([f32; 3], f32)>,
    /// `gameTest=hill`: a landed chain's points for the next sample (an
    /// ATV placed in the air opens no stunt).
    inject: Option<(usize, f32)>,
    /// `gameTest=hill2`/`hill3`/`hill4`: the hill hook moves that player
    /// (split screen).
    hill_player: usize,
    /// The last finished chain: landed, committed delta, stunt sum.
    last_chain: Option<(bool, f32, f32)>,
    /// One-shot sounds for the ride's mixer (start beeps, pickups).
    sounds: Vec<&'static str>,
    /// Each hill's `kohflag` scene model (`00208840` binds the nearest).
    pub flags: Option<Vec<Option<usize>>>,
}

/// The player colours at +0x210 (`00220220`): red, blue, green, yellow.
pub const OWNER_COLORS: [[f32; 3]; 4] = [[1., 0., 0.], [0., 0., 1.], [0., 1., 0.], [1., 1., 0.]];

impl GameSession {
    pub fn new(
        kind: GameKind,
        venue: String,
        course: String,
        minutes: u32,
        players: usize,
    ) -> Self {
        Self {
            kind,
            venue,
            course,
            minutes,
            session: None,
            error: None,
            starts: Vec::new(),
            countdown: COUNTDOWN,
            go: 0.,
            placed: false,
            steps: 0,
            committed: [0.; 4],
            players: players.clamp(1, 4),
            reported: false,
            summary: None,
            fast: crate::url_flag("gameTest=fast"),
            visit: crate::url_flag("gameTest=visit"),
            hill: u8::from(
                crate::url_flag("gameTest=hill")
                    || crate::url_flag("gameTest=hill2")
                    || crate::url_flag("gameTest=hill3")
                    || crate::url_flag("gameTest=hill4")
                    || crate::url_flag("gameTest=hillride"),
            ),
            ride_off: crate::url_flag("gameTest=hillride").then_some(0),
            test_clock: 0.,
            visits: 0,
            venue_start: None,
            inject: None,
            hill_player: (2..=4)
                .find(|n| crate::url_flag(&format!("gameTest=hill{n}")))
                .map_or(0, |n| n - 1),
            sounds: Vec::new(),
            flags: None,
            last_chain: None,
        }
    }

    /// The mode TOY text arrived (or why not).
    pub fn load(&mut self, toy: Result<String, String>) {
        match toy.and_then(|text| {
            let rules = atv_core::games::rules_for(self.kind, &text, self.players)
                .map_err(|e| e.to_string())?;
            Ok((rules, atv_core::games::parse_starts(&Toy::parse(&text))))
        }) {
            Ok((mut rules, starts)) => {
                // The original draws the next treasure group with `rand`.
                rules.seed(crate::fresh_seed() as u32);
                rules.restart();
                let mut session = Session::new(rules, self.minutes, self.players);
                if self.fast {
                    session.time_left = session.time_left.min(20.);
                }
                self.session = Some(session);
                self.starts = starts;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn place(&mut self, ride: &mut Ride, others: &mut [&mut Ride]) {
        // Ten courses' mode TOYs have no starts (every other one has
        // `Start1` and `Start2`): the venue's ride start, remembered.
        if self.starts.is_empty() && self.venue_start.is_none() {
            let f = ride.frame();
            let [w, x, y, z] = f.orientation;
            let yaw = atv_core::math::Det::atan2_d(2. * (w * y + x * z), 1. - 2. * (y * y + x * x));
            self.venue_start = Some((f.origin, yaw));
        }
        let first = self
            .starts
            .first()
            .map(|s| (s.position, s.yaw()))
            .or(self.venue_start);
        if let Some((at, yaw)) = first
            && let Err(error) = ride.place_beside(at, yaw, [0.; 3])
        {
            crate::status(&format!("Game placement failed: {error}"));
        }
        ride.take_chains();
        self.steps = ride.steps();
        self.committed[0] = ride.committed_points();
        // Player N takes the TOY's Nth start (`Start%d` is one per player
        // slot); without one it rides beside player 1's start.
        let origin = ride.frame().origin;
        for (index, ride2) in others.iter_mut().enumerate().take(3) {
            let n = index as u8 + 2;
            let (at, yaw) = match (self.starts.get(index + 1), first) {
                (Some(s), _) => (s.position, s.yaw()),
                (None, Some((at, yaw))) => crate::split::freeride_start_for(at, yaw, n),
                (None, None) => crate::split::freeride_start_for(origin, 0., n),
            };
            if let Err(error) = ride2.place_beside(at, yaw, [0.; 3]) {
                crate::status(&format!("Player {n} game placement failed: {error}"));
            }
            ride2.take_chains();
            ride2.take_free_records();
            self.committed[index + 1] = ride2.committed_points();
        }
        self.countdown = COUNTDOWN;
        self.go = 0.;
    }

    /// Restart Game (`00220d10`, `002d1450`): the rules and clock start
    /// over at the start position.
    pub fn restart(&mut self, ride: &mut Ride, others: &mut [&mut Ride]) {
        if let Some(session) = &mut self.session {
            session.restart();
            if self.fast {
                session.time_left = session.time_left.min(20.);
            }
        }
        self.reported = false;
        if self.hill != 0 {
            self.hill = 1;
        }
        if let Some(n) = &mut self.ride_off {
            *n = (*n + 1) % 4;
        }
        self.place(ride, others);
    }

    /// The ATV is held during the countdown and after the game ends.
    pub fn locked(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| !s.started || s.end.is_some())
    }

    /// Advances the game with the ride (and the other players' in split
    /// screen).
    pub fn update(&mut self, ride: &mut Ride, others: &mut [&mut Ride]) {
        if self.session.is_none() {
            return;
        }
        if !self.placed {
            self.placed = true;
            self.place(ride, others);
        }
        let steps = ride.steps();
        let dt = steps.saturating_sub(self.steps) as f32 * atv_game::ride::STEP;
        self.steps = steps;
        if dt <= 0. {
            return;
        }
        let Some(session) = &mut self.session else {
            return;
        };
        if session.started {
            self.go = (self.go - dt).max(0.);
        } else {
            // The race start's 3-2-1 beeps and go (`002e79a0`).
            if self.countdown >= COUNTDOWN {
                self.sounds.push("beep01.vag");
            }
            let before = self.countdown.ceil();
            self.countdown -= dt;
            if self.countdown <= 0. {
                self.countdown = 0.;
                session.begin();
                self.go = 1.;
                self.sounds.push("go.vag");
            } else if self.countdown.ceil() < before {
                self.sounds.push("beep01.vag");
            }
        }
        let collected: i32 = session.rules.scores().iter().sum();
        let inject = |player: usize, inject: &mut Option<(usize, f32)>| {
            inject
                .filter(|(p, _)| *p == player)
                .and_then(|_| inject.take())
                .map(|(_, points)| points)
        };
        let (first, chain) = sample(ride, &mut self.committed[0], inject(0, &mut self.inject));
        if chain.is_some() {
            self.last_chain = chain;
        }
        let mut riders = vec![first];
        let mut stunts = vec![self.committed[0]];
        for (index, ride2) in others
            .iter_mut()
            .enumerate()
            .take(self.players.saturating_sub(1))
        {
            let player = index + 1;
            let (rider, _) = sample(
                ride2,
                &mut self.committed[player],
                inject(player, &mut self.inject),
            );
            riders.push(rider);
            stunts.push(self.committed[player]);
        }
        session.tick(dt, &riders, &stunts);
        // `002d0d70` plays iconhit.vag for each treasure.
        if self.kind == GameKind::TreasureHunt
            && session.rules.scores().iter().sum::<i32>() > collected
        {
            self.sounds.push("iconhit.vag");
        }
        self.test_hooks(ride, others, dt);
    }

    /// The TOY triggers the rules took since the last call (Treasure
    /// Hunt's collected treasures, whose explosions the ride starts).
    pub fn take_triggered(&mut self) -> Vec<String> {
        self.session
            .as_mut()
            .map(|s| s.rules.take_triggered())
            .unwrap_or_default()
    }

    /// The `gameTest` placements (test tooling only).
    fn test_hooks(&mut self, ride: &mut Ride, others: &mut [&mut Ride], dt: f32) {
        let Some(session) = &mut self.session else {
            return;
        };
        if !session.started || session.end.is_some() {
            return;
        }
        self.test_clock += dt;
        let markers = session.rules.markers();
        let place = |ride: &mut Ride, at: [f32; 3]| {
            let _ = ride.place_beside(at, 0., [0.; 3]);
        };
        if self.visit
            && self.test_clock >= 1.
            && let Some(m) = markers.first()
        {
            self.test_clock = 0.;
            // Split screen: the players take turns, so each collects.
            self.visits += 1;
            let turn = self.visits as usize % (others.len() + 1);
            match others.get_mut(turn.wrapping_sub(1)) {
                Some(ride2) if turn > 0 => place(ride2, m.position),
                _ => place(ride, m.position),
            }
        }
        let Some(hill) = markers.first().map(|m| m.position) else {
            return;
        };
        let player = self.hill_player;
        let ride: &mut Ride = match others.get_mut(player.wrapping_sub(1)) {
            Some(ride2) if player > 0 => ride2,
            _ => ride,
        };
        let raise = atv_core::games::king_of_the_hill::MARKER_RAISE;
        // The hooked player's rider state (`state2`: split screen's player 2).
        let key = match player {
            0 => "state".to_string(),
            n => format!("state{}", n + 1),
        };
        match self.hill {
            1 => {
                place(ride, [hill[0], hill[1] - raise, hill[2]]);
                self.hill = 2;
                self.test_clock = 0.;
            }
            // Settled on the hill: the trigger enter is taken again with a
            // wheel down (the rules' contact flags restart).
            2 if self.test_clock > 1.5 && ride.frame().grounded.iter().any(|g| *g) => {
                session.rules.restart();
                self.hill = 3;
            }
            3 if session
                .rules
                .diagnostics()
                .iter()
                .any(|(k, v)| *k == key.as_str() && *v == -1.) =>
            {
                if let Some(n) = self.ride_off {
                    let yaw = f32::from(n) * std::f32::consts::FRAC_PI_2;
                    let (s, c) = [(0., 1.), (1., 0.), (0., -1.), (-1., 0.)][usize::from(n)];
                    let origin = ride.frame().origin;
                    let _ = ride.place_beside(origin, yaw, [s * 40., 12., c * 40.]);
                    self.hill = 7;
                    return;
                }
                // Off the hill's side in the air, moving away (a jump).
                let ground = ride.frame().origin[1];
                let _ =
                    ride.place_beside([hill[0] + 26., ground + 3., hill[2]], 0., [18., 14., 0.]);
                self.hill = 4;
            }
            4 if session
                .rules
                .diagnostics()
                .iter()
                .any(|(k, v)| *k == key.as_str() && *v >= 1.) =>
            {
                self.inject = Some((player, 1500.));
                self.hill = 5;
                self.test_clock = 0.;
            }
            // Captured: set down south of the hill facing its flag.
            5 if self.test_clock > 2.
                && markers.first().is_some_and(|m| m.owner == Some(player)) =>
            {
                let ground = ride.frame().origin[1].max(hill[1] - raise);
                let _ = ride.place_beside([hill[0], ground + 2., hill[2] - 45.], 0., [0.; 3]);
                self.hill = 6;
            }
            _ => {}
        }
    }

    /// The results summary once the game is over (shown once).
    /// `names`: each local player's name (rider index order).
    pub fn results(&mut self, names: &[&str]) -> Option<String> {
        let session = self.session.as_ref()?;
        if self.reported || !session.over() {
            return None;
        }
        self.reported = true;
        let end = match session.end {
            Some(atv_core::games::End::Complete) => "All Treasures Collected",
            _ => "Time Up",
        };
        let rows: Vec<String> = session
            .standings()
            .iter()
            .enumerate()
            .map(|(place, s)| {
                format!(
                    "{}. {}: {} {}, Stunt Score {}.",
                    place + 1,
                    names.get(s.rider).copied().unwrap_or("Rider"),
                    self.kind.score_label(),
                    s.score,
                    atv_ui::hud::objective::grouped(s.stunt as i64)
                )
            })
            .collect();
        let summary = format!(
            "{} on {}: {end}. {}",
            self.kind.name(),
            self.course,
            rows.join(" ")
        );
        self.summary = Some(summary.clone());
        Some(summary)
    }

    /// The HUD panel of a local player (0, or N - 1 for split screen's
    /// player N).
    pub fn view(&self, player: usize) -> Option<atv_ui::hud::games::View> {
        let session = self.session.as_ref();
        Some(atv_ui::hud::games::View {
            title: self.kind.name().to_string(),
            clock: session.map_or_else(
                || format!("{}:00", self.minutes),
                atv_core::games::Session::clock,
            ),
            low: session.is_some_and(|s| s.started && s.time_left <= 10.),
            count_label: match self.kind {
                GameKind::KingOfTheHills => "HILLS",
                GameKind::TreasureHunt => "TOKENS",
                _ => "SCORE",
            }
            .into(),
            count: session.map_or_else(String::new, |s| s.rules.progress(player)),
            score: session.map_or(0, |s| s.stunt.get(player).copied().unwrap_or(0.) as i64),
            place: session.map_or(1, |s| s.place(player)),
            start: session
                .filter(|s| !s.started)
                .map(|_| (self.countdown.ceil() as u32).max(1)),
            lights: session.and_then(|s| lamp_step(s.started, self.countdown, self.go)),
            sign: match session {
                None => Some(
                    self.error
                        .clone()
                        .unwrap_or_else(|| "Loading the game...".into()),
                ),
                Some(s) => s.notice.as_ref().map(|n| n.text.clone()).or_else(|| {
                    s.rules
                        .countdown()
                        .map(|t| format!("More Treasures in {}", t.ceil() as u32))
                }),
            },
        })
    }

    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }

    /// Each hill's flag model and colour (`00220b50` colours the captured
    /// hill's flag; `00220d10` resets them to white). The flags come from
    /// the King of the Hills scene: the `kohflag` model nearest each hill.
    pub fn flag_colors(
        &mut self,
        models: &[atv_core::scene::StaticModel],
    ) -> Vec<(usize, Option<[f32; 3]>)> {
        let Some(session) = &self.session else {
            return Vec::new();
        };
        if self.kind != GameKind::KingOfTheHills {
            return Vec::new();
        }
        let markers = session.rules.markers();
        let flags = self.flags.get_or_insert_with(|| {
            markers
                .iter()
                .map(|m| {
                    models
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.name.eq_ignore_ascii_case("kohflag"))
                        .map(|(i, s)| {
                            let (dx, dz) =
                                (s.position[0] - m.position[0], s.position[2] - m.position[2]);
                            let d = dx * dx + dz * dz;
                            (i, d)
                        })
                        .filter(|(_, d)| *d < 1600.)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                })
                .collect()
        });
        markers
            .iter()
            .zip(flags.iter())
            .filter_map(|(m, flag)| {
                Some((
                    flag.as_ref().copied()?,
                    m.owner.map(|o| OWNER_COLORS[o % 4]),
                ))
            })
            .collect()
    }

    /// World markers (hill flags, treasures) for the screen tags and map.
    pub fn markers(&self) -> Vec<atv_core::games::Marker> {
        self.session
            .as_ref()
            .filter(|s| s.end.is_none())
            .map_or_else(Vec::new, |s| s.rules.markers())
    }

    /// Diagnostics for tools/test_engine_games.mjs.
    pub fn state(&self) -> serde_json::Value {
        let session = self.session.as_ref();
        serde_json::json!({
            "kind": self.kind.key(),
            "venue": self.venue,
            "minutes": self.minutes,
            "loaded": session.is_some(),
            "error": self.error,
            "countdown": self.countdown,
            "lights": session.and_then(|s| lamp_step(s.started, self.countdown, self.go)),
            "started": session.is_some_and(|s| s.started),
            "timeLeft": session.map(|s| s.time_left),
            "end": session.and_then(|s| s.end),
            "over": session.is_some_and(Session::over),
            "notice": session.and_then(|s| s.notice.as_ref().map(|n| n.text.clone())),
            "scores": session.map(|s| s.rules.scores()),
            "standings": session.map(Session::standings),
            "stunt": session.map(|s| s.stunt[0]),
            "players": self.players,
            "stunts": session.map(|s| s.stunt.clone()),
            "progress": session.map(|s| (0..self.players).map(|p| s.rules.progress(p)).collect::<Vec<_>>()),
            "places": session.map(|s| (0..self.players).map(|p| s.place(p)).collect::<Vec<_>>()),
            "starts": self.starts,
            "markers": self.markers(),
            "rules": session.map(|s| s
                .rules
                .diagnostics()
                .into_iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect::<serde_json::Map<_, _>>()),
            "reported": self.reported,
            "summary": self.summary,
            "flags": self.flags,
            "lastChain": self.last_chain,
        })
    }
}

/// One player's sample for the rules: position, ground contact, the open
/// stunt and the chain finished since the last sample (its points: what it
/// added to the committed stunt score, else its stunts' sum, `00221020`).
/// Returns the rider and the finished chain's (landed, delta, sum).
fn sample(
    ride: &mut Ride,
    committed: &mut f32,
    inject: Option<f32>,
) -> (Rider, Option<(bool, f32, f32)>) {
    let total = ride.committed_points();
    let chains = ride.take_chains();
    let (_, free) = ride.take_free_records();
    let mut record = None;
    let chain = chains
        .last()
        .map(|(success, _)| {
            let sum = free.last().map_or(0., |c| c.2);
            let points = (total - *committed).max(0.);
            record = Some((*success, points, sum));
            (*success, if points > 0. { points } else { sum })
        })
        .or(inject.map(|points| (true, points)));
    *committed = total;
    let frame = ride.frame();
    (
        Rider {
            position: frame.origin,
            grounded: frame.grounded.iter().any(|g| *g),
            stunting: ride.stunting(),
            wrecked: frame.wreck.phase != atv_core::wreck::Phase::Riding,
            chain,
        },
        record,
    )
}

/// The mode TOY of a game archive's bytes.
pub fn mode_toy(bytes: &[u8], kind: GameKind, venue: &str) -> Result<String, String> {
    let archive = atv_core::resource::Resource::parse(bytes).map_err(|e| e.to_string())?;
    let name = atv_core::games::mode_toy(kind, venue);
    let index = archive
        .members
        .iter()
        .position(|m| m.name.to_ascii_lowercase().ends_with(&name))
        .ok_or_else(|| format!("The game archive lacks {name}."))?;
    let bytes = archive.extract(index).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|&b| char::from(b)).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn start_lamps_light_one_red_per_second_then_green_for_a_second() {
        use super::lamp_step;
        assert_eq!(lamp_step(false, 3., 0.), Some(1));
        assert_eq!(lamp_step(false, 2.5, 0.), Some(1));
        assert_eq!(lamp_step(false, 1.5, 0.), Some(2));
        assert_eq!(lamp_step(false, 0.2, 0.), Some(3));
        assert_eq!(lamp_step(true, 0., 0.6), Some(4));
        assert_eq!(lamp_step(true, 0., 0.), None);
    }
}
