//! Normal riding animation selection from USA v2.00 routines 0x002d8ba0/8e50.
//! This plans original clips and blend order; tricks, wrecks, events and look-target
//! selection remain separate gameplay state machines.
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Mode {
    Reverse,
    Turn,
    Lean,
    Air,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Clip {
    TurnFwd,
    TurnMid,
    TurnBck,
    LeanFwd,
    LeanMid,
    LeanBck,
    LeanFwdD,
    LeanMidD,
    LeanBckD,
    CrosFwd,
    CrosMid,
    CrosBck,
    Reverse,
    LookLr,
}
impl Clip {
    pub const ALL: [Self; 14] = [
        Self::TurnFwd,
        Self::TurnMid,
        Self::TurnBck,
        Self::LeanFwd,
        Self::LeanMid,
        Self::LeanBck,
        Self::LeanFwdD,
        Self::LeanMidD,
        Self::LeanBckD,
        Self::CrosFwd,
        Self::CrosMid,
        Self::CrosBck,
        Self::Reverse,
        Self::LookLr,
    ];
    /// Catalog suffix; its original rider-specific M_/F_ prefix is separate.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::TurnFwd => "TurnFwd",
            Self::TurnMid => "TurnMid",
            Self::TurnBck => "TurnBck",
            Self::LeanFwd => "LeanFwd",
            Self::LeanMid => "LeanMid",
            Self::LeanBck => "LeanBck",
            Self::LeanFwdD => "LeanFwdD",
            Self::LeanMidD => "LeanMidD",
            Self::LeanBckD => "LeanBckD",
            Self::CrosFwd => "CrosFwd",
            Self::CrosMid => "CrosMid",
            Self::CrosBck => "CrosBck",
            Self::Reverse => "Reverse",
            Self::LookLr => "Look_LR",
        }
    }
}

/// Original normal-riding controller evaluation. Tricks, IK, look-target
/// acquisition and the crouch signal are intentionally supplied separately.
pub struct Motion {
    pub animation: crate::animation::Animation,
    pub sampling: crate::animation::Sampling,
}
pub struct Animator {
    motions: Vec<Motion>,
    state: State,
    current: crate::animation::Channels,
    saved: crate::animation::Channels,
    head: usize,
}
impl Animator {
    /// Motions must follow Clip::ALL. Reject controller layouts that the original
    /// indexed pair/four-clip blend cannot safely combine.
    pub fn new(motions: Vec<Motion>, nodes: &[crate::mesh::Node]) -> Result<Self> {
        if motions.len() != Clip::ALL.len() {
            return Err("normal riding requires fourteen original motions".into());
        }
        let layout = |motion: &Motion| -> Result<(Vec<usize>, Vec<usize>)> {
            let a = &motion.animation;
            Ok((
                a.translations
                    .iter()
                    .map(|t| a.model_bone(t.bone, nodes))
                    .collect::<Result<_>>()?,
                a.rotations
                    .iter()
                    .map(|t| a.model_bone(t.bone, nodes))
                    .collect::<Result<_>>()?,
            ))
        };
        let expected = layout(&motions[0])?;
        for indices in [&expected.0, &expected.1] {
            if indices
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != indices.len()
            {
                return Err("duplicate normal riding controllers".into());
            }
        }
        // Reverse is sampled alone and may use a different controller order.
        for motion in &motions[..12] {
            if layout(motion)? != expected {
                return Err("incompatible normal riding controller order".into());
            }
        }
        let initial = &motions[Clip::TurnMid as usize];
        let current = initial.animation.sample_channels(
            nodes,
            initial.animation.duration * 0.5,
            initial.sampling,
        )?;
        let head = nodes
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case("Head"))
            .ok_or("rider Head node is missing")?;
        Ok(Self {
            motions,
            state: State::default(),
            saved: current.clone(),
            current,
            head,
        })
    }
    pub fn pose(&self, nodes: &[crate::mesh::Node]) -> Result<crate::animation::Pose> {
        self.current.pose(nodes)
    }
    pub fn reset(&mut self, nodes: &[crate::mesh::Node]) -> Result<()> {
        let initial = &self.motions[Clip::TurnMid as usize];
        self.current = initial.animation.sample_channels(
            nodes,
            initial.animation.duration * 0.5,
            initial.sampling,
        )?;
        self.saved = self.current.clone();
        self.state = State::default();
        Ok(())
    }
    pub fn step(
        &mut self,
        nodes: &[crate::mesh::Node],
        input: Input,
        dt: f32,
    ) -> Result<(Plan, crate::animation::Pose)> {
        let plan = self.state.step(input, dt)?;
        if plan.snapshot {
            self.saved = self.current.clone();
        }
        let first = plan.blend.clips()[0];
        let samples = plan.samples(self.motions[first as usize].animation.duration)?;
        let mut sampled = Vec::new();
        for sample in samples {
            let motion = &self.motions[sample.clip as usize];
            sampled.push(
                motion
                    .animation
                    .sample_channels(nodes, sample.time, motion.sampling)?,
            );
        }
        let target = match plan.blend {
            Blend::Single(_) => sampled.remove(0),
            Blend::Pair { first_weight, .. } => pair(&sampled[0], &sampled[1], first_weight)?,
            Blend::Four {
                first_weight,
                pair_weight,
                ..
            } => pair(
                &pair(&sampled[0], &sampled[1], first_weight)?,
                &pair(&sampled[2], &sampled[3], first_weight)?,
                pair_weight,
            )?,
        };
        self.apply(&target, plan.snapshot_weight, None)?;
        if let Some(phase) = plan.head_look_phase {
            let motion = &self.motions[Clip::LookLr as usize];
            let look = motion.animation.sample_channels(
                nodes,
                phase * motion.animation.duration,
                motion.sampling,
            )?;
            self.apply(&look, plan.snapshot_weight, Some(self.head))?;
        }
        Ok((plan, self.pose(nodes)?))
    }
    fn apply(
        &mut self,
        target: &crate::animation::Channels,
        weight: f32,
        only: Option<usize>,
    ) -> Result<()> {
        for index in 0..self.current.translations.len() {
            if only.is_some_and(|node| node != index) {
                continue;
            }
            if let Some(new) = target.translations[index] {
                let old = self.saved.translations[index]
                    .ok_or("rider snapshot translation is missing")?;
                self.current.translations[index] = Some(if weight > 0. {
                    std::array::from_fn(|i| new[i] + weight * (old[i] - new[i]))
                } else {
                    new
                });
            }
            if let Some(new) = target.rotations[index] {
                let old =
                    self.saved.rotations[index].ok_or("rider snapshot rotation is missing")?;
                self.current.rotations[index] = Some(if weight > 0. {
                    crate::animation::blend_rotation(new, old, weight)
                } else {
                    new
                });
            }
        }
        Ok(())
    }
}
fn pair(
    a: &crate::animation::Channels,
    b: &crate::animation::Channels,
    weight: f32,
) -> Result<crate::animation::Channels> {
    if a.translations.len() != b.translations.len() || a.rotations.len() != b.rotations.len() {
        return Err("rider blend channel count mismatch".into());
    }
    let mut result = a.clone();
    for (index, channel) in result.translations.iter_mut().enumerate() {
        if let Some(first) = *channel {
            let second = b.translations[index].ok_or("rider blend translation is missing")?;
            *channel = Some(std::array::from_fn(|i| {
                second[i] - weight * (second[i] - first[i])
            }));
        }
    }
    for (index, channel) in result.rotations.iter_mut().enumerate() {
        if let Some(first) = *channel {
            let second = b.rotations[index].ok_or("rider blend rotation is missing")?;
            *channel = Some(crate::animation::blend_rotation(second, first, weight));
        }
    }
    Ok(result)
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Blend {
    Single(Clip),
    /// Interpolate second toward first, preserving this order.
    Pair {
        clips: [Clip; 2],
        first_weight: f32,
    },
    /// Blend (1,2) and (3,4) by first_weight, then second pair toward first
    /// pair by pair_weight. Flattening weights changes quaternion blending.
    Four {
        clips: [Clip; 4],
        first_weight: f32,
        pair_weight: f32,
    },
}
impl Blend {
    pub fn clips(&self) -> &[Clip] {
        match self {
            Self::Single(c) => std::slice::from_ref(c),
            Self::Pair { clips, .. } => clips,
            Self::Four { clips, .. } => clips,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// Smoothed original joystick axes, not raw browser axes.
    pub axes: [f32; 2],
    pub local_forward_speed: f32,
    pub stand_up_speed: f32,
    pub wheel_contact: bool,
    pub reverse_gear: bool,
    /// Original actor +0x860 gate; its wider gameplay meaning is not assumed.
    pub reverse_override: bool,
    pub crouch: f32,
    /// Original look-target controller output. Constructor value is 0.5.
    pub look_phase: f32,
    pub force_snapshot: bool,
    pub suppress_snapshot: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct Plan {
    pub mode: Mode,
    pub blend: Blend,
    pub phase: f32,
    pub snapshot: bool,
    /// Weight of the saved old pose, applied after the current clip blend.
    pub snapshot_weight: f32,
    /// Turn mode's final Look_LR overlay affects only the Head node.
    pub head_look_phase: Option<f32>,
}
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub clip: Clip,
    pub time: f32,
}
impl Plan {
    /// All body clips use FIRST clip duration × phase, even if their durations
    /// differ. The caller must reject incompatible controller layouts.
    pub fn samples(&self, first_duration: f32) -> Result<Vec<Sample>> {
        if !first_duration.is_finite() || first_duration < 0.0 {
            return Err("invalid rider motion duration".into());
        }
        let time = first_duration * self.phase;
        Ok(self
            .blend
            .clips()
            .iter()
            .map(|&clip| Sample { clip, time })
            .collect())
    }
}
#[derive(Debug, Clone, Copy)]
pub struct State {
    pub previous_mode: Mode,
    pub snapshot_weight: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            previous_mode: Mode::Air,
            snapshot_weight: 0.0,
        }
    }
}
fn unit(value: f32) -> f32 {
    // Native branch order also canonicalizes negative zero to positive zero.
    if value >= 1.0 {
        1.0
    } else if value > 0.0 {
        value
    } else {
        0.0
    }
}
impl State {
    pub fn step(&mut self, input: Input, dt: f32) -> Result<Plan> {
        if [
            input.axes[0],
            input.axes[1],
            input.local_forward_speed,
            input.stand_up_speed,
            input.crouch,
            input.look_phase,
            self.snapshot_weight,
            dt,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || dt < 0.0
        {
            return Err("invalid rider pose input".into());
        }
        let mode = if input.reverse_gear || input.reverse_override {
            Mode::Reverse
        } else if !input.wheel_contact {
            Mode::Air
        } else if input.local_forward_speed < input.stand_up_speed {
            Mode::Turn
        } else {
            Mode::Lean
        };
        self.snapshot_weight = (self.snapshot_weight - dt * 4.0).clamp(0.0, 1.0);
        let snapshot =
            !input.suppress_snapshot && (mode != self.previous_mode || input.force_snapshot);
        if snapshot {
            self.snapshot_weight = 1.0;
            self.previous_mode = mode;
        }
        let phase = unit(input.axes[0] * -0.5 + 0.5);
        let fore_aft = (input.axes[1] + 1.0) * 0.5;
        let back = fore_aft >= 0.5;
        let first_weight = unit(if back {
            (fore_aft - 0.5) * 2.0
        } else {
            fore_aft * 2.0
        });
        use Clip::*;
        let blend = match mode {
            Mode::Reverse => Blend::Single(Reverse),
            Mode::Turn => Blend::Pair {
                clips: if back {
                    [TurnBck, TurnMid]
                } else {
                    [TurnMid, TurnFwd]
                },
                first_weight,
            },
            Mode::Lean => Blend::Four {
                clips: if back {
                    [LeanBck, LeanMid, LeanBckD, LeanMidD]
                } else {
                    [LeanMid, LeanFwd, LeanMidD, LeanFwdD]
                },
                first_weight,
                pair_weight: unit(input.crouch),
            },
            Mode::Air => Blend::Pair {
                clips: if back {
                    [CrosBck, CrosMid]
                } else {
                    [CrosMid, CrosFwd]
                },
                first_weight,
            },
        };
        Ok(Plan {
            mode,
            blend,
            phase,
            snapshot,
            snapshot_weight: self.snapshot_weight,
            head_look_phase: if mode == Mode::Turn {
                Some(input.look_phase)
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<Motion>, Vec<crate::mesh::Node>) {
        use crate::animation::{Animation, RotationKey, Sampling, Track, TranslationKey};
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let nodes = ["Root", "Head", "Hand"]
            .iter()
            .enumerate()
            .map(|(i, name)| crate::mesh::Node {
                name: (*name).into(),
                parent: if i == 0 { -1 } else { 0 },
                local_matrix: identity,
                world_matrix: identity,
                bounds: [0.; 7],
            })
            .collect();
        let motions = Clip::ALL
            .iter()
            .map(|clip| Motion {
                sampling: Sampling::default(),
                animation: Animation {
                    version: 2,
                    frame_count: 2,
                    duration: 1.,
                    bone_names: vec![],
                    translation_kind: 0,
                    translations: if *clip == Clip::LookLr {
                        vec![]
                    } else {
                        vec![Track {
                            bone: 0,
                            keys: vec![TranslationKey {
                                time: 0.,
                                value: [0.; 3],
                                tangents: None,
                            }],
                        }]
                    },
                    rotations: (0..3)
                        .filter(|bone| *clip != Clip::LookLr || *bone == 1)
                        .map(|bone| Track {
                            bone,
                            keys: vec![RotationKey {
                                time: 0.,
                                packed_angle: if *clip == Clip::LookLr { 1024 } else { 0 },
                                packed_axis: [0, 32767, 0],
                            }],
                        })
                        .collect(),
                },
            })
            .collect();
        (motions, nodes)
    }
    #[test]
    fn head_overlay_is_masked_and_reset_restores_initial_pose() {
        let (motions, nodes) = fixture();
        let mut animator = Animator::new(motions, &nodes).unwrap();
        let initial = animator.pose(&nodes).unwrap();
        let (_, first) = animator.step(&nodes, input(), 1. / 60.).unwrap();
        assert_eq!(
            first.local_matrices, initial.local_matrices,
            "mode change snapshots before applying look"
        );
        let (_, next) = animator.step(&nodes, input(), 0.3).unwrap();
        assert_eq!(next.local_matrices[0], initial.local_matrices[0]);
        assert_eq!(next.local_matrices[2], initial.local_matrices[2]);
        assert!(next.local_matrices[1][0].abs() < 0.00001);
        animator.reset(&nodes).unwrap();
        assert_eq!(
            animator.pose(&nodes).unwrap().local_matrices,
            initial.local_matrices
        );
    }
    #[test]
    fn indexed_blends_reject_reordered_controllers_but_single_reverse_allows_them() {
        let (mut motions, nodes) = fixture();
        motions[Clip::Reverse as usize]
            .animation
            .rotations
            .swap(0, 1);
        assert!(Animator::new(motions, &nodes).is_ok());
        let (mut motions, nodes) = fixture();
        motions[Clip::LeanFwd as usize]
            .animation
            .rotations
            .swap(0, 1);
        assert!(Animator::new(motions, &nodes).is_err());
    }
    fn input() -> Input {
        Input {
            axes: [0.0; 2],
            local_forward_speed: 0.0,
            stand_up_speed: 10.0,
            wheel_contact: true,
            reverse_gear: false,
            reverse_override: false,
            crouch: 0.5,
            look_phase: 0.5,
            force_snapshot: false,
            suppress_snapshot: false,
        }
    }
    #[test]
    fn transition_saves_pose_then_fades_without_restarting() {
        let mut state = State::default();
        let p = state.step(input(), 1.0 / 60.0).unwrap();
        assert!(p.snapshot);
        assert_eq!(p.snapshot_weight, 1.0);
        let p = state.step(input(), 0.125).unwrap();
        assert!(!p.snapshot);
        assert_eq!(p.snapshot_weight, 0.5);
        assert_eq!(p.head_look_phase, Some(0.5));
        let p = state.step(input(), 0.5).unwrap();
        assert_eq!(p.snapshot_weight, 0.0);
    }
    #[test]
    fn reverse_precedes_air_and_lean_threshold_is_inclusive() {
        let mut state = State::default();
        let mut i = input();
        i.local_forward_speed = 10.0;
        assert_eq!(state.step(i, 0.0).unwrap().mode, Mode::Lean);
        i.wheel_contact = false;
        assert_eq!(state.step(i, 0.0).unwrap().mode, Mode::Air);
        i.reverse_override = true;
        assert_eq!(state.step(i, 0.0).unwrap().mode, Mode::Reverse);
    }
    #[test]
    fn four_clips_preserve_pair_order_and_shared_absolute_time() {
        let mut i = input();
        i.local_forward_speed = 20.0;
        i.axes = [-0.5, -0.25];
        i.crouch = 0.75;
        let p = State::default().step(i, 0.0).unwrap();
        assert_eq!(
            p.blend,
            Blend::Four {
                clips: [Clip::LeanMid, Clip::LeanFwd, Clip::LeanMidD, Clip::LeanFwdD],
                first_weight: 0.75,
                pair_weight: 0.75
            }
        );
        assert!(p.samples(2.0).unwrap().iter().all(|s| s.time == 1.5));
    }
    #[test]
    fn suppressed_transition_does_not_consume_mode_change() {
        let mut state = State::default();
        let mut i = input();
        i.suppress_snapshot = true;
        assert!(!state.step(i, 0.0).unwrap().snapshot);
        assert_eq!(state.previous_mode, Mode::Air);
        i.suppress_snapshot = false;
        assert!(state.step(i, 0.0).unwrap().snapshot);
    }
}
