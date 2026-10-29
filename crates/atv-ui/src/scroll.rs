//! Kinetic scrolling for menu lists and panels, as native iOS/Android
//! scroll views behave (authored; the original menus had no touch input):
//!
//! - The content follows the finger from the moment the touch slop is
//!   crossed, without jumping by the slop distance.
//! - Releasing a moving finger throws the content: it keeps the finger's
//!   velocity and decelerates exponentially (UIScrollView's normal rate,
//!   0.998 per ms).
//! - Past either end the content stretches with resistance (rubber band)
//!   while dragged, and a fling that reaches an end overshoots slightly;
//!   both spring back with a critically damped spring.
//! - A touch during a fling catches it (the content stops under the finger)
//!   and that touch is not a tap.
//! - Keyboard and controller focus glides to a row with a short ease-out
//!   instead of jumping; wheels and trackpads move the content directly.
//!
//! All positions are CSS pixels; `offset` is how far the content has moved
//! up (0 at the top, `max` at the bottom). Times are milliseconds from any
//! monotonic clock ([`now`] in the app).

/// Movement (CSS px) before a press becomes a drag.
pub const SLOP: f32 = 6.;
/// Velocity decay per millisecond while flung (UIScrollView normal).
pub const DECELERATION: f32 = 0.998;
/// Slowest fling worth animating (px/ms, 30 px/s).
const MIN_FLING: f32 = 0.03;
/// Fastest fling (px/ms): a flick never travels more than a few screens.
const MAX_FLING: f32 = 8.;
/// Samples older than this at release do not count towards its velocity;
/// a finger held still that long before lifting does not fling.
const VELOCITY_WINDOW: f64 = 100.;
const HOLD_STILL: f64 = 60.;
/// Samples kept: a janky frame can leave the window with one sample, so one
/// older sample (up to this age) still gives the release a velocity.
const KEEP_SAMPLES: f64 = 250.;
/// Spring stiffness (critically damped, rad/ms): settles in ~400 ms.
const SPRING: f32 = 0.013;
/// Rubber-band resistance (UIScrollView's 0.55).
const RUBBER: f32 = 0.55;
/// The indicator stays this long after the content rests, then fades (ms).
const INDICATOR_HOLD: f64 = 500.;
const INDICATOR_FADE: f64 = 300.;
/// Keyboard/controller glide duration (ms).
pub const GLIDE_MS: f64 = 180.;

/// The current time in ms for scroll physics.
pub fn now() -> f64 {
    thread_local! {
        static START: web_time::Instant = web_time::Instant::now();
    }
    START.with(|start| start.elapsed().as_secs_f64() * 1000.)
}

/// Stretch for `overshoot` px past an end of a viewport `dimension` high.
fn rubber(overshoot: f32, dimension: f32) -> f32 {
    let d = dimension.max(1.);
    (1. - 1. / (overshoot * RUBBER / d + 1.)) * d
}

/// The finger distance that stretches the content by `stretch`.
fn unrubber(stretch: f32, dimension: f32) -> f32 {
    let d = dimension.max(1.);
    let r = (stretch / d).min(0.999);
    d / RUBBER * (1. / (1. - r) - 1.)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Motion {
    Rest,
    /// Thrown at `velocity` px/ms.
    Fling {
        velocity: f32,
    },
    /// Returning to `target` from `from` with `velocity`, started at `start`.
    Spring {
        target: f32,
        from: f32,
        velocity: f32,
        start: Option<f64>,
    },
    /// Keyboard/controller reveal: ease-out from `from` to `to`.
    Glide {
        from: f32,
        to: f32,
        start: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Drag {
    /// Where the press began.
    start: f32,
    /// Crossed the slop: the content follows the finger.
    tracking: bool,
    /// The finger position and unstretched offset the content follows from.
    anchor: f32,
    anchor_offset: f32,
    /// Recent (time, unstretched offset) samples for the release velocity.
    samples: Vec<(f64, f32)>,
}

/// One scrollable region's position and motion.
#[derive(Clone, Debug, PartialEq)]
pub struct Scroller {
    offset: f32,
    max: f32,
    /// The visible height, for the rubber band's resistance.
    viewport: f32,
    motion: Motion,
    drag: Option<Drag>,
    /// The last tick (ms).
    clock: Option<f64>,
    /// When the content last moved (ms), for the scroll indicator.
    moved_at: Option<f64>,
}

impl Default for Scroller {
    fn default() -> Self {
        Self {
            offset: 0.,
            max: 0.,
            viewport: 400.,
            motion: Motion::Rest,
            drag: None,
            clock: None,
            moved_at: None,
        }
    }
}

impl Scroller {
    /// The content offset to draw (may lie outside 0..=max while stretched).
    pub fn offset(&self) -> f32 {
        self.offset
    }

    pub fn max(&self) -> f32 {
        self.max
    }

    /// The visible height last laid out.
    pub fn viewport(&self) -> f32 {
        self.viewport
    }

    /// Something is moving: frames must keep coming.
    pub fn active(&self) -> bool {
        self.motion != Motion::Rest
    }

    /// A finger has crossed the slop and the content follows it.
    pub fn dragging(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.tracking)
    }

    /// The scroll range and visible height from the latest layout. At rest,
    /// an offset past the new end moves back inside it.
    pub fn set_bounds(&mut self, max: f32, viewport: f32) {
        self.max = max.max(0.);
        self.viewport = viewport.max(1.);
        if self.drag.is_none() && self.motion == Motion::Rest {
            self.offset = self.offset.clamp(0., self.max);
        }
    }

    /// Back to the top at once (a new page).
    pub fn reset(&mut self) {
        *self = Self {
            max: self.max,
            viewport: self.viewport,
            ..Self::default()
        };
    }

    /// Moves to `offset` at once (clamped), ending any motion.
    pub fn jump_to(&mut self, offset: f32) {
        self.motion = Motion::Rest;
        self.offset = offset.clamp(0., self.max);
    }

    /// Glides to `offset` (keyboard/controller focus), or jumps when
    /// `animate` is false (reduced motion).
    pub fn glide_to(&mut self, offset: f32, animate: bool) {
        let to = offset.clamp(0., self.max);
        if self.drag.is_some() {
            return;
        }
        if !animate {
            self.jump_to(to);
            return;
        }
        let current = match self.motion {
            Motion::Glide { to: t, .. } => t,
            _ => self.offset,
        };
        if (to - current).abs() < 0.5 && (to - self.offset).abs() < 0.5 {
            return;
        }
        self.motion = Motion::Glide {
            from: self.offset,
            to,
            start: None,
        };
    }

    /// Where the content will rest (a glide's target, else the offset).
    pub fn target(&self) -> f32 {
        match self.motion {
            Motion::Glide { to, .. } => to,
            Motion::Spring { target, .. } => target,
            _ => self.offset,
        }
    }

    /// Mouse wheel / trackpad: the content moves directly (trackpads send
    /// their own momentum).
    pub fn wheel(&mut self, dy: f32) {
        let base = self.target().clamp(0., self.max);
        self.motion = Motion::Rest;
        self.offset = (base + dy).clamp(0., self.max);
        self.moved_at = Some(now());
    }

    /// A press at `y`. Returns true when it caught moving content: that
    /// press is a grab, not a tap.
    pub fn press(&mut self, y: f32, now: f64) -> bool {
        let caught = match self.motion {
            // Any visibly moving content (from 20 px/s) is caught, as natively.
            Motion::Fling { velocity } => velocity.abs() > 0.02,
            Motion::Spring { .. } => true,
            _ => false,
        };
        self.motion = Motion::Rest;
        let unstretched = self.unstretched();
        self.drag = Some(Drag {
            start: y,
            tracking: caught,
            anchor: y,
            anchor_offset: unstretched,
            samples: vec![(now, unstretched)],
        });
        caught
    }

    /// The finger moved to `y`. Returns true while the content follows it.
    pub fn drag(&mut self, y: f32, now: f64) -> bool {
        let (max, viewport) = (self.max, self.viewport);
        let Some(drag) = &mut self.drag else {
            return false;
        };
        if !drag.tracking {
            if (y - drag.start).abs() <= SLOP {
                return false;
            }
            // Follow from here: no jump by the slop distance.
            drag.tracking = true;
            drag.anchor = y;
        }
        let raw = drag.anchor_offset - (y - drag.anchor);
        drag.samples.push((now, raw));
        drag.samples.retain(|(t, _)| now - t <= KEEP_SAMPLES);
        self.offset = if raw < 0. {
            -rubber(-raw, viewport)
        } else if raw > max {
            max + rubber(raw - max, viewport)
        } else {
            raw
        };
        self.moved_at = Some(now);
        true
    }

    /// The finger lifted at `now`: fling, spring back, or rest.
    pub fn release(&mut self, now: f64) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.clock = None;
        let velocity = if drag.tracking {
            Self::velocity(&drag.samples, now)
        } else {
            0.
        };
        if self.offset < 0. || self.offset > self.max {
            self.spring(velocity);
        } else if velocity.abs() >= MIN_FLING {
            self.motion = Motion::Fling { velocity };
        } else {
            self.motion = Motion::Rest;
        }
    }

    /// The press ended without a release (cancelled, focus lost): no
    /// fling, but stretched content still springs back.
    pub fn cancel(&mut self) {
        if self.drag.take().is_some() && (self.offset < 0. || self.offset > self.max) {
            self.clock = None;
            self.spring(0.);
        }
    }

    /// Velocity (offset px/ms) over the recent samples.
    fn velocity(samples: &[(f64, f32)], now: f64) -> f32 {
        let Some(&(last_t, last)) = samples.last() else {
            return 0.;
        };
        if now - last_t > HOLD_STILL {
            return 0.;
        }
        // The oldest sample in the window, or the one just before it.
        let inside = samples
            .iter()
            .position(|(t, _)| last_t - t <= VELOCITY_WINDOW)
            .unwrap_or(samples.len() - 1);
        let from = if inside + 1 >= samples.len() {
            inside.saturating_sub(1)
        } else {
            inside
        };
        let (first_t, first) = samples[from];
        let dt = (last_t - first_t) as f32;
        if dt < 8. {
            return 0.;
        }
        ((last - first) / dt).clamp(-MAX_FLING, MAX_FLING)
    }

    fn unstretched(&self) -> f32 {
        if self.offset < 0. {
            -unrubber(-self.offset, self.viewport)
        } else if self.offset > self.max {
            self.max + unrubber(self.offset - self.max, self.viewport)
        } else {
            self.offset
        }
    }

    fn spring(&mut self, velocity: f32) {
        let target = self.offset.clamp(0., self.max);
        self.motion = Motion::Spring {
            target,
            from: self.offset,
            velocity,
            start: None,
        };
    }

    /// The scroll indicator's opacity at `now`: shown while the content
    /// moves or is held, then fading out (UIScrollView's thin thumb).
    pub fn indicator_alpha(&self, now: f64) -> f32 {
        if self.max <= 0. {
            return 0.;
        }
        if self.active() || self.dragging() {
            return 1.;
        }
        let Some(at) = self.moved_at else {
            return 0.;
        };
        let since = now - at;
        (1. - ((since - INDICATOR_HOLD) / INDICATOR_FADE) as f32).clamp(0., 1.)
    }

    /// Paints the indicator thumb inside `pane`'s right edge.
    pub fn paint_indicator(
        &self,
        list: &mut crate::paint::DisplayList,
        pane: crate::Rect,
        now: f64,
    ) {
        let alpha = self.indicator_alpha(now);
        if alpha <= 0. || pane.h < 40. {
            return;
        }
        let track = pane.h - 8.;
        let content = pane.h + self.max;
        // Past an end the thumb shortens, as it does natively.
        let over = if self.offset < 0. {
            -self.offset
        } else {
            (self.offset - self.max).max(0.)
        };
        let length = (track * pane.h / content - over).clamp(18., track);
        let t = (self.offset / self.max).clamp(0., 1.);
        let y = pane.y + 4. + (track - length) * t;
        list.push(crate::paint::Item::Box(crate::paint::BoxPaint {
            rect: crate::Rect::new(pane.right() - 6., y, 3., length),
            fill: Some(crate::Color::hex("#ffffff").with_alpha(0.55 * alpha).into()),
            radii: [1.5; 4],
            ..Default::default()
        }));
    }

    /// The scroll cue: while more content lies past an edge, that edge of
    /// `pane` fades to shadow, and a small gold chevron at the bottom says
    /// the list goes on (Safari's toolbars can leave half a menu below the
    /// fold with nothing to show it).
    pub fn paint_edges(&self, list: &mut crate::paint::DisplayList, pane: crate::Rect) {
        if self.max <= 1. || pane.h < 80. {
            return;
        }
        let fade = |list: &mut crate::paint::DisplayList, y: f32, down: bool| {
            let (from, to) = (
                crate::Color::hex("#00000000"),
                crate::Color::hex("#000000a0"),
            );
            let stops = if down {
                [(0., from), (1., to)]
            } else {
                [(0., to), (1., from)]
            };
            list.push(crate::paint::Item::Box(crate::paint::BoxPaint {
                rect: crate::Rect::new(pane.x, y, pane.w, 28.),
                fill: Some(crate::color::Gradient::new(180., &stops).into()),
                ..Default::default()
            }));
        };
        if self.offset > 1. {
            fade(list, pane.y, false);
        }
        if self.offset < self.max - 1. {
            fade(list, pane.bottom() - 28., true);
            // A down triangle in the rows' arrow style (20-unit box at 0.8).
            list.push(crate::paint::Item::Shape {
                shape: crate::text::Shape {
                    path: "M3 7h14l-7 8z".into(),
                    bounds: [0., 0., 20., 20.],
                    unit: 0.8,
                    fill: crate::Color::hex("#f1d84e"),
                    stroke: None,
                    drop_shadow: Some(crate::text::Shadow {
                        dx: 0.,
                        dy: 1.,
                        blur: 2.,
                        color: crate::Color::hex("#000a"),
                    }),
                },
                x: pane.x + pane.w / 2. - 8.,
                y: pane.bottom() - 17.,
                rotation: 0.,
            });
        }
    }

    /// Advances the motion to `now`. Returns true while it moves.
    pub fn tick(&mut self, now: f64) -> bool {
        let dt = self.clock.map_or(0., |c| (now - c).clamp(0., 50.)) as f32;
        self.clock = Some(now);
        if self.motion != Motion::Rest {
            self.moved_at = Some(now);
        }
        match self.motion {
            Motion::Rest => {}
            Motion::Fling { velocity } => {
                // Exact exponential decay over dt.
                let decay = DECELERATION.powf(dt);
                let ln = DECELERATION.ln();
                self.offset += velocity * (decay - 1.) / ln;
                let velocity = velocity * decay;
                if self.offset < 0. || self.offset > self.max {
                    // Hit an end: overshoot carries into the spring, damped
                    // so a fast throw bounces a little, not a screen.
                    let edge = self.offset.clamp(0., self.max);
                    let over = self.offset - edge;
                    self.offset = edge + over.signum() * rubber(over.abs(), self.viewport);
                    self.spring(velocity * 0.5);
                } else if velocity.abs() < MIN_FLING / 3. {
                    self.motion = Motion::Rest;
                } else {
                    self.motion = Motion::Fling { velocity };
                }
            }
            Motion::Spring {
                target,
                from,
                velocity,
                start,
            } => {
                let start = start.unwrap_or(now);
                let t = (now - start) as f32;
                // Critically damped: x(t) = T + (A + B t) e^{-wt}.
                let a = from - target;
                let b = velocity + SPRING * a;
                let e = (-SPRING * t).exp();
                self.offset = target + (a + b * t) * e;
                let speed = (b - SPRING * (a + b * t)) * e;
                if (self.offset - target).abs() < 0.25 && speed.abs() < 0.01 {
                    self.offset = target;
                    self.motion = Motion::Rest;
                } else {
                    self.motion = Motion::Spring {
                        target,
                        from,
                        velocity,
                        start: Some(start),
                    };
                }
            }
            Motion::Glide { from, to, start } => {
                let start = start.unwrap_or(now);
                let p = (((now - start) / GLIDE_MS).clamp(0., 1.)) as f32;
                // ease-out cubic
                let eased = 1. - (1. - p).powi(3);
                self.offset = from + (to - from) * eased;
                self.motion = if p >= 1. {
                    self.offset = to;
                    Motion::Rest
                } else {
                    Motion::Glide {
                        from,
                        to,
                        start: Some(start),
                    }
                };
            }
        }
        self.active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded(max: f32) -> Scroller {
        let mut s = Scroller::default();
        s.set_bounds(max, 400.);
        s
    }

    /// Drags from y0 to y1 over `ms` in 8 ms steps; returns the end time.
    fn swipe(s: &mut Scroller, y0: f32, y1: f32, start: f64, ms: f64) -> f64 {
        s.press(y0, start);
        let steps = (ms / 8.) as i32;
        for i in 1..=steps {
            let t = start + f64::from(i) * 8.;
            s.drag(y0 + (y1 - y0) * i as f32 / steps as f32, t);
        }
        start + f64::from(steps) * 8.
    }

    fn settle(s: &mut Scroller, mut now: f64) -> (f64, Vec<f32>) {
        let mut path = vec![s.offset()];
        s.tick(now);
        while s.active() {
            now += 16.;
            s.tick(now);
            path.push(s.offset());
            assert!(now < 20_000., "never settles");
        }
        (now, path)
    }

    #[test]
    fn follows_the_finger_without_jumping_by_the_slop() {
        let mut s = bounded(1000.);
        s.press(500., 0.);
        assert!(!s.drag(497., 8.), "inside the slop");
        assert_eq!(s.offset(), 0.);
        assert!(s.drag(490., 16.));
        // Tracking starts where the slop was crossed.
        assert_eq!(s.offset(), 0.);
        s.drag(440., 24.);
        assert_eq!(s.offset(), 50.);
        assert!(s.dragging());
    }

    #[test]
    fn a_flick_keeps_moving_after_release_then_settles() {
        let mut s = bounded(5000.);
        // 300 px up in 120 ms: 2.5 px/ms.
        let end = swipe(&mut s, 700., 400., 0., 120.);
        let released = s.offset();
        s.release(end);
        assert!(s.active(), "flung");
        let (_, path) = settle(&mut s, end);
        let rest = *path.last().unwrap();
        assert!(
            rest > released + 500.,
            "momentum carried it on: {released} -> {rest}"
        );
        // Monotonic and decelerating.
        for w in path.windows(3) {
            assert!(w[1] >= w[0]);
            assert!(w[2] - w[1] <= w[1] - w[0] + 0.01);
        }
    }

    #[test]
    fn a_slow_drag_or_a_pause_before_lifting_does_not_fling() {
        let mut s = bounded(5000.);
        let end = swipe(&mut s, 700., 400., 0., 120.);
        // Held still for 100 ms before lifting.
        s.release(end + 100.);
        assert!(!s.active());
        let mut s = bounded(5000.);
        let end = swipe(&mut s, 700., 690., 0., 2000.);
        s.release(end);
        assert!(!s.active());
    }

    #[test]
    fn a_janky_drag_still_flings() {
        // Moves 120 ms apart (stalled frames): the release still has the
        // finger's velocity from the previous sample.
        let mut s = bounded(5000.);
        s.press(700., 0.);
        s.drag(600., 120.);
        s.drag(450., 240.);
        s.release(250.);
        assert!(s.active(), "flung");
    }

    #[test]
    fn stretches_past_the_ends_and_springs_back() {
        let mut s = bounded(300.);
        let end = swipe(&mut s, 300., 500., 0., 400.);
        // Dragged 200 px down at the top: stretched less than that.
        assert!(s.offset() < 0. && s.offset() > -200., "{}", s.offset());
        s.release(end + 100.);
        let (t, path) = settle(&mut s, end + 100.);
        assert_eq!(*path.last().unwrap(), 0.);
        assert!(t - end < 1000., "springs back in {} ms", t - end);
        // A fling into the bottom overshoots a little and returns.
        let mut s = bounded(300.);
        let end = swipe(&mut s, 700., 450., 0., 80.);
        s.release(end);
        let (_, path) = settle(&mut s, end);
        let peak = path.iter().cloned().fold(0., f32::max);
        assert!(peak > 300. && peak < 300. + 400., "overshoot {peak}");
        assert_eq!(*path.last().unwrap(), 300.);
    }

    #[test]
    fn a_touch_catches_a_fling_and_is_not_a_tap() {
        let mut s = bounded(5000.);
        let end = swipe(&mut s, 700., 300., 0., 100.);
        s.release(end);
        s.tick(end);
        s.tick(end + 48.);
        assert!(s.press(500., end + 48.), "caught");
        let held = s.offset();
        s.tick(end + 200.);
        assert_eq!(s.offset(), held, "stopped under the finger");
        // The catching finger drags at once (no slop).
        assert!(s.drag(498., end + 210.));
        s.release(end + 400.);
        // A plain press on resting content is not a catch.
        assert!(!s.press(100., end + 500.));
    }

    #[test]
    fn grabbing_stretched_content_continues_from_the_stretch() {
        let mut s = bounded(300.);
        let end = swipe(&mut s, 300., 500., 0., 300.);
        let stretched = s.offset();
        s.release(end + 100.);
        s.tick(end + 100.);
        s.tick(end + 116.);
        let springing = s.offset();
        assert!(springing > stretched && springing < 0.);
        assert!(s.press(400., end + 116.));
        s.drag(400.5, end + 124.);
        assert!(
            (s.offset() - springing).abs() < 1.,
            "{} vs {springing}",
            s.offset()
        );
    }

    #[test]
    fn the_indicator_shows_while_moving_then_fades() {
        let mut s = bounded(1000.);
        assert_eq!(s.indicator_alpha(0.), 0., "hidden until it scrolls");
        let end = swipe(&mut s, 700., 400., 0., 120.);
        assert_eq!(s.indicator_alpha(end), 1.);
        s.release(end);
        let (rest, _) = settle(&mut s, end);
        assert_eq!(s.indicator_alpha(rest + 100.), 1., "held briefly");
        assert!(s.indicator_alpha(rest + 650.) < 1.);
        assert_eq!(s.indicator_alpha(rest + 900.), 0.);
        let mut list = crate::paint::DisplayList::default();
        s.paint_indicator(&mut list, crate::Rect::new(0., 100., 300., 400.), end);
        match &list.items[..] {
            [crate::paint::Item::Box(b)] => {
                assert!(b.rect.x > 290. && b.rect.w == 3.);
                assert!(b.rect.h >= 18. && b.rect.h < 400.);
            }
            other => panic!("{other:?}"),
        }
        // Nothing to scroll: never shown.
        let mut flat = bounded(0.);
        flat.wheel(10.);
        assert_eq!(flat.indicator_alpha(0.), 0.);
    }

    #[test]
    fn glides_wheels_and_bounds() {
        let mut s = bounded(1000.);
        s.glide_to(300., true);
        let (t, path) = settle(&mut s, 0.);
        assert_eq!(*path.last().unwrap(), 300.);
        assert!(t <= GLIDE_MS + 32.);
        s.glide_to(5000., false);
        assert_eq!(s.offset(), 1000.);
        s.wheel(-40.);
        assert_eq!(s.offset(), 960.);
        s.set_bounds(500., 400.);
        assert_eq!(s.offset(), 500.);
        s.reset();
        assert_eq!(s.offset(), 0.);
        assert!(!s.active());
    }
}
