# Resources

## Local source material

All source material is on MML under this project's `private/` directory. The full disc is available; individual files can be extracted on demand with `tools/extract_file.py` or the Rust `atv-tools disc-extract` command rather than duplicating the entire image. [FORMATS.md](FORMATS.md) records recovered layouts and validation limits; [IMPLEMENTATION.md](IMPLEMENTATION.md) records the chosen incremental Rust reconstruction strategy.

## Local reference checkouts

| Local path | Upstream | Revision copied during setup |
| --- | --- | --- |
| `references/PS2Recomp/` | https://github.com/ran-j/PS2Recomp | `2c5fbb9389e11dd95693385969490c9e8e6f57b4` |
| `references/ATV-Quadzilla-Recomp/` | https://github.com/lewilou22/ATV-Off-Road-Fury-Quadzilla-Engine-recomp | `21b989812bd0d1fbcee60b25ac381ce8f986e8c8` |

Both were shallow-cloned, retain their own Git metadata, and are excluded from the parent project Git history. Both upstreams identify a GPL-3.0 license; inspect their actual license files before reuse. Cloning does not establish that their code builds or works on this Mac or in a browser.

Useful files already present locally:

- PS2Recomp `README.md`: architecture, limitations, and the workflow for stripped executables.
- PS2Recomp `ps2xRecomp/tools/ghidra/`: function-export scripts and supporting reverse-engineering tools.
- Quadzilla reference `README.md`: findings and debugging log for the first game.

## REA

The user explicitly requested [Reverse Engineer Anything (REA)](https://github.com/morluto/rea). The published 6.4.0 CLI was actually tested on the exact boot ELF and rejected its machine-specific MIPS architecture before analysis. MCP uses the same provider and does not resolve that compatibility issue. See [REA integration evidence and next compatibility gate](REA.md). No pseudocode or verified REA function map was produced. Its own instructions and MIT license were read; an unmodified ignored checkout is available under `references/rea/`.

## Documentation entry points

- [Ghidra](https://github.com/NationalSecurityAgency/ghidra): reverse-engineering framework for REA's native backend; verify installed version and R5900 processor support before trusting analysis.
- [Rust WebAssembly target](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html): the browser target is already installed on MML.
- [WebGPU specification](https://www.w3.org/TR/webgpu/): potential later graphics target; the current reconstruction renderer uses WebGL2.
- [wgpu](https://github.com/gfx-rs/wgpu): potential Rust graphics library; not added as a dependency.
- [PCSX2](https://github.com/PCSX2/pcsx2): possible original-execution and SPU2 behavior reference; no gameplay trace/reference execution environment is established yet.
- [PS Rewired](https://psrewired.com/): lists support for original ATV Offroad Fury 2 online multiplayer. Multiplayer integration is not an agreed initial milestone.

REA is user-selected analysis tooling. The other listed references remain research inputs or candidates unless explicitly adopted in the implementation strategy. None establishes a ready Rust browser runtime for this game.

