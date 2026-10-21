//! The UI layer over the ride: touch controls and the toolbar, drawn by the
//! engine's UI pass (crates/atv-ui).
use atv_ui::{
    DisplayList, Fonts, Rect, Renderer,
    hud::{Viewport, toolbar},
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

/// A press on a toolbar button (utility pointers in touch.js).
struct Press {
    pointer: u64,
    button: toolbar::Button,
    x: f32,
    y: f32,
    moved: bool,
}

pub const MOUSE: u64 = u64::MAX;

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
    hovered: Option<toolbar::Button>,
    press: Option<Press>,
    /// CPU time of the last `prepare`, milliseconds.
    pub prepare_ms: f64,
    /// Parity test hook: a flat colour painted under the UI.
    pub backdrop: Option<[u8; 3]>,
}

impl Overlay {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, view: Viewport) -> Self {
        let settings = Settings::load();
        let mut touch = TouchControls::new(&settings, view.coarse);
        touch.set_viewport(view.width, view.height);
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

    fn hit(&self, x: f32, y: f32) -> Option<toolbar::Button> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .and_then(|(_, id)| toolbar::Button::from_id(*id))
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
            (!cancelled && !p.moved && nearby && hit == Some(p.button)).then_some(match p.button {
                toolbar::Button::Pause => Action::Pause,
                _ => Action::Camera,
            }),
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
        self.touch
            .paint(active, now, &mut self.fonts, &mut self.list);
        let state = toolbar::State {
            paused,
            hovered: self.hovered,
            pressed: self.press.as_ref().map(|p| p.button),
            camera_enabled,
            keys_enabled: false,
            // The keyboard card arrives with the HUD checkpoint.
            keys_visible: false,
            compact: false,
        };
        let root = layout::El::new(taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::length(self.view.width),
                height: taffy::Dimension::length(self.view.height),
            },
            ..Default::default()
        })
        .child(toolbar::element(&self.view, &state));
        layout::render(
            &root,
            &mut self.fonts,
            self.view.width,
            self.view.height,
            &mut self.list,
            &mut self.hits,
        );
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
        })
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
