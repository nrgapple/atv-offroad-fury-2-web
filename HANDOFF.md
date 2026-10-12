# Implementation handoff

## Latest verified public map checkpoint

Public main `f585991579f8c44b098d6b137b1679eb8050e3fd` passed CI
`38302626466`; HTTPS verification matches all 31 authored files and 399 served
objects. The R2 package validates 400 approved objects, with six incremental
changes totaling 305,224,223 bytes. The public gallery has 110 entries and 93
screenshots. Vancouver Ridge/FREE05 is connected to Single Player → Freestyle
under its original SPStunt6 name and two-part thumbnail, unlocked by default.

Public desktop and 390px traces each cover the original terrain, 37,649 snow
plants, 78 source scenery definitions/75 drawn, 43 placed colliders, 123
wheel-support frames, 31 body-contact frames, flight at step 19, later ground
contact, and drag-light frame 1. The bounded ramp route uses normal throttle
without injected velocity, force or wreck state. The public ENV01–ENV08 plus
FREE05 regression, source-material inspection and original title/menu/controller/
touch/pause routes pass without JavaScript or GL errors or a disc prompt. These
are browser component and route checks, not full map, hardware or game parity.
Three Vancouver ice-reflection overlays, exact fog, 92 light/lens behaviors,
projected terrain/vegetation LOD, shadows, weather, remaining modes and progression
remain open. See [map evidence and limits](research/MAP_FIDELITY.md) and the
[deployment record](research/DEPLOYMENT.md). Preserve the separate multiplayer
integration lane. The newly inspected desktop public-ride screenshot is referenced
in the local log but remains in ignored private storage pending the next validated
R2 gallery upload.

## Current host-only reflection candidate

An uncommitted renderer candidate now draws ENV01's 88 original reflection groups
across 275 instances and FREE05's three groups across all 78 instances, with no
skipped groups. Host inspection records 152,862 and 236,566 contributing pixels.
The updated Vancouver desktop and 390px traces draw all 78 placements with zero
skipped groups; both have 43 colliders, 123 wheel-support frames and 31
body-contact frames, plus takeoff at step 19, later ground contact, animation and
menu checks. Expanded GPU transform feedback compares 768 original arithmetic
cases across identity, translated/rotated placement and rotated child-node
transforms: 2,304 GPU comparisons across 5,376 interpreted instructions, with
maximum UV difference 7.152557373046875e-7. The test supplies a pre-normalized
finite local view direction and does not emulate PS2 accumulators, reciprocal
behavior or the full pipeline. `make check` passes 308 core tests, 22 network
tests, 11 relay tests, 3 bridge tests and 66 JavaScript tests; broader all-nine-map,
materials and frontend runs are still pending. The candidate is host-only and not
in public commit f585991; see
[map-fidelity evidence](research/MAP_FIDELITY.md). Keep the public Vancouver
checkpoint and the separate multiplayer lane intact.

## Rust multiplayer host checkpoint, public verification pending

Isolated `feature/rust-multiplayer` adds four-player host-controlled Freeride
lobbies, eight-character share codes/links and an authored Rust Cloudflare relay.
Rust owns packet validation, sender identity, pose interpolation and per-peer
spatial sound selection. Local driving remains immediate in its existing Rust
simulation; this is owner-authoritative presentation, with no player-to-player
collision resolution or competitive race authority yet. See
[network implementation](research/MULTIPLAYER.md), [original multiplayer evidence](research/MULTIPLAYER_ORIGINAL.md)
and [model census](research/MULTIPLAYER_MODELS.md).

The garage adds all 26 recovered in-race quads with their own PHY/AO/AR/linkage
resources, and male 18/female 27-node rigs with their matching 78-motion catalogs.
Original unlock/profile rules are not reconstructed. Networking preserves source
node shear and detached rider poses; state switches/teleports are discontinuities.
Host departure closes the event. Protocol/relay component commits are 4fa8ad0 and
53a0db4; browser integration and public release identity follow at the completed
release checkpoint. No source disc/executable/unused VUT or first-person arms enter
GitHub or the runtime package.

Initial host gates pass 299 core + 22 net + 3 relay + 11 WASM + 66 JS tests, strict formatting/Clippy,
15 release + 5 Worker checks and make verify. Actual two-browser code/link admission,
different models/rigs, remote movement, engine voices and host departure pass.
A hardware Metal touch-driven failed Nac Nac replicates Riding/Airborne/Wreck,
crash serial, independent rider separation, original impact audio and recovery;
local RTT measurements do not establish Internet or physical-phone performance.
The private native probe covers all 26 quads and both rigs’ 3,276 sampled poses.

The checked CI artifact includes the compiled Rust relay and the existing game
Worker hosts its routes and Durable Object, preserving the scoped CI credential. `/multiplayer/version` reports its configured checked
commit/WASM hash. Expanded required resources are staged separately in
`private/reports/multiplayer-content` and must be uploaded from a validated current-main
artifact using `upload_cloudflare_content.py --content`; public release still needs
CI, R2 publication and production browser verification. Preserve other agents'
water/snow and UI lanes while integrating current main.

The combined merge against water/snow main cec8a6d passes 303 core, 22 net,
3 relay, 11 WASM and 66 JS tests, plus 15 release/5 Worker checks. Serial
host browser gates retain multiplayer touch-wreck replication, all four wet-map
reset/Swim routes and 24 desktop/390px snow collision approaches.
## Verified public water and Caribou collision checkpoint

Main `b9a3a448d6d19103d63be8e6ec530974297b1870`, CI `38292520844` success,
is public on Cloudflare after PR #5 from `feature/water-snow-collisions` in the
separate managed worktree. The original five-second deep-water reset uses the
preceding cached surface and retained checkpoint. Public ENV04/05/07 routes
expire/reset at 348/349, 345/346 and 337/338 and return above water to the dry bank.
ENV08's shallower route exercises countdown clearing without expiration. All
four wet maps pass water-only rider buoyancy, Swim and recovery; ENV04 passes
at desktop/390px. Existing Caribou ramp and all five ski colliders needed no
new shape/offset: 776 per-face tree rays and 24 public desktop/390px approaches
cover wheel/body response and clean ramp launch/landing at 35.631ft/s upward.

Public HTTPS verification matches 30 authored files and 343 served objects,
with 103 dev-log entries / 90 inspected screenshots before the verification note.
R2 validates 344 objects and uploads only the new approved host snow PNG and
gallery metadata; no runtime resource changes. A first hash fetch resets its
connection; the complete retry passes. Public clean touch landing, Nac Nac
wreck/recovery/driving, title/menu/controller/Tioga/Controls/390px Caribou reload
without a disc prompt, F3/Pause Stats persistence and portrait fit, uninterrupted
ENV01–08 driving and dev-log filters/expandable evidence pass without JS/GL errors.
302 core + 2 WASM tests, 47 JS tests, strict Clippy/fmt, setup, 13 release/4 Worker
checks pass. The snow screenshot is a host capture; no public capture is claimed.
See [water/snow evidence and limits](research/WATER_SNOW_GATE.md). These bounded
source-resource routes and component probes do not prove complete gameplay,
render/wake fidelity, physical-phone behavior or PS2 hardware parity.

## Latest verified public rail checkpoint

`841f11c5d619cac92bc6033fe01e1e2d09cbdcc4` is public, CI 38287671823 success.
All 12 ENV08 curved rail sections now render at original scene placements on
desktop and 390px, with 36,027–49,735 and 6,041–8,366 changed pixels respectively,
versus zero before repair. Native 68 setter cases confirm the static engine
replaces saved PSG root translation/basis; the old browser composed both roots.
The scene-only decoder preserves vehicle/rider/vegetation/train bind decoding,
COL, source scene matrices and terrain heights. See research/RAILS.md.

Public all-eight-map driving loads 1,875 scene instances without unsupported
meshes, JS or GL errors. Normal title/menu, keyboard/controller-to-Tioga,
Controls and 390px Caribou reload pass without a disc picker. Original train
front lift/landing, 65.4826ft/s upward head-on launch, side wreck/recovery and
moving top/wheel contacts retain the same public results as 9ac7365. Original
sound/wheel effects have their separate verified 9ac evidence below. The incoming
f21 original-game UI merge and its devlog capture are preserved; combined
Chromium/WebKit UI and source/package gates pass. Inspection rail views are test
cameras, not a change to the gameplay camera or a complete PS2 renderer replay.

## Verified train and wheel public checkpoint

`9ac73657ba94045aa319d3eaed6d481162f242fd` is public on Cloudflare, CI
38285135155 success. HTTPS verification matches 29 authored files and 334 served
objects (93 devlog entries, 81 inspected screenshots before the verification
entry). Public train tests pass the same four source-route cases as the host:
a rideable front lift/airborne interval/landing, a 65.48ft/s upward head-on launch,
side-body wreck/recovery and top/drop wheel/body contacts. The bridge binds all 11
source collision-enabled cars through original list order, moving velocities,
sequential scene height queries and pose publication. No scripted front-launch
or unconditional side-wreck rule was introduced. See research/TRAIN_CONTACT.md.

Original dust, turning dirt chunks/spray, water and actual MUD roost pass all six
public desktop/390px gates, pause and context restoration without JS/GL errors.
They use at most two draws and 1,031,664 GPU bytes in the tested scene. Mud detection
retains terrain support and excludes MUD from established AR/AO geometry before
nearest-row generation, as proven by 001e92c0/0011eeb0. Native kernel comparisons
and these checks do not establish PS2 GS/VU, physical-phone or full-game parity.
See research/WHEEL_EFFECTS.md.

Four original rider impact samples now play from actual touch wrecks. Public
isolated PCM, native sampled deltas, no overlap/backlog, cached actor position,
recovery and pause/resume pass. Three original train samples retain 11 spatial
loops, near/far gains, pan/Doppler and context restoration. Public title/menu,
controller-to-Tioga, Controls and 390px Caribou reload pass without a disc picker.

The user asks to deploy these updates before repairing missing ENV08 rails; that
ordering is fulfilled. Read-only census finds all 68 track placements loaded.
Native root setters show that 12 curved meshes carry a saved PSG model translation
which must be replaced by scene placement, rather than applied twice. The separate
scene mesh decoder repair is now publicly verified above; terrain/COL/route
transforms are unchanged. Keep normal ATV/rider bind-pose decoding intact.

## Earlier verified public gameplay checkpoint

`0541085b495fd9e8d0a0dae59ba6c8ea186f49cc`, CI `38264831246` success,
is live on the public Cloudflare app. Original placed cylinders now provide
physical chassis, wheel and active wreck-rider contacts. Native constructor,
tree/contact and dispatch comparisons plus 266 core + 2 WASM tests pass; serial
host and public checks cover the real ENV04 tree contact, boundary
launch/separation/recovery, Nac Nac recovery/driving, clean landing, ENV01–08,
diagnostics, saved one-handed input, touch stunts and dev-log filters. HTTPS
verification matches 26 authored files and 323 runtime/gallery objects, with 79
entries and 70 inspected screenshots before the public verification note.

The public tree hit is identical to host step 484: one chassis callback/contact,
five proxy queries and no ordinary candidates. Public boundary recovery covers
1,733 frames, 559 boundary/cached-rider frames, three rider tree-hit frames and
60 flag-suppressed frames with zero plant expansion. A preceding steered public
attempt stalled near a hill; a fresh run passes. The temporary lean-assist test
branch never sent lean input and was removed. Host and public routes are not
identical native gameplay replays. Public landing measures 0.637466ft tire overlap
across 210 riding frames/no skips, versus host 0.393066ft/201; different inputs and
timing prevent causal improvement/regression claims. Tire overlap remains open.

The stranded Riding pose is below the source angle limits, and native chassis
support is permitted. A later native phase audit confirms state0 has no independent
animated rider physics/query; the normal AR chassis/classifier path handles riding
contacts. Do not add a guessed rider query. Replay the complete
classifier/contact inputs before changing automatic recovery. Do not invent an
upright snap or assume wheel contact absence mandates ejection.

The user has promoted visible water and the working Arlington Stream train
(ENV08), and requested more informative one-finger touch feedback. These are
parallel feature lanes, now delivered as the public checkpoints above: water uses original WaterFile,
signatures and material resources; train uses original procedural/VUE routes and
models; mobile feedback preserves existing controls. Feature agents may commit,
push and complete their own tested checkpoints without waiting for every lane.
Coordinate only shared-file staging and current-main publication, keep required
checks and lighter-agent dev-log upkeep, and never publish WIP. Remaining original
priorities—wreck/recovery, stunt fidelity, rider/ATV animation, HUD/modes,
terrain/static collision, terrain LOD and vegetation—remain incomplete.

## Vegetation host gate, subsequently released as 0541085

The authored simulation now expands the original ESB cylinders for chassis,
wheels and the active wreck rider. All eight Freestyle ecosystems contain only
kind-1 cylinder records: 40 definitions and 94,992 placed proxy slots. Original
sixteen-sided open meshes, tree ordering, packed translation, mixed candidate
order and separate supplied/cached-rider gates are preserved. No new runtime
resource is needed. Synthetic contacts verify chassis velocity rejection and
attached rider response; the full gate passes 266 core + 2 WASM tests, 39 JS,
strict formatting/Clippy, setup, 10 release/4 Worker checks and hygiene. Native
component comparisons and their hooks are documented in
[vegetation evidence](research/VEGETATION_COLLISION.md).

The serial host gate passes original ENV01–08 loading/driving, failed Nac Nac
separation/ground/recovery/driving, clean landing, desktop/portrait diagnostics,
saved one-handed controls and touch stunts. A dedicated ENV04 normal-throttle
route has one tree chassis callback/contact at step 484. The normal-keyboard
steered ENV04 boundary route passes launch, paired query flags, rider separation
and inward automatic recovery across 2,126 frames. It records 24 chassis, one
wheel and eight active wreck-rider frames with plant callbacks; 553 frames are
boundary flight and 710 use the cached rider query. Full-batch clean landing
still measures 0.393066ft maximum tire overlap across 201 riding frames, none
skipped. This is not a clipping fix.

The historical straight-throttle ENV04 boundary route now times out after
repeated wreck/deflection; ENV03 also times out after one recovery with a tilted
Riding ATV and no grounded wheels. A first steering-harness attempt reversed
the correction sign; the corrected route passes. Four maps passed before an
ENV05 startup timeout without browser errors on the five-slot host HTTP backlog;
a fresh uninterrupted ENV01–08 run on a 128-slot backlog passes. The latter
also serves 48 simultaneous authored-module requests while private paths remain
404; the prior startup cause is not conclusively isolated. This component is
host-verified and pending public release. Existing mobile controls through
`e6878eb` are retained. Normal riding rider collision, exceptional masks, dynamic
objects, unshipped proxy kinds and full replay remain open. Priority 1 next is
the stranded tilted recovery case and wet-wreck coverage; priority 2 then
addresses original wreck policy and controlled rotation/stunt behavior. No
additional scored flip was established by the read-only source audit.

Preceding verified public gameplay component: `f6353c37684063777cd5d0cff1d87e99a0de615f`,
CI `38212608120` success. The active wreck rider now borrows the Vehicle's
shared scene cache, using its animated thirteen-spoke bounds and inclusive/current
eligibility rules. The wreck camera uses its independent original partition query
and retains the obstruction latch. Native probes match 500 rider spheres,
12 paired boundary flag cases and 120 oblique camera queries; original-content
probes cover 1,694 poses and five recovery directions with an empty scene.
The combined build passes 251 core + 2 WASM tests, strict Clippy/formatting,
39 JS tests, 10 release/4 Worker checks and setup/hygiene. Serial host checks pass
clean landing, failed Nac Nac separation/ground/recovery/driving, paired boundary
flags and recovery, all eight maps, diagnostics and saved one-handed controls.
Landing overlap remains 0.393066ft across 201 riding frames with none skipped.
Three concurrent host startup runs timed out without browser errors; a fresh
single run and the subsequent serial suite pass, but concurrency is unverified.
Public HTTPS verification matches 26 app files and 319 runtime/gallery objects
(74 entries, 66 screenshots before its verification entry). The serial public
suite passes the same routes, including all eight maps, with no browser/GL errors.
The Nac Nac trace covers 195 cached rider frames and 195 direct camera queries;
the boundary trace covers 490 rider refreshes and retained plants. No physical
rider-prop contact or camera latch occurs in these public routes; those response
checks use synthetic core geometry. Public landing overlap is also 0.393066ft
over 201 riding frames with none skipped. Normal riding rider collision, MUD
metadata, custom masks, physical plant proxies, dynamic objects and full original
replay parity remain incomplete. See [scene integration](research/SCENE_INTEGRATION.md)
and [camera](research/CAMERA.md).

Preceding verified public gameplay gate: `f27869c84b1ea09a8425de50b6bdf50935d2ba54`,
CI `38210131335` success. Source-verified spring torque now preserves the
Model inverse-transform order. A single original static/ESB partition and 40/50ft
cache supplies quad wheel/chassis candidates and recovery's plant-clearance gate.
Physical vegetation proxies, exceptional categories/dynamic objects and the
newly recovered rider cache binding remain incomplete. Landing/wreck/slope tests
now record every delivered WASM batch step. The isolated combined build includes
committed mobile controls `179fd92` and passes 245 core + 2 WASM tests, strict
Clippy/formatting, 39 JS tests, release/Worker checks, all eight maps, clean landing,
Nac Nac recovery, uneven turns, boundary launch/recovery, one-handed touch and
desktop/portrait diagnostics. Recorded landing overlap is 0.392776ft across 199
frames; this is not an identical before/after comparison or a clipping fix.
Public HTTPS verification matched 26 authored files and 317 served objects
(72 log entries and 64 screenshots before this verification entry). All eight
maps, clean landing, wreck/recovery, boundary launch/recovery, one-handed mobile,
dev-log filters and desktop/portrait diagnostics passed publicly. Full-batch
public landing capture measured 0.392639ft overlap across 184 riding frames with
none skipped. This is not an identical before/after comparison or a clipping fix.
Rider shared-cache integration followed in `f6353c3` above. See
[coupled landing](research/COUPLED_LANDING.md) and
[scene integration](research/SCENE_INTEGRATION.md).

Verified public continuation `01c9cbb0c8358ba43f589dab1e342c3f746f0a1b`, CI
`38207020177`: terrain normals use the original single-round reciprocal
square root; detached rider water velocity retains the previous integration cache,
including across recovery and another wreck; wheelie accounting/HUD callbacks now
run in the live WASM sequence with the original scoring flag off. Native checks
cover 1,500 terrain queries, 300 water kernels, 64 wreck-cache lifecycle cases and
2,070 wheelie operations. Shared spatial/vegetation loader contracts are documented;
their helpers still do not supply live vegetation contacts.

That combined build passed 233 core + 2 WASM tests, strict Clippy/formatting,
33 JS tests, 10 release and 4 Worker tests, setup/hygiene checks, clean landing,
failed Nac Nac separation/contact/recovery/resumed driving, desktop/portrait F3 and
Pause diagnostics, immediate touch throttle/brake/reverse and one uninterrupted
ENV01–ENV08 load/drive run. Landing tire overlap remains 0.497238ft across 62
riding frames in its host sample. Public HTTPS verification matched 26 authored
files and 313 served objects; public landing, wreck/recovery, touch and one
uninterrupted eight-map run passed. The original RAF clearance samples were
incomplete and are not comparable; later full-batch capture is the measurement
baseline. This is not a clipping fix or complete original gameplay parity. See
[terrain contacts](research/TERRAIN_CONTACT.md), [water](research/WATER.md) and
[HUD](research/HUD.md).

Latest public handling release: `35d328ef61c88619f35291e13fc3193ed9736e15` (CI `38205280162`, success) passes public clean landing, keyboard slope turns, wreck/recovery and immediate touch input. HTTPS matched 26 authored files and 312 served objects (66 entries, 59 screenshots before this verification entry). The public slope route has 148 riding frames, zero wreck frames and 13.915ft elevation variation; this is not an identical before/after comparison. Public geometric clearance is pending; residual clipping and full handling parity remain unclaimed. Boundary and uninterrupted all-eight-map evidence remains `22ab240` below, with those paths unchanged. Terrain contact and rider-water cache audits continue.

Latest public boundary release: `22ab240cce0cea813b482ee20dbc22d5908ec69c` (CI `38204399428`, success). The public ENV04 keyboard route passed inward/upward launch, rider separation, full-ray recovery acceptance and cleared boundary state. HTTPS matched 26 authored files and 310 served objects (63 entries, 57 screenshots before the new verification entry). This release passed one uninterrupted public ENV01–ENV08 loading/driving run without JavaScript/WebGL errors. Wet ENV04/05/07/08 load all four live dynamic channels with finite observed heights; dry maps retain null fields and the -256 sentinel. Water-height connection does not imply rendered water/wakes or full rider-water parity. Callback cadence, vegetation collision, RNG ordering and residual tire overlap remain open.

The public checkpoint restores the original four-point suspension travel-stop actor, missing from the coupled vehicle simulation. User screenshots showed deep burial on landing and cross-slope turns. The original stop removes inward center velocity and force before wheel springs, including during wrecks; it does not snap position upward. Exact instruction probes cover constructor points, slope contacts and actor gates. Public clean-landing measurement reduced deepest visible tire overlap from 1.9423 to 0.4792 feet; residual overlap is still present and full coupled parity remains unproven. Synthetic flat/slope tests, browser clean landing and wreck/recovery pass. A separate host steering trace crosses 14.29 feet of elevation with at most 0.3196 feet of measured riding tire overlap, then hits scenery; this is not an isolated incline-parity test. The wreck camera implements the original ten-foot obstruction ray and activation-long position latch. At that public suspension checkpoint, water heights and spatial candidate ordering were verified components pending live integration.

Verified public follow-up: F3 cycles Off/Performance/Debug, mirrored by a persisted Pause → Stats control. The overlay reports measured render/WASM timing and existing physics diagnostics; browser tests verify keyboard repeat, persistence and portrait layout. Mobile right-side press now requests throttle immediately, with backward drag braking/reversing through the existing Rust direction gate. A source-verified suspension fix uses the Model forward axis for the steep-angle force gate. Clean landing, geometric clearance (0.4971-foot residual), wreck recovery and the expanded touch route pass. The original boundary launch is located and being integrated separately; it is not yet live. See [diagnostic measurement limits](research/DIAGNOSTICS.md).

Current gameplay checkpoint: the user prioritizes wreck/recovery, tricks, ATV/rider animation, HUD, terrain/static collisions, terrain LOD/detail and original vegetation, in that order. The user's rail/vertical-ATV screenshot prompted a coupled movement audit before further visuals. Original wheel rays now support static props; grip/controller inputs keep native outer/substep cadence; engine/drag receive total speed separately from local-forward speed; physical tire axes use the preceding mechanical pose; upright chassis terrain contacts use the native body-up gate. Forced hard-landing controls and the original reverse-chord speed gate are connected. See [movement evidence and remaining gaps](research/MOVEMENT.md).

This checkpoint also connects native stunt scoring and repeat rollback, original HUD fonts/controllers, hard-landing clips, detached rider static contacts/root motion and the two-subject wreck camera. Host browser tests pass a clean preload takeoff/landing and a failed Nac Nac → separation → rider terrain contact → automatic recovery → driving sequence, with the failed score unawarded. Component instruction probes and these browser tests do not prove whole-game or hardware parity. Water force kernels and dormant hand-IK behavior have isolated verification; complete water integration, object camera clearance, dynamic collision, full racing/HUD modes, terrain LOD/visibility and vegetation parity remain unfinished. Runtime assets, native probes and screenshots remain outside Git/CI. The app is authored Rust/WASM, not PS2 emulation.

## Current scope

The setup-only session is historical. On 2026-10-11 the user authorized implementation of ATV Offroad Fury 2 in Rust for browser play. Preserve the original as the baseline, including music and sound; follow with optional modern graphics, genuine 16:9 presentation, animations, and more realistic physics retaining arcade accessibility.

All authored UI should evoke the PlayStation 2 era. Support browser-exposed controllers plus intuitive iOS touch driving/trick gestures. The user explicitly selected Reverse Engineer Anything (REA) as an analysis tool, with CLI versus MCP left to engineering judgment. The latest steering prioritizes the actual fullscreen game and reverse engineering over more workbench or website presentation. See [strategy](research/IMPLEMENTATION.md) and [resources](research/RESOURCES.md).

## Current result

A Rust workspace and fullscreen browser driving reconstruction exist. **This is not the full game or a completed decompilation.** The public production app is now on Cloudflare Workers, with original runtime resources and approved screenshots served separately through its R2 binding. Its Play landing enters the recovered title → Main Menu → Single Player → Freestyle route. ENV01/02/03/05/06/07/08 are public, including Caribou Peak, Tioga Valley and Arlington. The public frontend test passed controller Play, title/menu routes, Tioga driving, pause/Controls and 390px Caribou touch input. The public seven-map test passed loading and movement with terrain detail and vegetation, with no JavaScript or WebGL errors. All 281 initial public runtime/manifest/PNG objects passed size, SHA-256 and MIME checks, and all 21 songs remain in the manifest. The initial Cloudflare release identifies main `ad605e1b589720bd9b093ae84c25a753ea65a0fc`, CI `38183457495`.

The ATV's 27 mechanical nodes, recovered vehicle routines, static type-0 obstacle contacts and single-player chase camera run in authored Rust/WASM. Normal rider pose blends and selected original stunt gestures are connected; full scoring/chaining, crash integration, IK, racing and progression remain unfinished. Tioga's `ruins.col` contains four NaN plane records for zero-area faces; only those faces are omitted while original indices/tree order are preserved. Terrain detail uses the original `_d.pst` layer, and vegetation visibility is provisional. Most original menu callbacks/modes, race HUD, remaining maps and complete PS2 rendering/audio fidelity are still missing. The previous Sites v8 is a historical two-map release. Development inspection remains separate at `/workbench.html`.

| Capability | Evidence and limits |
| --- | --- |
| RES/LSI directory and payload recovery | 487 archives, 58,598 member entries, 91,522 zlib blocks; zero archive failures |
| VAG preview decoding | 192 standalone files (162 mono, 30 stereo), 798 packed entries decoded; SPU2 mixer/resampler/reverb and game scheduling not implemented |
| PST preview decoding | All 934 loading archive PST entries and 13 ATV01 material textures decoded; indexed 4/8-bit mip chains validated |
| Fullscreen renderer | Public Cloudflare: title/menu/Freestyle route, seven environments ENV01/02/03/05/06/07/08; animated 27-node ATV linkage and 18-node normal rider pose blends; recovered single-player follow camera; static type-0 collision query; recovered-art pause panel and revised touch controls. Other original menu callbacks/modes, race HUD and additional maps are not implemented. |
| ELF reconnaissance | 5,599 direct-call target candidates; mixed code/data means these are not verified function identities or boundaries |
| Browser integration | Actual WASM, image pixels, audio playback clock, malformed input, private path isolation, synthetic gamepad, keyboard/focus, narrow layout, simultaneous touch/cancellation, camera cycling, and graphics-context recovery tested |
| Real content in browser | Initial Cloudflare release: main `ad605e1b589720bd9b093ae84c25a753ea65a0fc`, CI `38183457495`; public seven-map, frontend and 281-object HTTPS verification passed. |
| Original fidelity | Component instruction-interpreter comparisons cover selected vehicle, camera and collision routines; no PS2 hardware validation or whole-game replay equivalence yet |

The numerical audit covers entries, not unique assets. Binary decoding and an advancing audio clock do not prove visual/audio fidelity. A stereo reference comparison against FFmpeg differs by up to 70/68 PCM units (left/right); it is not a bit-exact pass. See [format notes](research/FORMATS.md).

## Project and access

- Project: `/Users/nrgapple/Projects/atv-offroad-fury-2-web` on MML (`ssh mac-mini`, user alias `mml`).
- Host: Apple Silicon arm64, macOS 27.0; private GitHub remote https://github.com/nrgapple/atv-offroad-fury-2-web with main CI checks and WASM builds.
- Applicable parent instructions: `/Users/nrgapple/Projects/AGENTS.md`.
- Original download was preserved at `/Users/adamgeiger/Downloads/ATV Offroad Fury 2 (USA) (v2.00).zip` during setup.
- Public HTTPS production: `https://atv-offroad-fury-2.champtoday.workers.dev/`; screenshot log at `/devlog.html`. GitHub main checks/builds deploy authored application assets. A dedicated R2 bucket holds only validated runtime content and approved screenshots, uploaded separately from the private host at completed checkpoints. The publication heartbeat is retired; releases follow `research/CI_CHECKPOINTS.md`. See `research/DEPLOYMENT.md` for current status and release evidence. The ISO, BIOS, raw boot executable, unrelated disc data and reverse-engineering output remain private.
- `make serve` is a foreground development process, not a persistent service. The server serves an explicit static-file allowlist, never `private/`, `references/`, or the repository root.

## Exact source build

| Item | Observed value |
| --- | --- |
| Disc boot file | `SCUS_972.11` |
| `SYSTEM.CNF` version / video mode | `2.00` / `NTSC` |
| ISO bytes | `4,487,446,528` |
| ELF bytes | `3,543,492` |
| Architecture | ELF32 little-endian MIPS |
| Entry point | `0x00100008` |
| Named symbols/functions | `0`; symbol/string table sections are empty |
| Disc files | `742` |
| Main load segment | file offset `128`, guest address `0x00100000`, file bytes `3,542,912`, memory bytes `4,348,160` |

ISO SHA-256: `fb710837bbb1852f53706df6086476b507b63cc9e522777bf91446ef04cf708c`

ELF SHA-256: `11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`

Disc inventory: 368 `.RES`, 119 `.LSI`, 192 `.VAG`, 29 `.IRX`, 10 `.PSS`, plus auxiliary files. The original ZIP and ISO remain under `private/source/`. Extract selected files on demand instead of duplicating the entire disc.

## Reproducible commands and evidence

See [README](README.md) for `make verify`, `check`, `web`, `serve`, `audit`, `code-map`, and `browser-check`. Native extraction commands:

```sh
~/.cargo/bin/cargo run --locked --release -p atv-tools -- disc-extract \
  "private/source/ATV Offroad Fury 2 (USA) (v2.00).iso" /MODULES/PADMAN.IRX private/extracted/MODULES/PADMAN.IRX
~/.cargo/bin/cargo run --locked --release -p atv-tools -- list path/to/file.res
~/.cargo/bin/cargo run --locked --release -p atv-tools -- extract path/to/file.res 0 private/extracted/member.bin
~/.cargo/bin/cargo run --locked --release -p atv-tools -- wav path/to/file.vag work/verification/preview.wav
```

Persistent reports (ignored by Git):

- `private/reports/import.json`, `disc-files.json`, `elf.json`: source identity and layout.
- `private/reports/content-audit.json`: all archive members, decode statuses, dimensions, and audio metadata.
- `private/reports/code-candidates.json`: provisional call-site/target reconnaissance.
- `private/reports/audio-oracle.json`: measured FFmpeg comparison differences.

Screenshots and temporary audio comparisons live under `work/verification/`. All bulk build/test outputs use the mounted SSD; preserve the existing symlinks.

## Environment

- Rust `1.99.0`, Cargo at `~/.cargo/bin/cargo`; targets `aarch64-apple-darwin` and `wasm32-unknown-unknown`, rustfmt and Clippy installed.
- Python 3, Git, system Node `v25.8.1`, FFmpeg, Xcode clang, uv, Homebrew, and native Capstone available.
- Bundled Node `v24.19.0`: `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node`; use this supported major for REA rather than system Node 25.
- Bundled Playwright module: `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright`; Chromium installed for browser checks.
- OpenJDK `26.0.2.1` available. REA 6.4.0 CLI rejected the exact ELF's machine-specific MIPS architecture. Direct Ghidra 12.1.4 with Emotion Engine: Reloaded 2.1.38 now imports and decompiles selected routines using `r5900:LE:32:default`; all original load bytes match. See [REA and native analysis evidence](research/REA.md). REA integration remains unresolved; MCP does not fix its processor admission.
- Rust dependencies are `serde`, `serde_json`, and `miniz_oxide`; browser code needs no npm build dependencies. Playwright is only a test dependency.
- PS2Recomp and the first-game reference are research inputs, not project dependencies. Their source has not been incorporated or built as this runtime.

## Next work

The public Cloudflare app and local browser build now load original runtime content automatically through their manifest. `/workbench.html` retains inspection tools. See `research/GEOMETRY.md` for the 26-quad/40-terrain audit and `research/COLLISION.md` for collision coverage. Physics data and exact-build loader evidence are in `private/reports/physics-*.json`; selected pseudocode and instruction bytes are in `private/reports/ghidra/`. Reuse the database through `tools/analyze_ps2.py`; do not repeat imports or mistake observed functions for verified semantics.

Continue from the public Cloudflare Play landing page and title-to-Freestyle route; seven environments are connected. The 59-page UI archives, 120 layouts, fonts, and animation data now feed the route, but most pages and callbacks are not yet implemented; race HUD and other modes remain to be built. Tioga collision loads after a targeted skip of four zero-area faces with NaN planes; see `research/COLLISION.md`. The revised touch controls and one native-gated ramp jump now pass public browser checks. The public v8 landing page keeps only Play and Dev log; Play enters the unchanged game at `/play.html`. ENV03/05/06/07/08 now load and drive locally after fixing unquoted-space parsing, a material UV default and packed `trussbridge.col` signed-16-bit expansion; all seven routes now pass the public Cloudflare regression. The seven-map detail build passed tests with original 64×64 detail textures, moving full-sample patches and batched vegetation. 390px renderer checks passed gamepad, touch cancellation, audio-engine startup and graphics-context restoration; menu navigation passed immediately before the terrain-only shader changes. The ENV01 ecosystem archive lists 37,948 vegetation placements, and the local renderer batches up to 1,500 nearest placements per map into 7–24 draws. The original `_d.pst` RGB+alpha detail layer now blends publicly at recovered world scale with near-pass modulation. Projected-coverage pass selection is still pending; distant WebGL mip averages soften detail, and fog/GS rounding plus vegetation visibility are not yet matched to the original. The terrain detail patch already follows the player; current work extends terrain range from 300 to 864 feet, not all scenery. Next broaden menu/map coverage, recover original spatial-grid obstacle order and remaining dynamic collision types, and extend rider animation with input-driven crouch/look, IK, tricks and crashes, plus races and progression. The single-player follow camera and ATV mechanical linkage now run in the browser; camera clearance against objects, original render-distance policies and whole-game fidelity remain incomplete. Validate each recovered component against original instructions without presenting that result as PS2 hardware or full-game equivalence. REA still needs an explicit R5900 provider profile; direct PS2 Ghidra observations are not REA Evidence.

Keep enhancements behind explicit options after the baseline. The full-screen renderer uses the recovered single-player follow camera and animated ATV mechanical parts, with provisional terrain shading/LOD. Normal riding animation blends are connected in public v5, while crouch/look inputs still use constructor defaults and IK, tricks and crash poses are unfinished. Public v7 adds the recovered title/menu/Freestyle route, two driving scenes, a pause panel, and revised touch controls. The v7 public ramp run demonstrated native preload and airborne wheel contacts; physical iOS behavior and original stunt/scoring systems remain unverified or incomplete. Public v8 serves the minimal Play/Dev log landing page at `/` and routes Play to the unchanged game at `/play.html`. Object-aware camera clearance, moving obstacle collisions, full race systems and original multiplayer/split-screen presentation also remain incomplete. Widescreen framing preserves recovered 4:3 vertical framing while extending the horizontal view as a browser adaptation. The tested synthetic controller and touch paths do not verify physical Bluetooth controllers or iOS Safari. Audio playback is connected, but original game-wide mixer behavior is still unfinished.


Host boundary follow-up: a normal ENV04 keyboard drive now passes edge crossing, inward/upward launch, rider separation and recovery about 1,379 feet inward. The regression requires more than 100 feet of inward return, one launch, query-flag clearing after the strict one-second gate and cleared recovery fields. This is not public yet; checkpoint callback cadence, vegetation collision coverage and full RNG/gameplay parity remain unverified. Host integration gates now pass touch Nac Nac → wreck/separation/ground/recovery/driving, clean preload landing with Debug, and F3/Pause persistence at desktop and portrait sizes; workspace 218 core + 2 WASM tests and strict Clippy pass. Map coverage comes from two runs: ENV01–04 passed before an undiagnosed ENV05 startup timeout; a fresh ENV05–08 run passed. Wet ENV04/05/07/08 load four live dynamic signature channels and finite changing heights; dry maps have a null field and cached -256 sentinel. Visible water surfaces and wakes remain absent, and complete water/rider parity is unclaimed. This build is host-verified and pending public release. See research/BOUNDARY_LAUNCH.md.


Historical host wheel-center checkpoint (subsequently released as `35d328e`): the first wheel velocity/spring substep now retains the preceding cached world center until integration; geometry, torque and the chassis contact solver keep their recovered new-center rules. A 600-case original `001ed330` replay executes 386,315 instructions with zero compared binary32 mismatches, versus 1,813 differing components from the former immediate-center input. Host clean landing, both-direction keyboard cross-slope turning and wreck/recovery pass; 222 core + 2 WASM tests, strict Clippy, formatting and `make verify` pass. Landing minimum clearance is unchanged at -0.4970245ft. Slope clearance is -0.3231964ft before/-0.3015594ft after over 14.457ft elevation, but props and 119 riding/30 wreck frame exclusions prevent isolated slope parity or causal-improvement claims. Three vegetation helper tests are registered but collision is not live. The inspected landing screenshot and evidence are recorded in `research/devlog.json`; no terrain-clipping fix was claimed at this host gate. The subsequent public verification is recorded above. See `research/MOVEMENT.md`.
