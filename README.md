# ATV Offroad Fury 2 — Rust browser project

Recover ATV Offroad Fury 2 USA v2.00 in Rust/WebAssembly, preserving original gameplay, music, sound, animation, and progression before optional graphics and physics enhancements.

The public Cloudflare app opens with Play and Dev log; Play enters the recovered title, Main Menu, Single Player and Freestyle route into a **fullscreen driving reconstruction, not yet the complete game**. Seven environments are connected: ENV01/02/03/05/06/07/08, including Caribou Peak, Tioga Valley and Arlington. Public checks passed all seven map loads and driving movement with terrain detail and vegetation, plus controller navigation, pause/Controls and 390px Caribou touch input, with no JavaScript or WebGL errors. The browser autoloads original runtime content from a same-origin manifest without a disc picker. Authored game logic runs as Rust/WebAssembly; the runtime files, all 21 songs and approved screenshots live separately in R2 and remain outside GitHub source and CI. Original menu callbacks/modes, racing, progression, race HUD, rider IK, full stunts/crashes and rendering fidelity remain incomplete. The development inspector remains at `/workbench.html`.

## Start here

- [Implementation strategy](research/IMPLEMENTATION.md): chosen approach and acceptance gates.
- [Handoff](HANDOFF.md): exact source build, environment, verified results, and remaining work.
- [Recovered formats](research/FORMATS.md): observed layouts and limits of the evidence.
- [Geometry](research/GEOMETRY.md) and [collision](research/COLLISION.md): original terrain and vehicle recovery.
- [Resources](research/RESOURCES.md): references and reverse-engineering tools.
- [REA integration](research/REA.md): actual CLI probe and current R5900 compatibility blocker.

## Current capabilities

Rust readers share the same implementation between native tools and WebAssembly. The exact disc audit validated all 487 RES/LSI archives, 58,598 member entries, and 91,522 compressed blocks. All 192 standalone VAG files, 798 packed VAG entries, and 934 loading-screen PST entries decoded. Entries are not necessarily unique assets; successful decoding does not establish original rendering or sound fidelity.

Original geometry is now usable: all 26 in-race quad meshes decode, 36 of 40 named terrains pass dense-heightfield checks, and 597 distinct collision meshes pass the baseline finite-coordinate validation. Those decode counts are not playable-map coverage: the public Cloudflare app connects seven environments, ENV01/02/03/05/06/07/08, selected from the recovered menu route. Tioga's `ruins.col` prop collision mesh has four zero-area faces with NaN plane records; the targeted handling skips those faces while preserving source indices and rejecting other invalid geometry. See [UI recovery](research/UI.md) and [collision evidence](research/COLLISION.md). The scenes use the recovered single-player chase camera. The original ATV linkage update animates 27 mesh nodes with steering, suspension and wheel motion. Original terrain and sky textures decode through the Rust IPUM decoder; 174 placed track objects are visible. The static obstacle query loads 176 collision placements, 31 meshes and 41 actor rays. The public renderer now blends the original `_d.pst` detail texture using recovered world scale and near-pass modulation. Its projected-coverage pass selection is still pending; distant WebGL mip levels average away this detail, so this is not a full LOD match. The original VU path uses constant terrain RGB, and the browser-only extra sun lighting has been removed. Four terrain seam cases and unsupported mesh variants remain explicit errors. The latest seven-map build loads original ecosystem placements and draws up to 1,500 nearest instances per map in batches; its visibility policy is provisional and is not the PS2 level-of-detail system. The public Cloudflare detail build passed seven-map tests with original 64×64 detail textures and moving full-sample terrain patches. Mobile renderer checks passed at 390px, including gamepad, touch cancellation, audio-engine startup and graphics-context restoration. The Cloudflare release also passed public frontend and seven-map regression checks. Its original `_d.pst` detail layer now blends RGB and alpha at recovered world scale; projected-coverage selection remains to be recovered, and distant mip filtering loses fine detail. ENV08 (Arlington) now loads after recovering the packed `trussbridge.col` signed-16-bit expansion path.

All 28 physics configuration files parse with original ordering preserved. Exact-build executable analysis located the physics loader and established first-match duplicate-key behavior, units conversions, and spring/damping helpers. The vehicle solver connects recovered drivetrain, suspension, lateral grip, rigid-body integration, rider preload/air control, dynamic center of mass and static type-0 obstacle contacts. Component instruction-interpreter comparisons validate selected vehicle, camera and collision routines; full PS2 gameplay replay equivalence remains unverified.

The workbench supports file filtering, nested archives, supported image previews with aspect-preserving fullscreen, audio previews, text/hex inspection, and an ELF call-target candidate report. The driving test supports browser-exposed gamepads, remapping, keyboard input and simultaneous touches. The public gameplay page keeps Pause and Camera on the toolbar and uses the right half of the gameplay view as a hold/lean/release surface; there are no separate preload, brake or reverse buttons. Its browser regression passed cancellation, simultaneous left/right input, focus/menu pause, a narrow viewport and game-only text-selection suppression. The left half accelerates forward, brakes when pulled back, then reverses once nearly stopped. On Caribou, a real WASM session reached native preload arming on a ramp, activated preload after release and lifted all four wheels; this is one demonstrated ramp jump, not full stunt support. Driving controls and normal rider pose blends are connected to the Rust simulation; most original menu callbacks and modes and the race HUD are still unimplemented, as are trick/crash animation. Physical iOS Safari and Bluetooth controllers remain unverified.

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

**Public HTTPS:** [landing page and browser game](https://atv-offroad-fury-2.champtoday.workers.dev/) and [screenshot development log](https://atv-offroad-fury-2.champtoday.workers.dev/devlog.html). The initial Cloudflare deployment identifies validated main commit `ad605e1b589720bd9b093ae84c25a753ea65a0fc` from CI run `38183457495`. Public frontend and seven-map driving regressions passed. Every approved runtime object and screenshot passed HTTPS size, SHA-256 and MIME verification. See [deployment evidence](research/DEPLOYMENT.md) for current release identity and automation status.

[Private GitHub source](https://github.com/nrgapple/atv-offroad-fury-2-web) has automated checks, WASM release builds and main deployment to Cloudflare. Original content and screenshots are uploaded separately from the private host to the dedicated R2 bucket. `make serve` is only a host development server. The previous Sites v8 remains a historical two-map release.

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

Recovered runtime resources use the authorized Cloudflare R2 manifest and stay out of GitHub source and CI. The ISO, BIOS, raw executable, reverse-engineering outputs and reference repositories remain private. Continue original menu/map recovery, race HUD/progression, dynamic and compound collisions, rider IK/stunts/crashes, camera clearance and original render-distance behavior. The public renderer uses up to 1,500 nearest vegetation placements per map and moving terrain detail patches; these provisional policies do not establish the original visibility or LOD system. Physical iOS behavior, Bluetooth controllers, original stunt scoring and full game-wide audio mixing remain unverified or incomplete.
