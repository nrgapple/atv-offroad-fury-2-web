# MX USA map format adapters

The adapters target the exact MX vs. ATV Unleashed USA source. Runtime/menu
integration, browser acceptance and original rendering fidelity are separate
checks. Source assets, extraction output and loader exports remain private.

## Source and strategy

Source: `private/source/MX vs. ATV Unleashed (USA).iso`, indexed by
`private/reports/mx-unleashed/psr-directory.json`. The disc directory places
MXVSATV.PSR at sector 1400000; member LBAs are relative to that base. Existing
v3 Resource decoding reads the nested environment archives. Terrain is nested
as `Env/<id>/<ID>.ps2.trn`, not listed as loose TR2/TRN members in the PSR.

The 56 venues comprise FS01–08, MC01–10, Nat01–17, SM01–03 and SX01–18.
Every terrain begins `PLN` NUL, ASCII `7ps2` NUL, then
`UnCompressedPSXPal8` NUL. This is a separate decoder, never a signature change
to the ATV2 TR2 reader. Strategy: recover geometry and texture addressing from
the exact loader, validate every map, then reuse the existing bounded terrain
representation and renderer. Original adaptive LOD is not reconstructed here.

## Interfaces and representation

- `terrain::decode_pln(data) -> Result<PlnTerrain>` returns `terrain: Terrain`
  and `vertex_streams: Vec<[[u8; 289]; 2]>`. Callers may move out `terrain` and
  release the additional streams when they are not needed.
- Existing `Terrain::render_heights`, `heightfield`, `contact` and spatial leaf
  lookup work on the decoded geometry. No simulation enum is needed.
- `terrain::pln_texture_layout(data) -> Result<TerrainTextures>` returns the
  existing hierarchy regions, references and texture records.
- `terrain::decode_pln_texture(data, record) -> Result<texture::Texture>` expands
  one requested indexed layer into GS RGBA, with no retained decoded cache.
- `scene::parse_mx(scn) -> Result<Scene>` selects the explicit MX profile. SCN
  supplies spawn; common TOY has no SceneInfo and is not used as an override.

Coordinates remain positive X/Z with Y up, spacing from PLN and no translation.
SCN extents agree with 256 × terrain dimension × scale: ordinary large maps
are 5760 square at spacing 4.5, most arenas 896 square at 3.5. Rectangular SX07
and the exceptional arena scales retain their source dimensions and spacing.

Heights use padded rows of 20 u16 values, retaining 17 per row. Each tile has
289 signed XYZ normals and two separate 289-byte streams. The second stream,
masked with 0x7f, is preserved as the zero-based MX `TerrainContact.surface`;
no ATV2 material conversion occurs. IDs observed across every stored source
vertex are 0, 1, 3, 4, 6 and 7. The parent owns MX semantic surface definitions.

The global palette starts at byte 45. All 1,812 embedded terrain texture
records have format 39, one 256 × 256 level and 65,536 indexed bytes (65,560
bytes including their headers). The RGBA decoder follows the existing MX PST
CLUT ordering and retains GS alpha units. The hierarchy mapping remains
available rather than flattening source references into a new atlas.

## Scene profile and collision semantics

MX permits 8 MiB documents, 131,072 assignments and 8,192 static/collision
records; ATV2 retains its original 1 MiB/4,096 assignment/model limits. SX08
has 4,874 static models. MX documents build a bounded lookup index preserving
exact section names, ASCII-insensitive keys and first-duplicate precedence.
Normal PhysicsData and ATV2 scene parsing keep the existing scan behavior.
The index is transient and omitted from serialization. weather.rs only changes
its existing local PhysicsData construction to an equivalent unindexed helper.

Zero MinDivisionSize adapts to 96 only in MX. This is a port partition choice,
not a claim about the PS2 implementation. Negative values still fail. The 29
scenes authoring both ecosystem distances as zero retain `[0, 0]`; integration
must avoid transition-distance division for this pair. Nonzero pairs retain
the existing ordering/finite checks.

`Scene.collision_sources` retains object index, name, world matrix, HasCol,
IsTrigger, Invisible, Hide, Type and Activity. Values use existing scene text
normalization, preserving spaces inside source quotes. MX metadata accepts
uppercase TRUE/FALSE as observed in Hide declarations. Explicit HasCol=False
records remain metadata and are excluded from physical `collision_models`.
ATV2 filtering and boolean parsing are unchanged. Missing resource names are
never aliased, combined or repositioned. Source-absent collision resolution and
PCO behavior belong to parent integration, not this terrain/scene adapter.

## Exact-build evidence

The private MX module exports corroborate these facts:

- 00480330 reads the PLN header, palette, scale/depth and tile/texture tables.
- 00475100 reads raw tiles as 4 + 680 + 867 + 289 + 289 bytes. Every consecutive
  source tile offset differs by 2,129 bytes across all 56 maps.
- 0047a0c0 addresses height index i using `i + (i / 17) * 3`.
- 0047b300 reads signed XYZ normals, applies morph to X/Z but not Y, and returns
  the second byte stream masked by 0x7f with corner contact data. It also has an
  alternate normal-table mode using the first byte stream; runtime selection
  remains unverified. This port uses the explicit XYZ normals.
- The shared signed-baseline/morph height formula matches every source leaf's
  authored extrema within 0.000018 world units. Ignoring morph causes large
  errors on National maps.
- 00171940/00171b50 read texture level counts, dimensions, byte extents and raw
  indexed payloads for format 39. Native code can swizzle these linear bytes
  for GS upload; this port expands linear indices to RGBA.
- 00478260/004786c0 use UV × 0.9375 + 0.03125; 0047f970 uses 30/32 plus 0.03125.
  This confirms the shared shader's eight-pixel inset and 240-pixel interior.

No pseudocode, disassembly or source bytes are included in this repository.
Persistent byte/bounds audit summaries are under the ignored private report
folder `mx-unleashed/map-formats`; intermediate extraction is in ignored work.

## Verification and remaining differences

Generated fixtures cover padded heights, split normals/surfaces, palette CLUT
order, truncation, large bounded scenes, MX nonphysical triggers and lookup
normalization/duplicate equivalence. The private source audits expect a flat
folder of 56 matching `.scn`/`.trn` pairs via ATV_MX_SCENES. Guarded atv-core
`mx_` tests with `--include-ignored --nocapture` run both source sweeps and
report host timing. CI needs no source content; private tests stay ignored.

The final guarded MX suite passed all 10 tests, including both private source
sweeps: all 56 SCNs and all 56 PLN terrains. Every embedded texture was expanded
(1,812 records), every dense heightfield was finite, and every SCN spawn had a
terrain contact. The SCN sweep also checks collision-count retention and MC09
Object52's nonphysical trigger. The combined suite took 45.79 seconds after
compilation on the shared host.

Thirty existing terrain, scene, PhysicsData and weather regression tests also
pass from the same compiled test binary. Precheck passes repository hygiene,
metadata, secret scanning and formatting. No full local CI or browser matrix
was repeated.

Two unoptimized shared-host runs measured 5,000 equivalent key lookups at
348.1/465.3 ms scanning versus 6.94/10.17 ms indexed. The 56-scene sweeps took
8.65/11.19 seconds while other tests ran. These are inspectable debug-test
measurements, not phone results or end-to-end ride startup measurements.

Remaining fidelity work: PS2 framebuffer comparison of palette/shading, exact
MX contact interpolation equivalence, alternate normal mode, original adaptive
LOD transitions and weather selection. Rendering uses the existing moving
full-sample patch/far grid. Physical-phone memory, frame time and startup have
not been measured. Runtime/menu integration, screenshot inspection and public
acceptance remain parent-owned delivery checks.
