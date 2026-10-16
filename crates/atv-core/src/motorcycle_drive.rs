//! Motocross riding controller recovered from SLUS_211.04/GAME_REL: pre-frame
//! input filtering, kinematic steer and drivetrain shifting (004d8b20,
//! 004e3ea0, 004e5fe0, 004e6a60, 004e4190); per-substep forces (004d90f0:
//! low-speed hold 004dc970, wheelie/stoppie 004dc4f0, preload 004e60d0,
//! backflip 004e4f20, three-sixty 004e53d0, engine 0049fbb0, drag 004e6c00,
//! airborne control 004e65d0, reverse 004dcc80, tires, rev limiter 0049ff40);
//! constant-acceleration body integration with the roll axis locked on the
//! ground (00495d30/00496240); the post-frame bank (004dbd10) and the wreck
//! decision chain (004e24a0).
//!
//! Host trigonometry, the authored terrain/obstacle contact adapter, the
//! browser input adapter and the omitted body-collision hull mean this is
//! component recovery, not native whole-step parity.
use super::*;
use crate::motorcycle_tire::{SideState, Tire};

const MPH: f32 = 1.46666;
const RPM_PER_RAD: f32 = 9.549296;

/// Tuning resolved once from the source `.phy` with the parser's conversions.
#[derive(Debug, Clone)]
pub(super) struct Params {
    pub weight: f32,
    pub gravity_scale: f32,
    pub cofg: V,
    pub wreck_cofg: V,
    pub travel_forward: f32,
    pub travel_back: f32,
    pub brake_travel_forward: f32,
    pub yaw_rate_limit: f32,
    pub tires: [Tire; 2],
    // Engine.
    pub rpm_points: Vec<(f32, f32)>,
    pub gears: Vec<f32>,
    pub final_ratio: f32,
    pub min_rpm: f32,
    pub idle_throttle: f32,
    pub engine_inertia: f32,
    pub engine_braking: f32,
    pub clutch_static: f32,
    pub clutch_kinetic: f32,
    pub upshift: f32,
    pub downshift: f32,
    pub boost_time: f32,
    pub boost_max: f32,
    pub boost_speed: f32,
    pub clutch_travel_back: f32,
    pub reverse_speed: f32,
    pub reverse_gain: f32,
    // Roll / pitch.
    pub max_roll: f32,
    pub max_roll_speed: f32,
    pub roll_gain: f32,
    pub stand_min: f32,
    pub stand_max: f32,
    pub max_pitch: f32,
    pub pitch_gain: f32,
    pub pitch_rate_gain: f32,
    pub wheelie_top: f32,
    pub max_stoppie: f32,
    pub stoppie_gain: f32,
    pub stoppie_top: f32,
    pub stoppie_yaw: f32,
    pub stoppie_brake: f32,
    pub drag: f32,
    // Air.
    pub air_damping: f32,
    pub air_force: f32,
    pub air_moment: V,
    pub brake_gyro: f32,
    pub unroll_gain: f32,
    pub unroll_torque: f32,
    pub unyaw_gain: f32,
    pub unyaw_torque: f32,
    pub max_crossup: f32,
    // Preload and spins.
    pub preload_time: f32,
    pub max_preload: f32,
    pub preload_window: [f32; 2],
    pub backflip: [f32; 2],
    pub backflip_decrease: f32,
    pub backflip_window: [f32; 2],
    pub spin_x: [f32; 2],
    pub spin_y: [f32; 2],
    pub spin_decrease: f32,
    pub spin_window: [f32; 2],
    // Donuts.
    pub donut_speed: f32,
    pub donut_angle: f32,
    pub donut_roll: f32,
    // Joystick.
    pub steer_rate: f32,
    pub lean_rate: f32,
    pub steer_scale: [(f32, f32); 2],
    pub gas_rate: f32,
    pub brake_rate: f32,
    pub clutch_rates: [f32; 2],
    // Wrecks.
    pub front_limit: f32,
    pub back_limit: f32,
    pub side_limit: f32,
    pub down_limit: f32,
    pub ground_pitch: [f32; 2],
    pub air_pitch: [f32; 2],
    pub air_roll: f32,
    pub crossup_wreck: f32,
    pub crossup_time: f32,
    pub max_wreck_time: f32,
}

impl Params {
    pub fn parse(data: &PhysicsData) -> Result<Self> {
        let get = |section: &str, key: &str, default: f32| {
            data.first_scalar(section, key).unwrap_or(default)
        };
        let vector =
            |section: &str, key: &str, default: V| data.vector3(section, key).unwrap_or(default);
        let weight = data.first_scalar("Mass Properties", "Weight")?;
        let tires = [
            Tire::parse(data, "Front Tire", weight, false)?,
            Tire::parse(data, "Rear Tire", weight, true)?,
        ];
        let rpm_points: Vec<(f32, f32)> = (1..=get("Engine", "NumberOfRPMPoints", 0.) as usize)
            .map(|i| {
                let rpm = get("Engine", &format!("RPM{i}"), 0.);
                let hp = get("Engine", &format!("HP{i}"), 0.);
                (rpm, if rpm > 0. { hp * 5252. / rpm } else { 0. })
            })
            .collect();
        let gears: Vec<f32> = (1..=get("Engine", "NumberOfGears", 1.) as usize)
            .map(|i| get("Engine", &format!("GearRatio{i}"), 1.))
            .collect();
        if rpm_points.is_empty() || gears.is_empty() {
            return Err("motorcycle engine lacks a torque curve or gears".into());
        }
        let cofg = data.vector3("Mass Properties", "CofG")?;
        Ok(Self {
            weight,
            gravity_scale: get("Mass Properties", "GravityScale", 1.),
            cofg,
            wreck_cofg: vector("Mass Properties", "WreckCofG", cofg),
            travel_forward: get("Mass Properties", "CofGTravelForward", 0.),
            travel_back: get("Mass Properties", "CofGTravelBack", 0.),
            brake_travel_forward: get("Mass Properties", "BrakeCofGTravelForward", 0.),
            yaw_rate_limit: get("Mass Properties", "YawRateLimit", 20.),
            tires,
            rpm_points,
            gears,
            final_ratio: get("Engine", "FinalGearRatio", 1.),
            min_rpm: get("Engine", "MinRPM", 1000.),
            idle_throttle: get("Engine", "IdleThrottle", 0.),
            engine_inertia: get("Engine", "EngineInertia", 0.01).max(1e-4),
            engine_braking: get("Engine", "EngineBraking", 0.),
            clutch_static: get("Engine", "ClutchStaticFriction", 100.),
            clutch_kinetic: get("Engine", "ClutchKineticFriction", 100.),
            upshift: get("Engine", "UpShiftRPM", 8000.),
            downshift: get("Engine", "DownShiftRPM", 6000.),
            boost_time: get("Engine", "ClutchTorqueScaleTime", 0.),
            boost_max: get("Engine", "MaxClutchTorqueScale", 1.),
            boost_speed: get("Engine", "MaxClutchTorqueScaleSpeed", 1.) * MPH,
            clutch_travel_back: get("Engine", "ClutchCofGTravelBack", 0.),
            reverse_speed: get("Engine", "ReverseSpeed", 0.) * MPH,
            reverse_gain: get("Engine", "ReverseGain", 0.),
            max_roll: get("Roll Control", "MaxRollCommand", 0.).to_radians(),
            max_roll_speed: get("Roll Control", "MaxRollSpeed", 0.) * MPH,
            roll_gain: get("Roll Control", "RollGain", 0.),
            stand_min: get("Roll Control", "MinStandUprightSpeed", 0.) * MPH,
            stand_max: get("Roll Control", "MaxStandUprightSpeed", 0.) * MPH,
            max_pitch: get("Pitch Control", "MaxPitchCommand", 0.).to_radians(),
            pitch_gain: get("Pitch Control", "PitchGain", 0.),
            pitch_rate_gain: get("Pitch Control", "PitchRateGain", 0.),
            wheelie_top: get("Pitch Control", "WheelieTopSpeed", 1.) * MPH,
            max_stoppie: get("Pitch Control", "MaxStoppiePitchCommand", 0.).to_radians(),
            stoppie_gain: get("Pitch Control", "StoppiePitchGain", 0.),
            stoppie_top: get("Pitch Control", "StoppieTopSpeed", 1.) * MPH,
            stoppie_yaw: get("Pitch Control", "StoppieYawRateScale", 0.),
            stoppie_brake: get("Pitch Control", "StoppieBrake", 1.),
            drag: get("Aerodynamics", "Drag", 0.),
            air_damping: get("Airborne Control", "AirborneDamping", 0.),
            air_force: get("Airborne Control", "AirborneControlForce", 0.),
            air_moment: vector("Airborne Control", "AirborneControlMoment", [0.; 3]),
            brake_gyro: get("Airborne Control", "BrakeGyroTorque", 0.),
            unroll_gain: get("Airborne Control", "UnrollGain", 0.),
            unroll_torque: get("Airborne Control", "UnrollTorque", 0.),
            unyaw_gain: get("Airborne Control", "UnyawGain", 0.),
            unyaw_torque: get("Airborne Control", "UnyawTorque", 0.),
            max_crossup: get("Airborne Control", "MaxCrossupAngle", 45.).to_radians(),
            preload_time: get("Preload", "PreloadTime", 0.),
            max_preload: get("Preload", "MaxPreload", 0.),
            preload_window: [
                get("Preload", "MinPreloadTime", 0.),
                get("Preload", "MaxPreloadTime", 60.),
            ],
            backflip: [
                get("Backflip", "MinBackflipTorque", 0.),
                get("Backflip", "MaxBackflipTorque", 0.),
            ],
            backflip_decrease: get("Backflip", "BackflipTorqueRateDecrease", 0.),
            backflip_window: [
                get("Backflip", "MinBackflipTime", 0.),
                get("Backflip", "MaxBackflipTime", 60.),
            ],
            spin_x: [
                get("Three Sixty", "MinThreeSixtyTorqueX", 0.),
                get("Three Sixty", "MaxThreeSixtyTorqueX", 0.),
            ],
            spin_y: [
                get("Three Sixty", "MinThreeSixtyTorqueY", 0.),
                get("Three Sixty", "MaxThreeSixtyTorqueY", 0.),
            ],
            spin_decrease: get("Three Sixty", "ThreeSixtyTorqueRateDecrease", 0.),
            spin_window: [
                get("Three Sixty", "MinThreeSixtyTime", 0.),
                get("Three Sixty", "MaxThreeSixtyTime", 60.),
            ],
            donut_speed: get("Donuts", "MaxDonutSpeed", 0.) * MPH,
            donut_angle: get("Donuts", "MinDonutAngle", 30.).to_radians(),
            donut_roll: get("Donuts", "DonutRollAngleScale", 0.),
            steer_rate: get("Joystick", "XAxisRateLimit", 4.),
            lean_rate: get("Joystick", "YAxisRateLimit", 3.5),
            steer_scale: [
                (
                    get("Joystick", "SteerSpeed1", 0.) * MPH,
                    get("Joystick", "SteerScale1", 1.),
                ),
                (
                    get("Joystick", "SteerSpeed2", 0.) * MPH,
                    get("Joystick", "SteerScale2", 1.),
                ),
            ],
            gas_rate: get("Joystick", "GasRateLimit", 10.),
            brake_rate: get("Joystick", "BrakeRateLimit", 10.),
            clutch_rates: [
                get("Joystick", "ClutchInRateLimit", 10.),
                get("Joystick", "ClutchOutRateLimit", 10.),
            ],
            front_limit: get("Wrecks", "FrontWreckSpeedLimit", 12.5) * MPH,
            back_limit: get("Wrecks", "BackWreckSpeedLimit", 15.) * MPH,
            side_limit: get("Wrecks", "SideWreckSpeedLimit", 13.) * MPH,
            down_limit: get("Wrecks", "FreestyleDownWreckSpeedLimit", 48.) * MPH,
            ground_pitch: [
                get("Wrecks", "GroundPitchDownWreckAngle", 75.).to_radians(),
                get("Wrecks", "GroundPitchUpWreckAngle", 75.).to_radians(),
            ],
            air_pitch: [
                get("Wrecks", "AirbornePitchDownWreckAngle", 85.).to_radians(),
                get("Wrecks", "AirbornePitchUpWreckAngle", 89.).to_radians(),
            ],
            air_roll: get("Wrecks", "AirborneRollWreckAngle", 89.).to_radians(),
            crossup_wreck: get("Wrecks", "CrossupWreck", 0.99),
            crossup_time: get("Wrecks", "CrossupWreckTime", 0.75),
            max_wreck_time: get("Wrecks", "MaxWreckTime", 1.75),
        })
    }
    /// Piecewise-linear torque (ft·lbf), clamped to the table ends.
    pub fn torque(&self, rpm: f32) -> f32 {
        let p = &self.rpm_points;
        if rpm <= p[0].0 {
            return p[0].1;
        }
        for w in p.windows(2) {
            if rpm <= w[1].0 {
                let span = (w[1].0 - w[0].0).max(1e-3);
                return w[0].1 + (w[1].1 - w[0].1) * (rpm - w[0].0) / span;
            }
        }
        p[p.len() - 1].1
    }
    pub fn max_rpm(&self) -> f32 {
        self.rpm_points[self.rpm_points.len() - 1].0
    }
    pub fn ratio(&self, gear: usize) -> f32 {
        self.final_ratio * self.gears[gear.clamp(1, self.gears.len()) - 1]
    }
    /// Linear SteerScale between SteerSpeed1 and SteerSpeed2 (signed speed).
    pub fn steer_scale(&self, speed: f32) -> f32 {
        let [(s1, k1), (s2, k2)] = self.steer_scale;
        if s1 == s2 {
            return k1;
        }
        (k1 + (k2 - k1) * (speed - s1) / (s2 - s1)).clamp(k1.min(k2), k2.max(k1))
    }
}

fn unit(v: V) -> V {
    scale(v, 1. / dot(v, v).sqrt().max(1e-6))
}
fn slew(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current).clamp(-rate * dt, rate * dt)
}
fn window(value: f32, [min, max]: [f32; 2]) -> f32 {
    if max == min {
        1.
    } else {
        ((value - min) / (max - min)).clamp(0., 1.)
    }
}

/// Filtered controls and controller state that persists between steps.
#[derive(Debug, Clone, Default)]
pub(super) struct Drive {
    pub steer: f32,
    pub lean: f32,
    pub gas: f32,
    pub brake: f32,
    pub clutch: f32,
    pub raw_gas: bool,
    pub raw_brake: bool,
    pub reverse: bool,
    pub stick: [f32; 2],
    pub delta: f32,
    pub donut: f32,
    pub tires: [TireState; 2],
    pub rpm: f32,
    pub gear: usize,
    pub locked: bool,
    pub wheel_rpm: f32,
    pub boost_scale: f32,
    pub boost_t: f32,
    pub boost_start: f32,
    pub launch: f32,
    pub wheelie_i: f32,
    pub stoppie: bool,
    pub stoppie_i: f32,
    pub pitch_control: bool,
    pub preload: Preload,
    pub backflip: Spin,
    pub spin: Spin,
    pub spin_dir: f32,
    pub recovery: f32,
    pub hold: [f32; 3],
    pub impact: V,
    pub airtime: f32,
    pub crab: f32,
    pub was_airborne: bool,
}
impl Drive {
    /// Per-frame peak body-frame contact velocity change, per axis.
    pub fn latch_impact(&mut self, local: V) {
        for (peak, v) in self.impact.iter_mut().zip(local) {
            if v.abs() > peak.abs() {
                *peak = v;
            }
        }
    }
    pub fn new(p: &Params) -> Self {
        Self {
            rpm: p.min_rpm,
            gear: 1,
            boost_scale: 1.,
            // The boost timer starts expired; a spawn grants no clutch dump.
            boost_t: p.boost_time + 1.,
            ..Self::default()
        }
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TireState {
    pub grounded: bool,
    pub borrowed: bool,
    pub normal: V,
    pub point: V,
    pub load: f32,
    pub compression: f32,
    pub display: f32,
    pub spin: bool,
    pub lock: bool,
    pub omega: f32,
    pub angle: f32,
    pub torque: f32,
    pub force: f32,
    pub surface: usize,
}
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Preload {
    pub armed: bool,
    pub firing: bool,
    pub timer: f32,
    pub force: f32,
}
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Spin {
    pub armed: bool,
    pub active: bool,
    pub airborne: bool,
    pub torque: [f32; 2],
    pub timer: f32,
    pub recovering: bool,
}

impl Motorcycle {
    fn grounded_any(&self) -> bool {
        self.drive.tires.iter().any(|t| t.grounded || t.borrowed)
    }
    /// Signed forward speed of the contacting tire (front preferred), else body.
    pub(super) fn forward_speed(&self) -> f32 {
        dynamics::inverse_transform(self.body.basis(), self.body.linear_velocity)[2]
    }
    fn pitch(&self) -> f32 {
        self.body.basis()[2][1].clamp(-1., 1.).asin()
    }
    /// Original roll: positive when the body +x side is low.
    fn roll(&self) -> f32 {
        let b = self.body.basis();
        -b[0][1].atan2(b[1][1])
    }
    fn set_cofg(&mut self, cofg: V) {
        let origin = self.body.origin(&self.body_config);
        self.body_config.center_of_mass = cofg;
        self.body.center = add(origin, dynamics::transform(self.body.basis(), cofg));
    }

    /// Once per frame before physics.
    pub(super) fn prepare(&mut self, input: VehicleInput, dt: f32) {
        let p = &self.params;
        let wreck = self.frame.wreck.phase != crate::wreck::Phase::Riding;
        let speed = self.forward_speed();
        let d = &mut self.drive;
        // Raw controls. The source's gas/brake are digital; browser analog
        // values are kept as targets for the same up-rate/instant-release filter.
        let mut gas = input.throttle;
        let mut brake = input.brake;
        d.reverse = input.reverse;
        if gas > 0. && brake > 0. {
            if speed.abs() < 5. {
                d.reverse = true;
                gas = 0.;
                brake = 0.;
            } else if !d.was_airborne {
                gas = 0.;
            }
        }
        if d.stoppie && brake > 0. {
            brake = p.stoppie_brake;
        }
        d.raw_gas = gas > 0.;
        d.raw_brake = brake > 0.;
        d.stick = [input.steer, input.lean];
        d.steer = slew(d.steer, input.steer, p.steer_rate, dt);
        d.lean = slew(d.lean, input.lean, p.lean_rate, dt);
        d.gas = if gas < d.gas {
            gas
        } else {
            slew(d.gas, gas, p.gas_rate, dt)
        };
        if d.reverse && !d.was_airborne {
            d.gas = 0.;
        }
        d.brake = if brake < d.brake {
            brake
        } else {
            slew(d.brake, brake, p.brake_rate, dt)
        };
        if d.was_airborne {
            d.brake = 0.;
        }
        let clutch = if self.clutch_down { 1. } else { 0. };
        d.clutch = slew(
            d.clutch,
            clutch,
            if clutch > d.clutch {
                p.clutch_rates[0]
            } else {
                p.clutch_rates[1]
            },
            dt,
        );
        // Kinematic front steer.
        d.delta = if wreck {
            0.
        } else {
            p.tires[0].max_steer * d.steer * p.steer_scale(speed)
        };
        // Donut factor: both wheels down, slow, full gas and the stick pushed
        // forward on a diagonal, peaking MinDonutAngle below horizontal.
        d.donut = 0.;
        let magnitude = (input.steer * input.steer + input.lean * input.lean).sqrt();
        let speed_abs = dot(self.body.linear_velocity, self.body.linear_velocity).sqrt();
        if d.tires.iter().all(|t| t.grounded)
            && speed_abs < p.donut_speed
            && magnitude > 0.9
            && d.raw_gas
            && input.lean < 0.
        {
            // Angle below horizontal, folded to [0, 90°].
            let u = (-input.lean / magnitude).clamp(0., 1.).asin();
            let peak = p
                .donut_angle
                .clamp(1e-3, std::f32::consts::FRAC_PI_2 - 1e-3);
            d.donut = if u <= peak {
                u / peak
            } else {
                (std::f32::consts::FRAC_PI_2 - u) / (std::f32::consts::FRAC_PI_2 - peak)
            }
            .clamp(0., 1.);
        }
        // Anti-stall clutch and automatic shifting on predicted rpm.
        if speed < 7.3333 && d.gas == 0. {
            d.clutch = 1.;
        }
        let estimate = p.ratio(d.gear) * speed / p.tires[1].radius * RPM_PER_RAD;
        if estimate > p.upshift && d.gear < p.gears.len() {
            d.gear += 1;
        } else if estimate < p.downshift && d.gear > 1 {
            d.gear -= 1;
        }
        // Clutch dump: hold clutch and gas, release for a decaying torque boost
        // and a rearward centre of gravity.
        let raw_clutch = self.clutch_down;
        if raw_clutch && d.raw_gas {
            d.boost_t = 0.;
            d.boost_start = 0.;
            d.launch = 0.;
        } else if !raw_clutch && d.boost_t <= p.boost_time {
            if d.boost_t == 0. {
                let k = speed / p.boost_speed.max(1e-3);
                let m = p.boost_max;
                d.boost_start = (k * (1. - m) + m).clamp(m.min(1.), m.max(1.));
                d.launch = (1. - k).clamp(0., 1.);
            }
            d.boost_t += dt;
            let t = if p.boost_time > 0. {
                d.boost_t / p.boost_time
            } else {
                1.
            };
            d.boost_scale = (d.boost_start + (1. - d.boost_start) * t)
                .clamp(d.boost_start.min(1.), d.boost_start.max(1.));
        }
        // Rider weight: raw stick Y moves the centre of gravity fore/aft.
        let base = if wreck { p.wreck_cofg } else { p.cofg };
        let y = input.lean;
        let mut z = base[2]
            + if y < 0. {
                -p.travel_forward * y
            } else {
                -p.travel_back * y
            };
        if d.raw_brake {
            z += (d.steer.abs() * p.brake_travel_forward).clamp(0., p.brake_travel_forward.max(0.));
        }
        let dump = if p.boost_time != 0. {
            (p.clutch_travel_back * (1. - d.boost_t / p.boost_time)).clamp(
                0.0f32.min(p.clutch_travel_back),
                p.clutch_travel_back.max(0.),
            )
        } else {
            p.clutch_travel_back
        };
        z -= d.launch * dump;
        // Stick hold durations in frames (source windows are 0..60) for the
        // preload, backflip and three-sixty strengths. Without a trick
        // button the trick stick is the riding stick, as on the source pad.
        if self.trick_mask == 0 {
            self.trick_axes = [input.steer, input.lean];
        }
        let held = [
            self.preload_axis > 0.7,
            self.trick_axes[1].abs() > 0.7,
            self.trick_axes[0].abs() > 0.7,
        ];
        for (hold, held) in d.hold.iter_mut().zip(held) {
            *hold = if held { *hold + dt * 60. } else { 0. };
        }
        let cofg = [base[0], base[1], z];
        self.set_cofg(cofg);
        for (i, tire) in self.params.tires.iter().enumerate() {
            self.springs[i] = tire.spring_rate(
                self.body
                    .linear_velocity
                    .iter()
                    .map(|v| v * v)
                    .sum::<f32>()
                    .sqrt(),
            );
        }
    }

    /// One physics substep: forces, tires and integration.
    pub(super) fn substep(&mut self, terrain: &Terrain, h: f32) -> Result<()> {
        let wreck = self.frame.wreck.phase != crate::wreck::Phase::Riding;
        let mass = self.body_config.mass;
        let p = self.params.clone();
        self.body.force = [0., -p.gravity_scale * p.weight, 0.];
        self.body.local_torque = [0.; 3];
        let grounded = self.grounded_any();
        let speed = self.forward_speed();
        if !wreck {
            self.low_speed_hold(&p);
            self.pitch_control(&p, h);
            self.preload_step(&p, h, mass);
            self.backflip_step(&p, h);
            self.spin_step(&p, h);
        }
        // Engine: drive torque to the rear tire (rear spin from the last step).
        let mut drive = 0.;
        if !wreck {
            let d = &mut self.drive;
            let ratio = p.ratio(d.gear);
            let braking = p.engine_braking * d.rpm * d.rpm;
            let throttle = (d.gas + p.idle_throttle).min(1.);
            let te = d.boost_scale * ((p.torque(d.rpm) + braking) * throttle - braking);
            let previous = d.wheel_rpm;
            d.wheel_rpm = d.tires[1].omega * RPM_PER_RAD * ratio;
            let c = d.clutch;
            if d.locked && (c >= 1. || te.abs() > p.clutch_static * (1. - c)) {
                d.locked = false;
            }
            let mut transmitted = te;
            if !d.locked {
                transmitted =
                    p.clutch_kinetic * (1. - c) * if d.rpm - d.wheel_rpm >= 0. { 1. } else { -1. };
                let crossed = (previous < d.rpm && d.rpm < d.wheel_rpm)
                    || (previous > d.rpm && d.rpm > d.wheel_rpm);
                if c < 1. && crossed {
                    d.locked = true;
                    transmitted = te;
                }
            }
            drive = transmitted * ratio;
            if !d.locked {
                let load = if d.tires[1].spin {
                    (1. - c) * d.tires[1].force * p.tires[1].radius / ratio
                } else {
                    transmitted
                };
                d.rpm += (te - load) / p.engine_inertia * h * RPM_PER_RAD;
            } else {
                d.rpm = d.wheel_rpm;
            }
            d.rpm = d.rpm.clamp(p.min_rpm, p.max_rpm());
        }
        // Aerodynamic drag.
        let v = self.body.linear_velocity;
        let s = dot(v, v).sqrt();
        if s > 0. {
            self.body.force = sub(self.body.force, scale(v, p.drag * 0.5 * s));
        }
        if !wreck {
            self.air_control(&p, grounded);
            if self.drive.reverse && self.drive.tires[1].grounded {
                let f = p.reverse_gain * (p.reverse_speed - speed);
                self.body.force = add(self.body.force, scale(self.body.basis()[2], f));
            }
        }
        self.tires_step(terrain, &p, drive, h, wreck)?;
        // Rev limiter: a locked clutch caps rear wheel speed per gear.
        if self.drive.locked && !wreck {
            let ratio = p.ratio(self.drive.gear);
            if self.drive.tires[1].omega * RPM_PER_RAD * ratio > p.max_rpm() {
                self.drive.tires[1].omega = p.max_rpm() / RPM_PER_RAD / ratio;
            }
        }
        self.integrate(&p, h, wreck)
    }

    fn low_speed_hold(&mut self, p: &Params) {
        let d = &self.drive;
        if !(d.tires.iter().all(|t| t.grounded) && !d.raw_gas && self.forward_speed().abs() < 10.) {
            return;
        }
        let n = d.tires[0].normal;
        let g = [0., -p.gravity_scale * p.weight, 0.];
        let gp = sub(g, scale(n, dot(n, g)));
        let f = self.body.basis()[2];
        let f = unit(sub(f, scale(n, dot(n, f))));
        let along = scale(f, dot(f, gp));
        let mut force = scale(sub(gp, along), -1.);
        if d.raw_brake {
            force = sub(force, along);
        }
        self.body.force = add(self.body.force, force);
    }

    fn pitch_control(&mut self, p: &Params, h: f32) {
        let d = &mut self.drive;
        let [front, rear] = [d.tires[0].grounded, d.tires[1].grounded];
        let speed = dynamics::inverse_transform(self.body.basis(), self.body.linear_velocity)[2];
        d.pitch_control = false;
        if !(front || rear) {
            d.stoppie = false;
            return;
        }
        let pitch = self.body.basis()[2][1].clamp(-1., 1.).asin();
        let rate = self.body.local_angular_velocity[0];
        if d.stoppie && (rear || d.brake == 0. || d.lean > -0.1 || speed <= 7.) {
            d.stoppie = false;
        }
        if !d.stoppie && front && d.brake > 0. && speed > 15. && d.lean <= -0.99 {
            d.stoppie = true;
        }
        let wheelie =
            rear && d.gas > 0. && speed > 5. && d.brake == 0. && d.clutch == 0. && d.lean >= 0.5;
        if wheelie {
            let target = (2. * d.lean - 1.) * p.max_pitch;
            let e = p.pitch_gain * (pitch - target) - rate;
            d.wheelie_i += 500. * e * h;
            let fade = (1. - speed / p.wheelie_top).clamp(0., 1.);
            self.body.local_torque[0] += fade * (p.pitch_rate_gain * e + d.wheelie_i);
            d.pitch_control = true;
        } else {
            d.wheelie_i = 0.;
        }
        if d.stoppie {
            let f = (speed / p.stoppie_top).clamp(0., 1.);
            let target = f * p.max_stoppie * (2. * d.lean + 1.);
            let e = p.stoppie_gain * (pitch - target) - rate;
            d.stoppie_i += 500. * e * h;
            self.body.local_torque[0] += p.pitch_rate_gain * e + d.stoppie_i;
            let n = d.tires[1].normal;
            let n = if dot(n, n) > 0. { n } else { [0., 1., 0.] };
            let omega = self.body.world_angular_velocity();
            let torque = scale(n, 1000. * (p.stoppie_yaw * d.steer - dot(omega, n)));
            let local = dynamics::inverse_transform(self.body.basis(), torque);
            self.body.local_torque = add(self.body.local_torque, local);
            d.pitch_control = true;
        } else {
            d.stoppie_i = 0.;
        }
    }

    fn preload_step(&mut self, p: &Params, h: f32, _mass: f32) {
        let grounded = self.grounded_any();
        let speed = self.forward_speed();
        let rising = self.body.linear_velocity[1];
        let y = self.preload_axis;
        let hold = self.drive.hold[0];
        let pre = &mut self.drive.preload;
        if pre.firing {
            pre.timer -= h;
            if pre.timer > 0. {
                self.body.force[1] += pre.force;
            } else {
                pre.firing = false;
            }
            return;
        }
        let eligible = grounded && rising > 5. && speed > 30. && !self.drive.raw_brake;
        if !eligible {
            return;
        }
        if y > 0.8 {
            pre.armed = true;
            pre.timer = 0.;
        } else if y < -0.8 && pre.armed {
            let [min, max] = p.preload_window;
            pre.force = if min == max {
                -min
            } else {
                (window(hold, [min, max]) * p.max_preload)
                    .clamp(p.max_preload.min(0.), p.max_preload.max(0.))
            };
            pre.timer = p.preload_time;
            pre.armed = false;
            pre.firing = true;
        } else if pre.armed {
            pre.timer += h;
        }
    }

    fn backflip_step(&mut self, p: &Params, h: f32) {
        let grounded = self.grounded_any();
        let speed = self.forward_speed();
        let rising = self.body.linear_velocity[1];
        let (mask, y) = (self.trick_mask, self.trick_axes[1]);
        let hold = self.drive.hold[1];
        let spin_busy = self.drive.spin.active;
        let attitude_ok = self.roll().abs() < std::f32::consts::FRAC_PI_2
            && (-p.air_pitch[0]..p.air_pitch[1]).contains(&self.pitch());
        let early_air = !grounded && self.drive.airtime < 0.6;
        let f = &mut self.drive.backflip;
        if !f.active && early_air && mask & 0xf == 4 && y > 0.8 && !spin_busy {
            // Authored browser adaptation: trick button plus pull-back early in
            // a jump starts the flip at its minimum strength, keeping the
            // port's airborne trick request alongside the source ramp gesture.
            *f = Spin {
                active: true,
                airborne: true,
                torque: [-1.25 * p.backflip[0], 0.],
                ..Spin::default()
            };
        } else if !f.active {
            if !grounded || spin_busy || rising <= 5. || speed <= 30. || self.drive.raw_brake {
                f.armed = false;
            } else if y < -0.8 && mask & 0xf == 4 {
                f.armed = true;
            } else if y > 0.8 && f.armed {
                let t = window(hold, p.backflip_window);
                let torque = (1.25 * p.backflip[0] + (p.backflip[1] - 1.25 * p.backflip[0]) * t)
                    .clamp(
                        (1.25 * p.backflip[0]).min(p.backflip[1]),
                        (1.25 * p.backflip[0]).max(p.backflip[1]),
                    );
                *f = Spin {
                    active: true,
                    torque: [-torque, 0.],
                    timer: 0.6,
                    ..Spin::default()
                };
            }
        } else if !f.airborne {
            if !grounded {
                f.airborne = true;
            } else {
                f.timer -= h;
                if f.timer <= 0. {
                    *f = Spin::default();
                }
            }
        } else if grounded || f.torque[0].abs() < p.backflip[0] {
            let recovering = f.recovering && !grounded;
            *f = Spin::default();
            f.recovering = recovering;
            if grounded {
                self.drive.recovery = 1.;
            }
        } else {
            if mask == 0 && y <= 0. {
                f.torque[0] -= h * y * p.backflip_decrease;
            }
            self.body.local_torque[0] += f.torque[0];
            f.recovering = true;
        }
        if f.recovering && !f.active && (attitude_ok || grounded) {
            f.recovering = false;
            self.drive.recovery = 1.;
        }
    }

    fn spin_step(&mut self, p: &Params, h: f32) {
        let grounded = self.grounded_any();
        let speed = self.forward_speed();
        let rising = self.body.linear_velocity[1];
        let (mask, x) = (self.trick_mask, self.trick_axes[0]);
        let hold = self.drive.hold[2];
        let flipping = self.drive.backflip.active;
        let early_air = !grounded && self.drive.airtime < 0.6;
        let s = &mut self.drive.spin;
        if !s.active && early_air && mask & 0xf == 4 && x.abs() > 0.7 && !flipping {
            // Authored browser adaptation, as for the backflip: trick button
            // plus a side flick early in a jump spins toward the flick.
            self.drive.spin_dir = x.signum();
            *s = Spin {
                active: true,
                airborne: true,
                torque: [
                    -p.spin_x[0],
                    x.signum() * p.spin_y[0].max(0.5 * p.spin_y[1]),
                ],
                ..Spin::default()
            };
        } else if !s.active {
            if !grounded || flipping || rising <= 5. || speed <= 30. || self.drive.raw_brake {
                s.armed = false;
            } else if mask & 0xf == 4 && x.abs() > 0.7 && !s.armed {
                s.armed = true;
                self.drive.spin_dir = -x.signum();
            } else if s.armed && x.abs() > 0.7 && x.signum() == self.drive.spin_dir {
                let t = window(hold, p.spin_window);
                let lerp = |[a, b]: [f32; 2]| a + (b - a) * t;
                *s = Spin {
                    active: true,
                    torque: [-lerp(p.spin_x), self.drive.spin_dir * lerp(p.spin_y)],
                    timer: 0.9,
                    ..Spin::default()
                };
            }
        } else if !s.airborne {
            if !grounded {
                s.airborne = true;
            } else {
                s.timer -= h;
                if s.timer <= 0. {
                    *s = Spin::default();
                }
            }
        } else if grounded || s.torque[1].abs() < p.spin_y[0] {
            *s = Spin::default();
            if grounded {
                self.drive.recovery = 1.;
            }
        } else {
            let dir = self.drive.spin_dir;
            if mask == 0 && x.signum() == -dir && x != 0. {
                s.torque[1] -= h * p.spin_decrease * dir * x.abs();
                s.torque[0] -= h * x * p.spin_decrease;
            }
            self.body.local_torque[0] += s.torque[0];
            self.body.local_torque[1] += s.torque[1];
            s.recovering = true;
        }
    }

    fn air_control(&mut self, p: &Params, grounded: bool) {
        if grounded {
            return;
        }
        let basis = self.body.basis();
        let omega = self.body.local_angular_velocity;
        let d = &mut self.drive;
        let mut fade = 1.;
        if d.recovery > 0. {
            fade = (1. - d.recovery).clamp(0., 1.);
        }
        let flipping = d.backflip.active || d.spin.active;
        let mut torque = [0.; 3];
        if !(d.backflip.active && d.spin.active) {
            let [x, y] = [d.stick[0], d.stick[1]];
            let m = p.air_moment;
            let k = if self.active_stunt.is_some() {
                [0.5, 0.4, 0.4]
            } else {
                [1.; 3]
            };
            torque = [y * m[0] * k[0], x * m[1] * k[1], x * m[2] * k[2]];
            if d.backflip.active {
                torque[0] = 0.;
            }
            if d.backflip.recovering {
                torque[1] = 0.;
            }
            torque = torque.map(|t| t * fade);
        }
        let steep = basis[2][1].abs() >= 0.9396926;
        if steep || d.steer.abs() >= 0.1 || flipping || d.backflip.recovering || d.recovery > 0. {
            for i in 0..3 {
                torque[i] -= p.air_damping * omega[i];
            }
        } else {
            // Auto-level: unroll toward upright, unyaw heading toward travel.
            let roll = -basis[0][1].atan2(basis[1][1]);
            torque[2] += p.unroll_torque * (p.unroll_gain * roll - omega[2]);
            torque[1] += p.unyaw_torque * (p.unyaw_gain * d.crab - omega[1]);
            if omega[0].abs() > 0.1 {
                torque[0] -= p.air_damping * omega[0];
            }
            if omega[1].abs() > 0.1 {
                torque[1] -= p.air_damping * omega[1];
            }
        }
        if self.active_stunt.is_none() && d.brake > 0. {
            torque[0] += p.brake_gyro;
        }
        self.body.local_torque = add(self.body.local_torque, torque);
        let v = self.body.linear_velocity;
        if self.active_stunt.is_none() && !flipping && dot(v, v).sqrt() > 10. {
            let side = crate::wheel::cross([0., 1., 0.], v);
            if dot(side, side) > 1e-6 {
                let f = scale(unit(side), p.air_force * d.steer);
                self.body.force = add(self.body.force, f);
            }
        }
    }

    fn tires_step(
        &mut self,
        terrain: &Terrain,
        p: &Params,
        drive: f32,
        h: f32,
        wreck: bool,
    ) -> Result<()> {
        let basis = self.body.basis();
        let world_up_local = dynamics::inverse_transform(basis, [0., 1., 0.]);
        let strut = unit([0., world_up_local[1], world_up_local[2]]);
        let up_world = basis[1];
        for i in 0..2 {
            let tire = &p.tires[i];
            let com = self.body_config.center_of_mass;
            let mount = add(
                self.body.center,
                dynamics::transform(basis, sub(self.model.wheel_rest[i], com)),
            );
            let bottom = add(
                self.body.center,
                dynamics::transform(
                    basis,
                    sub(
                        sub(self.model.wheel_rest[i], scale(strut, tire.radius)),
                        com,
                    ),
                ),
            );
            let mut state = self.drive.tires[i];
            state.grounded = false;
            state.borrowed = false;
            state.load = 0.;
            let mut contact = terrain
                .contact(bottom[0], bottom[2])?
                .map(|c| (c.height, c.normal, c.surface as usize));
            // Authored adapter: solid props from the existing map collision.
            let ray = crate::collision::CollisionRay {
                start: add(mount, [0., tire.strut, 0.]),
                end: add(bottom, [0., -0.5, 0.]),
            };
            for hit in crate::wreck::query_static_segments(&[ray], &self.obstacles, 16)? {
                if hit.world_normal[1] > 0.17 && contact.is_none_or(|c| hit.world_point[1] > c.0) {
                    contact = Some((hit.world_point[1], hit.world_normal, 0));
                }
            }
            if let Some((height, normal, surface)) = contact {
                state.surface = surface.min(crate::motorcycle_tire::SURFACES - 1);
                state.normal = normal;
                let d = sub(mount, bottom);
                let dn = dot(d, normal);
                let ground = [bottom[0], height, bottom[2]];
                let t = if height >= bottom[1] && dn > 1e-6 {
                    dot(sub(ground, bottom), normal) / dn
                } else {
                    0.
                };
                let local_n = dynamics::inverse_transform(basis, normal);
                let c = local_n[1]
                    / (local_n[0] * local_n[0] + local_n[1] * local_n[1])
                        .sqrt()
                        .max(1e-6);
                let mut compression = t * tire.radius * Tire::lean_factor(c);
                if compression > 0. {
                    let mut point = add(bottom, scale(d, t));
                    state.grounded = true;
                    if compression > tire.strut {
                        // Bottoming: push out 90 % of the overshoot, remove
                        // approach speed along the bike up axis, and cancel
                        // accumulated force into the ground.
                        let over = compression - tire.strut;
                        self.body.center = add(self.body.center, scale(normal, over * 0.9));
                        point = add(point, scale(normal, over * 0.9));
                        let vn = dot(self.body.linear_velocity, normal);
                        if vn < 0. {
                            let change = scale(up_world, -vn);
                            self.body.linear_velocity = add(self.body.linear_velocity, change);
                            let local = dynamics::inverse_transform(basis, change);
                            self.drive.latch_impact(local);
                            self.frame.crash_impact_speed = self.frame.crash_impact_speed.max(-vn);
                        }
                        let into = dot(self.body.force, normal);
                        if into < 0. {
                            self.body.force = sub(self.body.force, scale(normal, into));
                        }
                        compression = tire.strut;
                    }
                    state.compression = compression;
                    state.point = point;
                    let vn = dot(self.body.velocity_at(point), normal);
                    state.load = tire.normal_load(self.springs[i], compression, vn);
                    self.body
                        .apply_world_force(scale(normal, state.load), point);
                }
            }
            self.drive.tires[i] = state;
        }
        // A grounded front lends its load to an airborne rear (endos/stoppies).
        if self.drive.tires[0].grounded && !self.drive.tires[1].grounded {
            let front = self.drive.tires[0];
            let rear = &mut self.drive.tires[1];
            rear.borrowed = true;
            rear.load = p.tires[1].other_load_scale * front.load;
            if dot(rear.normal, rear.normal) == 0. {
                rear.normal = front.normal;
            }
            rear.point = add(
                self.body.center,
                dynamics::transform(
                    basis,
                    sub(
                        sub(self.model.wheel_rest[1], scale(strut, p.tires[1].radius)),
                        self.body_config.center_of_mass,
                    ),
                ),
            );
        }
        let v_body = dynamics::inverse_transform(basis, self.body.linear_velocity)[2];
        for i in 0..2 {
            let tire = &p.tires[i];
            let state = self.drive.tires[i];
            let contact = state.grounded || state.borrowed;
            // Surface adapter: ATV2 maps number their eight terrain zones per
            // map, so MX zone 1 (standard track dirt) is scaled by the map's
            // own SurfaceFriction/Traction/Braking/Drag for this zone.
            let zone = 0;
            let material = self.materials.and_then(|m| m.get(state.surface).copied());
            let (grip, traction, braking, surface_drag) =
                material.map_or((1., 1., 1., 0.), |m| (m.grip, m.drive, m.braking, m.drag));
            let brake = if wreck { 0.1 } else { self.drive.brake } * braking;
            let sigma = if contact {
                (0.3 * v_body).clamp(-1., 1.)
            } else {
                state.omega.clamp(-1., 1.)
            };
            let mut torque = tire.resistance_torque(zone, sigma, brake);
            if i == 1 {
                torque += drive;
            }
            let wheel_drag = tire.wheel_drag[zone] + surface_drag;
            if contact && wheel_drag != 0. {
                let f = -v_body.signum() * wheel_drag * v_body * v_body;
                self.body.force = add(self.body.force, scale(basis[2], f));
            }
            let mut out = state;
            out.torque = torque;
            if contact {
                // Tire frame: heading (front steered about body Y unless pitch
                // control owns the front), projected onto the ground plane.
                let n = state.normal;
                let heading = if i == 0 && !self.drive.pitch_control {
                    add(
                        scale(basis[2], self.drive.delta.cos()),
                        scale(basis[0], self.drive.delta.sin()),
                    )
                } else {
                    basis[2]
                };
                let long = unit(sub(heading, scale(n, dot(heading, n))));
                let lat = crate::wheel::cross(n, long);
                let v = self.body.velocity_at(state.point);
                let (v_lat, v_long) = (dot(v, lat), dot(v, long));
                let donut = i == 1 && self.drive.donut > 0.;
                let l = tire.longitudinal(zone, torque, state.load, traction, donut);
                let side_state = if brake > 0. {
                    SideState::Braking
                } else if l.spin || l.lock {
                    SideState::Slipping {
                        donut: if i == 1 { self.drive.donut } else { 0. },
                    }
                } else {
                    SideState::Rolling
                };
                let f_lat = grip * tire.lateral(zone, v_lat, v_long, state.load, side_state);
                let force = add(scale(lat, f_lat), scale(long, l.force));
                self.body.force = add(self.body.force, force);
                let local = dynamics::inverse_transform(basis, force);
                self.body.local_torque = add(
                    self.body.local_torque,
                    crate::wheel::cross(tire.force_position, local),
                );
                out.spin = l.spin;
                out.lock = l.lock;
                out.force = l.force;
                out.omega = if l.lock {
                    0.
                } else if !l.spin {
                    v_long / tire.radius
                } else {
                    out.omega + h * (torque - l.force * tire.radius) / tire.spin_inertia
                };
            } else {
                out.spin = false;
                out.lock = false;
                out.force = 0.;
                out.omega += h * torque / tire.spin_inertia;
            }
            out.angle = (out.angle + out.omega * h + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            // Display compression relaxes at MaxDecompressionRate.
            let target = if state.grounded {
                state.compression
            } else {
                0.
            };
            out.display = if target < out.display {
                out.display - (out.display - target).min(tire.max_decompression_rate * h)
            } else {
                target
            }
            .min(tire.strut);
            self.drive.tires[i] = out;
        }
        Ok(())
    }

    /// Constant-acceleration integration; body-frame angular-rate clamps of
    /// (20, YawRateLimit, 20); roll locked while riding on the ground.
    fn integrate(&mut self, p: &Params, h: f32, wreck: bool) -> Result<()> {
        let b = &mut self.body;
        if !b
            .force
            .iter()
            .chain(b.local_torque.iter())
            .all(|x| x.is_finite())
        {
            return Err("invalid motorcycle forces".into());
        }
        let grounded = self.drive.tires.iter().any(|t| t.grounded);
        let lock_roll = grounded && !wreck;
        let a = scale(b.force, 1. / self.body_config.mass);
        b.center = add(
            b.center,
            add(scale(b.linear_velocity, h), scale(a, 0.5 * h * h)),
        );
        b.linear_velocity = add(b.linear_velocity, scale(a, h));
        let speed2 = dot(b.linear_velocity, b.linear_velocity);
        if speed2 > self.body_config.max_speed_squared {
            b.linear_velocity = scale(
                b.linear_velocity,
                (self.body_config.max_speed_squared / speed2).sqrt(),
            );
        }
        let inertia = self.body_config.inertia;
        let mut alpha: V = std::array::from_fn(|i| b.local_torque[i] / inertia[i]);
        if lock_roll {
            alpha[2] = 0.;
            b.local_angular_velocity[2] = 0.;
        }
        // Midpoint rate for the quaternion step (host approximation of the
        // source's second-order term).
        let w = add(b.local_angular_velocity, scale(alpha, 0.5 * h));
        let [qw, qx, qy, qz] = b.orientation;
        let [x, y, z] = w;
        let dq = [
            ((qz * z + qx * x) + qy * y) * -0.5,
            ((qw * x + qy * z) - qz * y) * 0.5,
            ((qw * y + qz * x) - qx * z) * 0.5,
            ((qw * z + qx * y) - qy * x) * 0.5,
        ];
        let q: [f32; 4] = std::array::from_fn(|i| b.orientation[i] + dq[i] * h);
        let norm = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        b.orientation = q.map(|v| v / norm);
        let limits = [20., p.yaw_rate_limit, 20.];
        for i in 0..3 {
            b.local_angular_velocity[i] =
                (b.local_angular_velocity[i] + alpha[i] * h).clamp(-limits[i], limits[i]);
        }
        if wreck {
            b.linear_velocity = scale(b.linear_velocity, 0.999);
            b.local_angular_velocity = b.local_angular_velocity.map(|w| w * 0.999);
        }
        b.force = [0.; 3];
        b.local_torque = [0.; 3];
        Ok(())
    }

    /// Once per frame after physics: kinematic bank and derived state.
    pub(super) fn finish(&mut self, dt: f32) {
        let wreck = self.frame.wreck.phase != crate::wreck::Phase::Riding;
        let tire = self
            .drive
            .tires
            .iter()
            .find(|t| t.grounded && t.normal[1] > 0.17)
            .copied();
        if !wreck && let Some(tire) = tire {
            let (max_roll, max_roll_speed, roll_gain, donut_roll) = (
                self.params.max_roll,
                self.params.max_roll_speed,
                self.params.roll_gain,
                self.params.donut_roll,
            );
            let speed = self.forward_speed();
            let reference = self.bank_reference(tire.normal, speed);
            let mut command = if max_roll_speed == 0. {
                0.
            } else {
                (speed / max_roll_speed * max_roll).clamp(max_roll.min(0.), max_roll.max(0.))
            };
            let donut = self.drive.donut;
            if donut > 0. {
                let target = donut_roll * max_roll;
                command = (command + (target - command) * donut)
                    .clamp(command.min(target), command.max(target));
            }
            let current = balance::angle(self.body.basis(), reference);
            let next = current - roll_gain * dt * (current + command * self.drive.steer);
            self.body.orientation = balance::rotate(self.body.orientation, next - current);
        }
        let grounded = self.grounded_any();
        let d = &mut self.drive;
        let airborne = !grounded;
        if airborne {
            if !d.was_airborne {
                d.airtime = 0.;
            }
            d.airtime += dt;
        }
        // Crab: signed angle between horizontal heading and travel.
        let v = self.body.linear_velocity;
        let f = self.body.basis()[2];
        let (fh, vh) = ([f[0], 0., f[2]], [v[0], 0., v[2]]);
        let speed = dot(v, v).sqrt();
        d.crab = if speed > 10. && !d.backflip.active && !d.spin.active {
            let c = crate::wheel::cross(unit(fh), unit(vh));
            let s = dot(c, c).sqrt().clamp(0., 1.).asin();
            if c[1] >= 0. { s } else { -s }
        } else {
            0.
        };
        if d.recovery > 0. {
            d.crab *= (1. - d.recovery).clamp(0., 1.);
            d.recovery = (d.recovery - dt).max(0.);
        }
    }

    /// Wreck decision chain, once per frame. Returns a wreck kind to begin.
    pub(super) fn wreck_check(&mut self) -> Option<crate::wreck::Kind> {
        use crate::wreck::Kind;
        let p = &self.params;
        let d = &self.drive;
        let grounded = self.grounded_any();
        let pitch = self.pitch();
        let roll = self.roll();
        let tricking = d.backflip.active || d.spin.active;
        let imp = d.impact;
        let landing = d.was_airborne && grounded;
        let crossup = ((d.crab + p.max_crossup) / (2. * p.max_crossup)).clamp(0., 1.);
        let kind = if imp[2] > p.back_limit {
            Some(Kind::Back)
        } else if imp[2] < -p.front_limit {
            Some(Kind::Front)
        } else if imp[0] > p.side_limit {
            Some(Kind::Left)
        } else if imp[0] < -p.side_limit {
            Some(Kind::Right)
        } else if imp[1] > p.down_limit {
            Some(Kind::Vertical)
        } else if grounded && pitch < -p.ground_pitch[0] {
            Some(Kind::Front)
        } else if grounded && pitch > p.ground_pitch[1] {
            Some(Kind::Back)
        } else if !grounded && !tricking && (pitch < -p.air_pitch[0] || roll.abs() > p.air_roll) {
            Some(Kind::Vertical)
        } else if !grounded && !tricking && pitch > p.air_pitch[1] {
            Some(Kind::Back)
        } else if landing
            && d.airtime > p.crossup_time
            && (crossup > p.crossup_wreck || crossup < 1. - p.crossup_wreck)
        {
            Some(if crossup > 0.5 {
                Kind::Left
            } else {
                Kind::Right
            })
        } else {
            None
        };
        let d = &mut self.drive;
        d.was_airborne = !grounded;
        d.impact = [0.; 3];
        kind
    }
}
