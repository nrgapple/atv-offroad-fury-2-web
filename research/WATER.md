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
