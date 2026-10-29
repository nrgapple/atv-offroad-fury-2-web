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
| `crates/atv-game/`, `atv-render/`, `atv-ui/`, `atv-content/`, `atv-app/` | The Rust engine: game layer, wgpu renderer, menus/HUD, content loading and the one winit app that builds the web page and the desktop `fury` binary (research/RUST_ENGINE.md) |
| `crates/atv-net/` | Multiplayer snapshot codec, compaction and smoothing |
| `crates/atv-relay/` | Rust Cloudflare Durable Object room relay (built with `worker-build`, config in `relay/`) |
| `crates/atv-tools/` | Native tools for the private disc: audit, extraction, ELF reconnaissance |
| `web/` | The game page `play.html` (the Rust engine, `crates/atv-app` built by `tools/build_engine.py`; `engine.html` is the same page under its pre-switch name) and its shell: landing, install, offline cache, analytics, dev log. The JavaScript game was retired at the 2026-10-23 switch-over (research/SWITCHOVER.md); a pinned pre-switch build remains a parity reference (`tools/js_reference.py`) |
| `cloudflare/` | Production Worker: static assets, R2 runtime content, multiplayer admission guards |
| `tools/` | Build, packaging, release, verification, analysis and browser test scripts (`test_*.mjs` need Playwright and a server) |
| `research/` | Shareable notes per system, deployment record, dev-log metadata |
| `private/`, `references/`, `work/` | Ignored: source disc, extracted data, analysis/Ghidra output, reference repos, intermediate outputs |

Runtime resources reach the browser through a separate allowlisted R2 content
manifest, never GitHub or CI. The Rust engine simulates and renders on the page's
main thread (WebGL2 through wgpu); desktop builds embed the same content.

## Component status

"Recovered" means implemented from exact-build routines with bounded instruction
comparisons; "authored" means designed for the port. Each note's own status
section is authoritative.

| Component | Status | Notes |
| --- | --- | --- |
| Resource formats | Recovered layouts; all 487 archives parse | `FORMATS.md`, `GEOMETRY.md`, `IPUM.md` |
| Maps | All 40 Freestyle venue families load (terrain, sky, scenery, vegetation) | `FREESTYLE_MAPS.md`, `MAP_FIDELITY.md` |
| Rendering | Authored WebGL2 using recovered materials, animated UVs, fog, ice reflections, near-vegetation colors, water and wheel effects, the recovered scene-model LOD selection, 130 ft multi-pass cull and single-zone visibility on every venue route (High keeps the finest level); actor/vehicle LOD, lighting/shadows and GS precision incomplete | `MAP_FIDELITY.md`, `STADIUM_MATERIALS.md`, `PERFORMANCE.md`, `VISIBILITY.md` |
| ATV physics | Recovered vehicle routines (suspension/travel stop, static contact, terrain contact, wheel support, drivetrain, rider force) on all 26 quads. Native fixed tick and whole-step parity unestablished; residual landing/slope tire overlap mostly explained by original ordering and contact-model properties (one-step display order, travel bottoming, single-point contact) | `MOVEMENT.md`, `CONTACT.md`, `TERRAIN_CONTACT.md`, `COUPLED_LANDING.md` |
| Collision | Recovered static COL queries, scene partition/candidate order, shipped vegetation proxies, moving train bodies. Some proxy kinds and model-COL resolution remain | `COLLISION.md`, `OBSTACLES.md`, `SCENE_COLLISION.md`, `VEGETATION_COLLISION.md`, `TRAIN_CONTACT.md` |
| Wrecks and recovery | Recovered classifier, boundary launch, deep-water failure and recovery; post-loop full-correction pass for moving bodies; detached rider contacts terrain, static props, vegetation proxies and train bodies | `WRECK.md`, `BOUNDARY_LAUNCH.md`, `WATER_SNOW_GATE.md` |
| Rider and animation | Recovered MOT decoding, pose selection, crouch, stunt, wreck and hard-landing clips; the recovered hand solver stays gated off as in normal riding; foot IK, the event system and female hair unfinished | `ANIMATION.md` |
| Stunts and scoring | Recovered scoring tick/finalization and repeat rollback for single-player driving; full chaining/modes incomplete | `HUD.md`, `TOUCH.md` |
| Camera | Recovered follow camera and wreck camera; widescreen, portrait, uphill and tight-crash framing are browser adaptations | `CAMERA.md`, `HILL_CAMERA.md` |
| Water, weather, ambient | Recovered buoyancy kernel and dynamic heights, visible surfaces; weather/emitters from source TOY records with bounded GPU adaptations; keyframed TOY actors (crows, fish, windmills, blimps, twisters...) load, loop and draw on every venue route; their looping sounds, moving collision and trigger-driven sequences (SX10 roofs) are open | `WATER.md`, `WEATHER.md`, `AMBIENT_EFFECTS.md`, `AMBIENT_ACTORS.md`, `WHEEL_EFFECTS.md` |
| Arlington train | Route, wheel motion, collision, sound and locomotive smoke connected | `TRAIN.md`, `TRAIN_CONTACT.md` |
| HUD and minimap | Authored controller using original layout/fonts/atlases; GPS minimap is authored | `HUD.md`, `MINIMAP.md` |
| Menus and UI | Browser-native screens driven by Rust menu logic; Profile Editor and View Profile pages over a local profile book; Profile Store unlocks and memory-card UI missing | `UI.md`, `MENU_SCREENS.md`, `DESIGN_SYSTEM.md` |
| Audio | Recovered VAG decoding, 21-song catalog/shuffle streamed as AAC with VAG fallback, Jukebox player, engine banks, impacts, train; SPU2 mixer/reverb not reproduced | `AUDIO.md` |
| Racing | All 32 National-rules race courses plus the 16 original Enduro courses (gate races) and 3 Short Tracks; recovered CPU driver decisions (Normal/Expert, jump preload, track-edge and cheat flags, the Enduro gate driver and off-track-only static avoidance) inside authored race rules | `RACING.md` |
| Games (mini-games) | Hockey rules recovered (face-off, drop, goals, hat tricks, sudden death, game over) and playable from a temporary Main Menu row on Coonce Arena by one human against an empty net, the puck an authored flat-ice body shown as a screen tag; Tag's recovered ball-tag rules (possession, scoring, knock loose, carrier engine handicap, boundary throw-in, tie order, Games Prep time limit) play in multiplayer rooms on the fourteen original `*_TG.RES` courses with the `tagball.psg` model (Games → Tag hosts the room); King of the Hills and Treasure Hunt play alone, in local split screen, and in multiplayer rooms on every original `*_KH`/`*_TH` course (Games → Riders: Online Room hosts the room, or the room host's Pause row) with the same recovered rules | `GAMES.md`, `MULTIPLAYER.md` |
| Multiplayer | Authored four-player rooms through the Rust relay: Freeride, Tag (on the original TG courses), Hockey and host-started room races (National, Enduro and Short Track courses, shared grid and countdown, per-rider laps, host-agreed results, leavers ranked DNF, late joiners spectate, the race survives a host handoff; King of the Hills and Treasure Hunt for every room rider on the original KH/TH courses (Games → Riders: Online Room or the host's Pause row; host-authoritative pickups, guest-reported hill captures checked by the host, shared start, agreed results, Play Again, leavers dropped, late joiners spectate, host handoff); no CPU fill, championships or online Freestyle yet); authored rider contacts; local split screen (Multiplayer → Split Screen: Freestyle and races with CPUs, top/bottom or side by side, the original split camera presets, per-controller and split-keyboard input, combined race results; player 2's own ATV and rider, engine sound, wheel dust, rumble, terrain patch, and a shared race place with CPUs avoiding player 2; split Games: King of the Hills, Treasure Hunt, Hockey and Tag with per-view HUDs; three and four players in quarter views with the original quarter camera presets, per-pad input and no CPUs, as the manual specifies; the original ScreenLayout HUD positions, per-seat gear/paint and per-player tire tracks/dust); no original protocol | `MULTIPLAYER.md`, `SPLIT_SCREEN.md`, `MULTIPLAYER_ORIGINAL.md`, `SECURITY.md` |
| Motorcycles | Riding controller recovered component by component from MX vs. ATV Unleashed (steer/bank, tires/suspension, engine/clutch, wheelie/stoppie, air control, preload, flips, wreck rules) through authored terrain/surface/input adapters; native whole-step parity unmeasured; Single Player only | `MOTORCYCLES.md` |
| Crash effects | Optional authored presentation layer (Crazy/Gorn); enhancement-isolation checks show it leaves the simulation unchanged | `CRASH_EFFECTS.md` |
| Input | Remappable keyboard/controller bindings; one-handed and two-thumb touch are authored adaptations | `CONTROLS.md`, `TOUCH.md` |
| Progression and saves | Recovered race Profile Points award, per-course race record and racing championships (Am/Pro Enduro, Nationals and Supercross: rounds, 7/4/2/1/0 points, advance rule, trophies, rewards, unlocks); up to four named local profiles (the old implicit save migrates to PLAYER1) with the original Profile Editor (create, load, copy, delete) and View Profile (Overview, Trophies, Records); the original Profile Store (Unlock Items: ATVs, gear, courses, games, cheats) with Single Player unlock gating and the cheat-opened Short Tracks championship; a port-only one-press Unlock Everything (Profile Editor, Unlock Items and Pass Key pages; this profile or all profiles; synced with the Pass Key; no points, records or trophies granted) and a Game Settings device toggle so new profiles start unlocked; Amateur/Pro Freestyle championships and the standalone Single Player → Freestyle Objectives mode with all four objective types (green/red target runs from the courses' OB archives: cherries, one-time trigger spheres, bonus points/time, signs, the seconds-left bonus); jump/combo records and the racing Hall of Fame (per-course top-three laps and races, longest jump, most combos, highest stunt score, the race-over name entry, Main Menu → Hall of Fame and the loading-screen holders, with the original "New …" / "New Personal …" record messages); the original six-lesson Training Course (Main Menu row, recovered lesson rules, Talon and ATV2 gear reward; spoken lines shown as captions, the original bobbing arrow.slt pointers, no jumbotron replay); the original mini-games framework with Treasure Hunt and King of the Hills (Single Player → Games, recovered rules, time limit, store gating, results; one player, markers instead of the game models; Tag and Hockey pending); Freestyle objective Profile Points, Freestyle records and Hall of Fame; memory-card UI missing. Saves are local first and sync to the cloud under an automatic Pass Key (badge on the title/menus, Pass Key page: copy, QR, share codes, enter a key or share code, merge or replace, export/import save files; `#key=`/`#share=` links). Garage selections and settings stay on the device | `PROFILE.md`, `CHAMPIONSHIPS.md`, `PROFILE_STORE.md`, `TRAINING.md`, `GAMES.md`, `CLOUD_SAVE.md` |

## Build and test

Toolchain: Rust (`~/.cargo/bin/cargo`) with `wasm32-unknown-unknown`, rustfmt
and Clippy; Python 3; Node; Playwright with Chromium for browser tests.

```sh
make verify         # source identity, reference revisions, Git exclusions
make check          # secrets, repository hygiene, fmt, workspace tests, Clippy, JS unit tests, Python tool tests
make web            # build the browser app into web/dist
make relay          # build the multiplayer relay Worker
make serve          # host-only dev server on 127.0.0.1:4173 (BIND/PORT override)
PLAYWRIGHT_MODULE=/path/to/node_modules/playwright make browser-smoke
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
  ignored `private/reports/devlog-images/` and reach R2 through
  `tools/upload_cloudflare_content.py`.
- Any chat may upload to R2 when its work needs it; no release owner is
  required. Runtime resources go first with `--content-only` (additive,
  locked). Screenshots/gallery follow the deployed artifact from
  `tools/fetch_site_release.py --deployed`; verify with
  `tools/verify_cloudflare.py`. `tools/release_checkpoint.py` remains optional
  coordination for combined checkpoints.
- `tools/deploy_local.py --user-requested` is a fallback only when the user
  explicitly asks for a host deployment. No Mistakes stays disabled; the
  experimental local release queue was removed.

## Open issues and known limits

- Whole-game parity is unverified: no PS2 hardware or deterministic full replay
  comparison exists. Trigonometry and VU arithmetic use host approximations.
- Freestyle objectives: Select Objective is a front-end page and the round
  ends with Next Course (the original picks in-game and ends a round by
  quitting the event); target hits test the
  quad's wheel spheres, hull points and actor rays (`002fae60` registers no
  rider body) but not the chassis rigid body's or `f.col`'s own shapes;
  Treasure Hunt and King of the Hills keep the ATV origin test
  (`CHAMPIONSHIPS.md`, `GAMES.md`). Target, Treasure Hunt bomb and Hockey hat-trick
  emitters run from their TOYs (`AMBIENT_EFFECTS.md`).
- Single Player mini-games have no opponents: the original runs them split
  screen (or online) with human riders only. Multiplayer → Split Screen →
  Games plays King of the Hills, Treasure Hunt, Hockey and Tag for two
  local players (`SPLIT_SCREEN.md` "Slice 3", "Slice 4").
  Single Player → Games plays Treasure
  Hunt and King of the Hills alone (with the original bombs, hill flags,
  hill pills and compass) or in an online room (Riders: Online Room;
  guests' pickups and captures carry relay latency and a guest's capture
  is host-checked, `MULTIPLAYER.md` "Room games"), Hockey is
  a one-human practice and Tag plays only with other humans in a hosted
  room (no CPU riders, as in the original); a guest carrier's tricks do not
  knock the ball loose (not relayed), riders share one Tag start, and the
  ground respawn's rays and the ball test only the heightfield, not
  static objects or stadium walls (`GAMES.md`).
- Split screen runs two to four players (Freestyle, races, and the Games
  King of the Hills, Treasure Hunt, Hockey and Tag) with the original
  Overlays.pb `ScreenLayout` HUD positions, each seat's own gear and paint,
  per-player tire tracks and lingering dust, the original MultiATVRiderSelect
  Players page (each pad joins, dresses and readies its seat), the shared
  split race radar and the quarter stunt-text clock. The Players page's ATV
  stats row and original panel art, the Multi-Sign animation, split
  objectives/championships, records and profile credit for players 2-4 (a
  product decision: whose profile), a per-player audio mix, and online
  parity for split modes remain (`SPLIT_SCREEN.md`).
- Memory-card UI is missing; the Training Course shows
  captions for its 46 spoken lines and lacks the jumbotron replays (`TRAINING.md`);
  races, championships and Profile Store unlocks save to the loaded profile
  of a local four-profile book (`PROFILE.md`, `CHAMPIONSHIPS.md`,
  `PROFILE_STORE.md`).
- Race rules are authored around recovered start slots, track window and
  finish-gate laps; points are open (`RACING.md`); firework triggers run with
  the original CPU spark streams (`AMBIENT_EFFECTS.md`).
  Reset points, the off-track reset, the committed station, the Enduro CPU
  driver (VehicleAI mode 4, aimed through the next gate) and off-track-only
  static avoidance are recovered. In the idle-player headless sweep CPUs
  still wreck-loop on sx15's jump and occasionally on sx03/sx05 and
  env05endl (`RACING.md` "Enduro CPU driver and off-track avoidance").
- Rendering lacks point lights, shadows, light lens effects and native GS
  precision (`MAP_FIDELITY.md`). Scene-model LOD and the multi-pass far cull
  are recovered; keyframed actors and vehicles still draw their finest level,
  and multi-zone portal traversal is unrecovered (no shipped scene uses it)
  (`VISIBILITY.md`). ScreenRain lens drops
  are dormant in shipped content and intentionally not drawn (`WEATHER.md`).
- Residual tire/terrain overlap on landings and cross-slope turns follows mostly
  from original ordering and contact-model properties; about 0.12 ft on landings
  is not decomposed, and hardware equality is unverified (`COUPLED_LANDING.md`).
- Audio scheduling and the SPU2 mixer/reverb are approximations (`AUDIO.md`).
- Motorcycle controller components are recovered, but the chassis collision
  hull, COL-v2 routing, stunt linking/scoring and native whole-step parity are
  open; wrecked bikes still use an authored terrain clamp (`MOTORCYCLES.md`).
- Multiplayer has no server reconciliation; contacts are authored, and
  geographically separated latency is untested (`MULTIPLAYER.md`).
- Physical iOS/Android devices, Safari install flows and Bluetooth controllers
  have only host automation (Chromium/WebKit), not hands-on validation.
