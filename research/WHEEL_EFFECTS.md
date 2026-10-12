# Original wheel surface effects

Strategy: recover the USA v2.00 tire-effect selectors, emitter assets, CPU particle
updates and GPU birth packets, then add authored Rust state with batched WebGL
billboards. Vehicle physics remains authoritative. Visual emission reads final
wheel model world translations, source terrain surface bytes/heights, wheel
circumferential speed, applied throttle, tire steering, water height and successful
MUD category callbacks. Original runtime resources stay in the authorized R2
package; all eight hosted FR archives already contain the required particle banks
and ATV01 contains the three EMT definitions. No game content is embedded in Git.

Rear dust, dirt chunks/spray and all four wheels' water/mud roost are implemented.
`001d71b0` dispatches rear/front effects; `00165af0` returns the actual model world
translation, rather than its bounds center. Surface-specific groups come from
original PRT atlases. Original EMT files supply base emitter parameters, while
native constructors and riding setters override flags, velocity and lifetime.
The original CPU bank decodes to 128×512 in all eight maps. The original GPU bank
is 128×64 in ENV01 and128×256 elsewhere; its normalized UV table is shared.

`001d7620` allows dry dust when wheel center minus terrain height is at most 1.3ft
and shared circumferential speed exceeds 0.5ft/s. `001e5260` derives that speed from
the first wheel's angular speed times radius. Dirt chunks/spray require positive
throttle, body speed above 45ft/s, and either shared wheel speed below 10ft/s or
absolute tire steering above 0.2rad. Body speed clamps to 115 before setting spray
velocity/lifetime. There is no invented lateral-slip threshold or constant dust.
`001d9840`, `001da200` and `001da630` select the eight original surface groups.

`001d99f0` emits distance-based dust, using the original lifetime/size clamps,
source sine polynomial, staggered ages and atlas frame progression. `0019b9e0`
emits chunks/spray with original constructor size/velocity tables and separate
indices. Chunk direction jitter is selected once per rear emitter/frame. Spray
starts at the chunk lifetime and lives twice that long. The original velocity
index 32 aliases following size/index words; this observed behavior is preserved.
`00143aa0` updates age, growth, position and gravity. Dirt chunks settle on the
actual terrain after 250ms and change flags, while retaining gravity on that
settling step. Original GS packet alpha is recovered from `001451a0`.

`001ebb30` records MUD category mask 8 and the successful ray hit height separately
from physical support. Root's scene integration classifies the loaded original
SLT/COL name prefix using the raw environment name. A private all-eight-map census
finds 170 actual MUD placements: ENV02=44, ENV04=39, ENV05=39, ENV06=22, ENV07=16,
ENV08=10; ENV01/03 have none. The census retains real up-facing triangle witnesses
and terrain samples. No fabricated mud region or color is used.

Water/mud selects wet roost and suppresses dry effects. Native wheel GPU emitters
require shared wheel speed above 7ft/s. Front sampling lowers the model center by
exactly 1ft (`001d7ea0`). Water uses tiles 0–7, size 0.5, radial spread 6 and fan step 5;
mud uses tiles 8–15, size 0.75, spread 1 and fan step 10. `001db4a0` supplies the radial
fan, correlated jitter, original 800ms lifetime and birth age. Velocity and size
are quantized as in `001d1910`. The source ParticlePhysics mode 2 VU program mapped
by `001ad460` to `003b9740` applies position+velocity*time−16.1*time² in Y, its
minimum plane and packet alpha. Authored GLSL adapts that contract to WebGL.
Original wet color uses ambient+0.5*directional scene light.

The scene fixed clock advances exactly once per successful physics step.
Repeated frame reads do not emit or age particles again. ATV reset preserves
existing world particles, clears emitter path history and restores source+48c
suppression until all four wheels contact terrain. Map replacement starts a fresh
particle manager. Pause and context restoration use this same scene clock.

The portable backend uses at most two instanced draws: up to 600 CPU particles and
the original GPU ring capacity 900×12. Wet particles upload only new birth packets;
GLSL ages and moves them on the authoritative clock. Source birth history restores
live GPU slots after context loss. ENV04's complete effect textures/buffers consume
1,031,664 bytes. Host SwiftShader renderer submission measurements were 0–0.1ms;
these are not phone GPU or full-frame performance measurements.

Evidence: a selector probe executes 248,194 original instructions across 1,000
supplied transform/terrain/water cases; authored code matches 3,425 compared flags
and binary32 words. A `00143aa0` probe executes 159,834 instructions across 1,000
age/growth/velocity/gravity/terrain-settling cases, matching 10,603 flags/words.
A `001db4a0` birth probe executes 379,220 instructions across 200 water/mud cases,
matching 21,600 words with supplied constructor tables, heading and services.
These comparisons establish bounded kernels, not full gameplay replay parity.
Private decoded-resource census validates all eight original PRT/EMT/CPU/GPU banks.
Seven authored tests cover selectors, atlas validation, spray birth age, settling,
clock idempotence, relocation retention and separate water/mud tile selection.

Host gates `tools/test_wheel_effects.mjs` and `tools/test_wheel_wet.mjs` exercise real
runtime resources at desktop and 390px: normal riding dust and turning dirt/spray
change actual framebuffer pixels; original water roost changes pixels at a runtime
terrain-derived shallow-water placement. Pause/resume and GL context restoration
pass without JS/GL errors. The optional private MUD fixture is derived from actual
SCN/COL terrain witnesses. Its integration gate must pass before mud is described
as publicly verified. Public release verification is coordinated with the root
release agent and recorded in the public devlog.

Remaining fidelity work: whole-game shared RNG call ordering, native acos/table
heading, GS depth sorting and exact texture/color sampling, GPU/VU hardware word
parity, full emitter scene-visibility semantics, original body water/exhaust and
body/tire dirt texture animation. TerrainZone GenerateDust/GenerateDirt fields are
preserved by the terrain parser, but their actual consumer has not been established
in this wheel selector; no invented suppression rule is added. This is a partial
source reconstruction and portable graphics backend, not a completed decompilation.
