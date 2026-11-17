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

## Runtime (both games recovered)

Both games share one engine path: ATV2 001b8150/0020e3a0/002cf8c0 and
003413e0/003417b0/00341a20; MX PSXOcclusionFlare.cpp (001db7b0, 001dbcb0,
001dbeb0, 001dc0c0) driven by sunflare.cpp (004f3770, 004f3a70).

- Visibility. Each frame tests the sun point against the view frustum with
  VCLIP (ATV2 0016eea0, MX 001aff60). The flare draws only while it passes.
- Render target. The flare art is copied into a small render target
  (`renderTargetWidth`, 128): MX `FilterStream_1` (FilterSprite from the
  256x256 texture), ATV2 `[Prologue]` `renderTargetLocation`.
- Occlusion. In the frame buffer's alpha, a sprite clears a square of
  `OcclusionSampleSize` pixels around the sun (kept on screen), then a second
  sprite at the sun's depth writes 0xff where the z-test passes. Bilinear
  sprites average the square into a 32x32 block, then 16, 8, 4 and 2. One
  texel of that result is stretched over the render target with the colour
  channels masked (FBMSK 0xffffff). The target's alpha is then the visible
  share times 255 (`setOcclusionAlpha = 1` in ATV2).
- Composite. The target is drawn as a sprite with MODULATE and TCC, blended
  `Cs*As + Cd`. As = min(255, share x 255 x vertex alpha / 128), which can
  exceed 128 (1.0) on the GS.
  - ATV2 (002cf8c0): `renderTargetWidth x FlareMaxScale` pixels (384),
    centred on the sun, vertex alpha `max(FlareMaxAlpha, 80) >> 2`.
  - MX (001dc0c0): from the sun minus to plus `flareScreenSize`, twice that
    wide. Vertex RGB is `FlareRGBA` and vertex alpha is `flareAlpha` (times
    the frustum test). A build flag selects the source alpha over a fixed
    `0x5a`, and the shipped build sets it.
- Each sun strength, `1 - distance from the screen centre / 400`, feeds
  nothing in either sprite path.

At full visibility an MX flare with `flareAlpha` 60 adds its art at 0.93,
and an ATV2 flare at 0.31. The texture's own alpha never reaches the
screen.

## Port

`atv_game::sun_flare` parses the TOY and materials and halves the art to the
render target's width. `atv-render`'s flare pass draws the sprite last in
single-view scene passes, with `As / 128` scaling the colour so values
above 1 survive. Sizes are PS2 pixels scaled by the viewer's screen height.

Occlusion uses the scene's own depth. At the end of the scene pass, after
every depth-writing draw, the flare pass draws 16 probe quads over a 4x4
grid of the sample square at the far plane. Each sits inside an occlusion
query. A 17th, always-passing sentinel quad covers the whole square. The
visible share is the cells that passed, so terrain, scenery, plants and
riders all hide the sun, as on the GS.

Nothing waits on the GPU. A query set is resolved two frames after it is
drawn, straight into the uniform buffer the sprite reads (a ring of three
sets). WebGL releases query results only between frames. Until the
sentinel reports, the sprite falls back to 16 rays across the square
against the terrain heightfield. Diagnostics read the results back only
when the UI state asks for them.

Differences, named:

- The share is a 4x4 grid of cells, each visible or not, rather than the
  GS's continuous bilinear average. It lags two frames.
- Not drawn: `Saturation*` colours (all zero in ATV2; MX use not
  recovered), `MinScale`/`MaxScale` (unused by both sprite paths), and
  split screen, which also skips the other full-frame effects.

Verification: `test_engine_sun_flares.mjs` (manual group) uses fixed host
cameras on ATV2 ENV02 and MX Goblin Gulch, The Glades and Tipperary. It
checks visibility, the sun behind the view, terrain occlusion (rays and
depth queries agree) and a tree canopy that the rays miss but the depth
queries catch. Unit tests cover parsing, art halving and ray occlusion.
This is not a native-render or phone comparison.
