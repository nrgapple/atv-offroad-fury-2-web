//! Authored off-road tire presets. Contact forces are in lbf, relative slip
//! speeds in ft/s. Tread is scoped to one vehicle instance (one ride).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Handling {
    #[default]
    Original,
    Sport,
    Simulation,
}

impl Handling {
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Sport => "sport",
            Self::Simulation => "simulation",
        }
    }

    /// Progressive bump resistance starts at 70% travel. The recovered
    /// spring and damper still carry the vehicle through normal travel.
    pub fn spring_scale(self, travel_fraction: f32) -> f32 {
        let rise = match self {
            Self::Original => return 1.,
            Self::Sport => 0.35,
            Self::Simulation => 0.65,
        };
        let end_stroke = ((travel_fraction - 0.7) / 0.3).clamp(0., 1.);
        1. + rise * end_stroke * end_stroke
    }

    pub fn cycle(self, direction: i32) -> Self {
        let modes = [Self::Original, Self::Sport, Self::Simulation];
        modes[(self as i32 + direction.signum()).rem_euclid(3) as usize]
    }
}

#[derive(Clone, Copy, Debug)]
struct TireContact {
    pub demand: [f32; 2],
    pub slip: [f32; 2],
    pub normal_load: f32,
    pub static_load: f32,
    pub friction: f32,
}

/// Wheel and driveline quantities in ft, slug, seconds and lbf.
#[derive(Clone, Copy)]
pub struct WheelDrive {
    pub omega: f32,
    pub radius: f32,
    pub inverse_inertia: f32,
    pub drive_torque: f32,
    pub brake_torque: f32,
    pub bearing_torque: f32,
    pub static_load: f32,
    pub air_speed: f32,
}

#[derive(Clone, Copy, Default)]
pub struct ContactPatch {
    pub forward_speed: f32,
    pub lateral_speed: f32,
    pub lateral_demand: f32,
    pub load: f32,
    pub friction: f32,
}

pub struct WheelStep {
    pub omega: f32,
    pub forces: [f32; 2],
    pub brake_torque: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Tires<const N: usize> {
    pub mode: Handling,
    wear: [f32; N],
    brake_heat: [f32; N],
    shock_wear: [f32; N],
}

impl<const N: usize> Default for Tires<N> {
    fn default() -> Self {
        Self {
            mode: Handling::Original,
            wear: [0.; N],
            brake_heat: [0.; N],
            shock_wear: [0.; N],
        }
    }
}

impl<const N: usize> Tires<N> {
    pub fn worst_wear(&self) -> f32 {
        self.wear.iter().copied().fold(0_f32, f32::max)
    }

    pub fn worst_shock_wear(&self) -> f32 {
        self.shock_wear.iter().copied().fold(0., f32::max)
    }

    /// Severe compression near the travel stop fatigues this corner's
    /// damper. Engagement rises smoothly through the final quarter of travel.
    pub fn damping_scale(&mut self, i: usize, travel: f32, closing_speed: f32, dt: f32) -> f32 {
        let distance = match self.mode {
            Handling::Original => return 1.,
            Handling::Sport => 6000.,
            Handling::Simulation => 2200.,
        };
        let end_stroke = ((travel - 0.75) * 4.).clamp(0., 1.);
        let excess = (closing_speed - 8.).clamp(0., 50.);
        let wear = &mut self.shock_wear[i];
        *wear = (*wear + end_stroke * excess * excess * dt / distance).min(1.);
        1. - 0.22 * *wear
    }

    /// Travel stops clip inward velocity in a single solver impulse. Charge
    /// the lost energy once, independently of the integration step length.
    pub fn hit_stop(&mut self, i: usize, closing_speed: f32) {
        let service_distance = match self.mode {
            Handling::Original => return,
            Handling::Sport => 1600.,
            Handling::Simulation => 600.,
        };
        let excess_energy = (closing_speed.max(0.).powi(2) - 12_f32.powi(2)).max(0.);
        self.shock_wear[i] =
            (self.shock_wear[i] + excess_energy / (2. * 32.2 * service_distance)).min(1.);
    }

    /// Artistic rotor emission. Make heat visible before fade begins.
    pub fn brake_glow(&self) -> [f32; N] {
        if self.mode == Handling::Original {
            return [0.; N];
        }
        self.brake_heat.map(|heat| {
            let warm = (heat - 0.02).max(0.);
            warm / (warm + 0.09)
        })
    }

    pub fn hottest_brake(&self) -> f32 {
        self.brake_heat.iter().copied().fold(0., f32::max)
    }

    /// Predict free wheel rotation, resolve the slip impulse against available
    /// grip, then return the equal and opposite torque to the wheel. A brake
    /// can hold a stopped wheel but never spin it backwards on its own.
    pub fn step(&mut self, i: usize, wheel: WheelDrive, patch: ContactPatch, dt: f32) -> WheelStep {
        let fade = ((self.brake_heat[i] - 0.55) / 0.65).clamp(0., 1.);
        let max_loss = if self.mode == Handling::Sport {
            0.12
        } else {
            0.35
        };
        let brake = wheel.brake_torque.max(0.) * (1. - max_loss * fade);
        let resist = brake + wheel.bearing_torque.max(0.);
        let angular_step = dt * wheel.inverse_inertia;
        let slow = |spin: f32| spin - spin.clamp(-resist * angular_step, resist * angular_step);
        // Sport eases power when a driven tire is already spinning markedly
        // faster than the ground. The rider still controls brake and steer.
        let overspin =
            (wheel.omega * wheel.radius - patch.forward_speed) * wheel.drive_torque.signum();
        let throttle_scale = if self.mode == Handling::Sport && patch.load > 0. {
            (1. - (overspin - 4.).max(0.) / 16.).clamp(0.15, 1.)
        } else {
            1.
        };
        let driven = wheel.omega + angular_step * wheel.drive_torque * throttle_scale;
        let slip = slow(driven) * wheel.radius - patch.forward_speed;
        // Effective mass of a rotating wheel against its share of the chassis.
        // A 20 ms tire relaxation time softens contact engagement across substeps.
        let inverse_mass = wheel.radius.powi(2) * wheel.inverse_inertia + 32.2 / wheel.static_load;
        let forces = self.forces(
            i,
            TireContact {
                demand: [slip / (inverse_mass * (dt + 0.020)), patch.lateral_demand],
                slip: [slip, patch.lateral_speed],
                normal_load: patch.load,
                static_load: wheel.static_load,
                friction: patch.friction,
            },
            dt,
        );
        let unbraked = driven - forces[0] * wheel.radius * angular_step;
        let omega = slow(unbraked);
        let brake_applied = brake.min(unbraked.abs() / angular_step);
        // Rotor work is torque times swept angle. Locked skids dissipate
        // energy at the contact patch; stationary brakes generate no heat.
        let work = brake_applied * 0.5 * (omega.abs() + wheel.omega.abs()) * dt;
        let cooling = (0.008 + 0.00016 * wheel.air_speed.abs().min(150.)) * dt;
        self.brake_heat[i] = (self.brake_heat[i] * (1. - cooling)
            + work / (wheel.static_load * 230.))
            .clamp(0., 1.2);
        WheelStep {
            omega,
            forces,
            brake_torque: brake_applied,
        }
    }

    fn forces(&mut self, wheel: usize, contact: TireContact, dt: f32) -> [f32; 2] {
        let (grip, wear_loss, scrub_distance) = match self.mode {
            Handling::Original => return contact.demand,
            Handling::Sport => (1.65, 0.12, 20000.),
            Handling::Simulation => (1.12, 0.30, 10000.),
        };
        let load = contact.normal_load.max(0.);
        let tread = &mut self.wear[wheel];
        // Acceleration, braking and cornering all draw on one contact patch.
        let limit = grip * contact.friction.max(0.) * load * (1. - wear_loss * *tread);
        let [drive, side] = contact.demand;
        let length_sq = drive * drive + side * side;
        let forces = if length_sq > limit * limit {
            let scale = limit / length_sq.sqrt();
            [drive * scale, side * scale]
        } else {
            contact.demand
        };
        // Sliding work normalized by static wheel load gives an equivalent
        // scrub distance. The deadband filters small rolling/contact jitter.
        let work_rate = forces
            .iter()
            .zip(contact.slip)
            .map(|(force, slip)| force.abs() * (slip.abs() - 0.75).max(0.))
            .sum::<f32>();
        *tread = (*tread + dt * work_rate / (contact.static_load * scrub_distance)).min(1.);
        forces
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Det;

    fn wheel() -> WheelDrive {
        WheelDrive {
            omega: 0.,
            radius: 1.,
            inverse_inertia: 0.8,
            drive_torque: 0.,
            brake_torque: 0.,
            bearing_torque: 0.,
            static_load: 160.,
            air_speed: 0.,
        }
    }

    #[test]
    fn spinning_wheel_transfers_momentum_only_on_landing() {
        let mut tires = Tires::<1> {
            mode: Handling::Simulation,
            ..Default::default()
        };
        let mut wheel = wheel();
        wheel.drive_torque = 125.;
        let dt = 1. / 60.;
        for _ in 0..30 {
            let air = tires.step(0, wheel, ContactPatch::default(), dt);
            assert_eq!(air.forces, [0.; 2]);
            wheel.omega = air.omega;
        }
        assert!((wheel.omega - 50.).abs() < 0.001);
        assert_eq!(tires.worst_wear(), 0.);
        wheel.drive_torque = 0.;
        let landing = tires.step(
            0,
            wheel,
            ContactPatch {
                load: 160.,
                friction: 0.7,
                ..Default::default()
            },
            dt,
        );
        assert!(landing.omega > 0. && landing.omega < wheel.omega);
        let lost_angular_impulse = (wheel.omega - landing.omega) / wheel.inverse_inertia;
        assert!((lost_angular_impulse - landing.forces[0] * wheel.radius * dt).abs() < 0.0001);
        assert!(tires.worst_wear() > 0.);
    }

    #[test]
    fn brake_work_heats_rotors_and_coasting_restores_bite() {
        let mut tires = Tires::<1> {
            mode: Handling::Simulation,
            ..Default::default()
        };
        // A brake dynamometer drives the wheel at a constant 50 rad/s.
        let wheel = WheelDrive {
            omega: 50.,
            brake_torque: 100.,
            drive_torque: 100.,
            ..wheel()
        };
        let dt = 1. / 120.;
        let air = ContactPatch::default();
        let cold = tires.step(0, wheel, air, dt).brake_torque;
        for _ in 0..2400 {
            tires.step(0, wheel, air, dt);
        }
        let hot = tires.step(0, wheel, air, dt).brake_torque;
        assert!(hot < 0.8 * cold);
        assert_eq!(tires.worst_wear(), 0.);
        let coasting = WheelDrive {
            brake_torque: 0.,
            drive_torque: 0.,
            air_speed: 50.,
            ..wheel
        };
        for _ in 0..14400 {
            tires.step(0, coasting, air, dt);
        }
        assert_eq!(tires.step(0, wheel, air, dt).brake_torque, cold);
        let parked = WheelDrive {
            omega: 0.,
            drive_torque: 0.,
            ..wheel
        };
        let warm = tires.hottest_brake();
        for _ in 0..120 {
            assert_eq!(tires.step(0, parked, air, dt).omega, 0.);
        }
        assert!(tires.hottest_brake() < warm);
    }

    #[test]
    fn braking_distance_responds_to_grip_and_converges_with_step_size() {
        let stop = |friction: f32, dt: f32| {
            let mut tires = Tires::<1> {
                mode: Handling::Simulation,
                ..Default::default()
            };
            let mut wheel = WheelDrive {
                omega: 48.,
                brake_torque: 180.,
                ..wheel()
            };
            let mut speed = 48.;
            let mut distance = 0.;
            for _ in 0..(15. / dt) as usize {
                let result = tires.step(
                    0,
                    wheel,
                    ContactPatch {
                        forward_speed: speed,
                        load: wheel.static_load,
                        friction,
                        ..Default::default()
                    },
                    dt,
                );
                assert!(result.forces[0] <= 0.001);
                wheel.omega = result.omega;
                speed += result.forces[0] * 32.2 / wheel.static_load * dt;
                distance += speed * dt;
                if speed < 0.5 {
                    return distance;
                }
            }
            panic!("failed to stop on friction {friction}");
        };
        let firm = stop(1., 1. / 60.);
        let slick = stop(0.4, 1. / 60.);
        assert!(firm > 20. && slick > firm * 1.5);
        assert!((slick - stop(0.4, 1. / 120.)).abs() < 1.);
    }

    #[test]
    fn damper_wear_needs_a_hard_deep_hit_and_stays_on_that_corner() {
        let mut tires = Tires::<4> {
            mode: Handling::Simulation,
            ..Default::default()
        };
        assert_eq!(tires.damping_scale(0, 1., -30., 0.02), 1.);
        assert_eq!(tires.damping_scale(0, 0.6, 30., 0.02), 1.);
        assert_eq!(tires.damping_scale(0, 1., 4., 0.02), 1.);
        for _ in 0..100 {
            tires.damping_scale(0, 1., 50., 0.02);
        }
        assert_eq!(tires.worst_shock_wear(), 1.);
        assert_eq!(tires.damping_scale(0, 0., 0., 0.02), 0.78);
        assert_eq!(tires.damping_scale(1, 0., 0., 0.02), 1.);
    }

    #[test]
    fn progressive_spring_leaves_normal_travel_alone() {
        for mode in [Handling::Original, Handling::Sport, Handling::Simulation] {
            assert_eq!(mode.spring_scale(0.7), 1.);
            assert_eq!(mode.spring_scale(1.5), mode.spring_scale(1.));
        }
        assert_eq!(Handling::Original.spring_scale(1.), 1.);
        assert!(Handling::Simulation.spring_scale(1.) > Handling::Sport.spring_scale(1.));
        assert!(Handling::Sport.spring_scale(1.) > Handling::Sport.spring_scale(0.85));
    }

    #[test]
    fn loaded_slides_share_grip_and_wear_only_the_sliding_wheel() {
        let contact = TireContact {
            demand: [100., -100.],
            slip: [10., -10.],
            normal_load: 100.,
            static_load: 100.,
            friction: 1.,
        };
        let mut tires = Tires::<2>::default();
        assert_eq!(tires.forces(0, contact, 1.), contact.demand);
        assert_eq!(tires.worst_wear(), 0.);
        tires.mode = Handling::Simulation;
        let fresh = tires.forces(0, contact, 0.);
        assert!((fresh[0].hypot_d(fresh[1]) - 112.).abs() < 0.001);
        assert_eq!(
            tires.forces(
                0,
                TireContact {
                    normal_load: 0.,
                    ..contact
                },
                1.
            ),
            [0., 0.]
        );
        tires.forces(
            0,
            TireContact {
                slip: [0., 0.],
                ..contact
            },
            1.,
        );
        assert_eq!(tires.worst_wear(), 0.);
        for _ in 0..1600 {
            tires.forces(0, contact, 1.);
        }
        assert_eq!(tires.worst_wear(), 1.);
        assert!((tires.forces(0, contact, 0.)[0] - fresh[0] * 0.7).abs() < 0.001);
        assert_eq!(tires.forces(1, contact, 0.), fresh);
        tires.mode = Handling::Sport;
        assert_eq!(tires.forces(1, contact, 0.), contact.demand);
    }
}
