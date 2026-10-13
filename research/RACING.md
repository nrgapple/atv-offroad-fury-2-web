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
