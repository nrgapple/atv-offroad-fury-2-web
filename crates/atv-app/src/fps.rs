//! Display → Show FPS: the host side of [`atv_ui::fps`]. Records the time
//! of every frame the GPU surface presented, rebuilds the readout's text at
//! most twice a second, and draws it with its own small UI renderer after
//! the frame's UI, so the menus' cached UI layer and display list never
//! change for it.
use atv_ui::{DisplayList, Fonts, Renderer, fps, hud::Viewport};
use web_time::Instant;

/// The screen under the readout, which decides its corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// Menus: the status bar holds the top on phones in portrait, so the
    /// bottom-left corner (Back sits bottom right); elsewhere the empty
    /// middle of the status bar.
    Menu,
    /// A ride: the top-right corner above the speed and lap times, clear of
    /// the Pause/Camera toolbar (`toolbar_right`, CSS px) and of a top-right
    /// minimap (`map`: x, y, width, height).
    Ride {
        toolbar_right: f32,
        map: Option<[f32; 4]>,
    },
}

pub struct FpsHud {
    /// `?fps=1` / `?fps=0` for this session, over the saved choice.
    pub forced: Option<bool>,
    on: bool,
    origin: Instant,
    rate: fps::FrameRate,
    /// When the text was last rebuilt (ms since `origin`).
    refreshed: Option<f64>,
    top: String,
    bottom: String,
    /// The text being formatted (swapped in when it differs).
    next: (String, String),
    /// The next frame is the one drawn after the app went idle: it reads
    /// "idle" rather than the last active rate.
    idle_frame: bool,
    /// "idle" is on screen; no further idle frame until frames resume.
    idle_shown: bool,
    /// Its own renderer, for the device it was made on.
    renderer: Option<(wgpu::Device, Renderer)>,
    list: DisplayList,
    /// The text and layout the renderer holds.
    prepared: Option<(u32, u32, u32, Place, [u32; 4])>,
    changed: bool,
    /// Presents recorded since it was switched on (tests).
    presents: u64,
    /// Readout rectangle in CSS pixels (tests, diagnostics).
    pub rect: [f32; 4],
}

impl FpsHud {
    pub fn new(forced: Option<bool>) -> Self {
        Self {
            forced,
            on: false,
            origin: Instant::now(),
            rate: fps::FrameRate::default(),
            refreshed: None,
            top: String::with_capacity(16),
            bottom: String::with_capacity(32),
            next: (String::with_capacity(16), String::with_capacity(32)),
            idle_frame: false,
            idle_shown: false,
            renderer: None,
            list: DisplayList::default(),
            prepared: None,
            changed: true,
            presents: 0,
            rect: [0.; 4],
        }
    }

    fn ms(&self, at: Instant) -> f64 {
        at.saturating_duration_since(self.origin).as_secs_f64() * 1000.
    }

    /// A frame the surface presented at `at`.
    pub fn presented(&mut self, at: Instant) {
        if !self.on {
            return;
        }
        let t = self.ms(at);
        self.rate.present(t);
        self.presents += 1;
        if self.idle_frame {
            self.idle_frame = false;
            self.idle_shown = true;
        } else {
            self.idle_shown = false;
        }
    }

    /// While the app sleeps: when to draw the one frame that says "idle"
    /// (a little over a window after the last present), if still due.
    pub fn idle_due(&self) -> Option<Instant> {
        if !self.on || self.idle_shown {
            return None;
        }
        let last = self.rate.last()?;
        let after = std::time::Duration::from_secs_f64((last + fps::WINDOW_MS + 100.) / 1000.);
        Some(self.origin + after)
    }

    /// The next frame draws "idle" (the app wakes for it once).
    pub fn mark_idle_frame(&mut self) {
        self.idle_frame = true;
    }

    /// Before a frame's UI is drawn: follows the setting, rebuilds the text
    /// when due and the readout when its text or layout changed.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: &mut Fonts,
        view: &Viewport,
        (width, height): (u32, u32),
        setting: bool,
        place: Place,
    ) {
        let on = self.forced.unwrap_or(setting);
        if on != self.on {
            self.on = on;
            self.rate.clear();
            self.presents = 0;
            self.refreshed = None;
            self.idle_shown = false;
            self.idle_frame = false;
            self.prepared = None;
            if !on {
                // Frees the readout's atlas and buffers.
                self.renderer = None;
                return;
            }
        }
        if !on {
            return;
        }
        let now = self.ms(Instant::now());
        if self.idle_frame {
            fps::text(None, &mut self.top, &mut self.bottom);
            self.changed = true;
            self.refreshed = Some(now);
        } else if self.refreshed.is_none_or(|t| now - t >= fps::REFRESH_MS) {
            // Read at the last present: the frame being drawn now has not
            // been presented yet.
            let reading = self.rate.last().map(|t| self.rate.reading(t));
            let (top, bottom) = &mut self.next;
            match reading {
                Some(r) => fps::text(Some(&r), top, bottom),
                None => {
                    top.clear();
                    top.push_str("-- FPS");
                    bottom.clear();
                }
            }
            if *top != self.top || *bottom != self.bottom {
                std::mem::swap(top, &mut self.top);
                std::mem::swap(bottom, &mut self.bottom);
                self.changed = true;
            }
            self.refreshed = Some(now);
        }
        let layout = (
            width,
            height,
            view.scale.to_bits(),
            place,
            [
                view.width.to_bits(),
                view.height.to_bits(),
                view.safe[2].to_bits(),
                view.safe[3].to_bits(),
            ],
        );
        if !self.changed && self.prepared == Some(layout) {
            return;
        }
        self.changed = false;
        self.prepared = Some(layout);
        let scale = width as f32 / view.width.max(1.);
        let (w, h) = fps::size(fonts, self.top.chars().count(), self.bottom.chars().count());
        let [x, y] = position(view, place, w, h);
        self.rect = [x, y, w, h];
        self.list.clear();
        fps::paint(&mut self.list, fonts, &self.top, &self.bottom, x, y, scale);
        // A new device (a lost context, a driver reset) gets a new renderer.
        if self.renderer.as_ref().is_none_or(|(d, _)| d != device) {
            self.renderer = Some((
                device.clone(),
                Renderer::with_atlas(device, format, 512, 1024),
            ));
        }
        if let Some((_, renderer)) = &mut self.renderer {
            renderer.prepare(device, queue, fonts, &self.list, width, height, scale);
        }
    }

    /// The readout's renderer while it is shown, for the frame's UI pass to
    /// draw in the same pass ([`Renderer::draw_with`]).
    pub fn renderer(&self) -> Option<&Renderer> {
        self.renderer.as_ref().filter(|_| self.on).map(|(_, r)| r)
    }

    /// Its own pass, when the frame has no UI pass to join.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if let (true, Some((_, renderer))) = (self.on, &self.renderer) {
            renderer.draw(encoder, target);
        }
    }

    /// `?uiDiagnostics=1`: the readout as drawn.
    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({
            "on": self.on,
            "top": self.top,
            "bottom": self.bottom,
            "rect": self.rect,
            "presents": self.presents,
            "refreshed": self.refreshed,
        })
    }
}

/// The readout's top-left corner for a `w` x `h` box (CSS pixels).
fn position(view: &Viewport, place: Place, w: f32, h: f32) -> [f32; 2] {
    const GAP: f32 = 8.;
    let [top, right, bottom, left] = view.safe.map(|v| v.max(0.));
    let top = top + GAP;
    let right_x = view.width - right - GAP - w;
    match place {
        Place::Menu if view.height > view.width => [left + GAP, view.height - bottom - GAP - h],
        Place::Menu => [((view.width - w) / 2.).round(), top],
        Place::Ride {
            toolbar_right,
            map: Some([x, y, _, map_h]),
        } => {
            // Beside the map when the toolbar leaves room, else under it.
            let beside = x - GAP - w;
            if beside >= toolbar_right + GAP {
                [beside, top]
            } else {
                [right_x, y + map_h + GAP]
            }
        }
        Place::Ride { .. } => [right_x, top],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(width: f32, height: f32) -> Viewport {
        Viewport {
            width,
            height,
            scale: 3.,
            safe: [47., 0., 34., 0.],
            ..Default::default()
        }
    }

    #[test]
    fn corners_stay_inside_the_safe_area_and_clear_of_the_hud() {
        let (w, h) = (90., 34.);
        // Portrait menus: bottom left above the home indicator.
        assert_eq!(position(&view(390., 844.), Place::Menu, w, h), [8., 768.]);
        // Landscape menus: the status bar's middle.
        assert_eq!(position(&view(844., 390.), Place::Menu, w, h), [377., 55.]);
        let ride = Place::Ride {
            toolbar_right: 210.,
            map: None,
        };
        assert_eq!(position(&view(390., 844.), ride, w, h), [292., 55.]);
        // A top-right map: beside it with room, else under it.
        let map = |x| Place::Ride {
            toolbar_right: 210.,
            map: Some([x, 55., 160., 160.]),
        };
        assert_eq!(position(&view(844., 390.), map(676.), w, h), [578., 55.]);
        assert_eq!(position(&view(390., 844.), map(222.), w, h), [292., 223.]);
    }
}
