//! Gameplay bindings (web/bindings.js `GameplayBindings` and gamepads.js
//! `ControllerSettings`): remappable keyboard and controller profiles with
//! chords, the Fury Remastered / ATV Offroad Fury 2 / MX vs. ATV Unleashed
//! controller presets, controller response tuning, queued keyboard edges and
//! the browser's saved-profile format (`atv2-gameplay-bindings-v1`).
use super::Controls;
pub use atv_core::browser_input::TouchIntent;
use atv_core::math::Det;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Throttle,
    Brake,
    SteerLeft,
    SteerRight,
    LeanForward,
    LeanBack,
    Reverse,
    StuntCircle,
    StuntTriangle,
    StuntL1,
    Clutch,
    PreloadHold,
    Camera,
    CameraLeft,
    CameraRight,
    Reset,
    Pause,
}

/// bindings.js `ACTIONS`: id and label in menu order.
pub const ACTIONS: [(Action, &str, &str); 17] = [
    (Action::Throttle, "throttle", "Gas"),
    (Action::Brake, "brake", "Brake"),
    (Action::SteerLeft, "steerLeft", "Steer left"),
    (Action::SteerRight, "steerRight", "Steer right"),
    (Action::LeanForward, "leanForward", "Lean forward"),
    (Action::LeanBack, "leanBack", "Lean back"),
    (Action::Reverse, "reverse", "Reverse"),
    (Action::StuntCircle, "stuntCircle", "Stunt Circle"),
    (Action::StuntTriangle, "stuntTriangle", "Stunt Triangle"),
    (Action::StuntL1, "stuntL1", "Stunt L1"),
    (Action::Clutch, "clutch", "Motorcycle clutch"),
    (Action::PreloadHold, "preloadHold", "Preload hold / release"),
    (Action::Camera, "camera", "Change camera"),
    (Action::CameraLeft, "cameraLeft", "Camera orbit left"),
    (Action::CameraRight, "cameraRight", "Camera orbit right"),
    (Action::Reset, "reset", "Reset ATV"),
    (Action::Pause, "pause", "Pause"),
];

fn index(action: Action) -> usize {
    ACTIONS.iter().position(|(a, ..)| *a == action).unwrap()
}

/// web/bindings.js action ids.
pub fn action_named(id: &str) -> Option<Action> {
    ACTIONS.iter().find(|(_, i, _)| *i == id).map(|(a, ..)| *a)
}

pub fn action_id(action: Action) -> &'static str {
    ACTIONS[index(action)].1
}

/// Keyboard rows read differently: brake also reverses, tricks combine.
pub fn keyboard_label(action: Action) -> &'static str {
    match action {
        Action::Brake => "Brake / reverse",
        Action::Reverse => "Reverse (extra key)",
        Action::StuntCircle => "Trick ○ (hold + direction)",
        Action::StuntTriangle => "Trick △ (hold + direction)",
        Action::StuntL1 => "Trick L1 (combine)",
        _ => ACTIONS[index(action)].2,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Atom {
    Key(String),
    Button(u8),
    /// Axis index and direction (-1 or 1).
    Axis(u8, i8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    One(Atom),
    /// 2-4 atoms held together.
    Chord(Vec<Atom>),
}

impl Binding {
    pub fn atoms(&self) -> &[Atom] {
        match self {
            Binding::One(a) => std::slice::from_ref(a),
            Binding::Chord(atoms) => atoms,
        }
    }
}

/// Bindings per action, in `ACTIONS` order.
pub type Rows = Vec<Vec<Binding>>;

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub keyboard: Rows,
    pub controller: Rows,
}

fn key(code: &str) -> Binding {
    Binding::One(Atom::Key(code.into()))
}
fn button(b: u8) -> Binding {
    Binding::One(Atom::Button(b))
}
fn axis(a: u8, s: i8) -> Binding {
    Binding::One(Atom::Axis(a, s))
}
fn chord(atoms: &[Binding]) -> Binding {
    Binding::Chord(atoms.iter().map(|b| b.atoms()[0].clone()).collect())
}

fn rows(entries: Vec<(Action, Vec<Binding>)>) -> Rows {
    let mut rows = vec![Vec::new(); ACTIONS.len()];
    for (action, bindings) in entries {
        rows[index(action)] = bindings;
    }
    rows
}

fn default_keyboard() -> Rows {
    use Action::*;
    rows(vec![
        (Throttle, vec![key("KeyW"), key("ArrowUp")]),
        (Brake, vec![key("KeyS"), key("ArrowDown")]),
        (SteerLeft, vec![key("KeyA"), key("ArrowLeft")]),
        (SteerRight, vec![key("KeyD"), key("ArrowRight")]),
        (LeanForward, vec![key("KeyQ")]),
        (LeanBack, vec![key("KeyE"), key("ShiftLeft")]),
        (StuntCircle, vec![key("KeyJ")]),
        (StuntTriangle, vec![key("KeyK")]),
        (StuntL1, vec![key("KeyL")]),
        (Clutch, vec![key("KeyV")]),
        (PreloadHold, vec![key("Space")]),
        (Camera, vec![key("KeyC")]),
        (CameraLeft, vec![key("KeyZ")]),
        (CameraRight, vec![key("KeyX")]),
        (Reset, vec![key("KeyR"), key("Backspace")]),
        (Pause, vec![key("Escape"), key("KeyP")]),
    ])
}

/// bindings.js version 1 keyboard defaults (migrated rows adopt the new ones).
fn v1_keyboard() -> Rows {
    use Action::*;
    rows(vec![
        (Throttle, vec![key("KeyW")]),
        (Brake, vec![key("KeyS")]),
        (SteerLeft, vec![key("KeyA"), key("ArrowLeft")]),
        (SteerRight, vec![key("KeyD"), key("ArrowRight")]),
        (LeanForward, vec![key("KeyQ"), key("ArrowUp")]),
        (LeanBack, vec![key("KeyE"), key("ArrowDown")]),
        (Reverse, vec![key("KeyR")]),
        (StuntCircle, vec![key("KeyJ")]),
        (StuntTriangle, vec![key("KeyK")]),
        (StuntL1, vec![key("KeyL")]),
        (Clutch, vec![key("KeyV")]),
        (PreloadHold, vec![key("Space")]),
        (Camera, vec![key("KeyC")]),
        (Reset, vec![key("Backspace")]),
        (Pause, vec![key("Escape")]),
    ])
}

fn pad_common() -> Vec<(Action, Vec<Binding>)> {
    use Action::*;
    vec![
        (SteerLeft, vec![axis(0, -1), button(14)]),
        (SteerRight, vec![axis(0, 1), button(15)]),
        (LeanForward, vec![axis(1, -1), button(12)]),
        (LeanBack, vec![axis(1, 1), button(13)]),
        (Throttle, vec![button(0)]),
        (Brake, vec![button(2)]),
        (Reverse, vec![chord(&[button(2), button(0)])]),
        (StuntCircle, vec![button(1)]),
        (StuntTriangle, vec![button(3)]),
        (Camera, vec![button(8)]),
        (CameraLeft, vec![axis(2, -1)]),
        (CameraRight, vec![axis(2, 1)]),
        (Pause, vec![button(9)]),
    ]
}

/// Controller presets: label and rows (bindings.js `CONTROLLER_PRESETS`).
pub fn controller_preset(id: &str) -> Option<(&'static str, Rows)> {
    use Action::*;
    Some(match id {
        "remastered" => (
            "Fury Remastered",
            rows(vec![
                (Throttle, vec![button(0), button(7)]),
                (Brake, vec![button(2), button(6)]),
                (SteerLeft, vec![axis(0, -1), button(14)]),
                (SteerRight, vec![axis(0, 1), button(15)]),
                (LeanForward, vec![axis(1, -1), button(12)]),
                (LeanBack, vec![axis(1, 1), button(13)]),
                (Reverse, vec![chord(&[button(0), button(2)])]),
                (StuntCircle, vec![button(1)]),
                (StuntTriangle, vec![button(3)]),
                (StuntL1, vec![button(4)]),
                (Clutch, vec![button(10)]),
                (Camera, vec![button(11)]),
                (CameraLeft, vec![axis(2, -1)]),
                (CameraRight, vec![axis(2, 1)]),
                (Reset, vec![chord(&[button(4), button(5)])]),
                (Pause, vec![button(9)]),
            ]),
        ),
        // Original: L1 is the third stunt modifier and L1 + R1 resets.
        "atv2" => {
            let mut entries = pad_common();
            entries.push((StuntL1, vec![button(4)]));
            entries.push((Clutch, vec![button(10)]));
            entries.push((Reset, vec![chord(&[button(4), button(5)])]));
            ("ATV Offroad Fury 2", rows(entries))
        }
        // Unleashed: R1 is the stunt modifier, L1 the clutch, no reset.
        "unleashed" => {
            let mut entries = pad_common();
            entries.push((StuntL1, vec![button(5)]));
            entries.push((Clutch, vec![button(4)]));
            ("MX vs. ATV Unleashed", rows(entries))
        }
        _ => return None,
    })
}
pub const CONTROLLER_PRESETS: [&str; 3] = ["remastered", "atv2", "unleashed"];

impl Default for Profile {
    fn default() -> Self {
        Self {
            keyboard: default_keyboard(),
            controller: controller_preset("remastered").unwrap().1,
        }
    }
}

// --- the browser's saved JSON ------------------------------------------

fn atom_json(atom: &Atom) -> Value {
    match atom {
        Atom::Key(k) => json!({"key": k}),
        Atom::Button(b) => json!({"button": b}),
        Atom::Axis(a, s) => json!({"axis": a, "sign": s}),
    }
}

fn binding_json(b: &Binding) -> Value {
    match b {
        Binding::One(a) => atom_json(a),
        Binding::Chord(atoms) => json!({"chord": atoms.iter().map(atom_json).collect::<Vec<_>>()}),
    }
}

fn rows_json(rows: &Rows) -> Value {
    Value::Object(
        ACTIONS
            .iter()
            .zip(rows)
            .map(|((_, id, _), r)| {
                (
                    id.to_string(),
                    Value::Array(r.iter().map(binding_json).collect()),
                )
            })
            .collect(),
    )
}

/// bindings.js `validAtom`.
fn parse_atom(keyboard: bool, v: &Value) -> Option<Atom> {
    let o = v.as_object()?;
    if keyboard {
        let k = o.get("key")?.as_str()?;
        let valid = o.len() == 1
            && k.len() <= 40
            && k.chars().next()?.is_ascii_alphabetic()
            && k.chars().all(|c| c.is_ascii_alphanumeric());
        return valid.then(|| Atom::Key(k.into()));
    }
    if o.len() == 1 {
        let b = o.get("button")?.as_u64()?;
        return (b < 64).then_some(Atom::Button(b as u8));
    }
    let a = o.get("axis")?.as_u64()?;
    let s = o.get("sign")?.as_i64()?;
    (o.len() == 2 && a < 16 && (s == 1 || s == -1)).then_some(Atom::Axis(a as u8, s as i8))
}

fn parse_binding(keyboard: bool, v: &Value) -> Option<Binding> {
    if let Some(atom) = parse_atom(keyboard, v) {
        return Some(Binding::One(atom));
    }
    let o = v.as_object()?;
    let chord = o.get("chord")?.as_array()?;
    let atoms: Vec<Atom> = chord
        .iter()
        .map(|a| parse_atom(keyboard, a))
        .collect::<Option<_>>()?;
    let unique = atoms.iter().collect::<std::collections::HashSet<_>>().len() == atoms.len();
    (o.len() == 1 && (2..=4).contains(&atoms.len()) && unique).then_some(Binding::Chord(atoms))
}

/// bindings.js `normalize`: at most four valid bindings, else invalid.
fn parse_row(keyboard: bool, v: &Value) -> Option<Vec<Binding>> {
    let values: Vec<&Value> = match v {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    if values.len() > 4 {
        return None;
    }
    values
        .into_iter()
        .map(|b| parse_binding(keyboard, b))
        .collect()
}

/// bindings.js `migrateKeyboard`: unchanged version-1 rows adopt the new
/// defaults, minus keys that a customized row now owns.
fn migrate_keyboard(saved: &Rows) -> Rows {
    let defaults = default_keyboard();
    let v1 = v1_keyboard();
    let mut adopted = Vec::new();
    let mut profile: Rows = saved.clone();
    for i in 0..ACTIONS.len() {
        if saved[i] == v1[i] {
            profile[i] = defaults[i].clone();
            adopted.push(i);
        }
    }
    let custom: std::collections::HashSet<Atom> = (0..ACTIONS.len())
        .filter(|i| !adopted.contains(i))
        .flat_map(|i| profile[i].iter().flat_map(|b| b.atoms().to_vec()))
        .collect();
    for i in adopted {
        profile[i].retain(|b| b.atoms().iter().all(|a| !custom.contains(a)));
    }
    profile
}

pub const STORAGE_KEY: &str = "atv2-gameplay-bindings-v1";
const PROFILE_VERSION: i64 = 2;

// --- controller tuning (gamepads.js ControllerSettings) -----------------

pub const CONTROLLER_STORAGE_KEY: &str = "atv2-controller-v1";
pub const TUNING_KEYS: [&str; 5] = ["steering", "lean", "triggers", "camera", "deadzone"];

/// Port enhancements; 100% keeps the unmodified response.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    pub steering: f32,
    pub lean: f32,
    pub triggers: f32,
    pub camera: f32,
    pub deadzone: f32,
    /// A pinned controller (`selected.id`, index), or Auto.
    pub selected: Option<(String, u32)>,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            steering: 100.,
            lean: 100.,
            triggers: 100.,
            camera: 100.,
            deadzone: 15.,
            selected: None,
        }
    }
}

impl Tuning {
    /// (min, max, step) of a tuning key.
    pub fn range(key: &str) -> (f32, f32, f32) {
        if key == "deadzone" {
            (0., 40., 5.)
        } else {
            (50., 150., 5.)
        }
    }
    pub fn get(&self, key: &str) -> f32 {
        match key {
            "steering" => self.steering,
            "lean" => self.lean,
            "triggers" => self.triggers,
            "camera" => self.camera,
            _ => self.deadzone,
        }
    }
    pub fn set(&mut self, key: &str, value: f32) {
        let (min, max, step) = Self::range(key);
        if !value.is_finite() {
            return;
        }
        let value = ((value / step).round() * step).clamp(min, max);
        match key {
            "steering" => self.steering = value,
            "lean" => self.lean = value,
            "triggers" => self.triggers = value,
            "camera" => self.camera = value,
            "deadzone" => self.deadzone = value,
            _ => {}
        }
    }
    /// Restores the response settings; the chosen controller is kept.
    pub fn reset(&mut self) {
        let selected = self.selected.take();
        *self = Self {
            selected,
            ..Self::default()
        };
    }
    pub fn from_json(text: &str) -> Self {
        let mut tuning = Self::default();
        let Ok(saved) = serde_json::from_str::<Value>(text) else {
            return tuning;
        };
        for key in TUNING_KEYS {
            if let Some(v) = saved[key].as_f64() {
                tuning.set(key, v as f32);
            }
        }
        if let Some(id) = saved["selected"]["id"]
            .as_str()
            .filter(|id| id.len() <= 256)
        {
            tuning.selected = Some((
                id.into(),
                saved["selected"]["index"].as_u64().unwrap_or(0) as u32,
            ));
        }
        tuning
    }
    pub fn to_json(&self) -> String {
        json!({
            "selected": self.selected.as_ref().map(|(id, index)| json!({"id": id, "index": index})),
            "steering": self.steering, "lean": self.lean, "triggers": self.triggers,
            "camera": self.camera, "deadzone": self.deadzone,
        })
        .to_string()
    }
    fn zone(&self) -> f32 {
        self.deadzone / 100.
    }
    /// Response curve: full travel still reaches 1.
    fn shape(&self, action: Action, value: f32) -> f32 {
        use Action::*;
        let sensitivity = match action {
            SteerLeft | SteerRight => self.steering,
            LeanForward | LeanBack => self.lean,
            Throttle | Brake => self.triggers,
            CameraLeft | CameraRight => self.camera,
            _ => 100.,
        };
        if value > 0. && value < 1. && sensitivity != 100. {
            value.powf_d(100. / sensitivity)
        } else {
            value
        }
    }
}

// --- reading ----------------------------------------------------------------

/// A standard-mapping controller sample (W3C Gamepad button/axis order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pad {
    /// Distinguishes controllers; a change is a cancellation.
    pub id: u64,
    pub buttons: [f32; 17],
    pub axes: [f32; 4],
}

fn atom_amount(atom: &Atom, keys: &dyn Fn(&str) -> bool, pad: Option<&Pad>, zone: f32) -> f32 {
    match atom {
        Atom::Key(code) => keys(code) as u8 as f32,
        Atom::Button(b) => pad.map_or(0., |p| {
            p.buttons
                .get(*b as usize)
                .copied()
                .unwrap_or(0.)
                .clamp(0., 1.)
        }),
        Atom::Axis(a, sign) => {
            let value = pad
                .map_or(0., |p| {
                    p.axes.get(*a as usize).copied().unwrap_or(0.) * *sign as f32
                })
                .clamp(0., 1.);
            if value <= zone {
                0.
            } else {
                (value - zone) / (1. - zone)
            }
        }
    }
}

fn amount(binding: &Binding, keys: &dyn Fn(&str) -> bool, pad: Option<&Pad>, zone: f32) -> f32 {
    match binding {
        Binding::One(a) => atom_amount(a, keys, pad, zone),
        Binding::Chord(atoms) => atoms
            .iter()
            .map(|a| atom_amount(a, keys, pad, zone))
            .fold(f32::INFINITY, f32::min),
    }
}

/// Stateful reader: edge actions, preload release and cancellation.
#[derive(Default)]
pub struct Bindings {
    pub profile: Profile,
    /// Edited controller bindings kept while a preset is chosen.
    pub custom_controller: Option<Rows>,
    pub tuning: Tuning,
    pub generation: u64,
    previous: Vec<Action>,
    blocked: Vec<Action>,
    preload_held: bool,
    controller: Option<Option<u64>>,
    keyboard_held: Vec<Action>,
    queued: Vec<Action>,
    /// Keyboard press order and time (seconds) for trick directions.
    pressed: std::collections::HashMap<String, (u64, f64)>,
    press_count: u64,
}

/// One read: driving controls, edge actions and the camera orbit axis.
#[derive(Clone, Debug, Default)]
pub struct Read {
    pub controls: Controls,
    pub camera: bool,
    pub reset: bool,
    pub pause: bool,
    /// cameraRight - cameraLeft.
    pub orbit: f32,
}

/// A held direction pressed before the trick key waits this long.
const TRICK_DIRECTION_GRACE: f64 = 0.150;

impl Bindings {
    /// The saved profile and tuning (bindings.js / gamepads.js constructors).
    /// Returns whether the profile was migrated (and should be saved again).
    pub fn load(saved: Option<&str>, tuning: Option<&str>) -> (Self, bool) {
        let mut bindings = Self::default();
        if let Some(text) = tuning {
            bindings.tuning = Tuning::from_json(text);
        }
        let mut migrated = false;
        if let Some(saved) = saved.and_then(|t| serde_json::from_str::<Value>(t).ok()) {
            let parse = |keyboard: bool, device: &Value, defaults: &Rows| -> Option<Rows> {
                ACTIONS
                    .iter()
                    .enumerate()
                    .map(|(i, (_, id, _))| match device.get(*id) {
                        Some(v) => parse_row(keyboard, v),
                        None => Some(defaults[i].clone()),
                    })
                    .collect()
            };
            let defaults = Profile::default();
            match (
                parse(true, &saved["keyboard"], &defaults.keyboard),
                parse(false, &saved["controller"], &defaults.controller),
            ) {
                (Some(keyboard), Some(controller)) => {
                    bindings.profile = Profile {
                        keyboard,
                        controller,
                    };
                    bindings.custom_controller = saved
                        .get("customController")
                        .and_then(|c| parse(false, c, &vec![Vec::new(); ACTIONS.len()]));
                    if saved["version"].as_i64() != Some(PROFILE_VERSION) {
                        bindings.profile.keyboard = migrate_keyboard(&bindings.profile.keyboard);
                        migrated = true;
                    }
                }
                _ => bindings.profile = defaults,
            }
        }
        (bindings, migrated)
    }

    /// The browser's saved form.
    pub fn to_json(&self) -> String {
        let mut value = json!({
            "keyboard": rows_json(&self.profile.keyboard),
            "controller": rows_json(&self.profile.controller),
            "version": PROFILE_VERSION,
        });
        if let Some(custom) = &self.custom_controller {
            value["customController"] = rows_json(custom);
        }
        value.to_string()
    }

    pub fn row(&self, keyboard: bool, action: Action) -> &[Binding] {
        let rows = if keyboard {
            &self.profile.keyboard
        } else {
            &self.profile.controller
        };
        &rows[index(action)]
    }

    pub fn set_binding(&mut self, keyboard: bool, action: Action, bindings: Vec<Binding>) {
        let rows = if keyboard {
            &mut self.profile.keyboard
        } else {
            &mut self.profile.controller
        };
        rows[index(action)] = bindings;
        self.cancel();
    }

    pub fn reset_device(&mut self, keyboard: bool) {
        if keyboard {
            self.profile.keyboard = default_keyboard();
            self.cancel();
        } else {
            self.apply_controller_preset("remastered");
        }
    }

    /// The preset matching the controller rows, or "custom".
    pub fn controller_preset(&self) -> &'static str {
        CONTROLLER_PRESETS
            .into_iter()
            .find(|id| controller_preset(id).unwrap().1 == self.profile.controller)
            .unwrap_or("custom")
    }

    pub fn controller_preset_label(&self) -> &'static str {
        match self.controller_preset() {
            "custom" => "Custom",
            id => controller_preset(id).unwrap().0,
        }
    }

    /// Choosing a preset keeps edited bindings as "Custom" to choose again.
    pub fn apply_controller_preset(&mut self, id: &str) {
        if id == "custom" && self.custom_controller.is_none() {
            return;
        }
        let next = if id == "custom" {
            self.custom_controller.clone().unwrap()
        } else {
            match controller_preset(id) {
                Some((_, rows)) => rows,
                None => return,
            }
        };
        if self.controller_preset() == "custom" {
            self.custom_controller = Some(self.profile.controller.clone());
        }
        self.profile.controller = next;
        self.cancel();
    }

    pub fn cycle_controller_preset(&mut self, direction: i32) {
        let mut options: Vec<&str> = CONTROLLER_PRESETS.to_vec();
        if self.custom_controller.is_some() || self.controller_preset() == "custom" {
            options.push("custom");
        }
        let at = options
            .iter()
            .position(|o| *o == self.controller_preset())
            .unwrap_or(0) as i32;
        let n = options.len() as i32;
        let next = options[((at + direction.signum() + n) % n) as usize];
        self.apply_controller_preset(next);
    }

    pub fn uses_key(&self, code: &str) -> bool {
        self.profile.keyboard.iter().flatten().any(|b| {
            b.atoms()
                .iter()
                .any(|a| matches!(a, Atom::Key(k) if k == code))
        })
    }

    /// Keyboard key groups of an action (keyboard help: chords as groups).
    pub fn keyboard_codes(&self, action: Action) -> Vec<Vec<String>> {
        self.row(true, action)
            .iter()
            .map(|b| {
                b.atoms()
                    .iter()
                    .filter_map(|a| match a {
                        Atom::Key(k) => Some(k.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .collect()
    }

    /// Blur, pause, reset and controller changes cancel held edge controls.
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.previous.clear();
        self.keyboard_held.clear();
        self.queued.clear();
        self.preload_held = false;
        self.blocked = vec![
            Action::Camera,
            Action::Reset,
            Action::Pause,
            Action::PreloadHold,
        ];
    }

    fn keyboard_value(&self, action: Action, keys: &dyn Fn(&str) -> bool) -> f32 {
        self.profile.keyboard[index(action)]
            .iter()
            .map(|b| amount(b, keys, None, 0.15))
            .fold(0., f32::max)
    }

    fn controller_value(&self, action: Action, pad: Option<&Pad>) -> f32 {
        let none = |_: &str| false;
        let zone = self.tuning.zone();
        self.tuning.shape(
            action,
            self.profile.controller[index(action)]
                .iter()
                .map(|b| amount(b, &none, pad, zone))
                .fold(0., f32::max),
        )
    }

    /// One action's value from the keyboard or the (tuned) controller.
    pub fn value(&self, action: Action, keys: &dyn Fn(&str) -> bool, pad: Option<&Pad>) -> f32 {
        self.keyboard_value(action, keys)
            .max(self.controller_value(action, pad))
    }

    fn binds_key(&self, action: Action, code: &str) -> bool {
        self.profile.keyboard[index(action)].iter().any(|b| {
            b.atoms()
                .iter()
                .any(|a| matches!(a, Atom::Key(k) if k == code))
        })
    }

    /// bindings.js `observeKeys` on keydown: trick timing, and Camera/Reset/
    /// Pause edges queued so a tap between two physics batches still counts.
    pub fn key_pressed(&mut self, code: &str, now: f64, keys: &dyn Fn(&str) -> bool) {
        self.press_count += 1;
        self.pressed.insert(code.into(), (self.press_count, now));
        self.observe(keys, Some(code));
    }

    /// bindings.js `observeKeys` on keyup.
    pub fn key_released(&mut self, keys: &dyn Fn(&str) -> bool) {
        self.observe(keys, None);
    }

    fn observe(&mut self, keys: &dyn Fn(&str) -> bool, pressed: Option<&str>) {
        for action in [Action::Camera, Action::Reset, Action::Pause] {
            let held = self.keyboard_value(action, keys) > 0.5;
            let fresh = pressed.is_some_and(|code| self.binds_key(action, code));
            if fresh
                && held
                && !self.keyboard_held.contains(&action)
                && !self.queued.contains(&action)
            {
                self.queued.push(action);
            }
            if held {
                if !self.keyboard_held.contains(&action) {
                    self.keyboard_held.push(action);
                }
            } else {
                self.keyboard_held.retain(|a| *a != action);
            }
        }
        if let Some(code) = pressed
            && self.binds_key(Action::PreloadHold, code)
            && self.keyboard_value(Action::PreloadHold, keys) > 0.5
        {
            self.blocked.retain(|a| *a != Action::PreloadHold);
        }
    }

    fn keyboard_trick_axes(&self, keys: &dyn Fn(&str) -> bool, now: f64) -> Option<[f32; 2]> {
        let held = |action: Action| -> Vec<(u64, f64)> {
            self.profile.keyboard[index(action)]
                .iter()
                .flat_map(|b| b.atoms().to_vec())
                .filter_map(|a| match a {
                    Atom::Key(name) if keys(&name) => {
                        Some(*self.pressed.get(&name).unwrap_or(&(0, f64::NEG_INFINITY)))
                    }
                    _ => None,
                })
                .collect()
        };
        let modifiers: Vec<_> = [Action::StuntCircle, Action::StuntTriangle, Action::StuntL1]
            .into_iter()
            .flat_map(held)
            .collect();
        if modifiers.is_empty() {
            return None;
        }
        let mut best: Option<((u64, f64), [f32; 2])> = None;
        for (action, axes) in [
            (Action::LeanForward, [0., -1.]),
            (Action::Throttle, [0., -1.]),
            (Action::LeanBack, [0., 1.]),
            (Action::Brake, [0., 1.]),
            (Action::Reverse, [0., 1.]),
            (Action::SteerLeft, [-1., 0.]),
            (Action::SteerRight, [1., 0.]),
        ] {
            for press in held(action) {
                if best.is_none_or(|(b, _)| press.0 > b.0) {
                    best = Some((press, axes));
                }
            }
        }
        let (press, axes) = best?;
        let modifier_at = modifiers
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        Some(
            if press.1 < modifier_at && now - modifier_at < TRICK_DIRECTION_GRACE {
                [0., 0.]
            } else {
                axes
            },
        )
    }

    pub fn read(&mut self, keys: &dyn Fn(&str) -> bool, pad: Option<&Pad>, now: f64) -> Read {
        let identity = pad.map(|p| p.id);
        // Losing or switching a controller is cancellation, never a flick.
        if self.controller.is_some_and(|c| c != identity) {
            self.cancel();
        }
        self.controller = Some(identity);
        let none = |_: &str| false;
        let zone = self.tuning.zone();
        let v = |action| self.value(action, keys, pad);
        let pd = |action| self.controller_value(action, pad);
        let kb = |action| self.keyboard_value(action, keys);
        // The reverse chord's full brake goes to Rust's native speed gate.
        let consumed: Vec<Atom> = self.profile.controller[index(Action::Reverse)]
            .iter()
            .filter(|b| matches!(b, Binding::Chord(_)) && amount(b, &none, pad, zone) > 0.5)
            .flat_map(|b| b.atoms().to_vec())
            .collect();
        let brake = self.profile.controller[index(Action::Brake)]
            .iter()
            .filter(|b| !b.atoms().iter().all(|a| consumed.contains(a)))
            .map(|b| {
                self.tuning
                    .shape(Action::Brake, amount(b, &none, pad, zone))
            })
            .fold(0., f32::max);
        let gas = kb(Action::Throttle) > 0.5;
        let back = kb(Action::Brake).max(kb(Action::Reverse)) > 0.5;
        let x = v(Action::SteerRight) - v(Action::SteerLeft);
        let y = v(Action::LeanBack) - v(Action::LeanForward);
        let mask = (v(Action::StuntCircle) > 0.5) as u8
            | ((v(Action::StuntTriangle) > 0.5) as u8) << 1
            | ((v(Action::StuntL1) > 0.5) as u8) << 2;
        let [stunt_x, stunt_y] = self.keyboard_trick_axes(keys, now).unwrap_or([x, y]);
        let mut controls = Controls {
            // Keys and controllers spin with steering under Alpha Arcade
            // flips (touch keeps its own axis).
            arcade_spin: Some(x.clamp(-1., 1.)),
            flip_request: None,
            throttle: pd(Action::Throttle),
            brake,
            steer: x,
            lean: y,
            reverse: pd(Action::Reverse) > 0.5,
            reverse_chord_brake: (!consumed.is_empty()).then(|| pd(Action::Brake)),
            preload_axis: None,
            keyboard_drive: Some(TouchIntent {
                generation: self.generation,
                held: gas || back,
                axis: if back {
                    -1.
                } else if gas {
                    1.
                } else {
                    0.
                },
                grounded_only: false,
                forward_only: false,
                reverse_held: false,
            }),
            touch_drive: None,
            stunt_modifiers: mask,
            stunt_x,
            stunt_y,
            generation: self.generation,
            clutch: v(Action::Clutch) > 0.5,
        };
        let orbit = v(Action::CameraRight) - v(Action::CameraLeft);
        let edge_actions = [Action::Camera, Action::Reset, Action::Pause];
        let values = edge_actions.map(|a| v(a) > 0.5);
        let preload = v(Action::PreloadHold) > 0.5;
        let mut edges = [false; 3];
        for (i, action) in edge_actions.into_iter().enumerate() {
            let held = values[i];
            if !held {
                self.blocked.retain(|a| *a != action);
            }
            let queued = self.queued.iter().position(|a| *a == action);
            if let Some(q) = queued {
                self.queued.remove(q);
            }
            edges[i] = queued.is_some()
                || (held && !self.previous.contains(&action) && !self.blocked.contains(&action));
            if held {
                if !self.previous.contains(&action) {
                    self.previous.push(action);
                }
            } else {
                self.previous.retain(|a| *a != action);
            }
        }
        if !preload {
            self.blocked.retain(|a| *a != Action::PreloadHold);
        }
        if !self.blocked.contains(&Action::PreloadHold) {
            if preload {
                controls.preload_axis = Some(1.);
            } else if self.preload_held {
                controls.preload_axis = Some(-1.);
            }
            self.preload_held = preload;
        }
        // drivingInput clamps the combined throttle/brake/steer.
        controls.throttle = controls.throttle.clamp(0., 1.);
        controls.brake = controls.brake.clamp(0., 1.);
        controls.steer = controls.steer.clamp(-1., 1.);
        Read {
            controls,
            camera: edges[0],
            reset: edges[1],
            pause: edges[2],
            orbit,
        }
    }
}

/// Default keyboard codes of an action, in binding order.
pub fn keyboard_codes(action: Action) -> Vec<String> {
    default_keyboard()[index(action)]
        .iter()
        .flat_map(|b| b.atoms().to_vec())
        .filter_map(|a| match a {
            Atom::Key(k) => Some(k),
            _ => None,
        })
        .collect()
}

// --- names (bindings.js bindingLabel, pad-glyphs.js bindingText) ----------

const PAD_BUTTONS: [&str; 17] = [
    "Cross",
    "Circle",
    "Square",
    "Triangle",
    "L1",
    "R1",
    "L2",
    "R2",
    "SELECT",
    "START",
    "L3",
    "R3",
    "D-pad up",
    "D-pad down",
    "D-pad left",
    "D-pad right",
    "Home",
];

/// pad-glyphs.js `inputName` / bindings.js `bindingLabel` atom names.
pub fn input_name(atom: &Atom, standard: bool) -> String {
    match atom {
        Atom::Key(k) => k
            .strip_prefix("Key")
            .or_else(|| k.strip_prefix("Digit"))
            .unwrap_or(k)
            .into(),
        Atom::Button(b) => match PAD_BUTTONS.get(*b as usize).filter(|_| standard) {
            Some(name) => (*name).into(),
            None => format!("Button {b}"),
        },
        Atom::Axis(a, s) => {
            if standard && *a < 4 {
                let stick = if *a < 2 { "Left" } else { "Right" };
                let direction = match (a % 2, *s < 0) {
                    (0, true) => "left",
                    (0, false) => "right",
                    (_, true) => "up",
                    (_, false) => "down",
                };
                format!("{stick} stick {direction}")
            } else {
                format!("Axis {a} {}", if *s < 0 { '-' } else { '+' })
            }
        }
    }
}

/// A row as text: chords joined with " + ", alternatives with " / ".
pub fn binding_text(bindings: &[Binding], standard: bool) -> String {
    if bindings.is_empty() {
        return "Unbound".into();
    }
    bindings
        .iter()
        .map(|b| {
            b.atoms()
                .iter()
                .map(|a| input_name(a, standard))
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_defaults_match_the_browser() {
        let mut bindings = Bindings::default();
        let held = ["KeyW", "KeyA", "Space"];
        let keys = |code: &str| held.contains(&code);
        let read = bindings.read(&keys, None, 0.);
        let c = read.controls;
        assert_eq!((c.throttle, c.brake, c.steer, c.lean), (0., 0., -1., 0.));
        let drive = c.keyboard_drive.unwrap();
        assert!(drive.held && drive.axis == 1.);
        assert_eq!(c.preload_axis, Some(1.));
        let none = |_: &str| false;
        assert_eq!(
            bindings.read(&none, None, 0.).controls.preload_axis,
            Some(-1.)
        );
        assert_eq!(bindings.read(&none, None, 0.).controls.preload_axis, None);
        let reset = |code: &str| code == "KeyR";
        assert!(bindings.read(&reset, None, 0.).reset);
        assert!(!bindings.read(&reset, None, 0.).reset);
    }

    #[test]
    fn a_tap_between_batches_still_pauses() {
        let mut bindings = Bindings::default();
        let held = |code: &str| code == "Escape";
        bindings.key_pressed("Escape", 0., &held);
        let none = |_: &str| false;
        bindings.key_released(&none);
        assert!(bindings.read(&none, None, 0.).pause);
        assert!(!bindings.read(&none, None, 0.).pause);
    }

    #[test]
    fn controller_reverse_chord_keeps_its_brake_for_the_speed_gate() {
        let mut bindings = Bindings::default();
        let mut pad = Pad::default();
        pad.buttons[0] = 1.;
        pad.buttons[2] = 1.;
        pad.axes[0] = 0.15 + 0.85 * 0.5;
        let none = |_: &str| false;
        let c = bindings.read(&none, Some(&pad), 0.).controls;
        assert!(c.reverse);
        assert_eq!(c.brake, 0.);
        assert_eq!(c.reverse_chord_brake, Some(1.));
        assert!((c.steer - 0.5).abs() < 1e-6);
    }

    #[test]
    fn saved_profiles_presets_and_names_round_trip() {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.controller_preset(), "remastered");
        bindings.set_binding(true, Action::Camera, vec![key("KeyV")]);
        bindings.cycle_controller_preset(1);
        assert_eq!(bindings.controller_preset_label(), "ATV Offroad Fury 2");
        bindings.set_binding(false, Action::Pause, vec![button(8)]);
        assert_eq!(bindings.controller_preset(), "custom");
        bindings.apply_controller_preset("unleashed");
        assert!(bindings.custom_controller.is_some());
        let text = bindings.to_json();
        let (loaded, migrated) = Bindings::load(Some(&text), None);
        assert!(!migrated);
        assert_eq!(loaded.profile, bindings.profile);
        assert_eq!(loaded.custom_controller, bindings.custom_controller);
        assert_eq!(
            binding_text(loaded.row(false, Action::Reverse), true),
            "Square + Cross"
        );
        assert_eq!(binding_text(loaded.row(true, Action::Camera), true), "V");
        assert_eq!(binding_text(&[axis(1, -1)], true), "Left stick up");
        // A version-1 keyboard adopts the new defaults except customized keys.
        let mut v1 = serde_json::from_str::<Value>(&text).unwrap();
        v1["version"] = json!(1);
        v1["keyboard"] = rows_json(&v1_keyboard());
        v1["keyboard"]["camera"] = json!([{"key": "ArrowUp"}]);
        let (old, migrated) = Bindings::load(Some(&v1.to_string()), None);
        assert!(migrated);
        assert_eq!(old.row(true, Action::Throttle), &[key("KeyW")]);
        assert_eq!(old.row(true, Action::Camera), &[key("ArrowUp")]);
    }

    #[test]
    fn tuning_shapes_controller_values_only() {
        let (mut bindings, _) = Bindings::load(None, Some(r#"{"steering": 50, "deadzone": 0}"#));
        let mut pad = Pad::default();
        pad.axes[0] = 0.5;
        let none = |_: &str| false;
        let steer = bindings.read(&none, Some(&pad), 0.).controls.steer;
        assert!((steer - 0.25).abs() < 1e-6, "{steer}");
        assert!(bindings.tuning.to_json().contains("\"steering\":50"));
    }
}
