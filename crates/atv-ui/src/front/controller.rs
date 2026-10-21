//! Frontend state: pages, selection, course filter, garage choice and input
//! (frontend.js `show`, `action`, `run` and the pointer/controller handling
//! in menu-screens.js). Rows and labels come from atv_core::ui_menu with the
//! same request fields the browser sends.
use super::{
    BACK, COURSE, DONE, GROUPS, Img, LIST, NEXT, PREV, QUAD_NEXT, QUAD_PREV, RIDE, RIDER, ROW,
    Screen, Stats, TAB, VEHICLE, assets::Track, course_group,
};
use crate::{hud::Viewport, paint::Rect, settings::Settings};
use serde_json::{Value, json};

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
    /// Open Settings over the frontend.
    Settings,
    /// Display → Fullscreen in Settings.
    Fullscreen,
}

/// localStorage key shared with the browser game.
pub const SELECTION_KEY: &str = "atv-player-selection";
/// The Rust engine rides ATVs; motorcycles are not ported yet.
pub const VEHICLES: u16 = 26;

/// Rows the Rust engine cannot perform yet are left out of its menus.
const UNSUPPORTED: [&str; 5] = ["multiplayer", "freeride", "jukebox", "race", "gear"];

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
    /// Dragging the list: the scroll at press.
    scroll: Option<f32>,
}

/// The course loader's state.
#[derive(Clone, Debug, Default)]
pub struct Loading {
    pub environment: String,
    pub track: String,
    pub status: String,
    pub progress: f32,
    pub tip: Vec<String>,
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
    pub list_scroll: f32,
    list_max: f32,
    hovered: Option<u32>,
    pressed: Option<Pressed>,
    pad: u32,
    garage_return: String,
    /// The vehicle names (ATVs 1-26) from ui_menu's catalog.
    pub catalog: Vec<(u16, String)>,
    choices: Vec<(String, String)>,
    /// Scroll the current card into view after the next layout.
    reveal: bool,
    pub loading: Option<Loading>,
    /// The browser dev log is a web page; the desktop build hides it.
    pub web: bool,
    /// Saved selection fields other than the ones this port changes.
    saved: Value,
}

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
        let mut front = Self {
            page: "title".into(),
            selection: 0,
            tracks,
            track,
            filter: "all".into(),
            quad_id,
            rider_id,
            list_scroll: 0.,
            list_max: 0.,
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
        };
        front.refresh(&Viewport {
            width: 1280.,
            height: 720.,
            ..Default::default()
        });
        front
    }

    /// Stores the garage choice and last Freestyle course, keeping the
    /// browser's other saved fields (race setup, name, gear).
    pub fn save(&mut self, settings: &mut Settings) {
        let mut value = self.saved.clone();
        value["quadId"] = self.quad_id.into();
        value["riderId"] = self.rider_id.into();
        if let Some(track) = self.tracks.get(self.track) {
            value["freestyleEnvironment"] = track.id.clone().into();
        }
        self.saved = value.clone();
        settings.set(SELECTION_KEY, &value.to_string());
    }

    /// ui_menu rows for the page (labels and keyboard order).
    pub fn refresh(&mut self, view: &Viewport) {
        let page = match self.page.as_str() {
            "tracks" | "loading" => "tracks",
            other => other,
        };
        let request = json!({
            "page": page,
            "width": view.width.clamp(240., 16384.),
            "height": view.height.clamp(240., 16384.),
            "selection": self.selection,
            "quad_id": self.quad_id,
            "rider_id": self.rider_id,
            "track_name": self.tracks.get(self.track).map_or("", |t| t.name.as_str()),
            "race_available": false,
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
                .filter(|c| c.group == "ATV" && c.id <= VEHICLES)
                .map(|c| (c.id, c.label.clone()))
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
            .filter(|a| self.web || a.as_str() != "devlog")
            .map(|a| (a.clone(), label(a)))
            .collect();
        self.selection = self.selection.min(self.choices.len().saturating_sub(1));
    }

    pub fn choices(&self) -> &[(String, String)] {
        &self.choices
    }

    fn current(&self) -> Option<&str> {
        self.choices.get(self.selection).map(|(a, _)| a.as_str())
    }

    pub fn show(&mut self, page: &str, view: &Viewport) {
        self.page = page.into();
        self.selection = 0;
        self.list_scroll = 0.;
        self.list_max = 0.;
        self.pressed = None;
        self.hovered = None;
        self.reveal = matches!(page, "tracks" | "garage");
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
        self.list_scroll = 0.;
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

    fn step_quad(&mut self, direction: i32) {
        let n = i32::from(VEHICLES);
        self.quad_id = ((i32::from(self.quad_id) - 1 + direction).rem_euclid(n) + 1) as u16;
        self.reveal = true;
    }

    fn ride(&mut self, settings: &mut Settings) -> Option<Event> {
        let track = self.tracks.get(self.track)?.clone();
        self.save(settings);
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
        });
    }

    /// A loader progress line; the bar never moves backwards.
    pub fn loading_stage(&mut self, text: &str) {
        if let Some(loading) = &mut self.loading {
            let (status, progress) = loading_stage(text);
            loading.status = status.into();
            loading.progress = loading.progress.max(progress);
        }
    }

    fn back(&mut self, view: &Viewport) {
        let to = match self.page.as_str() {
            "main" => "title".to_string(),
            "single" => "main".into(),
            "tracks" => "single".into(),
            "garage" => self.garage_return.clone(),
            _ => return,
        };
        let from = self.page.clone();
        self.show(&to, view);
        // Back lands on the row that opened the page just left.
        if let Some(index) = self.choices.iter().position(|(a, _)| *a == from) {
            self.selection = index;
        }
    }

    fn run(&mut self, action: &str, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        match action {
            "main" | "single" | "tracks" => self.show(action, view),
            "garage" => {
                self.garage_return = self.page.clone();
                self.show("garage", view);
            }
            "back" => self.back(view),
            "devlog" => return Some(Event::DevLog),
            "settings" => return Some(Event::Settings),
            "quadprev" | "quadnext" => {
                self.step_quad(if action == "quadnext" { 1 } else { -1 });
                self.save(settings);
                self.refresh(view);
            }
            "riderchange" => {
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
        if self.page == "loading" {
            return None;
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
            (_, "back") => self.back(view),
            (_, "accept") => {
                let action = self.current()?.to_string();
                return self.run(&action, view, settings);
            }
            _ => {}
        }
        None
    }

    /// Standard-mapping controller edges: D-pad/stick move, A or Start
    /// accepts, B/Y go back, LB/RB switch the course tabs.
    pub fn pad(
        &mut self,
        buttons: [f32; 17],
        axes: [f32; 4],
        view: &Viewport,
        settings: &mut Settings,
    ) -> Option<Event> {
        let mut held = 0u32;
        let mut set = |bit: u32, on: bool| {
            if on {
                held |= 1 << bit;
            }
        };
        set(0, buttons[0] > 0.5 || buttons[9] > 0.5);
        set(1, buttons[1] > 0.5 || buttons[3] > 0.5);
        set(3, buttons[12] > 0.5 || axes[1] < -0.55);
        set(4, buttons[13] > 0.5 || axes[1] > 0.55);
        set(5, buttons[14] > 0.5 || axes[0] < -0.55);
        set(6, buttons[15] > 0.5 || axes[0] > 0.55);
        set(7, buttons[4] > 0.5);
        set(8, buttons[5] > 0.5);
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
        event
    }

    fn listed(id: u32) -> bool {
        (COURSE..COURSE + 100).contains(&id) || (VEHICLE..VEHICLE + 100).contains(&id)
    }

    fn hit(hits: &[(Rect, u32)], x: f32, y: f32) -> Option<u32> {
        let list = hits.iter().find(|(_, id)| *id == LIST).map(|(r, _)| *r);
        hits.iter()
            .rev()
            // List items outside the scrolled viewport are clipped.
            .filter(|(r, id)| {
                r.contains(x, y) && !(Self::listed(*id) && list.is_some_and(|l| !l.contains(x, y)))
            })
            .map(|(_, id)| *id)
            .find(|id| *id != LIST)
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
        let in_list = hits.iter().any(|(r, id)| *id == LIST && r.contains(x, y));
        self.pressed = Some(Pressed {
            pointer,
            id: Self::hit(hits, x, y),
            x,
            y,
            moved: false,
            scroll: in_list.then_some(self.list_scroll),
        });
    }

    /// Touch/mouse drag: lists scroll with the finger.
    pub fn drag(&mut self, pointer: u64, x: f32, y: f32) {
        let max = self.list_max;
        let Some(p) = self.pressed.as_mut().filter(|p| p.pointer == pointer) else {
            return;
        };
        if (x - p.x).hypot(y - p.y) > 8. {
            p.moved = true;
        }
        if let (true, Some(start)) = (p.moved, p.scroll) {
            self.list_scroll = (start - (y - p.y)).clamp(0., max);
        }
    }

    pub fn cancel_pointer(&mut self) {
        self.pressed = None;
        self.hovered = None;
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
        let pressed = self.pressed.take().filter(|p| p.pointer == pointer)?;
        if pressed.moved && pressed.scroll.is_some() {
            return None;
        }
        let id = Self::hit(hits, x, y).filter(|id| Some(*id) == pressed.id)?;
        self.click(id, view, settings)
    }

    fn click(&mut self, id: u32, view: &Viewport, settings: &mut Settings) -> Option<Event> {
        match id {
            BACK | DONE => self.back(view),
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
                return self.run(&action, view, settings);
            }
            _ if (TAB..TAB + 10).contains(&id) => {
                let key = self.tabs().get((id - TAB) as usize)?.0.clone();
                self.set_filter(&key);
                self.refresh(view);
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
                if self.catalog.iter().any(|(q, _)| *q == quad) {
                    self.quad_id = quad;
                    self.save(settings);
                    self.refresh(view);
                }
            }
            _ => {}
        }
        None
    }

    pub fn wheel(&mut self, dy: f32) {
        self.list_scroll = (self.list_scroll + dy).clamp(0., self.list_max);
    }

    /// After layout: the list's scroll range, and the current card brought
    /// into view (`scrollIntoView({block: 'nearest'})`). True when the
    /// scroll moved (lay out again).
    pub fn laid_out(&mut self, hits: &[(Rect, u32)], padding_bottom: f32) -> bool {
        let Some(list) = hits.iter().find(|(_, id)| *id == LIST).map(|(r, _)| *r) else {
            return false;
        };
        let items: Vec<&(Rect, u32)> = hits.iter().filter(|(_, id)| Self::listed(*id)).collect();
        let bottom =
            items.iter().map(|(r, _)| r.bottom()).fold(list.y, f32::max) + self.list_scroll;
        self.list_max = (bottom + padding_bottom - list.bottom()).max(0.);
        let before = self.list_scroll;
        if std::mem::take(&mut self.reveal) {
            let current = if self.page == "garage" {
                VEHICLE + u32::from(self.quad_id)
            } else {
                let order = self.order();
                COURSE + order.iter().position(|&i| i == self.track).unwrap_or(0) as u32
            };
            if let Some((r, _)) = items.iter().find(|(_, id)| *id == current) {
                if r.y < list.y {
                    self.list_scroll -= list.y - r.y;
                } else if r.bottom() > list.bottom() {
                    self.list_scroll += r.bottom() - list.bottom();
                }
            }
        }
        self.list_scroll = self.list_scroll.clamp(0., self.list_max);
        self.list_scroll != before
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
        Screen {
            page: self.page.clone(),
            choices: self.choices.clone(),
            selection: self.selection,
            hovered: self.hovered,
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
            } else {
                Vec::new()
            },
            filter: self.filter.clone(),
            tabs: self.tabs(),
            current: track.map(|t| t.id.clone()),
            hero: track.and_then(|t| photo(&t.id)),
            hero_name: track.map_or(String::new(), |t| t.name.clone()),
            list_scroll: self.list_scroll,
            vehicle_label: self
                .catalog
                .iter()
                .find(|(q, _)| *q == self.quad_id)
                .map_or(format!("Vehicle {}", self.quad_id), |(_, l)| l.clone()),
            rider: self.rider_id,
            catalog: if self.page == "garage" {
                self.catalog.clone()
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
            progress: self.loading.as_ref().map_or(0., |l| l.progress),
            shot: shot.0,
            previous_shot: shot.1,
            shot_fade: 1.,
            pulse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks() -> Vec<Track> {
        ["env01", "env02", "free01", "nat01", "sx01"]
            .iter()
            .map(|id| Track {
                id: id.to_string(),
                name: id.to_uppercase(),
                image: String::new(),
            })
            .collect()
    }

    fn view(width: f32, height: f32) -> Viewport {
        Viewport {
            width,
            height,
            ..Default::default()
        }
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
            r#"{"quadId":26,"riderId":1,"playerName":"Sam"}"#,
        );
        let mut front = Frontend::new(tracks(), &settings, false);
        assert!(front.choices().iter().all(|(a, _)| a != "devlog"));
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
        front.key("Escape", &view, &mut settings);
        assert_eq!(front.page, "single");
        assert_eq!(front.choices()[front.selection].0, "garage");
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
