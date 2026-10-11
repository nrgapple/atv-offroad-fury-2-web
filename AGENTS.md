# Working in this project

- Read `README.md`, `HANDOFF.md`, and `research/RESOURCES.md` before making implementation choices.
- The user's long-term target is ATV Offroad Fury 2 in a browser using Rust, followed by optimization, modern graphics, and optional more realistic physics.
- This initial session was explicitly limited to setup and handoff. Future implementation follows the user's instructions to the next model.
- Do not describe this setup as a playable game, completed decompilation, or working recompilation.
- The available source is USA v2.00, `SCUS_972.11`. Its symbol table is empty. Addresses and named functions from the first game's reference project are not verified for this build.
- Preserve the source files under `private/`. Do not commit or publish game binaries, assets, BIOS files, generated source containing game code/data, or the reference repositories.
- Keep original behavior as the baseline. Put graphics and physics enhancements behind explicit options once a baseline exists.
- Record evidence and uncertainty. Choose and document the implementation strategy before building it; Rust migration is not an automatic conversion of the PS2 binary.
- Read each reference repository's own instructions and license before modifying or incorporating it. References are research inputs, not project dependencies yet.
- Keep intermediate work under `work/`; persistent analysis under `private/reports/`; shareable notes under `research/`.
- Use `make verify` for setup validation. Add relevant build, formatting, linting, and tests when real code is introduced.

