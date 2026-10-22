//! Menu page transitions (frontend.js `captureTransition` / `animatePage`).
//!
//! Every `show(page)` slides the new page in: `#menu-stage` animates from
//! `opacity: 0; translateX(36px)` to `opacity: 1; translateX(0)` over 240 ms
//! with `cubic-bezier(.16,1,.3,1)`. The menu backdrop (`#frontend` and its
//! `::before` art) stays put; under `prefers-reduced-motion: reduce` nothing
//! animates. A resize cancels the slide (the page snaps to rest), and input
//! is never held back by it: buttons follow the moving page.
//!
//! frontend.js also fades a snapshot of `#menu-canvas` out over 180 ms
//! (`ease-in`, to `translateX(-28px)`). Every page now draws as HD DOM
//! (menu-screens.js) or a pause panel, so that canvas is cleared at each
//! layout and the snapshot is transparent: the old page vanishes at once.
//! [`EXIT_MS`] and [`exit`] keep the recorded curve; the engine draws no
//! exit, matching what a player sees.

/// `animatePage` entry duration (ms) and start offset (CSS px).
pub const ENTER_MS: f64 = 240.;
pub const ENTER_DX: f32 = 36.;
/// The (invisible) exit snapshot's duration (ms) and end offset (CSS px).
pub const EXIT_MS: f64 = 180.;
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

/// The entry `elapsed` ms after the page was shown; None once it rests.
pub fn enter(elapsed: f64) -> Option<Motion> {
    if !(0. ..ENTER_MS).contains(&elapsed) {
        return None;
    }
    let p = cubic_bezier(0.16, 1., 0.3, 1., (elapsed / ENTER_MS) as f32);
    Some(Motion {
        opacity: p,
        dx: ENTER_DX * (1. - p),
    })
}

/// The exit snapshot's motion (`ease-in`); recorded, not drawn (see above).
pub fn exit(elapsed: f64) -> Option<Motion> {
    if !(0. ..EXIT_MS).contains(&elapsed) {
        return None;
    }
    let p = cubic_bezier(0.42, 0., 1., 1., (elapsed / EXIT_MS) as f32);
    Some(Motion {
        opacity: 1. - p,
        dx: EXIT_DX * p,
    })
}

/// A menu surface's slide-in: restarted by each page shown.
///
/// The browser runs the slide on its compositor, so it keeps time while the
/// page's script is busy (a first garage frame building its 3D previews).
/// The engine draws every step itself: its clock advances at most
/// [`MAX_STEP`] ms per drawn frame, so a stalled frame delays the slide
/// instead of skipping it.
#[derive(Clone, Debug, Default)]
pub struct PageTransition {
    /// The page generation last seen (see the controllers' `shown`).
    seen: Option<u64>,
    /// Slide time so far and when it was last sampled (ms).
    elapsed: Option<f64>,
    sampled: Option<f64>,
}

/// The longest step of slide time per drawn frame (ms): two 60 Hz frames.
pub const MAX_STEP: f64 = 1000. / 30.;

impl PageTransition {
    /// Notes the current page generation; a new one starts the slide unless
    /// `reduced` (prefers-reduced-motion). None: no menu.
    pub fn observe(&mut self, shown: Option<u64>, reduced: bool) {
        if shown != self.seen {
            self.seen = shown;
            self.elapsed = (shown.is_some() && !reduced).then_some(0.);
            self.sampled = None;
        }
    }

    /// A resize (or a reduced-motion change) cancels it: the page rests.
    pub fn cancel(&mut self) {
        self.elapsed = None;
    }

    /// The page's motion for a frame drawn at `now` (ms); None at rest.
    pub fn motion(&mut self, now: f64) -> Option<Motion> {
        let elapsed = self.elapsed.as_mut()?;
        if let Some(sampled) = self.sampled {
            *elapsed += (now - sampled).clamp(0., MAX_STEP);
        }
        self.sampled = Some(now);
        let motion = enter(*elapsed);
        if motion.is_none() {
            self.elapsed = None;
        }
        motion
    }

    /// Frames keep coming while it moves.
    pub fn active(&self) -> bool {
        self.elapsed.is_some()
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
    fn entry_matches_animate_page() {
        let start = enter(0.).unwrap();
        assert_eq!(start.opacity, 0.);
        assert_eq!(start.dx, ENTER_DX);
        let mid = enter(120.).unwrap();
        assert!(mid.opacity > 0.9 && mid.dx < 4., "{mid:?}");
        assert!(enter(ENTER_MS).is_none());
        assert!(enter(-1.).is_none());
        let exit_mid = exit(90.).unwrap();
        assert!((exit_mid.opacity - (1. - 0.3153)).abs() < 1e-3);
        assert!(exit(EXIT_MS).is_none());
    }

    #[test]
    fn shows_restart_and_reduced_motion_skips() {
        let mut t = PageTransition::default();
        t.observe(Some(1), false);
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
        t.observe(Some(1), false);
        assert!(t.motion(6_000.).is_none());
        t.observe(Some(2), false);
        assert_eq!(t.motion(7_000.).map(|m| m.dx), Some(ENTER_DX));
        t.cancel();
        assert!(t.motion(7_010.).is_none());
        t.observe(Some(3), true);
        assert!(t.motion(8_000.).is_none());
        // Closing the menu starts nothing.
        t.observe(None, false);
        assert!(!t.active());
    }

    #[test]
    fn a_stalled_frame_delays_the_slide_instead_of_skipping_it() {
        let mut t = PageTransition::default();
        t.observe(Some(1), false);
        t.motion(0.);
        // 480 ms without a frame: the next one is only MAX_STEP further on.
        let m = t.motion(480.).unwrap();
        assert_eq!(Some(m), enter(MAX_STEP));
    }
}
