# Working in this project

- Read `README.md`, `HANDOFF.md`, and `research/RESOURCES.md` before making implementation choices.
- The user's long-term target is ATV Offroad Fury 2 in a browser using Rust, followed by optimization, modern graphics, and optional more realistic physics.
- The initial session was limited to setup. The user has since authorized full implementation of an almost exact port, followed by optional enhancements. Continue real game work rather than stopping at setup or rendering checkpoints.
- Describe component progress precisely. A partial driving reconstruction is not a completed decompilation or full playable port.
- The available source is USA v2.00, `SCUS_972.11`. Its symbol table is empty. Addresses and named functions from the first game's reference project are not verified for this build.
- Preserve the source files under `private/`. Do not commit or publish game binaries, assets, BIOS files, generated source containing game code/data, or the reference repositories.
- Keep original behavior as the baseline. Put graphics and physics enhancements behind explicit options once a baseline exists.
- Record evidence and uncertainty. Choose and document the implementation strategy before building it; Rust migration is not an automatic conversion of the PS2 binary.
- Read each reference repository's own instructions and license before modifying or incorporating it. References are research inputs, not project dependencies yet.
- Keep intermediate work under `work/`; persistent analysis under `private/reports/`; shareable notes under `research/`.
- Use `make verify` for setup validation. Add relevant build, formatting, linting, and tests when real code is introduced.

## Ongoing progress and delivery

- The user requests a continuously updated screenshot dev log alongside the game on public Sites hosting. At meaningful checkpoints, use `tools/update_devlog.py` to record what changed, discoveries, verification, and limitations. Inspect screenshots before publishing them. Keep working on the game while maintaining the log.
- Screenshot files belong under ignored `private/reports/devlog-images/`, never in the project GitHub repository. The user explicitly authorized public Sites hosting of the game build and selected checkpoint screenshots. This does not authorize publishing disc images, source assets, BIOS files, or reverse-engineering artifacts. Disc data stays in the visitor's browser.
- The log is `/devlog.html`; metadata is `research/devlog.json`. The game remains the main fullscreen view.
- The user authorizes pushing project code to a private GitHub repository, merging to main, and automatic public Sites deployment without repeated confirmation. Check that game data, reference repositories, generated analysis, screenshots, secrets, and build outputs are excluded before each GitHub push. Tailscale is optional host verification, not the user-facing production path.

