//! Controllers through gilrs, in the W3C standard-gamepad order the browser
//! bindings use. The pad with the latest input is the active one, like
//! web/gamepads.js "Auto".
use atv_game::ride::bindings::Pad;
use gilrs::{Axis, Button, GamepadId, Gilrs};

pub struct Gamepads {
    gilrs: Option<Gilrs>,
    active: Option<GamepadId>,
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
        }
    }

    /// Name of the controller in use (Controller Settings: "Auto (name)").
    pub fn name(&self) -> Option<String> {
        let gilrs = self.gilrs.as_ref()?;
        Some(gilrs.connected_gamepad(self.active?)?.name().to_string())
    }

    pub fn sample(&mut self) -> Option<Pad> {
        let gilrs = self.gilrs.as_mut()?;
        while let Some(event) = gilrs.next_event() {
            if !matches!(event.event, gilrs::EventType::Disconnected) {
                self.active = Some(event.id);
            } else if self.active == Some(event.id) {
                self.active = None;
            }
        }
        let id = self
            .active
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
}
