//! The UI layer over the ride: touch controls and the toolbar, drawn by the
//! engine's UI pass (crates/atv-ui).
use atv_core::math::Det;
use atv_ui::{
    DisplayList, Fonts, Rect, Renderer,
    hud::{
        Viewport,
        assets::OverlayAssets,
        help,
        meter::Meter,
        minimap::{Instrument, MapPose, Minimap, MinimapSettings},
        race::{OverlaySettings, RaceView},
        results::{self, Results},
        toolbar,
    },
    layout,
    menu::{
        self as pause,
        controller::{Context, Event as MenuEvent, PauseMenu},
        prefs::Preferences,
    },
    settings::Settings,
    taffy,
    touch::{Controls, Feedback, TouchControls},
};

/// Toolbar actions the ride applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pause,
    Camera,
    /// Race over → Restart (reload the event).
    RaceRetry,
    /// Race over → Quit Event (Single Player).
    RaceQuit,
    /// Race over → Continue Championship (the next round).
    RaceNextRound,
}

/// Hit target of the minimap (tap toggles north-up / heading-up).
const MINIMAP: u32 = 10;

/// What the HUD shows this frame.
pub struct HudInput<'a> {
    pub hud: atv_core::hud::Frame,
    pub rider_world: [f32; 16],
    pub terrain: &'a atv_game::venue::Terrain,
    /// A race's latest frame, the committed stunt points and the drawn
    /// player origin (the race overlays and 321 lamps).
    pub race: Option<(&'a atv_core::race::Frame, f32, [f32; 3])>,
    /// Other riders in a multiplayer room, the player's origin and the
    /// scene's view-projection (name tags and map markers).
    pub peers: Option<(&'a [atv_ui::hud::peers::Peer], [f32; 3], [f32; 16])>,
    /// A Freestyle objective's panel (time left, progress, score).
    pub objective: Option<atv_ui::hud::objective::View>,
}

/// A press on a toolbar button or the minimap (utility pointers in touch.js).
struct Press {
    pointer: u64,
    button: u32,
    x: f32,
    y: f32,
    moved: bool,
}

pub const MOUSE: u64 = u64::MAX;

/// A laid-out menu: view, state, paint, hits and scroll range.
type MenuCache = (Viewport, pause::State, DisplayList, Vec<(Rect, u32)>, f32);

/// Laid-out race results: inputs (view, state, summary), paint and hits.
type ResultsCache = (Viewport, Results, String, DisplayList, Vec<(Rect, u32)>);

/// A laid-out toolbar: its inputs, paint and hit targets.
type ToolbarCache = (Viewport, toolbar::State, DisplayList, Vec<(Rect, u32)>);

pub struct Overlay {
    /// The multiplayer room for the pause menu's online rows.
    pub lobby: Option<atv_ui::menu::controller::Lobby>,
    /// Name tags drawn on the latest frame (tests).
    pub tags: usize,
    pub fonts: Fonts,
    renderer: Renderer,
    pub settings: Settings,
    pub touch: TouchControls,
    pub view: Viewport,
    list: DisplayList,
    uploaded: DisplayList,
    upload_size: (u32, u32, f32),
    hits: Vec<(Rect, u32)>,
    hovered: Option<u32>,
    press: Option<Press>,
    pub minimap: Minimap,
    meter: Option<Meter>,
    /// Pause → Race Overlays choices (reloaded when the menu closes).
    race_overlays: OverlaySettings,
    /// The race load's fade from black (race.js `.race-fade`, 00210980):
    /// when the pre-race camera started.
    race_fade: Option<f64>,
    /// The race-over results and menu (race.js `RaceOverlay`).
    pub results: Results,
    /// Laid-out results: inputs and paint/hits, reused while unchanged.
    results_cache: Option<ResultsCache>,
    /// Keyboard controls card: open, and its first-ride close time.
    help_open: bool,
    help_until: Option<f64>,
    ride_started: bool,
    /// Laid-out toolbar and keyboard card, reused while their inputs are
    /// unchanged (the browser HUD's unchanged-state redraw avoidance).
    toolbar_cache: Option<ToolbarCache>,
    help_cache: Option<(Viewport, DisplayList)>,
    /// Keyboard card rows: action id -> key labels (chords joined with +).
    help_keys: Vec<(String, Vec<String>)>,
    /// Name of the controller in use (Controller Settings row).
    pub pad_name: Option<String>,
    /// Connected controllers and whether the one in use is standard-mapped.
    pub pads: Vec<(String, u32)>,
    pub pad_standard: bool,
    /// One-handed flips armed with a buzz (diagnostics).
    pub flip_buzzes: u32,
    pub prefs: Preferences,
    /// Controller glyph rasters for the bindings pages.
    glyphs: atv_ui::menu::glyphs::GlyphImages,
    /// The ride's soundtrack player view (set by the host each frame while
    /// a menu is open) and the original Jukebox art.
    pub music: Option<atv_ui::menu::jukebox::MusicView>,
    pub jukebox_art: Option<atv_ui::menu::jukebox::Art>,
    /// The Jukebox was opened: the host loads its art once.
    pub jukebox_art_wanted: bool,
    /// This frame's Jukebox spectrum (analyser bytes; None while silent).
    pub spectrum: Option<Vec<u8>>,
    /// The last menu layout: its inputs, paint, hits and scroll range.
    menu_cache: Option<MenuCache>,
    /// The Jukebox has centred its playing row since it opened.
    jukebox_centred: bool,
    created: web_time::Instant,
    /// The pause menu while the ride is paused.
    pub menu: Option<PauseMenu>,
    menu_hits: Vec<(Rect, u32)>,
    menu_scroll_max: f32,
    menu_reveal: bool,
    /// CPU time of the last `prepare`, milliseconds.
    pub prepare_ms: f64,
    /// The diagnostics overlay's lines (empty while Off; menus hide it, as
    /// `body.menu-open #diagnostics-overlay` does).
    pub diagnostics: Vec<String>,
    /// Parity test hook: a flat colour painted under the UI.
    pub backdrop: Option<[u8; 3]>,
    /// The engine's frontend can take the player back (Quit to Main Menu).
    pub frontend: bool,
    /// The ride is a race (Pause → Restart Race).
    pub race: bool,
    /// Pause pages slide in as they open (frontend.js `animatePage`), and
    /// this frame's slide.
    transition: atv_ui::transition::PageTransition,
    menu_motion: Option<atv_ui::transition::Motion>,
}

impl Overlay {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, view: Viewport) -> Self {
        Self::with_settings(device, format, view, Settings::load())
    }

    /// An overlay with the given saved settings (in-memory for captures).
    pub fn with_settings(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view: Viewport,
        settings: Settings,
    ) -> Self {
        let mut touch = TouchControls::new(&settings, view.coarse);
        touch.set_viewport(view.width, view.height);
        let minimap = Minimap::new(MinimapSettings::load(&settings));
        let mut prefs = Preferences::load(&settings, view.touch_first);
        crate::sound::sync_audio_session(&mut prefs);
        let race_overlays = OverlaySettings::load(&settings);
        Self {
            fonts: Fonts::new(),
            renderer: Renderer::new(device, format),
            settings,
            touch,
            view,
            list: DisplayList::default(),
            uploaded: DisplayList::default(),
            upload_size: (0, 0, 0.),
            hits: Vec::new(),
            hovered: None,
            press: None,
            minimap,
            meter: None,
            race_overlays,
            race_fade: None,
            results: Results::default(),
            results_cache: None,
            help_open: false,
            tags: 0,
            lobby: None,
            help_until: None,
            ride_started: false,
            toolbar_cache: None,
            help_cache: None,
            help_keys: Vec::new(),
            pad_name: None,
            pads: Vec::new(),
            pad_standard: true,
            flip_buzzes: 0,
            prefs,
            glyphs: Default::default(),
            music: None,
            jukebox_art: None,
            jukebox_art_wanted: false,
            spectrum: None,
            menu_cache: None,
            jukebox_centred: false,
            created: web_time::Instant::now(),
            menu: None,
            menu_hits: Vec::new(),
            menu_scroll_max: 0.,
            menu_reveal: false,
            prepare_ms: 0.,
            diagnostics: Vec::new(),
            backdrop: None,
            frontend: false,
            race: false,
            transition: Default::default(),
            menu_motion: None,
        }
    }

    pub fn set_view(&mut self, view: Viewport) {
        if self.view != view {
            self.clear_input();
            // frontend.js: a resize cancels the page's slide.
            self.transition.cancel();
        }
        self.view = view;
        self.touch.coarse = view.coarse;
        self.touch.set_viewport(view.width, view.height);
    }

    fn hit(&self, x: f32, y: f32) -> Option<u32> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, id)| *id)
    }

    /// The graphics context came back: a new UI renderer on `device` with
    /// the HUD artwork registered again. The GPS map (a 2D canvas in
    /// game.js, untouched by context loss) is rebuilt from the course;
    /// glyphs and the Jukebox art return at their next use.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn rebuild_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        venue: &atv_game::venue::Venue,
    ) {
        self.renderer = Renderer::new(device, format);
        self.uploaded = DisplayList::default();
        self.upload_size = (0, 0, 0.);
        self.menu_cache = None;
        self.toolbar_cache = None;
        self.help_cache = None;
        self.results_cache = None;
        self.glyphs.clear();
        self.jukebox_art = None;
        self.jukebox_art_wanted = self.jukebox_open();
        if let Some(meter) = &mut self.meter {
            meter.images.clear();
            for (name, image) in &meter.assets.images {
                let id =
                    self.renderer
                        .add_image(device, queue, image.width, image.height, &image.rgba);
                meter.images.insert(name.clone(), id);
            }
        }
        self.minimap.image = None;
        self.set_course(venue);
        self.finish_course(&venue.terrain);
    }

    /// Starts the GPS map build for a course (minimap.js `setTerrain`).
    pub fn set_course(&mut self, venue: &atv_game::venue::Venue) {
        self.minimap.set_terrain(&venue.terrain, &venue.scenery);
    }

    /// Uploads the original speed meter and stunt text resources.
    pub fn set_meter(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: OverlayAssets) {
        let mut meter = Meter::new(assets);
        for (name, image) in &meter.assets.images {
            let id = self
                .renderer
                .add_image(device, queue, image.width, image.height, &image.rgba);
            meter.images.insert(name.clone(), id);
        }
        self.meter = Some(meter);
    }

    /// Opens the pause menu at Pause when the ride pauses; closes it on resume.
    pub fn sync_menu(&mut self, paused: bool) {
        match (paused, self.menu.is_some()) {
            (true, false) => {
                let mut menu = PauseMenu::new();
                menu.frontend = self.frontend;
                menu.race_active = self.race;
                self.menu = Some(menu);
                self.press = None;
                self.touch.clear();
                // keyboard-help.js setRiding(false): menus hide the card.
                self.help_open = false;
                self.help_until = None;
            }
            (false, true) => {
                self.menu = None;
                self.menu_hits.clear();
                self.race_overlays = OverlaySettings::load(&self.settings);
            }
            _ => {}
        }
    }

    /// Runs a menu input with the settings it may change.
    fn with_menu<T>(
        &mut self,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
        f: impl FnOnce(&mut PauseMenu, &mut Context, &[(Rect, u32)], f32) -> T,
    ) -> Option<T> {
        let menu = self.menu.as_mut()?;
        let mut ctx = Context {
            settings: &mut self.settings,
            prefs: &mut self.prefs,
            minimap: &mut self.minimap.settings,
            touch: &mut self.touch,
            bindings,
            pad_name: self.pad_name.clone(),
            pads: self.pads.clone(),
            pad_standard: self.pad_standard,
            view: self.view,
            lobby: self.lobby.as_ref(),
            music: self.music.as_ref(),
        };
        let out = f(menu, &mut ctx, &self.menu_hits, self.menu_scroll_max);
        crate::sound::sync_audio_session(&mut self.prefs);
        Some(out)
    }

    pub fn menu_key(
        &mut self,
        code: &str,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) -> Option<MenuEvent> {
        self.menu_reveal = true;
        self.with_menu(bindings, |m, ctx, _, _| m.key(code, ctx))
            .flatten()
    }

    pub fn menu_key_up(
        &mut self,
        code: &str,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) -> Option<MenuEvent> {
        self.with_menu(bindings, |m, ctx, _, _| m.key_up(code, ctx))
            .flatten()
    }

    pub fn menu_pad(
        &mut self,
        pad: Option<&atv_game::ride::bindings::Pad>,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) -> Option<MenuEvent> {
        self.menu_reveal = true;
        self.with_menu(bindings, |m, ctx, _, _| m.pad(pad, ctx))
            .flatten()
    }

    pub fn menu_press(
        &mut self,
        pointer: u64,
        x: f32,
        y: f32,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) {
        self.with_menu(bindings, |m, ctx, hits, _| {
            m.press(hits, pointer, x, y, ctx)
        });
    }

    pub fn menu_drag(
        &mut self,
        pointer: u64,
        x: f32,
        y: f32,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) {
        self.with_menu(bindings, |m, ctx, hits, max| {
            m.hover(hits, x, y);
            m.drag(pointer, x, y, max, ctx)
        });
    }

    pub fn menu_release(
        &mut self,
        pointer: u64,
        x: f32,
        y: f32,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) -> Option<MenuEvent> {
        self.with_menu(bindings, |m, ctx, hits, _| {
            m.release(hits, pointer, x, y, ctx)
        })
        .flatten()
    }

    pub fn menu_wheel(&mut self, dy: f32) {
        self.with_menu(None, |m, _, _, max| m.wheel(dy, max));
    }

    /// The keyboard card follows the live (possibly remapped) bindings.
    pub fn set_help_keys(&mut self, bindings: &atv_game::ride::bindings::Bindings) {
        let keys = help::action_keys(bindings);
        if keys != self.help_keys {
            self.help_keys = keys;
            self.help_cache = None;
        }
    }

    /// H toggles the keyboard card while riding (keyboard-help.js).
    pub fn key(&mut self, code: &str, riding: bool) {
        if code == "KeyH" && riding && !self.view.touch_first {
            self.toggle_help();
        }
    }

    fn toggle_help(&mut self) {
        self.help_until = None;
        self.help_open = !self.help_open;
        if self.help_open {
            self.settings.set(help::SEEN_KEY, "1");
        }
    }

    /// A pointer pressed at CSS `x`, `y`; true when the UI takes it.
    pub fn press(&mut self, pointer: u64, x: f32, y: f32) -> bool {
        match self.hit(x, y) {
            Some(button) => {
                // Another utility finger cannot steal an existing capture.
                if self.press.is_some() {
                    return true;
                }
                self.press = Some(Press {
                    pointer,
                    button,
                    x,
                    y,
                    moved: false,
                });
                true
            }
            None => false,
        }
    }

    pub fn hover(&mut self, x: f32, y: f32) {
        self.hovered = self.hit(x, y);
        if let Some(p) = &mut self.press
            && p.pointer == MOUSE
            && (x - p.x).hypot_d(y - p.y) > 16.
        {
            p.moved = true;
        }
    }

    /// Moves a pressed utility pointer; true while the UI owns it.
    pub fn drag(&mut self, pointer: u64, x: f32, y: f32) -> bool {
        match &mut self.press {
            Some(p) if p.pointer == pointer => {
                if (x - p.x).hypot_d(y - p.y) > 16. {
                    p.moved = true;
                }
                true
            }
            _ => false,
        }
    }

    /// Releases a pointer: `Some(Some(action))` activates a button, `Some(None)`
    /// was a UI press without activation, None is not the UI's.
    pub fn release(
        &mut self,
        pointer: u64,
        x: f32,
        y: f32,
        cancelled: bool,
    ) -> Option<Option<Action>> {
        let p = self.press.take_if(|p| p.pointer == pointer)?;
        let nearby = (x - p.x).hypot_d(y - p.y) <= 16.;
        let hit = self.hit(x, y);
        Some(
            if cancelled || p.moved || !nearby || hit != Some(p.button) {
                None
            } else if p.button == MINIMAP {
                self.minimap.settings.heading_up = !self.minimap.settings.heading_up;
                self.minimap.settings.save(&mut self.settings);
                None
            } else {
                if (results::CONTINUE..=results::SHOW).contains(&p.button) {
                    self.results.focus_visible = false;
                    return Some(self.results.act(p.button).map(|command| match command {
                        results::Command::Retry => Action::RaceRetry,
                        results::Command::Quit => Action::RaceQuit,
                        results::Command::NextRound => Action::RaceNextRound,
                    }));
                }
                match toolbar::Button::from_id(p.button) {
                    Some(toolbar::Button::Pause) => Some(Action::Pause),
                    Some(toolbar::Button::Camera) => Some(Action::Camera),
                    Some(toolbar::Button::Keys) => {
                        self.toggle_help();
                        None
                    }
                    None => None,
                }
            },
        )
    }

    pub fn clear_input(&mut self) {
        self.touch.clear();
        self.press = None;
        self.hovered = None;
    }

    /// The page lost focus: input clears and a binding capture is cancelled
    /// (frontend.js `blur` → `cancelCapture`).
    pub fn blur(&mut self) {
        self.clear_input();
        if let Some(menu) = &mut self.menu {
            menu.cancel_capture();
        }
    }

    pub fn touch_down(&mut self, id: u64, x: f32, y: f32, now: f64, active: bool) -> bool {
        self.touch.down(id, x, y, now, active, &mut self.settings)
    }

    pub fn feedback(&mut self, frame: Feedback, now: f64) {
        self.touch.feedback(frame, now);
    }

    pub fn sample(&mut self, now: f64, active: bool) -> Controls {
        // touch.js mountTouchControls: Extras flips and Alpha Arcade flips
        // decide the thumb's flip zones and its 360 axis each frame.
        let arcade = self.prefs.alpha("arcade-flips");
        self.touch.input.flips = self.prefs.flips && !arcade;
        self.touch.input.arcade = arcade;
        let controls = self.touch.sample(now, active, &mut self.settings);
        if active && self.touch.flip_buzz(now) {
            self.flip_buzzes += 1;
            buzz(12);
        }
        controls
    }

    /// Builds this frame's UI and uploads it.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        now: f64,
        active: bool,
        camera_enabled: bool,
        paused: bool,
        hud: Option<HudInput>,
        bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) {
        let started = web_time::Instant::now();
        self.list.clear();
        self.hits.clear();
        if let Some([r, g, b]) = self.backdrop {
            self.list.rect(
                Rect::new(0., 0., self.view.width, self.view.height),
                atv_ui::Color::rgb(r, g, b),
            );
        }
        let view = self.view;
        // Each pause page shown slides in; closing the menu shows nothing.
        self.transition
            .observe(self.menu.as_ref().map(|m| m.shown), crate::reduced_motion());
        self.menu_motion = self.transition.motion(now);
        if self.menu.is_some() {
            self.prepare_menu(device, queue, bindings);
            // drawBars: the spectrum repaints every frame over the cached panel.
            if let Some((area, _)) = self
                .menu_hits
                .iter()
                .find(|(_, id)| *id == atv_ui::menu::BARS)
            {
                if let Some(m) = self.menu_motion {
                    self.list.opacity(m.opacity);
                }
                atv_ui::menu::jukebox::paint_bars(
                    &mut self.list,
                    *area,
                    self.spectrum.as_deref().unwrap_or(&[]),
                );
                if self.menu_motion.is_some() {
                    self.list.unopacity();
                }
            }
            self.upload(device, queue, width, height);
            self.prepare_ms = started.elapsed().as_secs_f64() * 1000.;
            return;
        }
        let touch_driving = self.touch.driving(active) || view.touch_first;
        // The GPS map builds in ~2 ms slices like buildSlice, then uploads once.
        if let Some(hud) = &hud
            && self.minimap.building()
        {
            self.minimap.build_slice(hud.terrain, 2.);
        }
        if let Some(pixels) = self.minimap.pending.take() {
            let size = atv_ui::hud::minimap::MAP_RESOLUTION as u32;
            self.minimap.image = Some(self.renderer.add_image(device, queue, size, size, &pixels));
        }
        // Visible while riding (game.js hides HUD and map behind menus/blur);
        // a race hides the HUD through its pre-race shots.
        let race = hud.as_ref().and_then(|h| h.race);
        let intro = race.is_some_and(|(frame, ..)| frame.phase == "intro");
        let shown = hud.as_ref().filter(|_| active && !intro);
        let instrument = shown.map(|h| Instrument {
            speed: h.hud.speed_text.clone(),
            units: h.hud.units.into(),
            preload_visible: h.hud.preload_visible,
            preload_fill: h.hud.preload_fill,
            preload_blue_alpha: h.hud.preload_blue_alpha as f32,
        });
        self.minimap.peers = hud
            .as_ref()
            .and_then(|h| h.peers)
            .map(|(peers, ..)| {
                peers
                    .iter()
                    .filter(|p| !p.stale)
                    .map(|p| (p.slot, p.position))
                    .collect()
            })
            .unwrap_or_default();
        // Races hide the GPS map (minimap.draw's !raceSettings).
        self.minimap.update(
            now,
            shown
                .filter(|_| race.is_none())
                .map(|h| MapPose::from_matrix(&h.rider_world)),
            instrument,
        );
        // First keyboard ride: open the card for 15 s (keyboard-help.js).
        if active && !self.ride_started {
            self.ride_started = true;
            if !touch_driving && self.settings.get(help::SEEN_KEY).as_deref() != Some("1") {
                self.help_open = true;
                self.help_until = Some(now + help::FIRST_RIDE_SECONDS * 1000.);
                self.settings.set(help::SEEN_KEY, "1");
            }
        }
        if self.help_until.is_some_and(|until| now >= until) {
            self.help_open = false;
            self.help_until = None;
        }
        self.touch
            .paint(active, now, &mut self.fonts, &mut self.list);
        if let (Some(meter), Some(h)) = (&self.meter, shown) {
            let speed = match race {
                Some(_) => self.race_overlays.shown("speed"),
                None => !self.minimap.settings.enabled,
            };
            meter.paint_speed(
                &mut self.fonts,
                &mut self.list,
                &h.hud,
                speed,
                view.width,
                view.height,
            );
        }
        // The race overlays sit below the toolbar: drawn once it is laid out.
        let hud_at = self.list.items.len();
        let map_visible = self.minimap.visible() && !paused && race.is_none();
        let keys_visible = !touch_driving;
        // A large top-right map can meet the labelled toolbar: icon-only buttons.
        let labels: f32 = ["PAUSE", "CAMERA", "KEYS"]
            .iter()
            .take(if keys_visible { 3 } else { 2 })
            .map(|label| {
                let style = toolbar::label_style(atv_ui::Color::WHITE);
                12. + 20.
                    + 8.
                    + self
                        .fonts
                        .measure(label, &style.font, style.letter_spacing)
                        .width
                    + 12.
            })
            .sum();
        let count = if keys_visible { 3. } else { 2. };
        let bar_right = view.safe[3].max(14.) + labels + 8. * (count - 1.);
        let bar_top = view.safe[0].max(14.);
        let map = self.minimap.rect(&view);
        let compact = map_visible
            && !self.minimap.settings.bottom_left
            && bar_right + 8. > map.x
            && bar_top < map.bottom();
        let state = toolbar::State {
            paused,
            hovered: self.hovered.and_then(toolbar::Button::from_id),
            pressed: self
                .press
                .as_ref()
                .and_then(|p| toolbar::Button::from_id(p.button)),
            camera_enabled,
            keys_enabled: active,
            keys_visible,
            compact,
        };
        let full = |child: layout::El| {
            layout::El::new(taffy::Style {
                size: taffy::Size {
                    width: taffy::Dimension::length(view.width),
                    height: taffy::Dimension::length(view.height),
                },
                ..Default::default()
            })
            .child(child)
        };
        if !self
            .toolbar_cache
            .as_ref()
            .is_some_and(|(v, s, ..)| *v == view && *s == state)
        {
            let (mut list, mut hits) = (DisplayList::default(), Vec::new());
            layout::render(
                &full(toolbar::element(&view, &state)),
                &mut self.fonts,
                view.width,
                view.height,
                &mut list,
                &mut hits,
            );
            self.toolbar_cache = Some((view, state, list, hits));
        }
        if let Some((_, _, list, hits)) = &self.toolbar_cache {
            self.list.items.extend_from_slice(&list.items);
            self.hits.extend_from_slice(hits);
        }
        if let (Some(meter), Some(h)) = (&self.meter, shown) {
            let mut items = DisplayList::default();
            if let Some((frame, score, origin)) = race {
                // toolbarBottom(): the toolbar's bottom edge.
                let top_inset = self.toolbar_cache.as_ref().map_or(0., |(.., hits)| {
                    hits.iter().map(|(r, _)| r.bottom()).fold(0., f32::max)
                });
                meter.paint_race(
                    &mut self.fonts,
                    &mut items,
                    &RaceView {
                        frame,
                        score,
                        origin,
                        top_inset,
                        settings: &self.race_overlays,
                    },
                    view.width,
                    view.height,
                );
                if let Some(lights) = frame.lights {
                    meter.paint_lamps(&mut items, lights, view.width, view.height);
                }
            }
            if race.is_none() || self.race_overlays.shown("info") {
                meter.paint_stunt(&mut self.fonts, &mut items, &h.hud, view.width, view.height);
            }
            self.list.items.splice(hud_at..hud_at, items.items);
        }
        // Race load fades in from black over 500 ms as the pre-race camera
        // starts, over everything (#race-overlay is the topmost layer).
        if intro && self.race_fade.is_none() && active {
            self.race_fade = Some(now);
        }
        let fade = self
            .race_fade
            .filter(|_| active)
            .map(|start| ((now - start) / 500.) as f32)
            .filter(|t| *t < 1.);
        // Name tags over other riders, under the map and menus.
        if let Some((peers, origin, vp)) = shown.and_then(|h| h.peers) {
            self.tags = atv_ui::hud::peers::paint_tags(
                &mut self.fonts,
                &mut self.list,
                &view,
                peers,
                origin,
                &vp,
            );
        } else {
            self.tags = 0;
        }
        if map_visible {
            self.minimap.paint(&view, &mut self.fonts, &mut self.list);
            self.hits.push((map, MINIMAP));
        }
        if self.help_open && active && !touch_driving {
            let rows = &self.help_keys;
            let key_names = |id: &str| -> Vec<String> {
                rows.iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, keys)| keys.clone())
                    .unwrap_or_default()
            };
            if !self.help_cache.as_ref().is_some_and(|(v, _)| *v == view) {
                let mut list = DisplayList::default();
                layout::render(
                    &full(help::element(&view, &key_names)),
                    &mut self.fonts,
                    view.width,
                    view.height,
                    &mut list,
                    &mut Vec::new(),
                );
                self.help_cache = Some((view, list));
            }
            if let Some((_, list)) = &self.help_cache {
                self.list.items.extend_from_slice(&list.items);
            }
        }
        // A Freestyle objective's panel, under the toolbar.
        if let Some(objective) = shown.and_then(|h| h.objective.as_ref())
            && !self.results.shown
        {
            let top = self.toolbar_cache.as_ref().map_or(0., |(.., hits)| {
                hits.iter().map(|(r, _)| r.bottom()).fold(0., f32::max)
            }) + 8.;
            let mut list = DisplayList::default();
            layout::render(
                &atv_ui::hud::objective::element(&view, objective, top),
                &mut self.fonts,
                view.width,
                view.height,
                &mut list,
                &mut Vec::new(),
            );
            self.list.items.extend_from_slice(&list.items);
        }
        // Race over: the results panel or the Show Results button.
        match race {
            Some((frame, ..)) if frame.phase == "finished" => {
                // race.js always offers Quit Event; a page without the
                // engine's frontend leaves for its menus.
                self.results.can_quit = self.frontend || cfg!(target_arch = "wasm32");
                if self.results.finish(frame) {
                    self.credit_profile();
                }
            }
            // A race restarted in place starts without results.
            _ if self.results.shown && self.results.objective.is_none() => {
                self.results = Results::default()
            }
            _ => {}
        }
        if self.results.shown && active {
            self.paint_results(race.map(|(frame, ..)| frame));
        }
        if let Some(t) = fade {
            self.list.rect(
                Rect::new(0., 0., view.width, view.height),
                atv_ui::Color::rgb(0, 0, 0).fade(1. - t.max(0.)),
            );
        }
        if !self.diagnostics.is_empty() {
            atv_ui::diagnostics::paint(
                &mut self.list,
                &mut self.fonts,
                view.width,
                view.height,
                &self.diagnostics,
            );
        }
        self.upload(device, queue, width, height);
        self.prepare_ms = started.elapsed().as_secs_f64() * 1000.;
    }

    /// Credits the finished race to the saved profile (`001f1e30` →
    /// `00323440`/`00322080`) once per race.
    fn credit_profile(&mut self) {
        use atv_ui::settings::{load_profile, save_profile};
        let Some((place, stunt, points)) = self.results.award else {
            return;
        };
        let mut profile = load_profile(&self.settings);
        profile.record_race(place, points, stunt);
        if let Some((round, places)) = self.results.championship.clone()
            && let Some(series) = atv_core::championship::series(round.series)
        {
            let outcome = profile.championships.record(series, round.expert, &places);
            if let Some((_, reward, _)) = &outcome.finished {
                profile.points = profile.points.saturating_add(*reward);
            }
            self.results.championship_line =
                Some(results::Results::describe_round(series, &outcome));
            self.results.round = Some(if outcome.finished.is_some() {
                results::Round::Over
            } else if outcome.advanced {
                results::Round::Next
            } else {
                results::Round::Replay
            });
        }
        save_profile(&mut self.settings, &profile);
        self.results.profile_total = Some(profile.points);
    }

    /// Lays out the results (`translate(-50%, -50%)`: measured, then
    /// centred) and adds their paint and buttons.
    fn paint_results(&mut self, frame: Option<&atv_core::race::Frame>) {
        let view = self.view;
        let summary = self
            .results
            .objective
            .clone()
            .or_else(|| {
                frame.map(|f| {
                    let line = results::summary(f, self.results.error.as_deref());
                    if self.results.error.is_some() {
                        line
                    } else {
                        line + &self.results.points_line()
                    }
                })
            })
            .unwrap_or_default();
        let fresh = self
            .results_cache
            .as_ref()
            .is_some_and(|(v, r, s, ..)| *v == view && *r == self.results && *s == summary);
        if !fresh {
            let (mut list, mut hits) = (DisplayList::default(), Vec::new());
            let element = results::element(&view, &self.results, &summary, &mut self.fonts);
            layout::render(
                &element,
                &mut self.fonts,
                view.width,
                view.height,
                &mut list,
                &mut hits,
            );
            if !self.results.hidden
                && let Some(atv_ui::paint::Item::Box(panel)) = list.items.first()
            {
                let dy = -panel.rect.h / 2.;
                for item in &mut list.items {
                    offset_item(item, dy);
                }
                for (rect, _) in &mut hits {
                    rect.y += dy;
                }
            }
            self.results_cache = Some((view, self.results.clone(), summary, list, hits));
        }
        if let Some((.., list, hits)) = &self.results_cache {
            self.list.items.extend_from_slice(&list.items);
            self.hits.extend_from_slice(hits);
        }
    }

    /// Registers the original Jukebox art (PAGE14 backdrop and CD frames).
    pub fn set_jukebox_art(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        art: atv_game::jukebox::JukeboxArt,
    ) {
        let backdrop = art.backdrop.map(|(w, h, rgba)| {
            let id = self.renderer.add_image(device, queue, w, h, &rgba);
            (id, [w as f32, h as f32])
        });
        let disc_size = art
            .disc
            .first()
            .map_or([0.; 2], |(w, h, _)| [*w as f32, *h as f32]);
        let disc = art
            .disc
            .iter()
            .map(|(w, h, rgba)| self.renderer.add_image(device, queue, *w, *h, rgba))
            .collect();
        self.jukebox_art = Some(atv_ui::menu::jukebox::Art {
            backdrop,
            disc,
            disc_size,
        });
    }

    /// A pause page is sliding in (frames keep coming until it rests).
    pub fn sliding(&self) -> bool {
        self.transition.active()
    }

    /// The Jukebox page is open (it animates every frame).
    pub fn jukebox_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|m| m.page == "jukebox")
    }

    /// Escape/Enter while the race results are up: true when taken.
    pub fn results_key(&mut self, code: &str) -> (bool, Option<Action>) {
        if self.menu.is_some() {
            return (false, None);
        }
        let (taken, command) = self.results.key(code);
        (taken, command.map(command_action))
    }

    /// Controller A/B/X and the D-pad while the race results are up.
    pub fn results_pad(&mut self, pad: Option<&atv_game::ride::bindings::Pad>) -> Option<Action> {
        if self.menu.is_some() {
            return None;
        }
        let idle = atv_game::ride::bindings::Pad::default();
        let pad = pad.unwrap_or(&idle);
        self.results
            .pad(&pad.buttons, &pad.axes)
            .map(command_action)
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) {
        let upload_size = (width, height, self.view.scale);
        if self.list != self.uploaded || self.upload_size != upload_size {
            self.renderer.prepare(
                device,
                queue,
                &mut self.fonts,
                &self.list,
                width,
                height,
                self.view.scale,
            );
            self.uploaded.clone_from(&self.list);
            self.upload_size = upload_size;
        } else {
            self.renderer.rasterized = 0;
        }
    }

    /// The pause layer: the dim backdrop and the panel (the HUD, map, toolbar
    /// and touch controls hide behind menus, as `body.menu-open` does).
    fn prepare_menu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mut bindings: Option<&mut atv_game::ride::bindings::Bindings>,
    ) {
        let view = self.view;
        let start = self.list.items.len();
        let Some(mut state) = self.with_menu(bindings.as_deref_mut(), |m, ctx, _, _| {
            m.refresh(ctx);
            m.state(ctx)
        }) else {
            return;
        };
        state.motion = self.menu_motion;
        if state.page == "jukebox" {
            // The original art loads on the first visit (jukebox.js loadArt).
            self.jukebox_art_wanted = true;
            state.jukebox_art = self.jukebox_art.clone();
            // jb-turn: 20 steps every 1.6 s while a song plays.
            if state.music.as_ref().is_some_and(|m| m.playing && m.enabled) {
                state.frame = (web_time::Instant::now()
                    .duration_since(self.created)
                    .as_millis()
                    / 80) as usize;
            }
        }
        self.glyphs.prepare(
            &mut self.renderer,
            device,
            queue,
            state.pad.as_ref(),
            view.scale,
        );
        // An unchanged menu reuses its layout (the Jukebox redraws often).
        if let Some((v, s, list, hits, max)) = &self.menu_cache
            && *v == view
            && *s == state
        {
            self.list.items.extend_from_slice(&list.items);
            self.menu_hits.clone_from(hits);
            self.menu_scroll_max = *max;
            return;
        }
        let mut hits = Vec::new();
        layout::render(
            &pause::element(&view, &state, &self.glyphs),
            &mut self.fonts,
            view.width,
            view.height,
            &mut self.list,
            &mut hits,
        );
        let rect = |id| hits.iter().find(|(_, i)| *i == id).map(|(r, _)| *r);
        if let (Some(panel), Some(content)) = (rect(pause::PANEL), rect(pause::CONTENT)) {
            // The Jukebox scrolls only its track list (no panel padding).
            let padding = if state.page == "jukebox" {
                0.
            } else if view.width <= 580. || view.height <= 450. {
                20.
            } else {
                30.
            };
            self.menu_scroll_max = (content.h - (panel.h - 2. * padding)).max(0.);
        }
        // jukebox.js: open with the playing song centred in the list.
        let centre = state.page == "jukebox" && !self.jukebox_centred;
        self.jukebox_centred = state.page == "jukebox";
        if centre
            && let (Some(panel), Some(index)) = (
                rect(pause::PANEL),
                state
                    .music
                    .as_ref()
                    .and_then(|m| m.playing_index)
                    .and_then(|i| {
                        state
                            .rows
                            .choices
                            .iter()
                            .position(|c| *c == format!("song:{i}"))
                    }),
            )
            && let Some(row) = rect(pause::ROW + index as u32)
            && let Some(menu) = &mut self.menu
        {
            let before = menu.scroll;
            menu.scroll = (menu.scroll + row.y - panel.y - (panel.h - row.h) / 2.)
                .clamp(0., self.menu_scroll_max);
            if menu.scroll != before {
                self.list.items.truncate(start);
                self.menu_cache = None;
                self.prepare_menu(device, queue, bindings);
                return;
            }
        }
        self.menu_cache = Some((
            view,
            state,
            DisplayList {
                items: self.list.items[start..].to_vec(),
            },
            hits.clone(),
            self.menu_scroll_max,
        ));
        self.menu_hits = hits;
        if std::mem::take(&mut self.menu_reveal) {
            let hits = std::mem::take(&mut self.menu_hits);
            if let Some(menu) = &mut self.menu {
                let before = menu.scroll;
                menu.reveal(&hits);
                menu.scroll = menu.scroll.min(self.menu_scroll_max);
                if menu.scroll != before {
                    self.menu_hits = hits;
                    self.list.items.truncate(start);
                    self.prepare_menu(device, queue, bindings);
                    return;
                }
            }
            self.menu_hits = hits;
        }
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "hits": self.hits.iter().map(|(r, id)| serde_json::json!({
                "id": id, "x": r.x, "y": r.y, "width": r.w, "height": r.h,
            })).collect::<Vec<_>>(),
            "touchOwned": self.touch.input.pointer.is_some(),
            "flipBuzzes": self.flip_buzzes,
            "secondaryOwned": self.touch.input.secondary.as_ref().is_some_and(|s| s.pointer.is_some()),
            "scale": self.view.scale,
            "rasterized": self.renderer.rasterized,
            "prepareMs": self.prepare_ms,
            "menu": self.menu.as_ref().map(|m| serde_json::json!({
                "page": m.page,
                // The panel's slide: [opacity, translateX px].
                "motion": self.menu_motion.map(|m| [m.opacity, m.dx]),
                "selection": m.selection,
                "choices": m.rows.choices,
                "scroll": m.scroll,
                "notice": m.notice,
                "labels": m.rows.labels.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
                "musicPage": m.music_page(),
                "music": self.music.as_ref().map(|v| serde_json::json!({
                    "playing": v.playing_index, "enabled": v.enabled, "paused": v.paused,
                    "position": v.position, "duration": v.duration, "volume": v.volume,
                    "songs": v.songs.len(), "art": self.jukebox_art.is_some(),
                    "spectrum": self.spectrum.as_ref().map(|b| b.iter().copied().max().unwrap_or(0)),
                    "bars": self.menu_hits.iter().any(|(_, id)| *id == atv_ui::menu::BARS),
                })),
                "hits": self.menu_hits.iter().filter(|(_, id)| *id >= pause::ROW).map(|(r, id)| serde_json::json!({
                    "action": m.rows.choices.get((*id - pause::ROW) as usize),
                    "x": r.x, "y": r.y, "width": r.w, "height": r.h,
                })).collect::<Vec<_>>(),
            })),
            "hud": {
                "helpOpen": self.help_open,
                "meter": self.meter.is_some() && !self.minimap.settings.enabled,
                "compact": self.toolbar_cache.as_ref().is_some_and(|(_, s, ..)| s.compact),
                "minimap": {
                    "visible": self.minimap.visible(),
                    "headingUp": self.minimap.settings.heading_up,
                    "buildMs": self.minimap.build_ms,
                    "updates": self.minimap.updates,
                },
            },
        })
    }

    /// Finishes any pending GPS map build at once (parity captures, a
    /// restored graphics context).
    pub fn finish_course(&mut self, terrain: &atv_game::venue::Terrain) {
        while self.minimap.building() {
            self.minimap.build_slice(terrain, f64::INFINITY);
        }
    }

    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        self.renderer.draw(encoder, target);
    }
}

/// The page's viewport traits (media queries and safe-area insets).
pub fn viewport(width: u32, height: u32, scale: f64) -> Viewport {
    let scale = scale.max(0.1) as f32;
    #[allow(unused_mut)]
    let mut view = Viewport {
        width: width as f32 / scale,
        height: height as f32 / scale,
        scale,
        hover: true,
        ..Default::default()
    };
    // Android: a touch-first phone or tablet, as its browser reports
    // `(hover: none) and (pointer: coarse)`.
    if cfg!(target_os = "android") {
        view.hover = false;
        view.coarse = true;
        view.touch_first = true;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window();
        // winit's device-pixel observer may report the renderer's actual backing
        // resolution independently of the CSS viewport (including browser DPR
        // emulation and future adaptive resolution). Layout/hit targets use CSS
        // pixels; rasterization uses backing pixels per CSS pixel.
        if let Some(w) = &window {
            if let Some(width) = w
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .filter(|v| *v > 0.)
            {
                view.width = width as f32;
            }
            if let Some(height) = w
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .filter(|v| *v > 0.)
            {
                view.height = height as f32;
            }
            view.scale = width as f32 / view.width;
        }
        let matches = |query: &str| {
            window
                .as_ref()
                .and_then(|w| w.match_media(query).ok().flatten())
                .is_some_and(|m| m.matches())
        };
        view.hover = matches("(hover: hover)");
        view.coarse = matches("(any-pointer: coarse)");
        view.touch_first = matches("(hover: none) and (pointer: coarse)");
        view.safe = safe_area();
    }
    view
}

/// `env(safe-area-inset-*)` read from a probe element.
#[cfg(target_arch = "wasm32")]
fn safe_area() -> [f32; 4] {
    use wasm_bindgen::JsCast;
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return [0.; 4];
    };
    let probe = document.get_element_by_id("safe-area-probe").or_else(|| {
        let element = document.create_element("div").ok()?;
        element.set_id("safe-area-probe");
        document.body()?.append_child(&element).ok()?;
        Some(element)
    });
    let Some(style) = probe
        .and_then(|p| p.dyn_into::<web_sys::HtmlElement>().ok())
        .and_then(|p| web_sys::window()?.get_computed_style(&p).ok().flatten())
    else {
        return [0.; 4];
    };
    let px = |name: &str| {
        style
            .get_property_value(name)
            .ok()
            .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
            .unwrap_or(0.)
    };
    [
        px("padding-top"),
        px("padding-right"),
        px("padding-bottom"),
        px("padding-left"),
    ]
}

fn command_action(command: results::Command) -> Action {
    match command {
        results::Command::Retry => Action::RaceRetry,
        results::Command::Quit => Action::RaceQuit,
        results::Command::NextRound => Action::RaceNextRound,
    }
}

/// Moves a laid-out item down by `dy` CSS pixels.
fn offset_item(item: &mut atv_ui::paint::Item, dy: f32) {
    use atv_ui::paint::Item;
    match item {
        Item::Box(b) => {
            b.rect.y += dy;
            b.pivot[1] += dy;
        }
        Item::Text { y, .. } | Item::Shape { y, .. } => *y += dy,
        Item::Image { dst, pivot, .. } => {
            dst.y += dy;
            pivot[1] += dy;
        }
        Item::ImageShadow { dst, .. } => dst.y += dy,
        Item::PushClip { rect, .. } => rect.y += dy,
        Item::PopClip | Item::PushOpacity(_) | Item::PopOpacity => {}
    }
}

/// `navigator.vibrate?.(ms)` where it exists (iOS Safari has none, and
/// calling a missing binding would throw through the frame).
fn buzz(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::Reflect;
        use wasm_bindgen::{JsCast, JsValue};
        let Some(window) = web_sys::window() else {
            return;
        };
        let navigator: JsValue = window.navigator().into();
        if let Some(vibrate) = Reflect::get(&navigator, &"vibrate".into())
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        {
            let _ = vibrate.call1(&navigator, &f64::from(ms).into());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = ms;
}
