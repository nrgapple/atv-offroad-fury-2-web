//! Original ATV deep-water reset clock, USA v2.00 001e5260 (+814).
//! This is separate from the detached rider's buoyancy and swimming actor.
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Clock {
    pub remaining: f32,
    pub submerged: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            remaining: 5.,
            submerged: false,
        }
    }
}

impl Clock {
    /// The reset gate precedes physics; crossing zero in the late water stage
    /// therefore requests recovery on the next outer update, including zero.
    pub fn reset_due(self) -> bool {
        self.remaining <= 0.
    }

    /// Uses the preceding outer frame's cached water height, not a new sample.
    /// Wrecked riders recover through their independent water/solid contact gate.
    pub fn update(&mut self, origin_y: f32, cached_height: f32, riding: bool, dt: f32) {
        self.submerged = origin_y < cached_height - 3.;
        if !self.submerged {
            self.remaining = 5.;
        } else if riding {
            self.remaining -= dt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_strict_depth_and_riding_gates() {
        let mut clock = Clock::default();
        clock.update(97., 100., true, 0.1);
        assert!(!clock.submerged);
        assert_eq!(clock.remaining, 5.);
        clock.update(f32::from_bits(97_f32.to_bits() - 1), 100., true, 0.1);
        assert!(clock.submerged);
        assert_eq!(clock.remaining.to_bits(), 4.9_f32.to_bits());
        clock.update(90., 100., false, 0.1);
        assert_eq!(clock.remaining.to_bits(), 4.9_f32.to_bits());
        clock.update(97., 100., false, 0.1);
        assert_eq!(clock.remaining, 5.);
    }

    #[test]
    fn expiration_is_consumed_by_following_outer_update() {
        let mut clock = Clock {
            remaining: 0.1,
            submerged: true,
        };
        assert!(!clock.reset_due());
        clock.update(90., 100., true, 0.1);
        assert_eq!(clock.remaining, 0.);
        assert!(clock.reset_due());
        clock = Clock::default();
        assert!(!clock.reset_due());
        // The -256 sentinel and equality never start a dry-map countdown.
        clock.update(0., -256., true, 0.1);
        assert!(!clock.submerged);
        assert_eq!(clock.remaining, 5.);
    }
}
