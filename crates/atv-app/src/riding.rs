//! The ride loop shared by desktop and browser builds: input sampling, the
//! browser's fixed-step batching (pumpSimulation in web/game.js) and the chase
//! presentation of the recovered camera.
use atv_game::{
    ride::{
        self, Controls, Ride,
        bindings::{Bindings, Pad},
        view::{SpeedRush, View, chase},
    },
    venue::Venue,
};
use atv_render::{Camera, Gpu, SceneRenderer};

pub struct Riding {
    pub ride: Ride,
    bindings: Bindings,
    /// Follow-camera orbit offset (Z/X keys, right stick, mouse drag).
    pub orbit: f32,
    accumulator: f32,
    rush: SpeedRush,
    pub paused: bool,
    /// A replayed parity state: input never resumes it.
    pub held: bool,
    /// Fine terrain patch centre, in terrain cells.
    center: [f32; 2],
    pub steps_last_frame: u32,
    /// Each of those steps' follow camera (position, target), for the world
    /// effects that track the simulation camera (Snow).
    pub step_cameras: Vec<([f32; 3], [f32; 3])>,
}

/// What a frame sampled from the keyboard/controller.
pub struct Sample<'a> {
    pub keys: &'a dyn Fn(&str) -> bool,
    pub pad: Option<&'a Pad>,
    /// Seconds, for keyboard trick-direction timing.
    pub now: f64,
    /// Mouse drag this frame, in pixels.
    pub drag: f32,
}

impl Riding {
    pub fn new(ride: Ride, venue: &Venue) -> Self {
        let origin = ride.frame().origin;
        Self {
            ride,
            bindings: Bindings::default(),
            orbit: 0.,
            accumulator: 0.,
            rush: SpeedRush::default(),
            paused: false,
            held: false,
            center: cells(venue, origin),
            steps_last_frame: 0,
            step_cameras: Vec::new(),
        }
    }

    pub fn key_pressed(&mut self, code: &str, now: f64, keys: &dyn Fn(&str) -> bool) {
        self.bindings.key_pressed(code, now, keys);
    }

    /// Blur, pause or reset: drop held controls and the step accumulator.
    pub fn clear_input(&mut self) {
        self.bindings.cancel();
        self.accumulator = 0.;
        self.rush.reset();
    }

    /// Advances one rendered frame of `dt` seconds (already capped at 50 ms).
    pub fn frame(&mut self, input: &Sample, dt: f32) -> Result<(), String> {
        self.steps_last_frame = 0;
        self.step_cameras.clear();
        if self.held {
            return Ok(());
        }
        if self.paused {
            let read = self.bindings.read(input.keys, input.pad, input.now);
            if read.pause {
                self.paused = false;
                self.clear_input();
            }
            return Ok(());
        }
        // updateCamera -> followCamera: orbit follows render time.
        let orbit_axis =
            ride::bindings::value(ride::bindings::Action::CameraRight, input.keys, input.pad)
                - ride::bindings::value(ride::bindings::Action::CameraLeft, input.keys, input.pad);
        self.orbit -= orbit_axis * dt * 1.7;
        self.orbit -= input.drag * 0.004;
        self.accumulator = (self.accumulator + dt).min(0.1);
        let count = ((self.accumulator * 60.).floor() as u32).min(4);
        if count < 1 {
            return Ok(());
        }
        let read = self.bindings.read(input.keys, input.pad, input.now);
        if read.pause {
            self.paused = true;
            self.clear_input();
            return Ok(());
        }
        if read.reset {
            self.clear_input();
            self.ride.reset()?;
            self.orbit = 0.;
            return Ok(());
        }
        for _ in 0..count {
            self.accumulator -= 1. / 60.;
        }
        if read.camera {
            self.orbit = 0.;
            self.ride.cycle_camera();
        }
        self.run(&read.controls, count)
    }

    /// `count` fixed steps with one control sample (an op 50 batch).
    pub fn run(&mut self, controls: &Controls, count: u32) -> Result<(), String> {
        for _ in 0..count {
            self.ride.step(controls, self.orbit)?;
            let camera = self.ride.camera().0;
            self.step_cameras.push((camera.position, camera.target));
        }
        self.steps_last_frame += count;
        Ok(())
    }

    /// The scene camera for this frame and the pose upload.
    pub fn present(
        &mut self,
        gpu: &Gpu,
        scene: &mut SceneRenderer,
        venue: &Venue,
        aspect: f32,
        dt: f32,
    ) -> Result<(Camera, f32), String> {
        let frame = self.ride.frame();
        let riding = frame.wreck.phase == ride::Phase::Riding;
        self.rush
            .update(frame.speed_magnitude, riding && !self.paused, dt);
        let View {
            position,
            yaw,
            pitch,
            focal_length,
        } = chase(self.ride.camera().0, aspect);
        // buildTerrainMesh again once the rider leaves 48 cells of the patch.
        let at = cells(venue, frame.origin);
        if (at[0] - self.center[0]).abs() > 48. || (at[1] - self.center[1]).abs() > 48. {
            self.center = at;
            scene.recenter_terrain(gpu, venue, [frame.origin[0], frame.origin[2]]);
        }
        scene.pose_ride(gpu, &self.ride.pose()?);
        Ok((
            Camera {
                x: position[0],
                y: position[1],
                z: position[2],
                yaw,
                pitch,
            },
            focal_length * self.rush.focal_scale(),
        ))
    }
}

fn cells(venue: &Venue, origin: [f32; 3]) -> [f32; 2] {
    let t = &venue.terrain;
    [
        (origin[0] / t.spacing).clamp(0., (t.width - 1) as f32),
        (origin[2] / t.spacing).clamp(0., (t.depth - 1) as f32),
    ]
}
