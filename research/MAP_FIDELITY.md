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
static meshes decode; 75 placements draw. At this initial connection gate, the
three original ice-reflection overlays remained skipped; their later host
renderer recovery is recorded below.
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
The public release and browser gates are recorded below. Ninety-two original light
definitions are retained as metadata; outside the host-tested ice-reflection
groups below, point/spot/lens behavior, fog, shadows, vegetation/terrain LOD and
full PS2 rendering/gameplay equivalence remain unfinished.

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

## Public Vancouver and nine-map checkpoint

Public main `f585991579f8c44b098d6b137b1679eb8050e3fd` passed CI `38302626466`.
HTTPS checks match 31 authored files and 399 served objects; the approved R2
package contains 400 objects, six changes, and 305,224,223 bytes. The public
gallery serves 110 entries and 93 screenshot references. Vancouver Ridge/FREE05
is available through the original SPStunt6 Freestyle route, using its original
two-part thumbnail and default unlocked state.

The public nine-map route passes ENV01–ENV08 plus FREE05. FREE05 loads the original
1281×1281 terrain at 4.5-foot spacing, detail/sky, 37,649 vegetation placements,
78 source scenery definitions (75 drawn) and 43 placed solid colliders. Desktop
and 390px contact traces each report 123 wheel-support frames, 31 body-contact
frames, takeoff beginning at step 19 and later ground contact. The test starts at
an asset-derived ramp location and uses ordinary throttle without injecting
velocity, forces or wreck state. It does not exercise every collider face or
establish hardware handling equivalence.

Public material inspection reports 2,729 contributing pixels for ENV04 rain,
73,978 for ENV07 lava, 6,331 for ENV08 waterflow and 115,571 for FREE05's ski
jump. Source prelit attributes and authored rain/waterflow UV movement pass with
pause and graphics-context phase retention. The public frontend route passes the
original menu/track resources, keyboard/controller navigation, Tioga drive,
pause/Controls, mobile touch selection and Caribou reload without a disc picker.
The nine-map, material and frontend browser checks report no JavaScript or GL
errors. Pixel contributions are inspection evidence, not a PS2 framebuffer
comparison or a whole-map fidelity score.

The public f585991 release predates the reflection change below and still skips
the three Vancouver overlays. Exact fog, 92 original light/lens behaviors,
projected terrain/vegetation LOD, shadows, weather, other modes and progression
remain open; the public release does not establish original hardware or full-game
parity.

## Host reflection shader gate

`tools/test_reflections.mjs` passes the original local-space projection renderer
on ENV01 and FREE05. ENV01 restores 88 reflection groups across 275 instances,
with zero skipped groups and 152,862 contributing ice pixels. FREE05 restores its
three groups across all 78 instances, with zero skipped groups and 236,566
contributing pixels. The vertex path uses each PSG node's original local position
and normal, transforms the local camera through the transposed world basis, and
evaluates the original XY, XZ and YZ projections with source RGBA, blend and depth
behavior. Host inspection images remain under ignored
`private/reports/devlog-images/reflection-host-env01.png` and
`reflection-host-free05.png`.

Expanded GPU transform feedback compares the actual renderer vertex shader with
the same 768 private native arithmetic cases under identity, translated/rotated
placement and rotated child-node transforms: 2,304 GPU comparisons across 5,376
interpreted arithmetic instructions, with maximum absolute UV difference
7.152557373046875e-7. This validates local-versus-world camera mapping and the
XY/XZ/YZ swizzles for these cases. The test supplies a pre-normalized finite local
view direction; it does not validate PS2 hardware accumulators or reciprocal
behavior, nor replay the complete GS/VU pipeline.

The updated Vancouver desktop and 390px gate draws all 78 placements with zero
skipped groups. Both traces report 43 colliders, 123 wheel-support frames, 31
body-contact frames, takeoff at step 19, later ground contact, drag-light
animation and unlocked menu access. The nine-map browser regression passes
ENV01–ENV08 and FREE05 with terrain following, batched vegetation and no
unsupported meshes or JS/GL errors. The four-map material regression passes with
2,488 contributing rain pixels, 73,978 lava, 6,329 waterflow and 115,571
ski-jump pixels, including source colors, UV motion, pause and context retention.
The frontend route passes title/menu/controller/touch/pause, Tioga drive and
Caribou reload without a disc picker or browser errors.

`make check` passes 308 core tests, 22 network tests, 11 relay tests, 3 bridge
tests and 66 JavaScript tests. `make verify`, `make web`, 17 release tests, 6
Worker tests, source hygiene and `git diff --check` pass. The host reflection
integration gate is complete. It remains unpublished and absent from the public
f585991 release; public release is pending PR9 integration. Fog, lighting/lens
effects, LOD, shadows and weather are not covered by this checkpoint.

## Host fog and shared coverage gate

The later integrated host candidate reads source Fog Visibility and Haziness
from connected map scenes. ENV01/ENV04 resolve to visibility 1, haziness 1 and
range 0–768; FREE05 defaults to visibility 1, haziness 0.5 and range 352–768.
The ENV01/ENV04 source haziness value is 1.2; Rust applies `min(1)`, yielding 1.
The view far clip follows fog end through source
routine `0012b360`. Compressed prelit materials use source-correlated 8-bit GS
fog-coverage packing and the per-PSM `foggingOnOff` switch (native constructor
`001b9810` defaults to 1); the water base pass uses its source fog switch. The
shared GPU coverage path uses screen-affine interpolation, replacing the prior
guessed exponential curve.

The actual host GPU matches 3,000 source-correlated coverage samples with zero
byte differences. One triangle sample produces 55 on GPU, 55.25 under affine
interpolation, and 96.25 under perspective interpolation. Rust range/coefficient
checks match 600 complete scalar cases across 35,393 original EE instructions
with zero compared binary32 mismatches. A further 3,000 packing samples are
source-correlated expressions with finite distance supplied; they are not full
VU runs or a hardware replay. Native GS precision for terrain and water remains
unverified. The fog-integrated reflection regression passes 2,304 GPU
comparisons with maximum UV error `7.152557373046875e-7`; ENV01 and FREE05 report
141,770 and 236,380 contributing pixels respectively.

The corrected disassembler execution table confirms SUBAz writes ACC, contrary
to the previous special SUB broadcast label. Reflection XYZ comparisons remain
unchanged because their tested operation wrote W only. Vancouver desktop and
390px traces each draw all 78 placements with zero skipped groups, 43 colliders,
123 wheel-support frames and 31 body-contact frames, with takeoff at step 19 and
later landing. All nine maps, material source RGBA/UV/pause/context tests, four
wet-map and 390px water checks, dry-map behavior and frontend routes pass with
zero JS/GL errors. `make check` passes 309 core, 22 network, 11 relay, 3 bridge
and 66 JavaScript tests; `make verify`, `make web`, 17 release tests, 6 Worker
tests and source hygiene pass.

At the time of this host-only integration result, it was absent from public
`a4d3003` (CI `38309959773`) and historical releases `cef26f4` and `f585991`;
it was later validated in the e18a371e public release below.
The packing validation does not establish a full
native VU or PS2 pipeline. Terrain/water GS precision, water reflection/froth,
source-specific program precision, active-view overrides, weather callbacks,
lighting, shadows, lens effects and LOD remain open. The inspected ENV04 image
is a host candidate capture, not a public-build or PS2 framebuffer comparison.

The combined current-main host gate also passes multiplayer integration checks.
Two-client FREE05 and copied-link-pair routes draw 13 remote models. Four
actual clients each render three remote quads/riders in 39 draws with three
voices; all four movement drivers pass, and host departure clears remote
models/voices. Cancellation during map loading passes. Timing was measured in
one Mac headless Metal synthetic-focus run, so it does not establish physical
device or Internet performance. Fog/reflection remain host-only candidates;
this result does not add them to public `a4d3003`. The combined check passes 309
core, 25 network, 3 relay, 12 bridge and 68 JavaScript tests, plus 17 release
tests, 6 Worker tests, verify, web and source hygiene.

## Public fog and reflection release (e18a371e)

Public main `e18a371e81b471137dd5fb1121b05d454584b1d0` passed CI
`38312395498`. The validated artifact matches 32 authored files and all 405
served objects. Canonical R2 has 106 records, 306 referenced files and 406
approved objects across three changes; the gallery has 118 entries and 99
screenshots. ENV01/ENV04/FREE05 public fog checks match 3,000 GPU coverage
samples with zero byte differences, including the tested screen-affine sample.
Fog packing remains source-correlated; this is not full VU/GS or PS2 hardware
replay.

Public reflections draw ENV01's 88 groups/275 instances (141,770 contributing
pixels) and FREE05's three groups/78 instances (236,380 pixels), with 2,304 GPU
comparisons and maximum UV error `7.15e-7`. All nine maps pass. Materials report
4,618 rain, 73,970 lava, 6,335 waterflow and 115,571 ski-jump contributing
pixels with pause/context retention. Four wet maps, 390px water projection and
dry-map behavior pass. Vancouver desktop/390px contact traces draw 78 placements
with 43 colliders, 123 wheel-support frames, 31 body-contact frames, takeoff at
step 19 and later landing. The public frontend and copied-link FREE05 multiplayer
pair pass with 13 remote draws, both rigs, audio and movement.

## Remaining fidelity gates

The fog/reflection path is now public at e18a371e. Original hardware equivalence
remains open. Native terrain and water GS precision, source-specific precision,
active-view overrides and weather callbacks remain unverified. Source vegetation
material work is a separate host-only WIP. Lighting is still under research:
ambient prototype tests validate supplied zero directional weights only; they
do not establish source tree lighting, and the recovered global Sun matrices
still need to be incorporated into the prototype. Terrain projected pass selection and
adaptive LOD, vegetation visibility, point lights, random weather/lens effects, shadows, remaining
ambient emitters and actors, shader families and gameplay modes remain incomplete.
Original dynamic assets and configuration presence do not establish that they
interact, move or emit particles correctly. Continue through the source census
and record those distinctions at each tested checkpoint.
