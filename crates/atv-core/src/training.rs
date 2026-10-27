//! The original Training Course (research/TRAINING.md).
//!
//! Exact-build evidence (private Ghidra exports of `SCUS_972.11`, its
//! `QuadzillaEventTraining.cpp` event and `AUDIO/TRAINING/DIALOGUE.AUD`;
//! bounded reading, no runtime traces):
//!
//! - Six lessons (`00226f30` ends the course after index 5), named by the
//!   table at `004085f0`: Gas and Brake, Reverse, Steering, Jump and Land,
//!   Stunt, Preload. Each lesson places the ATV at its start (`004084f0`)
//!   facing its direction (`00408540`).
//! - Four ground circles (`00408590`: centre and radius 18). A circle counts
//!   only while the ATV touches the ground (`00227140`). `00227250` names
//!   each lesson's start circle and `00227540` its goal, over which
//!   `00224860` hangs the `arrow.slt` marker 25 units up (Steering also
//!   marks its first circle).
//! - The per-frame update `00225980` runs four phases (introduction,
//!   lessons, ending, done). A lesson has four states: demonstration
//!   (the jumbotron `tracks\lesson%d.vcr` replay with the Jumbo lines),
//!   active (Chase lines while the ATV is held, then free riding with a
//!   TooLong line every 24 s), success and failure (`002273a0`: the
//!   "Lesson Success" / "Lesson Failed" banner, then one second after the
//!   line). Success advances to the next demonstration; failure retries
//!   the lesson at its start without it (`00226f30`). A wreck fails the
//!   lesson with a Wreck line (or the lesson's own crash line); the ATV's
//!   recovery in Training resolves it (`001e8d10`).
//! - Pad (`00225200`): Accept (button 4) skips the demonstration or the
//!   Chase lines after 0.5 s; button 7 during free riding replays the
//!   lesson from its demonstration.
//! - Completion (`00225980` ending): IntroPrize and FirstEnding play only
//!   while the profile does not own store ATV 8 (the blue Ravage Talon,
//!   profile +0x5c8 + 8); the ending always calls `0021f440`, which grants
//!   the four Talons and the ATV2 helmet, jersey, sleeves and pants
//!   ([`crate::store::training_reward`]). No other completion flag exists.
//!
//! The original speaks every instruction (46 VAG lines, no subtitles).
//! The port does not play them yet: each line is a port-authored caption
//! shown for a length-based time (see [`Line::seconds`]). The lesson
//! rules, circles, timers and order are the original's.

/// `00408590`: the four ground circles (x, y, z).
pub const CIRCLES: [[f32; 3]; 4] = [
    [89.0, 90.0, 433.48],
    [294.56, 90.0, 396.25],
    [332.73, 90.0, 597.4],
    [129.7, 90.0, 638.0],
];
/// `0040859c`: every circle's radius.
pub const CIRCLE_RADIUS: f32 = 18.0;
/// `00224860`: the marker hangs this far above its circle.
pub const MARKER_HEIGHT: f32 = 25.0;
/// `00225980` lessons 0/1: forward/reverse time allowed against the lesson.
pub const WRONG_WAY_SECONDS: f32 = 2.0;
/// `00225980` lesson 0: the stop speed inside the goal.
pub const STOP_SPEED: f32 = 1.0;
/// `00225980` lessons 3/5: a landing this far from the take-off counts as a
/// real jump.
pub const JUMP_DISTANCE: f32 = 40.0;
/// `00225980` lesson 3: the nose-down pitch (-5 degrees) in the air.
pub const PITCH: f32 = -0.087_266_45;
/// `00224860` +0x1fc: seconds between TooLong reminders.
pub const TOO_LONG_SECONDS: f32 = 24.0;
/// `002273a0` +0x1e0: the pause after a result line.
pub const RESULT_SECONDS: f32 = 1.0;
/// `002e2cf0` 2.0: the lesson name and result banners.
pub const BANNER_SECONDS: f32 = 2.0;
/// `00225200`: Accept skips only after this much of a line.
pub const SKIP_AFTER: f32 = 0.5;

/// One spoken line: its `DIALOGUE.AUD` number and the port's caption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub id: u16,
    pub caption: &'static str,
}

impl Line {
    /// The caption's display time standing in for the original line.
    pub fn seconds(&self) -> f32 {
        let words = self.caption.split_whitespace().count() as f32;
        (1.2 + words * 0.32).clamp(2.5, 9.0)
    }
}

const fn line(id: u16, caption: &'static str) -> Line {
    Line { id, caption }
}

/// One lesson of `004085f0` with its start, circles and lines.
#[derive(Debug)]
pub struct Lesson {
    pub name: &'static str,
    /// `004084f0`.
    pub start: [f32; 3],
    /// `00408540`.
    pub facing: [f32; 3],
    /// `00227250`.
    pub start_circle: usize,
    /// `00227540`.
    pub goal_circle: usize,
    pub jumbo: &'static [Line],
    pub chase: &'static [Line],
    /// The lesson's own failure lines (`Special%d`).
    pub special: &'static [Line],
}

impl Lesson {
    /// The start yaw, as rides face (atan2 of x over z).
    pub fn yaw(&self) -> f32 {
        use crate::math::Det;
        self.facing[0].atan2_d(self.facing[2])
    }
}

pub const LESSONS: [Lesson; 6] = [
    Lesson {
        name: "Gas and Brake",
        start: [130.0, 90.75, 646.54],
        facing: [-0.203, 0.0, -0.979],
        start_circle: 3,
        goal_circle: 0,
        jumbo: &[line(
            3,
            "Lesson 1, Gas and Brake: give it gas and ride to the far marker, then brake to a stop inside it.",
        )],
        chase: &[line(4, "Ride to the marker and stop inside it.")],
        special: &[line(46, "Don't back up. Drive forward to the marker.")],
    },
    Lesson {
        name: "Reverse",
        start: [87.74, 90.75, 433.21],
        facing: [-0.203, 0.0, -0.979],
        start_circle: 0,
        goal_circle: 3,
        jumbo: &[line(
            7,
            "Lesson 2, Reverse: hold the brake from a stop to back up, and reverse into the marker behind you.",
        )],
        chase: &[line(8, "Back up into the marker behind you.")],
        special: &[line(45, "Keep reversing. Don't drive forward.")],
    },
    Lesson {
        name: "Steering",
        start: [130.03, 90.75, 646.63],
        facing: [-0.203, 0.0, -0.979],
        start_circle: 3,
        goal_circle: 1,
        jumbo: &[line(
            11,
            "Lesson 3, Steering: ride through the first marker, then steer to the second one.",
        )],
        chase: &[line(12, "Through the first marker, then on to the second.")],
        special: &[line(15, "You have to ride through the first marker first.")],
    },
    Lesson {
        name: "Jump and Land",
        start: [292.58, 90.75, 388.82],
        facing: [0.171, 0.0, 0.985],
        start_circle: 1,
        goal_circle: 2,
        jumbo: &[line(
            16,
            "Lesson 4, Jump and Land: hit the jump and lean forward in the air so the nose points down for the landing, then ride to the marker.",
        )],
        chase: &[line(
            17,
            "Jump, lean forward to pitch the nose down, and land.",
        )],
        special: &[
            line(20, "Lean forward in the air to pitch the ATV's nose down."),
            line(21, "You crashed. Pitch the nose down to land cleanly."),
        ],
    },
    Lesson {
        name: "Stunt",
        start: [292.58, 90.75, 388.82],
        facing: [0.171, 0.0, 0.985],
        start_circle: 1,
        goal_circle: 2,
        jumbo: &[line(
            22,
            "Lesson 5, Stunt: catch air off the jump, perform a stunt and land it, then ride to the marker.",
        )],
        chase: &[line(23, "Jump, do a stunt and land it.")],
        special: &[
            line(
                26,
                "You didn't land a stunt. Do one in the air off the jump.",
            ),
            line(
                27,
                "You wrecked during the stunt. Finish it before you land.",
            ),
        ],
    },
    Lesson {
        name: "Preload",
        start: [340.51, 90.75, 596.91],
        facing: [-0.983, 0.0, 0.182],
        start_circle: 2,
        goal_circle: 3,
        jumbo: &[
            line(
                28,
                "Lesson 6, Preload: hold the preload button as you approach a jump and release it at the lip to launch higher.",
            ),
            line(29, "A preloaded jump goes farther than a plain one."),
        ],
        chase: &[
            line(30, "Preload into the jump and ride to the marker."),
            line(31, "Hold preload, then release it at the lip."),
        ],
        special: &[
            line(
                34,
                "You didn't preload that jump. Hold preload, then release.",
            ),
            line(35, "Good preload, but you wrecked. Try it again."),
        ],
    },
];

pub const INTRO: Line = line(
    1,
    "Welcome to the Training Course. Six lessons teach you the basics of riding.",
);
pub const INTRO_PRIZE: Line = line(
    2,
    "Finish every lesson to unlock the Ravage Talon ATVs and the ATV2 riding gear.",
);
pub const FIRST_ENDING: Line = line(
    36,
    "You completed the Training Course! The Ravage Talons and ATV2 gear are yours.",
);
pub const REST_ENDING: Line = line(37, "You completed the Training Course again. Nice riding!");
/// `[Success]`: a random one after each lesson but the last.
pub const SUCCESS: [Line; 5] = [
    line(5, "Nice work!"),
    line(9, "Well done!"),
    line(13, "That's how it's done!"),
    line(18, "Great job!"),
    line(24, "Excellent!"),
];
pub const FINAL_SUCCESS: Line = line(32, "That's the last lesson. Well done!");
/// `[TooLong]`: a random reminder every 24 s of free riding.
pub const TOO_LONG: [Line; 6] = [
    line(6, "Head for the marker."),
    line(10, "Follow the arrow to the marker."),
    line(14, "The arrow shows where to go."),
    line(19, "Ride to the marker under the arrow."),
    line(25, "Come on, the marker is waiting."),
    line(33, "Find the arrow and ride to it."),
];
/// `[Wreck]`: a random line after a wreck.
pub const WRECK: [Line; 7] = [
    line(38, "Ouch! Let's try that again."),
    line(39, "That had to hurt. Try again."),
    line(40, "Wipeout! Back to the start."),
    line(41, "Stay on the ATV this time."),
    line(42, "Shake it off and try again."),
    line(43, "You wrecked. Try again."),
    line(44, "Careful! Once more."),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Introduction,
    Lessons,
    Ending,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Demonstration,
    Active,
    Success,
    Failed,
}

/// What the ride shows this frame (sampled by the host).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// Forward (+) and reverse (-) drive command, -1..1 (+0x9e4).
    pub drive: f32,
    /// A wheel touches the ground (+0x198 +0x34 and +0x904).
    pub grounded: bool,
    /// Body pitch, radians, nose down negative (+0x150 +0xa0).
    pub pitch: f32,
    /// Preload is held (+0x194 +0x90).
    pub preload: bool,
    /// The rider is wrecked (+0x7f0, +0x884 +0x40c).
    pub wrecked: bool,
    /// A stunt is in progress (+0x30 +8 & 0x20).
    pub stunting: bool,
    /// A stunt started this frame (vtable +0x184 with 0: +0x230).
    pub stunt_started: bool,
    /// A stunt was landed this frame (vtable +0x184 with 1: +0x22c).
    pub stunt_landed: bool,
}

/// What the host must do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Place the ATV at a lesson start (`001e35d0`).
    Place { origin: [f32; 3], yaw: f32 },
    /// Grant the Training reward (`0021f440`).
    Reward,
    /// The course is over; return to the menus.
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Speech {
    Queue,
    Playing,
    Done,
}

/// The Training event (`QuadzillaEventTraining`).
#[derive(Clone, Debug)]
pub struct Course {
    pub phase: Phase,
    /// +0x1d4.
    pub lesson: usize,
    /// +0x1d0.
    pub state: State,
    /// Owned the blue Talon when the course started (IntroPrize).
    pub completed_before: bool,
    /// The current caption and its time left.
    pub caption: Option<(Line, f32)>,
    /// The banner ("Gas and Brake", "Lesson Success") and its time left.
    pub banner: Option<(&'static str, f32)>,
    /// Lessons passed and failed this visit (diagnostics).
    pub passed: u32,
    pub failed: u32,
    queue: Vec<Line>,
    speech: Speech,
    /// +0x1f4: the demonstration (0 waiting, 1 playing, 2 watched).
    demo: u8,
    /// Seconds into the current lesson state (+0x208).
    elapsed: f32,
    too_long: f32,
    wrong_way: f32,
    entered: bool,
    gate: bool,
    achieved: bool,
    settled: bool,
    tracking: bool,
    takeoff: [f32; 3],
    stunt_landed: bool,
    stunt_started: bool,
    result_wait: f32,
    rng: u32,
    wrecked_failure: bool,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl Course {
    pub fn new(completed_before: bool) -> Self {
        Self {
            phase: Phase::Introduction,
            lesson: 0,
            state: State::Demonstration,
            completed_before,
            caption: None,
            banner: None,
            passed: 0,
            failed: 0,
            queue: Vec::new(),
            speech: Speech::Queue,
            demo: 0,
            elapsed: 0.,
            too_long: TOO_LONG_SECONDS,
            wrong_way: 0.,
            entered: false,
            gate: false,
            achieved: false,
            settled: false,
            tracking: true,
            takeoff: [0.; 3],
            stunt_landed: false,
            stunt_started: false,
            result_wait: 0.,
            rng: 0x2545_f491,
            wrecked_failure: false,
        }
    }

    /// Where the first lesson starts (`00224a80` places it on entry).
    pub fn start(&self) -> Command {
        self.place()
    }

    pub fn current(&self) -> &'static Lesson {
        &LESSONS[self.lesson.min(LESSONS.len() - 1)]
    }

    fn place(&self) -> Command {
        let l = self.current();
        Command::Place {
            origin: l.start,
            yaw: l.yaw(),
        }
    }

    /// The ATV is held still (+0x94c / +0x948): introduction, demonstration,
    /// Chase lines, results and the ending.
    pub fn locked(&self) -> bool {
        !(self.phase == Phase::Lessons
            && self.state == State::Active
            && self.speech == Speech::Done)
    }

    /// Where the arrow marker hangs: the lesson's goal (and, in Steering,
    /// its first circle until it is passed).
    pub fn markers(&self) -> Vec<[f32; 3]> {
        if self.phase != Phase::Lessons {
            return Vec::new();
        }
        let up = |c: usize| {
            let [x, y, z] = CIRCLES[c];
            [x, y + MARKER_HEIGHT, z]
        };
        let mut out = vec![up(self.current().goal_circle)];
        if self.lesson == 2 && !self.gate {
            out.push(up(0));
        }
        out
    }

    /// Random line, as `0036e7a0` picks from a list.
    fn pick(&mut self, lines: &[Line]) -> Line {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        lines[(self.rng >> 16) as usize % lines.len()]
    }

    fn say(&mut self, lines: &[Line]) {
        self.queue.extend_from_slice(lines);
        self.speech = Speech::Playing;
        self.next_line();
    }

    fn next_line(&mut self) {
        self.caption = if self.queue.is_empty() {
            None
        } else {
            let line = self.queue.remove(0);
            Some((line, line.seconds()))
        };
    }

    /// Stops the spoken lines (`00150bd0`).
    fn hush(&mut self) {
        self.queue.clear();
        self.caption = None;
    }

    fn speak(&mut self, dt: f32) {
        if let Some((_, left)) = &mut self.caption {
            *left -= dt;
            if *left <= 0. {
                self.next_line();
            }
        }
        if self.speech == Speech::Playing && self.caption.is_none() {
            self.speech = Speech::Done;
        }
    }

    /// `00225200` Accept: skip the demonstration or the Chase lines.
    pub fn skip(&mut self) -> bool {
        if self.phase == Phase::Lessons
            && matches!(self.state, State::Demonstration | State::Active)
            && (self.speech == Speech::Playing || self.demo == 1)
            && self.elapsed >= SKIP_AFTER
        {
            self.hush();
            self.demo = 2;
            self.elapsed = 0.;
            return true;
        }
        false
    }

    /// `00225200` button 7: replay the current lesson from its
    /// demonstration while riding it.
    pub fn replay(&mut self) -> Vec<Command> {
        if self.phase == Phase::Lessons
            && self.state == State::Active
            && self.speech == Speech::Done
        {
            // `lesson - 1` then the success path's `+ 1`: the same lesson.
            self.state = State::Demonstration;
            self.demo = 0;
            return self.restart();
        }
        Vec::new()
    }

    /// `00224860`: the lesson's flags start over.
    fn reset_lesson(&mut self) {
        self.gate = false;
        self.result_wait = 0.;
        self.wrong_way = 0.;
        self.tracking = true;
        self.achieved = false;
        self.stunt_landed = false;
        self.stunt_started = false;
        self.settled = false;
        self.too_long = TOO_LONG_SECONDS;
        self.takeoff = [0.; 3];
        self.entered = false;
        self.wrecked_failure = false;
    }

    /// `002273a0`.
    fn finish(&mut self, success: bool) {
        self.state = if success {
            State::Success
        } else {
            State::Failed
        };
        self.banner = Some((
            if success {
                "Lesson Success"
            } else {
                "Lesson Failed"
            },
            BANNER_SECONDS,
        ));
        if success {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        self.speech = Speech::Queue;
        self.result_wait = RESULT_SECONDS;
    }

    fn fail(&mut self, line: Line) {
        self.finish(false);
        self.hush();
        self.say(&[line]);
    }

    /// `00226f30`: after a result, the next lesson (or the ending) or the
    /// same lesson again.
    fn resolve(&mut self) -> Vec<Command> {
        if self.phase != Phase::Lessons {
            return Vec::new();
        }
        match self.state {
            State::Success => {
                self.lesson += 1;
                if self.lesson == LESSONS.len() {
                    self.lesson = LESSONS.len() - 1;
                    self.phase = Phase::Ending;
                    self.speech = Speech::Queue;
                    return Vec::new();
                }
                self.state = State::Demonstration;
                self.demo = 0;
            }
            State::Failed => self.state = State::Active,
            _ => return Vec::new(),
        }
        self.restart()
    }

    fn restart(&mut self) -> Vec<Command> {
        self.speech = Speech::Queue;
        self.hush();
        self.elapsed = 0.;
        self.reset_lesson();
        vec![self.place()]
    }

    fn circle(&self, input: &Input, c: usize) -> bool {
        input.grounded && distance(input.origin, CIRCLES[c]) <= CIRCLE_RADIUS
    }

    /// `00225980` lessons 3 and 5: a jump from take-off to landing.
    fn jump(&mut self, input: &Input, preload: bool) {
        if !self.achieved && self.settled && !self.circle(input, self.current().start_circle) {
            if !self.tracking {
                if !input.grounded {
                    if !preload && input.pitch <= PITCH {
                        self.achieved = true;
                    }
                } else if distance(input.origin, self.takeoff) >= JUMP_DISTANCE {
                    if !self.achieved {
                        let line = self.current().special[0];
                        self.fail(line);
                        return;
                    }
                } else {
                    self.tracking = true;
                }
            } else if !input.grounded {
                self.tracking = false;
                self.takeoff = input.origin;
            }
            if preload && input.preload {
                self.achieved = true;
            }
        }
    }

    /// One frame of `00225980`.
    pub fn tick(&mut self, dt: f32, input: &Input) -> Vec<Command> {
        let mut out = Vec::new();
        if let Some((_, left)) = &mut self.banner {
            *left -= dt;
            if *left <= 0. {
                self.banner = None;
            }
        }
        self.speak(dt);
        match self.phase {
            Phase::Done => {}
            Phase::Introduction => {
                if self.speech == Speech::Queue {
                    let mut lines = vec![INTRO];
                    if !self.completed_before {
                        lines.push(INTRO_PRIZE);
                    }
                    self.say(&lines);
                } else if self.speech == Speech::Done {
                    self.phase = Phase::Lessons;
                    self.state = State::Demonstration;
                    self.speech = Speech::Queue;
                    self.demo = 0;
                    self.elapsed = 0.;
                }
            }
            Phase::Ending => {
                if self.speech == Speech::Queue {
                    let line = if self.completed_before {
                        REST_ENDING
                    } else {
                        FIRST_ENDING
                    };
                    self.say(&[line]);
                    out.push(Command::Reward);
                } else if self.speech == Speech::Done {
                    self.phase = Phase::Done;
                    out.push(Command::Finished);
                }
            }
            Phase::Lessons => self.lessons(dt, input, &mut out),
        }
        out
    }

    fn lessons(&mut self, dt: f32, input: &Input, out: &mut Vec<Command>) {
        match self.state {
            State::Demonstration => {
                self.elapsed += dt;
                if self.speech == Speech::Queue && self.demo == 0 {
                    // The jumbotron replay plays with the Jumbo lines; the
                    // port has no replay, so it ends with them.
                    self.demo = 1;
                    self.elapsed = 0.;
                    let lines = self.current().jumbo;
                    self.say(lines);
                }
                if self.demo == 1 && self.speech == Speech::Done {
                    self.demo = 2;
                }
                if self.demo == 2 && self.speech != Speech::Queue {
                    self.hush();
                    self.banner = Some((self.current().name, BANNER_SECONDS));
                    self.state = State::Active;
                    self.speech = Speech::Queue;
                    self.elapsed = 0.;
                }
            }
            State::Success | State::Failed => {
                if self.speech == Speech::Queue {
                    if self.state == State::Success {
                        let line = if self.lesson == LESSONS.len() - 1 {
                            FINAL_SUCCESS
                        } else {
                            self.pick(&SUCCESS)
                        };
                        self.say(&[line]);
                    } else {
                        self.speech = Speech::Done;
                    }
                }
                if self.speech == Speech::Done {
                    self.result_wait -= dt;
                    if self.result_wait <= 0. && !input.wrecked {
                        out.extend(self.resolve());
                    }
                }
            }
            State::Active => self.active(dt, input),
        }
    }

    fn active(&mut self, dt: f32, input: &Input) {
        self.elapsed += dt;
        if self.speech == Speech::Done && self.too_long > 0. {
            self.too_long -= dt;
            if self.too_long <= 0. {
                let line = self.pick(&TOO_LONG);
                self.say(&[line]);
                self.too_long = TOO_LONG_SECONDS;
            }
        }
        if !self.settled && input.grounded {
            self.settled = true;
        }
        if self.speech == Speech::Queue {
            self.elapsed = 0.;
            let lines = self.current().chase;
            self.say(lines);
            return;
        }
        if input.stunt_started {
            self.stunt_started = true;
        }
        if input.stunt_landed {
            self.stunt_landed = true;
        }
        let lesson = self.current();
        let speed = distance(input.velocity, [0.; 3]);
        match self.lesson {
            0 => {
                if !self.entered && self.circle(input, 3) {
                    self.entered = true;
                }
                if input.drive <= -0.5 {
                    self.wrong_way += dt;
                    if self.wrong_way >= WRONG_WAY_SECONDS {
                        return self.fail(lesson.special[0]);
                    }
                }
                if speed < STOP_SPEED && self.entered && self.circle(input, 0) {
                    return self.finish(true);
                }
            }
            1 => {
                if input.drive >= 0.5 {
                    self.wrong_way += dt;
                    if self.wrong_way >= WRONG_WAY_SECONDS {
                        return self.fail(lesson.special[0]);
                    }
                }
                if self.circle(input, 3) {
                    return self.finish(true);
                }
            }
            2 => {
                if !self.entered && self.circle(input, 3) {
                    self.entered = true;
                }
                if !self.gate {
                    if self.circle(input, 0) {
                        self.gate = true;
                    }
                    if self.circle(input, 1) && self.entered {
                        return self.fail(lesson.special[0]);
                    }
                } else if self.circle(input, 1) {
                    return self.finish(true);
                }
            }
            3 => {
                if input.wrecked {
                    return self.fail(lesson.special[1]);
                }
                self.jump(input, false);
                if self.state != State::Active {
                    return;
                }
                if self.circle(input, 2) && self.achieved {
                    return self.finish(true);
                }
            }
            4 => {
                if !self.entered && self.circle(input, 1) {
                    self.entered = true;
                }
                if !input.stunting && self.circle(input, 2) && self.entered {
                    if self.stunt_landed {
                        return self.finish(true);
                    }
                    return self.fail(lesson.special[0]);
                }
            }
            _ => {
                if !self.entered && self.circle(input, 2) {
                    self.entered = true;
                }
                self.jump(input, true);
                if self.state != State::Active {
                    return;
                }
                if self.circle(input, 3) && self.entered {
                    if self.achieved {
                        return self.finish(true);
                    }
                    return self.fail(lesson.special[0]);
                }
            }
        }
        // The wreck check after the lesson's own (`00225980` tail).
        if input.wrecked {
            self.wrecked_failure = true;
            let line = match self.lesson {
                4 if self.stunt_started => lesson.special[1],
                5 if self.achieved => lesson.special[1],
                _ => self.pick(&WRECK),
            };
            self.fail(line);
        }
    }

    /// The lesson line the HUD shows ("Lesson 3 of 6: Steering").
    pub fn title(&self) -> String {
        match self.phase {
            Phase::Introduction => "Training Course".into(),
            Phase::Ending | Phase::Done => "Training Course Complete".into(),
            Phase::Lessons => format!(
                "Lesson {} of {}: {}",
                self.lesson + 1,
                LESSONS.len(),
                self.current().name
            ),
        }
    }

    /// The distance to the lesson's goal circle (the arrow's target).
    pub fn goal_distance(&self, origin: [f32; 3]) -> Option<f32> {
        (self.phase == Phase::Lessons && self.state == State::Active)
            .then(|| distance(origin, CIRCLES[self.current().goal_circle]))
    }

    /// Test tooling: the active lesson succeeds now.
    pub fn force_success(&mut self) -> bool {
        if self.phase == Phase::Lessons {
            if self.state == State::Demonstration {
                self.hush();
                self.demo = 2;
                self.state = State::Active;
                self.speech = Speech::Done;
            }
            if self.state == State::Active {
                self.hush();
                self.finish(true);
                return true;
            }
        }
        false
    }

    /// Test tooling: every caption ends now.
    pub fn hurry(&mut self) {
        if self.speech == Speech::Playing {
            self.hush();
        }
        self.result_wait = 0.;
    }

    /// Lesson flags for diagnostics.
    pub fn diagnostics(&self) -> [bool; 6] {
        [
            self.entered,
            self.gate,
            self.achieved,
            self.settled,
            self.stunt_started,
            self.stunt_landed,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(c: usize) -> Input {
        Input {
            origin: [CIRCLES[c][0], CIRCLES[c][1] + 0.75, CIRCLES[c][2]],
            grounded: true,
            ..Default::default()
        }
    }

    fn run(course: &mut Course, input: &Input, seconds: f32) -> Vec<Command> {
        let mut out = Vec::new();
        let steps = (seconds * 60.).ceil() as usize;
        for _ in 0..steps {
            out.extend(course.tick(1. / 60., input));
        }
        out
    }

    /// Plays the introduction and the current lesson's lines.
    fn to_active(course: &mut Course) {
        let idle = at(CIRCLES.len() - 1);
        for _ in 0..4000 {
            if course.phase == Phase::Lessons && !course.locked() {
                return;
            }
            course.tick(
                1. / 60.,
                &Input {
                    origin: [0.; 3],
                    ..idle
                },
            );
        }
        panic!("never became active: {:?} {:?}", course.phase, course.state);
    }

    fn settle(course: &mut Course) {
        let mut input = at(course.current().start_circle);
        input.origin = course.current().start;
        course.tick(1. / 60., &input);
    }

    #[test]
    fn tables_match_the_executable() {
        assert_eq!(LESSONS.len(), 6);
        let names: Vec<_> = LESSONS.iter().map(|l| l.name).collect();
        assert_eq!(
            names,
            [
                "Gas and Brake",
                "Reverse",
                "Steering",
                "Jump and Land",
                "Stunt",
                "Preload"
            ]
        );
        // Each lesson starts inside its start circle.
        for l in &LESSONS {
            let d = distance(l.start, CIRCLES[l.start_circle]);
            assert!(d < CIRCLE_RADIUS, "{} {d}", l.name);
            assert!(distance(l.start, CIRCLES[l.goal_circle]) > CIRCLE_RADIUS);
        }
        // 46 dialogue lines, each used once.
        let mut ids: Vec<u16> = LESSONS
            .iter()
            .flat_map(|l| l.jumbo.iter().chain(l.chase).chain(l.special))
            .chain(&SUCCESS)
            .chain(&TOO_LONG)
            .chain(&WRECK)
            .chain(&[INTRO, INTRO_PRIZE, FIRST_ENDING, REST_ENDING, FINAL_SUCCESS])
            .map(|l| l.id)
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=46).collect::<Vec<_>>());
    }

    #[test]
    fn introduction_mentions_the_prize_until_completed() {
        let mut c = Course::new(false);
        c.tick(0.01, &Input::default());
        assert_eq!(c.caption.unwrap().0, INTRO);
        assert_eq!(c.queue, vec![INTRO_PRIZE]);
        assert!(c.locked());
        let mut c = Course::new(true);
        c.tick(0.01, &Input::default());
        assert!(c.queue.is_empty());
    }

    #[test]
    fn gas_and_brake_needs_a_stop_in_the_far_circle() {
        let mut c = Course::new(false);
        to_active(&mut c);
        assert_eq!(c.lesson, 0);
        // Moving through the goal does not count; a stop does.
        let mut start = at(3);
        start.drive = 1.;
        c.tick(1. / 60., &start);
        let mut moving = at(0);
        moving.velocity = [0., 0., -30.];
        c.tick(1. / 60., &moving);
        assert_eq!(c.state, State::Active);
        c.tick(1. / 60., &at(0));
        assert_eq!(c.state, State::Success);
        assert_eq!(c.banner.unwrap().0, "Lesson Success");
        // The success line, a second, then lesson 2's demonstration.
        let out = run(&mut c, &at(0), 12.);
        assert_eq!(c.lesson, 1);
        assert_eq!(
            out[0],
            Command::Place {
                origin: LESSONS[1].start,
                yaw: LESSONS[1].yaw()
            }
        );
        assert!(c.locked());
    }

    #[test]
    fn reversing_two_seconds_fails_and_retries_without_demonstration() {
        let mut c = Course::new(false);
        to_active(&mut c);
        let mut back = at(3);
        back.drive = -1.;
        run(&mut c, &back, 1.9);
        assert_eq!(c.state, State::Active);
        run(&mut c, &back, 0.2);
        assert_eq!(c.state, State::Failed);
        assert_eq!(c.caption.unwrap().0.id, 46);
        let out = run(&mut c, &at(3), 12.);
        assert!(matches!(out[0], Command::Place { .. }));
        assert_eq!((c.lesson, c.state), (0, State::Active));
        assert_eq!(c.failed, 1);
    }

    #[test]
    fn reverse_and_steering_order() {
        let mut c = Course::new(true);
        to_active(&mut c);
        c.force_success();
        run(&mut c, &at(0), 12.);
        to_active(&mut c);
        assert_eq!(c.lesson, 1);
        c.tick(1. / 60., &at(3));
        assert_eq!(c.state, State::Success);
        run(&mut c, &at(3), 12.);
        to_active(&mut c);
        assert_eq!(c.lesson, 2);
        assert_eq!(c.markers().len(), 2);
        // The second circle first fails.
        c.tick(1. / 60., &at(3));
        c.tick(1. / 60., &at(1));
        assert_eq!(c.state, State::Failed);
        assert_eq!(c.caption.unwrap().0.id, 15);
        run(&mut c, &at(3), 12.);
        c.tick(1. / 60., &at(3));
        c.tick(1. / 60., &at(0));
        assert_eq!(c.markers().len(), 1);
        c.tick(1. / 60., &at(1));
        assert_eq!(c.state, State::Success);
    }

    fn lesson(index: usize) -> Course {
        let mut c = Course::new(true);
        to_active(&mut c);
        while c.lesson < index {
            c.force_success();
            run(&mut c, &at(0), 12.);
            to_active(&mut c);
        }
        settle(&mut c);
        c
    }

    #[test]
    fn jump_and_land_needs_a_pitched_nose() {
        let mut c = lesson(3);
        let ground = |z: f32| Input {
            origin: [300., 91., z],
            grounded: true,
            ..Default::default()
        };
        let air = |z: f32, pitch: f32| Input {
            origin: [300., 100., z],
            pitch,
            ..Default::default()
        };
        // A short hop resets; a long flat jump fails.
        c.tick(0.016, &ground(440.));
        c.tick(0.016, &air(441., 0.));
        c.tick(0.016, &ground(450.));
        assert_eq!(c.state, State::Active);
        c.tick(0.016, &air(451., 0.));
        c.tick(0.016, &ground(500.));
        assert_eq!(c.state, State::Failed);
        assert_eq!(c.caption.unwrap().0.id, 20);
        run(&mut c, &ground(450.), 12.);
        // Pitched down, landed, then the goal.
        c.tick(0.016, &ground(440.));
        c.tick(0.016, &air(441., 0.));
        c.tick(0.016, &air(460., -0.2));
        c.tick(0.016, &ground(500.));
        assert_eq!(c.state, State::Active);
        c.tick(0.016, &at(2));
        assert_eq!(c.state, State::Success);
    }

    #[test]
    fn jump_and_land_crash_line() {
        let mut c = lesson(3);
        c.tick(
            0.016,
            &Input {
                wrecked: true,
                ..at(1)
            },
        );
        assert_eq!(c.caption.unwrap().0.id, 21);
        // The result waits for the recovery.
        run(
            &mut c,
            &Input {
                wrecked: true,
                ..at(1)
            },
            15.,
        );
        assert_eq!(c.state, State::Failed);
        run(&mut c, &at(1), 1.2);
        assert_eq!(c.state, State::Active);
    }

    #[test]
    fn stunt_needs_a_landed_stunt() {
        let mut c = lesson(4);
        c.tick(0.016, &at(1));
        c.tick(0.016, &at(2));
        assert_eq!(c.caption.unwrap().0.id, 26);
        run(&mut c, &at(1), 12.);
        c.tick(0.016, &at(1));
        c.tick(
            0.016,
            &Input {
                stunt_started: true,
                stunting: true,
                ..Default::default()
            },
        );
        c.tick(
            0.016,
            &Input {
                stunt_landed: true,
                ..at(1)
            },
        );
        // Still stunting inside the goal: not yet.
        c.tick(
            0.016,
            &Input {
                stunting: true,
                ..at(2)
            },
        );
        assert_eq!(c.state, State::Active);
        c.tick(0.016, &at(2));
        assert_eq!(c.state, State::Success);
    }

    #[test]
    fn stunt_wreck_line() {
        let mut c = lesson(4);
        c.tick(
            0.016,
            &Input {
                stunt_started: true,
                ..Default::default()
            },
        );
        c.tick(
            0.016,
            &Input {
                wrecked: true,
                ..Default::default()
            },
        );
        assert_eq!(c.caption.unwrap().0.id, 27);
    }

    #[test]
    fn preload_then_final_success_and_ending() {
        let mut c = lesson(5);
        // The goal without a preloaded jump fails.
        c.tick(0.016, &at(2));
        c.tick(0.016, &at(3));
        assert_eq!(c.caption.unwrap().0.id, 34);
        run(&mut c, &at(2), 12.);
        c.tick(0.016, &at(2));
        c.tick(
            0.016,
            &Input {
                origin: [250., 91., 600.],
                grounded: true,
                preload: true,
                ..Default::default()
            },
        );
        c.tick(0.016, &at(3));
        assert_eq!(c.state, State::Success);
        run(&mut c, &at(3), 0.1);
        assert_eq!(c.caption.unwrap().0, FINAL_SUCCESS);
        let mut out = Vec::new();
        while c.phase == Phase::Lessons {
            out.extend(c.tick(1. / 60., &at(3)));
        }
        out.extend(c.tick(1. / 60., &at(3)));
        assert_eq!(c.phase, Phase::Ending);
        assert!(out.contains(&Command::Reward));
        assert_eq!(c.caption.unwrap().0, REST_ENDING);
        let out = run(&mut c, &at(3), 12.);
        assert_eq!(out, vec![Command::Finished]);
        assert_eq!(c.phase, Phase::Done);
    }

    #[test]
    fn first_completion_ending() {
        let mut c = Course::new(false);
        to_active(&mut c);
        for _ in 0..6 {
            c.force_success();
            for _ in 0..2000 {
                if c.phase != Phase::Lessons || c.state == State::Demonstration {
                    break;
                }
                c.tick(1. / 60., &at(0));
            }
            if c.phase == Phase::Lessons {
                to_active(&mut c);
            }
        }
        c.tick(1. / 60., &at(0));
        assert_eq!(c.phase, Phase::Ending);
        assert_eq!(c.caption.unwrap().0, FIRST_ENDING);
        assert_eq!(c.passed, 6);
    }

    #[test]
    fn skip_replay_and_too_long() {
        let mut c = Course::new(true);
        run(&mut c, &Input::default(), 6.);
        assert_eq!(c.phase, Phase::Lessons);
        assert_eq!(c.state, State::Demonstration);
        run(&mut c, &Input::default(), 0.6);
        assert!(c.skip());
        c.tick(0.016, &Input::default());
        assert_eq!(c.state, State::Active);
        assert!(c.locked());
        run(&mut c, &Input::default(), 0.6);
        assert!(c.skip());
        c.tick(0.016, &Input::default());
        assert!(!c.locked());
        // A reminder after 24 s, with no failure.
        run(&mut c, &Input::default(), 24.1);
        assert!(TOO_LONG.contains(&c.caption.unwrap().0));
        assert_eq!(c.state, State::Active);
        run(&mut c, &Input::default(), 10.);
        // Replay: back to the same lesson's demonstration.
        let out = c.replay();
        assert!(matches!(out[0], Command::Place { .. }));
        assert_eq!((c.lesson, c.state), (0, State::Demonstration));
        assert_eq!(c.passed, 0);
    }
}
