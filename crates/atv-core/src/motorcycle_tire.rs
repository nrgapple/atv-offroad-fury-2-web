//! Motocross tire and suspension model recovered from SLUS_211.04/GAME_REL
//! (tire class around 004a0030; parser 004dae60; suspension 004a16f0; wheel
//! torques 004a1ec0; tire forces 004a25b0/004a2040; stiffness blend 004d8b20).
//! Units are ft, s, slug, lbf. Host trigonometry/powf and the authored terrain
//! adapter mean this is component recovery, not native whole-step parity.
use crate::math::Det;
use crate::{Result, physics_data::PhysicsData};

pub const SURFACES: usize = 16;
type Zones = [f32; SURFACES];

/// One side-force curve set: parametric (RoundOff, DropOff, Scale, SpeedScale).
#[derive(Debug, Clone, Copy, Default)]
pub struct Curve {
    pub round_off: Zones,
    pub drop_off: Zones,
    pub scale: Zones,
    pub speed_scale: Zones,
}

#[derive(Debug, Clone, Copy)]
pub struct Tire {
    pub parametric: bool,
    pub strut: f32,
    pub max_compression_rate: f32,
    pub max_decompression_rate: f32,
    pub radius: f32,
    /// Spring rates (lbf/ft) moving and stopped; damping (lbf·s/ft) on
    /// extension (`BounceDampingRatio`) and compression (`ReboundDampingRatio`).
    pub spring: f32,
    pub spring_stopped: f32,
    pub extension_damping: f32,
    pub compression_damping: f32,
    pub max_steer: f32,
    pub force_position: [f32; 3],
    pub other_load_scale: f32,
    /// Wheel spin inertia from the source's fixed 6 in tire width.
    pub spin_inertia: f32,
    pub side: Zones,
    pub max_side: Zones,
    pub skid_side: Zones,
    pub slipping_side: Zones,
    pub max_slipping_side: Zones,
    pub skid_slipping_side: Zones,
    pub max_drive: Zones,
    pub skid_drive: Zones,
    pub normal: Curve,
    pub slipping: Curve,
    pub braking: Curve,
    pub donut_speed_scale: Zones,
    pub max_braking_torque: Zones,
    pub wheel_friction: Zones,
    pub wheel_drag: Zones,
}

impl Tire {
    pub fn parse(data: &PhysicsData, section: &str, weight: f32, rear: bool) -> Result<Self> {
        let or = |key: &str, default: f32| data.first_scalar(section, key).unwrap_or(default);
        let zones = |key: &str| -> Zones {
            std::array::from_fn(|i| {
                data.first_scalar(section, &format!("{key}{}", i + 1))
                    .unwrap_or(0.)
            })
        };
        let curve = |prefix: &str| Curve {
            round_off: zones(&format!("{prefix}RoundOff")),
            drop_off: zones(&format!("{prefix}DropOff")),
            scale: zones(&format!("{prefix}Scale")),
            speed_scale: zones(&format!("{prefix}SpeedScale")),
        };
        let mass = weight / 32.2;
        let spring = (or("Spring", 0.) * weight).abs();
        let critical = 2. * (mass * spring).sqrt();
        let radius = or("TireRadius", 1.);
        let width = 0.5;
        let tire = Self {
            parametric: data
                .first(section, "UseParametricCurve")
                .is_ok_and(|e| e.value.trim().eq_ignore_ascii_case("true")),
            strut: or("StrutCompression", 1.),
            max_compression_rate: or("MaxCompressionRate", 20.),
            max_decompression_rate: or("MaxDecompressionRate", 1.),
            radius,
            spring,
            spring_stopped: (or("SpringWhenStopped", 0.) * weight).abs(),
            extension_damping: or("BounceDampingRatio", 0.) * critical,
            compression_damping: or("ReboundDampingRatio", 0.) * critical,
            max_steer: or("MaxSteerAngle", 0.).to_radians(),
            force_position: data
                .vector3(section, "BodyTireForcePosition")
                .unwrap_or([0.; 3]),
            other_load_scale: or("OtherTireNormalLoadScale", 1.),
            spin_inertia: std::f32::consts::PI * radius.powi(4) * width * width,
            side: zones("SideForce"),
            max_side: zones("MaxSideForce"),
            skid_side: zones("SkidSideForce"),
            slipping_side: zones("SlippingSideForce"),
            max_slipping_side: zones("MaxSlippingSideForce"),
            skid_slipping_side: zones("SkidSlippingSideForce"),
            max_drive: zones("MaxDriveForce"),
            skid_drive: zones("SkidDriveForce"),
            normal: curve("SideForce"),
            slipping: curve("SlippingSideForce"),
            braking: curve("BrakingSideForce"),
            // The front stores no donut scale; only the rear reads one.
            donut_speed_scale: if rear {
                zones("DonutsSideForceSpeedScale")
            } else {
                [0.; SURFACES]
            },
            max_braking_torque: zones("MaxBrakingTorque"),
            wheel_friction: zones("WheelFriction"),
            wheel_drag: zones("WheelDrag"),
        };
        if !(tire.radius > 0. && tire.strut > 0. && tire.spring > 0.) {
            return Err("invalid motorcycle tire".into());
        }
        Ok(tire)
    }

    /// Stopped stiffness blends to the moving stiffness between 10 and 20 mph.
    pub fn spring_rate(&self, speed: f32) -> f32 {
        let (lo, hi) = (
            self.spring_stopped.min(self.spring),
            self.spring_stopped.max(self.spring),
        );
        (self.spring_stopped + (speed - 14.6666) / 14.6666 * (self.spring - self.spring_stopped))
            .clamp(lo, hi)
    }

    /// Normal load from compression (ft) and contact-point normal velocity
    /// (ft/s, positive when separating). Rebound damping acts while
    /// compressing and bounce damping while extending, with the damper speed
    /// clamped symmetrically by MaxCompressionRate. There is no force cap.
    pub fn normal_load(&self, spring: f32, compression: f32, normal_velocity: f32) -> f32 {
        let rate = normal_velocity.clamp(-self.max_compression_rate, self.max_compression_rate);
        let damping = if rate > 0. {
            self.extension_damping
        } else {
            self.compression_damping
        };
        (spring * compression - damping * rate).max(0.)
    }

    /// Rolled bikes compress less: full until ~50° against the ground, half from 60°.
    pub fn lean_factor(cos_ground_roll: f32) -> f32 {
        ((cos_ground_roll - 0.5) / 0.14 * 0.5 + 0.5).clamp(0.5, 1.)
    }

    /// Wheel torque from friction and brake input, before engine torque.
    /// `sigma` saturates body forward speed (grounded) or wheel spin (air).
    pub fn resistance_torque(&self, zone: usize, sigma: f32, brake: f32) -> f32 {
        let mut torque = -sigma * self.wheel_friction[zone];
        if brake > 0. {
            torque -= sigma * self.max_braking_torque[zone] * brake;
        }
        torque
    }

    /// Longitudinal force threshold model: torque passes through until it
    /// exceeds MaxDriveForce·N, then the tire spins/locks at SkidDriveForce·N.
    pub fn longitudinal(
        &self,
        zone: usize,
        torque: f32,
        load: f32,
        traction: f32,
        donut: bool,
    ) -> Longitudinal {
        let desired = torque / self.radius;
        let load = load * traction;
        let limit = self.max_drive[zone] * load;
        if desired > limit || (donut && desired > 0.) {
            Longitudinal {
                force: self.skid_drive[zone] * load,
                spin: true,
                lock: false,
            }
        } else if desired < -limit && limit > 0. {
            Longitudinal {
                force: -self.skid_drive[zone] * load,
                spin: false,
                lock: true,
            }
        } else {
            Longitudinal {
                force: desired,
                spin: false,
                lock: false,
            }
        }
    }

    /// Lateral force (lbf) from contact lateral speed (ft/s, tire frame).
    pub fn lateral(
        &self,
        zone: usize,
        lateral_speed: f32,
        longitudinal_speed: f32,
        load: f32,
        state: SideState,
    ) -> f32 {
        let mut v = lateral_speed;
        if longitudinal_speed.abs() < 10. {
            v *= (5. - 0.4 * longitudinal_speed.abs()).clamp(1., 5.);
        }
        if self.parametric {
            let (curve, speed_scale) = match state {
                SideState::Braking => (&self.braking, self.braking.speed_scale[zone]),
                SideState::Slipping { donut } => {
                    let a = self.slipping.speed_scale[zone];
                    let b = self.donut_speed_scale[zone];
                    let s = a + (b - a) * donut;
                    (&self.slipping, s.clamp(a.min(b), a.max(b)))
                }
                SideState::Rolling => (&self.normal, self.normal.speed_scale[zone]),
            };
            let x = v * curve.round_off[zone] * speed_scale;
            -v * speed_scale * curve.scale[zone] * load
                / (1. + x.abs().powf_d(curve.drop_off[zone]))
        } else {
            let (gain, max, skid) = match state {
                SideState::Rolling => (self.side, self.max_side, self.skid_side),
                _ => (
                    self.slipping_side,
                    self.max_slipping_side,
                    self.skid_slipping_side,
                ),
            };
            let force = -v * gain[zone] * load;
            if force.abs() > max[zone] * load {
                force.signum() * skid[zone] * load
            } else {
                force
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Longitudinal {
    pub force: f32,
    pub spin: bool,
    pub lock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SideState {
    Rolling,
    Slipping { donut: f32 },
    Braking,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tire() -> Tire {
        let text = b"[Front Tire]\nUseParametricCurve = True\nStrutCompression = 1\nTireRadius = 1.2\nSpring = 1.65\nSpringWhenStopped = 1.1\nBounceDampingRatio = 0.3\nReboundDampingRatio = 0.3\nMaxSteerAngle = 30\nMaxDriveForce1 = 2\nSkidDriveForce1 = 1.9\nSideForceRoundOff1 = 4\nSideForceDropOff1 = 1.5\nSideForceScale1 = 41\nSideForceSpeedScale1 = 0.006\nMaxBrakingTorque1 = 325\nWheelFriction1 = 10\n";
        Tire::parse(
            &PhysicsData::parse(text).unwrap(),
            "Front Tire",
            400.,
            false,
        )
        .unwrap()
    }
    #[test]
    fn stiffness_blends_between_ten_and_twenty_mph() {
        let t = tire();
        assert_eq!(t.spring_rate(0.), 440.);
        assert_eq!(t.spring_rate(40.), 660.);
        assert!((t.spring_rate(22.) - 550.).abs() < 0.1);
    }
    #[test]
    fn damping_uses_rebound_while_compressing_and_never_pulls() {
        let t = tire();
        assert!(t.normal_load(660., 0.3, -2.) > 660. * 0.3);
        assert_eq!(t.normal_load(660., 0.01, 30.), 0.);
    }
    #[test]
    fn side_curve_is_a_saturating_damper() {
        let t = tire();
        let f = t.lateral(0, 1., 60., 100., SideState::Rolling);
        assert!((f + 0.246 * 100. / (1. + 0.024f32.powf_d(1.5))).abs() < 0.01);
        assert!(t.lateral(0, -1., 60., 100., SideState::Rolling) > 0.);
        // Standstill amplifies lateral speed five times.
        let slow = t.lateral(0, 1., 0., 100., SideState::Rolling);
        assert!(slow < 4.5 * f);
    }
    #[test]
    fn traction_threshold_spins_and_locks() {
        let t = tire();
        assert!(!t.longitudinal(0, 100., 200., 1., false).spin);
        let spin = t.longitudinal(0, 1000., 200., 1., false);
        assert!(spin.spin && (spin.force - 380.).abs() < 0.01);
        assert!(t.longitudinal(0, -1000., 200., 1., false).lock);
    }
}
