//! The Training Course session on the Training venue (research/TRAINING.md):
//! samples the ride for `atv_core::training::Course`, places the ATV at
//! each lesson's start, holds it while a line plays and grants the reward
//! to the loaded profile.
use atv_core::training::{Command, Course, Input, Phase};
use atv_game::ride::Ride;

pub struct TrainingSession {
    pub course: Course,
    /// The ride's step count last counted (the course clock).
    steps: u64,
    /// The first lesson has been placed.
    placed: bool,
    stunting: bool,
    /// The ride's wreck recoveries last seen.
    resets: u32,
    /// The reward was granted this visit.
    pub rewarded: bool,
    /// The course ended (Finished).
    pub finished: bool,
    /// The Profile Store's ATVs (uiQuad.txt order) for the reward.
    catalog: Vec<atv_core::store::Atv>,
    /// Test tooling: `trainingTest=fast` ends every line at once;
    /// `trainingPass=N` passes lessons from index N on as they start.
    fast: bool,
    pass_from: Option<usize>,
    /// The latest sample and line (diagnostics).
    last: Input,
    last_line: Option<u16>,
    /// The lines shown, newest last (at most 32).
    lines: Vec<u16>,
}

/// Whether the loaded profile owns store ATV 8, the blue Ravage Talon
/// (`00225980` reads profile +0x5c8 + 8). No profile reads as completed,
/// as the original skips the prize line without one.
pub fn completed(catalog: &[atv_core::store::Atv]) -> bool {
    let settings = atv_ui::settings::Settings::load();
    if !atv_ui::settings::profile_loaded(&settings) {
        return true;
    }
    let profile = atv_ui::settings::load_profile(&settings);
    catalog
        .get(8)
        .is_some_and(|atv| atv_core::store::atv_owned(catalog, atv.id, &profile))
}

/// `0021f440` on the loaded profile; false without one.
fn reward(catalog: &[atv_core::store::Atv]) -> bool {
    let mut settings = atv_ui::settings::Settings::load();
    if !atv_ui::settings::profile_loaded(&settings) {
        return false;
    }
    let mut profile = atv_ui::settings::load_profile(&settings);
    atv_core::store::training_reward(&mut profile, catalog);
    atv_ui::settings::save_profile(&mut settings, &profile)
}

/// Body pitch from the ride's [w, x, y, z] orientation: the local +Z
/// (forward) axis's climb, nose down negative.
fn pitch(orientation: [f32; 4]) -> f32 {
    let [w, x, y, z] = orientation;
    use atv_core::math::Det;
    (2. * (y * z - w * x)).clamp(-1., 1.).asin_d()
}

impl TrainingSession {
    pub fn new(completed_before: bool, catalog: Vec<atv_core::store::Atv>) -> Self {
        Self {
            catalog,
            fast: crate::url_flag("trainingTest=fast"),
            last: Input::default(),
            last_line: None,
            lines: Vec::new(),
            pass_from: (0..6).find(|n| crate::url_flag(&format!("trainingPass={n}"))),
            course: Course::new(completed_before),
            steps: 0,
            placed: false,
            stunting: false,
            resets: 0,
            rewarded: false,
            finished: false,
        }
    }

    /// Restart Training (`00226d60`): the introduction and lesson 1 again.
    pub fn restart(&mut self, ride: &mut Ride) {
        let completed = self.course.completed_before || self.rewarded;
        let catalog = std::mem::take(&mut self.catalog);
        *self = Self::new(completed, catalog);
        self.update(ride);
    }

    fn place(ride: &mut Ride, command: Command) {
        if let Command::Place { origin, yaw } = command
            && let Err(error) = ride.place_beside(origin, yaw, [0.; 3])
        {
            crate::status(&format!("Training placement failed: {error}"));
        }
    }

    /// Advances the course with the ride. Returns true once, when the
    /// course is over.
    pub fn update(&mut self, ride: &mut Ride) -> bool {
        if !self.placed {
            self.placed = true;
            if let Some(n) = (1..6).find(|n| crate::url_flag(&format!("trainingStart={n}"))) {
                self.course.begin_at(n);
            }
            Self::place(ride, self.course.start());
            ride.take_chains();
            self.steps = ride.steps();
            self.resets = ride.frame().reset_count;
        }
        let steps = ride.steps();
        let dt = steps.saturating_sub(self.steps) as f32 * atv_game::ride::STEP;
        self.steps = steps;
        if dt <= 0. {
            return false;
        }
        let landed = ride.take_chains().iter().any(|(success, _)| *success);
        let frame = ride.frame();
        let applied = ride.applied();
        let stunting = ride.stunting();
        let started = stunting && !self.stunting;
        self.stunting = stunting;
        // Forward drive is gas; brake counts as reverse once the ATV no
        // longer rolls forward (the original's signed drive command).
        // The held ATV (a line playing) has no drive command.
        let drive = if self.course.locked() {
            0.
        } else if applied.reverse {
            -applied.throttle.max(applied.brake)
        } else if applied.throttle > 0.05 {
            applied.throttle
        } else if applied.brake > 0.05 && frame.speed <= 1. {
            -applied.brake
        } else {
            0.
        };
        let wrecked = frame.wreck.phase != atv_core::wreck::Phase::Riding;
        let input = Input {
            origin: frame.origin,
            velocity: frame.linear_velocity,
            drive,
            grounded: frame.grounded.iter().any(|g| *g),
            pitch: pitch(frame.orientation),
            preload: frame.preload_active || frame.preload_armed,
            wrecked,
            stunting,
            stunt_started: started,
            stunt_landed: landed,
        };
        let resets = frame.reset_count;
        let recovered = resets != self.resets;
        self.resets = resets;
        self.last = input;
        self.note_line();
        if self.fast {
            self.course.hurry();
        }
        if self.pass_from.is_some_and(|n| self.course.lesson >= n)
            && self.course.phase == Phase::Lessons
            && self.course.state == atv_core::training::State::Active
        {
            self.course.force_success();
        }
        let mut commands = self.course.tick(dt, &input);
        // `001e8d10`: the wreck recovery resolves the failed lesson at once.
        if recovered && commands.is_empty() {
            self.course.hurry();
            commands = self.course.tick(
                0.,
                &Input {
                    wrecked: false,
                    ..input
                },
            );
        }
        self.note_line();
        for command in commands {
            match command {
                Command::Place { .. } => {
                    Self::place(ride, command);
                    self.resets = ride.frame().reset_count;
                }
                Command::Reward => self.rewarded = reward(&self.catalog),
                Command::Finished => self.finished = true,
            }
        }
        self.course.phase == Phase::Done && self.finished
    }

    /// Records a newly shown line (diagnostics).
    fn note_line(&mut self) {
        if let Some((line, _)) = self.course.caption
            && self.last_line != Some(line.id)
        {
            self.last_line = Some(line.id);
            if self.lines.len() == 32 {
                self.lines.remove(0);
            }
            self.lines.push(line.id);
        }
    }

    /// The HUD panel.
    pub fn view(&self, origin: [f32; 3]) -> atv_ui::hud::training::View {
        let course = &self.course;
        atv_ui::hud::training::View {
            title: course.title(),
            caption: course.caption.map(|(line, _)| line.caption.to_string()),
            banner: course.banner.map(|(text, _)| text.to_string()),
            distance: course.goal_distance(origin),
            hint: (course.phase == Phase::Lessons
                && course.locked()
                && matches!(
                    course.state,
                    atv_core::training::State::Demonstration | atv_core::training::State::Active
                ))
            .then(|| "ACCEPT SKIPS".to_string()),
        }
    }

    /// Diagnostics for tools/test_engine_training.mjs.
    pub fn state(&self, origin: [f32; 3]) -> serde_json::Value {
        let c = &self.course;
        serde_json::json!({
            "origin": origin,
            "lastLine": self.last_line,
            "lines": self.lines,
            "drive": self.last.drive,
            "grounded": self.last.grounded,
            "pitch": self.last.pitch,
            "speed": (self.last.velocity.iter().map(|v| v * v).sum::<f32>()).sqrt(),
            "distance": c.goal_distance(origin),
            "phase": format!("{:?}", c.phase),
            "lesson": c.lesson,
            "name": c.current().name,
            "state": format!("{:?}", c.state),
            "locked": c.locked(),
            "caption": c.caption.map(|(l, _)| l.caption),
            "line": c.caption.map(|(l, _)| l.id),
            "banner": c.banner.map(|(t, _)| t),
            "passed": c.passed,
            "failed": c.failed,
            "completedBefore": c.completed_before,
            "rewarded": self.rewarded,
            "finished": self.finished,
            "flags": c.diagnostics(),
        })
    }
}
