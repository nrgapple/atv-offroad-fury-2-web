# Follow camera recovery

The browser follow camera now runs in Rust from the local disc's `FollowCamPresets.pb`. The exact USA v2.00 loader `003029a0`, constructor `001f2d20`, follow update `00319e10`, look target `0031a5e0`, camera position `0031aa70`, terrain lift/clamp `00319d50` / `00319c30`, and filters `00302600` establish this implementation. The original preset file is served through the separate authorized R2 runtime manifest; it remains outside GitHub source and CI.

The loader preserves the seven constructor defaults, converts authored heading/elevation angles from degrees, and chooses the original single, horizontal split, vertical split or quarter-screen sections. All 28 entries were decoded privately. The browser starts with the single-screen chase preset. The Camera button, default C key and default controller right-stick click cycle through the original eligible presets. Keyboard/controller shortcuts and right-stick orbit are remappable browser adaptations; the inspected manual controls page does not establish an original camera shortcut. See [control audit](CONTROLS.md).

Recovered behavior includes speed-dependent elevation, reverse distance/target changes, yaw-rate camera swing, forward prediction, falling look-ahead, terrain clearance and independent position/target filters. The original actor-binding call establishes a 70 MPH speed reference, converted with its original constant. Wheel contact for reverse detection comes from the actor's any-wheel-grounded flag. A final terrain clamp affects the rendered camera without feeding back into the filter's stored history.

## Validation and limits

A private finite-binary32 instruction interpreter ran the original executable bytes, including branch delay slots and selected VU operations:

- 600 filter cases, 120,000 original instructions: all eight output float bit patterns match.
- 600 initial follow-camera frames spanning all seven presets, reverse motion, falling and orbit: maximum absolute difference below 0.000028 across position, target and field of view.
- 1,260 sequential frames across seven presets with a sloping floor, changing velocity, yaw and contact: 1,575,771 original instructions; maximum absolute difference below 0.0000053 across the same outputs.

These are component checks, not PS2 hardware or full-game replay equivalence. Scene/body access is supplied by explicit hooks. Trigonometry still uses host sine/cosine instead of the original lookup table, and VU exceptional arithmetic is not fully emulated. Terrain queries are integrated. The recovered single-player chase setup disables its optional object query; the separate wreck camera uses direct scene candidates as described below. Browser right-stick/mouse orbit scaling is provisional. Gameplay touches never orbit the camera; the left touch steers/leans and the right touch drives/braces. Projection recovery (`0011b070` → `0013bdb0` → `00325e80`) establishes horizontal field of view. The browser uses the recovered FOV at a 4:3 reference aspect and preserves its vertical framing while extending the horizontal view for widescreen. This widescreen framing is a browser adaptation. Original render-distance/LOD policies remain incomplete.

Private reproduction scripts and results live under `work/physics/camera_*` and `private/reports/`; extracted presets and decompiler output remain ignored.

### Portrait presentation

Portrait view now pulls the recovered chase position back by up to 35%, raises
the sightline by up to 0.12 radians and reduces focal length by up to 12%. The change
blends from square to narrow portrait; landscape retains the exact recovered
position, target-derived pitch and 85-degree native horizontal FOV projection,
with the later uphill sightline lift applied as a separate default-chase browser
adaptation on supported ground/ramp slopes.
The renderer applies this from each untouched native camera frame, so rotating
the device does not compound offsets. `tools/test_portrait_camera.mjs` verifies
portrait→landscape→portrait behavior in the live renderer; before/after 390×844
Caribou captures were inspected. This is an explicitly requested mobile adaptation,
not a recovered original PS2 camera preset. The original single-player chase setup disables object obstruction queries; the
portrait adaptation does not enable a different collision policy.

The host-only uphill camera checkpoint records the ordinary Caribou route and
measured pitch/lift in [iOS fullscreen and camera evidence](IOS_CAMERA_CRASH.md).
It is not evidence of original uphill-camera parity or physical iOS behavior.

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
interpreter is not PS2 hardware. The secondary water-height provider is not yet
connected. Full coupled original body/camera replay remains a separate parity gap.
Private probes/results are in `work/wreck-camera/`; no original instruction bytes
or pseudocode enter source control. Browser/public verification is recorded in the
dev log only after the corresponding tests pass.

### Wreck-camera motion obstruction (next host checkpoint)

`0031c880` creates a one-ray type-3 query for the wreck camera. After filtering and
terrain clearance, `0031d190` casts from the previous rendered position ten feet
along the normalized desired movement direction. The ray remains ten feet long
even if this frame's movement is shorter. A collision latches camera `+334`;
`0031cc90` then preserves the rendered position for the rest of that activation.
Target, field of view and internal position filters continue advancing. Entering
the next wreck camera clears the latch. This differs from a generic camera that
slides continuously around obstacles.

The authored camera now implements this guard against loaded static props selected
through its independent scene query. Its native query has vegetation expansion
disabled; ordinary enabled static props are eligible with default category masks.
Original custom masks/exclusions and dynamic scene actors remain incomplete. Water
clearance remains pending the original wave field.

Private instruction probes cover 600 actual `0031d190` executions (51,900
instructions): all 3,600 ray coordinate bit patterns match Rust, including zero
movement and the collision flag branch. The finite VRSQRT comparison uses a direct
reciprocal-square-root result without an intermediate rounded binary32 square
root; this is interpreter evidence, not PS2 VU hardware verification. A further
240-frame `0031cc90` sequence executes 390,723 instructions with a synthetic
overhead-plane scene hook. The query runs once before the latch, the output
position stays fixed, and Rust target/FOV differ by at most 0.00000763. A coupled
Rust regression uses an actual type-0 triangle query, confirms the ten-foot
lookahead, and preserves the latch after removal while filters continue unchanged.
Probe sources and data stay under ignored `work/camera-obstruction/`.


### Direct scene candidate integration

The wreck camera now runs `SceneCollision::direct_camera_candidates` against the
combined ordinary/vegetation partition. Native `0031d190` calls `0011f2c0` with
its cache interval argument zero. The original type-3 prepare `00327360` computes
segment AABB center/half extents and their diagonal sphere; its virtual bounds
accessor supplies that box to the original partition iterator. Ordinary candidates
must have zero sphere separation (inclusive overlap), then pass current
`0011eeb0` eligibility before the geometry test. Candidates retain iterator order.
This path has no retained quad/rider membership, distance sort, forty-foot cache
radius, or 256-entry collector limit. Camera queries neither refresh nor alter the
shared vehicle/rider cache.

The former camera tested every static placement. That omitted native partition,
sphere and eligibility gates. The live WASM camera call now uses the independent
scene path. Loaded ordinary placements still supply default enabled eligibility;
native custom category masks, object inclusion/exclusion lists and dynamic
registrations lack transport. The API accepts an explicit eligibility predicate
for verified callers. It excludes vegetation expansion using the camera's native
query bit, rather than assigning visual plants invented collision shapes.

An original-instruction comparison covers 120 oblique ten-foot queries through
30 mixed scenes and 1,930,484 instructions. Actual registration, type-3 bounds
preparation, partition traversal, deduplication, sphere distance and ordinary
eligibility execute. A no-hit geometry service records the selected object order;
physical geometry is independently tested by the existing segment probes. All
candidate sequences, query centers and radius bits match the Rust scene bridge.
Scene registration uses synthetic bounds and region links, including enabled and
disabled ordinary objects and nearby plant nodes. This does not establish complete
loaded-scene or hardware parity.

A separate 100-case `00319b70` caller-prefix replay executes 3,800 instructions
and confirms that normal follow-camera initialization supplies zero for optional
object-query setup, even with varied incoming values. Its vtable `+28` resolves to
`00319c30`, the terrain/water clearance routine. The ordinary single-player chase
path was therefore left unchanged. This does not establish other original camera
modes or custom configurations.

Two authored regressions require direct selection to exceed 256 ordinary objects,
retain inclusive exact-sphere touching, reevaluate supplied eligibility, exclude
plants and preserve an already populated vehicle cache. The live-camera regression
also makes the explicit scene authoritative over the old full-static fixture list
and checks obstruction latching and retained diagnostics. Seven camera and ten
scene-collision tests pass; the atv-web bridge crate passes native Cargo checking.
WASM build and release/browser verification are separate gates.

WASM camera frames expose `scene_query` with `direct`, `queried_this_step`,
`center`, `radius` and `static_candidates`. After obstruction latches,
`queried_this_step` is false and the remaining fields describe the last actual
query; they do not claim another query ran. Private probes and reports live under
`work/camera-scene-probe/` and `private/reports/camera-scene-*.json`. Reproduce with:

```sh
python3 work/camera-scene-probe/oracle.py
python3 work/camera-scene-probe/follow_setup.py
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo run --release \
  --manifest-path work/camera-scene-probe/Cargo.toml
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo test --locked \
  -p atv-core camera::tests
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo test --locked \
  -p atv-core scene_collision::tests
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo check --locked \
  -p atv-web
```
