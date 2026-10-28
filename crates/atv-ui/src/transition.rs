//! Menu page transitions: a quick, direction-aware cross-slide.
//!
//! A shared-axis transition: opening a page (a push) moves everything to
//! the left, the old page sliding out as it fades and the new one sliding in
//! from the right; going back (a pop) mirrors it, so the page you return to
//! comes in from the left. The red backdrop (and in landscape the silver
//! stage frame, shared by every frontend page as in the original) stays put;
//! only page content moves. The old page fades out over the first 90 ms
//! (`ease-in`, 28px), drawn under the new one from a snapshot of its last
//! frame; the new page slides 36px over 240 ms (`cubic-bezier(.2,0,0,1)`)
//! and fades in from 80 ms, so the two pages' different layouts never show
//! on top of each other.
//!
//! Direction comes from the pages seen: returning to a page already on the
//! way here is a pop, anything else a push. Navigation code is untouched.
//! A page shown mid-transition starts a new one from what is on screen
//! (interruptible); input always goes to the new page at once; a resize
//! cancels both halves; under `prefers-reduced-motion: reduce` nothing
//! moves. Exact-build evidence: the original page layouts animate only
//! their `Animated_Icon` widgets (the 21-frame ATM selection pill) and no
//! page transition was recovered, so this is an authored treatment in the
//! original's sweep-from-the-right direction.

use crate::paint::{DisplayList, Item};

/// Entry duration (ms) and start offset (CSS px, a push; a pop mirrors it).
pub const ENTER_MS: f64 = 240.;
pub const ENTER_DX: f32 = 36.;
/// The new page stays transparent until the old one has faded (ms).
pub const ENTER_FADE_DELAY: f64 = 80.;
/// The old page's exit duration (ms) and end offset (CSS px, a push).
pub const EXIT_MS: f64 = 90.;
pub const EXIT_DX: f32 = -28.;

/// CSS `cubic-bezier(x1, y1, x2, y2)` at input progress `t` (0..1).
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t == 0. || t == 1. {
        return t;
    }
    let curve = |a: f32, b: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * s * a + 3. * u * s * s * b + s * s * s
    };
    let slope = |a: f32, b: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * a + 6. * u * s * (b - a) + 3. * s * s * (1. - b)
    };
    // Newton's method from s = t, falling back to bisection (x is monotonic
    // because x1 and x2 lie in 0..1).
    let mut s = t;
    for _ in 0..8 {
        let error = curve(x1, x2, s) - t;
        if error.abs() < 1e-6 {
            return curve(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - error / d).clamp(0., 1.);
    }
    let (mut lo, mut hi) = (0f32, 1f32);
    s = t;
    for _ in 0..40 {
        let x = curve(x1, x2, s);
        if (x - t).abs() < 1e-6 {
            break;
        }
        if x < t {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.;
    }
    curve(y1, y2, s)
}

/// A new page generation for a controller's `show` (unique per process, so
/// a menu closed and reopened between two frames still slides in).
pub fn next_show() -> u64 {
    static SHOWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    SHOWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The moving page's `opacity` and `translateX` (CSS px) at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub opacity: f32,
    pub dx: f32,
}

/// The entry `elapsed` ms after the page was shown (a push; see
/// [`enter_towards`]); None once it rests.
pub fn enter(elapsed: f64) -> Option<Motion> {
    enter_towards(elapsed, 1.)
}

/// The entry for a push (`direction` 1, from the right) or a pop (-1).
pub fn enter_towards(elapsed: f64, direction: f32) -> Option<Motion> {
    if !(0. ..ENTER_MS).contains(&elapsed) {
        return None;
    }
    let travel = cubic_bezier(0.2, 0., 0., 1., (elapsed / ENTER_MS) as f32);
    let fade = ((elapsed - ENTER_FADE_DELAY) / (ENTER_MS - ENTER_FADE_DELAY)).clamp(0., 1.);
    Some(Motion {
        opacity: cubic_bezier(0., 0., 0.58, 1., fade as f32),
        dx: direction * ENTER_DX * (1. - travel),
    })
}

/// The old page's exit (`ease-in`) for a push; [`exit_towards`] for a pop.
pub fn exit(elapsed: f64) -> Option<Motion> {
    exit_towards(elapsed, 1.)
}

pub fn exit_towards(elapsed: f64, direction: f32) -> Option<Motion> {
    if !(0. ..EXIT_MS).contains(&elapsed) {
        return None;
    }
    let p = cubic_bezier(0.42, 0., 1., 1., (elapsed / EXIT_MS) as f32);
    Some(Motion {
        opacity: 1. - p,
        dx: direction * EXIT_DX * p,
    })
}

/// A menu surface's transition: restarted by each page shown.
///
/// The engine draws every step itself: its clock advances at most
/// [`MAX_STEP`] ms per drawn frame, so a stalled frame (a first garage frame
/// building its 3D previews) delays the slide instead of skipping it.
#[derive(Clone, Debug, Default)]
pub struct PageTransition {
    /// The page generation last seen (see the controllers' `shown`).
    seen: Option<u64>,
    /// Entry time so far and when it was last sampled (ms).
    elapsed: Option<f64>,
    sampled: Option<f64>,
    /// 1: a push (from the right), -1: a pop (from the left).
    direction: f32,
    /// Pages on the way to the current one, for the direction.
    trail: Vec<String>,
    /// The old page's last frame and its exit time so far.
    exit: Option<(DisplayList, f64)>,
}

/// The longest step of slide time per drawn frame (ms): two 60 Hz frames.
pub const MAX_STEP: f64 = 1000. / 30.;

impl PageTransition {
    /// Whether `shown` starts a new transition (the caller keeps what it
    /// needs to draw the old page sliding out).
    pub fn changes(&self, shown: Option<u64>) -> bool {
        shown != self.seen
    }

    /// Notes the current page generation and name; a new one starts the
    /// transition unless `reduced` (prefers-reduced-motion). None: no menu.
    /// `on_screen` is the content drawn last frame, which slides out.
    pub fn observe(&mut self, shown: Option<u64>, page: &str, reduced: bool, on_screen: &[Item]) {
        if shown == self.seen {
            return;
        }
        let had_page = self.seen.is_some();
        self.seen = shown;
        self.sampled = None;
        if shown.is_none() {
            self.trail.clear();
            self.elapsed = None;
            self.exit = None;
            return;
        }
        self.direction = self.step(page);
        self.elapsed = (!reduced).then_some(0.);
        // The exit runs even without a snapshot: a menu redraws its old
        // page itself from [`Self::exit_motion`].
        self.exit = (!reduced && had_page).then(|| {
            (
                DisplayList {
                    items: on_screen.to_vec(),
                },
                0.,
            )
        });
    }

    /// Push or pop: back to a page on the trail pops to it.
    fn step(&mut self, page: &str) -> f32 {
        if let Some(at) = self.trail.iter().rposition(|p| p == page) {
            let popped = at + 1 < self.trail.len();
            self.trail.truncate(at + 1);
            return if popped { -1. } else { 1. };
        }
        if page == "title" {
            self.trail.clear();
        }
        if self.trail.len() >= 32 {
            self.trail.remove(0);
        }
        self.trail.push(page.into());
        1.
    }

    /// A resize (or a reduced-motion change) cancels it: the page rests.
    pub fn cancel(&mut self) {
        self.elapsed = None;
        self.exit = None;
    }

    /// The new page's motion for a frame drawn at `now` (ms); None at rest.
    /// Advances the old page's exit too.
    pub fn motion(&mut self, now: f64) -> Option<Motion> {
        let step = self
            .sampled
            .map_or(0., |sampled| (now - sampled).clamp(0., MAX_STEP));
        self.sampled = Some(now);
        if let Some((_, exit)) = &mut self.exit {
            *exit += step;
            if *exit >= EXIT_MS {
                self.exit = None;
            }
        }
        let elapsed = self.elapsed.as_mut()?;
        *elapsed += step;
        let motion = enter_towards(*elapsed, self.direction);
        if motion.is_none() {
            self.elapsed = None;
        }
        motion
    }

    /// The old page's motion while it slides out.
    pub fn exit_motion(&self) -> Option<Motion> {
        let (_, elapsed) = self.exit.as_ref()?;
        exit_towards(*elapsed, self.direction)
    }

    /// Paints the old page's snapshot sliding out, if it still is.
    pub fn paint_exit(&self, list: &mut DisplayList) {
        let Some((snapshot, elapsed)) = &self.exit else {
            return;
        };
        let Some(m) = exit_towards(*elapsed, self.direction) else {
            return;
        };
        let mut moved = snapshot.clone();
        moved.translate(m.dx, 0.);
        list.push(Item::PushOpacity(m.opacity));
        list.items.extend(moved.items);
        list.push(Item::PopOpacity);
    }

    /// 1 for a push, -1 for a pop (the last page shown).
    pub fn direction(&self) -> f32 {
        self.direction
    }

    /// Frames keep coming while either page moves.
    pub fn active(&self) -> bool {
        self.elapsed.is_some() || self.exit.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_curves() {
        // Linear control points reproduce the input.
        for t in [0.1f32, 0.37, 0.8] {
            assert!((cubic_bezier(0.25, 0.25, 0.75, 0.75, t) - t).abs() < 1e-4);
        }
        // ease-in at the halfway point (browser value 0.3153).
        assert!((cubic_bezier(0.42, 0., 1., 1., 0.5) - 0.3153).abs() < 1e-3);
        // cubic-bezier(.16,1,.3,1) is a strong ease-out.
        let early = cubic_bezier(0.16, 1., 0.3, 1., 0.25);
        assert!(early > 0.8 && early < 1., "{early}");
        assert_eq!(cubic_bezier(0.16, 1., 0.3, 1., 0.), 0.);
        assert_eq!(cubic_bezier(0.16, 1., 0.3, 1., 1.), 1.);
    }

    #[test]
    fn entry_and_exit_curves() {
        let start = enter(0.).unwrap();
        assert_eq!(start.opacity, 0.);
        assert_eq!(start.dx, ENTER_DX);
        // Transparent until the old page has faded, still sliding.
        let early = enter(ENTER_FADE_DELAY).unwrap();
        assert_eq!(early.opacity, 0.);
        assert!(early.dx > 8. && early.dx < ENTER_DX, "{early:?}");
        let mid = enter(160.).unwrap();
        assert!(mid.opacity > 0.5 && mid.dx < 4., "{mid:?}");
        let late = enter(ENTER_MS - 1.).unwrap();
        assert!(late.opacity > 0.99 && late.dx < 0.1, "{late:?}");
        // The old page is gone before the new one shows.
        assert!(exit(ENTER_FADE_DELAY).unwrap().opacity < 0.2);
        assert!(enter(ENTER_MS).is_none());
        assert!(enter(-1.).is_none());
        let exit_mid = exit(EXIT_MS / 2.).unwrap();
        assert!((exit_mid.opacity - (1. - 0.3153)).abs() < 1e-3);
        assert!(exit(EXIT_MS).is_none());
    }

    fn list(n: usize) -> DisplayList {
        let mut l = DisplayList::default();
        for i in 0..n {
            l.rect(
                crate::Rect::new(i as f32, 0., 10., 10.),
                crate::Color::WHITE,
            );
        }
        l
    }

    #[test]
    fn shows_restart_and_reduced_motion_skips() {
        let none = DisplayList::default();
        let mut t = PageTransition::default();
        t.observe(Some(1), "main", false, &none.items);
        // The first drawn frame starts the clock.
        assert_eq!(t.motion(5_000.).map(|m| m.dx), Some(ENTER_DX));
        assert!(t.motion(5_016.).is_some());
        let mut now = 5_016.;
        while t.motion(now).is_some() {
            now += 16.;
        }
        assert!((now - 5_000. - ENTER_MS).abs() <= 16., "{now}");
        assert!(!t.active());
        // The same page generation does not restart it.
        t.observe(Some(1), "main", false, &none.items);
        assert!(t.motion(6_000.).is_none());
        t.observe(Some(2), "single", false, &none.items);
        assert_eq!(t.motion(7_000.).map(|m| m.dx), Some(ENTER_DX));
        t.cancel();
        assert!(t.motion(7_010.).is_none());
        t.observe(Some(3), "tracks", true, &list(3).items);
        assert!(t.motion(8_000.).is_none());
        assert!(!t.active(), "reduced motion: no exit either");
        // Closing the menu starts nothing.
        t.observe(None, "", false, &list(3).items);
        assert!(!t.active());
    }

    #[test]
    fn back_mirrors_the_slide_and_the_old_page_slides_out() {
        let mut t = PageTransition::default();
        t.observe(Some(1), "title", false, &[]);
        t.observe(Some(2), "main", false, &list(2).items);
        assert_eq!(t.direction(), 1.);
        t.observe(Some(3), "single", false, &list(2).items);
        assert_eq!(t.motion(0.).map(|m| m.dx), Some(ENTER_DX));
        // The old page is drawn, wrapped in its fading opacity, moved left.
        let mut out = DisplayList::default();
        t.motion(80.);
        t.paint_exit(&mut out);
        assert_eq!(out.items.len(), 4);
        match (&out.items[0], &out.items[1]) {
            (Item::PushOpacity(o), Item::Box(b)) => {
                assert!(*o > 0. && *o < 1.);
                assert!(b.rect.x < 0.);
            }
            other => panic!("{other:?}"),
        }
        // Back to main: a pop, from the left, and the old page goes right.
        t.observe(Some(4), "main", false, &list(1).items);
        assert_eq!(t.direction(), -1.);
        assert_eq!(t.motion(1_000.).map(|m| m.dx), Some(-ENTER_DX));
        t.motion(1_080.);
        let mut out = DisplayList::default();
        t.paint_exit(&mut out);
        match &out.items[1] {
            Item::Box(b) => assert!(b.rect.x > 0.),
            other => panic!("{other:?}"),
        }
        // Forward again is a push; the exit ends after EXIT_MS.
        t.observe(Some(5), "tracks", false, &list(1).items);
        assert_eq!(t.direction(), 1.);
        let mut now = 2_000.;
        t.motion(now);
        while t.active() {
            now += 16.;
            t.motion(now);
        }
        let mut out = DisplayList::default();
        t.paint_exit(&mut out);
        assert!(out.items.is_empty());
        assert!(now - 2_000. <= ENTER_MS + 16.);
    }

    #[test]
    fn a_stalled_frame_delays_the_slide_instead_of_skipping_it() {
        let mut t = PageTransition::default();
        t.observe(Some(1), "main", false, &[]);
        t.motion(0.);
        // 480 ms without a frame: the next one is only MAX_STEP further on.
        let m = t.motion(480.).unwrap();
        assert_eq!(Some(m), enter(MAX_STEP));
    }
}
