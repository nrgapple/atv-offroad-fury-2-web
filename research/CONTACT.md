# Chassis contact reconstruction

The USA v2.00 executable supplies a callable Rust static-contact response in `crates/atv-core/src/contact.rs`. This covers normal riding against stationary surfaces. Moving-body response, obstacle contact generation, wreck transitions, and the complete collision system remain separate work.

## Recovered behavior

- `00194930` processes chassis points in stored order. Positive plane penetration projects each point onto the surface and invokes the response, followed by friction.
- `001953d0` computes point velocity by converting the center-to-contact arm into body coordinates, crossing it with local angular velocity, and rotating the result back to world coordinates.
- `00195b40` computes effective contact mass using inverse mass and rotational inertia. Inward point velocity produces a restitution impulse. The configured angular impulse scale is applied after the inertia calculation; zero angular scale does not remove rotational terms from effective mass.
- The normal riding branch removes a fraction of accumulated force directed into the surface. Penetration produces a deferred translation correction, applied after body integration.
- `00146250` combines corrections by adding perpendicular components and keeping the greater parallel component. Simply summing every overlapping contact would differ.
- `00195890` applies tangential friction based on material grip, vehicle tuning, mass, and tangential speed, with a separate torque scale. It uses the point velocity captured before the impact response.
- `001e1fd0` selects the quad's `ao.col` hull for chassis terrain contacts and `ar.col` for an object query. These are type-2 ray trees, unlike the type-0 triangle files used by most scenery.
- `00194590` walks the ray tree's first child followed by its second. Each leaf adds its endpoint as a zero-radius chassis point. `001943d0` preserves the first occurrence of coincident points and limits the list to the solver's capacity.

The implementation keeps the assembly's floating-point operation order; several decompiler expressions reorder multiplication and addition. The reciprocal-square-root helper models a single finite IEEE rounding operation. PS2-specific exceptional values, denormal handling, and reciprocal approximation remain unverified.

## Validation

A private bounded R5900/VU instruction interpreter executes the original ELF instructions; it is not a PS2 emulator or a hardware replay oracle. It substitutes only the scene matrix-cache refresh with the supplied final transform.

- 500 static-response cases: 577,606 instructions, all 15 output scalars match bit for bit.
- 500 cases with rotated/transformed bodies and original point-velocity evaluation: 672,836 instructions, all 15 output scalars match bit for bit.
- 400 combinations of existing/incoming position corrections: 43,600 instructions, all three output scalars match bit for bit.
- Focused synthetic Rust tests cover inward impact and overlapping corrections. Core Clippy passes.

Inputs, original instruction listings, interpreter scripts, and detailed comparisons stay in ignored work/private paths. These results validate component arithmetic against the stated interpreter model, not complete driving behavior or original PS2 hardware fidelity.
