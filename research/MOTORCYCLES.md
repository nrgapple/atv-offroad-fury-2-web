# Motorcycle reconstruction

**Status: the riding controller is recovered component by component from the
MX vs. ATV Unleashed game module, running through authored adapters.** Steer,
bank, tires/suspension, engine/clutch/shifting, wheelie/stoppie, airborne
control, preload, backflip/three-sixty and the wreck decision chain follow
SLUS_211.04/GAME_REL routines (see "Recovered riding controller" below). ATV2
terrain/obstacle contact, ATV2 map surface coefficients, browser input, the
wreck body and a few airborne trick shortcuts are authored adapters, and the
chassis collision hull and COL-v2 routing are not reconstructed. Native
whole-step parity is unmeasured; describe the bikes as a recovered controller
through adapters, not a decompiled or verified-equivalent port. Sections below
dated before 2026-10-15 describe the earlier authored controller.

Remaining authored stabilizers (marked `Authored stabilizer:` in the code):

- wreck body held at terrain + 0.5 ft with 0.12 rebound and 0.97 damping, in
  place of the unreconstructed chassis collision springs;
- detached rider clamped to terrain with 0.85 horizontal friction.

"Paired steps" below are the enhancement-isolation check from
`tools/crash_isolation.mjs` (enhanced vs Original-mode WASM workers, zero
gameplay/camera differences). They show optional crash effects leave the port
unchanged; they are not comparisons with MX vs. ATV Unleashed.

Source: the user-supplied MX vs. ATV Unleashed USA disc, boot `SLUS_211.04`,
version 1.02. Boot SHA-256:
`4b74c0c76ffe0838e00c1b9f75b31e1bb91adae06ec9da1416355b363572102a`.
This is a distinct build; ATV2 addresses and verified kernels do not establish
motorcycle behavior here.

## Strategy

Recover this build's PSR directory, newer PSG meshes, modular vehicle assembly,
motion/configuration resources and executable functions before wiring a separate
two-wheel Rust simulation into the existing selection/loading/terrain system.
Preserve source Race and Pro tuning as separate modes. Reuse existing authored
format/math/browser infrastructure only where source layouts or behavior agree.
Compare recovered kernels against exact instructions and exercise the integrated
bike in the browser before publishing a playable motorcycle option.

Original resource bytes, source executables, module images and decompiler output
remain in ignored `private/`. Only required validated runtime resources are
eligible for the separate R2 package. Never add a raw executable/module to it.
Optional funny crashes remain an authored presentation layer; new players default
to Original and choose Crazy/Gorn in Extras. They must not change baseline
motorcycle handling or recovery.

## Observed resources

The ISO has a large `MXVSATV.PSR` rather than ATV2's loose resource tree. Its v2
header describes 20,391 variable-length named records occupying 751,948 bytes
after a 24-byte header. Member payload offsets are 2048-byte sectors relative
to the PSR. The separate lookup tables are not yet assigned meanings. Contained
`.res` files use the existing v3 block container.

Motorcycle tuning exists for Race/Pro, player/AI, and 50/125/250/500 profiles.
Front/rear tire definitions contain sixteen surfaces; the files also define
clutch torque boost/center-of-gravity movement, roll balance, wheelies/stoppies,
preload, airborne gas/brake gyro torque, crossup limits, backflip/360 torque
ramps, donuts, no-hander balance, and crash thresholds. Data names and values
alone do not establish update order, units, controller gates or fidelity.

The base `bike.psg` has 22 parent-first nodes, including steering, front forks,
swingarm, two wheels, foot/hand markers and clutch/throttle handles. It is PSG
v5: accepting it as v4 would misread its packed LOD and material-layer headers.
Selected bike `.sub` files attach branded engine/frame/plastics/exhaust/fender
models and remap materials. Rendering only the base rig would omit body parts.

Rider configuration defines 46 stunt entries, including ground wheelie/stoppie
and special backflip/left/right 360 cases. Several moves animate both rider and
bike, with separate hold/safe/duration/link values. Particle definitions include
tire dust/chunks/spray, exhaust, foot effects, and bike/rider wreck emitters;
these differ from ATV2's current wheel-only implementation.

## Playable integration

Single Player garage selections 27–46 contain 16 source bike designs and 20
choices. The original Fast50/Suzuki choices 27–34 retain explicitly labeled Race
and Pro setups; Honda, Yamaha, Kawasaki, KTM, Sano and Red Baron assemblies add
distinct designs. The source has four 50/125/250/500 handling profiles; different
brands sharing one profile do not imply independently recovered per-brand physics. They use a separate authored two-wheel Rust controller:
front/rear suspension and sixteen-surface forces, source engine curves/gears,
clutch boost, lean balance, wheelies/stoppies, preload, airborne gyro/control,
crossup and flip/360 torque. Geometry preserves each SUB part hierarchy beneath
its placement bone, with source materials, newer indexed PST textures and the
THQ male rider. Standard riding and reachable configured stunt MOT clips drive
rider poses. The newer -7 MOT packed-axis profile is decoded separately from
ATV2's ShortAxis records. Steering, forks, swingarm and wheels animate.

Keyboard W/S/A/D drives; Q/E shifts rider weight, Space holds/releases preload,
V operates the motorcycle clutch, J/K/L plus direction selects stunts and
L plus airborne backward/side input requests backflip/360 torque. Controls are
remappable; the controller clutch defaults to left-stick press. Existing touch
riding input reaches the motorcycle controller. Race preview uses the existing
authored CPU route system. Motorcycles are restricted to Single Player because
the multiplayer model protocol currently supports the 26 ATVs.

Source engine/clutch/impact samples are decoded with the existing audio controller
as an authored adapter. Bounded dust/chunks/spray/exhaust/wreck particles use
source emitter ranges and atlas data. Source mud textures accumulate during
forward riding and clear on reset. Crash thresholds initiate rider separation;
recovery uses the last safe grounded location. Optional funny crashes add
articulated ragdolls and contact-triggered breakaway pieces without altering
baseline physics. Original ATV behavior remains on its existing solver.

This is source-tuned authored gameplay, not a complete decompilation or verified
native whole-step parity. Native tire curves/update order, collision-v2 dispatch,
full stunt timing/linking/scoring, exact bike/rider animation coupling, original
camera obstruction, audio scheduling, water/foot effects and tire tracks remain
unfinished. Selected required runtime assets stay in the separate R2 package;
boot/module/ISO/reverse-engineering output remain private. Publication requires
the current-main CI artifact and public no-disc browser checks.

## Verified recovery component

The authored Rust PSR reader matches every name and extent from an independent
parser: 20,391 entries, 751,972 header/directory bytes. It bounds allocation,
names, traversal, case-folded duplicate lookup, truncated directories and member
extents without reading the whole multi-gigabyte container into memory. Native
tools can list/extract it directly from the ISO:

```sh
cargo run -p atv-tools -- psr-list \
  'private/source/MX vs. ATV Unleashed (USA).iso' private/reports/mx-unleashed/psr.json
cargo run -p atv-tools -- psr-extract \
  'private/source/MX vs. ATV Unleashed (USA).iso' Veh/Anim/bike.psg \
  private/extracted/mx-unleashed/bike.psg
python3 tools/analyze_mx.py --game-module 0x004dae60 0x004def40
```

`GAME_REL.BIN` is 854,016 bytes, SHA-256
`bea3eb36260f5ab64d9de1919318593a6a58b08fb2c160e9450f5a0e1e334957`.
Boot `001cd380 → 00155480 → 002d2620` reads the entire module into the
destination table's `0045c580`, then zeros its BSS tail and dispatches initializer
pointers. The file's header agrees with that boot destination. `MxLoadModule.java`
maps the exact bytes into the private analysis database and checks byte identity;
it never executes the module. Ghidra function counts/boundaries remain analysis
candidates, including misleading fragments and nonreturn inferences that require
instruction-level review. They are not decompiled/verified gameplay counts.

The PSG v5 LOD header uses four separate bytes. Surface geometry reads an inherited
material index, flags, five policy/layer bytes, per-layer source identifiers, two
u16 counts and packet length. Native `0019f0c0/00166360` replay agrees with Rust
on all 42 groups of nine inspected models across 6,846 original instructions.
File reads and heap allocation are bounded supplied services; this verifies
header/packet extents, not rendering or physics. Quantized rigid VIF vertices
retain a positive node magnitude under their high restart bit, unlike ATV2's
negative magnitude encoding. Existing v3/v4 tests remain passing.

The standard bike decodes 22 nodes/3 LODs/2,470 finest triangles; bike50 decodes
19 nodes/4 LODs/2,388 finest triangles. The RM125 engine, exhaust, plastics, frame,
fender, number plate and crossbar also decode. Uninterpreted mesh trailers remain
explicit (30 and 27 bytes for the base bikes). The private screenshot is a flat
shaded mesh inspection with separate components, not an assembled riding scene,
original materials, native GS/VU replay or motorcycle gameplay.

The optional authored crash layer recognizes the bike's `TireFront/TireRear`
markers and transforms visible descendants below their nearest detached part.
Nested rear wheel/swingarm pieces retain independent motion; reset and Off clear
the presentation layer. A synthetic motorcycle hierarchy verifies this support.
The existing ATV severe-wreck browser regression passes the enhancement-isolation
check (855 steps, enhanced vs Original mode, zero gameplay/camera differences), seven visible pieces and intact recovery.
No live motorcycle crash or rider integration is established yet.

The host gate passes 322 core, 25 net, 3 relay, 13 bridge and 69 JS tests, strict
format/Clippy, WASM build, setup/hygiene and 18 release tests. A temporary ignored
internal host target avoids observed stalled SSD hard-link/clone operations;
the configured SSD symlinks remain intact. The checkpoint is separate from public
production; no new original runtime resources or source module have been uploaded.

## Verification and next fidelity gates

The host implementation suite passes 335 core, 25 net, 3 web, 13 relay and 69 JS
tests with strict formatting/Clippy, plus release/WASM checks. The normal-input
browser gate selects, renders and drives all eight configurations, exercises
steering/clutch/particles/reset, and compares optional crash presentation Off/On
against the same Rust input stream. Reports remain under ignored work/verification.
The original ATV frontend and severe crash regression also pass.

The original recovery-only evidence above predates playable integration. Next
fidelity work is instruction-correlated suspension/tire/drivetrain/controller
kernels, source COL-v2 collision, stunt scoring/linking, full movement coupling
and exact audio/particle scheduling. A working browser route does not close these
native parity gates.

Final host normal-input replay covers all eight options and 634 paired steps with
no differences after retaining 21/30 source part nodes. A separate 390px route
records 260 frames and 320 paired steps, airborne stunt poses, rider separation,
touch steering/cancellation and safe recovery. That mild wreck produced no severe
bike breakaway transition; the seven-piece severe visual regression is an ATV
check, not evidence of a severe motorcycle impact. MX 125 Race passes the existing
race countdown/pause/five-CPU/touch/restart gate. The 26-ATV garage with both rider
rigs, responsive layout and context restore passes. Setup and 19 release tests pass.


## Motorcycle correction gate — October 13, 2026

The first playable release, `8e1aa77`, passed loading/finite-pose/movement checks
but missed reversed steering relative to the rendered camera and detached hands.
Those checks were insufficient for rider fit and turning quality.

The correction follows the shared browser input convention (positive Rust steer
is screen-left) in ground and airborne bike controls. Forks now turn around the
source inclined steering axis through the front wheel, rather than world Y.
The original `003b2924` initializer uses pi/256 and signed axis bytes /127 for
packed MOT quaternions. The Mini MXF riding/stunt bank uses its own lower root
pose, including bounded -6 clips; full-size MX clips cannot position a Mini rider
correctly. Source NAME/VUE aliases also connect previously mismatched stunt names.

Normal riding blends still/riding/air and forward/back source pose sweeps
continuously. A two-link adaptation of the existing ATV arm solver keeps source
hand/boot markers on the rotating grips and pegs. Stunts and wrecks bypass these
constraints so intentional release remains possible. The chase camera filters
body motion and follows airborne travel heading, avoiding backflip yaw whips.
Render interpolation applies one clock to chassis, bike bones, rider bones and
camera between delivered physics batches, and snaps on resets, camera changes,
stunts, wrecks and cancellation. This is presentation only; Rust baseline and
optional funny-crash comparisons remain exact.

Host checks load/drive all 20 selections, with audio, clutch, particles and reset,
and 1,906 exact paired Off/On steps. Actual 390px touch drags on Mini/125/500 bikes
turn left/right on screen and preserve grip/peg markers (under 0.00004 ft).
The smoothing test observes 2,508 intermediate model uniforms and 660 actual
WebGL rider/bike bone contact comparisons, with maximum rendered hand error
0.00000145 ft and boot error 0.0000284 ft. It also tests reset/pause/wreck
boundaries. Bike50 touch jump/stunt/wreck/separation/recovery passes 310 paired
steps; that mild trace has no severe breakaway. Honda35 race and the existing
ATV severe-crash (1,187 paired steps/seven pieces), frontend and 26-model garage
responsive/context-restore gates pass. Full checks pass 336 core, 25 net,
3 web, 13 relay and 72 JS tests, strict formatting/Clippy, setup, 29 release tests
(one credential-dependent skip) and eight Cloudflare tests.

These are host correction gates pending validated main deployment and R2 upload.
Native whole-controller/IK routing, COL-v2, all stunt linking/scoring and full
original physics/audio timing still require recovery. Actual physical-phone and
controller play remain unverified.

Final integration retains the authored airborne cross-up angle in the normal
pose's fork target before solving hand contact. A normal-input Vancouver 500
flight records 24 cross-up frames, maximum 0.510509 radians, grip error below
0.00000114 ft, boot error below 0.00000536 ft and 432 exact paired steps.
The ordinary portrait contact and mild stunt/wreck/recovery gates pass again;
this checks the connected movement, not original whole-controller equivalence.

## Public correction verification

Main `4fbb399` passes CI 38399468481 and is publicly deployed. Actual touch
left/right, source rider fit and intermediate GPU pose/contact checks pass on
Mini/125/500 profiles: 762 paired steps, 1,760 rendered bone-marker comparisons
(not distinct frames), maximum rendered grip error 0.0000014 ft and peg error
0.0000284 ft. All20 public choices load/drive with audio/clutch/particles/reset
and 1,543 paired steps; Honda35 Race and the mild Mini stunt/wreck/recovery
route (308 paired steps, no severe breakaway) pass. No browser/GL errors or
disc selection are observed. The screenshot log preserves the source-tuned
controller and physical-phone/native-fidelity limitations. Current-main
metadata/gallery validation and full R2 HTTPS verification remain release steps.


## Ground bank and original standing menu correction (2026-10-13)

The preceding correction did **not** verify bike/rider banking direction. The
user reproduced outward lean on full-size bikes, and the previous source-pose
hand sweep also independently controlled visible fork steering. Finite state,
turning direction and attached grip markers were insufficient acceptance gates.

The original GAME_REL ground-bank component directly approaches the commanded
orientation around the existing forward axis. Its source profile uses a 45-degree
maximum command at 35 mph and RollGain=5; contact-normal blending approaches the
support plane between the source 10/30 mph stand-upright thresholds. The Rust
component now uses that recovered bank behavior instead of the competing ground
roll torque spring. Normal ground roll has one owner; free airborne and wreck
roll remain active. The support-contact adapter, generic-body roll-axis ownership,
host trigonometry and tire force routing are integration work, **not proof of
native whole-step equivalence**. Original lateral CG disturbance handling and
complete tire/actor update order still require recovery.

Physical steering now supplies the visible inclined fork angle, including
airborne cross-up, while the existing shared arm/leg constraint engine attaches
normal hands and boots. The source pose sweep runs in the matching steering
direction, so the rider leans into the turn. Actual touch tests now compare
rendered body/rider bank against rendered heading and trajectory, in addition
to grip/peg contact. Mini/125/250/500 gates pass 1,061 exact paired enhancement
Off/On steps and 2,455 GPU bone-marker comparisons; maximum rendered hand error
is below 0.00000170 ft and boot error below 0.0000371 ft. A normal-input 500
Vancouver backflip/wreck/separation/breakaway/recovery gate passes 1,297 paired
steps, four intact airborne ragdoll frames and one contact-triggered damage
transition. These counts describe these host traces, not all possible play.

Original UI_mrider selection breathing and three idle clips were found. All
20 motorcycle selections now use those original standing clips through the
same Rust menu playback and frontend animation engine as ATV riders. The
actual GPU standing-pose, changing-pixel/bone, all-three-idles and return-to-ATV
checks pass. Initial preview facing adapts the MX rig's opposite orientation.
The ordered breathing/idle playlist remains an authored adaptation; native menu
random selection and delay callbacks have not been reconstructed.

The user's Sano 50 screenshot shows an original mini/pit bike. Its source SUB
has no wheel-size override: visible tire geometry measures about 14.91 inches
in diameter, versus about 28.88 inches for Suzuki RM125 front tires. Those source
proportions are retained and 50cc selections now say **Mini**. Larger wheels belong
to the selectable full-size 125/250/500 bikes.

All20 selections also pass host drive/audio/clutch/particles/reset (1,536
paired steps). The unchanged26-ATV garage passes responsive, male/female
animation, rapid selection and context restoration checks. Full workspace
tests, formatting, strict Clippy, source/setup and repository hygiene pass, as
do29 release tests (one credential-dependent skip) and8 Cloudflare tests.

These fixes are host-verified pending the validated-main release and R2 menu
resource upload. Full original handling, animation/controller timing, stunt
linking/scoring and physical iOS/controller fidelity remain unverified.


## Full-lock rigid-arm reach follow-up (2026-10-13)

The four-profile public thumb gate above checked inward bank and normal contact,
but the later full catalog run exposed a normal Sano Mini hand gap of 0.25177 ft
at full steering. The shared original ATV two-link solver correctly preserves
rigid joint lengths: a source turn pose could place the shoulder beyond reach of
the physical fork's grip. A flat source-profile run also reproduced 0.09533 ft
of separation; this was a real pose-integration defect, not an intentional stunt.

Normal MX posing now keeps the requested original pose whenever both wrists are
reachable. At the reach boundary it chooses a close feasible blend toward the
original forward riding/air pose before applying the same arm/foot constraints.
The bounded search retains rigid joint lengths and original source clip data.
This is an **authored pose adaptation**, not recovered native MX pose-controller
logic. Vehicle physics, original ATV pose routes, and intentional source stunt/
wreck releases are unchanged.

All720 flat full-input source-profile frames (four sizes/both directions) keep
hand error below0.00000828 ft. Actual sustained full-lock Mini/125/250/500 touch
gates pass1,730 paired steps and3,415 GPU marker comparisons, with maximum
rendered grip error0.0000267 ft and boot error0.0000297 ft. A separate Mini repeat
explicitly checks steering commands+1 and-1. The20-choice host drive/audio/
clutch/particles/reset gate passes1,588 paired steps; airborne cross-up also
passes24 normal air frames at0.510509 radians and537 paired steps. These are
host checks pending validated-main/public verification. Full native controller
parity, every control/terrain combination and physical-phone play remain open.


## Recovered riding controller (2026-10-15)

The authored arcade controller handled badly: full steer at 30 mph produced a
5 rad/s yaw rate (a ~9 ft radius, ~7 g), small bumps bottomed the forks and
threw the bike ~20 ft up, the air had no self-levelling so throttle pitched the
nose to 60°, and a fixed lateral velocity-kill scrubbed speed in turns. It is
replaced by the original's logic, recovered from the exact GAME_REL module with
the existing private Ghidra project (reports stay under ignored
`private/reports/mx-unleashed/physics-map/`).

Recovered behavior, by component:

- **Frame structure.** The physics world runs `int(dt·55)+1` substeps (one at
  60 fps). Once per frame before physics: input filtering, kinematic steer,
  donut factor, stopped/moving spring blend, auto-shift, clutch dump and
  centre-of-gravity travel. Each substep: gravity, low-speed hold,
  wheelie/stoppie, preload, backflip, three-sixty, engine, drag, airborne
  control, reverse, tires, rev limiter, then constant-acceleration integration
  with body-frame rate clamps (20, YawRateLimit, 20). Once per frame after
  physics: the kinematic bank and derived state, then the wreck chain.
- **Steering and lean.** Stick X is slew-limited (XAxisRateLimit), scaled by
  MaxSteerAngle and a SteerScale ramp between SteerSpeed1/2 (125: 0.9 at
  standstill to 0.2 at 75 mph). The roll axis is locked on the ground and the
  bank is written once per frame toward −MaxRollCommand·min(v/MaxRollSpeed,1)·
  steer at RollGain; the grounded bike cannot tip over. There is no yaw
  controller: yaw comes from tire side forces with a hard YawRateLimit.
- **Tires and suspension.** Spring = Spring×Weight, blended from
  SpringWhenStopped between 10 and 20 mph; ReboundDampingRatio damps
  compression and BounceDampingRatio extension, at the contact point, with no
  force cap. Bottoming pushes out 90% of the overshoot and removes approach
  speed. Side force is the parametric RoundOff/DropOff/Scale/SpeedScale curve
  (a load-proportional lateral damper in practice) with Braking/Slipping sets;
  drive and brake torque pass through a MaxDriveForce·N threshold to
  SkidDriveForce·N spin/lock. Tire forces act at the CG with
  BodyTireForcePosition as the moment arm.
- **Engine.** HP·5252/RPM torque, engine braking, a static/kinetic clutch,
  gear×final ratio to the rear tire, per-gear rev limit, predicted-rpm
  shifting and a clutch dump (hold clutch+gas, release) that scales torque and
  moves the CG back. Drag is 0.5·Drag·v².
- **Air.** Stick torques from AirborneControlMoment. With the stick centred,
  Unroll/Unyaw level the bike and turn it toward its travel; steering switches
  to AirborneDamping, so the stick commands a rate. Brake gyro is on; gas gyro
  is off by default as in the source. Lateral AirborneControlForce drift.
- **Preload and tricks.** Preload is a world-up force for PreloadTime after a
  back-then-forward gesture on a rising ramp face above ~20 mph; strength
  scales with hold time. Backflip/three-sixty use the trick-button ramp
  gestures, start torque from hold time and bleed with opposing stick.
- **Wrecks.** Directional impact limits, the freestyle landing limit, ground
  pitch 75°, airborne pitch 85°/89° and roll 89°, cross-up landings beyond
  ~44° after 0.75 s of air, and landing during a stunt clip. Respawn follows
  the source 0.5 s plus MaxWreckTime.

Adapters and deliberate choices:

- ATV2 maps number their eight terrain zones per map (ENV01 zone 3 is ice,
  zone 7 off-track snow), so MX zone numbering cannot be reused. The bike uses
  MX zone 1 (standard track dirt) scaled by the map's own TerrainZone
  SurfaceFriction (side force), SurfaceTraction (drive threshold),
  SurfaceBraking and SurfaceDrag (added to WheelDrag).
- Browser analog throttle/brake use the source's ramp-up/instant-release
  filter as targets; the source buttons are digital.
- Hold durations for preload/flip strength are counted in frames of held
  stick, matching the source's 0–60 windows. The source channel's exact
  semantics are inferred.
- An airborne shortcut (trick button plus pull-back or side flick early in a
  jump) starts a minimum-strength flip or spin alongside the source ramp
  gestures, preserving the port's existing trick input.
- Spawn grants no clutch-dump boost (the source timer may grant a race-start
  holeshot; unconfirmed).
- The source quirk where an airborne wheel's brake torque still slows the body
  during a wheelie is not reproduced.

Native host probe (`atv-tools bike-probe`, scripted inputs on a level plane and
on ENV01 with its TerrainZone table; 125 Race unless noted):

| Scenario | Authored controller | Recovered controller |
| --- | --- | --- |
| Full steer at 30 mph, peak yaw rate | 5.1 rad/s | 2.9 rad/s (limit 3) |
| 0.4 s steer tap at 50 mph, peak yaw rate | 5.2 rad/s | 3.3 rad/s |
| 10 s full throttle, level | 60 mph | 73 mph (drag limit ~81) |
| Full throttle + lean back, level | no wheelie | holds ~47° wheelie |
| ENV01 2 s jump, landing pitch | 60° nose up | level |

Holding full lean-back with throttle over ENV01 bumps still loops the bike
into a wreck on every class, as pulling back in the air does in the source.
The Mini/125/250/500 probes show no wrecks in braking, turning, slalom or
straight runs.

Host browser gates pass on the recovered controller: all 20 selections load,
drive, steer, clutch-dump, emit particles and reset (1,991 exact paired
enhancement Off/On steps); actual touch left/right turns lean bike and rider
into the turn with grip error 0.0000016 ft and boot error 0.000037 ft (230
paired steps); the touch stunt/wreck/recovery route passes (346 paired steps).
`make check` passes. These are host results pending validated-main
deployment; physical-device play and native whole-step comparison remain open.


## Rider pose, chassis collision and scene contact (2026-10-16)

Play feedback: elbows stuck out, riding felt rigid, crashing was too hard, and
bikes passed through the Arlington train and some objects.

- **Rider pose (MX rider system, adapted).** The source rider is an 18-part
  powered ragdoll pinned to the grips and pegs, with no analytic IK. The port's
  ATV2 hinge solver raised the elbows 0.3–0.4 ft to shoulder height. Limbs now
  use a two-bone solve that keeps the clip's elbow/knee side and twist. Clip
  time follows the filtered crab sweep (τ 0.15 s) instead of the steering
  stick, lean blends fwd/mid/bck (τ 0.125 s), and the source stop/still/riding
  speed bands (5/10/15 mph) select clips. Feet go down when slow and not
  yawing, and the inside foot leaves its peg in a leaned slide; pins return
  over 0.5 s / 2 s. Air crossfades at 3/s in and 2/s out.
- **Flow (authored reduction).** One spring-damped pelvis offset uses the
  source riding part springs/damping (≈1.5 Hz, ζ≈0.3, doubled at a stop),
  driven by the pelvis-point load. Because the ragdoll's pins hold the pelvis
  far tighter than one spring, the drive is scaled by 0.35 and travel limited
  to (0.12, 0.2, 0.3) ft.
- **Chassis collision (recovered).** The bike's COL v2 whisker rays
  (`bikem.col`/`WreckBikem.col`, 50cc `bike50m.col`/`WreckBike50m.col`, ATV2
  type-2 layout plus one header word) are tested once per frame: downward
  rays against terrain, all rays against the scene. Each substep a contact
  closing faster than 10 ft/s takes a zero-restitution impulse with angular
  terms; otherwise a SpringStiffness/SpringDamping spring with
  SpringMomentScale; friction is always 0.5·weight ramped over 10 ft/s of
  slip. Impulses and bottoming feed the per-frame impact vector, so front,
  back and side wrecks now come from real hits. Contact during a flip or
  three-sixty, or touching down while a stunt is held, also wrecks. Wrecked
  bikes rest on the wreck rays with roll free; the terrain clamp is now only
  a sink guard. Packages built before these members existed fall back to
  authored rays from the bike model's grips, pegs and wheels.
- **Scene contact (adapter).** The bike now builds the same partitioned scene
  as the ATV (static props, plant proxies, mud categories, moving train
  bodies). Tires stand on moving surfaces using their point velocity, the
  train installs into the bike's scene, and bike sessions load the map's
  train/rain samples from AUDIO.RES. An explicit start pose now places an
  existing bike, as it does for the ATV.
- **Reverse fix.** The browser adapters set `reverse` both for reverse
  throttle and for braking while rolling backwards; only reverse with
  throttle now drives the reverse controller. Previously W on a slope after
  rolling back could leave the bike stuck reversing.

Host results (125 Race, source rays from a rebuilt private package): level
wall impacts at 15 mph stop without a wreck, and 20/25/35 mph give Front
wrecks, matching the recovered thresholds. Normal riding on ENV01 does not
wreck 125/500 bikes; the light Mini wrecks when it cases rough bumps flat out.
Browser gates pass: all 20 selections (1,683 paired steps), touch turns with
grip error 0.005 ft and pinned-boot error 0.004 ft, stunt/wreck/recovery,
the train broadside (wreck) and roof drop (rides the moving train), a tree
hit at about 31 mph (Front wreck), and the ATV train-collision regression.
The new collision members need the release owner's R2 package upload;
until then the hosted bikes use the authored fallback rays.
