//! Controllers through gilrs, in the W3C standard-gamepad order the browser
//! bindings use. The pad with the latest input is the active one, like
//! web/gamepads.js "Auto", unless Controller Settings pins a connected pad.
use atv_game::ride::bindings::Pad;
use gilrs::{Axis, Button, GamepadId, Gilrs};

pub struct Gamepads {
    gilrs: Option<Gilrs>,
    active: Option<GamepadId>,
    /// Controller Settings' pinned pad (name, index), or Auto.
    pin: Option<(String, u32)>,
    /// The playing rumble (dropping a gilrs effect stops it).
    #[cfg(not(target_arch = "wasm32"))]
    effect: Option<gilrs::ff::Effect>,
    /// Swallows an unsupported effect's rejection (one closure, reused).
    #[cfg(target_arch = "wasm32")]
    ignore: wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)>,
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
        let (name, index) = self.pin.as_ref()?;
        let gilrs = self.gilrs.as_ref()?;
        let same: Vec<GamepadId> = gilrs
            .gamepads()
            .filter(|(_, pad)| pad.name() == name)
            .map(|(id, _)| id)
            .collect();
        same.iter()
            .copied()
            .find(|id| usize::from(*id) as u32 == *index)
            .or_else(|| same.first().copied())
    }

    /// pad-glyphs.js `standardPad`: standard unless the active pad reports
    /// no mapping (the browser's empty `mapping`; gilrs without a layout).
    pub fn standard(&self) -> bool {
        let Some(gilrs) = self.gilrs.as_ref() else {
            return true;
        };
        let Some(id) = self.pinned().or(self.active) else {
            return true;
        };
        let Some(pad) = gilrs.connected_gamepad(id) else {
            return true;
        };
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::Reflect;
            use wasm_bindgen::{JsCast, JsValue};
            let Some(window) = web_sys::window() else {
                return true;
            };
            let navigator: JsValue = window.navigator().into();
            let get = |target: &JsValue, key: &str| Reflect::get(target, &key.into()).ok();
            let pads = get(&navigator, "getGamepads")
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                .and_then(|f| f.call0(&navigator).ok())
                .map(|list| js_sys::Array::from(&list))
                .unwrap_or_default();
            let name = pad.name().to_string();
            pads.iter()
                .find(|p| get(p, "id").and_then(|v| v.as_string()).as_deref() == Some(&name))
                .and_then(|p| get(&p, "mapping")?.as_string())
                .is_none_or(|mapping| !mapping.is_empty())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            pad.mapping_source() != gilrs::MappingSource::None
        }
    }

    pub fn sample(&mut self) -> Option<Pad> {
        let pinned = self.pinned();
        let gilrs = self.gilrs.as_mut()?;
        while let Some(event) = gilrs.next_event() {
            if !matches!(event.event, gilrs::EventType::Disconnected) {
                self.active = Some(event.id);
            } else if self.active == Some(event.id) {
                self.active = None;
            }
        }
        // A pinned pad wins while connected; otherwise Auto follows input.
        let id = pinned
            .filter(|id| gilrs.connected_gamepad(*id).is_some())
            .or(self.active)
            .filter(|id| gilrs.connected_gamepad(*id).is_some())
            .or_else(|| gilrs.gamepads().next().map(|(id, _)| id))?;
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
        out.axes = [
            pad.value(Axis::LeftStickX),
            -pad.value(Axis::LeftStickY),
            pad.value(Axis::RightStickX),
            -pad.value(Axis::RightStickY),
        ];
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
        let Some(id) = self
            .active
            .filter(|id| gilrs.connected_gamepad(*id).is_some())
            .or_else(|| gilrs.gamepads().next().map(|(id, _)| id))
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
    /// pad with the latest input, else a touch device's `navigator.vibrate`
    /// for pulses from 0.3 (iOS has none). gilrs has no web force feedback.
    #[cfg(target_arch = "wasm32")]
    pub fn rumble(&mut self, pulse: atv_game::ride::impact::Pulse) -> bool {
        use js_sys::Reflect;
        use wasm_bindgen::{JsCast, JsValue};
        let Some(window) = web_sys::window() else {
            return false;
        };
        let navigator: JsValue = window.navigator().into();
        let get = |target: &JsValue, key: &str| Reflect::get(target, &key.into()).ok();
        let pads = get(&navigator, "getGamepads")
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
            .and_then(|f| f.call0(&navigator).ok())
            .map(|list| js_sys::Array::from(&list))
            .unwrap_or_default();
        let pad = pads
            .iter()
            .filter(|p| get(p, "connected").and_then(|c| c.as_bool()) == Some(true))
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
