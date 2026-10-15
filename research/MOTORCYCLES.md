# Motorcycle reconstruction

**Status: the motorcycle controller is authored, not recovered.** Resources
(PSR records, PSG/SUB meshes, PST textures, MOT clips, audio) and named tuning
values come from MX vs. ATV Unleashed, but the integrator, tire/suspension
model, stunt timing and wreck handling in `crates/atv-core/src/motorcycle.rs`
are authored arcade code. Only the ground bank angle
(`motorcycle_balance.rs`, GAME_REL `004dbd10`) cites a recovered routine, and it
runs through an authored contact adapter. Native whole-step parity is unmeasured.

"Paired steps" below are the enhancement-isolation check from
`tools/crash_isolation.mjs` (enhanced vs Original-mode WASM workers, zero
gameplay/camera differences). They show optional crash effects leave the port
unchanged; they are not comparisons with MX vs. ATV Unleashed.

Known non-original constraints (marked `Authored stabilizer:` in the code):

- per-wheel spring force capped at `weight * 8`; fixed lateral-slip (`mass * 8`)
  and braking (`mass * 10`) velocity-kill gains ahead of the tuned friction clamps;
- over-travel push-out of up to 0.3 ft along the contact normal, removing 70% of
  the inward normal velocity;
- wreck body held at terrain + 0.5 ft with 0.12 rebound and 0.97 damping;
- detached rider clamped to terrain with 0.85 horizontal friction;
- airborne-landing wreck angle floors of 1.4 rad (roll `.max(1.4)`, fixed pitch)
  and a minimum wreck time of 1.75 s (`.max(1.75)`).

A real recovery would require Ghidra analysis of the SLUS_211.04/GAME_REL.BIN
bike controller, tire and wreck routines (update order, units, gates), then an
instruction-level oracle comparison of whole steps against the Rust controller,
like the ATV physics oracle work kept under ignored `work/physics`. Until then,
describe motorcycles as an authored controller using source resources and tuning.

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
Optional funny crashes remain an authored presentation layer enabled explicitly;
they must not change baseline motorcycle handling or recovery.

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
