//! Controllers through gilrs, in the W3C standard-gamepad order the browser
//! bindings use. The pad with fresh input is the active one, like
//! web/gamepads.js "Auto", unless Controller Settings pins a connected pad.
use atv_game::ride::bindings::Pad;
use gilrs::{Axis, Button, GamepadId, Gilrs};
use std::collections::HashMap;

/// gamepads.js: axes resting far from centre (idle triggers, hat switches,
/// pedals) are measured from their first-seen value, so they never count as
/// activity; a move of more than MOVED from there does.
const RESTING: f32 = 0.5;
const MOVED: f32 = 0.35;

pub struct Gamepads {
    gilrs: Option<Gilrs>,
    /// The pad in use: the pin, else Auto's choice (`activeGamepad`).
    active: Option<GamepadId>,
    /// Auto's choice and each pad's axis baselines.
    tracker: Tracker<GamepadId>,
    /// Controller Settings' pinned pad (name, index), or Auto.
    pin: Option<(String, u32)>,
    /// The playing rumble (dropping a gilrs effect stops it).
    #[cfg(not(target_arch = "wasm32"))]
    effect: Option<gilrs::ff::Effect>,
    /// Swallows an unsupported effect's rejection (one closure, reused).
    #[cfg(target_arch = "wasm32")]
    ignore: wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)>,
}

/// One connected pad as the tracker sees it: its key, name (browser `id`),
/// index, standard mapping and this sample's input.
struct PadView<K> {
    key: K,
    name: String,
    index: u32,
    standard: bool,
    pressed: bool,
    /// (axis code, value).
    axes: Vec<(u32, f32)>,
}

/// gamepads.js `createPadTracker`: Auto follows the pad with fresh input.
struct Tracker<K> {
    current: Option<K>,
    /// Each pad's axis baselines (gamepads.js `baselines`).
    baselines: HashMap<K, HashMap<u32, f32>>,
}

impl<K> Default for Tracker<K> {
    fn default() -> Self {
        Self {
            current: None,
            baselines: HashMap::new(),
        }
    }
}

impl<K: Copy + Eq + std::hash::Hash> Tracker<K> {
    /// gamepads.js `activeGamepad`: the pinned pad while connected, else a
    /// pad with fresh input (the current one first), else the current pad,
    /// a standard-mapped one, or the first.
    fn choose(&mut self, pads: &[PadView<K>], pin: Option<&(String, u32)>) -> Option<K> {
        self.baselines
            .retain(|key, _| pads.iter().any(|p| p.key == *key));
        // Every pad's baseline is taken at first sight, pinned or not.
        let live: Vec<K> = pads
            .iter()
            .filter(|pad| {
                let base = self.baselines.entry(pad.key).or_default();
                let mut moved = pad.pressed;
                for &(code, value) in &pad.axes {
                    if !value.is_finite() {
                        continue;
                    }
                    let rest =
                        *base
                            .entry(code)
                            .or_insert(if value.abs() > RESTING { value } else { 0. });
                    moved |= (value - rest).abs() > MOVED;
                }
                moved
            })
            .map(|pad| pad.key)
            .collect();
        let current = self
            .current
            .filter(|key| pads.iter().any(|p| p.key == *key));
        let chosen = pinned(pads.iter().map(|p| (p.key, p.name.as_str(), p.index)), pin)
            .or_else(|| current.filter(|key| live.contains(key)))
            .or_else(|| live.first().copied())
            .or(current)
            .or_else(|| pads.iter().find(|p| p.standard).map(|p| p.key))
            .or_else(|| pads.first().map(|p| p.key));
        self.current = chosen;
        chosen
    }
}

/// gamepads.js `pinned`: among connected (key, name, index), the pinned
/// name, preferring the pinned index.
fn pinned<'a, K: Copy>(
    pads: impl Iterator<Item = (K, &'a str, u32)>,
    pin: Option<&(String, u32)>,
) -> Option<K> {
    let (name, index) = pin?;
    let same: Vec<(K, u32)> = pads
        .filter(|(_, n, _)| n == name)
        .map(|(key, _, i)| (key, i))
        .collect();
    same.iter()
        .find(|(_, i)| i == index)
        .or_else(|| same.first())
        .map(|(key, _)| *key)
}

const BUTTONS: [Button; 17] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
    Button::DPadRight,
    Button::Mode,
];

impl Gamepads {
    pub fn new() -> Self {
        Self {
            gilrs: Gilrs::new().ok(),
            active: None,
            tracker: Tracker::default(),
            pin: None,
            #[cfg(not(target_arch = "wasm32"))]
            effect: None,
            #[cfg(target_arch = "wasm32")]
            ignore: wasm_bindgen::closure::Closure::new(|_| {}),
        }
    }

    /// Name of the controller in use (Controller Settings: "Auto (name)").
    pub fn name(&self) -> Option<String> {
        let gilrs = self.gilrs.as_ref()?;
        let id = self.pinned().or(self.active)?;
        Some(gilrs.connected_gamepad(id)?.name().to_string())
    }

    /// The saved pin (`atv2-controller-v1` `selected`), or None for Auto.
    pub fn set_pin(&mut self, pin: Option<(String, u32)>) {
        self.pin = pin;
    }

    /// Connected controllers: (name, index) in the platform's order.
    pub fn connected(&self) -> Vec<(String, u32)> {
        self.gilrs.as_ref().map_or_else(Vec::new, |gilrs| {
            gilrs
                .gamepads()
                .map(|(id, pad)| (pad.name().to_string(), usize::from(id) as u32))
                .collect()
        })
    }

    /// gamepads.js `pinned`: the pinned pad while connected (same name,
    /// preferring the same index).
    fn pinned(&self) -> Option<GamepadId> {
        let gilrs = self.gilrs.as_ref()?;
        let pads: Vec<(GamepadId, String, u32)> = gilrs
            .gamepads()
            .map(|(id, pad)| (id, pad.name().to_string(), usize::from(id) as u32))
            .collect();
        pinned(
            pads.iter()
                .map(|(id, name, index)| (*id, name.as_str(), *index)),
            self.pin.as_ref(),
        )
    }

    /// pad-glyphs.js `standardPad`: standard unless the active pad reports
    /// no mapping (the browser's empty `mapping`; gilrs without a layout).
    pub fn standard(&self) -> bool {
        let Some(id) = self.pinned().or(self.active) else {
            return true;
        };
        self.is_standard(id).unwrap_or(true)
    }

    /// Whether a connected pad reports the standard mapping (None: gone).
    fn is_standard(&self, id: GamepadId) -> Option<bool> {
        let pad = self.gilrs.as_ref()?.connected_gamepad(id)?;
        #[cfg(target_arch = "wasm32")]
        {
            let name = pad.name().to_string();
            Some(
                browser_pads()
                    .into_iter()
                    .find(|p| {
                        browser_get(p, "id").and_then(|v| v.as_string()).as_deref() == Some(&name)
                    })
                    .and_then(|p| browser_get(&p, "mapping")?.as_string())
                    .is_none_or(|mapping| !mapping.is_empty()),
            )
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Some(pad.mapping_source() != gilrs::MappingSource::None)
        }
    }

    /// gamepads.js `activeGamepad` over the connected gilrs pads.
    fn choose(&mut self) -> Option<GamepadId> {
        let gilrs = self.gilrs.as_ref()?;
        let pads: Vec<PadView<GamepadId>> = gilrs
            .gamepads()
            .map(|(id, pad)| {
                let state = pad.state();
                PadView {
                    key: id,
                    name: pad.name().to_string(),
                    index: usize::from(id) as u32,
                    standard: self.is_standard(id) == Some(true),
                    pressed: state.buttons().any(|(_, b)| b.is_pressed()),
                    axes: state
                        .axes()
                        .map(|(code, axis)| (code.into_u32(), axis.value()))
                        .collect(),
                }
            })
            .collect();
        self.tracker.choose(&pads, self.pin.as_ref())
    }

    pub fn sample(&mut self) -> Option<Pad> {
        let gilrs = self.gilrs.as_mut()?;
        while gilrs.next_event().is_some() {}
        self.active = self.choose();
        let id = self.active?;
        let gilrs = self.gilrs.as_ref()?;
        let pad = gilrs.gamepad(id);
        let mut out = Pad {
            id: usize::from(id) as u64,
            ..Default::default()
        };
        for (slot, button) in out.buttons.iter_mut().zip(BUTTONS) {
            *slot = pad
                .button_data(button)
                .map_or(0., |data| data.value())
                .clamp(0., 1.);
        }
        // Standard mapping: stick Y is positive downwards.
        out.axes[..4].copy_from_slice(&[
            pad.value(Axis::LeftStickX),
            -pad.value(Axis::LeftStickY),
            pad.value(Axis::RightStickX),
            -pad.value(Axis::RightStickY),
        ]);
        extra_inputs(&pad, &mut out);
        Some(out)
    }

    /// Rumble.pulse on the active controller: gilrs force feedback (Linux
    /// and Windows; gilrs has none on macOS). True when a pad took it.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn rumble(&mut self, pulse: atv_game::ride::impact::Pulse) -> bool {
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
        let Some(gilrs) = self.gilrs.as_mut() else {
            return false;
        };
        // game.js Rumble: the pad in use (pinned or Auto), as it drives.
        let Some(id) = self
            .active
            .filter(|id| gilrs.connected_gamepad(*id).is_some())
            .filter(|id| gilrs.gamepad(*id).is_ff_supported())
        else {
            return false;
        };
        let scheduling = Replay {
            play_for: Ticks::from_ms(pulse.duration_ms),
            ..Default::default()
        };
        let magnitude = |v: f64| (v.clamp(0., 1.) * f64::from(u16::MAX)) as u16;
        let effect = EffectBuilder::new()
            .add_effect(BaseEffect {
                kind: BaseEffectType::Strong {
                    magnitude: magnitude(pulse.strength),
                },
                scheduling,
                envelope: Default::default(),
            })
            .add_effect(BaseEffect {
                kind: BaseEffectType::Weak {
                    magnitude: magnitude(pulse.weak),
                },
                scheduling,
                envelope: Default::default(),
            })
            .gamepads(&[id])
            .finish(gilrs);
        match effect {
            Ok(effect) if effect.play().is_ok() => {
                self.effect = Some(effect);
                true
            }
            _ => false,
        }
    }

    /// Rumble.pulse in the browser: the Gamepad API `dual-rumble` on the
    /// pad in use (pinned or Auto), else a touch device's `navigator.vibrate`
    /// for pulses from 0.3 (iOS has none). gilrs has no web force feedback.
    #[cfg(target_arch = "wasm32")]
    pub fn rumble(&mut self, pulse: atv_game::ride::impact::Pulse) -> bool {
        use js_sys::Reflect;
        use wasm_bindgen::{JsCast, JsValue};
        let Some(window) = web_sys::window() else {
            return false;
        };
        let navigator: JsValue = window.navigator().into();
        let get = browser_get;
        // The browser pad behind the gilrs pad in use (same id; the latest
        // input among identical names).
        let name = self.name();
        let pad = browser_pads()
            .into_iter()
            .filter(|p| get(p, "id").and_then(|v| v.as_string()) == name)
            .max_by(|a, b| {
                let t = |p: &JsValue| get(p, "timestamp").and_then(|v| v.as_f64()).unwrap_or(0.);
                t(a).total_cmp(&t(b))
            });
        if let Some(actuator) = pad
            .and_then(|p| get(&p, "vibrationActuator"))
            .filter(|a| a.is_object())
            && let Some(play) =
                get(&actuator, "playEffect").and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        {
            let options = js_sys::Object::new();
            for (key, value) in [
                ("startDelay", 0.),
                ("duration", f64::from(pulse.duration_ms)),
                ("strongMagnitude", pulse.strength),
                ("weakMagnitude", pulse.weak),
            ] {
                let _ = Reflect::set(&options, &key.into(), &value.into());
            }
            if let Ok(promise) = play.call2(&actuator, &"dual-rumble".into(), &options) {
                // A rejected effect (unsupported) must not surface as an error.
                if let Ok(promise) = promise.dyn_into::<js_sys::Promise>() {
                    let _ = promise.catch(&self.ignore);
                }
                return true;
            }
        }
        let coarse = window
            .match_media("(pointer: coarse)")
            .ok()
            .flatten()
            .is_some_and(|m| m.matches());
        // Only where `navigator.vibrate` exists: iOS Safari has none, and
        // calling the missing binding throws through the frame and freezes
        // the engine (game.js calls `navigator.vibrate?.()`).
        if coarse
            && pulse.strength >= 0.3
            && let Some(vibrate) =
                get(&navigator, "vibrate").and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        {
            return vibrate
                .call1(&navigator, &f64::from(pulse.duration_ms).into())
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
        false
    }
}

/// Inputs past the standard 17 buttons and 4 axes (gamepads.js reads every
/// index): the browser pad's own further buttons and axes.
#[cfg(target_arch = "wasm32")]
fn extra_inputs(pad: &gilrs::Gamepad, out: &mut Pad) {
    use atv_game::ride::bindings::{PAD_AXIS_COUNT, PAD_BUTTON_COUNT};
    let name = pad.name().to_string();
    let Some(browser) = browser_pads()
        .into_iter()
        .find(|p| browser_get(p, "id").and_then(|v| v.as_string()).as_deref() == Some(&name))
    else {
        return;
    };
    let list = |key: &str| {
        browser_get(&browser, key)
            .map(|v| js_sys::Array::from(&v))
            .unwrap_or_default()
    };
    let buttons = list("buttons");
    for i in BUTTONS.len()..(buttons.length() as usize).min(PAD_BUTTON_COUNT) {
        let button = buttons.get(i as u32);
        out.buttons[i] = browser_get(&button, "value")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.)
            .clamp(0., 1.) as f32;
    }
    let axes = list("axes");
    for i in 4..(axes.length() as usize).min(PAD_AXIS_COUNT) {
        out.axes[i] = axes.get(i as u32).as_f64().unwrap_or(0.) as f32;
    }
}

/// Inputs past the standard 17 buttons and 4 axes (gamepads.js reads every
/// index): the pad's unmapped buttons and axes, in event-code order.
#[cfg(not(target_arch = "wasm32"))]
fn extra_inputs(pad: &gilrs::Gamepad, out: &mut Pad) {
    use gilrs::ev::AxisOrBtn;
    let unmapped = |code: gilrs::ev::Code| {
        matches!(
            pad.axis_or_btn_name(code),
            None | Some(AxisOrBtn::Btn(Button::Unknown)) | Some(AxisOrBtn::Axis(Axis::Unknown))
        )
    };
    let state = pad.state();
    let mut buttons: Vec<_> = state
        .buttons()
        .filter(|(code, _)| unmapped(*code))
        .map(|(code, data)| (code.into_u32(), data.value()))
        .collect();
    buttons.sort_by_key(|(code, _)| *code);
    for (slot, (_, value)) in out.buttons[BUTTONS.len()..].iter_mut().zip(buttons) {
        *slot = value.clamp(0., 1.);
    }
    let mut axes: Vec<_> = state
        .axes()
        .filter(|(code, _)| unmapped(*code))
        .map(|(code, data)| (code.into_u32(), data.value()))
        .collect();
    axes.sort_by_key(|(code, _)| *code);
    for (slot, (_, value)) in out.axes[4..].iter_mut().zip(axes) {
        *slot = value;
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_get(target: &wasm_bindgen::JsValue, key: &str) -> Option<wasm_bindgen::JsValue> {
    js_sys::Reflect::get(target, &key.into()).ok()
}

/// `navigator.getGamepads()`'s connected pads.
#[cfg(target_arch = "wasm32")]
fn browser_pads() -> Vec<wasm_bindgen::JsValue> {
    use wasm_bindgen::{JsCast, JsValue};
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let navigator: JsValue = window.navigator().into();
    browser_get(&navigator, "getGamepads")
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&navigator).ok())
        .map(|list| js_sys::Array::from(&list))
        .unwrap_or_default()
        .iter()
        .filter(|p| browser_get(p, "connected").and_then(|c| c.as_bool()) == Some(true))
        .collect()
}

#[cfg(test)]
mod tests {
    //! web/gamepads.test.mjs `createPadTracker` cases.
    use super::*;

    fn pad(index: u32, name: &str, standard: bool, axes: [f32; 4], pressed: bool) -> PadView<u32> {
        PadView {
            key: index,
            name: name.into(),
            index,
            standard,
            pressed,
            axes: axes
                .iter()
                .enumerate()
                .map(|(i, v)| (i as u32, *v))
                .collect(),
        }
    }
    const REST: [f32; 4] = [0.; 4];

    #[test]
    fn follows_the_pad_in_use_not_an_idle_pad_listed_first() {
        let mut t = Tracker::default();
        let steam = pad(0, "Steam Virtual Gamepad", true, REST, false);
        let sense = |axes, pressed| pad(1, "DualSense Wireless Controller", true, axes, pressed);
        assert_eq!(
            t.choose(&[steam, sense(REST, false)], None),
            Some(0),
            "nothing pressed yet: first standard pad"
        );
        let steam = || pad(0, "Steam Virtual Gamepad", true, REST, false);
        assert_eq!(
            t.choose(&[steam(), sense(REST, true)], None),
            Some(1),
            "a button press selects the real controller"
        );
        assert_eq!(
            t.choose(&[steam(), sense(REST, false)], None),
            Some(1),
            "released: stays on the last used controller"
        );
        assert_eq!(
            t.choose(&[steam(), sense([-0.8, 0., 0., 0.], false)], None),
            Some(1)
        );
        let moving = pad(0, "Steam Virtual Gamepad", true, [0.9, 0., 0., 0.], false);
        assert_eq!(
            t.choose(&[moving, sense([-0.8, 0., 0., 0.], false)], None),
            Some(1),
            "the current pad keeps priority while both are active"
        );
    }

    #[test]
    fn idle_devices_with_resting_axes_never_take_over() {
        let mut t = Tracker::default();
        let hid = |axes| pad(0, "Generic HID", false, axes, false);
        let xbox = |pressed| pad(1, "Xbox Controller", true, REST, pressed);
        let resting = [-1., 1.2857, 0., 0.];
        assert_eq!(
            t.choose(&[hid(resting), xbox(false)], None),
            Some(1),
            "prefers a standard-mapping pad when idle"
        );
        assert_eq!(t.choose(&[hid(resting), xbox(true)], None), Some(1));
        assert_eq!(
            t.choose(&[hid(resting), xbox(false)], None),
            Some(1),
            "resting -1 / hat axes are not activity"
        );
        assert_eq!(
            t.choose(&[hid([0., 1.2857, 0., 0.]), xbox(false)], None),
            Some(0),
            "real movement on that device does count"
        );
    }

    #[test]
    fn disconnects_and_no_pads() {
        let mut t = Tracker::default();
        let pro = || pad(2, "Pro Controller", true, REST, false);
        assert_eq!(
            t.choose(&[pad(1, "Xbox Controller", true, REST, true), pro()], None),
            Some(1)
        );
        assert_eq!(
            t.choose(&[pro()], None),
            Some(2),
            "falls back when the current pad disconnects"
        );
        assert_eq!(t.choose(&[], None), None);
        assert!(t.baselines.is_empty(), "departed pads drop their baselines");
    }

    #[test]
    fn a_pinned_pad_wins_while_connected_then_auto() {
        let mut t = Tracker::default();
        let xbox = |pressed| pad(0, "Xbox Controller", true, REST, pressed);
        let sense = || pad(1, "DualSense", true, REST, false);
        assert_eq!(t.choose(&[xbox(false), sense()], None), Some(0));
        let pin = ("DualSense".to_string(), 1);
        assert_eq!(
            t.choose(&[xbox(true), sense()], Some(&pin)),
            Some(1),
            "input on another pad does not steal a pinned pad"
        );
        assert_eq!(
            t.choose(&[xbox(false)], Some(&pin)),
            Some(0),
            "absent pinned pad falls back to Auto"
        );
        // Same name elsewhere: the pinned index first, else the first of that name.
        let twins = [
            pad(3, "DualSense", true, REST, false),
            pad(5, "DualSense", true, REST, false),
        ];
        assert_eq!(t.choose(&twins, Some(&("DualSense".into(), 5))), Some(5));
        assert_eq!(t.choose(&twins, Some(&("DualSense".into(), 9))), Some(3));
    }

    #[test]
    fn baselines_are_taken_while_a_pad_is_pinned() {
        // gamepads.js measures every pad, pinned or not: a pedal centred when
        // first seen under a pin and pressed far once the pin is cleared is
        // fresh input, not a resting value.
        let mut t = Tracker::default();
        let wheel = |x| pad(0, "Wheel", false, [x, 0., 0., 0.], false);
        let pin = ("Pad".to_string(), 1);
        let pinned = pad(1, "Pad", true, REST, false);
        assert_eq!(t.choose(&[wheel(0.), pinned], Some(&pin)), Some(1));
        let other = pad(2, "Other", true, REST, false);
        assert_eq!(t.choose(&[wheel(0.9), other], None), Some(0));
    }
}
