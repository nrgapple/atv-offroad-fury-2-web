# Water contact reconstruction

Public release `22ab240cce0cea813b482ee20dbc22d5908ec69c` (CI `38204399428`) connects the shared dynamic physics height field for ENV04/05/07/08 from their original WTT profiles and raw PST signature channels in the separate runtime manifest. The uninterrupted public eight-map check verifies four live channels and finite heights on those maps, and null fields with cached -256 on dry maps. The authored Rust rider-water response is connected, but that browser check does not exercise an actual rider-water contact route. Source bindings and synthetic response regressions provide narrower evidence. At that physics checkpoint, visible water surfaces and wakes remained absent; full coupled water parity is unproven. The later host-visible-surface gate is recorded below.

The authored Rust `water` module reproduces the USA v2.00 buoyancy force kernel at `0033dd90` and its spring configuration at `0033da90`. The following initial kernel and surface checkpoints describe earlier isolated work; current integration is detailed below.

The original selects one complete set of contact points based on speed, samples
water height at each world point, caps penetration depth, and applies a spring
and velocity damping force. Angular point velocity contributes to both force and
ripple strength. Translational drag is applied once when any point is submerged.
The rider's tuning comes from its constructor at `001ee090`. The recovered direct
call graph has no other constructor/configuration caller for this buoyancy actor;
no matching ATV buoyancy actor appears in `001dd830`. Do not apply the rider's
float response to the ATV. Callers supply displaced heights and original point
bindings.

A private instruction interpreter executed 300 varied configurations and contact
sets through the actual source routines: 192,299 instructions, with bit-identical
spring coefficients, force, torque, selected points and ripple strengths. The
probe supplies identity point transforms and a controlled height callback. It
covers angular velocity, water flow, depth limits, dry contacts, and high/low-speed
point selection. Synthetic regression tests preserve an original force/torque
checkpoint and the strict speed threshold.

At the initial isolated-kernel checkpoint, live integration still required recovery of the dynamic surface and its initialization. The
original `00199cd0` height query combines a ripple grid, authored signature waves,
and the environment's base height; using only a flat base plane would change
contacts. Dynamic surface initialization, world flow/up initialization and rider point bindings were subsequently recovered and connected as detailed below. Ripple propagation and water rendering remain incomplete. No complete water-physics or rendering parity
is claimed by this isolated kernel.


## Initial isolated surface height checkpoint

`water_surface` now reproduces the composed height query at `00199cd0` and
signature texture sampling at `00192090`. Live water was disconnected at this historical checkpoint.
The first probe executes 600 original height queries (59,592 instructions),
matching every output height bit and sampler-call gate. A second executes 500
signature queries (51,212 instructions), matching every output height bit across
signed maps, texture strides, tiled/non-tiled coverage and frame weights.

Level loader `00200650` selects the scene's WaterFile. `00200800` creates the
field at level `+408` from `baseDisplacement.tga`; setter `0019a240` binds that
field to water `+e0` and exposes its source map. The field's virtual height getter
is `00192090`. It sums separate positive/negative raw height texture channels,
using whole-byte integer sampling and original multiplication order, and blends
two frame sums. Native unsigned DIVU wrapping is retained, including negative
coordinates; non-tiled coverage includes the upper edge. Display palette colors
must not be substituted for the raw height bytes.

The composed query adds signed ripple-grid heights and optional base displacement
before the signature result, scales by 1/4096, and adds the current base. A nonzero
base-map contribution selects the authored unswelled base instead. A forced-base
query, or a result more than PoolDepth above the input point (default 100 feet),
returns the current base. Disabled infinite-plane mode with zero base-map
contribution returns zero without invoking the signature sampler.

Native ripple storage packs eight signed heights in the first sixteen bytes of
a sixty-four-byte block. The query samples one cell without interpolation.
`00199f80` injects contact disturbance into the velocity portion of those blocks;
the grid propagation/update remains to be ported. The current helper accepts
logical height grids; it does not claim a complete evolving ripple simulation.

The eight currently connected scene files select WaterFile only for ENV04/05/07/08.
Their authored bases are 114.409/128/149.5/119.5 feet, respectively. All four WTTs
use 0.75-foot base swell and period 1.5, and include animated/scrolling signature
height maps. Native `00188680` advances elapsed time and computes sine of
`elapsed / period` after angle wrapping; the period is a divisor in radians,
not an invented full-cycle duration. The source sine-polynomial backend and one-frame signature initialization/update
are now recovered below. Live actor binding was subsequently connected as detailed below; ripple propagation and rendering remain separate work. A flat-plane fallback is not a parity implementation.

Rider update `001ef690` runs gravity, water response, then solid contacts during
its at-most-1/90-second substeps. Either water touching or solid contacts activates
the rider body and latches the rider contact/recovery flags. Only solid contacts
apply the separate final damping stage. The connected rider path now feeds this water contact into the existing wreck recovery grounded gate; its source dispatch and synthetic tests are described below.

Native inputs, source textures, original exports and result files remain in ignored
`work/priority-water/` and `private/reports/`; no game content was added to source.


## Connected dynamic physics surface

`water_animation` now provides a runtime-byte constructor and stateful height
field. `Definition::parse` reads the WTT profile and
`required_height_resources` returns its deduplicated raw height PST names.
`LiveField::new` requires every required texture; it rejects unsupported animated
frame profiles and nonzero wake/base-map contributions rather than replacing
those fields with a flat plane. All four currently connected water WTTs explicitly
use one-frame height maps, zero `WakeScale`, and zero `WaterHeightRange`.
Those two zero scales make evolving ripple grids and the base displacement map
irrelevant to **these physics height queries**, while their render effects remain
separate work. The field now has live caller APIs for the connected scene profiles; browser checkpoint verification is reported separately.

Each authored channel-zero wave expands into separate `HeightNeg` and `HeightPos`
channels; bump-only waves omit both. `00198ca0` multiplies authored height scale
by sixteen. Native `0030a9c0`/`0030acb0` starts the bitmap dimension at one;
`00197410` then divides the initialized scrolling rates by the loaded bitmap
width. Thus stored rate is `(VelocityX * widthX) / bitmapWidth`, likewise Z.
It is not the authored VelocityX used directly as feet per second.
`0030af00` advances time and signed `fmod` scrolling. `00197f10` separately
publishes the height sampling origin as authored position plus scrolling during
render preparation. The height query uses the last completed preparation's
origin, while the current frame's water base swell is already updated.

`001ab180` is a binary32 sine polynomial through the fifteenth power, with seven
coefficients and a fixed multiplication/addition order. `SwellState::advance`
implements the original elapsed/period division, signed remainder and angle
wrapping, then the polynomial. No sine lookup table or host sine substitution is
needed. The base-swell period remains a radians divisor, not a full-cycle duration.

The source bitmap binding is also established. `00159f00` reads the stored
8/16/32/64/128-pixel levels into indices 4/3/2/1/0, and stores the largest width
as the base bitmap dimension. `0015d150`, the getter used by the water field,
returns index-zero pixels and the base stride. `00197410` clears the global texture-swizzle
flag at `00404868` while loading signatures, disabling the swizzle in `0015a340`.
Therefore physics uses the largest level's unmodified raw palette-index bytes,
not the smallest stored mip or displayed palette colors.

The FR runtime archives already transported for ENV04/05/07/08 contain
`tracks/env0N/env0N.wtt` and the two required textures:
`tracks/env0N/Sigs/wave0000HeightNeg.pst` and
`tracks/env0N/Sigs/wave0000HeightPos.pst`. Each PST is 22,940 bytes and supplies
128 by 128 raw height pixels. Multiple authored waves reuse this pair.
Look up resource member names case-insensitively; no new public source-code
asset files or flat-plane metadata are required.

Validation executes 480 complete staged surface frames from the actual four
WTT/PST profiles: 11,520 composed height queries and 5,002,596 original
instructions. It runs `00188680`/`001ab180`, `0018d760`/`00198080`/`0030af00`,
`00197f10`, and `00199cd0`/`00192090`, checking both prior and newly prepared
origins. Every queried height, swell state, scroll and published origin matches
Rust binary32 results. Graphics emission, bitmap access and the libc remainder
service are hooked at their service boundaries. Shared signature blend-clock
provenance remains unresolved; this fixture supplies its zero-initialized value,
and live scope explicitly requires one-frame signatures with authored fps zero.
This is a dynamic **surface** replay, not a complete vehicle/water replay.

Independent checks execute 600 varied wave updates (35,513 instructions), 600
swell branches including the complete original polynomial (60,630), and 500
wave constructors/bitmap-rate normalizers (104,000). All compared state values
match Rust. The original PST loader/pixel-copy/getter executes another 1,158
instructions for the positive and negative textures, verifying largest-mip
binding and all 32,768 raw pixels. Targeted tests and strict core Clippy pass.

The connected caller advances this shared field once per outer frame, lets the ATV and detached rider query it during their updates, then publishes signature origins during the completed render-preparation phase. Vehicle recovery/reset preserves the shared scene's swell/scroll clocks; loading a new scene constructs a fresh field. Terrain-solid and water-touching flags remain separate. The rider `+44` water actor runs before its solid solver and feeds the
recovery contact latch. ATV `001e5260` stores water height in `+900` late in the
outer update, so its earlier submerged tests read the previous outer value.
Its `+994`/`+99c` effect categories become eight for a grounded corresponding
front wheel when origin Y is strictly below cached height minus 0.5; this does
not write the wheel's physical terrain material byte. The deeper cached-height
minus-three test drives engine/audio state. The detached rider bindings and response are described below. Live water rendering remains separate work, and boundary/checkpoint gates require their own actor verification.


## Detached rider water contacts

`Animator::step_with_water` accepts an optional shared `LiveField`, preserving
`step` as the existing no-water wrapper. `002d7dc0` registers four low-list
water points on **UprTorso**, in this order: `[+x,0.5,+0.3]`,
`[-x,0.5,-0.3]`, `[+x,0.5,-0.3]`, `[-x,0.5,+0.3]`, where x has binary32
bits `0x3eb95810`. The high-speed point list is empty, so `0033dd90` selects
these same four low-list points even above its speed threshold. The thirteen
solid contacts remain a separate list and supply the bounds-based body center.

The source global initializer `00451470` executes 32 instructions with no
hooks, writing flow `[0,0,0,1]` at `004b84c0` and up `[0,1,0,1]` at
`004b84e0`. XYZ therefore yields zero current and vertical buoyancy.
The constructor is present in the contiguous static initializer pointer list
at `004518f0`; these vectors are not guessed from the BSS zero state.

Within every at-most-1/90-second substep, `001ef690` runs gravity (except
while attached), then the water actor, then the solid contact solver, then the
body integrator. Each water point samples the dynamic field and applies the
already validated `0033dd90` force kernel. Any water or solid touch activates
the detached body and latches the current outer frame's recovery contact flag.
`Frame::grounded` exposes that latch; `water_touching` exposes the **last**
substep's water flag, and `water_contact_count` counts point touches across the
outer frame. Solid contact counts remain independent. Final linear damping
runs only when the last substep has solid contacts, not for water-only contact.

`001ee990` uses phase-specific last-substep priorities: attached prioritizes
water over solid; airborne prioritizes solid over water; ground enters swimming
on water contact; swimming enters ground on solid contact. A dry swimming frame
retains its swimming phase. Swimming selects the original `Swim` motion, and
its one-entry clip table still consumes one random word. Attached-to-water
entry uses the ordinary no-motion detach path, including its three angular
random draws and velocity reset, after the current frame's body integration.
Clip requests are consumed on the next outer animation step.

Native setup and phase-dispatch probes execute 3,084 instructions, proving the
four offsets/node bindings/list layout and all sixteen phase/solid/water
combinations. Another 64 original `001ef690` sequences execute 29,088
instructions, proving dispatch order, substep sizes, body activation, recovery
latch, last-water flag and solid-only damping. Allocation, node lookup,
animation/resource loading, RNG services and actor kernels are hooks in these
dispatch proofs. The force kernel's earlier 300-case instruction comparison and
the dynamic height query replay remain independent proofs; this is **not** a
complete instruction replay of coupled animation, buoyancy and solid collision.

Integrated regressions use a synthetic varying signature to check water-only
body activation, recovery contact, swimming entry and finite buoyancy, with no
solid hits. A dry supplied field matches the old no-water path for 120 frames.
Phase priorities and dry/reset flag lifecycle are also covered. Connected
profiles have authored `WakeScale=0`, so the unconnected ripple-injection visual
service cannot alter these physics heights. No rider buoyancy is applied to the
ATV, and visible water/wakes remain separate rendering work.

### Rider kinematics cache across animated rebinding

The water kernel consumes world center `body+60` and world angular velocity
`body+290` cached by the last body integration. `001ef690` changes local CoG
from animated bounds before dispatching water, while retaining both world
fields until `001469e0`. Model rebasing and `001eff20`'s local angular velocity
transfer/random spin likewise do not refresh the water angular cache. Force
application still uses the **current** Model and new local CoG for its torque
lever. The chassis solver independently refreshes its own world CoG; it must
not inherit this delayed wheel/water center behavior.

`water::PointKinematics` and `apply_contacts_with_kinematics` expose this source
distinction, retaining the existing kernel wrapper for callers whose body state
is already current. The wreck animator publishes the explicit center/angular
cache only after integration and deferred contact translation. It preserves that
cache during disabled integration and detach. Wreck entry and recovery also retain
it: `001efb90` calls body reset `00146700`/`00146780` to clear velocity, force and
integration history without writing world `+60` or `+290`; `001f0740` deactivates
the wreck query/animation without clearing those fields. A fresh animator starts
with zero world cache, while successive wrecks use the last enabled integration's
cache until the next enabled body step. Animation and force geometry continue
using the current pose.

The complete original `0033dd90` kernel, including four torso Model transforms
and force/torque application, executes 300 synthetic rebinding cases and 504,600
instructions. The cached-state API matches every compared force and torque bit.
The formerly immediate state differs in 300 force and 600 torque components;
maximum differences are 109.79849 and 74.17312 in these synthetic source units.
The native height/ripple services are hooks in this probe; source-height replay
remains a separate proof. Four original `001ef690` dispatcher cases execute
1,802 more instructions and establish the first/second substep cache boundary.
An additional 64 original wreck-entry/recovery cases execute 17,280 instructions,
checking preserved world center/angular cache and cleared local/linear velocity.
Query activation, animation reset, RNG and the memset service are hooked in this
lifecycle probe; the cache and velocity stores and body-history dispatcher execute.
Integrated regressions exercise animated bounds/Model rebinding, different
cached versus detached local angular velocity, publication between two native
substeps, preserved detach/recovery/repeated-wreck cache and disabled-body dry
frames. A checked binary32 kernel case covers forces, torque and all four ripple
strengths independently of the integration assertions. Existing dry and wet
wreck tests and the actual-content 1,694-pose/five-type recovery probe pass.
These remain component/port checks; a fully coupled original rider/water/solid
instruction replay and public wet-wreck integration check remain outstanding.
Reports and reproducible probes remain private under
`private/reports/water-rider-*` and `work/priority-water/`.

## Visible surface checkpoint (host verified, publication pending)

`water_render` and `web/water.js` connect a portable projected water pass for the
four actual WaterFile scenes, ENV04/05/07/08. Arlington Stream is ENV08, as already
established by the original scene/catalog work in `GEOMETRY.md`; it is the
railroad environment. Dry scenes do not instantiate this renderer. All four
connected WTTs request `RenderInfinitePlane=1`, `NumPools=0`, zero wake and base
map height scales, and one-frame signatures. Finite pool objects, animated
signature frames, and differing height/derivative scrolling bitmap dimensions
fail explicitly instead of receiving an invented flat-plane replacement.

The scene's water is not an ordinary PSG in the static model list. Native
`00200650` creates the dedicated PSXWater actor; `00188390` submits its VU-generated
projected surface. The initial portable implementation derived its source quality-one
counts from `00187bc0`: 334 projected subdivisions and three strip groups. A
private original-instruction prefix probe verifies those counts in seven quality
cases, 434 instructions, without hooks. It stops before profile initialization;
it does **not** prove the VU's spatial orientation or exact topology. The portable
335-by-4 indexed mesh had 1,340 vertices and 6,012 triangle indices. That first
version assigned the dense axis to viewport width; subsequent native viewport
evidence corrects this below. The camera projection, triangulation and
interpolation remain authored adaptations, rather than an instruction-equivalent
VU geometry port.

The material and texture inputs are original runtime resources already present
in each hosted Freestyle RES. `env0N_water.psm` names the Water microprogram and
`Env0N_WaterEnvironment.pst` / `Env0N_Froth.pst` texture bindings. WTT environment
RGB values use the original GS modulation units, with 128 as unity. The connected
environment pass uses the authored alpha-blend and disabled-depth-write settings.
The environment texture's original alpha is retained. No generated water color,
modern Fresnel term, extra lighting or second visual wave clock is introduced.
`PolygonScale` is preserved as authored metadata but is not applied: no verified
native use of that key was found. AdditionalFov is likewise exposed rather than
inventing a separate gameplay camera.

The height atlas contains the largest raw positive/negative signature bytes,
independently of their display palette. Repeated waves share two 128-by-128 byte
maps. Visual normal inputs use the original four `DxPos/DxNeg/DzPos/DzNeg` PSTs,
packed into one 128-by-128 RGBA atlas. The original one-frame GS blend rounds
`128 * BumpScale + 0.5` to an integer FIX before blending: authored 0.2
becomes 26/128. The browser uses this recovered weight. An actual `00197980`
instruction probe covers both derivative signs, five scales and two frame weights
(40 cases, 3,220 instructions), including the GS ALPHA sign and FIX fields. These normal
textures use display intensities and smooth sampling; treating nearest raw
physics heights as a visual normal map produced visible noise and was removed.
The native derivative channel construction and BumpScale are observed at
`00198ca0` / `0030acb0` / `00197980`. Complete original GS intermediate-map arithmetic
and its VU normal/reflection-coordinate mapping are still unverified; the
portable environment-coordinate mapping is a documented approximation.

WASM operation 66 builds these render inputs only after operation 64 installs the
shared `LiveField`. Each fixed-step batch exports that field's completed
render-preparation base height and signature origins. The browser uploads only
those small uniforms per frame, and samples the same raw textures with the
original integer truncation and wrapping for vertex displacement. Physics keeps
its authoritative Rust sampler and the original previous-preparation contact
ordering. Resetting the ATV retains field clocks, while loading a scene creates
a new field. Context restoration recreates GPU resources from the retained
runtime render inputs and last prepared snapshot.

The initial per-fragment projected-plane implementation was measurably expensive.
Moving height queries to the reused projected mesh also restores ordinary early
bank/prop depth rejection. With original normal textures, the final pass adds one
draw and 137,432 bytes of GPU texture/mesh storage (about 134 KiB): 32 KiB raw
heights, 64 KiB normal channels, 16 KiB environment pixels, and 22,744 mesh bytes.
No texture or mesh-buffer uploads occur during ordinary frames. At most four
height fetches occur per vertex and two gradient plus one environment fetch per
visible fragment for the shipped profiles. The optional asynchronous GPU timer
uses a bounded reusable query pool; it reports unavailable when unsupported.

Host Chromium uses ANGLE SwiftShader and does not expose the disjoint GPU timer.
A forced-readback, full-water-coverage benchmark, after warmup, measures about
9.63–9.94 ms per pass at 960-by-540 and 18.25–18.59 ms at 585-by-1266. The earlier
per-fragment version measured about 20–21 ms and 29–31 ms respectively. These are
synchronized software-driver measurements, including their readback amortization,
not physical-phone GPU timings or a whole-game frame-rate guarantee. Ordinary
CPU command submission is usually below the browser timer's 0.1 ms resolution.
Hardware browser/mobile profiling and the original VU projection remain the next
optimization evidence gates.

Verification:

- All four original render profiles load their exact material, environment and
  derivative resources from the existing runtime manifest; no new game data is
  committed or added to the R2 package.
- An isolated GPU transform-feedback probe compares the production height-sampler
  shader to `LiveField::height` for 7,680 points total (1,920 per map) over 12
  prepared frames on each of the four maps, including negative coordinates and wrapping seams. All
  sampled values match exactly. This validates the sampled kernel, not the
  interpolated rendered mesh against native VU output.
- `tools/test_water_render.mjs` derives wet test positions from the original
  runtime terrain, rather than embedding a fabricated footprint. It verifies
  changed water pixels and unchanged banks/sky on all four maps, no water pass
  on ENV01, finite 390-pixel Arlington projection, one draw, and zero WebGL/JS
  errors. In the inspected host captures, the top ten sky rows remain unchanged.
- Core regressions cover original material selection/GS units, prepared-origin
  transport and raw-map deduplication, ordered derivative-channel packing,
  BumpScale, repeated-wave deduplication, and ignoring inactive WTT sections.

Native VU tessellation, reflection coordinates, froth compositing, wakes and
ATV spray particles remain unfinished. All four WTTs explicitly set WakeFroth
Skip=1 and WakeScale=0, but this does not establish every original particle or
froth behavior. Public surface deployment and physical-device performance are
not claimed by this host checkpoint. Probes, resource-derived fixtures,
benchmarks and screenshots remain ignored under `work/water-render/` and
`private/reports/water-render-*`.


## Deep-water failure checkpoint

The recovered ATV five-second reset clock is now connected in the authored port.
It uses strict origin Y below the preceding cached surface minus three feet,
decrements only while riding, restores five seconds in shallower water, and
consumes expiration before the following update's physics. Recovery uses the
retained dry checkpoint and resets rider/camera state. Detached riders retain
the existing water-contact/Swim/solid recovery route and original buoyancy.
There is no new ATV floating actor or invented head-submersion death.

The 144-case native clock comparison and all-four-map source-resource browser
water/rider gates are recorded in [water and snow verification](WATER_SNOW_GATE.md).
ENV04/05/07 routes drown and return to the dry bank; ENV08's shallower route
exercises wave/depth timer clearing. Public main `b9a3a44` / CI `38292520844` passes those routes and the separate
HTTPS/content gates. These remain bounded checks, not whole-game or hardware parity.


## Animated signature integration (host component gate, October 13)

NAT01 requires eleven original `nat01waves` frames at 4 fps, mixed 64/128-pixel
height/derivative maps and eight signed height channels. LiveField now loads every
original frame, runs the recovered 0030af00 clock and publishes the current
00198080 frame pair before the next origin preparation. Normal rendering follows
00197f10/00197980 with independently rounded GS FIX weights for both frames.

The GPU holds all deduplicated original frames in resident texture arrays.
Smaller derivative maps repeat within a larger layer with UV scale adjustment;
raw height queries retain their original integer dimension and byte samples.
Ordinary frames change uniforms only, with one surface draw and no mesh/texture
uploads. This is a browser storage optimization, not a recovered native atlas.

Physics retains the existing explicit shared blend-clock weight 1/0. Original
00192090 blends the sums of all first/second signed channels with 0018d760's
shared clock, independently of per-signature normal weights. The source of that
shared clock's water-node +ac frequency remains unresolved. Animated physics
therefore selects changing authored first frames, while normals interpolate;
this does not establish full NAT01 water-physics or native VU parity. Froth and
reflection-coordinate parity remain unfinished.

Host component checks pass the new frame-pair/previous-origin/wrap regression,
20 focused water regressions, strict workspace Clippy and WASM build. A private
original NAT01 resource fixture runs 45 prepared frames across all eleven source
frames. Actual production GPU height sampling matches Rust at 2,970 points
exactly, including negative coordinates, inclusive non-tiled edges and mixed
sizes. The isolated 390×844 surface changes 239,235 pixels between original
frames, retains identical held-frame pixels, costs 1,317,080 GPU bytes and one
draw, and uploads no textures or mesh buffers during 45 ordinary frames. These
are renderer work counts, not physical-phone timings. Existing four wet-map render gates and dry ENV01 pass, including portrait
Arlington, unchanged banks/sky and zero GL/browser errors. Full new-map driving,
pause/context and public verification belong to the integrating map checkpoint.
Source-derived fixture and GPU reports stay under ignored private storage.


## Water visual stability checkpoint (host component, October 13)

The initial projected mesh placed 334 subdivisions across the viewport and only
three into the distance. Native `00190aa0` instead scales water-node +250 strip
groups with viewport width/640 and +440 dense samples with viewport height/448.
The emitted row stride is strip groups times 19 plus one. This establishes the
orientation missing from the earlier constructor-count probe: the dense axis
belongs into the distance. It does not establish every VU vertex or native
triangle-strip boundary.

The portable renderer now starts with that orientation and sample-count basis,
then bounds resident geometry at 8,000 vertices in portrait and 12,000 in
landscape. At 585×1266 it uses 58 columns and 136 depth subdivisions, compared
with the previous 335 columns and three depth subdivisions. This avoids sparse
height interpolation across most of the visible lake while keeping the integer
height-fetch work bounded. Mesh buffers update only when the effective viewport
grid changes; ordinary frames still update uniforms and submit one draw.
A lost context or zero drawing-buffer size skips the draw before resize or
queries touch invalid GPU state. Restoration rebuilds the grid and textures
from retained original resources and preserves the completed shared field.

Original derivative and environment images now receive resident mip chains with
trilinear minification. This reduces undersampled normal/reflection detail during
camera movement without changing the raw height maps, signed channel ordering,
original GS normal weights, transparency, depth behavior, environment color,
physics clock or wave frame selection. It is a portable browser filtering
adaptation; exact native intermediate-map filtering and reflection coordinates
remain unverified. The R8UI physics-height array retains integer NEAREST fetches
at level zero. All mip generation happens during upload/restoration, never during
ordinary animation.

A fixed original NAT01 frame at 585×1266 compares the old and bounded new passes
under a 0.05-foot lateral camera movement. In the bottom 40% of the image, pixels
with summed RGB changes above 40 decrease from 26 to four; mean adjacent RGB
variation decreases from 0.993 to 0.841. This is a bounded minification/stability
probe, not a universal quality score or native output comparison. Serial warmed
SwiftShader timings including forced readback measure 28.55 ms before and 30.21
ms after; the rejected 24,000-vertex candidate measured about 53 ms. Physical
phone performance is not established by these software-driver measurements.
The final portrait NAT01 probe holds 7,946 vertices and 1,740,360 GPU bytes,
including every original animated frame, with one draw and no ordinary uploads.

Verification covers all six wet scenes (ENV04/05/07/08, FREE04, NAT01) and dry
ENV01. The real source-resource browser gate selects wet positions from runtime
terrain, waits for the loading overlay and initial fixed steps to complete,
checks changed water pixels, unchanged banks/sky and zero GL/browser errors, then
rotates every portrait view and restores its graphics context. Isolated original
resource passes assert zero ordinary mesh/texture/mipmap uploads. Chromium passes
all scenes. The Safari-family WebKit gate uses public HTTPS resources with only
the unpublished authored water module overridden in the host test because this
Mac's WebKit network process cannot reach the local HTTP server; this is host
engine verification, not a public deployment. The retained NAT01 production
shader matches Rust at all 2,970 original-resource points, uses every one of its
eleven frames, retains identical held-frame pixels and emits no ordinary uploads.
Source-derived fixtures, measurements and captures remain in ignored private
storage. Froth, native VU/reflection parity and the shared physics blend-frequency
provenance remain unresolved.
