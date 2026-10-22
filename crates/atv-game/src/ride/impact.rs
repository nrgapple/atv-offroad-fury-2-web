//! Impact detection and controller rumble (ImpactEvents in
//! web/render-support.js, Rumble and rumbleStrength in web/graphics.js).
//! One `accept` per accepted simulation frame. Authored presentation: hard
//! body contacts, heavy landings and rider wreck impacts become one
//! severity per impact (ft/s, 0.3 s cooldown); severities from 18 ft/s and a
//! fresh wreck pulse the active controller.
use super::{Phase, Ride};

/// What ImpactEvents reads from one simulation frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Actor {
    pub reset_count: u32,
    pub origin: [f32; 3],
    pub riding: bool,
    pub grounded: bool,
    pub crash_impact_speed: f32,
    /// Chassis, obstacle or dynamic contacts this step.
    pub body: bool,
    pub linear_velocity: [f32; 3],
    /// The ATV wreck rig's `sampled_impact_delta`.
    pub rider_delta: [f32; 3],
}

impl Actor {
    pub fn of(ride: &Ride) -> Self {
        let f = ride.frame();
        Self {
            reset_count: f.reset_count,
            origin: f.origin,
            riding: f.wreck.phase == Phase::Riding,
            grounded: f.grounded.iter().any(|g| *g),
            crash_impact_speed: f.crash_impact_speed,
            body: f.chassis_contact_count + f.obstacle_contact_count + f.dynamic_contact_count > 0,
            linear_velocity: f.linear_velocity,
            rider_delta: ride.rider_impact_delta(),
        }
    }
}

fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[derive(Clone, Debug, Default)]
pub struct ImpactEvents {
    previous: Option<Actor>,
    cooldown_until: f64,
    air_time: f64,
    crash_severity: f64,
    pub impacts: u32,
}

impl ImpactEvents {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The impact severity of this frame (0 for none); `now` in seconds.
    pub fn accept(&mut self, actor: Actor, enabled: bool, now: f64) -> f64 {
        let old = self.previous.replace(actor);
        let moved = old.map_or(0., |o| {
            length(std::array::from_fn(|i| {
                f64::from(actor.origin[i]) - f64::from(o.origin[i])
            }))
        });
        let Some(old) =
            old.filter(|o| enabled && o.reset_count == actor.reset_count && moved <= 40.)
        else {
            self.air_time = 0.;
            self.crash_severity = 0.;
            self.cooldown_until = 0.;
            return 0.;
        };
        if actor.riding && !old.riding {
            self.crash_severity = 0.;
        }
        let air_time = self.air_time;
        self.air_time = if actor.grounded {
            0.
        } else {
            self.air_time + 1. / 60.
        };
        let contact = f64::from(actor.crash_impact_speed);
        let delta = length(std::array::from_fn(|i| {
            f64::from(actor.linear_velocity[i]) - f64::from(old.linear_velocity[i])
        }));
        let fall = f64::from(old.linear_velocity[1]);
        let landing = actor.grounded && !old.grounded && air_time >= 0.18 && fall < -12.;
        let rider = length(actor.rider_delta.map(f64::from));
        let severity = [
            if actor.body && delta >= 5. && contact >= 30. {
                contact
            } else {
                0.
            },
            if landing { contact.max(-fall) } else { 0. },
            if rider >= 40. { rider } else { 0. },
        ]
        .into_iter()
        .fold(0., f64::max);
        // A crash gets one impulse and at most a meaningful severity upgrade.
        let upgrade = severity >= 50. && self.crash_severity < 50.;
        if severity > 0.
            && (now >= self.cooldown_until || upgrade)
            && (actor.riding || self.crash_severity == 0. || upgrade)
        {
            self.cooldown_until = now + 0.30;
            if !actor.riding || actor.body {
                self.crash_severity = self.crash_severity.max(severity);
            }
            self.impacts += 1;
            return severity;
        }
        0.
    }
}

/// rumbleStrength: gentle hops stay silent.
pub fn rumble_strength(severity: f64) -> f64 {
    if severity >= 18. {
        (0.2 + (severity - 18.) / 55.).min(1.)
    } else {
        0.
    }
}

/// One controller pulse: dual-rumble strong/weak magnitudes and duration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    pub strength: f64,
    pub weak: f64,
    pub duration_ms: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Rumble {
    until: f64,
    strength: f64,
    pub pulses: u32,
    pub last: Option<Pulse>,
}

impl Rumble {
    /// A pulse to play, if `strength` warrants one now (`now` in ms).
    /// Bouncing inside one impact cannot chain pulses; a stronger hit can.
    pub fn pulse(&mut self, strength: f64, now: f64) -> Option<Pulse> {
        if strength.is_nan() || strength <= 0. {
            return None;
        }
        if now < self.until && strength <= self.strength + 0.15 {
            return None;
        }
        let duration = (70. + 230. * strength).round();
        self.until = now + duration;
        self.strength = strength;
        Some(Pulse {
            strength,
            weak: (0.25 + strength * 0.75).min(1.),
            duration_ms: duration as u32,
        })
    }

    /// The device took the pulse (Rumble's `sent`).
    pub fn sent(&mut self, pulse: Pulse) {
        self.pulses += 1;
        self.last = Some(pulse);
    }

    /// One simulation frame: an ImpactEvents severity and the wreck
    /// transition from `previous` (reset count, riding).
    pub fn accept(
        &mut self,
        previous: Option<(u32, bool)>,
        actor: &Actor,
        severity: f64,
        enabled: bool,
        now: f64,
    ) -> Option<Pulse> {
        if !enabled {
            return None;
        }
        let wrecked = previous
            .is_some_and(|(count, riding)| count == actor.reset_count && riding && !actor.riding);
        let wreck = if wrecked {
            rumble_strength(f64::from(actor.crash_impact_speed)).max(0.7)
        } else {
            0.
        };
        self.pulse(rumble_strength(severity).max(wreck), now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> Actor {
        Actor {
            riding: true,
            grounded: true,
            ..Default::default()
        }
    }

    // Expected values from web/render-support.js ImpactEvents and
    // web/graphics.js Rumble on the same frames.
    #[test]
    fn follows_the_browser() {
        let mut events = ImpactEvents::default();
        assert_eq!(events.accept(actor(), true, 0.), 0.);
        // 0.2 s airborne falling at 20 ft/s, then a landing.
        let air = Actor {
            grounded: false,
            linear_velocity: [0., -20., 0.],
            ..actor()
        };
        for i in 0..12 {
            assert_eq!(events.accept(air, true, f64::from(i) / 60.), 0.);
        }
        assert_eq!(events.accept(actor(), true, 0.25), 20.);
        // A hard body hit inside the cooldown is dropped; a crash upgrade is not.
        let hit = Actor {
            body: true,
            crash_impact_speed: 35.,
            linear_velocity: [10., 0., 0.],
            ..actor()
        };
        assert_eq!(events.accept(hit, true, 0.3), 0.);
        let crash = Actor {
            riding: false,
            crash_impact_speed: 60.,
            linear_velocity: [0., 0., 0.],
            ..hit
        };
        assert_eq!(events.accept(crash, true, 0.31), 60.);
        assert_eq!(events.impacts, 2);

        assert_eq!(rumble_strength(17.9), 0.);
        assert!((rumble_strength(40.) - (0.2 + 22. / 55.)).abs() < 1e-12);
        let mut rumble = Rumble::default();
        let first = rumble.pulse(0.5, 1000.).unwrap();
        assert_eq!(first.duration_ms, 185);
        assert_eq!(first.weak, 0.625);
        assert_eq!(rumble.pulse(0.6, 1100.), None);
        assert!(rumble.pulse(0.7, 1100.).is_some());
        // A fresh wreck pulses at least 0.7.
        let mut rumble = Rumble::default();
        let wrecked = Actor {
            riding: false,
            ..actor()
        };
        let pulse = rumble
            .accept(Some((0, true)), &wrecked, 0., true, 0.)
            .unwrap();
        assert_eq!(pulse.strength, 0.7);
        assert_eq!(
            rumble.accept(Some((0, true)), &wrecked, 30., false, 0.),
            None
        );
    }
}
