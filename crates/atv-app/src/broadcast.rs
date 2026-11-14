//! The replay's broadcast camera (#542, a port enhancement). In a race it
//! is the original's own after-finish director (`atv_core::race_director`,
//! `001f3bb0`) run over the replay: it cuts between the course's authored
//! trackside cameras (`<venue>nat.tv`) by the original rules, and its
//! follow-camera cuts show the recorded ride camera. A ride without TV
//! cameras (a free ride's instant replay) gets a built-in director that
//! cuts between vantages the way a race broadcast does. Each cut picks
//! the next setup in a fixed order (a wide crane, trackside, a low angle,
//! head-on, an aerial that flies alongside, the rider's own chase camera),
//! places it from the recorded line ahead, and holds it until the rider has
//! passed and is pulling away, the hold runs out, or the terrain hides the
//! rider. Sides alternate cut to cut. A scrub or jump starts a fresh cut.
//! Nothing is allocated; the director only reads the recording.
use crate::instant_replay::{Moment, Recorder, look, vantage_beside};
use atv_core::math::Det;
use atv_core::race_director::{self, Cameras};
use atv_render::Camera;

/// The live race's director seed (`Ride::race_director`).
const TV_SEED: u64 = 97211 + 6 * 47;

/// The broadcast camera: the course's TV director, else built-in cuts.
pub struct Broadcast {
    tv: Option<race_director::Director>,
    pub fallback: Director,
    /// The last frame's replay time, and its view (held while paused).
    last: Option<f64>,
    shown: (Camera, f32),
    /// The TV director's cuts so far, and whether it shows the ride camera.
    tv_cuts: u32,
    onboard: bool,
}

impl Broadcast {
    /// `tv`: the course's trackside cameras (cloned once, as a replay opens).
    pub fn new(tv: Option<&Cameras>) -> Self {
        Self {
            tv: tv
                .filter(|tv| !tv.cameras.is_empty())
                .map(|tv| race_director::Director::new(tv.clone(), TV_SEED)),
            fallback: Director::default(),
            last: None,
            shown: (Camera::default(), 1.),
            tv_cuts: 0,
            onboard: false,
        }
    }

    /// Starts directing afresh (another camera was chosen meanwhile).
    pub fn restart(&mut self) {
        self.last = None;
        self.fallback = Director::default();
    }

    pub fn authored(&self) -> bool {
        self.tv.is_some()
    }

    pub fn cuts(&self) -> u32 {
        if self.tv.is_some() {
            self.tv_cuts
        } else {
            self.fallback.cuts
        }
    }

    /// The shown vantage: the TV camera's number, the ride camera, or the
    /// built-in setup.
    pub fn setup(&self) -> (&'static str, Option<usize>) {
        match &self.tv {
            Some(_) if self.onboard => ("Onboard", None),
            Some(tv) => ("TV", tv.current().map(|i| i + 1)),
            None => (self.fallback.setup.label(), None),
        }
    }

    /// The camera at replay `time` for `moment` (sampled there).
    pub fn camera(
        &mut self,
        time: f64,
        moment: &Moment,
        recorder: &Recorder,
        ground: &dyn Fn(f32, f32) -> f32,
        aspect: f32,
    ) -> (Camera, f32) {
        let Some(tv) = &mut self.tv else {
            return self.fallback.camera(time, moment, recorder, ground);
        };
        let w = &moment.player.quad_world;
        let rider = [w[12], w[13], w[14]];
        let elapsed = self.last.map(|last| time - last);
        self.last = Some(time);
        // A scrub or jump directs afresh from Camera1, as a new director.
        let steps = match elapsed {
            Some(e) if (0. ..=JUMP).contains(&e) => e,
            _ => {
                tv.restart(TV_SEED);
                self.tv_cuts = 0;
                self.onboard = false;
                1.
            }
        };
        if steps > 0. {
            // The rider's recorded speed (ft/s) for the fly-by shake.
            let a = recorder.origin_at(time - 2.).unwrap_or(rider);
            let b = recorder.origin_at(time + 2.).unwrap_or(rider);
            let speed = (b[0] - a[0]).hypot_d(b[2] - a[2]).hypot_d(b[1] - a[1]) * 15.;
            let racers = [race_director::Racer {
                origin: rider,
                speed,
            }];
            let mut left = (steps / 60.) as f32;
            let mut pose = None;
            while left > 1e-6 {
                let dt = left.min(0.1);
                left -= dt;
                let Ok(step) = tv.step(dt, rider, &racers) else {
                    break;
                };
                if let Some(view) = step.cut {
                    self.tv_cuts += 1;
                    self.onboard = matches!(view, race_director::View::Follow { .. });
                }
                pose = step.tv;
            }
            self.shown = match pose {
                Some(pose) => {
                    let frame = atv_core::camera::CameraFrame {
                        position: pose.position,
                        target: pose.target,
                        fov: pose.fov,
                        preset: 0,
                    };
                    let view = atv_game::ride::view::cinematic(&frame, aspect);
                    let [x, y, z] = view.position;
                    (
                        Camera {
                            x,
                            y,
                            z,
                            yaw: view.yaw,
                            pitch: view.pitch,
                        },
                        view.focal_length,
                    )
                }
                None if self.onboard => (moment.camera, moment.focal),
                None => self.shown,
            };
        } else if self.onboard {
            self.shown = (moment.camera, moment.focal);
        }
        self.shown
    }
}

/// The kinds of vantage the director cuts between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setup {
    /// High and wide beside the line ahead.
    Crane,
    /// Beside the line ahead at head height.
    Trackside,
    /// Close to the ground beside the line.
    Low,
    /// On the line ahead, facing the rider coming in.
    HeadOn,
    /// Flies alongside and above the rider.
    Aerial,
    /// The ride's own chase camera as it was.
    Onboard,
}

/// The cut order (seven long, so each setup alternates sides per round).
const ORDER: [Setup; 7] = [
    Setup::Crane,
    Setup::Trackside,
    Setup::Low,
    Setup::HeadOn,
    Setup::Aerial,
    Setup::Trackside,
    Setup::Onboard,
];
/// A fixed vantage holds at least this long (steps) before a pass cuts it.
const MIN_HOLD: f64 = 90.;
/// A move this large between frames (steps) is a scrub or jump: a new cut.
const JUMP: f64 = 40.;

struct Plan {
    /// How far ahead along the recorded line the vantage stands (steps).
    lead: f64,
    /// Beside the line (units, left of the direction of travel).
    side: f32,
    height: f32,
    /// The framing: about this much of the scene across the lens (units).
    frame: f32,
    /// The longest hold (steps).
    hold: f64,
}

impl Setup {
    fn plan(self) -> Plan {
        let plan = |lead, side, height, frame, seconds: f64| Plan {
            lead,
            side,
            height,
            frame,
            hold: seconds * 60.,
        };
        match self {
            Self::Crane => plan(150., 45., 28., 18., 7.),
            Self::Trackside => plan(90., 28., 7., 10., 6.),
            Self::Low => plan(60., 12., 1.2, 7., 4.),
            Self::HeadOn => plan(180., 6., 4., 9., 5.),
            Self::Aerial => plan(0., 18., 16., 12., 5.),
            Self::Onboard => plan(0., 0., 0., 0., 3.),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Crane => "Crane",
            Self::Trackside => "Trackside",
            Self::Low => "Low",
            Self::HeadOn => "Head-on",
            Self::Aerial => "Aerial",
            Self::Onboard => "Onboard",
        }
    }
}

#[derive(Debug)]
pub struct Director {
    /// The cuts made (the next setup's place in `ORDER`).
    pub cuts: u32,
    pub setup: Setup,
    /// The fixed vantage's eye.
    eye: [f32; 3],
    /// The replay time of the current cut and of the last frame.
    at: f64,
    last: f64,
    /// The rider's nearest approach to the vantage since the cut.
    nearest: f32,
    live: bool,
}

impl Default for Director {
    fn default() -> Self {
        Self {
            cuts: 0,
            setup: ORDER[0],
            eye: [0.; 3],
            at: 0.,
            last: 0.,
            nearest: f32::MAX,
            live: false,
        }
    }
}

/// The terrain rises above the sight line from `eye` to `target`.
fn hidden(eye: [f32; 3], target: [f32; 3], ground: &dyn Fn(f32, f32) -> f32) -> bool {
    (1..8).any(|i| {
        let t = i as f32 / 8.;
        let p: [f32; 3] = std::array::from_fn(|k| eye[k] + (target[k] - eye[k]) * t);
        let floor = ground(p[0], p[2]);
        floor.is_finite() && floor > p[1] + 0.5
    })
}

/// A lens that keeps about `frame` units across at `distance`.
fn lens(frame: f32, distance: f32) -> f32 {
    let half = (frame / distance.max(1.))
        .atan_d()
        .clamp(4f32.to_radians(), 32f32.to_radians());
    1. / half.tan_d()
}

impl Director {
    /// The broadcast camera at replay `time` for `moment` (sampled there).
    pub fn camera(
        &mut self,
        time: f64,
        moment: &Moment,
        recorder: &Recorder,
        ground: &dyn Fn(f32, f32) -> f32,
    ) -> (Camera, f32) {
        let w = &moment.player.quad_world;
        let subject = [w[12], w[13] + 2., w[14]];
        let jumped = !self.live || time < self.at || (time - self.last).abs() > JUMP;
        self.last = time;
        if jumped || self.done(time, subject, ground) {
            self.cut(time, subject, recorder, ground);
        }
        let plan = self.setup.plan();
        match self.setup {
            Setup::Onboard => (moment.camera, moment.focal),
            Setup::Aerial => {
                let behind = recorder.origin_at(time - 30.).unwrap_or(subject);
                let ahead = recorder.origin_at(time + 30.).unwrap_or(subject);
                let (dx, dz) = (ahead[0] - behind[0], ahead[2] - behind[2]);
                let length = dx.hypot_d(dz);
                let (ux, uz) = if length > 1. {
                    (dx / length, dz / length)
                } else {
                    (0., 1.)
                };
                let side = plan.side * self.sign();
                let (x, z) = (
                    subject[0] - ux * 12. - uz * side,
                    subject[2] - uz * 12. + ux * side,
                );
                let floor = ground(x, z);
                let base = if floor.is_finite() {
                    floor.max(subject[1])
                } else {
                    subject[1]
                };
                let eye = [x, base + plan.height, z];
                (look(eye, subject), lens(plan.frame, distance(eye, subject)))
            }
            _ => (
                look(self.eye, subject),
                lens(plan.frame, distance(self.eye, subject)),
            ),
        }
    }

    fn sign(&self) -> f32 {
        if self.cuts.is_multiple_of(2) { 1. } else { -1. }
    }

    /// The current setup has had its moment.
    fn done(&mut self, time: f64, subject: [f32; 3], ground: &dyn Fn(f32, f32) -> f32) -> bool {
        let plan = self.setup.plan();
        let held = time - self.at;
        if held >= plan.hold {
            return true;
        }
        if matches!(self.setup, Setup::Onboard | Setup::Aerial) {
            return false;
        }
        let d = distance(self.eye, subject);
        self.nearest = self.nearest.min(d);
        // Passed and pulling away, or behind a hill.
        held >= MIN_HOLD && (d > self.nearest + 30. && d > 45. || hidden(self.eye, subject, ground))
    }

    fn cut(
        &mut self,
        time: f64,
        subject: [f32; 3],
        recorder: &Recorder,
        ground: &dyn Fn(f32, f32) -> f32,
    ) {
        if self.live {
            self.cuts = self.cuts.wrapping_add(1);
        }
        self.live = true;
        self.at = time;
        self.setup = ORDER[self.cuts as usize % ORDER.len()];
        self.nearest = f32::MAX;
        let plan = self.setup.plan();
        if matches!(self.setup, Setup::Onboard | Setup::Aerial) {
            return;
        }
        let ahead = recorder.origin_at(time + plan.lead).unwrap_or(subject);
        let side = plan.side * self.sign();
        // This side, else the other, else high above the line.
        let near = vantage_beside(subject, ahead, side, plan.height, ground);
        let far = vantage_beside(subject, ahead, -side, plan.height, ground);
        self.eye = if !hidden(near, subject, ground) {
            near
        } else if !hidden(far, subject, ground) {
            far
        } else {
            vantage_beside(subject, ahead, side, plan.height.max(30.), ground)
        };
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot_d(a[2] - b[2]).hypot_d(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(x: f32) -> Moment {
        let mut m = Moment::default();
        m.player.quad_world = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, 0., 0., 1.,
        ];
        m
    }

    #[test]
    fn cuts_after_the_pass_and_on_a_jump() {
        let recorder = Recorder::default();
        let flat = |_: f32, _: f32| 0.;
        let mut d = Director::default();
        // An empty recording: vantages stand beside the rider.
        d.camera(0., &moment(0.), &recorder, &flat);
        assert_eq!((d.cuts, d.setup), (0, Setup::Crane));
        // Held while the rider is near.
        d.camera(30., &moment(0.), &recorder, &flat);
        assert_eq!(d.cuts, 0);
        // The hold runs out.
        let mut t = 30.;
        while t < 7. * 60. {
            t += 20.;
            d.camera(t, &moment(0.), &recorder, &flat);
        }
        assert_eq!((d.cuts, d.setup), (1, Setup::Trackside));
        // The rider passes the trackside vantage and pulls away.
        let start = t;
        let mut x = 0.;
        while d.cuts == 1 && t < start + 6. * 60. {
            t += 10.;
            x += 8.;
            d.camera(t, &moment(x), &recorder, &flat);
        }
        assert_eq!(d.setup, Setup::Low);
        assert!(
            t - start >= MIN_HOLD && t - start < 4. * 60.,
            "cut on the pass"
        );
        // A scrub back cuts at once.
        d.camera(10., &moment(0.), &recorder, &flat);
        assert_eq!(d.cuts, 3);
    }

    #[test]
    fn a_hidden_vantage_moves() {
        // A cliff on the left (-x) of a rider heading +z hides that side.
        let wall = |x: f32, _: f32| if x < -5. { 100. } else { 0. };
        assert!(hidden([-20., 2., 0.], [0., 2., 0.], &wall));
        assert!(!hidden([20., 2., 0.], [0., 2., 0.], &wall));
        let mut d = Director::default();
        let (camera, focal) = d.camera(0., &moment(0.), &Recorder::default(), &wall);
        assert!(camera.x > 5. && focal > 1., "{camera:?}");
    }
}
