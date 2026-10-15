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
Serve only the built web directory. (Superseded delivery detail: the browser
first read a user-selected ISO locally; the public app now autoloads approved
runtime resources from the separate R2 manifest, see `DEPLOYMENT.md`.)
Remote access requires an existing approved secure path or explicit authorization;
host-local verification is not remote delivery.

## Browser layer boundary (2026-10-15)

Recovered logic and gameplay decisions belong in Rust; the JavaScript layer
issues WebGL calls, manages the DOM and events, and moves bytes. Three paths
that had grown in `web/game.js` now live in `atv-core`:

- **Vegetation queues** (`vegetation_render`): placement registration, the
  00129b70 unsigned16 height resample (sampling the visible dense grid, as the
  former browser did), per-frame cell scan, 0012a1c0 distance phases,
  near-entry signed-short orientation, stale-visibility reset, the bounded
  1,500-plant nearest-depth heap, the descending-depth sort and the instance
  floats for the 0012af80 distant/transition/near queues. JavaScript uploads the
  produced ranges and issues the draws.
- **Ground height and starts** (`Heightfield`, `Terrain::clamped_height`,
  `spawn`): cameras (chase, crash framing, uphill sightline, free flight) and
  the presentation spawn clamp read the dense render grid the terrain mesh is
  built from, with the former interpolation. Gameplay uses the TR2 contact
  plane: race grid and race-reset heights (contact + 2.5 ft) and online slot offsets (8 ft per
  slot) are computed by worker op 140. Grid `RiderSpacing` is read from the
  race mode or scene TOY text by `race::rider_spacing` in op 96.
- **PSM reading** (`scenery_material::describe`, op 145): ordered pass
  splitting, the set-0 texture binding, microprogram name and reflection
  classification, alpha reference/test, explicit GEQUAL cutout values and
  MudAnimator maps. Text semantics reproduce the former regular expressions.

Per-frame helpers cannot wait for the asynchronous worker. The page therefore
instantiates a second instance of the same `atv_web.wasm` on its main thread.
The worker posts its compiled `WebAssembly.Module` with its ready message, so
the module is downloaded and compiled once; a browser that cannot clone modules
receives none and fetches its own copy. This instance holds only presentation
state (dense render heights, ESB placements and queue output). Exports take
numbers and write instance floats into its memory, which JavaScript views as a
`Float32Array`; no JSON crosses per frame. `loadGame` awaits the instance before
terrain exists, so no draw or camera helper can run first. A separate small
WASM artifact was rejected: it would add a second build, cache and release path
for code already in the module.

Host measurements (headless Chromium, Apple Silicon): instantiating the shared
module took 0.1–0.2 ms; render-instance memory was 20.6 MB on ENV01, 20.9 MB on
NAT06, 14.4 MB on FREE04 and 2.9 MB on SX07 (no vegetation), against 20–33 MB in
the worker. The release module grew from 2,694,934 to 2,786,017 bytes (717,470
to 744,022 gzip). In a Node replay of 1,500 frames per map, Rust selection averaged
0.01–0.11 ms per frame against 0.03–0.40 ms for the former JavaScript, and
placement registration 0.3–11 ms against 0.6–36 ms. These are host replay
figures, not phone frame-rate claims.

Equivalence evidence (private, not committed): the former functions were run
beside the Rust path on every Freestyle ecosystem (19 maps, 1,500-frame camera
paths with turns, teleports and skipped frames). Selection counts, order,
phases, orientation and every instance float (355 million) matched bit-for-bit
when both received the same binary32 inputs, including resampled plant heights.
Camera ground heights matched exactly at 380,000 random points inside and
outside every world and on dense sweeps of FREE04 (12.1 million points) and
ENV04 (9.7 million). With production inputs the former code also
read species/placement sizes through decimal JSON, so 1 ulp scale differences
remain; Rust now uses the exact authored binary32 values. PSM descriptions
matched for all 31,590 disc material/mode cases and 13,298 hosted-package cases,
including 160 MX motorcycle materials with 53 mud maps, and for 86 synthetic
edge cases. Race starts on all 32 courses keep identical X/Z and slot offsets;
heights differ by at most 4e-5 ft; all 32 authored `RiderSpacing` values read
identically. Remaining JavaScript decisions not moved in this pass: CPU roster
name selection in `terrainFromContent` and the camera presentation helpers in
`render-support.js`.
