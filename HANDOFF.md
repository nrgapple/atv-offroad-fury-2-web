# Handoff to the implementation model

## User request

Use the MML Mac mini as the project machine. Start from the ATV Offroad Fury 2 archive in the user's Downloads. Reverse engineer/decompile the game and migrate it to Rust for browser play. Optimize browser performance, then layer on better modern graphics and potentially more realistic physics.

The user clarified: "Just get the project set up. Don't go too deep into the how. I'm gonna have a different model on my computer figuring that out. I just want to have all the resources over there and what our goal is."

## Project and access

- SSH host: `mac-mini`; the user's interactive alias is `mml`.
- Remote project: `/Users/nrgapple/Projects/atv-offroad-fury-2-web`.
- Host: Apple Silicon arm64, macOS 27.0.
- Local original: `/Users/adamgeiger/Downloads/ATV Offroad Fury 2 (USA) (v2.00).zip`.
- Applicable parent instructions: `/Users/nrgapple/Projects/AGENTS.md`.
- Git initialized locally on MML; no remote configured.

## Completed setup

- Copied the original archive to `private/source/` on MML.
- Extracted its full ISO there, retaining both ZIP and ISO.
- Read the ISO9660 filesystem and extracted the boot executable and configuration.
- Recorded the full file inventory and ELF metadata.
- Hashed the ISO and ELF to identify this exact build.
- Copied two related reference repositories locally and recorded their revisions.
- Added reusable import/extraction/validation tools, project instructions, and this handoff.
- Reserved `crates/` and `web/` for future implementation.

## Observed build facts

| Item | Observed value |
| --- | --- |
| Disc boot file | `SCUS_972.11` |
| `SYSTEM.CNF` version | `2.00` |
| Video mode | `NTSC` |
| ISO bytes | `4,487,446,528` |
| ELF bytes | `3,543,492` |
| Architecture | ELF32 little-endian MIPS |
| Entry point | `0x00100008` |
| Named symbols/functions | `0`; symbol/string table sections are empty |
| Disc files | `742` |
| Main load segment | file offset `128`, guest address `0x00100000`, file bytes `3,542,912`, memory bytes `4,348,160` |

ISO SHA-256: `fb710837bbb1852f53706df6086476b507b63cc9e522777bf91446ef04cf708c`

ELF SHA-256: `11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`

Asset extensions observed: 368 `.RES`, 192 `.VAG`, 119 `.LSI`, 29 `.IRX`, 10 `.PSS`, plus auxiliary files. Major directories include `/AUDIO`, `/GEOMETRY`, `/MODULES`, `/NETCONF`, `/QUAD`, `/RIDER`, `/TRACKS`, and `/UI`. Their formats have not been reverse engineered here.

Reports are in `private/reports/import.json`, `disc-files.json`, and `elf.json`. `elf.json` includes sections and load segments, not a recovered function map.

## Available tools

- Rust `1.99.0`; use `~/.cargo/bin/cargo`, `~/.cargo/bin/rustc`, and `~/.cargo/bin/rustup`, since `.cargo/bin` was absent from the noninteractive PATH.
- Installed Rust targets: `aarch64-apple-darwin`, `wasm32-unknown-unknown`.
- Python 3, Git, Node `v25.8.1`, uv, Homebrew, Xcode clang, FFmpeg, and the native Capstone library are present.
- Ghidra was checked through Homebrew but **not installed**. PS2Recomp was copied but **not built**. No new compiler/tool packages were installed during setup.
- No Rust workspace, renderer, runtime, emulator, physics code, or browser app has been authored.

## Reference caveats and decisions left open

The related Quadzilla project targets the first ATV Offroad Fury game and its README documents a build with named symbols. Our second-game v2.00 executable has no named symbols. Treat any transfer of function identities, addresses, or asset knowledge as a hypothesis requiring verification.

PS2Recomp currently generates C++ with a console runtime; it is a potential research/tooling input, not an existing Rust browser port. Decide whether to use it for analysis, adapt a recompilation route, or progressively reconstruct selected engine systems in Rust. That choice was intentionally left to the implementation model.

Browser rendering, console services, audio, input, disc assets, and execution fidelity remain unresolved. Graphics and physics work should follow a working original baseline so changes can be evaluated independently.

## Suggested next action

Run `make verify`, review the private reports and local references, then choose and document a feasible first implementation milestone. The eventual success criterion is actual game content and controllable riding in a browser, with measured performance and an original-behavior baseline before enhancements.

