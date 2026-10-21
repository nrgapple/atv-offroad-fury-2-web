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
| `atv-game` | Platform-independent game layer: venue ids/archives (`launch.rs`, from `web/launch.js`), venue loading (`venue.rs`, from `terrainFromContent`), presentation terrain grid (`terrain_mesh.rs`, from `buildTerrainMesh`) |
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
builds read a package directory (`--content` or `FURY_CONTENT`); embedding the package
into the executables is a later packaging step that never runs in CI.

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

Limitations: water, the ENV08 train, weather drawing and ambient effects are not ported;
camera flashes stay hidden until the stadium crowd simulation drives them; reverse-
looping crowd ATMs keep their first frame (as in the browser); anisotropy is applied at
load only.

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
```

Cross-building from macOS: Colima only shares the home directory, so the source is
piped into a `rust:1-bookworm` container with `gcc-x86-64-linux-gnu` and
`gcc-mingw-w64-x86-64` (see the desktop packaging plan below).

## Desktop packaging plan

Desktop releases contain game content, so they are built on the owner's Mac, never in
GitHub Actions: a universal macOS app natively, Linux and Windows in Docker, with the
content package embedded. GitHub Actions only compiles and tests the content-free code.
Publishing archives that contain game content (for example GitHub Releases) needs the
owner's explicit confirmation first.
