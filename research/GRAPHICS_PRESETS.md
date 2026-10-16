# Graphics presets

Settings → Display → **Graphics** chooses how the browser presents the recovered
game. The Rust menu (`ui_menu`) owns the row and its labels; the choice persists
in `localStorage` as `atv-graphics` (`web/graphics.js`, `GraphicsSettings`).

| Choice | Meaning |
| --- | --- |
| Auto (default) | Enhanced on phones, tablets (including iPadOS's desktop user agent with multi-touch), touch-primary devices without a fine pointer, ≤4 logical cores, ≤4 GB `deviceMemory` or Save-Data; High otherwise. The row shows the resolved preset, e.g. `Graphics: Auto (High)`. |
| Original | The recovered renderer exactly as before presets existed. |
| Enhanced | Original plus the cheap authored layers below, tuned for phones. |
| High | Enhanced with 16× anisotropic filtering (later: longer tire tracks and a vehicle shadow map). |

Nothing in a preset reaches the Rust simulation, input sampling, the 60 Hz step,
multiplayer snapshots or race logic. The browser gate checks that the worker
receives the same input fields in every preset.

## What each preset contains

| Layer | Original | Enhanced | High | Provenance |
| --- | --- | --- | --- | --- |
| ATV render interpolation (player quad, attached/detached rider, Crazy/Gorn poses, CPU racers, chase camera) | Off | On | On | Authored presentation |
| Anisotropic filtering (terrain atlas/detail, scenery, vegetation) | Off (sampler never touched) | 4× | 16× (clamped to the device maximum) | Authored |
| Soft contact shadows (player, detached rider, CPU racers, remote riders) | Off | On | On | Authored; the original has no vehicle shadows |
| Dirt build-up on quad and rider | Off | On | On | Original dirt textures and zone rates; authored reveal, water wash and reset |
| Motorcycle interpolation and MX mud | Unchanged in every preset | | | Existing behavior (see `MOTORCYCLES.md`) |
| Vibration (Settings → Controls) | Independent of the preset | | | Authored adaptation of the original vibration option |

### ATV render interpolation

`AtvPresentation` (`web/motorcycle-presentation.js`) reuses the motorcycle
approach: each delivered batch of 1–4 physics steps becomes one segment from the
previous frame to the newest, shown over `ticks × 16.67 ms` on the render clock.
It blends origin (quaternion nlerp), quad and rider bone matrices, the detached
rider's world matrix, Crazy/Gorn ragdoll/debris matrices, CPU racer poses
(per racer, yaw on the short arc) and the chase camera. It snaps on resets,
moves over 30 ft, rig changes, attach/detach, crash-presentation changes, camera
preset/mode changes, pauses and gaps over four steps. The presented frame is a
copy; the Rust frame, HUD, audio, effects and multiplayer sends keep using the
simulation frame. Remote riders already arrive through the `atv-net`
Hermite smoothing sampled every render frame, so they need no second layer.

The segment starts when a batch arrives, so the displayed pose trails the newest
simulation state by up to one batch (16.7–66.7 ms), as with motorcycles.

### Anisotropic filtering

`EXT_texture_filter_anisotropic` with `TEXTURE_MAX_ANISOTROPY_EXT` set on the
terrain array atlas, the terrain detail map, scenery materials (including ATM
frames) and vegetation textures. Sky tiles, vehicles, riders and effect atlases
are left alone. Changing the preset updates every registered texture in place;
Original restores 1, which is the sampler default, and the gate confirms the
frame returns to the identical pixels. Context restoration re-registers textures
and re-queries the extension.

### Soft contact shadows

`ContactShadows` (`web/vehicle-shadows.js`) draws every blob in one indexed
draw after scenery and before vegetation: a 7×7 grid per caster draped over the
visible terrain (`render_terrain_height`), multiplying the framebuffer by
`1 − alpha` with polygon offset and no depth writes. The ellipse follows the
vehicle's horizontal axes (66 % of its length, 80 % of its width each side),
spreads with height and fades out by 12 ft of lift; casters beyond 240 ft fade
out. Lift is measured from a learned resting height, so a grounded quad keeps a
full contact shadow on slopes. The detached rider casts a smaller blob under the
pelvis node, including during Crazy/Gorn ragdolls. Capacity is 12 casters
(19,296 bytes of GPU buffers), recreated after context loss. Receivers are
terrain only; ramps, bridges and stadium props built from scenery meshes receive
no blob, and a vehicle high above the terrain on such a prop shows none.

### Dirt build-up

Every original quad material carries a second shader pass whose base texture is a
dirt overlay with `PixelAnimation` (for example `dirt250rm1`, `dirttireside`,
`dirttiretread`, `dirtshock`), and every rider material has one too (`dirtshirt`,
`dirtboot`...). `scenery_material::describe` now names it as `dirt_map`, and the
browser loads it into the same slot the MX mud textures use.

The level follows the source terrain-zone fields `DirtAnimChangeTime` and
`DirtAnimChangeAmount` read from the map's TOY: above 10 mph with grounded
wheels, every ChangeTime seconds the level changes by ChangeAmount of 255
(Caribou dirt: one step per 0.4 s; snow: per 1.0 s; ice: per 1.5 s; zones without
the fields change nothing). The zone under most grounded wheels wins.

Authored parts, labeled as such:

- **Reveal.** The native pixel-animation routine is not recovered. Texels appear
  as the level passes their inverse alpha, so heavily painted dirt shows first,
  then blends by the texel's GS alpha.
- **MUD scenery** (the original MUD collision category) dirties at least one step
  per 0.1 s.
- **Water** washes the quad: each half-submerged wheel removes about 6 % per
  second (fully submerged: clean in about four seconds).
- **Reset** (Reset button/key, new ride) cleans the quad; automatic wreck
  recovery keeps it.
- CPU racers and remote riders stay clean.

### Vibration

Settings → Controls → **Vibration** (default On, `atv-vibration`). Hard landings
and impacts reuse the existing `ImpactEvents` severity; entering a wreck pulses
at least 70 %. Strength `0.2 + (severity − 18)/55` (silent under 18 ft/s).
A connected controller receives `vibrationActuator.playEffect('dual-rumble')`
scaled by strength (70–300 ms). Touch devices without a controller call
`navigator.vibrate` for pulses of at least 30 % (Android; iOS Safari has no
vibration API). One impact cannot chain pulses; a stronger hit can upgrade.
It is independent of the Graphics preset.

## Cost

Measured with `tools/benchmark_graphics_presets.mjs`; see
[PERFORMANCE.md](PERFORMANCE.md#graphics-presets-2026-10-16) for the method and
numbers.

## Verification

- `web/graphics.test.mjs`: Auto detection, persistence, cycling, features,
  vibration settings and rumble scaling/rate limits, TOY dirt fields and the
  build-up/wash/reset rules.
- `web/motorcycle-presentation.test.mjs`: ATV interpolation blending and every
  snap condition.
- `crates/atv-core` `scenery_material` and `ui_menu` tests: dirt-pass naming,
  Graphics/Vibration rows and labels.
- `tools/test_graphics_presets.mjs` (content group): with
  `ATV_GRAPHICS_BASELINE` pointing at the previous main build, Original is
  pixel-identical on five fixed views per map (spawn camera plus four yaws) at
  390×844 DPR3 on Caribou, Supercross 1 and a water map; Enhanced/High apply
  anisotropy and one shadow draw; High→Original returns identical pixels;
  context loss/restore with High; menu rows and persistence; worker input
  isolation; interpolation only outside Original; dirt build-up and reset; a
  Supercross wreck rumbles a connected controller.
