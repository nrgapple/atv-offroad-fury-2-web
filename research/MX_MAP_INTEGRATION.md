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

## Runtime integration checkpoint (not yet accepted)

The shared venue and ride loaders now select the source stem independently of
its `mx_` menu ID. PLN geometry/contact, indexed terrain layers, SCN spawn,
COL v2, CUB sky baking and PST/ATM textures adapt into the existing interfaces.
No separate renderer, per-instance mesh copy, or multiplayer ID reassignment
is introduced. The host-only `mx_venues` acceptance example loads candidate
maps without opening the public launch/picker gate. It checks each authored
spawn and runs an ATV and motorcycle for 360 simulation steps each; this is a
CPU acceptance test, not a browser performance measurement.

Private staging now contains 56 venues, 204,664,076 bytes and 24,018 members.
Derived terrain detail/material resources and ESB billboard material/image
edges are included. A separate 8,497-byte `/MX/ADAPTER.RES` supplies original
ATV2 camera settings and labeled terrain-material profiles. Twelve source
collision omissions across maps remain documented; seven unique names are
involved. Missing source objects are reported, not substituted with similarly
named geometry. Active PhysicsCollections use their independent F collision
targets at the authored placement as an explicit **static approximation**.
Their M actor whiskers and movable rigid-body dynamics are not substituted
for static collision geometry.

The ATV surface adapter draws dirt, grass and shallow/deep mud coefficients
from ENV02, sand from NAT05 and ice from ENV01; pavement uses neutral
coefficients provisionally. The sixteen-zone order is dirt, pavement, sludge,
grass, sand, ice, shallow mud, deep mud, then eight dirt variants. Only friction
routing follows `MapZoneFriction`; the raw contact index remains unchanged.
Motorcycles select their source sixteen-zone tire profiles without multiplying
in ATV2 map coefficients. This is an authored ATV handling adapter, not proof
of original MX ATV tire-force parity.

Common MX prelit and XZ-reflection shader names retain those capabilities.
Context-one suffixed GS settings are fallbacks only; context two is not folded
into them. Numbered UVAnimation and TextureFrame children feed the shared UV
scroll and texture clocks. Signed V2-8 PSG UVs cover the NAT12 jump gate.
Three explicit texture spelling repairs match the packager audit and retain
original resource bytes.

Known presentation gaps remain: ambient environment reflection, glossy/specular
passes, scaled/constant-color behavior, and the Northern
Lights material's second UV set are not established as native equivalents.
Source-generated stadium RenderTarget displays retain their geometry with an
explicit black texture until a live screen camera is implemented.
The renderer still uses its existing authored detail UV scale; the MX loader's
`DetailMapTilingMultiplier` values (6, 8 or 16) are known, but their runtime UV
consumer is not recovered. No MX scene declares the existing animated water
heightfield resource; water-looking scenery still uses its source materials.
SCN fog and sky are retained. Source `IsWater`/`ResetTime` and `CleanVehicle`
metadata remain in the TOYs but are not yet connected to the shared reset and
vehicle-cleaning behavior. MX weather selection and an ATV wheel-particle
adapter remain unresolved; MX particle group order is not fed into ATV2's
incompatible eight-zone emitter table. Paired zero ecosystem draw distances
are treated as disabled vegetation to avoid a zero-width fade division.

The additional maps stay unavailable until source decoding and actual venue,
vehicle and browser acceptance pass. No public delivery is claimed by this
checkpoint.

## Signed-byte ecosystem mesh recovery

The actual NAT12 bush packet begins each bounded batch with V4-8 signed
positions at VU destination 0x8001, followed by V3-8 normals at 0x8001+N
and V2-8 UVs at 0x8001+2N, then MSCNT. The previous decoder mistook
V4-8 positions for unsigned colors and reported an ordering error. MX v5
now accepts this position encoding with signed negative rigid-node magnitudes;
ATV2 retains its existing accepted forms. Attribute destination/count and
whole-packet vertex checks remain enforced. A generated two-batch fixture
covers geometry, restart indices, attributes and malformed count/destination
rejection without source bytes. Private source acceptance remains required.
