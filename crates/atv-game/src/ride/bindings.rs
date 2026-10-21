//! Default gameplay bindings (web/bindings.js `DEFAULTS` and `read`, Fury
//! Remastered controller preset, 15% stick dead zone, linear response).
//! Remapping and the alternative presets are not ported yet.
use super::Controls;
pub use atv_core::browser_input::TouchIntent;

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
    PreloadHold,
    Camera,
    CameraLeft,
    CameraRight,
    Reset,
    Pause,
}

#[derive(Clone, Copy, Debug)]
enum Atom {
    Key(&'static str),
    Button(usize),
    Axis(usize, f32),
}
use Atom::{Axis, Button, Key};

fn keyboard(action: Action) -> &'static [Atom] {
    use Action::*;
    match action {
        Throttle => &[Key("KeyW"), Key("ArrowUp")],
        Brake => &[Key("KeyS"), Key("ArrowDown")],
        SteerLeft => &[Key("KeyA"), Key("ArrowLeft")],
        SteerRight => &[Key("KeyD"), Key("ArrowRight")],
        LeanForward => &[Key("KeyQ")],
        LeanBack => &[Key("KeyE"), Key("ShiftLeft")],
        Reverse => &[],
        StuntCircle => &[Key("KeyJ")],
        StuntTriangle => &[Key("KeyK")],
        StuntL1 => &[Key("KeyL")],
        PreloadHold => &[Key("Space")],
        Camera => &[Key("KeyC")],
        CameraLeft => &[Key("KeyZ")],
        CameraRight => &[Key("KeyX")],
        Reset => &[Key("KeyR"), Key("Backspace")],
        Pause => &[Key("Escape"), Key("KeyP")],
    }
}

/// Controller rows; a slice with more than one atom in `chords` is a chord.
fn controller(action: Action) -> (&'static [Atom], &'static [&'static [Atom]]) {
    use Action::*;
    match action {
        Throttle => (&[Button(0), Button(7)], &[]),
        Brake => (&[Button(2), Button(6)], &[]),
        SteerLeft => (&[Axis(0, -1.), Button(14)], &[]),
        SteerRight => (&[Axis(0, 1.), Button(15)], &[]),
        LeanForward => (&[Axis(1, -1.), Button(12)], &[]),
        LeanBack => (&[Axis(1, 1.), Button(13)], &[]),
        Reverse => (&[], &[&[Button(0), Button(2)]]),
        StuntCircle => (&[Button(1)], &[]),
        StuntTriangle => (&[Button(3)], &[]),
        StuntL1 => (&[Button(4)], &[]),
        PreloadHold => (&[], &[]),
        Camera => (&[Button(11)], &[]),
        CameraLeft => (&[Axis(2, -1.)], &[]),
        CameraRight => (&[Axis(2, 1.)], &[]),
        Reset => (&[], &[&[Button(4), Button(5)]]),
        Pause => (&[Button(9)], &[]),
    }
}

/// A standard-mapping controller sample (W3C Gamepad button/axis order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pad {
    /// Distinguishes controllers; a change is a cancellation.
    pub id: u64,
    pub buttons: [f32; 17],
    pub axes: [f32; 4],
}

const ZONE: f32 = 0.15;

fn amount(atom: Atom, keys: &dyn Fn(&str) -> bool, pad: Option<&Pad>) -> f32 {
    match atom {
        Key(code) => keys(code) as u8 as f32,
        Button(b) => pad.map_or(0., |p| {
            p.buttons.get(b).copied().unwrap_or(0.).clamp(0., 1.)
        }),
        Axis(a, sign) => {
            let value = pad
                .map_or(0., |p| p.axes.get(a).copied().unwrap_or(0.) * sign)
                .clamp(0., 1.);
            if value <= ZONE {
                0.
            } else {
                (value - ZONE) / (1. - ZONE)
            }
        }
    }
}

/// Stateful reader: edge actions, preload release and cancellation.
#[derive(Default)]
pub struct Bindings {
    pub generation: u64,
    previous: Vec<Action>,
    blocked: Vec<Action>,
    preload_held: bool,
    controller: Option<Option<u64>>,
    /// Keyboard press order and time (seconds) for trick directions.
    pressed: std::collections::HashMap<&'static str, (u64, f64)>,
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
    /// Blur, pause, reset and controller changes cancel held edge controls.
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.previous.clear();
        self.preload_held = false;
        self.blocked = vec![
            Action::Camera,
            Action::Reset,
            Action::Pause,
            Action::PreloadHold,
        ];
    }

    /// Record a fresh key press (for trick directions); repeats are ignored.
    pub fn key_pressed(&mut self, code: &str, now: f64, keys: &dyn Fn(&str) -> bool) {
        let all = [
            Action::Throttle,
            Action::Brake,
            Action::SteerLeft,
            Action::SteerRight,
            Action::LeanForward,
            Action::LeanBack,
            Action::StuntCircle,
            Action::StuntTriangle,
            Action::StuntL1,
            Action::PreloadHold,
        ];
        for action in all {
            for atom in keyboard(action) {
                if let Key(name) = atom
                    && *name == code
                {
                    self.press_count += 1;
                    self.pressed.insert(name, (self.press_count, now));
                }
            }
        }
        if keyboard(Action::PreloadHold)
            .iter()
            .any(|a| matches!(a, Key(name) if *name == code))
            && keys(code)
        {
            self.blocked.retain(|a| *a != Action::PreloadHold);
        }
    }

    fn keyboard_trick_axes(&self, keys: &dyn Fn(&str) -> bool, now: f64) -> Option<[f32; 2]> {
        let held = |action: Action| -> Vec<(u64, f64)> {
            keyboard(action)
                .iter()
                .filter_map(|a| match a {
                    Key(name) if keys(name) => {
                        Some(*self.pressed.get(name).unwrap_or(&(0, f64::NEG_INFINITY)))
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
        let v = |action| value(action, keys, pad);
        let pd = |atom: Atom| amount(atom, keys, pad);
        // The reverse chord's full brake goes to Rust's native speed gate.
        let chord = controller(Action::Reverse).1[0];
        let consumed = chord.iter().map(|a| pd(*a)).fold(f32::INFINITY, f32::min) > 0.5;
        let brake = controller(Action::Brake)
            .0
            .iter()
            .filter(|a| !(consumed && chord.iter().any(|c| same(**a, *c))))
            .map(|a| pd(*a))
            .fold(0., f32::max);
        let key = |action: Action| {
            keyboard(action)
                .iter()
                .map(|a| amount(*a, keys, None))
                .fold(0., f32::max)
        };
        let pad_value = |action: Action| {
            let (single, chords) = controller(action);
            single
                .iter()
                .map(|a| pd(*a))
                .chain(
                    chords
                        .iter()
                        .map(|c| c.iter().map(|a| pd(*a)).fold(f32::INFINITY, f32::min)),
                )
                .fold(0., f32::max)
        };
        let gas = key(Action::Throttle) > 0.5;
        let back = key(Action::Brake).max(key(Action::Reverse)) > 0.5;
        let x = v(Action::SteerRight) - v(Action::SteerLeft);
        let y = v(Action::LeanBack) - v(Action::LeanForward);
        let mask = (v(Action::StuntCircle) > 0.5) as u8
            | ((v(Action::StuntTriangle) > 0.5) as u8) << 1
            | ((v(Action::StuntL1) > 0.5) as u8) << 2;
        let [stunt_x, stunt_y] = self.keyboard_trick_axes(keys, now).unwrap_or([x, y]);
        let mut controls = Controls {
            throttle: pad_value(Action::Throttle),
            brake,
            steer: x,
            lean: y,
            reverse: pad_value(Action::Reverse) > 0.5,
            reverse_chord_brake: consumed.then(|| pad_value(Action::Brake)),
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
        };
        let mut edge = |action: Action| {
            let held = v(action) > 0.5;
            if !held {
                self.blocked.retain(|a| *a != action);
            }
            let fresh = held && !self.previous.contains(&action) && !self.blocked.contains(&action);
            if held {
                if !self.previous.contains(&action) {
                    self.previous.push(action);
                }
            } else {
                self.previous.retain(|a| *a != action);
            }
            fresh
        };
        let camera = edge(Action::Camera);
        let reset = edge(Action::Reset);
        let pause = edge(Action::Pause);
        let preload = v(Action::PreloadHold) > 0.5;
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
            camera,
            reset,
            pause,
            orbit: v(Action::CameraRight) - v(Action::CameraLeft),
        }
    }
}

pub fn value(action: Action, keys: &dyn Fn(&str) -> bool, pad: Option<&Pad>) -> f32 {
    let key = keyboard(action)
        .iter()
        .map(|a| amount(*a, keys, None))
        .fold(0., f32::max);
    let (single, chords) = controller(action);
    let pad_value = single
        .iter()
        .map(|a| amount(*a, keys, pad))
        .chain(chords.iter().map(|chord| {
            chord
                .iter()
                .map(|a| amount(*a, keys, pad))
                .fold(f32::INFINITY, f32::min)
        }))
        .fold(0., f32::max);
    key.max(pad_value)
}

fn same(a: Atom, b: Atom) -> bool {
    match (a, b) {
        (Key(x), Key(y)) => x == y,
        (Button(x), Button(y)) => x == y,
        (Axis(x, s), Axis(y, t)) => x == y && s == t,
        _ => false,
    }
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
}
