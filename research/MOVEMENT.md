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
