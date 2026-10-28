//! The ride loop shared by desktop and browser builds: input sampling, the
//! browser's fixed-step batching (pumpSimulation in web/game.js) and the chase
//! presentation of the recovered camera.
use atv_game::ride::present::{Presentation, Presented, Snapshot};
use atv_game::{
    ride::{
        self, Controls, Ride,
        bindings::{Bindings, Pad},
        view::{SpeedRush, View},
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
    /// The Training Course holds the ATV while a line plays (+0x94c): the
    /// player's drive input is replaced by a full brake.
    pub locked: bool,
    /// Fine terrain patch centre, in terrain cells.
    center: [f32; 2],
    /// Split screen: player 2's origin this frame, which gets its own fine
    /// terrain patch (`terrain_mesh::disjoint`), and that patch's centre.
    pub split_origin: Option<[f32; 3]>,
    split_center: Option<[f32; 2]>,
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
    /// Settings → Alpha presentation (web/hype.js): slow motion and the
    /// big-air camera read it.
    pub hype: atv_game::ride::hype::HypeDirector,
    pub alpha_slowmo: bool,
    pub alpha_big_air_camera: bool,
    /// Alpha Air dark blur (the screen pass reads `hype.blur`).
    pub alpha_air_blur: bool,
    /// Alpha Speed & air muffle and the (muffle, wind) last sent.
    pub alpha_muffle: bool,
    hype_audio: [f32; 2],
    /// Alpha Landing burst: dust, clods and a thump at hard landings.
    pub alpha_landing_burst: bool,
    pub burst: atv_game::burst::BurstEmitter,
    dust_tint: atv_game::wheel_effects::DustTint,
    /// Landing thumps played (diagnostics).
    pub thumps: u32,
    /// Enhanced/High: draw sunk wheels on the visible ground
    /// (graphics.js `wheelReprojection`).
    pub wheel_reprojection: bool,
    /// The rendered terrain heights the reprojection reads.
    ground: std::rc::Rc<atv_core::terrain::Heightfield>,
    /// The largest compression the last reprojection added (game.js
    /// `wheelLift`, diagnostics only).
    pub wheel_lift: std::cell::Cell<f32>,
    /// ATV dirt buildup (graphics.js DirtBuildup), drawn when `dirt_enabled`
    /// (Enhanced/High) or Alpha Heavy mud.
    pub dirt: atv_game::dirt::DirtBuildup,
    pub dirt_enabled: bool,
    /// Settings → Chase camera: Assisted (UphillSightline).
    pub chase_assist: bool,
    sightline: atv_game::ride::view::UphillSightline,
    /// Settings → Crash camera: Tight (the default) frames Crazy/Gorn crashes
    /// with crashPresentation; Original keeps the recovered wreck camera.
    pub crash_camera_tight: bool,
    /// LOD-0 node boxes of the quad and rider, built at the first tight shot.
    crash_bounds: Option<(
        Vec<atv_game::ride::view::NodeBox>,
        Vec<atv_game::ride::view::NodeBox>,
    )>,
    /// ImpactEvents and controller rumble (Settings → Vibration).
    pub impacts: atv_game::ride::impact::ImpactEvents,
    pub rumble: atv_game::ride::impact::Rumble,
    pub vibration: bool,
    /// The previous accepted frame's reset count and Riding phase.
    impact_previous: Option<(u32, bool)>,
    /// The latest impact severity (ft/s; Alpha landing effects read it).
    pub impact_severity: f64,
    /// Pulses for the controller (or a touch device's vibration), drained
    /// by the app each frame.
    pub pulses: Vec<atv_game::ride::impact::Pulse>,
    /// In a multiplayer room: pause and blur keep neutral-input simulation
    /// running so other riders see this one (web/game.js online pause).
    pub online: bool,
    /// Steps on which a rider contact touched.
    pub contacts: u64,
    /// The room's relay clock: local origin and offset (ms). Each step's
    /// shared train phase follows it (multiplayer_world.rs).
    pub server_clock: Option<(web_time::Instant, f64)>,
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

/// What the hype director reads from the ride's latest step.
fn hype_frame(ride: &Ride) -> (atv_game::ride::hype::HypeFrame, Option<f32>) {
    let f = ride.frame();
    (
        atv_game::ride::hype::HypeFrame {
            origin: f.origin,
            linear_velocity: f.linear_velocity,
            speed: f.speed_magnitude,
            reset_count: f.reset_count,
            riding: f.wreck.phase == ride::Phase::Riding,
            grounded: f.grounded.iter().any(|g| *g),
        },
        ride.ground(f.origin[0], f.origin[2]),
    )
}

/// Samples touch controls once per batch (game.js `drivingInput`).
pub type TouchSampler<'a> = &'a mut dyn FnMut() -> Option<atv_ui::touch::Controls>;

/// Other riders' contact bodies at a step's room-session time (ms), read
/// with that step's train phase (atv-web `collide` at each step's
/// `step_now`, after the train step).
pub type PeerSampler<'a> = &'a dyn Fn(u64, &Ride) -> Vec<atv_game::multiplayer::PeerBody>;

/// A room's contact source for one frame: the session clock now (ms) and
/// the sampler.
pub type Peers<'a> = Option<(u64, PeerSampler<'a>)>;

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
            locked: false,
            center: cells(venue, origin),
            split_origin: None,
            split_center: None,
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
            hype: Default::default(),
            alpha_slowmo: false,
            alpha_big_air_camera: false,
            alpha_air_blur: false,
            alpha_muffle: false,
            hype_audio: [0.; 2],
            alpha_landing_burst: false,
            burst: atv_game::burst::BurstEmitter::new(192),
            dust_tint: atv_game::wheel_effects::DustTint::new(venue),
            thumps: 0,
            wheel_reprojection: false,
            ground: venue.visible.clone(),
            wheel_lift: std::cell::Cell::new(0.),
            dirt: atv_game::dirt::DirtBuildup::new(venue.terrain_zones),
            dirt_enabled: false,
            chase_assist: false,
            sightline: Default::default(),
            crash_camera_tight: true,
            crash_bounds: None,
            impacts: Default::default(),
            rumble: Default::default(),
            vibration: true,
            impact_previous: None,
            impact_severity: 0.,
            pulses: Vec::new(),
            online: false,
            contacts: 0,
            server_clock: None,
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
        self.hype.reset();
        self.burst.reset();
        let (frame, ground) = hype_frame(&self.ride);
        self.hype.step(frame, ground);
        if let Some(effects) = &mut self.effects {
            effects.reset(&self.ride)?;
        }
        // A player Reset cleans the quad; the reset frame is accepted.
        self.dirt.reset();
        self.step_dirt()?;
        self.accept_impacts();
        self.orbit = 0.;
        // The reset frame (op 23) is the next segment's start.
        self.delivered = None;
        if self.presents() {
            let current = self.deliver()?;
            if let Some(log) = &mut self.present_log {
                log.push(serde_json::json!({"deliver": current.browser_frame()}));
            }
            self.delivered = Some(current);
        }
        Ok(())
    }

    /// A multiplayer arrival beside another rider (spawn), as Reset places.
    pub fn place_beside(
        &mut self,
        origin: [f32; 3],
        yaw: f32,
        velocity: [f32; 3],
    ) -> Result<(), String> {
        self.clear_input();
        self.ride.place_beside(origin, yaw, velocity)?;
        if let Some(effects) = &mut self.effects {
            effects.reset(&self.ride)?;
        }
        self.orbit = 0.;
        self.delivered = None;
        Ok(())
    }

    /// Test hook: stand on the train car whose model contains `model`.
    pub fn place_on_train(&mut self, car: usize) -> Result<(), String> {
        self.clear_input();
        self.ride.place_on_train(car)?;
        if let Some(effects) = &mut self.effects {
            effects.reset(&self.ride)?;
        }
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
    pub fn frame(
        &mut self,
        input: &Sample,
        touch: TouchSampler,
        dt: f32,
        peers: Peers,
    ) -> Result<(), String> {
        let started = web_time::Instant::now();
        self.clock += f64::from(dt) * 1000.;
        let result = self.frame_inner(input, touch, dt, peers);
        self.sync_hype_audio();
        if let Some(sound) = &mut self.sound {
            sound.maintain();
            sound.report(self.ride.steps());
        }
        self.sim_ms = started.elapsed().as_secs_f64() * 1000.;
        result
    }

    fn frame_inner(
        &mut self,
        input: &Sample,
        touch: TouchSampler,
        dt: f32,
        peers: Peers,
    ) -> Result<(), String> {
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
                self.run_with(&Controls::default(), count, peers)?;
                self.accept_batch(count, false)?;
            }
            return Ok(());
        }
        if !self.focused {
            return Ok(());
        }
        if self.autodrive.is_some() {
            // The browser's accumulator, with the script supplying each step.
            self.accumulator = (self.accumulator + dt).min(0.1);
            let count = ((self.accumulator * 60.).floor() as u32).min(4);
            for _ in 0..count {
                self.accumulator -= 1. / 60.;
                let next = self.autodrive.as_mut().and_then(|(controls, cursor)| {
                    let control = controls.get(*cursor).copied();
                    *cursor += 1;
                    control
                });
                let Some(control) = next else {
                    self.autodrive = None;
                    self.autodrive_done = true;
                    self.held = true;
                    return Ok(());
                };
                self.ride.step(&control, self.orbit)?;
                // The per-step presentation `run` performs.
                if let Some(effects) = &mut self.effects {
                    effects.step(&self.ride)?;
                }
                self.step_dirt()?;
                self.accept_impacts();
                if let Some(crashes) = &mut self.crashes {
                    crashes.step(&self.ride)?;
                }
                let (frame, ground) = hype_frame(&self.ride);
                self.hype.step(frame, ground);
                self.step_rest.push(rest_origin(&self.ride));
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
        // Alpha slow motion stretches real time between fixed steps
        // (Freestyle only: races and multiplayer rooms keep real time;
        // game.js updateHype `!raceSettings && !liveMultiplayer()`).
        let landed = self.hype.update(
            dt,
            slowmo_allowed(self.alpha_slowmo, self.ride.race().is_some(), self.online),
        );
        if let Some(landing) = landed {
            self.land(&landing)?;
        }
        self.accumulator = (self.accumulator + dt * self.hype.time_scale).min(0.1);
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
        if self.locked {
            read.controls = Controls {
                brake: 1.,
                ..Controls::default()
            };
        }
        self.run_with(&read.controls, count, peers)?;
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
        let current = self.deliver()?;
        match &previous {
            Some(previous) => self.presentation.accept(
                previous,
                &current,
                self.clock,
                ticks,
                continuous,
                self.interpolation || self.hype.time_scale < 1.,
                self.hype.time_scale,
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
            let current = self.deliver()?;
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
        self.run_with(controls, count, None)
    }

    /// [`Riding::run`] in a room: each step touches the other riders as
    /// sampled at that step's session time, after its train step.
    fn run_with(&mut self, controls: &Controls, count: u32, peers: Peers) -> Result<(), String> {
        let server = self
            .server_clock
            .map(|(origin, offset)| origin.elapsed().as_secs_f64() * 1000. + offset);
        for step in 0..count {
            // Each step's relay time, oldest first (op 50's step_now).
            self.ride.train_server_ms =
                server.map(|now| now - f64::from(count - 1 - step) * 1000. / 60.);
            let orbit = self.orbit;
            let contacts = &mut self.contacts;
            let mut touch = |ride: &mut Ride| {
                let Some((now, sample)) = peers else { return };
                let step_now = now.saturating_sub(u64::from(count - 1 - step) * 1000 / 60);
                for peer in sample(step_now, ride) {
                    if ride.peer_contact(peer.position, peer.velocity, peer.orientation, peer.tie) {
                        *contacts += 1;
                    }
                }
            };
            self.ride.step_with(controls, orbit, &mut touch)?;
            // acceptFrame: the Alpha hype director reads every step.
            let (frame, ground) = hype_frame(&self.ride);
            self.hype.step(frame, ground);
            if let Some(effects) = &mut self.effects {
                effects.step(&self.ride)?;
            }
            self.step_dirt()?;
            self.accept_impacts();
            if let Some(crashes) = &mut self.crashes {
                crashes.step(&self.ride)?;
            }
            let race = self.ride.take_race_events();
            if let Some(sound) = &mut self.sound {
                sound.step(self.ride.sound());
                sound.race(&race);
            }
            self.step_race.push(race);
            self.step_rest.push(rest_origin(&self.ride));
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
    /// The graphics context came back (web context loss): the scene
    /// rebuilds on `gpu` with its fine terrain patch around the rider, and
    /// the ride's quad, rider, race CPUs and wheel effects upload again from
    /// their CPU data. The ride itself (position, clock, race) is untouched.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn restore_gpu(
        &mut self,
        scene: &mut SceneRenderer,
        gpu: &Gpu,
        venue: &Venue,
    ) -> Result<(), String> {
        let origin = self.ride.frame().origin;
        self.center = cells(venue, origin);
        // Player 2's patch returns on the next presented frame.
        self.split_center = None;
        scene.rebuild(gpu, venue, Some([origin[0], origin[2]]));
        scene.set_ride(gpu, venue, &self.ride.quad, &self.ride.rider)?;
        scene.set_ride_paint(self.ride.paint());
        if let Some(race) = self.ride.race() {
            scene.set_race_cpus(gpu, venue, &race.draw)?;
        }
        if let Some(effects) = &self.effects {
            scene.set_wheel_effects(gpu, effects);
        }
        Ok(())
    }

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
        self.interpolation || self.ride.is_motorcycle() || self.alpha_slowmo
    }

    /// Settings → Speed rush intensity (`speedRushAmount`'s peak).
    pub fn set_speed_rush(&mut self, intensity: &str) {
        self.rush.peak = atv_game::ride::view::speed_rush_peak(intensity);
        if self.rush.peak == 0. {
            self.rush.reset();
        }
    }

    pub fn speed_rush_peak(&self) -> f64 {
        self.rush.peak
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
        self.present_view(gpu, Some(scene), venue, aspect, dt)
            .map(|(camera, focal, _)| (camera, focal))
    }

    /// The presented camera, focal length and pose. With no scene (split
    /// screen's second player, drawn as a scene rider slot) nothing is
    /// posed or recentred here.
    pub fn present_view(
        &mut self,
        gpu: &Gpu,
        mut scene: Option<&mut SceneRenderer>,
        venue: &Venue,
        aspect: f32,
        dt: f32,
    ) -> Result<(Camera, f32, atv_game::ride::Pose), String> {
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
        let subjects = self.crash_subjects()?;
        let frame = self.ride.frame();
        let riding = frame.wreck.phase == ride::Phase::Riding;
        self.rush.update(
            frame.speed_magnitude,
            // A held replay is not a pause: the browser keeps rendering it.
            riding && (!self.paused || self.held),
            dt,
        );
        // updateCamera: the uphill sightline follows the presented actor.
        let ground = self.ground.clone();
        let height = move |x: f32, z: f32| ground.height(f64::from(x), f64::from(z)) as f32;
        let actor = atv_game::ride::view::SightlineActor {
            origin: shown.map_or(frame.origin, |s| s.origin),
            orientation: shown.map_or(frame.orientation, |s| s.orientation),
            speed: frame.speed,
            grounded: frame.grounded,
            riding,
            reset_count: shown.map_or(frame.reset_count, |s| s.reset_count),
        };
        self.sightline.update(
            self.chase_assist.then_some(&actor),
            camera.1 == ride::CameraMode::Follow,
            camera.0.preset,
            dt,
            &height,
        );
        let assist = self.chase_assist.then_some((&self.sightline, actor.origin));
        let view = match (&camera.0, camera.1) {
            (frame, ride::CameraMode::Intro | ride::CameraMode::Tv) => {
                atv_game::ride::view::cinematic(frame, aspect)
            }
            (frame, _) if self.alpha_big_air_camera => {
                let ride = &self.ride;
                let shot = self.hype.camera(frame, |x, z| ride.ground(x, z));
                atv_game::ride::view::chase_assisted(&shot, aspect, assist, &height)
            }
            (frame, _) => atv_game::ride::view::chase_assisted(frame, aspect, assist, &height),
        };
        // presentCameraFrame: the tight Crazy/Gorn crash shot.
        let view = match subjects {
            Some(subjects) => atv_game::ride::view::crash(view, aspect, &subjects, &height),
            None => view,
        };
        let View {
            position,
            yaw,
            pitch,
            focal_length,
        } = view;
        // buildTerrainMesh again once the rider leaves 48 cells of the patch.
        let at = cells(venue, frame.origin);
        let far = |at: [f32; 2], center: [f32; 2]| {
            (at[0] - center[0]).abs() > 48. || (at[1] - center[1]).abs() > 48.
        };
        let split_at = self.split_origin.map(|o| cells(venue, o));
        let split_moved = match (split_at, self.split_center) {
            (Some(at), Some(center)) => far(at, center),
            (None, None) => false,
            _ => true,
        };
        if let Some(scene) = scene.as_deref_mut()
            && (far(at, self.center) || split_moved)
        {
            self.center = at;
            self.split_center = split_at;
            match self.split_origin {
                Some(second) => scene.recenter_terrain_many(
                    gpu,
                    venue,
                    &[[frame.origin[0], frame.origin[2]], [second[0], second[2]]],
                ),
                None => scene.recenter_terrain(gpu, venue, [frame.origin[0], frame.origin[2]]),
            }
        }
        let (mut pose, cpu_poses) = match shown {
            Some(shown) => (shown.pose(), shown.cpu_poses()),
            None => {
                let mut pose = self.ride.pose()?;
                // drawnQuadPose: the reprojected wheels on the latest state.
                if self.wheel_reprojection
                    && self.crashes.as_ref().is_none_or(|c| c.frame().is_none())
                {
                    let ground = &self.ground;
                    let height = |x: f32, z: f32| ground.height(f64::from(x), f64::from(z)) as f32;
                    if let Some((quad, lift)) = self.ride.reprojected_quad(&height) {
                        pose.quad_nodes = quad;
                        self.wheel_lift.set(lift);
                    } else {
                        self.wheel_lift.set(0.);
                    }
                }
                (pose, self.ride.cpu_poses())
            }
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
        if let Some(scene) = scene {
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
            scene.accept_burst(
                gpu,
                self.alpha_landing_burst
                    .then_some((&self.burst, self.ride.steps() as f32 / 60.)),
            );
            let mud = if self.ride.is_motorcycle() {
                self.ride.mud_amount()
            } else if self.dirt_enabled || self.dirt.heavy() {
                self.dirt.amount()
            } else {
                0.
            };
            scene.set_ride_mud(mud, !self.ride.is_motorcycle());
            scene.pose_ride(gpu, &pose);
            scene.pose_race_cpus(gpu, &cpu_poses, position);
        }
        Ok((
            Camera {
                x: position[0],
                y: position[1],
                z: position[2],
                yaw,
                pitch,
            },
            focal_length * self.rush.focal_scale(),
            pose,
        ))
    }
}

/// The state just delivered, as the presentation reads it.
impl Riding {
    /// The state just delivered, as the presentation reads it.
    fn deliver(&self) -> Result<Snapshot, String> {
        let ground = &self.ground;
        let height = |x: f32, z: f32| ground.height(f64::from(x), f64::from(z)) as f32;
        let (snapshot, lift) = Snapshot::capture(
            &self.ride,
            self.crashes.as_ref().and_then(|c| c.frame()),
            self.wheel_reprojection
                .then_some(&height as &dyn Fn(f32, f32) -> f32),
        )?;
        self.wheel_lift.set(lift);
        Ok(snapshot)
    }

    /// The lift the latest state's reprojection draws (parity traces).
    pub fn reprojected_lift(&self) -> f32 {
        let height = |x: f32, z: f32| self.visible_ground(x, z);
        self.ride
            .reprojected_quad(&height)
            .map_or(0., |(_, lift)| lift)
    }

    /// acceptFrame: ImpactEvents and the rumble for the latest frame (while
    /// focused with no menu open).
    fn accept_impacts(&mut self) {
        use atv_game::ride::impact::Actor;
        let actor = Actor::of(&self.ride);
        let enabled = self.focused && !self.menu_open;
        self.impact_severity = self.impacts.accept(actor, enabled, self.clock / 1000.);
        // acceptFrame: the authored boom belongs to Crazy/Gorn crashes.
        if self.impact_severity >= 40.
            && self
                .crashes
                .as_ref()
                .is_some_and(|c| c.mode != atv_game::crash::CrashMode::Original)
            && let Some(sound) = &mut self.sound
        {
            sound.crash_boom(self.impact_severity as f32);
        }
        if let Some(pulse) = self.rumble.accept(
            self.impact_previous,
            &actor,
            self.impact_severity,
            self.vibration && enabled,
            self.clock,
        ) {
            self.pulses.push(pulse);
        }
        self.impact_previous = Some((actor.reset_count, actor.riding));
    }

    /// One accepted frame of ATV dirt, only while it is drawn (the browser
    /// steps it on every preset; Original skips the four wheel samples).
    /// updateHype's landing: Alpha Landing burst (dust and clods at the
    /// visible ground, a thump) and Heavy mud's splat, unless a wheel is in
    /// water.
    fn land(&mut self, landing: &atv_game::ride::hype::Landing) -> Result<(), String> {
        if !self.focused || self.menu_open {
            return Ok(());
        }
        let wet = self.ride.dirt_sample()?.is_some_and(|s| {
            (0..4).any(|i| s.water_heights[i] > s.centers[i][1] - s.radii[i] * 0.5)
        });
        if wet {
            return Ok(());
        }
        if self.alpha_landing_burst {
            let [x, _, z] = landing.position;
            let v = self.ride.frame().linear_velocity;
            self.burst.burst(
                [x, self.visible_ground(x, z), z],
                landing.severity,
                self.ride.steps() as f64 / 60.,
                self.dust_tint.at(x, z),
                [v[0], v[2]],
            );
            if let Some(sound) = &mut self.sound
                && sound.landing_thump(landing.severity)
            {
                self.thumps += 1;
            }
        }
        if self.dirt.heavy() && !self.ride.is_motorcycle() {
            self.dirt
                .splat(f64::from((landing.severity * 0.9).min(60.)));
        }
        Ok(())
    }

    fn step_dirt(&mut self) -> Result<(), String> {
        if (self.dirt_enabled || self.dirt.heavy())
            && let Some(sample) = self.ride.dirt_sample()?
        {
            self.dirt.step(&sample, atv_game::dirt::STEP);
        }
        Ok(())
    }

    /// crashSubjects for the latest Crazy/Gorn crash frame (Crash camera:
    /// Tight), from the latest simulated pose as acceptFrame computes it.
    fn crash_subjects(&mut self) -> Result<Option<atv_game::ride::view::CrashSubjects>, String> {
        let Some(crash) = self
            .crashes
            .as_ref()
            .and_then(|c| c.frame())
            .filter(|_| self.crash_camera_tight)
        else {
            return Ok(None);
        };
        let ride = &self.ride;
        let (quad_bounds, rider_bounds) = self.crash_bounds.get_or_insert_with(|| {
            (
                atv_game::ride::view::mesh_node_bounds(&ride.quad.mesh),
                atv_game::ride::view::mesh_node_bounds(&ride.rider.mesh),
            )
        });
        let pose = ride.pose()?;
        Ok(Some(atv_game::ride::view::crash_subjects(
            &atv_game::ride::view::CrashPose {
                quad: (&ride.quad.mesh, quad_bounds),
                rider: (&ride.rider.mesh, rider_bounds),
                quad_nodes: if crash.quad_node_matrices.len() == pose.quad_nodes.len() {
                    &crash.quad_node_matrices
                } else {
                    &pose.quad_nodes
                },
                ragdoll: &crash.rider_node_matrices,
                rider_nodes: &pose.rider_nodes,
                rider_world: pose.rider_world,
                actor_world: pose.quad_world,
                origin: ride.frame().origin,
            },
        )))
    }

    /// The Assisted chase orbit (radians; `requestedHillOrbit`).
    pub fn hill_orbit(&self) -> f64 {
        self.sightline.lift
    }

    /// The Air dark blur amount this frame (0 with the toggle off).
    pub fn air_blur(&self) -> f32 {
        if self.alpha_air_blur {
            self.hype.blur
        } else {
            0.
        }
    }

    /// updateHype's sound: the director's muffle and wind while live, open
    /// in menus; sent when either moves more than 0.004.
    fn sync_hype_audio(&mut self) {
        if !self.alpha_muffle {
            return;
        }
        let live = self.focused && !self.menu_open && !self.paused;
        let next = if live {
            [self.hype.muffle, self.hype.wind]
        } else {
            [0.; 2]
        };
        if (next[0] - self.hype_audio[0]).abs() > 0.004
            || (next[1] - self.hype_audio[1]).abs() > 0.004
        {
            if let Some(sound) = &mut self.sound {
                sound.set_hype(next[0], next[1], self.hype.slam > 0.);
            }
            self.hype_audio = next;
        }
    }

    /// Alpha Speed & air muffle on or off (stopHype when it goes off).
    pub fn set_alpha_muffle(&mut self, on: bool) {
        if self.alpha_muffle
            && !on
            && let Some(sound) = &mut self.sound
        {
            sound.stop_hype();
        }
        if !on {
            self.hype_audio = [0.; 2];
        }
        self.alpha_muffle = on;
    }

    /// The visible terrain height (renderCore.terrainHeight).
    pub fn visible_ground(&self, x: f32, z: f32) -> f32 {
        self.ground.height(f64::from(x), f64::from(z)) as f32
    }
}

/// The step's body origin while Riding with every wheel down: the contact
/// shadows' resting lift (game.js `acceptFrame` measures every step).
fn rest_origin(ride: &Ride) -> Option<[f32; 3]> {
    let frame = ride.frame();
    (frame.wreck.phase == ride::Phase::Riding && frame.grounded.iter().all(|g| *g))
        .then_some(frame.origin)
}

/// Alpha Air slow motion stretches real time: Freestyle only (game.js
/// `updateHype`: not in races, not in a live multiplayer room).
fn slowmo_allowed(toggle: bool, race: bool, online: bool) -> bool {
    toggle && !race && !online
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
    // Arcade flips: touch steering stays the original air lean; its 360 axis
    // starts past full lock (keys and controllers spin with steering).
    controls.arcade_spin = controls
        .arcade_spin
        .map(|pad| (pad + t.spin).clamp(-1., 1.))
        .or((t.spin != 0.).then(|| t.spin.clamp(-1., 1.)));
    controls.flip_request = t.flip;
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

#[cfg(test)]
mod tests {
    #[test]
    fn air_slowmo_is_freestyle_only() {
        assert!(super::slowmo_allowed(true, false, false));
        assert!(!super::slowmo_allowed(false, false, false));
        // Races and multiplayer rooms keep real time (game.js updateHype).
        assert!(!super::slowmo_allowed(true, true, false));
        assert!(!super::slowmo_allowed(true, false, true));
    }
}
