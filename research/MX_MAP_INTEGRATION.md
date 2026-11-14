# MX Unleashed venue integration

Implementation strategy, 2026-11-14. Import work is in progress; none of the
56 additional venues is validated for public play yet.

The source is the existing private USA MX vs. ATV Unleashed disc,
`SLUS_211.04`, also used for motorcycles. Its resource inventory supplies
8 FS, 10 MC, 17 NAT, 3 SM and 18 SX venue families. Activity routes within
one venue do not count as separate terrain maps.

## Resource and selection contract

- Preserve all existing ATV2 course identifiers and ordering. New identifiers
  have an `mx_` prefix, for example `mx_nat01`; member stems remain `nat01`.
- Package each connected venue under `/MX/MAPS/NAT01.RES` (uppercase archive
  key), preserving source member paths. Include required scenery, textures,
  sky and collision, not just terrain. Executables and source shader programs
  remain private research inputs, never runtime package members.
- Append separate MX groups to the existing course picker. Imported venues
  initially expose Freeride only. Source activity routes need their own rules
  and validation before being offered as races or championships.
- Availability requires both runtime capability and a complete content
  package. A staged package alone must not make a venue selectable.
- Multiplayer integration must preserve existing numeric map IDs and verify
  server validation as well as client selection. Do not reinterpret an ATV2
  race ID or assume a new local menu item is supported by deployed servers.

## Source adapters

All 56 main archives contain PLN v7 `.ps2.trn` terrain. It must be decoded
separately from ATV2 TR2 v6. Reuse the shared terrain renderer, spatial queries
and buffer management where recovered geometry and material semantics permit;
do not reinterpret PLN records as TR2 tiles just because both are heightfields.

Keep terrain, scene placements, collision and spawn in the same source frame.
Every SCN provides `DefaultStartPosition` and `DefaultStartDirection`; none of
the common venue TOYs supplies a SceneInfo override. Scene metadata gives
extents as `256 * dimension * scale`, including SX07's rectangular 1-by-2
terrain. Validate these extents against decoded geometry before relying on them.

The existing collision loader assumes `geometry/<name>.col`; source-aware
resolution must retain every explicit MX Object placement. Thirty-nine MX
scenes author `MinDivisionSize=0`. Choose a positive bounded partition size
as a port adaptation, preserving the authored world bounds. ATV2 validation
must remain unchanged. SX08 has 4,874 static placements: audit both scene
count limits and text-parser entry limits, and retain shared geometry and
instance culling rather than duplicating models per placement.

The 32 unique connected skies use CUB format 43: VIF streams of XY
coordinates and RGB vertex colors. ATV2 uses format 32 IPUM image payloads. `mx_sky` decodes independent triangle strips and samples their colors
into 96 opaque 128-by-128 tiles for the shared sky renderer. This is an authored
sampling adapter. The private independent Python parser accepts all 112 disc
skies; the four Rust synthetic tests cover complete cube framing, malformed
extents and attribute pairing, interpolation and missing coverage. Full Rust
source acceptance is pending. Preserve per-map fog; the observed maximum
distances range from 500 to 4,000.

MX surface indices have fixed semantics: dirt, pavement, sludge, grass, sand,
ice, shallow mud/water and deep mud/water in slots 1 through 8; slots 9 through
16 are dirt with particle-color distinctions. Honor `MapZoneFriction` remaps
(SX04 and SX16 remap zone 9 to sludge), water/reset metadata and cleaning flags.
Motorcycles can use their source surface profiles. ATVs need an explicitly
documented semantic adapter to their existing solver, not an arbitrary reuse
of ATV2 per-map zone numbers. Preserve original zone IDs for effects.

## Acceptance gates

1. Audit all 56 archives and their dependency closure; report package sizes,
   unresolved references and source identity. Keep staged output private.
2. Validate decoded terrain dimensions, finite heights, tile coverage, surface
   indices, texture coverage and source-frame spawn for every venue.
3. Load and ride representative maps from every format/size group, then sweep
   all venues for terrain, sky, scenery, collision and map switching failures.
4. Measure startup, peak memory, draw work and frame time with the existing
   mobile browser viewport checks. Host results are not physical-phone results.
5. Inspect screenshots, record actual limitations in a dev-log checkpoint,
   publish validated content, ship the code through PR CI, and verify public
   no-disc startup on the deployed application.

Original MX terrain LOD and weather equivalence remain unverified. Any authored
replacement or omitted behavior must be named in the release evidence.
