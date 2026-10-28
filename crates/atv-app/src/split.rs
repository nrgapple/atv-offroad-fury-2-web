//! Local two-player split screen (research/SPLIT_SCREEN.md): the screen
//! layout, the second player's keyboard keys and controller slot, and the
//! combined race standings.
//!
//! Each player rides their own full `Riding` on the shared venue and scene;
//! the other player is drawn through the scene's multiplayer rider slots,
//! as a room's rider is. The camera presets are the original horizontal
//! (`Preset<n>h`) and vertical (`Preset<n>v`) split sections of
//! `FollowCamPresets.pb`.

/// The scene's multiplayer slot that draws player 2's rider (relay slots
/// stay below it; a split ride has no room).
pub const PEER: u8 = 250;

/// The second rider's name on the race results and standings.
pub const PLAYER_2: &str = "Player 2";
/// Player 1 as player 2's race lists it.
pub const PLAYER_1: &str = "Player 1";

/// Thin dark line between the two views (CSS pixels at scale 1; authored).
const DIVIDER: u32 = 2;

/// The original Game Settings "Split Screen Horizontal/Vertical" choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One view above the other (the screen split by a horizontal line).
    Horizontal,
    /// Side by side.
    Vertical,
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
    /// 1 horizontal split, 2 vertical split.
    pub fn camera_layout(self) -> u32 {
        match self {
            Self::Horizontal => 1,
            Self::Vertical => 2,
        }
    }

    /// The aspect each split preset's field of view was authored for: half
    /// of the PS2's 4:3 frame (full width × half height, or half width ×
    /// full height). An assumption: the build's split viewport rectangles
    /// were not traced.
    pub fn reference_aspect(self) -> f32 {
        match self {
            Self::Horizontal => 8. / 3.,
            Self::Vertical => 2. / 3.,
        }
    }

    /// The single-screen projection keeps a preset's FOV horizontal at 4:3
    /// and its vertical framing beyond; a split view keeps it horizontal at
    /// its own reference aspect, so the focal length scales by the ratio.
    pub fn focal_scale(self) -> f32 {
        self.reference_aspect() / (4. / 3.)
    }

    /// Each player's view (x, y, width, height in surface pixels): player 1
    /// on top or on the left.
    pub fn rects(self, width: u32, height: u32, scale: f32) -> [[u32; 4]; 2] {
        let divider = ((DIVIDER as f32 * scale.max(1.)).round() as u32).max(1);
        match self {
            Self::Horizontal => {
                let first = height.saturating_sub(divider) / 2;
                let second_y = first + divider;
                [
                    [0, 0, width, first.max(1)],
                    [0, second_y, width, height.saturating_sub(second_y).max(1)],
                ]
            }
            Self::Vertical => {
                let first = width.saturating_sub(divider) / 2;
                let second_x = first + divider;
                [
                    [0, 0, first.max(1), height],
                    [second_x, 0, width.saturating_sub(second_x).max(1), height],
                ]
            }
        }
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

/// Where player 2 starts: Freeride, ten feet to player 1's right; a race,
/// the grid slot after the field's last (`Course::grid` spacing).
pub fn freeride_start(origin: [f32; 3], yaw: f32) -> ([f32; 3], f32) {
    use atv_core::math::Det;
    let (sin, cos) = yaw.sin_cos_d();
    (
        [origin[0] + cos * 10., origin[1], origin[2] - sin * 10.],
        yaw,
    )
}

impl Layout {
    /// `?split=` / `FURY_SPLIT`: `h` (or `1`) horizontal, `v` vertical.
    pub fn requested(value: Option<&str>) -> Option<Self> {
        match value? {
            "h" | "horizontal" | "1" => Some(Self::Horizontal),
            "v" | "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }
}

/// Player 2: its own ride on the shared venue, its view and its camera.
pub struct Second {
    pub layout: Layout,
    pub riding: crate::riding::Riding,
    pub camera: atv_render::Camera,
    pub focal_length: f32,
    /// A connected controller drove player 2 this frame.
    pub pad: bool,
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
    /// Loads player 2 beside player 1 (`first`) and switches both to the
    /// layout's original split camera presets.
    pub async fn load(
        gpu: &atv_render::Gpu,
        content: &atv_content::Content,
        venue: &atv_game::venue::Venue,
        scene: &mut atv_render::SceneRenderer,
        first: &mut crate::riding::Riding,
        layout: Layout,
        (selection, appearance, race, crash, graphics): Shared,
    ) -> Result<Self, String> {
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
            // The grid slot after the field's last.
            Some(state) => {
                let riders = state.race.grid_slots().len();
                state.race.course.grid(riders as u32 + 1, riders)
            }
            None => {
                let start = venue.scene.start_position;
                let direction = venue.scene.start_direction;
                let (mut origin, yaw) = freeride_start(
                    start,
                    atv_core::math::Det::atan2_d(direction[0], direction[2]),
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
            PEER,
            key,
            (&ride.quad, &ride.rider),
            ride.paint(),
        )?;
        let mut riding = crate::riding::Riding::new(ride, venue);
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
        })
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
        let selection = ride.selection();
        let key: atv_game::multiplayer::remote::Key = (
            selection.quad,
            selection.rider,
            std::array::from_fn(|_| 0xff),
        );
        scene.set_peer(
            gpu,
            venue,
            PEER,
            key,
            (&ride.quad, &ride.rider),
            ride.paint(),
        )
    }

    /// One rendered frame of player 2's riding: its keys and controller,
    /// with player 1 as its contact body (`other`).
    pub fn frame(
        &mut self,
        keys: &dyn Fn(&str) -> bool,
        pad: Option<&atv_game::ride::bindings::Pad>,
        now: f64,
        dt: f32,
        other: atv_game::multiplayer::PeerBody,
    ) -> Result<(), String> {
        self.pad = pad.is_some();
        let held = |code: &str| player_2_key(keys, code);
        let sample = crate::riding::Sample {
            keys: &held,
            pad,
            now,
            drag: 0.,
        };
        let sampler = move |_: u64, _: &atv_game::ride::Ride| vec![other];
        let mut touch = || None;
        self.riding
            .frame(&sample, &mut touch, dt, Some((0, &sampler)))
    }

    /// Player 2's camera this frame, and its rider drawn in the scene's
    /// player 2 slot.
    pub fn present(
        &mut self,
        gpu: &atv_render::Gpu,
        scene: &mut atv_render::SceneRenderer,
        venue: &atv_game::venue::Venue,
        dt: f32,
    ) -> Result<(), String> {
        let (camera, focal, pose) =
            self.riding
                .present_view(gpu, None, venue, SPLIT_PRESENT_ASPECT, dt)?;
        self.camera = camera;
        self.focal_length = focal * self.layout.focal_scale();
        scene.pose_peers(gpu, &[(PEER, pose)]);
        Ok(())
    }
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

/// Adds player 2 (the only rider of its own race) to player 1's race
/// frame: the standings gain player 2's row in running order, and each
/// player's place and field count both. Player 2's frame gets its combined
/// place too. Idempotent: an earlier merge's row is replaced.
pub fn merge_race(
    first: &mut atv_core::race::Frame,
    second: &mut atv_core::race::Frame,
    origins: [[f32; 3]; 2],
) {
    first.standings.retain(|s| s.name != PLAYER_2);
    first.opponents.retain(|o| o.name != PLAYER_2);
    let cpus: Vec<atv_core::race::Opponent> = first
        .opponents
        .iter()
        .map(|o| atv_core::race::Opponent {
            physical: None,
            ..o.clone()
        })
        .collect();
    let Some(own) = second.standings.iter().find(|s| s.player) else {
        return;
    };
    let p2 = atv_core::race::Standing {
        name: PLAYER_2.into(),
        position: 0,
        laps: own.laps,
        // A finished player 2 keeps its time; an unfinished one is placed
        // by distance (no estimate: the race waits for both players).
        time: own.time.filter(|_| second.phase == "finished"),
        best: own.best,
        estimated: false,
        progress: own.progress,
        stunt: own.stunt,
        player: false,
    };
    first.standings.push(p2);
    let rows: Vec<Row> = first
        .standings
        .iter()
        .map(|s| Row {
            player: if s.player {
                1
            } else if s.name == PLAYER_2 {
                2
            } else {
                0
            },
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
    first.position = place_of(1);
    first.field = field;
    second.position = place_of(2);
    second.field = field;
    // The PLACE trees and the track map: each player's race lists the
    // other player among its opponents, with every rider's combined rank.
    let cpu_ranks: Vec<usize> = cpus
        .iter()
        .map(|o| rank_by_name(&sorted, &o.name))
        .collect();
    let p1_progress = sorted.iter().find(|s| s.player).map_or(0., |s| s.progress);
    let p2_progress = sorted
        .iter()
        .find(|s| s.name == PLAYER_2)
        .map_or(0., |s| s.progress);
    let opponent =
        |name: &str, origin: [f32; 3], distance: f32, finished: bool| atv_core::race::Opponent {
            name: name.into(),
            origin,
            yaw: 0.,
            distance,
            finished,
            physical: None,
        };
    first.ranks = std::iter::once(place_of(1))
        .chain(cpu_ranks.iter().copied())
        .chain(std::iter::once(place_of(2)))
        .collect();
    first.opponents.push(opponent(
        PLAYER_2,
        origins[1],
        p2_progress,
        second.phase == "finished",
    ));
    second.ranks = [place_of(2), place_of(1)]
        .into_iter()
        .chain(cpu_ranks.iter().copied())
        .collect();
    second.opponents = std::iter::once(opponent(
        PLAYER_1,
        origins[0],
        p1_progress,
        first.phase == "finished",
    ))
    .chain(cpus)
    .collect();
    first.standings = sorted;
    // The PLACE tree's names stand at their combined ranks (the original
    // tree's 0.3 s slide `002e5150` follows each race's own ranks only).
    for frame in [&mut *first, &mut *second] {
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
        let [a, b] = Layout::Horizontal.rects(1280, 720, 1.);
        assert_eq!(a, [0, 0, 1280, 359]);
        assert_eq!(b, [0, 361, 1280, 359]);
        let [a, b] = Layout::Vertical.rects(1281, 720, 2.);
        assert_eq!(a, [0, 0, 638, 720]);
        assert_eq!(b, [642, 0, 639, 720]);
        // Degenerate surfaces keep non-empty views.
        let [a, b] = Layout::Vertical.rects(1, 1, 1.);
        assert!(a[2] >= 1 && b[2] >= 1);
        assert_eq!(Layout::Horizontal.camera_layout(), 1);
        assert_eq!(Layout::Vertical.camera_layout(), 2);
        assert!((Layout::Horizontal.focal_scale() - 2.).abs() < 1e-6);
        assert!((Layout::Vertical.focal_scale() - 0.5).abs() < 1e-6);
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
        merge_race(&mut first, &mut second, [[0.; 3], [1.; 3]]);
        let names: Vec<_> = first.standings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["CPU A", PLAYER_2, "You", "CPU B"]);
        assert_eq!((first.position, first.field), (3, 4));
        assert_eq!((second.position, second.field), (2, 4));
        // Merging again replaces the row; a finished player 2 leads by time.
        second.phase = "finished";
        second.standings[0].time = Some(61.5);
        merge_race(&mut first, &mut second, [[0.; 3], [1.; 3]]);
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
    fn freeride_start_is_beside_player_1() {
        let (p, yaw) = freeride_start([100., 5., 100.], 0.);
        assert_eq!(yaw, 0.);
        assert!((p[0] - 110.).abs() < 1e-4 && (p[2] - 100.).abs() < 1e-4);
    }
}
