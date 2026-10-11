//! Recovered air control, wheelie and jump preload state machine (USA v2.00).
//! Inputs are the original routine's already-computed body measurements. Input
//! filtering, Euler-angle extraction and gameplay gates remain caller concerns.
use crate::{Result, physics_data::PhysicsData};

#[derive(Debug, Clone, Copy)]
pub struct RiderConfig {
    pub air_force: f32,
    pub air_moment: f32,
    pub air_damping: f32,
    pub moment_scale: [f32; 3],
    pub unroll_gain: f32,
    pub max_wheelie_angle: f32,
    pub wheelie_gain: f32,
    pub inverse_top_wheelie_speed: f32,
    pub max_preload: f32,
    pub preload_min_rate: f32,
    pub preload_max_rate: f32,
}
impl RiderConfig {
    /// Base authored tuning. Profile TweakPreload adjustment must be supplied
    /// separately by the caller once that profile selection is available.
    pub fn from_physics(data: &PhysicsData) -> Result<Self> {
        let value = |key| data.first_scalar("Aerodynamics", key);
        let top = value("TopWheelieSpeed")?;
        if top == 0.0 {
            return Err("zero top wheelie speed".into());
        }
        Ok(Self {
            air_force: value("AeroForce")?,
            air_moment: value("AeroMoment")?,
            air_damping: value("AeroDamping")?,
            moment_scale: [
                value("AeroMomentScaleX")?,
                value("AeroMomentScaleY")?,
                value("AeroMomentScaleZ")?,
            ],
            unroll_gain: value("UnrollGain")?,
            max_wheelie_angle: f32::from_bits(0x3c8efa34) * value("MaxWheelieAngle")?,
            wheelie_gain: value("WheelieGain")?,
            inverse_top_wheelie_speed: 1.0 / top,
            max_preload: value("MaxPreload")?,
            preload_min_rate: value("PreloadMinAxisRate")?,
            preload_max_rate: value("PreloadMaxAxisRate")?,
        })
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct RiderState {
    pub preload_armed: bool,
    pub preload_active: bool,
    pub preload_status: bool,
    pub wheelie_active: bool,
    pub wheelie_integral: f32,
    pub preload_strength: f32,
    pub timer: f32,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct RiderInput {
    pub grounded: bool,
    pub throttle_pressed: bool,
    pub brake_pressed: bool,
    pub neutral: bool,
    /// Original object +0x58 gate, supplied by gameplay code.
    pub half_air_control: bool,
    pub lateral_axis: f32,
    pub lean_axis: f32,
    /// Optional browser gesture channel; None is the original shared axis.
    pub preload_axis: Option<f32>,
    pub lean_axis_rate: f32,
    pub pitch: f32,
    pub roll: f32,
    pub local_angular_velocity: [f32; 3],
    pub local_forward_speed: f32,
    pub world_vertical_speed: f32,
    pub world_body_right: [f32; 3],
    /// Original global vector used when horizontal right has zero length.
    pub zero_right_fallback: [f32; 3],
    /// Original global preload direction; no guessed world axis is embedded.
    pub preload_world_direction: [f32; 3],
}
/// Original input slew limits and lean-dependent horizontal dead zone.
#[derive(Debug, Clone, Copy)]
pub struct AxisConfig {
    pub normal_rate: [f32; 2],
    pub stunt_rate: [f32; 2],
    pub horizontal_dead_zone: f32,
}
impl AxisConfig {
    pub fn from_physics(data: &PhysicsData) -> Result<Self> {
        Ok(Self {
            normal_rate: [
                data.first_scalar("Joystick", "XAxisRateLimit")?,
                data.first_scalar("Joystick", "YAxisRateLimit")?,
            ],
            stunt_rate: [
                data.first_scalar("Joystick", "StuntXAxisRateLimit")?,
                data.first_scalar("Joystick", "StuntYAxisRateLimit")?,
            ],
            horizontal_dead_zone: data.first_scalar("Joystick", "XAxisDeadZone")?,
        })
    }
}
/// Returns dead-zone-adjusted raw axes for airborne control. Smoothed axes feed
/// wheel steering and center-of-mass shift; these two paths must stay distinct.
pub fn smooth_axes(
    config: AxisConfig,
    smoothed: &mut [f32; 2],
    mut raw: [f32; 2],
    grounded: bool,
    stunt_state_four: bool,
    dt: f32,
) -> Result<[f32; 2]> {
    if raw
        .iter()
        .chain(smoothed.iter())
        .chain(config.normal_rate.iter())
        .chain(config.stunt_rate.iter())
        .chain([config.horizontal_dead_zone, dt].iter())
        .any(|v| !v.is_finite())
        || config
            .normal_rate
            .iter()
            .chain(config.stunt_rate.iter())
            .any(|v| *v < 0.0)
        || dt < 0.0
    {
        return Err("invalid axis smoothing input".into());
    }
    let rate = if !grounded && stunt_state_four {
        config.stunt_rate
    } else {
        config.normal_rate
    };
    let limit = rate.map(|v| v * dt);
    let dead_zone = (config.horizontal_dead_zone * raw[1]).abs();
    if dead_zone > 0.0 {
        if raw[0].abs() < dead_zone {
            raw[0] = 0.0;
        } else {
            if dead_zone == 1.0 {
                return Err("singular horizontal dead zone".into());
            }
            let sign = if raw[0] < 0.0 { -1.0 } else { 1.0 };
            raw[0] = (sign * (raw[0].abs() - dead_zone)) / (1.0 - dead_zone);
        }
    }
    for i in 0..2 {
        smoothed[i] += (raw[i] - smoothed[i]).max(-limit[i]).min(limit[i]);
    }
    Ok(raw)
}
/// Original active-riding center of mass update. The lateral speed threshold is
/// rider object+0x3d0; callers must supply its verified tuning value.
pub fn shifted_center(
    base: [f32; 3],
    range: [f32; 3],
    smoothed: [f32; 2],
    forward_speed: f32,
    lateral_speed_threshold: f32,
) -> [f32; 3] {
    [
        if lateral_speed_threshold < forward_speed {
            base[0] + smoothed[0] * range[0]
        } else {
            0.0
        },
        base[1],
        base[2] + (-smoothed[1]) * range[2],
    ]
}

/// Lean flick measurement from001eb0d0. Timestamp is milliseconds, matching
/// the original clock conversion. Only the crossing emits a rate.
#[derive(Debug, Clone, Copy, Default)]
pub struct LeanGesture {
    timestamp_ms: f32,
    starting_axis: f32,
}
impl LeanGesture {
    pub fn sample(&mut self, lean: f32, now_ms: f32) -> Result<f32> {
        if !lean.is_finite() || !now_ms.is_finite() {
            return Err("nonfinite lean gesture".into());
        }
        if lean > 0.8 {
            self.timestamp_ms = now_ms;
            self.starting_axis = lean;
        } else if self.timestamp_ms > 0.0 && lean < -0.7 {
            let elapsed = (now_ms - self.timestamp_ms) * 0.001;
            self.timestamp_ms = 0.0;
            if elapsed > 0.0 {
                return Ok((lean - self.starting_axis) / elapsed);
            }
        }
        Ok(0.0)
    }
}
/// Original00146850 orientation extraction with a caller-supplied original
/// atan2 implementation (001ab880). Quaternion order is W,X,Y,Z.
pub fn body_angles(q: [f32; 4], atan2: impl Fn(f32, f32) -> f32) -> [f32; 3] {
    let [w, x, y, z] = q;
    let r = (w * z + x * y) * 2.0;
    let d = 1.0 - (x * x + z * z) * 2.0;
    let p = (y * z - w * x) * 2.0;
    let yaw = atan2((w * y + x * z) * 2.0, 1.0 - (x * x + y * y) * 2.0);
    let roll = atan2(r, d);
    let pitch = atan2(-p, (d * d + r * r).sqrt());
    [-pitch, -yaw, -roll]
}

/// Adds directly to existing accumulators to retain original addition order.
/// The routine runs once per original physics substep. Finite inputs and tuning
/// are required; this models ordinary f32, not every R5900 FPU corner case.
pub fn step(
    config: RiderConfig,
    state: &mut RiderState,
    input: RiderInput,
    dt: f32,
    world_force: &mut [f32; 3],
    body_moment: &mut [f32; 3],
) -> Result<()> {
    let scalars = [
        config.air_force,
        config.air_moment,
        config.air_damping,
        config.unroll_gain,
        config.max_wheelie_angle,
        config.wheelie_gain,
        config.inverse_top_wheelie_speed,
        config.max_preload,
        config.preload_min_rate,
        config.preload_max_rate,
        state.wheelie_integral,
        state.preload_strength,
        state.timer,
        input.lateral_axis,
        input.lean_axis,
        input.lean_axis_rate,
        input.preload_axis.unwrap_or(input.lean_axis),
        input.pitch,
        input.roll,
        input.local_forward_speed,
        input.world_vertical_speed,
        dt,
    ];
    if scalars
        .iter()
        .chain(config.moment_scale.iter())
        .chain(input.local_angular_velocity.iter())
        .chain(input.world_body_right.iter())
        .chain(input.zero_right_fallback.iter())
        .chain(input.preload_world_direction.iter())
        .chain(world_force.iter())
        .chain(body_moment.iter())
        .any(|v| !v.is_finite())
    {
        return Err("nonfinite rider control input".into());
    }
    state.wheelie_active = false;
    if !input.grounded {
        if state.preload_strength <= 0.0 {
            state.preload_status = false;
        }
        let axes = [input.lean_axis, input.lateral_axis, input.lateral_axis];
        for i in 0..3 {
            let mut moment = axes[i] * (config.moment_scale[i] * config.air_moment);
            if input.half_air_control {
                moment *= 0.5;
            }
            // Assembly groups torque + damping BEFORE adding the accumulator.
            body_moment[i] += moment + config.air_damping * input.local_angular_velocity[i];
        }
        let x = input.world_body_right[0];
        let z = input.world_body_right[2];
        let length_squared = (x * x + 0.0) + z * z;
        let right = if length_squared == 0.0 {
            input.zero_right_fallback
        } else {
            let inverse = 1.0 / length_squared.sqrt();
            [x * inverse, 0.0 * inverse, z * inverse]
        };
        let force = config.air_force * input.lateral_axis;
        for i in 0..3 {
            world_force[i] += force * right[i];
        }
        if input.lateral_axis.abs() < f32::from_bits(0x3dcccccd)
            && input.pitch.abs() < f32::from_bits(0x3f47ae14)
            && input.roll.abs() < f32::from_bits(0x3f851eb8)
        {
            body_moment[0] += 0.0;
            body_moment[1] += 0.0;
            body_moment[2] +=
                100.0 * (input.local_angular_velocity[2] - (-config.unroll_gain * input.roll));
        }
        state.preload_armed = false;
    } else {
        if input.throttle_pressed
            && input.local_forward_speed > 5.0
            && !input.brake_pressed
            && !input.neutral
            && input.lean_axis > 0.5
        {
            let target =
                ((2.0 * input.lean_axis) * config.max_wheelie_angle) - config.max_wheelie_angle;
            let error =
                config.wheelie_gain * (input.pitch - target) - input.local_angular_velocity[0];
            state.wheelie_integral += (500.0 * error) * dt;
            let fade = (1.0 - input.local_forward_speed * config.inverse_top_wheelie_speed)
                .clamp(0.0, 1.0);
            state.wheelie_active = true;
            body_moment[0] += fade * (1000.0 * error + state.wheelie_integral);
            body_moment[1] += 0.0;
            body_moment[2] += 0.0;
        } else {
            state.wheelie_integral = 0.0;
        }
        if state.preload_active
            || input.world_vertical_speed <= 5.0
            || input.local_forward_speed <= 30.0
            || dt <= 0.0
        {
            state.preload_status = false;
        } else {
            state.preload_status = true;
            if input.preload_axis.unwrap_or(input.lean_axis) > f32::from_bits(0x3f4ccccd) {
                state.preload_armed = true;
                state.timer = 0.25;
            }
            if state.preload_armed
                && input.preload_axis.unwrap_or(input.lean_axis) < f32::from_bits(0xbe4ccccd)
                && input.lean_axis_rate < -config.preload_min_rate
            {
                state.preload_active = true;
                state.timer = 0.25;
                state.preload_strength = if -config.preload_max_rate == -config.preload_min_rate {
                    0.0
                } else {
                    (0.0 + ((input.lean_axis_rate - (-config.preload_min_rate)) * 1.0)
                        / (-config.preload_max_rate - (-config.preload_min_rate)))
                        .clamp(0.0, 1.0)
                };
            }
        }
    }
    if state.preload_active {
        state.timer -= dt;
        if state.timer > 0.0 {
            let force = config.max_preload * state.preload_strength;
            for (i, output) in world_force.iter_mut().enumerate() {
                *output += force * input.preload_world_direction[i];
            }
        } else {
            state.preload_active = false;
        }
        state.preload_status = true;
        state.preload_armed = false;
    } else {
        state.preload_strength = 0.0;
    }
    if state.preload_armed {
        state.timer -= dt;
        if state.timer < 0.0 {
            state.preload_armed = false;
            state.timer = 0.0;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> RiderConfig {
        RiderConfig {
            air_force: 2.,
            air_moment: 3.,
            air_damping: -4.,
            moment_scale: [-1., 0.5, -0.25],
            unroll_gain: 2.,
            max_wheelie_angle: 1.,
            wheelie_gain: 2.,
            inverse_top_wheelie_speed: 0.01,
            max_preload: 100.,
            preload_min_rate: 1.,
            preload_max_rate: 10.,
        }
    }
    #[test]
    fn preload_requires_arm_then_fast_lean_and_has_strict_expiry() {
        let mut state = RiderState::default();
        let mut f = [0.; 3];
        let mut m = [0.; 3];
        let mut input = RiderInput {
            grounded: true,
            local_forward_speed: 40.,
            world_vertical_speed: 6.,
            lean_axis: 1.,
            preload_world_direction: [0., 1., 0.],
            ..Default::default()
        };
        step(config(), &mut state, input, 0.05, &mut f, &mut m).unwrap();
        assert!(state.preload_armed);
        assert_eq!(state.timer, 0.2);
        input.lean_axis = -1.;
        input.lean_axis_rate = -10.;
        step(config(), &mut state, input, 0.05, &mut f, &mut m).unwrap();
        assert!(state.preload_active);
        assert_eq!(f[1], 100.);
        step(config(), &mut state, input, 0.2, &mut f, &mut m).unwrap();
        assert!(!state.preload_active);
        assert_eq!(f[1], 100.);
        assert_eq!(state.preload_strength, 1.); // cleared on the following call
    }
    #[test]
    fn held_then_released_lean_uses_native_ramp_gates_not_hold_duration() {
        let dt = 1.0 / 60.0;
        for (grounded, speed, vertical, allowed) in [
            (true, 40.0, 6.0, true),
            (true, 40.0, 0.0, false),
            (true, 30.0, 6.0, false),
            (true, 40.0, 5.0, false),
            (false, 40.0, 6.0, false),
        ] {
            let mut state = RiderState::default();
            let mut gesture = LeanGesture::default();
            let mut input = RiderInput {
                grounded,
                local_forward_speed: speed,
                world_vertical_speed: vertical,
                lean_axis: 1.0,
                preload_world_direction: [0.0, 1.0, 0.0],
                ..Default::default()
            };
            // Holding back refreshes the native timing sample and arming window;
            // it is not a cumulative charge, even after several seconds.
            for frame in 1..=300 {
                input.lean_axis_rate = gesture.sample(1.0, frame as f32 * 1000.0 * dt).unwrap();
                step(
                    config(),
                    &mut state,
                    input,
                    dt,
                    &mut [0.0; 3],
                    &mut [0.0; 3],
                )
                .unwrap();
            }
            assert_eq!(state.preload_armed, allowed);
            assert_eq!(state.preload_strength, 0.0);
            input.lean_axis = -1.0;
            input.lean_axis_rate = gesture.sample(-1.0, 301.0 * 1000.0 * dt).unwrap();
            let mut force = [0.0; 3];
            step(config(), &mut state, input, dt, &mut force, &mut [0.0; 3]).unwrap();
            assert_eq!(state.preload_active, allowed);
            assert_eq!(force[1], if allowed { 100.0 } else { 0.0 });
        }
    }
    #[test]
    fn separate_touch_preload_does_not_fire_when_physical_lean_changes() {
        let mut state = RiderState::default();
        let mut gesture = LeanGesture::default();
        let mut input = RiderInput {
            grounded: true,
            local_forward_speed: 40.0,
            world_vertical_speed: 6.0,
            preload_axis: Some(1.0),
            preload_world_direction: [0.0, 1.0, 0.0],
            ..Default::default()
        };
        for (i, lean) in [0.0, 1.0, -1.0, 0.5, -0.5].into_iter().enumerate() {
            input.lean_axis = lean;
            input.lean_axis_rate = gesture.sample(1.0, 100.0 + i as f32 * 16.0).unwrap();
            let mut force = [0.0; 3];
            step(
                config(),
                &mut state,
                input,
                0.016,
                &mut force,
                &mut [0.0; 3],
            )
            .unwrap();
            assert!(state.preload_armed);
            assert!(!state.preload_active);
            assert_eq!(force[1], 0.0);
        }
        input.lean_axis = 0.0;
        input.preload_axis = Some(-1.0);
        input.lean_axis_rate = gesture.sample(-1.0, 180.0).unwrap();
        let mut force = [0.0; 3];
        step(
            config(),
            &mut state,
            input,
            0.016,
            &mut force,
            &mut [0.0; 3],
        )
        .unwrap();
        assert!(state.preload_active);
        assert_eq!(force[1], 100.0);
    }
    #[test]
    fn returning_to_neutral_does_not_release_preload() {
        let mut state = RiderState::default();
        let mut gesture = LeanGesture::default();
        let mut input = RiderInput {
            grounded: true,
            local_forward_speed: 40.0,
            world_vertical_speed: 6.0,
            lean_axis: 1.0,
            preload_world_direction: [0.0, 1.0, 0.0],
            ..Default::default()
        };
        input.lean_axis_rate = gesture.sample(1.0, 100.0).unwrap();
        step(
            config(),
            &mut state,
            input,
            0.016,
            &mut [0.0; 3],
            &mut [0.0; 3],
        )
        .unwrap();
        input.lean_axis = 0.0;
        for frame in 1..=30 {
            input.lean_axis_rate = gesture.sample(0.0, 100.0 + frame as f32 * 16.0).unwrap();
            let mut force = [0.0; 3];
            step(
                config(),
                &mut state,
                input,
                0.016,
                &mut force,
                &mut [0.0; 3],
            )
            .unwrap();
            assert!(!state.preload_active);
            assert_eq!(force, [0.0; 3]);
        }
        assert!(!state.preload_armed);
    }
    #[test]
    fn flick_rate_measures_only_original_threshold_crossing() {
        let mut gesture = LeanGesture::default();
        assert_eq!(gesture.sample(1., 100.).unwrap(), 0.);
        assert_eq!(gesture.sample(0., 150.).unwrap(), 0.);
        assert_eq!(gesture.sample(-0.7, 200.).unwrap(), 0.);
        assert_eq!(gesture.sample(-1., 200.).unwrap(), -20.);
        assert_eq!(gesture.sample(-1., 250.).unwrap(), 0.);
    }
    #[test]
    fn slew_and_center_shift_keep_raw_and_filtered_axes_distinct() {
        let config = AxisConfig {
            normal_rate: [4., 4.],
            stunt_rate: [2., 2.],
            horizontal_dead_zone: 0.5,
        };
        let mut axes = [0.; 2];
        let raw = smooth_axes(config, &mut axes, [1., 1.], true, false, 0.1).unwrap();
        assert_eq!(raw, [1., 1.]);
        assert_eq!(axes, [0.4, 0.4]);
        let base = [0.1, 0.2, 0.3];
        let range = [1., 0., 2.];
        assert_eq!(shifted_center(base, range, axes, 10., 10.)[0], 0.);
        assert_eq!(
            shifted_center(base, range, axes, 11., 10.),
            [0.5, 0.2, -0.5]
        );
    }
    #[test]
    fn original_euler_axis_signs() {
        let s = (0.25f32).sin();
        let c = (0.25f32).cos();
        for i in 0..3 {
            let mut q = [c, 0., 0., 0.];
            q[i + 1] = s;
            let angles = body_angles(q, f32::atan2);
            assert!((angles[i] + 0.5).abs() < 0.00001);
        }
    }
    #[test]
    fn brake_resets_wheelie_integral() {
        let mut state = RiderState {
            wheelie_integral: 20.,
            ..Default::default()
        };
        let input = RiderInput {
            grounded: true,
            throttle_pressed: true,
            brake_pressed: true,
            lean_axis: 1.,
            local_forward_speed: 20.,
            ..Default::default()
        };
        step(
            config(),
            &mut state,
            input,
            0.01,
            &mut [0.; 3],
            &mut [0.; 3],
        )
        .unwrap();
        assert!(!state.wheelie_active);
        assert_eq!(state.wheelie_integral, 0.);
    }
}
