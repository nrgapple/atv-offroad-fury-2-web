# ATV Offroad Fury 2 — Rust browser project

Recover ATV Offroad Fury 2 USA v2.00 in Rust/WebAssembly, preserving original gameplay, music, sound, animation, and progression before optional graphics and physics enhancements.

The browser opens directly into a **full-screen reconstruction renderer, not yet a playable game**. A locally selected disc supplies the original ENV01 terrain and ATV01 model. Riding physics, rider animation, race logic, and in-game audio are unfinished. No game assets are bundled or uploaded. The content inspector remains at `/workbench.html` for development.

## Start here

- [Implementation strategy](research/IMPLEMENTATION.md): chosen approach and acceptance gates.
- [Handoff](HANDOFF.md): exact source build, environment, verified results, and remaining work.
- [Recovered formats](research/FORMATS.md): observed layouts and limits of the evidence.
- [Geometry](research/GEOMETRY.md) and [collision](research/COLLISION.md): original terrain and vehicle recovery.
- [Resources](research/RESOURCES.md): references and reverse-engineering tools.
- [REA integration](research/REA.md): actual CLI probe and current R5900 compatibility blocker.

## Current capabilities

Rust readers share the same implementation between native tools and WebAssembly. The exact disc audit validated all 487 RES/LSI archives, 58,598 member entries, and 91,522 compressed blocks. All 192 standalone VAG files, 798 packed VAG entries, and 934 loading-screen PST entries decoded. Entries are not necessarily unique assets; successful decoding does not establish original rendering or sound fidelity.

Original geometry is now usable: all 26 in-race quad meshes decode, 36 of 40 named terrains pass dense-heightfield checks, and 597 distinct collision meshes pass finite-coordinate validation. The renderer places the static quad at the disc's scene start and uses a free camera. Its terrain shading and level-of-detail selection are provisional. Four terrain seam cases and unsupported mesh variants remain explicit errors.

All 28 physics configuration files parse with original ordering preserved. Exact-build executable analysis located the physics loader and established first-match duplicate-key behavior, units conversions, and spring/damping helpers. This is preparation for the original simulation, not a working vehicle solver.

The workbench supports file filtering, nested archives, supported image previews with aspect-preserving fullscreen, audio previews, text/hex inspection, and an ELF call-target candidate report. The control test supports browser-exposed gamepads, remapping, keyboard input, a floating left driving pad, right-side directional trick swipes, and simultaneous touches. These inputs are not connected to riding physics yet. Real Bluetooth hardware and iOS Safari remain unverified.

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

**Private remote:** the user's existing Tailscale route is [the reconstruction renderer on MML](https://nrgapples-mac-mini.tail96c48c.ts.net:8443/). It requires authorized tailnet access, a locally selected ISO, and the host server running on port 4173. The full ISO-to-renderer browser flow has passed through HTTPS from the host; separate-device access has not been demonstrated. No new public tunnel or deployment was created. Local `make serve` alone provides only host access.

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

Source files, recovered content, and reference repositories must not be committed or published. No remote Git repository is configured. The next gameplay gate is the original vehicle/contact solver, input mapping, camera, and animation running together on the recovered terrain. Static rendering does not satisfy that gate. Race systems and original audio mixing remain unfinished.
