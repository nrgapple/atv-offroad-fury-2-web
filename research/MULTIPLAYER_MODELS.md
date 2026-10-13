# Multiplayer vehicle and rider resource census

USA v2.00 census, 2026-10-12. Evidence comes from the private full archive audit,
`private/reports/quad-mesh-audit.json`, `content-audit.json`, `disc-files.json`,
and existing [geometry](GEOMETRY.md) / [animation](ANIMATION.md) readers.
Only authored notes belong in Git; extracted assets remain private and enter
the separately validated R2 runtime package when used by the browser.

## Vehicles

All 26 in-race ATV models decode. IDs 01–13 live in `/QUAD/BIKESA/`; 14–26
live in `/QUAD/BIKESB/`. Each has `ATVNN.RES` and `ATVNN_MT.RES`.
Every vehicle archive contains these case-insensitive paths:

| Resource | Role |
| --- | --- |
| `quad/atvNN.psg` | 27-node articulated in-race mesh; four original LODs. |
| `quad/ATVNNS01.phy` | Per-model physics/tuning, only S01 present in this census. |
| `geometry/ATVNNao.col` | Chassis collision. |
| `geometry/ATVNNar.col` | Actor collision rays. |
| `geometry/ATVNNf.col` | Additional collision resource; current driving loader does not consume it. |
| `quad/ATVNN.mcf`, `Quad/ATVNN.Vut` | Motion/config resources; do not treat VUT as a decoded browser animation. |
| Sibling `ATVNN_MT.RES` PSM/PST members | Model's materials, textures and dirt bindings. |

Finest LOD counts range from 4,227 to 5,083 triangles. All have 40 unparsed
trailer bytes. Model labels/manufacturer mapping should come from decoded
`/UI/RESOURCE/QUADINFO.RES`, not inferred from mesh filenames.
Front-end `/UI/RESOURCE/QUAD/ATVNN.RES` models are distinct showroom resources;
use the in-race paths for player actors.

At initial inspection the browser hardcoded ATV01. Generalize its loader and feed the selected
model's PSG, PHY, AO and AR into the same Rust linkage/physics paths for both
single-player and multiplayer. Decoding a remote PSG through bridge op11 avoids
replacing local simulation's selected mesh (op11 is inspection-only; op22 loads
the driving mesh).

## Riders

| Selection | Archive | Mesh / catalog | Nodes |
| --- | --- | --- | --- |
| Male | `/RIDER/MRIDER.RES` | `anim/rider/mrider.psg`, `anim/Rider/mrider.mcf` | 18 |
| Female | `/RIDER/FRIDER.RES` | `anim/rider/frider.psg`, `anim/Rider/frider.mcf` | 27 |

Both archives include rider PHY, frontal collision and first-person arm models.
`/ANIMSRDR.RES` contains male/female MOT streams. Both 78-motion catalogs parse;
all 146 unique rider/arm clips have been audited for finite sampled poses. Shared
semantic normal/stunt/held/wreck/landing motion suffixes use `M_` versus `F_`
catalog names. At initial inspection bridge op51 hardcoded `M_` at each selection loop;
female initialization must select `F_`. Replicated node arrays must allow 27
rider nodes; male indices cannot be assigned to female meshes.

`/RIDER/RIDER_MT.RES` is the large configurable material bank. Current R2
packaging compacted it using only MRIDER mesh dependencies; include the union of
MRIDER and FRIDER material dependencies before offering female selection.
`/RIDER/AI_MAT_M/MT_AI01..21.RES` and
`/RIDER/AI_MAT_F/MT_AI22..44.RES` hold clothing texture/material presets,
not additional rider geometry. These are optional authored selection wiring,
not evidence of 44 separately animated characters.

## Existing Rust state useful for replication

The driving bridge already publishes frame origin/orientation, quad node matrices,
rider node matrices, separate wreck rider world matrix, rider mode, engine
telemetry and wreck state. Sample this state once per network snapshot rather
than resimulating remote inputs with unmatched native state. Rust `rider_pose`,
`wreck_animation`, `quad_pose`, and `engine_audio` provide semantic state and
presentation inputs. Renderer and browser audio still require explicit per-peer
instances; their current single-actor globals are not multiplayer support.

Remote engine RPM/throttle and crash serials can drive locally decoded sound
banks with Web Audio spatialization; sending PCM over the game socket is
unnecessary. Per-vehicle original engine selectors must be read from each PHY/
audio configuration, and female rider vocals require appropriate bank selection.
Original SPU2 mixing and complete female hair/IK fidelity remain unverified.

## Original player-facing quad names

`/UI/RESOURCE/QUADINFO.RES` → `ui/frontend/uiQuad.txt` contains 25 named
selector entries. Their `IngameID` values establish the mapping below;
selector index is a different order and must not be used as model ID.
The `uiQuad.txt` copies in `/UI/RESOURCE/PAGEA/PAGE07.RES` and `PAGE20.RES`
independently match all 25 names/IDs. Private extraction census is
`private/reports/multiplayer-quad-labels.json`.
The original `DefaultQuad_P1`–`DefaultQuad_P4` value 1 selects menu entry 1,
which maps to in-race ID 22 (Honda 400EX Yellow); it is not in-race ID 1.

| In-race ID | Original name |
| --- | --- |
| 01 | Ravage Talon Black |
| 02 | Kawasaki Lakota Sport |
| 03 | Kawasaki Mojave |
| 04 | Honda 400EX Red |
| 05 | Honda 300EX |
| 06 | No selector entry found; browser fallback: ATV 06 (unlisted model) |
| 07 | Kawasaki KFX700 |
| 08 | Patriot |
| 09 | CT Racing |
| 10 | Trinity Racing |
| 11 | Bombardier DS650 |
| 12 | Alba Racing |
| 13 | Nac's Racing |
| 14 | Honda 250EX |
| 15 | Duncan Racing |
| 16 | K&K Racing |
| 17 | Curtis Sparks Racing |
| 18 | Lonestar Racing |
| 19 | G Ride |
| 20 | Trenga Racing |
| 21 | B6000 |
| 22 | Honda 400EX Yellow |
| 23 | Ravage Talon Blue |
| 24 | Ravage Talon Red |
| 25 | Ravage Talon Yellow |
| 26 | Ravage Talon Green |

ATV06's materials include `dirtDZ400m1.pst` / `dirtDZ400m2.pst`; this is a
research lead, insufficient evidence for a manufacturer/model name. Honda 300EX
has original selector `Access=3` (hidden). Offering every decoded model immediately
in the browser garage is an authored adaptation, not recovered progression.
