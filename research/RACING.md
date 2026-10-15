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

## CPU avoidance and wreck strategy — completed host gate

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

The authored CPU avoidance now feeds the original near/far relative-velocity
response, eight midpoint rays over pi/4, 100ft ray attenuation, and horizontal
packed-coordinate vegetation response into the heading wrapper. Ordinary plants
use the distinct original gains 1/200; peers use 0.65/100. The spatial query uses
50ft half extents. No braking policy is inferred from corrected vector magnitude:
the original wrapper normalizes heading while retaining TDF/leader target speed.
Scene eligibility, native trig/VU precision and linked-actor update order remain
integration limitations.

Each CPU enables the recovered physical wreck decision/passive quad path and
owns its original wreck clips, detached rider body, contact-gated recovery clock,
normal/landing/stunt controller, and rider world matrix. It uses live water and
current dynamic scene transforms without advancing shared actors again. Driving
controls stop during wrecks; recovery refreshes route state and resumes driving.
A reset publishes the new physical pose without calling race checkpoint update
or counting teleport distance; subsequent normal movement can earn progress.

Low-air feedback uses the source 20ft split, terrain normal sampled 0.5 seconds
ahead, gains 10/0.1 and bounded lean. National trick requests now use the original
10-second interval/10% trial, flagged jumps, TDF tag 26 airtime minus elapsed air
time and 0.5-second margin, and selection among the 24 eligible main clip durations
(001e9650). The rider accepts the direct request through its native airborne,
speed/reset/minimum-time gates and runs the loaded original stunt clip without
synthetic held buttons. The original flag 64 preload/jump helper, optional AI
flags, global random stream and exact actor/controller update ordering are still
separate gates.

Private no-hook instruction comparisons pass 9,000 cases executing 1,184,803
original instructions with zero differing binary32 outputs: 3,000 peer cases,
3,000 plant cases, 1,500 ray attenuation cases and 1,500 low-air feedback cases.
These bounded branches use the existing finite-binary32 VU approximation; they
do not establish PS2 hardware math or complete National AI replay equivalence.

The combined main integration passes 346 core, 25 protocol, 3 relay, 13 bridge,
4 release-tool Rust and 69 JS tests, formatting/strict Clippy, WASM, source/setup
and repository hygiene, 29 release checks (one skipped) and 8 Worker tests.
The real-content race gate passes ATV and motorcycle 34, desktop/portrait/landscape,
countdown/input/pause/restart and the separate render-only results fixture.
Held-drive Pause/Camera passes both phone orientations; UI loading/transitions,
reduced motion, rotation and touch targets pass. Freestyle/controller/portrait
regression passes as well.

An untouched-physics 65.08-second race observes 3,905 actual worker frames per CPU,
5 natural wrecks and 5 recoveries across three riders, detached body contacts,
neutral wreck controls, resumed driving and 8 original stunt starts. All four CPUs
encounter peers, plants and real scenery rays and travel 3,180.7–3,728.7ft; matrices
stay finite, reset distance/lap awards are absent, pause freezes the full frame
in both phone orientations and browser errors are absent. This is a sustained
host behavior gate, not a complete lap or original finish-line replay. The
checkpoint remains host-only until its validated main artifact passes public
release verification. Rider-to-rider collision impulses, roster-specific tuning,
original start slots, race progression and finish rules remain open.

## All 32 original race courses — 2026-10-14

The disc ships a race archive for every Environment, National and Supercross venue:
`E0n_NAT.RES` (8), `N0n_NAT.RES` (8) and `S0n_NAT.RES` (16). Each contains the same
members as Caribou: `<venue>natE.tdf`, `AIRiders.pb`, `<venue>nat.toy` (radar,
championship points, `RiderSpacing`, Supercross fireworks), scene and terrain.
Only Caribou was connected because the Rust TDF reader was fitted to its single
closed loop and rejected the other 31.

Exact-build Track loader `00179680` reads the TDF as a section graph:

| Tag | Meaning in the loader |
| --- | --- |
| 1 | Section count (allocates the section table); not a version. Caribou 1, `env02` 9, `sx03` 17. |
| 5 / 6 / 7 / 24 | Section id, flags (loader ORs `0xc1`), editor length, extra word. |
| 8, 23, 9–11, 12, 18, 26, 27 | Route point and its fields, as before. Tag 12 is read and discarded. |
| 13 / 14 | Section link count, then (slot, target section) links. |
| 15 / 16 / 25 | Oriented boxes at Track +0x54/+0x58/+0x5c: center, forward, half-width vector, width. |
| 19–22, 40, 41 | Leader policy, lane limit, header word. |

Evidence for the box roles: on every course tag 15 lies at the start (section 0 or
the grid) and tag 16 on the lap circuit. `sx01.scn` `DefaultStartPosition`
(123.9, 90, 104.7) matches the tag 15 center (123, 89). In `sx01nat.toy`,
`FireworkTrigger2` (the 140ft start-gate trigger) matches tag 15's position and
direction, and the `RaceOrderMatters` `FireworkTrigger1` matches tag 16. The port
therefore treats 15 as the start gate and 16 as the finish gate. The consumers of
Track +0x54/+0x58 have not been traced, so exact gate tests remain provisional.

Sections flagged `0x100` are the parallel alternate line at each fork; the other
link is the main line. The reader follows the main line from section 0 until it
closes, giving an optional start straight (Nationals/Supercross) plus the lap
circuit. Riders start 4.66ft behind the gate (the native 001f8410 grid fallback)
at the course `RiderSpacing` (8–15ft from the race TOY; Supercross 10ft from the
scene TOY). A lap is completed at the checkpoint after the finish gate; when the
gate sits just ahead of the grid (<25% of a lap), its first crossing starts the
race instead. Riders on an alternate line are credited in order when they rejoin.

Host checks: all 32 TDFs parse; a scripted rider completes a two-lap race on every
course, with the travelled distance matching the decoded first-lap/lap lengths
(1,748–9,974ft laps, up to five forks). `tools/test_race_courses.mjs` starts every
course in Chromium with four physical CPUs and checks grid spacing, race direction,
progress, the course menu in three layouts and page/GL errors.

Limitations: CPU corner speed is not original on courses whose TDF speeds are all
the 150ft/s sentinel (e.g. `env02`, `nat01`). The recovered driver has no corner
braking and the project does not guess one, so CPUs can overshoot early corners
and use the existing wreck/recovery path. Supercross firework streams beyond the
ambient GPU packet budget are reported as unsupported instead of failing the
load; firework triggers, championship points and original finish-line box tests
remain unconnected. Race resources need the updated R2 content package.

## Track progress, off-track, recovery and rider contact — 2026-10-15

Exact-build RaceStatus (`RaceStatus.cpp`; per-frame update `00248b20`, reset
`00247900`) runs in track mode against the TDF graph:

- `0017be80` keeps each racer on a track node. It accepts only segments whose
  authored edge polygon (tag 10/11 points of consecutive nodes, tested by
  `0017bb50`) contains the rider, within a forward/backward route window
  (Track +0x34/+0x38). Fork links admit alternate (`0x100`) sections.
- When no segment qualifies, the racer is off track: progress stays on the last
  node and RaceStatus accumulates off-track time (+0xf0). It also computes a
  vector from the rider back to the track (+0x1a0).
- A lap counts on a forward crossing of the tag 16 finish box (`001a95b0`)
  only when the rider's tracked distance is near the finish.
- Every update passes the racer's track position and direction to its vehicle
  interface (callback +0x20).
- Race creation `001f9690` sets the CPU AI flags and constants already used by
  the port. `0x800` (direct velocity) depends on a game option.

The port now follows this model. It replaces the radius checkpoints, which
riders could miss and which stalled their progress:

- `Course` keeps the authored edge pairs and every alternate polyline.
- `Course::locate` accepts a rider only inside an edge polygon within 150ft
  ahead or 60ft behind its station. These window values are authored; the
  original Track +0x34/+0x38 values are unrecovered.
- Progress is the race distance along the racing line, so laps count when the
  furthest distance passes the finish gate. Cutting across the infield,
  teleporting or reversing cannot skip or re-count the course. Alternate lines
  map onto the main line between their fork and rejoin.
- The HUD shows OFF TRACK, with an arrow and distance back to the racing line.
- During a race, Reset returns the player to that point facing the race
  direction; Pause → Restart Race still restarts the event.
- Each racer's vehicle reset checkpoint is its on-track station, facing the
  racing line. Wreck recovery and deep-water resets in races therefore return
  to the course instead of the crash site.
- CPU steering projects only within the route window around its own race
  station. Global nearest-segment projection let nat01's lap, which runs beside
  its start straight, capture CPUs at the first corner and send them away from
  the course. This was the cause of the earlier nat01/env02 overshoots.
- Authored recovery: a riding CPU that stays off the track polygon for 5s, or
  below 3ft/s for 4s, returns to its race checkpoint. The original consumer of
  the off-track timer is unrecovered.
- Rider-to-rider contact reuses the authored equal-mass multiplayer proxy
  (`peer_contact`), including the recovered impact wreck classifier. Every pair
  of player and active CPUs resolves from pre-contact poses, and each racer
  applies its own half.

Checks: unit tests cover the polygon window, alternate lines, finish grace and
distance ranking. The private all-courses test drives a two-lap race on all 32
TDFs, plus each of the 60 alternate lines from fork to rejoin, without leaving
the track. In host Chromium, nat01 CPUs now take the first corner (route error
stays at 0–20ft for the leading CPU). Twenty-second races on env01, nat01, env02
and sx03 resolve 82–132 rider contacts, with CPU route error ≤24ft.
`tools/test_race_courses.mjs` also drives off track and checks the OFF TRACK
HUD, then checks that Reset returns to the racing line with the race running.

Limitations: the original window values and finish-box test are not yet ported
exactly. The CPU off-track and stuck recovery policy is authored, and rider
contact is the authored multiplayer proxy, not a recovered PS2 collision model.
