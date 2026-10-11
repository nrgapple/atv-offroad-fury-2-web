# Working in this project

- Read `README.md`, `HANDOFF.md`, and `research/RESOURCES.md` before making implementation choices.
- The user's long-term target is ATV Offroad Fury 2 in a browser using Rust, followed by optimization, modern graphics, and optional more realistic physics.
- The initial session was limited to setup. The user has since authorized full implementation of an almost exact port, followed by optional enhancements. Continue real game work rather than stopping at setup or rendering checkpoints.
- Describe component progress precisely. A partial driving reconstruction is not a completed decompilation or full playable port.
- The available source is USA v2.00, `SCUS_972.11`. Its symbol table is empty. Addresses and named functions from the first game's reference project are not verified for this build.
- Preserve the source disc, ISO and extracted working files under `private/`. The user explicitly authorizes hosting the extracted runtime resources required by the browser port through a separate public Cloudflare R2 asset manifest. Keep generated files in ignored private storage until they are transferred to the R2-only hosting package. Never put game content in GitHub source or CI. Never publish the ISO, BIOS, raw boot executable, unrelated disc content, reverse-engineering output/pseudocode, or reference repositories. The browser must run the authored Rust/WebAssembly port; do not substitute PS2 emulation or run the disc executable.
- Keep original behavior as the baseline. Put graphics and physics enhancements behind explicit options once a baseline exists.
- Record evidence and uncertainty. Choose and document the implementation strategy before building it; Rust migration is not an automatic conversion of the PS2 binary.
- Read each reference repository's own instructions and license before modifying or incorporating it. References are research inputs, not project dependencies yet.
- Keep intermediate work under `work/`; persistent analysis under `private/reports/`; shareable notes under `research/`.
- Use `make verify` for setup validation. Add relevant build, formatting, linting, and tests when real code is introduced.

## Ongoing progress and delivery

- Always delegate screenshot/dev-log maintenance to a separate lighter-model agent so it does not block game implementation. The user explicitly requested this ongoing division of work. Keep the main implementation agents focused on the port; send verified checkpoints and images to the blogging agent.
- The user requests a continuously updated screenshot dev log alongside the game on public Cloudflare Workers hosting. At meaningful checkpoints, use `tools/update_devlog.py` to record what changed, discoveries, verification, and limitations. Inspect screenshots before publishing them. Keep working on the game while maintaining the log.
- Screenshot files belong under ignored `private/reports/devlog-images/`, never in the project GitHub repository. The user explicitly authorizes public Cloudflare Workers/R2 hosting of the authored game port, selected checkpoint screenshots, and the extracted runtime resources required for browser autoload through the game's separate R2 content manifest. Keep game content out of GitHub source and CI. Never publish the ISO, BIOS, raw boot executable, unrelated disc content, reverse-engineering output/pseudocode, or reference repositories. Do not claim public no-disc startup is complete until it passes on the public Cloudflare app without a file-selection step.
- The log is `/devlog.html`; metadata is `research/devlog.json`. The game remains the main fullscreen view.
- The user authorizes pushing project code to a private GitHub repository, merging to main, and automatic public Cloudflare deployment without repeated confirmation. Check that game data, reference repositories, generated analysis, screenshots, secrets, and build outputs are excluded before each GitHub push. Tailscale is optional host verification, not the user-facing production path.


- Production is `https://atv-offroad-fury-2.champtoday.workers.dev/`. GitHub CI deploys authored application assets after main checks; the local 15-minute heartbeat maintains only validated runtime content/approved gallery data and verifies the public deployment. The previous Sites v8 is historical; do not publish further Sites versions.
