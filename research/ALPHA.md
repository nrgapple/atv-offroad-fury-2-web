# Settings → Alpha (experimental hype effects)

**Status: authored experiments, each off by default.** None of these exist in
ATV Offroad Fury 2. The user asked for experimental "hype and immersion"
presentation to try and rate, separate from the established Extras. Every row
is an independent On/Off toggle saved in `localStorage['atv-alpha']`. Except Arcade
flips (an authored gameplay option, see below), none changes the recovered
vehicle solver; slow motion only delays when the
next fixed 1/60 s step is requested.

The browser owns the list (`web/alpha.js`, `ALPHA_FEATURES`); the Rust menu
(`ui_menu.rs`, page `alpha`) renders whatever rows it receives as
`alpha:<id>` actions. Adding or retiring an experiment needs no menu change.

## Experiments

`web/hype.js` (`HypeDirector`) reads each delivered 60 Hz frame. It measures
clearance above the terrain under the quad, the highest point of the current
jump, the apex (vertical velocity crossing zero at least 10 ft up) and the
touchdown speed. A jump counts as big air from 7 ft and saturates at 24 ft.

| Row | What it does |
| --- | --- |
| Air slow-mo | At the apex of a big jump, time eases to 0.3× over 0.12 s, holds 0.55 s and eases back over 0.3 s (real time). It ends early on touchdown, a wreck, or within 4 ft of the ground. Freestyle only: never in races or online rides. Frame interpolation stays on during it, even on the Original preset. |
| Big-air camera | Pulls the chase camera back horizontally (up to 1.45×), lowers the eye 3 ft and widens the view 10% in big air, held until touchdown. Landings drive a spring that compresses the camera down and back, scaled by touchdown speed and jump height. It never drops the eye below terrain + 2 ft. |
| Speed & air muffle | Low-passes the soundtrack (decoded and streamed) from 22 kHz towards 320 Hz and dips it up to 30%: up to half strength between 40 and 72 mph, 85% in big air, fully during slow motion. A filtered wind rush on the SFX bus rises with it. Touchdown snaps both back within about 30 ms. |
| Apex sun glare | A screen-space additive pass near the apex of big jumps: a sun core, halo, horizontal streak and three lens ghosts at the course's directional light, plus a warm exposure lift. When the sun is behind the camera, its light leaks in over the top edge on its side. There is no occlusion test. |
| Landing burst | Hard landings (from 10 ft/s, full at 55 ft/s) throw a ring of terrain-tinted dust and ballistic dirt clods, with a deep thump on the SFX bus. Water landings are skipped. Dust next to the lens is thinned so it never fills the frame. |
| Heavy mud | The existing quad/rider dirt overlay soils three times faster, the source zone dirt rule runs four times faster, dusty zones without a rule also soil, and hard landings splat extra mud. Water still washes it off, and Reset still cleans it. It works on every graphics preset. |

| Arcade flips | Gameplay, not presentation: the riding stick steers rotation in the air (backflips, front flips, 360s, corks), the preload has a lip-pop timing bonus, and the HUD names landed tricks. It implies flips whatever the Extras row says and replaces the Unleashed gestures; original landing and wreck gates still decide. See [FLIPS.md](FLIPS.md#arcade-flips-alpha). |

## Verification

- `web/hype.test.mjs`: small hops change nothing; one apex slow motion per big jump that is back at full speed by touchdown; the slow-mo toggle; pull-back, wider view, muffle and wind; the terrain floor; reset handling; burst scaling; sun projection; settings persistence; heavy-mud rules.
- `ui_menu.rs` `alpha_page_lists_browser_toggles_and_validates_ids`; the WASM smoke test checks the Settings hub.
- `tools/test_alpha_hype.mjs` (content group) launches the quad about 24 ft into the air on Caribou Peak through the worker start-position op, then lets the recovered simulation fly and land it. Measured: slow motion reaches 0.3× and stretches each 60 Hz step to about 51–57 ms of real time; air framing, glare and muffle engage; the landing compresses the camera; the burst spawns about 45 particles; heavy mud rises. Then it checks the Alpha page rows and toggling. Screenshots on Caribou Peak and Sonora Dunes were inspected.

## Limits

- The muffle and thump were not listened to in automated runs; headless audio only proves the graph and parameters.
- The glare has no occlusion, and the sun position is the course's directional light, which may not match a painted sky sun.
- Physical phones and controllers are untested. All tuning is a first pass awaiting the user's feedback.
