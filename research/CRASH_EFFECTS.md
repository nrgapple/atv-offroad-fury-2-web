# Optional crash effects strategy

## Scope and baseline

Build funny crash presentation as an optional authored effects layer on top of
the recovered wreck and recovery flow. It may react to real wreck/separation
events, but it must not choose whether a wreck occurs or change the original
vehicle solver, rider detach/recovery timing, checkpoints, or reset behavior.
The recovered game remains the comparison baseline. At the user’s request, the
effects option is now on by default; Off restores the original crash presentation.
The setting is persisted through the existing settings path, retaining a saved Off.

## Effects model

Use a constrained articulated ragdoll for presentation. Keep its pose and
simulation bounded: finite state, capped time steps and velocities, limited
joint motion, and a deterministic upper bound on active bodies/iterations.
Terrain response may add a small, authored bounce for readable slapstick, but
must not feed forces back into the recovered gameplay bodies or delay native
recovery. When the effect ends, discard its state and let the existing recovery
sequence proceed unchanged.

ATV breakaway pieces should be selected from parts already present in the
authored vehicle model. Spawn only supported, separately renderable pieces at
the crash event, with bounded lifetime and motion. Do not invent geometry or
publish source-disc material. Missing or unsupported model parts should simply
remain attached; effects must tolerate incomplete model coverage.

The current implementation uses a 120 Hz constrained Verlet layer with bounded
joint angle ranges, terrain-only contacts, at most 32 source rider nodes, and a
cap of eight debris pieces. The toggle is persisted as `atv-funny-crashes` and
defaults to On, while preserving an existing saved Off. Only the local player’s
crash receives the presentation layer;
remote players keep their native poses. The source ATV chassis is a monolithic
mesh, so visual breakaway is limited to authored mechanical pieces rather than
arbitrary chassis or fender fracture.

Verified ordinary Nac Nac failure captures show the rider separating with two
front wheels. This is not evidence for every breakaway threshold or vehicle.
The damage thresholds now use the inward contact-point speed at an actual ATV
terrain/suspension or obstacle contact, not travel speed or rider separation.
At least 50 ft/s selects four wheels, handlebars, swingarm, and rear axle;
at least 25 ft/s selects the two front wheels. Airborne bailouts still start the
rider ragdoll but leave the ATV intact. Later contact during the wreck can start
or escalate breakaway damage; an already detached piece is never spawned twice. The severe path now passes an in-game host gate: normal
throttle and held stunt inputs through the existing WASM interface produce a
severe wreck that sheds all seven pieces, keeps rendered GPU arrays finite,
and reaches the original intact recovery. No velocity, pose, or wreck state is
injected. The supplied severe desktop capture is obscured by branches and is
not suitable for the dev log.

## Controls and compatibility

Expose a clearly named Funny Crashes option, initially on. Persist it using
the current settings mechanism. Toggling it must not reset the ride, alter
input, or change crash classification. With the option off, no ragdoll or
breakaway simulation runs and baseline wreck behavior remains equivalent.

## Acceptance checks

- Compare the same crash/recovery trace with the option off against the
  pre-effects baseline; wreck classification, vehicle/rider gameplay state,
  recovery timing, and reset result must be unchanged.
- Exercise the option on and off, including reload persistence and mid-session
  toggles. The off path must create no effect bodies.
- Bound simulation work and verify finite transforms through repeated impacts,
  terrain bounces, pause/resume, and recovery. Effects must expire or clear
  without trapping the player or extending recovery.
- Audit supported vehicle models for separately renderable parts; record
  coverage and unsupported cases rather than implying universal breakaway.
- Inspect desktop and 390px browser captures for readable framing, no runaway
  debris, and correct settings presentation. Label host and public evidence
  separately; a host result is not a public release check.

At this host checkpoint, paired mirror-WASM traces compare gameplay, camera,
water, train, and wheel fields after excluding the effects payload and echoed
toggle. They cover 415 desktop, 430 female-rider mobile, and 762 severe-impact
steps with zero mismatches. The ordinary touch Nac Nac failure was exercised with the male
ATV01 rider at 1280px and the female ATV26 rider at 390px: rider ragdoll and
two-front-wheel debris appeared, pause froze the scene, toggling Off mid-wreck
resumed the baseline draw path, toggling On again still allowed automatic
recovery, and driving resumed. `make check` passed (312 core, 3 web, workspace
net/relay, and 68 JavaScript tests); `make verify` passed. Detailed output is in
the host's `work/check-final.txt`.

## Evidence limits

This is a presentation enhancement, not recovered ATV2 crash behavior. The
existing wreck notes document the reconstructed native separation and recovery
path; they do not establish that the original game used a ragdoll or detachable
vehicle parts. Preserve that distinction in implementation notes and the dev
log. Original-game replay parity is not implied by these acceptance checks.
This checkpoint is host-only. The ordinary-failure screenshots do not show the
full severe-impact set, and the supplied severe capture is obscured by branches.
No public deployment or public browser check is claimed here.

## Impact gating follow-up (October 12, 2026)

The user requested default On and physical-impact timing for mechanical damage.
`ContactCorrections.closing_speed` observes the inward normal point speed only
for a penetrating contact. The vehicle also observes grounded suspension and
resolved travel-stop contacts; `crash_impact_speed` is the largest current-step
value, including secondary moving-object contacts. The observation never feeds
back into native forces or wreck classification, and is cleared on reset and
each step. Delayed native corrections do not replay the previous observation.

Unit gates cover high-speed airborne bailouts with no parts lost, soft tangential
landings, separating/no-penetration contacts, hard ground and horizontal obstacle
contacts, later damage escalation and no duplicate parts. Host severe source
inputs produce three aerial rider-ragdoll frames with intact ATV, then two parts
at a 27.420 ft/s obstacle contact and seven at a later 50.855 ft/s contact, GPU
draws and intact recovery. The 1,199 exact mirrored WASM steps have zero compared
gameplay/camera mismatches. Host touch ATV26/female passes at 390px, including
pause, mid-wreck Off/On and recovery; 439 paired steps have zero mismatches.
Explicit original Off passes the touch crash/recovery route. Fresh default On,
saved Off/On after reload, and unavailable-storage defaults/session toggles pass
at 844×390. These follow-up results are host verification, pending public release;
earlier captures and evidence above describe the original default-Off checkpoint.
