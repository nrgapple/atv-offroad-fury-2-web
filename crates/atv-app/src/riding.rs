//! The ride loop shared by desktop and browser builds: input sampling, the
//! browser's fixed-step batching (pumpSimulation in web/game.js) and the chase
//! presentation of the recovered camera.
use atv_game::ride::present::{Presentation, Presented, Snapshot};
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
    pub bindings: Bindings,
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
    /// The pause menu is open: it, not the Pause binding, resumes the ride.
    pub menu_open: bool,
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
    /// Each of those steps' race presentation (crowd, flashes, fireworks,
    /// gate, lamp sounds, race music).
    pub step_race: Vec<atv_game::ride::race::StepEvents>,
    /// Wheel dust, chunks, spray and wet births, stepped after each ride step.
    pub effects: Option<atv_game::wheel_effects::WheelEffects>,
    /// Optional Crazy/Gorn crashes (off under Original).
    pub crashes: Option<atv_game::crash::Crashes>,
    /// Enhanced/High ATV interpolation (motorcycles always interpolate).
    pub interpolation: bool,
    presentation: Presentation,
    /// The latest delivered state, and the presentation clock (ms).
    delivered: Option<Snapshot>,
    clock: f64,
    /// Whether the latest frame drew an interpolated state, and its alpha.
    pub presented: (bool, f32),
    /// Parity tooling: every delivered state and sampled presentation, in
    /// the browser's frame shape (tools/compare_presentation.mjs).
    pub present_log: Option<Vec<serde_json::Value>>,
    /// In a multiplayer room: pause and blur keep neutral-input simulation
    /// running so other riders see this one (web/game.js online pause).
    pub online: bool,
    /// Other riders' bodies for owner-local contacts on each step.
    pub peers: Vec<atv_game::multiplayer::PeerBody>,
    /// Steps on which a rider contact touched.
    pub contacts: u64,
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
            menu_open: false,
            input_cleared: false,
            sound: None,
            focused: true,
            autodrive: None,
            autodrive_done: false,
            sim_ms: 0.,
            step_cameras: Vec::new(),
            step_rest: Vec::new(),
            step_race: Vec::new(),
            effects: None,
            crashes: None,
            interpolation: false,
            presentation: Presentation::default(),
            delivered: None,
            clock: 0.,
            presented: (false, 1.),
            present_log: None,
            online: false,
            peers: Vec::new(),
            contacts: 0,
        }
    }

    pub fn key_released(&mut self, keys: &dyn Fn(&str) -> bool) {
        self.bindings.key_released(keys);
    }

    pub fn key_pressed(&mut self, code: &str, now: f64, keys: &dyn Fn(&str) -> bool) {
        self.bindings.key_pressed(code, now, keys);
    }

    /// Reset ATV (Backspace/R, or Pause → Reset ATV).
    pub fn reset(&mut self) -> Result<(), String> {
        self.clear_input();
        self.ride.reset()?;
        if let Some(effects) = &mut self.effects {
            effects.reset(&self.ride)?;
        }
        self.orbit = 0.;
        // The reset frame (op 23) is the next segment's start.
        self.delivered = None;
        if self.presents() {
            let current = deliver(&self.ride, &self.crashes)?;
            if let Some(log) = &mut self.present_log {
                log.push(serde_json::json!({"deliver": current.browser_frame()}));
            }
            self.delivered = Some(current);
        }
        Ok(())
    }

    /// A multiplayer arrival beside another rider (spawn), as Reset places.
    pub fn place_beside(&mut self, origin: [f32; 3], yaw: f32) -> Result<(), String> {
        self.clear_input();
        self.ride.place_beside(origin, yaw)?;
        if let Some(effects) = &mut self.effects {
            effects.reset(&self.ride)?;
        }
        self.orbit = 0.;
        self.delivered = None;
        Ok(())
    }

    /// Blur, pause or reset: drop held controls and the step accumulator.
    pub fn clear_input(&mut self) {
        self.input_cleared = true;
        self.cycle_requested = false;
        self.bindings.cancel();
        self.accumulator = 0.;
        self.rush.reset();
        self.presentation.reset();
    }

    /// Advances one rendered frame of `dt` seconds (already capped at 50 ms).
    pub fn frame(&mut self, input: &Sample, touch: TouchSampler, dt: f32) -> Result<(), String> {
        let started = web_time::Instant::now();
        self.clock += f64::from(dt) * 1000.;
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
        self.step_race.clear();
        // Online, a paused or unfocused rider keeps riding with neutral input:
        // the room still sees it and its world clocks keep running.
        if self.online
            && !self.held
            && self.autodrive.is_none()
            && (!self.focused || (self.paused && self.menu_open))
        {
            self.accumulator = (self.accumulator + dt).min(0.1);
            let count = ((self.accumulator * 60.).floor() as u32).min(4);
            if count > 0 {
                self.accumulator -= count as f32 / 60.;
                self.run(&Controls::default(), count)?;
                self.accept_batch(count, false)?;
            }
            return Ok(());
        }
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
                let race = self.ride.take_race_events();
                if let Some(sound) = &mut self.sound {
                    sound.step(self.ride.sound());
                    sound.race(&race);
                }
                self.step_race.push(race);
                self.steps_last_frame += 1;
            }
            if count > 0
                && let Some(sound) = &mut self.sound
            {
                sound.frame(self.ride.sound());
            }
            if count > 0 {
                self.accept_batch(count, self.focused && !self.menu_open)?;
            }
            return Ok(());
        }
        if self.held {
            return Ok(());
        }
        if self.paused && self.menu_open {
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
            self.bindings
                .value(ride::bindings::Action::CameraRight, input.keys, input.pad)
                - self
                    .bindings
                    .value(ride::bindings::Action::CameraLeft, input.keys, input.pad);
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
            return self.reset();
        }
        for _ in 0..count {
            self.accumulator -= 1. / 60.;
        }
        let cycle = read.camera || std::mem::take(&mut self.cycle_requested);
        if cycle {
            self.orbit = 0.;
            self.ride.cycle_camera();
        }
        self.run(&read.controls, count)?;
        // A camera cycle snaps.
        self.accept_batch(count, self.focused && !self.menu_open && !cycle)
    }

    /// pumpSimulation: each delivered batch of `ticks` steps starts a
    /// presentation segment from the previously delivered state.
    fn accept_batch(&mut self, ticks: u32, continuous: bool) -> Result<(), String> {
        if !self.presents() {
            return Ok(());
        }
        let previous = self.delivered.take();
        let current = deliver(&self.ride, &self.crashes)?;
        match &previous {
            Some(previous) => self.presentation.accept(
                previous,
                &current,
                self.clock,
                ticks,
                continuous,
                self.interpolation,
            ),
            None => self.presentation.reset(),
        }
        if let Some(log) = &mut self.present_log {
            log.push(
                serde_json::json!({"accept": {"now": self.clock, "ticks": ticks,
                "continuous": continuous, "frame": current.browser_frame()}}),
            );
        }
        self.delivered = Some(current);
        Ok(())
    }

    /// The state to draw this frame: the motorcycle presenter, else (with
    /// the preset's interpolation) the ATV presenter, between delivered
    /// 60 Hz states; None draws the ride's latest state directly.
    pub fn sample_presentation(&mut self) -> Result<Option<Presented>, String> {
        if !self.presents() {
            self.presented = (false, 1.);
            return Ok(None);
        }
        if self.delivered.is_none() {
            let current = deliver(&self.ride, &self.crashes)?;
            if let Some(log) = &mut self.present_log {
                log.push(serde_json::json!({"deliver": current.browser_frame()}));
            }
            self.delivered = Some(current);
        }
        let active = self.focused && !self.menu_open;
        let delivered = self.delivered.as_ref().unwrap();
        let presented = self.presentation.sample(delivered, self.clock, active);
        self.presented = (presented.interpolated, presented.alpha);
        if let Some(log) = &mut self.present_log {
            let s = &presented.snapshot;
            log.push(serde_json::json!({"sample": {"now": self.clock, "active": active,
                "interpolated": presented.interpolated, "alpha": presented.alpha,
                "origin": s.origin, "orientation": s.orientation,
                "camera": [s.camera.position, s.camera.target],
                "quad": s.quad_nodes.first(), "rider": s.rider_nodes.first(),
                "rider_world": s.rider_world,
                "cpu": s.opponents.as_ref().and_then(|o| o.first()).map(|o| (o.origin, o.physical.as_ref().map(|p| p.orientation)))}}));
        }
        Ok(Some(presented))
    }

    /// `count` fixed steps with one control sample (an op 50 batch).
    pub fn run(&mut self, controls: &Controls, count: u32) -> Result<(), String> {
        for _ in 0..count {
            for peer in &self.peers {
                if self
                    .ride
                    .peer_contact(peer.position, peer.velocity, peer.orientation, peer.tie)
                {
                    self.contacts += 1;
                }
            }
            self.ride.step(controls, self.orbit)?;
            if let Some(effects) = &mut self.effects {
                effects.step(&self.ride)?;
            }
            if let Some(crashes) = &mut self.crashes {
                crashes.step(&self.ride)?;
            }
            let race = self.ride.take_race_events();
            if let Some(sound) = &mut self.sound {
                sound.step(self.ride.sound());
                sound.race(&race);
            }
            self.step_race.push(race);
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

    /// Frames draw through the presenters: every motorcycle, and ATVs with
    /// the preset's interpolation.
    fn presents(&self) -> bool {
        self.interpolation || self.ride.is_motorcycle()
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
        // render(): the motorcycle presenter, else (Enhanced/High) the ATV
        // presenter, between the delivered 60 Hz states; Original ATVs draw
        // the latest state.
        let presented = self.sample_presentation()?;
        let shown = presented
            .as_ref()
            .filter(|p| p.interpolated)
            .map(|p| &p.snapshot);
        let camera = match shown {
            Some(shown) => (shown.camera, shown.camera_mode),
            None => {
                let (frame, mode) = self.ride.camera();
                (*frame, mode)
            }
        };
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
        } = match (&camera.0, camera.1) {
            (frame, ride::CameraMode::Intro | ride::CameraMode::Tv) => {
                atv_game::ride::view::cinematic(frame, aspect)
            }
            (frame, _) => chase(frame, aspect),
        };
        // buildTerrainMesh again once the rider leaves 48 cells of the patch.
        let at = cells(venue, frame.origin);
        if (at[0] - self.center[0]).abs() > 48. || (at[1] - self.center[1]).abs() > 48. {
            self.center = at;
            scene.recenter_terrain(gpu, venue, [frame.origin[0], frame.origin[2]]);
        }
        let (mut pose, cpu_poses) = match shown {
            Some(shown) => (shown.pose(), shown.cpu_poses()),
            None => (self.ride.pose()?, self.ride.cpu_poses()),
        };
        // Crazy/Gorn: the ragdoll and broken parts replace the drawn pose
        // (drawnQuadPose and acceptFrame in game.js); the rider's ragdoll
        // matrices are world space. An interpolated ATV draws the blended
        // crash pose.
        let crash_pose = match (
            shown.and_then(|s| s.crash.as_ref()),
            self.crashes.as_ref().and_then(|c| c.frame()),
        ) {
            (Some((quad, rider)), Some(_)) => Some((quad, rider)),
            (_, Some(frame)) => Some((&frame.quad_node_matrices, &frame.rider_node_matrices)),
            _ => None,
        };
        if let Some((quad, rider)) = crash_pose {
            if quad.len() == pose.quad_nodes.len() {
                pose.quad_nodes.clone_from(quad);
            }
            if !rider.is_empty() {
                pose.rider_nodes.clone_from(rider);
                pose.rider_world = [
                    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
                ];
            }
        }
        let crash = self.crashes.as_ref().and_then(|c| c.frame());
        scene.set_crash(
            gpu,
            crash.map_or(&[][..], |c| &c.severed_nodes[..]),
            crash
                .filter(|_| {
                    self.crashes
                        .as_ref()
                        .is_some_and(|c| c.mode == atv_game::crash::CrashMode::Gorn)
                })
                .map(|c| &c.blood),
        );
        scene.set_ride_mud(self.ride.mud_amount(), !self.ride.is_motorcycle());
        scene.pose_ride(gpu, &pose);
        scene.pose_race_cpus(gpu, &cpu_poses, position);
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

/// The state just delivered, as the presentation reads it.
fn deliver(ride: &Ride, crashes: &Option<atv_game::crash::Crashes>) -> Result<Snapshot, String> {
    Snapshot::capture(ride, crashes.as_ref().and_then(|c| c.frame()))
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
