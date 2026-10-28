//! Android controllers (gilrs has no Android backend). The patched winit
//! (third_party/winit) delivers a gamepad's buttons as device button events
//! carrying the Android key code, and its sticks, triggers and hat as device
//! motion events carrying the Android axis. They are laid out in the W3C
//! standard-gamepad order as Chrome on Android maps them (GamepadMappings
//! default mapping), so a handheld's built-in controls (Retroid, AYN,
//! Anbernic) play as they do in its browser. Auto and the pin choose among
//! pads as on the other platforms.
use super::{PadView, Tracker, pinned};
use atv_game::ride::bindings::{PAD_AXIS_COUNT, PAD_BUTTON_COUNT, Pad};
use std::collections::{HashMap, HashSet};
use web_time::Instant;

/// `KeyEvent.KEYCODE_*`.
mod key {
    pub const DPAD_UP: u32 = 19;
    pub const DPAD_DOWN: u32 = 20;
    pub const DPAD_LEFT: u32 = 21;
    pub const DPAD_RIGHT: u32 = 22;
    pub const A: u32 = 96;
    pub const B: u32 = 97;
    pub const C: u32 = 98;
    pub const X: u32 = 99;
    pub const Y: u32 = 100;
    pub const Z: u32 = 101;
    pub const L1: u32 = 102;
    pub const R1: u32 = 103;
    pub const L2: u32 = 104;
    pub const R2: u32 = 105;
    pub const THUMBL: u32 = 106;
    pub const THUMBR: u32 = 107;
    pub const START: u32 = 108;
    pub const SELECT: u32 = 109;
    pub const MODE: u32 = 110;
    /// `KEYCODE_BUTTON_1` to `KEYCODE_BUTTON_16`.
    pub const NUMBERED: std::ops::RangeInclusive<u32> = 188..=203;
}

/// `MotionEvent.AXIS_*`.
mod axis {
    pub const X: u32 = 0;
    pub const Y: u32 = 1;
    pub const Z: u32 = 11;
    pub const RX: u32 = 12;
    pub const RY: u32 = 13;
    pub const RZ: u32 = 14;
    pub const HAT_X: u32 = 15;
    pub const HAT_Y: u32 = 16;
    pub const LTRIGGER: u32 = 17;
    pub const RTRIGGER: u32 = 18;
    pub const GAS: u32 = 22;
    pub const BRAKE: u32 = 23;
}

/// Standard buttons 0-16 by key code (L2/R2 and the d-pad also read axes).
const BUTTON_KEYS: [u32; 17] = [
    key::A,
    key::B,
    key::X,
    key::Y,
    key::L1,
    key::R1,
    key::L2,
    key::R2,
    key::SELECT,
    key::START,
    key::THUMBL,
    key::THUMBR,
    key::DPAD_UP,
    key::DPAD_DOWN,
    key::DPAD_LEFT,
    key::DPAD_RIGHT,
    key::MODE,
];

/// Axes past the standard four (a pad whose right stick is RX/RY rather
/// than Z/RZ can still be bound there).
const EXTRA_AXES: [u32; 2] = [axis::RX, axis::RY];

/// A hat or stick reading past this counts as a d-pad press.
const HAT_PRESSED: f32 = 0.5;

/// How often departed devices are looked for.
const PRESENCE_CHECK_SECS: f64 = 1.;

/// One Android input device that has reported gamepad input.
struct Device {
    id: i32,
    /// Order of first input, like the browser's gamepad index.
    index: u32,
    name: String,
    held: HashSet<u32>,
    axes: HashMap<u32, f32>,
}

impl Device {
    fn axis(&self, code: u32) -> f32 {
        self.axes
            .get(&code)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(0.)
    }

    fn key(&self, code: u32) -> f32 {
        self.held.contains(&code) as u8 as f32
    }

    /// The standard layout: Chrome's mapping of buttons, X/Y left stick,
    /// Z/RZ right stick, trigger or pedal axes as the analog L2/R2, and a
    /// hat as the d-pad.
    fn pad(&self) -> Pad {
        let mut out = Pad {
            id: self.id as u32 as u64,
            ..Default::default()
        };
        for (slot, code) in out.buttons.iter_mut().zip(BUTTON_KEYS) {
            *slot = self.key(code);
        }
        let (hat_x, hat_y) = (self.axis(axis::HAT_X), self.axis(axis::HAT_Y));
        for (button, pressed) in [
            (12, hat_y < -HAT_PRESSED),
            (13, hat_y > HAT_PRESSED),
            (14, hat_x < -HAT_PRESSED),
            (15, hat_x > HAT_PRESSED),
        ] {
            if pressed {
                out.buttons[button] = 1.;
            }
        }
        for (button, trigger, pedal) in [
            (6, axis::LTRIGGER, axis::BRAKE),
            (7, axis::RTRIGGER, axis::GAS),
        ] {
            out.buttons[button] = out.buttons[button]
                .max(self.axis(trigger))
                .max(self.axis(pedal))
                .clamp(0., 1.);
        }
        let extra_keys = [key::C, key::Z].into_iter().chain(key::NUMBERED);
        for (slot, code) in out.buttons[BUTTON_KEYS.len()..PAD_BUTTON_COUNT]
            .iter_mut()
            .zip(extra_keys)
        {
            *slot = self.key(code);
        }
        // Android's stick Y is positive downwards, as in the standard mapping.
        for (slot, code) in out.axes[..PAD_AXIS_COUNT].iter_mut().zip(
            [axis::X, axis::Y, axis::Z, axis::RZ]
                .into_iter()
                .chain(EXTRA_AXES),
        ) {
            *slot = self.axis(code);
        }
        out
    }

    fn view(&self) -> PadView<i32> {
        PadView {
            key: self.id,
            name: self.name.clone(),
            index: self.index,
            standard: true,
            pressed: !self.held.is_empty(),
            axes: self
                .axes
                .iter()
                .map(|(code, value)| (*code, *value))
                .collect(),
        }
    }
}

pub struct Gamepads {
    devices: Vec<Device>,
    next_index: u32,
    /// The pad in use: the pin, else Auto's choice.
    active: Option<i32>,
    tracker: Tracker<i32>,
    pin: Option<(String, u32)>,
    checked: Option<Instant>,
}

impl Gamepads {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            next_index: 0,
            active: None,
            tracker: Tracker::default(),
            pin: None,
            checked: None,
        }
    }

    /// A gamepad button (Android key code) went down or up.
    pub fn button(&mut self, id: i32, code: u32, pressed: bool) {
        let device = self.device(id);
        if pressed {
            device.held.insert(code);
        } else {
            device.held.remove(&code);
        }
    }

    /// A gamepad axis (Android axis) moved.
    pub fn axis(&mut self, id: i32, code: u32, value: f32) {
        // Every motion reports all axes; one the device lacks reads 0 and
        // does not make it a pad.
        if value == 0. && !self.devices.iter().any(|d| d.id == id) {
            return;
        }
        self.device(id).axes.insert(code, value);
    }

    fn device(&mut self, id: i32) -> &mut Device {
        let at = match self.devices.iter().position(|d| d.id == id) {
            Some(at) => at,
            None => {
                self.devices.push(Device {
                    id,
                    index: self.next_index,
                    name: device_name(id).unwrap_or_else(|| "Android controller".into()),
                    held: HashSet::new(),
                    axes: HashMap::new(),
                });
                self.next_index += 1;
                self.devices.len() - 1
            }
        };
        &mut self.devices[at]
    }

    /// Name of the controller in use (Controller Settings: "Auto (name)").
    pub fn name(&self) -> Option<String> {
        let id = self.pinned().or(self.active)?;
        Some(self.devices.iter().find(|d| d.id == id)?.name.clone())
    }

    /// The saved pin (`atv2-controller-v1` `selected`), or None for Auto.
    pub fn set_pin(&mut self, pin: Option<(String, u32)>) {
        self.pin = pin;
    }

    /// Controllers that have reported input: (name, index).
    pub fn connected(&self) -> Vec<(String, u32)> {
        self.devices
            .iter()
            .map(|d| (d.name.clone(), d.index))
            .collect()
    }

    fn pinned(&self) -> Option<i32> {
        pinned(
            self.devices
                .iter()
                .map(|d| (d.id, d.name.as_str(), d.index)),
            self.pin.as_ref(),
        )
    }

    /// Every pad is laid out in the standard order.
    pub fn standard(&self) -> bool {
        true
    }

    /// Split screen: one controller plays player 1 on Android for now.
    pub fn sample_slot(&mut self, slot: usize) -> Option<Pad> {
        if slot == 0 { self.sample() } else { None }
    }

    pub fn sample(&mut self) -> Option<Pad> {
        let now = Instant::now();
        if self
            .checked
            .is_none_or(|at| (now - at).as_secs_f64() >= PRESENCE_CHECK_SECS)
        {
            self.checked = Some(now);
            self.devices.retain(|d| device_present(d.id) != Some(false));
        }
        let views: Vec<PadView<i32>> = self.devices.iter().map(Device::view).collect();
        self.active = self.tracker.choose(&views, self.pin.as_ref());
        let id = self.active?;
        Some(self.devices.iter().find(|d| d.id == id)?.pad())
    }

    /// No controller rumble on Android yet.
    pub fn rumble(&mut self, _pulse: atv_game::ride::impact::Pulse) -> bool {
        false
    }

    pub fn rumble_slot(&mut self, _slot: usize, _pulse: atv_game::ride::impact::Pulse) -> bool {
        false
    }
}

/// Runs `f` with the activity's JNI environment.
#[cfg(target_os = "android")]
fn with_env<T>(f: impl FnOnce(&mut jni::Env) -> jni::errors::Result<T>) -> Option<T> {
    let app = crate::android::activity()?;
    // SAFETY: the activity's Java VM lives as long as the process.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(f).ok()
}

/// `InputDevice.getDevice(id)`: the device, or a null object once gone.
#[cfg(target_os = "android")]
fn input_device<'j>(
    env: &mut jni::Env<'j>,
    id: i32,
) -> jni::errors::Result<jni::objects::JObject<'j>> {
    env.call_static_method(
        jni::jni_str!("android/view/InputDevice"),
        jni::jni_str!("getDevice"),
        jni::jni_sig!("(I)Landroid/view/InputDevice;"),
        &[id.into()],
    )?
    .l()
}

/// `InputDevice.getName()` (for example "Retroid Pocket Controller").
#[cfg(target_os = "android")]
fn device_name(id: i32) -> Option<String> {
    use jni::refs::Reference as _;
    with_env(|env| {
        let device = input_device(env, id)?;
        if device.is_null() {
            return Ok(None);
        }
        let name = env
            .call_method(
                &device,
                jni::jni_str!("getName"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        if name.is_null() {
            return Ok(None);
        }
        // SAFETY: getName returns a java.lang.String.
        let name = unsafe { jni::objects::JString::from_raw(env, name.into_raw()) };
        name.try_to_string(env).map(Some)
    })
    .flatten()
    .filter(|name| !name.is_empty())
}

/// Whether the device is still attached (None: unknown).
#[cfg(target_os = "android")]
fn device_present(id: i32) -> Option<bool> {
    use jni::refs::Reference as _;
    with_env(|env| Ok(!input_device(env, id)?.is_null()))
}

#[cfg(not(target_os = "android"))]
fn device_name(_: i32) -> Option<String> {
    None
}

#[cfg(not(target_os = "android"))]
fn device_present(_: i32) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handheld_buttons_and_sticks_take_the_standard_layout() {
        let mut pads = Gamepads::new();
        assert!(pads.sample().is_none(), "no pad before any input");
        pads.button(7, key::A, true);
        pads.button(7, key::R1, true);
        pads.button(7, key::START, true);
        pads.axis(7, axis::X, -0.75);
        pads.axis(7, axis::Y, 0.5);
        pads.axis(7, axis::Z, 0.25);
        pads.axis(7, axis::RZ, -1.);
        let pad = pads.sample().expect("the pad reported input");
        assert_eq!(pad.id, 7);
        assert_eq!(pad.buttons[0], 1., "A is the bottom face button");
        assert_eq!(pad.buttons[1], 0.);
        assert_eq!(pad.buttons[5], 1., "R1 is the right bumper");
        assert_eq!(pad.buttons[9], 1., "Start");
        assert_eq!(&pad.axes[..4], &[-0.75, 0.5, 0.25, -1.]);
        pads.button(7, key::A, false);
        assert_eq!(pads.sample().unwrap().buttons[0], 0., "released");
        assert_eq!(pads.connected(), vec![("Android controller".into(), 0)]);
        assert_eq!(pads.name().as_deref(), Some("Android controller"));
    }

    #[test]
    fn analog_triggers_pedals_and_the_hat_fill_their_buttons() {
        let mut pads = Gamepads::new();
        pads.axis(3, axis::RTRIGGER, 0.6);
        pads.axis(3, axis::BRAKE, 0.3);
        pads.axis(3, axis::HAT_X, 1.);
        pads.axis(3, axis::HAT_Y, -1.);
        let pad = pads.sample().unwrap();
        assert_eq!(pad.buttons[7], 0.6, "analog R2 from the trigger axis");
        assert_eq!(pad.buttons[6], 0.3, "analog L2 from the brake pedal axis");
        assert_eq!(pad.buttons[12], 1., "hat up");
        assert_eq!(pad.buttons[13], 0.);
        assert_eq!(pad.buttons[15], 1., "hat right");
        pads.button(3, key::R2, true);
        assert_eq!(pads.sample().unwrap().buttons[7], 1., "the digital R2 key");
        pads.button(3, key::DPAD_DOWN, true);
        assert_eq!(pads.sample().unwrap().buttons[13], 1., "a d-pad key");
    }

    #[test]
    fn zero_axes_from_a_non_gamepad_do_not_add_a_pad() {
        let mut pads = Gamepads::new();
        pads.axis(9, axis::HAT_X, 0.);
        assert!(pads.connected().is_empty());
        pads.axis(9, axis::X, 0.4);
        assert_eq!(pads.connected().len(), 1);
    }

    #[test]
    fn auto_follows_the_pad_in_use_and_numbered_buttons_are_extras() {
        let mut pads = Gamepads::new();
        pads.button(1, key::A, true);
        pads.button(1, key::A, false);
        pads.button(2, 188, true);
        let pad = pads.sample().unwrap();
        assert_eq!(pad.id, 2, "the pad with fresh input");
        assert_eq!(pad.buttons[17 + 2], 1., "BUTTON_1 after C and Z");
        assert_eq!(pads.connected().len(), 2);
        pads.set_pin(Some(("Android controller".into(), 0)));
        assert_eq!(pads.sample().unwrap().id, 1, "a pin holds the other pad");
    }
}
