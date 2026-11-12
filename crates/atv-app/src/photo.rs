//! Photo mode (#542): a free camera around the paused moment. The ride
//! stays paused and untouched (presentation only); the camera orbits,
//! pans and zooms around a focus point that starts on the rider, kept
//! above the drawn ground and near the rider so the fine terrain patch
//! stays under the shot. Save writes the frame without the controls.
use atv_core::math::Det;
use atv_game::ride::bindings::Pad;
use atv_render::Camera;

/// The focus point stays this close (feet) to the rider it started on.
const PAN_LIMIT: f32 = 120.;
const DISTANCE: (f32, f32) = (1.5, 150.);
/// Vertical field of view, degrees.
const FOV: (f32, f32) = (10., 110.);
const PITCH_LIMIT: f32 = 1.45;
/// The eye keeps this far above the drawn ground (feet).
const CLEARANCE: f32 = 0.6;
/// Radians per CSS pixel of drag.
const ORBIT_RATE: f32 = 0.006;

/// The orbit camera: a focus point and the eye's direction and distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub target: [f32; 3],
    /// The direction the eye looks (atv_render::Camera's yaw and pitch).
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Vertical field of view, degrees.
    pub fov: f32,
    /// The rider's origin: the pan limit's centre.
    anchor: [f32; 3],
}

impl Orbit {
    /// Looking at `subject` from `camera`'s eye with its lens.
    pub fn around(camera: &Camera, focal: f32, subject: [f32; 3]) -> Self {
        let target = [subject[0], subject[1] + 2., subject[2]];
        let d = [
            target[0] - camera.x,
            target[1] - camera.y,
            target[2] - camera.z,
        ];
        let flat = d[0].hypot_d(d[2]);
        let distance = flat.hypot_d(d[1]).clamp(DISTANCE.0, DISTANCE.1);
        let (yaw, pitch) = if distance > 0.01 && flat.is_finite() {
            (d[0].atan2_d(d[2]), d[1].atan2_d(flat))
        } else {
            (camera.yaw, camera.pitch)
        };
        let fov = if focal > 0. && focal.is_finite() {
            (2. * (1. / focal).atan_d()).to_degrees()
        } else {
            60.
        };
        Self {
            target,
            yaw,
            pitch: pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT),
            distance,
            fov: fov.clamp(FOV.0, FOV.1),
            anchor: subject,
        }
    }

    fn axes(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let (sy, cy) = self.yaw.sin_cos_d();
        let (sp, cp) = self.pitch.sin_cos_d();
        (
            [sy * cp, sp, cy * cp],
            [cy, 0., -sy],
            [-sy * sp, cp, -cy * sp],
        )
    }

    /// Drag `dx`, `dy` CSS pixels: the eye circles the focus point.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * ORBIT_RATE;
        self.pitch = (self.pitch - dy * ORBIT_RATE).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Drag `dx`, `dy` CSS pixels on a `height`-pixel view: the focus point
    /// follows the finger across the picture plane.
    pub fn pan(&mut self, dx: f32, dy: f32, height: f32) {
        let (_, right, up) = self.axes();
        let per_pixel = 2. * self.distance * (self.fov.to_radians() / 2.).tan_d() / height.max(1.);
        self.shift([
            (-right[0] * dx + up[0] * dy) * per_pixel,
            (-right[1] * dx + up[1] * dy) * per_pixel,
            (-right[2] * dx + up[2] * dy) * per_pixel,
        ]);
    }

    /// Moves the focus point along the ground: `forward` and `right` in
    /// the eye's heading, `up` straight up (feet).
    pub fn walk(&mut self, forward: f32, right: f32, up: f32) {
        let (sy, cy) = self.yaw.sin_cos_d();
        self.shift([sy * forward + cy * right, up, cy * forward - sy * right]);
    }

    fn shift(&mut self, by: [f32; 3]) {
        let mut target = [
            self.target[0] + by[0],
            self.target[1] + by[1],
            self.target[2] + by[2],
        ];
        let offset = [
            target[0] - self.anchor[0],
            target[1] - self.anchor[1],
            target[2] - self.anchor[2],
        ];
        let length = offset[0].hypot_d(offset[1]).hypot_d(offset[2]);
        if length > PAN_LIMIT {
            for (t, (a, o)) in target.iter_mut().zip(self.anchor.iter().zip(offset)) {
                *t = a + o * PAN_LIMIT / length;
            }
        }
        if target.iter().all(|v| v.is_finite()) {
            self.target = target;
        }
    }

    /// `factor` below 1 moves the eye in.
    pub fn zoom(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0. {
            self.distance = (self.distance * factor).clamp(DISTANCE.0, DISTANCE.1);
        }
    }

    pub fn widen(&mut self, degrees: f32) {
        self.fov = (self.fov + degrees).clamp(FOV.0, FOV.1);
    }

    /// The scene camera and focal length, the eye lifted clear of the
    /// ground `ground` draws.
    pub fn camera(&self, ground: impl Fn(f32, f32) -> f32) -> (Camera, f32) {
        let (forward, ..) = self.axes();
        let mut eye = [
            self.target[0] - forward[0] * self.distance,
            self.target[1] - forward[1] * self.distance,
            self.target[2] - forward[2] * self.distance,
        ];
        let floor = ground(eye[0], eye[2]) + CLEARANCE;
        let (mut yaw, mut pitch) = (self.yaw, self.pitch);
        if floor.is_finite() && eye[1] < floor {
            // Under the ground: lift the eye and keep it on the focus point.
            eye[1] = floor;
            let d = [
                self.target[0] - eye[0],
                self.target[1] - eye[1],
                self.target[2] - eye[2],
            ];
            let flat = d[0].hypot_d(d[2]);
            if flat > 0.01 {
                yaw = d[0].atan2_d(d[2]);
                pitch = d[1].atan2_d(flat);
            }
        }
        (
            Camera {
                x: eye[0],
                y: eye[1],
                z: eye[2],
                yaw,
                pitch,
            },
            1. / (self.fov.to_radians() / 2.).tan_d(),
        )
    }
}

/// What photo-mode input asks the app for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Back,
    Save,
}

/// Mouse, touch, keyboard and controller input for the orbit camera.
pub struct Photo {
    pub orbit: Orbit,
    /// Pointers down off the controls (at most two: a pinch).
    pointers: Vec<(u64, [f32; 2], bool)>,
    /// The previous pinch's spread and midpoint.
    pinch: Option<(f32, [f32; 2])>,
    /// Controller buttons held on the previous frame (edges only).
    pad_held: u32,
    /// A notice for the hint line and the time (s) it clears.
    pub notice: Option<(String, f64)>,
    /// Save asked: the next frame draws without the controls and is saved.
    pub capture: bool,
}

/// A pointer id the right mouse button uses (pans).
pub const RIGHT_MOUSE: u64 = u64::MAX - 1;

impl Photo {
    pub fn new(orbit: Orbit) -> Self {
        Self {
            orbit,
            pointers: Vec::with_capacity(4),
            pinch: None,
            // Buttons held as photo mode opens (the menu's Accept) are not
            // presses.
            pad_held: u32::MAX,
            notice: None,
            capture: false,
        }
    }

    /// `#ui-state` `photo`.
    pub fn state(&self) -> serde_json::Value {
        let o = &self.orbit;
        serde_json::json!({
            "target": o.target, "yaw": o.yaw, "pitch": o.pitch,
            "distance": o.distance, "fov": o.fov,
            "notice": self.notice.as_ref().map(|n| &n.0),
        })
    }

    /// The hint line: a notice, else the controls for this device.
    pub fn hint(&self, touch: bool, now: f64) -> String {
        match &self.notice {
            Some((text, until)) if now < *until => text.clone(),
            _ if touch => "Drag to orbit · Pinch to zoom · Two fingers to pan".into(),
            _ => "Drag/arrows orbit · Right-drag/WASD pan · Wheel/Q/E zoom · [ ] FOV".into(),
        }
    }

    pub fn pointer_down(&mut self, id: u64, x: f32, y: f32) {
        self.pointers.retain(|(p, ..)| *p != id);
        if self.pointers.len() < 2 {
            self.pointers.push((id, [x, y], id == RIGHT_MOUSE));
        }
        self.pinch = None;
    }

    pub fn pointer_move(&mut self, id: u64, x: f32, y: f32, height: f32) {
        let Some(at) = self.pointers.iter().position(|(p, ..)| *p == id) else {
            return;
        };
        let (_, last, pan) = self.pointers[at];
        self.pointers[at].1 = [x, y];
        if let [(_, a, _), (_, b, _)] = self.pointers[..] {
            let spread = (a[0] - b[0]).hypot_d(a[1] - b[1]);
            let middle = [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
            if let Some((was, from)) = self.pinch
                && spread > 1.
            {
                self.orbit.zoom(was / spread);
                self.orbit
                    .pan(middle[0] - from[0], middle[1] - from[1], height);
            }
            self.pinch = Some((spread, middle));
        } else if pan {
            self.orbit.pan(x - last[0], y - last[1], height);
        } else {
            self.orbit.orbit(x - last[0], y - last[1]);
        }
    }

    pub fn pointer_up(&mut self, id: u64) {
        self.pointers.retain(|(p, ..)| *p != id);
        self.pinch = None;
    }

    /// A wheel turn of `dy` CSS pixels (positive away from the view).
    pub fn wheel(&mut self, dy: f32) {
        self.orbit.zoom((dy * 0.002).exp_d());
    }

    /// A key press (held keys move the camera in `frame`).
    pub fn key(&mut self, code: &str) -> Option<Command> {
        match code {
            "Escape" | "Backspace" | "KeyP" => Some(Command::Back),
            "Enter" | "NumpadEnter" | "Space" => Some(Command::Save),
            _ => None,
        }
    }

    /// One frame of held keys and the controller.
    pub fn frame(
        &mut self,
        dt: f32,
        keys: &dyn Fn(&str) -> bool,
        pad: Option<&Pad>,
    ) -> Option<Command> {
        let dt = dt.clamp(0., 0.1);
        let axis = |minus: &[&str], plus: &[&str]| -> f32 {
            f32::from(u8::from(plus.iter().any(|k| keys(k))))
                - f32::from(u8::from(minus.iter().any(|k| keys(k))))
        };
        let turn = axis(&["ArrowLeft"], &["ArrowRight"]);
        let tilt = axis(&["ArrowDown"], &["ArrowUp"]);
        let ahead = axis(&["KeyS"], &["KeyW"]);
        let side = axis(&["KeyA"], &["KeyD"]);
        let rise = axis(&["KeyF"], &["KeyR"]);
        let push = axis(
            &["KeyE", "Equal", "NumpadAdd"],
            &["KeyQ", "Minus", "NumpadSubtract"],
        );
        let widen = axis(&["BracketLeft"], &["BracketRight"]);
        let mut command = None;
        let (mut turn, mut tilt, mut ahead, mut side, mut push, mut widen) =
            (turn, tilt, ahead, side, push, widen);
        if let Some(pad) = pad {
            let dead = |v: f32| if v.abs() < 0.15 { 0. } else { v };
            side += dead(pad.axes[0]);
            ahead -= dead(pad.axes[1]);
            turn += dead(pad.axes.get(2).copied().unwrap_or(0.));
            tilt -= dead(pad.axes.get(3).copied().unwrap_or(0.));
            let button = |i: usize| pad.buttons.get(i).copied().unwrap_or(0.);
            push += button(6) - button(7);
            widen += button(5) - button(4);
            let held = pad
                .buttons
                .iter()
                .take(32)
                .enumerate()
                .fold(0u32, |bits, (i, b)| bits | (u32::from(*b > 0.5) << i));
            let pressed = held & !self.pad_held;
            self.pad_held = held;
            if pressed & 1 != 0 {
                command = Some(Command::Save);
            }
            if pressed & (1 << 1 | 1 << 9) != 0 {
                command = Some(Command::Back);
            }
        } else {
            self.pad_held = 0;
        }
        self.orbit.orbit(turn * 260. * dt, tilt * 260. * dt);
        let speed = self.orbit.distance.max(4.) * 1.2 * dt;
        self.orbit.walk(ahead * speed, side * speed, rise * speed);
        self.orbit.zoom((push * 1.5 * dt).exp_d());
        self.orbit.widen(widen * 30. * dt);
        command
    }
}

/// The overlay's controls for photo mode at `now` (s): none on the frame
/// that saves.
pub fn controls(photo: &Photo, touch: bool, now: f64) -> atv_ui::hud::viewer::State {
    use atv_ui::hud::viewer::{Button, State};
    State {
        buttons: if photo.capture {
            Vec::new()
        } else {
            vec![Button::Back, Button::FovDown, Button::FovUp, Button::Save]
        },
        hint: photo.hint(touch, now),
        fov: photo.orbit.fov.round() as u32,
        pressed: None,
        hovered: None,
    }
}

/// A photo's file name for `seconds` since the Unix epoch (UTC).
pub fn file_name(seconds: u64) -> String {
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "fury-remastered-{year:04}-{month:02}-{day:02}-{:02}{:02}{:02}.png",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

/// Desktop photos: the user's Pictures folder, else beside the settings
/// (Android) or in the home folder.
#[cfg(not(target_arch = "wasm32"))]
fn photo_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let env = |name| std::env::var_os(name).map(PathBuf::from);
    let home = env("HOME").or_else(|| env("USERPROFILE"));
    if let Some(pictures) = home
        .as_ref()
        .map(|h| h.join("Pictures"))
        .filter(|p| p.is_dir())
    {
        return Some(pictures.join("Fury Remastered"));
    }
    // Android keeps its settings file in the app's own storage.
    env("FURY_SETTINGS")
        .and_then(|p| p.parent().map(|d| d.join("Photos")))
        .or_else(|| home.map(|h| h.join("Fury Remastered Photos")))
}

#[cfg(not(target_arch = "wasm32"))]
fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .map_err(|e| e.to_string())
}

/// Browser photos: the canvas as just drawn, downloaded as a PNG.
#[cfg(target_arch = "wasm32")]
fn download_canvas(name: &str) -> Result<(), String> {
    use wasm_bindgen::{JsCast, closure::Closure};
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let canvas = document
        .get_element_by_id("game")
        .or_else(|| document.query_selector("canvas").ok().flatten())
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or("no canvas")?;
    let name = name.to_string();
    // toBlob copies the drawn frame now and encodes it off the frame.
    let done = Closure::once_into_js(move |blob: Option<web_sys::Blob>| {
        let Some(blob) = blob else { return };
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
            return;
        };
        if let Some(link) = document
            .create_element("a")
            .ok()
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = link.set_attribute("href", &url);
            let _ = link.set_attribute("download", &name);
            link.click();
        }
        // Revoked after the download has had its turn.
        let revoke = Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                revoke.unchecked_ref(),
                10_000,
            );
        }
    });
    canvas
        .to_blob(done.unchecked_ref())
        .map_err(|e| format!("{e:?}"))
}

impl crate::App {
    /// Pause → Photo mode: the menu closes over the paused ride and the
    /// free camera starts at the ride's own view (solo and split screen).
    pub(crate) fn open_photo(&mut self) {
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        if ready.net.is_some() || riding.online {
            return;
        }
        let subject = riding.ride.frame().origin;
        self.photo = Some(Photo::new(Orbit::around(
            &ready.camera,
            ready.focal_length,
            subject,
        )));
        riding.clear_input();
        if let Some(overlay) = &mut self.overlay {
            overlay.sync_menu(false);
            overlay.clear_input();
        }
        // The ride stays paused under the free camera.
        riding.menu_open = true;
        self.wake();
    }

    /// Back to the Pause menu, the ride as it was.
    pub(crate) fn close_photo(&mut self) {
        self.photo = None;
        if let Some(overlay) = &mut self.overlay {
            overlay.viewer = None;
            overlay.clear_input();
            let paused = self
                .ready
                .as_ref()
                .and_then(|r| r.riding.as_ref())
                .is_some_and(|r| r.paused && !r.held);
            overlay.sync_menu(paused);
        }
        self.wake();
    }

    /// A pointer in photo mode: the controls, else the camera.
    pub(crate) fn photo_pointer(
        &mut self,
        id: u64,
        phase: winit::event::TouchPhase,
        x: f32,
        y: f32,
    ) {
        use winit::event::TouchPhase;
        let (Some(photo), Some(overlay)) = (&mut self.photo, &mut self.overlay) else {
            return;
        };
        let height = overlay.view.height;
        match phase {
            TouchPhase::Started => {
                if id == RIGHT_MOUSE || !overlay.press(id, x, y) {
                    photo.pointer_down(id, x, y);
                }
            }
            TouchPhase::Moved => {
                if !overlay.drag(id, x, y) {
                    photo.pointer_move(id, x, y, height);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                photo.pointer_up(id);
                let cancelled = phase == TouchPhase::Cancelled;
                if let Some(Some(crate::overlay::Action::Viewer(button))) =
                    overlay.release(id, x, y, cancelled)
                {
                    self.viewer_button(button);
                }
            }
        }
    }

    pub(crate) fn photo_key(&mut self, code: &str) {
        let command = self.photo.as_mut().and_then(|p| p.key(code));
        if let Some(command) = command {
            self.photo_command(command);
        }
    }

    pub(crate) fn viewer_button(&mut self, button: atv_ui::hud::viewer::Button) {
        use atv_ui::hud::viewer::Button;
        match button {
            Button::Back => self.photo_command(Command::Back),
            Button::Save => self.photo_command(Command::Save),
            Button::FovDown | Button::FovUp => {
                if let Some(photo) = &mut self.photo {
                    photo
                        .orbit
                        .widen(if button == Button::FovUp { 5. } else { -5. });
                }
            }
        }
    }

    pub(crate) fn photo_command(&mut self, command: Command) {
        match command {
            Command::Back => self.close_photo(),
            Command::Save => {
                if let Some(photo) = &mut self.photo {
                    photo.capture = true;
                }
            }
        }
    }

    /// One ride frame in photo mode, after the ride presented: the free
    /// camera replaces the ride's and the overlay shows the controls.
    pub(crate) fn photo_frame(&mut self, dt: f32, pad: Option<&Pad>) {
        let names: std::collections::HashSet<String> =
            self.keys.iter().map(|k| format!("{k:?}")).collect();
        let now = (web_time::Instant::now() - self.started).as_secs_f64();
        let (Some(photo), Some(ready)) = (&mut self.photo, &mut self.ready) else {
            return;
        };
        let Some(riding) = &ready.riding else { return };
        let command = photo.frame(dt, &|code: &str| names.contains(code), pad);
        let (camera, focal) = photo.orbit.camera(|x, z| riding.visible_ground(x, z));
        ready.camera = camera;
        ready.focal_length = focal;
        if let Some(overlay) = &mut self.overlay {
            overlay.viewer = Some(controls(photo, overlay.view.touch_first, now));
        }
        if let Some(command) = command {
            self.photo_command(command);
        }
    }

    /// After a frame is drawn: a photo asked for is saved from it.
    pub(crate) fn finish_photo(&mut self) {
        let Some(photo) = self.photo.as_mut().filter(|p| p.capture) else {
            return;
        };
        // The frame just drawn had no controls (`controls`).
        if self
            .overlay
            .as_ref()
            .and_then(|o| o.viewer.as_ref())
            .is_some_and(|v| !v.buttons.is_empty())
        {
            return;
        }
        photo.capture = false;
        let seconds = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let name = file_name(seconds);
        let now = (web_time::Instant::now() - self.started).as_secs_f64();
        #[cfg(target_arch = "wasm32")]
        let result = download_canvas(&name).map(|()| "Photo saved to your downloads".to_string());
        #[cfg(not(target_arch = "wasm32"))]
        let result = match (&mut self.ready, photo_dir()) {
            (Some(ready), Some(dir)) => {
                let (width, height) = (ready.gpu.config.width, ready.gpu.config.height);
                let view = atv_render::View {
                    camera: ready.camera,
                    focal_length: ready.focal_length,
                    turns: 0,
                };
                let path = dir.join(&name);
                ready
                    .scene
                    .capture_with(&ready.gpu, &view, width, height, &mut |_, _| {})
                    .and_then(|rgba| write_png(&path, width, height, &rgba))
                    .map(|()| format!("Photo saved to {}", path.display()))
            }
            _ => Err("no folder for photos".into()),
        };
        let text = result.unwrap_or_else(|e| format!("Photo not saved: {e}"));
        crate::status(&text);
        if let Some(photo) = &mut self.photo {
            photo.notice = Some((text, now + 3.));
        }
        self.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn behind() -> Orbit {
        // The eye 10 ft behind (-z) and level with the focus point.
        let camera = Camera {
            x: 0.,
            y: 2.,
            z: -10.,
            yaw: 0.,
            pitch: 0.,
        };
        Orbit::around(&camera, 1. / 30f32.to_radians().tan_d(), [0., 0., 0.])
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn starts_on_the_rider_from_the_ride_camera() {
        let orbit = behind();
        assert_eq!(orbit.target, [0., 2., 0.]);
        assert!(close(orbit.distance, 10.));
        assert!(close(orbit.yaw, 0.) && close(orbit.pitch, 0.));
        assert!(close(orbit.fov, 60.));
        let (camera, focal) = orbit.camera(|_, _| -100.);
        assert!(close(camera.x, 0.) && close(camera.y, 2.) && close(camera.z, -10.));
        assert!(close(focal, 1. / 30f32.to_radians().tan_d()));
    }

    #[test]
    fn orbit_keeps_distance_and_the_eye_stays_above_ground() {
        let mut orbit = behind();
        orbit.orbit(200., 0.);
        let (camera, _) = orbit.camera(|_, _| -100.);
        let d = (camera.x.powi(2) + (camera.y - 2.).powi(2) + camera.z.powi(2)).sqrt();
        assert!(close(d, 10.));
        // Looking up from below the ground: the eye is lifted clear.
        orbit.orbit(0., 400.);
        let (camera, _) = orbit.camera(|_, _| 1.);
        assert!(camera.y >= 1. + CLEARANCE - 1e-4);
    }

    #[test]
    fn zoom_fov_and_pan_are_limited() {
        let mut orbit = behind();
        orbit.zoom(1e-6);
        assert_eq!(orbit.distance, DISTANCE.0);
        orbit.zoom(1e9);
        assert_eq!(orbit.distance, DISTANCE.1);
        orbit.widen(1e3);
        assert_eq!(orbit.fov, FOV.1);
        for _ in 0..100 {
            orbit.walk(50., 0., 0.);
        }
        let t = orbit.target;
        assert!((t[0].powi(2) + t[1].powi(2) + t[2].powi(2)).sqrt() <= PAN_LIMIT + 1e-3);
    }

    #[test]
    fn one_finger_orbits_and_a_pinch_zooms() {
        let mut photo = Photo::new(behind());
        photo.pointer_down(1, 100., 100.);
        photo.pointer_move(1, 150., 100., 800.);
        assert!(photo.orbit.yaw < 0.);
        photo.pointer_down(2, 200., 100.);
        photo.pointer_move(2, 200., 100., 800.);
        let before = photo.orbit.distance;
        photo.pointer_move(2, 250., 100., 800.);
        assert!(photo.orbit.distance < before);
        photo.pointer_up(1);
        photo.pointer_up(2);
        assert!(photo.pointers.is_empty());
    }

    #[test]
    fn controller_buttons_held_on_entry_are_not_presses() {
        let mut photo = Photo::new(behind());
        let mut pad = Pad {
            id: 1,
            buttons: Default::default(),
            axes: Default::default(),
        };
        pad.buttons[0] = 1.;
        assert_eq!(photo.frame(0.016, &|_| false, Some(&pad)), None);
        pad.buttons[0] = 0.;
        assert_eq!(photo.frame(0.016, &|_| false, Some(&pad)), None);
        pad.buttons[0] = 1.;
        assert_eq!(
            photo.frame(0.016, &|_| false, Some(&pad)),
            Some(Command::Save)
        );
        pad.buttons[1] = 1.;
        assert_eq!(
            photo.frame(0.016, &|_| false, Some(&pad)),
            Some(Command::Back)
        );
    }
}
