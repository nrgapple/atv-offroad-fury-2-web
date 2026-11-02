//! The Lap Attack ghost (research/RACING.md "Lap Attack event type").
//!
//! Exact-build evidence (USA v2.00 `SCUS_972.11`, private exports):
//!
//! - The race scene setup `001f6c60`, in session mode 2 (Lap Attack), gives
//!   each human (`+0x84` of them) a second rider (`001f9170`, the human's
//!   own quad) whose vehicle carries flag 0x10 (`00248460`). `001e4c90`
//!   draws a 0x10 vehicle at the scene's GhostOpacity (`00209680`: default
//!   0.4, Environment key "GhostOpacity").
//! - `001f6640` then binds the human (source) to that ghost (target) in the
//!   session's per-rider recorder (`+0x30 +100 +4n`): quantized channels
//!   for the rider's position, velocities, the body, wheels and controls.
//!   In other modes the same call binds a rider to itself (the replay).
//!
//! The port records the player's own fixed-step motion (one sample per
//! simulation step, unquantized: a port choice) and, once a lap completes,
//! keeps the fastest one and plays it back from each new lap's start.
use serde::Serialize;

/// GhostOpacity's default (`00209680`: 0x3ecccccd).
pub const OPACITY: f32 = 0.4;

/// One fixed step of the player's motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Sample {
    pub origin: [f32; 3],
    /// The vehicle orientation quaternion.
    pub orientation: [f32; 4],
}

/// The ghost's state for the HUD and `#ui-state`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Frame {
    /// The best lap the ghost replays (seconds) and its sample count.
    pub best: f32,
    pub samples: usize,
    /// The ghost's pose this step (none once its lap has played out).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pose: Option<Sample>,
    pub opacity: f32,
}

/// Records each lap and replays the best one.
#[derive(Clone, Debug, Default)]
pub struct Ghost {
    current: Vec<Sample>,
    best: Option<(f32, Vec<Sample>)>,
    /// Steps into the current lap (the playback index).
    step: usize,
}

impl Ghost {
    /// Records the player's motion for this step and advances playback.
    pub fn record(&mut self, sample: Sample) {
        self.current.push(sample);
        self.step += 1;
    }

    /// A lap completed in `time` seconds: a new best replaces the ghost,
    /// if the race `judged` it for the best lap (Supercross's first lap
    /// from the grid is not). Either way recording and playback restart for
    /// the next lap. Returns whether the lap is the new best.
    pub fn lap(&mut self, time: f32, judged: bool) -> bool {
        let lap = std::mem::take(&mut self.current);
        self.step = 0;
        let better = judged
            && !lap.is_empty()
            && time > 0.
            && self.best.as_ref().is_none_or(|(best, _)| time < *best);
        if better {
            self.best = Some((time, lap));
        }
        better
    }

    /// The best lap's time.
    pub fn best(&self) -> Option<f32> {
        self.best.as_ref().map(|(t, _)| *t)
    }

    /// The ghost's pose at the current step of the lap.
    pub fn pose(&self) -> Option<Sample> {
        let (_, samples) = self.best.as_ref()?;
        samples.get(self.step.checked_sub(1)?).copied()
    }

    pub fn frame(&self) -> Option<Frame> {
        let (best, samples) = self.best.as_ref()?;
        Some(Frame {
            best: *best,
            samples: samples.len(),
            pose: self.pose(),
            opacity: OPACITY,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(i: usize) -> Sample {
        let t = i as f32;
        Sample {
            origin: [t, 0.5 * t, -t],
            orientation: [0., t * 0.001, 0., 1. - t * 0.0005],
        }
    }

    #[test]
    fn replays_the_recorded_lap_step_for_step() {
        let mut ghost = Ghost::default();
        assert!(ghost.frame().is_none());
        for i in 0..120 {
            ghost.record(sample(i));
        }
        assert!(ghost.lap(2.0, true));
        // Playback follows the recorded samples exactly, from the lap start.
        for i in 0..120 {
            ghost.record(sample(1000 + i));
            assert_eq!(ghost.pose(), Some(sample(i)));
        }
        // Past the recorded lap the ghost has finished.
        ghost.record(sample(0));
        assert_eq!(ghost.pose(), None);
        assert_eq!(ghost.frame().unwrap().samples, 120);
    }

    #[test]
    fn keeps_only_the_fastest_lap() {
        let mut ghost = Ghost::default();
        for i in 0..100 {
            ghost.record(sample(i));
        }
        assert!(ghost.lap(10., true));
        for i in 0..200 {
            ghost.record(sample(500 + i));
        }
        // A slower lap keeps the old ghost.
        assert!(!ghost.lap(12., true));
        assert_eq!(ghost.best(), Some(10.));
        ghost.record(sample(9));
        assert_eq!(ghost.pose(), Some(sample(0)));
        for i in 0..50 {
            ghost.record(sample(700 + i));
        }
        // The current lap restarted at the slower lap's end: this one is 51
        // steps and faster.
        assert!(ghost.lap(8., true));
        assert_eq!(ghost.frame().unwrap().samples, 51);
        assert_eq!(ghost.frame().unwrap().best, 8.);
    }

    #[test]
    fn an_unjudged_lap_never_becomes_the_ghost() {
        let mut ghost = Ghost::default();
        for i in 0..80 {
            ghost.record(sample(i));
        }
        assert!(!ghost.lap(5., false));
        assert!(ghost.frame().is_none());
        for i in 0..90 {
            ghost.record(sample(200 + i));
        }
        assert!(ghost.lap(6., true));
        ghost.record(sample(0));
        assert_eq!(ghost.pose(), Some(sample(200)));
    }

    #[test]
    fn identical_runs_give_identical_ghosts() {
        let run = || {
            let mut ghost = Ghost::default();
            for lap in 0..3 {
                for i in 0..(90 - lap * 10) {
                    ghost.record(sample(i * 3 + lap));
                }
                ghost.lap(9. - lap as f32, true);
            }
            ghost.record(sample(0));
            ghost.frame()
        };
        assert_eq!(run(), run());
        assert_eq!(run().unwrap().pose, Some(sample(2)));
    }
}
