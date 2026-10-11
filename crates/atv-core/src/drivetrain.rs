//! Authored reconstruction of the exact-build engine update (0x001f5b40).
//! This is a per-vehicle-update calculation, not a time-integrating wheel solver.
//! Ordinary finite binary32 behavior is implemented; PS2 FPU edge semantics and
//! timing fidelity still require original execution comparisons.
use crate::{Result, physics_data::PhysicsData};

const RPM_PER_RADIAN_SECOND: f32 = f32::from_bits(0x4118_c9eb);
const MPH_TO_FEET_SECOND: f32 = f32::from_bits(0x3fbb_bb84);
const HORSEPOWER_TO_TORQUE_RPM: f32 = f32::from_bits(0x45a4_2000);
const REVERSE_SCALE: f32 = f32::from_bits(0x3ecc_cccd);
const HIGH_RPM_FRACTION: f32 = f32::from_bits(0x3f66_6666);

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Authored (RPM, horsepower) samples, with strictly increasing RPM.
    pub power_curve: Vec<(f32, f32)>,
    pub gear_ratios: Vec<f32>,
    pub final_ratio: f32,
    pub transmission_loss: f32,
    pub ramp_up_filter: f32,
    pub ramp_down_filter: f32,
    pub ramp_down_speed: f32,
    pub top_speed: f32,
}

impl EngineConfig {
    pub fn from_physics(data: &PhysicsData) -> Result<Self> {
        let config = Self {
            power_curve: data
                .power_curve()?
                .into_iter()
                .map(|p| (p.rpm, p.horsepower))
                .collect(),
            gear_ratios: data.gear_ratios()?,
            final_ratio: data.first_scalar("Engine", "FinalGearRatio")?,
            transmission_loss: data.first_scalar("Engine", "TransmissionLoss")?,
            ramp_up_filter: data.first_scalar("Engine", "RampUpFilter")?,
            ramp_down_filter: data.first_scalar("Engine", "RampDownFilter")?,
            ramp_down_speed: data.first_scalar("Engine", "RampDownSpeed")? * MPH_TO_FEET_SECOND,
            top_speed: data.first_scalar("Engine", "TopSpeed")? * MPH_TO_FEET_SECOND,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !(2..=256).contains(&self.power_curve.len())
            || !(1..=256).contains(&self.gear_ratios.len())
        {
            return Err("unsupported engine curve or gear count".into());
        }
        let mut previous = 0.0;
        for &(rpm, hp) in &self.power_curve {
            if !rpm.is_finite() || !hp.is_finite() || rpm <= previous || hp < 0.0 {
                return Err(
                    "engine curve requires increasing positive RPM and finite nonnegative HP"
                        .into(),
                );
            }
            previous = rpm;
        }
        if self.gear_ratios.iter().any(|g| !g.is_finite() || *g <= 0.0)
            || !self.final_ratio.is_finite()
            || self.final_ratio <= 0.0
            || ![
                self.transmission_loss,
                self.ramp_up_filter,
                self.ramp_down_filter,
            ]
            .iter()
            .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            || !self.ramp_down_speed.is_finite()
            || !self.top_speed.is_finite()
        {
            return Err("invalid engine gearing, filters, or speed limits".into());
        }
        Ok(())
    }

    /// Original lookup uses the upper knot and a reverse interpolation fraction.
    /// Values below/above the authored RPM range use the first/last sample.
    pub fn horsepower(&self, rpm: f32) -> Result<f32> {
        self.validate()?;
        if !rpm.is_finite() {
            return Err("non-finite engine RPM".into());
        }
        Ok(self.sample_power(rpm))
    }

    fn sample_power(&self, rpm: f32) -> f32 {
        let upper = self.power_curve.partition_point(|&(x, _)| x <= rpm);
        if upper == 0 {
            return self.power_curve[0].1;
        }
        if upper == self.power_curve.len() {
            return self.power_curve[upper - 1].1;
        }
        let (x1, y1) = self.power_curve[upper];
        let (x0, y0) = self.power_curve[upper - 1];
        let reverse_fraction = (x1 - rpm) / (x1 - x0);
        y1 - reverse_fraction * (y1 - y0)
    }

    fn peak_rpm(&self) -> f32 {
        // Equality replaces the previous peak: the last tied sample wins.
        self.power_curve
            .iter()
            .fold(
                (0.0, 0.0),
                |best, &(rpm, hp)| {
                    if hp >= best.1 { (rpm, hp) } else { best }
                },
            )
            .0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveDirection {
    Reverse,
    Neutral,
    Forward,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EngineModifiers {
    /// Engine +0x9c; populated by original garage/profile loading.
    pub horsepower_adjustment: f32,
    /// Engine +0xa0, already in feet/second.
    pub speed_adjustment: f32,
    /// Engine +0x68; effect source/lifecycle is supplied by the caller.
    pub effect: f32,
    pub effect_horsepower: f32,
    pub effect_speed: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct EngineInput {
    pub throttle: f32,
    pub direction: DriveDirection,
    /// Signed body speed supplied by the original outer vehicle update.
    pub speed_ft_per_second: f32,
    /// Original outer update enables automatic shifts when either rear wheel contacts.
    pub allow_shifting: bool,
    pub modifiers: EngineModifiers,
}

#[derive(Debug, Clone, Copy)]
pub struct DrivenWheel {
    /// Wheel +0x1e8, radians/second.
    pub angular_speed: f32,
    /// Wheel +0x224; any nonzero value disengages drive for this update.
    pub brake: f32,
    /// Material table record +0xc, selected by the wheel's material byte.
    pub surface_drive_scale: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct EngineState {
    /// One-based current gear.
    pub gear: usize,
    pub rpm: f32,
    pub filtered_throttle: f32,
    pub previous_direction: DriveDirection,
    pub mean_wheel_speed: f32,
    pub torque_per_wheel: f32,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            gear: 1,
            rpm: 0.0,
            filtered_throttle: 0.0,
            previous_direction: DriveDirection::Forward,
            mean_wheel_speed: 0.0,
            torque_per_wheel: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct EngineOutput {
    pub state: EngineState,
    pub effective_direction: DriveDirection,
    /// Add these to the driven wheels' existing torque accumulators.
    pub wheel_torques: Vec<f32>,
}

pub struct Drivetrain {
    config: EngineConfig,
    pub state: EngineState,
}

impl Drivetrain {
    pub fn new(config: EngineConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            state: EngineState::default(),
        })
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Runs once per outer vehicle update, before wheel forces and rigid-body substeps.
    /// No invented delta-time correction is applied to the recovered throttle filter.
    pub fn step(&mut self, input: EngineInput, wheels: &[DrivenWheel]) -> Result<EngineOutput> {
        let m = input.modifiers;
        if wheels.len() > 64
            || ![
                input.throttle,
                input.speed_ft_per_second,
                m.horsepower_adjustment,
                m.speed_adjustment,
                m.effect,
                m.effect_horsepower,
                m.effect_speed,
                self.state.filtered_throttle,
                self.state.rpm,
                self.state.mean_wheel_speed,
                self.state.torque_per_wheel,
            ]
            .iter()
            .all(|x| x.is_finite())
            || wheels.iter().any(|w| {
                ![w.angular_speed, w.brake, w.surface_drive_scale]
                    .iter()
                    .all(|x| x.is_finite())
            })
            || self.state.gear == 0
            || self.state.gear > self.config.gear_ratios.len()
        {
            return Err("invalid engine state or input".into());
        }
        let mut next = self.state;
        let mut direction = input.direction;
        if !wheels.is_empty() {
            let config = &self.config;
            let share = 1.0 / wheels.len() as f32;
            if wheels.iter().any(|w| w.brake != 0.0) {
                direction = DriveDirection::Neutral;
            }
            let min_rpm = config.power_curve[0].0;
            let max_rpm = config.power_curve[config.power_curve.len() - 1].0;
            let peak_rpm = config.peak_rpm();
            if direction == DriveDirection::Neutral {
                next.rpm = input.throttle * peak_rpm;
            } else {
                next.mean_wheel_speed =
                    wheels.iter().fold(0.0, |sum, w| sum + w.angular_speed) * share;
                let wheel_rpm = RPM_PER_RADIAN_SECOND * next.mean_wheel_speed;
                let geared_rpm = wheel_rpm * config.gear_ratios[next.gear - 1];
                next.rpm = (config.final_ratio * geared_rpm).abs();
            }
            next.rpm = next.rpm.min(max_rpm).max(min_rpm);
            if direction == DriveDirection::Neutral {
                next.filtered_throttle = input.throttle;
            } else if next.previous_direction == DriveDirection::Reverse
                && direction == DriveDirection::Forward
            {
                next.filtered_throttle = 0.0;
            } else {
                let filter = if input.throttle > 0.0 {
                    config.ramp_up_filter
                } else {
                    config.ramp_down_filter
                };
                let weighted_target = input.throttle * filter;
                next.filtered_throttle = weighted_target + next.filtered_throttle * (1.0 - filter);
            }
            let sampled = config.sample_power(next.rpm);
            let base_power = m.horsepower_adjustment + sampled;
            let mut horsepower = base_power + m.effect * m.effect_horsepower;
            if next.rpm > max_rpm * HIGH_RPM_FRACTION {
                horsepower -= 2.0 * m.horsepower_adjustment;
            } else {
                horsepower += m.horsepower_adjustment;
            }
            let torque = (HORSEPOWER_TO_TORQUE_RPM * horsepower) / next.rpm;
            let throttle_torque = torque * next.filtered_throttle.max(0.0);
            let start = (config.ramp_down_speed + m.speed_adjustment) + m.effect * m.effect_speed;
            let end = (config.top_speed + m.speed_adjustment) + m.effect * m.effect_speed;
            let taper = if end == start {
                0.0
            } else {
                (1.0 + -(input.speed_ft_per_second - start) / (end - start)).clamp(0.0, 1.0)
            };
            let engine_torque = throttle_torque * taper;
            if direction != DriveDirection::Forward {
                next.gear = 1;
            } else if input.allow_shifting
                && next.gear < config.gear_ratios.len()
                && (next.rpm >= peak_rpm || next.rpm >= max_rpm)
            {
                next.gear += 1;
            } else if input.allow_shifting && next.gear > 1 && next.rpm <= config.power_curve[1].0 {
                next.gear -= 1;
            }
            let delivered = if direction == DriveDirection::Reverse {
                REVERSE_SCALE * (-engine_torque * (1.0 - config.transmission_loss))
            } else {
                engine_torque * (1.0 - config.transmission_loss)
            };
            let geared = delivered * config.gear_ratios[next.gear - 1];
            next.torque_per_wheel = (config.final_ratio * geared) * share;
            if direction == DriveDirection::Neutral {
                next.torque_per_wheel = 0.0;
            }
        }
        next.previous_direction = direction;
        let wheel_torques: Vec<_> = wheels
            .iter()
            .map(|w| next.torque_per_wheel * w.surface_drive_scale)
            .collect();
        if ![
            next.rpm,
            next.filtered_throttle,
            next.mean_wheel_speed,
            next.torque_per_wheel,
        ]
        .iter()
        .chain(wheel_torques.iter())
        .all(|x| x.is_finite())
        {
            return Err("engine arithmetic exceeded finite binary32 range".into());
        }
        self.state = next;
        Ok(EngineOutput {
            state: next,
            effective_direction: direction,
            wheel_torques,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> EngineConfig {
        EngineConfig {
            power_curve: vec![(1000., 10.), (2000., 20.), (3000., 20.)],
            gear_ratios: vec![2., 1.],
            final_ratio: 2.,
            transmission_loss: 0.,
            ramp_up_filter: 1.,
            ramp_down_filter: 0.5,
            ramp_down_speed: 10.,
            top_speed: 20.,
        }
    }
    fn input() -> EngineInput {
        EngineInput {
            throttle: 1.,
            direction: DriveDirection::Forward,
            speed_ft_per_second: 0.,
            allow_shifting: true,
            modifiers: EngineModifiers::default(),
        }
    }
    fn wheels(speed: f32) -> [DrivenWheel; 2] {
        [DrivenWheel {
            angular_speed: speed,
            brake: 0.,
            surface_drive_scale: 1.,
        }; 2]
    }
    #[test]
    fn interpolation_clamps_and_uses_last_peak_tie() {
        let c = config();
        assert_eq!(c.horsepower(0.).unwrap(), 10.);
        assert_eq!(c.horsepower(1500.).unwrap(), 15.);
        assert_eq!(c.horsepower(2000.).unwrap(), 20.);
        assert_eq!(c.horsepower(4000.).unwrap(), 20.);
        assert_eq!(c.peak_rpm(), 3000.);
        assert!(c.horsepower(f32::NAN).is_err());
    }
    #[test]
    fn shift_uses_old_gear_for_rpm_and_new_gear_for_torque() {
        let mut engine = Drivetrain::new(config()).unwrap();
        let output = engine.step(input(), &wheels(100.)).unwrap();
        assert_eq!(output.state.rpm, 3000.);
        assert_eq!(output.state.gear, 2);
        assert_eq!(output.wheel_torques, vec![105040. / 3000.; 2]);
        let down = engine.step(input(), &wheels(0.)).unwrap();
        assert_eq!(down.state.gear, 1);
    }
    #[test]
    fn brakes_neutralize_and_reverse_to_forward_resets_filter() {
        let mut engine = Drivetrain::new(config()).unwrap();
        let mut braking = wheels(100.);
        braking[1].brake = 0.1;
        let output = engine.step(input(), &braking).unwrap();
        assert_eq!(output.effective_direction, DriveDirection::Neutral);
        assert_eq!(output.state.gear, 1);
        assert_eq!(output.wheel_torques, [0., 0.]);
        let reverse = EngineInput {
            direction: DriveDirection::Reverse,
            ..input()
        };
        assert!(
            engine
                .step(reverse, &wheels(10.))
                .unwrap()
                .state
                .torque_per_wheel
                < 0.
        );
        let forward = engine.step(input(), &wheels(10.)).unwrap();
        assert_eq!(forward.state.filtered_throttle, 0.);
        assert_eq!(forward.state.torque_per_wheel, 0.);
    }
    #[test]
    fn taper_surface_and_filter_are_separate_stages() {
        let mut engine = Drivetrain::new(config()).unwrap();
        let mut ws = wheels(0.);
        ws[1].surface_drive_scale = 0.25;
        let mut controls = input();
        controls.speed_ft_per_second = 15.;
        controls.allow_shifting = false;
        let out = engine.step(controls, &ws).unwrap();
        assert_eq!(out.wheel_torques[0], 52.52);
        assert_eq!(out.wheel_torques[1], out.wheel_torques[0] * 0.25);
        controls.throttle = 0.;
        assert_eq!(
            engine.step(controls, &ws).unwrap().state.filtered_throttle,
            0.5
        );
        controls.speed_ft_per_second = 20.;
        assert_eq!(
            engine.step(controls, &ws).unwrap().state.torque_per_wheel,
            0.
        );
    }
    #[test]
    fn rejects_bad_configuration_and_does_not_mutate_on_bad_input() {
        let mut c = config();
        c.gear_ratios[0] = 0.;
        assert!(Drivetrain::new(c).is_err());
        let mut engine = Drivetrain::new(config()).unwrap();
        let bad = EngineInput {
            throttle: f32::NAN,
            ..input()
        };
        assert!(engine.step(bad, &wheels(0.)).is_err());
        assert_eq!(engine.state.rpm, 0.);
        let out = engine.step(input(), &[]).unwrap();
        assert_eq!(out.state.rpm, 0.);
        assert!(out.wheel_torques.is_empty());
    }
}
