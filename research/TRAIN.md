# Arlington Stream train — USA v2.00

The implementation follows the original ENV08 Freestyle procedural route and
named wheel motion. This core checkpoint provides authored Rust route time,
carriage poses and wheel-node poses. The host integration draws the original
runtime meshes. Status: live collision with all 11 train bodies, train
encounters and the three original train samples passed public verification on
2026-10-12 (see [TRAIN_CONTACT.md](TRAIN_CONTACT.md) and `DEPLOYMENT.md`);
locomotive smoke is connected as a host component (see
[Locomotive smoke](#locomotive-smoke)); complete original train behavior remains
unfinished. The
sections below are dated history. Original
resources, native exports, instruction probes and their output stay under ignored
`private/` and `work/`. No disc executable or emulator runs in the browser.

## Binding and implementation strategy

`train::definitions` reads the actual TOY procedural definitions in file order.
This lane binds the Arlington `locomotive.Vue` family: eleven bodies configured
for original collision and the separate noncolliding smoke attachment. Other procedural
toys are outside this component. The engine, tender, coal cars, flatbeds and mail
car use their original PSG resources; repeated models share GPU geometry. The
fifth-node `loco_wheels` model uses its separate named MOT. No synthetic carriage,
route, speed, collider, terrain offset or rail timeline is substituted.

The route clock advances once per fixed simulation step. Per-car lags use the
original integer reader, not floating-point authoring text. The shipped decimal
strings therefore become 15, 71, 120, 174, 227, 281, 334, 387, 439, 491 and 544.
The smoke attachment has zero lag. All twelve procedural definitions use FPS 2.
The authored route has 300 frames from 0 through 299 and a 149.5-second loop.

## Native route and body contracts

The ASCII VUE reader `0011c0b0` stores author translation XYZ as runtime XZY.
Its rotation rows are not the procedural train heading. `0011d8a0/0011de20`
interpolate positions with the original cubic weights and frame-spacing-scaled
neighbor tangents. The first and last segments retain their endpoint tangent
rules. `0011c5e0` estimates each segment length from ten substeps, including its
reciprocal-square-root then reciprocal operation order. `0011d5d0` walks those
lengths to find each lagged time, allowing cars to turn separately around bends.
Negative returned times are wrapped by the position sampler.

`0023be10` reads `LagDistance` with `001421b0/00141fc0`, whose numeric path is
`strtol(base 0)`. `PathIsLevel` and `DriveOnSoultrees` default true, while
`NumberOfTires` defaults zero. The procedural body's own tire-spin loop therefore
does not animate these train wheels: their wheel assembly is the independent
keyframed object.

The original procedural loader `0023d670` passes terrain pointer zero into
`0011b950`. The body uses authored VUE height, with a separate direct scene-object
height override. `0011c840` stores height on the shared route owner; level-path
followers use that owner's height. After the owner obtains the same height on
successive updates, its level-path height is cached. Treating every car as a
separate terrain follower changed the initial body matrices and was corrected
against the native replay. Horizontal displacement drives heading, with the
constructor's 0.5 history blend. The up vector and matrix update retain the native
owner/follower branch timing.

## Wheel motion

The wheel MOT is version 3 with named bones, linear translations and rotation
kind 0. Its eight-byte rotation keys hold time, an unsigned angle byte and three
signed axis bytes. The existing general MOT reader accepts only the twelve-byte
ShortAxis kind, so the train keeps this verified extension in `train_wheels.rs`.
`001cb7e0` constructs a 256-entry sine/cosine table; `001cbb10` multiplies the
signed axes by that table's sine scale without renormalizing them. The source
`001c8830` sampler uses the same quaternion blend kernel already reconstructed
for rider motion. The named channels compose the original five-node hierarchy.

Host sine, cosine and inverse-cosine remain the existing animation backend
limitation: their PS2-library bit parity is not established. The packed decoder
probe explicitly supplies table sine/cosine services; it is not a claim that the
whole PS2 transcendental library was executed.

## Evidence and verification

| Actual instruction probe | Scope | Instructions | Rust comparison |
| --- | --- | ---: | --- |
| `0011c5e0`, `0011d8a0`, `0011de20`, `0011d5d0` | 300 shipped route keys; 600 varied position and lag queries | 2,811,481 | All segment-length, position and lag-time bits match |
| `0011c840`, route/lag and model orientation helpers | 240 fixed frames × 12 route followers | 23,983,339 | All 46,080 compared body-matrix words match |
| `001cb7e0`, `001cbb10` | 1,000 varied packed keys | 168,499 | Native decoder observations with explicitly supplied sine/cosine table services |

The body replay supplies engine zero/up/forward constants, remainder, model
world propagation, direct scene collision as no-hit, scene actor updates and
base-actor callbacks. It executes the actual native body arithmetic and
orientation helpers; it does not establish full scene collision, audio callback
or startup-manager scheduling parity. Probes are a finite-binary32 MIPS/VU
interpreter, not an original-hardware run or a complete PS2 execution environment.

Private reproduction:

```sh
python3 work/train-probe/native_route.py
python3 work/train-probe/native_update.py
python3 work/train-probe/native_packed_rotation.py
/Users/nrgapple/.cargo/bin/cargo run --manifest-path work/train-probe/Cargo.toml --bin route --target-dir work/terrain-contact-target
/Users/nrgapple/.cargo/bin/cargo run --manifest-path work/train-probe/Cargo.toml --bin train-probe --target-dir work/terrain-contact-target
/Users/nrgapple/.cargo/bin/cargo test -p atv-core --lib train:: --target-dir work/terrain-contact-target
```

Reports are under `private/reports/train/`. Committed synthetic tests cover
coordinate conversion, cubic/loop behavior, integer truncation and defaults,
independent lagged turns, signed packed-axis handling and all truncated prefixes
of an authored eight-byte MOT fixture. None embeds original game content.

## Integration status and remaining work

WASM operation 67 installs the original train from the existing ENV08 archive;
68 returns its frame or performs a bounded diagnostic step. The regular batch
updates it before vehicle/camera operations and carries each intermediate train
snapshot. `web/train.js` draws the original body and wheel meshes using those
Rust poses. Source definitions and GPU resources survive graphics-context rebuilds;
route time pauses with the existing simulation lifecycle.

The desktop and 390px host gate executes the regular WASM batch and verifies
eleven configured train bodies, five wheel nodes, changing body/wheel poses,
pause/resume and graphics restoration. Train draws change 3,990 desktop and
1,710 portrait pixels in the tested view, with no page or WebGL errors. Original
route/MOT output supplies the test camera placement; no extracted map coordinates
are committed. The renderer submits 97 draws and 3,232 triangles for the twelve
procedural models and wheel assembly, sharing repeated model buffers.

```sh
ATV_TEST_URL=<host-or-public-url> PLAYWRIGHT_MODULE=<playwright-module> node tools/test_train.mjs
```

At this checkpoint this was host evidence, not a verified public release. The bridge then supplied no-hit for the
train's scene-height override; dynamic train collision response and spatial loop
sounds were later connected (see TRAIN_CONTACT.md), and locomotive smoke after
that (below). The smoke PSG is the small original emitter marker the smoke
attaches to. These gaps must remain explicit
through host/public verification; moving rendered cars and wheels alone do not
establish complete original train behavior or full handling parity.

## Locomotive smoke

Source: the Arlington Freestyle mode TOY declares one type0 emitter
`locosmoke.emt` attached to `Smoke.slt` (the zero-lag procedural follower) with
offset (0, 18, 0). `0019a680` copies the emitter position to its previous slot,
then takes the actor's world translation (`00165af0`) plus the offset rotated by
its basis (`001662f0`, X·x + Y·y, then + Z·z). The type0 update `0019c200` then
uses this step's displacement:

- distance = |current − previous|; speed = distance / dt. Below
  `MinimumEmissionSpeed` nothing is emitted.
- size = clamp(`InitialSizeToSpeedRatio`·speed, min, max initial size);
  life = clamp(`LifeToSpeedRatio`·speed, `MinimumLife`, `MaximumLife`);
  rate = clamp(`ParticlesToDistanceRatio`·distance + `ParticlesToSpeedRatio`·speed,
  `MinimumEmitRate`·dt, `MaximumEmitRate`·dt), then the shared fractional
  accumulator (which adds the carried phase twice below one particle) and
  max(`MinParticlesPerFrame`, count).
- Births start at the current position and step back by displacement / count,
  so a fast emitter leaves a trail along its path.
- Above 0.25 ft of travel the authored velocity is re-expressed in a travel
  frame by VU macro code: n = travel direction, side = normalize(up × n)
  (+Z replaces up when |n.x| and |n.z| are both below 1/64), up' = normalize(n ×
  side), velocity = (Vx·side + Vy·up') + Vz·n. Particles never inherit the
  train's velocity, so smoke stays where it was born while the train moves on.
- `EmitMode=3` alternates 0.25 s on and 0.25 s off, giving separate puffs.

At the route's ~44 ft/s the source file yields size 1 (growing 4 ft/s), life
5,000 ms with the standard 0.5/1.0 jitter, and the minimum rate, so roughly 50
smoke sprites are alive. The rest of the path (lifetime, growth, fade, lighting,
atlas group 9, depth-sorted instanced draw) is the existing ambient CPU sprite
path in [AMBIENT_EFFECTS.md](AMBIENT_EFFECTS.md); no new runtime content,
texture or draw family is needed. Smoke shares the ambient alpha-blend instanced
draw, so it adds at most one draw call (when no other ambient sprite is in view).

Integration. The ambient planner now turns an attachment with no static model
into an actor placement. It stays dormant until a pose arrives; there is no
phantom static emitter. Each fixed step (ATV and motorcycle batches) passes the
train frame's car matrices to `Effects::set_actor_poses` before the particle step.
Multiplayer keeps the shared train clock unchanged. When synchronization jumps
the route clock, `relocate_actors` treats the next pose like a freshly placed
emitter, so smoke is not sprayed along the skipped stretch of route (port
adaptation). Pause stops births; vehicle reset keeps the scene clock and smoke.

Capacity. The native scene CPU manager is created by `00209790` through
`001435f0` with 1,500 shared particles. The port's static budget remains 600, and
on Arlington the waterfalls alone come close to filling it. Moving-actor emitters
therefore get a separate bounded reserve of 128 slots (frame capacity 728 only
when an actor emitter exists). This keeps the total below the native pool and
leaves other maps unchanged.

Evidence. A private finite interpreter runs the actual `0019c200` instructions
for 400 moving cases (256,910 EE/VU-macro instructions, 778 births). These
include static, sub-0.25 ft, near-vertical, horizontal and minimum-per-frame
cases. Rust matches all 10,514 compared words: position, velocity, life, size,
growth, frame, flags, age and the output phase. The existing static type0/2/8
(7,272 and 6,216) and timer (2,000) gates still match. The timer/attachment
gate is supplied, and the original LCG is a seeded service. Not covered: the
zone-visibility test, under which `0019c200` emits only when the attached
model's zone was reached by this frame's portal walk (`001a2a80`). The port
has no zone visibility, so it emits whenever the timer allows. Also not
covered: native update order between the train actor and the particle manager,
and shared global RNG order.

Host checks: Rust unit tests cover dormancy without a pose, the dirty first
step, speed-scaled size/life, the minimum-rate count, the side-axis velocity
and relocation. `tools/test_train_smoke.mjs` (content group) runs at desktop and
390×844/DPR3 and checks the binding, ~44.7 ft/s emitter travel, ≥10 trailing
sprites, the instanced draw, reset continuity, pause freezing births, and zero
GL/page errors.
Phone emulation (390×844/DPR3, ×4 CPU throttle, smoke in view, two alternating
rounds): render CPU 1.94/1.43 ms before vs 1.55/1.44 ms after; worker batch
0.74/0.67 ms vs 0.63/0.71 ms; 199 vs 200 draws. That is host noise, not a
physical-phone result.
