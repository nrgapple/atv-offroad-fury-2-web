# Original map fidelity

The scope is every original map, including locked venues. The authored Rust port
uses the original assets and source-correlated behavior. An asset census, a
decoding pass, a working browser route and original-game equivalence are separate
acceptance gates. Modern effects are not substitutes for an unrecovered baseline.

## Complete source census, October 12, 2026

`python3 tools/audit_map_assets.py` reads all 147 original TRACKS RES archives,
covering 40 families: eight ENV, six FREE, eight NAT, sixteen SX, GAME01 and
TRAINING. It identifies 14,802 distinct name+byte-hash variants within those
families, retaining separate large/small and mode variants rather than counting
only basenames. These include 2,931 PSG, 3,254 PSM, 4,691 PST, 1,942 COL, 313 EMT,
106 ATM and 40 TR2 variants. Cross-family duplicates are still counted separately.
All inspected config and PSG headers parse; this does not establish full geometry
decoding, shader support or collision/effect behavior for the unconnected maps.

The report stays under `private/reports/map-fidelity/all-map-assets.json` and
contains source-derived asset identities and authored configurations. It is not
part of Git, CI or the R2 runtime package. Original asset variants are retained
privately; only the resources needed for a connected runtime route may enter R2.

Vancouver Ridge is FREE05. Its original `tracks.dat` record is `SPStunt6`, with
access initially disabled and a 1,000-point unlock price. Its source archive has
the actual `skijump.psg` and `skijump.col`, ski lodges, snowy vegetation, terrain,
sky and original materials. It is a different venue from Caribou Peak's smaller
ramp and ski props. The host connection and collision gates now pass below.

## Vancouver Ridge connection and collision gate

FREE05 now appears in Single Player → Freestyle with its original SPStunt6
name and two-part thumbnail. It is available without profile unlocking, as the
user requests. The archive uses `free05.scn` and `free05.toy`, unlike ENV's FR
suffix. Empty WaterFile metadata retains a dry field rather than requesting a
nonexistent file. The mode TOY supplies the first authored start/direction.

The host build decodes the original 1281×1281 terrain at 4.5-foot spacing,
detail texture, sky and 37,649 snowy vegetation placements. All 78 original
static meshes decode; 75 placements draw. The three original ice reflection
overlays await their coordinate-generation shader and remain explicitly skipped.
All 43 original solid placements load with their original collision matrices:
draglight, l_post, skijump, skilodge1, skilodge2 and skilodge3. Missing COLs on
non-solid overlays follow the established native loader rule. The ski jump's
PSG has one LOD/280 triangles; its COL has 200 vertices/279 original triangles.

`tools/test_vancouver.mjs` passes desktop and 390px: an asset-derived upper-ramp
start followed by ordinary throttle produces 123 wheel-support frames, 31 body
contact frames, an airborne interval after step 19 and later ground contact.
The harness changes the test start but supplies no velocity, impulse or wreck
state. It verifies every loaded solid's source placement, not physical encounters
with every face. The unlocked menu route and no-disc startup also pass. Inspection
views verify source RGBA uploads and 115,571 contributing ski-jump pixels. The
normal-camera captures show route context; the closer ramp image uses a test view.

The original drag light uses five ATM frames rather than a missing static PST.
Its source FPS 2 and forward looping now use a Rust frame clock following
`001bd8e0`/`001bdb60`. First preparation has no elapsed interval; elapsed unsigned
milliseconds, initial frame and loop-to-frame rules are preserved. The browser
supplies an active simulation clock quantized to milliseconds, preserving pause
and scene/reset lifetimes; original engine-wide scheduling remains unverified.
300 instruction cases/21,582 instructions have zero compared binary32 phase
differences against Rust. Texture-handle/frame-count lookup and finite host
fmodf are explicit services. Browser frame advancement/pause passes; original
hardware timing, reverse playback and other animation families remain open.

The source package adds only the required F05_FR archive and two Vancouver
thumbnail halves, through ignored storage/R2. No original content enters Git or
CI. The host gate passes 307 core, 22 network, 11 relay, three bridge and 66 JS
tests, setup, strict formatting/Clippy, web build, 16 release and five Worker tests.
Public release is a separate gate. Ninety-two original light definitions are
retained as metadata; point/spot/lens behavior, fog, shadows, vegetation/terrain
LOD, ice reflections and full PS2 rendering/gameplay equivalence remain unfinished.

## Static prelit materials and UV movement

The renderer formerly discarded PSG vertex colors and shaded prelit scenery as
ordinary sunlit props. It also held TexTransformPrelitCompressed materials still.
Static scenery now uploads the original RGBA stream for supported prelit passes,
uses original GS color/alpha modulation units, and connects source-alpha blending.
GEQUAL alpha-test materials distinguish KEEP from FB_ONLY: failed FB_ONLY pixels
retain color without Z writes through two mutually exclusive browser draws.
This restores the authored fade inputs; full GS quantization/ordering remains open.

The Rust translation-only UV animator follows loader `001be080` and update
`001be5d0`: accumulated offsets wrap with signed fmod, and the texture matrix
subtracts them. Freeport's static `rain` cards use authored V velocity 2; Arlington
waterduct `waterflow` uses .983. These are texture animation on existing original
geometry, not the random weather/lens-drop system. Scene loading initializes the
phase; ATV reset and graphics restoration preserve it. Fixed simulation batches
advance it once per step and pause stops it. Unsupported rotation/shear/frame
sequencing is reported rather than silently passed off as recovered animation.

Verification at the first host gate:

- 600 original-instruction cases, 78,000 instructions, zero compared binary32
  mismatches against Rust. Zero-angle trig and finite host fmodf are explicit
  supplied services; this is not a full native engine or hardware replay.
- `make check`, `make verify`, `make web`, 15 release tests and five Worker tests
  pass. All eight ENV loading/driving routes pass with no unsupported scenery
  meshes or JS/GL errors.
- Source-correlated inspection views verify original RGBA uploads and visible
  prelit draw contributions: rain 4,977 pixels, lava 73,978, waterflow 6,334.
  Animated offsets, pause and context phase retention pass. These contributions
  are not before/after improvement metrics or a PS2 framebuffer comparison.

## Remaining fidelity gates

Caribou ice/reflection groups still need their original coordinate generation.
Terrain projected pass selection and adaptive LOD, vegetation visibility,
per-material fog, point lights, random weather/lens effects, shadows, remaining
ambient emitters and actors, shader families and gameplay modes remain incomplete.
Original dynamic assets and configuration presence do not establish that they
interact, move or emit particles correctly. Continue through the source census
and record those distinctions at each tested checkpoint.
