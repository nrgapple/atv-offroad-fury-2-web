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

## Commands

```sh
python3 tools/build_engine.py          # browser build into web/dist (part of make web)
cargo run --release -p atv-app --bin fury -- --content private/reports/hosted-content --venue env01
cargo run --release -p atv-app --bin fury -- --content DIR --venue env01 \
  --camera x,y,z,yaw,pitch,focal --capture out.png 1280x720
FURY_BIN=target/release/fury ATV_TEST_URL=http://127.0.0.1:4173 \
  node tools/compare_engine.mjs env01   # writes private/reports/engine-parity/*.png
FURY_BIN=target/release/fury FURY_CONTENT=private/reports/hosted-content ATV_TEST_URL=... \
  node tools/compare_engine.mjs ride env01 [script.json]   # private/reports/engine-parity/ride/
cargo run --release -p atv-app --bin fury -- --content DIR --script s.json --trace out.json
cargo run --release -p atv-app --bin fury -- --free ...   # terrain free camera, no ride
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
