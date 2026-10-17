# Original weather selection and browser cost

Weather is selected from the actual common and mode TOY documents. The initial
147-SCN census found no `[Weather]` sections; that result alone does not establish
inactive weather. A broader SCN/TOY census finds 17 Weather declarations across
the source archives. Legacy SCN `[Rain]`/`ChanceOfRain` declarations are not read
by the recovered Weather loader at 00208cc0.

| Venue/mode resources | Original Weather |
| --- | --- |
| Freeport/ENV04 common TOY | Rain, chance 1, `sx13rain.slt` |
| NAT06 common TOY | Rain, chance 0.9, `nat06rain.slt` |
| SX13 common TOY | Rain, chance 0.8, `sx13rain.slt` |
| Caribou/ENV01 National, Enduro large/small, King of the Hill, Tag mode TOYs | Snow, chance 1, four camera-relative rings |
| Caribou/ENV01 Freestyle TOY | No Weather declaration; snowy terrain is not evidence of falling snow |

All other inspected shipped SCN/common/mode TOYs omit Weather. These are source
resource declarations, not observations from a PS2 framebuffer.

00209dd0 selects Weather once during level activation. Its strict dry condition
is `chance < binary32(rand) * 2^-31`; equality selects Weather, including the
very rare zero-RNG endpoint at chance zero. The dry branch removes matching SLT
models. 00214d20 changes chance to zero when native player count exceeds one.
The browser supplies one per-load RNG sample and one local view, including
online rides; remote riders and race CPUs do not change that input. The original
field's mapping between local human players, views and online participants remains
unverified, so lobby size is not used as a substitute. Independent browser RNG,
native game-wide random ordering and original multiplayer event scheduling remain
unrecovered. ATV reset and GL restoration retain the selected scene.

The three rain declarations have no Weather particle-emitter list or snow-ring
flag, and their SLT name is not `screenrain`. Their wet branch therefore retains
the original placed rain geometry and assigns the Rain type; it does not create
a camera-relative WeatherParticleEmitter or ScreenRain object. Their original
prelit cards scroll V at 2. Freeport also places a Type 8 raindrops emitter and
Type 12 fog emitter independently of Weather. The ambient-particle component
owns the placed Type 8 effect.

The active Snow branch generates 4/8/16/32 emitters (60 total), at the source
ring heights/radii, enables camera-facing hemispheres and moves enabled emitters
relative to the active camera (00209dd0/00209310). The Rust SnowEmitter runtime
now creates those rings from the selected mode
TOY and original `snow.emt`, uses the camera-facing test, and emits the source
ceil(max(minimum, rate*dt)) birth count at each fixed simulation step. A single
WebGL instanced draw uses the original GPU particle texture and the previously
recovered mode2 velocity/alpha contract. Snow clears gravity, uses minimum Y
−10,000, source color/alpha and lifetime jitter, source X/Z velocity spread,
mirroring and source half-atlas selection. ScreenRain lens/world drops are
dormant original code that no shipped resource selects; see
[ScreenRain activation](#screenrain-lens-and-world-drops-dormant-in-shipped-content).
Rain audio initialization
at 00213bf0 requests `rain.vag` with looping flag 0x81 and sets volume 100 at
002c9cc0. The browser now plays that source sample as one looping voice through
the engine gain, including the motorcycle path, which loads the ATV audio bank
only when rain is selected. Pause mutes the voice; vehicle reset preserves it;
map replacement/page departure closes it. No generic storm was added to dry maps.

## Host component gate

The authored Rust policy and WASM operation 120 read common/mode TOYs, preserve
first-key selection, choose rain once and remove dry-choice geometry before GPU
construction. Two synthetic regressions cover original defaults, equality,
legacy Rain exclusion and multiplayer suppression. An ignored finite interpreter
executes 35 actual 00209dd0 selections (2,072 EE instructions) with controlled RNG
and string services. Active/dry decisions and binary32 samples match Rust exactly.
This covers the selection branch, not whole engine callbacks or hardware output.

All three shipped rain materials use alpha reference 120 and FB_ONLY failure.
Batching every accepted fragment before every failed fragment could change
ordering when cards overlap. The browser instead retains its previous per-card
accepted/failed draw order and caches identical material uniforms, texture and
VAO bindings across consecutive draws. Each placement still uses its source
matrix, colors, texture, scrolling, fog, alpha predicate and depth behavior.
No extra particles, full-screen postprocessing, visibility reduction, new buffer,
texture or per-frame texture/mesh upload is introduced by this optimization.
The previous browser order is preserved; native PS2 transparency queue parity is
not established.

The deterministic original-resource inspection gate compares the prior authored
renderer with this renderer in desktop and 390×844 portrait. Its test-only start
is derived from an original rain placement near the source field center, with
ordinary physics and a fixed 90-step clock; it is not a normal spawn/hardware
capture. Desktop has 30 visible rain placements/60 original draws, with one
material update replacing 30; portrait has 27/54, with one update replacing 27.
All 2,073,600 desktop framebuffer bytes and 1,316,640 portrait bytes match exactly.
This is identical visual output with 29/26 fewer complete material updates per
frame, not a measured physical-phone FPS gain. Test hooks and source-derived
fixtures stay private; `tools/test_weather.mjs` is an opt-in host/public gate.

Weather selection, rain rendering optimization, ambient placed emitters,
Snow and complete PS2 weather fidelity remain separate gates.

Seven actual-resource browser cases cover rain/dry selection in all three rain
venues, dry Caribou Freestyle, portrait ATV and motorcycle rain, pause/resume,
reset, context restoration and departure. Rain retains one decoded voice and
never restarts on reset. The original 19,500Hz sample decodes to 731,684 bytes
of host 48kHz PCM. The incremental 147-record runtime package preserves all
146 non-audio archives and adds only `rain.vag` to AUDIO.RES.

The lifecycle gate also exposed an existing synchronous reset race: rendering
could read a cleared interpolation frame after queuing the reset. Rendering
now finishes that frame before touching the cleared pose. The scenery material
gate uses a source-derived CPU start so CPU frustum selection and inspection
camera agree; the previous camera-only hook failed on both prior and new builds.
Public verification is pending integration and release.


## Snow component and mobile work budget

Caribou Race selects 60 ring emitters at radii 50/115/180/245 feet, height 65,
with 4/8/16/32 divisions. Caribou Freestyle remains dry. Native 00369120 returns
1 at zero angle (cosine); portable host trig still differs from original lookup
and rounding at some boundaries. Camera position/target supply the portable
forward direction; presentation-camera adjustments are not recovered native
camera state. Source `001a1ba0` birth, `001d1910` packet quantization and `001d2280`
mode2 packet submission correlate the connected path. A private interpreter
executes 100 controlled birth cases (24,460 actual EE/VU macro instructions).
Rust position, quantized velocity, mirror and half selection match those cases
with zero differences. The fixture supplies the original random-table inputs;
it does not establish the native global RNG initialization/order.

The portable Snow manager uses 10,800 resident slots, matching the already
supported wheel GPU allocation extent as a bounded backend choice; the original
Snow scene's shared-bank capacity and overwrite order remain unproven. The
current camera keeps roughly 27 emitters active and the source 7-second maximum
jittered lifetime produces around 5,500 live flakes, within that budget. No density
or lifetime reduction is applied on mobile. Its buffers and 128×64 source texture
use 508,016 GPU bytes. Births upload 44 bytes each, coalesced into adjacent slot
ranges. GLSL moves/fades flakes on the Rust fixed clock, so ordinary render-only
frames have no particle updates or uploads. Live birth history reconstructs
slots after context loss; map replacement configures a fresh manager. This
reduces CPU work and driver submission, not a measured physical-phone FPS claim.

Snow uses an independent source-algorithm 600-entry random table with a fixed
portable seed. The original shared table/cursor and its interaction with other
emitters remain unconnected. Portable billboard projection, shared GPU packet
batch lifetime/UV ordering, PS2 render queue/GS rounding and hardware parity are
still limits. Freeport's persistent placed type 12 fog is connected; see
[ambient fog implementation and remaining limits](AMBIENT_EFFECTS.md). ScreenRain
is never selected by shipped content (next section). Ambient's
placed type 8 raindrops are a separate source effect, documented in the same
component report.


The native Snow birth timestamp is zero; `001d1910` encodes bank clock minus
that timestamp into packet position W. Mode2 at `003b9b90` subtracts packet W
from current global clock, producing time since packet creation for these Snow
births. The portable per-birth timestamp implements that connected interpretation;
shared-bank update order, callback cadence and binary32 millisecond accumulation
remain gaps. This component advances at the port's fixed 60Hz scene clock and
makes no claim about complete original frame scheduling.

Desktop and 390×844 portrait actual-source gates isolate the production Snow
pass and hold its camera uniforms/physics results to separate it from the
settling presentation camera. Removing that pass confirms visible contributions
from its recovered source texture. All frozen Snow framebuffer bytes restore exactly after
context loss, with zero uploads on subsequent render-only frames and zero GL or
page errors. Real pause stops births and scene time. Physical vehicle reset op23
preserves Snow time/emission IDs; existing Race Backspace performs a full race
reload/map replacement, which intentionally starts a fresh manager.

## ScreenRain lens and world drops: dormant in shipped content

The executable contains a complete ScreenRain object (`ScreenRain.cpp`), but
the shipped USA v2.00 game never creates it. The browser therefore draws no lens
droplets on any map, including the three rain venues, in every graphics preset.

Activation. `00209dd0` takes the ScreenRain branch only when the selected
Weather has no `NumParticleEmitters` list, `UseRingEmitters` is off, and
`SltName` equals `screenrain` (case-insensitive compare `0036f6b0`). It then
allocates the object (`00219300`, the only call site) and configures it
(`00219650`, the only call site) with `MaxLensDrops` (default 20),
`MaxWorldDrops` (default 100), `Color`, `ColorOffsetStart` and
`ColorOffsetEnd`, all read by the Weather loader `00208cc0`. Freeport, NAT06 and
SX13 name `sx13rain.slt`/`nat06rain.slt`, so they keep the placed rain cards and
take the plain Rain branch. A byte census of all 742 disc files and every member
of every archive (all parse and extract) finds `screenrain` only in the
executable; no TOY, SCN or other resource declares it. `MaxLensDrops`/
`MaxWorldDrops` also appear in the legacy SCN `[Rain]` blocks (ENV04/05/07, for
example `ChanceOfRain=0.1`, `MaxLensDrops=10`), but the executable contains no
`ChanceOfRain` key at all; those blocks are never read.

Textures. `00219650` asks for `particle\drip`, `drop`, `ring` and `splat`
(16×16). None exists in any archive. The fallback `00219b50` would build a
procedural radial map (alpha `(1 − r/8)·256`, inner highlight 8 below radius 5,
outer shade 246). Each lens drop is a 16×16 copy of the framebuffer under it,
brightened and darkened by those maps (`00219100`, saturating byte add/subtract).

Motion (`00219da0`, 640×448 screen space). The live count grows by one per
update up to `MaxLensDrops`. Each drop drifts outward from (320, 224) by a random
fraction of half its size, and down 4 px per update. Its life decreases by
dt × 1 and its alpha by dt × 2. A drop leaving the screen, expiring or reaching
zero alpha respawns at a random position, or (half the time) next to the
following drop, with alpha 255. World drops (`0021a420`) are screen-space
streaks driven by camera displacement (×0.2 and ×0.1 terms).

Port decision. Rendering these would add behavior the shipped game never shows,
so nothing is drawn. Rust recovers the activation rule only:
`weather::Definition::selects_screen_rain` and the `screen_rain` field of the
selection (false on every shipped map). The Rust test covers the exact name, the
emitter-list exclusion, multiplayer chance zero and the unread legacy `[Rain]`
block. `tools/test_weather_lifecycle.mjs` asserts `screen_rain` is false for every
real rain case. A lens-drop overlay would be an authored enhancement for a
non-Original preset and needs an explicit product decision. Cost is zero: no
shader, texture, buffer or per-frame work exists for it. Freeport rain at
390×844/DPR3 with ×4 CPU throttling measured the same before and after
(render CPU 0.8–1.2 ms vs 1.2–1.7 ms, worker batch 0.46–0.49 ms vs 0.44–0.53 ms,
96 draws both; host-emulation noise, not a physical-phone result).
