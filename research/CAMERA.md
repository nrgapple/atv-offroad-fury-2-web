# Follow camera recovery

The browser follow camera now runs in Rust from the local disc's `FollowCamPresets.pb`. The exact USA v2.00 loader `003029a0`, constructor `001f2d20`, follow update `00319e10`, look target `0031a5e0`, camera position `0031aa70`, terrain lift/clamp `00319d50` / `00319c30`, and filters `00302600` establish this implementation. The original preset file is served through the separate authorized R2 runtime manifest; it remains outside GitHub source and CI.

The loader preserves the seven constructor defaults, converts authored heading/elevation angles from degrees, and chooses the original single, horizontal split, vertical split or quarter-screen sections. All 28 entries were decoded privately. The browser starts with the single-screen chase preset. The Camera button, default C key and default controller right-stick click cycle through the original eligible presets. Keyboard/controller shortcuts and right-stick orbit are remappable browser adaptations; the inspected manual controls page does not establish an original camera shortcut. See [control audit](CONTROLS.md).

Recovered behavior includes speed-dependent elevation, reverse distance/target changes, yaw-rate camera swing, forward prediction, falling look-ahead, terrain clearance and independent position/target filters. The original actor-binding call establishes a 70 MPH speed reference, converted with its original constant. Wheel contact for reverse detection comes from the actor's any-wheel-grounded flag. A final terrain clamp affects the rendered camera without feeding back into the filter's stored history.

## Validation and limits

A private finite-binary32 instruction interpreter ran the original executable bytes, including branch delay slots and selected VU operations:

- 600 filter cases, 120,000 original instructions: all eight output float bit patterns match.
- 600 initial follow-camera frames spanning all seven presets, reverse motion, falling and orbit: maximum absolute difference below 0.000028 across position, target and field of view.
- 1,260 sequential frames across seven presets with a sloping floor, changing velocity, yaw and contact: 1,575,771 original instructions; maximum absolute difference below 0.0000053 across the same outputs.

These are component checks, not PS2 hardware or full-game replay equivalence. Scene/body access is supplied by explicit hooks. Trigonometry still uses host sine/cosine instead of the original lookup table, and VU exceptional arithmetic is not fully emulated. Terrain queries are integrated; the optional camera clearance query against objects remains incomplete. Browser right-stick/mouse orbit scaling is provisional. Gameplay touches never orbit the camera; the left touch steers/leans and the right touch drives/braces. Projection recovery (`0011b070` → `0013bdb0` → `00325e80`) establishes horizontal field of view. The browser uses the recovered FOV at a 4:3 reference aspect and preserves its vertical framing while extending the horizontal view for widescreen. This widescreen framing is a browser adaptation. Original render-distance/LOD policies remain incomplete.

Private reproduction scripts and results live under `work/physics/camera_*` and `private/reports/`; extracted presets and decompiler output remain ignored.

### Portrait presentation

Portrait view now pulls the recovered chase position back by up to 35%, raises
the sightline by up to 0.12 radians and reduces focal length by up to 12%. The change
blends from square to narrow portrait; landscape retains the exact recovered
position, target-derived pitch and 85-degree native horizontal FOV projection.
The renderer applies this from each untouched native camera frame, so rotating
the device does not compound offsets. `tools/test_portrait_camera.mjs` verifies
portrait→landscape→portrait behavior in the live renderer; before/after 390×844
Caribou captures were inspected. This is an explicitly requested mobile adaptation,
not a recovered original PS2 camera preset. Original object camera clearance is
still not connected, so further obstruction testing remains relevant.

## Wreck camera (priority-parity host checkpoint)

The Rust camera now switches from the follow view to the original two-actor wreck
camera, then resets to follow on recovery. Exact-build `001f33f0` binds the quad and
rider; `0031d2f0`/`0031d640` seed its history from the outgoing view. `0031cc90` and
its helpers preserve the distance-dependent rider weighting, 80-foot secondary
actor cutoff, low-speed vertical-velocity suppression, retained heading, two-second
transition, original angle convergence, 30–90-foot distance limits, 25–85-degree
FOV limits, 1.2-second position/target filters, 0.4-second FOV filter, and five-foot
terrain clearance. Angle convergence remains per original simulation update,
not a rendering-frame effect; the browser calls it at 60 Hz.

A 240-frame synthetic sequence executes 396,390 original instructions, including
both actors, stopped vertical motion and separation beyond 80 feet. Its Rust
position/target/FOV outputs differ by at most 0.000011265 feet/degrees. Actor
transforms, terrain height and host trigonometry are explicit oracle hooks; the
interpreter is not PS2 hardware. The camera-position obstacle callback and secondary
surface-height provider are not connected. Attached-rider collision-induced motion
and the full original body-transform sequence remain separate parity gaps.
Private probes/results are in `work/wreck-camera/`; no original instruction bytes
or pseudocode enter source control. Browser/public verification is recorded in the
dev log only after the corresponding tests pass.
