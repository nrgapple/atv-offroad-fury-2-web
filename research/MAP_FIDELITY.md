# Original map fidelity

The scope is every original map, including locked venues. The authored Rust port
uses the original assets and source-correlated behavior. An asset census, a
decoding pass, a working browser route and original-game equivalence are separate
acceptance gates. Modern effects are not substitutes for an unrecovered baseline.

## Host ordered vegetation checkpoint

All nine connected maps now load their original `EcoSystemCode2D` materials and
atlas textures from the existing hosted archives. Species retain authored U
left/center/right, V top/bottom and the mesh-derived base-height offset. The
six-vertex triangle fan has four triangles and three separately shaded columns;
two shipped species with empty atlas rectangles retain that absence.

The scene reader recovers `EcoDistance3D`/`EcoDistance2D` defaults of120/180feet
from00202bd0.0012a1c0 supplies the quantized distance phase, near-entry signed-short
orientation and stale-visibility phase reset;002ff150 supplies distant alpha;
002fe710 overrides near TEST/alpha with GREATER/KEEP. The transition billboard
pass disables depth writes and uses reference1. Each map shares one original
atlas across species: distant and transition queues now use two ordered instance
draws total, with descending projected depth. Near meshes remain grouped.

Rust decoding uses direct species-slot lookup instead of a species search for
every placement. It builds stable64² packed-coordinate cells with prefix offsets
and original placement indices. The browser queries relevant cells and selects
up to1500 placements with a reusable max heap; it sorts only that bounded result
for transparency. This acceleration preserves the current browser visibility
envelope and source identities. It does not reproduce native spatial callbacks,
whole-depth-bucket cutoff or clipping/cache order.

Host evidence: all nine map driving and atlas gates pass with8–17 total vegetation
draws and two billboard draws per frame, zero browser/GL errors and visible
billboard pixels. The indexed selection matches a full scan on every map,
including capped ENV02/ENV06 selections. Starting views scan1385–4802 placements
out of6617–39641 (about69–92% fewer checks); this is work-count evidence, not an
FPS or physical-phone benchmark. All nine near-material gates pass; ENV07 uses a
test-only focused authored-fern inspection view because its starting near queue
has no contributing pixels. Normal driving captures remain separate.

A private finite interpreter executes600 original002fea30 frame preparations
(228606 EE instructions), supplying view/global ambient/Sun state. The actual
browser shader matches1800 three-column RGBA comparisons with zero differing
bytes. Another300 EcoSystemCode2D cases supply correlated facing-view matrices,
finite packet data, accepted clip flags and scalar Q/P:1800 GPU vertices have
maximum view-coordinate error0.0009765625 feet and exact atlas UVs. This does not
emulate VU latency, EFU/hardware precision, native clipping, GS viewport/depth or
full callback scheduling. Billboard fog currently uses shared per-corner biased
coverage; original per-row fog/depth packing remains to connect.

`make check` passes317 core,25 net,3 relay and13 bridge tests, strict Clippy,
formatting and68 JS tests. Setup verification,17 release and6 Worker tests pass.
Vancouver desktop/390px retains78 scenery placements,43 colliders,37649 plants,
123 wheel-support frames,31 body-contact frames, takeoff at step19, later landing,
ATM frame1 and the unlocked route. FREE05 copied-link multiplayer retains13
remote draws, both rider rigs, one voice each, movement and host cleanup. Original
material animation/context, fog and reflection regressions pass. These are host
checks; this candidate is not yet a verified public release. Wind/weather,
remaining lighting/shadows, exact visibility, other modes and full-map/game
equivalence remain open.

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

## Public near-vegetation material release (d9596d9)

Public main `d9596d937907395440b61e78be61b2045d987d4d` (PR #12) passed CI
`38318057407`; deployment identity matches the validated artifact. Canonical R2
retains 106 records and 306 references; 408 approved objects totaling
310,198,438 bytes are published. HTTPS verifies all 32 authored files and 407
served objects by hash, size and MIME. The gallery has 120 entries and 101
screenshots.

Near-vegetation materials pass on ENV01–ENV08 and FREE05, with 600 actual-GPU
RGBA comparisons and zero byte differences. The public 1,200-step transport
check exactly matches sequential frame and camera decimal outputs; this release
has no production timing benchmark. The public map route passes all nine maps.
Vancouver desktop/390px draws all 78 placements with 43 colliders and 37,649
plants; contact evidence includes 123 wheel-support frames, 31 body-contact
frames, takeoff at step 19, later landing and ATM frame 1, with the map unlocked
by default. Public touch input matches batched/sequential behavior for 124 steps,
including shifted release and cancellation. A public copied-link FREE05 pair
passes with 13 remote draws per client, male/female rigs, one voice per client,
host cleanup and 4.9307901763-foot remote distance. These are component/route
checks, not full-scene or PS2 parity; no public FPS/timing claim is made, and the
transport microbenchmark remains host-only. The inspected ENV07 capture remains
private until a later validated current-main artifact includes its gallery
reference. Weather callbacks, distant vegetation/LOD, point lights, shadows,
native GS/full-frame precision and complete source tree lighting remain open.

## Map audio/render configuration audit

A review of the extracted `.scn`/`.toy` configuration for ENV01–ENV08 and
FREE05 finds nine distinct map/config pairs: the eight `envNNfr.scn/.toy` pairs
and Vancouver's `free05.scn/.toy` pair. It is a configuration audit, not a
complete inventory of runtime events. All nine scene files name terrain, sky,
ecosystem and particle-texture
resources. ENV04/05/07/08 point to water files; FREE05's water filename is
blank. Ambient and directional base-light records appear in each scene. Five
scenes additionally declare 140 object-point lights in total: ENV03 30, ENV04
5, ENV05 1, ENV07 14 and FREE05 90. The FREE05 records also carry light-flare
attributes. Their presence in source configs does not prove the browser currently
renders each light, flare or callback faithfully.

Only ENV01 and ENV04 explicitly store fog visibility and haziness in these
files; the other seven provide fog color without those fields. Explicit rain
sections appear in ENV04 and ENV05 with chance 0, and ENV07/ENV08 with chance
0.1; the other five have no rain section in the audited SCN. This is a declaration
census, not proof of runtime weather policy or a map's full set of weather
effects. Detail/fog rendering and light behavior remain subject to the limits
above.

There are no map-level sound/audio/music keys or `[Sounds]` declarations in the
nine `.scn` files. The original ambient-sound reader `002053d0` reads
`[Sounds].NumAmbientSounds` with default zero, so these files declare no
per-map ambient entries on that reader path. Among the nine `.toy` files, only
Arlington/ENV08 declares looping sounds: eleven animation-bound loop records
reference three original train samples. This train audio route is separately
reconstructed and documented in [Audio reconstruction](AUDIO.md). The other
eight TOY files contain no sound/audio fields. Model-archive bindings, global
music selection and dynamic event dispatch remain outside this config census;
it does not establish that the maps have no audio.

## Host Sun-lit vegetation sample gate

A separate host-only candidate compares 600 selected directional vegetation
lighting cases. The expanded native probe includes the original light base
constructor `00137ca0`, validating its missing/zero-look fallback to native
direction +X rather than supplying initial object state. It matches 303,599
original EE and 12,600 VU instructions with zero directional-vector bit
differences and zero packed-color byte differences, including the zero-look
case. The expanded actual-GPU run also passes all 600 cases with zero byte
differences. The probe uses decoded normals and instance basis; it does not
execute the complete frame callback or establish whole-scene lighting,
vegetation LOD or PS2 hardware equivalence. Source tree lighting is not complete.

Focused browser checks pass source blending, fog, depth and renderer-state
isolation. The three host captures report 2,176 ENV01, 1,167 ENV04 and 4,251
FREE05 contributing pixels; these are from the original focused three-map run,
not full-scene scores. The subsequent near-vegetation material gate passes on
all nine requested maps, with source blending/fog/depth switches and renderer
state isolation, and zero GL errors.
All nine maps load and drive. Material checks pass with 2,604 rain, 73,970 lava,
6,335 waterflow and 115,571 ski-jump pixels plus pause/context retention.
Vancouver desktop/390px passes with all 78 placements drawn, 43 colliders, 123
wheel-support frames, 31 body-contact frames, takeoff at step 19, landing, ATM
animation and the default-unlocked menu route. A copied-link FREE05 multiplayer
pair passes with 13 draws per client, both rigs, audio and host cleanup. The
original-map touch test passes 124 steps batched versus sequential and restores
initial world clocks on fresh instances for shifted-phase replay. Fog sampling
passes 3,000 GPU bytes with zero difference and the affine sample at 55.

`make check` passes 311 core, 25 network, 3 relay, 13 bridge and 68 JavaScript
tests; formatting/Clippy, setup, 17 release, 6 Worker and hygiene checks pass.
The reflection gate passes ENV01's 88 groups/275 instances/141,770 pixels and
FREE05's three groups/78 instances/236,380 pixels, with 2,304 GPU comparisons
and maximum UV error `7.152557e-7`. The frontend gate passes original
title/menu/controller/mobile-touch/pause/Tioga/Caribou/no-disc routes. The
near-vegetation material and reconstructed Sun/ambient path was later released
and verified publicly on all nine maps in d9596d9, including the 600 GPU color
cases. The original arithmetic probe remains private research; neither that
probe nor the release validates the complete inherited Sun callback/global frame
path or complete source tree lighting.

The source material census assigns `DiffuseCompressed` to the near vegetation
shader family and `EcoSystemCode2D` to corresponding distant variants. Across
the tested ENV01/ENV04/FREE05 resource sets, all 25 inspected models have a
single-node identity root, so the tested normal/basis path has no additional
bind-root transform in those models. This bounded census does not cover every
map asset, distant transition, billboard/LOD behavior or complete GS parity.

The four-step WASM benchmark measures the selected operation plus output copy,
using five alternating runs of 200 batches after 30 warmups with the full
vegetation proxy loaded. Measured per-four-step batch times are ENV01 0.8435 to
0.479 ms (43.21% less batch time), ENV04 0.8115 to 0.448 ms (44.79% less),
ENV08 0.912 to 0.506 ms (44.52% less), and FREE05 0.800 to 0.4395 ms (45.06% less).
Loading, rendering,
workers and phones are excluded, so these are not
end-to-end game-performance results. Across 1,200 steps with counts 1–4, every
output f32 field remains bit-equivalent; frame and camera decimal outputs match
the current sequential implementation.

## Remaining fidelity gates

The fog/reflection path is public at e18a371e, and near-vegetation materials
were later verified publicly across all nine maps in d9596d9. Original hardware
equivalence remains open. Native terrain and water GS precision, source-specific
precision, active-view overrides and weather callbacks remain unverified. The
selected directional Sun vegetation cases pass the bounded host and public GPU checks above,
but they do not validate the full inherited Sun callback/global frame path or
establish complete source tree lighting. Terrain projected pass selection and
adaptive LOD, vegetation visibility, point lights, random weather/lens effects, shadows, remaining
ambient emitters and actors, shader families and gameplay modes remain incomplete.
Original dynamic assets and configuration presence do not establish that they
interact, move or emit particles correctly. Continue through the source census
and record those distinctions at each tested checkpoint.
