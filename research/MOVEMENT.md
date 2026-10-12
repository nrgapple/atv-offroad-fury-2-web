# Coupled movement audit

The browser reconstruction previously let chassis rays collide with props while
all four wheel supports sampled terrain only. Exact-build `001ebd90` instead
queries terrain and a finite suspension ray against obstacle geometry. This
omission made a recovered prop ramp or bridge support the chassis without
supporting its wheels. It is a concrete integration defect; correcting it does
not establish complete original handling parity.

## Recovered wheel support contract

`001ec200` sets the wheel's query direction to local down `(0,-1,0)`.
`001ebd30` sets the query offset, zero in the `001e1fd0` initialization path.
`001ebd90` transforms the uncompressed wheel center by the body matrix, samples
terrain there, and casts a body-oriented segment from suspension travel above
the center to wheel radius below it. This is a finite ray, not an arbitrary
vertical drop to terrain.

An ordinary prop hit replaces the wheel's stored plane point and normal and
switches terrain-source `+274` to prop-source `+278`. It retains the material
byte `+228` from the terrain query. Each object query resets nearest-face state;
the final successful eligible static object in scene order supplies the plane.
The special rider-category mask in callback `001ebb30` suppresses ordinary prop
overrides; moving-body categories populate `+27c`. The authored static helper
accepts the already-selected static prop list. Dynamic bodies, native category
registration and original spatial-grid object order remain separate gaps.

`wheel_support::sample` performs the outer query. The suspension reprojects each
moving wheel against that frozen plane during body substeps. `001ed330` clears
support-source availability after losing contact, preventing reacquisition until
the following outer query. Plane normals and material persist across that loss.

## Cadence and wiring

`001dd830` first snapshots previous wheel grounded flags, the first wheel's
material and the average of all four stored normals for grip/rider controllers.
It then updates body metadata, queries four wheel supports, runs the drivetrain
once and runs wheel rotation once. Body substeps use the controller snapshots
and the new frozen support planes. Wheel rotation projects body point velocity
at the previous suspension contact point (`wheel+40`) onto the fresh contact
frame. It reads the stored normal even when the previous grounded flag is false.

Two speed fields have separate roles: body `+34` stores total world speed, while
body `+288` stores local forward speed. The outer update supplies total speed
with the local-forward sign to engine `+64`; material drag consumes total speed.
Steering and rider gates use the local-forward field. These wiring differences
cannot be detected by isolated engine or drag kernel probes.

Wheel axle directions now come from the previously completed mechanical pose,
including camber and unscaled visual steering. In ordinary local riding,
`001e5260` updates controls and physics before `001d65c0` updates mechanical pose;
the next `001ecc40` transforms that stored axle by the current body matrix.
The speed-reduced angle drives lateral grip separately. Initial axes use the
loaded bind nodes. Reset `001dd440` clears wheel scalar state and moves the body
but preserves wheel local rotations until the following mechanical update.
The core vehicle caches the pose matrices and axle directions together; the
renderer reads that same completed pose.

The reverse speed gate consumes grip-controller `+88`, the cached rear virtual
contact point's local forward velocity. `001d8540` constructs this directly as
local linear velocity plus local angular velocity crossed with the lever from
center of mass to `(0,0,CofGZ-3)`, with moving-platform correction when applicable.
It updates only while any wheel contacts and otherwise persists. This can differ
from body local forward speed. The ordinary controller writes a forward request
before testing the reverse combination, then consumes the brake and selects
reverse only below the rider's strict stand-up-speed threshold. Independent
brakes remain separate from the consumable Cross+Square combination.

## Verification and limits

- 100 actual `001ebd90` instruction executions, 24,769 instructions, with terrain
  and scene services hooked: all 600 generated ray XYZ components bit-match the
  Rust helper. The wrapper probes also confirm terrain/prop precedence, retained
  materials and blocker behavior.
- 512 actual `001ebb30` executions confirm the callback category-mask branches,
  moving-body assignment and blocker flag. Mask values are fixture parameters;
  runtime scene registration is not reconstructed by this probe.
- The reused static segment helper passes independent exact-instruction probes
  for 300 transformed objects and 3,900 oblique rays, plus 4,000 four-ray box
  cases covering 16,000 rays. These validate the supported static, flat-normal
  query path; smooth normals and scene selection remain incomplete.
- 500 actual rear-contact velocity branches, 57,500 instructions: all 1,500
  XYZ results bit-match the direct local-space Rust helper.
- 800 actual `001ecc40` wheel-frame branches, 59,200 instructions, using axle
  directions from 200 real ATV01 linkage poses: all 4,800 right/forward XYZ
  components bit-match Rust. Linkage host trig and scene order remain separate
  numerical/runtime limits.
- A synthetic coupled vehicle regression settles on a prop fifty feet above
  terrain with four-wheel support and no chassis contacts, then falls after
  removing the prop. This catches terrain-only support and stale plane reuse.
- A coupled Rust replay loads the actual ATV01 mesh, ATV01S01 physics and original
  `e01ramp.col`, places the prop well above ENV01 terrain, and advances 240 frames.
  With the recovered linkage connected, after seven initial settling frames,
  all four wheels remain supported for 233 frames; final origin is `[500,10013.577,512.0007]`. This proves wheels can
  support the authored prop with the integrated Rust solver. It is not a replay
  against the original game.

Private reproducible probes and source-derived results stay under
`work/physics/` and `private/reports/`. No original bytes, instruction exports or
source-derived asset traces belong in GitHub/CI.

## Terrain suspension travel stop

The clipping investigation identified a second, independent terrain-contact actor
that was missing from the coupled vehicle. `001dcc70` creates solver `vehicle+184`
with four points and enables only its `+78` response. It disables impulse,
penetration correction and friction (`+58`, `+4c`, `+50` are zero). `001e1fd0`
constructs each point from the wheel's **local** rest translation plus body-up
multiplied by `(travel-radius)`, then adds the model body's local translation.
These are fully compressed tire-bottom points, in front-left, front-right,
rear-left, rear-right order. The registration margin is zero.

Query actor `+188` captures a terrain plane at each point once per outer frame
through `00193b40`/`0016ac10`. Its constructor's two obstacle-query arguments are
zero, so this actor has no prop collider or spoke query. Its configured
`[0,2.5,0]` offset is unused in this terrain-only path. It remains enabled when
upright; the chassis `+17c` terrain gate does not control it. Original setup and
phase-dispatch instruction probes also establish that riding, forced landing,
and wreck phases retain this terrain stop.

During every physics substep, after grip, engine, and rider force accumulation,
`001dd830` runs this solver **before** all four wheel springs. `00194930`
retransforms each local stop point against the frozen plane and tests strictly
positive normal penetration. The selected response, `00196cf0`, removes only
inward **body linear** velocity and force along that normal, records the velocity
change in body `+180`, and leaves angular velocity and position unchanged. It
does not use the generic contact-point velocity, reflection, positional
correction, or friction kernels. This is source behavior, not a ride-height
clamp. Fast crossings and angular motion still require coupled verification.

`wheel_stop` implements this separate actor. Original-instruction comparison
executes 600 complete four-point `00194930` stop solvers, including tilted
transforms and varied slopes: 372,418 instructions, all 2,400 contact decisions
and 5,400 velocity/force/impact components match Rust binary32 values. The
unused generic point-velocity helper is hooked at its service boundary.
Another 500 original constructor blocks execute 108,500 instructions and match
all 2,000 stop points (6,000 components), including the registration margin,
flags, and model binding. Private artifacts remain under `work/` and
`private/reports/`; these tests do not establish a complete vehicle replay.

## Outer frame timing remains a scheduling approximation

Original `00105010` averages ten integer-millisecond frame deltas, resets a raw
gap above0.1 seconds to0.033, and caps dispatched actor time at0.06 seconds.
An optional debug setting caps it at0.03333. The root actor runs once per main
render loop. Default display synchronization waits for one vertical event when
none has elapsed; the debug SYNC30HZ command selects two. This does not establish
a measured fixed30Hz or60Hz gameplay cadence. The browser's deterministic60Hz
scheduler remains an approximation and was not changed based on a frame-rate
guess. A private166-iteration timer probe executes37,529 original instructions.

The suspension clipping fix is independent of that timing audit: it restores a
missing original actor. Measured original-mesh tire vertices against the full
terrain samples in the browser landing trace improved from -1.9423ft to
-0.48358ft minimum clearance. The earlier clean-landing test only established
no false wreck; it did not establish acceptable tire clearance. The restored
stop leaves shallow overlap and does not establish complete original landing
or slope-turn parity.

## Full wheel spring replay and pitch attenuation

The coupled landing investigation now executes the complete original wheel
spring routine `001ed330`, including Model point/vector transforms, contact-point
velocity and body force/torque accumulation. This exposed an incorrect axis in
our spring attenuation gate. Native `00165330` copies the Model forward column
into the first output and up into the second. The spring routine tests the first
output's vertical component: attenuation applies only when
`abs(body_forward.Y) < 0.5`. The former Rust right-axis test could suppress a
pitched spring that the original retains, or retain a rolled spring that the
original attenuates. `SuspensionContact::body_forward` now names and supplies the
correct axis. A regression distinguishes sixty-degree pitch from roll and retains
the original strict comparison at the thirty-degree pitch boundary.

A private instruction replay covers 600 finite pitch/roll contact cases and
374,963 original instructions. With the old axis, 170 force components differ,
with a maximum absolute difference of 1,338.308 in source force units. With the
forward-axis gate, all contact decisions and compared compression, contact-point,
point-velocity, spring-magnitude and force components match binary32 bits. The
original acos lookup service is hooked with finite host acos; both paths receive
that same supplied angle. The fixture uses a fixed flat plane, varied body
orientations and linear/angular velocities, and no moving ground. It establishes
the wheel routine and gate, not original table initialization or a complete quad
frame. Private reproducible files are `work/physics/suspension_full_oracle.py`
and `work/suspension-full-probe/`; generated reports stay under `private/reports/`.

A separate 240-frame replay executes the full original `001469e0` body update,
its `0013c010` integrator and Model setters (292,320 instructions), with constant
synthetic falling/rotating forces and an identity scene parent. Rust linear and
angular velocities match all frames. Model-to-quaternion rebinding introduces
small pose differences: at most 0.00000763 feet in center and 0.000000417 in
quaternion components. That bounded body result does not explain the observed
shallow rendered tire overlap. Original full-quad orchestration, initial bindings,
terrain sampling and display-time mesh linkage remain to be compared together.

## Center-of-mass cache before the first wheel substep

The controller `001e7940` changes body-local CoG `+1a0`; body virtual `+9c`
(`001460e0`) only clears the contact count. `001469e0` refreshes world CoG `+60`
from the Model at the beginning of integration. Consequently wheel rotation
`001ecc40` and wheel spring `001ed330` point velocity use the preceding cached
world center before the first body integration of this outer frame.

The port refreshed the world center immediately and used it for those velocities.
It now retains the preceding center explicitly for outer wheel rotation and the
first spring substep. Geometry still uses the unchanged Model origin, and force
torque uses the newly selected local CoG. Subsequent substeps use the integrated
world center. This distinction matters: delaying every use of the center would
be incorrect. Chassis solver `00194930` explicitly reconstructs its **own**
world CoG `+130` from Model/new local CoG before velocity and impulse response;
its existing contact center is retained.

A 600-case full `001ed330` replay with changed local CoG and retained world CoG
executes 386,315 instructions. The retained-center point velocity and spring
force match all compared binary32 values. The former immediate-center input
produces 1,813 differing velocity/force components, with maximum difference
105.493774 in these synthetic source units. A coupled Vehicle regression checks
nonzero angular velocity plus rider CoG shift while preserving projected Model
points. Seventeen Vehicle tests and strict core Clippy pass. Private probes are
`work/physics/suspension_cog_cadence_oracle.py` and the `cog` binary under
`work/suspension-full-probe/`; reports remain private. This proves a handling
cadence correction, not that it explains or eliminates the remaining measured
landing/slope tire overlap. The detached rider's animated CoG cache requires its
own caller audit before applying this behavior there.

A stateful mechanical pose replay also retains preceding node orientations across
300 `001d65c0` updates while resetting the full wheel rest XYZ exactly as
`001edcd0` does. It executes 1,436,415 original instructions. All recovered Rust
pose matrices remain within 0.000009775162 of the native replay; the largest
component difference is rear-wheel Z. Scene matrix cache services are hooks and
host trig/VU precision remain numerical limits. This probe does not demonstrate
a missing linkage update or explain the residual landing overlap.
