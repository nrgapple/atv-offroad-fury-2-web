# Resources

## Local source material

All source material is on MML under this project's `private/` directory. The full disc is available; individual assets can be extracted on demand with `tools/extract_file.py` rather than duplicating the entire image.

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

## Documentation entry points

- [Ghidra](https://github.com/NationalSecurityAgency/ghidra): reverse-engineering framework; not installed yet.
- [Rust WebAssembly target](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html): the browser target is already installed on MML.
- [WebGPU specification](https://www.w3.org/TR/webgpu/): potential graphics target; no renderer selected yet.
- [wgpu](https://github.com/gfx-rs/wgpu): potential Rust graphics library; not added as a dependency.
- [PCSX2](https://github.com/PCSX2/pcsx2): possible original-behavior/reference implementation; not installed or copied during setup.
- [PS Rewired](https://psrewired.com/): lists support for original ATV Offroad Fury 2 online multiplayer. Multiplayer integration is not an agreed initial milestone.

These are research resources and candidates, not a prescribed stack beyond the user's Rust/browser target.

