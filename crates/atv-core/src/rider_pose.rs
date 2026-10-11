//! Normal riding animation selection from USA v2.00 routines 0x002d8ba0/8e50.
//! This plans original clips and blend order; tricks, wrecks, events and look-target
//! selection remain separate gameplay state machines.
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
