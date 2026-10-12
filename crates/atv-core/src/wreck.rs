//! USA v2.00 crash decisions (001e8690) and recovery clock (001e5260).
//! Impact input is the original local collision velocity delta, not body speed.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

/// Eight horizontal20-foot clearance rays from001ea4d0. The native ninth
/// downward ray adjusts platform height; terrain-supported checkpoints use
/// their existing wheel-supported height. Scene category/water tests are separate.
pub fn checkpoint_clearance_rays() -> crate::collision::CollisionRayTree {
    use crate::collision::{CollisionRay, CollisionRayTree, RayTreeNode};
    let identity = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let rays = (0..8)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 8.0;
            CollisionRay {
                start: [0., 3., 0.],
                end: [angle.sin() * 20., 3., angle.cos() * 20.],
            }
        })
        .collect();
    let nodes = (0..7)
        .map(|i| RayTreeNode {
            bound_measure: 800.,
            center: [0., 3., 0.],
            half_extents: [20., 0., 20.],
            children: if i < 3 {
                [(i * 2 + 1) as u16, (i * 2 + 2) as u16]
            } else {
                [0x8000 + ((i - 3) * 2) as u16, 0x8001 + ((i - 3) * 2) as u16]
            },
        })
        .collect();
    CollisionRayTree {
        transform: identity,
        query_capacity: 8,
        nodes,
        rays,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum Kind {
    Front = 0,
    Left = 1,
    Right = 2,
    Back = 3,
    Vertical = 4,
}
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub impact_limit: [f32; 3],
    pub ground_angles: [f32; 3],
    pub air_angles: [f32; 3],
}
impl Config {
    /// Profile upgrades are supplied explicitly; zero is the base ATV tuning.
    pub fn from_physics(data: &PhysicsData, stability_upgrade: f32) -> Result<Self> {
        let get = |name| data.first_scalar("Wrecks", name);
        let radians = |name| get(name).map(|v| v * 0.017_453_29);
        if !stability_upgrade.is_finite() || stability_upgrade < 0.0 {
            return Err("invalid stability upgrade".into());
        }
        Ok(Self {
            impact_limit: [
                get("LateralFpsLimit")?,
                get("VerticalFpsLimit")? + get("TweakVerticalFpsLimit")? * stability_upgrade,
                get("LongitudinalFpsLimit")?,
            ],
            ground_angles: [
                radians("PitchUpLimit")?,
                radians("PitchDownLimit")?,
                radians("RollLimit")?,
            ],
            air_angles: [
                radians("AirPitchUpLimit")?,
                radians("AirPitchDownLimit")?,
                radians("AirRollLimit")?,
            ],
        })
    }
    pub fn evaluate(self, input: Input) -> Option<Kind> {
        use Kind::*;
        let [x, y, z] = input.local_impact;
        let [lateral, vertical, longitudinal] = self.impact_limit;
        // Preserve native branch precedence when multiple limits are crossed.
        let impact = if z > longitudinal {
            Some(Back)
        } else if z < -longitudinal {
            Some(Front)
        } else if x > lateral {
            Some(Left)
        } else if x < -lateral {
            Some(Right)
        } else if y > vertical * input.vertical_limit_scale {
            Some(Vertical)
        } else if input.rider_contact {
            Some(if input.forward_speed > 0.0 {
                Back
            } else {
                Front
            })
        } else {
            None
        };
        if input.wheel_contact && !input.stunt_landing_safe {
            return Some(Vertical);
        }
        let [up, down, roll] = if !input.wheel_contact && !input.body_contact {
            self.air_angles
        } else {
            self.ground_angles
        };
        let angle = if input.pitch > up {
            Some(Back)
        } else if input.pitch < -down {
            Some(Front)
        } else if input.roll > roll {
            Some(Left)
        } else if input.roll < -roll {
            Some(Right)
        } else {
            None
        };
        // Native calls the entry routine again if an angle also crosses a limit;
        // that later decision is the last visible selected wreck type.
        angle.or(impact)
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Input {
    pub local_impact: [f32; 3],
    /// 1.25 for original modes0/1/2; 1.0 otherwise. Caller owns game-mode mapping.
    pub vertical_limit_scale: f32,
    pub pitch: f32,
    pub roll: f32,
    pub wheel_contact: bool,
    pub body_contact: bool,
    pub rider_contact: bool,
    pub forward_speed: f32,
    pub stunt_landing_safe: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Phase {
    Riding,
    Falling,
    Recovering,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct State {
    pub phase: Phase,
    pub kind: Option<Kind>,
    pub timer: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            phase: Phase::Riding,
            kind: None,
            timer: 0.0,
        }
    }
}
impl State {
    pub fn begin(&mut self, kind: Kind) {
        self.phase = Phase::Falling;
        self.kind = Some(kind);
        self.timer = 0.0;
    }
    /// Constructor +0x7f8 is1.8s. Recovery waits for the wrecked rider's contact,
    /// not for the ATV wheels. Returns true when the native reset is requested.
    pub fn step(&mut self, dt: f32, rider_grounded: bool) -> bool {
        let delay = f32::from_bits(0x3fe66666);
        match self.phase {
            Phase::Riding => false,
            Phase::Falling => {
                self.timer += dt;
                if self.timer > delay - 1.0 {
                    self.phase = Phase::Recovering;
                    self.timer = 0.0;
                }
                false
            }
            Phase::Recovering => {
                if self.timer != 0.0 || rider_grounded {
                    self.timer += dt;
                }
                self.timer > delay
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            impact_limit: [19.0, 84.0, 21.0],
            ground_angles: [1.4, 1.4, 1.3],
            air_angles: [1.5, 1.2, 1.4],
        }
    }
    fn input() -> Input {
        Input {
            local_impact: [0.; 3],
            vertical_limit_scale: 1.0,
            pitch: 0.,
            roll: 0.,
            wheel_contact: true,
            body_contact: false,
            rider_contact: false,
            forward_speed: 0.,
            stunt_landing_safe: true,
        }
    }
    #[test]
    fn normal_landing_is_safe_and_held_trick_is_not() {
        let mut i = input();
        assert_eq!(config().evaluate(i), None);
        i.stunt_landing_safe = false;
        assert_eq!(config().evaluate(i), Some(Kind::Vertical));
        i.wheel_contact = false;
        assert_eq!(config().evaluate(i), None);
    }
    #[test]
    fn strict_thresholds_and_native_branch_order() {
        let mut i = input();
        i.local_impact = [19., 84., 21.];
        assert_eq!(config().evaluate(i), None);
        i.local_impact = [20., 100., 22.];
        assert_eq!(config().evaluate(i), Some(Kind::Back));
        i.pitch = -1.41;
        assert_eq!(config().evaluate(i), Some(Kind::Front));
        i.stunt_landing_safe = false;
        assert_eq!(config().evaluate(i), Some(Kind::Vertical));
    }
    #[test]
    fn recovery_waits_for_rider_contact_then_finishes_even_if_contact_changes() {
        let mut s = State::default();
        s.begin(Kind::Front);
        for _ in 0..49 {
            assert!(!s.step(1. / 60., false));
        }
        assert_eq!(s.phase, Phase::Recovering);
        for _ in 0..180 {
            assert!(!s.step(1. / 60., false));
        }
        assert_eq!(s.timer, 0.);
        assert!(!s.step(1. / 60., true));
        for _ in 0..106 {
            assert!(!s.step(1. / 60., false));
        }
        assert!(s.step(2. / 60., false));
    }
}
