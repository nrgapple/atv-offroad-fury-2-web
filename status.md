# Active priority work and coordination — 2026-10-11

Owner: Codex thread `01a12dc0-d565-7a42-bac2-cef215336c20`.
User requested coordination with existing thread
`01a12c55-0503-7591-8d0d-7f91fb03cdb4`. Direct message delivery initially failed
with “already has an active writer”; this file is the shared claim record.

Priority order: wrecks/recovery, stunts, ATV/rider animation, HUD,
terrain/static collisions, terrain detail/LOD, original vegetation placements.
Full parity remains the goal; component checks do not establish whole-game parity.

Active claims for this thread (handling audit after user rail screenshot):
- Wreck/collision agent: core wreck*.rs, obstacle.rs, collision.rs; type3 queries, rider-height impact and chassis terrain gates.
- Stunt/animation agent: core stunt.rs, rider_pose.rs, rider_control.rs, stunt_score.rs, rider_ik.rs; hard-landing and reverse controller gates.
- Movement agent: wheel_support.rs, research/MOVEMENT.md; coupled wheel/static-prop support and original controller field provenance.
- Root: vehicle.rs, camera.rs, water.rs, HUD/WASM/game integration, narrow reverse-chord changes in web/bindings.js and relevant tests, verification and status.md.
- Logger completed two inspected host-only HUD/wreck checkpoints in research/devlog.json; it will resume after a verified handling checkpoint.

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
