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
}

/// The settings a pause action may change.
pub struct Context<'a> {
    pub settings: &'a mut Settings,
    pub prefs: &'a mut Preferences,
    pub minimap: &'a mut MinimapSettings,
    pub touch: &'a mut TouchControls,
    pub view: Viewport,
}

/// Rows the Rust engine cannot perform yet are left out of its menu.
const UNSUPPORTED: [&str; 5] = ["jukebox", "copyride", "keyboard", "controller", "alpha"];

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
        "pause" => "pause",
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
            "music_enabled": p.music_enabled,
            "music_volume": (p.music_volume * 100.).round() as usize,
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
                // Desktop has no dev log page or browser main menu to open.
                rows.choices.retain(|c| c != "devlog" && c != "main");
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
        self.page = page.into();
        self.selection = 0;
        self.scroll = 0.;
        self.hovered = None;
        self.key.clear();
    }

    fn back(&mut self) -> Option<Event> {
        if self.page == "pause" {
            return Some(Event::Resume);
        }
        let to = parent(&self.page);
        let from = std::mem::take(&mut self.page);
        self.show(to);
        // Return to the row that opened the page.
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
            "main" => return Some(Event::Leave("play.html")),
            "settings" | "extras" | "hud-settings" | "minimap-config" | "race-overlays"
            | "controls" | "music" => {
                self.show(action);
                return None;
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
            "minimap-size" | "minimap-zoom" | "steering-sensitivity" => {
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
                if self.rows.slider(&action).is_some() || valued {
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

    /// Standard-mapping controller buttons (edges): D-pad/stick move, A
    /// accepts, B/Y go back, Start resumes.
    pub fn pad(&mut self, buttons: [f32; 17], axes: [f32; 4], ctx: &mut Context) -> Option<Event> {
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
            self.rows.slider(action).map(|_| r)
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
        let Some(s) = self.rows.slider(&action).cloned() else {
            return;
        };
        let t = ((x - r.x - 11.) / (r.w - 22.)).clamp(0., 1.);
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
        if p.slider.is_some() || p.moved {
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
