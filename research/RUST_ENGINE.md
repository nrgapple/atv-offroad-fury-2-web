# Shared Rust engine (web, macOS, Windows, Linux)

Goal: one Rust game that builds for the browser and as standalone desktop apps, so a
change lands on all four at once. The JavaScript host (rendering, audio, menus, HUD,
input, effects and game flow in `web/*.js`) is migrated into Rust crates; the browser
keeps only a tiny loader plus the landing page, dev log, service worker and analytics.
The original game stays the baseline: every migrated piece is compared against the
current JavaScript build and, where the JavaScript cites an original routine, against
that recovered behavior.

## Crates

| Crate | Role |
| --- | --- |
| `atv-core` | Simulation and original formats (unchanged) |
| `atv-content` | Hosted content package reader (`manifest.json` + SHA-256 chunks); HTTP on the web, a package directory on desktop |
| `atv-game` | Platform-independent game layer: venue ids/archives (`launch.rs`, from `web/launch.js`), venue loading (`venue.rs`, from `terrainFromContent`), presentation terrain grid (`terrain_mesh.rs`, from `buildTerrainMesh`), the Freeride ride (`ride/`: loading, fixed-step simulation, default bindings, chase presentation) |
| `atv-render` | wgpu renderer: Metal (macOS), DX12/Vulkan (Windows), Vulkan (Linux), WebGL2 (browsers). WGSL ports of the `game.js` shaders |
| `atv-app` | One winit application for every target: `fury` desktop binary and the `engine.html` WebAssembly module |

`atv-web` (the current JavaScript game's Wasm ABI) is untouched until each subsystem
reaches parity; then the JavaScript and its ABI operations are deleted.

## Migration rule

New work lands in the Rust engine only; the JavaScript game is frozen as the parity
reference until the switch-over (AGENTS.md "Rust only for new work").

The Rust engine runs beside the live game at `/engine.html` until it matches. A
subsystem moves when `tools/compare_engine.mjs` shows the same frame from the same
camera in the JavaScript game, the Rust engine in the browser and the native build.
`game.js` publishes its render camera as `atvTerrainStatus.renderCamera` for this.
When the Rust engine reaches parity, `play.html` switches to it and the JavaScript
equivalents are removed.

## Fidelity and performance requirements for continuation

The target remains the recovered behavior of ATV Offroad Fury 2 USA v2.00
(`SCUS_972.11`), as established in [IMPLEMENTATION.md](IMPLEMENTATION.md). The
current JavaScript game is the migration reference for working integration and
platform adaptations. Its existing approximations remain documented gaps; matching
them is evidence of migration parity, not proof of PS2 equivalence. Preserve
original tuning, update order, fixed-step cadence, animation/camera transitions,
materials and sound decisions where recovered evidence exists. Correct known
fidelity gaps with that evidence and record the intentional difference from the
JavaScript reference.

Retain the performance improvements already validated in the browser port. Their
implementation can change in Rust, but their benefit and observable behavior must
survive. [PERFORMANCE.md](PERFORMANCE.md) records measured examples: conservative
culling without visible draw-order changes, terrain buffer reuse and partial
updates, unchanged-state HUD redraw avoidance, and inactive single-player render
sleep while preserving multiplayer clocks. The migration also needs to retain
bounded simulation batches and input/event ordering, shared resource caches,
parallel loading and economical audio/resource reads where the current port uses
them. These are acceptance requirements, not a claim that every optimization has
already reached the Rust engine.

For each completed subsystem checkpoint:

- Identify the JavaScript paths being replaced, the recovered routines or resource
  evidence, and the existing optimization/adaptation behavior to preserve.
- Compare matched inputs, simulation steps, cameras and graphics settings across
  the JavaScript game, Rust browser engine and native engine using the applicable
  replay, image, audio and input checks. Keep known differences explicit.
- Measure the affected costs against the same baseline: frame-time distribution
  and stalls, input responsiveness, memory/allocation or upload volume, startup
  and resource loading, and audio continuity as applicable. Removing JavaScript
  or moving simulation onto the main thread is not itself a performance result.
- Retain mobile touch ownership/cancellation, portrait and safe-area behavior,
  focus/pause/resume handling and audio activation. Keep the authored input
  adaptations documented in [TOUCH.md](TOUCH.md) and [CONTROLS.md](CONTROLS.md).
- Label host viewport simulations separately from physical iOS/Android results.
  Validate the affected mobile path before claiming mobile performance parity;
  document device coverage still missing.

Original remains the baseline preset. Existing optional graphics, camera, crash
and physics enhancements remain explicit options, with baseline behavior checked
separately. A performance shortcut that changes original behavior or visible output
must be recorded and exposed as an explicit quality/enhancement choice, not silently
substituted for the baseline. Keep the current game available until the replacement
passes its relevant behavior and performance gates.

## Feasibility checkpoint (2026-10-20)

Scope: ENV01 terrain only (base layers, detail blend, GS fog, fine patch, culling), a
free camera (WASD/arrows, Q/E, drag to look) and frame statistics. No sky, scenery,
vehicle, rider, audio, HUD or menus yet.

Observed:

- Parity. Same camera as the JavaScript game at the ENV01 start, 1280x720, terrain-only
  regions: mean absolute difference 0.01-0.02 of 255 against the JavaScript frame and
  0.00-0.02 between the Rust browser and native frames. Remaining differences are
  scenery the Rust engine does not draw yet.
- Browser. Chromium (ANGLE/Metal, Apple M4) renders about 78 fps uncapped headless,
  0.5 ms CPU per frame; WebKit renders at the 60 Hz cap. Not yet measured on phones.
- Download. `atv_app_bg.wasm` is 4.66 MB raw, 0.87 MB brotli, against 0.67 MB brotli
  for `atv_web.wasm` plus about 0.25 MB gzip of game JavaScript today. It grows as more
  of `atv-core` is linked and the JavaScript shrinks; the estimate at parity is
  1.2-1.4 MB brotli.
- Desktop. macOS (Metal) runs and captures (`fury --capture`). Linux x86_64/arm64 and
  Windows x86_64 (GNU) cross-build in Docker from macOS; they have not been launched.

Limitations: phone frame rate, Windows/Linux runtime and Firefox are unverified. Desktop
builds read a package directory (`--content` or `FURY_CONTENT`) or, in release archives, a
packed content file (see Desktop packaging below).

## First ride checkpoint (2026-10-20)

Scope: ENV01 Freeride on the default Garage selection (quad 1, male rider) in the
native build and `/engine.html`.

Ported:

- Loading (`atv-game/src/ride/load.rs`): quad mesh, physics, chassis and actor
  collision rays, terrain materials TOY, scene COL placements with mud flags,
  boundary/partition config, vegetation partition (ESB), water field, follow-camera
  presets, rider mesh, the 78 riding/stunt/wreck/landing motions and both models'
  original material textures (single-pass `baseTextures`). Mirrors
  `terrainFromContent`/`startSimulation` and atv-web ops 2/3, 11, 20-22, 26-27,
  44-45, 48-49, 51-53, 64-65, 89, 140 and 145, calling atv-core directly.
- Simulation (`ride/mod.rs`): op 23 placement, op 24 vehicle/rider/wreck step,
  op 46 follow and wreck camera, op 47 reset/cycle, in the op 50 batch order at the
  fixed 1/60 s; the browser's frame accumulator (dt capped at 50 ms, accumulator at
  0.1 s, at most four steps per frame, one control sample per batch).
- Input (`ride/bindings.rs`, `atv-app/src/gamepad.rs`): `web/bindings.js` keyboard
  defaults and the Fury Remastered controller preset (15% dead zone, linear),
  keyboard brake-then-reverse intent, preload hold/release, keyboard trick
  directions, Reset (R/Backspace), Camera (C), orbit (Z/X, right stick, mouse
  drag), Pause (Esc/P). Controllers use gilrs natively and in the browser.
- Camera presentation (`ride/view.rs`): `chasePresentation` with the 4:3 vertical
  framing on wide screens and the portrait pull-back/widening, plus the speed-rush
  FOV kick. The fine terrain patch follows the rider (48-cell rule). Chase camera
  Assisted (`UphillSightline`: up to 0.52 rad orbit about rider height and 24%
  pull-back on uphill grades sampled 8/20/36 ft ahead on the visible ground) and the
  default Tight crash camera (`crashSubjects`/`crashPresentation`: Crazy/Gorn crashes
  framed on the torso/pelvis and chassis centres, 16-32 ft, pitch -0.55..-0.22) follow
  render-support.js; the unit test checks both against its values. Evidence (host
  Chromium, `compare_engine.mjs ride env03 gorn-script OUT --crash gorn`): Tight held
  crash frame 0.47 (1280x720) / 0.07 (390x844) whole-frame difference, Original 1.70 /
  1.01; `tools/test_engine_cameras.mjs` (ENV05 climb: 305 of 901 frames orbited, max
  0.376 rad; Original none). Held replays start the sightline at rest, so an
  Assisted held frame differs from the browser's, whose sightline eased while the
  script played in real time (ENV05 at 460 steps: 0.013 rad there, 0 here).
- Drawing (`atv-render/src/vehicle.rs`, `vehicle.wgsl`): rigid-node quad linkage
  and rider skeleton (32 node uniforms), original textures, the browser's
  provisional ambient/sun vehicle lighting and per-pass GS fog.

Observed (`node tools/compare_engine.mjs ride env01`, which replays one script
through the JavaScript game's own op 50 batches, `fury --trace` and `/engine.html`):

- Physics. Default script (600 steps: settle, gas, both turns, brake into reverse):
  no step diverges by more than 0.001 ft; largest origin difference 0.00024 ft,
  orientation 2e-5, camera 0.0005 ft. A 930-step script with two wrecks and
  recoveries matches exactly in origin and orientation (camera within 0.001 ft).
  The browser (wasm) and native builds end within 0.00002 ft.
- Pixels (Original preset, canvas only): quad/rider screen box mean absolute
  difference 1.3 (1280x720) and 1.9-2.1 (390x844) of 255; whole frame 6.7 and 12.1.
  The rest is sky, scenery and vegetation the Rust engine does not draw yet.
- Cost. 0.04 ms per native fixed step (Apple M4). `atv_app_bg.wasm` is 5.63 MB
  raw, 1.05 MB brotli.

Limitations: no audio, HUD, minimap, menus, wheel/ambient effects, water surface,
speed-rush screen pass, remapping or touch controls; optional Crazy/Gorn crashes,
flips, gear and paint, motorcycles, races, multiplayer and trains are not ported.
Controller input has not been tried with physical pads. Linux builds need
`libudev-dev` for gilrs.

## Static world checkpoint: sky, scenery, vegetation (2026-10-20)

Ported under the Original preset, from the `game.js` scene shader and loaders:

- Sky (`atv-game/sky.rs`, `atv-render/sky.rs` + `sky.wgsl`): SCN `CubeFile` layout
  (`atv_core::sky`, 00124010), IPUM tiles, UVs .03125-.96875 (00125960), drawn first,
  unfogged, xyww with depth testing off. The Enhanced/High smooth sky (deblocked tiles
  and Catmull-Rom bicubic) is behind `Graphics::sky_smooth` (`--graphics enhanced`).
- Static scene models (`atv-game/materials.rs`, `scenery.rs`; `atv-render/scenery.rs` +
  `scenery.wgsl`): SCN placements of root-relative PSGs (`decode_psg_for_scene`), every
  ordered PSM pass (`scenery_material::describe`) with its PST/ATM/TGA texture, prelit
  vertex colour, provisional prop lighting for lit models, per-pass fog, blend/additive,
  depth write, LEQUAL for later passes, GEQUAL cutouts with the FB_ONLY alpha-failure
  pass, UV scroll and ATM clocks (60 Hz steps, as the simulation batch), local-plane
  reflection maps and hidden stadium camera flashes. Weather selection (00208cc0) removes
  the inactive weather model. Draw order is scene order, culled per object at the fog
  end; the depth range extends by the largest object as `matrix()` does.
- Vegetation (`atv-game/vegetation.rs`; `atv-render/vegetation.rs` + `vegetation.wgsl`):
  ESB species meshes and the EcoSystemCode2D atlas; `atv_core::vegetation_render`
  selects and orders instances (distant billboards, depth-disabled transition, near
  material queues) with the per-instance alpha reference and the biased-float vertex
  lighting. Like the browser, the frame serial stays 0, so 0012a1c0's stale-visibility
  reset only applies to a plant's first selection.

Parity (`tools/compare_engine.mjs`, Original preset, JavaScript actors hidden and clocks
held via the test-only `window.atvParityView`; mean absolute difference of 255,
JavaScript vs Rust browser, start camera unless noted): ENV01 0.04 (sky view 0.06, high
forest view 0.16), SX01 0.05, NAT01 0.39 (the water surface, not yet ported; 0.01-0.19
elsewhere), ENV08 0.03 (no train yet), ENV03 0.04, FREE01 0.03. Rust browser vs native:
0.00-0.01. Residuals are triangle edges and, in the forest view, near plant
orientations captured on entry (camera history differs).

Cost (Chromium ANGLE/Metal, Apple M4, 1280x720, uncapped): CPU 0.50-0.90 ms per frame
at 100-282 draws (terrain only: 0.5 ms, about 11 draws); 76-85 fps. Stadium SX01 is the
heaviest at 282 draws. Each scenery draw rebinds static uniform slots by dynamic offset
(no per-draw uniform uploads); vegetation is one instanced draw per material queue.
`atv_app_bg.wasm` is 4.92 MB raw, 0.93 MB brotli (+0.06 MB). Phones are not measured yet.

Water and train (second checkpoint):

- Water (`atv-game/water.rs`, `atv-render/water.rs` + `water.wgsl`): the WTT live
  height field (`water_animation::LiveField`, advanced and render-prepared once per 60 Hz
  step), height/derivative atlases and environment map from `water_render`, drawn as the
  viewport-sized projected grid of `water.js` (00190aa0 strip/row counts), with the
  Enhanced/High Fresnel behind `Graphics::fresnel`. Parity: NAT01 start 0.06 (was 0.39),
  NAT01 overview 0.15, ENV05 0.04, ENV07 0.12, FREE04 0.13, ENV04 0.02 (native).
- ENV08 train (`atv-game/train.rs`, `atv-render/train.rs` + `train.wgsl`): the
  `atv_core::train` runtime (route/MOT, 0011c840) stepped at 60 Hz, cars posed by their
  bind nodes and the locomotive wheels by the Rust wheel matrices. Two views beside the
  train: 2.19 and 1.04, all of it the locomotive smoke (an ambient effect, not yet ported).
  Mesh detail (2026-10-22, user report of a low-poly train): both ENV08 archives
  (`E08_FR.RES`, `E08_NAT.RES`) carry each car once, as a single-LOD PSG under
  `geometry/animgeometry/` (threshold f32::MAX): locomotive 451 triangles, tender 66,
  coal cars 262/284, flatbeds 426/293, mail car 167, plus `anim/loco_wheels.psg` 48. No
  hi/lo variants or other train meshes exist in the extracted content, every car group
  draws (no reflective passes are skipped), and the Rust draw matches the browser's
  apart from effects. The low-poly look is the original's own geometry; nothing to
  switch to.
- The GL backend creates a cube map for any texture whose layer count is a multiple of
  six, which WebGL then rejects for 2D-array views: ENV04's 102 terrain layers rendered
  garbage in the browser. Such arrays now get one unused layer (`texture::array_layers`).
- The live water field and train runtime are `Rc<RefCell<..>>` on `Venue`, so the
  physics step can share them. Until the ride loop steps them, `SceneRenderer::advance`
  does, and the train is stepped without collision heights (`step`, not
  `step_with_scene`). `atv_app_bg.wasm`: 5.06 MB raw, 0.96 MB brotli.

Ambient effects (third checkpoint; `atv-game/ambient.rs`, `atv-render/ambient.rs` +
`ambient.wgsl`): the placed TOY emitters simulated by `atv_core::ambient_effects`
(14 free-ride venues), stepped at 60 Hz after the train publishes its car poses (the
locomotive's Smoke.slt). CPU sprites are culled and sorted back to front each frame and
drawn as alpha and additive instanced families with the 001451a0 near fade; ENV05's
fountain births fill a 4,800-slot GPU ring aged in WGSL. Parity: ENV08 beside the
locomotive 0.06 (was 2.19), ENV05 fountain 0.13, ENV02 campfire 0.03, FREE06 grill smoke
0.03. A second ENV08 view differs by 0.74 in near plants whose orientation the browser
captured during the previous view (camera history).

Snow and camera flashes (fourth checkpoint): `atv-game/snow.rs` + `atv-render/snow.rs`
port `weather.js` (source SnowEmitter births from `atv_core::weather::Snow` into a
10,800-slot GPU ring aged in WGSL, unfogged). Only ENV01 races select Snow, so the
engine gained a static-world race-course load (`--race`, `?race=1`; ride disabled) and
`compare_engine.mjs --race`. Snow now steps around each simulation step's camera, as the
browser's op 50 batch does: the ride's follow camera (`Riding::step_cameras` ->
`SceneRenderer::set_weather_view`) or, for a race-course load, the pre-race shot camera
(`atv-game/intro.rs`, `atv_core::race_intro`), which also drives the free view and is
stepped once at load like startSimulation's first op 46 request. ENV01 race intro: 3.1/255
(was 5.5); the flakes now match and the residue is near plants whose orientation the
browser captured along the moving intro camera (the capture pins the view).
Stadium camera flashes now run on the browser's seeded `race_crowd::Flashes` clock
(0020ee10, about one of SX01's 187 lit at a time); `flash_burst` takes race events.

Limitations: reverse-looping crowd ATMs keep their first frame (as in the browser);
anisotropy is applied at load only; the train is stepped without collision heights; race
gates, grids and race-event flash bursts belong to the race port. The vehicle pass draws
between vegetation and water, as `game.js` does.

## Shared audio recovery checkpoint (2026-10-21)

The Rust ride now owns the `web/audio.js`, `quad-sounds.js` and `music.js` sound
path: original sample loading/ADPCM decoding, engine and impact decisions, chassis,
rider vocals, surface drive sounds, placed world sounds, a stereo software mixer
and soundtrack selection. Decisions use the recovered `atv-core` routines used by
operations 33, 77 and 173. CPAL supplies CoreAudio/WASAPI/ALSA output on desktop and
Web Audio output in browsers. Desktop soundtrack playback streams decoded VAG
blocks; the browser retains the hosted AAC media stream to avoid downloading and
decoding whole tracks. Original sound is the default; authored Enhanced echo stays
an explicit option.

Continuation fixes preserve existing browser behavior and resource savings:

- The first ride frame no longer waits for the sound bank. Browser loading runs
  asynchronously afterward. An earlier actual click/key/touch gesture opens and
  resumes the same output and silently primes the same media element that the
  eventual sound bank uses. Completed touches retry activation for mobile policies
  that reject touch-start playback; cancellation does not. Native bank loading
  still runs synchronously after that first frame.
- Encoded rider vocals decode only when selected, with a 24-entry PCM cache. In the
  measured ENV01 male bank this avoids 12,966,624 bytes of vocal PCM at startup;
  selecting all 51 cues leaves 8,558,608 bytes in the cache. First-decode median was
  0.735 ms, maximum 3.989 ms on the host Mac. This is not a physical-phone result;
  cue decoding remains synchronous while holding the mixer lock.
- Mixer scratch buffers are reused across callbacks. Soundtrack maintenance runs
  while the simulation is paused. Independent focus, visibility and suspension
  state freezes simulation and audio while away, including content/bank completion
  after focus loss. Returning to focus resumes an already activated output; it
  never activates untouched audio. A rejected/interrupted browser media play
  request can recover on subsequent activation or a bounded retry without
  skipping the selected song.

Verification: the content-backed `tools/test_rust_audio.mjs` passes Chromium and
WebKit on the host. It delays content until after actual canvas focus/blur, blocks
bank reads while requiring a rendered ride, and checks inactive late sound
installation, engine voices, soundtrack advance while paused and neutral recovery.
Controlled DOM visibility and persisted pagehide/pageshow fixtures cover the winit
occlusion and back/forward-cache paths; a deliberately suspended AudioContext
checks previously activated output recovery. These fixtures are not physical
mobile backgrounding measurements. `tools/test_rust_audio_activation.mjs` separately
checks a sole early click or completed touch with blocked bank reads, strict
per-element activation and a rejected first song play. Mixer tests cover
pause/focus behavior and streaming resampler continuity; vocal-cache tests cover
lazy decoding, capacity and eviction.

The Original ENV01 600-step JavaScript/native audio comparison matches all 12
engine sample-length signatures, all 10 drive and three chassis voices; median level
difference is 0.209 dB and p90 1.048 dB. Command timing still differs by 1–7 simulation
steps because the JavaScript reference schedules per delivered batch. Native mixer
cost is 0.0065 ms per 60 Hz frame in this host replay. These measurements compare
the authored implementations; they do not establish original PS2 SPU equivalence.
Enhanced was measured separately: 12/12 engine signatures, 3/3 chassis and 9/10
drive matches within the 2.5-step voice-start tolerance, median 0.169 dB and p90
0.935 dB. The final Rust drive restart at step 599 has no corresponding JavaScript
start in that capture; it remains an explicit timing/endpoint difference. Matching
uses sample duration signatures and timing, not byte-identical output. Reports and
captures remain under ignored `private/reports/engine-parity/audio-recovery/`.

Remaining acceptance work includes physical mobile audio latency/stalls, actual
Windows/Linux output devices and broader runtime feature coverage.

## Inactive render sleep (2026-10-21)

As `render()` in `web/game.js` does, a single-player ride schedules no further frame
while the window is unfocused, hidden or suspended, or while paused; the frame that
applies the change draws the paused overlay and then the event loop waits.
Lifecycle and input events (keys, pointer, touch, resize) wake it for one frame,
which applies the event and decides again; resuming restarts the frame clock as the
browser's `lastTime = 0` does. While paused in focus a 50 ms timer polls controllers
(a pressed button wakes the ride) and keeps the soundtrack fed, since the pause menu
keeps music playing. Parity holds and autodrive benchmarks keep rendering. A
multiplayer room keeps the ride awake so its socket and poses stay live (see
Multiplayer).

`tools/test_engine_sleep.mjs` (host Chromium, 960x540, two-second windows): before,
paused/blurred/hidden kept 152-161 animation frames and 191-204 ms of main-thread
task time; after, zero frames and 6-11 ms, against 159 frames and 161-189 ms while
riding (120 steps). The JavaScript game's own measurement (PERFORMANCE.md) is 120 to
zero frames for pause and blur. Host results, not phone or battery measurements;
the desktop build uses the same event-loop path (winit `ControlFlow::Wait`/
`WaitUntil`) but was not measured. The JavaScript game remains available until the shared engine passes
those relevant subsystem gates.

## CPU per frame against the JavaScript game (2026-10-21)

`tools/compare_engine.mjs ride` now also replays the ride script in real time and
measures CPU: the JavaScript game's main thread (Chrome `TaskDuration`) plus its
simulation worker (each operation's Wasm time from `worker.js`), the Rust engine in
the browser (one thread, `?autodrive=`) and the native build (`fury --autodrive
--bench`, vsync wait excluded). Headless frames are not vsync-capped, so builds are
compared per second of riding, expressed per 60 Hz frame. GPU-process time is
excluded everywhere.

ENV01, the 600-step default script, Original preset, sound on, Apple M4 host
(`private/reports/engine-parity/ride-cpu/`):

| Build | Frames (10 s) | Main thread | Worker | Per 60 Hz frame |
| --- | --- | --- | --- | --- |
| JavaScript game | 2304 | 0.34 ms/frame | 0.12 ms/frame | 1.78 ms |
| Rust, browser | 1450 | 0.70 ms/frame | - | 1.87 ms |
| Rust, native (vsync) | 598 | 1.54 ms/frame | - | 1.54 ms |

The Rust engine's own split (browser / native per frame): simulation and sound
0.14 / 0.24 ms, rendering 0.90 / 1.12 ms, mixer 0.01 / 0.03 ms. The Rust engine draws
the same world as the JavaScript game but its CPU cost is about equal in the browser,
not lower: rendering through wgpu's GL backend dominates. Moving the simulation onto
the main thread removed the worker round trip, not CPU. These are host numbers;
the user's iPhone shows the Rust ride at 2.49 ms CPU per frame (61 fps).

## Vehicle presentation: wheel effects and lingering dust (2026-10-21)

Replaces `web/wheel-effects.js` (WheelEffectsRenderer) and `web/lingering-dust.js`
for quads. Evidence: `atv_core::wheel_effects` (001d7620 selection, 001451a0 packets,
PRT/EMT emitters, `WHEEL_EFFECTS.md`), already the browser's simulation.

- `atv-game/wheel_effects.rs` loads Particles.prt, the quad's QuadDust/QuadDirtChunk/
  QuadDirtSpray EMTs and both atlases (op 74 path, light = ambient + sun/2) and steps
  `Effects` once after every ride step with atv-web's clock (summed step time, tick,
  zero-dt frame on a Reset placement and at load). The ride exposes the op 75 snapshot
  through two additive accessors (`Ride::wheel_effect_input`, `Ride::wheel_radii`).
- `atv-render/wheel_effects.rs` + `.wgsl`: sprites re-packed only when a new frame
  arrives (the browser's `pending` upload), wet births in a 10,800-slot ring with
  only new slots uploaded; every step's wet births are kept until the renderer takes
  them, as the browser accepts each frame of a batch.
- Lingering dust (Enhanced 24, High 96; `Graphics::dust`) is authored, not original:
  `atv-game/lingering_dust.rs` (TOY GenerateDust zones, terrain-layer tint) and
  `atv-render/lingering_dust.rs`, uploaded only when a puff is emitted.

Parity (`compare_engine.mjs ride`, scripted throttle, Rust browser / native vs JS,
mean abs /255): FREE01 Original frame 0.21 / 0.05, vehicle box 0.26 / 0.30; ENV03
Original 0.16 / 0.03 (box 0.21 / 0.25); ENV01 default script vehicle box 0.59 (was
1.29 without effects). Same held step: JS 62 wheel particles and 6 live dust puffs,
Rust 62 and 6. ENV03 High differs by 0.74-1.35 (box 3.2-3.8): the High shadow map,
contact shadows and tire tracks are not ported yet. Host cost with the effects live
(ENV03, held dust state, Apple M4): Chromium 0.58-0.70 ms CPU Original, 0.74-1.22 High;
WebKit 1.21-2.18 ms, 60 Hz held in all four; these are host viewport results, not
phones. `compare_engine.mjs ride` gained `--graphics` and reports the JS effect counts.

## Vehicle shadows: contact blobs and the High shadow map (2026-10-21)

Replaces `web/vehicle-shadows.js` (ContactShadows, drawContactShadows) and
`web/shadow-map.js` (VehicleShadowMap, lightMatrices, shadowReceiverGlsl). Both are
authored: the original renders no vehicle shadows, so they stay off under Original.

- `atv-render/shadows.rs`: Enhanced/High contact shadows (7x7 ground-draped blob per
  caster, ZERO / ONE_MINUS_SRC_ALPHA, LEQUAL, offset -1/-2) for the quad and the
  detached rider's torso, with the browser's resting lift (smoothed origin height over
  the visible ground on each Riding step with every wheel down, fed by
  `Riding::step_rest`); High 2048² vehicle-only sun shadow map rendered from the
  vehicle pass's bone uniforms (depth only, offset 2/4, texel-snapped light box).
- Terrain and scenery receive it (`vehicle_shadow` in `terrain.wgsl`/`scenery.wgsl`,
  four compare taps, soft box edge); the train, plants and water do not, as in
  game.js. Below High the shader returns before any tap.
- The shadow texture is 1x1 and never sampled without High; WebGL samples it through a
  comparison sampler (LEQUAL), like the browser's `sampler2DShadow`.

Parity (ENV03 scripted ride, Rust browser / native vs JS): High 0.46 / 0.33 (was 0.87 /
0.74; vehicle box 0.33 / 0.37, was 3.8), Enhanced 0.45 / 0.32; the remainder is tire
tracks (next port). Original static ENV01 unchanged at 0.04. Host CPU per frame, held
ride state: Chromium 0.58-0.66 ms, WebKit 1.13-1.51 ms (Enhanced/High, landscape and
portrait viewports), 60 Hz held in WebKit; host results, not phones.

## Tire tracks and terrain anisotropy (2026-10-21)

Replaces `web/tire-tracks.js` (TrackBuilder, TireTrackRenderer); authored, Enhanced
600 segments / 15 s, High 3,600 / 75 s, off under Original.

- `atv-game/tire_tracks.rs`: the TOY terrain-zone flags (`parseDirtZones`), track
  strength and wheel marks, and the segment ring stepped after each ride step from the
  op 75 snapshot plus the vehicle's slip (`Ride::wheel_effect_state`, additive); reset
  with the ride.
- `atv-render/tire_tracks.rs` + `.wgsl`: dirty-range uploads only, one modulate-2x draw
  (DST_COLOR, SRC_COLOR; LEQUAL, offset -1/-2) between contact shadows and plants.
- The browser applies its anisotropic filter to the terrain atlas and detail map too;
  the terrain samplers now take `Graphics::anisotropy` (Enhanced 4, High 16). This was
  the remaining Enhanced/High difference on terrain-dominated frames.

Parity (ENV03 scripted ride, Rust browser / native vs JS, /255): High 0.16 / 0.04 (was
0.46 / 0.33), Enhanced 0.17 / 0.04; vehicle box 0.28-0.43. Host CPU per frame, High,
held ride state: Chromium 0.79-1.20 ms, WebKit 1.36-2.31 ms, 60 Hz held in WebKit (host
viewports, not phones).

## Speed-rush screen pass (2026-10-21)

Replaces the screen pass of `web/speed-rush.js` (the ride already had its FOV kick and
amount, `atv_game::ride::view::SpeedRush`). Authored, Normal intensity; settings for
Off/Low/High are not ported yet.

- `atv-render/speed_rush.rs` + `.wgsl`: while the amount is above 0.001 the scene
  resolves into its own texture and a full-screen pass streaks it toward the travel
  focus (jittered taps, 8, or 5 on `(pointer: coarse)`, two fewer under 45 fps), adds
  the wind lines and vignette and resolves to the frame. The depth buffer is now
  Depth24PlusStencil8: the vehicle pass writes stencil 1 so the player's quad and rider
  stay crisp, as the browser's rushMask does. Below the threshold nothing changes (no
  copy, no pass), as in the browser.
- `Riding::rush_amount` (additive) feeds `SceneRenderer::set_speed_rush`; a held replay
  keeps rendering the rush like the browser (only a user pause stops it), and native
  captures settle one second of presentation first.

Parity (NAT03, 660-step throttle to 54 MPH, JS amount 0.255, Original): Rust browser
0.54 (landscape) / 0.80 (portrait), was 3.86 / 4.61 without the pass and FOV kick;
native 1.77 (its wind-line clock and focus smoothing run on fewer frames). Slow FREE01
rides are unchanged (0.05 native). Host CPU per frame with the rush active: Chromium
0.70-0.80 ms, WebKit 1.67-2.43 ms, WebKit at 60 Hz (host viewports, not phones).

## Crazy/Gorn crashes and Gorn blood (2026-10-21)

Ride Effects -> Crashes (authored, off under Original): `--crash original|crazy|gorn`
natively, `?crash=` or the JavaScript build's saved `atv-crash-mode` in the browser.

- `atv-game/crash.rs` steps `atv_core::crash_effects` after every ride step with the
  effects clock (tick, 1/60 s; a Reset re-reads with dt 0), through the additive
  `Ride::step_crash_effects` accessor (the inputs of atv-web's `simulation_frame`
  `funny_crash`). The frame's ragdoll and broken-part matrices replace the presented
  pose, as game.js does.
- `atv-render/vehicle.rs` `set_severed` ports `severedIndices`: rider triangles that
  span a severed joint are dropped from cut index buffers, rebuilt only when the
  severed set changes.
- `atv-render/blood.rs` + `.wgsl` port `BloodRenderer`/`packBlood`: splats, stump
  caps and droplet streaks, up to 216 instances in one draw (alpha blend, no depth
  write, depth bias -2/-2), drawn after snow and before the wheel effects.

Parity (ENV03, steer -0.5 throttle, held at step 733 with one severed limb, 42 blood
items and 2 stumps; JS crash camera set to Original, as the Rust engine has no tight
crash camera yet): Rust browser blood box 0.10 (Original) / 0.31 (High), frame
1.85 / 1.98, driven by the Rust paused toolbar and plant sway; the wreck at step 412
matches at 1.46. Native diverges once the ragdoll flies for about a second: Apple
libm and wasm math differ in the last bit, and the chaotic ragdoll amplifies that
(the shared-math change will make native match). Host CPU per frame at the held
crash, Original vs Gorn: Chromium 1.02/0.86 ms (landscape), 0.62/0.63 (portrait);
WebKit 2.39/1.56, 1.87/1.92, all at 60 Hz (noise; the blood is one draw). Wasm
5.21 MB with current main (4.90 MB, 1.28 MB brotli on the branch before merging).

## Render CPU pass (2026-10-21)

Behaviour-equivalent only: every change below leaves the output pixel-identical (native
Metal and Chromium/WebKit WebGL2 captures of seven venues in Original and High, and the
Gorn crash, against the previous build; the only differing pixels are the few
run-to-run unstable ones around the held vehicle, which differ between two runs of the
same build too).

- Profiling: `--profile` (native, with `--autodrive ... --bench`) or `?profile=1`
  (browser autodrive) adds per-pass CPU means (`passes`) and draws per pass (`draws`)
  to the bench report (`atv-render/profile.rs`; marks are no-ops otherwise). On WebGL2
  almost all render CPU is `submit`: wgpu's GL backend replays the frame as GL calls
  there. Each vertex-buffer change and each pipeline change re-specifies every vertex
  attribute (bindBuffer, enable, pointer, divisor), so the GL call count is the
  measure that matters.
- Sky: the 64 CubeFile tiles (128x128 in all 40 venues) are layers of one texture
  array, drawn as one draw instead of 64 texture rebinds and draws.
- Terrain, scenery and vehicles: each pass keeps its geometry in merged vertex/index
  buffers (indices rebased; scenery has a plain and a reflection-mapped set), so draws
  no longer rebind vertex buffers. Draws and their order are unchanged. Merging
  adjacent terrain tiles into one draw was tried and dropped: it changes a few hundred
  far pixels on Metal.
- Scenery: position+U and normal+V share two vec4 attributes (3 attributes instead of
  4). The material and the instance uniforms are one record per draw and pass (one
  dynamic offset instead of two). The records of UV-animated draws lead the buffer, so
  one write refreshes them.
- Vehicles: bones rebind only when a group switches between quad and rider.
- Vegetation selection (`atv_core::vegetation_render`, shared with the JS game):
  the 1,500-plant heap stores each depth beside its index (no random placement reads
  per comparison; the same heap and stable order), and the group-offset vector is
  reused instead of allocated each frame.

WebGL calls per frame (Chromium, ride autodrive): ENV01 4,452 -> 2,906, NAT05
4,023 -> 2,203. Host CPU, previous -> new build, alternating runs, median of 7
(browser) or 5 (native), 600-step autodrive at 1280x720. The host was shared with other
work, so single runs vary by up to 30%.

| | render ms/frame | CPU ms/frame |
| --- | --- | --- |
| Chromium ENV01 | 0.89 -> 0.71 | 1.26 -> 1.07 |
| Chromium NAT05 | 0.68 -> 0.57 | 1.03 -> 0.91 |
| WebKit ENV01 | 1.41 -> 1.17 | 1.80 -> 1.47 |
| WebKit NAT05 | 1.55 -> 1.30 | 2.03 -> 1.76 |
| native ENV01 | 0.99 -> 0.67 | noisy |
| native NAT05 | 1.11 -> 0.84 | noisy |

Against the JavaScript game in the same `compare_engine.mjs ride` CPU baseline
(Chromium, per 60 Hz step): ENV01 JS 1.96 ms vs Rust web 1.73 ms; NAT05 JS 1.93 vs
1.55. The ride parity of that run is unchanged from the previous build (ENV01 frame
4.13/255 in both builds: the paused toolbar and a near plant's history-dependent
orientation; NAT05 0.22).

Remaining GL cost is mostly pipeline changes in scene order: the scenery alpha-failure
pass alternates two pipelines per cutout chunk. Removing those changes would mean
reordering draws, which could change blending or depth ties, or patching wgpu's GL
backend to skip the attribute re-specification for identical layouts. Neither is done
here.
## Fidelity follow-up: water grid rows (2026-10-21)

Replaces a browser stand-in, so the output now intentionally differs from the
JavaScript build. 00190aa0 scales the water node's strip groups with viewport
width/640 (19 samples each, plus one) and its dense rows with height/448 (334 at
448). The browser kept that basis but capped the grid at 12,000 vertices in
landscape and 8,000 in portrait for its Uint16 indices (projectedGridSize in
web/water.js; research/WATER.md, "Water visual stability checkpoint"). The Rust
build draws with 32-bit indices, so `grid_size` drops the cap. At 640x448 it gives the
native 58 x 334. At 1280x720 it is 115 columns x 537 rows (was 103), about 61,900
vertices and 123,000 triangles (was 11,960 / 23,500). At 390x844 it is 39 x 629 (was
204). The 8..1024 row bounds are still the browser's.

Measured at the NAT01 sea view `2614.86,200,3019.69,4.3,-0.3,1.455078`, Original:
- Rust before vs after: 0.11/255 over the frame, all in the water band.
- vs JS: 0.06 -> 0.15. Distant water is now sampled at native density, so the JS
  build's sparse far rows no longer match.
- Start views barely change: NAT01 0.06 -> 0.08, ENV05 0.04, ENV07 0.02, FREE04
  0.03.
- CPU is unchanged: one draw, and the grid is rebuilt only on resize. Host status
  0.70 -> 0.67 ms, frame 12.1 -> 12.5 ms at vsync.
- GPU time, measured below on the desktop host. Portrait phones go from 8,000 to
  24,500 vertices; physical-phone timing is still to do.

### Scene-pass GPU time with the native rows (2026-10-21)

`fury --capture OUT WxH --gpu-frames N` now times the scene render pass with wgpu
timestamp queries (native only; WebGL2 has none): 30 untimed warm-up frames, then N
offscreen frames submitted back to back, and prints the median. Each row below is the
median of five such runs (240 frames each), alternating a build of current main with a
temporary build that re-applies the browser's 12,000/8,000-vertex cap (not shipped).
Views: the NAT01 sea view above (free camera), and the ride start of every wet venue.

Host: Apple M4, macOS, native Metal. **These are desktop host numbers, not phone
results.** No phone has been timed.

| View | Size | Preset | Native rows (ms) | Browser cap (ms) | Difference |
| --- | --- | --- | ---: | ---: | ---: |
| NAT01 sea | 1280x720 | Original | 2.07 | 1.14 | +0.93 (+81%) |
| NAT01 sea | 1280x720 | High | 3.18 | 2.26 | +0.93 (+41%) |
| NAT01 sea | 390x844 | Original | 0.88 | 0.55 | +0.33 (+60%) |
| NAT01 sea | 390x844 | High | 0.99 | 0.66 | +0.34 (+52%) |
| NAT01 start | 1280x720 | Original | 1.34 | 0.94 | +0.40 |
| NAT01 start | 390x844 | Original | 0.57 | 0.45 | +0.12 |
| ENV04/05/07/08, FREE04 starts | 1280x720 | Original | 0.94-1.04 | 0.66-0.80 | +0.25 to +0.27 |
| ENV04/05/07/08, FREE04 starts | 1280x720 | High | 1.16-1.43 | 0.92-1.21 | +0.22 to +0.25 |
| ENV04/05/07/08, FREE04 starts | 390x844 | Original | 0.44-0.53 | 0.32-0.43 | +0.10 to +0.12 |
| ENV04/05/07/08, FREE04 starts | 390x844 | High | 0.47-0.66 | 0.39-0.58 | +0.04 to +0.08 |

Observed:
- On this host the native rows cost about 0.25 ms of GPU time per frame at 1280x720
  wherever the water draws, and about 0.1 ms in portrait. The cost is close to
  constant across venues, so it is mostly per-vertex work: the grid is built for the
  viewport, not the visible water.
- The sea view, where water fills most of the frame, costs most: +0.93 ms at 1280x720
  (scene pass 1.14 -> 2.07 ms) and +0.33 ms in portrait (0.55 -> 0.88 ms). Distant rows
  there are thinner than a pixel, so the extra triangles add rasterizer and partial
  fragment-quad work as well as vertex work. This is an interpretation; no per-stage
  counters were read.
- Within a run the timing is stable (run medians agree to about 0.01 ms). An
  occasional whole run is 2-3x slower in every frame (GPU clock ramp-up); the
  five-run median discards it.

Not established: phone GPU time. Phone GPUs are slower than the M4, so the absolute
cost will be larger there; whether the sea view still fits the frame budget on the
weakest supported phones needs a device measurement before deciding between native
rows and a phone-only cap. If one is needed it must be an explicit option, because
the native rows are the recovered behaviour. Raw rows and the script are in ignored
`private/reports/water-gpu-timing/`.

## Browser stand-ins in the ported presentation

Labelled for follow-up replacement with recovered behaviour (separate PRs, each with a
measured comparison). Parity above is with the JavaScript build.

| Where | Stand-in | Original / evidence | Replaced |
| --- | --- | --- | --- |
| `atv_core::vegetation_render` (used by `atv-render/vegetation.rs`) | 64² cell scan, behind/range test, 1,500-plant nearest-depth heap and sort | 0012a1c0: spatial-callback candidates, frustum tests for near plants only, 600 depth buckets (u16 depth), submission far-to-near of buckets whose cumulative count is under 1,500 (`private/reports/map-fidelity/billboard-recovery/0012a1c0.c`) | No |
| `atv-render/vegetation.rs` | frame serial held at 0 (the browser passes `frame`, which stays 0) | 0012a1c0 increments its serial every call; a plant unseen for more than 3 frames restarts its fade | No |
| `atv-render/scenery.rs` | lit models: ambient + sun·N·L ("provisional prop lighting") | `use_lighting`/`dynamic_lighting` VU programs not recovered | No |
| `atv-render/scenery.rs`, `cull.rs` | per-object fog-end culling with depth range extended by the largest object | 0012b360 far clip at fog end; native object culling not recovered (behaviour-equivalent optimisation, keep) | Keep |
| `atv-render/water.rs` | projected grid capped at 12,000/8,000 vertices for 16-bit indices; portable reflection; no froth | 00190aa0 strip/row counts without the cap; VU reflection and froth pending | Grid cap: yes (see "Water grid rows"); reflection/froth: no |
| `atv-render/ambient.rs`, `snow.rs`, `wheel_effects.rs` | bounded GPU rings (4,800 / 10,800 slots), two blend-family draws | native render queue order and pool sizes not recovered | No |
| `atv-render/scenery.rs` | reverse-looping crowd ATMs keep frame 0 | native playback not recovered | No |
| `atv-game/intro.rs` | swoop end pose behind the scene start | the player's follow camera (race port) | No |
| `atv-render/terrain.rs` | detail blend averaged by mipmaps ("projected-coverage pass selection pending") | 00168490 pass selection | No |
| `atv-game/src/ride/mod.rs`, `atv-app/src/riding.rs` | browser frame batching: dt capped at 50 ms, accumulator at 0.1 s, at most four steps per frame, one control sample per batch | the original runs one simulation step per 60 Hz frame with per-frame input | No |
| `atv-game/src/ride/bindings.rs` | keyboard brake-then-reverse and touch drive adapters (`atv_core::browser_input`) | authored for keyboards/touch; the pad path is original | Keep (authored input) |
| `atv-game/src/audio/ride.rs` | engine sample selection once per rendered batch with the batch's summed dt (GameAudio.update) | the original emitter update runs every frame | No |
| `atv-game/src/audio/mixer.rs` | windowed-sinc resampling at load and linear-interpolated pitch (Web Audio semantics), 0.65 master and 0.45 music gains | SPU2 voice pitch/ADSR, mixer and reverb not recovered | No |
| `atv-game/src/ride/race.rs` | CPU recovery after 5 s off the track polygon or 4 s below 3 ft/s; the player handed to the path driver after the finish; equal-mass rider contact proxy | RaceStatus +0xf0 consumer, finish takeover routine and quad-to-quad response not identified | No |
| `atv-game/src/ride/race.rs` | Restart Race keeps the loaded roster | the original reloads the event (new roster draw) | Yes, through the frontend (direct parity rides keep the in-place restart) |
| desktop `f32` math (`atv-core` via std) | system libm for sin/cos/atan2/…; the browser build uses Rust's libm, so native races drift from the JavaScript/browser result | one deterministic implementation on every target (PS2 VU/libm equivalence is a separate question) | Yes: `atv_core::math` (Deterministic simulation maths) |
| `atv-render/vehicle.rs`, `vehicle.wgsl` (player, CPU and peer quads, bikes, riders) | was opaque like game.js (`alphaCutoff` 0), so cut-out texels drew black or grey (bike sprockets and spoked wheels, quad shock springs). Now each material's base PSM pass blends GS source alpha (`(Cs-Cd)*As+Cd`, As = byte·255/128) when `alphaBlendingOnOff` is set, and texels with zero alpha are dropped. This differs from the JavaScript build. | All 232 vehicle and rider materials surveyed (quads 1-26, bikes 27-46, ATV/MX riders) set `alphaBlendingOnOff=1`; authored inputs are A=Cs, B=Cd, C=As, D=Cd. Bike plastics and sprockets also set `AlphaTestOnOff=1` without a method; `StartApp/DefaultMT.psm` (read by 001b98f0 before each PSM) gives `alphaReferenceValue=0`. The default alpha-test method and Z behaviour of failed texels are not recovered: dropping zero-alpha texels stands in for them (before/after crops in `private/reports/rust-render/vehicle-alpha/`). | Partly (blend: yes; test method: no) |
| `atv-render/blood.rs`, `atv-game/crash.rs` | Crazy/Gorn crash breakup, ragdoll and blood | authored browser feature with no original counterpart (the original crash is the wreck animation, kept under Original) | Keep (option) |

## UI stand-ins to move toward original rendering

The Rust UI first matches the current JavaScript look (`RUST_UI.md`). These are
browser-era stand-ins, each to be replaced in its own labelled follow-up only
with original-game evidence:

- HUD text (speed, units, stunt names/points) keeps the original CEL metrics and
  placement but is drawn in the system display face instead of the original
  `tradegothic_*` glyph atlases (hud.js choice for HD sharpness).
- The GPS minimap, keyboard card, toolbar and touch compass are authored
  browser additions. The original radar belongs to the race overlay set
  (`drawRace` in hud.js; ported, RUST_UI.md "Race overlays checkpoint").
- Menu/pause screens in the JS game are HD re-layouts of original documents
  (`ui_menu.rs`); their Rust port will list each approximation as it lands.
- Frontend (Rust port of menu-screens.js): the title wordmark is the authored
  Fury Remastered vector logo, not original art; the HD pages re-lay the
  original PAGE01 backdrop, TP course photos, LOADICON shots, etips and
  uiQuad.txt data in the browser's CSS layout instead of the original page
  documents; the CSS filters (`blur`, `grayscale`, `saturate`, `contrast`) on
  backdrops and photos are applied on the CPU at load and approximate the
  browser's filter pipeline; Back, the event tabs and the vehicle list are
  browser additions to the original pages.

## Race checkpoint (2026-10-21)

Single-player races ride in the Rust engine (`--race [--laps N] [--cpu 0|3|4]
[--difficulty 1|2] [--seed N]`, `/engine.html?race=1&laps=..&cpu=..`). Ported from
`terrainFromContent`'s race branch, `startSimulation` and atv-web operations 95, 96,
105, 106, 113, 114 and the race paths of the op 50 batch (`race_controls`,
`step_race`, `follow_camera`), calling atv-core directly (`atv-game/src/ride/race.rs`,
`ride/cpu.rs`):

- Loading: the `*_NAT.RES` course (`<venue>nate.tdf`), pre-race shots, trackside TV
  cameras, the original CPU roster (`atvgroups.pb`, `airiders.pb`, mode-TOY
  `ChampQuadGroup`, seeded like the browser's per-load draw), each CPU's own quad
  (physics, mesh, chassis and actor rays) and, for drawing, its quad textures and the
  rig in the roster outfit; grid `RiderSpacing` from the mode or scene TOY.
- Per step, in the browser's order: race-imposed controls (brake through intro and
  countdown, the path driver after the finish, Off Track/stuck returns), the player's
  op 24, race progress, every CPU's driver/physics/rider/wreck step with checkpoints
  and recovery, pairwise rider contacts, race frame and race HUD clocks; then the
  camera: after-finish director, follow/wreck camera, pre-race shots that start the
  countdown. Shots and TV views use the cinematic presentation; any key skips the
  intro; Reset returns to the track point while racing and restarts before GO.
- Drawing: each CPU quad and outfitted rider through its own vehicle pass, skipped
  beyond 864 ft as the browser does. The status line shows countdown, lap, place and
  the results.

Parity (`tools/compare_engine.mjs race`, ENV01, 1 lap, 3 CPUs, roster seed 12345,
weather held off; one scripted player through the JavaScript game's own batches, the
Rust browser build's per-step trace and `fury --trace`): the Rust browser build
matches the JavaScript game bit for bit (binary32) at every step for the player and
all three CPUs — 1,800 steps of straight gas, and 9,000 steps where the script is the
race path driver's own recorded controls (`RACE_AUTOPILOT=9000`, recorded in the Rust
browser build): intro 1-435, countdown 436-675, GO at 676, the player finishing first
at step 7,213 in 108.96 s, then the after-finish autopilot and director. The native
build followed the same phases but drifted from step 240 (1.5e-5 ft, growing to
hundreds of feet over a full lap): desktop `f32` transcendental functions came from
the system libm, the browser's from Rust's own libm (fixed: Deterministic simulation
maths below). The same cause explains the
small native drift in the Freeride comparisons above.

Race overlays, the results panel and its menus: RUST_UI.md "Race overlays checkpoint"
and "Race results checkpoint". Restart Race reloads the event with a new roster draw
and no pre-race camera when the engine's frontend is present (race links); a direct
`?venue=…&race=1` / `fury --race` ride restarts in place with the loaded roster. Crowd, flashes, fireworks, the drop gate, lamp
sounds, race music and the grid rest pose: see Race presentation below.

## Race presentation: stadium, gate, lamps and music (2026-10-21)

Each race step now reports its presentation (`atv_game::ride::race::StepEvents`),
ported from the extras atv-web attaches to its op 50 race frame (`step_race`) and the
per-frame handlers of web/game.js:

- Stadium crowd (CrowdPresent races, op 115): `crowd.aud` from the race archive, the
  Pro crowd when Tracks.dat lists the venue under `prosxN` (frontend.js
  `proSupercross`), `atv_core::race_crowd::Crowd` with the browser's seed; it reacts to
  finishes, a final lap, the player's rank, wreck starts and stunt chains (the rider's
  ChainFinished, as atv-web's `stunt_result`). Sound plays one crowd voice with
  `crowdCue`'s rules (a clip keeps the voice, a Stomp loop gives way).
- Camera-flash bursts (0020ee10): a player win, a CPU top-3 finish and a landed stunt
  burst the existing renderer flash clock before that step's flash step.
- Supercross fireworks (00223c60): `race_fireworks::Controller` over the ambient mode
  TOY (now kept as `Ambient::mode`); its emitter commands reach the shared ambient
  emitters before that step's ambient step, as in atv-web's batch.
- National drop gate (op 116, 00222cd0/00223bf0): the race TOY's `[DropGate]` model
  is added as the last scene model when racing and posed each step (standing until GO,
  then its linear swing; scenery record and bounds updated in place).
- 321 lamps and gate sounds (002e79a0): `beep01.vag` per red lamp, `go.vag` at GO,
  `gate.vag` with a gate (game.js `startLightSound`).
- Race music (raceMusicEvent; 00217bb0, 00217590, 00216260): the soundtrack is held
  through the intro and countdown, a random INT001-INT004 stinger plays on the music
  bus at race load and is cut at GO, which releases the soundtrack (next song); race
  over fades the song over 1.5 s and plays OUT004 then the OUT001 loop after a win, or
  OUT001 after a loss. A sound bank that finishes loading mid-race applies the current
  phase's hold. Leaving the ride stops the stingers and crowd.
- CPU grid before the first step: each CPU quad's rest matrices with the player's
  seated rider pose at its grid origin and yaw (`drawRosterCpu`).

The race samples (lamps, gate, IntOut, the crowd lists) decode from AUDIO.RES after
the ride's bank. `fury --race ... --trace T.json` also writes `T.events.json`, and the
`determinism` example takes `--events`; `compare_engine.mjs race` compares the lamps,
crowd cues, gate matrix and lit flashes against the JavaScript game's batches per step.

Parity (`compare_engine.mjs race VENUE "" OUT 1 3 12345`, `RACE_AUTOPILOT=9000`, the
player finishing first; JavaScript game vs Rust native events, per step):

| Venue | Steps | Lamps | Crowd cues | Lit flashes | Gate matrix | Other |
| --- | --- | --- | --- | --- | --- | --- |
| SX01 (stadium, Am crowd) | 9,000 | identical | 8/8 identical (boo, cheers, win, Stomp stops) | identical every step | none | 58 firework commands (not observable in the JS trace) |
| NAT01 (drop gate) | 9,000 | identical | none | none | identical (binary32) | `gate.vag` at GO (step 676) |

Race fingerprints are unchanged (ENV01 `53fda056a945608e`; trajectories JS = Rust
browser = native over both 9,000-step races). Held frames after GO (NAT01, step 720)
and in the SX01 intro (step 300) match the browser by inspection
(`private/reports/engine-parity/race-presentation/`). Limitations: the sounds are
checked at the event level (which sample on which step), not by a recorded mix
(`compare_audio.mjs` has no race mode yet); fireworks are checked against the
ported controller only; the hold fade ignores the browser's shortened fade near a
song's end on desktop VAG music; the IntOut pick uses the clock rather than
`Math.random`.

## Deterministic simulation maths (2026-10-21)

Every transcendental the simulation calls (`sin`, `cos`, `tan`, `asin`, `acos`,
`atan`, `atan2`, `exp`, `ln`, `powf`, `hypot`; `f32` and `f64`) goes through
`atv_core::math::Det` (`x.sin_d()`, ...), backed by the `libm` crate on every target.
The browser build's `std` maths already is that implementation (`wasm32-unknown-unknown`
has no system libm; `compiler-builtins` carries Rust's libm port), so browser results
are unchanged; macOS, Linux and Windows now reproduce them. Measured on 1M random
inputs before the change: Wasm `std` == `libm` exactly; macOS `std` differed in the
last bit for 1-20% of inputs per function. `sqrt`, `abs`, `floor`, `powi` and basic
arithmetic are exactly specified and stay on `std` (no fused multiply-add: Rust never
contracts). `wasm32-wasip1` `std` (wasi-libc) also differs, another reason not to
rely on any platform's maths. clippy.toml disallows the `std` methods; presentation
crates (`atv-render`, `atv-ui`, `atv-net`, `atv-tools`) opt out crate-wide;
`atv-core`, `atv-game`, `atv-web` and `atv-app` are checked.

Evidence (`private/reports/engine-parity/{race,ride,ride-wreck}-libm/`):

| Script | JavaScript vs Rust browser | JavaScript vs native macOS | Old vs new JavaScript |
| --- | --- | --- | --- |
| Race, ENV01, 9,000 steps (1 lap, 3 CPUs, seed 12345, player finishes first) | 0 every step, player and CPUs | 0 every step (was: drift from step 240) | 0 |
| Freeride, 600 steps | 0 (final state) | 0 every step | 0 |
| Freeride with wreck, 930 steps | 0 (final state) | 0 every step | 0 |

Native fingerprints from `cargo run --release -p atv-game --example determinism`
(headless replay; FNV-1a over the trace's binary32 bits), identical on macOS aarch64,
Linux aarch64 and Linux x86_64 (Docker, `rust:1-bookworm`, x86_64 under QEMU):
race `53fda056a945608e`, ride `ec381279129db4de`, wreck `cabb5cac130294a9`
(macOS with the old `std` maths: race `255eba1e782f169f`, wreck `2cb9e003d9c63e0f`).
The example needs only 59 files (17.7 MB: ENV01, QUAD, RIDER, ANIMSRDR.RES) of the
hosted package.

Content-free CI check (`desktop-check.yml` job `determinism`: macOS arm64 and x86_64,
Windows x86_64 and arm64, Linux x86_64 and aarch64; also part of `make check`):
`math::tests::results_are_the_same_on_every_target` hashes a 20,000-input sweep of
every function, and `vehicle::tests::synthetic_ride_is_the_same_on_every_target`
fingerprints 2,400 steps of a scripted ride (throttle, brake, steering, lean,
reverse, an inverted drop) over bumpy synthetic terrain. Both expected values were
checked against `std` maths in a `wasm32-unknown-unknown` build run in Node (the
sweep) and pass under `wasm32-wasip1`; with macOS `std` maths both fail.

Cost: none measurable. Native race replay 0.260 ms/step with `libm` against 0.272
with the system libm (median of 8 interleaved runs incl. load, Apple M4; the `std`
run follows a different trajectory), wreck script 0.615 against 0.610. The browser
runs the same algorithm as before (simulation and sound 0.126 ms/frame against 0.14
in the CPU-per-frame baseline).

## Motorcycles: Freeride (2026-10-21)

The Garage's motorcycles (selections 27-46, `/MX/BIKE{50,125,250,500}.RES`) ride in
the Rust engine (`fury --quad 29`, `/engine.html?quad=29`; `--rider`/`rider=` pick the
rider). Ported from atv-web's two-wheel integration (`crates/atv-web/src/motorcycles.rs`:
op 100/102 load, op 23 placement with its 30 settling steps, the op 50 motorcycle batch
and the follow/wreck camera actor). `atv_game::ride` now holds the player's vehicle as
`Mount::Quad` (vehicle plus the ATV rider rig) or `Mount::Bike` (`atv_core::motorcycle`,
whose model carries its own rider animation, stunts, clutch and wreck); the venue,
camera, HUD, race and script plumbing are shared. The bike reports a `VehicleFrame`
summary (two wheels as front, front, rear, rear) to the shared consumers; traces record
its own two wheels. Drawing uses the material-remapped bike mesh and the MX rider from
the bike archive, as op 102 returns them. `atv_game::ride::Selection` carries the Garage
choice (`quad`, `rider`).

Parity (`tools/compare_engine.mjs ride ... --quad N`, ENV01; JavaScript game, Rust
browser build per step with `traceAll`, `fury --trace`): bit for bit at every step for
origin, orientation and camera.

| Bike | Script | Steps | Ends |
| --- | --- | --- | --- |
| 29 Suzuki RM125 | default ride | 600 | Riding |
| 33 RM500 XGX | ride with wreck | 930 | Falling (wreck camera) |
| 27 Fast50 | ride with wreck | 930 | Riding |
| 31 Suzuki RM250 | clutch, lean, stunt modifiers, brake, reverse | 860 | wreck and recovery, Riding |

Presentation (follow-up): the bike's MX tire dust, chunks, spray, exhaust and wreck
dust (`atv_core::motorcycle_effects`, stepped after each motorcycle step and reset on a
placement or recovery, as atv-web's batch does) draw through the wheel-effects pass with
the bike archive's FS01particles atlas (`loadMotorcycleEffects`; no tire tracks or
lingering dust, like the browser). Vehicle materials load their mud map (`image.mud`:
the MX MudAnimator's second map, else the ATV2 PixelAnimation dirt pass), and the
vehicle shader blends it by the bike's larger wheel mud amount (`motorcycleMudAmount`,
linear by mud alpha; the ATV reveal branch draws the quad's dirt buildup, see
"ATV dirt buildup" below). Held ENV01 frame of bike 29 after 600
steps, mean absolute difference in the vehicle box against the JavaScript game: 6.19
before, 0.13 Rust browser / 0.15 native after (quad 1 unchanged at 0.59 / 0.63).

Sound (follow-up): a motorcycle loads its own bank (GameAudio.loadBank for ids 27+):
the MX archive's `vehicle_<size>A.aud` engine table (op 101, `parse_mx`) and samples,
with venue samples the bike archive lacks from AUDIO.RES, and no rider effects, vocals,
chassis or drive-on sounds. The shared engine model plays it; the bike adds its wreck
impact (first Wreck sample on a contact of 18.8 ft/s or more, once per step) and the
Clutch sample on each clutch press. The engine bank and rider vocals now follow the
Garage selection for ATVs too (they were fixed to quad 1, male). Offline comparison
(`tools/compare_audio.mjs env01 stunts.json out original 31`, bike 31, 860 steps with
clutch, stunts, a wreck and reverse): all 13 engine samples in the same order within
1-6 steps, the clutch and both impacts matched, 100 ms levels within 0.70 dB mean
(0.21 dB median) of the JavaScript game.

Races (follow-up): a motorcycle player races the original CPU roster on its quads
(MX vs. ATV fields; the roster group is the comparable stock quad per class,
`motorcycle::roster_quad`). The CPUs fork the ATV2 rider rig, which a bike ride loads
for them (`loadAtvRider` with the player's rider, quad 1 selection). The race reads the
bike's rigid-body summary, drives it with the path driver after the finish (no
preload, no ATV scene query), sets its checkpoint 2.5ft above the racing line (the
authored course drop), resolves rider contact through the bike's own peer proxy, and
reports the bike's frame after the race step (contact can move it). No grid grounding
and no deep-water clock, as in atv-web. Parity (`compare_engine.mjs race env01 ... 1 3
12345 29` with `RACE_AUTOPILOT=9000`): JavaScript, Rust browser and native match bit
for bit for the player and all three CPUs over 9,000 steps (intro 1-435, countdown
436-675, GO at 676, the bike finishing fourth at step 8,428).

Interpolation between 60 Hz states (`MotorcyclePresentation`, every preset): see Frame
interpolation.

## Frame interpolation (2026-10-21)

Ports web/motorcycle-presentation.js into `atv_game::ride::present`:
`MotorcyclePresentation` in every preset and `AtvPresentation` under Enhanced/High
(graphics.js `interpolation`; Original ATVs snap to each state). Presentation only:
inputs, physics and traces are untouched. `Riding` delivers a snapshot after each
batch (pumpSimulation's `accept`, with the batch's tick count, the previous state and a
camera cycle or menu breaking continuity) and samples it on every rendered frame at the
frame clock (`render`'s `sample`). The bike blends origin, orientation, both rigs' node
matrices and the camera while Riding without a stunt, over its tick difference (up to 4);
the ATV blends the quad, attached or detached rider, Crazy/Gorn pose, race CPUs and the
chase camera over 1-4 ticks, snapping on a reset, a 30ft jump or a camera preset/mode
change. `blendMatrix` decomposes rotation, per-axis scale and translation as the
browser does (unit test against Node values). Batches are accepted on the frame that runs
them, so the first sample of a segment draws the previous state; the browser's worker
round trip delays its acceptance by a millisecond or two instead.

Evidence: `tools/compare_presentation.mjs` plays a script through the native frame loop
at a fixed render rate (`fury --present-trace OUT --present-hz N`) and replays the logged
delivered states and timings through the JavaScript presenters
(`tools/presentation_replay.mjs`). Every frame's interpolated flag agrees, and presented
origin, orientation, camera, first quad/rider node, detached rider and first CPU agree to
under 3e-4 ft (binary32 against the browser's binary64 at ENV01 coordinates):

| Ride | Preset | Display | Frames interpolated |
| --- | --- | --- | --- |
| quad 1, wreck script | Enhanced | 144 Hz | 2,221 / 2,235 (alpha 0, 0.42, 0.83) |
| quad 1, 9,000-step race with 3 CPUs | High | 144 Hz | 21,577 / 21,603 |
| bike 29, wreck script | Original | 120 Hz | 1,860 / 1,862 (alpha 0, 0.5) |
| bike 33, wreck script | High | 144 Hz | 1,586 / 2,235 (wreck frames snap) |
| bike 31, ride | Enhanced | 120 Hz | 1,200 / 1,202 |
| quad 1, ride | Original | 144 Hz | 0 (snaps) |

The browser build passes the same check at a simulated 120 Hz display
(`tools/test_engine_interpolation.mjs`: virtual-clock requestAnimationFrame and
performance.now, `?presentTrace=1`): Enhanced quad and Original bike interpolate on
alternate frames, Original quad snaps. Alpha slow motion (`hype.timeScale`) stretches
each segment by its scale (RUST_UI.md "Alpha experiments").

### Enhanced wheel re-projection (2026-10-21)

Enhanced and High redraw wheels that sink into the visible terrain after integration
(graphics.js `wheelReprojection`, game.js `reprojectWheels`,
`atvPresentation.quadPose`). `Ride::reprojected_quad` reads the posed wheel centres
(simulation_frame's `wheel_effects.centers`), the wheel radii and the dense render grid
(`Venue::visible`, renderCore.terrainHeight); a wheel deeper than 0.01 ft gains
compression `min(depth / up, MAX_WHEEL_LIFT 0.5)` and the linkage is resampled, but only
when the drawn linkage equals the rig's own sample within 1e-3, the quad is Riding and
no Crazy/Gorn crash pose is drawn. Every delivered state is reprojected before
interpolation; without interpolation the latest state is. Simulation is unchanged
(presentation only); Original keeps the simulated linkage.

Evidence (host browser, ENV01 default ride script, `tools/compare_engine.mjs ride
env01 "" <out> --graphics enhanced`): trajectories identical over 600 steps; the
JavaScript game's sampled `wheelLift` matched the Rust trace's per-step lift within
1e-3 for 548 of 559 frames (the rest are samples more than one step stale, worst
0.017 ft), 191 JavaScript samples and 207 Rust steps lifted; held-frame vehicle box
mean absolute RGB difference 0.57 (1280x720) and 1.17 (390x844).
`tools/test_engine_wheel_reprojection.mjs` autodrives Enhanced (463-579 of 901 sampled frames
lifted, max 0.5 ft) and Original (none). Phone performance is not measured; the work
is four terrain samples and at most one extra linkage sample per delivered state.

## Rider gear and quad paint (2026-10-21)

The Garage appearance (rider gear and quad paint) now dresses the Rust ride
(`atv_game::ride::gear`, `Ride::dress`), ported from game.js `rideAppearance` /
`gearTextures` and render-support.js `PAINT_PALETTE` / `paintable` / `paintGlsl`:

- Gear: the original Profile Store catalog (`/UI/RESOURCE/RIDEINFO.RES`
  `uiRider.txt`, `atv_core::rider_gear`, op 107) maps each of the seven slots
  (helmet, gloves, boots, shirt, sleeves, pants, goggles) to the first candidate
  material `RIDER_MT.RES` holds; shirts pass their base so the rider mesh's slot
  suffix applies, as for the CPU roster outfits. Packages without the catalog keep
  the original look.
- Paint: the vehicle shader recolours the quad's `s<n>m<n>` body materials only
  (never the rider, never CPU quads, which keep their own pass) with the same
  HSV maths as the GLSL (`vehicle.wgsl` `apply_paint`): hue modes keep each
  texel's shading, White lifts and Black darkens saturated texels; grey, black and
  metal parts stay original. Paint applies before the mud blend, as in the browser.
- Motorcycles keep their original livery and MX rider (`rideAppearance` is null for
  them). The appearance comes from the saved `atv-player-selection` value the
  JavaScript Garage writes (`normalizedAppearance`: seven indices 0-64 and a paint
  index, else the original look); the Rust frontend keeps that field when it saves.
  Captures and tests: `fury --gear H,G,B,S,SL,P,GO --paint N`, `engine.html?gear=1.2.1.3.3.2.1&paint=4`,
  `compare_engine.mjs ride ... --appearance 1.2.1.3.3.2.1:4`.

Parity (`compare_engine.mjs ride env01`, default 600-step script, held frame; mean
absolute RGB difference in the vehicle box against the JavaScript game, Rust
browser / native; whole frame in brackets, which includes the toolbar and
foreground plant differences that predate this change):

| Appearance | 1280x720 | 390x844 |
| --- | --- | --- |
| Original (no appearance) | 0.59 / 0.63 (4.13 / 4.00) | 1.12 / 1.19 (7.64 / 7.32) |
| Gear 1.2.1.3.3.2.1, Green paint (4) | 0.61 / 0.66 (4.13 / 4.00) | 1.14 / 1.22 (7.64 / 7.32) |
| White paint (9) | 0.62 / 0.68 | 1.16 / 1.25 |
| Black paint (10) | 0.58 / 0.62 | 1.12 / 1.18 |

The dressed differences stay within 0.03 of the undressed baseline: the gear,
jersey and paint match the browser (images inspected,
`private/reports/engine-parity/gear-paint/`). Not ported yet: the Garage's Rider
gear page and its gear/paint previews in the Rust frontend (the field is only
carried through). Multiplayer peers' gear and paint arrived with Multiplayer
slice 1.

## Multiplayer (2026-10-21)

Multiplayer is the largest piece the Rust engine needs before play.html can switch
to it. It is delivered in vertical slices; each one stays compatible with the
JavaScript game on the same relay (protocol v6, `atv-net`).

### Slice 1: relay client and remote riders

Ported:

- `atv-game/src/multiplayer/mod.rs`, `Session`: the protocol half of atv-web's
  networking adapter (crates/atv-web/src/multiplayer.rs ops 80-85, 109-111 and 118),
  with no I/O. It covers Join, Welcome (slot, code, rejoin key) and Lobby, Ready, and
  the host's SetLobby. Pings keep the socket warm and set the relay clock offset.
  Pose sending ports op 81 unchanged: the same adaptive cadence (20/10/5/1/0.2 Hz),
  compact full snapshots and changed-block deltas, and send receipts that advance
  the baseline. Receiving admits only snapshots from rostered riders whose models
  match. `PoseBuffer` smoothing is atv-net's, shared with the JavaScript game.
- `atv-game/src/multiplayer/remote.rs`: each remote rider's original quad, rider
  mesh, 78 motions and textures, posed locally from the replicated recipe (port of
  multiplayer_animation.rs). Wreck clips run on the receiver's clock. The rider wears
  the room's gear and the quad carries its paint (`ride::gear`, PR #220).
- `atv-app/src/net.rs`, the transport. The browser uses a web-sys `WebSocket` and
  `fetch`. Desktop uses tungstenite and ureq (rustls) on a socket thread, which sends
  `Origin` set to the relay's own origin. That header satisfies the relay's
  browser-origin check; it is not authentication.
- `atv-app/src/multiplayer.rs`, `Net`. Hosting POSTs `/multiplayer/rooms` and joins
  with the slot key; joining POSTs `{code, open:true}` first, as invite links do. A
  guest rides the room's course; the host starts the room on `--venue`. Every frame
  `Net` sends at the 50 ms tick, uploads remote models as they load (one
  `VehiclePass` per slot in `SceneRenderer`, beside the race CPUs) and draws the
  presented riders. Riders over 864 ft from the eye are skipped, as for CPUs. A room
  keeps the app awake, so inactive render sleep never stops its socket or poses.
- Entry points: `fury --host [--room CODE] | --room CODE [--relay URL] [--name NAME]`
  (relay default `https://furyremastered.com`), and `engine.html?host=1` or
  `?room=CODE[&name=]` on the page's own origin. Motorcycles fall back to ATV01, as
  in the JavaScript game.

Observed (`tools/test_engine_multiplayer.mjs`, run once on a local `wrangler dev`
relay with original content, one Mac, headless Chromium/Metal; host-only):

- Native `fury --host` created room FV246A4P on env01. A JavaScript guest (quad 14,
  female) and a Rust browser guest (`engine.html`, WebGL2) joined by room link.
- Each of the three clients drew both other riders: native `drawn` 2, engine
  `drawn` 2, and the JavaScript game listed 2 fresh remote riders.
- The JavaScript guest drove 240 steps. The Rust browser guest saw that rider move
  134.7 ft.
- The Rust browser guest sent 13 messages (10 full, 3 delta, 117-byte deltas) in
  1,688 bytes and suppressed 65 sends. RTT was 23 ms.
- Inspected screenshots are in ignored `private/reports/engine-multiplayer/`
  (`slice1-native.png`, `slice1-engine.png`, `slice1-js.png`). The native capture
  shows the JavaScript rider's Honda quad 14 and female rider beside the host. The
  Rust guest overlaps the host at the start (see gaps).
- The native capture holds its ride unstepped while it waits. Its spawn velocity
  therefore reads as moving, and it sent 296 no-change 26-byte deltas at the near
  cadence. A live ride steps normally. The native RTT of 256 ms includes blocking
  model loads in that capture loop.

### Slice 2: drop-in riding

Ported:

- Arrivals meet a rider (`atv-game/src/multiplayer/spawn.rs`, port of atv-web
  `world::spawn`). A client that joins a room where others already ride tries for 3 s
  (12 tries, 250 ms apart) to appear beside a fresh, Riding rider: the host first in a
  private room, a shuffled rider in a shared one. The six candidate spots, ground and
  prop surfaces, rider spacing and prop clearance are the browser's. A course change
  starts everyone at the start line, as `ride()` in web/multiplayer.js does.
- Owner-local rider contacts (`atv_core::peer_contact`, authored yawed boxes) against
  each other rider's current pose before every step, off for 2.5 s after any arrival.
- Reconnect: a retryable close (1001/1005/1006/1011-1013, or 1000 "Connection timed
  out") retries the slot key for 25 s with backoff (500 ms doubling, at most 3 s). It
  opens a fresh session and keeps the ride. The browser keeps the latest slot key in
  sessionStorage `atv-mp-session`, the same entry web/multiplayer.js uses, so a
  reloaded `engine.html` reclaims its slot. `?dropAfter=S` is a test hook: it drops
  the socket once with close code 4000.
- Following the room: when the host moves the room, the client reloads the ride on
  the new course and keeps the room, its socket, models and choices (`RoomRide`).
- Online pause and blur: with the pause menu open or the window unfocused, the ride
  keeps running on neutral input, so others still see this rider and its world clocks
  advance (web/game.js online pause). Only actual suspension (hidden page, suspended
  app) sends the paused presence pose.

Observed (`tools/test_engine_multiplayer_dropin.mjs`, run once on the local relay,
one Mac, headless Chromium/Metal; host-only):

- A JavaScript host rode env01 from the menus (room XMVSFJWK). The Rust browser guest
  (quad 7) arrived beside it: `met` was the host, 10.08 ft from the target, 9.98 ft
  from the host's origin.
- The guest's socket dropped after 25 s and it reconnected once, back in slot 1, with
  the host drawn again. Reloading the page then reclaimed slot 1 again.
- The host moved the room from Pause -> Select Track to env02. The Rust guest reloaded
  there and drew the host again, 4.08 ft away at the shared start.
- Inspected screenshots: `private/reports/engine-multiplayer/slice2-arrival-engine.png`,
  `slice2-arrival-js.png`, `slice2-moved-engine.png`.
- The slice 1 three-client gate passed again (room XNYAK4ZK). Native and engine each
  drew 2 riders, the JavaScript client listed 2, and the JavaScript rider moved
  135.0 ft. Arrivals now stand beside the native host instead of on it
  (`slice2-three-native.png`).

### Slice 3: Arlington Free Ride from the main menu

Ported:

- The Rust main menu lists **Arlington Free Ride** (`atv_core::ui_menu` row; the
  frontend no longer filters it). It shows the browser's NEW tag and the live
  "N riding" count from `GET /multiplayer/freeride/riders`. The count is polled every
  15 s while the main menu shows: a fetch on the web, a short-lived thread on desktop.
  The tag's glow and the count's pulse animations are not drawn.
- Choosing it needs no setup (frontend.js `freeride`). The remembered rider name
  rides, and the default `Rider` becomes `Rider NNNN`. A motorcycle choice rides
  ATV01. The loader opens on Arlington at once while `POST /multiplayer/freeride`
  reserves a slot in the first shared room with space. The ride then joins that
  `FREERDnn` room with the returned key and meets a rider already there.
- Pause -> Quit to Main Menu leaves the room. Dropping the ride closes the socket on
  the web too.
- Desktop menus use `--relay` (default `https://furyremastered.com`); the web uses
  the page origin.

Observed (`tools/test_engine_freeride.mjs`, run once on the local relay, one Mac,
headless Chromium/Metal; host-only):

- The Rust main menu choices were single, freeride, settings and devlog, with
  "0 riding".
- Free Ride joined FREERD01 on env08 as slot 0. A JavaScript player tapped Arlington
  Free Ride, joined the same room as slot 1 and met the Rust rider. The Rust client
  drew 1 remote rider; the JavaScript game recorded 13 remote draws.
- Quit to Main Menu returned to the main page, and the Rust rider left the
  JavaScript player's roster.
- `tools/test_engine_front.mjs` passes with the new row. Inspected screenshots:
  `private/reports/engine-multiplayer/slice3-menu.png`, `slice3-rust.png`,
  `slice3-js.png`.

### Slice 4: Arlington's train in physics and the shared train clock

Single-player Rust had the ENV08 train for presentation only. The renderer stepped
it with no collision heights, and riders passed through the cars.

Ported:

- `atv-game/src/train.rs` loads each `use_collision` car's original COL mesh
  (atv-web op 67 bindings).
- An ATV ride on ENV08 installs the cars as moving scene obstacles (op 69). It then
  steps the shared train runtime before every vehicle step with the scene's
  collision heights, republishing each car's collision body (`step_train_scene`).
  The renderer stops stepping a train the ride drives (`Train::driven`).
- A motorcycle ride does the same with the bike's own scene, as motorcycles.rs
  does: the scene installs the train bodies and op 24 steps the train after the
  race step. The content test `crates/atv-game/tests/motorcycle_train.rs` (bike
  29; `FURY_CONTENT=... cargo test -p atv-game --test motorcycle_train --
  --ignored`) repeats test_motorcycle_train.mjs's two cases. A broadside hit made
  4 dynamic contacts and wrecked. A drop onto car 0 stood on the moving train
  for 73 of 720 steps. Before this change the bike had no train (`train_car`
  returned none).
- In a room, every step's train phase follows the relay clock: the browser's
  `synchronize_train`, with gentle phase correction and a primed 16-step resync
  that carries a rider standing on a car along.
- Snapshots send the car the rider stands on (`platform`) and the train time.
  Received riders on a car are drawn, contact-tested, met and spawn-checked relative
  to this client's car (`rebase`).
- Settings → Train riding: Original by default. Realistic reaches the ride
  through the saved settings (`RideOptions::realistic_train`, applied in solo and
  room rides). The content test `crates/atv-game/tests/train_riding.rs`
  (`FURY_CONTENT=... cargo test -p atv-game --test train_riding -- --ignored`)
  repeats test_train_riding.mjs. A quad on flatbed car 6 through the long left
  curve (44.7 ft/s) behaves as follows:
  - coasting: drifts 0.3 ft with Realistic and 63.4 ft in Original;
  - braked across the deck: drifts 0.0 ft and 174.5 ft.

  These meet the browser test's bounds (under 2 ft and over 10 ft).
- An arrival beside a rider standing on a car starts with that car's point
  velocity at its spot, as atv-web's `spawn` returns it. `spawn::beside` returns
  the velocity, and the placement applies it to the quad. The same test checks
  this: the spot beside a parked flatbed rider got the car's
  [-44.77, 0, 0.12] ft/s; without a platform the arrival starts at rest. The
  shared clearance check rejects most spots beside a train rider, in both
  engines. The clear spot found was on the ground beside the car.

Observed:

- `node tools/compare_engine.mjs ride env08`: single-player ride parity passed. The
  native frame difference against the JavaScript game is 0.03 (landscape) and 0.07
  (portrait) of 255. The Rust-web capture includes the HUD and keyboard card, so
  its whole-frame number (6.3/16.6) is UI, not scene.
- `tools/test_engine_multiplayer_train.mjs`, run once on the local relay (host-only,
  headless Chromium/Metal): a Rust browser host on env08 and a JavaScript guest.
  - Train phases agreed within 0.20 s over 10 paired samples. That bound includes
    the skew between sampling two pages.
  - The JavaScript guest placed on flatbed car 3 was drawn by the Rust host within
    1.75 ft of its own deck-relative position (20 samples).
  - A Rust rider placed on the flatbed (`?trainPlace=S` test hook) was seen by the
    JavaScript guest within 3.25 ft (20 samples).
  - Later re-runs failed intermittently, with 0-8 platform samples. The cause is
    placement, not presentation: the riders were dropped 17 ft above car 3,
    10 ft along it, in Original riding. Logged deck positions show both the
    JavaScript guest and the Rust rider sliding along or off the moving car after
    the drop, depending on the route phase. One run also put both riders on the
    same spot, so they pushed each other off.
  - The test now places riders the way test_train_riding.mjs does: 8 ft above a
    car's centre, at the car's velocity, with Realistic riding. The JavaScript
    guest goes on car 6 and the Rust rider on car 3 (`?trainPlace`). Each check
    re-places up to three times until 10 of 20 samples are on a platform.
  - Content-test probes of the car centres (Realistic, curve) drifted 0.0-0.5 ft
    on cars 3-7. Car 8 and off-centre spots (±12 ft) do not hold.
  - Six consecutive runs passed on a host at load average 8-24. The guest was seen
    within 1.6-3.2 ft and the Rust rider within 1.9-3.6 ft. 3 of 12 checks needed
    a second placement.
  - Under Original train riding, both clients' riders later slide off the moving car,
    as the original does.
- Inspected screenshots: `private/reports/engine-multiplayer/train-js.png` (the
  JavaScript guest on the flatbed) and `train-rust-rider.png` (the Rust rider after
  sliding off).

### Slice 5: other riders' engines and body impacts

Ported (`atv-game/src/audio/remote.rs`, atv-web op 83 `presentation` with
web/audio.js `remoteFrame`):

- The nearest six riders keep an engine voice; distant, stale, departed or
  re-vehicled riders lose it.
- Each voice runs its own original engine sound state machine
  (`EngineAudioState`, seeded per slot) on the replicated throttle, speed and
  riding state. It plays that quad's original samples, loaded once per quad from
  `vehicle.aud` and `AUDIO.RES` (`bank::load_remote_engine`).
- Idle loops use the VAG loop markers, as the player's idle does. The browser loops
  the whole Web Audio buffer, so a looped remote idle can differ at its loop point.
- A new crash serial plays an original body impact (`rider.aud`) at the detached
  rider's position.
- Every voice is placed for the follow camera's listener by the recovered spatial
  arithmetic: distance attenuation of 15 to 300 ft for engines, `rider.aud` distances
  for impacts, then stereo pan and Doppler pitch.
- Voices stop while the output is suspended or unfocused, and when the ride ends.

Observed (`tools/test_engine_multiplayer_audio.mjs`, run once on the local relay;
host-only, headless Chromium with autoplay allowed):

- A Rust browser guest (quad 12) in a JavaScript host's room played the host's
  engine as 1 remote voice. The JavaScript host played the Rust rider's engine as 1
  voice, with its playhead advancing.
- Holding the host's throttle for 180 steps raised the guest's remote engine sample
  starts from 1 to 7.
- Unit test: a new crash serial plays exactly one spatial impact, and a silent
  output starts none.
- Native desktop output uses the same code; it was not exercised by this gate. The
  remote mix was not compared sample-for-sample with the browser's Web Audio output.

### Slice 6: presence while hidden and a kept desktop slot key

- A hidden or suspended room ride draws no frames: a hidden tab runs no animation
  frames, and a desktop window may get no redraws. A 1 s timer keeps the room live
  without them. The browser uses `setInterval`, which background tabs throttle but
  keep running, as web/multiplayer.js relies on. Desktop uses a `WaitUntil` wake-up.
  Each wake drains the socket, keeps pinging, loads arriving riders' models and
  sends the paused presence pose at its 0.2 Hz cadence.
- Only actual suspension sends paused presence; a blurred but visible window keeps
  neutral-input riding (slice 2). The train freezes while hidden, as in the browser.
- Desktop keeps the slot key `{code, key}` under `atv-mp-session` in the settings
  file (`FURY_SETTINGS` overrides its path). Restarting `fury --room CODE` within the
  relay's 30 s hold reclaims the same slot. The browser keeps it in
  sessionStorage (slice 2). An expired key just joins as a new rider.

Observed (host-only, local relay):

- `tools/test_engine_multiplayer_presence.mjs` hid the Rust browser guest for 12 s:
  `visibilityState` hidden and animation frames held, with a JavaScript host
  watching. The host saw the Rust rider fresh in all 12 one-second samples, and the
  guest sent 4 presence messages while hidden. Shown again, it resumed with the host
  drawn.
- Desktop: `fury --host --room RK617363` saved its key. A restarted
  `fury --room RK617363` with the same settings file reclaimed slot 0 as host. A
  control run with an empty settings file joined as slot 1.
- Not established: a run without the timer, as a negative control. The gate emulates
  a hidden tab in headless Chromium; real background-tab throttling and mobile
  page freezing were not measured.

### Slice 7: name tags and map markers

- `atv-ui/src/hud/peers.rs` ports `PlayerIndicators`.
  - Overhead name tags for the nearest eight fresh riders, at most three pinned to
    the screen edge.
  - Each tag sits 7 ft above the ATV, projected with the scene's view-projection.
  - The label is the display face at 18 px italic 800, white with a black stroke,
    over a slot-coloured arrow. The arrow points down over an on-screen rider and
    toward an edge-pinned one, at 85% opacity.
- The GPS map draws each fresh rider as a slot-coloured dot with an ink ring,
  clamped to 40% of the map and rotating with a heading-up map (`mapPeerMarker`).
- Slot colours are the original CompassArrow tints for slots 0-3 (red, blue,
  green, yellow; 0x002e68e0); slots 4-19 are authored. Tag placement and artwork
  are authored browser presentation.
- Observed (`tools/test_engine_multiplayer_tags.mjs`, local relay, host-only): a
  JavaScript host riding ahead of the Rust browser guest got its name tag
  (`tags` 1) and a map dot. Inspected: `private/reports/engine-multiplayer/tags.png`.
  Its marker was in the top 14% of the screen, so it is an edge tag pointing up
  toward the rider, as the browser draws it.

### Slice 8: online pause rows

- The Rust pause menu sends the browser's `lobby_*` request fields to
  `ui_menu`, so a room ride gets its online rows: Players, Copy invite link and
  Leave Game. A shared Free Ride room has no invite row.
- **Players** lists the rider and anyone still connecting (`.pause-player`: "You",
  "Connecting"). It offers "Spawn beside <name>" for each fresh rider, which runs
  the arrival spawn search (`Net::spawn_beside`, atv-web op 86's checks and
  messages) and resumes. Errors show as the menu notice.
- **Copy invite link** copies the room's play.html invite: the clipboard on the
  web, the menu notice on desktop.
- **Leave Game** forgets the kept slot key, closes the socket and returns to the
  engine's main menu (a direct room link reloads engine.html).
- Observed (`tools/test_engine_multiplayer_pause.mjs`, local relay, host-only):
  - The pause rows were resume, reset, players, settings, copyride, devlog, leave.
  - The copied invite carried the room code.
  - Spawn beside the JavaScript host moved the Rust guest from 145 ft to 15.1 ft
    away.
  - Leave Game removed the guest from the host's roster.
  - Inspected: `private/reports/engine-multiplayer/pause-pause.png` and
    `pause-players.png`.

### Slice 9: the Multiplayer menu (Host Game, Join Game)

- The Rust main menu lists **Multiplayer**. Its page offers Host Game and Join
  Game (`ui_menu` pages `multiplayer`, `create-lobby`, `join-lobby`), laid out like
  the main menu with the ride stage on the panel.
- The forms' text fields follow menu-screens.js `buildForm`: Rider name, plus the
  optional 8-letter/number lobby code when hosting or the lobby code when joining.
  - In the browser a real `<input>` sits over each drawn field
    (`atv-app/src/text_inputs.rs`), so phones open their soft keyboard and desktop
    browsers keep native editing and IME. Enter or Escape hands the keyboard back.
  - On desktop the fields take typed text with a caret: Tab, a click or Up from the
    first row focuses one; Backspace deletes.
  - Codes are upper-cased letters and digits; names are at most 24 bytes with no
    control characters. The name is remembered in `atv-player-selection`.
- Host Game → Select Track → Ride creates the room (with the custom code) on that
  course and rides. Join Game checks the 8-character code and opens the room; an
  ended room opens again on the current course. Garage opens the garage.
- On a motorcycle, Host Game's Select Track and Join Game refuse with the
  browser's notice ("Choose an ATV for multiplayer. Motorcycles are available
  in Single Player.") and stay on the form. As in frontend.js, the garage opened
  from the Multiplayer menus or from Change ATV and Rider over a room lists ATVs
  only, and stepping wraps within ATVs 1-26. Free Ride and the frontend-less
  entry points (`?room=`, `fury --room`) still fall back to ATV01, as the browser
  does for Free Ride (unit test `rooms_refuse_motorcycles_and_online_garages_list_atvs`).
- Observed (`tools/test_engine_multiplayer_menu.mjs`, local relay, host-only):
  - A Rust browser hosted room RM634747 from the menus, with the name and a
    lower-case custom code typed into the inputs.
  - A JavaScript player joined it by code.
  - A second Rust browser joined from Join Game and drew both riders (3 players).
  - `test_engine_front.mjs` and `test_engine_freeride.mjs` pass with the new main
    menu row; Free Ride now leaves through Leave Game.
  - Inspected: `private/reports/engine-multiplayer/menu-host-form.png` and
    `menu-join-form.png`.

### Slice 10: Change ATV and Rider, Select Track

- From the pause menu of a room ride started from the engine's frontend, **Change
  ATV and Rider** and the host's **Select Track** open the frontend's garage or
  course picker over a *held* room. The ride ends, but the room's `Net` stays live
  in the app: socket, reconnects and pings run on the 1 s presence timer.
  Done/Back or Ride returns to the room.
  - A new look sends `Profile` (atv-web op 110: the next pose is a full snapshot).
    The ride reloads in place at the rider's position (op 112's pose) and reports
    Ready.
  - The host's Ride sends `SetLobby` with the new course. Everyone, the host
    included, reloads there from the start line.
  - A guest returning after the host moved the room rides the room's current
    course.
- A direct room link (`engine.html?room=`, `fury --room`) has no frontend, so both
  rows stay hidden there.
- Behind the garage or course picker the ride waits paused and stays present, as
  in the browser (its simulation pauses under the menu while web/multiplayer.js
  keeps sending). The app keeps the room's `Ride` (`held_ride`) and
  `Net::hold` sends its paused presence (0.2 Hz; others keep a paused pose for
  15 s) on the 1 s presence timer. The GPU scene is released for the frontend,
  and the ride reloads after Done/Ride. During that reload no poses go out, as in
  the browser while it loads. Before this change the ride was dropped, and the
  guest saw the host go stale after about 1 s.
- Observed (`tools/test_engine_multiplayer_room_front.mjs`, local relay,
  host-only):
  - A Rust browser hosted room JKSRF5Y8 from its menus with a JavaScript guest.
  - Change ATV and Rider → next ATV → Done reloaded in place (moved 0.01 ft), and
    the guest saw slot 0's model become ATV 2.
  - Select Track → next course → Ride moved the room to env02. The Rust host reloaded
    there and drew the guest again.
  - The guest kept the host present (not stale) in all 24 samples over 6 s
    behind the garage. The same check on the previous build failed after 1 s
    (4 of 24 samples).

### Full Free Ride room in the Rust engine (measurement)

`tools/test_engine_freeride_full.mjs` (manual group; local relay only, never
production): one Rust browser rides Free Ride from its main menu, and 19
synthetic riders join FREERD01 at the near/fast 20 Hz cadence. These are
test_freeride.mjs's bots, using the Rust codec over Node WebSockets, circling the
Rust rider at 30-90 ft. Over 12 one-second samples (one Mac, headless
Chromium/Metal, 1280x720; host-only, not a phone or Internet result):

- The room held 20 riders, and all 19 others were drawn in every sample, with
  6 engine voices (the nearest-six limit). Up to 6 name tags were shown: tags are
  limited to the nearest eight, of which at most three may sit at the screen edge.
- Frame rate: median 79 fps, 13.4 ms frames, 6.9 ms main-thread CPU. The Rust
  figure covers simulation, rendering and UI; the browser game's 1.1-1.7 ms render
  CPU excludes its worker.
- Received about 52.5 KB/s of relay traffic and sent about 128 B/s (the rider stood
  still).
- Hitches:
  - A 192 ms frame came in the first second, while the six nearest riders' engine
    banks decoded on the main thread.
  - Two late samples ran at 28 fps (36 ms frames, up to 13.7 ms CPU); the cause was
    not identified.
  - The only relay round trip measured in the window (830 ms) was a ping answered
    during the first hitch. Pings run every 15 s, so this run gives no steady
    round-trip figure.
- Inspected: `private/reports/engine-multiplayer/freeride-full.png`, with the raw
  samples in `freeride-full.json`.
- Follow-up: decode remote engine banks and parse remote rider models off the
  frame, or spread them over frames (the browser decodes in Web Audio and loads
  models asynchronously). Find the late 28 fps samples. Measure on a phone.

### Arrivals spread across frames (multiplayer)

The first-second 192 ms frame was mostly the player's own sound bank: about 3 s
of `Resampler::process` in a Chrome CPU profile (wasm names kept for that build
only), plus per-sample kernel setup. Arrivals then cost 16-53 ms frames each.
Remote loads now run as frame tasks, the way the browser loads asynchronously:

- `atv_game::yield_frame()` returns Pending once. The app keeps remote rider
  models and engine banks as boxed futures and polls them for about 3 ms at the
  start of each ride frame (and in the hidden-tab presence tick), rotating which
  task goes first. It does this on web and desktop.
- Each remote model yields between archive read, PSG decode, quad mesh, each
  material's textures and the animator. The rider mesh, its looping flags and
  its 78 motions are decoded once per sex and shared (`Rc`). Outfit textures are
  cached by sex and outfit.
- Engine banks resample in 4096-frame pieces (`Pcm::from_vag_spread`, within
  1e-5 of the one-read conversion, unit-tested). The windowed-sinc kernel is
  cached per rate pair. `process` uses slice folds in the same summation order:
  the player's own bank stays bit-identical (`fury --audio` WAVs compared with
  `cmp`).
- Content chunks hash SHA-256 in 512 KiB pieces with a yield between them.
- GPU work: other riders share the player's vehicle pipeline, layouts and sampler
  (`VehicleShared`, compiled at ride start). Texture views are cached by name,
  size and a sampled fingerprint. At most one rider's GPU upload and one rig fork
  happen per frame.
- The status line shows the second's worst frame, room CPU, the longest load
  slice and the longest upload. `?pingMs=` (250-15000) shortens the ping
  interval for measurement. Messages carry their arrival time.

Observed (`ATV_FREERIDE_HZ=10 node tools/test_engine_freeride_full.mjs`, local
relay; the test waits 8 s after the ride starts, so the player's own loading is
outside the window; joins paced 250 ms; one Mac, headless Chromium/Metal,
1280x720; host-only, not a phone or Internet result):

- All 19 riders were drawn 6.5 s after the first join. In the arrival seconds the
  worst frames were 25-31 ms, the longest load slice 13.6 ms (most 5-7 ms), and
  the longest upload 7.7 ms. Before the change: 16-53 ms frames, 16-23 ms load
  slices, and a 33-39 ms first upload that included the pipeline compile.
- Over the 30-second window: median 77 fps, 13.1 ms frames, 6.3 ms CPU (max
  8.3 ms), and room work 1.8 ms median.
- Round trips with a 1 s ping (`?pingMs=1000`): Rust median 7 ms, p95 47 ms,
  max 164 ms (30 samples); a bot's own pings median 8 ms, p95 38 ms.
- The slow seconds: two seconds ran at 32-36 fps (28-31 ms frames, 50 ms worst)
  while the engine's CPU stayed at 6.7-7.0 ms, with no load or upload in them.
  The frame time grew outside the engine's main-thread work. This host was
  heavily loaded (load average 5-7, another agent's headless Chromium near
  114% CPU, workerd near 100%), and WebGL has no GPU timestamps here, so host
  scheduling and GPU/compositor time cannot be separated. They did not repeat at
  the same point across runs.
- At 20 Hz the local workerd relay saturates on this host (about 110% CPU). Claims
  time out, pongs are lost and delivery drops to about 7 KB/s, so 20 Hz runs
  measure the local relay, not the client. Steady round trips use 10 Hz.
- Inspected: `private/reports/devlog-images/rust-mp-full-room-loading.png`; raw
  samples in `private/reports/rust-mp/full-room-10hz.json`.

### Free Ride Lobby page while joining (multiplayer)

- Host Game → Select Track → Ride and Join Game now show the **Free Ride Lobby**
  page while the room is claimed and connects, as multiplayer.js `host` shows
  'lobby' before `connect`. The page shows:
  - the room code (or `--------` while the relay assigns one);
  - "Connecting…";
  - Copy invite link and Leave Game;
  - the course photo and name.
- After the join, the loader replaces it. Arlington Free Ride still goes straight
  to the loader, as in the browser.
- The join now runs before the ride load, with the frontend live: the GPU stays
  with the frontend until the room has connected. The joined room then loads
  through the held-room path (`pending_room`), keeping its meet-a-rider arrival.
- Leave Game, or Back, returns to the Multiplayer page. A connect still in flight
  is left as soon as it finishes. A failed host or join returns to its form with
  the error as the notice.
- Copy invite link copies the room's invite link (`invite_url`, as `Net::invite`)
  once the code is known.
- `?joinDelayMs=` (at most 10 s) is a test hook that holds the page before the
  claim.
- Observed (`tools/test_engine_multiplayer_menu.mjs`, local relay, host-only):
  - The host saw the lobby page with its typed code and the share/leave rows,
    then rode.
  - A Rust guest's Back on the lobby page returned to Multiplayer with no ride.
    It then joined and drew both riders (3 players).
  - `test_engine_multiplayer_room_front.mjs`, `test_engine_freeride.mjs` and
    `test_engine_front.mjs` pass.
  - Inspected: `private/reports/devlog-images/rust-mp-menu-lobby.png`.
- Difference: the page shows "Connecting…" until the ride loads. The browser
  fills in the room's rider list when the lobby state arrives; here the join hands
  over to the loader at that point.

Gaps, in planned slice order:

1. A phone measurement of a full room; arrival frames are 25-31 ms on this host.

## Commands

The browser build's debug strip (frame stats, load and error lines in `#status`) is hidden
by default: add `?debug=1` to `/engine.html` or press F3 to show it. Tests read its text,
which stays current while hidden; start-up failures still show. Desktop writes the same
lines to standard error.

```sh
python3 tools/build_engine.py          # browser build into web/dist (part of make web)
cargo run --release -p atv-app --bin fury -- --content private/reports/hosted-content --venue env01
cargo run --release -p atv-app --bin fury -- --content DIR --venue env01 \
  --camera x,y,z,yaw,pitch,focal [--weather on|off] [--steps N] [--graphics original] [--crash gorn] \
  --capture out.png 1280x720
FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content \
  ATV_TEST_URL=http://127.0.0.1:4173 node tools/compare_engine.mjs env01 \
  [out-dir] [--view x,y,z,yaw,pitch,focal]...   # PNGs, heatmaps and a JSON report
FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content ATV_TEST_URL=... \
  node tools/compare_engine.mjs ride env01 [script.json]   # private/reports/engine-parity/ride/
  #   ride options: --graphics original|enhanced|high --crash original|crazy|gorn --region x0,y0,x1,y1
cargo run --release -p atv-app --bin fury -- --content DIR --venue nat01 --free \
  --camera x,y,z,yaw,pitch,focal --capture out.png 1280x720 --gpu-frames 240   # median scene-pass GPU ms (native)
cargo run --release -p atv-app --bin fury -- --content DIR --venue env01 --mute \
  --autodrive drive.json --bench out.json --profile   # per-pass CPU (browser: ?autodrive=...&profile=1)
cargo run --release -p atv-app --bin fury -- --content DIR --script s.json --trace out.json
cargo run --release -p atv-app --bin fury -- --free ...   # terrain free camera, no ride
cargo run --release -p atv-app --bin fury -- --content DIR --venue env01 --host [--room CODE] \
  [--relay http://127.0.0.1:4187] [--name NAME]          # host a room; --room CODE alone joins
FURY_BIN=... FURY_CONTENT=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer.mjs
ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_dropin.mjs   # arrival, reconnect, course change
ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride.mjs   # Arlington Free Ride from the Rust menu
cargo run --release -p atv-app --bin fury -- --quad 29 ...   # a motorcycle (27-46); ride parity: compare_engine.mjs ride ... --quad 29
cargo run --release -p atv-app --bin fury -- --gear 1,2,1,3,3,2,1 --paint 4 ...   # Garage gear/paint; parity: compare_engine.mjs ride ... --appearance 1.2.1.3.3.2.1:4
cargo run --release -p atv-app --bin fury -- --race --venue env01 ...   # race (with --free: race course world, free camera)
FURY_BIN=... FURY_CONTENT=... ATV_TEST_URL=... [RACE_AUTOPILOT=9000] \
  node tools/compare_engine.mjs race env01 [script.json] [out] [laps] [cpu] [seed]
cargo run --release -p atv-game --example determinism -- CONTENT_DIR env01 script.json \
  [--race LAPS CPU SEED] [--out trace.json]   # headless fingerprint, any target
```

Cross-building from macOS: Colima only shares the home directory, so the source is
piped into a `rust:1-bookworm` container with `gcc-x86-64-linux-gnu` and
`gcc-mingw-w64-x86-64` (see Desktop packaging below).

## Desktop packaging

Standalone desktop builds embed the runtime content and are built on the owner's Mac,
never in GitHub Actions; GitHub Actions only compiles and tests the content-free code.
`fury` reads content from a package directory, a single content pack, a pack appended to the
executable or, on macOS, `Contents/Resources/content.pack`. The build, pack format, verification, GitHub
Release publication and signing caveats are in [DESKTOP_RELEASES.md](DESKTOP_RELEASES.md).

## ATV dirt buildup (2026-10-21)

`atv-game/dirt.rs` ports graphics.js `DirtBuildup`: one 0-255 level for the quad and
rider, stepped once per accepted simulation frame (and on the Reset frame) from the
latest wheel centres, radii, water heights, grounded wheels, surfaces, MUD contacts and
`speed_magnitude`. The venue's `[TerrainZoneN]` DirtAnimChangeTime/Amount rule soils the
quad above 10 mph in the zone under most grounded wheels; MUD scenery soils at 0.1 s per
level; wheels in water wash 63.75 levels per second per four submerged wheels; a player
Reset or a new ride cleans it and wreck recovery keeps it. Enhanced and High
(`Graphics::dirt`, applied on change) draw it through the vehicle shader's ATV reveal
(`mudReveal`), as game.js does; Original stays clean. Unlike the browser, Original does
not step the level (it skips the per-step wheel samples), so switching to Enhanced
mid-ride starts clean. `set_heavy`/`splat` carry Alpha Heavy mud's zones and landing
splats for the Alpha port.

Evidence (host Chromium, `compare_engine.mjs ride VENUE SCRIPT OUT --graphics enhanced`,
JavaScript `dirtLevel` sampled per frame against the Rust trace's per-step level): ENV05,
960 steps of throttle with 0.3 steer: 859/859 samples equal, final level 12 both,
vehicle box 0.42 (1280x720) / 0.54 (390x844); ENV01 default script: 473/475 (two
stale samples), final 6 both. The unit test checks zone soiling, the speed gate, MUD,
water washing and Heavy mud against values from web/graphics.js.
