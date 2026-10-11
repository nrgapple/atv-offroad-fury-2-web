# Implementation strategy — 2026-10-11

## Target

ATV Offroad Fury 2, USA v2.00 (`SCUS_972.11`), reconstructed in Rust and delivered
through WebAssembly. Preserve original gameplay, audio, music, progression, camera,
and animation as the baseline. Optional improvements follow measured comparisons.

## Decision

Use incremental native reconstruction backed by exact-build binary analysis and
original assets. Share pure Rust format and simulation code between native tools
and the browser. Recover the resource containers first: without their terrain,
vehicle, animation, tuning, and sound data, a renderer or driving demo would not
establish fidelity to this game.

Do not adopt the experimental C++ PS2Recomp runtime as the browser foundation.
Its checked-out README explicitly reports partial hardware emulation, stubs, and
poor VU/GS performance. The first-game reference's log still reports unresolved
camera/color issues. Both are GPL-3.0 research inputs, not dependencies; their
addresses and function names do not establish identities in our stripped build.
No reference code is being incorporated in this phase.

Static recompilation remains an option for verified routines. A faithful full
R5900/VU/GS/SPU2 emulator would be a separate substantial project. We will not
silently substitute return-zero stubs or invented driving physics and call that
the original game. Unknown executable boundaries remain explicitly provisional.

## First working slice

1. Recover resource directory/block layout from multiple exact-build files.
2. Implement bounded Rust readers and regression tests using synthetic fixtures.
3. Audit the real disc privately, including archive members and audio variants.
4. Compile the same readers to WebAssembly and expose local-file browsing and
   sound playback without bundling or uploading game data.
5. Establish a reproducible stripped-executable reconnaissance report, with
   candidate calls clearly distinguished from verified functions.

This is a content/reverse-engineering workbench, not playable gameplay.

## Following acceptance gates

- Recover terrain, collision, meshes, materials, skeletons, and animation; render
  an actual track and quad with an original-aspect camera.
- Recover update order, units, input mapping, suspension/contact, drivetrain,
  rider balance, camera, and race rules from code plus original execution traces.
  Deliver one controllable original track/vehicle before expanding content.
- Compare fixed input replays against original execution: positions, velocities,
  contact states, lap timing, jumps/landings, camera, and animation transitions.
  A reference PS2 or correctly configured emulator and legitimately supplied BIOS
  may be needed; none is currently established by setup.
- Connect original soundtrack, looping engine layers, pitch/volume curves,
  effects, spatialization, dialogue, and FMV audio. Decoding samples alone does
  not reproduce the original mixer, interpolation, reverb, or scheduling.
- Verify menus, race modes, progression, saves, and gamepad/keyboard controls.
- Measure startup, streaming, memory, audio underruns, and frame times on desktop
  and mobile browsers; publish no frame-rate claims before measurement.

## Enhancements after the baseline

Keep original presentation available. Add genuine 16:9 projection with correct
HUD layout and culling, not a stretched 4:3 image; support fullscreen independently.
Increase rendering resolution before replacing texture/material detail. Higher
resolution cannot recover detail absent from the original assets. Optional modern
lighting, shadows, materials, animation, and physics must preserve baseline replay
tests. More realistic suspension/grip/weight transfer should retain the game's
accessible arcade response and be switchable rather than rewriting baseline tuning.

## Interface and input requirements (user steering, 2026-10-11)

All authored UI should evoke the PlayStation 2 era: dark cobalt space, luminous
panels, compact typography, and restrained motion, respecting reduced-motion
settings. Recovered original menus remain the fidelity reference.

Support browser-exposed Bluetooth/USB gamepads, connection changes, dead zones,
two sticks, face/shoulder/trigger buttons, and remapping for nonstandard layouts.
Compatibility is bounded by what each browser/OS exposes through Gamepad API;
"any Bluetooth controller" cannot be guaranteed at the app layer.

iOS touch: floating left thumb zone combines steering with throttle/brake;
right thumb zone recognizes directional trick swipes, with separate accessible
buttons for held actions. Test simultaneous touches, cancellation, rotation,
safe areas, focus loss, and audio activation. Input tests are not riding gameplay.
Proposed mappings stay labeled until the exact game's original actions are verified.

## Privacy and delivery

Source media and recovered content stay under ignored `private/`; generated
analysis containing game data is private as well. Share only tools, synthetic
tests, and format/engineering notes. Build artifacts use the configured SSD.
Serve only the built web directory. The browser reads a user-selected ISO locally.
Remote access requires an existing approved secure path or explicit authorization;
host-local verification is not remote delivery.
