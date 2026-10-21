//! The UI layer over the ride: touch controls and the toolbar, drawn by the
//! engine's UI pass (crates/atv-ui).
use atv_ui::{
    DisplayList, Fonts, Rect, Renderer,
    hud::{
        Viewport,
        assets::OverlayAssets,
        help,
        meter::Meter,
        minimap::{Instrument, MapPose, Minimap, MinimapSettings},
        toolbar,
    },
    layout,
    settings::Settings,
    taffy,
    touch::{Controls, Feedback, TouchControls},
};

/// Toolbar actions the ride applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pause,
    Camera,
}

/// Hit target of the minimap (tap toggles north-up / heading-up).
const MINIMAP: u32 = 10;

/// What the HUD shows this frame.
pub struct HudInput<'a> {
    pub hud: atv_core::hud::Frame,
    pub rider_world: [f32; 16],
    pub terrain: &'a atv_game::venue::Terrain,
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

/// A laid-out toolbar: its inputs, paint and hit targets.
type ToolbarCache = (Viewport, toolbar::State, DisplayList, Vec<(Rect, u32)>);

pub struct Overlay {
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
    /// Keyboard controls card: open, and its first-ride close time.
    help_open: bool,
    help_until: Option<f64>,
    ride_started: bool,
    /// Laid-out toolbar and keyboard card, reused while their inputs are
    /// unchanged (the browser HUD's unchanged-state redraw avoidance).
    toolbar_cache: Option<ToolbarCache>,
    help_cache: Option<(Viewport, DisplayList)>,
    /// CPU time of the last `prepare`, milliseconds.
    pub prepare_ms: f64,
    /// Parity test hook: a flat colour painted under the UI.
    pub backdrop: Option<[u8; 3]>,
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
            help_open: false,
            help_until: None,
            ride_started: false,
            toolbar_cache: None,
            help_cache: None,
            prepare_ms: 0.,
            backdrop: None,
        }
    }

    pub fn set_view(&mut self, view: Viewport) {
        if self.view != view {
            self.clear_input();
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
            && (x - p.x).hypot(y - p.y) > 16.
        {
            p.moved = true;
        }
    }

    /// Moves a pressed utility pointer; true while the UI owns it.
    pub fn drag(&mut self, pointer: u64, x: f32, y: f32) -> bool {
        match &mut self.press {
            Some(p) if p.pointer == pointer => {
                if (x - p.x).hypot(y - p.y) > 16. {
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
        let nearby = (x - p.x).hypot(y - p.y) <= 16.;
        let hit = self.hit(x, y);
        Some(
            if cancelled || p.moved || !nearby || hit != Some(p.button) {
                None
            } else if p.button == MINIMAP {
                self.minimap.settings.heading_up = !self.minimap.settings.heading_up;
                self.minimap.settings.save(&mut self.settings);
                None
            } else {
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

    pub fn touch_down(&mut self, id: u64, x: f32, y: f32, now: f64, active: bool) -> bool {
        self.touch.down(id, x, y, now, active, &mut self.settings)
    }

    pub fn feedback(&mut self, frame: Feedback, now: f64) {
        self.touch.feedback(frame, now);
    }

    pub fn sample(&mut self, now: f64, active: bool) -> Controls {
        self.touch.sample(now, active, &mut self.settings)
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
        // Visible while riding (game.js hides HUD and map behind menus/blur).
        let shown = hud.as_ref().filter(|_| active);
        let instrument = shown.map(|h| Instrument {
            speed: h.hud.speed_text.clone(),
            units: h.hud.units.into(),
            preload_visible: h.hud.preload_visible,
            preload_fill: h.hud.preload_fill,
            preload_blue_alpha: h.hud.preload_blue_alpha as f32,
        });
        self.minimap.update(
            now,
            shown.map(|h| MapPose::from_matrix(&h.rider_world)),
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
            meter.paint(
                &mut self.fonts,
                &mut self.list,
                &h.hud,
                !self.minimap.settings.enabled,
                view.width,
                view.height,
            );
        }
        let map_visible = self.minimap.visible() && !paused;
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
        if map_visible {
            self.minimap.paint(&view, &mut self.fonts, &mut self.list);
            self.hits.push((map, MINIMAP));
        }
        if self.help_open && active && !touch_driving {
            let key_names = |id: &str| -> Vec<String> {
                atv_game::ride::bindings::action_named(id)
                    .map(|action| {
                        atv_game::ride::bindings::keyboard_codes(action)
                            .into_iter()
                            .map(help::key_name)
                            .collect()
                    })
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
        self.prepare_ms = started.elapsed().as_secs_f64() * 1000.;
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "hits": self.hits.iter().map(|(r, id)| serde_json::json!({
                "id": id, "x": r.x, "y": r.y, "width": r.w, "height": r.h,
            })).collect::<Vec<_>>(),
            "touchOwned": self.touch.input.pointer.is_some(),
            "secondaryOwned": self.touch.input.secondary.as_ref().is_some_and(|s| s.pointer.is_some()),
            "scale": self.view.scale,
            "rasterized": self.renderer.rasterized,
            "prepareMs": self.prepare_ms,
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

    /// Finishes any pending GPS map build at once (parity captures).
    #[cfg(not(target_arch = "wasm32"))]
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
