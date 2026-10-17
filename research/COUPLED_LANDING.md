# Coupled landing and spring torque audit

The coupled audit found a verified numerical discrepancy in spring torque at
large world coordinates. The Rust vehicle now preserves the original Model
inverse-transform order when applying each grounded spring force. This improves
bounded instruction comparisons; it does not establish a reduction in rendered
tire overlap or complete original handling parity.

## Original scheduling under test

The private probe executes actual `001dd830` instructions, including its previous
wheel-contact/normal snapshots, four outer wheel support wrappers (`001ebd90`),
gravity actor (`00164390`), substep loop, four-point travel stop (`00194930` /
`00196cf0`), all four springs (`001ed330`), full body update (`001469e0`) with its
integrator and Model setters, impact accumulation, and suspension visual update
(`001edcd0`). The engine substep routine `001f60f0` is an actual two-instruction
stub and is executed. Support planes remain frozen across each outer frame.

The fixture explicitly supplies analytic flat/slope terrain, empty static scene
queries, grip/rider force and torque inputs, zero wheel drive forces, and finite
host acos. Outer engine/wheel rotation services are hooks. It uses synthetic
body/wheel configurations and initialized integrator histories; this is not a
captured whole-quad PS2 state. Scene selection, game controllers, mechanical pose
updates, native math-table initialization and complete gameplay remain separate
gates. The Rust companion calls the production stop, spring and body kernels with
an adapter preserving the recovered scheduling. It is not a direct replay of
every branch of `Vehicle::step_with_scene`.

There are 300 independent outer frames: 200 one-substep frames and 100 two-substep
frames, for 400 body substeps and 2,185,204 original instructions. Cases vary
pitch, roll, slopes, downward velocity, angular velocity, previous contact state,
cached world CoG and visual compression. Half use world X/Z around 3,000–5,000
feet to expose cancellation at actual track coordinate scales. All 1,200 final
wheel-contact decisions agree.

## Verified spring force discrepancy

The grounded spring branch at `001ed85c` converts its world force with
`00166400` and world contact point with `00166510`, then subtracts the current
body-local CoG. The Model inverse applies rotation to the full world point and
adds the separately rotated negative Model translation. The former Rust call
subtracted world CoG before rotating. The two expressions are algebraically
equivalent, but their binary32 cancellation differs at large coordinates.

`vehicle::apply_spring_world_force` now retains the native ordering for this
spring call alone. Its geometry uses the current Model origin/current local CoG;
point velocity retains the already-recovered preceding world-center cache before
the first body integration. Generic body-force and other collision callers were
not changed by this spring audit.

An isolated original `001ed85c`–`001edad0` replay covers 1,000 synthetic world
force/contact-point cases and 220,000 instructions. The previous order differs in
2,099 of 3,000 torque components, with maximum absolute difference 3.826172 in
source torque units. The recovered order matches all 3,000 torque components
bit-for-bit. An authored large-world regression pins the production helper's
force/torque results and verifies that applying force does not change position,
orientation or velocity.

## Bounded coupled results

These are differences from the synthetic native frame replay, not measured
browser tire clearances or claims about perceptual improvement.

| Maximum absolute difference | Previous spring lever order | Recovered order |
| --- | ---: | ---: |
| World CoG position, feet | 0.000549316 | 0.000000477 |
| Linear velocity, feet/second | 0.0198259 | 0.00000191 |
| Local angular velocity, radians/second | 0.0693121 | 0.00285339 |
| Final visual suspension compression, feet | 0.000907987 | 0.000000238 |

The coupled result remains imperfect: 23 position, 28 linear-velocity, 128
angular-velocity and 47 compression components differ across these frames. The
one-substep subset's maximum angular difference is `0.00000620`; the largest
angular differences occur after the second substep. Native Model-to-quaternion
rebinding and matrix rounding remain numerical limits, with cancellation at large
world coordinates amplifying small pose differences. This audit has not shown
that those limits explain half-foot rendered tire overlap.

## Browser trace limits and reproduction

The initial clean-landing trace had 62 latest-render snapshots, with missing
simulation steps between them. During this audit, the release agent replaced
that shared trace with a capture of every delivered WASM batch frame; the observed
replacement has 192 consecutive steps. Earlier host/public minima from different
sample counts are not an isolated before/after comparison. The browser trace also
does not expose original PS2 state or internal solver histories/forces, so it
cannot initialize a complete native replay without additional instrumentation.

Private probes live under `work/coupled-landing-probe/`; reports remain under
ignored `private/reports/coupled-landing-*.json`. No source bytes, pseudocode or
runtime assets are included in these shareable notes.

```sh
python3 work/coupled-landing-probe/generate.py
python3 work/coupled-landing-probe/lever.py
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo run --release \
  --manifest-path work/coupled-landing-probe/Cargo.toml --bin lever
NATIVE_LEVER=1 CARGO_TARGET_DIR=work/terrain-contact-target \
  ~/.cargo/bin/cargo run --release \
  --manifest-path work/coupled-landing-probe/Cargo.toml --bin coupled-landing-probe
python3 work/coupled-landing-probe/compare.py
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo test --locked \
  -p atv-core vehicle::tests
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo clippy --locked \
  -p atv-core --all-targets -- -D warnings
~/.cargo/bin/rustfmt --edition 2024 --check crates/atv-core/src/vehicle.rs
make verify
```

All 18 vehicle tests, strict core Clippy, formatting, setup verification and diff
checks pass for this component gate. Browser integration and public delivery are
the release agent's next gate. No new public result is claimed here. The finite
instruction interpreter remains a bounded arithmetic model, not PS2 hardware
execution. See [movement](MOVEMENT.md) and [terrain contact](TERRAIN_CONTACT.md).

## Residual overlap decomposition (2026-10-16)

This time-boxed audit asks where the remaining rendered overlap comes from. It
changes no runtime behavior. A private scratch probe (ignored `work/clearance-probe/`,
outputs in `private/reports/coupled-landing-overlap-decomposition.txt`) reads the same inputs as `terrain_clearance`: the original
ATV01 PSG, the ENV01 TR2 and per-step browser traces from main `7a346ab`. For each
overlapping tire vertex it records the angle from body-down around the axle, the
lateral offset from the wheel's centre plane, and the vertical and normal-direction
clearance. It also renders the same frame's linkage at the **previous** frame's body
pose. At 60 Hz a frame is one substep, so that previous pose is the pose this
frame's spring evaluation used.

| Trace | Rendered, vertical | Rendered, along normal | Spring-time pose, along normal | Spring-time pose, centre plane only |
| --- | ---: | ---: | ---: | ---: |
| Clean preload landing (184 riding steps) | -0.349 ft | -0.348 ft | -0.126 ft | -0.119 ft |
| Slope turn (439 riding steps, 35° cross-slope landing) | -0.640 ft | -0.522 ft | -0.439 ft | -0.331 ft |

Observed contributors:

1. **One-step display order (about 0.22 ft on the landing).** The worst landing
   vertices sit within 30° of body-down at the centre plane. The body-down
   bottom point itself is 0.35 ft below terrain while the reported compression
   is only 0.33 ft. The spring compression is computed before the substep's
   body integration, but the frame is drawn after it. At landing the front of the
   quad drops about 0.25–0.37 ft per step (linear plus pitch rate). The
   original orders this the same way. In `001dd830` the four `001ed330` springs
   run before the body's integration virtual in each substep; the post-loop
   `001edcd0` then rebuilds the visual wheel offsets from the stored compression
   `+0x23c` without re-querying terrain, and `001d65c0` runs afterwards. The port
   reproduces this order.
2. **Travel bottoming on hard cross-slope landings.** In the slope trace the
   worst wheel reaches the full 0.95 ft travel (ATV01 `Compression`) with the
   four-point stop engaged. `00196cf0` only removes inward velocity and force
   (600-case comparison in [movement](MOVEMENT.md)); it applies no positional
   correction, so depth beyond travel stays visible for a few frames.
3. **Single-point wheel contact.** `001ed330` contacts the terrain plane at one
   point on the wheel's centre plane. When the body's roll differs from a 35°
   slope, the tire's outer edge (lateral offset about 0.4 ft) sits a further
   ~0.11 ft lower. Tire curvature against curved terrain is likewise not modelled.
4. **Measurement.** `terrain_clearance` reports vertical clearance, which
   overstates normal penetration by 1/cos(slope) (about 20% at 35°).

Ruled out as causes of terrain-landing overlap:

- The post-loop full-correction pass is connected for nonstatic actors. The
  original tail skips its query and solver when the restricted list is empty
  (24-case tail replay in [train contact](TRAIN_CONTACT.md)), so it never runs
  for ordinary terrain landings.
- Smooth normals: the type-3 query policy defaults to zero (`00326bb0`, copied by
  `00327810`). Wheel/terrain normals are the TR2 interpolated normals, which already
  match bit for bit ([terrain contact](TERRAIN_CONTACT.md)).
- Visual versus physical wheel size: the visible tire YZ radius is 0.7987/0.7456 ft
  (front/rear), against physical radii from bounds half-Y of 0.7985/0.7429 ft.
  The bounds centres are within 0.0002 ft of the wheel node origin.

Conclusion: most of the measured overlap follows from recovered original ordering
and contact-model properties, so no physics change is made. A residual of about
0.12 ft at the spring-time pose on the landing is not decomposed further; the
frame-cached outer support plane on curved terrain is a candidate, not
established. None of this is a PS2 capture, so equal visual overlap on hardware
remains unverified. Re-projecting the visual wheels after integration would
reduce the overlap, but it would be an explicit presentation enhancement, not
original behavior.
