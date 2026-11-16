# Sun flares

Both games author sun flares, but the port drew none until 2026-11-16 (#722).
The data appears in 25 ATV2 venue TOYs and every MX venue TOY. Evidence comes
from the exact builds, read privately, without code or bytes here.

## Authored data

`[SunFlares] NumberOfSunFlares` lists `[SunFlareN]` sections (ATV2 001fe180,
MX 00216650). Each section names `TextureMap` (a `.tga` stem resolved to the
flare PSM and PST) and gives a direction: `LightDirection`, or
`TargetPosition - Position`. The flare stands at the camera minus that
direction, at infinity. `MinScale`, `MaxScale` and the `Saturation*` colors
are also authored.

- ATV2 reads its occlusion and sprite settings from the flare PSM `[Prologue]`
  (001b8150): `OcclusionSampleSize` (default 32), `FlareMaxScale` (3.25),
  `FlareMaxAlpha` (128), `flareColor` (128s) and `renderTargetWidth` (128).
- MX reads the same settings from the TOY when `useToyFlareSettings` is set
  (00216650 defaults: sample 16, alpha 40, size 350). Otherwise it uses the
  PSM `OcclusionFlare` filter stream (001dbcb0 defaults: sample 16, alpha 80,
  size 400).

## Runtime (ATV2 recovered)

- 0020e3a0 projects the sun each frame. The flare draws only while the sun
  falls inside the viewport (0016eea0). A separate strength,
  `1 - distance from (320, 224) / 400`, feeds nothing in the sprite path.
- 0020e610 centers a screen object on the sun. 002cf8c0 draws it at
  `renderTargetWidth x FlareMaxScale` PS2 pixels (384 for the authored 3.0).
  Its vertex color is the authored color, and its vertex alpha is
  `max(FlareMaxAlpha, 80) >> 2`. The blend is additive, `Cs*As + Cd`, with no
  depth test.
- 002cf8c0 and 003413e0/003417b0 z-test samples in an `OcclusionSampleSize`
  square around the sun, kept on screen, against the sun's projected depth.
  The GS then reduces them (64 to 32, 16, 8 and 4) to dim the flare.

## Port

`atv_game::sun_flare` parses the TOY and materials. `atv-render`'s flare pass
draws the sprite last in single-view scene passes. Sizes are PS2 pixels
scaled by the viewer's screen height.

Adaptations, named:

- Occlusion. WebGL2, the browser backend, cannot return an occlusion query
  within the frame; wgpu reads zero. The port instead casts a 4x4 grid of rays
  across the sample square against the terrain heightfield. The visible share
  scales the sprite alpha. Ridges and hills hide the sun as in the original.
  Scenery models and vegetation do not.
- MX alpha. MX composites its flare through a render-target filter path that
  is not recovered. Its flare art is opaque on black, so the port uses the
  authored `FlareAlpha` as GS vertex alpha. ATV2 keeps the recovered quarter
  with its floor of 80.
- Not drawn: `Saturation*` colors (all zero in ATV2; MX use not recovered),
  `MinScale`/`MaxScale` (unused by ATV2's sprite path), and split screen,
  which also skips the other full-frame effects.

Verification: `test_engine_sun_flares.mjs` (manual group) uses fixed host
cameras on ATV2 ENV02 and MX Goblin Gulch / The Glades to check visibility,
terrain occlusion and the sun behind the view. Unit tests cover parsing and
ray occlusion. This is not a native-render or phone comparison.
