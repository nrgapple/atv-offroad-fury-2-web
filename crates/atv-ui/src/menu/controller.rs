//! Pause menu state: page, selection, navigation and setting actions
//! (frontend.js `show`/`run`/`action` for the ride's pause screens). Rows come
//! from atv_core::ui_menu with the same request fields the browser sends.
use super::{PANEL, ROW, Rows, State, prefs::Preferences};
use crate::{
    hud::{Viewport, minimap::MinimapSettings},
    paint::Rect,
    settings::Settings,
    touch::TouchControls,
};
use atv_game::ride::bindings::{
    self as binds, ACTIONS, Action, Atom, Binding, Bindings, CONTROLLER_STORAGE_KEY, STORAGE_KEY,
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
    pub view: Viewport,
    /// The multiplayer room this ride is in.
    pub lobby: Option<&'a Lobby>,
    /// The ride's soundtrack player (the Jukebox needs it).
    pub music: Option<&'a super::jukebox::MusicView>,
}

/// Rows the Rust engine cannot perform yet are left out of its menu.
const UNSUPPORTED: [&str; 1] = ["alpha"];

/// A binding being captured (frontend.js `beginCapture`).
struct Capture {
    action: Action,
    label: String,
    atoms: Vec<Atom>,
    held: Vec<String>,
    armed: bool,
}

pub struct PauseMenu {
    pub page: String,
    pub selection: usize,
    pub rows: Rows,
    pub scroll: f32,
    hovered: Option<usize>,
    pressed: Option<Pressed>,
    pad: u32,
    /// The rows were built for this request.
    key: String,
    /// After Back: select the row that opened the page just left.
    pending_select: Option<String>,
    /// Bindings page: keyboard (true) or controller rows, page and row.
    keyboard: bool,
    binding_page: usize,
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
}

struct Pressed {
    pointer: u64,
    index: usize,
    x: f32,
    y: f32,
    moved: bool,
    slider: Option<Rect>,
    scroll: f32,
}

/// Each page's Back target (frontend.js returns).
fn parent(page: &str) -> &'static str {
    match page {
        "settings" => "pause",
        "minimap-config" | "race-overlays" => "hud-settings",
        "controller-settings" | "bindings" => "controls",
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
            scroll: 0.,
            hovered: None,
            pressed: None,
            pad: u32::MAX,
            key: String::new(),
            pending_select: None,
            keyboard: true,
            binding_page: 0,
            binding_row: 0,
            capture: None,
            from_menu: false,
            frontend: false,
            notice: None,
            seek_to: None,
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

    pub fn state(&self, ctx: &Context) -> State {
        State {
            page: self.page.clone(),
            rows: self.rows.clone(),
            selection: self.selection,
            hovered: self.hovered,
            pressed: self.pressed.as_ref().map(|p| p.index),
            scroll: self.scroll,
            one_handed: ctx.touch.one_handed(),
            capture: self.capture.as_ref().map(|c| c.label.clone()),
            from_menu: self.from_menu,
            notice: self.notice.clone(),
            music: ctx.music.filter(|_| self.page == "jukebox").cloned(),
            jukebox_art: None,
            frame: 0,
            // Controller bindings show glyphs (standard-mapping pads).
            pad: (self.page == "bindings" && !self.keyboard)
                .then_some(ctx.bindings.as_deref())
                .flatten()
                .map(|b| super::glyphs::PadGlyphs::new(b, true)),
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
            page_count: (self.page == "bindings").then(|| {
                format!(
                    "Page {} of {}",
                    self.binding_page + 1,
                    ACTIONS.len().div_ceil(4)
                )
            }),
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
            "race_overlays_off": p.overlays_off,
            "race_position": p.race_position,
            "funny_crashes": p.crash_mode != "original",
            "crash_mode": p.crash_mode,
            "original_camera": p.crash_camera_original,
            "chase_assist": p.chase_assist,
            "flips": p.flips,
            "realistic_train": p.realistic_train,
            "speed_rush": p.speed_rush,
            "graphics_preset": p.graphics,
            "graphics_resolved": p.graphics_resolved(),
            "vibration": p.vibration,
            "diagnostics_mode": p.diagnostics,
            "sound_mode": p.sound_mode,
            "sfx_volume": (p.sfx_volume * 100.).round() as usize,
            "music_enabled": p.music_enabled && !(self.page == "jukebox" && ctx.music.is_some_and(|m| m.paused)),
            "music_volume": (p.music_volume * 100.).round() as usize,
            "music_rows": if self.page == "jukebox" {
                ctx.music.map_or_else(Vec::new, |m| m.songs.iter().enumerate().map(|(i, (title, artist, _))| {
                    json!({"id": i.to_string(), "label": title, "value": artist})
                }).collect())
            } else {
                Vec::new()
            },
            "minimap_enabled": m.enabled,
            "minimap_size": m.size,
            "minimap_zoom": m.zoom,
            "minimap_shape": if m.shape == crate::hud::minimap::MapShape::Circle { "circle" } else { "square" },
            "minimap_corner": if m.bottom_left { "bottom-left" } else { "top-right" },
            "minimap_orientation": if m.heading_up { "heading-up" } else { "north-up" },
            "mobile_steering_sensitivity": ctx.touch.input.steering_sensitivity,
            "one_handed_mobile": ctx.touch.one_handed(),
            "fullscreen": true,
        });
        let mut request = request;
        if let Some(b) = ctx.bindings.as_deref() {
            let t = &b.tuning;
            request["controller_label"] = match &ctx.pad_name {
                Some(name) => format!("Auto ({name})"),
                None => "Auto (press a button)".into(),
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
                            "value": binds::binding_text(b.row(self.keyboard, *action), true),
                        })
                    })
                    .collect();
            }
        }
        if let Some(c) = &self.capture {
            request["capture_label"] = c.label.clone().into();
        }
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
            // The Jukebox plays the ride's soundtrack: rides with music only.
            if !ctx.music.is_some_and(|m| m.ready) || self.from_menu {
                rows.choices.retain(|c| c != "jukebox");
            }
            if !cfg!(target_arch = "wasm32") {
                // Desktop has no dev log page to open, and no web page to
                // link a ride to (a room's invite link is shown instead).
                let online = ctx.lobby.is_some();
                rows.choices
                    .retain(|c| c != "devlog" && (online || c != "copyride"));
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
            self.selection = self.selection.min(rows.choices.len().saturating_sub(1));
            self.rows = rows;
        }
        self.key = key;
        if let Some(from) = self.pending_select.take()
            && let Some(i) = self.rows.choices.iter().position(|c| *c == from)
        {
            self.selection = i;
            self.refresh(ctx);
        }
    }

    fn show(&mut self, page: &str) {
        self.capture = None;
        self.notice = None;
        self.page = page.into();
        self.selection = 0;
        self.scroll = 0.;
        self.hovered = None;
        self.key.clear();
    }

    fn back(&mut self) -> Option<Event> {
        if self.page == "pause" || (self.from_menu && self.page == "settings") {
            return Some(Event::Resume);
        }
        if self.page == "jukebox" {
            self.show("pause");
            self.pending_select = Some("jukebox".into());
            return Some(Event::Jukebox(super::jukebox::JukeboxAction::Leave));
        }
        let to = if self.page == "bindings" && !self.keyboard {
            "controller-settings"
        } else {
            parent(&self.page)
        };
        // Return to the row that opened the page.
        let from = match self.page.as_str() {
            "bindings" if self.keyboard => "keyboard".to_string(),
            "bindings" => "padbindings".to_string(),
            "controller-settings" => "controller".to_string(),
            page => page.to_string(),
        };
        self.show(to);
        self.pending_select = Some(from);
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
            "settings" | "extras" | "hud-settings" | "minimap-config" | "race-overlays"
            | "controls" | "music" => {
                self.show(action);
                return None;
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
            "bindprev" | "bindnext" => {
                let pages = ACTIONS.len().div_ceil(4);
                let step = if action == "bindprev" { pages - 1 } else { 1 };
                self.binding_page = (self.binding_page + step) % pages;
                self.binding_row = self.binding_page * 4;
                self.selection = 0;
                Event::Changed
            }
            "bindcancel" => {
                self.capture = None;
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
            "padselect" => Event::Changed,
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
                });
                if let Some(b) = ctx.bindings.as_deref_mut() {
                    b.cancel();
                }
                self.selection = 0;
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
        }
    }

    /// Keyboard input while the menu is open (winit key code names).
    pub fn key(&mut self, code: &str, ctx: &mut Context) -> Option<Event> {
        if let Some(c) = &mut self.capture {
            if code == "Escape" {
                self.capture = None;
                self.key.clear();
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
        match code {
            "ArrowUp" | "KeyW" => self.step(-1),
            "ArrowDown" | "KeyS" | "Tab" => self.step(1),
            "ArrowLeft" | "ArrowRight" | "KeyA" | "KeyD" => {
                let direction = if matches!(code, "ArrowLeft" | "KeyA") {
                    -1
                } else {
                    1
                };
                let action = self.selected()?.to_string();
                // Left/Right step sliders and cycle valued rows; never navigate.
                let valued = self.rows.label(&action).is_some_and(|l| l.contains(": "));
                if self.rows.slider(&action).is_some() || valued || action == "jukeseek" {
                    return self.run(&action, direction, true, ctx);
                }
            }
            "Enter" | "Space" | "NumpadEnter" => {
                let action = self.selected()?.to_string();
                return self.run(&action, 1, false, ctx);
            }
            "Escape" | "Backspace" | "KeyP" => {
                if code == "KeyP" && self.page != "pause" {
                    return None;
                }
                return self.back();
            }
            _ => {}
        }
        None
    }

    /// Key release: a keyboard capture saves once every key is up.
    pub fn key_up(&mut self, code: &str, ctx: &mut Context) -> Option<Event> {
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
        let c = self.capture.take()?;
        let binding = if c.atoms.len() == 1 {
            Binding::One(c.atoms[0].clone())
        } else {
            Binding::Chord(c.atoms)
        };
        let bindings = ctx.bindings.as_deref_mut()?;
        bindings.set_binding(self.keyboard, c.action, vec![binding]);
        save_bindings(bindings, ctx.settings);
        self.key.clear();
        Some(Event::Changed)
    }

    /// frontend.js `pollCapture`: buttons and axes held together become one
    /// binding when all are released.
    fn capture_pad(
        &mut self,
        buttons: &[f32; 17],
        axes: &[f32; 4],
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
    /// accepts, B/Y go back, Start resumes.
    pub fn pad(
        &mut self,
        buttons: [f32; 17],
        axes: [f32; 4],
        connected: bool,
        ctx: &mut Context,
    ) -> Option<Event> {
        if self.capture.is_some() {
            if self.keyboard {
                // A pad press does not navigate under a keyboard capture.
                return None;
            }
            return self.capture_pad(&buttons, &axes, connected, ctx);
        }
        let mut held = 0u32;
        let mut set = |bit: u32, on: bool| {
            if on {
                held |= 1 << bit;
            }
        };
        set(0, buttons[0] > 0.5);
        set(1, buttons[1] > 0.5 || buttons[3] > 0.5);
        set(2, buttons[9] > 0.5);
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
                event = event.or(self.key(code, ctx));
            }
        }
        if fresh & 1 << 2 != 0 {
            event = event.or(Some(Event::Resume));
        }
        event
    }

    fn hit(hits: &[(Rect, u32)], x: f32, y: f32) -> Option<(usize, Rect)> {
        hits.iter()
            .rev()
            .find(|(r, id)| *id >= ROW && r.contains(x, y))
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
        if let Some((index, _)) = target {
            self.selection = index;
            self.key.clear();
        }
        self.pressed = Some(Pressed {
            pointer,
            index: target.map_or(usize::MAX, |(i, _)| i),
            x,
            y,
            moved: false,
            slider,
            scroll: self.scroll,
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

    /// Pointer motion while pressed: drags a slider, or scrolls the panel.
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
        } else if p.moved {
            self.scroll = (p.scroll + p.y - y).clamp(0., max_scroll.max(0.));
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
        if p.moved {
            return None;
        }
        let (index, _) = Self::hit(hits, x, y)?;
        if index != p.index {
            return None;
        }
        let action = self.rows.choices.get(index)?.clone();
        self.run(&action, 1, false, ctx)
    }

    pub fn wheel(&mut self, dy: f32, max_scroll: f32) {
        self.scroll = (self.scroll + dy).clamp(0., max_scroll.max(0.));
    }

    pub fn cancel_pointer(&mut self) {
        self.pressed = None;
        self.hovered = None;
    }

    /// Keeps the selected row inside the scrolled panel.
    pub fn reveal(&mut self, hits: &[(Rect, u32)]) {
        let panel = hits.iter().find(|(_, id)| *id == PANEL).map(|(r, _)| *r);
        let row = hits
            .iter()
            .find(|(_, id)| *id == ROW + self.selection as u32)
            .map(|(r, _)| *r);
        if let (Some(panel), Some(row)) = (panel, row) {
            if row.y < panel.y {
                self.scroll -= panel.y - row.y;
            } else if row.bottom() > panel.bottom() {
                self.scroll += row.bottom() - panel.bottom();
            }
            self.scroll = self.scroll.max(0.);
        }
    }
}

impl Default for PauseMenu {
    fn default() -> Self {
        Self::new()
    }
}

fn save_bindings(bindings: &Bindings, settings: &mut Settings) {
    settings.set(STORAGE_KEY, &bindings.to_json());
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
