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
Windows/Linux output devices, inactive render sleep, and broader runtime feature
coverage. The JavaScript game remains available until the shared engine passes
those relevant subsystem gates.

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

## Commands

```sh
python3 tools/build_engine.py          # browser build into web/dist (part of make web)
cargo run --release -p atv-app --bin fury -- --content private/reports/hosted-content --venue env01
cargo run --release -p atv-app --bin fury -- --content DIR --venue env01 \
  --camera x,y,z,yaw,pitch,focal [--weather on|off] [--steps N] [--graphics original] \
  --capture out.png 1280x720
FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content \
  ATV_TEST_URL=http://127.0.0.1:4173 node tools/compare_engine.mjs env01 \
  [out-dir] [--view x,y,z,yaw,pitch,focal]...   # PNGs, heatmaps and a JSON report
FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content ATV_TEST_URL=... \
  node tools/compare_engine.mjs ride env01 [script.json]   # private/reports/engine-parity/ride/
cargo run --release -p atv-app --bin fury -- --content DIR --script s.json --trace out.json
cargo run --release -p atv-app --bin fury -- --free ...   # terrain free camera, no ride
cargo run --release -p atv-app --bin fury -- --race --venue env01 ...   # race course world (Snow), free camera
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
