//! Instant replay playback (#542): plays the recorded stretch
//! (`instant_replay::Recorder`) with pause, slow motion and scrubbing,
//! through three cameras: the ride's own chase camera as it was, a
//! trackside vantage beside the line ahead that follows the rider, and a
//! slow orbit. The ride stays paused underneath; leaving resumes it as it
//! was.
use crate::instant_replay::{Moment, Recorder, Shot, look, vantage};
use atv_core::math::Det;
use atv_game::ride::bindings::Pad;
use atv_render::Camera;

/// Steps a jump moves (two seconds).
const JUMP: f64 = 120.;
/// A replay opens this far before the end (steps; 15 seconds).
const LEAD: f64 = 15. * 60.;
/// Playback speeds Slow cycles through.
const SPEEDS: [f32; 3] = [1., 0.5, 0.25];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Back,
    Photo,
}

pub struct Replay {
    /// The playback position, in ride steps (fractional).
    pub time: f64,
    pub playing: bool,
    speed: usize,
    pub shot: Shot,
    /// This frame's replayed riders (reused).
    pub moment: Moment,
    /// The trackside camera's vantage.
    vantage: Option<[f32; 3]>,
    /// The orbit camera's angle (radians).
    orbit: f32,
    pad_held: u32,
    /// The pointer holding the scrub bar.
    pub scrub: Option<u64>,
    /// Opened from the race results: Back returns there.
    pub from_results: bool,
    /// The ride's pause before the replay opened (restored after).
    pub was_paused: bool,
}

impl Replay {
    pub fn new(span: (u64, u64), from_results: bool, was_paused: bool) -> Self {
        Self {
            time: (span.1 as f64 - LEAD).max(span.0 as f64),
            playing: true,
            speed: 0,
            shot: Shot::Chase,
            moment: Moment::default(),
            vantage: None,
            orbit: 0.,
            // Buttons held as the replay opens are not presses.
            pad_held: u32::MAX,
            scrub: None,
            from_results,
            was_paused,
        }
    }

    /// `#ui-state` `replay`.
    pub fn state(&self, span: Option<(u64, u64)>) -> serde_json::Value {
        let m = &self.moment;
        serde_json::json!({
            "time": self.time, "span": span, "playing": self.playing,
            "speed": self.speed(), "shot": self.shot.label(),
            "fromResults": self.from_results,
            "origin": [m.player.quad_world[12], m.player.quad_world[13], m.player.quad_world[14]],
            "cpus": m.cpus.len(), "peers": m.peers.len(), "ghost": m.ghost.is_some(),
        })
    }

    pub fn speed(&self) -> f32 {
        SPEEDS[self.speed]
    }

    pub fn speed_label(&self) -> &'static str {
        ["1×", "½×", "¼×"][self.speed]
    }

    /// Play/pause; playing from the end starts over.
    pub fn toggle(&mut self, span: (u64, u64)) {
        if !self.playing && self.time >= span.1 as f64 {
            self.time = span.0 as f64;
        }
        self.playing = !self.playing;
    }

    pub fn slower(&mut self) {
        self.speed = (self.speed + 1) % SPEEDS.len();
    }

    pub fn cycle(&mut self) {
        self.shot = self.shot.next();
        self.vantage = None;
    }

    pub fn jump(&mut self, steps: f64, span: (u64, u64)) {
        self.time = (self.time + steps).clamp(span.0 as f64, span.1 as f64);
    }

    /// The position as a fraction of the recording.
    pub fn progress(&self, span: (u64, u64)) -> f32 {
        let length = (span.1 - span.0).max(1) as f64;
        ((self.time - span.0 as f64) / length).clamp(0., 1.) as f32
    }

    pub fn seek(&mut self, fraction: f32, span: (u64, u64)) {
        let length = (span.1 - span.0) as f64;
        self.time = span.0 as f64 + length * f64::from(fraction.clamp(0., 1.));
    }

    pub fn key(&mut self, code: &str, span: (u64, u64)) -> Option<Command> {
        match code {
            "Escape" | "Backspace" => return Some(Command::Back),
            "KeyP" => {
                self.playing = false;
                return Some(Command::Photo);
            }
            "Space" | "KeyK" | "Enter" | "NumpadEnter" => self.toggle(span),
            "KeyC" => self.cycle(),
            "KeyS" => self.slower(),
            "ArrowLeft" | "KeyJ" => self.jump(-JUMP, span),
            "ArrowRight" | "KeyL" => self.jump(JUMP, span),
            "Home" => self.time = span.0 as f64,
            _ => {}
        }
        None
    }

    /// One frame: the controller, then playback.
    pub fn frame(&mut self, dt: f32, pad: Option<&Pad>, span: (u64, u64)) -> Option<Command> {
        let dt = f64::from(dt.clamp(0., 0.1));
        let mut command = None;
        if let Some(pad) = pad {
            let held = pad
                .buttons
                .iter()
                .take(32)
                .enumerate()
                .fold(0u32, |bits, (i, b)| bits | (u32::from(*b > 0.5) << i));
            let pressed = held & !self.pad_held;
            self.pad_held = held;
            let edge = |i: u32| pressed & (1 << i) != 0;
            if edge(0) {
                self.toggle(span);
            }
            if edge(1) || edge(9) {
                command = Some(Command::Back);
            }
            if edge(2) {
                self.slower();
            }
            if edge(3) {
                self.cycle();
            }
            if edge(4) || edge(14) {
                self.jump(-JUMP, span);
            }
            if edge(5) || edge(15) {
                self.jump(JUMP, span);
            }
            if edge(8) {
                self.playing = false;
                command = Some(Command::Photo);
            }
            // The left stick scrubs at up to four times real time.
            let x = pad.axes[0];
            if x.abs() > 0.2 {
                self.jump(f64::from(x) * dt * 240., span);
            }
        } else {
            self.pad_held = 0;
        }
        if self.playing && self.scrub.is_none() {
            self.time += dt * 60. * f64::from(self.speed());
            if self.time >= span.1 as f64 {
                self.time = span.1 as f64;
                self.playing = false;
            }
        }
        self.time = self.time.clamp(span.0 as f64, span.1 as f64);
        command
    }

    /// This frame's camera for the replayed moment (`moment` sampled).
    pub fn camera(
        &mut self,
        recorder: &Recorder,
        dt: f32,
        ground: &dyn Fn(f32, f32) -> f32,
    ) -> (Camera, f32) {
        let w = &self.moment.player.quad_world;
        let subject = [w[12], w[13] + 2., w[14]];
        match self.shot {
            Shot::Chase => (self.moment.camera, self.moment.focal),
            Shot::Trackside => {
                let far = self
                    .vantage
                    .is_none_or(|v| (v[0] - subject[0]).hypot_d(v[2] - subject[2]) > 180.);
                if far {
                    let ahead = recorder.origin_at(self.time + 90.).unwrap_or(subject);
                    self.vantage = Some(vantage(subject, ahead, ground));
                }
                let eye = self.vantage.unwrap_or(subject);
                let distance = (eye[0] - subject[0])
                    .hypot_d(eye[2] - subject[2])
                    .hypot_d(eye[1] - subject[1]);
                // A long lens keeps the rider about the same size.
                let half = (10. / distance.max(1.))
                    .atan_d()
                    .clamp(6f32.to_radians(), 28f32.to_radians());
                (look(eye, subject), 1. / half.tan_d())
            }
            Shot::Orbit => {
                self.orbit += dt.clamp(0., 0.1) * 0.35;
                let (s, c) = self.orbit.sin_cos_d();
                let (x, z) = (subject[0] + s * 22., subject[2] + c * 22.);
                let floor = ground(x, z);
                let y = (subject[1] + 6.).max(if floor.is_finite() {
                    floor + 1.5
                } else {
                    f32::MIN
                });
                (look([x, y, z], subject), 1. / 30f32.to_radians().tan_d())
            }
        }
    }
}

/// The overlay's replay controls.
pub fn controls(replay: &Replay, span: (u64, u64), touch: bool) -> atv_ui::hud::viewer::State {
    use atv_ui::hud::viewer::{Button, State};
    let seconds = |steps: f64| steps / 60.;
    let at = seconds(replay.time - span.1 as f64);
    State {
        buttons: vec![
            Button::Back,
            Button::Camera,
            Button::Rewind,
            Button::Play,
            Button::Forward,
            Button::Speed,
            Button::Photo,
        ],
        hint: format!(
            "REPLAY {at:+.1}s · {}{}",
            replay.shot.label(),
            if touch {
                ""
            } else {
                " · Space play · ←/→ skip · C camera · S slow · P photo"
            }
        ),
        fov: 0,
        playing: replay.playing,
        shot: replay.shot.label(),
        speed: replay.speed_label(),
        progress: Some(replay.progress(span)),
        pressed: None,
        hovered: None,
    }
}

impl crate::App {
    /// Pause → Replay, or the race results' Replay: the recorded stretch
    /// plays over the paused ride (solo and split screen).
    pub(crate) fn open_replay(&mut self, from_results: bool) {
        let Some(ready) = &mut self.ready else { return };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        if ready.net.is_some() || riding.online || riding.held {
            return;
        }
        let Some(span) = riding.recorder.span() else {
            return;
        };
        let was_paused = riding.paused;
        riding.paused = true;
        riding.menu_open = true;
        riding.clear_input();
        riding.sync_sound();
        self.replay = Some(Replay::new(span, from_results, was_paused));
        if let Some(overlay) = &mut self.overlay {
            overlay.sync_menu(false);
            overlay.clear_input();
        }
        self.wake();
    }

    /// Back where the replay was opened: Pause, or the riding results.
    pub(crate) fn close_replay(&mut self) {
        let Some(replay) = self.replay.take() else {
            return;
        };
        self.photo = None;
        let mut paused = false;
        if let Some(riding) = self.ready.as_mut().and_then(|r| r.riding.as_mut()) {
            riding.paused = replay.was_paused;
            riding.menu_open = replay.was_paused;
            riding.clear_input();
            riding.sync_sound();
            paused = riding.paused && !riding.held;
        }
        if let Some(overlay) = &mut self.overlay {
            overlay.viewer = None;
            overlay.clear_input();
            overlay.sync_menu(paused);
        }
        self.wake();
    }

    pub(crate) fn replay_key(&mut self, code: &str) {
        let span = self.replay_span();
        let command = match (&mut self.replay, span) {
            (Some(replay), Some(span)) => replay.key(code, span),
            _ => None,
        };
        if let Some(command) = command {
            self.replay_command(command);
        }
    }

    fn replay_span(&self) -> Option<(u64, u64)> {
        self.ready
            .as_ref()
            .and_then(|r| r.riding.as_ref())
            .and_then(|r| r.recorder.span())
    }

    pub(crate) fn replay_command(&mut self, command: Command) {
        match command {
            Command::Back => self.close_replay(),
            Command::Photo => self.open_photo(),
        }
    }

    /// A replay control.
    pub(crate) fn replay_button(&mut self, button: atv_ui::hud::viewer::Button) {
        use atv_ui::hud::viewer::Button;
        let Some(span) = self.replay_span() else {
            return;
        };
        let Some(replay) = &mut self.replay else {
            return;
        };
        match button {
            Button::Back => self.close_replay(),
            Button::Photo => {
                replay.playing = false;
                self.open_photo();
            }
            Button::Camera => replay.cycle(),
            Button::Play => replay.toggle(span),
            Button::Speed => replay.slower(),
            Button::Rewind => replay.jump(-120., span),
            Button::Forward => replay.jump(120., span),
            _ => {}
        }
    }

    /// A pointer in a replay: the scrub bar, else the controls.
    pub(crate) fn replay_pointer(
        &mut self,
        id: u64,
        phase: winit::event::TouchPhase,
        x: f32,
        y: f32,
    ) {
        use winit::event::TouchPhase;
        let span = self.replay_span();
        let (Some(replay), Some(overlay), Some(span)) = (&mut self.replay, &mut self.overlay, span)
        else {
            return;
        };
        let bar = overlay.viewer_rect(atv_ui::hud::viewer::Button::Scrub);
        let fraction = |bar: atv_ui::Rect| (x - bar.x) / bar.w.max(1.);
        match phase {
            TouchPhase::Started => {
                if let Some(bar) = bar.filter(|b| b.contains(x, y)) {
                    replay.scrub = Some(id);
                    replay.seek(fraction(bar), span);
                } else {
                    overlay.press(id, x, y);
                }
            }
            TouchPhase::Moved => {
                if replay.scrub == Some(id) {
                    if let Some(bar) = bar {
                        replay.seek(fraction(bar), span);
                    }
                } else {
                    overlay.drag(id, x, y);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if replay.scrub == Some(id) {
                    replay.scrub = None;
                    return;
                }
                let cancelled = phase == TouchPhase::Cancelled;
                if let Some(Some(crate::overlay::Action::Viewer(button))) =
                    overlay.release(id, x, y, cancelled)
                {
                    self.replay_button(button);
                }
            }
        }
    }

    /// One ride frame of a replay, after the ride presented: the recorded
    /// riders replace the live ones and the replay camera the ride's.
    pub(crate) fn replay_frame(&mut self, dt: f32, pad: Option<&atv_game::ride::bindings::Pad>) {
        let (Some(replay), Some(ready)) = (&mut self.replay, &mut self.ready) else {
            return;
        };
        let Some(riding) = &mut ready.riding else {
            return;
        };
        let Some(span) = riding.recorder.span() else {
            return;
        };
        // Photo mode over a replay holds it still.
        let before = replay.time;
        let command = if self.photo.is_none() {
            replay.frame(dt, pad, span)
        } else {
            None
        };
        // Remastered rain falls (and plants sway) as the replay plays.
        if replay.playing {
            ready
                .scene
                .advance_weather(((replay.time - before) / 60.) as f32);
        }
        if !riding.recorder.sample(replay.time, &mut replay.moment) {
            return;
        }
        let m = &replay.moment;
        let origin = [
            m.player.quad_world[12],
            m.player.quad_world[13],
            m.player.quad_world[14],
        ];
        riding.recenter_on(&ready.gpu, &mut ready.scene, &ready.venue, origin);
        // Photo mode over a replay keeps its own camera.
        if self.photo.is_none() {
            let ground = |x: f32, z: f32| riding.visible_ground(x, z);
            let (camera, focal) = replay.camera(&riding.recorder, dt, &ground);
            ready.camera = camera;
            ready.focal_length = focal;
            if let Some(overlay) = &mut self.overlay {
                overlay.viewer = Some(controls(replay, span, overlay.view.touch_first));
            }
        }
        let m = &replay.moment;
        let scene = &mut ready.scene;
        scene.pose_ride(&ready.gpu, &m.player);
        scene.pose_race_cpus(&ready.gpu, &m.cpus, ready.camera.eye());
        if !m.peers.is_empty() {
            scene.pose_peers(&ready.gpu, &m.peers);
        }
        scene.pose_ghost(&ready.gpu, riding.ghost_slot, m.ghost.as_ref());
        if let Some(command) = command {
            self.replay_command(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_runs_slows_stops_at_the_end_and_restarts() {
        let span = (100, 100 + 20 * 60);
        let mut r = Replay::new(span, false, true);
        assert_eq!(r.time, (span.1 - 15 * 60) as f64);
        r.frame(1. / 60., None, span);
        assert!((r.time - (span.1 - 15 * 60 + 1) as f64).abs() < 1e-6);
        r.slower();
        let t = r.time;
        r.frame(1. / 60., None, span);
        assert!((r.time - t - 0.5).abs() < 1e-6);
        r.jump(1e9, span);
        r.frame(1. / 60., None, span);
        assert!(!r.playing && r.time == span.1 as f64);
        r.toggle(span);
        assert!(r.playing && r.time == span.0 as f64);
        r.seek(0.5, span);
        assert!((r.progress(span) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn keys_and_buttons_held_on_entry() {
        let span = (0, 600);
        let mut r = Replay::new(span, true, false);
        let mut pad = Pad {
            id: 1,
            buttons: Default::default(),
            axes: Default::default(),
        };
        pad.buttons[1] = 1.;
        assert_eq!(r.frame(0.01, Some(&pad), span), None);
        pad.buttons[1] = 0.;
        r.frame(0.01, Some(&pad), span);
        pad.buttons[1] = 1.;
        assert_eq!(r.frame(0.01, Some(&pad), span), Some(Command::Back));
        assert_eq!(r.key("KeyC", span), None);
        assert_eq!(r.shot, Shot::Trackside);
        assert_eq!(r.key("KeyP", span), Some(Command::Photo));
        assert!(!r.playing);
    }
}
