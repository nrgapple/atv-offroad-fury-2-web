# MX USA map format investigation

Partial investigation, 2026-11-14. This does **not** establish map import compatibility.

Source: private/source/MX vs. ATV Unleashed (USA).iso; private inventory
private/reports/mx-unleashed/psr-directory.json. The disc directory places
MXVSATV.PSR at sector 1400000. PSR member LBAs are relative to that base.
No source bytes or generated assets belong in Git.

The PSR inventory contains 56 loose SCN names and 168 Env resource archives.
Terrain is nested inside resources: absence of loose TR2/TRN names is not
absence of terrain. FS01/fs01.res uses the existing v3 resource directory and
zlib block layout. Its scene member is textual, with TerrainFile=FS01.ps2.trn,
TerrainWidth/Breadth=1 and Width/BreadthScale=3.5. Its nested terrain member
has 1,624,255 bytes and begins PLN NUL, ASCII 7ps2 NUL, then
UnCompressedPSXPal8 NUL. It is not TR2 v6. The existing decoder now reports
PLN explicitly rather than giving a generic TR2 version failure.

Implementation strategy: retain ATV2 decoding unchanged; establish PLN header,
tile addressing, heights, attributes, texture layers and hierarchy from source
before introducing a separate adapter into the bounded existing renderer.
Do not relabel PLN as TR2 or assume identical coordinate scale. Scene scale
fields are evidence, not yet a validated conversion rule.

Unresolved: complete nested terrain inventory; PLN payload and LOD layout;
scene/weather/material parser compatibility across all maps; coordinate units,
origin and orientation; collision surfaces; texture/lighting fidelity; mobile
memory and frame time. No rendering compatibility or all-map import is claimed.
Parent integration must not enable these maps using the TR2 path. No renderer
or public API changes were made in this checkpoint.

Follow-up inventory from parent: 56 venues (FS01–08, MC01–10, Nat01–17,
SM01–03, SX01–18), all nested terrain headers share PLN v7. TOY has
16 terrain zones; no ATV2 surface mapping is justified. Terrain materials name
Vu1TerrainRenderer.vuo and its LOD variant with totalTextureMaps=2.
Existing scene.rs already supports explicit CollisionObjects/ObjectN placement;
its 4096-object bound and other validation still require an all-scene audit.

FS01 candidate binary fields (observations, not decoder semantics): at byte
1069, f32 3.5, u32 1, u32 1598302, u32 257, u32 257, followed by
offsets 2189, 4318, 6447. Consecutive offsets differ by 2129. The first
record does not have TR2 raw_block headers; adapting signature alone is unsafe.
