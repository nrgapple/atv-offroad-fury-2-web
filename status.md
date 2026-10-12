# Active priority work and coordination — 2026-10-11

Owner: Codex thread `01a12dc0-d565-7a42-bac2-cef215336c20`.
User requested coordination with existing thread
`01a12c55-0503-7591-8d0d-7f91fb03cdb4`. Direct message delivery initially failed
with “already has an active writer”; this file is the shared claim record.

Priority order: wrecks/recovery, stunts, ATV/rider animation, HUD,
terrain/static collisions, terrain detail/LOD, original vegetation placements.
Full parity remains the goal; component checks do not establish whole-game parity.

Latest public gate: `0f074b4558f864ea4e4a8103b017b60e102c6690` (CI `38202065109`, success). Public F3/Pause diagnostics, persistent mode, portrait layout and pointer transparency; immediate right-press throttle/touch cancellation; and Debug-enabled clean preload landing passed. HTTPS verification matched 26 static files and 308 runtime/gallery objects (60 entries, 55 screenshots before the new verification entry). Headless FPS is not a phone benchmark; residual tire overlap remains. Boundary launch is not part of this verified release.

Latest user steering: ATV clips into terrain on landing and turning across
inclined sides. The missing original four-point suspension travel-stop actor is
now connected before the wheel springs. Original-instruction comparisons cover
600 solver cases and 500 constructor cases (480918 instructions, all penetration
decisions and 11400 numeric components match). Browser tire/terrain measurements
improved from -1.9423ft to -0.4836ft minimum clearance in the clean ramp landing.
Residual overlap remains; this is not complete penetration or handling parity.
No speculative ride-height clamp was added.

Suspension release gate: 195 core + 2 WASM tests, strict Clippy, formatting, web build,
31 JS tests, 10 release tests, Worker tests and setup/repository checks pass.
Real-touch clean landing and failed Nac Nac → separation/contact/recovery/driving
pass. All eight environments load and drive without JS/GL errors. The first run
timed out loading env05; a fresh env05–08 run passed. Wreck-camera obstruction
matches 600 original ray cases and 240 obstructed native frames. Static scene
partition/cache, Environment bounds, obstacle world bounds and dynamic water
height components have private instruction comparisons but are not yet connected
to live scene collision/water. Native actor dt uses a smoothed frame timer; a fixed
30Hz or 60Hz native tick has not been established.

Additional user requests: persisted performance/debug overlay with an F3 hotkey
and pause-menu control; use it during browser diagnosis. Root owns the narrow
frontend/game/worker/ui_menu and static-file allowlist changes for this feature.
The other thread's music/binding/menu behavior is preserved. User also requests the
original out-of-bounds launch; wreck agent located the native velocity launch in
001e5260 and is recovering its exact boundary predicate before live integration.

Active claims for this thread:
- Wreck/collision agent: private coupled original-quad replay and remaining landing divergence.
- HUD agent: native wheelie scoring/display path. The animation audit recovered +Y/−Y hand hinges and confirmed normal modes disable them; narrow API/test changes are validated and pending commit.
- Movement agent: original water initialization, ripple and frame updates.
- Root: vehicle.rs, camera.rs, scene/obstacle integration, WASM, browser verification and delivery.
- Lighter logger completed the inspected public suspension/camera verification entry; metadata publication is next.

The other thread committed audio/music/menu/browser changes as 0906822 and its public verification as 1de08b2. Preserve those changes.
Coordinate before editing/staging atv-web/src/lib.rs, web/game.js,
web/frontend.js, build/release tools, Makefile and shared devlog.
No blanket staging. Serialize Ghidra access and browser builds through root.

Current production is public Cloudflare, with authorized separate R2 runtime
content and screenshots. Eight environments are connected; the complete port is
unfinished. See AGENTS.md, README.md and research/DEPLOYMENT.md for current facts.


Integration now adds narrowly scoped gameplay sections to vehicle.rs and
atv-web/src/lib.rs, preserving the other thread's lifecycle/audio/settings edits.
Other thread is testing an isolated work/checkpoint-candidate on port4177.
Priority-parity host server is port4180; web/dist currently includes both sets of
uncommitted changes. Do not claim public delivery from either host-only URL.

Core checkpoint: native wreck camera sequence (240 frames / 396390 original
instructions, max error 0.000012), rider static-prop spokes, downward recovery
support ray, native hard-landing clips and stunt pose snapshot rules. Browser crash
and clean-jump regressions running; scorer recovery continues with stunt agent.
Full parity is unfinished, especially water/attached rider contacts, camera object
clearance, scoring/game-mode integration, IK, HUD, dynamic collision, LOD and
vegetation visibility. Public release remains the last verified deployment until
these changes pass CI and public checks.

Current checkpoint: original scoring tick/finalization callbacks and failure repetition rollback are recovered, HUD Rust controller/art bridge is integrated. First host startup identified missing hud.js in the local server allowlist; fixed and retesting. Root water force kernel matches 300 original cases but is not live until dynamic surface heights are recovered. Attached rider Model root, animation root-motion rebasing and native callback order now have isolated instruction comparisons; browser regression is still the integration gate.

Host HUD, original Nac Nac failure/separation/recovery and clean ramp landing passed before the current handling changes. Newly confirmed coupling defects: tires sampled terrain only while chassis hit props; engine/drag used local-forward instead of total speed; controller and grip input cadence differed; hard-landing clips did not suppress physical controls. Integrating the native contracts now. The native reverse chord also preserves brake above its speed gate, unlike the current JS binding. External-thread message transport is down (127.0.0.1:62391), so this file records the narrow bindings claim. Terrain LOD/vegetation work waits behind these higher-priority behavior corrections.

Handling checkpoint verified on host: all eight environments load and move without JS/GL errors; real-touch clean landing and failed Nac Nac → separation/contact/recovery/driving pass; remapped keyboard/controller bindings, native chord input, lifecycle cancellation and stale-preload replay pass. Core183 + WASM2 tests, strict workspace Clippy,31 JS tests,10 release tests, Worker tests, setup validation and Python syntax pass. Review corrected preservation of cached rear speed across recovery. Publishing this authored checkpoint through existing main CI next; no public verification claim yet. Remaining original-world collision order/categories, water and camera object clearance continue after checkpoint; full parity remains unfinished.

Public checkpoint verified as 0f074b4: immediate primary right-touch throttle and
back-drag brake/reverse; F3/permanent Pause Stats control; existing live physics
diagnostics in portrait and desktop; native forward-axis suspension attenuation
correction. Browser diagnostics/touch/landing/wreck routes, geometric clearance,
Rust workspace, Clippy, formatting, JS and release checks pass. Remaining landing
overlap is0.4971ft in this trace. Boundary and dynamic water components are being
validated separately and are not connected to the published runtime yet.


Host boundary follow-up: a normal ENV04 keyboard drive now passes edge crossing, inward/upward launch, rider separation and recovery about 1,379 feet inward. The regression requires more than 100 feet of inward return, one launch, query-flag clearing after the strict one-second gate and cleared recovery fields. This is not public yet; checkpoint callback cadence, vegetation collision coverage and full RNG/gameplay parity remain unverified. Host integration gates now pass touch Nac Nac → wreck/separation/ground/recovery/driving, clean preload landing with Debug, and F3/Pause persistence at desktop and portrait sizes; workspace 218 core + 2 WASM tests and strict Clippy pass. Map coverage comes from two runs: ENV01–04 passed before an undiagnosed ENV05 startup timeout; a fresh ENV05–08 run passed. Wet ENV04/05/07/08 load four live dynamic signature channels and finite changing heights; dry maps have a null field and cached -256 sentinel. Visible water surfaces and wakes remain absent, and complete water/rider parity is unclaimed. This build is host-verified and pending public release. See research/BOUNDARY_LAUNCH.md.
