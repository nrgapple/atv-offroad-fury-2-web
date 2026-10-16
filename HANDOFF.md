# Implementation handoff

Current-state handoff for Fury Remastered, the Rust/WebAssembly browser port of
ATV Offroad Fury 2. It replaces the earlier running log; read that history with
`git log -p -- HANDOFF.md` (the last log-style version predates the 2026-10-15
cleanup). Checkpoint evidence lives in the dev log entries under
`research/devlog/`, system detail in `research/*.md`, and release records in
`research/DEPLOYMENT.md`. Do not append checkpoint logs here; update this file
only when the current state, architecture or process changes.

## What the project is

- A browser port of ATV Offroad Fury 2 (PS2, USA v2.00, `SCUS_972.11`) written in
  Rust, compiled to WebAssembly, presented with WebGL2/Web Audio. It is not an
  emulator and never runs the disc executable.
- The original game is the baseline. Behavior is recovered one component at a
  time from the stripped executable (Ghidra, bounded instruction comparisons) and
  the original resources. Enhancements are optional layers on top.
- **It is a playable reconstruction, not a completed decompilation or a finished
  port.** Instruction comparisons are component evidence, not whole-game replay
  or PS2-hardware equivalence.
- Production: https://furyremastered.com/ (www redirects to the apex; the
  original https://atv-offroad-fury-2.champtoday.workers.dev/ remains supported).
  Dev log: `/devlog.html`.

Instructions: `AGENTS.md` (worktrees, privacy, delivery), `README.md` (player
features), `research/RESOURCES.md` (source material and references),
`research/IMPLEMENTATION.md` (strategy).

## Exact source build

| Item | Observed value |
| --- | --- |
| Disc boot file | `SCUS_972.11` |
| `SYSTEM.CNF` version / video mode | `2.00` / `NTSC` |
| ISO bytes | `4,487,446,528` |
| ELF bytes | `3,543,492` |
| Architecture | ELF32 little-endian MIPS (R5900) |
| Entry point | `0x00100008` |
| Named symbols/functions | `0`; symbol/string table sections are empty |
| Disc files | `742` |
| Main load segment | file offset `128`, guest address `0x00100000`, file bytes `3,542,912`, memory bytes `4,348,160` |

ISO SHA-256: `fb710837bbb1852f53706df6086476b507b63cc9e522777bf91446ef04cf708c`

ELF SHA-256: `11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`

Disc inventory: 368 `.RES`, 119 `.LSI`, 192 `.VAG`, 29 `.IRX`, 10 `.PSS`, plus
auxiliary files. The ZIP and ISO stay under ignored `private/source/`. Addresses
and names from the first game's reference project are not verified for this
build. Motorcycles use a separate MX vs. ATV Unleashed disc (`SLUS_211.04`); see
`research/MOTORCYCLES.md`.

## Architecture

| Path | Role |
| --- | --- |
| `crates/atv-core/` | Resource readers (RES/LSI, PST, PSG, MOT, TR2, COL, TDF, VAG, IPUM...) and the game simulation: vehicle, contacts, wrecks, rider, camera, race, water, train, HUD/menu logic |
| `crates/atv-web/` | Plain-ABI WASM bridge (no `wasm-bindgen`): numbered operations and batched frames for the browser worker; CPU racers, motorcycles, multiplayer world |
| `crates/atv-net/` | Multiplayer snapshot codec, compaction and smoothing |
| `crates/atv-relay/` | Rust Cloudflare Durable Object room relay (built with `worker-build`, config in `relay/`) |
| `crates/atv-tools/` | Native tools for the private disc: audit, extraction, ELF reconnaissance |
| `web/` | Browser game: `game.js` (render loop/WebGL), `worker.js` (WASM simulation worker), menus, HUD, audio, input/touch, effects; `/workbench.html` inspector |
| `cloudflare/` | Production Worker: static assets, R2 runtime content, multiplayer admission guards |
| `tools/` | Build, packaging, release, verification, analysis and browser test scripts (`test_*.mjs` need Playwright and a server) |
| `research/` | Shareable notes per system, deployment record, dev-log metadata |
| `private/`, `references/`, `work/` | Ignored: source disc, extracted data, analysis/Ghidra output, reference repos, intermediate outputs |

Runtime resources reach the browser through a separate allowlisted R2 content
manifest, never GitHub or CI. The simulation runs in a worker; the main thread
renders the frames it publishes.

## Component status

"Recovered" means implemented from exact-build routines with bounded instruction
comparisons; "authored" means designed for the port. Each note's own status
section is authoritative.

| Component | Status | Notes |
| --- | --- | --- |
| Resource formats | Recovered layouts; all 487 archives parse | `FORMATS.md`, `GEOMETRY.md`, `IPUM.md` |
| Maps | All 40 Freestyle venue families load (terrain, sky, scenery, vegetation) | `FREESTYLE_MAPS.md`, `MAP_FIDELITY.md` |
| Rendering | Authored WebGL2 using recovered materials, animated UVs, fog, ice reflections, near-vegetation colors, water and wheel effects. LOD, vegetation visibility, lighting/shadows and GS precision incomplete | `MAP_FIDELITY.md`, `STADIUM_MATERIALS.md`, `PERFORMANCE.md` |
| ATV physics | Recovered vehicle routines (suspension/travel stop, static contact, terrain contact, wheel support, drivetrain, rider force) on all 26 quads. Native fixed tick and whole-step parity unestablished; residual landing tire overlap unexplained | `MOVEMENT.md`, `CONTACT.md`, `TERRAIN_CONTACT.md`, `COUPLED_LANDING.md` |
| Collision | Recovered static COL queries, scene partition/candidate order, shipped vegetation proxies, moving train bodies. Some proxy kinds and model-COL resolution remain | `COLLISION.md`, `OBSTACLES.md`, `SCENE_COLLISION.md`, `VEGETATION_COLLISION.md`, `TRAIN_CONTACT.md` |
| Wrecks and recovery | Recovered classifier, boundary launch, deep-water failure and recovery; post-loop full-correction pass omitted | `WRECK.md`, `BOUNDARY_LAUNCH.md`, `WATER_SNOW_GATE.md` |
| Rider and animation | Recovered MOT decoding, pose selection, crouch, stunt, wreck and hard-landing clips; the recovered hand solver stays gated off as in normal riding; foot IK, the event system and female hair unfinished | `ANIMATION.md` |
| Stunts and scoring | Recovered scoring tick/finalization and repeat rollback for single-player driving; full chaining/modes incomplete | `HUD.md`, `TOUCH.md` |
| Camera | Recovered follow camera and wreck camera; widescreen, portrait, uphill and tight-crash framing are browser adaptations | `CAMERA.md`, `HILL_CAMERA.md` |
| Water, weather, ambient | Recovered buoyancy kernel and dynamic heights, visible surfaces; weather/emitters from source TOY records with bounded GPU adaptations | `WATER.md`, `WEATHER.md`, `AMBIENT_EFFECTS.md`, `WHEEL_EFFECTS.md` |
| Arlington train | Route, wheel motion, collision, sound and locomotive smoke connected | `TRAIN.md`, `TRAIN_CONTACT.md` |
| HUD and minimap | Authored controller using original layout/fonts/atlases; GPS minimap is authored | `HUD.md`, `MINIMAP.md` |
| Menus and UI | Browser-native screens driven by Rust menu logic; profiles, championships, unlocks and saves missing | `UI.md`, `MENU_SCREENS.md`, `DESIGN_SYSTEM.md` |
| Audio | Recovered VAG decoding, 21-song catalog/shuffle, engine banks, impacts, train; SPU2 mixer/reverb not reproduced | `AUDIO.md` |
| Racing | All 32 race courses; recovered CPU driver decisions inside authored race rules; difficulty mapping missing | `RACING.md` |
| Multiplayer | Authored four-player Freeride through the Rust relay; authored rider contacts; no split-screen or original protocol | `MULTIPLAYER.md`, `MULTIPLAYER_ORIGINAL.md`, `SECURITY.md` |
| Motorcycles | Riding controller recovered component by component from MX vs. ATV Unleashed (steer/bank, tires/suspension, engine/clutch, wheelie/stoppie, air control, preload, flips, wreck rules) through authored terrain/surface/input adapters; native whole-step parity unmeasured; Single Player only | `MOTORCYCLES.md` |
| Crash effects | Optional authored presentation layer (Crazy/Gorn); enhancement-isolation checks show it leaves the simulation unchanged | `CRASH_EFFECTS.md` |
| Input | Remappable keyboard/controller bindings; one-handed and two-thumb touch are authored adaptations | `CONTROLS.md`, `TOUCH.md` |
| Progression and saves | Not implemented; Garage selections and settings persist locally | — |

## Build and test

Toolchain: Rust (`~/.cargo/bin/cargo`) with `wasm32-unknown-unknown`, rustfmt
and Clippy; Python 3; Node; Playwright with Chromium for browser tests.

```sh
make verify         # source identity, reference revisions, Git exclusions
make check          # secrets, repository hygiene, fmt, workspace tests, Clippy, JS unit tests, Python tool tests
make web            # build the browser app into web/dist
make relay          # build the multiplayer relay Worker
make serve          # host-only dev server on 127.0.0.1:4173 (BIND/PORT override)
PLAYWRIGHT_MODULE=/path/to/node_modules/playwright make browser-check
```

Focused browser suites are `tools/test_*.mjs`; run them with `ATV_TEST_URL`
pointing at a running server and `PLAYWRIGHT_MODULE` set. Real-content checks
need the private hosted-content package. `make audit`, `make code-map` and
`make native-analyze` support private recovery work; native extraction examples:

```sh
~/.cargo/bin/cargo run --locked --release -p atv-tools -- disc-extract \
  "private/source/ATV Offroad Fury 2 (USA) (v2.00).iso" /MODULES/PADMAN.IRX private/extracted/MODULES/PADMAN.IRX
~/.cargo/bin/cargo run --locked --release -p atv-tools -- list path/to/file.res
~/.cargo/bin/cargo run --locked --release -p atv-tools -- extract path/to/file.res 0 private/extracted/member.bin
```

Ghidra 12.1.4 with Emotion Engine: Reloaded imports the exact ELF using
`r5900:LE:32:default`; reuse the existing database through
`tools/analyze_ps2.py`. REA's CLI rejects this ELF's MIPS variant (`REA.md`).
Pseudocode, instruction probes and reports stay in ignored `private/reports/`
and `work/physics/`; `tools/verify_repository.py` rejects decompiler labels and
game resource formats in tracked files.

Work in a dedicated worktree on the SSD (`AGENTS.md`). Cargo output defaults to
the worktree's `target/`; keep bulk output on the SSD.

## Delivery

- Merge tested work to `main`; GitHub Actions (`.github/workflows/ci.yml`)
  checks, builds and deploys authored application bytes to Cloudflare. The
  lane rules (documentation-only, dev-log metadata, full build) are in
  `tools/ci_changes.py` and `research/CI_CHECKPOINTS.md`.
- Record each checkpoint with `tools/update_devlog.py` as one new file under
  `research/devlog/entries/` (`research/devlog/README.md`). Screenshots stay in
  ignored `private/reports/devlog-images/` and reach R2 only through the
  release owner.
- One release owner per combined checkpoint tracks implementation-ready, merged,
  live and gallery-complete with `tools/release_checkpoint.py`, fetches the
  deployed artifact with `tools/fetch_site_release.py --deployed`, uploads R2
  content with `tools/upload_cloudflare_content.py` and verifies with
  `tools/verify_cloudflare.py`.
- `tools/deploy_local.py --user-requested` is a fallback only when the user
  explicitly asks for a host deployment. No Mistakes stays disabled; the
  experimental local release queue was removed.

## Open issues and known limits

- Whole-game parity is unverified: no PS2 hardware or deterministic full replay
  comparison exists. Trigonometry and VU arithmetic use host approximations.
- Progression, championships, unlocks, saves and split-screen are missing.
- Race rules are authored; original track windows, finish-box test, start slots,
  difficulty mapping, firework triggers and points are open (`RACING.md`).
- Rendering lacks original LOD/visibility policy, point lights, shadows, light
  lens effects and native GS precision (`MAP_FIDELITY.md`). ScreenRain lens drops
  are dormant in shipped content and intentionally not drawn (`WEATHER.md`).
- Residual tire/terrain overlap on landings and cross-slope turns is measured
  but not explained (`TERRAIN_CONTACT.md`, `COUPLED_LANDING.md`).
- Audio scheduling and the SPU2 mixer/reverb are approximations (`AUDIO.md`).
- Motorcycle controller components are recovered, but the chassis collision
  hull, COL-v2 routing, stunt linking/scoring and native whole-step parity are
  open; wrecked bikes still use an authored terrain clamp (`MOTORCYCLES.md`).
- Multiplayer has no server reconciliation; contacts are authored, and
  geographically separated latency is untested (`MULTIPLAYER.md`).
- Physical iOS/Android devices, Safari install flows and Bluetooth controllers
  have only host automation (Chromium/WebKit), not hands-on validation.
