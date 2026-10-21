# Shared Rust UI (atv-ui)

## Touch and toolbar checkpoint

The `atv-ui` crate is the shared browser/desktop interface foundation. This
checkpoint ports the existing authored mobile controls and Pause/Camera toolbar;
it does not claim original PS2 menu, HUD, or whole-game parity. The existing
JavaScript game remains available while the Rust preview catches up.

## Ownership and behavior

`atv-app::overlay` routes winit touch, mouse and resize/focus events. `atv-ui`
owns hit testing, gestures, layout and paint. `atv-game` passes touch drive intent
into the existing `BrowserDrive` adapter before the recovered fixed-step ride.
Touch is sampled once per simulation batch, so releasing preload is consumed at
the simulation boundary rather than expiring on a display timer. The original
preload, stunt and landing gates described in `TOUCH.md` remain authoritative.

The existing one-handed default, saved 50–150% steering response (85% default),
second-finger reverse/trick wheel, and explicit two-thumb mode are preserved.
Pause clears riding ownership; Resume requires a fresh press. Camera accepts a
utility finger while a driving finger remains held. Cancel, drag-away, focus
loss, document hiding, application suspension, keyboard pause/reset and viewport
resize discard ownership without a synthetic preload release. Independent focus
and visibility state survives loading; becoming visible cannot override a canvas
blur. Cached-page resume refreshes visibility before allowing simulation/audio. Toolbar targets remain at least 44 CSS pixels and
respect safe-area insets. The temporary Rust pause control says Resume while
paused; full pause-menu migration is a later checkpoint.

The touch mapping is an authored adaptation, not a PS2 control mapping. No
physics tuning or new stunt animations are introduced. Optional flip and arcade
logic is retained in the gesture implementation but is not enabled by this
checkpoint: the Rust ride does not yet expose those enhancement integrations.

## Rendering and performance boundary

Layout uses taffy; the wgpu overlay pass batches boxes, gradients and cached
text/vector bitmaps after the scene resolves. Browser text rasterization calls
Canvas 2D through Rust bindings; desktop text uses cosmic-text/swash with installed
system fonts. No JavaScript gesture, layout or UI state machine is added.

The atlas is bounded at 2048² RGBA (16 MiB). Text measurements are cached, gesture
sampling borrows its point history, instance buffers grow only when required,
and unchanged display lists reuse the previous GPU upload. Atlas exhaustion
rebuilds the frame before drawing, rather than overwriting texels referenced by
earlier instances. Trick-wheel sectors use tight bounds instead of ten full
wheel-sized bitmaps. All layout/hit tests use CSS pixels, independently of the
renderer backing resolution; safe-area probes run at resize, not per frame.

These choices preserve caching/batching principles from `PERFORMANCE.md`, but
are not evidence of physical-phone frame-time or memory parity. The Rust preview
still needs adaptive render resolution and inactive render-loop sleep. Sleeping
must be coordinated with audio maintenance and gamepad resume polling; pausing
rendering must not stop the soundtrack or its next-track scheduling. The
JavaScript path must not be retired before those comparisons pass.

## Verification and limits

- Fourteen Rust UI tests cover the saved defaults, one/two-thumb input,
  secondary ownership, all ten wheel sectors, release-once and cancellation,
  takeoff rebasing, landing drive restoration and gesture extension release.
- `tools/test_engine_touch.mjs` uses the shared browser harness and is registered
  in the content group. Host Chromium real multitouch and a WebKit synthetic
  pointer/coalescing fixture pass at 390×844 and 844×390, with DPR3 requested:
  axes, 44px utility hit targets, held-finger Camera/Pause, drag-away guards,
  paused simulation, neutral Resume, keyboard Reset/Pause, cancellation and
  resize. Portrait natural-hop captures also exercise the airborne trick wheel
  while the primary finger remains held, including WebKit at actual backing DPR3.
  The test reads an opt-in `?uiDiagnostics=1` hook; normal play does not
  serialize UI/game state each frame.
- Native optimized app/WebAssembly builds and focused UI/app Clippy pass. The same
  gesture crate builds for both; native touch hardware is not tested.
- The integrated ENV01 600-step comparison was inspected numerically: maximum
  JavaScript/native vector differences are 0.00024414 position, 0.000020876
  quaternion and 0.00048852 camera, with no step crossing the comparison's
  0.001 position / 0.0001 quaternion thresholds. Browser/native final position
  distance is 0.000015259 at step 600. Vehicle-crop RGB mean differences remain
  1.29/255 landscape and 1.94/255 portrait; full-frame differences also include
  the canvas toolbar and foreground plant orientation from camera history.
  This is authored-port agreement, not original-hardware parity.
- One Apple M4 host replay measured normalized main+worker CPU per 60 Hz step:
  JavaScript 2.727 ms, Rust browser 2.366 ms; native engine timing was 2.890 ms.
  The browser runs were uncapped and these are distinct timing measurements,
  not a sustained phone benchmark or proof of mobile memory equivalence.
- The combined audio/current-scene integration passes both browser lifecycle
  gates: pre-ready blur, late sound installation, pause soundtrack, context
  resume, and a sole early click/completed touch. Visibility and persisted page
  transitions are controlled DOM fixtures, not physical mobile backgrounding.
- Inspected combined-engine captures show the current venue/scenery, quad/rider,
  held control, toolbar and airborne wheel. Captures stay in private storage for
  the release owner. These are host checks, not physical iOS/Android,
  touch-screen desktop, or original-hardware results.

Menus, minimap, complete HUD, remapping screens, keyboard help, accessibility
semantics for canvas controls, optional flips/arcade integration, original-game
UI comparisons and desktop font/raster parity remain open. This checkpoint does
not switch `/play.html` away from the existing game.

## In-ride HUD checkpoint (2026-10-21)

Ports `web/hud.js` (Freeride path), `web/minimap.js`, `web/keyboard-help.js` and
the Keys toolbar button into `atv-ui::hud`, drawn by the same UI pass:

- Original meter and stunt text (`hud::meter`, `hud::assets`): `OverlayObject-5`
  and `OverlayObject-12` are read from `Overlays.pb` in the browser's archive
  order (STARTAPP, PAGE01, STARTEVT, course archive, OVERLAYS); CEL glyph metrics
  size and place the text and the PST preload art is drawn as hud.js does
  (640x448 logical frame scaled by height, compact phone layout below 560 px,
  orange preload stretched from its bottom edge). Values and clocks come from
  `atv_core::hud` (`Ride::hud`). Shown when the minimap is off, as in the browser.
- GPS minimap (`hud::minimap`): the 1024x1024 shaded terrain raster (`rasterRow`)
  plus static scenery triangles (`sceneryTriangles`, from `Venue::scenery`),
  built in ~2 ms slices then uploaded once; marker, north lettering, speed footer
  and preload bar at the 50 ms instrument rate; tap toggles north-up/heading-up.
  Settings use the browser's `atv-minimap-*` keys.
- Keyboard card (`hud::help`): taffy block/grid/flex with inline `<kbd>` chips,
  opened for 15 s on the first keyboard ride (`atv2-keyboard-help-seen-v1`),
  toggled by H or Keys, hidden on touch-first devices and while touch-driving.
- Toolbar: Keys button, and the icon-only layout when the top-right map meets
  the labelled toolbar (the reason the JS portrait toolbar shows icons only).

Layout follows Blink line boxes: fractional (unrounded) taffy boxes, font
ascent/descent rounded to whole pixels with the half-leading floored, and
`box-sizing: border-box` chip minimums. Redraw avoidance: the toolbar and card
layouts are cached on their inputs, the minimap repaints at 20 Hz, and an
unchanged display list skips the GPU upload (as hud.js skips unchanged keys).

Measured (host Chromium/ANGLE Metal, Apple M4, ENV01 start, DPR 1 unless noted;
JavaScript play.html vs Rust engine.html, flat backdrop, RGB mean absolute
difference of 255 per region, `work/` capture scripts, private images under
`private/reports/rust-ui/hud/`):

| Case | Toolbar | Minimap | Keyboard card | Meter | Whole |
| --- | --- | --- | --- | --- | --- |
| 1280x720 | 0.47 | 0.95 | 0.74 | 0.46 | 0.18 |
| 390x844 | 0.10 | 0.95 | 1.40 | 0.18 | 0.87 |
| 390x844 touch, DPR 3 | 0.82 | 3.12 | – | – | 1.00 |

The DPR 3 map difference is expected: the browser draws the map canvas at most
at 1.5x and upscales it, the engine draws at device resolution. UI CPU per frame
on the host stays 0.05-0.2 ms with the card and map shown (0.44 ms before the
layout caches). The map base builds in 27 ms of slices in the Rust browser build
vs 58.8 ms (29 slices) in play.html. Not measured on physical phones.

Known gaps: the keyboard card's `backdrop-filter: blur(12px)` is drawn as its
86%-opaque fill without the blur; remapped bindings, race overlays (`drawRace`,
321 lights, signs, radar), multiplayer peers on the map and the Pause→HUD
settings screens are later checkpoints. `tools/test_engine_hud.mjs` covers the
targets, compact rule, card first-ride/H/Keys, map orientation persistence and
the meter fallback.

## Desktop display face (2026-10-21)

Browsers keep the stylesheet stack (`--atv-font-display`), unchanged. Native
builds previously resolved the stack through cosmic-text, which only accepts a
face of the exact requested weight: on macOS the 800 weight fell through to the
system UI face (San Francisco), and Windows/Linux would use whatever the stack
happened to find. The native backend now:

- selects faces by CSS font matching (nearest weight, closest stretch) and treats
  faces named `…Italic` as italic (macOS Avenir Next Condensed Heavy Italic
  declares a normal style), so macOS draws Avenir Next Condensed Heavy Italic for
  `italic 800` exactly as Safari/Chrome do;
- uses the stack's first family when it is installed (macOS), otherwise the
  bundled **Fira Sans Condensed** (SIL OFL 1.1, `crates/atv-ui/fonts/`, unmodified
  Regular/SemiBold/Bold/ExtraBold/ExtraBold Italic, licence in `OFL.txt`, hashes
  pinned in `tools/verify_repository.py`). `FURY_BUNDLED_FONT=1` previews it on
  macOS.

Candidates were measured against Avenir Next Condensed with the native rasterizer
on the UI's strings and styles (ink width ratio; mean coverage difference after
scaling to the same box, `text::native::font_choice`, ignored test):

| Face | Width ratio (6 samples) | Shape difference |
| --- | --- | --- |
| Fira Sans Condensed | 0.94-1.11 | 0.12-0.28 |
| Asap Condensed | 0.85-0.95 | 0.13-0.28 |
| Barlow Condensed | 0.78-0.95 | 0.16-0.34 |
| Arial Narrow (macOS) | 0.87-1.06 | 0.13-0.35 |
| Encode Sans Condensed | 0.99-1.16 | 0.19-0.33 |

Fira Sans Condensed is closest in width (layout) and shape. On macOS keeping
Avenir Next Condensed keeps desktop closest to the web look: native-vs-browser
region differences of the same frame were 5.4 (toolbar) / 5.9 (keyboard card)
with Avenir and 11.5 / 22.1 with the bundled face (scene pixels included; native
capture `fury --capture … --ui`). The fonts add about 2.5 MB to native binaries
and nothing to the WebAssembly build.

## Pause menu and settings checkpoint (2026-10-21)

`atv-ui::menu` ports the ride's pause panel (frontend.js `rebuildPausePanel`,
`run`; game.css + menu-screens.css `.pause-panel`). Rows, labels, values and
choice order come from `atv_core::ui_menu::layout` with the same request fields
the browser sends; the panel (ellipse dim backdrop, centred 520/700 px panel,
title strip, gold-pill selection, value badges, `.atv-range` sliders, footer
Back with its glyph, controls tips, overflow scrolling) is laid out with taffy.

- Opens when the ride pauses (Esc/P, toolbar Pause, controller Start) and
  replaces the HUD, map, toolbar and touch layer like `body.menu-open`. Keyboard
  arrows/Enter/Escape, controller D-pad/A/B/Start, mouse/touch taps, slider drags
  and wheel/drag scrolling; Back returns to the row that opened the page.
- Pages: Pause, Settings, Display, Minimap, Race Overlays, Controls, Audio,
  Extras. Minimap rows/sliders, mobile steering sensitivity and one-handed mode
  apply at once; Audio → Sound switches the running mixer. All choices persist
  under play.html's keys (`atv-minimap-*`, `atv-mobile-*`, `atv-graphics`,
  `atv-speed-rush`, `atv-crash-*`, `atv-flips`, `atv-train-riding`,
  `atv.diagnostics.v1`, `atv-audio-v1`, `atv-soundtrack-v1` updated in place,
  `atv-race-*`), so the two games share them.
- Not yet in the Rust menu: the Alpha experiments (Copy ride link and Jukebox:
  see below)
  (rows hidden); Graphics, Speed rush, crash,
  flip, train and stats choices persist but the Rust ride does not apply them
  yet; music volume/toggle persist without driving the Rust soundtrack. Desktop
  hides Dev log and Quit to Main Menu (no browser pages to open).

Measured over a flat backdrop (JS `play.html` vs Rust `engine.html`, mean
absolute RGB difference of the whole viewport, rows identical): Minimap 3.05
(1280x720) / 4.94 (390x844), Extras 1.34 / 2.38. Pages whose row sets differ
(hidden rows above) are compared visually. The JS panel's
`backdrop-filter: blur(12px) saturate(1.1)` is not reproduced (86%-opaque fill
only). Opening and navigating the menu wakes single frames under the sleeping
paused ride (`test_engine_sleep` still sees no frames while idle). Tests:
`tools/test_engine_pause.mjs`; `test_engine_touch.mjs` resumes through the menu.
A Pause/Esc tap shorter than one frame is still missed by the Rust bindings
(the browser queues key edges); held taps work.

## Bindings, capture and controller settings (2026-10-21)

`atv_game::ride::bindings` now follows web/bindings.js and gamepads.js instead of
fixed defaults: per-action keyboard/controller rows with chords, the Fury
Remastered / ATV Offroad Fury 2 / MX vs. ATV Unleashed presets with a kept
Custom set, the version-1 keyboard migration, controller response tuning
(steering, lean, gas/brake, camera, dead zone) and the browser's saved formats
(`atv2-gameplay-bindings-v1`, `atv2-controller-v1`), so remaps carry over between
play.html and the Rust engine. Camera/Reset/Pause key edges are queued on
keydown like `observeKeys`, so a tap shorter than one frame now pauses.

The pause menu gains Controls → Keyboard bindings (four rows per page with the
pager, Clear, Defaults; capture by key or chord, released to save, Escape
cancels) and Controller settings (controller in use, preset cycling, tuning
sliders, Controller bindings captured from buttons/axes as frontend.js
`pollCapture` does, Defaults). The keyboard card follows the live bindings and H
stays a gameplay key if bound. Controller rows draw the
pad-glyphs.js button glyphs (see Controller glyphs below), and pinning a specific
controller (`padselect`) is Auto-only in the Rust engine.

Flat-backdrop comparison (mean absolute RGB, whole viewport): Keyboard bindings
1.07 (1280x720) / 2.15 (390x844); Controller settings 5.03 / 10.78 (the portrait
difference is one value badge that wraps in the browser and not in the engine,
shifting the rows below by a line). Tests: `test_engine_pause.mjs` captures a new
Gas key, restores Defaults, cycles the controller preset, sets a tuning slider
and checks a sub-frame Pause tap; atv-game unit tests cover saved-profile
round trips, presets/custom, migration, tuning curves and queued taps.

## Frontend checkpoint (2026-10-21)

`/engine.html` without `?venue=` and the desktop `fury` without a direct-ride
option now open the engine's own frontend (`atv_ui::front`, app glue in
`crates/atv-app/src/front.rs`), drawn by the UI pass without a 3D scene:

- Pages from web/menu-screens.js/css: Title (authored wordmark, Press Start
  pulse, Dev log on the web), Main Menu, Single Player, Select Track (all 40
  venues from `tracks.dat` via `unlockedCourses`, event tabs with counts, photo
  list with the original TP photo halves joined and trimmed, framed hero with
  arrows and Ride, the course photo as greyed, red-multiplied backdrop), Select
  ATV & Rider (vehicle row with arrows, Male/Female switch, the ATV and
  motorcycle lists with classes, Done, uiQuad.txt meters, maker logo and
  authored top speed; motorcycles carry the MX rider and ride in Freeride) and the course loader
  (LOADICON action shots crossfading every 2.5 s, Track pill, an original etips
  riding tip and the gold bar advancing with the real load stages).
  Settings from the main menu opens the shared pause-menu pages full screen
  over the frontend backdrop (`#frontend[data-context=menu]`).
- Layout follows the CSS grid and media rules (portrait stage above the rows,
  short landscape), with two renderer additions: `skewX` boxes for the silver
  stage panel and elliptical corner radii (`border-radius: 75% / 56px`) for the
  portrait stage. Chrome's grid sizing of the hero caption (Ride spanning empty
  rows) and CSS block behaviour (header strips spanning their container,
  collapsed heading margins, `letter-spacing: normal` on `<button>`) are matched.
- Input: keyboard (arrows, Enter/Space, Escape/Backspace, PageUp/PageDown/Q/E for
  tabs), standard controllers (D-pad/stick, A/Start, B/Y, LB/RB), mouse hover
  selection, clicks and taps, touch drag and wheel scrolling of the lists.
- `atv-player-selection` (quadId, riderId, freestyleEnvironment) is read and
  written with the browser game's other fields kept, so both games share the
  garage. The ride loads the chosen ATV and rider.
- launch.js contract (`atv_game::launch::parse_launch`, `menu_shortcut`, unit
  tests mirroring launch.test.mjs): ride links (`map`/`environment`,
  `quad`/`vehicle`/`atv`, `rider`, `player`, `play`/`drive`, race `laps`, `cpu`,
  `difficulty`, `cheats` validation) start their ride through the loader and
  become the saved selection (ATVs and motorcycles); setup-only links open Select Track; `?menu=`
  shortcuts open their menu; invalid, multiplayer and motorcycle-race links open the
  main menu with the reason on the status line. `?venue=` still rides at once
  (engine tests and parity tools); `?front=PAGE` / `fury --front PAGE` open a
  page directly, and `fury --front PAGE --capture OUT.png WxH` captures it.
- Pause → Quit to Main Menu returns to the frontend (also on desktop); the ride's
  GPU device moves between the frontend and the ride, and the frontend releases
  its textures while riding.

Parity, mean absolute RGB over the whole viewport against play.html (same
content, Chromium, DPR 1; images in private/reports/rust-ui/front/side-*.png):

| Page | 1280x720 | 390x844 |
| --- | --- | --- |
| Title | 1.45 | 1.76 |
| Select Track | 1.18 | 1.98 |
| Settings (from main) | 1.89 | 3.09 |
| Main / Single / Garage | 9.7 / 8.5 / 12.4 | 12.6 / 10.7 / 13.7 |

The main, single and garage differences were the missing 3D previews, added in
the next checkpoint. Hidden because the Rust engine cannot run them yet: Multiplayer, Arlington
Free Ride, Jukebox, Race setup, Rider gear and Alpha. Race links on a motorcycle
open the main menu with the reason, as motorcycle races are not ported. Not yet drawn: the Free Ride rider count,
backdrop-filter blur on the photo arrows and the menu notice line (later: Copy ride
link; the training venue's photo: "Training course photo"). The loader shows one frame per
load stage (the ride load owns the GPU meanwhile), so the shot crossfade only
advances between stages.

CPU (host Chromium, prepare = artwork + layout + upload; the page sleeps when
idle): title 0.2 ms/frame while Press Start pulses; garage 0.9 ms per input;
Select Track 4.3-6.5 ms when the course changes (the new course's blurred
backdrop and graded hero are filtered on the CPU, then kept for the last six
courses), 0.3 ms otherwise. Wasm grows from 5.21 to 5.50 MB (against main at 9f23629).
Tests: `tools/test_engine_front.mjs` (title → main → Settings → Single → garage
persistence → tabs and card click → loader → ride → Quit to Main Menu, and a ride
link on a phone viewport); atv-ui controller, layout, logo and pixel unit tests;
atv-game launch contract tests.

## Menu model previews (2026-10-21)

Main Menu, Single Player and Select ATV & Rider draw the 3D previews of
frontend.js `ModelView` (`atv_render::preview::ModelPreview`): the vehicle mesh
in its bind pose (a motorcycle in its resting pose), and in the garage the
original frontend rider (PAGE07 `mrider_ui`/`frider_ui` with its opaque UI
textures, playing the breathing and idle clips at the browser's ~30 Hz) or, for
a motorcycle, the MX rider with its selection clips turned the other way. Each
preview uses the browser shader's fit (height against horizontal turn radius,
0.25 rad pitch), studio lighting and grade, renders with 4x MSAA into a UI
image sized to its slot times min(DPR, 2), and draws with the canvas
`drop-shadow(0 10px 6px #0004)` (a new image drop-shadow item in the UI
renderer). Dragging a preview turns it. The two most recent vehicles stay
loaded, as `selectionAssets` keeps two.

Parity, mean absolute RGB against play.html (1280x720 / 390x844): Main 2.81 /
4.80 (panel 1.72 / stage 3.10), Single 1.56 / 2.96, Garage 6.11 / 9.34; the
garage rider's idle pose depends on when each capture is taken, and the Rider
gear button is still hidden. CPU (host Chromium): the garage costs 0.9 ms per
frame while the rider animates (layout and pose; the draw is on the GPU);
switching vehicle costs 2-6 ms when the new model uploads. Not ported: rider
gear and quad paint previews (`gearPreview`, `applyPaint`). Wasm is 5.70 MB on
current main. `test_engine_front.mjs` waits for both garage previews.

## Race overlays checkpoint (2026-10-21)

`atv-ui::hud::race` ports web/hud.js `drawRace` and the 321 LIGHTS lamps; the UI
pass draws them in races from the race frame (`atv_core::race`, `race_hud`):

- Assets (`hud::assets::load(.., racing)`): the race archive registers after the
  browser's archives; OverlaySet1 objects 1, 2, 3, 4, 7, 11, 14 and 41 from
  Overlays.pb, the `Single-Sign-*` sections, their CEL fonts and icons, the radar
  map and bounds from `<venue>nat.toy [Radar]`, `radaricon_racer` (plus a red copy,
  hud.js `tinted`) and the `321ron/roff/gon/goff` lamps.
- Layout as hud.js: the 640x448 frame scaled by `min(h/448, w/480)`, left items keep
  their x, right items their distance from the right edge, centred items stay centred
  (30 units left on phones); top items sit below the toolbar's bottom edge
  (`toolbarBottom`), and phones move the right column below the compact meter. LAP
  TIME, LAP, SCORE (committed stunt points), the PLACE name tree with its sliding
  slots and pulsing rank or the place number, BEST/TOTAL/LEAD faded by the lap
  history (LEAD green ahead, red behind), the lap history, info text and signs with
  Off Track / Too Deep digits, and the north-up radar with CPU dots and the player's
  red dot. Pause → Race Overlays choices (`atv-race-overlays-off`,
  `atv-race-position`) apply when the menu closes.
- Races hide the GPS map; the speed meter follows the overlays' Speed group; stunt
  text follows the Info group; the HUD hides during the pre-race shots; the lamps
  draw while the race frame carries them. The race load fades in from black over
  500 ms (race.js `.race-fade`). A held parity replay keeps its HUD and the Pause
  toolbar, as the browser renders it.

Measured (NAT01 race, seed 12345, 720 scripted steps, held frame; mean absolute RGB
against play.html in the Rust browser build, host Chromium, images under
`private/reports/engine-parity/race-presentation/`): 1280x720 PLACE tree 3.91, top
row 2.13, timing column 6.51 (the scene behind it differs; the text matches by
inspection), radar 2.43, lamps 0.41, meter 0.76, whole frame 2.03 (the same frame
without the overlays: 7.13); 390x844 top items 1.19, lamps 1.26, whole 2.70. Native
captures (`fury --capture .. --ui`) place the display face a few pixels lower than
Chrome's canvas text (the desktop font's metrics), as for the Freeride HUD.

Not yet: the countdown digits shown when a course lacks the lamp art (every race
archive carries it).

## Race results checkpoint (2026-10-21)

`atv-ui::hud::results` ports race.js `RaceOverlay`'s race-over half (game.css +
hud.css `.race-results`, `.race-show`): on the first finished frame the panel shows
Race Results, the summary (`Place N of M · total · Best`, race.js `raceTime`) and the
standings table fixed at that moment (the player's row in HUD gold; CSS auto table
layout approximated by sharing the width by each column's max-content width), over
the riders who drive on. The original race-over menu: Continue opens Restart / Quit
Event, Hide Menu swaps the panel for Show Results. Escape (back, hide, show) and Enter
(the menu's first button) are taken from the ride while the results are up, the
controller's A/B/X act on a fresh press as race.js polls them, pointers and touches
click the buttons. The panel is centred with `translate(-50%, -50%)` after one layout.

- Restart: with the engine's frontend (race links), the ride ends and the event
  reloads through the course loader with a new roster seed and no pre-race camera
  (race.js → `$('reset').onclick({restart:true})` → `loadGame`); race links now draw a
  new roster per load too (the frontend path used a fixed seed). A direct ride
  restarts in place. The new load holds the soundtrack through the countdown (the
  hold now also applies when the bank becomes ready before the first race step, and
  no song starts while held).
- Quit Event returns to Single Player (`frontend.show('single')`); it is hidden without
  the engine's frontend.

Measured (NAT01, 9,000-step autopilot race finishing first, held frame; mean absolute
RGB against play.html, Rust browser build): heading 1.17, whole panel 4.12, table
6.47, buttons 6.38 — the remaining difference is the panel's `backdrop-filter:
blur(12px)`, drawn as its 86%-opaque fill without the blur (inspected,
`private/reports/engine-parity/race-presentation/shots/results-side.png`). A live
link race (`engine.html?activity=race&map=nat01&laps=1&cpu=3&play=1&autopilot=1`)
finished, Continue → Restart reloaded the event with a new roster (inspected
screenshots `restart-*.png`), the soundtrack stayed paused through the countdown and
started at GO (`?audioDiagnostics=1`). Host Chromium only; names longer than the
column are not ellipsised; Tab focus movement between the buttons is not ported.

## Copy ride link (2026-10-21)

Pause → Copy ride link (frontend.js `rideUrl`, launch.js `buildRideUrl` and
`copyRideUrl`) builds the single-player link of the running ride
(`atv_game::launch::ride_url`: mode, activity, map, quad, rider, the saved player
name, and for races laps, CPU riders, Expert and the EATDIRT/BRINGIT cheats, in the
browser's key order and form encoding; the result parses back through
`parse_launch`). The link opens the page the engine runs on (`engine.html` while the
Rust engine sits beside play.html), is written to the clipboard and confirmed on the
menu notice line (`#menu-notice`, now drawn by the pause panel), or shown there in
full when the browser refuses the clipboard. Desktop builds have no web page to link
a ride to and leave the row out, as they leave out Dev log. Test:
`tools/test_engine_ride_link.mjs` (clipboard text and query fields; host Chromium).
## Saved settings applied during a ride (2026-10-21)

Audit of the choices play.html reads while riding (game.js `graphicsSettings`,
`crashSettings`, `vibrationSettings`, `alphaSettings` and the audio/soundtrack
settings, graphics.js, render-support.js `RideEffectsSettings`, frontend.js) against
the Rust ride, which until now only persisted them:

| Setting (key) | play.html | Rust engine now |
| --- | --- | --- |
| Display → Graphics (`atv-graphics`, Auto = `detectGraphicsTier`) | preset at load; on change: interpolation, anisotropy, contact shadows, dust/tire pools, shadow map, smoothed sky, Fresnel, wheel re-projection, dirt | saved preset for rides from the frontend (Auto ports `detectGraphicsTier`; URL/CLI `graphics=` wins; direct `?venue=` parity rides keep Original); on change: interpolation, smoothed sky (when decoded at load), Fresnel, contact shadows, dust and tire-track pools. Anisotropy and shadow-map size apply from the next load; wheel re-projection and ATV dirt are not ported |
| Speed rush (`atv-speed-rush`) | FOV kick and screen pass intensity Off/Low/Normal/High; Off by default with reduced motion | applied at load and on change, same peaks and reduced-motion default |
| Crashes (`atv-crash-mode`) | Original/Crazy/Gorn switch mid-ride | applied at load and on change (crash layer created or dropped) |
| Flips (`atv-flips`) | `input.flips` each batch | `RideOptions::flips` each step |
| Train riding (`atv-train-riding`) | `input.realistic_train` each batch | `RideOptions::realistic_train` each step |
| Audio → Effects volume, Sound mode (`atv-audio-v1`) | live | live (mode was already live) |
| Audio → Music on/off and volume (`atv-soundtrack-v1`) | live | live; default volume fixed to the browser's 0.45 (was 0.7 in the Rust menu) |
| Minimap, Race overlays, touch steering/one-handed, bindings | live | already live |
| Crash camera (`atv-crash-camera`, authored Tight by default) | crash framing | not ported: the Rust ride always uses the original crash camera |
| Chase camera (`atv-chase-camera`, uphill assist) | assisted sightline | not ported |
| Vibration (`atv-vibration`) | gamepad/touch rumble | not ported (no rumble output in the Rust engine yet) |
| Diagnostics (`atv.diagnostics.v1`) | performance/debug overlay | not ported (the status line carries frame and CPU numbers) |
| Silent switch (`silent`, iOS) | mute on the ringer switch | persisted only |
| Alpha experiments (`atv-alpha-*`) | see the Alpha registry | separate checkpoint |

Parity replays (`script=`, held states) and `--autodrive` benchmarks keep the
defaults they were recorded with. Evidence: `tools/test_engine_settings.mjs` (saved
Speed rush High, Gorn, Unleashed flips, realistic train, effects 0.5 and music 0.3
applied at load; Speed rush Off, Graphics Enhanced and Crashes Original applied from
the pause menu; a ride link with `atv-graphics=high` loads High: interpolation,
16x anisotropy, 96 dust, 3,600 tire segments, contact shadows). Host Chromium.
## Controller glyphs (2026-10-21)

`atv-ui::menu::glyphs` ports web/pad-glyphs.js: each controller input describes
as the original Controller Settings page's buttons (face buttons, L1/R1 shoulders,
L2/R2 triggers, START/SELECT, L3/R3 and stick directions, D-pad arms, Home, raw
`B4`/`A2-` inputs for non-standard indices), drawn from the same 32x32 vector
elements (the wordmark rasterizer: Canvas 2D in browsers, zeno natively, now with
round caps and joins) once per device scale, with their labels as UI text in the
browser's `system-ui` face. The Controller bindings page (titled Controller, as in
the browser) shows each row's alternatives as glyph chords (`+`, `/`, Unbound) and
the original-style summary panel (ATV, Stunt and Camera sections; matched pairs
collapse to one stick or D-pad glyph, `pairEntries`), placed beside the panel on
wide frontend menu screens as menu-screens.css does. The engine reads every pad in
the standard layout, so glyphs always use standard names; play.html falls back to
raw names for a pad reporting a non-standard mapping. As in the browser, the
keyboard card and HUD carry no controller glyphs.

Measured (host Chromium, ENV01 paused, Pause → Settings → Controls → Controller →
Controller bindings; mean absolute RGB against play.html; `work/pad_pages.mjs`,
images `private/reports/rust-ui/pad-glyphs/`): 1280x720 binding rows 1.93, summary
panel 1.12, whole panel 1.37; 390x844 whole frame 2.63. Glyph shapes match by
inspection (`glyph-rows-side.png`); the glyph labels sit within about a pixel.

## Jukebox (2026-10-21)

Pause → Jukebox ports web/jukebox.js and its music.js hooks for rides:

- Player (`atv-app::sound::Player`, MusicStream): song choice (the shuffle continues
  after it, op 41 `select`), the transient Jukebox Pause (Music on/off stays a
  setting; the mixer holds the decoded queue, the web build pauses its stream),
  Next/Previous through the play history (`playingHistoryIndex`, the next shuffle
  pick at the end), seeking the hosted stream (the desktop VAG decoder is not
  seekable, as the browser's decoder fallback is not), and the saved
  `atv-soundtrack-v1` v3 record (`history`, `historyIndex`, at most 64, restored at
  load; a fresh session avoids repeating the saved last track). Leaving the
  Jukebox or the menu ends its pause (`stopJukebox`). Race holds still win.
- Page (`atv-ui::menu::jukebox`, jukebox.css): Back, title and source (Streaming /
  Original decoder); the original PAGE14 art (`bk_jukebox.pst` backdrop with its
  vignette, the 20-frame `cd64.atm` disc turning in 80 ms steps while a song plays,
  face-on when stopped; loaded on the first visit); genre, title, artist; seek with
  elapsed and length; Previous / gold Play-Pause / Next; the volume range; the 21
  original songs with number, title, artist, genre and the playing row's bars. The
  track list is the only scrolling region and opens with the playing song centred.
  Phones (≤560px or portrait) get the compact card, short landscape the smaller
  art. Keyboard/controller order and labels come from `atv_core::ui_menu`;
  Left/Right seek 10 s and step the volume 5%; pointer drags set seek and volume
  (applied on release, as `onchange`).
- The Jukebox redraws while open (disc, progress); an unchanged menu now reuses its
  layout, so it lays out at most when the disc frame or quarter-second position
  changes (about 12 Hz).

Measured (host Chromium, ENV01 paused, song 3 chosen, list at the top; mean absolute
RGB against play.html, `work/jukebox_pages.mjs`, images
`private/reports/rust-ui/jukebox/`): 1280x720 header 7.55, list 8.79, title block
12.04, controls 10.42, art 10.68 (the browser's spectrum bars and disc frame
timing), panel 7.90; 390x844 player 5.40, list 11.27, frame 8.53. Test:
`tools/test_engine_jukebox.mjs` (21 songs, choice, Pause/Play, Next/Previous history,
volume and the saved record, pause cleared on leaving).

Not ported: the spectrum bars (the web stream plays through a media element outside
the Rust audio graph, and desktop has no analyser yet; the bars area stays empty),
the main-menu Jukebox (the Rust frontend has no soundtrack player outside rides; the
row stays hidden there), dragging feedback before release, Media Session release
(play.html keeps the soundtrack out of system media controls; the Rust web build
never registers it).

## Screen wake lock (2026-10-21)

`atv-app::wake` ports web/wake-lock.js for the web build: the Screen Wake Lock is
requested while a ride is on screen and released when a menu opens (game.js
`screenWake.set(!menuOpen)`); a hidden page loses it and the next visible frame or
pointer/key gesture requests it again (browsers that want a user activation, and
iOS Home Screen apps from iOS 18.4). The API is called through `js_sys::Reflect`
because web-sys gates it behind `web_sys_unstable_apis`. Desktop builds do nothing:
winit has no screensaver inhibitor and the cross-platform crates add per-OS
backends (D-Bus on Linux) for little gain on machines a player is actively using;
a controller-only desktop session can still dim. Test:
`tools/test_engine_wake_lock.mjs` (a recording `navigator.wakeLock` stand-in:
request while riding, release in Pause, request on resume). Host Chromium only;
real-device lock behaviour is not tested.

## Training course photo (2026-10-21)

Training ships no TP photo pair; play.html shows the approved capture of the venue
from the site's dev log images (frontend.js `trainingMapPreview`,
`/devlog-images/training-map-preview.png`). The Rust web build loads the same image
once at boot (decoded by the browser into RGBA) and uses it wherever a course photo
appears: the Select Track card and hero, the greyed backdrop and the course loader
(Training has no action shots). Measured against play.html (Select Track on
Training, host Chromium, mean absolute RGB): 1280x720 hero 0.71, card 2.04, frame
1.19; 390x844 frame 2.64 (images `private/reports/rust-ui/training/`). Desktop
builds keep the photo-less card: the image is a site asset, not part of the game
content package. Arlington Free Ride is the multiplayer frontend's.

