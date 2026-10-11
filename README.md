# ATV Offroad Fury 2 — Rust browser project

Recover ATV Offroad Fury 2 USA v2.00 in Rust/WebAssembly, preserving original gameplay, music, sound, animation, and progression before optional graphics and physics enhancements.

The public v5 build opens directly into a **fullscreen driving reconstruction, not yet the complete game**. The latest local build also has a tested route through the recovered title, Main Menu, Single Player, Freestyle and environment picker. Both Caribou Peak and Tioga Valley currently connect locally; this newer UI route and second map are not in public v5. The browser loads required runtime content from a same-origin manifest without asking for a disc image. Authored game logic runs as Rust/WebAssembly, not PS2 emulation; extracted runtime files are separately hosted on Sites and excluded from GitHub source and CI. Recovered Rust physics, original engine samples and streaming music are connected. ATV steering, suspension and wheel linkages animate; the single-player chase camera follows recovered presets; static type-0 obstacle contacts are connected; and public v5 blends 14 normal rider clips. Other original menu callbacks, modes, maps, race HUD and game flow are incomplete. Trick/crash animation, hand/foot IK, dynamic obstacle variants, racing and progression remain unfinished. The content inspector remains at `/workbench.html` for development.

## Start here

- [Implementation strategy](research/IMPLEMENTATION.md): chosen approach and acceptance gates.
- [Handoff](HANDOFF.md): exact source build, environment, verified results, and remaining work.
- [Recovered formats](research/FORMATS.md): observed layouts and limits of the evidence.
- [Geometry](research/GEOMETRY.md) and [collision](research/COLLISION.md): original terrain and vehicle recovery.
- [Resources](research/RESOURCES.md): references and reverse-engineering tools.
- [REA integration](research/REA.md): actual CLI probe and current R5900 compatibility blocker.

## Current capabilities

Rust readers share the same implementation between native tools and WebAssembly. The exact disc audit validated all 487 RES/LSI archives, 58,598 member entries, and 91,522 compressed blocks. All 192 standalone VAG files, 798 packed VAG entries, and 934 loading-screen PST entries decoded. Entries are not necessarily unique assets; successful decoding does not establish original rendering or sound fidelity.

Original geometry is now usable: all 26 in-race quad meshes decode, 36 of 40 named terrains pass dense-heightfield checks, and 597 distinct collision meshes pass the baseline finite-coordinate validation. Those decode counts are not playable-map coverage: the local renderer connects ENV01 (Caribou Peak) and ENV02 (Tioga Valley), while public v5 still connects only Caribou Peak. The local menu route selects between the two. Tioga's `ruins.col` prop collision mesh has four zero-area faces with NaN plane records; the targeted handling skips those faces while preserving source indices and rejecting other invalid geometry. See [UI recovery](research/UI.md) and [collision evidence](research/COLLISION.md). Both scenes use the recovered single-player chase camera. The original ATV linkage update animates 27 mesh nodes with steering, suspension and wheel motion. Original terrain and sky textures decode through the Rust IPUM decoder; 174 placed track objects are visible. The static obstacle query loads 176 collision placements, 31 meshes and 41 actor rays. Lighting and level-of-detail selection remain provisional. Four terrain seam cases and unsupported mesh variants remain explicit errors.

All 28 physics configuration files parse with original ordering preserved. Exact-build executable analysis located the physics loader and established first-match duplicate-key behavior, units conversions, and spring/damping helpers. The vehicle solver connects recovered drivetrain, suspension, lateral grip, rigid-body integration, rider preload/air control, dynamic center of mass and static type-0 obstacle contacts. Component instruction-interpreter comparisons validate selected vehicle, camera and collision routines; full PS2 gameplay replay equivalence remains unverified.

The workbench supports file filtering, nested archives, supported image previews with aspect-preserving fullscreen, audio previews, text/hex inspection, and an ELF call-target candidate report. The control test supports browser-exposed gamepads, remapping, keyboard input, a floating left driving pad, right-side directional trick swipes, and simultaneous touches. The local original-menu route passed keyboard, synthetic controller, touch, scene-loading, menu-return, pause, narrow-layout and error checks. Driving controls and normal rider pose blends are connected to the Rust simulation; most original menu callbacks and modes and the race HUD are still unimplemented, as are trick/crash animation. Real Bluetooth hardware and iOS Safari remain unverified.

## Run and verify

Requires Rust with `wasm32-unknown-unknown`, rustfmt, Clippy, Python 3, and Node. Cargo uses the configured SSD build storage. The existing `target/`, `web/dist/`, and `work/verification/` SSD symlinks must be preserved.

```sh
make verify       # Source identity, inventory, reference revisions, Git exclusions
make check        # Rust format/tests/Clippy, input unit tests, Python syntax
make web          # Build asset-free renderer and development workbench
make serve        # Host-only server, port 4173; keep running for access
make audit        # Audit private disc content into private/reports/content-audit.json
make code-map     # Candidate direct-call targets, not verified functions
make native-analyze # Reuse the private PS2 Ghidra database; requires local toolchain
make verify-full  # Also rehash the complete ISO
```

`CARGO`, `NODE`, `ISO`, `ELF`, `BIND`, and `PORT` are overridable Make variables. `BIND=0.0.0.0` is appropriate only with an approved access path; it does not itself provide remote access.

Browser checks require Playwright and its Chromium browser plus a running server:

```sh
PLAYWRIGHT_MODULE=/path/to/node_modules/playwright make browser-check
# Optional real-source checks; this file stays local to the test browser:
ATV_TEST_ISO="$PWD/private/source/ATV Offroad Fury 2 (USA) (v2.00).iso" \
  PLAYWRIGHT_MODULE=/path/to/node_modules/playwright make browser-check
```

`ATV_TEST_URL` selects a server; its default is host-only `http://127.0.0.1:4173`. Tests cover actual WASM extraction, texture pixels, an advancing audio playback clock, malformed input, private-path isolation, synthetic gamepad input, focus loss, narrow layout, simultaneous touch gestures, and cancellation. A browser playback clock is not a listening test or proof of accurate PS2 audio mixing.

## Access

**Public HTTPS:** [browser game build](https://atv-offroad-fury-2.nrgapple.chatgpt.site/) and [screenshot development log](https://atv-offroad-fury-2.nrgapple.chatgpt.site/devlog.html). Public v5 starts without selecting a disc and includes the normal rider-animation connection. A fresh run on commit `0dc5c09890df6bbf8f942309558ad168b7a8857b` verified the manifest-backed game and rider pose changes. Public v5 still enters Caribou Peak directly; the recovered menu route and Tioga scene are currently verified only in the local build. Other maps, original menu callbacks, modes and race HUD remain unfinished. The browser port remains incomplete.

[Private GitHub source](https://github.com/nrgapple/atv-offroad-fury-2-web) has automated checks and WASM release builds on main. See [deployment](research/DEPLOYMENT.md) for public Sites publishing and its automation status. `make serve` is only a host development server.

## Project contents

| Path | Purpose |
| --- | --- |
| `crates/atv-core/` | Resource, ISO, audio, texture, terrain, mesh, collision, physics-data, and ELF readers |
| `crates/atv-tools/` | Native private-disc audit and extraction CLI |
| `crates/atv-web/` | WebAssembly interface to the shared Rust readers |
| `web/` | Full-screen reconstruction renderer and separate development inspector |
| `tools/` | Import, validation, web build/server, and browser tests |
| `research/` | Shareable strategy, evidence summaries, and references |
| `private/source/` | Preserved original ZIP and ISO, ignored by Git |
| `private/extracted/` | Selected original files, ignored by Git |
| `private/reports/` | Persistent private analysis, ignored by Git |
| `references/` | Reference checkouts, ignored by Git |
| `work/` | Intermediate outputs and verification artifacts, ignored by Git |

Recovered runtime files may be served by the authorized Sites manifest, but must stay out of GitHub source and CI; the ISO, BIOS, raw executable, reverse-engineering outputs and reference repositories remain private. The private GitHub remote is configured. The latest local checkpoint connects the original title-to-Freestyle route and two driving environments; public v5 still has the earlier direct-to-Caribou start. Continue implementing original menu callbacks, broader map selection, race HUD and progression, plus obstacle ordering, moving and compound collisions, rider IK/tricks/crashes, and camera clearance. Component reconstruction does not establish a full-game match; original audio mixing also remains unfinished.
