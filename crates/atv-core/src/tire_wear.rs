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

    pub fn cycle(self, direction: i32) -> Self {
        let modes = [Self::Original, Self::Sport, Self::Simulation];
        modes[(self as i32 + direction.signum()).rem_euclid(3) as usize]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TireContact {
    pub demand: [f32; 2],
    pub slip: [f32; 2],
    pub normal_load: f32,
    pub static_load: f32,
    pub friction: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Tires<const N: usize> {
    pub mode: Handling,
    wear: [f32; N],
}

impl<const N: usize> Default for Tires<N> {
    fn default() -> Self {
        Self {
            mode: Handling::Original,
            wear: [0.; N],
        }
    }
}

impl<const N: usize> Tires<N> {
    pub fn worst_wear(&self) -> f32 {
        self.wear.iter().copied().fold(0_f32, f32::max)
    }

    pub fn forces(&mut self, wheel: usize, contact: TireContact, dt: f32) -> [f32; 2] {
        let (grip, wear_loss, scrub_distance) = match self.mode {
            Handling::Original => return contact.demand,
            Handling::Sport => (1.65, 0.12, 7000.),
            Handling::Simulation => (1.12, 0.30, 4000.),
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
        assert!((fresh[0].hypot(fresh[1]) - 112.).abs() < 0.001);
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
        for _ in 0..600 {
            tires.forces(0, contact, 1.);
        }
        assert_eq!(tires.worst_wear(), 1.);
        assert!((tires.forces(0, contact, 0.)[0] - fresh[0] * 0.7).abs() < 0.001);
        assert_eq!(tires.forces(1, contact, 0.), fresh);
        tires.mode = Handling::Sport;
        assert_eq!(tires.forces(1, contact, 0.), contact.demand);
    }
}
