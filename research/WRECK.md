# Crash and recovery reconstruction

The USA v2.00 crash path is being connected to the authored Rust runtime. This is
not an emulator or a replacement physics engine. Native export files and source
assets remain private.

## Recovered behavior

`001e8690` classifies local collision velocity changes and pitch/roll against the
ATV's authored `[Wrecks]` settings. Normal speed alone is not an impact. The body
integrator's `001dd830` retains the signed largest absolute collision delta on each
world axis across substeps; `001e5260` transforms that result into vehicle space.
The Rust classifier matched 1,000 synthetic original-instruction cases, including
branch precedence, wheel/body contact and unfinished-stunt landing gates. The
instruction interpreter is finite-binary32 component validation, not PS2 hardware
or whole-game replay equivalence.

Landing while holding a stunt, or with at least 0.5 seconds left in its released main
clip, selects native vertical wreck type 4. Angle and impact thresholds also select
front, left, right or back separation. ATV01's original airborne pitch limits are
85 degrees upward and 70 degrees downward, with 80 degrees roll. These are ejection
thresholds, so retaining the original rules does not promise unrestricted full
flips. More permissive rotation would be an explicit enhancement.

`001e8a20` changes the quad's center of gravity, collision friction, torque and
angular impulse settings to the original wreck values. It switches the obstacle
ray hull from AR to AO; `001e8d10` restores AR and the normal settings. Rider steering,
throttle, brakes and air-control forces are suppressed while wrecked. The quad
continues through the existing rigid-body, suspension and static-contact solvers.

The constructor recovery delay is 1.8 seconds. `001e5260` first spends 0.8 seconds in
falling state, then waits for wrecked-rider contact before starting another 1.8 second
recovery timer. The ATV's wheels are not a substitute for rider contact. Reset
clears motion and integration history and returns to a saved safe riding point.
`001ea860` explicitly flattens and normalizes its saved heading and uses world up;
it does not retain the previous ground pitch/roll.

The original checkpoint gates require all four wheels down and body up Y at least
0.8. Eight 20-foot horizontal clearance rays at 3 feet above the vehicle origin are
reconstructed from `001ea4d0` and queried against supported static obstacles before
saving a new checkpoint. The current port retains the terrain-supported height.
The original downward platform-height ray, water gating, vegetation-category and
other-player exclusion checks remain separate work.

## Rider motion

`002d7dc0` registers 14 original wreck clips: five directional separation clips,
four airborne clips, four ground-bounce clips and swimming. `001ee990` moves from
attached separation to independent airborne/ground behavior. Separation completes
when the selected clip has 0.5 seconds remaining; it is not one fixed ejection delay.
`001eff20` supplies vehicle velocity plus separation root-motion velocity at detach.
The rider uses 13 original bone-local terrain contact points and its own rigid body.
Water and rider-versus-prop/dynamic collision are not yet connected. The native
wreck camera is also separate from the currently connected chase camera.

## Validation scope

Core regressions check unsafe stunt landings, strict threshold comparisons,
passive quad behavior during a wreck, contact-gated recovery, cleared motion on
reset and normal collision settings after recovery. Actual-content clip and
browser validation are recorded separately; component tests alone do not establish
whole-game equivalence.

The private original-content probe sampled all 14 clips at 121 times each (1,694
poses) and exercised each of the five separation types through airborne motion,
terrain contact and the recovery clock. All poses and body transforms were finite;
all five cases reached recovery. The original rear separation reaches its detach
window earlier than the other four because its MOT is shorter.

`tools/test_wreck.mjs` drives actual Chromium touch events on the original Caribou
ramp. It verifies preload takeoff, Nac Nac, an unfinished landing selecting vertical
wreck, original separation/ground motion, rider-ground contact, automatic reset
and resumed driving. This path makes early terrain contact and therefore skips
the airborne wreck clip, as the original phase rules permit. The private isolated
cases cover the airborne path. A separate real-touch preload hop without a stunt landed cleanly, stayed in Riding
state throughout and never incremented its reset counter (`tools/test_clean_landing.mjs`).
These are host checks until the exact release is
separately verified in production.

The detached rider uses original mass/inertia, 1/90-second maximum substeps,
13 animated terrain points and 0.25 penetration correction. Reflection is zero;
the rider has its own friction table, not the quad's material-grip table. Ground
damping uses the last substep contact count, while recovery remembers any contact
in the outer frame. Clip selection uses the original random algorithm, with an
independently seeded stream; the complete game's shared RNG call order is not yet
reproduced. Pelvis anchoring and root-motion velocity are reconstructed, but their
full native transform sequence has not been instruction-replayed end to end.
The native body also updates its center of gravity from the animated contact-point
bounds; the current detached body keeps its center at the pelvis, so that update
remains to be connected.
The current follow camera can crop the detached rider while the quad rolls; the
original wreck camera remains pending.
