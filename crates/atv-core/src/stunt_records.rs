//! The profile's stunt records: Longest Jump, Most Combos and Most Points
//! In One Jump (research/PROFILE.md, "Stunt records").
//!
//! Exact-build evidence (private exports, bounded reading):
//!
//! - `0024df70`, run by the race status update (`0024ba00`) for every
//!   racer, measures a jump. On the first airborne step it keeps the
//!   takeoff position (+0x1b0); on the first grounded step after that it
//!   keeps the landing position (+0x1c0) and the straight-line 3D distance
//!   between them (+0x114, world units = feet) and marks a jump pending.
//!   Ground time (+0x1d0) runs while grounded and restarts at 0 in the air;
//!   once it reaches the race state's settle time (+0xe4, 0.5 s from
//!   `0020f780`) the pending jump is judged: a distance above the racer's
//!   race best (+0x118) becomes the best and calls the mode's jump hook
//!   (vtable +0x134: race `001f2610`, Freestyle `002fd7c0`).
//! - Race `001f2610` → `00324cc0` keeps the course record's longest jump
//!   (+0x88) when the race best exceeds 40 ft and the record ("New Personal
//!   Longest Jump %d Feet!").
//! - A finished stunt chain (vtable +0x108: race `001f22a0`, Freestyle
//!   `002fd350`) is credited only when it lands: its points are
//!   (int)(sum × count) and its combo count is the chain's event count
//!   (rider +0xfc). `00324f50` keeps the most points in one jump (+0x90),
//!   `00324e60` the most combos (+0x8c) for counts other than 1, and a
//!   chain of `n` > 1 events adds `n - 1` Profile Points to the race award
//!   accumulator (manager +300) that `001f1e30` credits at the finish.
//! - `00322080` copies the course record's maxima into the profile's
//!   longest jump (+0x1c), most combos (+0x24) and most points in one jump
//!   (+0x28) at the finish.
use serde::{Deserialize, Serialize};

/// Race state +0xe4 (`0020f780`): seconds a landing must stay grounded.
pub const SETTLE_SECONDS: f32 = 0.5;
/// `00324cc0`: a race jump must exceed 40 ft to be a course record.
pub const RACE_MIN_JUMP_FEET: f32 = 40.;

/// `0024df70`'s per-racer jump measurement.
#[derive(Clone, Debug, Default)]
pub struct JumpTracker {
    airborne: bool,
    takeoff: [f32; 3],
    /// Distance of the last landing, waiting to settle.
    pending: Option<f32>,
    grounded_seconds: f32,
}

impl JumpTracker {
    /// One step: returns a jump's takeoff-to-landing distance once its
    /// landing has stayed grounded for [`SETTLE_SECONDS`].
    pub fn step(&mut self, grounded: bool, position: [f32; 3], dt: f32) -> Option<f32> {
        if !grounded {
            self.grounded_seconds = 0.;
            if !self.airborne {
                self.takeoff = position;
            }
            self.airborne = true;
        } else {
            self.grounded_seconds += dt;
            if self.airborne {
                let d: f32 = (0..3)
                    .map(|i| (position[i] - self.takeoff[i]).powi(2))
                    .sum();
                self.pending = Some(d.sqrt());
            }
            self.airborne = false;
        }
        if self.pending.is_some() && self.grounded_seconds >= SETTLE_SECONDS {
            return self.pending.take();
        }
        None
    }
}

/// One race's (or a profile's) stunt records. Lengths are feet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StuntRecords {
    /// Longest Jump, feet (0 when none).
    pub longest_jump: f32,
    /// Most Combos: the most events in one landed chain.
    pub most_combos: u32,
    /// Most Points In One Jump.
    pub most_points: i64,
}

// Records hold finite lengths only, so equality is reflexive.
impl Eq for StuntRecords {}

impl StuntRecords {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Keeps the larger of each record (`00322080`'s maxima).
    pub fn merge(&mut self, other: &StuntRecords) {
        self.longest_jump = self.longest_jump.max(other.longest_jump);
        self.most_combos = self.most_combos.max(other.most_combos);
        self.most_points = self.most_points.max(other.most_points);
    }
}

/// The player's stunt records for one race, as the race hooks keep them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct RaceStunts {
    pub records: StuntRecords,
    /// Profile Points from combos: `n - 1` per landed chain of `n` > 1.
    pub combo_points: i32,
}

impl RaceStunts {
    /// `001f2610` → `00324cc0`: a settled jump.
    pub fn jump(&mut self, feet: f32) {
        if feet.is_finite() && feet > RACE_MIN_JUMP_FEET && feet > self.records.longest_jump {
            self.records.longest_jump = feet;
        }
    }

    /// `001f22a0`: a finished chain of `count` events whose points sum to
    /// `sum`; only a landed chain counts.
    pub fn chain(&mut self, success: bool, count: usize, sum: f32) {
        if !success || count == 0 {
            return;
        }
        let count32 = count.min(u32::MAX as usize) as u32;
        let points = (sum * count as f32) as i64;
        self.records.most_points = self.records.most_points.max(points);
        if count32 != 1 {
            self.records.most_combos = self.records.most_combos.max(count32);
        }
        if count32 > 1 {
            self.combo_points = self.combo_points.saturating_add(count32 as i32 - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1. / 60.;

    fn ride(tracker: &mut JumpTracker, grounded: bool, p: [f32; 3], steps: usize) -> Vec<f32> {
        (0..steps)
            .filter_map(|_| tracker.step(grounded, p, DT))
            .collect()
    }

    #[test]
    fn jump_is_takeoff_to_landing_distance_after_settling() {
        let mut t = JumpTracker::default();
        assert!(ride(&mut t, true, [0., 0., 0.], 60).is_empty());
        // Takeoff at the first airborne step's position.
        assert!(t.step(false, [0., 1., 0.], DT).is_none());
        assert!(ride(&mut t, false, [20., 15., 0.], 30).is_empty());
        // Landing 30 ft away (and 40 ft further along z, 3D distance 50).
        assert!(t.step(true, [30., 1., 40.], DT).is_none());
        // Judged once grounded for 0.5 s (30 steps at 60 Hz).
        let settled = ride(&mut t, true, [35., 1., 45.], 40);
        assert_eq!(settled.len(), 1);
        assert!((settled[0] - 50.).abs() < 1e-4);
    }

    #[test]
    fn bounce_before_settling_restarts_the_judgement() {
        let mut t = JumpTracker::default();
        t.step(false, [0., 0., 0.], DT);
        t.step(true, [10., 0., 0.], DT);
        assert!(ride(&mut t, true, [10., 0., 0.], 10).is_empty());
        // A short hop: the second takeoff starts a new jump.
        t.step(false, [12., 0., 0.], DT);
        t.step(true, [15., 0., 0.], DT);
        let settled = ride(&mut t, true, [15., 0., 0.], 40);
        assert_eq!(settled, vec![3.]);
    }

    #[test]
    fn race_keeps_jumps_over_40_feet_and_landed_chains() {
        let mut r = RaceStunts::default();
        r.jump(39.9);
        assert_eq!(r.records.longest_jump, 0.);
        r.jump(62.5);
        r.jump(50.);
        assert_eq!(r.records.longest_jump, 62.5);
        r.chain(true, 1, 750.);
        assert_eq!((r.records.most_combos, r.records.most_points), (0, 750));
        r.chain(true, 3, 500.25);
        assert_eq!((r.records.most_combos, r.records.most_points), (3, 1500));
        r.chain(false, 5, 9000.);
        assert_eq!((r.records.most_combos, r.records.most_points), (3, 1500));
        r.chain(true, 2, 100.);
        assert_eq!(r.combo_points, 2 + 1);
        let mut p = StuntRecords {
            longest_jump: 70.,
            ..Default::default()
        };
        p.merge(&r.records);
        assert_eq!(
            p,
            StuntRecords {
                longest_jump: 70.,
                most_combos: 3,
                most_points: 1500
            }
        );
    }
}
