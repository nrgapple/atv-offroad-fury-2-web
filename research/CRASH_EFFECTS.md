# Optional crash effects strategy

## Scope and baseline

Build funny crash presentation as an optional authored effects layer on top of
the recovered wreck and recovery flow. It may react to real wreck/separation
events, but it must not choose whether a wreck occurs or change the original
vehicle solver, rider detach/recovery timing, checkpoints, or reset behavior.
The ordinary game remains the baseline, with the effects option off by default
and its setting persisted through the existing settings path.

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
defaults to Off. Only the local player's crash receives the presentation layer;
remote players keep their native poses. The source ATV chassis is a monolithic
mesh, so visual breakaway is limited to authored mechanical pieces rather than
arbitrary chassis or fender fracture.

Motorcycle resource recovery adds support for Unleashed's `TireFront/TireRear`
markers. Their visible tire children and handlebar attachments inherit the
nearest detached part's transformed pose; a separately detached rear wheel
inside a detached swingarm retains its own motion. Synthetic hierarchy tests
cover four severe-crash parts, child transforms, unchanged frame geometry and
reset/Off. The existing ATV browser gate still passes 855 paired gameplay/camera
steps and seven visible pieces. This is compatible presentation groundwork;
playable motorcycle handling, rider and crash integration remain unfinished.

Verified ordinary Nac Nac failure captures show the rider separating with two
front wheels. This is not evidence for every breakaway threshold or vehicle.
The configured severe-impact path (at least 50 ft/s) selects four wheels,
handlebars, swingarm, and rear axle; hard impact (at least 25 ft/s) selects the
two front wheels. The severe path now passes an in-game host gate: normal
throttle and held stunt inputs through the existing WASM interface produce a
severe wreck that sheds all seven pieces, keeps rendered GPU arrays finite,
and reaches the original intact recovery. No velocity, pose, or wreck state is
injected. The supplied severe desktop capture is obscured by branches and is
not suitable for the dev log.

## Controls and compatibility

Expose a clearly named Funny Crashes option, initially off. Persist it using
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
