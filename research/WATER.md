# Water contact reconstruction

The authored Rust `water` module reproduces the USA v2.00 buoyancy force kernel
at `0033dd90` and its spring configuration at `0033da90`. It is a verified
building block; water contact is not yet connected to live driving or wrecks.

The original selects one complete set of contact points based on speed, samples
water height at each world point, caps penetration depth, and applies a spring
and velocity damping force. Angular point velocity contributes to both force and
ripple strength. Translational drag is applied once when any point is submerged.
The rider's tuning comes from its constructor at `001ee090`; ATV tuning remains
separate. Callers supply the original displaced surface heights and point sets.

A private instruction interpreter executed 300 varied configurations and contact
sets through the actual source routines: 192,299 instructions, with bit-identical
spring coefficients, force, torque, selected points and ripple strengths. The
probe supplies identity point transforms and a controlled height callback. It
covers angular velocity, water flow, depth limits, dry contacts, and high/low-speed
point selection. Synthetic regression tests preserve an original force/torque
checkpoint and the strict speed threshold.

Live integration must recover the dynamic surface and its initialization. The
original `00199cd0` height query combines a ripple grid, authored signature waves,
and the environment's base height; using only a flat base plane would change
contacts. Ripple updates, world flow/up initialization, actor point bindings and
water rendering remain incomplete. No complete water-physics or rendering parity
is claimed by this isolated kernel.


## Surface height checkpoint

`water_surface` now reproduces the composed height query at `00199cd0` and
signature texture sampling at `00192090`. Live water remains disconnected.
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
not an invented full-cycle duration. Source sine-table backend, wave initialization,
scroll/frame update, ripple propagation, runtime resource delivery and rendering
remain separate integration work. A flat-plane fallback is not a parity implementation.

Rider update `001ef690` runs gravity, water response, then solid contacts during
its at-most-1/90-second substeps. Either water touching or solid contacts activates
the rider body and latches the rider contact/recovery flags. Only solid contacts
apply the separate final damping stage. Water therefore must feed the existing
wreck recovery grounded gate when live integration is added.

Native inputs, source textures, original exports and result files remain in ignored
`work/priority-water/` and `private/reports/`; no game content was added to source.
