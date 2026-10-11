# Implementation status

Updated 2026-10-11. The user authorized implementation; the earlier setup-only scope is historical. The full game is **not playable**. No public deployment is authorized.

Completed first slice: shared Rust/native/WebAssembly content readers; a PS2-inspired local-file browser for original archives, supported textures and audio; an input test surface for gamepad, keyboard and touch. Audit coverage is 487 archives, 58,598 member entries, 91,522 compressed blocks, 192 standalone and 798 packed VAG entries, and 934 loading PST entries. Successful decoding is not original-game fidelity.

Current implementation: full-screen original ENV01 terrain and static ATV01 geometry render from the disc. Rust readers recover all 26 quad meshes, 36/40 dense terrain heightfields, 597 distinct finite collision meshes, and all 28 physics configurations. Original scene spawn/direction are loaded from data. The renderer's free camera and provisional terrain shading/LOD are not the original game camera or physics.

Direct PS2-aware Ghidra analysis is working, with original load bytes verified. Physics, INI, terrain and tile loaders have private instruction/pseudocode evidence. REA 6.4.0 rejects this machine-specific MIPS build; see [REA evidence and compatibility gate](research/REA.md). REA needs a supported R5900 profile; MCP cannot fix that rejection. Ghidra observations are not claimed as REA Evidence.

Active implementation: recovering the original drivetrain, suspension/contact solver, and rider animation in parallel, reusing the exact-build analysis database. The full faithful port remains the requested deliverable; intermediate verification is not a completion condition.

Verified checkpoint: the fullscreen renderer loads the actual disc through the existing private HTTPS route, including five original PSM-selected ATV textures. Browser checks passed with no GL errors or exceptions, including GPU context recovery and simultaneous touch cancellation. Rust checks pass with 20 tests. Next gameplay gate: recover and connect original contact/suspension/drivetrain, input, camera, and rider animation into controllable riding. Original audio mixing and race behavior remain unfinished. Proposed input mappings need original-game verification; hardware controller and iOS Safari testing remain open. Enhanced graphics and physics follow the baseline as explicit options. Do not spend more effort on a landing page or marketing copy.

See [strategy and acceptance gates](research/IMPLEMENTATION.md), [handoff](HANDOFF.md), and [format evidence](research/FORMATS.md). Private reports and intermediate outputs remain ignored by Git. The existing private Tailscale route requires the host workbench server to remain running.
