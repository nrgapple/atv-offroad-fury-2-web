# Implementation handoff

## Current scope

The setup-only session is historical. On 2026-10-11 the user authorized implementation of ATV Offroad Fury 2 in Rust for browser play. Preserve the original as the baseline, including music and sound; follow with optional modern graphics, genuine 16:9 presentation, animations, and more realistic physics retaining arcade accessibility.

All authored UI should evoke the PlayStation 2 era. Support browser-exposed controllers plus intuitive iOS touch driving/trick gestures. The user explicitly selected Reverse Engineer Anything (REA) as an analysis tool, with CLI versus MCP left to engineering judgment. The latest steering prioritizes the actual fullscreen game and reverse engineering over more workbench or website presentation. See [strategy](research/IMPLEMENTATION.md) and [resources](research/RESOURCES.md).

## Current result

A Rust workspace and fullscreen browser driving reconstruction exist. **This is not the full game or a completed decompilation.** Public v6 opens through the original title → Main Menu → Single Player → Freestyle route, with driving scenes for ENV01/Caribou Peak and ENV02/Tioga Valley. A fresh public frontend test passed the route through Tioga driving, controller input, Caribou selection on a 390-pixel viewport, pause/return behavior, and zero JavaScript errors. The current local build adds a pause panel from recovered overlay art; the tested Resume, Controls/back and Quit actions use authored web callbacks, not the original callbacks. Public v5 introduced 14 normal rider clips blended from filtered steering, lean and air state plus a separate reverse path; the public regression confirmed all 18 rider nodes animate and respond to controls. The current local mobile layout uses the right half of the game view for hold/release preload and independent lean dragging; pulling back on the left pad brakes, then transitions to reverse near a stop. The revised Chromium tests passed cancellation, simultaneous input, pause/focus, no-overflow and game-only selection suppression. A real WASM run on a Caribou ramp armed native preload, activated it on release, and lifted all four wheels. That is one verified ramp jump, not complete stunt support; the pause and touch additions are not in public v6. Tioga's `ruins.col` contains four NaN plane records for zero-area faces; only those faces are omitted from query geometry while original indices and tree order are preserved. Other non-finite geometry remains rejected. The ATV's 27 mechanical mesh nodes, recovered vehicle routines, static type-0 obstacle contacts and single-player follow camera also run in WASM; original engine samples and chunked stereo music play through Web Audio. Other original menu callbacks and modes, race HUD, additional environments, trick/crash animation, hand/foot IK, dynamic collision variants, racing and progression remain unfinished. See the live dev log and component notes for evidence. The browser executes the authored Rust/WebAssembly port, not the PS2 disc executable or an emulator. The development content browser and control test surface are separate at `/workbench.html`.

| Capability | Evidence and limits |
| --- | --- |
| RES/LSI directory and payload recovery | 487 archives, 58,598 member entries, 91,522 zlib blocks; zero archive failures |
| VAG preview decoding | 192 standalone files (162 mono, 30 stereo), 798 packed entries decoded; SPU2 mixer/resampler/reverb and game scheduling not implemented |
| PST preview decoding | All 934 loading archive PST entries and 13 ATV01 material textures decoded; indexed 4/8-bit mip chains validated |
| Fullscreen renderer | Public v6: title/menu/Freestyle route, ENV01 and Tioga Valley scenes; animated 27-node ATV linkage and 18-node normal rider pose blends; recovered single-player follow camera; static type-0 collision query. Other original menu callbacks/modes, race HUD and additional maps are not implemented. |
| ELF reconnaissance | 5,599 direct-call target candidates; mixed code/data means these are not verified function identities or boundaries |
| Browser integration | Actual WASM, image pixels, audio playback clock, malformed input, private path isolation, synthetic gamepad, keyboard/focus, narrow layout, simultaneous touch/cancellation, camera cycling, and graphics-context recovery tested |
| Real content in browser | Public v6 on commit `989214801f4f9a3f33971d3d0c65f91c4f6d816a` autoloads original runtime content and passes a public title/menu/Freestyle route test through Tioga, plus controller and mobile viewport checks. The newer local hold/release preload control also produced a native-gated Caribou ramp jump; it is not in public v6. |
| Original fidelity | Component instruction-interpreter comparisons cover selected vehicle, camera and collision routines; no PS2 hardware validation or whole-game replay equivalence yet |

The numerical audit covers entries, not unique assets. Binary decoding and an advancing audio clock do not prove visual/audio fidelity. A stereo reference comparison against FFmpeg differs by up to 70/68 PCM units (left/right); it is not a bit-exact pass. See [format notes](research/FORMATS.md).

## Project and access

- Project: `/Users/nrgapple/Projects/atv-offroad-fury-2-web` on MML (`ssh mac-mini`, user alias `mml`).
- Host: Apple Silicon arm64, macOS 27.0; private GitHub remote https://github.com/nrgapple/atv-offroad-fury-2-web with main CI checks and WASM builds.
- Applicable parent instructions: `/Users/nrgapple/Projects/AGENTS.md`.
- Original download was preserved at `/Users/adamgeiger/Downloads/ATV Offroad Fury 2 (USA) (v2.00).zip` during setup.
- Public production: `https://atv-offroad-fury-2.nrgapple.chatgpt.site/`; screenshot log at `/devlog.html`. Public v6 serves authorized extracted runtime resources separately from GitHub source/CI, starts without disc selection, and includes the recovered title/menu route with Caribou Peak and Tioga Valley. The newer hold/release preload controls are local-only pending a later release. The ISO, BIOS, raw boot executable, unrelated disc data and reverse-engineering output remain private. The authored browser port runs as Rust/WebAssembly, not as PS2 emulation. See `research/DEPLOYMENT.md` for details.
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

The public Site and local browser build now load original runtime content automatically through their manifest. `/workbench.html` retains inspection tools. See `research/GEOMETRY.md` for the 26-quad/40-terrain audit and `research/COLLISION.md` for collision coverage. Physics data and exact-build loader evidence are in `private/reports/physics-*.json`; selected pseudocode and instruction bytes are in `private/reports/ghidra/`. Reuse the database through `tools/analyze_ps2.py`; do not repeat imports or mistake observed functions for verified semantics.

Continue from public v6's title-to-Freestyle route and two connected maps. The 59-page UI archives, 120 layouts, fonts, and animation data now feed the route, but most pages and callbacks are not yet implemented; race HUD and other modes remain to be built. Tioga collision loads after a targeted skip of four zero-area faces with NaN planes; see `research/COLLISION.md`. A local touch-control update maps hold/release to the original lean flick and passed multi-touch/input checks plus one actual Caribou ramp jump; it is not public yet. Next broaden menu/map coverage, recover original spatial-grid obstacle order and remaining dynamic collision types, and extend rider animation with input-driven crouch/look, IK, tricks and crashes, plus races and progression. The single-player follow camera and ATV mechanical linkage now run in the browser; camera clearance against objects, original render-distance policies and whole-game fidelity remain incomplete. Validate each recovered component against original instructions without presenting that result as PS2 hardware or full-game equivalence. REA still needs an explicit R5900 provider profile; direct PS2 Ghidra observations are not REA Evidence.

Keep enhancements behind explicit options after the baseline. The full-screen renderer uses the recovered single-player follow camera and animated ATV mechanical parts, with provisional terrain shading/LOD. Normal riding animation blends are connected in public v5, while crouch/look inputs still use constructor defaults and IK, tricks and crash poses are unfinished. Public v6 adds the recovered title/menu/Freestyle route and two driving scenes. The local build's newer touch preload produced one Caribou ramp jump, but physical iOS behavior and original stunt/scoring systems remain unverified or incomplete. Object-aware camera clearance, moving obstacle collisions, full race systems and original multiplayer/split-screen presentation also remain incomplete. Widescreen framing preserves recovered 4:3 vertical framing while extending the horizontal view as a browser adaptation. The tested synthetic controller and touch paths do not verify physical Bluetooth controllers or iOS Safari. Audio playback is connected, but original game-wide mixer behavior is still unfinished.
