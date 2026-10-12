# Rider animation recovery

`animation::decode_mot` reads the original MOT version 2 and 3 streams. `Animation::sample_pose` binds controllers to PSG nodes and returns column-major local/world matrices. Original rider vertices already retain rigid joint indices and local positions in the PSG reader; transform each local vertex by its sampled joint matrix.

The exact USA v2 executable provides the evidence: loader `0x001cb0d0`, ShortAxis decoder `0x001cbd90`, lookup initializer `0x001cb7e0`, rotation sampler `0x001c8160`, VU interpolation `0x002f8650`, and Hermite sampler/search `0x001c9160`/`0x001c9400`. Private Ghidra exports include assembly and incoming/call context. These are this build's addresses, not first-game symbol substitutions.

MOT starts with signed -2/-3 and frame count. Version 3 adds length-prefixed NUL-terminated bone names; version 2 uses model indices directly. Duration, rotation/translation type and two controller counts follow. Each controller starts with key count and bone index. Linear translation keys contain time and XYZ (16 bytes); Hermite keys also contain incoming/outgoing tangents (40 bytes). ShortAxis rotation keys contain time, u16 angle and three i16 axis components (12 bytes). Duplicate controllers are preserved in file order. Other rotation formats fail explicitly.

ShortAxis decoding uses half-angle `angle * f32(0x3a490fdb)` and axis scale `sin(half-angle) * f32(0x38000100)`, matching the original 4096-entry lookup initializer. Quaternion interpolation follows the recovered shortest-arc branch and seventh-order sine polynomial. Host sin/cos/acos are not established as bit-identical to PS2 library/lookup results. Hermite tangents are already segment-scaled; no extra time-interval multiplier is applied. Terminal Hermite translation extrapolation follows the original controller; rotations hold their final key. Public sampling accepts only finite times in the stored clip duration. Game-side timing remains separate.

All 146 extracted rider/arm MOT streams decode and sample at 101 times each, yielding 14,746 finite poses against their appropriate original PSG skeleton. Male and female models have 18 and 27 nodes; the five first-person arm motions use separate arm models. Evidence is private in `private/reports/animation-pose-rust-audit.tsv`. Synthetic tests cover truncation, trailing data, invalid times, packed rotation, shortest-arc interpolation, name binding, hierarchy composition, Hermite/step sampling and terminal extrapolation.

A representative seated pose is `/ANIMSRDR.RES` → `Anim/Rider/m_turnm.mot`, time 0.8 seconds, applied to `/RIDER/MRIDER.RES` → `anim/rider/mrider.psg`. Original materials/textures reside in `/RIDER/RIDER_MT.RES`. A private render confirms the rider sits on the recovered quad with hands and feet near the original handle/foot nodes.

## Normal riding selection

`motion_catalog` recovers the MCF loader's motion paths, interpolation flags and pre-load transition defaults. Both 78-motion rider catalogs and the separate arm catalogs parse. MCF FPS does not rescale MOT key times.

`rider_pose` recovers the normal reverse, slow-turn, ground-lean and airborne clip selection from `0x002d8ba0`/`0x002d8e50`. It preserves pair/four-clip ordering, shared absolute sample time, pose snapshots and the Head-only look overlay. An instruction-interpreter comparison covered 1,000 cases and 216,406 original instructions with no selector, weight or clip-argument mismatches. Private evidence is in `work/physics/rider-pose-*`.

The renderer now connects the selector to 14 normal riding clips, ordered quaternion channel blends, pose snapshots/crossfades and the Head-only look overlay. The reverse path is sampled separately because its controller order differs from the normal clip path. The native Rust animator probe in `work/animation/animator_probe.rs` produced 1,200 finite poses across turn, reverse, lean and air states. Separately, browser diagnostics confirmed all 18 rider nodes animate in turn/turn/reverse modes with changing matrices, and the full browser regression passed locally and in public v5. Crouch and look signals at that historical checkpoint came from constructor defaults. Later checkpoints connect vertical crouch, main/held stunts and wreck clips; hand/foot IK, the full event system and female hair remain unfinished. Public v5 includes this normal-riding connection. This is component evidence, not a full animation-system or PS2-hardware match.

### Preload presentation and vertical crouch (host checkpoint)

The normal riding crouch is no longer fixed at its constructor value. The tail
of USA v2.00 `001e5260` drives the existing original lean/down clip blend from
local vertical velocity changes, with separate demand and blend rate limits.
Nine synthetic velocity samples match the original instruction sequence
`001e6b88..001e6d58` bit-for-bit for the resulting blend; the transform call was
hooked to supply known local velocities. This is a finite-instruction comparison,
not whole-game or PS2-hardware equivalence.

The browser's separate hold/release preload gesture also feeds the original
fore/aft pose clips when grounded and physical lean is neutral, using the original
joystick smoothing rate. This presentation adaptation restores visible preparation
without adding suspension or launch forces. Physical lean and airborne stunt poses
retain priority. Original wreck animation and rider separation remain separate work.


## Stunt control and forced hard landings (host checkpoint, 2026-10-11)

The existing stunt controller selects all 24 original main motions and their 24
held extensions, using Circle, Triangle and L1 modifier families. Main clocks
stop during a hold; releasing the accepted modifiers resumes the main motion.
A neutral modifier release rearms another stunt, subject to the current clip's
minimum main time. This is the recovered control/animation path, with scoring,
profile unlocks and game-mode event consequences still separate; the scoring checkpoint below now connects core stunt accounting.

`002d9870` also accepts digital Up, Down, Left and Right, in that priority, when
the analog stick's squared magnitude does not exceed 0.25. Analog input takes
priority above that threshold, with vertical priority outside ±0.25. The core
now exposes this fallback through `direction_with_buttons` and
`Animator::step_controls_with_buttons`; browser bindings still determine how
physical buttons enter that path. Stunt main and hold transitions now honor the
same force/suppress snapshot flags as normal riding. Suppressing a snapshot
leaves the previous mode unchanged so the transition can occur on a later frame.

Two original forced actions were missing from the 76-clip browser selection:
`LandHardF` and `LandHard`. Exact-build loader `002d6cc0` binds these in that order;
`002daa50` selects id 0 or 1. `001e8690` requests the front action when the local
collision velocity correction Z is below `-LongitudinalHardLandingFps`, otherwise
the vertical action when Y exceeds `VerticalHardLandingFps`. The vertical
threshold includes `TweakVerticalHardLandingFps` times the profile stability
upgrade, as loaded by `001e3b80`. These branches follow all impact-wreck branches;
later angle-wreck checks can still supersede the action. Body velocity and a
simple wheel-contact edge are not substitutes for this collision correction.

The core `LandingConfig` recovers those strict thresholds and impact precedence.
The animator now supports native mode 6: a forced landing action preempts the
stunt pose and pauses both stunt clocks while input acceptance continues. The
shared phase resets on an accepted mode snapshot, then advances by
`dt * (1 / duration)` even on that first frame. Repeated action requests replace
the action id without restarting its phase. It samples the terminal pose before
returning to normal/stunt animation on the next frame. Reset clears the action.
This host checkpoint does not itself establish browser integration or deployment.

Verification uses the exact SCUS_972.11 instructions with finite binary32 math,
branch delay slots and hooked external callbacks. The new private oracle covered
15 main/hold/landing snapshot cases (2,179 instructions), 128 analog/digital
boundary combinations (5,166 instructions) and 300 impact-wreck/hard-landing
branch cases (28,514 instructions), with no decision mismatches. Five mode-6
snapshot/phase/weight cases are also pinned bit-for-bit in Rust regression tests.
A real-resource host probe produced 4,195 finite original male rider poses while
exercising all 24 main → hold → release → completion sequences and both landing
actions. Dedicated tests cover paused stunt clocks, repeated landing requests,
action completion/reentry, reset, snapshot suppression and force flags. These
checks do not establish whole-game scoring, hand/foot IK, RNG ordering, PS2
hardware math, female runtime integration or full animation parity. Private
reports and probe sources remain in `private/reports/` and `work/`.

## Stunt scoring and chaining (host checkpoint)

`stunt_score` now implements the recovered single-rider event accounting from
`0024d5c0`, `0024da00`, `0024db30`, `0024dcb0`, `0024dd00` and `0024d690`.
Base points are 500 for the first eight tricks, 1,000 for the next four, 1,500
for the next eight and 2,000 for the last four. Held time adds 750 points per
second before the per-trick repetition weight. Each accepted repeat uses the
previous weight, then multiplies that trick's future weight by 0.85, with a
minimum of `1 / base_points`. Original mode 7 bypasses repeat decay; this is
not the Freestyle mode.

The exact pause-label selector `002de690` proves settings mode 11 is
Freestyle/Freeride: that case chooses “Restart Freeride” when settings+0 is 1
and “Restart Freestyle” otherwise. The gameplay initializer `0020f780` maps
mode 11 to scoring rules type 3; its timed/untimed session variant also depends
on settings+0. The browser's existing route does not establish that complete
session variant, timer, objectives or original menu callback behavior. Its
stunt scorer uses mode 11, which retains repeat penalties.

The scorer distinguishes pending event points from committed points. Hold
updates floor event points to a one-point quantum, sum valid event records in
original newest-first order and floor the pending sum. Successful chain
finalization sums records, multiplies by the number of recorded tricks and adds
the result to committed points; a wreck discards the chain. The original
constructor `00247570` allocates 100 shared event records and uses a global
one-point quantum. This implementation preserves that capacity for one rider;
a multiplayer port must share the original pool across participants. Virtual
mode overrides and HORSE comparison/progression remain separate.

The rider callback scheduler preserves the original contact grace: the landing
timer starts at 0.5 seconds; sustained wheel contact decrements it and commits
the chain on the following frame after it expires. A new accepted stunt while
the timer is below 0.43 commits the old chain first; a faster touch-and-go can
retain it. Main-clip completion closes only the current record. A new stunt
cannot interrupt the previous main clip until its existing `MIN_TIME` gate
passes. `002daa90` aborts the current record and chain on wreck/reset. Actor callback
`001eaad0` checkpoints all 24 repetition weights after a successful chain and
restores that checkpoint on failure; an unsuccessful trick does not permanently
reduce its future points. Committed points survive ATV reset. Starting a new
event clears score and repetition history separately.

The animator exposes `score_frame`, a drainable authored notification bridge,
`abort_score_chain` and `reset_score_session`. It ticks scoring once before its
rider control callbacks, including the digital-direction entry point. The
original game-rules update `00248020` calls `0024dd00` before its child-entity
traversal; the exact scene sibling/parent attachment order remains unverified.
The HUD's direct vehicle landing callbacks can occur before delayed score
commit, so score-record closure is not assumed to be identical to HUD fading.

A private instruction-interpreter comparison executes 2,147 operations across
60 chains and 162,426 original instructions, covering event allocation, held
updates, record success/failure, chain commit/abort, linked-list recycling and
repeat decay. Rust matches every captured state bit-for-bit. External memory
clear and HUD name dispatch are hooked; the comparison now executes the actual
`001eaa70` pending-score cache, `001eaad0` repetition checkpoint and `001ead10`
current-record callbacks. HORSE overrides and PS2 hardware FPU equivalence are
not established. Seven dedicated scorer tests and two additional animator tests
cover contact grace, completion, abort, repeat floors, capacity, both update
entry points, rejected early chaining and session versus ATV reset. The
private oracle and results remain in `work/stunt-animation-probe/` and
`private/reports/stunt-score-oracle.json`.


## Gated two-link hand solver (host checkpoint)

The rider constructor `002d8810` builds two `002b7070` solvers from `HandL` and
`HandR`, following each hand's parent and grandparent. The target operation
`002b7a60` is synchronous: it uses projected two-link lengths and a signed hinge
offset to replace elbow rotation, then rotates the shoulder toward the target.
It clamps unreachable target distances and retains original hand joint offsets.
`rider_ik` now reconstructs this path; `Animator::configure_hand_ik` prepares it
and `pose_with_hand_targets` accepts explicit targets in the sampled pose frame.
The mirrored local hinge axis remains supplied separately, as in the native
constructor; its BSS initializer is not yet verified.

The recovered normal, stunt, hold and forced-landing selectors all clear the
rider's hand-target gate at +0x41c. The port therefore keeps normal hand IK
disabled; constructing a solver does not force hands onto handlebars or prevent
stunt poses from releasing them. First-person arm constructors `00313680` and
`00313ca0` also use this solver, but their complete runtime controller remains
separate. The seven-node constraint path in `002d6380`/`002d9a20` is the female
`HairParent`/`Hair01`…`Hair07` chain, not a foot or arm IK system. No original
foot-target path has been verified here.

A private original-instruction probe runs 120 synthetic rigid-arm targets through
`002b7070`/`002b7a60` (98,760 instructions). Scalar/VU solver math executes from
the exact executable; world-transform, matrix-setter and trigonometry-library
helpers are hooked. Rust's 5,760 resulting matrix values differ by at most
0.00000188 from those captured values. A native fixture is pinned in a Rust
regression test. A separate real-resource check covers 1,519 poses from 49 male
riding/stunt/hold clips: disabling targets preserves every matrix; enabling
explicit synthetic hinge/target inputs produces finite poses. The worst
endpoint error in that latter check is 0.02244 original world units, and its
hinge axis is not established as the original runtime axis. These checks verify
a bounded solver component, not hand-contact parity or PS2 hardware equivalence.
Private probe evidence is retained under `private/reports/hand-ik-*`.
