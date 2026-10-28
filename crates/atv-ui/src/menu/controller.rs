//! Pause menu state: page, selection, navigation and setting actions
//! (frontend.js `show`/`run`/`action` for the ride's pause screens). Rows come
//! from atv_core::ui_menu with the same request fields the browser sends.
use super::{CONTENT, PANEL, ROW, Rows, State, prefs::Preferences};
use crate::{
    hud::{Viewport, minimap::MinimapSettings},
    paint::Rect,
    scroll::Scroller,
    settings::Settings,
    touch::TouchControls,
};
use atv_game::ride::bindings::{
    self as binds, ACTIONS, Action, Atom, Binding, Bindings, CONTROLLER_STORAGE_KEY, Pad,
    STORAGE_KEY,
};
use serde_json::json;

/// What the ride must do after a menu input.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Resume,
    Reset,
    /// Leave the ride (web: the given page; desktop: quit).
    Leave(&'static str),
    /// A preference changed (sound mode, volumes, minimap…).
    Changed,
    /// Display → Fullscreen.
    Fullscreen,
    /// Pause → Copy ride link (web), or Copy invite link in a room.
    CopyRide,
    /// A Jukebox control for the host's soundtrack player.
    Jukebox(super::jukebox::JukeboxAction),
    /// Players → Spawn beside this rider's slot.
    Spawn(u8),
    /// Leave Game: leave the multiplayer room.
    LeaveGame,
    /// Change ATV and Rider: the garage over the held room.
    ChangeRide,
    /// The host's Select Track: the course picker over the held room.
    SelectTrack,
    /// The host's Play Tag: a Tag game for the room on this course.
    PlayTag,
}

/// The multiplayer room as the pause menu shows it (the browser's
/// `lobby_*` request fields).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lobby {
    /// "N/4 riders / names" or "Arlington Free Ride / N/20 riders".
    pub status: String,
    pub host: bool,
    pub public: bool,
    pub started: bool,
    /// (slot, label, "self" | "ready" | "waiting").
    pub players: Vec<(u8, String, &'static str)>,
}

/// The settings a pause action may change.
pub struct Context<'a> {
    pub settings: &'a mut Settings,
    pub prefs: &'a mut Preferences,
    pub minimap: &'a mut MinimapSettings,
    pub touch: &'a mut TouchControls,
    /// The ride's gameplay bindings and controller tuning.
    pub bindings: Option<&'a mut Bindings>,
    /// Name of the controller in use, if any (Controller Settings row).
    pub pad_name: Option<String>,
    /// Connected controllers (name, index) for pinning one, and whether
    /// the one in use reports the standard mapping (glyph names).
    pub pads: Vec<(String, u32)>,
    pub pad_standard: bool,
    pub view: Viewport,
    /// The multiplayer room this ride is in.
    pub lobby: Option<&'a Lobby>,
    /// The ride's soundtrack player (the Jukebox needs it).
    pub music: Option<&'a super::jukebox::MusicView>,
}

/// Rows the Rust engine cannot perform yet are left out of its menu.
const UNSUPPORTED: [&str; 0] = [];

/// A binding being captured (frontend.js `beginCapture`).
struct Capture {
    action: Action,
    label: String,
    atoms: Vec<Atom>,
    held: Vec<String>,
    armed: bool,
    /// The row it began on: the selection returns there (frontend.js keeps
    /// `selection` through a capture).
    selection: usize,
}

pub struct PauseMenu {
    pub page: String,
    pub selection: usize,
    pub rows: Rows,
    /// The panel's kinetic scroll (crate::scroll).
    pub scroller: Scroller,
    hovered: Option<usize>,
    pressed: Option<Pressed>,
    pad: u32,
    /// The rows were built for this request.
    key: String,
    /// Bindings page: keyboard (true) or controller rows, page and row.
    keyboard: bool,
    binding_page: usize,
    /// Settings → Audio's song list page (four rows a page, musicPage).
    music_page: usize,
    binding_row: usize,
    capture: Option<Capture>,
    /// Opened from the frontend's main menu: Settings is the root page.
    pub from_menu: bool,
    /// The ride can return to the engine's own frontend (Quit to Main Menu).
    pub frontend: bool,
    /// The menu notice line (`#menu-notice`), e.g. "Ride link copied.".
    pub notice: Option<String>,
    /// A Jukebox seek being dragged (fraction of the song).
    pub seek_to: Option<f32>,
    /// The ride is a race: Pause's second row is Restart Race
    /// (frontend.js `raceActive`).
    pub race_active: bool,
    /// The ride is the Training Course (Restart Training).
    pub training_active: bool,
    /// The ride is a mini-game (Restart Game).
    pub game_active: bool,
    /// Keys held while the menu is open (frontend.js `menuKeys`), for the
    /// rebindable Pause action.
    held: Vec<String>,
    /// Generation of the page last shown (each `show` slides a page in).
    pub shown: u64,
    /// The pages Back returns through with the row each was left by
    /// (the original's page history, research/MENU_FLOW.md).
    history: Vec<(String, usize, Option<String>)>,
    /// The row to select once the returned-to page's rows are built.
    restore: Option<String>,
}

struct Pressed {
    pointer: u64,
    index: usize,
    x: f32,
    y: f32,
    moved: bool,
    slider: Option<Rect>,
    /// It caught a moving panel: a grab, never a tap.
    caught: bool,
}

/// Each page's Back target (frontend.js returns).
fn parent(page: &str) -> &'static str {
    match page {
        "settings" => "pause",
        "minimap-config" | "race-overlays" => "hud-settings",
        "controller-settings" | "bindings" => "controls",
        "ecosystem-warning" => "game-settings",
        "pause" | "players" | "jukebox" => "pause",
        _ => "settings",
    }
}

impl PauseMenu {
    pub fn new() -> Self {
        Self {
            page: "pause".into(),
            selection: 0,
            rows: Rows::default(),
            scroller: Scroller::default(),
            hovered: None,
            pressed: None,
            pad: u32::MAX,
            key: String::new(),
            keyboard: true,
            binding_page: 0,
            music_page: 0,
            binding_row: 0,
            capture: None,
            from_menu: false,
            frontend: false,
            notice: None,
            seek_to: None,
            race_active: false,
            training_active: false,
            game_active: false,
            held: Vec::new(),
            shown: crate::transition::next_show(),
            history: Vec::new(),
            restore: None,
        }
    }

    /// Settings opened from the frontend's main menu.
    pub fn settings() -> Self {
        Self {
            page: "settings".into(),
            from_menu: true,
            ..Self::new()
        }
    }

    /// The Jukebox opened from the frontend's main menu (jukeboxReturn
    /// 'main'): Back leaves it for the menu.
    pub fn jukebox() -> Self {
        Self {
            page: "jukebox".into(),
            from_menu: true,
            ..Self::new()
        }
    }

    /// Opened over the frontend rather than a ride.
    pub fn from_menu(&self) -> bool {
        self.from_menu
    }

    /// Settings → Audio's song list page.
    pub fn music_page(&self) -> usize {
        self.music_page
    }

    pub fn state(&self, ctx: &Context) -> State {
        State {
            page: self.page.clone(),
            rows: self.rows.clone(),
            selection: self.selection,
            hovered: self.hovered,
            // Pressed feedback ends once the finger scrolls.
            pressed: self.pressed.as_ref().filter(|p| !p.moved).map(|p| p.index),
            scroll: self.scroller.offset(),
            one_handed: ctx.touch.one_handed(),
            capture: self.capture.as_ref().map(|c| c.label.clone()),
            from_menu: self.from_menu,
            notice: self.notice.clone(),
            music: ctx.music.filter(|_| self.page == "jukebox").cloned(),
            jukebox_art: None,
            frame: 0,
            motion: None,
            // Controller bindings show glyphs (standard-mapping pads).
            pad: (self.page == "bindings" && !self.keyboard)
                .then_some(ctx.bindings.as_deref())
                .flatten()
                .map(|b| super::glyphs::PadGlyphs::new(b, ctx.pad_standard)),
            roster: if self.page == "players" {
                ctx.lobby
                    .iter()
                    .flat_map(|l| l.players.iter())
                    .filter(|(_, _, value)| *value != "ready")
                    .map(|(_, label, value)| {
                        (
                            label.clone(),
                            if *value == "self" {
                                "You"
                            } else {
                                "Connecting"
                            }
                            .into(),
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            },
            page_count: match self.page.as_str() {
                "bindings" => Some(format!(
                    "Page {} of {}",
                    self.binding_page + 1,
                    ACTIONS.len().div_ceil(4)
                )),
                // frontend.js: Page n of the song list's four-row pages.
                "music" => Some(format!(
                    "Page {} of {}",
                    self.music_page + 1,
                    ctx.music.map_or(0, |m| m.songs.len()).div_ceil(4).max(1)
                )),
                _ => None,
            },
            // `.pause-header .pause-status`: Audio's soundtrack line.
            status: (self.page == "music")
                .then(|| ctx.music.map(music_status))
                .flatten(),
        }
    }

    /// Rebuilds the rows when any input of the request changed.
    pub fn refresh(&mut self, ctx: &Context) {
        let p = &*ctx.prefs;
        let m = &*ctx.minimap;
        let request = json!({
            "page": self.page,
            "width": ctx.view.width.clamp(240., 16384.),
            "height": ctx.view.height.clamp(240., 16384.),
            "selection": self.selection,
            "race_active": self.race_active,
            "training_active": self.training_active,
            "game_active": self.game_active,
            "race_overlays_off": p.overlays_off,
            "race_position": p.race_position,
            "funny_crashes": p.crash_mode != "original",
            "crash_mode": p.crash_mode,
            "original_camera": p.crash_camera_original,
            "chase_assist": p.chase_assist,
            "flips": p.flips,
            "realistic_train": p.realistic_train,
            "metric": p.metric,
            "ecosystem_collision": p.ecosystem_collision,
            "sound_configuration": p.sound_configuration,
            "speed_rush": p.speed_rush,
            "graphics_preset": p.graphics,
            "graphics_resolved": p.graphics_resolved(),
            "vibration": p.vibration,
            "diagnostics_mode": p.diagnostics,
            "sound_mode": p.sound_mode,
            // frontend.js: the row only where the audio session exists.
            "silent_mode": if p.audio_session { p.silent.as_str() } else { "" },
            "sfx_volume": (p.sfx_volume * 100.).round() as usize,
            "alpha_rows": if self.page == "alpha" {
                super::prefs::ALPHA_FEATURES.iter().map(|(id, label)| json!({
                    "id": id, "label": label, "value": if p.alpha(id) { "On" } else { "Off" },
                })).collect::<Vec<_>>()
            } else {
                Vec::new()
            },
            "music_enabled": p.music_enabled && !(self.page == "jukebox" && ctx.music.is_some_and(|m| m.paused)),
            "music_volume": (p.music_volume * 100.).round() as usize,
            "music_rows": if self.page == "jukebox" {
                ctx.music.map_or_else(Vec::new, |m| m.songs.iter().enumerate().map(|(i, (title, artist, _))| {
                    json!({"id": i.to_string(), "label": title, "value": artist})
                }).collect())
            } else if self.page == "music" {
                // frontend.js: the playing song is marked on Audio's rows.
                ctx.music.map_or_else(Vec::new, |m| m.songs.iter().enumerate().map(|(i, (title, artist, _))| {
                    let mark = match (Some(i) == m.playing_index, m.enabled) {
                        (true, true) => "Playing / ",
                        (true, false) => "Selected / ",
                        _ => "",
                    };
                    json!({"id": i.to_string(), "label": title, "value": format!("{mark}{artist}")})
                }).collect())
            } else {
                Vec::new()
            },
            "music_page": self.music_page,
            "music_status": ctx.music.map_or_else(String::new, music_status),
            "minimap_enabled": m.enabled,
            "minimap_size": m.size,
            "minimap_zoom": m.zoom,
            "minimap_shape": if m.shape == crate::hud::minimap::MapShape::Circle { "circle" } else { "square" },
            "minimap_corner": if m.bottom_left { "bottom-left" } else { "top-right" },
            "minimap_orientation": if m.heading_up { "heading-up" } else { "north-up" },
            "mobile_steering_sensitivity": ctx.touch.input.steering_sensitivity,
            "one_handed_mobile": ctx.touch.one_handed(),
            // frontend.js: Fullscreen only where the API exists (not iPhone).
            "fullscreen": p.fullscreen,
        });
        let mut request = request;
        // Past json!'s recursion limit.
        request["new_profiles_unlocked"] = p.new_profiles_unlocked.into();
        if let Some(b) = ctx.bindings.as_deref() {
            let t = &b.tuning;
            // gamepads.js controllerLabel: Auto, a pinned pad, or a pinned
            // pad that is absent.
            request["controller_label"] = match (&t.selected, &ctx.pad_name) {
                (Some((id, _)), _) if ctx.pads.iter().any(|(name, _)| name == id) => pad_name(id),
                (Some((id, _)), _) => format!("{} (not connected, using Auto)", pad_name(id)),
                (None, Some(name)) => format!("Auto ({})", pad_name(name)),
                (None, None) => "Auto (press a button)".into(),
            }
            .into();
            request["controller_preset"] = b.controller_preset_label().into();
            request["controller_steering"] = t.steering.into();
            request["controller_lean"] = t.lean.into();
            request["controller_triggers"] = t.triggers.into();
            request["controller_camera"] = t.camera.into();
            request["controller_deadzone"] = t.deadzone.into();
            request["binding_device"] = if self.keyboard {
                "keyboard"
            } else {
                "controller"
            }
            .into();
            request["binding_page"] = self.binding_page.into();
            if self.page == "bindings" {
                request["control_rows"] = ACTIONS
                    .iter()
                    .map(|(action, id, label)| {
                        json!({
                            "id": id,
                            "label": if self.keyboard { binds::keyboard_label(*action) } else { label },
                            "value": binds::binding_text(
                                b.row(self.keyboard, *action),
                                self.keyboard || ctx.pad_standard,
                            ),
                        })
                    })
                    .collect();
            }
        }
        if let Some(c) = &self.capture {
            request["capture_label"] = c.label.clone().into();
        }
        // Options from the main menu leads with the Jukebox.
        request["options_jukebox"] = self.from_menu.into();
        if let Some(lobby) = ctx.lobby {
            request["lobby_status"] = lobby.status.clone().into();
            request["lobby_host"] = lobby.host.into();
            request["lobby_public"] = lobby.public.into();
            request["lobby_started"] = lobby.started.into();
            request["lobby_players"] = lobby
                .players
                .iter()
                .map(|(id, label, value)| json!({"id": id.to_string(), "label": label, "value": value}))
                .collect();
        }
        let key = request.to_string();
        if key == self.key {
            return;
        }
        let rows = serde_json::from_value::<atv_core::ui_menu::Request>(request)
            .map_err(|e| e.to_string())
            .and_then(atv_core::ui_menu::layout)
            .map(|frame| Rows::from_frame(&frame));
        if let Ok(mut rows) = rows {
            rows.choices.retain(|c| !UNSUPPORTED.contains(&c.as_str()));
            if !cfg!(target_arch = "wasm32") {
                // Desktop has no dev log page to open. Copy ride link stays:
                // it copies the site's play.html link for this ride.
                rows.choices.retain(|c| c != "devlog");
            }
            // Change ATV and Rider and Select Track open the engine's own
            // frontend; a direct room link has none.
            if !self.frontend {
                rows.choices
                    .retain(|c| c != "changeride" && c != "hosttrack");
            }
            if !cfg!(target_arch = "wasm32") && !self.frontend {
                rows.choices.retain(|c| c != "main");
            }
            if let Some(action) = self.restore.take()
                && let Some(at) = rows.choices.iter().position(|c| *c == action)
            {
                self.selection = at;
            }
            self.selection = self.selection.min(rows.choices.len().saturating_sub(1));
            self.rows = rows;
        }
        self.key = key;
    }

    /// Opens `page` over the current one; a page already in the history is
    /// returned to on the row it was left by.
    fn show(&mut self, page: &str) {
        if page == self.page {
            self.present(page);
            return;
        }
        if let Some(at) = self.history.iter().rposition(|(p, ..)| p == page) {
            let (_, selection, action) = self.history[at].clone();
            self.history.truncate(at);
            self.present(page);
            self.selection = selection;
            self.restore = action;
            return;
        }
        let action = self.selected().map(String::from);
        self.history
            .push((self.page.clone(), self.selection, action));
        self.present(page);
    }

    /// The pages Back returns through, oldest first.
    pub fn history(&self) -> impl Iterator<Item = &str> {
        self.history.iter().map(|(p, ..)| p.as_str())
    }

    fn present(&mut self, page: &str) {
        self.shown = crate::transition::next_show();
        self.capture = None;
        self.notice = None;
        self.page = page.into();
        self.selection = 0;
        self.scroller = Scroller::default();
        self.hovered = None;
        self.key.clear();
    }

    fn back(&mut self) -> Option<Event> {
        if self.page == "pause" || (self.from_menu && self.page == "settings") {
            return Some(Event::Resume);
        }
        if self.page == "jukebox" {
            // Opened on its own from the main menu the Jukebox closes (the
            // host ends the menu); otherwise it returns to its opener.
            if let Some((page, ..)) = self.history.last() {
                let page = page.clone();
                self.show(&page);
            } else if !self.from_menu {
                self.show("pause");
            }
            return Some(Event::Jukebox(super::jukebox::JukeboxAction::Leave));
        }
        // The page the player came from, on the row they left it by;
        // without one (a page the host opened) its parent.
        let to = match self.history.last() {
            Some((page, ..)) => page.clone(),
            None if self.page == "bindings" && !self.keyboard => "controller-settings".into(),
            None => parent(&self.page).into(),
        };
        self.show(&to);
        None
    }

    /// frontend.js `run`.
    pub fn run(
        &mut self,
        action: &str,
        direction: i32,
        stepped: bool,
        ctx: &mut Context,
    ) -> Option<Event> {
        let event = match action {
            "resume" => return Some(Event::Resume),
            "reset" => return Some(Event::Reset),
            "back" | "pause" => return self.back(),
            "devlog" => return Some(Event::Leave("devlog.html")),
            "fullscreen" => return Some(Event::Fullscreen),
            "copyride" => return Some(Event::CopyRide),
            "leave" => return Some(Event::LeaveGame),
            "changeride" => return Some(Event::ChangeRide),
            "hosttrack" => return Some(Event::SelectTrack),
            "playtag" => return Some(Event::PlayTag),
            "players" => {
                self.show("players");
                return None;
            }
            _ if action.starts_with("spawn:") => {
                return action[6..].parse().ok().map(Event::Spawn);
            }
            "jukebox" => {
                self.show("jukebox");
                return None;
            }
            "jukeplay" | "jukeprev" | "jukenext" => {
                use super::jukebox::JukeboxAction as J;
                let playing = ctx.music.is_some_and(|m| m.enabled && !m.paused);
                if action == "jukeplay" && !playing && !ctx.prefs.music_enabled {
                    let volume = ctx.prefs.music_volume;
                    ctx.prefs.set_music(volume, true, ctx.settings);
                }
                return Some(Event::Jukebox(match action {
                    "jukeplay" => J::Toggle,
                    "jukeprev" => J::Previous,
                    _ => J::Next,
                }));
            }
            "jukeseek" => {
                let m = ctx.music?;
                if !stepped || !m.seekable {
                    return None;
                }
                return Some(Event::Jukebox(super::jukebox::JukeboxAction::Seek(
                    m.position + direction as f32 * super::jukebox::SEEK_STEP,
                )));
            }
            "jukevolume" => {
                if stepped {
                    let (volume, enabled) = (ctx.prefs.music_volume, ctx.prefs.music_enabled);
                    ctx.prefs
                        .set_music(volume + direction as f32 * 0.05, enabled, ctx.settings);
                    Event::Changed
                } else {
                    return None;
                }
            }
            _ if action.starts_with("song:") => {
                let index = action[5..].parse::<usize>().ok()?;
                if !ctx.prefs.music_enabled {
                    let volume = ctx.prefs.music_volume;
                    ctx.prefs.set_music(volume, true, ctx.settings);
                }
                return Some(Event::Jukebox(super::jukebox::JukeboxAction::Select(index)));
            }
            "main" => return Some(Event::Leave("play.html")),
            "settings" | "extras" | "alpha" | "hud-settings" | "minimap-config"
            | "race-overlays" | "controls" | "music" | "game-settings" => {
                self.show(action);
                return None;
            }
            // Turning Ecosystem Collision off asks first (Warning 0x90);
            // turning it on does not.
            "ecosystem" => {
                if ctx.prefs.ecosystem_collision {
                    self.show("ecosystem-warning");
                    return None;
                }
                ctx.prefs.set_ecosystem_collision(true, ctx.settings);
                Event::Changed
            }
            "ecosystem-yes" | "ecosystem-no" => {
                if action == "ecosystem-yes" {
                    ctx.prefs.set_ecosystem_collision(false, ctx.settings);
                }
                // Back on the Ecosystem Collision row.
                self.show("game-settings");
                self.restore = Some("ecosystem".into());
                Event::Changed
            }
            "keyboard" | "padbindings" => {
                self.keyboard = action == "keyboard";
                self.binding_page = 0;
                self.binding_row = 0;
                self.show("bindings");
                return None;
            }
            "controller" => {
                self.show("controller-settings");
                return None;
            }
            "musicprev" | "musicnext" => {
                let pages = ctx.music.map_or(1, |m| m.songs.len().div_ceil(4).max(1));
                let step = if action == "musicprev" { pages - 1 } else { 1 };
                self.music_page = (self.music_page + step) % pages;
                self.selection = 0;
                Event::Changed
            }
            "bindprev" | "bindnext" => {
                let pages = ACTIONS.len().div_ceil(4);
                let step = if action == "bindprev" { pages - 1 } else { 1 };
                self.binding_page = (self.binding_page + step) % pages;
                self.binding_row = self.binding_page * 4;
                self.selection = 0;
                Event::Changed
            }
            "bindcancel" => {
                self.end_capture();
                Event::Changed
            }
            "bindclear" | "bindreset" | "padpreset" | "paddefaults" => {
                let bindings = ctx.bindings.as_deref_mut()?;
                match action {
                    "bindclear" => {
                        let row = ACTIONS.get(self.binding_row)?.0;
                        bindings.set_binding(self.keyboard, row, Vec::new());
                    }
                    "bindreset" => bindings.reset_device(self.keyboard),
                    "padpreset" => bindings.cycle_controller_preset(direction),
                    _ => bindings.tuning.reset(),
                }
                save_bindings(bindings, ctx.settings);
                Event::Changed
            }
            // gamepads.js cycleController: Auto → each connected pad → Auto.
            "padselect" => {
                let bindings = ctx.bindings.as_deref_mut()?;
                let options: Vec<Option<(String, u32)>> = std::iter::once(None)
                    .chain(ctx.pads.iter().cloned().map(Some))
                    .collect();
                let at = bindings
                    .tuning
                    .selected
                    .as_ref()
                    .and_then(|(id, index)| {
                        options
                            .iter()
                            .position(|o| o.as_ref().is_some_and(|(n, i)| n == id && i == index))
                            .or_else(|| {
                                options
                                    .iter()
                                    .position(|o| o.as_ref().is_some_and(|(n, _)| n == id))
                            })
                    })
                    .unwrap_or(0) as i32;
                let next = (at + direction).rem_euclid(options.len() as i32) as usize;
                bindings.tuning.selected = options[next].clone();
                ctx.settings
                    .set(CONTROLLER_STORAGE_KEY, &bindings.tuning.to_json());
                Event::Changed
            }
            _ if action.starts_with("bind:") => {
                let id = &action[5..];
                let (index, (row, _, label)) =
                    ACTIONS.iter().enumerate().find(|(_, (_, i, _))| *i == id)?;
                self.binding_row = index;
                self.capture = Some(Capture {
                    action: *row,
                    label: if self.keyboard {
                        binds::keyboard_label(*row).into()
                    } else {
                        (*label).into()
                    },
                    atoms: Vec::new(),
                    held: Vec::new(),
                    armed: false,
                    selection: index % 4,
                });
                if let Some(b) = ctx.bindings.as_deref_mut() {
                    b.cancel();
                }
                self.held.clear();
                Event::Changed
            }
            "minimap-toggle" => {
                ctx.minimap.enabled = !ctx.minimap.enabled;
                ctx.minimap.save(ctx.settings);
                Event::Changed
            }
            "minimap-shape" => {
                use crate::hud::minimap::MapShape;
                ctx.minimap.shape = match ctx.minimap.shape {
                    MapShape::Circle => MapShape::Square,
                    MapShape::Square => MapShape::Circle,
                };
                ctx.minimap.save(ctx.settings);
                Event::Changed
            }
            "minimap-corner" => {
                ctx.minimap.bottom_left = !ctx.minimap.bottom_left;
                ctx.minimap.save(ctx.settings);
                Event::Changed
            }
            "minimap-orientation" => {
                ctx.minimap.heading_up = !ctx.minimap.heading_up;
                ctx.minimap.save(ctx.settings);
                Event::Changed
            }
            "onehanded" => {
                ctx.touch
                    .set_one_handed(!ctx.touch.one_handed(), ctx.settings);
                Event::Changed
            }
            "minimap-size"
            | "minimap-zoom"
            | "steering-sensitivity"
            | "pad-steering"
            | "pad-lean"
            | "pad-triggers"
            | "pad-camera"
            | "pad-deadzone" => {
                if let Some(s) = self.rows.slider(action).cloned() {
                    self.set_slider(action, s.value + s.step * direction as f32, ctx);
                }
                Event::Changed
            }
            _ if ctx.prefs.apply(action, direction, stepped, ctx.settings) => Event::Changed,
            _ => return None,
        };
        self.key.clear();
        Some(event)
    }

    fn set_slider(&mut self, action: &str, value: f32, ctx: &mut Context) {
        let Some(s) = self.rows.slider(action) else {
            return;
        };
        let value = ((value.clamp(s.min, s.max) - s.min) / s.step).round() * s.step + s.min;
        match action {
            "minimap-size" => ctx.minimap.size = crate::hud::minimap::clamp_size(value),
            "minimap-zoom" => ctx.minimap.zoom = crate::hud::minimap::clamp_zoom(value),
            _ if action.starts_with("pad-") => {
                if let Some(b) = ctx.bindings.as_deref_mut() {
                    b.tuning.set(&action[4..], value);
                    ctx.settings
                        .set(CONTROLLER_STORAGE_KEY, &b.tuning.to_json());
                }
            }
            _ => ctx.touch.set_steering_sensitivity(value, ctx.settings),
        }
        if action.starts_with("minimap") {
            ctx.minimap.save(ctx.settings);
        }
        self.key.clear();
    }

    fn selected(&self) -> Option<&str> {
        self.rows.choices.get(self.selection).map(String::as_str)
    }

    fn step(&mut self, delta: i32) {
        let n = self.rows.choices.len() as i32;
        if n > 0 {
            self.selection = (self.selection as i32 + delta).rem_euclid(n) as usize;
            self.key.clear();
            self.follow_binding_row();
        }
    }

    /// frontend.js `bindingRow` follows the highlighted binding row (arrow
    /// keys and pointer focus), so Clear binding clears that row.
    fn follow_binding_row(&mut self) {
        if self.page != "bindings" {
            return;
        }
        let row = self
            .selected()
            .and_then(|a| a.strip_prefix("bind:"))
            .and_then(|id| ACTIONS.iter().position(|(_, a, _)| *a == id));
        if let Some(row) = row {
            self.binding_row = row;
        }
    }

    /// Ends a binding capture, back on the row it began on.
    fn end_capture(&mut self) -> Option<Capture> {
        let c = self.capture.take()?;
        self.selection = c.selection;
        self.held.clear();
        self.key.clear();
        Some(c)
    }

    /// Losing focus cancels a binding capture (frontend.js `blur` and
    /// `visibilitychange` → `cancelCapture`) and forgets held keys.
    pub fn cancel_capture(&mut self) {
        self.end_capture();
        self.held.clear();
        self.pad = u32::MAX;
    }

    /// Keyboard input while the menu is open (winit key code names).
    pub fn key(&mut self, code: &str, ctx: &mut Context) -> Option<Event> {
        if let Some(c) = &mut self.capture {
            if code == "Escape" {
                self.end_capture();
                return Some(Event::Changed);
            }
            let valid = code.len() <= 40
                && code.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && code.chars().all(|c| c.is_ascii_alphanumeric());
            if self.keyboard && valid {
                if !c.held.iter().any(|h| h == code) {
                    c.held.push(code.into());
                }
                let atom = Atom::Key(code.into());
                if !c.atoms.contains(&atom) && c.atoms.len() < 4 {
                    c.atoms.push(atom);
                }
            }
            return None;
        }
        if !self.held.iter().any(|h| h == code) {
            self.held.push(code.into());
        }
        // frontend.js: the (rebindable) Pause action resumes from Pause.
        if self.page == "pause"
            && let Some(b) = ctx.bindings.as_deref()
            && b.value(
                Action::Pause,
                &|c: &str| self.held.iter().any(|h| h == c),
                None,
            ) > 0.5
        {
            return Some(Event::Resume);
        }
        self.navigate(code, ctx)
    }

    /// A menu key or a controller press mapped to one.
    fn navigate(&mut self, code: &str, ctx: &mut Context) -> Option<Event> {
        match code {
            "ArrowUp" | "KeyW" => self.step(-1),
            "ArrowDown" | "KeyS" => self.step(1),
            // Focus order: Shift+Tab walks it backwards.
            "Tab" => self.step(
                if self
                    .held
                    .iter()
                    .any(|h| h == "ShiftLeft" || h == "ShiftRight")
                {
                    -1
                } else {
                    1
                },
            ),
            "ArrowLeft" | "ArrowRight" | "KeyA" | "KeyD" => {
                let direction = if matches!(code, "ArrowLeft" | "KeyA") {
                    -1
                } else {
                    1
                };
                let action = self.selected()?.to_string();
                // Left/Right step sliders and cycle valued rows; never
                // navigate. Binding and song rows are lists, not values.
                let list = action.starts_with("bind:") || action.starts_with("song:");
                let valued = !list && self.rows.label(&action).is_some_and(|l| l.contains(": "));
                if self.rows.slider(&action).is_some()
                    || valued
                    || matches!(action.as_str(), "jukeseek" | "jukevolume")
                {
                    return self.run(&action, direction, true, ctx);
                }
                // frontend.js: elsewhere the Jukebox changes track, and
                // Bindings and Audio turn the page.
                let turn = match (self.page.as_str(), direction < 0) {
                    ("jukebox", true) => "jukeprev",
                    ("jukebox", false) => "jukenext",
                    ("bindings", true) => "bindprev",
                    ("bindings", false) => "bindnext",
                    ("music", true) => "musicprev",
                    ("music", false) => "musicnext",
                    _ => return None,
                };
                return self.run(turn, direction, false, ctx);
            }
            "Enter" | "Space" | "NumpadEnter" => {
                let action = self.selected()?.to_string();
                return self.run(&action, 1, false, ctx);
            }
            // Only the Pause binding (handled in `key`) and Escape resume
            // from Pause; a P no longer bound to Pause does nothing.
            "Escape" | "Backspace" => return self.back(),
            _ => {}
        }
        None
    }

    /// Key release: a keyboard capture saves once every key is up.
    pub fn key_up(&mut self, code: &str, ctx: &mut Context) -> Option<Event> {
        self.held.retain(|h| h != code);
        let c = self.capture.as_mut()?;
        if !self.keyboard {
            return None;
        }
        c.held.retain(|h| h != code);
        if c.held.is_empty() && !c.atoms.is_empty() {
            return self.finish_capture(ctx);
        }
        None
    }

    fn finish_capture(&mut self, ctx: &mut Context) -> Option<Event> {
        let c = self.end_capture()?;
        let binding = if c.atoms.len() == 1 {
            Binding::One(c.atoms[0].clone())
        } else {
            Binding::Chord(c.atoms)
        };
        let bindings = ctx.bindings.as_deref_mut()?;
        bindings.set_binding(self.keyboard, c.action, vec![binding]);
        // frontend.js finishCapture: the notice shows a failed save.
        self.notice = (!save_bindings(bindings, ctx.settings))
            .then(|| crate::settings::STORAGE_ERROR.to_string());
        self.key.clear();
        Some(Event::Changed)
    }

    /// frontend.js `pollCapture`: buttons and axes held together become one
    /// binding when all are released.
    fn capture_pad(
        &mut self,
        buttons: &[f32],
        axes: &[f32],
        connected: bool,
        ctx: &mut Context,
    ) -> Option<Event> {
        if self.keyboard {
            return None;
        }
        let c = self.capture.as_mut()?;
        if !connected {
            c.armed = false;
            c.atoms.clear();
            return None;
        }
        let mut atoms = Vec::new();
        for (i, v) in buttons.iter().enumerate() {
            if *v > 0.5 {
                atoms.push(Atom::Button(i as u8));
            }
        }
        for (i, v) in axes.iter().enumerate() {
            if v.abs() > 0.65 {
                atoms.push(Atom::Axis(i as u8, if *v < 0. { -1 } else { 1 }));
            }
        }
        if !c.armed {
            c.armed = atoms.is_empty();
            return None;
        }
        for atom in &atoms {
            let same_axis =
                |a: &Atom| matches!((a, atom), (Atom::Axis(x, _), Atom::Axis(y, _)) if x == y);
            if !c.atoms.iter().any(|a| a == atom || same_axis(a)) && c.atoms.len() < 4 {
                c.atoms.push(atom.clone());
            }
        }
        if atoms.is_empty() && !c.atoms.is_empty() {
            return self.finish_capture(ctx);
        }
        None
    }

    /// Standard-mapping controller buttons (edges): D-pad/stick move, A
    /// accepts, B/Y go back, Start (or the Pause binding) resumes.
    pub fn pad(&mut self, pad: Option<&Pad>, ctx: &mut Context) -> Option<Event> {
        let idle = Pad::default();
        let sample = pad.unwrap_or(&idle);
        let (buttons, axes) = (&sample.buttons, &sample.axes);
        if self.capture.is_some() {
            if self.keyboard {
                // A pad press does not navigate under a keyboard capture.
                return None;
            }
            return self.capture_pad(buttons, axes, pad.is_some(), ctx);
        }
        let pause = pad.is_some()
            && ctx
                .bindings
                .as_deref()
                .is_some_and(|b| b.value(Action::Pause, &|_: &str| false, pad) > 0.5);
        let mut held = 0u32;
        let mut set = |bit: u32, on: bool| {
            if on {
                held |= 1 << bit;
            }
        };
        set(0, buttons[0] > 0.5);
        set(1, buttons[1] > 0.5 || buttons[3] > 0.5);
        set(2, buttons[9] > 0.5 || pause);
        set(3, buttons[12] > 0.5 || axes[1] < -0.55);
        set(4, buttons[13] > 0.5 || axes[1] > 0.55);
        set(5, buttons[14] > 0.5 || axes[0] < -0.55);
        set(6, buttons[15] > 0.5 || axes[0] > 0.55);
        // The press that opened the menu does not also act in it.
        let fresh = held & !self.pad;
        self.pad = held;
        let mut event = None;
        for (bit, code) in [
            (3, "ArrowUp"),
            (4, "ArrowDown"),
            (5, "ArrowLeft"),
            (6, "ArrowRight"),
            (0, "Enter"),
            (1, "Escape"),
        ] {
            if fresh & 1 << bit != 0 {
                event = event.or(self.navigate(code, ctx));
            }
        }
        if fresh & 1 << 2 != 0 {
            // frontend.js: Start resumes the ride from its menus; Settings
            // and the Jukebox over the main menu take it as accept.
            event = event.or(if self.from_menu {
                self.navigate("Enter", ctx)
            } else {
                Some(Event::Resume)
            });
        }
        event
    }

    fn hit(hits: &[(Rect, u32)], x: f32, y: f32) -> Option<(usize, Rect)> {
        // Rows inside the scrolled content are clipped to the panel (the
        // Jukebox scrolls only its track list; its controls stay put).
        let rect = |want: u32| hits.iter().find(|(_, id)| *id == want).map(|(r, _)| *r);
        let (panel, content) = (rect(PANEL), rect(CONTENT));
        let scrolled = |r: &Rect| {
            content.is_some_and(|c| {
                r.y >= c.y - 0.5 && r.bottom() <= c.bottom() + 0.5 && r.x >= c.x - 0.5
            })
        };
        hits.iter()
            .rev()
            .find(|(r, id)| {
                *id >= ROW
                    && r.contains(x, y)
                    && !(scrolled(r) && panel.is_some_and(|p| !p.contains(x, y)))
            })
            .map(|(r, id)| ((id - ROW) as usize, *r))
    }

    pub fn hover(&mut self, hits: &[(Rect, u32)], x: f32, y: f32) {
        let hovered = Self::hit(hits, x, y).map(|(i, _)| i);
        if hovered != self.hovered {
            self.hovered = hovered;
        }
    }

    pub fn press(&mut self, hits: &[(Rect, u32)], pointer: u64, x: f32, y: f32, ctx: &mut Context) {
        let target = Self::hit(hits, x, y);
        let slider = target.and_then(|(i, r)| {
            let action = self.rows.choices.get(i)?;
            let juke = action == "jukevolume"
                || (action == "jukeseek" && ctx.music.is_some_and(|m| m.seekable));
            (self.rows.slider(action).is_some() || juke).then_some(r)
        });
        // A slider takes the press at once; any other row is chosen by a
        // tap on release, so a drag that scrolls never selects.
        if let (Some((index, _)), Some(_)) = (target, slider) {
            self.selection = index;
            self.key.clear();
            self.follow_binding_row();
        }
        let caught = slider.is_none() && self.scroller.press(y, crate::scroll::now());
        self.pressed = Some(Pressed {
            pointer,
            index: target.map_or(usize::MAX, |(i, _)| i),
            x,
            y,
            moved: caught,
            slider,
            caught,
        });
        if slider.is_some() {
            self.drag_slider(x, ctx);
        }
    }

    fn drag_slider(&mut self, x: f32, ctx: &mut Context) {
        let Some(p) = &self.pressed else { return };
        let (Some(r), Some(action)) = (p.slider, self.rows.choices.get(p.index).cloned()) else {
            return;
        };
        let t = ((x - r.x - 11.) / (r.w - 22.)).clamp(0., 1.);
        if action == "jukevolume" {
            let enabled = ctx.prefs.music_enabled;
            ctx.prefs.set_music(t, enabled, ctx.settings);
            return;
        }
        if action == "jukeseek" {
            self.seek_to = Some(t);
            return;
        }
        let Some(s) = self.rows.slider(&action).cloned() else {
            return;
        };
        self.set_slider(&action, s.min + t * (s.max - s.min), ctx);
    }

    /// Pointer motion while pressed: drags a slider, or scrolls the panel
    /// (it follows the finger, with momentum after release).
    pub fn drag(
        &mut self,
        pointer: u64,
        x: f32,
        y: f32,
        max_scroll: f32,
        ctx: &mut Context,
    ) -> bool {
        let Some(p) = &mut self.pressed else {
            return false;
        };
        if p.pointer != pointer {
            return false;
        }
        if (x - p.x).hypot(y - p.y) > 8. {
            p.moved = true;
        }
        if p.slider.is_some() {
            self.drag_slider(x, ctx);
        } else {
            let viewport = self.scroller.viewport();
            self.scroller.set_bounds(max_scroll, viewport);
            if self.scroller.drag(y, crate::scroll::now())
                && let Some(p) = &mut self.pressed
            {
                p.moved = true;
            }
        }
        true
    }

    pub fn release(
        &mut self,
        hits: &[(Rect, u32)],
        pointer: u64,
        x: f32,
        y: f32,
        ctx: &mut Context,
    ) -> Option<Event> {
        let p = self.pressed.take_if(|p| p.pointer == pointer)?;
        // The Jukebox ranges apply on release (`onchange`).
        if p.slider.is_some() {
            return match self.rows.choices.get(p.index).map(String::as_str) {
                Some("jukevolume") => Some(Event::Changed),
                Some("jukeseek") => {
                    let t = self.seek_to.take()?;
                    let duration = ctx.music.map_or(0., |m| m.duration);
                    Some(Event::Jukebox(super::jukebox::JukeboxAction::Seek(
                        t * duration,
                    )))
                }
                _ => None,
            };
        }
        self.scroller.release(crate::scroll::now());
        if p.moved || p.caught {
            return None;
        }
        let (index, _) = Self::hit(hits, x, y)?;
        if index != p.index {
            return None;
        }
        self.selection = index;
        self.key.clear();
        self.follow_binding_row();
        let action = self.rows.choices.get(index)?.clone();
        self.run(&action, 1, false, ctx)
    }

    pub fn wheel(&mut self, dy: f32, max_scroll: f32) {
        let viewport = self.scroller.viewport();
        self.scroller.set_bounds(max_scroll, viewport);
        self.scroller.wheel(dy);
    }

    pub fn cancel_pointer(&mut self) {
        self.pressed = None;
        self.hovered = None;
        self.scroller.cancel();
    }

    /// Keeps the selected row inside the scrolled panel: it glides there
    /// (or jumps when `animate` is off: reduced motion). The panel's scroll
    /// range is `max_scroll`.
    pub fn reveal(&mut self, hits: &[(Rect, u32)], max_scroll: f32, animate: bool) {
        let panel = hits.iter().find(|(_, id)| *id == PANEL).map(|(r, _)| *r);
        let row = hits
            .iter()
            .find(|(_, id)| *id == ROW + self.selection as u32)
            .map(|(r, _)| *r);
        if let (Some(panel), Some(row)) = (panel, row) {
            self.scroller.set_bounds(max_scroll, panel.h);
            let before = self.scroller.offset();
            let target = self.scroller.target();
            // Rows sit at their layout place minus the drawn offset.
            let (top, bottom) = (row.y + before, row.bottom() + before);
            if top < target + panel.y {
                self.scroller.glide_to(top - panel.y, animate);
            } else if bottom > target + panel.bottom() {
                self.scroller.glide_to(bottom - panel.bottom(), animate);
            }
        }
    }
}

impl Default for PauseMenu {
    fn default() -> Self {
        Self::new()
    }
}

fn save_bindings(bindings: &Bindings, settings: &mut Settings) -> bool {
    settings.set(STORAGE_KEY, &bindings.to_json())
}

/// The ride's saved bindings and controller tuning (play.html's keys).
pub fn load_bindings(settings: &mut Settings) -> Bindings {
    let (bindings, migrated) = Bindings::load(
        settings.get(STORAGE_KEY).as_deref(),
        settings.get(CONTROLLER_STORAGE_KEY).as_deref(),
    );
    if migrated {
        save_bindings(&bindings, settings);
    }
    bindings
}

/// gamepads.js padName: the readable pad name, without Chrome's
/// "(STANDARD GAMEPAD Vendor: …)" or XInput groups and Firefox's
/// "054c-0ce6-" prefix, at most 48 characters.
pub fn pad_name(id: &str) -> String {
    let mut out = String::new();
    let mut rest = id;
    // Drop each innermost "( … )" group that names the mapping or vendor.
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open + 1..].find(['(', ')']).map(|i| open + 1 + i) else {
            break;
        };
        if rest.as_bytes()[close] == b'(' {
            out.push_str(&rest[..close]);
            rest = &rest[close..];
            continue;
        }
        let inner = rest[open + 1..close].to_ascii_lowercase();
        if ["standard gamepad", "vendor:", "xinput"]
            .iter()
            .any(|k| inner.contains(k))
        {
            out.push_str(rest[..open].trim_end());
            out.push(' ');
            rest = rest[close + 1..].trim_start();
        } else {
            out.push_str(&rest[..=close]);
            rest = &rest[close + 1..];
        }
    }
    out.push_str(rest);
    let mut name = out.as_str();
    // Firefox: "054c-0ce6-" vendor-product prefix.
    let hex = |s: &str| (1..=4).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit());
    let mut parts = name.splitn(3, '-');
    if let (Some(a), Some(b), Some(tail)) = (parts.next(), parts.next(), parts.next())
        && hex(a)
        && hex(b)
    {
        name = tail;
    }
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = if name.is_empty() {
        "Controller 1".to_string()
    } else {
        name
    };
    name.chars().take(48).collect()
}

/// frontend.js music_status: "On / Volume 45% / <song>".
fn music_status(m: &super::jukebox::MusicView) -> String {
    format!(
        "{} / Volume {}% / {}",
        if m.enabled { "On" } else { "Off" },
        (m.volume * 100.).round() as i32,
        m.playing_index
            .and_then(|i| m.songs.get(i))
            .map_or("Choose a song", |s| s.0.as_str())
    )
}

#[cfg(test)]
mod pad_name_tests {
    use super::pad_name;

    // Values from web/gamepads.js padName on the same ids.
    #[test]
    fn matches_the_browser() {
        for (id, name) in [
            (
                "Xbox 360 compliant game controller (STANDARD GAMEPAD Vendor: 045e Product: 028e)",
                "Xbox 360 compliant game controller",
            ),
            ("054c-0ce6-Wireless Controller", "Wireless Controller"),
            (
                "Xbox Wireless Controller (XInput STANDARD GAMEPAD)",
                "Xbox Wireless Controller",
            ),
            (
                "Pro Controller (Nintendo) (Vendor: 057e Product: 2009)",
                "Pro Controller (Nintendo)",
            ),
            ("", "Controller 1"),
            (
                "A very long controller name that goes on and on beyond forty eight characters",
                "A very long controller name that goes on and on ",
            ),
        ] {
            assert_eq!(pad_name(id), name, "{id}");
        }
    }
}

#[cfg(test)]
mod pad_select_tests {
    use super::*;
    use crate::{hud::minimap::MinimapSettings, menu::prefs::Preferences, touch::TouchControls};

    #[test]
    fn padselect_cycles_auto_and_connected_pads_and_labels_the_pin() {
        let mut settings = Settings::memory();
        let mut prefs = Preferences::load(&settings, false);
        let mut minimap = MinimapSettings::load(&settings);
        let mut touch = TouchControls::new(&settings, false);
        let mut bindings = load_bindings(&mut settings);
        let pads = vec![
            ("Virtual Pad".to_string(), 0),
            (
                "Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e)".to_string(),
                1,
            ),
        ];
        let mut menu = PauseMenu::new();
        let mut label =
            |menu: &mut PauseMenu, bindings: &mut Bindings, pads: &[(String, u32)], step: i32| {
                let mut ctx = Context {
                    settings: &mut settings,
                    prefs: &mut prefs,
                    minimap: &mut minimap,
                    touch: &mut touch,
                    bindings: Some(bindings),
                    pad_name: Some(pads[0].0.clone()),
                    pads: pads.to_vec(),
                    pad_standard: true,
                    view: Default::default(),
                    lobby: None,
                    music: None,
                };
                if step != 0 {
                    menu.run("padselect", step, true, &mut ctx);
                }
                menu.show("controller-settings");
                menu.refresh(&ctx);
                menu.rows.label("padselect").unwrap_or_default().to_string()
            };
        assert_eq!(
            label(&mut menu, &mut bindings, &pads, 0),
            "Controller: Auto (Virtual Pad)"
        );
        // Auto → the second pad (Left wraps from Auto to the last pad).
        assert_eq!(
            label(&mut menu, &mut bindings, &pads, -1),
            "Controller: Xbox Wireless Controller"
        );
        assert_eq!(bindings.tuning.selected, Some((pads[1].0.clone(), 1)));
        // Absent: Auto stands in until it reconnects.
        assert_eq!(
            label(&mut menu, &mut bindings, &pads[..1], 0),
            "Controller: Xbox Wireless Controller (not connected, using Auto)"
        );
        // Right from the pinned pad wraps to Auto, and the choice is saved.
        assert_eq!(
            label(&mut menu, &mut bindings, &pads, 1),
            "Controller: Auto (Virtual Pad)"
        );
        assert_eq!(bindings.tuning.selected, None);
        let saved = settings.get(CONTROLLER_STORAGE_KEY).unwrap();
        assert!(saved.contains("\"selected\":null"), "{saved}");
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use crate::{
        hud::minimap::MinimapSettings,
        menu::{jukebox::MusicView, prefs::Preferences},
        touch::TouchControls,
    };

    /// A menu's settings, bindings and soundtrack view.
    struct Fixture {
        settings: Settings,
        prefs: Preferences,
        minimap: MinimapSettings,
        touch: TouchControls,
        bindings: Bindings,
        music: MusicView,
    }

    impl Fixture {
        fn new() -> Self {
            let mut settings = Settings::memory();
            Self {
                prefs: Preferences::load(&settings, false),
                minimap: MinimapSettings::load(&settings),
                touch: TouchControls::new(&settings, false),
                bindings: load_bindings(&mut settings),
                music: MusicView {
                    ready: true,
                    enabled: true,
                    songs: (0..9)
                        .map(|i| (format!("Song {i}"), "Artist".into(), String::new()))
                        .collect(),
                    playing_index: Some(0),
                    ..Default::default()
                },
                settings,
            }
        }

        /// Runs `f` with a context, the rows refreshed before and after.
        fn with<T>(
            &mut self,
            menu: &mut PauseMenu,
            f: impl FnOnce(&mut PauseMenu, &mut Context) -> T,
        ) -> T {
            let mut ctx = Context {
                settings: &mut self.settings,
                prefs: &mut self.prefs,
                minimap: &mut self.minimap,
                touch: &mut self.touch,
                bindings: Some(&mut self.bindings),
                pad_name: None,
                pads: Vec::new(),
                pad_standard: true,
                view: Viewport {
                    width: 1280.,
                    height: 720.,
                    ..Default::default()
                },
                lobby: None,
                music: Some(&self.music),
            };
            menu.refresh(&ctx);
            let out = f(menu, &mut ctx);
            menu.refresh(&ctx);
            out
        }
    }

    fn pad(buttons: &[usize]) -> Pad {
        let mut pad = Pad::default();
        for b in buttons {
            pad.buttons[*b] = 1.;
        }
        pad
    }

    #[test]
    fn only_the_pause_binding_or_escape_resumes_and_shift_tab_goes_back() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        // The default Pause keys (Escape, P) resume.
        assert_eq!(
            fx.with(&mut menu, |m, ctx| m.key("KeyP", ctx)),
            Some(Event::Resume)
        );
        fx.with(&mut menu, |m, ctx| m.key_up("KeyP", ctx));
        // Pause rebound to K: P does nothing, K and Escape resume.
        fx.bindings.set_binding(
            true,
            Action::Pause,
            vec![Binding::One(Atom::Key("KeyK".into()))],
        );
        assert_eq!(fx.with(&mut menu, |m, ctx| m.key("KeyP", ctx)), None);
        fx.with(&mut menu, |m, ctx| m.key_up("KeyP", ctx));
        assert_eq!(menu.page, "pause");
        assert_eq!(
            fx.with(&mut menu, |m, ctx| m.key("KeyK", ctx)),
            Some(Event::Resume)
        );
        fx.with(&mut menu, |m, ctx| m.key_up("KeyK", ctx));
        assert_eq!(
            fx.with(&mut menu, |m, ctx| m.key("Escape", ctx)),
            Some(Event::Resume)
        );
        fx.with(&mut menu, |m, ctx| m.key_up("Escape", ctx));
        // Tab walks forward, Shift+Tab backward (wrapping to the last row).
        let rows = menu.rows.choices.len();
        fx.with(&mut menu, |m, ctx| m.key("Tab", ctx));
        assert_eq!(menu.selection, 1);
        fx.with(&mut menu, |m, ctx| m.key("ShiftLeft", ctx));
        fx.with(&mut menu, |m, ctx| m.key("Tab", ctx));
        fx.with(&mut menu, |m, ctx| m.key("Tab", ctx));
        assert_eq!(menu.selection, rows - 1);
    }

    #[test]
    fn back_returns_to_the_row_it_left_and_jukebox_waits_for_its_catalog() {
        let mut fx = Fixture::new();
        fx.music.ready = false;
        let mut menu = PauseMenu::new();
        fx.with(&mut menu, |_, _| ());
        // The Jukebox row shows before the soundtrack catalog is ready.
        assert!(menu.rows.choices.iter().any(|c| c == "jukebox"));
        assert!(menu.rows.choices.iter().any(|c| c == "copyride"));
        let row = |menu: &PauseMenu, action: &str| {
            menu.rows.choices.iter().position(|c| c == action).unwrap()
        };
        let settings = row(&menu, "settings");
        menu.selection = settings;
        fx.with(&mut menu, |m, ctx| m.key("Enter", ctx));
        fx.with(&mut menu, |_, _| ());
        assert_eq!(menu.page, "settings");
        let controls = row(&menu, "controls");
        menu.selection = controls;
        fx.with(&mut menu, |m, ctx| m.key("Enter", ctx));
        fx.with(&mut menu, |_, _| ());
        assert_eq!(menu.history().collect::<Vec<_>>(), ["pause", "settings"]);
        fx.with(&mut menu, |m, ctx| m.key("Escape", ctx));
        fx.with(&mut menu, |_, _| ());
        assert_eq!((menu.page.as_str(), menu.selection), ("settings", controls));
        fx.with(&mut menu, |m, ctx| m.key("Escape", ctx));
        fx.with(&mut menu, |_, _| ());
        assert_eq!((menu.page.as_str(), menu.selection), ("pause", settings));
        // Back on Pause resumes the ride.
        assert_eq!(
            fx.with(&mut menu, |m, ctx| m.key("Escape", ctx)),
            Some(Event::Resume)
        );
    }

    #[test]
    fn ecosystem_collision_asks_before_turning_off() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        fx.with(&mut menu, |m, ctx| m.run("settings", 1, false, ctx));
        fx.with(&mut menu, |m, ctx| m.run("game-settings", 1, false, ctx));
        assert_eq!(
            menu.rows.choices,
            [
                "measurement",
                "ecosystem",
                "sound-configuration",
                "new-profiles",
                "back"
            ]
        );
        fx.with(&mut menu, |m, ctx| m.run("ecosystem", 1, false, ctx));
        assert_eq!(menu.page, "ecosystem-warning");
        fx.with(&mut menu, |m, ctx| m.run("ecosystem-no", 1, false, ctx));
        assert_eq!((menu.page.as_str(), menu.selection), ("game-settings", 1));
        assert!(fx.prefs.ecosystem_collision);
        fx.with(&mut menu, |m, ctx| m.run("ecosystem", 1, false, ctx));
        fx.with(&mut menu, |m, ctx| m.run("ecosystem-yes", 1, false, ctx));
        assert!(!fx.prefs.ecosystem_collision);
        assert_eq!(
            fx.settings.get("atv-ecosystem-collision").as_deref(),
            Some("off")
        );
        // Turning it back on does not ask.
        fx.with(&mut menu, |m, ctx| m.run("ecosystem", 1, false, ctx));
        assert_eq!(menu.page, "game-settings");
        assert!(fx.prefs.ecosystem_collision);
        fx.with(&mut menu, |m, ctx| m.run("ecosystem", 1, false, ctx));
        fx.with(&mut menu, |m, ctx| m.key("Escape", ctx));
        assert_eq!(menu.page, "game-settings");
        fx.with(&mut menu, |m, ctx| m.key("Escape", ctx));
        assert_eq!(menu.page, "settings");
    }

    #[test]
    fn each_page_shown_slides_its_panel_in() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        let opened = menu.shown;
        fx.with(&mut menu, |m, ctx| m.run("settings", 1, false, ctx));
        assert!(menu.shown > opened, "Settings is a new page");
        let settings = menu.shown;
        fx.with(&mut menu, |m, ctx| m.key("ArrowDown", ctx));
        assert_eq!(menu.shown, settings, "moving the selection is not a page");
        fx.with(&mut menu, |m, ctx| m.key("Escape", ctx));
        assert_eq!(menu.page, "pause");
        assert!(menu.shown > settings, "Back shows Pause again");

        let view = Viewport {
            width: 1280.,
            height: 720.,
            ..Default::default()
        };
        let render = |state: &crate::menu::State| {
            let (mut list, mut hits) = (crate::DisplayList::default(), Vec::new());
            crate::layout::render(
                &crate::menu::element(&view, state, &Default::default()),
                &mut crate::Fonts::new(),
                1280.,
                720.,
                &mut list,
                &mut hits,
            );
            (list, hits)
        };
        let mut state = fx.with(&mut menu, |m, ctx| m.state(ctx));
        let (rest, rest_hits) = render(&state);
        state.motion = crate::transition::enter(0.);
        let (list, hits) = render(&state);
        let panel = |hits: &[(Rect, u32)]| {
            hits.iter()
                .find(|(_, id)| *id == crate::menu::PANEL)
                .unwrap()
                .0
        };
        assert_eq!(panel(&hits).x - panel(&rest_hits).x, 36.);
        // The dim backdrop stays; the stage starts transparent.
        assert_eq!(list.items[0], rest.items[0]);
        assert_eq!(list.items[1], crate::paint::Item::PushOpacity(0.));
    }

    #[test]
    fn a_race_offers_restart_race() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        fx.with(&mut menu, |_, _| ());
        assert_eq!(menu.rows.label("reset"), Some("Reset ATV"));
        let mut race = PauseMenu::new();
        race.race_active = true;
        fx.with(&mut race, |_, _| ());
        assert_eq!(race.rows.label("reset"), Some("Restart Race"));
        assert_eq!(
            fx.with(&mut race, |m, ctx| m.run("reset", 1, false, ctx)),
            Some(Event::Reset)
        );
    }

    #[test]
    fn clear_binding_follows_the_highlighted_row() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        fx.with(&mut menu, |m, ctx| m.run("keyboard", 1, false, ctx));
        // Down twice: the third action (Steer left) is highlighted.
        fx.with(&mut menu, |m, ctx| m.key("ArrowDown", ctx));
        fx.with(&mut menu, |m, ctx| m.key("ArrowDown", ctx));
        assert_eq!(menu.selected(), Some("bind:steerLeft"));
        fx.with(&mut menu, |m, ctx| m.run("bindclear", 1, false, ctx));
        assert!(fx.bindings.row(true, Action::SteerLeft).is_empty());
        assert!(!fx.bindings.row(true, Action::Throttle).is_empty());
        // A tap on a row moves it too; the press alone does not (a press
        // that turns into a scroll must not select).
        let hits = vec![(Rect::new(0., 0., 100., 40.), crate::menu::ROW + 1)];
        fx.with(&mut menu, |m, ctx| m.press(&hits, 1, 10., 10., ctx));
        assert_eq!(menu.binding_row, 2);
        fx.with(&mut menu, |m, ctx| m.release(&hits, 1, 10., 10., ctx));
        assert_eq!(menu.binding_row, 1);
    }

    #[test]
    fn left_right_turn_list_pages_and_change_jukebox_tracks() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        fx.with(&mut menu, |m, ctx| m.run("keyboard", 1, false, ctx));
        assert_eq!(menu.selected(), Some("bind:throttle"));
        fx.with(&mut menu, |m, ctx| m.key("ArrowRight", ctx));
        assert!(menu.capture.is_none(), "Right does not start a capture");
        assert_eq!(menu.binding_page, 1);
        fx.with(&mut menu, |m, ctx| m.key("ArrowLeft", ctx));
        assert_eq!(menu.binding_page, 0);
        // Audio: a song row turns the page instead of playing the song.
        let mut audio = PauseMenu::new();
        fx.with(&mut audio, |m, _| m.show("music"));
        let song = audio
            .rows
            .choices
            .iter()
            .position(|c| c.starts_with("song:"))
            .unwrap();
        audio.selection = song;
        let event = fx.with(&mut audio, |m, ctx| m.key("ArrowRight", ctx));
        assert_eq!(event, Some(Event::Changed));
        assert_eq!(audio.music_page(), 1);
        // Jukebox: Left/Right on a track row change track.
        let mut juke = PauseMenu::new();
        fx.with(&mut juke, |m, _| m.show("jukebox"));
        let song = juke
            .rows
            .choices
            .iter()
            .position(|c| c.starts_with("song:"))
            .unwrap();
        juke.selection = song;
        assert_eq!(
            fx.with(&mut juke, |m, ctx| m.key("ArrowRight", ctx)),
            Some(Event::Jukebox(crate::menu::jukebox::JukeboxAction::Next))
        );
        assert_eq!(
            fx.with(&mut juke, |m, ctx| m.key("ArrowLeft", ctx)),
            Some(Event::Jukebox(
                crate::menu::jukebox::JukeboxAction::Previous
            ))
        );
    }

    #[test]
    fn options_from_the_main_menu_leads_with_the_jukebox_and_back_returns_to_it() {
        let mut fx = Fixture::new();
        let mut options = PauseMenu::settings();
        fx.with(&mut options, |_, _| ());
        assert_eq!(
            options.rows.choices[..3],
            ["jukebox", "game-settings", "controls"]
        );
        fx.with(&mut options, |m, ctx| m.key("Enter", ctx));
        assert_eq!(options.page, "jukebox");
        // Back leaves the song paused and returns to Options on the row.
        assert_eq!(
            fx.with(&mut options, |m, ctx| m.key("Escape", ctx)),
            Some(Event::Jukebox(crate::menu::jukebox::JukeboxAction::Leave))
        );
        assert_eq!((options.page.as_str(), options.selection), ("settings", 0));
        // Back on Options closes it; Options over a ride has no Jukebox row.
        assert_eq!(
            fx.with(&mut options, |m, ctx| m.key("Escape", ctx)),
            Some(Event::Resume)
        );
        let mut ride = PauseMenu::new();
        fx.with(&mut ride, |m, ctx| m.run("settings", 1, false, ctx));
        assert!(ride.rows.choices.iter().all(|c| c != "jukebox"));
    }

    #[test]
    fn start_accepts_over_the_main_menu_and_resumes_a_ride() {
        let mut fx = Fixture::new();
        let mut settings = PauseMenu::settings();
        fx.with(&mut settings, |m, ctx| m.pad(Some(&pad(&[])), ctx));
        let event = fx.with(&mut settings, |m, ctx| m.pad(Some(&pad(&[9])), ctx));
        assert_eq!(event, None, "Start accepted the first row");
        assert_ne!(settings.page, "settings");
        let mut ride = PauseMenu::new();
        fx.with(&mut ride, |m, ctx| m.pad(Some(&pad(&[])), ctx));
        assert_eq!(
            fx.with(&mut ride, |m, ctx| m.pad(Some(&pad(&[9])), ctx)),
            Some(Event::Resume)
        );
    }

    #[test]
    fn the_pause_binding_resumes() {
        let mut fx = Fixture::new();
        fx.bindings.set_binding(
            true,
            Action::Pause,
            vec![Binding::One(Atom::Key("KeyQ".into()))],
        );
        fx.bindings
            .set_binding(false, Action::Pause, vec![Binding::One(Atom::Button(8))]);
        let mut menu = PauseMenu::new();
        assert_eq!(
            fx.with(&mut menu, |m, ctx| m.key("KeyQ", ctx)),
            Some(Event::Resume)
        );
        let mut sub = PauseMenu::new();
        fx.with(&mut sub, |m, _| m.show("settings"));
        assert_eq!(fx.with(&mut sub, |m, ctx| m.key("KeyQ", ctx)), None);
        // The controller binding resumes from any ride menu page.
        fx.with(&mut sub, |m, ctx| m.pad(Some(&pad(&[])), ctx));
        assert_eq!(
            fx.with(&mut sub, |m, ctx| m.pad(Some(&pad(&[8])), ctx)),
            Some(Event::Resume)
        );
    }

    #[test]
    fn capture_keeps_its_row_and_blur_cancels_it() {
        let mut fx = Fixture::new();
        let mut menu = PauseMenu::new();
        fx.with(&mut menu, |m, ctx| m.run("keyboard", 1, false, ctx));
        fx.with(&mut menu, |m, ctx| m.key("ArrowDown", ctx));
        fx.with(&mut menu, |m, ctx| m.run("bind:brake", 1, false, ctx));
        assert!(menu.capture.is_some());
        fx.with(&mut menu, |m, ctx| m.key("KeyJ", ctx));
        fx.with(&mut menu, |m, ctx| m.key_up("KeyJ", ctx));
        assert!(menu.capture.is_none());
        assert_eq!(menu.selected(), Some("bind:brake"));
        assert_eq!(
            fx.bindings.row(true, Action::Brake),
            &[Binding::One(Atom::Key("KeyJ".into()))]
        );
        fx.with(&mut menu, |m, ctx| m.run("bind:brake", 1, false, ctx));
        fx.with(&mut menu, |m, _| m.cancel_capture());
        assert!(menu.capture.is_none());
        assert_eq!(menu.selected(), Some("bind:brake"));
    }
}
