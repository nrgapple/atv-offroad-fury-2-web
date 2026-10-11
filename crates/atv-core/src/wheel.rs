//! Wheel rotation and suspension kernels recovered from the USA v2.00 executable.
//!
//! These are component routines, not a complete vehicle/contact solver. Callers
//! supply contact geometry and original material parameters. Binary32 host math
//! is not yet a bit-exact implementation of the EE FPU/VU edge cases.
use crate::Result;
use serde::Serialize;

pub type Vec3 = [f32; 3];
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn scale(a: Vec3, s: f32) -> Vec3 {
    a.map(|v| v * s)
}
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// World point velocity, including rotational motion and an optional moving ground.
pub fn point_velocity(linear: Vec3, angular: Vec3, offset: Vec3, ground: Vec3) -> Vec3 {
    sub(add(linear, cross(angular, offset)), ground)
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct WheelConfig {
    pub radius: f32,
    pub inverse_inertia: f32,
    pub rolling_resistance: f32,
    pub brake_torque: f32,
}
impl WheelConfig {
    /// 001ec600: collision-cylinder radius and width determine wheel inertia.
    pub fn from_dimensions(
        radius: f32,
        width: f32,
        rolling_resistance: f32,
        brake_torque: f32,
    ) -> Result<Self> {
        if ![radius, width, rolling_resistance, brake_torque]
            .iter()
            .all(|x| x.is_finite())
            || radius <= 0.0
            || width <= 0.0
            || rolling_resistance < 0.0
            || brake_torque < 0.0
        {
            return Err("invalid wheel dimensions or resistance".into());
        }
        let inertia =
            (((((radius * width) * radius) * f32::from_bits(0x41fb53d2)) * radius) * 0.5 * radius)
                .max(2.0);
        Ok(Self {
            radius,
            inverse_inertia: 1.0 / inertia,
            rolling_resistance,
            brake_torque,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct WheelState {
    pub angle: f32,
    pub angular_speed: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct WheelInput {
    pub dt: f32,
    /// Previous contact state: wheel motion precedes new contact-force evaluation.
    pub grounded: bool,
    pub longitudinal_velocity: f32,
    pub drive_torque: f32,
    pub brake: f32,
    pub surface_brake_scale: f32,
    pub surface_drive_scale: f32,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WheelOutput {
    /// Forward force in wheel/contact coordinates; zero when airborne.
    pub longitudinal_force: f32,
    /// Body-local X torque when the airborne wheel is braked.
    pub airborne_brake_torque: f32,
    pub resistance_torque: f32,
}
impl WheelState {
    /// 001ecc40 scalar rotation/drive branch, after contact-frame velocity projection.
    pub fn step(&mut self, config: WheelConfig, input: WheelInput) -> Result<WheelOutput> {
        if ![
            input.dt,
            input.longitudinal_velocity,
            input.drive_torque,
            input.brake,
            input.surface_brake_scale,
            input.surface_drive_scale,
            self.angle,
            self.angular_speed,
        ]
        .iter()
        .all(|x| x.is_finite())
            || input.dt < 0.0
            || config.radius <= 0.0
            || !config.radius.is_finite()
        {
            return Err("invalid wheel step".into());
        }
        let travel = (0.1 * input.longitudinal_velocity).clamp(-1.0, 1.0);
        // Keep multiplication order from 001ed01c..001ed030.
        let braking =
            input.surface_brake_scale * (config.brake_torque * (travel * -input.brake.max(0.0)));
        let resistance = braking + (-travel * config.rolling_resistance);
        let torque = input.drive_torque + resistance;
        let mut output = WheelOutput {
            longitudinal_force: 0.0,
            airborne_brake_torque: 0.0,
            resistance_torque: resistance,
        };
        if input.grounded {
            output.longitudinal_force = torque * (1.0 / config.radius);
            self.angular_speed = if input.surface_brake_scale < 1.0 && input.brake > 0.0 {
                0.0
            } else {
                let rate = input.longitudinal_velocity * (1.0 / config.radius);
                if input.surface_drive_scale > 0.0
                    && input.surface_drive_scale < 1.0
                    && torque > 0.0
                {
                    rate / input.surface_drive_scale
                } else {
                    rate
                }
            };
        } else if input.brake > 0.0 {
            self.angular_speed = 0.0;
            output.airborne_brake_torque = config.brake_torque;
        } else {
            // The original attenuates airborne drive by existing wheel spin.
            // In particular a stopped airborne wheel remains stopped.
            let spin = (0.1 * self.angular_speed).clamp(-1.0, 1.0);
            self.angular_speed += input.dt * (config.inverse_inertia * (spin * torque));
        }
        self.angle = (self.angle + self.angular_speed * input.dt) % f32::from_bits(0x40c90fdb);
        if self.angle > f32::from_bits(0x40490fdb) {
            self.angle -= f32::from_bits(0x40c90fdb);
        }
        if self.angle < -f32::from_bits(0x40490fdb) {
            self.angle += f32::from_bits(0x40c90fdb);
        }
        if !self.angle.is_finite()
            || !self.angular_speed.is_finite()
            || !output.longitudinal_force.is_finite()
        {
            return Err("nonfinite wheel result".into());
        }
        Ok(output)
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SuspensionConfig {
    pub travel: f32,
    pub extension_speed: f32,
    pub stiffness: f32,
    pub damping: f32,
}
impl SuspensionConfig {
    /// 001ec720 with helpers 001ec560/001ec5a0; mass is weight / 32.2.
    pub fn new(
        travel: f32,
        extension_speed: f32,
        spring: f32,
        shock: f32,
        mass: f32,
    ) -> Result<Self> {
        if ![travel, extension_speed, spring, shock, mass]
            .iter()
            .all(|x| x.is_finite())
            || travel <= 0.0
            || mass <= 0.0
            || extension_speed < 0.0
        {
            return Err("invalid suspension configuration".into());
        }
        let stiffness = ((spring * mass) * f32::from_bits(0x4200cccd)).abs();
        let damping = if stiffness > 0.0 {
            (2.0 * shock) * (mass * stiffness).sqrt()
        } else {
            0.0
        };
        Ok(Self {
            travel,
            extension_speed,
            stiffness,
            damping,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SuspensionState {
    pub compression: f32,
    pub previous_visual_compression: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct SuspensionContact {
    pub available: bool,
    pub normal: Vec3,
    pub ground_point: Vec3,
    pub wheel_center: Vec3,
    pub radius: f32,
    pub body_up: Vec3,
    pub body_right: Vec3,
    pub point_velocity: Vec3,
    pub stiffness_scale: f32,
    /// Angle from the original acos table for dot(normal,body_up).
    /// Keep this supplied until table initialization/edge behavior is recovered.
    pub tilt_angle: f32,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SuspensionOutput {
    pub grounded: bool,
    pub point: Vec3,
    pub force: Vec3,
    pub normal_speed: f32,
    pub magnitude: f32,
}
impl SuspensionState {
    /// 001ed330 geometry, clamped spring/damper force and rollover attenuation.
    pub fn contact(
        &mut self,
        config: SuspensionConfig,
        c: SuspensionContact,
    ) -> Result<SuspensionOutput> {
        if !c
            .normal
            .iter()
            .chain(c.ground_point.iter())
            .chain(c.wheel_center.iter())
            .chain(c.body_up.iter())
            .chain(c.body_right.iter())
            .chain(c.point_velocity.iter())
            .all(|x| x.is_finite())
            || ![c.radius, c.stiffness_scale, c.tilt_angle]
                .iter()
                .all(|x| x.is_finite())
        {
            return Err("nonfinite suspension contact".into());
        }
        let bottom = sub(c.wheel_center, scale(c.normal, c.radius));
        let depth = dot(sub(c.ground_point, bottom), c.normal);
        let point = add(bottom, scale(c.normal, depth));
        if !c.available || depth <= 0.0 || dot(c.normal, c.body_up) <= 0.0 {
            self.compression = 0.0;
            return Ok(SuspensionOutput::default());
        }
        self.compression = depth.min(config.travel).max(0.0);
        let speed = dot(c.point_velocity, c.normal);
        let magnitude = (self.compression * (c.stiffness_scale * config.stiffness)
            - config.damping * speed)
            .max(0.0);
        let mut force = scale(c.normal, magnitude);
        if c.body_right[1].abs() < 0.5 {
            let angle_start = f32::from_bits(0x3f5f66f1);
            let angle_range = f32::from_bits(0x3e32b8c4);
            let factor = ((-(c.tilt_angle - angle_start)) / angle_range + 1.0).clamp(0.0, 1.0);
            force = scale(force, factor);
        }
        Ok(SuspensionOutput {
            grounded: true,
            point,
            force,
            normal_speed: speed,
            magnitude,
        })
    }
    /// 001edcd0 rate-limits extension only, after the physics substeps.
    pub fn visual_position(
        &mut self,
        config: SuspensionConfig,
        dt: f32,
        rest: Vec3,
        down: Vec3,
    ) -> Result<(Vec3, f32)> {
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid suspension time step".into());
        }
        if self.compression < self.previous_visual_compression {
            let released = (self.previous_visual_compression - self.compression)
                .min(config.extension_speed * dt);
            self.compression = self.previous_visual_compression - released;
        }
        self.previous_visual_compression = self.compression;
        Ok((
            sub(rest, scale(down, self.compression)),
            1.0 - self.compression * (1.0 / config.travel),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wheel() -> WheelConfig {
        WheelConfig::from_dimensions(1.0, 0.5, 2.0, 20.0).unwrap()
    }
    fn input() -> WheelInput {
        WheelInput {
            dt: 0.1,
            grounded: true,
            longitudinal_velocity: 5.0,
            drive_torque: 12.0,
            brake: 0.0,
            surface_brake_scale: 1.0,
            surface_drive_scale: 1.0,
        }
    }
    #[test]
    fn rolling_reverse_and_surface_wheel_lock() {
        let mut s = WheelState::default();
        let o = s.step(wheel(), input()).unwrap();
        assert_eq!(o.resistance_torque, -1.0);
        assert_eq!(o.longitudinal_force, 11.0);
        assert_eq!(s.angular_speed, 5.0);
        let mut i = input();
        i.longitudinal_velocity = -5.0;
        i.drive_torque = -12.0;
        assert_eq!(s.step(wheel(), i).unwrap().longitudinal_force, -11.0);
        i.brake = 1.0;
        i.surface_brake_scale = 0.5;
        s.step(wheel(), i).unwrap();
        assert_eq!(s.angular_speed, 0.0);
    }
    #[test]
    fn airborne_spin_and_brake_do_not_apply_ground_force() {
        let mut s = WheelState::default();
        let mut i = input();
        i.grounded = false;
        assert_eq!(s.step(wheel(), i).unwrap().longitudinal_force, 0.0);
        assert_eq!(s.angular_speed, 0.0);
        s.angular_speed = 12.0;
        s.step(wheel(), i).unwrap();
        assert!(s.angular_speed > 12.0);
        i.brake = 0.5;
        let o = s.step(wheel(), i).unwrap();
        assert_eq!(s.angular_speed, 0.0);
        assert_eq!(o.airborne_brake_torque, 20.0);
    }
    #[test]
    fn compression_damping_release_and_inversion() {
        let config = SuspensionConfig::new(0.5, 1.0, 0.2, 0.1, 10.0).unwrap();
        let mut s = SuspensionState::default();
        let mut c = SuspensionContact {
            available: true,
            normal: [0., 1., 0.],
            ground_point: [0.; 3],
            wheel_center: [0., 0.8, 0.],
            radius: 1.0,
            body_up: [0., 1., 0.],
            body_right: [1., 0., 0.],
            point_velocity: [0.; 3],
            stiffness_scale: 1.0,
            tilt_angle: 0.0,
        };
        let o = s.contact(config, c).unwrap();
        assert!(o.grounded);
        assert!((s.compression - 0.2).abs() < 1e-6);
        assert!(o.force[1] > 0.0);
        s.visual_position(config, 0.1, [0.; 3], [0., -1., 0.])
            .unwrap();
        c.available = false;
        s.contact(config, c).unwrap();
        let (p, _) = s
            .visual_position(config, 0.1, [0.; 3], [0., -1., 0.])
            .unwrap();
        assert!((p[1] - 0.1).abs() < 1e-6);
        c.available = true;
        c.body_up = [0., -1., 0.];
        assert!(!s.contact(config, c).unwrap().grounded);
        c.body_up = [0., 1., 0.];
        c.point_velocity = [0., 100., 0.];
        assert_eq!(s.contact(config, c).unwrap().magnitude, 0.0);
    }
    #[test]
    fn point_velocity_includes_body_rotation_and_moving_ground() {
        assert_eq!(
            point_velocity([1., 2., 3.], [0., 1., 0.], [2., 0., 0.], [0., 1., 0.]),
            [1., 1., 1.]
        );
    }
}
