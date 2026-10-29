//! Local split screen for two to four players (research/SPLIT_SCREEN.md):
//! the screen layout, player 2's keyboard keys, each player's controller
//! slot, and the combined race standings.
//!
//! Each player rides their own full `Riding` on the shared venue and scene;
//! the other player is drawn through the scene's multiplayer rider slots,
//! as a room's rider is. The camera presets are the original horizontal
//! (`Preset<n>h`) and vertical (`Preset<n>v`) split sections of
//! `FollowCamPresets.pb`.

/// The scene's multiplayer slot that draws player 2's rider; players 3
/// and 4 take the next slots (relay slots stay below them; a split ride
/// has no room).
pub const PEER: u8 = 250;

/// The most local players (the manual's four-player split screen).
pub const MAX_PLAYERS: u8 = 4;

/// The second rider's name on the race results and standings.
pub const PLAYER_2: &str = "Player 2";
/// Player 1 as player 2's race lists it.
pub const PLAYER_1: &str = "Player 1";

/// Each local player's name on standings and results ("Player N").
pub const PLAYER_NAMES: [&str; 4] = [PLAYER_1, PLAYER_2, "Player 3", "Player 4"];

/// Player `n`'s name (1-based).
pub fn player_name(n: u8) -> &'static str {
    PLAYER_NAMES[usize::from(n.clamp(1, MAX_PLAYERS)) - 1]
}

/// The scene slot that draws player `n` (2-4).
pub fn peer(n: u8) -> u8 {
    PEER + n.clamp(2, MAX_PLAYERS) - 2
}

/// Thin dark line between the two views (CSS pixels at scale 1; authored).
const DIVIDER: u32 = 2;

/// The original Game Settings "Split Screen Horizontal/Vertical" choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One view above the other (the screen split by a horizontal line).
    Horizontal,
    /// Side by side.
    Vertical,
    /// Three or four players: a quarter of the screen each (the camera
    /// loader's quarter-screen presets, `Preset<n>q`).
    Quad,
}

impl Layout {
    pub fn from_vertical(vertical: bool) -> Self {
        if vertical {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }

    /// The camera loader's layout enum (`atv_core::camera::presets`):
    /// 1 horizontal split, 2 vertical split, 3 quarter screen.
    pub fn camera_layout(self) -> u32 {
        match self {
            Self::Horizontal => 1,
            Self::Vertical => 2,
            Self::Quad => 3,
        }
    }

    /// Columns and rows of views.
    pub fn grid(self) -> (u32, u32) {
        match self {
            Self::Horizontal => (1, 2),
            Self::Vertical => (2, 1),
            Self::Quad => (2, 2),
        }
    }

    /// The aspect each split preset's field of view was authored for: half
    /// of the PS2's 4:3 frame (full width × half height, or half width ×
    /// full height; a quarter keeps 4:3). An assumption: the build's split
    /// viewport rectangles were not traced.
    pub fn reference_aspect(self) -> f32 {
        match self {
            Self::Horizontal => 8. / 3.,
            Self::Vertical => 2. / 3.,
            Self::Quad => 4. / 3.,
        }
    }

    /// The single-screen projection keeps a preset's FOV horizontal at 4:3
    /// and its vertical framing beyond; a split view keeps it horizontal at
    /// its own reference aspect, so the focal length scales by the ratio.
    pub fn focal_scale(self) -> f32 {
        self.reference_aspect() / (4. / 3.)
    }

    /// Each view (x, y, width, height in surface pixels) in player order:
    /// player 1 on top or on the left; quarters read left to right, top to
    /// bottom (two views, or four for `Quad`; three players leave the
    /// fourth quarter empty).
    pub fn rects(self, width: u32, height: u32, scale: f32) -> Vec<[u32; 4]> {
        let divider = ((DIVIDER as f32 * scale.max(1.)).round() as u32).max(1);
        // The two spans of a length cut by the divider.
        let halves = |length: u32| {
            let first = length.saturating_sub(divider) / 2;
            let second = first + divider;
            [
                (0, first.max(1)),
                (second, length.saturating_sub(second).max(1)),
            ]
        };
        let (columns, rows) = self.grid();
        let xs = if columns == 2 {
            halves(width).to_vec()
        } else {
            vec![(0, width)]
        };
        let ys = if rows == 2 {
            halves(height).to_vec()
        } else {
            vec![(0, height)]
        };
        ys.iter()
            .flat_map(|&(y, h)| xs.iter().map(move |&(x, w)| [x, y, w, h]))
            .collect()
    }

    /// The layout for `players` (three or four always share quarters).
    pub fn for_players(self, players: u8) -> Self {
        if players > 2 { Self::Quad } else { self }
    }
}

/// Player 2's keyboard keys (the right-hand side of the keyboard) and the
/// default binding each stands for: arrows drive, `.` and `/` (or right
/// Shift) lean, Numpad 1-3 stunt, Numpad 0 preload, `'` camera, Delete
/// resets. A port adaptation: the original seats each player at a pad.
/// Player 1 keeps every other key, so its WASD layout is unchanged.
pub const PLAYER_2_KEYS: [(&str, &str); 13] = [
    ("ArrowUp", "KeyW"),
    ("ArrowDown", "KeyS"),
    ("ArrowLeft", "KeyA"),
    ("ArrowRight", "KeyD"),
    ("Period", "KeyQ"),
    ("Slash", "KeyE"),
    ("ShiftRight", "KeyE"),
    ("Numpad1", "KeyJ"),
    ("Numpad2", "KeyK"),
    ("Numpad3", "KeyL"),
    ("Numpad0", "Space"),
    ("Quote", "KeyC"),
    ("Delete", "KeyR"),
];

/// Whether player 1's bindings see a held key while split.
pub fn player_1_key(code: &str) -> bool {
    !PLAYER_2_KEYS.iter().any(|(key, _)| *key == code)
}

/// Player 2's view of the keyboard: a default-binding code is held when its
/// player 2 key is.
pub fn player_2_key(held: &dyn Fn(&str) -> bool, code: &str) -> bool {
    PLAYER_2_KEYS
        .iter()
        .any(|(key, binding)| *binding == code && held(key))
}

/// Player `n`'s Freeride start: ten feet to the right of the player before
/// it (player 2 ten feet right of player 1; a port choice, the original's
/// split starts were not traced). In a race each takes the grid slot after
/// the field's last (`Course::grid` spacing).
pub fn freeride_start_for(origin: [f32; 3], yaw: f32, n: u8) -> ([f32; 3], f32) {
    use atv_core::math::Det;
    let (sin, cos) = yaw.sin_cos_d();
    let feet = 10. * f32::from(n.saturating_sub(1));
    (
        [origin[0] + cos * feet, origin[1], origin[2] - sin * feet],
        yaw,
    )
}

impl Layout {
    /// `?split=` / `FURY_SPLIT`: `h` (or `1`) horizontal, `v` vertical,
    /// `q` quarters (three or four players).
    pub fn requested(value: Option<&str>) -> Option<Self> {
        match value? {
            "h" | "horizontal" | "1" => Some(Self::Horizontal),
            "v" | "vertical" => Some(Self::Vertical),
            "q" | "quad" => Some(Self::Quad),
            _ => None,
        }
    }

    /// The `?split=` value that requests this layout.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn param(self) -> &'static str {
        match self {
            Self::Horizontal => "h",
            Self::Vertical => "v",
            Self::Quad => "q",
        }
    }
}

/// `url` with `query` appended (`?` or `&`).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn with_query(url: &str, query: &str) -> String {
    let joiner = if url.contains('?') { '&' } else { '?' };
    format!("{url}{joiner}{query}")
}

/// A split ride's request: the layout, the number of players (2-4) and
/// players 2-4's own Garage choices (none rides player 1's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub layout: Layout,
    pub players: u8,
    pub seconds: [Option<atv_game::ride::Selection>; 3],
    /// Players 2-4's own gear and paint (none dresses player 1's).
    pub looks: [Option<atv_game::ride::gear::Appearance>; 3],
}

impl Request {
    /// Players 2.. and their Garage choices.
    pub fn others(&self) -> impl Iterator<Item = (u8, Option<atv_game::ride::Selection>)> + '_ {
        (2..=self.players.clamp(2, MAX_PLAYERS)).map(|n| (n, self.seconds[usize::from(n) - 2]))
    }

    /// The screen layout these players share.
    pub fn view_layout(&self) -> Layout {
        self.layout.for_players(self.players)
    }
}

/// Player 2, 3 or 4: its own ride on the shared venue, its view and its
/// camera.
pub struct Second {
    /// The player number (2-4).
    pub player: u8,
    pub layout: Layout,
    pub riding: crate::riding::Riding,
    pub camera: atv_render::Camera,
    pub focal_length: f32,
    /// A connected controller drove player 2 this frame.
    pub pad: bool,
    /// Wrecks so far and whether the rider is off now (the room's crash
    /// serial, which plays a body impact).
    crash: (u32, bool),
}

/// Load-time choices both players share: (ATV and rider, gear, race,
/// crash mode, graphics).
pub type Shared = (
    atv_game::ride::Selection,
    atv_game::ride::gear::Appearance,
    Option<atv_game::ride::race::Settings>,
    atv_game::crash::CrashMode,
    atv_render::Graphics,
);

impl Second {
    /// Loads player `player` (2-4) beside player 1 (`first`) and switches
    /// both to the layout's original split camera presets.
    #[allow(clippy::too_many_arguments)]
    pub async fn load(
        gpu: &atv_render::Gpu,
        content: &atv_content::Content,
        venue: &atv_game::venue::Venue,
        scene: &mut atv_render::SceneRenderer,
        first: &mut crate::riding::Riding,
        request: Request,
        player: u8,
        (selection, appearance, race, crash, graphics): Shared,
    ) -> Result<Self, String> {
        let layout = request.view_layout();
        // The player's own Garage ATV, rider, gear and paint (the Split
        // Screen page's rows; a link without them rides player 1's).
        let seat = usize::from(player.saturating_sub(2));
        let selection = request
            .seconds
            .get(seat)
            .copied()
            .flatten()
            .unwrap_or(selection);
        let appearance = request
            .looks
            .get(seat)
            .copied()
            .flatten()
            .unwrap_or(appearance);
        first.ride.set_camera_layout(layout.camera_layout())?;
        // Player 2 races its own copy of the course with no CPUs: player
        // 1's race holds the field, and the standings merge both.
        let race = race.map(|r| atv_game::ride::race::Settings { cpu: 0, ..r });
        let mut ride = atv_game::ride::load_with(
            content,
            venue,
            selection.quad,
            selection.rider,
            race,
            |_| {},
        )
        .await?;
        ride.dress(content, appearance).await?;
        atv_game::yield_frame().await;
        ride.set_camera_layout(layout.camera_layout())?;
        let (origin, yaw) = match first.ride.race() {
            // The grid slots after the field's last, in player order.
            Some(state) => {
                let riders = state.race.grid_slots().len();
                state
                    .race
                    .course
                    .grid((riders + usize::from(player) - 1) as u32, riders)
            }
            None => {
                let start = venue.scene.start_position;
                let direction = venue.scene.start_direction;
                let (mut origin, yaw) = freeride_start_for(
                    start,
                    atv_core::math::Det::atan2_d(direction[0], direction[2]),
                    player,
                );
                if let Some(ground) = ride.ground(origin[0], origin[2]) {
                    origin[1] = ground + 2.5;
                }
                (origin, yaw)
            }
        };
        ride.place_beside(origin, yaw, [0.; 3])?;
        let key: atv_game::multiplayer::remote::Key = (
            selection.quad,
            selection.rider,
            std::array::from_fn(|_| 0xff),
        );
        scene.set_peer(
            gpu,
            venue,
            peer(player),
            key,
            (&ride.quad, &ride.rider),
            ride.paint(),
        )?;
        let mut riding = crate::riding::Riding::new(ride, venue);
        // Player 2's own wheel dust, chunks and spray (startSimulation's
        // wheel effects for its ATV); tire tracks and lingering dust stay
        // player 1's (one pass each).
        if let Some(mut effects) =
            atv_game::wheel_effects::load(content, venue, selection.quad).await?
        {
            effects.reset(&riding.ride)?;
            scene.set_second_wheel_effects(gpu, usize::from(player) - 2, Some(&effects));
            riding.effects = Some(effects);
        }
        riding.interpolation = graphics.interpolation;
        riding.wheel_reprojection = graphics.wheel_reprojection;
        riding.dirt_enabled = graphics.dirt;
        if crash != atv_game::crash::CrashMode::Original {
            let mut crashes = atv_game::crash::Crashes::new(crash);
            crashes.reset(&riding.ride)?;
            riding.crashes = Some(crashes);
        }
        if crate::url_flag("autopilot=1")
            && let Some(state) = riding.ride.race_mut()
        {
            state.autopilot_always = true;
        }
        Ok(Self {
            player,
            layout,
            riding,
            camera: atv_render::Camera {
                x: origin[0],
                y: origin[1] + 6.,
                z: origin[2],
                yaw,
                pitch: -0.25,
            },
            focal_length: 1.,
            pad: false,
            crash: (0, false),
        })
    }

    /// `splitN=quad.rider`: this player's part of a link's query.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn resume_part(&self) -> String {
        let s = self.riding.ride.selection();
        let look = self.riding.ride.appearance();
        let look: Vec<_> = look
            .gear
            .iter()
            .chain([&look.paint])
            .map(u8::to_string)
            .collect();
        format!(
            "split{n}={}.{}&look{n}={}",
            s.quad,
            s.rider,
            look.join("."),
            n = self.player
        )
    }

    /// Uploads player 2's rider into a rebuilt scene (a restored context).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn restore(
        &self,
        gpu: &atv_render::Gpu,
        venue: &atv_game::venue::Venue,
        scene: &mut atv_render::SceneRenderer,
    ) -> Result<(), String> {
        let ride = &self.riding.ride;
        scene.set_second_wheel_effects(
            gpu,
            usize::from(self.player) - 2,
            self.riding.effects.as_ref(),
        );
        let selection = ride.selection();
        let key: atv_game::multiplayer::remote::Key = (
            selection.quad,
            selection.rider,
            std::array::from_fn(|_| 0xff),
        );
        scene.set_peer(
            gpu,
            venue,
            peer(self.player),
            key,
            (&ride.quad, &ride.rider),
            ride.paint(),
        )
    }

    /// One rendered frame of this player's riding: its keys (player 2's
    /// keyboard half; players 3 and 4 play controllers only) and
    /// controller, with the other players and player 1's CPUs as its
    /// contact bodies (`others`).
    pub fn frame(
        &mut self,
        keys: &dyn Fn(&str) -> bool,
        pad: Option<&atv_game::ride::bindings::Pad>,
        now: f64,
        dt: f32,
        others: Vec<atv_game::multiplayer::PeerBody>,
    ) -> Result<(), String> {
        self.pad = pad.is_some();
        let player_2 = self.player == 2;
        let held = |code: &str| player_2 && player_2_key(keys, code);
        let sample = crate::riding::Sample {
            keys: &held,
            pad,
            now,
            drag: 0.,
        };
        let sampler = move |_: u64, _: &atv_game::ride::Ride| others.clone();
        let mut touch = || None;
        self.riding
            .frame(&sample, &mut touch, dt, Some((0, &sampler)))
    }

    /// Player 2 as the room riders' engine and impact voices hear it.
    pub fn audio_rider(&mut self) -> atv_game::audio::remote::Rider {
        let ride = &self.riding.ride;
        let f = ride.frame();
        let wreck = ride.rider_wreck();
        if wreck.is_some() && !self.crash.1 {
            self.crash.0 = self.crash.0.wrapping_add(1);
        }
        self.crash.1 = wreck.is_some();
        let (rider_position, rider_velocity) = wreck
            .map_or((f.origin, f.linear_velocity), |(m, v)| {
                ([m[12], m[13], m[14]], v)
            });
        atv_game::audio::remote::Rider {
            peer: peer(self.player),
            quad: ride.selection().quad,
            position: f.origin,
            velocity: f.linear_velocity,
            rider_position,
            rider_velocity,
            throttle: if self.riding.paused {
                0.
            } else {
                ride.applied().throttle
            },
            speed: f.speed_magnitude,
            // The room's RiderState::Riding: on the ground, not recovering.
            riding: wreck.is_none()
                && f.wreck.phase != atv_core::wreck::Phase::Recovering
                && f.grounded.iter().any(|g| *g),
            crash_serial: self.crash.0,
            stale: false,
        }
    }

    /// This player's camera this frame, and its rider's pose for its scene
    /// slot (`peer(player)`). The caller poses every player's slot in one
    /// `pose_peers` call: that call hides any slot it is not given.
    pub fn present(
        &mut self,
        gpu: &atv_render::Gpu,
        venue: &atv_game::venue::Venue,
        dt: f32,
    ) -> Result<(u8, atv_game::ride::Pose), String> {
        let (camera, focal, pose) =
            self.riding
                .present_view(gpu, None, venue, SPLIT_PRESENT_ASPECT, dt)?;
        self.camera = camera;
        self.focal_length = focal * self.layout.focal_scale();
        Ok((peer(self.player), pose))
    }
}

/// `split=..&splitn=..&split2=quad.rider..`: a split ride as a link's
/// query (the context-loss resume link reopens it split).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn resume_query(players: &[Second]) -> Option<String> {
    let first = players.first()?;
    let mut query = format!(
        "split={}&splitn={}",
        first.layout.param(),
        players.len() + 1
    );
    for player in players {
        query.push('&');
        query.push_str(&player.resume_part());
    }
    Some(query)
}

/// Links each player's race to the others before their steps: player 1's
/// CPUs see the other players as human peers, each race's PLACE and finish
/// signs read the combined place, and no race is over (no CPU estimates)
/// until every player has finished. Returns, for each of players 2..,
/// the contact bodies it rides against: player 1, each CPU, then the
/// other local players.
pub fn link(
    first: &mut atv_game::ride::Ride,
    others: &mut [Second],
) -> Vec<Vec<atv_game::multiplayer::PeerBody>> {
    let finished = |ride: &atv_game::ride::Ride| {
        ride.race()
            .and_then(|r| r.frame.as_ref())
            .is_some_and(|f| f.phase == "finished")
    };
    let placed = |ride: &atv_game::ride::Ride| {
        ride.race()
            .and_then(|r| r.frame.as_ref())
            .map(|f| f.position)
            .filter(|p| *p > 0)
    };
    let first_done = finished(first);
    let done: Vec<bool> = others.iter().map(|s| finished(&s.riding.ride)).collect();
    let first_place = placed(first);
    let humans: Vec<_> = others
        .iter()
        .map(|s| {
            let f = s.riding.ride.frame();
            (f.origin, f.linear_velocity, f.orientation)
        })
        .collect();
    if let Some(state) = first.race_mut() {
        state.split = atv_game::ride::race::SplitLink {
            humans,
            place: first_place,
            waiting: done.iter().any(|d| !d),
        };
    }
    let cpus: Vec<atv_game::multiplayer::PeerBody> = first
        .race_cpu_bodies()
        .into_iter()
        .map(
            |(position, velocity, orientation)| atv_game::multiplayer::PeerBody {
                position,
                velocity,
                orientation,
                tie: -1.,
            },
        )
        .collect();
    let local: Vec<(u8, atv_game::multiplayer::PeerBody)> = others
        .iter()
        .map(|s| (s.player, body(&s.riding.ride, 1.)))
        .collect();
    let mut bodies = Vec::with_capacity(others.len());
    for (index, second) in others.iter_mut().enumerate() {
        let place = placed(&second.riding.ride);
        let waiting = !first_done
            || done
                .iter()
                .enumerate()
                .any(|(other, d)| other != index && !d);
        if let Some(state) = second.riding.ride.race_mut() {
            state.split = atv_game::ride::race::SplitLink {
                humans: Vec::new(),
                place,
                waiting,
            };
        }
        let own = second.player;
        bodies.push(
            std::iter::once(body(first, -1.))
                .chain(cpus.iter().copied())
                .chain(local.iter().filter(|(p, _)| *p != own).map(|(p, b)| {
                    // The lower player number takes the -1 side of a tie.
                    atv_game::multiplayer::PeerBody {
                        tie: if *p < own { -1. } else { 1. },
                        ..*b
                    }
                }))
                .collect(),
        );
    }
    bodies
}

/// Split views present at the 4:3 reference (no portrait adaptation); the
/// layout's focal scale then fits the preset's FOV to its view.
pub const SPLIT_PRESENT_ASPECT: f32 = 4. / 3.;

/// A rider's contact body for the other player (`tie`: -1 for player 1,
/// the lower slot, else 1).
pub fn body(ride: &atv_game::ride::Ride, tie: f32) -> atv_game::multiplayer::PeerBody {
    let f = ride.frame();
    atv_game::multiplayer::PeerBody {
        position: f.origin,
        velocity: f.linear_velocity,
        orientation: f.orientation,
        tie,
    }
}

/// One rider in the combined race order.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// 0 a CPU, else the player number.
    pub player: u8,
    pub time: Option<f32>,
    pub progress: f32,
}

/// The combined running order: finished riders by time, then the rest by
/// race distance (the order `Race::race_over` and the standings use).
/// Returns indices into `rows`.
pub fn order(rows: &[Row]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&rows[a], &rows[b]);
        match (a.time, b.time) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => b.progress.total_cmp(&a.progress),
        }
    });
    order
}

/// Adds players 2.. (each the only rider of its own race) to player 1's
/// race frame: the standings gain their rows in running order, and each
/// player's place and field count them all. Each other player's frame
/// gets its combined place too. Idempotent: an earlier merge's rows are
/// replaced. `origins`: player 1's, then each other player's.
pub fn merge_race(
    first: &mut atv_core::race::Frame,
    others: &mut [&mut atv_core::race::Frame],
    origins: &[[f32; 3]],
) {
    let names: Vec<&str> = (0..others.len())
        .map(|i| player_name(i as u8 + 2))
        .collect();
    first
        .standings
        .retain(|s| !names.contains(&s.name.as_str()));
    first
        .opponents
        .retain(|o| !names.contains(&o.name.as_str()));
    let cpus: Vec<atv_core::race::Opponent> = first
        .opponents
        .iter()
        .map(|o| atv_core::race::Opponent {
            physical: None,
            ..o.clone()
        })
        .collect();
    let finished: Vec<bool> = others.iter().map(|o| o.phase == "finished").collect();
    let mut merged = vec![false; others.len()];
    for (i, other) in others.iter().enumerate() {
        let Some(own) = other.standings.iter().find(|s| s.player) else {
            continue;
        };
        merged[i] = true;
        first.standings.push(atv_core::race::Standing {
            name: names[i].into(),
            position: 0,
            laps: own.laps,
            // A finished player keeps its time; an unfinished one is
            // placed by distance (no estimate: the race waits for all).
            time: own.time.filter(|_| finished[i]),
            best: own.best,
            estimated: false,
            progress: own.progress,
            stunt: own.stunt,
            player: false,
        });
    }
    if !merged.iter().any(|m| *m) {
        return;
    }
    let player_of = |s: &atv_core::race::Standing| -> u8 {
        if s.player {
            1
        } else {
            names
                .iter()
                .position(|n| *n == s.name)
                .map_or(0, |i| i as u8 + 2)
        }
    };
    let rows: Vec<Row> = first
        .standings
        .iter()
        .map(|s| Row {
            player: player_of(s),
            time: s.time,
            progress: s.progress,
        })
        .collect();
    let order = order(&rows);
    let mut sorted: Vec<atv_core::race::Standing> = Vec::with_capacity(order.len());
    let mut taken: Vec<Option<atv_core::race::Standing>> = std::mem::take(&mut first.standings)
        .into_iter()
        .map(Some)
        .collect();
    for (place, &index) in order.iter().enumerate() {
        if let Some(mut s) = taken[index].take() {
            s.position = place + 1;
            sorted.push(s);
        }
    }
    let field = sorted.len();
    let place_of = |player: u8| {
        order
            .iter()
            .position(|&i| rows[i].player == player)
            .map_or(field, |p| p + 1)
    };
    let progress_of = |player: u8| {
        sorted
            .iter()
            .find(|s| player_of(s) == player)
            .map_or(0., |s| s.progress)
    };
    first.position = place_of(1);
    first.field = field;
    // The PLACE trees and the track map: each player's race lists the
    // other players among its opponents, with every rider's combined rank.
    let cpu_ranks: Vec<usize> = cpus
        .iter()
        .map(|o| rank_by_name(&sorted, &o.name))
        .collect();
    let opponent =
        |name: &str, origin: [f32; 3], distance: f32, finished: bool| atv_core::race::Opponent {
            name: name.into(),
            origin,
            yaw: 0.,
            distance,
            finished,
            physical: None,
        };
    let locals: Vec<(u8, atv_core::race::Opponent)> = (0..others.len())
        .filter(|i| merged[*i])
        .map(|i| {
            let n = i as u8 + 2;
            (
                n,
                opponent(
                    names[i],
                    origins.get(i + 1).copied().unwrap_or_default(),
                    progress_of(n),
                    finished[i],
                ),
            )
        })
        .collect();
    let first_opponent = opponent(
        PLAYER_1,
        origins.first().copied().unwrap_or_default(),
        progress_of(1),
        first.phase == "finished",
    );
    first.ranks = std::iter::once(place_of(1))
        .chain(cpu_ranks.iter().copied())
        .chain(locals.iter().map(|(n, _)| place_of(*n)))
        .collect();
    first
        .opponents
        .extend(locals.iter().map(|(_, o)| o.clone()));
    for (i, other) in others.iter_mut().enumerate() {
        if !merged[i] {
            continue;
        }
        let own = i as u8 + 2;
        other.position = place_of(own);
        other.field = field;
        let rest = locals.iter().filter(|(n, _)| *n != own);
        other.ranks = [place_of(own), place_of(1)]
            .into_iter()
            .chain(cpu_ranks.iter().copied())
            .chain(rest.clone().map(|(n, _)| place_of(*n)))
            .collect();
        other.opponents = std::iter::once(first_opponent.clone())
            .chain(cpus.iter().cloned())
            .chain(rest.map(|(_, o)| o.clone()))
            .collect();
    }
    first.standings = sorted;
    // The PLACE tree's names stand at their combined ranks (the original
    // tree's 0.3 s slide `002e5150` follows each race's own ranks only).
    for frame in std::iter::once(&mut *first).chain(others.iter_mut().map(|f| &mut **f)) {
        let atv_core::race::Frame { hud, ranks, .. } = frame;
        if let Some(hud) = hud {
            hud.tree = ranks.iter().map(|r| (*r as f32 - 1.) * 23.).collect();
        }
    }
}

/// A CPU's combined place in the sorted standings.
fn rank_by_name(sorted: &[atv_core::race::Standing], name: &str) -> usize {
    sorted
        .iter()
        .position(|s| !s.player && s.name == name)
        .map_or(sorted.len(), |p| p + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_split_the_frame_with_a_divider() {
        assert_eq!(
            with_query("https://x/play.html?map=a", "split=v"),
            "https://x/play.html?map=a&split=v"
        );
        assert_eq!(with_query("/play.html", "split=h"), "/play.html?split=h");
        for layout in [Layout::Horizontal, Layout::Vertical, Layout::Quad] {
            assert_eq!(Layout::requested(Some(layout.param())), Some(layout));
        }
        assert_eq!(
            Layout::Horizontal.rects(1280, 720, 1.),
            [[0, 0, 1280, 359], [0, 361, 1280, 359]]
        );
        assert_eq!(
            Layout::Vertical.rects(1281, 720, 2.),
            [[0, 0, 638, 720], [642, 0, 639, 720]]
        );
        // Quarters: left to right, top to bottom.
        assert_eq!(
            Layout::Quad.rects(1280, 720, 1.),
            [
                [0, 0, 639, 359],
                [641, 0, 639, 359],
                [0, 361, 639, 359],
                [641, 361, 639, 359]
            ]
        );
        // Degenerate surfaces keep non-empty views.
        for layout in [Layout::Vertical, Layout::Quad] {
            assert!(
                layout
                    .rects(1, 1, 1.)
                    .iter()
                    .all(|r| r[2] >= 1 && r[3] >= 1)
            );
        }
        assert_eq!(Layout::Horizontal.camera_layout(), 1);
        assert_eq!(Layout::Vertical.camera_layout(), 2);
        assert_eq!(Layout::Quad.camera_layout(), 3);
        assert!((Layout::Horizontal.focal_scale() - 2.).abs() < 1e-6);
        assert!((Layout::Vertical.focal_scale() - 0.5).abs() < 1e-6);
        assert!((Layout::Quad.focal_scale() - 1.).abs() < 1e-6);
        // Three and four players always share quarters.
        assert_eq!(Layout::Vertical.for_players(2), Layout::Vertical);
        assert_eq!(Layout::Vertical.for_players(3), Layout::Quad);
        assert_eq!(Layout::requested(Some("q")), Some(Layout::Quad));
        assert_eq!(peer(2), PEER);
        assert_eq!(peer(4), PEER + 2);
        assert_eq!(player_name(3), "Player 3");
    }

    #[test]
    fn keyboard_halves_do_not_overlap() {
        let held = |code: &str| matches!(code, "ArrowUp" | "KeyW" | "Numpad1");
        // Player 1 sees W but not the arrow; player 2 sees its arrow as W.
        assert!(player_1_key("KeyW") && !player_1_key("ArrowUp"));
        assert!(player_2_key(&held, "KeyW"));
        assert!(player_2_key(&held, "KeyJ"));
        assert!(!player_2_key(&held, "KeyS"));
        // Every player 2 key maps to a default binding player 1 also has.
        for (key, binding) in PLAYER_2_KEYS {
            assert!(!player_1_key(key));
            assert!(player_1_key(binding), "{binding}");
        }
    }

    #[test]
    fn controllers_fill_slots_in_order_and_follow_disconnects() {
        use crate::gamepad::split_slot;
        assert_eq!(split_slot([3, 7].into_iter(), 0), Some(3));
        assert_eq!(split_slot([3, 7].into_iter(), 1), Some(7));
        // Player 1's pad unplugged: player 2's pad now plays player 1.
        assert_eq!(split_slot([7].into_iter(), 0), Some(7));
        assert_eq!(split_slot([7].into_iter(), 1), None);
    }

    fn standing(
        name: &str,
        time: Option<f32>,
        progress: f32,
        player: bool,
    ) -> atv_core::race::Standing {
        atv_core::race::Standing {
            name: name.into(),
            position: 0,
            laps: 1,
            time,
            best: time,
            estimated: false,
            progress,
            stunt: 0.,
            player,
        }
    }

    fn frame(
        phase: &'static str,
        standings: Vec<atv_core::race::Standing>,
    ) -> atv_core::race::Frame {
        atv_core::race::Frame {
            opponents: standings
                .iter()
                .filter(|s| !s.player)
                .map(|s| atv_core::race::Opponent {
                    name: s.name.clone(),
                    origin: [0.; 3],
                    yaw: 0.,
                    distance: s.progress,
                    finished: false,
                    physical: None,
                })
                .collect(),
            phase,
            field: standings.len(),
            standings,
            ..Default::default()
        }
    }

    #[test]
    fn standings_merge_by_time_then_distance() {
        let mut first = frame(
            "racing",
            vec![
                standing("CPU A", None, 900., false),
                standing("You", None, 500., true),
                standing("CPU B", None, 100., false),
            ],
        );
        let mut second = frame("racing", vec![standing("You", None, 700., true)]);
        merge_race(&mut first, &mut [&mut second], &[[0.; 3], [1.; 3]]);
        let names: Vec<_> = first.standings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["CPU A", PLAYER_2, "You", "CPU B"]);
        assert_eq!((first.position, first.field), (3, 4));
        assert_eq!((second.position, second.field), (2, 4));
        // Merging again replaces the row; a finished player 2 leads by time.
        second.phase = "finished";
        second.standings[0].time = Some(61.5);
        merge_race(&mut first, &mut [&mut second], &[[0.; 3], [1.; 3]]);
        assert_eq!(first.standings.len(), 4);
        assert_eq!(first.standings[0].name, PLAYER_2);
        assert_eq!(first.standings[0].position, 1);
        assert_eq!(second.position, 1);
        assert!(first.standings.iter().map(|s| s.position).eq(1..=4));
        // Each race's PLACE tree lists the other player once.
        let names = |f: &atv_core::race::Frame| {
            f.opponents
                .iter()
                .map(|o| o.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&first), ["CPU A", "CPU B", PLAYER_2]);
        assert_eq!(names(&second), [PLAYER_1, "CPU A", "CPU B"]);
        assert_eq!(first.ranks, [3, 2, 4, 1]);
        assert_eq!(second.ranks, [1, 3, 2, 4]);
    }

    #[test]
    fn standings_merge_four_players_without_cpus() {
        let mut first = frame("racing", vec![standing("You", None, 300., true)]);
        let mut p2 = frame("finished", vec![standing("You", Some(70.), 1000., true)]);
        let mut p3 = frame("racing", vec![standing("You", None, 900., true)]);
        let mut p4 = frame("racing", vec![standing("You", None, 100., true)]);
        let origins = [[0.; 3], [1.; 3], [2.; 3], [3.; 3]];
        for _ in 0..2 {
            merge_race(&mut first, &mut [&mut p2, &mut p3, &mut p4], &origins);
        }
        let names: Vec<_> = first.standings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, [PLAYER_2, "Player 3", "You", "Player 4"]);
        assert_eq!(
            [first.position, p2.position, p3.position, p4.position],
            [3, 1, 2, 4]
        );
        assert!([&first, &p2, &p3, &p4].iter().all(|f| f.field == 4));
        // Each race lists the three other players, with their ranks.
        let names = |f: &atv_core::race::Frame| {
            f.opponents
                .iter()
                .map(|o| o.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&first), [PLAYER_2, "Player 3", "Player 4"]);
        assert_eq!(names(&p3), [PLAYER_1, PLAYER_2, "Player 4"]);
        assert_eq!(first.ranks, [3, 1, 2, 4]);
        assert_eq!(p3.ranks, [2, 3, 1, 4]);
        assert!(p3.opponents[1].finished && !p3.opponents[2].finished);
    }

    #[test]
    fn freeride_start_is_beside_player_1() {
        let (p, yaw) = freeride_start_for([100., 5., 100.], 0., 2);
        assert_eq!(yaw, 0.);
        assert!((p[0] - 110.).abs() < 1e-4 && (p[2] - 100.).abs() < 1e-4);
        // Players 3 and 4 line up further right.
        let (p, _) = freeride_start_for([100., 5., 100.], 0., 4);
        assert!((p[0] - 130.).abs() < 1e-4);
    }
}
