//! The Waypoint Editor ride (research/RACING.md "User Created Enduro and the
//! Waypoint Editor"): the player rides the venue's long Enduro scene and
//! drops a gate at the quad's position and heading (G, the pad's R3 or the
//! toolbar's GATE), takes the last one back (U, L3 or UNDO), and saves the
//! course from Pause → Save Course into its Waypoint Editor slot.
//!
//! The original moves a cursor gate in a separate edit mode (Overlay Set 4's
//! help); the port drops gates where the rider stands so a phone can build
//! a course with one button. The venue loads [`MAX_GATES`] hidden
//! `endwgate` models (`Venue::editor_gates`); each placed gate poses the
//! next one.
use atv_core::enduro::{MODEL_LIFT, Waypoint};
use atv_core::user_course::{MAX_GATES, MIN_GATES, UserCourse};
use atv_game::ride::bindings::Pad;

/// R3 drops a gate, L3 takes the last one back (unbound by every preset).
const PAD_DROP: usize = 11;
const PAD_UNDO: usize = 10;
/// Seconds a placement notice stays.
const NOTICE: f64 = 2.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Drop,
    Undo,
}

pub struct Session {
    pub course: UserCourse,
    /// The Waypoint Editor slot (0-2) Save Course writes.
    pub slot: usize,
    /// The gate models shown, `Venue::editor_gates` onwards.
    shown: usize,
    /// Each gate's ground height, from the last sync.
    heights: Vec<f32>,
    /// The models need posing (a new ride, a placement or an undo).
    dirty: bool,
    notice: Option<(String, f64)>,
    pad: [bool; 2],
}

/// The fixed editor keys (when no ride binding uses them).
pub fn key_command(code: &str) -> Option<Command> {
    match code {
        "KeyG" => Some(Command::Drop),
        "KeyU" => Some(Command::Undo),
        _ => None,
    }
}

/// The quad's heading across the ground (its local +z), as `race.rs` reads
/// the yaw.
pub fn heading(orientation: [f32; 4]) -> [f32; 2] {
    let [w, x, y, z] = orientation;
    [2. * (w * y + x * z), 1. - 2. * (y * y + x * x)]
}

impl Session {
    pub fn new(course: UserCourse, slot: usize) -> Self {
        Self {
            course,
            slot,
            // The first sync hides every unused model.
            shown: MAX_GATES,
            heights: Vec::with_capacity(MAX_GATES),
            dirty: true,
            notice: None,
            pad: [false; 2],
        }
    }

    /// A new ride of the editor (or its reload): pose every model again.
    pub fn restart(&mut self) {
        self.dirty = true;
        self.shown = MAX_GATES;
    }

    /// R3 / L3 press edges.
    pub fn pad(&mut self, pad: Option<&Pad>) -> Option<Command> {
        let held = |b: usize| pad.is_some_and(|p| p.buttons.get(b).is_some_and(|v| *v > 0.5));
        let now = [held(PAD_DROP), held(PAD_UNDO)];
        let pressed = [now[0] && !self.pad[0], now[1] && !self.pad[1]];
        self.pad = now;
        match pressed {
            [true, _] => Some(Command::Drop),
            [_, true] => Some(Command::Undo),
            _ => None,
        }
    }

    /// Applies a command at the rider's position and heading; `now` in
    /// seconds for the notice.
    pub fn apply(&mut self, command: Command, origin: [f32; 3], orientation: [f32; 4], now: f64) {
        let text = match command {
            Command::Drop => {
                if self.course.gates.len() >= MAX_GATES {
                    format!("{MAX_GATES} gates is the most a course can hold")
                } else if self
                    .course
                    .place(origin[0], origin[2], heading(orientation))
                {
                    self.dirty = true;
                    format!("Gate {} placed", self.course.gates.len())
                } else {
                    "Ride further from the last gate".into()
                }
            }
            Command::Undo => match self.course.undo() {
                Some(_) => {
                    self.dirty = true;
                    format!("Gate {} removed", self.course.gates.len() + 1)
                }
                None => "No gates to remove".into(),
            },
        };
        self.notice = Some((text, now + NOTICE));
    }

    /// The posed gate models' changes since the last call: each shown
    /// model's index offset and world matrix (`None` hides it).
    pub fn sync(
        &mut self,
        ground: impl Fn(f32, f32) -> f32,
        mut model: impl FnMut(usize, Option<[f32; 16]>),
    ) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let count = self.course.gates.len();
        self.heights.clear();
        for (i, w) in self.course.gates.iter().enumerate() {
            let y = ground(w.x, w.z);
            self.heights.push(y);
            model(i, Some(matrix(w, y)));
        }
        for i in count..self.shown.max(count) {
            model(i, None);
        }
        self.shown = count;
    }

    /// The editor panel (the Training panel's layout).
    pub fn view(&self, now: f64) -> atv_ui::hud::training::View {
        let count = self.course.gates.len();
        atv_ui::hud::training::View {
            title: format!("Waypoint Editor   Gate {count} of {MAX_GATES}"),
            caption: Some(if count < MIN_GATES {
                format!(
                    "Drop at least {MIN_GATES} gates: GATE, G or R3. UNDO, U or L3 takes one back."
                )
            } else {
                "Pause → Save Course to keep it. The last gate is the finish.".into()
            }),
            banner: self
                .notice
                .as_ref()
                .filter(|(_, until)| now < *until)
                .map(|(text, _)| text.clone()),
            distance: None,
            hint: None,
        }
    }

    /// Minimap markers for every gate and screen tags for the start and
    /// the newest.
    pub fn markers(&self) -> Vec<atv_ui::hud::peers::Peer> {
        let last = self.course.gates.len().saturating_sub(1);
        self.course
            .gates
            .iter()
            .enumerate()
            .map(|(i, w)| atv_ui::hud::peers::Peer {
                slot: 3,
                name: format!("Gate {}", i + 1),
                position: [w.x, self.heights.get(i).copied().unwrap_or(0.), w.z],
                stale: false,
                pill: None,
                map_only: i != 0 && i != last,
            })
            .collect()
    }
}

/// `enduro::model_matrix` for a gate at ground height `y`.
fn matrix(w: &Waypoint, y: f32) -> [f32; 16] {
    let [dx, dz] = w.direction;
    [
        dz,
        0.,
        -dx,
        0.,
        0.,
        1.,
        0.,
        0.,
        dx,
        0.,
        dz,
        0.,
        w.x,
        y + MODEL_LIFT,
        w.z,
        1.,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_gates_on_the_heading_and_syncs_models() {
        let mut s = Session::new(UserCourse::new("env05"), 1);
        // Yaw 90°: +z turned to +x.
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let east = [half, 0., half, 0.];
        let [hx, hz] = heading(east);
        assert!((hx - 1.).abs() < 1e-6 && hz.abs() < 1e-6);
        s.apply(Command::Drop, [100., 5., 100.], east, 0.);
        s.apply(Command::Drop, [105., 5., 100.], east, 0.);
        assert_eq!(s.course.gates.len(), 1, "too close to the last gate");
        assert_eq!(
            s.view(1.).banner.as_deref(),
            Some("Ride further from the last gate")
        );
        assert_eq!(s.view(3.).banner, None);
        s.apply(Command::Drop, [150., 5., 100.], east, 0.);
        let mut posed = Vec::new();
        s.sync(|_, _| 10., |i, m| posed.push((i, m)));
        assert_eq!(posed.len(), MAX_GATES, "the unused models hide");
        assert_eq!(posed.iter().filter(|(_, m)| m.is_some()).count(), 2);
        let m = posed[1].1.unwrap();
        assert_eq!([m[12], m[13], m[14]], [150., 12., 100.]);
        assert!((m[8] - 1.).abs() < 1e-4, "forward along +x");
        posed.clear();
        s.sync(|_, _| 10., |i, m| posed.push((i, m)));
        assert!(posed.is_empty(), "nothing changed");
        s.apply(Command::Undo, [0.; 3], east, 0.);
        s.sync(|_, _| 10., |i, m| posed.push((i, m)));
        assert_eq!(posed.last(), Some(&(1, None)));
        assert_eq!(s.markers().len(), 1);
    }

    #[test]
    fn pad_presses_are_edges() {
        let mut s = Session::new(UserCourse::new("env05"), 0);
        let mut pad = Pad::default();
        assert_eq!(s.pad(Some(&pad)), None);
        pad.buttons[PAD_DROP] = 1.;
        assert_eq!(s.pad(Some(&pad)), Some(Command::Drop));
        assert_eq!(s.pad(Some(&pad)), None);
        pad.buttons[PAD_DROP] = 0.;
        pad.buttons[PAD_UNDO] = 1.;
        assert_eq!(s.pad(Some(&pad)), Some(Command::Undo));
        assert_eq!(key_command("KeyG"), Some(Command::Drop));
        assert_eq!(key_command("KeyW"), None);
    }
}
