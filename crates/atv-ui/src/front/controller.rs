//! Frontend state: pages, selection, course filter, garage choice and input
//! (frontend.js `show`, `action`, `run` and the pointer/controller handling
//! in menu-screens.js). Rows and labels come from atv_core::ui_menu with the
//! same request fields the browser sends.
use super::{
    BACK, COURSE, DONE, FIELD, GROUPS, Img, LIST, NEXT, PREV, QUAD_NEXT, QUAD_PREV, RIDE, RIDER,
    ROW, ROWS, ROWS_END, ROWS_PADDING, SHARE, START, STEP_NEXT, STEP_PREV, Screen, Stats, TAB,
    VEHICLE, assets::Track, course_group, group_label,
};
use crate::{hud::Viewport, paint::Rect, scroll::Scroller, settings::Settings};
use atv_core::rider_gear::{Appearance, Category, PAINTS, SLOTS};
use serde_json::{Value, json};

#[path = "store.rs"]
pub mod store;
pub use store::{StoreView, store_page};
pub mod games_page;
pub mod navigation;
pub mod passkey_page;
pub mod profile_pages;
pub mod seats_page;
pub mod status_page;
pub use passkey_page::{CloudCommand, CloudView};
use profile_pages::NO_PROFILE;

/// What the application must do after a frontend input.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Load and ride this course with the garage selection.
    Ride {
        environment: String,
        quad: u16,
        rider: u16,
    },
    /// Open the browser dev log (web).
    DevLog,
    /// Open the desktop and Android downloads (GitHub Releases) in a new tab (web).
    Downloads,
    /// Open Settings over the frontend.
    Settings,
    /// Main menu → Jukebox: the soundtrack player over the frontend.
    Jukebox,
    /// A Jukebox (or Settings → Audio song) control for the page's
    /// soundtrack player.
    Music(crate::menu::jukebox::JukeboxAction),
    /// Display → Fullscreen in Settings.
    Fullscreen,
    /// Arlington Free Ride: join a shared room at once (no setup) with the
    /// remembered rider name and the garage ATV (motorcycles ride ATV01).
    FreeRide { quad: u16, rider: u16, name: String },
    /// Host Game → Select Track → Ride: create a room on this course with an
    /// optional custom code and start riding at once.
    Host {
        environment: String,
        code: Option<String>,
        quad: u16,
        rider: u16,
        name: String,
    },
    /// Games → Tag → Start Game: host a room on the Tag course and start
    /// Tag there with Games Prep's minutes (research/GAMES.md "Tag").
    TagHost {
        environment: String,
        minutes: u32,
        quad: u16,
        rider: u16,
        name: String,
    },
    /// Games → King of the Hills / Treasure Hunt with Riders: Online Room:
    /// host a room on the course and start the game there for every room
    /// rider (research/MULTIPLAYER.md "Room games").
    GameHost {
        kind: atv_core::games::GameKind,
        environment: String,
        minutes: u32,
        quad: u16,
        rider: u16,
        name: String,
    },
    /// Host Game with Mode: Freestyle: host a room on the course and start
    /// room Freestyle there once joined (research/MULTIPLAYER.md "Room
    /// Freestyle").
    FreestyleHost {
        environment: String,
        code: Option<String>,
        quad: u16,
        rider: u16,
        name: String,
    },
    /// Back from the garage or course picker opened over a held room ride:
    /// ride again with this look (a new course for the host's Ride).
    RoomRide {
        quad: u16,
        rider: u16,
        environment: Option<String>,
    },
    /// Join Game: open the room by its code (an ended room opens again on
    /// `environment`, the current course, as an invite reopens on its map).
    Join {
        code: String,
        environment: String,
        quad: u16,
        rider: u16,
        name: String,
    },
    /// Free Ride Lobby → Copy invite link (while joining).
    CopyInvite,
    /// Free Ride Lobby → Leave Game, or Back: abandon the room being joined.
    LeaveLobby,
    /// Single Race → Start Race: race this course with the garage choice.
    Race {
        environment: String,
        quad: u16,
        rider: u16,
        laps: u32,
        cpu: u32,
        difficulty: u32,
        /// A race link's EATDIRT / BRINGIT cheats (aggressive, all out).
        cheats: (bool, bool),
        /// A championship round: the series index and Expert.
        championship: Option<(usize, bool)>,
    },
    /// Single Race → Copy ride link: the race setup's link.
    CopyRaceLink,
    /// Main Menu → Training Course: the original's six lessons on the
    /// Training venue (`atv_core::training`) with the garage selection.
    Training { quad: u16, rider: u16 },
    /// Single Player → Games → Start Game: an original mini-game
    /// (`atv_core::games`) on a course of its Tracks.dat list.
    Game {
        kind: atv_core::games::GameKind,
        venue: String,
        course: String,
        minutes: u32,
        quad: u16,
        rider: u16,
    },
    /// Single Player → Games → Hockey: the original Hockey on Coonce
    /// Arena (`atv_core::hockey`).
    Hockey { quad: u16, rider: u16 },
    /// Select Objective: ride a Freestyle championship course's objective
    /// (the series index into `atv_core::freestyle::SERIES`, Expert and the
    /// objective index).
    Objective {
        environment: String,
        quad: u16,
        rider: u16,
        series: usize,
        expert: bool,
        objective: usize,
    },
    /// A Pass Key page's request: copy, share, look up, save files.
    Cloud(CloudCommand),
}

/// Race setup offers 1-5 laps, 0, 3 or 4 CPU riders and the original
/// Normal (1) or Expert (2) difficulty (frontend.js RACE_CPU).
pub const RACE_CPU: [u32; 3] = [0, 3, 4];

/// The Single Race setup (frontend.js `raceSettings`).
#[derive(Clone, Debug, PartialEq)]
pub struct RaceSetup {
    pub course: String,
    pub laps: u32,
    pub cpu: u32,
    pub difficulty: u32,
    /// A race link's EATDIRT (aggressive) and BRINGIT (all out) cheats:
    /// kept through Start Race and Copy ride link, never saved
    /// (frontend.js `raceSettings` vs `normalizedRace`).
    pub cheats: (bool, bool),
}

impl RaceSetup {
    /// frontend.js `normalizedRace` of a saved `race` value.
    pub fn from_value(value: &Value) -> Self {
        Self {
            course: value["course"]
                .as_str()
                .filter(|c| atv_game::launch::is_race_course(c))
                .unwrap_or("env01")
                .into(),
            laps: value["laps"]
                .as_u64()
                .filter(|l| (1..=5).contains(l))
                .unwrap_or(3) as u32,
            cpu: value["cpu"]
                .as_u64()
                .and_then(|c| RACE_CPU.iter().copied().find(|r| u64::from(*r) == c))
                .unwrap_or(3),
            difficulty: if value["difficulty"].as_u64() == Some(2) {
                2
            } else {
                1
            },
            cheats: (false, false),
        }
    }

    pub fn value(&self) -> Value {
        json!({"course": self.course, "laps": self.laps, "cpu": self.cpu, "difficulty": self.difficulty})
    }
}

/// Rider Gear's rows (choice, appearance row) in page order.
const GEAR_ROWS: [(&str, &str); 7] = [
    ("gearhelmet", "helmet"),
    ("gearjersey", "jersey"),
    ("gearpant", "pant"),
    ("gearglove", "glove"),
    ("gearboot", "boot"),
    ("geargoggle", "goggle"),
    ("gearpaint", "paint"),
];

/// Value rows that Left/Right (and their arrows) step.
pub fn stepper(action: &str) -> bool {
    matches!(
        action,
        "racetrack"
            | "racelaps"
            | "racecpu"
            | "racedifficulty"
            | "champdifficulty"
            | "profileletter"
            | "recordevent"
            | "recordcourse"
            | "fameevent"
            | "famecourse"
            | "famedifficulty"
            | "storeitem"
            | "codeletter"
    ) || games_page::game_row(action)
        || GEAR_ROWS.iter().any(|(a, _)| *a == action)
        || action.strip_prefix("seat").is_some_and(|rest| {
            rest.len() > 1
                && seats_page::SEAT_ROWS
                    .iter()
                    .any(|(row, _)| *row == &rest[1..])
        })
}

/// The original catalog names pants and sleeves generically ("pants 10").
fn generic_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["pants ", "sleeves "].iter().any(|p| {
        lower
            .strip_prefix(p)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// The Free Ride Lobby page shown while a hosted or joined room connects
/// (multiplayer.js `host` shows 'lobby' before `connect`).
#[derive(Clone, Debug, PartialEq)]
pub struct Lobby {
    /// The room code, once known (a custom or typed code; blank while the
    /// relay assigns one).
    pub code: String,
    /// `lobby_status`: blank while connecting.
    pub status: String,
}

/// frontend.js refuses rooms on a motorcycle with this message.
pub const MOTORCYCLE_ROOM: &str =
    "Choose an ATV for multiplayer. Motorcycles are available in Single Player.";

/// A text field on the Host/Join forms (`ui_menu::TextInput`).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub action: &'static str,
    pub label: &'static str,
    pub value: String,
    pub max_length: usize,
}

/// Rider names: at most 24 bytes, no control characters (frontend.js).
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 24 && !name.chars().any(char::is_control)
}

/// localStorage key shared with the browser game.
pub const SELECTION_KEY: &str = "atv-player-selection";
/// ATVs 1-26 and motorcycles 27-46 (Freeride).
pub const VEHICLES: u16 = 46;

/// Rows the Rust engine cannot perform yet are left out of its menus.
const UNSUPPORTED: [&str; 0] = [];

/// Loader phases (frontend.js `loadingStage`) and their bar positions.
pub fn loading_stage(text: &str) -> (&'static str, f32) {
    let lower = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let status = if has(&["engine", "sound"]) {
        "Tuning sound…"
    } else if text.contains("ATV") {
        "Preparing your ride…"
    } else if has(&["rider"]) {
        "Getting your rider ready…"
    } else if has(&["texture", "sky", "model", "vegetation"]) {
        "Building the scenery…"
    } else if has(&["collision"]) {
        "Preparing the trail…"
    } else if has(&["incomplete", "static terrain"]) {
        "Ready to ride"
    } else {
        "Opening course…"
    };
    let progress = match status {
        "Opening course…" => 0.12,
        "Preparing the trail…" => 0.3,
        "Building the scenery…" => 0.5,
        "Preparing your ride…" => 0.7,
        "Getting your rider ready…" => 0.82,
        "Tuning sound…" => 0.92,
        _ => 1.,
    };
    (status, progress)
}

struct Pressed {
    pointer: u64,
    id: Option<u32>,
    x: f32,
    y: f32,
    moved: bool,
    /// The press began in a scrolling region (the list or the rows).
    scroll: Option<Pane>,
    /// It caught moving content: a grab, never a tap.
    caught: bool,
}

/// The frontend's scrolling regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    List,
    Rows,
}

/// The course loader's state.
#[derive(Clone, Debug, Default)]
pub struct Loading {
    pub environment: String,
    pub track: String,
    pub status: String,
    pub progress: f32,
    pub tip: Vec<String>,
    /// A championship round's briefing (the original loading screen).
    pub championship: Vec<String>,
    /// A race's Hall of Fame lines (`002585c0`).
    pub fame: Vec<String>,
}

pub struct Frontend {
    pub page: String,
    pub selection: usize,
    pub tracks: Vec<Track>,
    /// Index into `tracks`.
    pub track: usize,
    pub filter: String,
    pub quad_id: u16,
    pub rider_id: u16,
    /// The course/vehicle list's kinetic scroll (crate::scroll).
    pub list: Scroller,
    /// The rows column's scroll, where a page's rows outgrow it.
    pub rows: Scroller,
    /// Bring the selected row into view after the next layout (keys/pad).
    reveal_row: bool,
    /// The page just opened: its list shows the current card without a glide.
    fresh: bool,
    hovered: Option<u32>,
    pressed: Option<Pressed>,
    pad: u32,
    garage_return: String,
    /// The garage vehicles from ui_menu's catalog.
    pub catalog: Vec<super::Vehicle>,
    choices: Vec<(String, String)>,
    /// Scroll the current card into view after the next layout.
    reveal: bool,
    pub loading: Option<Loading>,
    /// The browser dev log is a web page; the desktop build hides it.
    pub web: bool,
    /// Saved selection fields other than the ones this port changes.
    saved: Value,
    /// Riders in Arlington Free Ride (the main menu's live count).
    pub freeride_riders: Option<u32>,
    /// The Host/Join forms (frontend.js playerName, createCode, roomCode).
    pub player_name: String,
    pub create_code: String,
    pub room_code: String,
    /// The focused form field, by index in [`Frontend::fields`].
    pub focus: Option<usize>,
    /// Android's soft keyboard is up over the lower screen: the form shows
    /// only the focused field, in the top slot above it.
    pub soft_keyboard: bool,
    /// The course select picks the room's course (Host Game → Select Track).
    pub hosting: bool,
    /// A form error (`#menu-notice` / the thrown message in frontend.js).
    pub notice: Option<String>,
    /// The loaded profile, shown on Single Player (PROFILE.md); a new
    /// unnamed one with none loaded.
    pub profile: atv_core::profile::Profile,
    /// The saved profiles and the loaded one (the Profile Editor's book).
    pub book: atv_core::profile::ProfileBook,
    /// The Hall of Fame (Main Menu → Hall of Fame).
    pub hall: atv_core::hall_of_fame::HallOfFame,
    /// Game Settings → Measurement for jump lengths.
    pub units: atv_core::hall_of_fame::Units,
    /// The Profile Editor pages' working state.
    pub profile_ui: profile_pages::ProfileUi,
    /// Single Player → Games' selection.
    pub games: games_page::GamesUi,
    /// The Championship page lists Expert progress.
    pub champ_expert: bool,
    /// The Freestyle championship on Select Objective: its index into
    /// `atv_core::freestyle::SERIES` and Expert.
    pub freestyle: Option<(usize, bool)>,
    /// Courses whose objective (OB) archive is published: their target
    /// runs are playable.
    pub objective_courses: Vec<String>,
    /// Single Player → Freestyle Objectives: the course picked outside a
    /// championship (Freestyle Prep's Objectives event).
    pub objective_course: Option<String>,
    /// The garage ("garage") or course picker ("tracks") is open over a
    /// held room ride; leaving it rides again.
    pub room: Option<&'static str>,
    /// The room being joined (the Free Ride Lobby page).
    pub lobby: Option<Lobby>,
    /// Race courses whose original race archive the content holds; the
    /// Race row shows when there are any.
    pub race_courses: Vec<String>,
    pub race: RaceSetup,
    pub race_filter: String,
    /// Single Race's course picker is open over the setup rows: the event
    /// tabs and every course as a full-height card list (a phone's column
    /// had room for one card, or none).
    pub course_sheet: bool,
    /// The garage's vehicle picker is open: the full ATV and motorcycle
    /// list over the column (it had room for one or two names).
    pub vehicle_sheet: bool,
    /// Rider gear and quad paint (normalizedAppearance), and the original
    /// Profile Store catalog that names and bounds it.
    pub appearance: Appearance,
    pub gear_catalog: Vec<Category>,
    /// Generation of the page last shown (each `show` slides a page in).
    pub shown: u64,
    /// A Shift key is held: Tab moves focus backwards.
    shift: bool,
    /// The Profile Store's catalogs and browsing state.
    pub store: StoreView,
    /// The Pass Key sync as the host last reported it.
    pub cloud: CloudView,
    /// The status bar's open popover and its chip's centre x.
    status_open: Option<super::status_bar::Chip>,
    status_anchor: f32,
    /// The Pass Key pages' working state.
    pub passkey: passkey_page::PassKeyUi,
    /// Multiplayer → Split Screen: the next Freestyle or Race rides
    /// `split_players` local players (research/SPLIT_SCREEN.md).
    pub split: bool,
    /// Split Screen layout for two players: side by side, else top and
    /// bottom (saved). Three and four players share quarters.
    pub split_vertical: bool,
    /// Split Screen: how many local players (2-4, saved).
    pub split_players: u8,
    /// Split Screen: players 2-4's own ATV and rider (0 male, 1 female),
    /// chosen on the Split Screen page (saved).
    pub split_seconds: [(u16, u16); 3],
    /// Split Screen: players 2-4's own gear and quad paint, chosen on the
    /// Split Screen page (saved).
    pub split_looks: [Appearance; 3],
    /// Host Game → Room: Freestyle: the hosted room starts a room
    /// Freestyle session (research/MULTIPLAYER.md "Room Freestyle").
    pub host_freestyle: bool,
    /// Split Screen → Players: the seats claimed before a split event.
    pub seats: seats_page::Seats,
    /// The pages Back returns through (`navigation`).
    history: Vec<navigation::Visit>,
}

/// Saved Split Screen layout ("vertical" or "horizontal").
pub const SPLIT_LAYOUT_KEY: &str = "atv-split-screen";
/// Saved player 2 ATV and rider ("quad,rider").
pub const SPLIT_SECOND_KEY: &str = "atv-split-player2";
/// Saved Split Screen player count (2-4).
pub const SPLIT_PLAYERS_KEY: &str = "atv-split-players";

/// Player `n`'s saved ATV and rider key (`atv-split-player2`..4).
pub fn split_player_key(n: u8) -> String {
    format!("atv-split-player{n}")
}

/// Player `n`'s saved gear and paint key (`atv-split-look2`..4):
/// the seven gear indices and the paint, comma-separated.
pub fn split_look_key(n: u8) -> String {
    format!("atv-split-look{n}")
}

/// A saved split look ("g0,..,g6,paint"); none when malformed.
pub fn parse_split_look(value: &str) -> Option<Appearance> {
    let parts: Vec<u8> = value
        .split([',', '.'])
        .map(|v| v.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [g0, g1, g2, g3, g4, g5, g6, paint] = parts[..] else {
        return None;
    };
    let look = Appearance {
        gear: [g0, g1, g2, g3, g4, g5, g6],
        paint,
    };
    look.valid().then_some(look)
}

/// A split look as saved ("g0,..,g6,paint").
pub fn split_look_value(look: &Appearance) -> String {
    look.gear
        .iter()
        .chain([&look.paint])
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Player 2's ATVs: the 26 original quads (rooms and the rider slot that
/// draws player 2 carry ATVs only).
const SPLIT_QUADS: u16 = 26;

impl Frontend {
    pub fn new(tracks: Vec<Track>, settings: &Settings, web: bool) -> Self {
        let saved: Value = settings
            .get(SELECTION_KEY)
            .and_then(|t| serde_json::from_str(&t).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        let quad_id = saved["quadId"]
            .as_u64()
            .filter(|q| (1..=u64::from(VEHICLES)).contains(q))
            .unwrap_or(1) as u16;
        let rider_id = u16::from(saved["riderId"].as_u64() == Some(1));
        let environment = saved["freestyleEnvironment"].as_str().unwrap_or("env01");
        let track = tracks.iter().position(|t| t.id == environment).unwrap_or(0);
        let race = RaceSetup::from_value(&saved["race"]);
        let appearance = atv_game::ride::gear::saved_appearance(&saved);
        let mut front = Self {
            page: "title".into(),
            selection: 0,
            tracks,
            track,
            filter: "all".into(),
            quad_id,
            rider_id,
            list: Scroller::default(),
            rows: Scroller::default(),
            reveal_row: false,
            fresh: false,
            hovered: None,
            pressed: None,
            pad: u32::MAX,
            garage_return: "single".into(),
            catalog: Vec::new(),
            choices: Vec::new(),
            reveal: false,
            loading: None,
            web,
            saved,
            freeride_riders: None,
            player_name: String::new(),
            create_code: String::new(),
            room_code: String::new(),
            focus: None,
            soft_keyboard: false,
            hosting: false,
            notice: None,
            profile: Default::default(),
            book: Default::default(),
            hall: Default::default(),
            units: Default::default(),
            profile_ui: Default::default(),
            games: Default::default(),
            champ_expert: false,
            freestyle: None,
            objective_courses: Vec::new(),
            objective_course: None,
            room: None,
            lobby: None,
            race_courses: Vec::new(),
            race,
            race_filter: "all".into(),
            course_sheet: false,
            vehicle_sheet: false,
            appearance,
            gear_catalog: Vec::new(),
            shown: crate::transition::next_show(),
            shift: false,
            store: StoreView::default(),
            cloud: CloudView::default(),
            status_open: None,
            status_anchor: 0.,
            passkey: Default::default(),
            split: false,
            split_vertical: settings.get(SPLIT_LAYOUT_KEY).as_deref() == Some("vertical"),
            split_players: settings
                .get(SPLIT_PLAYERS_KEY)
                .and_then(|v| v.parse().ok())
                .filter(|n| (2..=4).contains(n))
                .unwrap_or(2),
            split_seconds: [2u8, 3, 4].map(|n| {
                settings
                    .get(&split_player_key(n))
                    .and_then(|v| {
                        let (q, r) = v.split_once(',')?;
                        Some((q.parse().ok()?, r.parse().ok()?))
                    })
                    .filter(|(q, r): &(u16, u16)| (1..=SPLIT_QUADS).contains(q) && *r <= 1)
                    .unwrap_or((u16::from(n), 0))
            }),
            split_looks: [2u8, 3, 4].map(|n| {
                settings
                    .get(&split_look_key(n))
                    .and_then(|v| parse_split_look(&v))
                    .unwrap_or_default()
            }),
            host_freestyle: false,
            seats: Default::default(),
            history: Vec::new(),
        };
        front.load_profiles(settings);
        front.player_name = front.saved["playerName"]
            .as_str()
            .filter(|n| valid_name(n))
            .unwrap_or("Rider")
            .to_string();
        front.refresh(&Viewport {
            width: 1280.,
            height: 720.,
            ..Default::default()
        });
        front
    }

    /// Stores the garage choice, keeping the browser's other saved fields
    /// (last Freestyle course, race setup, name, gear).
    pub fn save(&mut self, settings: &mut Settings) {
        let mut value = self.saved.clone();
        value["quadId"] = self.quad_id.into();
        value["riderId"] = self.rider_id.into();
        if valid_name(&self.player_name) {
            value["playerName"] = self.player_name.clone().into();
        }
        value["appearance"] = json!({"gear": self.appearance.gear, "paint": self.appearance.paint});
        self.saved = value.clone();
        settings.set(SELECTION_KEY, &value.to_string());
    }

    /// A Freestyle ride starts on the current course: it becomes the saved
    /// last Freestyle course (frontend.js `lastFreestyleEnvironment`, which
    /// only Freestyle rides change).
    pub fn remember_freestyle(&mut self, settings: &mut Settings) {
        if let Some(track) = self.tracks.get(self.track) {
            self.saved["freestyleEnvironment"] = track.id.clone().into();
        }
        self.save(settings);
    }

    /// ui_menu rows for the page (labels and keyboard order).
    pub fn refresh(&mut self, view: &Viewport) {
        if store_page(&self.page) {
            self.choices = self.store_choices();
            self.selection = self.selection.min(self.choices.len().saturating_sub(1));
            return;
        }
        if self.page == "splitseats" {
            self.choices = self.seat_choices();
            self.selection = self.selection.min(self.choices.len().saturating_sub(1));
            return;
        }
        if self.page == "games" {
            self.choices = self.game_choices();
            self.selection = self.selection.min(self.choices.len().saturating_sub(1));
            return;
        }
        if matches!(
            self.page.as_str(),
            "championship" | "objectives" | "objective-courses"
        ) {
            self.choices = if self.page == "objectives" {
                self.objective_choices()
            } else if self.page == "objective-courses" {
                self.objective_course_choices()
            } else {
                self.championship_choices()
            };

            self.selection = self.selection.min(self.choices.len().saturating_sub(1));
            return;
        }
        if Self::is_passkey_page(&self.page) {
            self.choices = self.passkey_choices();
            self.selection = self.selection.min(self.choices.len().saturating_sub(1));
            return;
        }
        if Self::is_profile_page(&self.page) {
            self.choices = self.profile_choices();
            self.selection = self.selection.min(self.choices.len().saturating_sub(1));
            return;
        }
        let page = match self.page.as_str() {
            "tracks" | "loading" => "tracks",
            other => other,
        };
        let track_name = if page == "race" {
            self.course_name(&self.race.course)
        } else {
            self.tracks
                .get(self.track)
                .map_or(String::new(), |t| t.name.clone())
        };
        let request = json!({
            "page": page,
            "split_vertical": self.split_vertical,
            "host_freestyle": self.host_freestyle,
            "split_players": self.split_players,
            "split_seconds": (0..usize::from(self.split_players.clamp(2, 4)) - 1)
                .flat_map(|i| {
                    let (quad, rider) = self.split_seconds[i];
                    let look = self.split_looks[i];
                    [
                        format!("Player {} ATV: {}", i + 2, self.quad_label(quad)),
                        format!(
                            "Player {} Rider: {}",
                            i + 2,
                            if rider == 1 { "Female" } else { "Male" }
                        ),
                        format!("Player {} Gear: {}", i + 2, self.jersey_label(look)),
                        format!(
                            "Player {} Paint: {}",
                            i + 2,
                            PAINTS[usize::from(look.paint).min(PAINTS.len() - 1)]
                        ),
                    ]
                })
                .collect::<Vec<_>>(),
            "width": view.width.clamp(240., 16384.),
            "height": view.height.clamp(240., 16384.),
            "selection": self.selection,
            "quad_id": self.quad_id,
            "rider_id": self.rider_id,
            "track_name": track_name,
            "race_available": !self.race_courses.is_empty(),
            "race_course": self.race.course,
            "race_laps": self.race.laps,
            "race_cpu": self.race.cpu,
            "race_difficulty": self.race.difficulty,
            "gear_rows": if page == "gear" { self.gear_rows() } else { Vec::new() },
            "menu_logo": true,
            "menu_atv": true,
        });
        let Ok(request) = serde_json::from_value(request) else {
            return;
        };
        let Ok(frame) = atv_core::ui_menu::layout(request) else {
            return;
        };
        if self.catalog.is_empty() {
            self.catalog = frame
                .catalog
                .iter()
                .filter(|c| c.id <= VEHICLES)
                .map(|c| super::Vehicle {
                    id: c.id,
                    label: c.label.clone(),
                    motorcycle: c.group == "Motorcycle",
                    class: c.class.clone(),
                })
                .collect();
        }
        let label = |action: &str| {
            frame
                .hits
                .iter()
                .find(|h| h.action == action)
                .map_or(action.to_string(), |h| h.label.clone())
        };
        self.choices = frame
            .choices
            .iter()
            .filter(|a| !UNSUPPORTED.contains(&a.as_str()))
            .filter(|a| self.web || !matches!(a.as_str(), "devlog" | "downloads"))
            .map(|a| (a.clone(), label(a)))
            .collect();
        // Main Menu in the original's order (PAGE01: Single Player,
        // Multiplayer, Training Course, Profile Editor, Hall of Fame,
        // Options); Training Course when its venue is available.
        if page == "main" {
            let at = self
                .choices
                .iter()
                .position(|(a, _)| a == "multiplayer")
                .map_or(0, |i| i + 1);
            let mut rows = vec![
                ("profile", "Profile Editor"),
                ("halloffame", "Hall of Fame"),
            ];
            if self.tracks.iter().any(|t| t.id == "training") {
                rows.insert(0, ("training", "Training Course"));
            }
            for (i, (action, label)) in rows.into_iter().enumerate() {
                self.choices.insert(at + i, (action.into(), label.into()));
            }
        }
        // Single Player in the original's order (PAGE02: Race, Freestyle,
        // Championships) with the port's rows between Freestyle and the
        // Garage: Freestyle Objectives (the original's Freestyle Prep
        // Objectives event, `26a720`, `26d010`) after Freestyle, and Games
        // (the original's live under Multiplayer → Split Screen) after
        // Championships.
        if page == "single" {
            let mut rows = Vec::new();
            if !self.objective_courses.is_empty() {
                rows.push(("objective-courses", "Freestyle Objectives"));
            }
            if !self.race_courses.is_empty() {
                rows.push(("championship", "Championships"));
            }
            if self.store.courses.iter().any(|l| l.kind == 12) {
                rows.push(("games", "Games"));
            }
            let at = self
                .choices
                .iter()
                .position(|(a, _)| a == "garage")
                .unwrap_or(self.choices.len());
            for (i, (action, label)) in rows.into_iter().enumerate() {
                self.choices.insert(at + i, (action.into(), label.into()));
            }
        }
        self.selection = self.selection.min(self.choices.len().saturating_sub(1));
    }

    /// A course's display name (its id upper-cased when unlisted).
    pub fn course_name(&self, id: &str) -> String {
        let (venue, variant) = atv_game::launch::split_race_course(id)
            .unwrap_or((id, atv_game::launch::RaceVariant::National));
        let name = self
            .tracks
            .iter()
            .find(|t| t.id == venue)
            .map_or_else(|| venue.to_uppercase(), |t| t.name.clone());
        match variant.label() {
            Some(label) => format!("{name} {label}"),
            None => name,
        }
    }

    /// The race courses under the race event tab (frontend.js `raceOrder`).
    pub fn race_order(&self) -> Vec<String> {
        self.race_courses
            .iter()
            .filter(|id| self.race_filter == "all" || course_group(id) == self.race_filter)
            .cloned()
            .collect()
    }

    /// Race event tabs with at least one course.
    pub fn race_tabs(&self) -> Vec<(String, String, usize)> {
        GROUPS
            .iter()
            .filter_map(|(key, label)| {
                let count = if *key == "all" {
                    self.race_courses.len()
                } else {
                    self.race_courses
                        .iter()
                        .filter(|id| course_group(id) == *key)
                        .count()
                };
                (count > 0).then(|| (key.to_string(), label.to_string(), count))
            })
            .collect()
    }

    /// The content's race courses; the setup's course must be one of them.
    pub fn set_race_courses(&mut self, courses: Vec<String>, view: &Viewport) {
        self.race_courses = courses;
        if !self.race_courses.is_empty() && !self.race_courses.contains(&self.race.course) {
            self.race.course = self.race_courses[0].clone();
        }
        self.refresh(view);
    }

    fn set_race_filter(&mut self, filter: &str) {
        self.race_filter = filter.into();
        let order = self.race_order();
        if !order.is_empty() && !order.contains(&self.race.course) {
            self.race.course = order[0].clone();
        }
        self.list.reset();
        self.reveal = true;
    }

    /// Opens the garage's vehicle picker on the current vehicle.
    pub fn open_vehicle_sheet(&mut self, view: &Viewport) {
        self.vehicle_sheet = true;
        if let Some(index) = self.choices.iter().position(|(a, _)| a == "quadprev") {
            self.selection = index;
        }
        self.notice = None;
        self.list = Scroller::default();
        self.reveal = true;
        self.fresh = true;
        self.shown = crate::transition::next_show();
        self.refresh(view);
    }

    fn close_vehicle_sheet(&mut self, view: &Viewport) {
        self.vehicle_sheet = false;
        self.shown = crate::transition::next_show();
        self.refresh(view);
    }

    /// Keys while the vehicle picker is open: the arrows walk the owned
    /// vehicles (as the vehicle row's arrows do), Enter or Back closes it.
    fn vehicle_sheet_key(
        &mut self,
        code: &str,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Event> {
        match code {
            "ArrowUp" | "ArrowLeft" => {
                self.run("quadprev", view, settings);
                self.reveal = true;
            }
            "ArrowDown" | "ArrowRight" => {
                self.run("quadnext", view, settings);
                self.reveal = true;
            }
            "Enter" | "Space" | "NumpadEnter" | "Escape" | "Backspace" => {
                self.close_vehicle_sheet(view)
            }
            _ => {}
        }
        None
    }

    /// Opens Single Race's course picker on the current course.
    pub fn open_course_sheet(&mut self, view: &Viewport) {
        self.course_sheet = true;
        if let Some(index) = self.choices.iter().position(|(a, _)| a == "racetrack") {
            self.selection = index;
        }
        self.list = Scroller::default();
        self.reveal = true;
        self.fresh = true;
        self.shown = crate::transition::next_show();
        self.refresh(view);
    }

    /// Closes the course picker back to the setup rows, on the Course row.
    fn close_course_sheet(&mut self, view: &Viewport) {
        self.course_sheet = false;
        self.shown = crate::transition::next_show();
        self.refresh(view);
    }

    /// Keys (and controller buttons) while the course picker is open: the
    /// arrows walk the courses, Q/E (LB/RB) the event tabs, and Enter or
    /// Back closes it on the course chosen.
    fn sheet_key(&mut self, code: &str, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        match code {
            "ArrowUp" | "ArrowLeft" => self.step("racetrack", -1, view, settings),
            "ArrowDown" | "ArrowRight" => self.step("racetrack", 1, view, settings),
            "PageUp" | "KeyQ" => {
                self.cycle_race_filter(-1);
                self.refresh(view);
            }
            "PageDown" | "KeyE" => {
                self.cycle_race_filter(1);
                self.refresh(view);
            }
            "Enter" | "Space" | "NumpadEnter" | "Escape" | "Backspace" => {
                self.close_course_sheet(view)
            }
            _ => {}
        }
        None
    }

    fn cycle_race_filter(&mut self, direction: i32) {
        let tabs = self.race_tabs();
        if tabs.is_empty() {
            return;
        }
        let at = tabs
            .iter()
            .position(|(k, _, _)| *k == self.race_filter)
            .unwrap_or(0) as i32;
        let next = tabs[(at + direction).rem_euclid(tabs.len() as i32) as usize]
            .0
            .clone();
        self.set_race_filter(&next);
    }

    /// A look's item name in one gear slot (Original for the rider's own;
    /// pants and sleeves take their jersey line's name).
    pub(super) fn gear_name(&self, a: Appearance, slot: &str) -> String {
        let at = SLOTS.iter().position(|s| *s == slot).unwrap_or(0);
        let index = usize::from(a.gear[at]);
        let item = index
            .checked_sub(1)
            .and_then(|i| self.gear_catalog.get(at)?.items.get(i))
            .map(|item| item.name.clone());
        match item {
            None => "Original".to_string(),
            Some(item) if generic_name(&item) => {
                let shirt = SLOTS.iter().position(|s| *s == "shirt").unwrap_or(3);
                self.gear_catalog
                    .get(shirt)
                    .and_then(|c| c.items.get(index - 1))
                    .map_or(item, |i| i.name.clone())
            }
            Some(item) => item,
        }
    }

    /// Gear rows' values (frontend.js `gearRows`): helmet, jersey, pants,
    /// gloves, boots, goggles and the quad paint.
    pub fn gear_rows(&self) -> Vec<String> {
        let a = self.appearance;
        let name = |slot: &str| self.gear_name(a, slot);
        vec![
            name("helmet"),
            name("shirt"),
            name("pant"),
            name("glove"),
            name("boot"),
            name("goggle"),
            PAINTS[usize::from(a.paint)].to_string(),
        ]
    }

    /// A look's jersey line (the Garage's Jersey row name), or Original.
    fn jersey_label(&self, look: Appearance) -> String {
        let shirt = SLOTS.iter().position(|s| *s == "shirt").unwrap_or(3);
        usize::from(look.gear[shirt])
            .checked_sub(1)
            .and_then(|i| self.gear_catalog.get(shirt)?.items.get(i))
            .map_or_else(|| "Original".to_string(), |item| item.name.clone())
    }

    /// Split Screen → Player N Gear: the next owned jersey line (Original
    /// and the catalog's jerseys, as the Garage's Jersey row steps), with
    /// its matching sleeves and pants (the catalog's lines share an index).
    fn step_split_gear(&self, mut look: Appearance) -> Appearance {
        let at = SLOTS.iter().position(|s| *s == "shirt").unwrap_or(3);
        let items = self.gear_catalog.get(at).map_or(0, |c| c.items.len());
        if items == 0 {
            return look;
        }
        for _ in 0..=items {
            look.gear[at] = (usize::from(look.gear[at]) + 1).rem_euclid(items + 1) as u8;
            if self.gear_choice_owned(at, look.gear[at]) {
                break;
            }
        }
        for slot in ["sleeve", "pant"] {
            let Some(s) = SLOTS.iter().position(|n| *n == slot) else {
                continue;
            };
            let count = self.gear_catalog.get(s).map_or(0, |c| c.items.len());
            let index = look.gear[at].min(count.min(255) as u8);
            look.gear[s] = if self.gear_choice_owned(s, index) {
                index
            } else {
                0
            };
        }
        look
    }

    /// frontend.js `stepGear`: one row through Original and the catalog;
    /// the jersey sets the shirt and its matching sleeves together.
    fn step_gear(&mut self, row: &str, direction: i32) {
        let mut a = self.appearance;
        if row == "paint" {
            a.paint = (i32::from(a.paint) + direction).rem_euclid(PAINTS.len() as i32) as u8;
        } else {
            let slot = if row == "jersey" { "shirt" } else { row };
            let Some(at) = SLOTS.iter().position(|s| *s == slot) else {
                return;
            };
            let items = self.gear_catalog.get(at).map_or(0, |c| c.items.len());
            if items == 0 {
                return;
            }
            for _ in 0..=items {
                a.gear[at] = (i32::from(a.gear[at]) + direction).rem_euclid(items as i32 + 1) as u8;
                // Locked gear is skipped (Original, index 0, is the rider's own).
                if self.online_garage() || self.gear_choice_owned(at, a.gear[at]) {
                    break;
                }
            }
            if row == "jersey" {
                let sleeve = SLOTS.iter().position(|s| *s == "sleeve").unwrap_or(4);
                let sleeves = self.gear_catalog.get(sleeve).map_or(0, |c| c.items.len());
                a.gear[sleeve] = a.gear[at].min(sleeves.min(255) as u8);
            }
        }
        self.appearance = a;
    }

    /// A value row stepped by Left/Right, its arrows or a tap (+1).
    fn step(&mut self, action: &str, direction: i32, view: &Viewport, settings: &mut Settings) {
        if self.page == "splitseats" {
            self.seat_action(action, direction, view, settings);
            return;
        }
        if matches!(
            action,
            "profileletter"
                | "recordevent"
                | "recordcourse"
                | "fameevent"
                | "famecourse"
                | "famedifficulty"
        ) {
            // Arrows step; a tap or Enter on the letter row adds its letter
            // (`run`).
            self.step_profile(action, direction, view);
            return;
        }
        match action {
            "codeletter" => return self.step_passkey(direction, view),
            _ if games_page::game_row(action) => self.step_game(action, direction),
            "champdifficulty" => {
                self.champ_expert = !self.champ_expert;
                self.notice = None;
            }
            "storeitem" => return self.store_step(direction, view),
            "racetrack" => {
                let order = self.race_order();
                if !order.is_empty() {
                    let at = order
                        .iter()
                        .position(|c| *c == self.race.course)
                        .unwrap_or(0) as i32;
                    self.race.course =
                        order[(at + direction).rem_euclid(order.len() as i32) as usize].clone();
                    self.reveal = true;
                }
            }
            "racelaps" => {
                self.race.laps = ((self.race.laps as i32 - 1 + direction).rem_euclid(5) + 1) as u32
            }
            "racecpu" => {
                let at = RACE_CPU
                    .iter()
                    .position(|c| *c == self.race.cpu)
                    .unwrap_or(1) as i32;
                self.race.cpu = RACE_CPU[(at + direction).rem_euclid(3) as usize];
            }
            "racedifficulty" => {
                self.race.difficulty = if self.race.difficulty == 2 { 1 } else { 2 }
            }
            _ => {
                let Some((_, row)) = GEAR_ROWS.iter().find(|(a, _)| *a == action) else {
                    return;
                };
                self.step_gear(row, direction);
                self.save(settings);
            }
        }
        self.refresh(view);
    }

    fn race_event(&mut self, settings: &mut Settings) -> Option<Event> {
        if !self.race_courses.contains(&self.race.course) {
            return None;
        }
        if let Some(cost) = self.race_course_cost(&self.race.course) {
            self.notice = Some(Self::locked_course(cost));
            return None;
        }
        self.notice = self.enforce_unlocks(settings);
        // The last started race is remembered (frontend.js `lastRace`).
        self.saved["race"] = self.race.value();
        self.save(settings);
        Some(Event::Race {
            environment: self.race.course.clone(),
            quad: self.quad_id,
            rider: self.rider_id,
            laps: self.race.laps,
            cpu: self.race.cpu,
            difficulty: self.race.difficulty,
            cheats: self.race.cheats,
            championship: None,
        })
    }

    /// The Championship page rows: each series with its progress, then the
    /// difficulty (PROFILE.md, CHAMPIONSHIPS.md).
    fn championship_choices(&self) -> Vec<(String, String)> {
        use atv_core::championship::SERIES;
        let champ = &self.profile.championships;
        let expert = self.champ_expert;
        let mut rows: Vec<(String, String)> = SERIES
            .iter()
            .map(|s| {
                let label = if !champ.is_open(s, expert) {
                    format!("{} · Locked", s.name)
                } else {
                    let p = champ.progress(s, expert);
                    let rounds = s.rounds[usize::from(expert)];
                    let mut label = format!(
                        "{} · Round {} of {} · {} pts",
                        s.name,
                        p.round + 1,
                        rounds,
                        p.points[0]
                    );
                    if let Some(t) = p.trophy {
                        label += &format!(" · {}", t.name());
                    }
                    label
                };
                (format!("champ:{}", s.key), label)
            })
            .collect();
        // The Freestyle championships (objectives, FREESTYLE notes).
        rows.extend(atv_core::freestyle::SERIES.iter().map(|s| {
            let label = if !champ.freestyle_open(s, expert) {
                format!("{} · Locked", s.name)
            } else {
                let p = champ.progress(s, expert);
                let mut label = format!(
                    "{} · Round {} of {} · {}/{}",
                    s.name,
                    p.round + 1,
                    s.rounds[usize::from(expert)],
                    champ.completed_objectives(s, expert),
                    atv_core::freestyle::total_objectives(s)
                );
                if let Some(t) = p.trophy {
                    label += &format!(" · {}", t.name());
                }
                label
            };
            (format!("free:{}", s.key), label)
        }));
        rows.push((
            "champdifficulty".into(),
            format!("Difficulty: {}", if expert { "Expert" } else { "Normal" }),
        ));
        rows.push(("back".into(), "Back".into()));
        rows
    }

    /// The line under the rows: the profile on Single Player, the
    /// highlighted series on Championship.
    pub fn profile_text(&self) -> Option<String> {
        match self.page.as_str() {
            // The status bar shows the profile and its points; the line
            // stays only for the original's warning (0x431440: no profile,
            // nothing is saved).
            "single" => match self.profile_name() {
                Some(_) => None,
                None => {
                    Some("No profile loaded · Remember, your progress is not being saved!".into())
                }
            },
            "championship" => self.series_line(),
            "games" => self.games_line(),
            "objectives" => self.objectives_line(),
            "objective-courses" => {
                Some("Ride any Freestyle course's objectives outside a championship.".into())
            }
            page if store_page(page) => self.store_text(),
            _ => self.profile_status(),
        }
    }

    /// The highlighted series' next course and standings (the player, then
    /// the four CPU slots).
    fn series_line(&self) -> Option<String> {
        let (action, _) = self.choices.get(self.selection)?;
        if let Some(key) = action.strip_prefix("free:") {
            let series = atv_core::freestyle::series(key)?;
            let champ = &self.profile.championships;
            if !champ.freestyle_open(series, self.champ_expert) {
                return None;
            }
            let course = champ.next_course(series, self.champ_expert);
            return Some(format!(
                "Next: {} · Completed Course Objectives: {} of 6",
                self.course_name(course),
                champ
                    .course_mask(series, self.champ_expert, course)
                    .count_ones()
            ));
        }
        let series = atv_core::championship::series(action.strip_prefix("champ:")?)?;
        let champ = &self.profile.championships;
        if !champ.is_open(series, self.champ_expert) {
            return None;
        }
        let p = champ.progress(series, self.champ_expert);
        let cpus: Vec<String> = p.points[1..].iter().map(i32::to_string).collect();
        Some(format!(
            "Next: {} · Standings: You {}, CPUs {}",
            self.course_name(champ.next_course(series, self.champ_expert)),
            p.points[0],
            cpus.join(" / ")
        ))
    }

    /// The Freestyle championship on Select Objective.
    fn freestyle_series(&self) -> Option<(&'static atv_core::championship::Series, bool)> {
        let (index, expert) = self.freestyle?;
        Some((atv_core::freestyle::SERIES.get(index)?, expert))
    }

    /// The objective page's series, difficulty and course: the
    /// championship round's, or the standalone course (its series' Normal
    /// mask: `322080` saves standalone runs to the same per-course bits).
    fn objective_target(&self) -> Option<(&'static atv_core::championship::Series, bool, &str)> {
        if let Some((series, expert)) = self.freestyle_series() {
            return Some((
                series,
                expert,
                self.profile.championships.next_course(series, expert),
            ));
        }
        let course = self.objective_course.as_deref()?;
        let series = atv_core::freestyle::SERIES
            .iter()
            .find(|s| s.courses.contains(&course))?;
        Some((series, false, course))
    }

    /// Single Player → Freestyle Objectives: every course with an
    /// objective archive, Amateur venues first, with its completed count.
    fn objective_course_choices(&self) -> Vec<(String, String)> {
        let champ = &self.profile.championships;
        let mut rows: Vec<(String, String)> = atv_core::freestyle::SERIES
            .iter()
            .flat_map(|s| s.courses.iter().map(move |c| (s, *c)))
            .filter(|(_, c)| self.objective_courses.iter().any(|o| o == c))
            .filter(|(_, c)| self.tracks.iter().any(|t| t.id == *c))
            .map(|(s, c)| {
                (
                    format!("objcourse:{c}"),
                    format!(
                        "{} · {}/6",
                        self.course_name(c),
                        champ.course_mask(s, false, c).count_ones()
                    ),
                )
            })
            .collect();
        rows.push(("back".into(), "Back".into()));
        rows
    }

    /// Select Objective (Overlays.pb `[Objectives Menu]`): the round
    /// course's objectives, completed ones checked, then Finish Course.
    fn objective_choices(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        if let Some((series, expert, course)) = self.objective_target() {
            let champ = &self.profile.championships;
            let mask = champ.course_mask(series, expert, course);
            let done = |bit: u32| mask & (1 << bit) != 0;
            for (i, o) in atv_core::freestyle::course_objectives(course)
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                let bits = o.bits(i);
                let label = match &o.kind {
                    atv_core::freestyle::Kind::Stunt { levels } => format!(
                        "{} · {} of {} levels",
                        o.description,
                        bits.iter().filter(|b| done(**b)).count(),
                        levels.len()
                    ),
                    _ if bits.iter().all(|b| done(*b)) => format!("{} · Done", o.description),
                    _ if !o.playable(self.objective_courses.iter().any(|c| c == course)) => {
                        format!("{} · Unavailable", o.description)
                    }
                    _ => o.description.clone(),
                };
                rows.push((format!("obj:{i}"), label));
            }
        }
        if let Some((series, expert)) = self.freestyle_series() {
            let champ = &self.profile.championships;
            let last =
                champ.progress(series, expert).round + 1 == series.rounds[usize::from(expert)];
            rows.push((
                "objfinish".into(),
                if last {
                    "Finish Championship"
                } else {
                    "Next Course"
                }
                .into(),
            ));
        }
        rows.push(("back".into(), "Back".into()));
        rows
    }

    /// Select Objective's line: the course, round and counts (the
    /// original ProfileView_FreestyleEvent readouts, `0025a100`).
    fn objectives_line(&self) -> Option<String> {
        if self.freestyle.is_none() {
            let (series, expert, course) = self.objective_target()?;
            let champ = &self.profile.championships;
            return Some(format!(
                "{} · {} · Course objectives {}/6",
                self.course_name(course),
                series.name.replace(" Freestyle", " course"),
                champ.course_mask(series, expert, course).count_ones(),
            ));
        }
        let (series, expert) = self.freestyle_series()?;
        let champ = &self.profile.championships;
        let course = champ.next_course(series, expert);
        let p = champ.progress(series, expert);
        Some(format!(
            "{} · Round {} of {} · Course objectives {}/6 · Total {}/{}",
            self.course_name(course),
            p.round + 1,
            series.rounds[usize::from(expert)],
            champ.course_mask(series, expert, course).count_ones(),
            champ.completed_objectives(series, expert),
            atv_core::freestyle::total_objectives(series)
        ))
    }

    /// A Freestyle championship from the Championship page: Select
    /// Objective for its next course.
    fn freestyle_event(&mut self, key: &str, view: &Viewport) {
        let Some(index) = atv_core::freestyle::SERIES
            .iter()
            .position(|s| s.key == key)
        else {
            return;
        };
        let series = &atv_core::freestyle::SERIES[index];
        let expert = self.champ_expert;
        if !self.profile.championships.freestyle_open(series, expert) {
            self.notice = Some(if expert {
                format!("Win gold in {} on Normal to open Expert.", series.name)
            } else {
                "Win a trophy in Amateur Freestyle to open Pro Freestyle.".into()
            });
            return;
        }
        self.notice = None;
        self.freestyle = Some((index, expert));
        self.objective_course = None;
        self.show("objectives", view);
    }

    /// Select Objective → an objective: ride it on the round's course.
    fn objective_event(&mut self, objective: usize) -> Option<Event> {
        let (series, expert, course) = self.objective_target()?;
        let course = course.to_string();
        let course = course.as_str();
        let o = atv_core::freestyle::course_objectives(course)?
            .into_iter()
            .nth(objective)?;
        if !o.playable(self.objective_courses.iter().any(|c| c == course)) {
            self.notice = Some(format!(
                "{} needs the course's original target set, which is not published.",
                o.description
            ));
            return None;
        }
        if !self.tracks.iter().any(|t| t.id == course) {
            self.notice = Some(format!("{} is not available.", self.course_name(course)));
            return None;
        }
        self.notice = None;
        Some(Event::Objective {
            environment: course.into(),
            quad: self.quad_id,
            rider: self.rider_id,
            series: atv_core::freestyle::SERIES
                .iter()
                .position(|s| s.key == series.key)?,
            expert,
            objective,
        })
    }

    /// Next Course / Finish Championship: the round always advances
    /// (`00322c60`); the last one awards the trophy (`00322440`).
    fn finish_objectives(&mut self, view: &Viewport, settings: &mut Settings) {
        let Some((series, expert)) = self.freestyle_series() else {
            return;
        };
        let mut profile = self.profile.clone();
        let outcome = profile.championships.finish_freestyle_round(series, expert);
        if let Some((_, reward, opened)) = &outcome.finished {
            profile.credit(*reward);
            let mut line = atv_core::freestyle::trophy_line(series, &outcome);
            for key in opened {
                if let Some(s) = key
                    .strip_suffix("-normal")
                    .and_then(atv_core::freestyle::series)
                {
                    line += &format!(" {} opened.", s.name);
                }
                if let Some(s) = key
                    .strip_suffix("-expert")
                    .and_then(atv_core::freestyle::series)
                {
                    line += &format!(" {} Expert opened.", s.name);
                }
            }
            self.notice = Some(line);
        } else {
            self.notice = Some(format!(
                "{} round {} of {} done: {} objectives completed.",
                series.name,
                outcome.round + 1,
                outcome.rounds,
                outcome.completed
            ));
        }
        crate::settings::save_profile(settings, &profile);
        self.profile = profile;
        if outcome.finished.is_some() {
            // The series ended: back to the Championship page with the line
            // (`show` clears the notice of the page it leaves).
            let line = self.notice.take();
            self.show("championship", view);
            self.notice = line;
        } else {
            self.refresh(view);
        }
    }

    /// Starts the selected series' next round: five laps against four CPUs,
    /// Expert CPUs on Expert (`00289d50`).
    fn championship_event(&mut self, key: &str, settings: &mut Settings) -> Option<Event> {
        use atv_core::championship::SERIES;
        let index = SERIES.iter().position(|s| s.key == key)?;
        let series = &SERIES[index];
        let expert = self.champ_expert;
        let champ = &self.profile.championships;
        if !champ.is_open(series, expert) {
            self.notice = Some(match (series.opened_by, expert) {
                (_, true) => format!("Win gold in {} on Normal to open Expert.", series.name),
                (Some(am), false) => format!(
                    "Win a trophy in {} to open {}.",
                    atv_core::championship::series(am).map_or(am, |s| s.name),
                    series.name
                ),
                (None, false) if series.kind == 8 => {
                    "Locked. The original opens Short Tracks only with a cheat.".into()
                }
                (None, false) => "Locked.".into(),
            });
            return None;
        }
        let course = champ.next_course(series, expert);
        if !self.race_courses.iter().any(|c| c == course) {
            self.notice = Some(format!("{} is not available.", self.course_name(course)));
            return None;
        }
        self.notice = self.enforce_unlocks(settings);
        Some(Event::Race {
            environment: course.into(),
            quad: self.quad_id,
            rider: self.rider_id,
            laps: series.laps,
            cpu: 4,
            difficulty: if expert { 2 } else { 1 },
            cheats: (false, false),
            championship: Some((index, expert)),
        })
    }

    /// A race link opened without play: its setup on the race page.
    pub fn show_race(&mut self, setup: RaceSetup, view: &Viewport) {
        self.race = setup;
        if !self.race_courses.is_empty() && !self.race_courses.contains(&self.race.course) {
            self.race.course = self.race_courses[0].clone();
        }
        self.race_filter = "all".into();
        self.show("race", view);
    }

    /// The page's text fields (ui_menu `create-lobby` / `join-lobby`).
    pub fn fields(&self) -> Vec<Field> {
        let name = Field {
            action: "player-name",
            label: "Rider name",
            value: self.player_name.clone(),
            max_length: 24,
        };
        match self.page.as_str() {
            "profile-name" => vec![Field {
                action: "profile-name",
                label: "Enter Player Profile Save File Name:",
                value: self.profile_ui.name.clone(),
                max_length: atv_core::profile::NAME_MAX,
            }],
            "create-lobby" => vec![
                name,
                Field {
                    action: "create-code",
                    label: "Lobby code (8 letters/numbers, optional)",
                    value: self.create_code.clone(),
                    max_length: 8,
                },
            ],
            "join-lobby" => vec![
                name,
                Field {
                    action: "room-code",
                    label: "Lobby code",
                    value: self.room_code.clone(),
                    max_length: 8,
                },
            ],
            "passkey-entry" => vec![Field {
                action: "passkey-code",
                label: if self.passkey.share {
                    "Enter Share Code:"
                } else {
                    "Enter Pass Key:"
                },
                value: self.passkey.entry.clone(),
                max_length: 40,
            }],
            "store-cheats" => vec![Field {
                action: "cheat",
                label: "Enter Cheat:",
                value: self.store.cheat.clone(),
                max_length: 8,
            }],
            _ => Vec::new(),
        }
    }

    /// Replaces a field's text (the browser's `input` event; the web build's
    /// DOM input and desktop typing both land here). Codes are upper case
    /// letters and digits; names keep at most 24 bytes without controls.
    pub fn set_field(&mut self, action: &str, text: &str) {
        let code = |t: &str| {
            t.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_uppercase())
                .take(8)
                .collect::<String>()
        };
        match action {
            "player-name" => {
                let mut name: String = text.chars().filter(|c| !c.is_control()).collect();
                while name.len() > 24 {
                    name.pop();
                }
                self.player_name = name;
            }
            "profile-name" => {
                self.set_profile_name(text);
                return;
            }
            "passkey-code" => {
                self.set_passkey_entry(text);
                return;
            }
            "create-code" => self.create_code = code(text),
            "room-code" => self.room_code = code(text),
            "cheat" => self.store.cheat = code(text),
            _ => return,
        }
        self.notice = None;
    }

    /// Typed text for the focused field (desktop keyboards).
    pub fn type_text(&mut self, text: &str) -> bool {
        let Some(field) = self.focus.and_then(|i| self.fields().get(i).cloned()) else {
            return false;
        };
        let value = format!("{}{}", field.value, text);
        self.set_field(field.action, &value);
        true
    }

    /// Focus a field (a click on it), or none.
    pub fn focus_field(&mut self, index: Option<usize>) {
        self.focus = index.filter(|&i| i < self.fields().len());
    }

    pub fn choices(&self) -> &[(String, String)] {
        &self.choices
    }

    fn current(&self) -> Option<&str> {
        self.choices.get(self.selection).map(|(a, _)| a.as_str())
    }

    /// Switches to `page` on its first row; [`Frontend::show`] (the
    /// history) decides where Back then leads.
    fn present(&mut self, page: &str, view: &Viewport) {
        self.shown = crate::transition::next_show();
        // frontend.js clears `#menu-notice` with each action.
        if self.page != page {
            self.notice = None;
        }
        self.page = page.into();
        self.course_sheet = false;
        self.vehicle_sheet = false;
        // Only the Split Screen branch's own pages keep two players.
        if matches!(page, "title" | "main" | "single" | "multiplayer" | "split") {
            self.split = false;
        }
        self.selection = 0;
        self.list = Scroller::default();
        self.rows = Scroller::default();
        self.pressed = None;
        self.hovered = None;
        self.reveal = matches!(page, "tracks" | "garage" | "race");
        self.fresh = true;
        // A held button does not also act on the new page.
        self.pad = u32::MAX;
        self.refresh(view);
    }

    /// Visible course order under the event filter.
    pub fn order(&self) -> Vec<usize> {
        (0..self.tracks.len())
            .filter(|&i| self.filter == "all" || course_group(&self.tracks[i].id) == self.filter)
            .collect()
    }

    /// Event tabs with at least one course: (key, label, count).
    pub fn tabs(&self) -> Vec<(String, String, usize)> {
        GROUPS
            .iter()
            .filter_map(|(key, label)| {
                let count = if *key == "all" {
                    self.tracks.len()
                } else {
                    self.tracks
                        .iter()
                        .filter(|t| course_group(&t.id) == *key)
                        .count()
                };
                (count > 0).then(|| (key.to_string(), label.to_string(), count))
            })
            .collect()
    }

    fn set_filter(&mut self, filter: &str) {
        self.filter = filter.into();
        let order = self.order();
        if !order.is_empty() && !order.contains(&self.track) {
            self.track = order[0];
        }
        self.list.reset();
        self.reveal = true;
    }

    fn cycle_filter(&mut self, direction: i32) {
        let tabs = self.tabs();
        if tabs.is_empty() {
            return;
        }
        let at = tabs
            .iter()
            .position(|(k, _, _)| *k == self.filter)
            .unwrap_or(0) as i32;
        let next = tabs[(at + direction).rem_euclid(tabs.len() as i32) as usize]
            .0
            .clone();
        self.set_filter(&next);
    }

    fn step_track(&mut self, direction: i32) {
        let order = self.order();
        if order.is_empty() {
            return;
        }
        let at = order.iter().position(|&i| i == self.track).unwrap_or(0) as i32 + direction;
        self.track = order[at.rem_euclid(order.len() as i32) as usize];
        self.reveal = true;
    }

    /// Multiplayer garages (from its menus, or Change ATV and Rider over a
    /// room) list ATVs only (frontend.js `onlineGarage`).
    pub fn online_garage(&self) -> bool {
        matches!(
            self.garage_return.as_str(),
            "multiplayer" | "create-lobby" | "join-lobby"
        ) || self.room == Some("garage")
    }

    fn step_quad(&mut self, direction: i32) {
        let n = if self.online_garage() {
            26
        } else {
            i32::from(VEHICLES)
        };
        let online = self.online_garage();
        let mut quad = self.quad_id;
        // Single Player skips ATVs the profile has not unlocked.
        for _ in 0..n {
            quad = ((i32::from(quad) - 1 + direction).rem_euclid(n) + 1) as u16;
            if online || self.quad_owned(quad) {
                break;
            }
        }
        self.quad_id = quad;
        self.reveal = true;
    }

    /// The remembered rider name, or `Rider`.
    fn room_name(&self) -> String {
        if valid_name(&self.player_name) {
            self.player_name.trim().to_string()
        } else {
            "Rider".into()
        }
    }

    fn ride(&mut self, settings: &mut Settings) -> Option<Event> {
        let track = self.tracks.get(self.track)?.clone();
        self.save(settings);
        if self.room == Some("tracks") {
            // The host's Select Track over a room: move everyone there.
            self.room = None;
            return Some(self.room_ride(Some(track.id)));
        }
        if self.hosting && self.host_freestyle {
            return Some(Event::FreestyleHost {
                environment: track.id,
                code: Some(self.create_code.clone()).filter(|c| !c.is_empty()),
                quad: self.quad_id,
                rider: self.rider_id,
                name: self.room_name(),
            });
        }
        if self.hosting {
            // Select Track refused a motorcycle, as frontend.js does.
            return Some(Event::Host {
                environment: track.id,
                code: Some(self.create_code.clone()).filter(|c| !c.is_empty()),
                quad: self.quad_id,
                rider: self.rider_id,
                name: self.room_name(),
            });
        }
        // Single Player Freestyle keeps Tracks.dat's course unlocks.
        if let Some(cost) = self.freestyle_cost(&track.id) {
            self.notice = Some(Self::locked_course(cost));
            return None;
        }
        self.notice = self.enforce_unlocks(settings);
        self.remember_freestyle(settings);
        Some(Event::Ride {
            environment: track.id,
            quad: self.quad_id,
            rider: self.rider_id,
        })
    }

    /// Opens the loader for a course (frontend.js `beginLoading`).
    pub fn begin_loading(&mut self, environment: &str, view: &Viewport) {
        let name = self
            .tracks
            .iter()
            .find(|t| t.id == environment)
            .map_or(environment.to_uppercase(), |t| t.name.clone());
        self.show("loading", view);
        self.loading = Some(Loading {
            environment: environment.into(),
            track: name,
            status: "Preparing course…".into(),
            progress: 0.,
            tip: Vec::new(),
            championship: Vec::new(),
            fame: Vec::new(),
        });
    }

    /// The loading round's championship briefing (`002591f0`).
    pub fn set_briefing(&mut self, lines: Vec<String>) {
        if let Some(loading) = &mut self.loading {
            loading.championship = lines;
        }
    }

    /// The loading race's Hall of Fame lines (`002585c0`).
    pub fn set_fame_briefing(&mut self, lines: Vec<String>) {
        if let Some(loading) = &mut self.loading {
            loading.fame = lines;
        }
    }

    /// A loader progress line; the bar never moves backwards.
    pub fn loading_stage(&mut self, text: &str) {
        if let Some(loading) = &mut self.loading {
            let (status, progress) = loading_stage(text);
            loading.status = status.into();
            loading.progress = loading.progress.max(progress);
        }
    }

    fn back(&mut self, view: &Viewport) -> Option<Event> {
        // Over a held room: Done/Back returns to the ride.
        if self.room == Some(self.page.as_str())
            && matches!(self.page.as_str(), "garage" | "tracks")
        {
            self.room = None;
            return Some(self.room_ride(None));
        }
        if self.page == "lobby" {
            // frontend.js: Back on the lobby page is Leave Game.
            return self.leave_lobby(view);
        }
        let from = self.page.clone();
        self.focus = None;
        // The page the player came from, on the row they left it by.
        self.go_back(view);
        match from.as_str() {
            "tracks" => self.hosting = false,
            "profile-confirm" => self.profile_ui.confirm = None,
            "passkey-confirm" => self.passkey.pending = None,
            _ => {}
        }
        None
    }

    /// Host/Join: the Free Ride Lobby page over the room's course while
    /// the room connects; the loader follows once it has.
    pub fn show_lobby(&mut self, code: &str, environment: &str, view: &Viewport) {
        if let Some(index) = self.tracks.iter().position(|t| t.id == environment) {
            self.track = index;
        }
        self.lobby = Some(Lobby {
            code: code.into(),
            status: String::new(),
        });
        self.notice = None;
        self.focus = None;
        self.show("lobby", view);
    }

    fn leave_lobby(&mut self, view: &Viewport) -> Option<Event> {
        self.lobby = None;
        self.show("multiplayer", view);
        Some(Event::LeaveLobby)
    }

    fn room_ride(&self, environment: Option<String>) -> Event {
        Event::RoomRide {
            quad: if self.quad_id >= 27 { 1 } else { self.quad_id },
            rider: self.rider_id,
            environment,
        }
    }

    fn run(&mut self, action: &str, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        if self.store_run(action, view, settings) {
            return None;
        }
        if let Some(event) = self.passkey_action(action, view, settings) {
            return event;
        }
        if let Some(event) = self.profile_action(action, view, settings) {
            return event;
        }
        if self.page == "splitseats"
            && let Some(event) = self.seat_action(action, 1, view, settings)
        {
            return event;
        }
        match action {
            "main" | "single" | "tracks" | "multiplayer" | "race" | "gear" | "split" => {
                self.show(action, view)
            }
            "splitfree" | "splitrace" | "splitgames" => {
                self.split = true;
                self.show(
                    match action {
                        "splitrace" => "race",
                        "splitgames" => "games",
                        _ => "tracks",
                    },
                    view,
                );
            }
            // Split Screen's player count: 2, 3 or 4 (the manual's
            // multitap split screen).
            "splitplayers" => {
                self.split_players = self.split_players % 4 + 1;
                if self.split_players < 2 {
                    self.split_players = 2;
                }
                settings.set(SPLIT_PLAYERS_KEY, &self.split_players.to_string());
                self.refresh(view);
            }
            // Players 2-4's own garage (the original seats each player at
            // a pad with its own ATV and rider choice).
            "split2quad" | "split2rider" | "split3quad" | "split3rider" | "split4quad"
            | "split4rider" => {
                let n = action.as_bytes()[5] - b'0';
                let index = usize::from(n) - 2;
                let (mut quad, mut rider) = self.split_seconds[index];
                if action.ends_with("rider") {
                    rider = 1 - rider.min(1);
                } else {
                    for _ in 0..SPLIT_QUADS {
                        quad = quad % SPLIT_QUADS + 1;
                        if self.quad_owned(quad) {
                            break;
                        }
                    }
                }
                self.split_seconds[index] = (quad, rider);
                settings.set(&split_player_key(n), &format!("{quad},{rider}"));
                self.refresh(view);
            }
            // Players 2-4's own gear and paint (each original player
            // dresses in its own Garage).
            "split2gear" | "split2paint" | "split3gear" | "split3paint" | "split4gear"
            | "split4paint" => {
                let n = action.as_bytes()[5] - b'0';
                let index = usize::from(n) - 2;
                let mut look = self.split_looks[index];
                if action.ends_with("paint") {
                    look.paint = (look.paint + 1) % PAINTS.len() as u8;
                } else {
                    look = self.step_split_gear(look);
                }
                self.split_looks[index] = look;
                settings.set(&split_look_key(n), &split_look_value(&look));
                self.refresh(view);
            }
            "splitlayout" => {
                self.split_vertical = !self.split_vertical;
                settings.set(
                    SPLIT_LAYOUT_KEY,
                    if self.split_vertical {
                        "vertical"
                    } else {
                        "horizontal"
                    },
                );
                self.refresh(view);
            }
            "racestart" => return self.race_event(settings),
            "games" => self.show(action, view),
            "gamestart" => return self.game_event(settings),
            // Championship progress lives in the profile: it needs one loaded.
            "championship" if self.book.current.is_none() => {
                self.notice = Some(NO_PROFILE.into());
            }
            "championship" => self.show(action, view),
            "champdifficulty" => {
                self.champ_expert = !self.champ_expert;
                self.notice = None;
                self.refresh(view);
            }
            _ if action.starts_with("champ:") => {
                return self.championship_event(&action["champ:".len()..], settings);
            }
            _ if action.starts_with("free:") => {
                self.freestyle_event(&action["free:".len()..], view);
            }
            "objective-courses" => {
                self.freestyle = None;
                self.show(action, view);
            }
            _ if action.starts_with("objcourse:") => {
                self.freestyle = None;
                self.objective_course = Some(action["objcourse:".len()..].to_string());
                self.show("objectives", view);
            }
            _ if action.starts_with("obj:") => {
                return self.objective_event(action["obj:".len()..].parse().ok()?);
            }
            "objfinish" => self.finish_objectives(view, settings),
            "copyride" if self.page == "race" => return Some(Event::CopyRaceLink),
            // A tap on the Course row opens the picker (its arrows step).
            "racetrack" if self.page == "race" => self.open_course_sheet(view),
            _ if stepper(action) => self.step(action, 1, view, settings),
            "create-lobby" | "join-lobby" => {
                self.notice = None;
                self.focus = None;
                self.show(action, view);
            }
            // Host Game's Room row: Free Ride or Freestyle.
            "hostmode" => {
                self.host_freestyle = !self.host_freestyle;
                self.refresh(view);
            }
            "share" if self.page == "lobby" => return Some(Event::CopyInvite),
            "leave" if self.page == "lobby" => return self.leave_lobby(view),
            "hosttrack" if self.page == "create-lobby" && self.quad_id >= 27 => {
                self.notice = Some(MOTORCYCLE_ROOM.into());
                return None;
            }
            "join" if self.quad_id >= 27 => {
                self.notice = Some(MOTORCYCLE_ROOM.into());
                return None;
            }
            "hosttrack" => {
                // Host Game → Select Track; its Ride creates the room.
                if !self.create_code.is_empty() && self.create_code.len() != 8 {
                    self.notice = Some(
                        "Choose exactly 8 letters or numbers, or leave the code blank.".into(),
                    );
                    return None;
                }
                // FREERDnn codes belong to the shared Free Ride rooms.
                if atv_game::multiplayer::is_shared_room(&self.create_code) {
                    self.notice = Some("That code is reserved. Choose another.".into());
                    return None;
                }
                self.save(settings);
                self.hosting = true;
                self.focus = None;
                self.show("tracks", view);
            }
            "join" => {
                if self.room_code.len() != 8 {
                    self.notice = Some("Enter the 8-letter or number lobby code.".into());
                    return None;
                }
                self.save(settings);
                // A shared room's code opens Free Ride, which finds a shared
                // room with space (multiplayer.js `host` with `open`).
                if atv_game::multiplayer::is_shared_room(&self.room_code) {
                    return Some(Event::FreeRide {
                        quad: self.quad_id,
                        rider: self.rider_id,
                        name: self.room_name(),
                    });
                }
                return Some(Event::Join {
                    code: self.room_code.clone(),
                    environment: self
                        .tracks
                        .get(self.track)
                        .map_or("env01".into(), |t| t.id.clone()),
                    quad: self.quad_id,
                    rider: self.rider_id,
                    name: self.room_name(),
                });
            }
            "garage" => {
                self.garage_return = self.page.clone();
                if !self.online_garage() {
                    self.notice = self.enforce_unlocks(settings);
                }
                let notice = self.notice.take();
                self.show("garage", view);
                self.notice = notice;
            }
            "back" => return self.back(view),
            "devlog" => return Some(Event::DevLog),
            "downloads" => return Some(Event::Downloads),
            "freeride" => {
                // frontend.js: no setup; motorcycles fall back to ATV01. The
                // current name rides (also one typed but not yet saved); the
                // host numbers a blank or default "Rider" for the session.
                let name = self.room_name();
                return Some(Event::FreeRide {
                    quad: if self.quad_id >= 27 { 1 } else { self.quad_id },
                    rider: self.rider_id,
                    name,
                });
            }
            "training" => {
                // The garage choice rides it under the Single Player unlocks.
                self.notice = self.enforce_unlocks(settings);
                self.save(settings);
                return Some(Event::Training {
                    quad: self.quad_id,
                    rider: self.rider_id,
                });
            }
            "settings" => return Some(Event::Settings),
            "jukebox" => return Some(Event::Jukebox),
            "quadprev" | "quadnext" => {
                self.step_quad(if action == "quadnext" { 1 } else { -1 });
                self.save(settings);
                self.refresh(view);
            }
            // Motorcycles carry the MX rider.
            "riderchange" if self.quad_id < 27 => {
                self.rider_id = 1 - self.rider_id;
                self.save(settings);
                self.refresh(view);
            }
            _ => {}
        }
        None
    }

    /// Keyboard navigation (frontend.js `action`).
    pub fn key(&mut self, code: &str, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        let event = self.key_inner(code, view, settings);
        self.seat_gate(event, view)
    }

    fn key_inner(&mut self, code: &str, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        self.reveal_row = true;
        // An open status popover takes the next key and closes.
        if self.status_open() && !matches!(code, "ShiftLeft" | "ShiftRight") {
            self.close_status();
            return None;
        }
        if matches!(code, "ShiftLeft" | "ShiftRight") {
            self.shift = true;
            return None;
        }
        if self.page == "loading" {
            return None;
        }
        if code == "Tab" {
            self.tab(self.shift);
            return None;
        }
        if self.course_sheet && self.page == "race" {
            return self.sheet_key(code, view, settings);
        }
        if self.vehicle_sheet && self.page == "garage" {
            return self.vehicle_sheet_key(code, view, settings);
        }
        // A focused field owns editing keys; the arrows move on.
        if let Some(index) = self.focus {
            let fields = self.fields();
            match code {
                "Backspace" => {
                    if let Some(field) = fields.get(index) {
                        let mut value = field.value.clone();
                        value.pop();
                        self.set_field(field.action, &value);
                    }
                    return None;
                }
                // frontend.js: Escape in a field still goes back a page.
                "Escape" => self.focus = None,
                "ArrowDown" | "Enter" | "NumpadEnter" => {
                    self.focus = (index + 1 < fields.len()).then_some(index + 1);
                    if self.focus.is_none() {
                        self.selection = 0;
                    }
                    return None;
                }
                "ArrowUp" => {
                    self.focus = Some(index.saturating_sub(1));
                    return None;
                }
                _ => return None,
            }
        }
        let action = match code {
            "ArrowUp" => "up",
            "ArrowDown" => "down",
            "ArrowLeft" => "left",
            "ArrowRight" => "right",
            "Enter" | "Space" | "NumpadEnter" => "accept",
            "Escape" | "Backspace" => "back",
            "PageUp" | "KeyQ" => "prevtab",
            "PageDown" | "KeyE" => "nexttab",
            _ => return None,
        };
        let page = self.page.clone();
        match (page.as_str(), action) {
            ("tracks", "left" | "right" | "up" | "down") => {
                self.step_track(if matches!(action, "left" | "up") {
                    -1
                } else {
                    1
                });
                self.refresh(view);
            }
            ("tracks", "prevtab" | "nexttab") => {
                self.cycle_filter(if action == "prevtab" { -1 } else { 1 });
                self.refresh(view);
            }
            ("tracks", "accept") => return self.ride(settings),
            ("race", "prevtab" | "nexttab") => {
                self.cycle_race_filter(if action == "prevtab" { -1 } else { 1 });
                self.refresh(view);
            }
            // Accepting the Course row opens the course picker.
            ("race", "accept") if self.current() == Some("racetrack") => {
                self.open_course_sheet(view);
            }
            (
                "race" | "gear" | "games" | "profile-name" | "profile-records" | "hall-of-fame"
                | "passkey-entry" | "store-atv" | "store-gear" | "store-games" | "store-tracks"
                | "splitseats",
                "left" | "right",
            ) => {
                // Only value rows step; Start Race, the link and Back do not.
                if let Some(current) = self.current().filter(|a| stepper(a)).map(String::from) {
                    self.step(
                        &current,
                        if action == "left" { -1 } else { 1 },
                        view,
                        settings,
                    );
                }
            }
            ("garage", "left" | "right") => {
                return self.run(
                    if action == "left" {
                        "quadprev"
                    } else {
                        "quadnext"
                    },
                    view,
                    settings,
                );
            }
            (_, "up") if self.selection == 0 && !self.fields().is_empty() => {
                // Up from the first row returns to the last field.
                self.focus = Some(self.fields().len() - 1);
            }
            (_, "up" | "down") => {
                // The garage's vehicle row is one stop for both arrows.
                let order: Vec<usize> = (0..self.choices.len())
                    .filter(|&i| page != "garage" || self.choices[i].0 != "quadnext")
                    .collect();
                if order.is_empty() {
                    return None;
                }
                let current = match self.current() {
                    Some("quadnext") => self
                        .choices
                        .iter()
                        .position(|(a, _)| a == "quadprev")
                        .unwrap_or(self.selection),
                    _ => self.selection,
                };
                let at = order.iter().position(|&i| i == current).unwrap_or(0) as i32
                    + if action == "up" { -1 } else { 1 };
                self.selection = order[at.rem_euclid(order.len() as i32) as usize];
            }
            (_, "back") => return self.back(view),
            // Accepting the garage's vehicle row opens the vehicle picker.
            ("garage", "accept") if matches!(self.current(), Some("quadprev" | "quadnext")) => {
                self.open_vehicle_sheet(view);
            }
            (_, "accept") => {
                let action = self.current()?.to_string();
                return self.run(&action, view, settings);
            }
            _ => {}
        }
        None
    }

    pub fn key_up(&mut self, code: &str) {
        if matches!(code, "ShiftLeft" | "ShiftRight") {
            self.shift = false;
        }
    }

    /// Tab and Shift+Tab walk the page's focus order like the browser's DOM
    /// focus (a focused button takes the selection, menu-screens.js
    /// `selectChoice`): the form fields first, then every choice (rows,
    /// cards, Back), wrapping. Buttons that are not choices (event tabs,
    /// photo arrows) take no selection in the browser either and are skipped.
    pub fn tab(&mut self, back: bool) {
        let fields = self.fields().len();
        let choices = self.choices.len();
        // Stops: fields 0..fields, then choices.
        let stops = fields + choices;
        if stops == 0 {
            return;
        }
        let at = match self.focus {
            Some(index) => index,
            None => fields + self.selection.min(choices.saturating_sub(1)),
        };
        let next = if back {
            (at + stops - 1) % stops
        } else {
            (at + 1) % stops
        };
        if next < fields {
            self.focus = Some(next);
        } else {
            self.focus = None;
            self.selection = next - fields;
        }
    }

    /// Standard-mapping controller edges: D-pad/stick move, A or Start (or
    /// the rebound controller Pause binding, `pause`) accepts, B/Y go back,
    /// LB/RB switch the course tabs.
    pub fn pad(
        &mut self,
        buttons: &[f32],
        axes: &[f32],
        pause: bool,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Event> {
        let mut held = 0u32;
        let mut set = |bit: u32, on: bool| {
            if on {
                held |= 1 << bit;
            }
        };
        let (buttons, axes) = (
            |i: usize| buttons.get(i).copied().unwrap_or(0.),
            |i: usize| axes.get(i).copied().unwrap_or(0.),
        );
        set(0, buttons(0) > 0.5);
        set(2, buttons(9) > 0.5 || pause);
        set(1, buttons(1) > 0.5 || buttons(3) > 0.5);
        set(3, buttons(12) > 0.5 || axes(1) < -0.55);
        set(4, buttons(13) > 0.5 || axes(1) > 0.55);
        set(5, buttons(14) > 0.5 || axes(0) < -0.55);
        set(6, buttons(15) > 0.5 || axes(0) > 0.55);
        set(7, buttons(4) > 0.5);
        set(8, buttons(5) > 0.5);
        // Buttons held when the page opened act only after a release.
        let fresh = if self.pad == u32::MAX {
            0
        } else {
            held & !self.pad
        };
        self.pad = if self.pad == u32::MAX && held != 0 {
            u32::MAX
        } else {
            held
        };
        let mut event = None;
        for (bit, code) in [
            (3, "ArrowUp"),
            (4, "ArrowDown"),
            (5, "ArrowLeft"),
            (6, "ArrowRight"),
            (7, "PageUp"),
            (8, "PageDown"),
            (0, "Enter"),
            (1, "Escape"),
        ] {
            if fresh & 1 << bit != 0 && event.is_none() {
                event = self.key(code, view, settings);
            }
        }
        // Start accepts, except over a held room: there it leaves the
        // garage or course picker for the ride (frontend.js pause action).
        if fresh & 1 << 2 != 0 && event.is_none() {
            let room = self.room == Some(self.page.as_str());
            event = self.key(if room { "Escape" } else { "Enter" }, view, settings);
        }
        event
    }

    fn listed(id: u32) -> bool {
        (COURSE..COURSE + 100).contains(&id) || (VEHICLE..VEHICLE + 100).contains(&id)
    }

    /// Targets drawn inside the scrolling rows column.
    fn in_rows(id: u32) -> bool {
        (ROW..ROW + 100).contains(&id)
            || (STEP_PREV..STEP_NEXT + 10).contains(&id)
            || (FIELD..FIELD + 4).contains(&id)
    }

    fn hit(hits: &[(Rect, u32)], x: f32, y: f32) -> Option<u32> {
        let pane = |pane: u32| hits.iter().find(|(_, id)| *id == pane).map(|(r, _)| *r);
        let (list, rows) = (pane(LIST), pane(ROWS));
        hits.iter()
            .rev()
            // Items scrolled outside their pane are clipped.
            .filter(|(r, id)| {
                r.contains(x, y)
                    && !(Self::listed(*id) && list.is_some_and(|l| !l.contains(x, y)))
                    && !(Self::in_rows(*id) && rows.is_some_and(|l| !l.contains(x, y)))
            })
            .map(|(_, id)| *id)
            .find(|id| !matches!(*id, LIST | ROWS | super::STAGE | super::DETAILS))
    }

    /// Mouse movement: hover, and a row under a moving mouse takes the
    /// selection (menu-screens.js `onpointermove`).
    pub fn hover(&mut self, hits: &[(Rect, u32)], x: f32, y: f32) {
        self.hovered = Self::hit(hits, x, y);
        if let Some(id) = self.hovered
            && (ROW..ROW + 100).contains(&id)
        {
            self.selection = (id - ROW) as usize;
        }
    }

    pub fn press(&mut self, hits: &[(Rect, u32)], pointer: u64, x: f32, y: f32) {
        let inside = |pane: u32| hits.iter().any(|(r, id)| *id == pane && r.contains(x, y));
        // The list sits inside the rows column on Select Track: it wins.
        // An open status popover covers both.
        let scroll = if self.status_open() {
            None
        } else if inside(LIST) {
            Some(Pane::List)
        } else if inside(ROWS) && self.rows.max() > 0. {
            Some(Pane::Rows)
        } else {
            None
        };
        let now = crate::scroll::now();
        let caught = match scroll {
            Some(pane) => self.pane(pane).press(y, now),
            None => false,
        };
        self.pressed = Some(Pressed {
            pointer,
            id: Self::hit(hits, x, y),
            x,
            y,
            moved: caught,
            scroll,
            caught,
        });
    }

    fn pane(&mut self, pane: Pane) -> &mut Scroller {
        match pane {
            Pane::List => &mut self.list,
            Pane::Rows => &mut self.rows,
        }
    }

    /// Touch/mouse drag: lists and overflowing rows follow the finger.
    pub fn drag(&mut self, pointer: u64, x: f32, y: f32) {
        let Some(p) = self.pressed.as_mut().filter(|p| p.pointer == pointer) else {
            return;
        };
        let scroll = p.scroll;
        if (x - p.x).hypot(y - p.y) > 8. {
            p.moved = true;
        }
        if let Some(pane) = scroll
            && self.pane(pane).drag(y, crate::scroll::now())
            && let Some(p) = self.pressed.as_mut()
        {
            // The content follows the finger: this press is a scroll.
            p.moved = true;
        }
    }

    /// Advances list and rows momentum; true while either moves.
    pub fn tick_scroll(&mut self, now: f64) -> bool {
        let list = self.list.tick(now);
        self.rows.tick(now) || list
    }

    /// Scrolling content is moving (frames must keep coming).
    pub fn scrolling(&self) -> bool {
        self.list.active() || self.rows.active()
    }

    pub fn cancel_pointer(&mut self) {
        self.pressed = None;
        self.hovered = None;
        self.list.cancel();
        self.rows.cancel();
    }

    /// A menu over the page takes the pad: a button still held when it
    /// closes (the Back that closed Settings) acts on the page only after a
    /// release.
    pub fn hold_pad(&mut self) {
        self.pad = u32::MAX;
    }

    /// The page lost focus or was hidden (frontend.js `padBlocked`): a pad
    /// button held across it acts only after a release, and held keys end.
    pub fn blur(&mut self) {
        self.cancel_pointer();
        self.pad = u32::MAX;
        self.shift = false;
    }

    pub fn release(
        &mut self,
        hits: &[(Rect, u32)],
        pointer: u64,
        x: f32,
        y: f32,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Event> {
        let event = self.release_inner(hits, pointer, x, y, view, settings);
        self.seat_gate(event, view)
    }

    fn release_inner(
        &mut self,
        hits: &[(Rect, u32)],
        pointer: u64,
        x: f32,
        y: f32,
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Event> {
        let pressed = self.pressed.take().filter(|p| p.pointer == pointer)?;
        if let Some(pane) = pressed.scroll {
            self.pane(pane).release(crate::scroll::now());
            // A drag scrolls; a grab stops a fling. Neither selects.
            if pressed.moved || pressed.caught {
                return None;
            }
        }
        let id = Self::hit(hits, x, y).filter(|id| Some(*id) == pressed.id)?;
        let rect = hits
            .iter()
            .rev()
            .find(|(r, i)| *i == id && r.contains(x, y))
            .map(|(r, _)| *r);
        if self.status_click(id, rect, view) {
            return None;
        }
        self.click(id, view, settings)
    }

    fn click(&mut self, id: u32, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        if (FIELD..FIELD + 4).contains(&id) {
            self.focus_field(Some((id - FIELD) as usize));
            return None;
        }
        self.focus = None;
        match id {
            BACK | DONE if self.course_sheet => self.close_course_sheet(view),
            BACK | DONE if self.vehicle_sheet => self.close_vehicle_sheet(view),
            BACK | DONE => return self.back(view),
            super::PASSKEY => self.open_passkey(view),
            START => return self.race_event(settings),
            SHARE if self.page == "race" => return Some(Event::CopyRaceLink),
            PREV | NEXT if self.page == "race" => {
                if let Some(index) = self.choices.iter().position(|(a, _)| a == "racetrack") {
                    self.selection = index;
                }
                self.step("racetrack", if id == PREV { -1 } else { 1 }, view, settings);
            }
            _ if (STEP_PREV..STEP_PREV + 10).contains(&id)
                || (STEP_NEXT..STEP_NEXT + 10).contains(&id) =>
            {
                let (index, direction) = if id >= STEP_NEXT {
                    ((id - STEP_NEXT) as usize, 1)
                } else {
                    ((id - STEP_PREV) as usize, -1)
                };
                let action = self.choices.get(index)?.0.clone();
                self.selection = index;
                self.step(&action, direction, view, settings);
            }
            RIDE => return self.ride(settings),
            PREV | NEXT => {
                self.step_track(if id == PREV { -1 } else { 1 });
                self.refresh(view);
            }
            QUAD_PREV | QUAD_NEXT => {
                self.selection = self
                    .choices
                    .iter()
                    .position(|(a, _)| a == "quadprev")
                    .unwrap_or(0);
                return self.run(
                    if id == QUAD_PREV {
                        "quadprev"
                    } else {
                        "quadnext"
                    },
                    view,
                    settings,
                );
            }
            RIDER => return self.run("riderchange", view, settings),
            _ if (ROW..ROW + 100).contains(&id) => {
                let index = (id - ROW) as usize;
                let action = self.choices.get(index)?.0.clone();
                self.selection = index;
                // A tap on the garage's vehicle name opens the picker (its
                // arrows step).
                if self.page == "garage" && matches!(action.as_str(), "quadprev" | "quadnext") {
                    self.open_vehicle_sheet(view);
                    return None;
                }
                return self.run(&action, view, settings);
            }
            _ if (TAB..TAB + 10).contains(&id) && self.page == "race" => {
                let key = self.race_tabs().get((id - TAB) as usize)?.0.clone();
                self.set_race_filter(&key);
                self.refresh(view);
            }
            _ if (TAB..TAB + 10).contains(&id) => {
                let key = self.tabs().get((id - TAB) as usize)?.0.clone();
                self.set_filter(&key);
                self.refresh(view);
            }
            _ if (COURSE..COURSE + 100).contains(&id) && self.page == "race" => {
                // frontend.js `selectRaceCourse`: the course row takes focus;
                // picking a card closes the picker.
                self.race.course = self.race_order().get((id - COURSE) as usize)?.clone();
                if let Some(index) = self.choices.iter().position(|(a, _)| a == "racetrack") {
                    self.selection = index;
                }
                if self.course_sheet {
                    self.close_course_sheet(view);
                } else {
                    self.refresh(view);
                }
            }
            _ if (COURSE..COURSE + 100).contains(&id) => {
                let index = *self.order().get((id - COURSE) as usize)?;
                if index == self.track {
                    return self.ride(settings);
                }
                self.track = index;
                self.refresh(view);
            }
            _ if (VEHICLE..VEHICLE + 100).contains(&id) => {
                let quad = (id - VEHICLE) as u16;
                let online = self.online_garage();
                if !online && !self.quad_owned(quad) {
                    self.notice = Some(format!(
                        "{} is locked. Unlock it in Profile Editor → Unlock Items.",
                        self.quad_label(quad)
                    ));
                    return None;
                }
                if self
                    .catalog
                    .iter()
                    .any(|v| v.id == quad && !(online && v.motorcycle))
                {
                    self.quad_id = quad;
                    self.save(settings);
                    if self.vehicle_sheet {
                        self.close_vehicle_sheet(view);
                    } else {
                        self.refresh(view);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Wheel/trackpad: the list when the page has one, else the rows.
    pub fn wheel(&mut self, dy: f32) {
        if self.list.max() > 0. {
            self.list.wheel(dy);
        } else {
            self.rows.wheel(dy);
        }
    }

    /// After layout: the list's and the rows column's scroll ranges, and
    /// the current card or the keyboard-selected row brought into view
    /// (`scrollIntoView({block: 'nearest'})`): at once when a page opens,
    /// gliding when the keys or a controller moved it (unless `animate` is
    /// off: reduced motion). True when the scroll jumped (lay out again).
    pub fn laid_out(&mut self, hits: &[(Rect, u32)], padding_bottom: f32, animate: bool) -> bool {
        let rect = |pane: u32| hits.iter().find(|(_, id)| *id == pane).map(|(r, _)| *r);
        let mut moved = false;
        // The rows column (pages whose rows can outgrow it).
        let reveal_row = std::mem::take(&mut self.reveal_row);
        if let (Some(pane), Some(end)) = (rect(ROWS), rect(ROWS_END)) {
            let before = self.rows.offset();
            let bottom = end.y + before + ROWS_PADDING;
            self.rows
                .set_bounds((bottom - pane.bottom()).max(0.), pane.h);
            if reveal_row && let Some(row) = rect(ROW + self.selection as u32) {
                let target = self.rows.target();
                // Rows sit at their layout place minus the drawn offset.
                let (top, bottom) = (row.y + before, row.bottom() + before);
                let margin = 8.;
                if top - margin < target + pane.y {
                    self.rows.glide_to(top - margin - pane.y, animate);
                } else if bottom + margin > target + pane.bottom() {
                    self.rows.glide_to(bottom + margin - pane.bottom(), animate);
                }
            }
            moved |= self.rows.offset() != before;
        } else {
            self.rows.set_bounds(0., 1.);
        }
        let Some(list) = rect(LIST) else {
            self.list.set_bounds(0., 1.);
            return moved;
        };
        let items: Vec<&(Rect, u32)> = hits.iter().filter(|(_, id)| Self::listed(*id)).collect();
        let before = self.list.offset();
        let bottom = items.iter().map(|(r, _)| r.bottom()).fold(list.y, f32::max) + before;
        self.list
            .set_bounds((bottom + padding_bottom - list.bottom()).max(0.), list.h);
        if std::mem::take(&mut self.reveal) {
            let current = if self.page == "garage" {
                VEHICLE + u32::from(self.quad_id)
            } else if self.page == "race" {
                let order = self.race_order();
                COURSE
                    + order
                        .iter()
                        .position(|c| *c == self.race.course)
                        .unwrap_or(0) as u32
            } else {
                let order = self.order();
                COURSE + order.iter().position(|&i| i == self.track).unwrap_or(0) as u32
            };
            // A page that just opened shows its card at once.
            let animate = animate && !std::mem::take(&mut self.fresh);
            if let Some((r, _)) = items.iter().find(|(_, id)| *id == current) {
                let target = self.list.target();
                let (top, bottom) = (r.y + before, r.bottom() + before);
                if top < target + list.y {
                    self.list.glide_to(top - list.y, animate);
                } else if bottom > target + list.bottom() {
                    self.list.glide_to(bottom - list.bottom(), animate);
                }
            }
        }
        moved || self.list.offset() != before
    }

    /// The page's draw state with its artwork.
    #[allow(clippy::too_many_arguments)]
    pub fn screen(
        &self,
        backdrop: Option<Img>,
        backdrop_track: bool,
        logo: Option<(Img, Img)>,
        photo: &dyn Fn(&str) -> Option<Img>,
        stats: Option<Stats>,
        shot: (Option<Img>, Option<(Img, f32)>),
        pulse: f32,
    ) -> Screen {
        let order = self.order();
        let track = self.tracks.get(self.track);
        let race = self.page == "race";
        Screen {
            page: self.page.clone(),
            choices: self.choices.clone(),
            selection: self.selection,
            hovered: self.hovered,
            freeride_riders: self.freeride_riders,
            fields: {
                let fields = self.fields();
                fields
                    .into_iter()
                    .enumerate()
                    .map(|(i, f)| (f.label.to_string(), f.value, self.focus == Some(i)))
                    .collect()
            },
            typing: self.soft_keyboard && self.focus.is_some(),
            notice: self.notice.clone(),
            profile: self.profile_text(),
            details: self.profile_details(),
            // The status bar's save chip replaced the Pass Key badge.
            badge: None,
            status: self.status_bar(),
            toast: None,
            qr: self.passkey_qr(),
            heading: if self.course_sheet && self.page == "race" {
                Some("Choose Course".into())
            } else if self.vehicle_sheet && self.page == "garage" {
                Some("Choose Vehicle".into())
            } else {
                self.profile_heading()
            },
            course_sheet: self.course_sheet && self.page == "race",
            vehicle_sheet: self.vehicle_sheet && self.page == "garage",
            lobby: self.lobby.clone(),
            motion: None,
            backdrop,
            backdrop_track,
            logo: logo.map(|l| l.0),
            logo_shadow: logo.map(|l| l.1),
            courses: if self.page == "tracks" {
                order
                    .iter()
                    .map(|&i| {
                        let t = &self.tracks[i];
                        (t.id.clone(), t.name.clone(), photo(&t.id))
                    })
                    .collect()
            } else if race {
                self.race_order()
                    .into_iter()
                    .map(|id| {
                        let name = self.course_name(&id);
                        let image = photo(&id);
                        (id, name, image)
                    })
                    .collect()
            } else {
                Vec::new()
            },
            filter: if race {
                self.race_filter.clone()
            } else {
                self.filter.clone()
            },
            tabs: if race { self.race_tabs() } else { self.tabs() },
            current: if race {
                Some(self.race.course.clone())
            } else {
                track.map(|t| t.id.clone())
            },
            hero: if race {
                photo(&self.race.course)
            } else {
                track.and_then(|t| photo(&t.id))
            },
            // The hero names the venue; the mode line names an Enduro or
            // Short Track race.
            hero_name: if race {
                self.course_name(atv_game::launch::race_venue(&self.race.course))
            } else {
                track.map_or(String::new(), |t| t.name.clone())
            },
            hero_mode: if race {
                match atv_game::launch::split_race_course(&self.race.course)
                    .and_then(|(_, variant)| variant.label())
                {
                    Some(label) => format!("{label} race"),
                    None => format!("{} race", group_label(&self.race.course)),
                }
            } else {
                String::new()
            },
            list_scroll: self.list.offset(),
            rows_scroll: self.rows.offset(),
            vehicle_label: self.quad_label(self.stage_quad()),
            rider: self.rider_id,
            catalog: if self.page == "garage" {
                let online = self.online_garage();
                self.catalog
                    .iter()
                    .filter(|v| !online || !v.motorcycle)
                    .map(|v| {
                        let mut v = v.clone();
                        if !online && !self.quad_owned(v.id) {
                            v.label += " (Locked)";
                        }
                        v
                    })
                    .collect()
            } else {
                Vec::new()
            },
            quad_id: self.quad_id,
            stats,
            loading_track: self
                .loading
                .as_ref()
                .map_or(String::new(), |l| l.track.clone()),
            tip: self.loading.as_ref().map_or(Vec::new(), |l| l.tip.clone()),
            briefing: self
                .loading
                .as_ref()
                .map_or(Vec::new(), |l| l.championship.clone()),
            fame: self.loading.as_ref().map_or(Vec::new(), |l| l.fame.clone()),
            progress: self.loading.as_ref().map_or(0., |l| l.progress),
            shot: shot.0,
            previous_shot: shot.1,
            shot_fade: 1.,
            pulse,
            quad_model: None,
            rider_model: None,
            model_status: [None, None],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn tracks() -> Vec<Track> {
        ["env01", "env02", "free01", "nat01", "sx01"]
            .iter()
            .map(|id| Track {
                id: id.to_string(),
                name: id.to_uppercase(),
                image: String::new(),
            })
            .collect()
    }

    pub(super) fn view(width: f32, height: f32) -> Viewport {
        Viewport {
            width,
            height,
            ..Default::default()
        }
    }

    #[test]
    fn host_and_join_forms_take_text_and_validate_codes() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.key("Enter", &view, &mut settings);
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "multiplayer")
            .unwrap();
        front.selection = at;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "multiplayer");
        // Split Screen leads; Host Game is the next row.
        front.key("ArrowDown", &view, &mut settings);
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "create-lobby");
        assert_eq!(front.fields().len(), 2);
        // The fields come before the rows in the focus order: Shift+Tab from
        // the first row reaches the code, then the name. Typing edits it,
        // Backspace deletes.
        front.key("ShiftLeft", &view, &mut settings);
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.focus, Some(1));
        front.key("Tab", &view, &mut settings);
        front.key_up("ShiftLeft");
        assert_eq!(front.focus, Some(0));
        front.set_field("player-name", "");
        assert!(front.type_text("Ann"));
        front.key("Backspace", &view, &mut settings);
        assert_eq!(front.player_name, "An");
        front.key("Tab", &view, &mut settings);
        assert!(front.type_text("ab12-cd34xyz"));
        assert_eq!(front.create_code, "AB12CD34");
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.focus, None);
        // Select Track, then Ride creates the room on that course.
        front.key("Enter", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.hosting), ("tracks", true));
        let event = front.key("Enter", &view, &mut settings);
        assert!(matches!(event, Some(Event::Host { ref code, ref name, .. })
            if code.as_deref() == Some("AB12CD34") && name == "An"));
        // The name is remembered with the garage choice.
        let saved: Value = serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap();
        assert_eq!(saved["playerName"], "An");
        // Join needs the full code.
        front.key("Escape", &view, &mut settings);
        front.key("Escape", &view, &mut settings);
        front.show("join-lobby", &view);
        front.set_field("room-code", "abc");
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert!(front.notice.is_some());
        front.set_field("room-code", "abcd1234");
        assert!(matches!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Join { ref code, .. }) if code == "ABCD1234"
        ));
        // A shared Free Ride code joins Free Ride instead.
        front.set_field("room-code", "freerd07");
        assert!(matches!(
            front.key("Enter", &view, &mut settings),
            Some(Event::FreeRide { .. })
        ));
        // Hosting one is refused.
        front.key("Escape", &view, &mut settings);
        front.show("create-lobby", &view);
        front.set_field("create-code", "FREERD01");
        assert_eq!(front.run("hosttrack", &view, &mut settings), None);
        assert_eq!(
            front.notice.as_deref(),
            Some("That code is reserved. Choose another.")
        );
        assert_eq!(front.page, "create-lobby");
    }

    #[test]
    fn host_game_room_freestyle_hosts_a_freestyle_room() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("create-lobby", &view);
        let rows: Vec<&str> = front.choices().iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(&rows[..3], ["hosttrack", "hostmode", "garage"]);
        assert_eq!(front.choices()[1].1, "Room: Free Ride");
        front.run("hostmode", &view, &mut settings);
        assert!(front.host_freestyle);
        assert_eq!(front.choices()[1].1, "Room: Freestyle (timed stunts)");
        front.set_field("create-code", "fs12ab34");
        front.run("hosttrack", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.hosting), ("tracks", true));
        let event = front.key("Enter", &view, &mut settings);
        assert!(
            matches!(event, Some(Event::FreestyleHost { ref code, ref environment, .. })
                if code.as_deref() == Some("FS12AB34") && environment == "env01"),
            "{event:?}"
        );
        // Back to Free Ride: an ordinary hosted room.
        front.key("Escape", &view, &mut settings);
        front.run("hostmode", &view, &mut settings);
        front.run("hosttrack", &view, &mut settings);
        assert!(matches!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Host { .. })
        ));
    }

    #[test]
    fn escape_in_a_field_goes_back_and_start_leaves_a_room_garage() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        // Opened from a link: Back climbs to its canonical parent.
        front.open("join-lobby", &view);
        front.tab(true);
        assert!(front.focus.is_some());
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.focus), ("multiplayer", None));
        // Over a held room, Start leaves the garage for the ride.
        front.show("garage", &view);
        front.room = Some("garage");
        let pad = |held: &[usize]| {
            let mut buttons = [0.; 17];
            for b in held {
                buttons[*b] = 1.;
            }
            buttons
        };
        front.pad(&pad(&[]), &[0.; 4], false, &view, &mut settings);
        let event = front.pad(&pad(&[9]), &[0.; 4], false, &view, &mut settings);
        assert!(matches!(event, Some(Event::RoomRide { .. })), "{event:?}");
    }

    #[test]
    fn a_race_links_cheats_last_through_start_race() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.set_race_courses(vec!["env01".into(), "nat01".into()], &view);
        front.show_race(
            RaceSetup {
                course: "env01".into(),
                laps: 2,
                cpu: 3,
                difficulty: 1,
                cheats: (true, true),
            },
            &view,
        );
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "racelaps")
            .unwrap();
        front.selection = at;
        front.key("ArrowRight", &view, &mut settings);
        let event = front.race_event(&mut settings);
        assert!(
            matches!(
                event,
                Some(Event::Race {
                    laps: 3,
                    cheats: (true, true),
                    ..
                })
            ),
            "{event:?}"
        );
        // The saved setup never keeps them (normalizedRace).
        let saved: Value = serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap();
        assert!(saved["race"].get("aggressive").is_none());
    }

    #[test]
    fn rooms_refuse_motorcycles_and_online_garages_list_atvs() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        settings.set(SELECTION_KEY, r#"{"quadId":29,"riderId":0}"#);
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("create-lobby", &view);
        assert_eq!(front.run("hosttrack", &view, &mut settings), None);
        assert_eq!(front.page, "create-lobby");
        assert_eq!(front.notice.as_deref(), Some(MOTORCYCLE_ROOM));
        front.notice = None;
        front.show("join-lobby", &view);
        front.set_field("room-code", "abcd1234");
        assert_eq!(front.run("join", &view, &mut settings), None);
        assert_eq!(front.notice.as_deref(), Some(MOTORCYCLE_ROOM));
        // The garage opened from the Multiplayer menus lists ATVs only, and
        // stepping wraps within ATVs 1-26.
        let catalog = |front: &Frontend| {
            front
                .screen(None, false, None, &|_| None, None, (None, None), 0.)
                .catalog
        };
        front.show("multiplayer", &view);
        front.run("garage", &view, &mut settings);
        assert_eq!(front.page, "garage");
        assert!(front.online_garage());
        assert!(!catalog(&front).is_empty());
        assert!(catalog(&front).iter().all(|v| !v.motorcycle));
        front.click(VEHICLE + 30, &view, &mut settings);
        assert_eq!(front.quad_id, 29);
        front.click(VEHICLE + 3, &view, &mut settings);
        assert_eq!(front.quad_id, 3);
        front.quad_id = 26;
        front.step_quad(1);
        assert_eq!(front.quad_id, 1);
        // Single Player's garage keeps the motorcycles.
        front.show("single", &view);
        front.run("garage", &view, &mut settings);
        assert!(!front.online_garage());
        assert!(catalog(&front).iter().any(|v| v.motorcycle));
    }

    #[test]
    fn lobby_page_shows_while_joining_and_back_leaves() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        let env = front.tracks[1].id.clone();
        front.show_lobby("ABCD1234", &env, &view);
        assert_eq!(front.page, "lobby");
        assert_eq!(front.tracks[front.track].id, env);
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(actions, ["share", "leave"]);
        let screen = front.screen(None, false, None, &|_| None, None, (None, None), 0.);
        assert_eq!(
            screen.lobby.as_ref().map(|l| l.code.as_str()),
            Some("ABCD1234")
        );
        assert_eq!(
            front.run("share", &view, &mut settings),
            Some(Event::CopyInvite)
        );
        // Back is Leave Game: the Multiplayer page again.
        assert_eq!(
            front.key("Escape", &view, &mut settings),
            Some(Event::LeaveLobby)
        );
        assert_eq!(
            (front.page.as_str(), front.lobby.is_none()),
            ("multiplayer", true)
        );
        front.show_lobby("", &env, &view);
        assert_eq!(
            front.run("leave", &view, &mut settings),
            Some(Event::LeaveLobby)
        );
        assert_eq!(front.page, "multiplayer");
    }

    #[test]
    fn training_course_row_rides_the_training_venue() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("main", &view);
        assert!(front.choices().iter().all(|(a, _)| a != "training"));
        let mut list = tracks();
        list.push(Track {
            id: "training".into(),
            name: "Training Course".into(),
            image: String::new(),
        });
        let mut front = Frontend::new(list, &settings, true);
        front.show("main", &view);
        let choices: Vec<&str> = front.choices().iter().map(|(a, _)| a.as_str()).collect();
        let at = choices.iter().position(|a| *a == "training").unwrap();
        assert_eq!(choices[at + 1], "profile");
        assert_eq!(front.choices()[at].1, "Training Course");
        assert!(matches!(
            front.run("training", &view, &mut settings),
            Some(Event::Training { .. })
        ));
    }

    #[test]
    fn free_ride_needs_no_setup_and_rides_an_atv() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        settings.set(
            SELECTION_KEY,
            r#"{"quadId":29,"riderId":1,"playerName":"Ann"}"#,
        );
        let mut front = Frontend::new(tracks(), &settings, true);
        front.key("Enter", &view, &mut settings);
        // Main Menu → Multiplayer → Arlington Free Ride (the original's
        // Online row, last).
        front.key("ArrowDown", &view, &mut settings);
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "multiplayer");
        assert_eq!(front.choices()[3].0, "freeride");
        for _ in 0..3 {
            front.key("ArrowDown", &view, &mut settings);
        }
        assert_eq!(
            front.key("Enter", &view, &mut settings),
            Some(Event::FreeRide {
                quad: 1,
                rider: 1,
                name: "Ann".into()
            })
        );
    }

    #[test]
    fn navigates_title_to_a_ride() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        assert_eq!(front.choices()[0].0, "main");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "main");
        assert!(
            front
                .choices()
                .iter()
                .all(|(a, _)| !UNSUPPORTED.contains(&a.as_str()))
        );
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "single");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "tracks");
        front.key("ArrowRight", &view, &mut settings);
        front.key("PageDown", &view, &mut settings);
        assert_eq!(front.filter, "env");
        assert_eq!(front.tracks[front.track].id, "env02");
        let event = front.key("Enter", &view, &mut settings);
        assert_eq!(
            event,
            Some(Event::Ride {
                environment: "env02".into(),
                quad: 1,
                rider: 0
            })
        );
        let saved: Value = serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap();
        assert_eq!(saved["freestyleEnvironment"], "env02");
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.selection), ("single", 0));
    }

    #[test]
    fn garage_cycles_atvs_and_keeps_other_saved_fields() {
        let view = view(390., 844.);
        let mut settings = Settings::memory();
        settings.set(
            SELECTION_KEY,
            r#"{"quadId":46,"riderId":1,"playerName":"Sam"}"#,
        );
        let mut front = Frontend::new(tracks(), &settings, false);
        assert!(
            front
                .choices()
                .iter()
                .all(|(a, _)| a != "devlog" && a != "downloads")
        );
        front.show("single", &view);
        let garage = front
            .choices()
            .iter()
            .position(|(a, _)| a == "garage")
            .unwrap();
        front.selection = garage;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "garage");
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.quad_id, 1);
        front.key("ArrowDown", &view, &mut settings);
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.rider_id, 0);
        let saved: Value = serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap();
        assert_eq!(
            (saved["quadId"].as_u64(), saved["riderId"].as_u64()),
            (Some(1), Some(0))
        );
        assert_eq!(saved["playerName"], "Sam");
        // Motorcycles keep the MX rider: the switch does nothing.
        front.key("ArrowLeft", &view, &mut settings);
        assert_eq!(front.quad_id, 46);
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.rider_id, 0);
        assert!(
            front
                .catalog
                .iter()
                .any(|v| v.motorcycle && !v.class.is_empty())
        );
        // Back returns to the row that opened the page (the original's
        // page history, MENU_FLOW.md).
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "single");
        assert_eq!(front.current(), Some("garage"));
    }

    fn store_catalogs(front: &mut Frontend) {
        let quads = "[General]\nNumQuads=3\n[Quad1]\nName=\"Honda 400EX Yellow\"\nAccess=1\nIngameID=22\nPoints=1000\n\
            [Quad2]\nName=\"Patriot\"\nAccess=0\nIngameID=8\nPoints=250\n\
            [Quad3]\nName=\"G Ride\"\nAccess=0\nIngameID=19\nPoints=1000\n";
        front.store.atvs = atv_core::store::parse_atvs(quads.as_bytes()).unwrap();
        let tracks = "[unspnat1]\nscn=env05\nname=\"Prospect Springs\"\naccess=1\npoints=0\n\
            [unspnat2]\nscn=env02\nname=\"Tioga Valley\"\naccess=0\npoints=200\n\
            [unspstunt1]\nscn=env01\nname=\"Caribou Peak\"\naccess=1\npoints=0\n\
            [spstunt1]\nscn=free01\nname=\"Tempe\"\naccess=0\npoints=250\n";
        front.store.courses = atv_core::store::parse_courses(tracks.as_bytes()).unwrap();
        front.catalog = [
            (22, "Honda 400EX Yellow"),
            (8, "Patriot"),
            (19, "G Ride"),
            (27, "MX"),
        ]
        .iter()
        .map(|(id, label)| super::super::Vehicle {
            id: *id,
            label: label.to_string(),
            motorcycle: *id >= 27,
            class: String::new(),
        })
        .collect();
    }

    fn row(front: &Frontend, action: &str) -> usize {
        front
            .choices()
            .iter()
            .position(|(a, _)| a == action)
            .unwrap_or_else(|| panic!("{action}: {:?}", front.choices()))
    }

    #[test]
    fn unlock_everything_opens_the_whole_save_and_saves_it() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        store_catalogs(&mut front);
        // Two profiles; the second is loaded.
        let mut book = atv_core::profile::ProfileBook::migrate(None);
        book.create("ACE").unwrap();
        crate::settings::save_book(&mut settings, &book);
        front.load_profiles(&settings);
        let pro = atv_core::championship::series("prosx").unwrap();
        let free = atv_core::freestyle::series("spstunt").unwrap();
        assert!(!front.quad_owned(19) && !front.quad_owned(8));
        assert_eq!(front.race_course_cost("env02"), Some(200));
        assert!(!front.profile.championships.is_open(pro, true));
        front.games.kind = atv_core::games::GameKind::ALL
            .iter()
            .position(|k| k.event_type() == 12)
            .unwrap();
        assert!(front.game_lock().is_some());
        // Profile Editor → Unlock Everything, then Cancel.
        front.show("profile", &view);
        front.selection = row(&front, "profile-unlock");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "profile-unlock");
        let actions: Vec<&str> = front.choices().iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(actions, ["unlockthis", "unlockall", "unlockcancel", "back"]);
        assert_eq!(front.choices()[0].1, "Unlock For ACE");
        assert_eq!(front.choices()[1].1, "Unlock For All 2 Profiles");
        assert!(
            front.profile_details()[0]
                .0
                .contains("records and trophies stay")
        );
        front.selection = 2;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "profile");
        assert_eq!(front.current(), Some("profile-unlock"));
        assert!(!front.quad_owned(19));
        // This profile only.
        front.key("Enter", &view, &mut settings);
        front.selection = 0;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "profile");
        assert_eq!(front.notice.as_deref(), Some("Unlocked Everything!"));
        assert!(front.quad_owned(19) && front.quad_owned(8));
        assert_eq!(front.race_course_cost("env02"), None);
        assert_eq!(front.freestyle_cost("free01"), None);
        assert!(front.profile.championships.is_open(pro, true));
        assert!(front.profile.championships.freestyle_open(free, true));
        assert!(front.game_lock().is_none());
        assert_eq!(front.profile.points, 0);
        let saved = crate::settings::load_book(&settings);
        assert!(atv_core::store::everything_unlocked(&saved.profiles[1]));
        assert!(!atv_core::store::everything_unlocked(&saved.profiles[0]));
        assert!(atv_core::store::everything_unlocked(
            &crate::settings::load_profile(&settings)
        ));
        // From Unlock Items: every profile; Back returns there.
        front.selection = row(&front, "profile-store");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "store");
        front.selection = row(&front, "profile-unlock");
        front.key("Enter", &view, &mut settings);
        front.selection = row(&front, "unlockall");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "store");
        assert_eq!(front.notice.as_deref(), Some("Unlocked Everything!"));
        let saved = crate::settings::load_book(&settings);
        assert!(
            saved
                .profiles
                .iter()
                .all(atv_core::store::everything_unlocked)
        );
        // Again: nothing left to unlock.
        front.selection = row(&front, "profile-unlock");
        front.key("Enter", &view, &mut settings);
        front.selection = row(&front, "unlockall");
        front.key("Enter", &view, &mut settings);
        assert_eq!(
            front.notice.as_deref(),
            Some("Everything is already unlocked.")
        );
        // Game Settings → New Profiles: profiles made here start unlocked.
        settings.set(crate::settings::NEW_PROFILES_KEY, "on");
        front.show("profile", &view);
        front.run("profile-create", &view, &mut settings);
        front.set_profile_name("NEW");
        front.run("profilesave", &view, &mut settings);
        assert_eq!(front.profile_name(), Some("NEW"));
        assert!(atv_core::store::everything_unlocked(&front.profile));
        // The Pass Key page offers it too.
        front.show("passkey", &view);
        assert!(front.choices().iter().any(|(a, _)| a == "profile-unlock"));
    }

    #[test]
    fn store_sells_unlocks_and_gates_single_player() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        store_catalogs(&mut front);
        front.quad_id = 22;
        // Profile Editor → Unlock Items (the original's entry).
        front.show("profile", &view);
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "profile-store")
            .unwrap();
        front.selection = at;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "store");
        // The points to spend are on the status bar's emphasized coin chip.
        assert_eq!(front.profile_text(), None);
        let bar = front.status_bar().unwrap();
        assert!(bar.store && bar.points == 0);
        // Cheats: GIMMEPTS adds 1,000 points.
        front.selection = 4;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "store-cheats");
        front.set_field("cheat", "gimmepts");
        front.selection = 0;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.notice.as_deref(), Some("Added 1,000 Profile Points!"));
        front.set_field("cheat", "xyz");
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.notice.as_deref(), Some("Invalid cheat!"));
        front.key("Escape", &view, &mut settings);
        // ATVs: the Patriot costs 250.
        front.selection = 0;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "store-atv");
        assert!(
            front.choices()[0]
                .1
                .ends_with("Unlocked: Honda 400EX Yellow")
        );
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.stage_quad(), 8);
        assert_eq!(front.choices()[0].1, "ATV 2/3 · Locked: Patriot");
        assert_eq!(front.profile_text().as_deref(), Some("Point Cost: 250"));
        assert_eq!(front.status_bar().unwrap().points, 1_000);
        // Single Player skips the locked ATVs.
        assert!(!front.quad_owned(8) && front.quad_owned(27) && front.quad_owned(6));
        front.selection = 1;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.notice.as_deref(), Some("Unlocked Patriot!"));
        assert_eq!(atv_core::store::remaining(&front.profile), 750);
        assert!(front.quad_owned(8));
        // The purchase is saved with the profile.
        let saved = crate::settings::load_profile(&settings);
        assert_eq!(saved.store.atvs, [8]);
        assert_eq!(saved.store.spent, 250);
        // Tracks: Switch Event moves to the next event's list.
        front.key("Escape", &view, &mut settings);
        front.selection = 3;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "store-tracks");
        assert!(front.choices()[0].1.contains("Prospect Springs"));
        // A locked course refuses a Single Race.
        front.race.course = "env02".into();
        front.race_courses = vec!["env02".into(), "env05".into()];
        assert_eq!(front.race_event(&mut settings), None);
        assert!(
            front
                .notice
                .as_deref()
                .unwrap()
                .contains("Spend 200 Profile Points")
        );
        front.race.course = "env05".into();
        assert!(front.race_event(&mut settings).is_some());
        // A locked garage choice gives way to the first owned ATV.
        front.quad_id = 19;
        assert!(front.enforce_unlocks(&mut settings).is_some());
        assert_eq!(front.quad_id, 22);
        // View Profile → ATVs lists the collection without purchases.
        front.show("profile-view", &view);
        front.view_collection("store-atv", &view);
        front.key("ArrowRight", &view, &mut settings);
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.choices()[0].1, "ATV 3/3 · Locked: G Ride");
        assert!(front.choices().iter().all(|(a, _)| a != "storebuy"));
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "profile-view");
        // Percent Complete counts the Patriot over the original 201.
        assert_eq!(front.profile.percent_complete(), 100. / 201.);
    }

    #[test]
    fn freestyle_championships_select_objectives_and_advance() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        // Round 2 of Amateur Freestyle (env02), its combos done.
        let key = "unspstunt-normal";
        front.profile.championships.progress.insert(
            key.into(),
            atv_core::championship::Progress {
                round: 1,
                ..Default::default()
            },
        );
        front
            .profile
            .championships
            .objectives
            .insert(format!("{key}:env02"), 0b001100);
        front.show("championship", &view);
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "free:unspstunt")
            .unwrap();
        assert_eq!(
            front.choices()[at].1,
            "Amateur Freestyle · Round 2 of 6 · 2/36"
        );
        assert_eq!(front.choices()[at + 1].1, "Pro Freestyle · Locked");
        front.selection = at;
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert_eq!(front.page, "objectives");
        let labels: Vec<_> = front.choices().iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Hit Every Green Target · Unavailable",
                "Hit Every Red Target · Unavailable",
                "Bust 10 Different Combos · Done",
                "Score Stunt Points · 1 of 3 levels",
                "Next Course",
                "Back",
            ]
        );
        front.selection = 0;
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert!(front.notice.is_some());
        // Its OB archive published, the target run rides.
        front.objective_courses = vec!["env02".into()];
        front.refresh(&view);
        assert_eq!(front.choices()[0].1, "Hit Every Green Target");
        front.selection = 0;
        assert!(matches!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Objective { objective: 0, .. })
        ));
        front.selection = 3;
        assert_eq!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Objective {
                environment: "env02".into(),
                quad: 1,
                rider: 0,
                series: 0,
                expert: false,
                objective: 3,
            })
        );
        // Next Course always advances (00322c60).
        front.selection = 4;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.profile.championships.progress[key].round, 2);
        assert!(
            front
                .objectives_line()
                .unwrap()
                .starts_with("ENV08 · Round 3 of 6")
        );
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "championship");
    }

    #[test]
    fn standalone_objectives_pick_a_course_and_save_its_bits() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("single", &view);
        assert!(
            !front
                .choices()
                .iter()
                .any(|(a, _)| a == "objective-courses")
        );
        front.objective_courses = vec!["env01".into(), "free01".into(), "env04".into()];
        front
            .profile
            .championships
            .objectives
            .insert("spstunt-normal:free01".into(), 0b11);
        front.refresh(&view);
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "objective-courses")
            .unwrap();
        assert_eq!(front.choices()[at - 1].0, "tracks");
        front.selection = at;
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert_eq!(front.page, "objective-courses");
        // Courses with an OB archive and a published venue (env04 has
        // none here), Amateur first.
        let rows: Vec<_> = front.choices().to_vec();
        assert_eq!(
            rows,
            [
                ("objcourse:env01".to_string(), "ENV01 · 0/6".to_string()),
                ("objcourse:free01".into(), "FREE01 · 2/6".into()),
                ("back".into(), "Back".into()),
            ]
        );
        front.selection = 1;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "objectives");
        let labels: Vec<_> = front.choices().iter().map(|(_, l)| l.as_str()).collect();
        // No round row outside a championship.
        assert_eq!(
            labels,
            [
                "Hit Every Green Target · Done",
                "Hit Every Red Target · Done",
                "Bust 10 Different Combos",
                "Score Stunt Points · 0 of 3 levels",
                "Back",
            ]
        );
        assert_eq!(
            front.objectives_line().unwrap(),
            "FREE01 · Pro course · Course objectives 2/6"
        );
        front.selection = 2;
        assert_eq!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Objective {
                environment: "free01".into(),
                quad: 1,
                rider: 0,
                series: 1,
                expert: false,
                objective: 2,
            })
        );
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "objective-courses");
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "single");
        // A championship later clears the standalone course.
        front.freestyle_event("unspstunt", &view);
        assert_eq!(front.objective_course, None);
        assert_eq!(front.page, "objectives");
    }

    #[test]
    fn championship_page_starts_the_next_round() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("single", &view);
        assert!(front.choices().iter().all(|(a, _)| a != "championship"));
        front.set_race_courses(
            ["env05", "env05ends", "env02", "sx08", "nat01"]
                .map(String::from)
                .to_vec(),
            &view,
        );
        let choices: Vec<_> = front.choices().iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(&choices[..4], ["race", "tracks", "championship", "garage"]);
        front.selection = 2;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "championship");
        assert_eq!(
            front.choices()[0].1,
            "Amateur Enduro · Round 1 of 5 · 0 pts"
        );
        assert_eq!(front.choices()[1].1, "Pro Enduro · Locked");
        assert_eq!(
            front.choices()[2].1,
            "Amateur Nationals · Round 1 of 5 · 0 pts"
        );
        assert_eq!(front.choices()[3].1, "Pro Nationals · Locked");
        assert_eq!(
            front.series_line().as_deref(),
            Some("Next: ENV05 Enduro Short · Standings: You 0, CPUs 0 / 0 / 0 / 0")
        );
        // Amateur Enduro: env05's short Enduro, three laps, four CPUs.
        assert_eq!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Race {
                environment: "env05ends".into(),
                quad: 1,
                rider: 0,
                laps: 3,
                cpu: 4,
                difficulty: 1,
                cheats: (false, false),
                championship: Some((0, false)),
            })
        );
        // Amateur Nationals: env05, five laps.
        front.selection = 2;
        assert!(matches!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Race {
                laps: 5,
                championship: Some((2, false)),
                ..
            })
        ));
        front.show("championship", &view);
        // A locked series explains how it opens.
        front.selection = 3;
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert!(
            front
                .notice
                .as_deref()
                .unwrap()
                .contains("Amateur Nationals")
        );
        // Expert progress is separate; Back returns to Single Player.
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "champdifficulty")
            .unwrap();
        front.selection = at;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.choices()[at].1, "Difficulty: Expert");
        assert!(front.choices()[0].1.ends_with("Locked"));
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "single");
    }

    #[test]
    fn split_events_wait_on_the_players_page_until_seated() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("split", &view);
        front.split = true;
        front.show("tracks", &view);
        // Start on Freestyle opens Players with the last two players joined.
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert_eq!(front.page, "splitseats");
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.clone()).collect();
        assert_eq!(actions[..3], ["splitgo", "seat1ready", "seat2ready"]);
        assert!(actions.contains(&"seat2helmet".to_string()));
        assert!(actions.contains(&"seat3join".to_string()));
        // Player 3's pad: START joins; its D-pad moves to Rider and steps it.
        let idle = [0f32; 17];
        let press = |front: &mut Frontend, settings: &mut Settings, button: usize| {
            let mut pad = idle;
            pad[button] = 1.;
            let event = front.seat_pad(2, Some((&pad, &[0.; 4])), &view, settings);
            front.seat_pad(2, Some((&idle, &[0.; 4])), &view, settings);
            event
        };
        front.seat_pad(2, Some((&idle, &[0.; 4])), &view, &mut settings);
        press(&mut front, &mut settings, 9);
        assert!(front.seats.joined[2]);
        press(&mut front, &mut settings, 13);
        press(&mut front, &mut settings, 15);
        assert_eq!(
            front.split_seconds[1].1, 1,
            "player 3 rides the female rider"
        );
        // Player 3's helmet steps like Rider Gear's (Original when locked).
        let helmet = front
            .choices()
            .iter()
            .position(|(a, _)| a == "seat3helmet")
            .unwrap();
        front.selection = helmet;
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.seats.cursor[2], 3);
        // Everyone ready: player 1's row, player 2's row, player 3's START.
        for row in ["seat1ready", "seat2ready"] {
            front.selection = front.choices().iter().position(|(a, _)| a == row).unwrap();
            assert_eq!(front.key("Enter", &view, &mut settings), None);
        }
        let event = press(&mut front, &mut settings, 9);
        assert!(matches!(event, Some(Event::Ride { .. })), "{event:?}");
        let (players, seconds, _) = front.split_launch();
        assert_eq!(players, 3);
        assert_eq!(seconds[1].1, 1);
        assert_eq!(settings.get(SPLIT_PLAYERS_KEY).as_deref(), Some("3"));
        // Players 1 and 4 only: player 4 rides seat 2 (pad order compacts).
        front.split_players = 2;
        front.show("tracks", &view);
        front.key("Enter", &view, &mut settings);
        front.seats.joined = [true, false, false, true];
        front.split_seconds[2] = (7, 1);
        front.selection = 0;
        let event = front.key("Enter", &view, &mut settings);
        assert!(matches!(event, Some(Event::Ride { .. })));
        let (players, seconds, _) = front.split_launch();
        assert_eq!((players, seconds[0]), (2, (7, 1)));
    }

    #[test]
    fn split_screen_branch_marks_two_players_until_it_is_left() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.set_race_courses(vec!["env01".into()], &view);
        front.show("multiplayer", &view);
        assert_eq!(front.choices()[0].0, "split");
        front.key("Enter", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("split", false));
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.clone()).collect();
        assert_eq!(
            actions[..5],
            [
                "splitrace",
                "splitfree",
                "splitgames",
                "splitplayers",
                "splitlayout"
            ]
        );
        // The layout row toggles and is saved.
        front.selection = 4;
        front.key("Enter", &view, &mut settings);
        assert!(front.split_vertical);
        assert_eq!(settings.get(SPLIT_LAYOUT_KEY).as_deref(), Some("vertical"));
        assert!(front.choices()[4].1.contains("Side by Side"));
        // Games opens the Games page for two players.
        front.selection = 2;
        front.key("Enter", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("games", true));
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("split", false));
        front.selection = 1;
        front.key("Enter", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("tracks", true));
        // Back returns to Split Screen, which rides one player again.
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("split", false));
        front.selection = 0;
        front.key("Enter", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("race", true));
        front.key("Escape", &view, &mut settings);
        front.key("Escape", &view, &mut settings);
        assert_eq!((front.page.as_str(), front.split), ("multiplayer", false));
        assert!(Frontend::new(tracks(), &settings, true).split_vertical);
        // Player 2's own ATV and rider rows, saved.
        front.show("split", &view);
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.clone()).collect();
        assert_eq!(actions[5..7], ["split2quad", "split2rider"]);
        let quad = front.split_seconds[0].0;
        front.selection = 5;
        front.key("Enter", &view, &mut settings);
        assert_ne!(front.split_seconds[0].0, quad);
        front.selection = 6;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.split_seconds[0].1, 1);
        assert!(front.choices()[6].1.contains("Female"));
        assert_eq!(
            Frontend::new(tracks(), &settings, true).split_seconds,
            front.split_seconds
        );
        // Player 2's own gear and paint rows, saved.
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.clone()).collect();
        assert_eq!(actions[7..9], ["split2gear", "split2paint"]);
        assert!(front.choices()[8].1.contains("Paint: Original"));
        front.selection = 8;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.split_looks[0].paint, 1);
        assert!(front.choices()[8].1.contains("Paint: Red"));
        front.selection = 7;
        front.key("Enter", &view, &mut settings);
        assert!(front.choices()[7].1.starts_with("Player 2 Gear: "));
        assert_eq!(
            Frontend::new(tracks(), &settings, true).split_looks,
            front.split_looks
        );
        assert_eq!(
            parse_split_look(&split_look_value(&front.split_looks[0])),
            Some(front.split_looks[0])
        );
        assert_eq!(parse_split_look("1,2,3"), None);
        // Three and four players: quarters (no layout row) and rows for
        // players 3 and 4, saved.
        front.selection = 3;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.split_players, 3);
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.clone()).collect();
        assert!(!actions.contains(&"splitlayout".to_string()));
        assert!(front.choices()[3].1.contains("Players: 3"));
        assert_eq!(
            actions[4..],
            [
                "split2quad",
                "split2rider",
                "split2gear",
                "split2paint",
                "split3quad",
                "split3rider",
                "split3gear",
                "split3paint"
            ]
        );
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.split_players, 4);
        let actions: Vec<_> = front.choices().iter().map(|(a, _)| a.clone()).collect();
        assert_eq!(actions.len(), 16);
        assert_eq!(
            actions[12..],
            ["split4quad", "split4rider", "split4gear", "split4paint"]
        );
        front.selection = 13;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.split_seconds[2].1, 1);
        let restored = Frontend::new(tracks(), &settings, true);
        assert_eq!(restored.split_players, 4);
        assert_eq!(restored.split_seconds, front.split_seconds);
        // Four wraps back to two.
        front.selection = 3;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.split_players, 2);
    }

    #[test]
    fn single_race_setup_steps_and_starts_a_race() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("single", &view);
        assert!(front.choices().iter().all(|(a, _)| a != "race"));
        front.set_race_courses(vec!["env01".into(), "nat01".into(), "sx01".into()], &view);
        let at = front
            .choices()
            .iter()
            .position(|(a, _)| a == "race")
            .unwrap();
        front.selection = at;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "race");
        assert_eq!(front.current(), Some("racetrack"));
        // Left/Right on the course row walk the race courses.
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.race.course, "nat01");
        front.key("ArrowDown", &view, &mut settings);
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.race.laps, 4);
        front.key("ArrowDown", &view, &mut settings);
        front.key("ArrowLeft", &view, &mut settings);
        assert_eq!(front.race.cpu, 0);
        front.key("ArrowDown", &view, &mut settings);
        // Accept on a value row steps it forward.
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.race.difficulty, 2);
        assert!(front.choices()[front.selection].1.contains("Expert"));
        // The event tabs filter the course list.
        front.key("PageDown", &view, &mut settings);
        assert_eq!(front.race_filter, "env");
        assert_eq!(front.race.course, "env01");
        front.key("PageUp", &view, &mut settings);
        assert_eq!(front.race_filter, "all");
        // Copy ride link and Start Race.
        let copy = front
            .choices()
            .iter()
            .position(|(a, _)| a == "copyride")
            .unwrap();
        front.selection = copy;
        assert_eq!(
            front.key("Enter", &view, &mut settings),
            Some(Event::CopyRaceLink)
        );
        // Enter on the Course row opens the course picker: the arrows walk
        // the courses, the tabs filter them, Back closes it on the page.
        front.selection = 0;
        assert_eq!(front.key("Enter", &view, &mut settings), None);
        assert!(front.course_sheet);
        assert_eq!(front.page, "race");
        front.key("ArrowDown", &view, &mut settings);
        assert_eq!(front.race.course, "nat01");
        front.key("ArrowUp", &view, &mut settings);
        front.key("PageDown", &view, &mut settings);
        assert_eq!(front.race_filter, "env");
        front.key("PageUp", &view, &mut settings);
        assert_eq!(front.key("Escape", &view, &mut settings), None);
        assert!(!front.course_sheet);
        assert_eq!(
            (front.page.as_str(), front.current()),
            ("race", Some("racetrack"))
        );
        assert_eq!(front.race.course, "env01");
        // Start Race.
        front.selection = front
            .choices()
            .iter()
            .position(|(a, _)| a == "racestart")
            .unwrap();
        assert_eq!(
            front.key("Enter", &view, &mut settings),
            Some(Event::Race {
                environment: "env01".into(),
                quad: 1,
                rider: 0,
                laps: 4,
                cpu: 0,
                difficulty: 2,
                cheats: (false, false),
                championship: None,
            })
        );
        let saved: Value = serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap();
        assert_eq!(saved["race"]["laps"], 4);
        // The setup comes back from the saved selection.
        let again = Frontend::new(tracks(), &settings, true);
        assert_eq!(again.race, front.race);
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "single");
    }

    #[test]
    fn rider_gear_steps_the_catalog_and_paint() {
        use atv_core::rider_gear::Item;
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        let item = |name: &str| Item {
            name: name.into(),
            materials: vec![],
            locked: false,
            points: 0,
            logo: String::new(),
        };
        front.gear_catalog = SLOTS
            .iter()
            .map(|slot| Category {
                slot,
                items: match *slot {
                    "shirt" => vec![item("Fox Jersey"), item("Thor Jersey")],
                    "sleeve" => vec![item("sleeves 1")],
                    "pant" => vec![item("pants 1"), item("pants 2")],
                    _ => vec![item(&format!("{slot} one"))],
                },
            })
            .collect();
        front.show("garage", &view);
        let gear = front
            .choices()
            .iter()
            .position(|(a, _)| a == "gear")
            .unwrap();
        front.selection = gear;
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.page, "gear");
        assert_eq!(front.gear_rows()[0], "Original");
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.gear_rows()[0], "helmet one");
        // Original, item 1, back to Original.
        front.key("ArrowRight", &view, &mut settings);
        assert_eq!(front.appearance.gear[0], 0);
        // The jersey sets the shirt and its matching sleeves.
        front.key("ArrowDown", &view, &mut settings);
        front.key("ArrowLeft", &view, &mut settings);
        assert_eq!(front.appearance.gear[3], 2);
        assert_eq!(front.appearance.gear[4], 1);
        // Pants take their jersey line's name.
        front.key("ArrowDown", &view, &mut settings);
        front.key("Enter", &view, &mut settings);
        assert_eq!(front.gear_rows()[2], "Fox Jersey");
        // Paint wraps through the authored colours.
        let paint = front
            .choices()
            .iter()
            .position(|(a, _)| a == "gearpaint")
            .unwrap();
        front.selection = paint;
        front.key("ArrowLeft", &view, &mut settings);
        assert_eq!(front.gear_rows()[6], "Black");
        let saved: Value = serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap();
        assert_eq!(saved["appearance"]["paint"], 10);
        assert_eq!(saved["appearance"]["gear"][3], 2);
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "garage");
        assert_eq!(front.current(), Some("gear"));
    }

    #[test]
    fn tab_walks_rows_and_fields_both_ways() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("main", &view);
        let rows = front.choices().len();
        assert!(rows > 2);
        // Tab walks every row and wraps; Shift+Tab goes back.
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.selection, 1);
        front.key("ShiftRight", &view, &mut settings);
        front.key("Tab", &view, &mut settings);
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.selection, rows - 1);
        front.key_up("ShiftRight");
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.selection, 0);
        // On a form the fields come first: past the last row, Tab focuses
        // the name, then the code, then the first row again.
        front.show("join-lobby", &view);
        let rows = front.choices().len();
        for _ in 0..rows {
            front.key("Tab", &view, &mut settings);
        }
        assert_eq!(front.focus, Some(0));
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.focus, Some(1));
        front.key("Tab", &view, &mut settings);
        assert_eq!((front.focus, front.selection), (None, 0));
        // A blur forgets a held Shift.
        front.key("ShiftLeft", &view, &mut settings);
        front.blur();
        front.key("Tab", &view, &mut settings);
        assert_eq!(front.selection, 1);
    }

    #[test]
    fn only_freestyle_rides_change_the_saved_freestyle_course() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        settings.set(SELECTION_KEY, r#"{"freestyleEnvironment":"env01"}"#);
        let mut front = Frontend::new(tracks(), &settings, true);
        let saved = |settings: &Settings| -> Value {
            serde_json::from_str(&settings.get(SELECTION_KEY).unwrap()).unwrap()
        };
        // Another course chosen for a room (or by a link, or the garage)
        // is saved with the garage choice but not as the Freestyle course.
        front.track = 1;
        front.show("garage", &view);
        front.run("quadnext", &view, &mut settings);
        assert_eq!(saved(&settings)["freestyleEnvironment"], "env01");
        front.show("join-lobby", &view);
        front.set_field("room-code", "abcd1234");
        front.run("join", &view, &mut settings);
        assert_eq!(saved(&settings)["freestyleEnvironment"], "env01");
        // Riding Freestyle remembers its course.
        front.show("tracks", &view);
        front.key("Enter", &view, &mut settings);
        assert_eq!(saved(&settings)["freestyleEnvironment"], "env02");
    }

    #[test]
    fn free_ride_takes_the_name_just_typed() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        settings.set(SELECTION_KEY, r#"{"playerName":"Ann"}"#);
        let mut front = Frontend::new(tracks(), &settings, true);
        front.show("join-lobby", &view);
        front.set_field("player-name", " Bea ");
        front.show("main", &view);
        assert!(matches!(
            front.run("freeride", &view, &mut settings),
            Some(Event::FreeRide { ref name, .. }) if name == "Bea"
        ));
        // A blank name rides as "Rider" (the host numbers it).
        front.set_field("player-name", "  ");
        assert!(matches!(
            front.run("freeride", &view, &mut settings),
            Some(Event::FreeRide { ref name, .. }) if name == "Rider"
        ));
    }

    #[test]
    fn the_pause_binding_accepts_and_a_blur_blocks_held_buttons() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        let idle = [0f32; 17];
        let mut a = [0f32; 17];
        a[0] = 1.;
        // The rebound controller Pause (any input) accepts like Start.
        front.pad(&idle, &[0.; 4], false, &view, &mut settings);
        front.pad(&idle, &[0.; 4], true, &view, &mut settings);
        assert_eq!(front.page, "main");
        // A button held across a blur acts only after its release.
        front.pad(&idle, &[0.; 4], false, &view, &mut settings);
        front.blur();
        front.pad(&a, &[0.; 4], false, &view, &mut settings);
        assert_eq!(front.page, "main");
        front.pad(&idle, &[0.; 4], false, &view, &mut settings);
        front.pad(&a, &[0.; 4], false, &view, &mut settings);
        assert_eq!(front.page, "single");
        // The left stick navigates too.
        front.pad(&idle, &[0.; 4], false, &view, &mut settings);
        front.pad(&idle, &[0., 0.9, 0., 0.], false, &view, &mut settings);
        assert_eq!(front.selection, 1);
    }

    #[test]
    fn back_held_as_a_menu_closes_does_not_also_leave_the_page() {
        let view = view(1280., 720.);
        let mut settings = Settings::memory();
        let mut front = Frontend::new(tracks(), &settings, true);
        let idle = [0f32; 17];
        let mut b = [0f32; 17];
        b[1] = 1.;
        front.show("main", &view);
        front.pad(&idle, &[0.; 4], false, &view, &mut settings);
        // Settings (a menu over the page) took the Back press and closed.
        front.hold_pad();
        front.pad(&b, &[0.; 4], false, &view, &mut settings);
        assert_eq!(front.page, "main", "the held Back stays with the menu");
        front.pad(&idle, &[0.; 4], false, &view, &mut settings);
        front.pad(&b, &[0.; 4], false, &view, &mut settings);
        assert_eq!(front.page, "title", "a fresh Back acts on the page");
    }

    #[test]
    fn loader_stages_only_advance() {
        let mut front = Frontend::new(tracks(), &Settings::memory(), true);
        front.begin_loading("env02", &view(1280., 720.));
        front.loading_stage("Reading original ATV…");
        front.loading_stage("Reading original ENV02…");
        let loading = front.loading.as_ref().unwrap();
        assert_eq!(loading.progress, 0.7);
        assert_eq!(loading.track, "ENV02");
    }
}

/// The Single Player profile readout: the original's "Current Profile
/// Points:" with the race record.
pub fn profile_line(profile: &atv_core::profile::Profile) -> String {
    let plural = |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    format!(
        "Profile Points {} · {} · {}",
        profile.points,
        plural(profile.races, "race", "races"),
        plural(profile.wins, "win", "wins")
    )
}

#[cfg(test)]
mod profile_tests {
    #[test]
    fn profile_line_reads_the_record() {
        let mut p = atv_core::profile::Profile::default();
        assert_eq!(
            super::profile_line(&p),
            "Profile Points 0 · 0 races · 0 wins"
        );
        p.record_race(1, 12, 2400.);
        assert_eq!(
            super::profile_line(&p),
            "Profile Points 12 · 1 race · 1 win"
        );
    }
}
