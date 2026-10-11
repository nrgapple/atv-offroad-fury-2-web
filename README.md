# ATV Offroad Fury 2 — Rust browser project

Project home: `/Users/nrgapple/Projects/atv-offroad-fury-2-web` on the MML Mac mini (`ssh mac-mini`).

## Goal

Reverse engineer the user's ATV Offroad Fury 2 game and develop a Rust-based version playable in a browser. Establish working original gameplay first, optimize it for browser performance, then add optional modern graphics and more realistic physics.

The user requested **project setup and a resource handoff in this session**. A different model will determine the implementation approach. No game port, graphics overhaul, physics changes, or working browser game has been implemented.

## Start here

Read [HANDOFF.md](HANDOFF.md) for the exact source build, available files, environment, and next steps. Read [research/RESOURCES.md](research/RESOURCES.md) for the local reference repositories and documentation links.

## Contents

| Path | Purpose |
| --- | --- |
| `private/source/` | Original ZIP and extracted full ISO |
| `private/extracted/` | Boot executable and `SYSTEM.CNF` |
| `private/reports/` | Disc manifest, ELF layout, hashes, and import results |
| `references/PS2Recomp/` | Local copy of the PS2 static recompiler project |
| `references/ATV-Quadzilla-Recomp/` | Local copy of the related first-game recomp project |
| `tools/` | Reproducible import, file extraction, and validation commands |
| `research/` | Goals, source-build facts, and resource index |
| `crates/` | Reserved for future Rust code; currently empty |
| `web/` | Reserved for the future browser app; currently empty |
| `work/` | Ignored scratch files and logs |

Game files, generated analysis, and reference checkouts are excluded from this repository's Git history. The original download on the user's computer was left intact. No remote repository was created or published.

## Commands

```sh
cd /Users/nrgapple/Projects/atv-offroad-fury-2-web
make verify
make inventory
make import
# Optional complete source hash verification:
make verify-full
# Extract an individual file from the disc, using its manifest path:
python3 tools/extract_file.py /MODULES/PADMAN.IRX
```

The example file path should be checked against `private/reports/disc-files.json` before extraction. Tools use Python's standard library and require no package installation.

## Intended progression

1. Recover the code and asset formats for this exact game build.
2. Deliver a playable Rust/WebAssembly browser baseline with original behavior.
3. Optimize startup, memory use, asset streaming, and frame time against measured browser results.
4. Add modern lighting, materials, shadows, and other graphics improvements as optional layers.
5. Explore optional physics changes for suspension, tire grip, weight transfer, jumping, and landing, with comparisons to original behavior.

These are project goals, not completed milestones or a settled technical design.

