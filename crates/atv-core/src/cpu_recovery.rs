//! **Port adaptation, not recovered behavior.** An authored layer that keeps
//! race CPUs from looping or stranding where the recovered rules (wreck
//! recovery at the accepted reset point, the leader speed limit, the lane
//! offset) leave them repeating the same failure. The user approved this
//! (status.md "Product decisions": CPU riders must never stay stuck).
//!
//! It watches each CPU's race distance (progress along the route, not
//! speed) and its resets, and escalates only when the CPU repeats itself:
//!
//! 1. **Assist** after the second reset near the same race distance without
//!    clearing it: the leader speed limit is lifted (a CPU held to 75ft/s
//!    at sx15 takes a 220ft/s jump too slowly) and the lane goal is held on
//!    the racing line, until the CPU is [`CLEAR`] past that reset.
//! 2. **Skip ahead** after the third such reset, or when the race distance
//!    has not advanced [`PROGRESS_STEP`] for [`STALL_SECONDS`] while
//!    riding: the caller resets the CPU to a validated point further along
//!    the route (instead of the accepted point, or in Enduro the rider's
//!    own last valid position, which put a pinned CPU back where it stood).
//!
//! Pure and deterministic (f32 arithmetic on the fixed race step, no clock),
//! applied to CPU riders only. Recovered logic is untouched; with the layer
//! never consulted the race runs exactly as before.

/// Race distance a CPU must gain to count as progress (ft).
pub const PROGRESS_STEP: f32 = 25.;
/// Seconds without [`PROGRESS_STEP`] of progress, riding, before a skip.
pub const STALL_SECONDS: f32 = 15.;
/// Resets within this race distance of the previous one count as repeats.
pub const SAME_PLACE: f32 = 80.;
/// Race distance past a repeated reset that clears the escalation.
pub const CLEAR: f32 = 400.;
/// Route distances tried, in order, for a skip-ahead reset point.
pub const SKIP_DISTANCES: [f32; 8] = [80., 120., 160., 200., 260., 320., 400., 500.];

/// What the race should do for this CPU now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    /// Reset the CPU to a validated point ahead on the route.
    SkipAhead,
}

/// Driver adjustments while a repeat is being worked through.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Assist {
    /// Ignore the leader speed limit (`002536b0`).
    pub no_leader_limit: bool,
    /// Hold the lane goal on the racing line.
    pub centre_lane: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Recovery {
    /// Race distance at the last progress mark and seconds since it.
    mark: Option<f32>,
    since_progress: f32,
    /// Race distance of the last reset and how many resets in a row came
    /// back to about the same place.
    last_reset: Option<f32>,
    repeats: u32,
    /// Race distance at which the escalation clears.
    clear_at: f32,
    /// Skip-ahead resets requested (for traces and tests).
    pub skips: u32,
}

impl Recovery {
    /// One race step for a running, unfinished CPU at race `distance`.
    pub fn observe(&mut self, distance: f32, riding: bool, dt: f32) -> Action {
        match self.mark {
            Some(mark) if distance < mark + PROGRESS_STEP => {
                if riding {
                    self.since_progress += dt;
                }
            }
            _ => {
                self.mark = Some(distance);
                self.since_progress = 0.;
            }
        }
        if self.repeats > 0 && distance >= self.clear_at {
            self.repeats = 0;
        }
        if riding && (self.since_progress > STALL_SECONDS || self.repeats >= 2) {
            return Action::SkipAhead;
        }
        Action::None
    }

    /// The CPU was reset (wreck recovery, off-track or stuck reset) and is
    /// now at race `distance`.
    pub fn reset(&mut self, distance: f32) {
        let repeat = self
            .last_reset
            .is_some_and(|previous| (distance - previous).abs() < SAME_PLACE);
        if repeat {
            self.repeats += 1;
        } else {
            self.repeats = 0;
        }
        self.last_reset = Some(distance);
        self.clear_at = distance + CLEAR;
    }

    /// A skip-ahead reset was made; the CPU is now at race `distance`.
    pub fn skipped(&mut self, distance: f32) {
        self.skips += 1;
        self.repeats = 0;
        self.last_reset = Some(distance);
        self.clear_at = distance + CLEAR;
        self.mark = Some(distance);
        self.since_progress = 0.;
    }

    pub fn assist(&self) -> Assist {
        let on = self.repeats >= 1;
        Assist {
            no_leader_limit: on,
            centre_lane: on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1. / 60.;

    #[test]
    fn a_single_wreck_changes_nothing() {
        let mut r = Recovery::default();
        for i in 0..600 {
            assert_eq!(r.observe(i as f32, true, DT), Action::None);
        }
        r.reset(500.);
        assert_eq!(r.assist(), Assist::default());
        assert_eq!(r.observe(500., true, DT), Action::None);
    }

    #[test]
    fn repeated_resets_assist_then_skip() {
        let mut r = Recovery::default();
        r.reset(1000.);
        r.observe(1000., true, DT);
        r.reset(1010.);
        assert!(r.assist().no_leader_limit && r.assist().centre_lane);
        assert_eq!(r.observe(1010., true, DT), Action::None);
        r.reset(995.);
        assert_eq!(r.observe(995., true, DT), Action::SkipAhead);
        // Not while wrecked: the skip waits for the rider.
        assert_eq!(r.observe(995., false, DT), Action::None);
        r.skipped(1200.);
        assert_eq!(r.assist(), Assist::default());
        assert_eq!(r.observe(1200., true, DT), Action::None);
        assert_eq!(r.skips, 1);
    }

    #[test]
    fn clearing_the_spot_ends_the_assist() {
        let mut r = Recovery::default();
        r.reset(1000.);
        r.reset(1000.);
        assert!(r.assist().no_leader_limit);
        r.observe(1300., true, DT);
        assert!(r.assist().no_leader_limit);
        r.observe(1401., true, DT);
        assert_eq!(r.assist(), Assist::default());
        // A later reset elsewhere starts afresh.
        r.reset(3000.);
        assert_eq!(r.assist(), Assist::default());
    }

    #[test]
    fn no_progress_while_riding_skips() {
        let mut r = Recovery::default();
        let mut skipped = None;
        for i in 0..(STALL_SECONDS * 60.) as usize + 120 {
            // Bouncing in place: the distance wobbles but never gains 25ft.
            let d = 700. + (i % 30) as f32 * 0.5;
            if r.observe(d, true, DT) == Action::SkipAhead {
                skipped = Some(i);
                break;
            }
        }
        let at = skipped.expect("stall detected") as f32 * DT;
        assert!((STALL_SECONDS..STALL_SECONDS + 0.1).contains(&at));
    }

    #[test]
    fn wreck_time_does_not_count_as_stalling() {
        let mut r = Recovery::default();
        for _ in 0..(STALL_SECONDS * 120.) as usize {
            assert_eq!(r.observe(700., false, DT), Action::None);
        }
    }

    #[test]
    fn steady_progress_never_stalls() {
        let mut r = Recovery::default();
        for i in 0..20_000 {
            assert_eq!(r.observe(i as f32 * 0.5, true, DT), Action::None);
        }
    }
}
