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
saving a new checkpoint. The ninth ray now starts ten feet above the candidate
and points downward fifteen feet. `001ea050` raises the candidate Y directly to
a higher platform hit, while hits on rays 1–8 reject it. Water gating,
vegetation-category and other-player exclusion checks remain separate work.

## Rider motion

`002d7dc0` registers 14 original wreck clips: five directional separation clips,
four airborne clips, four ground-bounce clips and swimming. `001ee990` moves from
attached separation to independent airborne/ground behavior. Separation completes
when the selected clip has 0.5 seconds remaining; it is not one fixed ejection delay.
`001eff20` supplies vehicle velocity plus separation root-motion velocity at detach.
The rider uses 13 original bone-local terrain contact points and its own rigid body.
Supported static type-0 props now receive the rider collision spokes; active
wrecks use the Vehicle's shared scene cache with the native inclusive rider
distance gate. Dynamic water contacts are connected as described in
[water evidence](WATER.md). Physical vegetation proxies, moving props and
compound/dynamic collision are not yet connected. A separate
native wreck camera now tracks both the ATV and rider during this sequence.

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
reproduced. The Model/Pelvis rebase and world root-motion velocity kernels now have direct
original-instruction comparisons; their coupled animation/contact/body sequence
is still not a full original-game replay.
The detached body now updates its center of gravity from all 13 animated
contact points. `00193b40` takes the midpoint of their world-axis bounds;
`001ef690` transforms that point into body-local space. The port retains the
rendered body origin and existing velocities when the pose changes, so this
moves the contact lever arms without teleporting the rider or adding an impulse.
This replaces the previous pelvis-only center. The Rust bounds kernel matches
500 finite synthetic cases executed through the original `00193b40` instructions
bit for bit; point-transform outputs are supplied by hooks and terrain queries
are disabled in that comparison. The original-content probe still passes all
1,694 clip samples and five detach/ground/recovery cases. These checks validate
the bounds kernel and integration invariants, not complete original crash parity.
The reconstructed wreck camera replaces the normal follow camera during
separation and recovery. Its static-object clearance remains incomplete; see
[camera evidence](CAMERA.md).

## Full-rotation investigation

The report that touch pullback cannot complete a flip is not resolved by the
component comparisons above. A separate 361-orientation pitch sweep ran the
original `00146850` orientation extraction followed by `001e8690` with ATV01's
actual angle settings, no wheel/body contact, no impact, and safe stunt state.
Its crash decisions agree with the Rust path for every sampled degree. Backward
pitch first requests a rear separation at approximately 85 degrees. Angle error
is at most 0.000000239 radians; the native atan2 call uses a host-math hook, so this
is not an end-to-end original-game replay. The private interpreter probe explicitly
uses the R5900 SQRT.S source field (`ft`). Evidence is retained under
`private/reports/wreck/flip-angle-probe.json` and `work/flip-angle-probe/`.

This establishes a full-flip barrier in the currently connected normal crash
rules; it does not establish all original mode/cheat bypasses. `001e8a20` additionally
checks the global wreck-enable flag (or boundary override), and that mode policy
is not yet reconstructed. Do not describe unlimited flips, or the overall driving
mechanics, as verified original parity. A permissive full-flip mode would need to
be labeled as an explicit gameplay option, separate from these recovered rules.


## Static rider contacts and platform recovery

The rider uses a runtime type-3 segment array, not the quad's type-2 ray tree.
`00193b40` builds thirteen spokes from the midpoint of the animated points'
world-axis bounds to those same points, preserving bone/contact registration
order. `003272e0` copies those endpoints into the query actor. `00121440` resets
nearest-hit results for each object and calls `001148c0` through `00117830`;
`00193060` appends that object's hits in increasing ray-index order, up to sixty
contact records. Prior-object nearest hits must not be carried into the next
object. The Rust static-segment path now preserves these per-object resets,
mesh-child order and contact order, reusing the recovered flat-face kernel.

The active-wreck caller now borrows the quad/recovery `SceneCollision` cache
instead of scanning every ordinary prop. `00327360` derives its current actor
sphere from the thirteen animated world endpoints; `0011fd30` includes nodes
at exactly that radius and rechecks current ordinary eligibility even on cache
reuse. A rider refresh can remove actor-excluded nodes from shared membership;
a subsequent quad reuse does not restore them. The actor sphere uses binary32
XYZ square accumulation, matching 500 original endpoint-copy/preparation cases
and 1,112,500 instructions with zero compared center/radius mismatches. The
bound-result reset service is hooked in that component proof. Existing static
spoke/geometry comparisons below remain separate proofs.

`Animator::step_with_scene` retains direct fallback when no shared scene is
provided. Scene diagnostics distinguish ordinary selections, retained plants,
and configured but unsupported plant proxies. The boundary block explicitly
sets/clears both quad and rider actor `bf.0`; the bridge passes that paired flag
without changing collector `+82c`. General game `+674` initialization remains
unverified. Explicit ordinary-index exclusions are supported, but native `MUD`
category transport is not yet connected: command49 does not carry placement
names or the original game `+584` base-name provenance. No filename heuristic
is applied. Normal riding rider collider/physics updates remain separate from
this active-wreck integration.

Static prop contacts feed the same independent body solver, with the rider's
0.25 positional correction, zero reflection and default surface-0 grip 0.2.
The existing placement model lacks the native `.frN` filename material override;
nonzero prop material categories remain incomplete. Both attached overlap and
detached motion can report static contact for contact-gated recovery. The
`static_contact_count` frame field distinguishes those records. Air-to-ground
clip selection now uses the final substep's solver count, as `001ee990` does;
the recovery clock continues to receive any contact from the outer frame.

The private instruction probe executed `00193b40` and its real `003272e0`
endpoint copy for 500 finite clouds. All 6,500 spoke starts, ends and indices
bit-match the Rust kernel; world point transforms are supplied by hooks and the
scene query is stopped after the copy. A second probe executed 1,305,026 original
instructions through the flat leaf branch of `001148c0` for 500 sets of thirteen
axis-aligned rays. All 6,500 hit decisions, including 254 hits' XYZ, normal and
fraction, bit-match the shared Rust flat-face kernel. This direct-leaf check
excludes tree broadphase, world transforms, optional smooth normals and contact
response. An additional 500 thirteen-ray cases start at a two-child native mesh
root, exercising its box branch, child traversal and nearest-hit replacement.
The Rust query matches all 6,500 ray decisions and 545 hits' indices, XYZ and
flat normals after 2,914,699 original instructions. A further 300 cases execute
world preparation `00327360`, object inverse transforms `00121440`, tree and
leaf query, and world hit transforms using 3,900 oblique rays against translated,
non-cardinal rotated props. All decisions and 3,240 hit XYZ/normals bit-match
Rust after 3,815,525 original instructions, including `00193060` endpoint
reconstruction. The returned contact endpoint is start + direction * length,
not a reread of the unnormalized original endpoint. This exposed and corrected local
endpoint recomputation: the native prepares normalized world direction and
length once, then transforms start and direction independently without changing
length. A separate 4,000 four-ray box probe (3,202,886 instructions) matches all
16,000 prefix-mask decisions, including zero direction axes and face starts.
Type-3 boxes use X/Z/Y plane-entry order and stop pruning after their first
entering ray, preserving the later incoming mask. These are finite synthetic
cases, not exhaustive hardware floating-point or whole-game replay. Evidence is in
`private/reports/wreck/static-spokes-validation.json`.

Meaningful core regressions cover platform height raising, the fifteen-foot
ray's reach, side obstruction, transformed placements, per-object hit reset and
source ray order, contact capacity, and a rider prop collision that slows motion
and applies positional correction without terrain. The existing original-content
probe still passes all 1,694 clip samples and five detach/terrain/recovery cases.
A separate original-content probe places the actual `distillery.col` roof twenty
feet above Caribou terrain and starts each original separation pose one foot
inside that roof. All five types detect static prop contact above the terrain,
detach, keep finite transforms and reach recovery. That initial-overlap fixture
uses a synthetic placement; it is not an original-game capture or proof of all
prop impacts. Its report is `private/reports/wreck/static-prop-validation.json`.

The native default type-3 flat-normal path now includes its coarse center/radius
gate, exact box entry arithmetic and prefix mask. Optional smooth-normal policy
is not implemented; constructor `00326bb0` defaults it to zero and `00327810`
copies that policy to the query. The thirteen spokes
sample the current animated hull; they are not a swept-volume replacement.
Complete plane crossings and back-face starts can miss in an isolated segment
query. No browser-specific swept collision has been added to the original
baseline. The native Model/Pelvis placement and attached contact/body steps are now
connected as described below. The coupled original-game sequence, collision
categories, water and moving actors remain prerequisites for complete wreck
parity. Native component and original-content checks do not establish public
browser or whole-game equivalence.


## Model container, Pelvis and attached physics

The independent body belongs to the rider's Model scene container (`001ee090`),
not the animated Pelvis child at rider offset `+0x168`. `001e7600` copies the
quad body's world matrix to that container during Attached state, then applies
the rider placement offset. Native `001ef690` calculates the bounds center and
runs contact/body substeps before `001ef470` advances animation. Gravity is
skipped in Attached state, and the body's integration flag initially stays off.
Any contact activates that flag before the same substep's body integration;
position correction therefore also applies during separation. Subsequent
Attached frames obtain the new quad placement again.

The Rust rider body now follows this ordering and uses the Model container's
axes for inertia, torque and angular velocity. It samples collision points from
the pre-animation pose, solves both terrain and supported static props, saves
the Pelvis world transform after physics, then advances the clip. It transfers
world Pelvis translation/rotation changes at detachment; those changes are not
rotated through the quad basis a second time. New Air/Ground clip categories
start on the following outer frame, preserving the native phase/clip timing.

`001f03e0` rebases the Model container after independent clip changes/loops to
retain the saved Pelvis world transform. It orthonormalizes forward, then right
and up, and corrects translation after evaluating the Pelvis again. This
replaces the previous scheme that made Pelvis itself the physical body's frame
and inverted its animation root on every body substep. Model scene placement
is synchronized back into the body's position/orientation without clearing
velocity or integration history.

A private probe runs the actual `001f03e0`, scene getters/setters, parent/child
matrix updates and invalidation routines on 500 finite synthetic Model/Pelvis
cases (302,000 instructions). The Rust rebase output bit-matches all 500 cases;
there are no transform hooks. Another probe runs actual `001eff20`, the Pelvis
world getter and Model inverse-vector routine for 500 synthetic cases with both
root-motion branches (110,750 instructions). Its transferred linear/angular
velocities also bit-match; a fixed RNG hook produces zero angular noise. The
report is `private/reports/wreck/model-pelvis-validation.json`. Neither probe
executes the original animation decoder or a coupled gameplay contact replay.

Core regressions verify that an attached rider retains Model axes while its
Pelvis rotates, follows quad placement without gravity, activates/corrects its
body on an overlapping prop, and holds the saved Pelvis transform across root
rebasing. The real-content terrain and original distillery overlap probes still
pass all five separation types after this change. Public browser behavior must
be verified for the exact deployed release; complete wreck parity remains open.

## Riding rider-height crash input

The post-substep block of `001dd830` resets vehicle flag `+0x3b4` each outer
step, then sets it when the final solver has any contact, there is a static
contact row, and its first local endpoint Y exceeds four feet. The native loop
never advances its contact-row pointer; a later tall endpoint alone does not
set the flag. `wreck::rider_height_contact` preserves that first-row and strict
boundary behavior. Six native instruction cases (215 instructions) cover no
solver contact, no rows, the exact boundary, later tall rows and a first tall
row; report `private/reports/wreck/riding-rider-height-validation.json`.

`001e8690` checks this input after impact-wreck and hard-landing branches and
requires cooldown `+0x800<=0`; forward local speed above zero chooses Back,
otherwise Front. `001e9050` sets the recovery cooldown to two seconds; initial
construction uses zero. The vehicle caller owns cooldown and landing-action
gates. This flag is a riding hull-height test, separate from the animated
independent rider's thirteen static spokes. It does not establish coupled
vehicle/rider/static-prop gameplay equivalence.

## Chassis query cadence and terrain gate

`001dd830` freezes the chassis terrain-enable flag before its outer queries from
Model world-up Y: solver terrain loop is enabled at or below binary32 `0.4` and
disabled above it. The later `00194930` uses this cached gate throughout every
substep. Six actual instruction cases execute the gate then the terrain solver
with a changed Model up-axis; all six retain the outer gate and dispatch a
penetrating terrain contact only when enabled (934 instructions). Report:
`private/reports/wreck/chassis-terrain-gate-validation.json`. Static prop
contacts remain enabled independently. Normal upright support belongs to wheels.

Chassis actor `+0x180` queries both chassis terrain endpoints and the bound AR
bundle once before substeps. Actor `+0x188` is a separate four-point controller
query, not the chassis terrain actor. Query cadence therefore agrees with the
port's outer cached planes; each substep reprojects those planes against the
updated body and runs response before body integration. Scene candidate ordering,
category filters, and the restricted post-loop full-correction query remain
separate coupled collision questions. The port's geometric/contact component
checks are not an original captured sideways-rail gameplay replay.

Recovery cooldown provenance is initial `+0x800=0` in `001e1b10` and reset
`+0x800=2.0` in `001e9050`. On the regular riding route, `001e5260` subtracts the
outer timestep before physics when cooldown is positive, player bit one is clear
and global session state is not two; subtraction is not clamped. Exceptional
reset/paused player routes also manipulate collision category masks and opacity.
Only the rider-height branch reads this cooldown in `001e8690`; impact and later
stunt/angle decisions stay active. Hard-landing actions precede and suppress the
height branch, but later stunt/angle wrecks still override them. Five hundred
original `001e8690` cases (51,712 instructions) with varied height flag,
positive/zero/negative cooldown, forward speed, hard landing and stunt safety
match `rider_height_allowed` plus the Rust crash evaluator.

The post-loop full-correction pass remains omitted in the port. `0011e7e0`
registers the runtime node class names `CollisionObject` and `Vegetation`;
`DAT_00460f00` is the registered CollisionObject tag, not a dynamic-body-only
class or a literal 255 after initialization. `001dd830` builds a restricted
second candidate list from CollisionObject nodes with mode bit `bf.3` clear.
After all body substeps, it re-queries chassis actor `+0x180`, temporarily sets
solver positional correction scale to `1.0` (normal scale `0.3`), runs the solver
and restores the normal scale. No body integration follows that pass. Its body
correction/force buffers therefore need exact next-frame handling: `001469e0`
reconstructs COM from Model origin, applies buffer `+0x160` during integration,
and clears it at the end. Node filters and category registration are independent
of the parsed COL flags. The standard static placement loader `00206e90` passes
mode `0x0d` to `00120510`, whose `0011eb90` call sets `bf.3`, then assigns
category `00462d38`. These ordinary static COL placements are excluded from the
restricted second pass; its omission must not be used to explain a normal
static railing impact. Vehicle colliders use modes with `bf.3` clear. Other
loaders, exact pending-buffer handling and moving-body response still need
validation. The current placement transport lacks native runtime category and
filter metadata. A captured coupled rail impact, original candidate order and
full driving replay remain required before claiming driving or wreck parity.

## Four-point terrain stop during riding and wrecks

The independent four-point solver at quad `+0x184` remains active in the wreck
path. Its query actor `+0x188` is created by `00193720`, which initializes collider
and spoke-array pointers to zero. The exact `001e1fd0` caller passes both optional
collider modes as zero to `00193830`. The supplied offset `[0,2.5,0]` is unused on
that branch; it must not be interpreted as an active type-3 prop ray origin.
This actor samples terrain planes only. Scene prop order and static segment queries
do not supply its stop points.

Its solver terrain flag remains enabled (`+0x48=1`), separately from the chassis
terrain gate at up.Y<=0.4. Query flag `+0x68` only gates an optional collider and
therefore does not disable terrain sampling here. `001dd830` samples these points
once per outer frame, then runs solver `+0x184` after rider forces and before wheel
suspension each active quad substep. `001e8a20`/`001e8d10` change the chassis wreck
collider and response configuration but do not disable this stop actor/solver.
The stop response performs velocity/force projection rather than a chassis
impact impulse or positional correction; see the authored `wheel_stop` component.

A private probe executes the original constructors, exact vehicle caller block,
Model world getter, terrain query dispatcher and contact solver dispatcher for
six riding/wreck/recovery and query-gate combinations (7,044 instructions).
Every case samples four terrain points and dispatches four penetrating stop
responses, without allocation of a collider or any prop query. Terrain values
and the stop-response kernel are hooks in this dispatch check. Report:
`private/reports/wreck/wheel-stop-query-validation.json`. This establishes setup
and dispatch gates; it does not establish coupled incline/landing gameplay parity
or prove that a deeply clipped quad can recover from an already invalid state.

## Boundary launch connection

The original map-edge branch forces the random wreck path and temporarily raises
both quad and rider speed limits. Its verified thresholds, launch velocity,
vegetation-query timing, RNG endpoint fallthrough and recovery obligations are
recorded in [boundary launch evidence](BOUNDARY_LAUNCH.md). The rider Animator
exposes consecutive launch-related RNG words and a boundary speed-limit setter.
Vehicle/WASM now share those four draws, connect the limit and launch helpers,
and permit terrain-projected recovery candidates during boundary flight. The
acceptance helper matches432 native wrappers; checkpoint invocation remains the
port's per-outer-update phase. Its original manager callback, player flags and
Model matrix inputs are now recovered, while its ordering relative to quad
physics and late water sampling remains unverified; see the boundary evidence.
Vegetation clearance and complete launch/camera equivalence remain
separate work.
