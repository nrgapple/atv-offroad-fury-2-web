# Racing reconstruction strategy — 2026-10-12

The [USA manual](https://www.videogamemanual.com/PS2/ATV%20Offroad%20Fury%202%20%28USA%29.pdf) (printed p.10) identifies Place, Lap, Lap Time, Best, Total, Lead, radar and position as race HUD information. The current Freestyle route omits racing. The exact USA v2.00 National archive contains an editor-tagged TDF course, a race-specific TOY radar rectangle, and AIRiders.pb identities.

Implement an explicitly marked race preview using original National terrain/scenery and recovered course points, with authored Rust countdown, ordered checkpoint validation, laps, timing, rank and results. Original CPU decisions, rider interactions and vehicle-to-vehicle collisions are not recovered. The preview CPUs use deterministic route motion with terrain-following poses and distinct speeds, not recovered physical AI. Keep Freestyle physics unchanged; do not call this the original race implementation.

TDF observations: leading byte tags 1(version), 2/3(world offsets), 4(scale); route marker 8 followed by 23(flags), 9(center), 10/11(edges), 12,18,26,27 scalars. Coordinates are editor-local and transform by offset + scale × point. The route closes with its first point repeated; tag13 starts the trailing start-grid section. The parser consumes/validates the stream including that tail, but does not interpret unknown gameplay flags as race rules. Synthetic tests plus private real-resource probes establish decoding consistency, not native instruction parity.

Host browser gates cover desktop, portrait and phone landscape, pause, rotation, countdown input lock, CPU motion, restart and Freestyle regression. Finish/shortcut rules receive Rust tests. Publication is separate; the user requested no deployment waits. Original resources and screenshots remain ignored and outside GitHub.

## Completed host gate

`make check` passes 305 core tests, strict Clippy, formatting, 47 JavaScript tests
and Python syntax. `make verify`, 8 release tests, 5 content tests and 4 Worker
tests pass. Native validation used a temporary nonincremental cache after SSD
hard-link/clonefile calls stalled; that cache was removed after the gate. WASM,
content packages and screenshots remain on the SSD/private storage paths.

`tools/test_race.mjs` passes 1280×720, 390×844 and 844×390 setup/HUD bounds,
44px targets, input-driven player acceleration, one-finger steering/cancellation,
five CPU poses and advancing distances, countdown input lock, pause, restart,
no disc selector and zero JS/GL errors. Results bounds/actions use a clearly
separate render-only fixture; synthetic Rust tests validate final-lap time,
best lap, CPU completion, finish ordering and ordered-checkpoint shortcut gates.
No complete Caribou lap has been claimed as a browser replay. The existing
`tools/test_frontend.mjs` Freestyle/controller/portrait regression and dev-log
filter checks pass on this host. WebKit startup stalled and its run was stopped;
physical phones, Safari and Bluetooth hardware remain unverified.

The National runtime archive and selected screenshot are pending the validated
current-main R2 release procedure. The user requested no deployment waits.
Race availability follows the content manifest so an older public package
continues exposing working Freestyle until its National resource is published.

## Integration with current main

The race branch integrates `c895ba4` (including multiplayer, all-model selection,
Vancouver, optional crash effects, fog and vegetation). Race ABI operations now
use 95/96, preserving the multiplayer 80–85 range. The existing RawValue batch
transport remains intact; an optional race field accompanies race batches and
is omitted in Freestyle. Race start leaves an active multiplayer session.

The combined native gate passes 325 core, 25 protocol, 3 relay and 13 bridge
tests, strict Clippy, formatting, 69 JavaScript tests and Python syntax. The
combined WASM build passes; 18 release/content and 6 Worker tests pass. Temporary
native/WASM build caches avoided the host SSD clone stalls; browser artifacts
were copied back to the existing SSD output. Production remains unverified.

Final combined Chromium gates pass race play and all three layouts, actual
one-finger steering/cancellation, player acceleration, countdown/pause/restart,
radar and CPU motion, plus the separate results fixture. The existing frontend
regression passes nine-map selection, Tioga driving, keyboard/controller,
portrait Caribou and pause/Controls. The menu-model regression passes six
viewport sizes, all 26 quad inspections, female rider views, idle/music and
selected-model course loading without JS/GL errors. These are host checks.

## Verified public race release

Public HTTPS `https://atv-offroad-fury-2.champtoday.workers.dev/` serves
`af3e6b5a7ac3fc4095477326597a27cfe84b97ce`, CI 38333554805 successful.
The R2 manifest now has 108 records including the National archive. Five changed
objects were uploaded; HTTPS checks passed 33 authored files and 416 served
runtime/gallery objects. The public log has 129 entries and 108 approved images
at this verification point. The selected race picture is an inspected host
capture, not a public screenshot or a PS2 parity comparison.

The public race gate passes no-disc startup, all three viewports, five named
CPU actors, actual player acceleration, one-finger steering/cancellation,
countdown, pause, restart, radar and results render-only fixture controls.
Sampled CPU distances are 70.53–80.96 feet; no JS/GL errors occur. Race CPU
motion remains authored and provisional; original CPU physics/passing/collisions
and whole-course browser replay equivalence remain unverified.

## Original CPU driving reconstruction — next implementation gate

The exact USA v2.00 executable now supplies the driving strategy. Private
Ghidra exports retain matching source/load bytes for VehicleAI mode dispatch
0024ee40, path driver 0024f9d0, constructor 0024ef20, race creation 001f9690,
leader speed policy 002536b0, and Track loader/interpolation 00179680,
0017d810/0017d980. This is original-instruction evidence, not recovered source
code or whole-game equivalence. The stripped sequel remains the authority.

Replace constant-speed CPU position updates with independent recovered quad
physics. Supply recovered driver decisions: lookahead (25 + 4 × route error,
limited to 60ft on track), lateral target variation/slew with the track's 3ft
limit, tangent plus 0.05 target correction, signed heading gain 5 and sine
saturation, and throttle/brake with a 1ft/s deadband. TDF tag18 stores target
speed (sentinel -1 means 150ft/s), tag23 point flags, and tag40 lateral bound.
Tags19–22 provide leader-distance/speed factors. Retain explicit limits for
unconnected native obstacle-avoidance, trick choice, race transitions, roster
cosmetics and full-course parity. Reusing the recovered vehicle solver does
not make these missing AI decisions original.

Validate scalar decision branches against bounded original-byte instruction
probes; private content replay then covers independent acceleration, turning,
suspension, wheel/rider animation, terrain response, pause/restart and phone
layouts. Source pseudocode/instructions, private probes and images stay ignored.

The National baseline uses the recovered 60ft/s reference floor and track
leader policy. The preview's arbitrary Easy/Normal/Hard speed labels are removed;
original difficulty/mode mapping remains a separate recovery gate. CPU selection
now tops out at four opponents, consistent with the five-slot AIRiders selection
loop in 0021f680. The provisional starting row uses Caribou's authored 8ft rider
spacing and leaves the player lane clear; native start-marker/slot placement is
still unverified. Original roster/quad diversity and costumes remain open.

Private finite-binary32 probes of 002536b0, 002509ac and 0024fe4c execute
124,410 original instructions across 1,500 cases with zero differing output
bits for leader speed, throttle/brake deadband and lateral slew/clamp. These
probes do not prove route localization, whole-driver behavior, PS2 trigonometric
tables, obstacle avoidance or complete race replay equivalence. Route projection
and ground/contact mapping are integration adaptations; trigonometry uses host
Rust sine/atan2. CPU wreck/recovery, rider collisions, AI tricks, low-clearance
air feedback, dynamic obstacles and separate quad tuning remain unconnected.

The source grid helper 001f8410 has now been exported from the matching executable.
National event cases 3–10 use event start nodes, slot ordering, rider spacing and
a turn-side sign derived from neighboring tangents. Connecting those start
records is a separate gate; the current symmetric row is an integration fallback.
Race distance now interpolates along the ordered checkpoint segment rather than
using a global wrapped projection. This fixes the start-line reverse-drift case
that incorrectly disabled the original ahead-only speed policy. Checkpoint radii
and event progression remain provisional rather than native RaceStatus parity.

Host integration passes 330 core, 25 protocol, 3 relay, 13 bridge and 69 JS tests,
formatting/strict Clippy, WASM, setup/source and repository hygiene, 18 release
checks and 6 Worker tests. Race setup/driving passes desktop 1280×720, portrait
390×844 and landscape 844×390, rotation, countdown, touch cancellation, pause
and restart. Normal driving publishes 27 quad/18 rider transforms per CPU,
rolling wheels and independent suspension. Frontend/Freestyle regression passes.
The separate results fixture is render-only, not a complete race replay.

The sustained browser gate passes 45.05 simulated seconds, 2,435–2,466ft per
CPU, 64–69 observed braking decisions, 165–186 coasting/braking observations,
809–814 ahead-speed limits and changing elevations. Maximum observed centerline
error is 22.0–24.5ft; this is not whole-course adherence. CPU matrices reach 52
GPU draw calls when the field is in rendering range. Distant CPUs are culled,
not simulated through a constant-speed shortcut. Pause freezes the entire
published frame and no browser exceptions occur. This remains a host checkpoint
until its tested commit passes main CI and public deployment verification.

The second main integration includes the separately authored motorcycle mode.
Its existing CPU route preview is preserved through an explicit motorcycle-only
step; the original ATV branch never invokes that kinematic path. Physical ATV
CPUs still use Vehicle steps and observations. Quad and motorcycle 34 race gates
pass all three layouts, pause/restart/input/results fixture and no browser errors.
The merged gate passes 339 core, 25 protocol, 3 relay, 13 bridge, 4 release-tool
Rust and 69 JS tests, strict Clippy/formatting, WASM, 29 release tests (one skipped)
and 8 Worker tests.

A separate untouched-physics CPU run reaches the current one-lap finish radius
for all four ATV racers by 120.061 simulated seconds. Traveled distances are
6,746.55–6,761.15ft versus a 6,811.33ft decoded closed route. The finish radius
awards the lap before exact start/finish crossing; original TDF marker placement
and native finish-line rules remain open. This verifies current preview route
completion, not original event/lap or PS2 replay parity.

## CPU avoidance and wreck strategy — active gate

Use the exact SCUS_972.11 National call chain 0024f9d0 → 0024f840 →
00252080. Port ordinary peer near/far relative-velocity correction and eight-ray
static obstacle direction attenuation before steering; validate the peer loop
and attenuation branches against bounded original instructions. Retain query
category/order, VU precision and special aggression flags as separately scoped
gates until they are reproduced. Do not add guessed braking or randomized crashes.

Enable each CPU's recovered Vehicle wreck decision/passive-body path and its
own cloned original wreck clips/detached-rider controller. Reuse the player
ordering: vehicle step, wreck begin/body step, rider-contact recovery clock,
then publish the appropriate independent rider world/pose. Feed live water and
scene contacts without advancing shared water or train actors once per CPU.
Recovery must resume route driving with refreshed driver state and retain
ordered race progression, without awarding checkpoints for a reposition.
Verify natural racing, peer/obstacle scenarios, natural impacts/rollovers,
recovery and both phone orientations; injected wreck fixtures must stay labeled.
