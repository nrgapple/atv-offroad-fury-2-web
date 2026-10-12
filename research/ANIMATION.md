# Rider animation recovery

`animation::decode_mot` reads the original MOT version 2 and 3 streams. `Animation::sample_pose` binds controllers to PSG nodes and returns column-major local/world matrices. Original rider vertices already retain rigid joint indices and local positions in the PSG reader; transform each local vertex by its sampled joint matrix.

The exact USA v2 executable provides the evidence: loader `0x001cb0d0`, ShortAxis decoder `0x001cbd90`, lookup initializer `0x001cb7e0`, rotation sampler `0x001c8160`, VU interpolation `0x002f8650`, and Hermite sampler/search `0x001c9160`/`0x001c9400`. Private Ghidra exports include assembly and incoming/call context. These are this build's addresses, not first-game symbol substitutions.

MOT starts with signed -2/-3 and frame count. Version 3 adds length-prefixed NUL-terminated bone names; version 2 uses model indices directly. Duration, rotation/translation type and two controller counts follow. Each controller starts with key count and bone index. Linear translation keys contain time and XYZ (16 bytes); Hermite keys also contain incoming/outgoing tangents (40 bytes). ShortAxis rotation keys contain time, u16 angle and three i16 axis components (12 bytes). Duplicate controllers are preserved in file order. Other rotation formats fail explicitly.

ShortAxis decoding uses half-angle `angle * f32(0x3a490fdb)` and axis scale `sin(half-angle) * f32(0x38000100)`, matching the original 4096-entry lookup initializer. Quaternion interpolation follows the recovered shortest-arc branch and seventh-order sine polynomial. Host sin/cos/acos are not established as bit-identical to PS2 library/lookup results. Hermite tangents are already segment-scaled; no extra time-interval multiplier is applied. Terminal Hermite translation extrapolation follows the original controller; rotations hold their final key. Public sampling accepts only finite times in the stored clip duration. Game-side timing remains separate.

All 146 extracted rider/arm MOT streams decode and sample at 101 times each, yielding 14,746 finite poses against their appropriate original PSG skeleton. Male and female models have 18 and 27 nodes; the five first-person arm motions use separate arm models. Evidence is private in `private/reports/animation-pose-rust-audit.tsv`. Synthetic tests cover truncation, trailing data, invalid times, packed rotation, shortest-arc interpolation, name binding, hierarchy composition, Hermite/step sampling and terminal extrapolation.

A representative seated pose is `/ANIMSRDR.RES` → `Anim/Rider/m_turnm.mot`, time 0.8 seconds, applied to `/RIDER/MRIDER.RES` → `anim/rider/mrider.psg`. Original materials/textures reside in `/RIDER/RIDER_MT.RES`. A private render confirms the rider sits on the recovered quad with hands and feet near the original handle/foot nodes.

## Normal riding selection

`motion_catalog` recovers the MCF loader's motion paths, interpolation flags and pre-load transition defaults. Both 78-motion rider catalogs and the separate arm catalogs parse. MCF FPS does not rescale MOT key times.

`rider_pose` recovers the normal reverse, slow-turn, ground-lean and airborne clip selection from `0x002d8ba0`/`0x002d8e50`. It preserves pair/four-clip ordering, shared absolute sample time, pose snapshots and the Head-only look overlay. An instruction-interpreter comparison covered 1,000 cases and 216,406 original instructions with no selector, weight or clip-argument mismatches. Private evidence is in `work/physics/rider-pose-*`.

The renderer now connects the selector to 14 normal riding clips, ordered quaternion channel blends, pose snapshots/crossfades and the Head-only look overlay. The reverse path is sampled separately because its controller order differs from the normal clip path. The native Rust animator probe in `work/animation/animator_probe.rs` produced 1,200 finite poses across turn, reverse, lean and air states. Separately, browser diagnostics confirmed all 18 rider nodes animate in turn/turn/reverse modes with changing matrices, and the full browser regression passed locally and in public v5. Crouch and look signals still come from constructor defaults; hand/foot IK, tricks, crash poses, the full event system and female hair remain unfinished. Public v5 includes this normal-riding connection. This is component evidence, not a full animation-system or PS2-hardware match.

### Preload presentation and vertical crouch (host checkpoint)

The normal riding crouch is no longer fixed at its constructor value. The tail
of USA v2.00 `001e5260` drives the existing original lean/down clip blend from
local vertical velocity changes, with separate demand and blend rate limits.
Nine synthetic velocity samples match the original instruction sequence
`001e6b88..001e6d58` bit-for-bit for the resulting blend; the transform call was
hooked to supply known local velocities. This is a finite-instruction comparison,
not whole-game or PS2-hardware equivalence.

The browser's separate hold/release preload gesture also feeds the original
fore/aft pose clips when grounded and physical lean is neutral, using the original
joystick smoothing rate. This presentation adaptation restores visible preparation
without adding suspension or launch forces. Physical lean and airborne stunt poses
retain priority. Original wreck animation and rider separation remain separate work.
