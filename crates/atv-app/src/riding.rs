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
    /// The toolbar Camera button (game.js `cameraCycle`).
    pub cycle_requested: bool,
    /// The host must cancel touch ownership after keyboard pause/reset.
    pub input_cleared: bool,
    pub sound: Option<crate::sound::Sound>,
    focused: bool,
    /// Scripted controls driving the live loop (`--autodrive`), one per step.
    pub autodrive: Option<(Vec<Controls>, usize)>,
    /// Set once the autodrive script has played out.
    pub autodrive_done: bool,
    /// Simulation and sound time of the latest frame (ms).
    pub sim_ms: f64,
    /// Each of those steps' follow camera (position, target), for the world
    /// effects that track the simulation camera (Snow).
    pub step_cameras: Vec<([f32; 3], [f32; 3])>,
    /// Each step's body origin while Riding with every wheel down (contact
    /// shadow resting lift).
    pub step_rest: Vec<Option<[f32; 3]>>,
    /// Wheel dust, chunks, spray and wet births, stepped after each ride step.
    pub effects: Option<atv_game::wheel_effects::WheelEffects>,
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

/// Samples touch controls once per batch (game.js `drivingInput`).
pub type TouchSampler<'a> = &'a mut dyn FnMut() -> Option<atv_ui::touch::Controls>;

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
            cycle_requested: false,
            input_cleared: false,
            sound: None,
            focused: true,
            autodrive: None,
            autodrive_done: false,
            sim_ms: 0.,
            step_cameras: Vec::new(),
            step_rest: Vec::new(),
            effects: None,
        }
    }

    pub fn key_pressed(&mut self, code: &str, now: f64, keys: &dyn Fn(&str) -> bool) {
        self.bindings.key_pressed(code, now, keys);
    }

    /// Blur, pause or reset: drop held controls and the step accumulator.
    pub fn clear_input(&mut self) {
        self.input_cleared = true;
        self.cycle_requested = false;
        self.bindings.cancel();
        self.accumulator = 0.;
        self.rush.reset();
    }

    /// Advances one rendered frame of `dt` seconds (already capped at 50 ms).
    pub fn frame(&mut self, input: &Sample, touch: TouchSampler, dt: f32) -> Result<(), String> {
        let started = web_time::Instant::now();
        let result = self.frame_inner(input, touch, dt);
        if let Some(sound) = &mut self.sound {
            sound.maintain();
            sound.report(self.ride.steps());
        }
        self.sim_ms = started.elapsed().as_secs_f64() * 1000.;
        result
    }

    fn frame_inner(&mut self, input: &Sample, touch: TouchSampler, dt: f32) -> Result<(), String> {
        self.steps_last_frame = 0;
        self.input_cleared = false;
        self.step_cameras.clear();
        self.step_rest.clear();
        if !self.focused {
            return Ok(());
        }
        if let Some((controls, cursor)) = &mut self.autodrive {
            // The browser's accumulator, with the script supplying each step.
            self.accumulator = (self.accumulator + dt).min(0.1);
            let count = ((self.accumulator * 60.).floor() as u32).min(4);
            for _ in 0..count {
                self.accumulator -= 1. / 60.;
                let Some(control) = controls.get(*cursor).copied() else {
                    self.autodrive = None;
                    self.autodrive_done = true;
                    self.held = true;
                    return Ok(());
                };
                *cursor += 1;
                self.ride.step(&control, self.orbit)?;
                let frame = self.ride.frame();
                self.step_rest.push(
                    (frame.wreck.phase == ride::Phase::Riding && frame.grounded.iter().all(|g| *g))
                        .then_some(frame.origin),
                );
                let camera = self.ride.camera().0;
                self.step_cameras.push((camera.position, camera.target));
                if let Some(sound) = &mut self.sound {
                    sound.step(self.ride.sound());
                }
                self.steps_last_frame += 1;
            }
            if count > 0
                && let Some(sound) = &mut self.sound
            {
                sound.frame(self.ride.sound());
            }
            return Ok(());
        }
        if self.held {
            return Ok(());
        }
        if self.paused {
            let read = self.bindings.read(input.keys, input.pad, input.now);
            if read.pause {
                self.paused = false;
                self.clear_input();
                self.sync_sound();
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
        let mut read = self.bindings.read(input.keys, input.pad, input.now);
        if let Some(t) = touch() {
            merge_touch(&mut read.controls, &t);
        }
        if read.pause {
            self.paused = true;
            self.clear_input();
            self.sync_sound();
            return Ok(());
        }
        if read.reset {
            self.clear_input();
            self.ride.reset()?;
            if let Some(effects) = &mut self.effects {
                effects.reset(&self.ride)?;
            }
            self.orbit = 0.;
            return Ok(());
        }
        for _ in 0..count {
            self.accumulator -= 1. / 60.;
        }
        if read.camera || std::mem::take(&mut self.cycle_requested) {
            self.orbit = 0.;
            self.ride.cycle_camera();
        }
        self.run(&read.controls, count)
    }

    /// `count` fixed steps with one control sample (an op 50 batch).
    pub fn run(&mut self, controls: &Controls, count: u32) -> Result<(), String> {
        for _ in 0..count {
            self.ride.step(controls, self.orbit)?;
            if let Some(effects) = &mut self.effects {
                effects.step(&self.ride)?;
            }
            if let Some(sound) = &mut self.sound {
                sound.step(self.ride.sound());
            }
            let camera = self.ride.camera().0;
            self.step_cameras.push((camera.position, camera.target));
        }
        self.steps_last_frame += count;
        // GameAudio.update runs once per delivered batch.
        if let Some(sound) = &mut self.sound {
            sound.frame(self.ride.sound());
        }
        Ok(())
    }

    /// Window focus, like the browser's blur/focus: sound pauses while away.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.sync_sound();
    }

    pub(crate) fn sync_sound(&mut self) {
        if let Some(sound) = &mut self.sound {
            sound.set_lifecycle(self.focused, self.paused);
        }
    }

    /// The speed-rush amount after this frame's update (screen pass).
    pub fn rush_amount(&self) -> f32 {
        self.rush.amount()
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
        self.rush.update(
            frame.speed_magnitude,
            // A held replay is not a pause: the browser keeps rendering it.
            riding && (!self.paused || self.held),
            dt,
        );
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

/// game.js `drivingInput`: touch adds steer/lean, supplies its drive intent,
/// overrides preload and stunts, and joins the cancellation generation.
pub fn merge_touch(controls: &mut atv_game::ride::Controls, t: &atv_ui::touch::Controls) {
    controls.steer = (controls.steer + t.steer).clamp(-1., 1.);
    controls.lean += t.lean;
    controls.touch_drive = Some(t.sample.drive);
    controls.generation += t.generation;
    if t.sample.stunt_modifiers != 0 {
        controls.stunt_modifiers = t.sample.stunt_modifiers;
        controls.stunt_x = t.sample.stunt_x;
        controls.stunt_y = t.sample.stunt_y;
    }
    if let Some(axis) = t.sample.preload_axis {
        controls.preload_axis = Some(axis);
    }
}
