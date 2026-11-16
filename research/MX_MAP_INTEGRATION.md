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
Terrain detail tiling and blend strength are now recovered (see "Terrain and
foliage rendering recovery" below). No MX scene declares the existing animated water
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

The subsequent complete 56-package sweep found one additional decoder failure:
Northern Lights declares two geometry layers and writes signed V2-16 UVs
at 0x8001+4N after the primary UV and unsigned color arrays. That bounded
second stream is retained separately as `Vertex::uv2`, with layer declaration,
matching count/destination, duplicate and completeness checks. It does not
overwrite primary UVs. Its shader-specific rendering consumer remains a gap.

The actual Rust package sweep now accepts all 56 exact manifest venue archives:
2,905 PST textures, 129 animated textures (all frames), 7,968 static collision
members and 2,782 PSG meshes. All 112 sky files separately passed earlier.
Native drive acceptance next exposed the plural `[TerrainZones]` summary,
which is now distinguished from the sixteen numbered zone declarations.
MX ambient emitters remain explicitly inactive: the ATV2 ambient program is
not an adapter for MX source emission layouts. This removes a misleading
ATV2 parser-limit warning without claiming source particle equivalence.

FS05's bike settling can enter a wreck before its first camera tick. The ride
camera now seeds its normal initial follow pose when that specific startup
case needs a wreck-camera seed. The crash and settling simulation are retained.

The full drive sweep caught ATV2 boundary launches on small MX arenas,
including MC08 losing all ground contact. ATV2's 110-cell inset is not a
recovered MX edge rule; shared dimension key names do not establish behavior
parity. MX therefore leaves the ATV2 launcher inactive. Source MX edge/reset
behavior remains unresolved, alongside water reset metadata. Acceptance still
requires actual contact and movement; ejection is not counted as driving.

NAT04's finest geometry covers 896 by 1280 cells while its texture tree retains
256-cell-wide regions through X=1024. MX texture mapping clips those outer
cells to the actual geometry footprint, still checking every layer index and
requiring exactly one mapping per in-bounds cell. ATV2 retains strict region
coverage. A generated 14-by-20 mapping fixture validates partial edge regions,
missing coverage, overlapping regions and bad indices. Native acceptance is
rerun against the actual map after this repair.

## Actual native acceptance checkpoint

The full guarded venue loader pass completed all 56 maps with ATV selection 1
and motorcycle selection 29, 360 steps each: 112 runs and 40,320 steps.
Every run retained finite simulation values, actual wheel contact and movement;
minimum grounded count was 235/360 and minimum horizontal travel 38.12 world
units. No scenery models were unsupported. The private actual acceptance JSON
was written only after the complete sweep, with exact package/adapter digests.

Host debug venue-only load times ranged from 2,922 to 7,385 ms (median 4,893.5
ms). These exclude vehicle loading and are not browser startup measurements.
Five maps retain skipped shader groups: NAT04 (5), NAT05 (2), SM01 (1), SM02 (2)
and SM03 (1). The largest authored scene has 4,874 models. Source-missing
colliders and all previously documented fidelity gaps remain explicit.

The draft runtime capability is enabled for the next private browser validation
pass after these native gates. Public content has not been promoted/uploaded;
there is no public or physical-phone acceptance claim. A matching CI artifact,
all-56 host mobile browser checks and inspected renders remain required.

The first browser attempt exposed a direct-link filter still limited to the
original 40 maps: MX query URLs silently loaded ENV01. Those observations are
invalid as MX acceptance and are not counted. Single-player direct links now
use the same runtime course resolver; direct multiplayer room links retain
the original environment whitelist. Browser acceptance asserts the actual
loaded venue ID. Scripted parity steps hold their final pose, so frame timing
is measured separately with a live-loop 360-step autodrive pass per map;
paused/scripted-load frame samples are not represented as gameplay timing.

The skipped shader groups resolve to secondary ambient-environment-reflection
passes on `nat05_window` (NAT04/NAT05) and `SM02_DiaPlat` (SM01–03). Their
primary diffuse passes remain present; the loader does not drop those models.
NAT04 water uses the separately supported XZ-reflection path. Screenshot
inspection still needs to establish the visible impact in the actual venues.

Browser timing inspection found that the scripted trace can precede the first
completed render, so acceptance waits for initialized visibility counters.
NAT01's final scripted camera legitimately culls all 692 scenery instances;
its captured terrain, vegetation, rider and sky remain rendered. Requiring
a visible prop at every final pose would conflict with preserved frustum/fog
culling. The gate therefore requires uploaded scene instances and a completed
draw/cull pass, records all culled counts, and retains image inspection alongside
actual venue identity, finite simulation, contact and movement assertions.

## Browser acceptance and delivery checkpoint

All 56 venues passed the registered browser sweep on the authored application
artifact from GitHub Actions run `43656216594`, CI merge
`a6440637aaa2512cd3a99d833119757b8383ffba` (source tree
`dbbf90afcde16ea6e2ff4660dacaaac8bbecb9fa`). The run took 594.4 seconds and
checked the actual loaded venue IDs, finite simulation, ground contact,
movement, scene upload/visibility, page errors, and 360 live simulation steps
separately from each scripted trace. It captured 56 screenshots; inspection
covered every venue family, the partial NAT04 coast/submerged edge, and maps
with omitted reflection passes. The earlier ENV01 fallback and diagnostic runs
are excluded from acceptance. Native acceptance separately covers both vehicles
on every venue; the browser sweep alternates ATV and motorcycle selections.

This is functional host acceptance, not a phone-performance claim. Headless
Chromium on an Apple M4 at a 720-by-405 touch viewport measured median engine
CPU time of 5.63 ms per rendered frame; SX04 reached 63.26 ms with a 101 ms
95th-percentile frame interval. Dense arenas still need optimization. JS heap
measurements exclude Wasm, GPU allocations and total process memory. Warm
sequential loads do not establish cold-network startup performance.

The 56 venues are exposed through five appended single-player Freeride groups
when their packages and the compatibility adapter are available. Existing ATV2
ordering and multiplayer IDs remain intact. Source collider omissions,
secondary reflection omissions, original weather/LOD/contact behavior and MX
multiplayer integration remain open as described above. Runtime assets are
published separately to R2; none belong in Git or CI.

## Terrain and foliage rendering recovery

Visual review on 2026-11-16 found MX venues dark and murky, with palms and
bushes drawn half transparent. The causes are ATV2 constants applied to MX
data. Evidence comes from the exact-build module and its shipped VU1
microprograms, read privately and summarized here without code or bytes.

Foliage alpha. MX selection (004702a0) scales the near/far fade phase by
`128 / (outer - inner)`, twice ATV2's 64. Near meshes (004730a0) use vertex
alpha `min(128, 128 - (phase - 32) * 4/3)` and a GREATER alpha test against
`int(alpha * 0.5)`. Billboards (00474350) fade in to `min(128, phase * 4/3)`,
and the billboard/near switch is phase 97. Each value is ATV2's doubled. The
port kept ATV2's 64-unit vertex alpha, so GS modulation halved every MX texel.
Covered leaves blended at about 50%, and billboards mostly failed the PSM
reference of 120. The earlier one-byte alpha-margin workaround is replaced by
an explicit MX profile in `vegetation.wgsl`. It doubles the shared 0-64
selection phase, so the MX switch at 97 matches the shared switch at 49 exactly.
ATV2 arithmetic is unchanged. Billboard colors clamp at 128, as in 00474350.

Plant light colors come from the Ambient light only: ambient times
`EcoAmbientScale` and ambient times `EcoDiffuseScale` (00471a40, colors
stored /255 by 0012e8e0). All scenes author both scales as 1.0, giving ranges
close to the shared adapter. This lighting is not changed here.

Terrain. `Vu1TerrainRenderer` reads no light matrices. Each vertex receives one
constant RGBA (`vf14`, from VU 0x40/0x41) for both base and detail passes, so
terrain shading is baked into the PLN layers. The stored normals are not used
for terrain color. All 56 `TerrainPlain.psm` files declare
`TerrainColor = 128,128,128,128`. ATV2's fixed packet is RGB 127, A 64. MX
therefore blends its detail texture at full texel alpha, twice ATV2's strength.
Per-venue `DetailReplacementColor` values (101-134, the far-terrain stand-in)
corroborate this. They match each venue's average full-strength detail
darkening, not a half-strength one. The row loop advances detail U/V by
ShaderParameter8.w, which 00481610 sets to the terrain loader's multiplier / 64.
That is the SCN `DetailMapTilingMultiplier`: default 8 (0031e0c0); authored 6,
8 or 16. Base U advances by 15/1024 per cell, the existing 240/256 inset.
The renderer now takes per-venue detail tiling, blend alpha and base modulation
from the loader. ATV2 keeps 32/textureWidth, 255/256 and 127/128 exactly.

Checked and unchanged: MX fog uses ATV2's formula, including the constant 64
passed by 0031af40 to 00124200. No venue authors per-zone fog. Static props
mostly use `Vu1StripDiffuse`, whose clamped ambient-plus-directional sum
matches the existing prop lighting scale. `HighDynamicRange` is a vehicle and
rider material (`Veh/Tex/HighDynamicRange.psm`), not a venue grade.
Remaining: far-terrain `DetailReplacementColor` modulation (mip averaging
stands in), sun flares (authored in MX and ATV2 TOYs, absent from the port),
and the native framebuffer comparison.
