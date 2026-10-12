# Water failure and Caribou collision work

Branch `feature/water-snow-collisions` starts from main `49ba6c5` in a separate
managed worktree. Implementation strategy: recover the original water failure
clock from USA v2.00 `001e5260`, connect it to the existing native-style reset,
and exercise actual loaded water/rider contacts. Preserve detached-rider
buoyancy; no constructor evidence supports applying that float actor to the ATV.

The source uses strict origin Y < cached water height - 3, restores a five-second
clock in shallower water, decrements only while riding, and checks expiration
before the following update's physics. That reset calls `001e35d0`; it is not an
invented water wreck, head-submersion death or ATV buoyancy force. Instruction
replays and runtime routes will gate the implementation.

Snow work audits Caribou Peak's original placed models, COL meshes and scene
query participation. Check the large ramp and ski obstacles at their authored
positions, including wheels, chassis, approach/takeoff/landing, before changing
collision transforms or adding geometry. The original rail render root repair
already applies to static scenery; it does not prove collision coverage.

Original bytes, generated census/coordinate data, native probes and screenshots
stay under ignored work/private storage. This branch is not yet a public release.

## Verified host gate

`water_failure::Clock` implements the recovered +814 clock. `Vehicle` publishes
its depth/remaining time for Debug stats, checks expiration before physics, and
uses the same checkpoint reset as wreck recovery. The WASM caller resets rider,
wreck camera and touch state after this independent reset. Shared water clocks
and the preceding water-height cache survive recovery. The ATV retains normal
solid/gravity physics; rider water forces were already implemented and remain
independent. No head-submersion death, forced water wreck or ATV floating actor
was added.

144 original reset/depth/state slices execute 6,160 instructions. The Rust clock
matches every compared binary32 result, including threshold equality, negative
sentinel, riding/wreck states and zero expiration. The reset body itself is a
hook in that bounded probe; unrelated heat/HUD/audio branches use idle flags.
Synthetic vehicle tests additionally prove delayed height use, retained checkpoint,
next-update expiration and expiration while wrecked. Existing rider kernel and
surface comparisons remain separate; this is not a coupled original replay.

The actual-content browser gate derives shoreline coordinates from each loaded
TR2 grid. Normal throttle enters deep water on ENV04/05/07: expiration/reset is
at steps 348/349, 345/346 and 337/338. Recovery returns above water to the dry
bank. ENV08's tested shallower route exercises the strict depth/wave gates without
five seconds of continuous submersion, so it does not drown in this route.
High test placement followed by actual lean controls induces an angle wreck;
all four water maps show water-only rider contact, the original Swim clip,
positive upward buoyancy velocity and recovery. ENV04 also passes at 390px;
ENV08's separate desktop/390px run passes. These fixtures reposition the ATV,
but inject no velocity, force, wreck state or water footprint. They are bounded
source-resource routes, not ordinary menu-to-lake navigation or PS2 replay.

## Caribou ramp and ski obstacle coverage

The rail root-placement repair already places the original `e01ramp` visual and
COL in the same coordinate space: their bounds differ by less than 0.000003ft.
Its saved PSG root translation is replaced by the authored scene placement.
The five `snowski1` instances also use their authored transforms; their deliberately
simpler COL differs from visual ski geometry, and is retained without an invented
shape. The ramp contains 34 faces; each ski collider contains 32, totaling 194
placed faces. Four interior normal rays per face produce 776/776 hits through
the recovered Rust COL tree query. This is a source-resource coverage probe,
not another fresh original-instruction proof of every collision consumer.

Desktop and 390px browser runs approach all six placements from both directions:
24 routes supply wheel support and body responses through the live scene cache.
The ramp's curved approach records support, takeoff, and landing at steps
36/90/239, stays Riding throughout and reaches 35.631ft/s upward velocity.
The steep opposite approach can wreck. These tests leave terrain, ecosystem,
other props and original collision order installed. The requested ramp/obstacle
checks require no additional collision shape, source geometry offset or forced
launch. They do not establish every Caribou contact or whole-map replay fidelity.

`make verify`, formatting, strict Clippy, 301 core + 2 WASM tests, 45 JS tests,
13 release tests and 4 Worker tests pass. Normal touch clean preload landing,
failed Nac Nac separation/contact/recovery/resumed driving and uninterrupted
ENV01–08 loading/driving pass without JS/GL errors. Private reports are
`water-failure-native.json`, `water-physics-final-*`, `snow-collision-census.json`,
`snow-face-coverage.json` and `snow-collision-gate*.json`; probes stay under ignored
`work/water-snow/`. No new runtime resource is required. This checkpoint is
host-verified; publication needs passing main CI, R2/gallery and public checks.

Current-main integration retains the published mobile steering/sensitivity update
`31946a5`. Combined formatting/Clippy, 302 core + 2 WASM and 47 JS checks pass.
The dev-log merge preserves all incoming current-main entries and appends only
this host checkpoint. The shared primary checkout and its local edits are untouched.
The integrated host gate also passes the full water and snow routes, normal
clean touch landing, Nac Nac wreck/recovery and normal title/menu/controller,
Controls, 390px selection and Caribou reload without a disc prompt or JS errors.


## Public verification

Main `b9a3a448d6d19103d63be8e6ec530974297b1870` / CI `38292520844` succeeds
and identifies the automatic public Cloudflare deployment. R2 validates 344
objects and uploads two changes: the approved host snow PNG and gallery metadata.
No new runtime content is needed. A complete HTTPS retry after one transient
connection reset matches 30 authored files and 343 served objects, with 103 log
entries / 90 screenshots before the public verification entry.

Public all-four-map water routes reproduce the recorded depth clocks/resets and
water-only rider buoyancy, Swim and recovery; ENV04 desktop/390px passes. Public
24-route snow coverage reproduces wheel/body contacts and clean ramp launch/landing.
Normal touch clean landing, failed Nac Nac recovery/driving, original menu and
controller-to-Tioga/Controls/390px Caribou reload without disc selection, F3/Pause
Stats persistence and portrait fit, uninterrupted eight-map driving and the
desktop/mobile dev-log filters and expandable evidence all pass without JS/GL
errors. The source-resource fixtures are still bounded routes, not whole-game
or PS2 replay. The published snow PNG remains explicitly a host capture.
