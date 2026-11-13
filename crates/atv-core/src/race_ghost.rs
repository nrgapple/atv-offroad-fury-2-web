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
//!
//! The port records the player's own fixed-step motion (one sample per
//! simulation step, unquantized: a port choice) and the drawn quad and
//! rider node matrices every [`KEY_STEPS`] steps and, once a lap completes,
//! keeps the fastest one and plays it back from each new lap's start. The
//! renderer draws it as the player's own quad and rider at the opacity.
//!
//! A world board row's ghost (`world_ghost`, a port addition) can stand in
//! as the lap to beat ([`Ghost::set_rival`]): it carries motion only, so it
//! is drawn in the pose of the player's own first key (seated on the grid).
use serde::Serialize;

/// GhostOpacity's default (`00209680`: 0x3ecccccd, Environment key
/// "GhostOpacity"): the ghost rider's draw opacity (`001e4c90` sets it on
/// a vehicle with flag 0x10 through `001b42e0`, the object's +0x19c).
pub const OPACITY: f32 = 0.4;

/// Node matrix keys are at least this many steps apart (15 Hz at the 60 Hz
/// step); playback blends between keys. A port choice that keeps a
/// minute's lap near 1.7 MB (the original quantizes its channels).
pub const KEY_STEPS: usize = 4;

/// A column-major 4x4 matrix.
pub type Matrix = [f32; 16];

const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

/// One fixed step of the player's motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Sample {
    pub origin: [f32; 3],
    /// The vehicle orientation quaternion.
    pub orientation: [f32; 4],
}

/// The drawn pose of one key step: the quad's and rider's model-space node
/// matrices and the rider's world relative to the quad's (identity while
/// seated; the wreck rig moves it).
#[derive(Clone, Copy, Debug)]
pub struct Nodes<'a> {
    pub quad: &'a [Matrix],
    pub rider: &'a [Matrix],
    pub rider_offset: Matrix,
}

/// A replayed pose's node matrices, reused frame to frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseNodes {
    pub quad: Vec<Matrix>,
    pub rider: Vec<Matrix>,
    pub rider_offset: Matrix,
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

/// One lap's recording: every step's motion and keyed node matrices,
/// flat (`widths` per key) with each key's step.
#[derive(Clone, Debug, Default)]
struct Lap {
    samples: Vec<Sample>,
    key_steps: Vec<usize>,
    quad: Vec<Matrix>,
    rider: Vec<Matrix>,
    offsets: Vec<Matrix>,
    widths: Option<(usize, usize)>,
    /// The node counts changed mid-lap: the lap replays its motion but
    /// cannot be drawn.
    broken: bool,
    /// A downloaded lap (motion only): it takes the player's first key as
    /// its one pose.
    borrows: bool,
}

impl Lap {
    fn clear(&mut self) {
        self.samples.clear();
        self.key_steps.clear();
        self.quad.clear();
        self.rider.clear();
        self.offsets.clear();
        self.widths = None;
        self.broken = false;
        self.borrows = false;
    }

    fn keys(&self) -> usize {
        self.offsets.len()
    }

    fn drawable(&self) -> bool {
        !self.broken && self.keys() > 0
    }
}

/// Records each lap and replays the best one.
#[derive(Clone, Debug)]
pub struct Ghost {
    current: Lap,
    best: Option<(f32, Lap)>,
    /// Steps into the current lap (the playback index).
    step: usize,
    opacity: f32,
}

impl Default for Ghost {
    fn default() -> Self {
        Self::new(OPACITY)
    }
}

impl Ghost {
    /// A ghost drawn at `opacity` (the venue's GhostOpacity).
    pub fn new(opacity: f32) -> Self {
        Self {
            current: Lap::default(),
            best: None,
            step: 0,
            opacity: if opacity.is_finite() {
                opacity.clamp(0., 1.)
            } else {
                OPACITY
            },
        }
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// Starts against another rider's lap of `time` seconds (a world board
    /// row's ghost, `world_ghost::Lap::samples`): it replays from each lap's
    /// start until the player laps faster, as the player's own best would.
    pub fn set_rival(&mut self, time: f32, samples: Vec<Sample>) {
        if samples.is_empty() || !time.is_finite() || time <= 0. {
            return;
        }
        self.best = Some((
            time,
            Lap {
                samples,
                borrows: true,
                ..Default::default()
            },
        ));
    }

    /// Whether the next [`Self::record`] keeps node matrices: the lap has
    /// no key yet or the last is [`KEY_STEPS`] old.
    pub fn wants_nodes(&self) -> bool {
        let lap = &self.current;
        lap.key_steps
            .last()
            .is_none_or(|key| lap.samples.len() >= key + KEY_STEPS)
    }

    /// Records the player's motion for this step and advances playback.
    /// Nodes given while [`Self::wants_nodes`] become a key (a step without
    /// them simply leaves the key to a later step).
    pub fn record(&mut self, sample: Sample, nodes: Option<Nodes<'_>>) {
        // A rival's lap has no pose of its own: it holds the player's.
        if let (Some(nodes), Some((_, rival))) = (nodes, self.best.as_mut())
            && rival.borrows
            && rival.keys() == 0
        {
            rival.widths = Some((nodes.quad.len(), nodes.rider.len()));
            rival.key_steps.push(0);
            rival.quad.extend_from_slice(nodes.quad);
            rival.rider.extend_from_slice(nodes.rider);
            rival.offsets.push(nodes.rider_offset);
        }
        if let Some(nodes) = nodes.filter(|_| self.wants_nodes()) {
            let lap = &mut self.current;
            let widths = (nodes.quad.len(), nodes.rider.len());
            if lap.widths.is_none_or(|w| w == widths) {
                lap.widths = Some(widths);
                lap.key_steps.push(lap.samples.len());
                lap.quad.extend_from_slice(nodes.quad);
                lap.rider.extend_from_slice(nodes.rider);
                lap.offsets.push(nodes.rider_offset);
            } else {
                lap.broken = true;
            }
        }
        self.current.samples.push(sample);
        self.step += 1;
    }

    /// A lap completed in `time` seconds: a new best replaces the ghost,
    /// if the race `judged` it for the best lap (Supercross's first lap
    /// from the grid is not). Either way recording and playback restart for
    /// the next lap, reusing the buffers. Returns whether the lap is the new
    /// best.
    pub fn lap(&mut self, time: f32, judged: bool) -> bool {
        self.step = 0;
        let better = judged
            && !self.current.samples.is_empty()
            && time > 0.
            && self.best.as_ref().is_none_or(|(best, _)| time < *best);
        if better {
            let lap = std::mem::take(&mut self.current);
            if let Some((_, old)) = self.best.replace((time, lap)) {
                self.current = old;
            }
        }
        self.current.clear();
        better
    }

    /// The best lap's time.
    pub fn best(&self) -> Option<f32> {
        self.best.as_ref().map(|(t, _)| *t)
    }

    /// The ghost's pose at the current step of the lap.
    pub fn pose(&self) -> Option<Sample> {
        let (_, lap) = self.best.as_ref()?;
        lap.samples.get(self.step.checked_sub(1)?).copied()
    }

    /// The ghost's pose this step with its node matrices, blended between
    /// the neighbouring keys into `out` (no allocation once its buffers have
    /// grown). None once the lap has played out or if it cannot be drawn.
    pub fn pose_nodes(&self, out: &mut PoseNodes) -> Option<Sample> {
        let (_, lap) = self.best.as_ref()?;
        let index = self.step.checked_sub(1)?;
        let sample = *lap.samples.get(index)?;
        if !lap.drawable() {
            return None;
        }
        let (quad_width, rider_width) = lap.widths?;
        // The keys around this step (the first and last hold).
        let after = lap.key_steps.partition_point(|&k| k <= index);
        let key = after.saturating_sub(1);
        let next = after.min(lap.keys() - 1);
        let t = match (lap.key_steps[key], lap.key_steps[next]) {
            (a, b) if b > a && index > a => (index - a) as f32 / (b - a) as f32,
            _ => 0.,
        };
        let blend = |out: &mut Vec<Matrix>, all: &[Matrix], width: usize| {
            out.clear();
            let (a, b) = (
                &all[key * width..(key + 1) * width],
                &all[next * width..(next + 1) * width],
            );
            out.extend(a.iter().zip(b).map(|(a, b)| mix(a, b, t)));
        };
        blend(&mut out.quad, &lap.quad, quad_width);
        blend(&mut out.rider, &lap.rider, rider_width);
        out.rider_offset = mix(&lap.offsets[key], &lap.offsets[next], t);
        Some(sample)
    }

    pub fn frame(&self) -> Option<Frame> {
        let (best, lap) = self.best.as_ref()?;
        Some(Frame {
            best: *best,
            samples: lap.samples.len(),
            pose: self.pose(),
            opacity: self.opacity,
        })
    }
}

fn mix(a: &Matrix, b: &Matrix, t: f32) -> Matrix {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// `a * b` for column-major matrices.
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| {
        let (column, row) = (i / 4, i % 4);
        (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum()
    })
}

/// The rider's world relative to the quad's, for rigid (rotation plus
/// translation) quad worlds: `inverse(quad) * rider`. Identity when they
/// coincide.
pub fn rider_offset(quad: &Matrix, rider: &Matrix) -> Matrix {
    if quad == rider {
        return IDENTITY;
    }
    // Rigid inverse: the transposed rotation and the rotated-back origin.
    let mut inverse = IDENTITY;
    for row in 0..3 {
        for column in 0..3 {
            inverse[column * 4 + row] = quad[row * 4 + column];
        }
    }
    for row in 0..3 {
        inverse[12 + row] = -(0..3)
            .map(|k| quad[row * 4 + k] * quad[12 + k])
            .sum::<f32>();
    }
    multiply(&inverse, rider)
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
            ghost.record(sample(i), None);
        }
        assert!(ghost.lap(2.0, true));
        // Playback follows the recorded samples exactly, from the lap start.
        for i in 0..120 {
            ghost.record(sample(1000 + i), None);
            assert_eq!(ghost.pose(), Some(sample(i)));
        }
        // Past the recorded lap the ghost has finished.
        ghost.record(sample(0), None);
        assert_eq!(ghost.pose(), None);
        assert_eq!(ghost.frame().unwrap().samples, 120);
    }

    #[test]
    fn keeps_only_the_fastest_lap() {
        let mut ghost = Ghost::default();
        for i in 0..100 {
            ghost.record(sample(i), None);
        }
        assert!(ghost.lap(10., true));
        for i in 0..200 {
            ghost.record(sample(500 + i), None);
        }
        // A slower lap keeps the old ghost.
        assert!(!ghost.lap(12., true));
        assert_eq!(ghost.best(), Some(10.));
        ghost.record(sample(9), None);
        assert_eq!(ghost.pose(), Some(sample(0)));
        for i in 0..50 {
            ghost.record(sample(700 + i), None);
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
            ghost.record(sample(i), None);
        }
        assert!(!ghost.lap(5., false));
        assert!(ghost.frame().is_none());
        for i in 0..90 {
            ghost.record(sample(200 + i), None);
        }
        assert!(ghost.lap(6., true));
        ghost.record(sample(0), None);
        assert_eq!(ghost.pose(), Some(sample(200)));
    }

    #[test]
    fn identical_runs_give_identical_ghosts() {
        let run = || {
            let mut ghost = Ghost::default();
            for lap in 0..3 {
                for i in 0..(90 - lap * 10) {
                    ghost.record(sample(i * 3 + lap), None);
                }
                ghost.lap(9. - lap as f32, true);
            }
            ghost.record(sample(0), None);
            ghost.frame()
        };
        assert_eq!(run(), run());
        assert_eq!(run().unwrap().pose, Some(sample(2)));
    }

    fn nodes(i: usize) -> (Vec<Matrix>, Vec<Matrix>, Matrix) {
        let t = i as f32;
        let mut quad = vec![IDENTITY; 3];
        quad[1][12] = t;
        let mut rider = vec![IDENTITY; 2];
        rider[0][13] = -t;
        let mut offset = IDENTITY;
        offset[14] = 2. * t;
        (quad, rider, offset)
    }

    fn record_lap(ghost: &mut Ghost, steps: usize, base: usize) {
        for i in 0..steps {
            let n = ghost.wants_nodes().then(|| nodes(base + i));
            ghost.record(
                sample(base + i),
                n.as_ref().map(|(quad, rider, offset)| Nodes {
                    quad,
                    rider,
                    rider_offset: *offset,
                }),
            );
        }
    }

    #[test]
    fn replays_node_matrices_between_keys() {
        let mut ghost = Ghost::new(0.4);
        record_lap(&mut ghost, 41, 0);
        assert!(ghost.lap(3., true));
        let mut out = PoseNodes::default();
        // Steps 0..40 hold keys at 0, 4, ..., 40; step 6 lies halfway
        // between keys 4 and 8.
        for i in 0..41 {
            ghost.record(sample(500 + i), None);
            let pose = ghost.pose_nodes(&mut out).unwrap();
            assert_eq!(pose, sample(i));
            assert_eq!(out.quad.len(), 3);
            assert_eq!(out.rider.len(), 2);
            assert!((out.quad[1][12] - i as f32).abs() < 1e-4, "{i}");
            assert!((out.rider[0][13] + i as f32).abs() < 1e-4, "{i}");
            assert!((out.rider_offset[14] - 2. * i as f32).abs() < 1e-4, "{i}");
        }
        ghost.record(sample(0), None);
        assert!(ghost.pose_nodes(&mut out).is_none());
    }

    #[test]
    fn a_lap_without_nodes_replays_but_does_not_draw() {
        let mut ghost = Ghost::default();
        for i in 0..20 {
            ghost.record(sample(i), None);
        }
        assert!(ghost.lap(2., true));
        ghost.record(sample(0), None);
        assert_eq!(ghost.pose(), Some(sample(0)));
        assert!(ghost.pose_nodes(&mut PoseNodes::default()).is_none());
        // A step without nodes leaves the key to the next: this lap's
        // first step had none, so its first key is step 1 and step 0 holds
        // it.
        record_lap(&mut ghost, 12, 100);
        assert!(ghost.lap(1., true));
        ghost.record(sample(0), None);
        let mut out = PoseNodes::default();
        assert_eq!(ghost.pose_nodes(&mut out), Some(sample(0)));
        assert!((out.quad[1][12] - 100.).abs() < 1e-4);
        ghost.record(sample(0), None);
        assert_eq!(ghost.pose_nodes(&mut out), Some(sample(100)));
        assert!((out.quad[1][12] - 100.).abs() < 1e-4);
    }

    #[test]
    fn a_rival_lap_replays_in_the_player_s_first_pose_until_beaten() {
        let mut ghost = Ghost::default();
        ghost.set_rival(0., vec![sample(1)]);
        ghost.set_rival(2., Vec::new());
        assert!(ghost.frame().is_none(), "no lap, no rival");
        ghost.set_rival(2., (0..120).map(|i| sample(300 + i)).collect());
        assert_eq!(ghost.best(), Some(2.));
        let mut out = PoseNodes::default();
        // It replays from the first step; it draws once the player's first
        // key gives it a pose, and holds that pose.
        ghost.record(sample(0), None);
        assert_eq!(ghost.pose(), Some(sample(300)));
        assert!(ghost.pose_nodes(&mut out).is_none());
        record_lap(&mut ghost, 119, 1);
        assert_eq!(ghost.pose_nodes(&mut out), Some(sample(419)));
        assert!((out.quad[1][12] - 1.).abs() < 1e-4, "the first key's pose");
        assert!((out.rider_offset[14] - 2.).abs() < 1e-4);
        // A slower lap keeps the rival; it restarts with the next lap.
        assert!(!ghost.lap(2.5, true));
        record_lap(&mut ghost, 60, 1000);
        assert_eq!(ghost.pose_nodes(&mut out), Some(sample(359)));
        assert!((out.quad[1][12] - 1.).abs() < 1e-4);
        // A faster lap replaces it with the player's own recording.
        assert!(ghost.lap(1., true));
        assert_eq!(ghost.best(), Some(1.));
        record_lap(&mut ghost, 10, 2000);
        assert_eq!(ghost.pose_nodes(&mut out), Some(sample(1009)));
        assert!((out.quad[1][12] - 1009.).abs() < 0.5, "{}", out.quad[1][12]);
        // The buffers the rival left are recorded into as any lap's.
        assert!(!ghost.lap(3., true));
        record_lap(&mut ghost, 8, 3000);
        assert!(ghost.lap(0.5, true));
        ghost.record(sample(0), None);
        assert_eq!(ghost.pose_nodes(&mut out), Some(sample(3000)));
    }

    #[test]
    fn changed_node_counts_break_the_lap() {
        let mut ghost = Ghost::default();
        record_lap(&mut ghost, 8, 0);
        let (quad, rider, offset) = nodes(9);
        ghost.record(
            sample(9),
            Some(Nodes {
                quad: &quad[..2],
                rider: &rider,
                rider_offset: offset,
            }),
        );
        assert!(ghost.lap(1., true));
        ghost.record(sample(0), None);
        assert!(ghost.pose().is_some());
        assert!(ghost.pose_nodes(&mut PoseNodes::default()).is_none());
    }

    #[test]
    fn opacity_is_clamped_and_defaults_to_ghost_opacity() {
        assert_eq!(Ghost::default().opacity(), OPACITY);
        assert_eq!(Ghost::new(2.).opacity(), 1.);
        assert_eq!(Ghost::new(f32::NAN).opacity(), OPACITY);
    }

    #[test]
    fn rider_offset_recovers_the_rider_world() {
        // A quad turned 90 degrees about Y at (3, 4, 5).
        let quad: Matrix = [
            0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 3., 4., 5., 1.,
        ];
        let mut rider = quad;
        rider[12] += 1.5;
        rider[13] -= 2.;
        let offset = rider_offset(&quad, &rider);
        let back = multiply(&quad, &offset);
        for (a, b) in back.iter().zip(&rider) {
            assert!((a - b).abs() < 1e-5);
        }
        assert_eq!(rider_offset(&quad, &quad), IDENTITY);
    }
}
