//! Alpha "hype" presentation (web/hype.js, Settings → Alpha): authored, not
//! original. It reads delivered 60 Hz frames and shapes time and camera
//! around big air; it never feeds anything back into the recovered solver
//! (slow motion only delays when the next fixed step is requested).
use atv_core::camera::CameraFrame;
use atv_core::math::Det;

const STEP: f32 = 1. / 60.;
const MPH: f32 = 5280. / 3600.;
/// A big jump: the quad climbs RISE ft above takeoff and reaches BIG_AIR ft
/// above the terrain (FULL_AIR saturates).
pub const BIG_AIR: f32 = 12.;
pub const FULL_AIR: f32 = 30.;
pub const RISE: [f32; 2] = [4., 10.];
pub const APEX_MIN: f32 = 14.;
pub const APEX_RISE: f32 = 8.;
pub const MIN_AIR_TIME: f32 = 0.45;
/// The big-air moment (real seconds): ease in, hold, ease out. Fired at the
/// apex, it drives slow motion, the chase camera, the blur and the muffle
/// together so they rise and fall as one beat.
pub const MOMENT: [f32; 3] = [0.2, 0.55, 0.4];
/// Slow-motion time scale at the moment's peak.
pub const SLOWMO_SCALE: f32 = 0.3;

fn smooth(edge0: f32, edge1: f32, v: f32) -> f32 {
    let t = ((v - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn approach(value: f32, target: f32, dt: f32, tc: f32) -> f32 {
    value + (target - value) * (1. - (-dt / tc).exp_d())
}

/// What `step` reads from a delivered frame.
#[derive(Clone, Copy, Debug)]
pub struct HypeFrame {
    pub origin: [f32; 3],
    pub linear_velocity: [f32; 3],
    pub speed: f32,
    pub reset_count: u32,
    pub riding: bool,
    pub grounded: bool,
}

/// A landing: impact speed (ft/s), air time, peak clearance and position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub severity: f32,
    pub air_time: f32,
    pub peak: f32,
    pub position: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct HypeDirector {
    previous: Option<HypeFrame>,
    pub air_time: f32,
    pub clearance: f32,
    pub peak: f32,
    pub speed: f32,
    takeoff: f32,
    pub rise: f32,
    apex_fired: bool,
    pub apexes: u32,
    pub landings: u32,
    pub landing: Option<Landing>,
    riding: bool,
    grounded: bool,
    pub air: f32,
    pub blur: f32,
    pub muffle: f32,
    muffle_speed: f32,
    pub wind: f32,
    wind_speed: f32,
    pub slam: f32,
    pub dip: f32,
    dip_velocity: f32,
    /// The moment's age (real seconds) while it runs, and its strength.
    moment: Option<f32>,
    strength: f32,
    /// The moment envelope (0..1) this frame.
    pub envelope: f32,
    pub time_scale: f32,
    pub slowmos: u32,
    seen_apexes: u32,
    seen_landings: u32,
}

impl Default for HypeDirector {
    fn default() -> Self {
        Self {
            previous: None,
            air_time: 0.,
            clearance: 0.,
            peak: 0.,
            speed: 0.,
            takeoff: 0.,
            rise: 0.,
            apex_fired: false,
            apexes: 0,
            landings: 0,
            landing: None,
            riding: false,
            grounded: true,
            air: 0.,
            blur: 0.,
            muffle: 0.,
            muffle_speed: 0.,
            wind: 0.,
            wind_speed: 0.,
            slam: 0.,
            dip: 0.,
            dip_velocity: 0.,
            moment: None,
            strength: 0.,
            envelope: 0.,
            time_scale: 1.,
            slowmos: 0,
            seen_apexes: 0,
            seen_landings: 0,
        }
    }
}

impl HypeDirector {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Exactly one call per accepted 60 Hz simulation frame; `ground` is the
    /// terrain height under the rider.
    pub fn step(&mut self, frame: HypeFrame, ground: Option<f32>) {
        let old = self.previous.replace(frame);
        let jumped = old.is_none_or(|o| {
            o.reset_count != frame.reset_count
                || (0..3)
                    .map(|i| (frame.origin[i] - o.origin[i]).powi(2))
                    .sum::<f32>()
                    .sqrt()
                    > 40.
        });
        let Some(old) = old.filter(|_| !jumped) else {
            self.air_time = 0.;
            self.peak = 0.;
            self.rise = 0.;
            self.apex_fired = false;
            self.grounded = true;
            self.riding = false;
            return;
        };
        self.riding = frame.riding;
        self.grounded = frame.grounded;
        self.clearance = ground
            .filter(|g| g.is_finite())
            .map_or(0., |g| frame.origin[1] - g);
        self.speed = frame.speed.abs();
        let (vy, old_vy) = (frame.linear_velocity[1], old.linear_velocity[1]);
        if self.grounded {
            if !old.grounded && self.air_time >= 0.3 && self.riding {
                self.landing = Some(Landing {
                    severity: (-old_vy).max(0.),
                    air_time: self.air_time,
                    peak: self.peak,
                    position: frame.origin,
                });
                self.landings += 1;
            }
            self.air_time = 0.;
            self.peak = 0.;
            self.rise = 0.;
            self.apex_fired = false;
            return;
        }
        if self.air_time == 0. {
            self.takeoff = old.origin[1];
        }
        self.air_time += STEP;
        self.peak = self.peak.max(self.clearance);
        self.rise = self.rise.max(frame.origin[1] - self.takeoff);
        if !self.apex_fired
            && self.riding
            && old_vy > 0.
            && vy <= 0.
            && self.clearance >= APEX_MIN
            && self.rise >= APEX_RISE
            && self.air_time > 0.25
        {
            self.apex_fired = true;
            self.apexes += 1;
        }
    }

    /// Big-air amount (0..1): the highest point so far, held to touchdown.
    pub fn big(&self) -> f32 {
        if self.riding && !self.grounded && self.air_time >= MIN_AIR_TIME {
            smooth(BIG_AIR, FULL_AIR, self.peak) * smooth(RISE[0], RISE[1], self.rise)
        } else {
            0.
        }
    }

    /// Once per rendered frame with real seconds; returns a new landing.
    pub fn update(&mut self, dt: f32, slowmo: bool) -> Option<Landing> {
        let dt = dt.clamp(0., 0.1);
        let apex = self.apexes != self.seen_apexes;
        let landed = self.landings != self.seen_landings;
        self.seen_apexes = self.apexes;
        self.seen_landings = self.landings;
        // One moment per jump, at its apex; touchdown or a wreck ends it.
        if apex && self.moment.is_none() {
            self.moment = Some(0.);
            // A barely-qualifying apex still reads; the biggest air saturates.
            self.strength = 0.5 + 0.5 * self.big();
            if slowmo {
                self.slowmos += 1;
            }
        }
        let [ease_in, hold, ease_out] = MOMENT;
        self.envelope = match self.moment {
            Some(mut t) => {
                if landed || !self.riding || (self.clearance < 4. && t < ease_in + hold) {
                    t = t.max(ease_in + hold);
                }
                t += dt;
                let out = t - ease_in - hold;
                self.moment = (out < ease_out).then_some(t);
                if t < ease_in {
                    smooth(0., ease_in, t)
                } else if out < 0. {
                    1.
                } else {
                    1. - smooth(0., ease_out, out)
                }
            }
            None => 0.,
        };
        self.time_scale = if slowmo {
            1. - (1. - SLOWMO_SCALE) * self.envelope
        } else {
            1.
        };
        // Camera and blur ride the envelope directly (no lag of their own).
        self.air = self.envelope * self.strength;
        self.blur = self.air;
        if landed && let Some(l) = self.landing {
            let hit = l.severity.min(70.) * (0.4 + 0.6 * smooth(2., FULL_AIR, l.peak));
            self.dip_velocity -= hit * 0.42;
            self.slam = 0.35;
        }
        let (k, c) = (130., 15.);
        self.dip_velocity += (-k * self.dip - c * self.dip_velocity) * dt;
        self.dip += self.dip_velocity * dt;
        self.dip = self.dip.clamp(-3., 1.5);
        self.slam = (self.slam - dt).max(0.);
        let fast = 0.5 * smooth(40. * MPH, 72. * MPH, self.speed);
        self.muffle_speed = approach(
            self.muffle_speed,
            if self.slam > 0. { 0. } else { fast },
            dt,
            if self.slam > 0. { 0.03 } else { 0.4 },
        );
        // The moment's share holds only while airborne: touchdown's slam
        // opens the sound at once.
        let airborne = if self.grounded || self.slam > 0. {
            0.
        } else {
            self.envelope
        };
        let slow = if slowmo { 1. - SLOWMO_SCALE } else { 0. };
        self.muffle = self
            .muffle_speed
            .max(airborne * (0.85 * self.strength).max(slow));
        self.wind_speed = approach(
            self.wind_speed,
            if self.slam > 0. { 0. } else { 2. * fast },
            dt,
            if self.slam > 0. { 0.05 } else { 0.35 },
        );
        self.wind = self.wind_speed.max(airborne * self.strength);
        landed.then_some(self.landing).flatten()
    }

    /// Big-air chase framing: close in behind the rider, widen slightly;
    /// landings compress the camera and spring back.
    pub fn camera(
        &self,
        frame: &CameraFrame,
        ground: impl Fn(f32, f32) -> Option<f32>,
    ) -> CameraFrame {
        let (a, dip) = (self.air, self.dip);
        if a < 0.001 && dip.abs() < 0.001 {
            return *frame;
        }
        let offset: [f32; 3] = std::array::from_fn(|i| frame.target[i] - frame.position[i]);
        let pull = 1. - 0.4 * a;
        let mut position = [
            frame.target[0] - offset[0] * pull,
            frame.target[1] - offset[1] * (1. - 0.6 * a),
            frame.target[2] - offset[2] * pull,
        ];
        if let Some(floor) = ground(position[0], position[2]).filter(|f| f.is_finite()) {
            position[1] = position[1].max(floor + 2.);
        }
        let mut target = frame.target;
        position[1] += dip;
        target[1] += dip * 0.55;
        CameraFrame {
            position,
            target,
            fov: frame.fov * (1. + 0.1 * a),
            ..*frame
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(y: f32, vy: f32, grounded: bool) -> HypeFrame {
        HypeFrame {
            origin: [0., y, 0.],
            linear_velocity: [0., vy, 0.],
            speed: 40.,
            reset_count: 0,
            riding: true,
            grounded,
        }
    }

    #[test]
    fn a_big_jump_fires_an_apex_slow_motion_and_a_landing() {
        let mut h = HypeDirector::default();
        h.step(frame(0., 0., true), Some(0.));
        // Launch at 40 ft/s under 32 ft/s² gravity.
        let (mut y, mut vy) = (0., 40.);
        let mut scales = Vec::new();
        for _ in 0..200 {
            vy -= 32. * STEP;
            y += vy * STEP;
            let grounded = y <= 0.;
            if grounded {
                y = 0.;
            }
            h.step(frame(y, vy, grounded), Some(0.));
            h.update(STEP, true);
            scales.push(h.time_scale);
            if grounded {
                break;
            }
        }
        assert_eq!(h.apexes, 1);
        assert_eq!(h.landings, 1);
        assert_eq!(h.slowmos, 1);
        assert!(scales.iter().any(|s| (*s - SLOWMO_SCALE).abs() < 1e-6));
        assert!(h.landing.unwrap().severity > 30.);
    }

    #[test]
    fn slow_motion_camera_blur_and_muffle_share_one_beat() {
        let mut h = HypeDirector::default();
        h.step(frame(0., 0., true), Some(0.));
        let (mut y, mut vy) = (0., 40.);
        let mut seen = false;
        for _ in 0..600 {
            vy -= 32. * STEP;
            y += vy * STEP;
            let grounded = y <= 0.;
            if grounded {
                y = 0.;
            }
            h.step(frame(y, vy, grounded), Some(0.));
            h.update(STEP, true);
            // Nothing leads or trails the slow motion.
            assert_eq!(h.air > 0., h.time_scale < 1.);
            assert_eq!(h.blur, h.air);
            if h.envelope > 0. && !grounded {
                assert!(h.muffle >= 0.7 * h.envelope - 1e-6);
            }
            seen |= h.envelope == 1.;
            if grounded {
                break;
            }
        }
        assert!(seen);
    }
}
