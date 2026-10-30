# Vendored, patched dependencies

Third-party crates the workspace patches through `[patch.crates-io]` in the root
`Cargo.toml`. They are excluded from the workspace, so `make check` does not format, lint
or test them. Each keeps its own license file.

## winit 0.30.13 (Apache-2.0)

The crates.io release, with its examples, tests and docs removed (and their targets taken
out of `Cargo.toml`; the X11 settings test fixture `xsettings.dat` is gone too, so winit's
own unit tests do not build). Only the Android backend changes; every place is marked
`Fury patch`:

- `src/platform_impl/android/mod.rs`: a motion event from a joystick-class source (a
  gamepad's sticks, triggers and hat) is no longer turned into touches. On `ACTION_MOVE`
  it is sent as one `DeviceEvent::Motion` per gamepad axis (X, Y, Z, RZ, RX, RY, HAT_X,
  HAT_Y, LTRIGGER, RTRIGGER, GAS, BRAKE), with the Android axis number as the `AxisId`. A
  gamepad button key (`KeyEvent.isGamepadButton`: key codes 96-110 and 188-203), or a d-pad
  key from a source with the gamepad bit, is sent as `DeviceEvent::Button` with the
  Android key code as the `ButtonId` instead of `KeyboardInput` (repeats dropped). Other
  keys, including Back and keyboard or TV-remote d-pads, are unchanged.
- `src/platform/android.rs`: `DeviceIdExtAndroid::android_device_id` returns the
  `InputDevice` id behind a `DeviceId` (the app reads controller names over JNI).

Used by `crates/atv-app/src/gamepad/android.rs` (research/ANDROID.md, "Controllers").
To upgrade winit, vendor the new release the same way and reapply the marked changes.
