//! Touch riding controls (web/touch.js `mountTouchControls` + touch.css):
//! one-finger compass (hold anywhere to ride, drag to steer and lean, lift on
//! ramps to jump, second finger for reverse or the airborne trick wheel) and
//! the two-finger layout (right side drives, left thumb steers and leans).
pub mod input;

use crate::{
    color::{Color, Fill, Gradient},
    paint::{BoxPaint, BoxShadow, DisplayList, Item, Rect},
    settings::Settings,
    text::{Font, Fonts, Join, Shape, Stroke, TextStyle},
};
use input::{FLIPS, Mode, OUTER_DISTANCE, Sample, TRICK_WHEEL, TouchInput, TouchLeanIntent};

const COACH_RIDES: u32 = 3;
const COACH_WHEEL: u32 = 5;
const WHEEL_RADIUS: f32 = 156.;
const HUB: f32 = 28.;
const INNER_EDGE: f32 = 126.;
const INNER_LABEL: f32 = 80.;
const OUTER_LABEL: f32 = 143.;
const SIDE_MARGIN: f32 = 160.;

const GOLD: Color = Color::hex("#e5d360");
const GOLD_LIGHT: Color = Color::hex("#fffaa1");
const SILVER_LIGHT: Color = Color::hex("#e9e9e9");
const BLUE: Color = Color::hex("#3f9cff");
const ORANGE: Color = Color::hex("#f4a13c");
const RED_BRIGHT: Color = Color::hex("#e0262a");
const INK: Color = Color::hex("#080808");

/// What the overlay needs from the latest simulation frame (`feedback`).
#[derive(Clone, Debug, Default)]
pub struct Feedback {
    pub grounded: [bool; 4],
    pub preload_available: bool,
    pub preload_armed: bool,
    pub preload_active: bool,
    /// The applied touch intent's generation and held flag.
    pub applied_touch: Option<(u64, bool)>,
    pub applied_brake: f32,
    pub flip_launch_window: bool,
    pub stunt_name: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Preload {
    Idle,
    Available,
    Armed,
    Active,
}

/// Touch input for one batch, merged by the ride like game.js `drivingInput`.
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    /// Added to the keyboard/controller steer and lean.
    pub steer: f32,
    pub lean: f32,
    /// Alpha Arcade flips' 360 axis: the one-handed thumb or the two-finger
    /// lean pad dragged past full lock (game.js `touchLean.spin + touch.spin`).
    pub spin: f32,
    /// An Extras flip to request: (flip, launch, hold) from the thumb's flip
    /// zones or swipes, or the two-finger pad's pull when the drive thumb
    /// lifts (game.js `drivingInput` padFlip / `touch.flip_request`).
    pub flip: Option<(&'static str, bool, f32)>,
    pub sample: Sample,
    pub generation: u64,
}

pub struct TouchControls {
    pub input: TouchInput,
    pub lean: TouchLeanIntent,
    /// A touch has driven this session.
    pub touched: bool,
    /// `(any-pointer: coarse)`.
    pub coarse: bool,
    airborne: bool,
    last: Option<Feedback>,
    /// The flip zone whose arming last buzzed (touch.js `flipBuzzed`).
    flip_buzzed: Option<usize>,
    preload: Preload,
    rides: u32,
    wheel_opens: u32,
    rode_this_session: bool,
    wheel_visible: bool,
    marker: [f32; 2],
    held: bool,
    held_since: f64,
    burst_since: Option<f64>,
    throttle: (f32, f32, f64),
    label: String,
    viewport: [f32; 2],
}

thread_local! {
    /// touch.js `rodeThisSession`: coaching counts one ride per page load,
    /// not one per ride (each ride builds new controls).
    static RODE_THIS_SESSION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn count(settings: &Settings, key: &str) -> u32 {
    settings
        .get(key)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.)
        .map_or(0, |v| v as u32)
}

impl TouchControls {
    pub fn new(settings: &Settings, coarse: bool) -> Self {
        let mut input = TouchInput::new(false);
        // Only an explicitly saved Off overrides the one-finger default.
        input.set_one_handed(settings.get("atv-mobile-one-handed").as_deref() != Some("false"));
        input.steering_sensitivity =
            input::load_sensitivity(settings.get("atv-mobile-steering-sensitivity").as_deref());
        Self {
            input,
            lean: TouchLeanIntent::default(),
            touched: false,
            coarse,
            airborne: false,
            last: None,
            flip_buzzed: None,
            preload: Preload::Idle,
            rides: count(settings, "atv-touch-coach-rides"),
            wheel_opens: count(settings, "atv-touch-coach-wheel"),
            rode_this_session: RODE_THIS_SESSION.get(),
            wheel_visible: false,
            marker: [0., 0.],
            held: false,
            held_since: f64::NEG_INFINITY,
            burst_since: None,
            throttle: (0., 0., 0.),
            label: "PRELOAD".into(),
            viewport: [0., 0.],
        }
    }

    /// Controls → One-handed mobile (saved like `toggleOneHanded`).
    pub fn set_one_handed(&mut self, enabled: bool, settings: &mut Settings) {
        self.input.set_one_handed(enabled);
        settings.set("atv-mobile-one-handed", &enabled.to_string());
        self.clear();
    }

    /// Controls → Mobile steering sensitivity (`setSteeringSensitivity`).
    pub fn set_steering_sensitivity(&mut self, value: f32, settings: &mut Settings) {
        self.input.steering_sensitivity = input::normalize_sensitivity(value);
        settings.set(
            "atv-mobile-steering-sensitivity",
            &self.input.steering_sensitivity.to_string(),
        );
    }

    pub fn one_handed(&self) -> bool {
        self.input.one_handed
    }

    /// `body.touch-driving`: the overlay owns the screen on touch devices.
    pub fn driving(&self, active: bool) -> bool {
        active && (self.coarse || self.touched)
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.lean.clear();
        self.last = None;
        self.held = false;
        self.preload = Preload::Idle;
        self.burst_since = None;
        self.wheel_visible = false;
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        if self.viewport != [width, height] {
            if self.viewport != [0., 0.] {
                self.clear();
            }
            self.viewport = [width, height];
        }
    }

    fn air_label(&self) -> &'static str {
        if self.input.flips {
            "SWIPE · LONG SWIPE FLIPS"
        } else {
            "SWIPE / CIRCLE"
        }
    }

    /// The secondary wheel origin stays on screen (`placeWheel`).
    fn place_wheel(&mut self) {
        let [width, height] = self.viewport;
        let Some(p) = self
            .input
            .secondary
            .as_mut()
            .and_then(|s| s.pointer.as_mut())
            .filter(|p| p.mode == Mode::Radial)
        else {
            return;
        };
        p.x = if width < SIDE_MARGIN * 2. {
            width / 2.
        } else {
            p.current_x.clamp(SIDE_MARGIN, width - SIDE_MARGIN)
        };
        p.y = p
            .current_y
            .min(height - WHEEL_RADIUS - 16.)
            .max(WHEEL_RADIUS + 26.);
    }

    /// A touch started at CSS `x`, `y`. Returns true when the overlay owns it.
    pub fn down(
        &mut self,
        id: u64,
        x: f32,
        y: f32,
        now: f64,
        active: bool,
        settings: &mut Settings,
    ) -> bool {
        if !active {
            return false;
        }
        let [width, height] = self.viewport;
        if !self.input.one_handed && x < width / 2. {
            // game.js: the left half steers and leans (TouchLeanIntent).
            self.touched = true;
            return self.lean.begin(id, x, y);
        }
        self.touched = true;
        if !self.input.begin(id, x, y, self.airborne) {
            return true;
        }
        if self.input.secondary.as_ref().is_some_and(|s| s.owns(id)) {
            self.place_wheel();
        }
        if !self.held {
            self.held_since = now;
        }
        self.held = true;
        self.burst_since = None;
        if self.input.pointer.as_ref().is_some_and(|p| p.id == id) {
            if !self.rode_this_session {
                self.rode_this_session = true;
                RODE_THIS_SESSION.set(true);
                self.rides += 1;
                settings.set("atv-touch-coach-rides", &self.rides.to_string());
            }
            let (side, top, bottom) = if self.input.one_handed {
                (74., 82., 60.)
            } else {
                (72., 64., 40.)
            };
            self.marker = [
                x.min(width - side).max(side),
                y.min(height - bottom).max(top),
            ];
        }
        if !self.input.one_handed {
            self.label = if self.airborne {
                self.air_label().into()
            } else {
                "PRELOAD".into()
            };
        }
        true
    }

    pub fn moved(&mut self, id: u64, x: f32, y: f32, now: f64) -> bool {
        if self.lean.owns(id) {
            self.lean
                .move_to(id, x, y, now, self.input.steering_sensitivity);
            return true;
        }
        if !self.input.owns(id) {
            return false;
        }
        self.input.move_to(id, x, y, now);
        true
    }

    pub fn up(&mut self, id: u64, x: f32, y: f32, cancelled: bool, now: f64) -> bool {
        if self.lean.owns(id) {
            self.lean.end(id);
            return true;
        }
        if !self.input.owns(id) {
            return false;
        }
        let radial = self.input.secondary.as_ref().is_some_and(|s| {
            s.owns(id) && s.pointer.as_ref().is_some_and(|p| p.mode == Mode::Radial)
        });
        if !cancelled && radial {
            self.input.move_to(id, x, y, now);
        }
        self.input.end(id, cancelled, now);
        self.held = self.input.pointer.is_some();
        if !self.held && self.preload == Preload::Active && self.input.one_handed {
            self.burst_since = Some(now);
        }
        true
    }

    /// Latest simulation frame: airborne state and preload cues.
    pub fn feedback(&mut self, frame: Feedback, now: f64) {
        let airborne = frame.grounded.iter().all(|g| !g);
        let was = self.input.airborne;
        self.airborne = airborne;
        self.input.set_airborne(airborne);
        if airborne != was {
            self.place_wheel();
        }
        let preload = if frame.preload_active {
            Preload::Active
        } else if frame.preload_armed {
            Preload::Armed
        } else if frame.preload_available {
            Preload::Available
        } else {
            Preload::Idle
        };
        if preload == Preload::Active && self.preload != Preload::Active && !self.held {
            self.burst_since = Some(now);
        }
        self.preload = preload;
        if !self.input.one_handed {
            self.label = if airborne {
                frame
                    .stunt_name
                    .clone()
                    .unwrap_or_else(|| self.air_label().into())
            } else {
                match preload {
                    Preload::Armed => "RELEASE",
                    Preload::Active => "PRELOADED",
                    _ => "BRACED",
                }
                .into()
            };
        }
        self.last = Some(frame);
    }

    /// This batch's touch controls (`touchControls.sample()` plus the
    /// two-finger lean thumb).
    pub fn sample(&mut self, now: f64, active: bool, settings: &mut Settings) -> Controls {
        if !active {
            self.clear();
        }
        let sample = self.input.sample(now);
        let wheel = self.wheel_open();
        if wheel && !self.wheel_visible {
            self.wheel_opens += 1;
            settings.set("atv-touch-coach-wheel", &self.wheel_opens.to_string());
        }
        self.wheel_visible = wheel;
        let sensitivity = self.input.steering_sensitivity;
        let pad_flip = (sample.flip_request.is_none()
            && sample.preload_axis == Some(-1.)
            && self.input.flips
            && !self.input.arcade
            && !self.input.one_handed)
            .then(|| self.lean.launch_flip(now))
            .flatten()
            .map(|(flip, hold)| (flip, true, hold));
        let flip = pad_flip.or_else(|| {
            sample
                .flip_request
                .map(|flip| (flip, sample.flip_launch, sample.flip_hold))
        });
        Controls {
            steer: self.lean.steer(sensitivity) + sample.steer,
            lean: self.lean.lean() + sample.lean,
            spin: self.lean.spin(sensitivity) + sample.spin,
            flip,
            sample,
            generation: self.input.generation,
        }
    }

    fn wheel_open(&self) -> bool {
        self.input.one_handed
            && self.input.pointer.is_some()
            && self
                .input
                .secondary
                .as_ref()
                .and_then(|s| s.pointer.as_ref())
                .is_some_and(|p| p.mode == Mode::Radial)
    }

    /// touch.js: a short buzz confirms a one-handed flip armed (once per
    /// zone); lifting now flips. True when the caller should vibrate.
    pub fn flip_buzz(&mut self, now: f64) -> bool {
        if !self.input.one_handed {
            self.flip_buzzed = None;
            return false;
        }
        let (.., armed) = self.one_handed_state(now);
        let zone = self.input.pointer.as_ref().and_then(|p| p.flip);
        if !armed {
            self.flip_buzzed = None;
            return false;
        }
        if zone.is_some() && self.flip_buzzed != zone {
            self.flip_buzzed = zone;
            return true;
        }
        false
    }

    /// `oneHandedFeedback`: compass position and cue without sampling.
    fn one_handed_state(&self, now: f64) -> (f32, f32, String, bool) {
        let input = &self.input;
        let Some(p) = &input.pointer else {
            return (0., 0., String::new(), false);
        };
        let frame = self.last.clone().unwrap_or_default();
        let current = frame
            .applied_touch
            .is_some_and(|(g, held)| g == input.generation && held);
        let can_launch = input.flips
            && (frame.preload_armed || frame.preload_active || frame.flip_launch_window);
        let flip = (can_launch && p.flip_armed(now))
            .then_some(p.flip)
            .flatten();
        let steer = input::steering_axis(p.current_x - p.x, input.steering_sensitivity);
        let lean = ((p.current_y - p.y) / 70.).clamp(-1., 1.);
        let reverse = input
            .secondary
            .as_ref()
            .is_some_and(|s| s.pointer.is_some());
        let cue = if let Some(f) = flip {
            FLIPS[f].cue
        } else if reverse && !input.airborne {
            if current && frame.applied_brake > 0. {
                "BRAKE → REVERSE"
            } else {
                "REVERSE · BRACE"
            }
        } else if frame.preload_active {
            "PRELOADED"
        } else if frame.preload_armed {
            "LIFT FOR PRELOAD"
        } else if current && frame.applied_brake > 0. {
            "BRAKE · BRACE"
        } else {
            "DRIVE · BRACE"
        };
        (steer, lean, cue.into(), flip.is_some())
    }

    pub fn paint(&mut self, active: bool, now: f64, fonts: &mut Fonts, list: &mut DisplayList) {
        if !active {
            return;
        }
        let coach = self.rides <= COACH_RIDES;
        let one = self.input.one_handed;
        let size = if one { 96. } else { 72. };
        let unit = size / 96.;
        let held = self.held;
        // The red throttle arc eases toward the held drive axis (.16 s).
        let reverse = one
            && self
                .input
                .secondary
                .as_ref()
                .is_some_and(|s| s.pointer.is_some())
            && !self.input.airborne;
        let target = if reverse {
            1.
        } else {
            self.input
                .pointer
                .as_ref()
                .filter(|p| p.mode == Mode::Drive)
                .map_or(0., |p| p.axis.max(0.))
        };
        if target != self.throttle.1 {
            let shown = self.throttle_value(now);
            self.throttle = (shown, target, now);
        }
        let throttle = self.throttle_value(now);
        // Marker visibility, burst and wheel dimming.
        let wheel = self.wheel_open();
        let mut opacity = if held { 1. } else { 0. };
        let mut scale = 1.;
        if !held && self.preload == Preload::Active {
            if one {
                if let Some(since) = self.burst_since {
                    let t = (((now - since) / 400.) as f32).clamp(0., 1.);
                    opacity = 1. - t;
                    scale = 1. + 0.3 * t;
                }
            } else {
                opacity = 1.;
            }
        }
        if one && held && wheel {
            opacity = 0.3;
        }
        if opacity > 0. {
            self.paint_marker(
                now, fonts, list, size, unit, opacity, scale, throttle, reverse, coach,
            );
        }
        let hint = !self.rode_this_session && self.rides < COACH_RIDES;
        if self.driving(active) && hint && !held {
            self.paint_hint(now, fonts, list);
        }
        if wheel {
            self.paint_wheel(fonts, list, self.wheel_opens <= COACH_WHEEL);
        }
    }

    fn throttle_value(&self, now: f64) -> f32 {
        let (from, to, since) = self.throttle;
        let t = (((now - since) / 160.) as f32).clamp(0., 1.);
        // ease-out
        let e = 1. - (1. - t).powi(2);
        from + (to - from) * e
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_marker(
        &self,
        now: f64,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        size: f32,
        unit: f32,
        opacity: f32,
        scale: f32,
        throttle: f32,
        reverse: bool,
        coach: bool,
    ) {
        let one = self.input.one_handed;
        let [cx, cy] = self.marker;
        let s = size * scale;
        let rect = Rect::new(cx - s / 2., cy - s / 2., s, s);
        let k = unit * scale;
        let at = |x: f32, y: f32| [cx + (x - 48.) * k, cy + (y - 48.) * k];
        let accent = match self.preload {
            Preload::Armed => BLUE,
            Preload::Active => ORANGE,
            _ => GOLD,
        };
        list.opacity(opacity);
        // Recess: radial charcoal, blue when armed, orange when preloaded.
        let (centre, inner) = match self.preload {
            Preload::Armed => (Color::hex("#1f5fae66"), 0.5),
            Preload::Active => (Color::hex("#f4a13c8c"), 0.46),
            _ => (Color::hex("#1c1c1ca6"), 0.5),
        };
        let edge = if self.preload == Preload::Active {
            Color::hex("#5a2a04d9")
        } else {
            Color::hex("#080808d9")
        };
        let mut shadows = vec![BoxShadow::outer(0., 2., 8., Color::hex("#000a"))];
        match self.preload {
            Preload::Armed => {
                // touch-preload-pulse: .5 s ease-in-out alternate.
                let phase = ((now / 500.) % 2.) as f32;
                let t = if phase > 1. { 2. - phase } else { phase };
                let t = t * t * (3. - 2. * t);
                let blur = 4. + 16. * t;
                let alpha = (0x66 as f32 + (0xdd - 0x66) as f32 * t) / 255.;
                shadows.insert(0, BoxShadow::outer(0., 0., blur, BLUE.with_alpha(alpha)));
            }
            Preload::Active => {
                shadows.insert(0, BoxShadow::outer(0., 0., 18., Color::hex("#f4a13caa")))
            }
            _ => {}
        }
        list.push(Item::Box(BoxPaint {
            rect,
            radii: [s / 2.; 4],
            fill: Some(Fill::radial(&[
                (0., centre),
                (inner, centre),
                (0.72, edge),
                (0.92, Color::hex("#262626e6")),
            ])),
            shadows,
            ..Default::default()
        }));
        // touch-ring-press: the ring scales in from .82 over .14 s.
        let press = (((now - self.held_since) / 140.) as f32).clamp(0., 1.);
        let ring_scale = if self.held {
            0.82 + 0.18 * (1. - (1. - press).powi(2))
        } else {
            1.
        };
        let rk = k * ring_scale;
        let circle = |r: f32, width: f32| {
            let outer = (r + width / 2.) * rk;
            Rect::new(cx - outer, cy - outer, outer * 2., outer * 2.)
        };
        let ring = |r: f32, width: f32, color: Color| BoxPaint {
            rect: circle(r, width),
            radii: [(r + width / 2.) * rk; 4],
            border: [width * rk; 4],
            border_color: color,
            ..Default::default()
        };
        list.push(Item::Box(ring(46.5, 2., INK)));
        // Rim: the vertical metal gradient over the circle's bounding box.
        let remap = |t: f32| (t * 88. + 2.) / 92.;
        list.push(Item::Box(BoxPaint {
            fill: Some(Fill::Linear(Gradient::new(
                180.,
                &[
                    (remap(0.), Color::hex("#f4f4f4")),
                    (remap(0.45), Color::hex("#9a9a9a")),
                    (remap(0.55), Color::hex("#5c5c5c")),
                    (remap(1.), Color::hex("#c9c9c9")),
                ],
            ))),
            ring: true,
            ..ring(44., 4., Color::TRANSPARENT)
        }));
        // Throttle gauge: from eight o'clock over the top, 42% of the circle.
        let sweep = throttle * 0.42 * 360.;
        if sweep > 0.5 {
            let q = (sweep * 2.).round() / 2.;
            let (a0, a1) = (150f32.to_radians(), (150. + q).to_radians());
            let point = |a: f32| (48. + 44. * a.cos(), 48. + 44. * a.sin());
            let (x0, y0) = point(a0);
            let (x1, y1) = point(a1);
            list.push(Item::Shape {
                shape: Shape {
                    path: format!(
                        "M{:.3} {:.3}A44 44 0 {} 1 {:.3} {:.3}",
                        x0 - 48.,
                        y0 - 48.,
                        (q > 180.) as u8,
                        x1 - 48.,
                        y1 - 48.
                    ),
                    bounds: [-48., -48., 48., 48.],
                    unit: rk,
                    fill: Color::TRANSPARENT,
                    stroke: Some(Stroke {
                        width: 4. * rk,
                        color: if reverse { SILVER_LIGHT } else { RED_BRIGHT },
                        join: Join::Round,
                    }),
                    drop_shadow: None,
                },
                x: cx,
                y: cy,
                rotation: 0.,
            });
        }
        let (preload_color, width, alpha) = match self.preload {
            Preload::Idle => (BLUE, 3., 0.),
            Preload::Available => (BLUE, 3., 0.45),
            Preload::Armed => (BLUE, 4., 1.),
            Preload::Active => (ORANGE, 5., 1.),
        };
        if alpha > 0. {
            list.push(Item::Box(ring(38.5, width, preload_color.fade(alpha))));
        }
        // Children hidden after release (visibility: hidden on i, b, em).
        if !self.held {
            if one {
                self.paint_cue(fonts, list, rect, k, coach);
            } else {
                self.paint_cue(fonts, list, rect, k, false);
            }
            list.unopacity();
            return;
        }
        let (steer, lean, _, _) = self.one_handed_state(now);
        if one && coach {
            let line = Color::hex("#b9b9b938");
            list.rect(rect_at(at(47., 16.), 2. * k, 64. * k), line);
            list.rect(rect_at(at(16., 47.), 64. * k, 2. * k), line);
            let style = TextStyle::new(Font::display(9., 800, true), SILVER_LIGHT)
                .shadow(0., 1., 2., Color::BLACK)
                .shadow(0., 0., 2., Color::BLACK);
            let ascent = fonts.measure("", &style.font, 0.);
            let base = crate::layout::css_baseline(9., ascent.ascent, ascent.descent);
            let fwd = fonts.measure("FWD", &style.font, 0.).width;
            let back = fonts.measure("BACK", &style.font, 0.).width;
            let r = fonts.measure("R", &style.font, 0.).width;
            list.text(
                "FWD",
                &style,
                rect.x + (28. + 20.) * k - fwd / 2.,
                rect.y - 12. * k + base,
            );
            list.text(
                "BACK",
                &style,
                rect.x + (28. + 20.) * k - back / 2.,
                rect.y + (96. + 12. - 9.) * k + base,
            );
            list.text("L", &style, rect.x - 12. * k, rect.y + 44. * k + base);
            list.text(
                "R",
                &style,
                rect.right() + 12. * k - r,
                rect.y + 44. * k + base,
            );
        }
        if one {
            // Direction line from the centre toward the thumb.
            let length = steer.hypot(lean) * 34.;
            if length > 0.01 {
                let origin = at(48., 47.);
                list.push(Item::Box(BoxPaint {
                    rect: Rect::new(origin[0], origin[1], length * k, 2. * k),
                    fill: Some(accent.into()),
                    shadows: vec![BoxShadow::outer(0., 1., 2., Color::BLACK)],
                    rotation: lean.atan2(steer),
                    pivot: [origin[0], origin[1] + k],
                    ..Default::default()
                }));
            }
        }
        // Gold thumb ball.
        let ball = if one {
            at(38. + steer * 34., 38. + lean * 34.)
        } else {
            let drive = -self.input.pointer.as_ref().map_or(1., |p| p.axis) * 30.;
            at(26. * 96. / 72., 26. * 96. / 72. + drive / unit)
        };
        list.push(Item::Box(BoxPaint {
            rect: Rect::new(ball[0], ball[1], 20. * k, 20. * k),
            radii: [10. * k; 4],
            fill: Some(Fill::radial_at(
                [0.34, 0.3],
                &[
                    (0., Color::hex("#fff8bf")),
                    (0.32, Color::hex("#ead046")),
                    (0.62, Color::hex("#b49704")),
                    (1., Color::hex("#5e4d02")),
                ],
            )),
            border: [k; 4],
            border_color: GOLD_LIGHT,
            shadows: vec![
                BoxShadow::inset(0., 1., 1., Color::hex("#fff9")),
                BoxShadow::outer(0., 1., 4., Color::hex("#000d")),
            ],
            ..Default::default()
        }));
        self.paint_cue(fonts, list, rect, k, one && coach);
        list.unopacity();
    }

    fn paint_cue(
        &self,
        fonts: &mut Fonts,
        list: &mut DisplayList,
        rect: Rect,
        k: f32,
        coach: bool,
    ) {
        let one = self.input.one_handed;
        let (_, _, state_cue, armed) = self.one_handed_state(0.);
        let cue = if one {
            if !state_cue.is_empty() {
                state_cue
            } else if self.preload == Preload::Active {
                "PRELOADED".into()
            } else {
                String::new()
            }
        } else {
            self.label.clone()
        };
        if cue.is_empty() {
            return;
        }
        let size = if armed { 11. } else { 10. };
        let style = TextStyle::new(
            Font::display(size, 800, true),
            if armed { GOLD_LIGHT } else { Color::WHITE },
        )
        .spacing(size * 0.045)
        .stroke(2., Color::BLACK, Join::Miter)
        .shadow(0., 1., 2., Color::BLACK);
        let (left, top) = if one {
            (-24., if coach { -34. } else { -24. })
        } else {
            (-36., -22.)
        };
        let m = fonts.measure(&cue, &style.font, style.letter_spacing);
        let line = size * 1.2;
        let baseline = 1. + crate::layout::css_baseline(line, m.ascent, m.descent);
        let x = rect.x + left * k + (144. - m.width) / 2.;
        list.text(cue, &style, x, rect.y + top * k + baseline);
    }

    fn paint_hint(&self, now: f64, fonts: &mut Fonts, list: &mut DisplayList) {
        let [width, height] = self.viewport;
        let one = self.input.one_handed;
        let cx = width * if one { 0.5 } else { 0.75 };
        let top = height * 0.68;
        // touch-hint-pulse: 1.4 s ease-in-out between .9/.7 and 1.05/1.
        let phase = ((now % 1400.) / 1400.) as f32;
        let t = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
        let scale = 0.9 + 0.15 * t;
        let alpha = 0.7 + 0.3 * t;
        let r = 32. * scale;
        let c = [cx, top + 32.];
        list.opacity(alpha);
        let rect = Rect::new(c[0] - r, c[1] - r, r * 2., r * 2.);
        let far = 29. * std::f32::consts::SQRT_2;
        list.push(Item::Box(BoxPaint {
            rect,
            radii: [r; 4],
            fill: Some(Fill::radial(&[
                (0., GOLD_LIGHT),
                (9. / far, GOLD_LIGHT),
                (10. / far, Color::hex("#e5d36066")),
                (12. / far, Color::TRANSPARENT),
            ])),
            border: [3. * scale; 4],
            border_color: Color::hex("#e9e9e9aa"),
            shadows: vec![
                BoxShadow {
                    spread: 1. * scale,
                    ..BoxShadow::outer(0., 0., 0., Color::hex("#0008"))
                },
                BoxShadow {
                    spread: 1. * scale,
                    ..BoxShadow::inset(0., 0., 0., Color::hex("#0008"))
                },
            ],
            ..Default::default()
        }));
        list.unopacity();
        let outlined = |size: f32, weight: u16, color: Color| {
            TextStyle::new(Font::display(size, weight, true), color)
                .spacing(size * 0.045)
                .stroke(2., Color::BLACK, Join::Miter)
                .shadow(0., 1., 2., Color::BLACK)
        };
        let (title, detail) = if one {
            (
                "HOLD ANYWHERE TO RIDE",
                "DRAG TO STEER & LEAN · LIFT ON RAMPS TO JUMP",
            )
        } else {
            (
                "HOLD RIGHT SIDE TO RIDE",
                "LEFT THUMB STEERS & LEANS · LIFT ON RAMPS TO JUMP",
            )
        };
        // Centred lines in a 280px (200px two-finger) block under the ring.
        let top = top + 74.;
        centered(
            fonts,
            list,
            title,
            &outlined(16., 800, Color::WHITE),
            cx,
            top,
            23.2,
            if one { 280. } else { 200. },
        );
        centered(
            fonts,
            list,
            detail,
            &outlined(10., 700, SILVER_LIGHT),
            cx,
            top + 23.2 + 4.,
            14.5,
            if one { 280. } else { 200. },
        );
    }

    fn paint_wheel(&self, fonts: &mut Fonts, list: &mut DisplayList, coach: bool) {
        let Some(p) = self
            .input
            .secondary
            .as_ref()
            .and_then(|s| s.pointer.as_ref())
        else {
            return;
        };
        let (cx, cy) = (p.x, p.y);
        let selected = match p.selection {
            Some(input::Selection::Wheel(i)) => Some(i),
            _ => None,
        };
        for (i, t) in TRICK_WHEEL.iter().enumerate() {
            let (inner, outer, gap) = if t.outer {
                (OUTER_DISTANCE + 2., WHEEL_RADIUS, 0.025)
            } else {
                (HUB, INNER_EDGE, 0.035)
            };
            let chosen = selected == Some(i);
            let (fill, stroke, width) = if chosen {
                (GOLD.fade(0.9), GOLD_LIGHT, 2.)
            } else if t.outer {
                (Color::hex("#210504b3"), RED_BRIGHT, 1.)
            } else {
                (Color::hex("#08080894"), Color::hex("#b9b9b955"), 1.)
            };
            list.push(Item::Shape {
                shape: Shape {
                    path: sector_path(t.angle, t.span, inner, outer, gap),
                    bounds: sector_bounds(t.angle, t.span, inner, outer, gap),
                    unit: 1.,
                    fill,
                    stroke: Some(Stroke {
                        width,
                        color: stroke,
                        join: Join::Miter,
                    }),
                    drop_shadow: None,
                },
                x: cx,
                y: cy,
                rotation: 0.,
            });
        }
        let hub = HUB - 2.;
        list.push(Item::Box(BoxPaint {
            rect: Rect::new(
                cx - hub - 0.75,
                cy - hub - 0.75,
                (hub + 0.75) * 2.,
                (hub + 0.75) * 2.,
            ),
            radii: [hub + 0.75; 4],
            fill: Some(Color::hex("#080808c4").into()),
            border: [1.5; 4],
            border_color: Color::hex("#b9b9b980"),
            ..Default::default()
        }));
        let outlined = |size: f32, color: Color| {
            TextStyle::new(Font::display(size, 800, true), color)
                .stroke(2., Color::BLACK, Join::Miter)
                .shadow(0., 1., 2., Color::BLACK)
        };
        for (i, t) in TRICK_WHEEL.iter().enumerate() {
            let r = if t.outer { OUTER_LABEL } else { INNER_LABEL };
            let size =
                if t.outer { 10. } else { 11. } * if selected == Some(i) { 1.15 } else { 1. };
            centered(
                fonts,
                list,
                t.label,
                &outlined(size, Color::WHITE),
                cx + t.angle.cos() * r,
                cy + t.angle.sin() * r - size * 1.1 / 2.,
                size * 1.1,
                0.,
            );
        }
        if let Some(i) = selected {
            centered(
                fonts,
                list,
                &TRICK_WHEEL[i].name.to_uppercase(),
                &outlined(19., GOLD_LIGHT).spacing(0.38),
                cx,
                cy - WHEEL_RADIUS - 26.,
                20.9,
                312.,
            );
        } else if coach {
            centered(
                fonts,
                list,
                "TRICKS",
                &outlined(10., Color::WHITE),
                cx,
                cy - WHEEL_RADIUS + 150.,
                12.,
                60.,
            );
        }
        if coach {
            let cue = if selected.is_some() {
                "LIFT TO PERFORM · CENTER CANCELS"
            } else {
                "DRAG TO A TRICK · LIFT TO PERFORM"
            };
            centered(
                fonts,
                list,
                cue,
                &outlined(9., Color::WHITE),
                cx,
                cy - WHEEL_RADIUS + 314.,
                10.8,
                312.,
            );
        }
    }
}

/// Lines centred on `x` from `top` (CSS `text-align: center`), wrapped to `width`.
#[allow(clippy::too_many_arguments)]
fn centered(
    fonts: &mut Fonts,
    list: &mut DisplayList,
    text: &str,
    style: &TextStyle,
    x: f32,
    top: f32,
    line_height: f32,
    width: f32,
) {
    let lines = if width > 0. {
        fonts.wrap(text, &style.font, style.letter_spacing, width)
    } else {
        vec![text.to_string()]
    };
    let m = fonts.measure("", &style.font, 0.);
    let baseline = crate::layout::css_baseline(line_height, m.ascent, m.descent);
    for (i, line) in lines.iter().enumerate() {
        let w = fonts.measure(line, &style.font, style.letter_spacing).width;
        list.text(
            line,
            style,
            x - w / 2.,
            top + i as f32 * line_height + baseline,
        );
    }
}

fn rect_at(at: [f32; 2], w: f32, h: f32) -> Rect {
    Rect::new(at[0], at[1], w, h)
}

/// Tight annular-sector bounds keep the ten wheel sectors inside the text
/// atlas at high DPR. Full-wheel bounds per sector wasted most cache texels.
fn sector_bounds(angle: f32, span: f32, inner: f32, outer: f32, gap: f32) -> [f32; 4] {
    let (a0, a1) = (angle - span + gap, angle + span - gap);
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut include = |a: f32| {
        for r in [inner, outer] {
            let (x, y) = (r * a.cos(), r * a.sin());
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
        }
    };
    include(a0);
    include(a1);
    let quarter = std::f32::consts::FRAC_PI_2;
    for i in (a0 / quarter).ceil() as i32..=(a1 / quarter).floor() as i32 {
        include(i as f32 * quarter);
    }
    bounds
}

/// touch.js `sectorPath` around the wheel centre.
fn sector_path(angle: f32, span: f32, inner: f32, outer: f32, gap: f32) -> String {
    let (a0, a1) = (angle - span + gap, angle + span - gap);
    let at = |r: f32, a: f32| format!("{:.2} {:.2}", r * a.cos(), r * a.sin());
    format!(
        "M{}L{}A{outer} {outer} 0 0 1 {}L{}A{inner} {inner} 0 0 0 {}Z",
        at(inner, a0),
        at(outer, a0),
        at(outer, a1),
        at(inner, a1),
        at(inner, a0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coaching_counts_and_one_handed_default_load_from_settings() {
        let mut settings = Settings::memory();
        let mut touch = TouchControls::new(&settings, true);
        assert!(touch.one_handed());
        touch.set_viewport(390., 844.);
        assert!(touch.down(1, 195., 500., 0., true, &mut settings));
        assert_eq!(settings.get("atv-touch-coach-rides").as_deref(), Some("1"));
        let c = touch.sample(16., true, &mut settings);
        assert!(c.sample.drive.held);
        // The next ride's controls count no second ride this page load.
        let mut next = TouchControls::new(&settings, true);
        next.set_viewport(390., 844.);
        assert!(next.down(2, 195., 500., 0., true, &mut settings));
        assert_eq!(settings.get("atv-touch-coach-rides").as_deref(), Some("1"));
        settings.set("atv-mobile-one-handed", "false");
        let mut two = TouchControls::new(&settings, true);
        two.set_viewport(390., 844.);
        assert!(!two.one_handed());
        // The left half belongs to the lean thumb.
        assert!(two.down(5, 50., 500., 0., true, &mut settings));
        two.moved(5, 85., 500., 5.);
        assert!(two.sample(10., true, &mut settings).steer > 0.);
    }

    #[test]
    fn an_armed_one_handed_flip_buzzes_once_per_zone() {
        let mut settings = Settings::memory();
        let mut touch = TouchControls::new(&settings, true);
        touch.set_viewport(390., 844.);
        touch.input.flips = true;
        touch.feedback(
            Feedback {
                preload_armed: true,
                ..Default::default()
            },
            0.,
        );
        assert!(touch.down(1, 195., 400., 0., true, &mut settings));
        assert!(!touch.flip_buzz(10.));
        // Pull down into the backflip zone; it arms after the dwell.
        touch.moved(1, 195., 530., 20.);
        assert!(!touch.flip_buzz(100.), "not armed before the dwell");
        assert!(touch.flip_buzz(300.));
        assert!(!touch.flip_buzz(320.), "one buzz per zone");
        // Back to the centre disarms; re-entering buzzes again.
        touch.moved(1, 195., 400., 340.);
        assert!(!touch.flip_buzz(350.));
        touch.moved(1, 195., 530., 360.);
        assert!(touch.flip_buzz(700.));
    }
}
