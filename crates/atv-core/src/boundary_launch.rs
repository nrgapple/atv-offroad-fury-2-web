//! USA v2.00 map-edge predicate0020aec0 and launch block001e5260.
//! Caller evaluates after quad physics, applies the replacement velocities,
//! raises quad/rider speed limits and enters the original random wreck path.
use crate::{Result, wheel::Vec3};
use serde::{Deserialize, Serialize};

pub const NORMAL_MAX_SPEED_SQUARED: f32 = f32::from_bits(0x468e6200);
pub const LAUNCH_MAX_SPEED_SQUARED: f32 = f32::from_bits(0x47f42400);

///001efb90 kind5 selector, after the two launch-spin draws. Stock rider setup
/// registers one clip per direction. Both words are consumed, including the
/// variant word. Unmatched selection falls through to the final SepUp entry.
pub fn random_wreck_kind(random: [u32; 2]) -> Result<crate::wreck::Kind> {
    if random.iter().any(|word| *word > 0x7fffffff) {
        return Err("invalid boundary wreck random word".into());
    }
    let index = ((random[0] as f32 * f32::from_bits(0x30000000)) * 5.) as usize;
    let variant = (random[1] as f32 * f32::from_bits(0x30000000)) as usize;
    if variant != 0 {
        return Ok(crate::wreck::Kind::Vertical);
    }
    match index {
        0 => Ok(crate::wreck::Kind::Front),
        1 => Ok(crate::wreck::Kind::Left),
        2 => Ok(crate::wreck::Kind::Right),
        3 => Ok(crate::wreck::Kind::Back),
        // Native count5 is zero. The unmatched loop selects the last clip,
        // whose registration is SepUp; this is table fallthrough, not clamping.
        _ => Ok(crate::wreck::Kind::Vertical),
    }
}

/// Original Environment dimensions, before multiplication by256 world units.
/// Spatial-grid minima/maxima and BorderWidth do not enter this predicate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Config {
    pub terrain_width: f32,
    pub terrain_breadth: f32,
    pub width_scale: f32,
    pub breadth_scale: f32,
}
impl Config {
    fn validate(self) -> Result<()> {
        if [
            self.terrain_width,
            self.terrain_breadth,
            self.width_scale,
            self.breadth_scale,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err("invalid boundary launch terrain dimensions".into());
        }
        Ok(())
    }

    /// `extended` is native level+658 !=0 or game mode6. Equality is safe.
    pub fn outside(self, position: Vec3, extended: bool) -> Result<bool> {
        self.validate()?;
        if !position[0].is_finite() || !position[2].is_finite() {
            return Err("nonfinite boundary launch position".into());
        }
        let margin = if extended { -1000. } else { 110. };
        Ok(position[0] < margin * self.width_scale
            || position[0] > self.width_scale * (256. * self.terrain_width - margin)
            || position[2] < margin * self.breadth_scale
            || position[2] > self.breadth_scale * (256. * self.terrain_breadth - margin))
    }

    /// First random word supplies yaw, second pitch. The caller owns the shared
    /// native RNG stream and consumes direction and variant words afterward.
    pub fn launch(self, position: Vec3, random: [u32; 2]) -> Result<Launch> {
        self.validate()?;
        if position.iter().any(|v| !v.is_finite()) || random.iter().any(|v| *v > 0x7fffffff) {
            return Err("invalid boundary launch input".into());
        }
        let delta = [
            (128. * self.terrain_width) * self.width_scale - position[0],
            0.,
            (128. * self.terrain_breadth) * self.breadth_scale - position[2],
        ];
        let square = (delta[0] * delta[0] + delta[1] * delta[1]) + delta[2] * delta[2];
        let direction = if square == 0. {
            [0.; 3]
        } else {
            // RSQRT.S rounds its reciprocal-square-root result once.
            let reciprocal = (1. / f64::from(square).sqrt()) as f32;
            delta.map(|v| v * reciprocal)
        };
        let flight_time = 2. * f32::from_bits(0x40f8727c).sqrt();
        let horizontal = 950. / flight_time;
        let vertical = 125. / flight_time + f32::from_bits(0x4180cccd) * flight_time;
        let linear_velocity = [
            horizontal * direction[0] + 0.,
            horizontal * direction[1] + vertical,
            horizontal * direction[2] + 0.,
        ];
        let yaw = (random[0] as f32 * f32::from_bits(0x30000000)) * 0.4;
        let pitch = (random[1] as f32 * f32::from_bits(0x30000000)) * 0.5;
        Ok(Launch {
            linear_velocity,
            local_angular_velocity: [pitch, yaw, f32::from_bits(0x3ca3d70a)],
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Launch {
    pub linear_velocity: Vec3,
    pub local_angular_velocity: Vec3,
}

///001ea4d0 gate inputs. Scene-clearance and vegetation checks follow these
/// gates; caller still supplies their result before saving a checkpoint.
#[derive(Debug, Clone, Copy)]
pub struct CheckpointGates {
    pub boundary_flight: bool,
    pub player_kind_two: bool,
    pub forced: bool,
    pub all_wheels_grounded: bool,
    pub wrecked: bool,
    pub body_up_y: f32,
    pub terrain_height: f32,
    pub water_height: f32,
}

/// Boundary mode projects the candidate to terrain before acceptance checks.
/// Return the mutated candidate even on rejection, matching the native wrapper.
pub fn checkpoint_candidate(mut position: Vec3, gates: CheckpointGates) -> Result<(Vec3, bool)> {
    if position.iter().any(|v| !v.is_finite())
        || [gates.body_up_y, gates.terrain_height, gates.water_height]
            .iter()
            .any(|v| !v.is_finite())
    {
        return Err("nonfinite boundary checkpoint input".into());
    }
    if gates.boundary_flight {
        position[1] = gates.terrain_height;
    }
    let ordinary_gate = gates.boundary_flight
        || gates.player_kind_two
        || gates.forced
        || (gates.all_wheels_grounded && !gates.wrecked && gates.body_up_y >= 0.8);
    Ok((
        position,
        ordinary_gate && gates.terrain_height > gates.water_height,
    ))
}

/// Native boundary flag+808 and clock+80c. Recovery clears both. This is not a
/// cooldown: active boundary flight cannot launch again, even after one second.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct State {
    pub active: bool,
    pub elapsed: f32,
}
impl State {
    pub fn begin(&mut self) {
        self.active = true;
        self.elapsed = 0.;
    }
    /// Before edge testing, tick flights active at outer-frame entry. Returns
    /// the native request to clear quad/rider query bit0 when game setting+674
    /// is enabled. This routine does not assign an unverified meaning to bit0.
    pub fn tick(&mut self, dt: f32, clear_query_bit: bool) -> Result<bool> {
        if !dt.is_finite() || dt < 0. {
            return Err("invalid boundary launch timestep".into());
        }
        if self.active {
            self.elapsed += dt;
        }
        Ok(self.active && self.elapsed > 1. && clear_query_bit)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn map() -> Config {
        Config {
            terrain_width: 22.5,
            terrain_breadth: 22.5,
            width_scale: 1.,
            breadth_scale: 1.,
        }
    }
    #[test]
    fn exact_edges_scale_the_margin_and_ignore_altitude() {
        let mut c = map();
        c.width_scale = 2.;
        c.breadth_scale = 0.5;
        assert!(!c.outside([220., -1000., 55.], false).unwrap());
        assert!(c.outside([219., 1000., 55.], false).unwrap());
        assert!(!c.outside([11300., 0., 2825.], false).unwrap());
        assert!(c.outside([11301., 0., 2825.], false).unwrap());
        assert!(!c.outside([-2000., 0., -500.], true).unwrap());
        assert!(c.outside([-2001., 0., -500.], true).unwrap());
    }
    #[test]
    fn launch_replaces_velocity_toward_map_center_without_moving_pose() {
        let c = map();
        let left = c.launch([0., 300., 2880.], [0, 0]).unwrap();
        assert!(left.linear_velocity[0] > 170.);
        assert_eq!(left.linear_velocity[2], 0.);
        assert!(left.linear_velocity[1] > 112.);
        assert_eq!(left.local_angular_velocity, [0., 0., 0.02]);
        let right = c.launch([5760., -300., 2880.], [0, 0]).unwrap();
        assert_eq!(right.linear_velocity[0], -left.linear_velocity[0]);
        assert_eq!(right.linear_velocity[1], left.linear_velocity[1]);
    }
    #[test]
    fn active_flight_clock_has_strict_one_second_gate_and_explicit_recovery() {
        let mut state = State::default();
        assert!(!state.tick(2., true).unwrap());
        state.begin();
        assert!(!state.tick(1., true).unwrap());
        assert!(!state.tick(0.01, false).unwrap());
        assert!(state.tick(0., true).unwrap());
        assert!(state.active);
        state.reset();
        assert!(!state.active);
        assert_eq!(state.elapsed, 0.);
    }
    #[test]
    fn random_wreck_direction_retains_original_float_selector() {
        use crate::wreck::Kind;
        for (word, kind) in [
            (0, Kind::Front),
            (0x20000000, Kind::Left),
            (0x40000000, Kind::Right),
            (0x50000000, Kind::Back),
            (0x70000000, Kind::Vertical),
        ] {
            assert_eq!(random_wreck_kind([word, 0]).unwrap(), kind);
        }
        assert_eq!(random_wreck_kind([0x7fffffff, 0]).unwrap(), Kind::Vertical);
        assert_eq!(random_wreck_kind([0, 0x7fffffff]).unwrap(), Kind::Vertical);
        assert!(random_wreck_kind([0x80000000, 0]).is_err());
    }
    #[test]
    fn boundary_checkpoint_projects_height_and_retains_water_gate() {
        let mut gates = CheckpointGates {
            boundary_flight: true,
            player_kind_two: false,
            forced: false,
            all_wheels_grounded: false,
            wrecked: true,
            body_up_y: -1.,
            terrain_height: 10.,
            water_height: 0.,
        };
        assert_eq!(
            checkpoint_candidate([2., 500., 3.], gates).unwrap(),
            ([2., 10., 3.], true)
        );
        gates.water_height = 10.;
        assert_eq!(
            checkpoint_candidate([2., 500., 3.], gates).unwrap(),
            ([2., 10., 3.], false)
        );
        gates.boundary_flight = false;
        gates.water_height = 0.;
        assert_eq!(
            checkpoint_candidate([2., 500., 3.], gates).unwrap(),
            ([2., 500., 3.], false)
        );
    }
}
