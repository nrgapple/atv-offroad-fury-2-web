# Scene visibility and model LOD

Recovered from USA v2.00 (`SCUS_972.11`) and implemented in the Rust engine
(`atv_core::model_lod`, `atv_core::zones`, `atv_game::scenery`,
`atv_render::scenery`). Addresses are guest addresses in the exact build;
function names are descriptive, not original symbols.

## Summary

| Rule | Original | Rust port |
| --- | --- | --- |
| Zones and portals | `001fc850` reads `[SceneInfo] NumZones` (default 1) and `NumPortals` (default 0); drawing walks the camera's zone | Parsed into `Scene::zones/portals`; every shipped scene is one zone with no portals, so the zone gate admits every model, TOY actor, emitter and light |
| Per-model visibility | Pre-draw visitor `001b39e0` skips hidden models and asks the class visibility method, which for PSG models (`001b39d0`) always answers yes | Unchanged: whole-object rejection stays with the existing frustum test |
| LOD selection | `001b3a90` picks one stored PSG level per model per frame from the projected size | `model_lod::select`, applied per scene model every frame (Original and Enhanced presets) |
| Multi-pass far cull | `001b2390` passes a "far" flag (model distance > 130 ft) to the shader dispatch `001b90b0`, which then draws only a material's first pass unless the PSM clears `[General] EnableMultiPassCull` (`001bf430`, default on) | Later passes of such materials are skipped while the model is beyond 130 ft |
| Vegetation | ESB selection `0012a1c0` / submission `0012af80` (far billboards, transition billboards, near meshes) | Already recovered in `atv_core::vegetation_render` (see `MAP_FIDELITY.md`); this work does not change it |

## LOD selection (`001b3a90`)

- Bounds: scene and TOY models load through the box branch of the bounds
  builder `001b3ed0` (only vehicles `001e3b80` and keyframed actors `0023cdc0`
  set the sphere flag). For a one-node PSG the root node's own centre and
  sphere radius are used; otherwise the aggregate box centre and the length of
  its half extents.
- Distance `d`: from the camera position to the model-local bounds centre
  plus the model translation. The centre offset is not rotated by the model
  matrix (observed in the arithmetic, kept as is).
- Projected scale: `s = F / max(d - r, 0.1)`, with `F` the camera's focal
  length in pixels. `0011b070` sets it to half the viewport width over
  `tan(FOV/2)`; every gameplay caller passes the 640-pixel viewport. The
  browser view's 4:3 reference focal maps to the same pixels (`240 *
  focal_length`).
- Each stored level except the last (whose threshold is `FLT_MAX`) is
  compared: when `geometryScale * s * globalFactor * threshold < 1` the level
  index becomes one past it. The loop does not break, so the selected level is
  one past the last passing entry. `globalFactor` (the float at `00408350`) is 1.0 in
  the ELF and the geometry scale (+0x14, initialised by `001b2610`) is 1.0; no
  writer of either was found.
- Equivalently, level `i + 1` starts once `d > r + threshold_i * F`.
- Every stored level loads at venue load (`001b2730`), so switching costs no
  streaming.

## Multi-pass far cull (`001b2390` -> `001b90b0`)

The far flag compares the stored LOD distance with the float at `00408348` (130.0 ft,
strictly greater). With the flag set, a material whose `EnableMultiPassCull`
is on draws only pass 0. In shipped content only the drop-gate light material
(`drawgate`, env04 family) has a later pass under this flag (8 draws across
91 routes); other multi-pass materials either clear the flag or are single
pass.

## Census (private all-venue sweep)

`crates/atv-game/tests/visibility.rs` (ignored; needs the private hosted
package) loads every Freestyle venue and every race variant:

- 91 routes, 41,616 placed scene models, 1,781 model names, 0 unresolved.
- 8,085 placements (153 model names) have more than one level: 118 names with
  2 levels, 31 with 3, 4 with 4. Every table is ascending and ends in
  `FLT_MAX`; every level of a drawn model has geometry.
- At the native follow camera's 85 degree field of view (`F` = 349.2 px), the
  first switch distance ranges from 25.6 ft to 368 ft (median 115 ft).
- Every scene, TOY and race TOY zone reference (1,097 `StartingZone`, `Zone`
  and `InitialZone` records) names zone 1; no scene declares `NumZones` or
  `NumPortals`.

## Graphics presets

- Original and Enhanced follow the recovered LOD and 130 ft multi-pass cull.
- High keeps every scene model at its finest level with every pass
  (`Graphics::finest_models`), an explicit enhancement that avoids level pops
  on capable devices. Auto resolves to Enhanced on phones and tablets, so
  those keep the original savings.

## Performance (host only)

Host Chromium (Metal, 1280x720 DPR1, Original preset, spawn view, weather
off), four alternating before/after pairs per venue against the previous
main build; 6 one-second status samples after 3 s warm-up. Not a phone
measurement. Frame intervals are vsync-bound on this host.

| Venue | Frame ms before -> after | CPU ms | Draws | Triangles |
| --- | --- | --- | --- | --- |
| env06 | 14.71 -> 14.87 | 4.03 -> 3.63 | 108 -> 108 | 115,135 -> 115,024 |
| env01 | 14.51 -> 14.32 | 3.51 -> 3.78 | 185 -> 171 | 105,041 -> 103,330 |
| sx01 | 14.27 -> 14.51 | 3.98 -> 3.94 | 280 -> 280 | 89,753 -> 89,443 |
| env04 | 15.28 -> 15.50 | 3.58 -> 3.43 | 91 -> 91 | 107,967 -> 107,997 |

CPU differences are within run-to-run noise in both directions. Level
selection only visits models that can change (several levels or a culled
later pass). At the spawn view most scenery is either single-level or
inside its first switch distance, so totals (dominated by terrain and
vegetation) move little; a fixed far view of env06's lodge
(`test_engine_visibility.mjs`) draws 5,925 scenery triangles in Original
against 6,731 at the finest level (-12%) with the same 70 draws. env01's
spawn view drops 14 draws because distant coarse levels have fewer material
groups. Every level stays
resident: across the 78 venue archives the coarser levels add a median 13%
(maximum 45%) to scene-model vertex counts.

## Uncertainty and open items

- `001b2390` also forces the far flag while a global (`00498b48`) is nonzero,
  only for models without per-group material tables. Its writer was not
  found; the port treats it as zero.
- `001b3a90` also hands each group a projected size (`s * k * r *
  2`, with k the float at `00408358`, 1.0); what the group method does with it is not recovered.
- Keyframed TOY actors and vehicles use the sphere bounds branch and would go
  through the same selection; the port's actor and vehicle passes still draw
  their finest level.
- Portal traversal between several zones is not recovered (no shipped scene
  needs it).
- Whether the camera FOV field at +0x1e0 is horizontal or vertical is
  inferred from the projection call; the port uses the follow camera's field.
