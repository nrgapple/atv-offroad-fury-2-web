# Racing reconstruction

The [USA manual](https://www.videogamemanual.com/PS2/ATV%20Offroad%20Fury%202%20%28USA%29.pdf) (printed p.10) identifies Place, Lap, Lap Time, Best, Total, Lead, radar and position as race HUD information. Each exact USA v2.00 race archive contains an editor-tagged TDF course, a race-specific TOY (radar rectangle, rider spacing, championship points) and AIRiders.pb identities.

## Current status

Race mode runs all 32 original Environment, National and Supercross courses with
up to four CPU opponents on the shared Vehicle solver. It is a reconstruction
with recovered driver decisions inside authored race rules, not the original
race implementation. Later sections are dated history; this summary is current.

**Recovered** (exact-build SCUS_972.11 evidence; bounded instruction probes
for the scalar branches, not whole-race replay):

- TDF section graph, route points, target speeds (tag 18; `-1` = 150ft/s),
  leader-policy factors (tags 19–22), lane limit (tag 40) and start/finish boxes
  (tags 15/16) from Track loader `00179680`.
- CPU path-driver decisions from `0024f9d0`: lookahead 25 + 4 × route error
  (60ft cap on track), lateral target and slew/clamp (`0024fe4c`), heading gain
  and sine saturation, 1ft/s throttle/brake deadband (`002509ac`), and the
  ahead-only leader speed policy with its 60ft/s floor (`002536b0`).
- Direction avoidance `0024f840` → `00252080`: peer near/far relative velocity,
  eight-ray static attenuation, vegetation response and the track-edge tail
  (AI flag 0x400000). The 0x10 response scale reads the aim node.
- Difficulty: race creation `001f9690` sets AI flag 0x400 (the leader speed
  limit) for every difficulty except Expert. Race setup offers Normal/Expert.
- Jump hook `0024ebb0` and preload release `001ebb10` (flag 0x40 jumps), the
  grounded flag-8 lean, and the EATDIRT/BRINGIT cheat branches (AI flags
  0x4000000 aggression, 0x800 velocity pull `001ebaf0`), link-only.
- Corner speed: the original has no corner braking. On courses whose TDF
  speeds are all the 150ft/s sentinel the target stays 150ft/s; only the
  leader limit, throttle/brake deadband and physics slow a CPU.
- Low-air lean feedback and the National trick gate (10-second interval, 10%
  trial, TDF airtime fit, `001e9650` clip eligibility).
- CPU wreck decision, passive body, wreck clips and rider controller reuse the
  player's Vehicle path. Grid fallback 4.66ft behind the start gate (`001f8410`)
  with the course `RiderSpacing`.
- Start slots: the `001f6350` pair shuffle and the `001f8410` race-mode row
  (slot − n/2 spacings across the start, side from section 0's first turn),
  for the player and every CPU. See "Start slots" below.

**Authored** (integration adaptations, not original behavior):

- Race rules: countdown, laps, timing, ranking and HUD. Progress follows
  the racing line inside the TDF edge polygon with an authored window (150ft
  ahead, 60ft behind); the original Track +0x34/+0x38 values and the native
  `001a95b0` finish-box test are not ported.
- Route projection near each racer's station and ground/contact mapping; host
  Rust `sin`/`atan2` instead of PS2 trigonometry/VU precision.
- Per-CPU random streams seeded `97211 + 47 × index` instead of the native
  global random stream; the start-slot shuffle draws from its own stream on
  the race seed.
- Off-track (5 s) and stuck (under 3ft/s for 4 s) CPU recovery to the race
  checkpoint; the original consumer of the off-track timer is unrecovered.
  The stuck clock runs only after GO: counting the braked grid hold reset
  every CPU at GO onto one shared checkpoint, stacked on each other.
- Rider-to-rider contact through the multiplayer `peer_contact` proxy.
- The motorcycle race preview is kinematic route following at fixed speeds; it
  never runs for ATV racers.

**Missing or unverified:**

- Championship-only difficulty effects (see "CPU difficulty" below), the All
  Out AI filters and angular-rate write, scene query category/order and actor
  update ordering.
- Championship points, firework triggers and whole-race PS2 replay parity.
- The Supercross start gate rotation, the race-load
  announcer cue and the Stunt Score and Profile Points results columns. See
  "Race start and race over" and "Pre-race camera and 321 lights" below.

## Initial preview (historical)

TDF observations (first pass; superseded by the section-graph table below, where tag 1 is the section count): leading byte tags 1(version), 2/3(world offsets), 4(scale); route marker 8 followed by 23(flags), 9(center), 10/11(edges), 12,18,26,27 scalars. Coordinates are editor-local and transform by offset + scale × point. The route closes with its first point repeated; tag13 starts the trailing start-grid section. The parser consumes/validates the stream including that tail, but does not interpret unknown gameplay flags as race rules. Synthetic tests plus private real-resource probes establish decoding consistency, not native instruction parity.

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

## Original CPU driving reconstruction

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

## Original CPU roster: quads, riders and outfits — 2026-10-15

The race archives carry `quad\ATVGroups.pb` and `AIRiders.pb`. Exact-build
`00210d20` gathers ATV groups: the course `ChampQuadGroup` from the race TOY
when positive, otherwise the human's ATV group. Groups 5 and 6 fold into 3.
It adds every `GroupN` fallback listed in those groups, pools their ATVs, and
draws each CPU slot's quad at random without replacement, refilling an
exhausted pool. `0021f680` then reads the slot quad's `[QuadN]` `MaleN`/`FemaleN`
list and draws an unused AI rider. AI ids below 22 are male. Each `[AIn]`
section supplies the rider's name, number, and helmet, glove, boot, sleeve,
pant, goggle and shirt materials. Both functions seed libc rand from the RTC.

`race_roster::select` ports this selection; the port seeds it per race.
Integration choices:

- The male/female column follows the player's rider, because the original
  slot flag's initial value is unrecovered and CPUs share the player's rig.
- Each CPU installs its own original quad (op 106): physics, mesh/linkage,
  `ao.col` chassis points with wreck rays, and `ar.col` actor rays. Its
  physical vehicle, handling and collisions follow that ATV.
- Rendering reuses the multiplayer quad/rider model loader. Each rider-mesh
  material is replaced by the AI's outfit material for its slot.
- Per-rider jerseys (`…_shirt_AIn`) are composed at runtime in the original,
  probably with the race number, and do not exist on the disc. The port uses
  the base shirt's mesh-slot variant (`<base>001`); numbers are unconnected.

The content package keeps the outfit materials in the compacted
`RIDER_MT.RES`: 119 materials, 0.3MB → 3.2MB. Motorcycles keep their separate
preview CPUs.

Checks:

- Unit tests cover player-group, course-group, folded-group and female
  selection, unique riders and exhausted-list fallback.
- A private test selects four named, fully outfitted riders for every quad
  1–26, for both sexes, with course groups −1, 1 and 2.
- Host Chromium races show distinct quads and outfits on the grid and while
  racing. On their own quad physics, CPU route error stays ≤13ft over 20s on
  env05, nat01 and sx07, with 127–164 rider contacts.
- `test_race.mjs` and `test_race_courses.mjs` assert the roster names, models
  and outfit textures.

## Race start and race over — 2026-10-16

Exact-build evidence (private exports; bounded reading, not instruction
comparisons):

- **Grid placement `001dd440`.** The grid helper `001f8410` passes a ground
  height; the placement routine keeps the heading horizontal, rolls the up axis
  toward the terrain normal (up = f × (n × f)), raises the origin to the ground
  and then lifts it by the deepest wheel-bottom penetration (wheel position
  minus radius along up). Race resets reach it through `001e35d0`.
- **Race over (RaceStatus `00248b20`).** The race ends once every human has
  finished; the game does not wait for the CPUs. For each unfinished rider
  +0xec = elapsed − leader time × (own distance / leader distance). At race
  over its total becomes the leader's time plus that gap, its last lap
  (gap + lap start time) / laps remaining, its best lap the faster of that and
  any best, capped at total / laps, and it is marked finished. Times are then
  made increasing down the running order by adding a random under one second.
  The Race Results menu opens immediately (`00216260`); after a 3s timer
  (`00215db0`) the end-race routine `001f1990` switches the camera director.
- **Race Results menu** (`Overlays.pb` `[RaceOver Menu]`): Name, Total Time,
  Best Lap, Stunt Score and Profile Points; X Continue, Square Hide/Unhide.
  Continue leads to a Restart / Quit Event menu.

Port:

- ATV racers (player and CPUs) are placed with `Vehicle::ground_pose`. Race
  checkpoint resets are placed before their next physics step. The lift axis is
  a runtime global in the original; world up is assumed. Motorcycle race resets
  keep the authored 2.5ft drop.
- Physics runs through the countdown with every racer held on full brake, so
  the grid settles on its suspension instead of hanging in the air until GO.
  Settling earns no race distance.
- The race-over estimate, tie-break and results follow the rules above. The
  per-race random stream replaces the native global `rand`.
- After the finish every racer keeps driving. The player's ATV or motorcycle
  is driven by the recovered path driver (the user's account of the original:
  the CPU takes over the player's quad). The exact-build routine for that
  takeover is not identified; `001f9690` creates no driver for human racers,
  and `001f1990` changes their rider state without an identified driver.
  Player input, tricks and Reset are ignored until Restart.
- The results table shows rider, total time and best lap, with Continue →
  Restart / Quit Event and Hide Menu / Show Results. Stunt Score and Profile
  Points are omitted: the port has no per-race stunt total or profiles.
  Keyboard: Enter continues or confirms, Escape hides, shows or goes back.
  Controller: A continues/confirms, B goes back or hides, X hides/shows.

Checks: Rust tests cover placement on flat ground, the wheel lift, the reset
placement, the race-over estimates and the ordering tie-break; `test_race.mjs`
checks the grid is grounded during the countdown and the new results menus.

## Pre-race camera and 321 lights — 2026-10-16

Exact-build evidence (private exports; bounded reading):

- **Race load** (`00210980`): fade in from black over 500ms and select the
  pre-race camera, mode 5 (QuadzillaPreRaceCamera on SimpleCutSceneCamera).
- **Shots** (`0030b280`/`0030b570`, tick `0030bac0`): every
  `<venue>nat<n>.vub` (n = 1–4) is joined into one sequence. All 32 race
  archives ship `nat1` and `nat2`. A VUB is a u32 header (2, frames, 1), then
  40-byte frames: unit forward, unit up, world position in feet, FOV in
  degrees. Playback is 30 frames per second, linear between frames,
  extrapolating at a shot's last frame. Position and FOV pass low-pass filters
  (0.3s, 0.4s) that reset on each cut.
- **Swoop** (`0030df00` case 5, single player): start 18ft from the rider at
  −45° about +Y from its forward, 2ft over the ground plus 2ft; arc for 2.5s
  around the midpoint to the follow pose about the unnormalized axis
  (n × up) × n, with the angle linear in time. The look target and FOV blend
  from rider + 3ft and 90° to the follow view.
- **Countdown** (`0020f780`, `00248020`): starts once under 2.2s of swoop
  remain, so it overlaps the swoop. It lasts 4.0s; the overlay steps each
  second (`00217590`) and the race goes live at 4s.
- **Skip** (`00212d40`): any button from a local pad before the countdown
  starts returns to the follow camera and starts the countdown at once.
- **Restart** (`00215460`) goes straight to the countdown, with no intro.
- **321 LIGHTS** (QuadzillaCountdownOverlay, `002e79a0`, `002e81f0`): four
  32×32 lamps from (256,208) in the 640×448 overlay space. Step 0 shows all
  off; steps 1–3 light the three red lamps one per second, each with
  `beep01.vag`; step 4 lights the green lamp with `go.vag` and hides 1s later.
  The lamp art is in every race and Freeride course archive.
- **Start gate** (`00223bf0`): only National race TOYs declare a `[DropGate]`;
  see "National drop gate" below. Supercross has none.

Port:

- `race_intro::Intro` plays the shots and the swoop, and requests the
  countdown at the original point. The follow camera keeps running underneath
  and supplies the swoop's end pose. The browser shows intro poses as authored
  (no widescreen, portrait or uphill chase adaptation) and hides the HUD.
- Any key, tap on the game surface or controller button skips the intro; the
  toolbar buttons keep their own actions. Restart Race skips the intro.
- `Race::frame().lights` gives the lamp step. The HUD draws the original lamps
  centred on wide screens, and the beep, go and gate sounds play from the
  content package (`AUDIO.RES` gains the three samples). Without the lamp
  art, the countdown digits remain. (An earlier build played `gate.vag` on
  Supercross; the original plays it only with a National drop gate.)
- Not reproduced: the fly-by/orbit fallbacks (every course has shots), the
  race-load announcer cue `00217bb0`, and the Supercross gate rotation.

## After-finish camera director — 2026-10-16

Exact-build evidence (private exports; bounded reading):

- **Timing.** Race over opens the results and arms a 3s timer (`00215db0`);
  when it expires, end race (`001f1990`) hands the camera controller to the
  director (`001f3bb0`). It always follows the player. Restart restores the
  player's own camera (`001f54b0`) and resets the TV cameras.
- **Cameras** (`0031b550`, `<venue>nat.tv` in every race archive):
  `[CameraPositions]` NumberOfCameras, CameraSwitchMinWaitTimer (2s) and
  CameraSwitchMaxWaitTimer (8s); per `[CameraN]` a world Position, FOV or
  MaxFOV/MinFOV (70/10) with MaxFOVDistance/MinFOVDistance (200/20), the
  NextCamera list and percentage weights Chase1, Chase2, Slot1, Slot2,
  Classic, LookBack and FirstPerson. The shipped files' `CameraSwitchWaitTimer`
  and `Classic1` keys are not read, so the defaults apply. `Blur` is loaded
  but unused; no shipped camera is `Stationary`.
- **Switching** (`0031bb50`, 3D distance to the rider): no cut within 2s of
  the last one. Then cut to the closest listed next camera when it is closer
  than the current one. When 8s pass without a cut, take the nearest camera if
  it is at least twice as close (squared distance) as the last best-next
  camera, else that best-next camera. The first camera is always Camera1.
- **Each cut** rolls 0–99 against the cumulative weights: Chase1, Chase2,
  Slot1, Slot2, Classic, LookBack select follow presets 0–5; FirstPerson
  selects preset 4 (Classic) in this build; the remainder is the TV view. One
  cut in four eases the follow camera in; the rest snap.
- **TV view** (`001ac230`, `001ac580`): the camera's position, aimed 3ft above
  the rider. FOV = MinFOV + (MaxFOV − MinFOV)(MaxFOVDistance − d) /
  (MaxFOVDistance − MinFOVDistance) on the XZ distance d, clamped, with a 0.4s
  filter that snaps on a cut. A racer passing within 55ft above 170ft/s shakes
  the camera on alternate frames (`0031c2d0`).

Port: `race_director::Director` reproduces these rules on the player's ATV or
motorcycle. Follow cuts select the port's follow presets; an eased cut keeps
the follow filters rather than seeding them from the TV pose. The libc `rand`
stream is replaced by the port's seeded stream. The browser shows TV views as
authored, like the intro shots. The CPU-driven player also gets the authored
CPU recovery (5s off the track, or 4s under 3ft/s). Split-screen fallbacks
and the STOPCAM cheat are not reproduced.

## Original race overlays — 2026-10-16

Exact-build evidence (Overlays.pb `[OverlaySet1]` 1-player layout and private
exports): LAP TIME (object 1, 609,23), LAP (2, 354,23), SCORE (3, 178,23),
PLACE position tree (4, 20,23), BEST/TOTAL/LEAD (7, 609,23), infotext (11,
320,304), radar (13, 20,305), signage (6, 320,80) and lap history (14,
609,23, hiding 7). Text without a colour draws 190,190,190 (`002e0770`).
Times are MM:SS.hh with rounded hundredths; 0 shows dashes.

- **LEAD** (`00248b20` +0xec): riders ordered by race distance; an unfinished
  rider's gap is its elapsed time less the leader's time scaled by its share
  of the leader's distance (≥ 0); the leader's is minus the second rider's.
  LEAD shows minus the player's gap, green when ≥ 0, red otherwise.
- **Signage:** Too Deep (priority 2, blue countdown of the deep-water clock),
  Off Track, Go Back! (priority 3, gold 5→1 countdown, then the rider is
  returned to the course), Wrong Way (1), Final Lap! (1, 3s, when the last lap
  begins) and First/Second/Third Place (1, 3s, at the finish). A sign replaces
  the current one at equal or higher priority; hiding a sign restores the
  highest waiting condition sign.
- **Lap events** (`001e9a00`): "Lap Time" infotext for 2s (queue of 3, the
  current one cut to 1s when more wait) and the lap history for 5s: the last
  five laps slide in from +200 over 1s, hold, slide out; the best lap uses
  76,181,255; BEST/TOTAL/LEAD fades out over 0.5s and back.
- **Position tree** (`002e5150`/`002e5440`): one slot per rider, the name
  sliding to its rank row in 0.3s; the player's name and place number in
  218,192,31; the place number pulses to 1.6× (0.1s up, 0.3s hold, 0.1s down)
  when the player's name arrives.
- **Radar:** the race TOY `[Radar]` texture at native size, north-up, scaled
  by (Right − Left) / width; 8×8 `radaricon_racer` dots, the player red.

Port: `race_hud::RaceHud` owns the overlay clocks and attaches its frame to
the race frame; `Race::frame` adds lap times, the best lap, LEAD and each
rider's rank. `web/hud.js` draws the objects with their original layouts,
fonts and icons from the race archive. Wide screens keep right-hand items on
the right edge; phones use a 480-unit minimum width, move the right column
below the compact speed meter and lower the signs. The text HUD remains a
fallback without the race art. Off the track for 5s returns the player to the
course, as the countdown sign and the manual describe.

Not reproduced: SCORE uses the port's committed stunt points; the qualifying
bar and new-record infotexts need championships and saved records.

**Overlay toggles** (original Pause → Overlays, `002df120`): Settings →
Display → Race overlays switches the original groups and saves them in this
browser. Timing hides LAP TIME, BEST/TOTAL/LEAD; Lap hides LAP and the lap
history; Score; Position chooses the name tree (object 4), the place number
"PLACE n/field" (object 41) or nothing; Informational hides signs, infotext
and stunt text; Speedometer/Preload; Radar. The 321 lights are not in a
group, as in the original. The port places the page under Display rather
than the pause menu itself.

## Stadium crowd and camera flashes — 2026-10-16

Exact-build evidence (private exports): only scenes with `[Sounds]
CrowdPresent=T` (all 16 Supercross venues) build the crowd emitter
(`00213bf0`, `002cb9b0`) and the camera flash manager (`0020f780`).

- **Crowd reactions** (`Audio\Crowd\crowd.aud`, in each race archive): one
  non-positional crowd voice. Boo when the player wrecks while leading
  (`001e8a20`); Cheer when the player takes the lead after at least 5s of
  racing (`001ea930`); the looping Stomp when the final lap begins with the
  player second (`001e9a00`), stopped when a CPU leads or any racer finishes
  (`002cc320`); Win when the player finishes first (`00216190`). Boo and
  Cheer step past an immediate repeat. A playing clip blocks new ones; a
  Stomp loop gives way. Tracks.dat category (settings+8) picks the lists:
  the eight Pro Supercross courses (`prosx`: sx03, 04, 07, 10, 11, 14, 15, 16)
  use Pro, the Amateur ones Am; only Pro has Stomp. The stunt chain finish
  (`001eaad0`) plays Cheer when a chain lands and Oh when it fails; its extra
  gate (chain+8 bit 2) is unidentified and treated as set.
- **Ambient crowd:** each venue `<venue>.aud` places 8 looping
  `[SoultreeSoundN]` sounds (`crdloop3`–`6`) with Min/MaxDistance, played
  through the train's recovered stereo attenuation (`00172e70`).
- **Camera flashes** (`0020f2c0`, `0020ee10`, `[CameraFlash]` in the venue
  TOY): every `cameraflash.slt` scene model is hidden and pooled (187–493 per
  venue). Batches of trunc(p · N) random models light for 0.09s. p is
  ConstantFlashPercent (0.01); a burst ramps it to its target over half of
  BurstDuration (2s) and back: PlayerWin (0.3) for a player win, AIFinish (0.3)
  for any other top-3 finish, PlayerStunt (0.4) for a landed player stunt.

Port: `race_crowd` holds the crowd cues and the flash manager; the
browser hides the pooled models outside the lit batch, plays the cues and
loops, and the content package adds the 18 crowd clips and four loops to
`AUDIO.RES`. Previously every flash model was drawn permanently. Freestyle on
a Supercross venue keeps the loops and constant flashes. AI stunt bursts,
and crowd volume are not reproduced.

## Supercross fireworks — 2026-10-16

Exact-build evidence (private exports; loader `00222170`, crossing handler
`00223c60`, controller `002217f0`/`00221730`, start `00221840`, countdown
Deactivate `0020ec20`): each Supercross race TOY declares 28 stream and
explosion emitters with `StartOn=T`, `[PreRaceFireworks]` listing every
instance and `[FireworkTriggers]`.

- From load (through the pre-race camera) the emitters run their authored
  EmitMode 2 random on/off cycles; every countdown update deactivates the
  pre-race group, so the stadium falls quiet for the countdown.
- A trigger is an upright gate (TriggerPosition, TriggerLook, TriggerWidth,
  TriggerHeight). A racer crossing it fires its emitter groups in turn,
  TimeBetweenGroups apart; each started emitter is forced to one-shot for one
  on-period and the previous group stops. It fires once on the lap equal to
  TriggerOnLap (clamped to the last lap), or on every lap with
  TriggerEveryLap. RaceOrderMatters limits a non-leader to group 0;
  DontTriggerForAI ignores CPUs. The controller only starts when idle.
- Trigger 2 is the start gate (lap 0): six pairs of shooters 0.15s apart as
  the field crosses after GO. Trigger 1 is the finish gate on the last lap.

Port: `race_fireworks::Controller` runs the triggers and sequence and drives
the ambient emitters by race TOY emitter and instance. A race with firework
triggers keeps every stream emitter instead of the static packet budget; the
bounded GPU ring takes the bursts. The gate test is an authored plane
crossing, not the RaceStatus gate (`002487c0`); the gate drop rotation is
still not reproduced.

## Race intro and race-over music — 2026-10-16

Exact-build evidence (private exports): `IntOut\intout.aud` defines eight
stereo stream cues: INT001–INT006 and OUT001 (looping) / OUT004. Race load
(`00217bb0`) plays a random INT001–INT004 for a single race (INT005/INT006
belong to championship modes), with no song. At GO (`00217590`) every IntOut
cue stops and the next song starts. Race over (`00216260`) fades the song over
1.5s (`0014f8d0`, 0.8 × the remainder when shorter) and plays OUT004 after a
win, handing over to the OUT001 loop when under 1s of OUT004 remains
(`00211490`), or OUT001 at once after a loss. Restart stops everything; the
countdown then runs without an intro cue.

Port: the eight cues join the repacked `AUDIO.RES` (the files use the music
VAG layout and decode with the existing stereo VAG path) and play on their own
voice at the music volume; `MusicStream.hold`/`release` give the silent
intro and the race-over fade. The IntOut base volume is not recovered.

## National drop gate — 2026-10-16

Exact-build evidence (private exports `00222170`, `00222cd0`, DropGateAction
`002212d0`/`002212f0`, reset `00221370`): only N01–N08 race TOYs declare
`[DropGate]`: its own `natNNdropgate.slt` soultree (not a scene model), placed
at Position with rows X = Up × Look, Y = Up, Z = Look ⟂ Up. At GO it turns
about RotationAxisLocal (−X) through RotationAngleDegrees (40°, N02 39.5°)
linearly over RotationRateInSeconds (0.1s) about its bottom edge, falling
toward the riders to about 5° above horizontal, and plays `gate.vag`. Restart
stands it up again. It has no collision.

Port: `race_gate::DropGate` poses the panel each frame from the race clock;
the browser adds the gate model to the scene draw list. The grid keeps the
original placement (4.66ft behind the TDF tag-15 box, `001f8410` cases 3–10,
which use the same box). On most Nationals the gate stands about 7–8ft past
that box, ~12ft ahead of the riders (nat03, nat05); on nat01 the authored
gate stands 12ft before the box, so it lies behind the grid there.

## CPU difficulty, jumps, track edge and cheats — 2026-10-16

Exact-build evidence (private exports; bounded instruction comparisons where
stated).

**Difficulty.** Race settings +0x610 holds the difficulty: the debug command
line maps `Easy`/`norm`/`Hard` to 1/2/3 (`0021add0`), and race prep
(`00277560`, `00289d50`, `0026b7a0`) stores the event difficulty through
`002f64a0`: event index 2 → 3 (Expert), 0 or 1 → 2 (Normal). A binary-wide
scan of loads/stores at +0x610 finds the settings readers only in race
creation `001f9690`, the championship roster draw `0021f680` and two
unrelated routines. Race creation:

- sets AI flag 0x400 unless +0x610 = 3, and `0024f9d0` calls the leader
  speed limit `002536b0` only with that flag. An Expert CPU ahead of the
  player keeps its TDF target speed; a Normal one slows to the leader policy.
- in a championship (+0x36d, set by `00289d50`), when not Expert and for race
  modes 0/3/5, scales a racer +0x178 table (count +0x80, values +0x88) by
  0.925. The table is unidentified and the port has no championships.

No other difficulty input to CPU driving was found. The manual's "blow a jump
now and then" (Normal) has no separate code path in single races. A guess,
not verified: the leader limit slows a leading Normal CPU before a jump. Easy (1) is stored only
by `00265620` and behaves like Normal in race creation.

Port: `race_ai::Options::difficulty` (1 Normal, 2 Expert) drops the leader
limit for Expert. Race setup has a Difficulty row (Normal/Expert, saved with
the last race), and play links accept `difficulty=normal|expert`.

**Jumps** (`0024ebb0`, VehicleAIInterface +0x28 after the lean, on the aim
node +0x278). Flags 2/4 hold a straight lean −1 while the clearance stays above
−1ft/−2.5ft (constructor +0x270/+0x26c) after an ordinary node armed the latch
(AI flag 0x8000); no shipped TDF uses them. A flag 0x40 node with the rider's
preload available (status +0x50 → AI flag 0x20000) chooses a strength once
(AI flag 0x1000000): +0x264 (0) + 0.05 (race creation +0x260) × (TDF tag 27 ×
(target − speed)), clamped to 0–1, 150ft/s for an unset target. It leans 1 to
arm (unless already ≥ 0.9) and −1 once armed (status +0x48 → 0x10000). The
racer post-update `001ebb10` then leans −0.3 and supplies the lean-flick rate
strength × −(max − min) − min, which rider control turns back into that
strength; `001e7940` clears the rate every frame. TDF tag 27 is node +0x4c
(`00179680`); every point of the 32 courses has it (0 or 1). 0–21 nodes per
course carry flag 0x40 (none on env02). Grounded on a flag-8 node (RaceStatus +0xbc) the lean
is constructor +0x258 = 0.6.

**Track edge** (00252080 tail `00253498`–`00253660`, AI flag 0x400000, set
for every CPU). A probe 20ft (+0x2f4) along the requested heading is located
on the nearest segment of the AI's section (`0017b6a0`). When it lies inside
neither that segment's edge polygon nor the next one's (`0017bb50`), and the
corrected velocity v points right of the direction t of the rider's
RaceStatus +0xbc node (`0017d980`),
cross = vx·tz − vz·tx > 0, then v loses cross·t (+0x2f0 = 1). The heading
turns further right. Nothing happens on the other side. This is the only
corner-related correction; it changes direction, not target speed. The port
uses the race station for that node; like RaceStatus it stays on the last
on-track node while the rider is off the track polygon. (Using the rider's
nearest segment instead, an earlier draft, rotated t through hairpins and let
the tail push off-track CPUs further out on nat01.)

**Cheats** (`0033ede0`): EATDIRT toggles settings+0x618 (AI flag 0x4000000),
BRINGIT settings+0x1880 (AI flag 0x800). With aggression the peer loop skips
ordinary avoidance of a human racer (RaceStatus flag 4); a human outside the
0.5 facing cone, within 100ft (+0x2dc) and travelling the same way (its
RaceStatus +0x150 track direction) pulls the velocity to 0.8 v + 0.2 |v| ·
the direction to it. All Out AI requests direction × target speed and
`001ebaf0` blends the grounded quad's horizontal velocity 0.5% toward it each
frame. Its write to quad +0x294 and the AI +0x90/+0x94/+0x98 filters are not
reproduced. The port exposes both only as `cheats=eatdirt,bringit` links.

**Start slots.** See "Original start slots — 2026-10-26" below.

Private comparisons (finite binary32 interpreter over the original bytes,
delay slots included, no hooks; `private/reports/racing/cpu-ai-probes.json`)
show zero differing outputs against the Rust functions:

- `0024ebb0`: 3,000 cases, 115,933 instructions;
- `001ebb10`: 1,500 cases, 50,219 instructions;
- the peer loop `00252e28`–`00253494` with the aggression branch: 3,000 cases,
  1,020 with an aggressive CPU and a human peer, 1,175,927 instructions;
- the track-edge tail with the real `0017b6a0`/`0017bb50`/`0017d980`: 2,778 of
  3,000 synthetic six-node sections compared (828 changed), 1,832,450
  instructions. The others end the section or pick a different nearest
  segment.

Host Chromium, four CPUs and an idle player, 40s per run. Mean/max CPU route
error in feet for current main, this branch without the tail, and this branch:
nat01 6.6/25, 7.5/42 and 10.3/150 (two earlier main runs: 6.5/27, 9.0/131;
two runs without the tail: 10.5/106, 9.3/131); env01 7.0/28, 6.4/28 and
8.4/43; sx07 8.1/33, 8.0/35 and 7.7/36; env02 11.7–12.4/34–37 on main and
12.2/40 with the tail. The tail fired in 358–687 of about 8,300 CPU frames.
Run-to-run spread is large, but on nat01 and env01 the recovered tail tends to
widen CPU lines. It is kept as the original behavior. Worker cost did not
change: 1.0–1.1 ms mean and 1.2–1.3 ms p95 per five-quad step on main and
this branch (env01, env02, nat01, 30s each).

These cover the interpreter's binary32 model, not PS2 hardware rounding.
The port finds the nearest probe segment in the station window instead of the
whole section and tests the four-corner edge quad instead of the native
hexagon. It does not prove whole-race behavior.

## Original start slots — 2026-10-26

Exact-build evidence (private exports of `001f6350`, `001f6c60`, `001f80e0`,
`001f8410` and `0017d980`; bounded reading, no instruction comparison):

- **Slot table `001f6350`.** Race setup stores the rider count (+0x88) and
  fills a table of slots 1…n. Unless settings +0x7d8 is set, it visits every
  pair i < j once, in order, and swaps the two entries when libc rand × 2⁻³¹
  exceeds 0.5. Rider k (1-based) later reads `table[k − 1]`.
- **Rider order `001f6c60`.** The human racers (settings +0x84) are created
  first, then the CPUs, so in single player the player is rider 1 and takes
  `table[0]`. Settings +0x7d8 also selects a per-player creation path in
  `001f6c60` and `001f80e0`; it is probably the network/link race flag, but
  this is not confirmed. Single races here take the shuffled branch.
- **Grid row `001f8410`, race modes 3–10.** With a track start box (Track
  +0x54, TDF tag 15) the rider stands at the box center with y = 0, facing
  the box forward f, then moves 4.66ft back and
  `sign × (slot − n × 0.5) × RiderSpacing` along (fz, 0, −fx). The product
  uses floats: with five riders the slots sit −1.5, −0.5, 0.5, 1.5 and 2.5
  spacings from the center line, so the row is one spacing wider on one side.
- **Row side.** `sign` comes from section 0's first node and its next two
  nodes. `0017d980` returns the normalized vector from a node to its next
  node; the sign is −1 when the cross product of the first two such vectors
  has a positive y, else 1, and −1 when the track or either next node is
  missing. Of the 32 race TDFs, 11 give −1 and 21 give 1.
- Scene start markers (+0x348 list) take precedence for slots below their
  count; race scenes declare none.

Port:

- `race::start_slots` draws the table and `Course::grid` places a slot;
  `Race::seeded` gives the player `table[0]` and each CPU the next entries.
  The player is placed in its slot before the grid settles (`place_racer`),
  so the player can start anywhere in the row instead of always in the
  middle.
- The draw uses `OriginalRandom` (the recovered libc generator) on the race
  seed with a fixed stream offset, not the native global random stream, so
  the original's sequence of draws is not reproduced. Restart Race reuses
  the same seed and therefore the same slots; whether the original reshuffles
  on restart is not established.
- The row height still comes from the route (then ground placement), not the
  native y = 0 followed by its ground drop.
- Section 0 is the TDF's section id 0. The `0017d980` node-length check
  (node +0x40 > 0) is assumed to pass on shipped courses.
- The race modes 3–10 mapping to Environment, National and Supercross races
  follows the earlier start-box reading, not a separate recovery of the mode
  numbers.

Checks: unit tests cover the shuffle against a per-pair reference (every slot
reachable for the player), the row offsets and side, slot validation and the
seeded race. The private all-courses test reports each course's side and how
many of five slots lie inside the track polygon: 30 of 32 courses keep all five
at 8ft spacing; env01 keeps 3 and env06 none (its start box was already outside
the polygon; the old symmetric row had the same misses). `test_engine_race_grid.mjs`
loads ENV01 with four CPUs for four seeds and checks that every rider stands in
its slot of one evenly spaced row, and that the player's slot changes with the
seed (slots 3, 3, 2 and 4 in host Chromium).

