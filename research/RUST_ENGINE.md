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
  FOV kick. The fine terrain patch follows the rider (48-cell rule).
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
keeps music playing. Parity holds and autodrive benchmarks keep rendering. No
multiplayer clock runs in the Rust engine yet; a shared ride must keep its clocks
live when it arrives, as the browser does.

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

## Browser stand-ins in the ported presentation

Labelled for follow-up replacement with recovered behaviour (separate PRs, each with a
measured comparison). Parity above is with the JavaScript build.

| Where | Stand-in | Original / evidence | Replaced |
| --- | --- | --- | --- |
| `atv_core::vegetation_render` (used by `atv-render/vegetation.rs`) | 64² cell scan, behind/range test, 1,500-plant nearest-depth heap and sort | 0012a1c0: spatial-callback candidates, frustum tests for near plants only, 600 depth buckets (u16 depth), submission far-to-near of buckets whose cumulative count is under 1,500 (`private/reports/map-fidelity/billboard-recovery/0012a1c0.c`) | No |
| `atv-render/vegetation.rs` | frame serial held at 0 (the browser passes `frame`, which stays 0) | 0012a1c0 increments its serial every call; a plant unseen for more than 3 frames restarts its fade | No |
| `atv-render/scenery.rs` | lit models: ambient + sun·N·L ("provisional prop lighting") | `use_lighting`/`dynamic_lighting` VU programs not recovered | No |
| `atv-render/scenery.rs`, `cull.rs` | per-object fog-end culling with depth range extended by the largest object | 0012b360 far clip at fog end; native object culling not recovered (behaviour-equivalent optimisation, keep) | Keep |
| `atv-render/water.rs` | projected grid capped at 12,000/8,000 vertices for 16-bit indices; portable reflection; no froth | 00190aa0 strip/row counts without the cap; VU reflection and froth pending | No |
| `atv-render/ambient.rs`, `snow.rs`, `wheel_effects.rs` | bounded GPU rings (4,800 / 10,800 slots), two blend-family draws | native render queue order and pool sizes not recovered | No |
| `atv-render/scenery.rs` | reverse-looping crowd ATMs keep frame 0 | native playback not recovered | No |
| `atv-game/intro.rs` | swoop end pose behind the scene start | the player's follow camera (race port) | No |
| `atv-render/terrain.rs` | detail blend averaged by mipmaps ("projected-coverage pass selection pending") | 00168490 pass selection | No |
| `atv-game/src/ride/mod.rs`, `atv-app/src/riding.rs` | browser frame batching: dt capped at 50 ms, accumulator at 0.1 s, at most four steps per frame, one control sample per batch | the original runs one simulation step per 60 Hz frame with per-frame input | No |
| `atv-game/src/ride/bindings.rs` | keyboard brake-then-reverse and touch drive adapters (`atv_core::browser_input`) | authored for keyboards/touch; the pad path is original | Keep (authored input) |
| `atv-game/src/audio/ride.rs` | engine sample selection once per rendered batch with the batch's summed dt (GameAudio.update) | the original emitter update runs every frame | No |
| `atv-game/src/audio/mixer.rs` | windowed-sinc resampling at load and linear-interpolated pitch (Web Audio semantics), 0.65 master and 0.45 music gains | SPU2 voice pitch/ADSR, mixer and reverb not recovered | No |
| `atv-game/src/ride/race.rs` | CPU recovery after 5 s off the track polygon or 4 s below 3 ft/s; the player handed to the path driver after the finish; equal-mass rider contact proxy | RaceStatus +0xf0 consumer, finish takeover routine and quad-to-quad response not identified | No |
| `atv-game/src/ride/race.rs` | Restart Race keeps the loaded roster | the original reloads the event (new roster draw) | No |
| desktop `f32` math (`atv-core` via std) | system libm for sin/cos/atan2/…; the browser build uses Rust's libm, so native races drift from the JavaScript/browser result | one deterministic implementation on every target (PS2 VU/libm equivalence is a separate question) | Yes: `atv_core::math` (Deterministic simulation maths) |
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
  (`drawRace` in hud.js), which is not ported yet.
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

Not connected yet: the race overlays (place, lap, times, results menu) beyond the
status line, race music and start-lamp sounds, stadium crowd cues and flash bursts,
Supercross fireworks, the National drop gate, Restart Race/Quit menus and the CPU
grid's rest pose before the first step. Restart reuses the loaded roster; the browser
reloads the event and draws a new one.

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
linear by mud alpha; the ATV reveal branch is wired but its dirt buildup is not
ported, so quads stay clean as in Original). Held ENV01 frame of bike 29 after 600
steps, mean absolute difference in the vehicle box against the JavaScript game: 6.19
before, 0.13 Rust browser / 0.15 native after (quad 1 unchanged at 0.59 / 0.63).

Not connected yet (labelled for follow-up PRs): motorcycle engine and impact sounds
(atv-web op 101, `web/audio.js` motorcycle branch; the bike is silent), the
`MotorcyclePresentation` 60 Hz interpolation (`web/motorcycle-presentation.js`, used in
every preset), and races on a motorcycle (a clear "not ported" error; atv-web races
the CPU roster on comparable quads).

## Commands

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
cargo run --release -p atv-app --bin fury -- --content DIR --script s.json --trace out.json
cargo run --release -p atv-app --bin fury -- --free ...   # terrain free camera, no ride
cargo run --release -p atv-app --bin fury -- --quad 29 ...   # a motorcycle (27-46); ride parity: compare_engine.mjs ride ... --quad 29
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
