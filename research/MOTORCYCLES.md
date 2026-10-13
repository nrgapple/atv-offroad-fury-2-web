# Motorcycle reconstruction

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

## Status and acceptance

Source identity, private extraction, PSR directory and bounded rigid PSG decoding
are established. Mesh assembly and native gameplay-code recovery remain in progress.
No selectable/playable motorcycle or complete decompilation is claimed. Full requested
handling, movement, sound, dirt/mud, stunt, crash and funny-crash integration must
pass its own component and public no-disc gates before delivery.

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
The existing ATV severe-wreck browser regression passes 855 exact paired WASM
gameplay/camera steps with effects Off/On, seven visible pieces and intact recovery.
No live motorcycle crash or rider integration is established yet.

The host gate passes 322 core, 25 net, 3 relay, 13 bridge and 69 JS tests, strict
format/Clippy, WASM build, setup/hygiene and 18 release tests. A temporary ignored
internal host target avoids observed stalled SSD hard-link/clone operations;
the configured SSD symlinks remain intact. The checkpoint is separate from public
production; no new original runtime resources or source module have been uploaded.

## Remaining integration gates

1. Trace and compare tire suspension/parametric grip, drivetrain/clutch, roll/
   pitch/air control and update cadence. ATV2's four-wheel vehicle cannot serve
   as a motorcycle solver or establish this game's behavior.
2. Connect a distinct two-wheel vehicle and source Race/Pro profiles to selection,
   loading, collision, camera and recovery. Assemble source subobjects/materials
   and animate steering/forks/swingarm/controls rather than drawing only the rig.
3. Recover rider/bike animation coupling, all 46 configured stunt entries,
   timing/linking/scoring and wheelie/stoppie/backflip/360 conditions.
4. Connect the source 50/125/250/450/500 engine banks, clutch/impact/fall audio,
   tire/exhaust/foot/wreck emitters and dirt/mud appearance. Discovered resources
   and current ATV2 effects do not establish these bike mechanics.
5. Verify ordinary and funny bike crashes/recovery, desktop/touch/controllers,
   multiplayer compatibility and original ATV regression; package only required
   resources for R2 and verify the public app's no-disc motorcycle route.
