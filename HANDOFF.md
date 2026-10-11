# Implementation handoff

## Current scope

The setup-only session is historical. On 2026-10-11 the user authorized implementation of ATV Offroad Fury 2 in Rust for browser play. Preserve the original as the baseline, including music and sound; follow with optional modern graphics, genuine 16:9 presentation, animations, and more realistic physics retaining arcade accessibility.

All authored UI should evoke the PlayStation 2 era. Support browser-exposed controllers plus intuitive iOS touch driving/trick gestures. The user explicitly selected Reverse Engineer Anything (REA) as an analysis tool, with CLI versus MCP left to engineering judgment. The latest steering prioritizes the actual fullscreen game and reverse engineering over more workbench or website presentation. See [strategy](research/IMPLEMENTATION.md) and [resources](research/RESOURCES.md).

## Current result

A Rust workspace and fullscreen browser driving reconstruction exist. **This is not the full game or a completed decompilation.** The public production app is now on Cloudflare Workers, with original runtime resources and approved screenshots served separately through its R2 binding. Its Play landing enters the recovered title → Main Menu → Single Player → Freestyle route. ENV01/02/03/05/06/07/08 are public, including Caribou Peak, Tioga Valley and Arlington. The public frontend test passed controller Play, title/menu routes, Tioga driving, pause/Controls and 390px Caribou touch input. The public seven-map test passed loading and movement with terrain detail and vegetation, with no JavaScript or WebGL errors. All 281 initial public runtime/manifest/PNG objects passed size, SHA-256 and MIME checks, and all 21 songs remain in the manifest. The initial Cloudflare release identifies main `ad605e1b589720bd9b093ae84c25a753ea65a0fc`, CI `38183457495`.

The ATV's 27 mechanical nodes, recovered vehicle routines, static type-0 obstacle contacts and single-player chase camera run in authored Rust/WASM. Normal rider pose blends and selected original stunt gestures are connected; full scoring/chaining, crash integration, IK, racing and progression remain unfinished. Tioga's `ruins.col` contains four NaN plane records for zero-area faces; only those faces are omitted while original indices/tree order are preserved. Terrain detail uses the original `_d.pst` layer, and vegetation visibility is provisional. Most original menu callbacks/modes, race HUD, remaining maps and complete PS2 rendering/audio fidelity are still missing. The previous Sites v8 is a historical two-map release. Development inspection remains separate at `/workbench.html`.

| Capability | Evidence and limits |
| --- | --- |
| RES/LSI directory and payload recovery | 487 archives, 58,598 member entries, 91,522 zlib blocks; zero archive failures |
| VAG preview decoding | 192 standalone files (162 mono, 30 stereo), 798 packed entries decoded; SPU2 mixer/resampler/reverb and game scheduling not implemented |
| PST preview decoding | All 934 loading archive PST entries and 13 ATV01 material textures decoded; indexed 4/8-bit mip chains validated |
| Fullscreen renderer | Public Cloudflare: title/menu/Freestyle route, seven environments ENV01/02/03/05/06/07/08; animated 27-node ATV linkage and 18-node normal rider pose blends; recovered single-player follow camera; static type-0 collision query; recovered-art pause panel and revised touch controls. Other original menu callbacks/modes, race HUD and additional maps are not implemented. |
| ELF reconnaissance | 5,599 direct-call target candidates; mixed code/data means these are not verified function identities or boundaries |
| Browser integration | Actual WASM, image pixels, audio playback clock, malformed input, private path isolation, synthetic gamepad, keyboard/focus, narrow layout, simultaneous touch/cancellation, camera cycling, and graphics-context recovery tested |
| Real content in browser | Initial Cloudflare release: main `ad605e1b589720bd9b093ae84c25a753ea65a0fc`, CI `38183457495`; public seven-map, frontend and 281-object HTTPS verification passed. |
| Original fidelity | Component instruction-interpreter comparisons cover selected vehicle, camera and collision routines; no PS2 hardware validation or whole-game replay equivalence yet |

The numerical audit covers entries, not unique assets. Binary decoding and an advancing audio clock do not prove visual/audio fidelity. A stereo reference comparison against FFmpeg differs by up to 70/68 PCM units (left/right); it is not a bit-exact pass. See [format notes](research/FORMATS.md).

## Project and access

- Project: `/Users/nrgapple/Projects/atv-offroad-fury-2-web` on MML (`ssh mac-mini`, user alias `mml`).
- Host: Apple Silicon arm64, macOS 27.0; private GitHub remote https://github.com/nrgapple/atv-offroad-fury-2-web with main CI checks and WASM builds.
- Applicable parent instructions: `/Users/nrgapple/Projects/AGENTS.md`.
- Original download was preserved at `/Users/adamgeiger/Downloads/ATV Offroad Fury 2 (USA) (v2.00).zip` during setup.
- Public HTTPS production: `https://atv-offroad-fury-2.champtoday.workers.dev/`; screenshot log at `/devlog.html`. GitHub main checks/builds deploy authored application assets. A dedicated R2 bucket holds only validated runtime content and approved screenshots, uploaded separately from the private host. The active 15-minute heartbeat maintains that content/gallery and checks deployment identity; it must never publish uncommitted code/log entries or further Sites versions. See `research/DEPLOYMENT.md` for current status and release evidence. The ISO, BIOS, raw boot executable, unrelated disc data and reverse-engineering output remain private.
- `make serve` is a foreground development process, not a persistent service. The server serves an explicit static-file allowlist, never `private/`, `references/`, or the repository root.

## Exact source build

| Item | Observed value |
| --- | --- |
| Disc boot file | `SCUS_972.11` |
| `SYSTEM.CNF` version / video mode | `2.00` / `NTSC` |
| ISO bytes | `4,487,446,528` |
| ELF bytes | `3,543,492` |
| Architecture | ELF32 little-endian MIPS |
| Entry point | `0x00100008` |
| Named symbols/functions | `0`; symbol/string table sections are empty |
| Disc files | `742` |
| Main load segment | file offset `128`, guest address `0x00100000`, file bytes `3,542,912`, memory bytes `4,348,160` |

ISO SHA-256: `fb710837bbb1852f53706df6086476b507b63cc9e522777bf91446ef04cf708c`

ELF SHA-256: `11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`

Disc inventory: 368 `.RES`, 119 `.LSI`, 192 `.VAG`, 29 `.IRX`, 10 `.PSS`, plus auxiliary files. The original ZIP and ISO remain under `private/source/`. Extract selected files on demand instead of duplicating the entire disc.

## Reproducible commands and evidence

See [README](README.md) for `make verify`, `check`, `web`, `serve`, `audit`, `code-map`, and `browser-check`. Native extraction commands:

```sh
~/.cargo/bin/cargo run --locked --release -p atv-tools -- disc-extract \
  "private/source/ATV Offroad Fury 2 (USA) (v2.00).iso" /MODULES/PADMAN.IRX private/extracted/MODULES/PADMAN.IRX
~/.cargo/bin/cargo run --locked --release -p atv-tools -- list path/to/file.res
~/.cargo/bin/cargo run --locked --release -p atv-tools -- extract path/to/file.res 0 private/extracted/member.bin
~/.cargo/bin/cargo run --locked --release -p atv-tools -- wav path/to/file.vag work/verification/preview.wav
```

Persistent reports (ignored by Git):

- `private/reports/import.json`, `disc-files.json`, `elf.json`: source identity and layout.
- `private/reports/content-audit.json`: all archive members, decode statuses, dimensions, and audio metadata.
- `private/reports/code-candidates.json`: provisional call-site/target reconnaissance.
- `private/reports/audio-oracle.json`: measured FFmpeg comparison differences.

Screenshots and temporary audio comparisons live under `work/verification/`. All bulk build/test outputs use the mounted SSD; preserve the existing symlinks.

## Environment

- Rust `1.99.0`, Cargo at `~/.cargo/bin/cargo`; targets `aarch64-apple-darwin` and `wasm32-unknown-unknown`, rustfmt and Clippy installed.
- Python 3, Git, system Node `v25.8.1`, FFmpeg, Xcode clang, uv, Homebrew, and native Capstone available.
- Bundled Node `v24.19.0`: `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node`; use this supported major for REA rather than system Node 25.
- Bundled Playwright module: `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright`; Chromium installed for browser checks.
- OpenJDK `26.0.2.1` available. REA 6.4.0 CLI rejected the exact ELF's machine-specific MIPS architecture. Direct Ghidra 12.1.4 with Emotion Engine: Reloaded 2.1.38 now imports and decompiles selected routines using `r5900:LE:32:default`; all original load bytes match. See [REA and native analysis evidence](research/REA.md). REA integration remains unresolved; MCP does not fix its processor admission.
- Rust dependencies are `serde`, `serde_json`, and `miniz_oxide`; browser code needs no npm build dependencies. Playwright is only a test dependency.
- PS2Recomp and the first-game reference are research inputs, not project dependencies. Their source has not been incorporated or built as this runtime.

## Next work

The public Cloudflare app and local browser build now load original runtime content automatically through their manifest. `/workbench.html` retains inspection tools. See `research/GEOMETRY.md` for the 26-quad/40-terrain audit and `research/COLLISION.md` for collision coverage. Physics data and exact-build loader evidence are in `private/reports/physics-*.json`; selected pseudocode and instruction bytes are in `private/reports/ghidra/`. Reuse the database through `tools/analyze_ps2.py`; do not repeat imports or mistake observed functions for verified semantics.

Continue from the public Cloudflare Play landing page and title-to-Freestyle route; seven environments are connected. The 59-page UI archives, 120 layouts, fonts, and animation data now feed the route, but most pages and callbacks are not yet implemented; race HUD and other modes remain to be built. Tioga collision loads after a targeted skip of four zero-area faces with NaN planes; see `research/COLLISION.md`. The revised touch controls and one native-gated ramp jump now pass public browser checks. The public v8 landing page keeps only Play and Dev log; Play enters the unchanged game at `/play.html`. ENV03/05/06/07/08 now load and drive locally after fixing unquoted-space parsing, a material UV default and packed `trussbridge.col` signed-16-bit expansion; all seven routes now pass the public Cloudflare regression. The seven-map detail build passed tests with original 64×64 detail textures, moving full-sample patches and batched vegetation. 390px renderer checks passed gamepad, touch cancellation, audio-engine startup and graphics-context restoration; menu navigation passed immediately before the terrain-only shader changes. The ENV01 ecosystem archive lists 37,948 vegetation placements, and the local renderer batches up to 1,500 nearest placements per map into 7–24 draws. The original `_d.pst` RGB+alpha detail layer now blends publicly at recovered world scale with near-pass modulation. Projected-coverage pass selection is still pending; distant WebGL mip averages soften detail, and fog/GS rounding plus vegetation visibility are not yet matched to the original. The terrain detail patch already follows the player; current work extends terrain range from 300 to 864 feet, not all scenery. Next broaden menu/map coverage, recover original spatial-grid obstacle order and remaining dynamic collision types, and extend rider animation with input-driven crouch/look, IK, tricks and crashes, plus races and progression. The single-player follow camera and ATV mechanical linkage now run in the browser; camera clearance against objects, original render-distance policies and whole-game fidelity remain incomplete. Validate each recovered component against original instructions without presenting that result as PS2 hardware or full-game equivalence. REA still needs an explicit R5900 provider profile; direct PS2 Ghidra observations are not REA Evidence.

Keep enhancements behind explicit options after the baseline. The full-screen renderer uses the recovered single-player follow camera and animated ATV mechanical parts, with provisional terrain shading/LOD. Normal riding animation blends are connected in public v5, while crouch/look inputs still use constructor defaults and IK, tricks and crash poses are unfinished. Public v7 adds the recovered title/menu/Freestyle route, two driving scenes, a pause panel, and revised touch controls. The v7 public ramp run demonstrated native preload and airborne wheel contacts; physical iOS behavior and original stunt/scoring systems remain unverified or incomplete. Public v8 serves the minimal Play/Dev log landing page at `/` and routes Play to the unchanged game at `/play.html`. Object-aware camera clearance, moving obstacle collisions, full race systems and original multiplayer/split-screen presentation also remain incomplete. Widescreen framing preserves recovered 4:3 vertical framing while extending the horizontal view as a browser adaptation. The tested synthetic controller and touch paths do not verify physical Bluetooth controllers or iOS Safari. Audio playback is connected, but original game-wide mixer behavior is still unfinished.
