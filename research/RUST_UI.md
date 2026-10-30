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
| Display → Graphics (`atv-graphics`, Auto = `detectGraphicsTier`) | preset at load; on change: interpolation, anisotropy, contact shadows, dust/tire pools, shadow map, smoothed sky, Fresnel, wheel re-projection, dirt | saved preset for rides from the frontend (Auto ports `detectGraphicsTier`; URL/CLI `graphics=` wins; direct `?venue=` parity rides keep Original); on change: interpolation, smoothed sky (when decoded at load), Fresnel, contact shadows, dust and tire-track pools. Wheel re-projection follows the preset (Enhanced/High). Anisotropy (terrain, scenery and vegetation samplers rebound) and the shadow map (resized, created or dropped with the terrain and world groups rebound) also apply on change, as applyAnisotropy and the shadowMap swap do (`tools/test_engine_settings.mjs`; `private/reports/rust-ui/graphics-live/switched-to-high.png` vs `loaded-high.png`); ATV dirt follows the preset |
| Speed rush (`atv-speed-rush`) | FOV kick and screen pass intensity Off/Low/Normal/High; Off by default with reduced motion | applied at load and on change, same peaks and reduced-motion default |
| Crashes (`atv-crash-mode`) | Original/Crazy/Gorn switch mid-ride | applied at load and on change (crash layer created or dropped) |
| Flips (`atv-flips`) | `input.flips` each batch | `RideOptions::flips` each step |
| Train riding (`atv-train-riding`) | `input.realistic_train` each batch | `RideOptions::realistic_train` each step |
| Audio → Effects volume, Sound mode (`atv-audio-v1`) | live | live (mode was already live) |
| Audio → Music on/off and volume (`atv-soundtrack-v1`) | live | live; default volume fixed to the browser's 0.45 (was 0.7 in the Rust menu) |
| Minimap, Race overlays, touch steering/one-handed, bindings | live | already live |
| Crash camera (`atv-crash-camera`, authored Tight by default) | crash framing | on change: Tight frames Crazy/Gorn crashes with `crashPresentation` (`ride/view.rs` `crash`); held parity rides read `crashcam=`/storage (`--crash-camera` natively) |
| Chase camera (`atv-chase-camera`, uphill assist) | assisted sightline | on change: `UphillSightline` and the assisted `chasePresentation` orbit/pull-back (`chase=`/storage, `--chase-camera` natively) |
| Vibration (`atv-vibration`) | gamepad/touch rumble | on change (and at load for direct rides): `ride/impact.rs` ports ImpactEvents, Rumble and rumbleStrength; the web build plays `dual-rumble` on the pad with the latest input (else `navigator.vibrate` on coarse pointers from 0.3), desktop uses gilrs force feedback (Linux and Windows; gilrs has none on macOS, so Mac desktops stay still) |
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

## Alpha experiments (2026-10-21)

Settings → Alpha (web/alpha.js) lists the seven authored experiments, Off by
default, toggled from the menu (Enter or Left/Right) and saved under `atv-alpha`,
shared with play.html. The Rust ride runs three of them:

- Air slow-mo and the big-air camera: `atv_game::ride::hype::HypeDirector` ports
  web/hype.js (big-air detection from clearance, rise and air time; the landing dip
  spring; the chase pull-in, flattening and FOV widening). Since 2026-10-24 one
  apex "moment" per jump (0.2 s ease in, 0.55 s hold, 0.4 s ease out, real time;
  ended early by touchdown, a wreck or dropping under 4 ft) drives slow motion
  (0.3x), the chase camera, the air blur and the muffle/wind together, scaled by
  the jump's size (0.5 at a barely qualifying apex, 1 at full big air). Previously
  the camera and blur eased in on their own during the climb and held to touchdown
  while slow motion started at the apex, so they fired at different times.
  It steps once per accepted simulation step (live rides, autodrive and the Reset
  frame; before 2026-10-22 only autodrive stepped it, so live rides never left the
  ground for it) and updates once per rendered frame; slow motion
  scales the step accumulator (Freestyle only, as in the browser). With the toggle
  on every ATV interpolates, and the presenters blend by the accumulator's phase
  (one step behind the latest state) rather than by wall-clock segments: steps
  arrive every three to four frames at an easing rate, which segments juddered
  across on phones' uneven frames (freeze, then snap). Clearance
  reads the physics terrain under the rider; the browser reads its rendered terrain
  height (the same heights after `render_heights`, sampled bilinearly rather than by
  the triangle the physics uses).
- Arcade flips: `RideOptions::arcade_flips` switches the recovered vehicle's arcade
  flip mode, as game.js `drivingInput` does:
  - The keyboard/controller steering spins the quad as `arcade_spin`, replacing the
    Unleashed gestures.
  - On touch, steering stays the original air lean. Dragging the one-handed thumb
    or the two-finger lean pad past full lock adds touch.js `arcadeSpinAxis`: it
    starts past full lock and reaches full rate 50 px further.
  - Leaning back (dragging down) and forward set the flip rate.
  - Motorcycles have no Arcade flips, in either engine.

  Fixed 2026-10-22, from a user report from an iPhone: "arcade flips aren't working;
  I can't get the bike to rotate backwards faster".
  - The ride called `set_flips(flips && !arcade)` every step. With Arcade on, that
    dropped the flip controller each step, so the rotation never spun up past one
    step (0.23 rad/s instead of about 6.5) on every input.
  - atv-web enables flips with Arcade on. The ride now does the same.
  - Touch also now passes its 360 axis.
  - Touch now passes its Extras flip requests: the thumb's flip zones and swipes,
    and the two-finger pad pull. These were never reaching `request_flip`.
  - The overlay now tells touch whether Extras or Arcade flips are on.
  - `tools/test_engine_arcade_touch.mjs` compares the peak commanded rates with
    the JavaScript game, on the env01 ramp jump:

    | Input | Rust (rad/s) | JavaScript (rad/s) |
    | --- | --- | --- |
    | Touch pull-down (backflip) | 6.53 | 6.65 |
    | Touch drag past lock (360) | 7.25 | 7.25 |
    | Keyboard lean back | 6.42 | 6.42 |

- Landing burst: each landing the director reports while focused (no wheel in
  water) throws `atv_game::burst::BurstEmitter` dust puffs and clods (hype-gl.js,
  192-particle ring, tinted by the terrain layer under it via `DustTint`) drawn by
  `atv-render` `burst.wgsl` after lingering dust, and plays `audio::synth::
  landing_thump` (78 -> 34 Hz sine plus lowpassed noise, rendered to PCM on the SFX
  bus). Turning it off clears the particles.
- Heavy mud: `DirtBuildup::set_heavy` (heavyMudZones: quarter times, doubled
  cleaning, dusty zones soil) and landing splats of `min(60, 0.9 x severity)`; the
  dirt draws on any preset while it is on, and the tire tracks read the heavy zones.

Evidence (host Chromium): unit tests match BurstEmitter rows and counts; ENV05 held
throttle on Original after the spawn drop gave 50, 45 and 29 particles per landing
in both games (`private/reports/rust-ui/alpha-landing/env05-{rust-web,javascript}.png`);
`tools/test_engine_alpha_landing.mjs` (burst, thumps and soiling on, none off).

- Air dark blur: `atv-render` `air_blur.rs`/`.wgsl` port AirBlurRenderer. While
  `hype.blur` exceeds 0.002 the frame (after speed rush) renders into a texture that
  the pass draws with ten zoom taps towards the centre, 45% edge desaturation and up
  to 70% edge darkening, before the UI. Host Chromium ENV05 jump at 37 mph:
  `private/reports/rust-ui/air-blur/env05-{rust-web,javascript}.png` (blur 0.83 /
  0.90); `tools/test_engine_air_blur.mjs` (drawn when on, 0 when off while the
  director still measures air). Costs one full-screen copy and ten taps per pixel
  (six on coarse-pointer devices, over the same blur length) only during big air.
  With the toggle on the pass and its frame texture are created before the first
  jump, so big air no longer stalls on pipeline creation mid-flight.
- Speed & air muffle: `audio::hype::HypeAudio` in the mixer ports setHype for the
  mixer's music bus (a lowpass from 22 kHz towards 320 Hz, Q 0.7 dB, and a level down
  to 0.7, eased like setTargetAtTime per 128-frame block) and the wind rush (2 s of
  leaky LCG noise, bandpass Q 0.6 at 380-1280 Hz, gain 0.32 w², on the SFX bus);
  the riding loop sends the director's muffle and wind when either moves 0.004, open
  in menus, and the toggle going off stops the wind. The web build's streamed
  soundtrack moves into a Web Audio graph (MediaElementSource -> volume gain ->
  lowpass -> level, as music.js MediaOutput) at the first muffle; until then it
  plays from its element as before. Evidence: unit test (open bus untouched; fully
  muffled a 2 kHz tone falls below 0.02 RMS while 100 Hz keeps about 0.7; wind
  audible); `tools/test_engine_alpha_muffle.mjs` (host Chromium ENV05: director and
  mixer muffle 0.47, the soundtrack graphed and still advancing; off: none sent).
  Not measured: what the graphed soundtrack sounds like on phones.

Evidence for the Alpha flags: `atv-game` unit test (a launched 40 ft/s jump fires
one apex, one slow motion reaching 0.3x and a landing over 30 ft/s; slow motion,
camera, blur and muffle share one envelope frame by frame; the slow-motion presenter
moves exactly with scaled time at alternating 14.5/21 ms frames, where wall-clock
segments fail);
`tools/test_engine_alpha.mjs` (rows, saved toggles, applied ride flags); the presenter
and interpolation tests are unchanged at scale 1. Host only.

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

Spectrum bars (2026-10-22): jukebox.js drawBars over the art (24 bars, 88% x 18% at
the bottom, hidden when compact) from a music.js analyser (fftSize 64, smoothing
0.78): the web build moves the streamed soundtrack into its Web Audio graph
(`MusicGraph`, shared with Alpha muffle) and reads an AnalyserNode tapping the
volume gain; desktop runs `atv_game::audio::spectrum::Analyser` (the AnalyserNode
rules: Blackman window, smoothing, -100..-30 dB bytes) over the mixer's music tap.
The bars repaint each frame on top of the cached panel layout. Evidence:
`tools/test_engine_jukebox.mjs` waits for a lit spectrum; host Chromium 1280x720
`private/reports/rust-ui/jukebox-bars/{rust-web,javascript}-1280.png` (same art box
and bar geometry; levels differ with the song position).

Main Menu → Options → Jukebox (the original Options page's first row,
[MENU_FLOW.md](MENU_FLOW.md)) opens the same page over the frontend. The page's one
soundtrack player (`atv-app/src/soundtrack.rs`, audio.js `this.music`) serves both
the menus and rides. A chosen song replaces the menu tune; Back pauses it in place
(music.js `pauseQueue`); each ride load then starts a new random song, never the one just
heard, as game.js `loadGame` builds a new MusicStream (`tools/test_engine_ride_songs.mjs`,
`tools/test_engine_menu_jukebox.mjs`, `private/reports/rust-ui/menu-jukebox.png`).
Settings → Audio's song rows preview the soundtrack the same way.

Not ported: dragging feedback before release, Media Session release
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

## Controller rumble (2026-10-22)

`atv-game/ride/impact.rs` ports ImpactEvents (render-support.js: hard body contacts
from 30 ft/s with a 5 ft/s velocity change, landings after 0.18 s airborne falling
faster than 12 ft/s, rider wreck impacts from 40 ft/s; 0.3 s cooldown, one crash
upgrade) and Rumble/rumbleStrength (graphics.js: strength 0.2-1 from 18 ft/s, a fresh
wreck at least 0.7, 70-300 ms, no chaining inside one impact unless 0.15 stronger).
They run once per accepted step (and on the Reset frame) while the ride is focused
with no menu; scripted autodrive rides now also step wheel effects, dirt, impacts and
Crazy/Gorn crashes as live rides do. Output: the web build's Gamepad API
`dual-rumble` through `js_sys::Reflect` on the pad with the latest timestamp (gilrs
has no web force feedback), else `navigator.vibrate` on touch devices for pulses from
0.3; desktop uses gilrs force feedback (Linux evdev and Windows XInput; macOS has
none in gilrs). Evidence: the unit test matches the JavaScript classes on landing,
cooldown, crash upgrade, pulse and wreck cases; `tools/test_engine_rumble.mjs`
(host Chromium, a recording `vibrationActuator` on a stand-in pad, ENV03 into a Gorn
wreck): 4 impacts and 5 pulses with Vibration On, none Off. Real controllers, phones
and desktop force feedback are not tested.

## Leaving the page (2026-10-22)

Leaving the page (an iOS app switch, a hidden tab, pagehide) used to leave the Rust
web build's soundtrack element loaded and its AudioContext running, so iOS kept
showing "Fury" in Now Playing and the Dynamic Island. `sound::background` now
listens to `visibilitychange` and `pagehide` directly and, inside that event,
suspends every audio output (cpal's AudioContext and the soundtrack graph's) and
parks the soundtrack as music.js `park()` does: pause, remember the position, drop
`src` and `load()`. At start it clears any Media Session metadata, action handlers
and playback state (music.js `releaseMediaSession`). Returning resumes through the
normal lifecycle, as the browser game's `setActive(true)` and `updateMedia` do:
the output restarts and the song reloads at its position. Evidence:
`tools/test_engine_background_audio.mjs` (host Chromium; simulated page hide:
contexts suspended, the song unloaded, no session entry; on return the same song
plays on). Real iOS is not exercised by the test.

## Diagnostics overlay (2026-10-22)

`atv_ui::diagnostics` ports web/diagnostics.js and replaces the engine page's
debug strip. F3 (web and desktop) cycles Off -> Performance -> Debug and saves the
choice under `atv.diagnostics.v1`, the key the Settings row already used; the row
and F3 stay in step. `?debug=1` opens Performance and `?debug=2` Debug (`ATV_DEBUG`
natively); without either the saved choice applies, Off by default. Performance
shows the browser overlay's frame lines (FPS and mean frame, p95 and CPU per frame
over the last 120 frames, simulation time and steps, draws and triangles,
resolution) and the former `#status` strip text regrouped into short lines; Debug
adds the map and step, position, velocity, wheel contacts, suspension, contact
counts, wreck phase and resets, water height, applied input and speed. The panel is
drawn by the Rust UI at the top right (`#diagnostics-overlay` box, 11 px monospace,
10 px and lower below 600 px), hidden behind ride menus as `body.menu-open` hides
it, and shown on the frontend too since the strip it replaces was. `#status` stays
hidden with its text updating; only `#status.fatal` shows it. Not ported: the
browser-only lines (worker round trip, Wasm heap, minimap backing size, scene
cache and plant query counters, rider wreck cache). Evidence:
`tools/test_engine_diagnostics.mjs`; screenshots
`private/reports/rust-ui/diagnostics/ride-{performance,debug}.png`.

## Frontend music (2026-10-22)

The Rust frontend now plays the original menu tune, as play.html does
(audio.js `loadMenuMusic`/`playMenuMusic`). `atv_game::audio::menu::load_tune` reads
`UIMUSIC.AUD` from PAGE07.RES (section `UIMusic`, `NumTracks` 4, `Track1`-`Track4`),
decodes the first tune's left/right VAGs from AUDIO.RES into stereo PCM and loops it
on its VAG markers on the music bus at the saved music volume. It starts after the
first gesture, plays while the frontend owns the screen and the page is focused with
Music on, and stops when a ride takes over or focus is lost.

ROM evidence (USA v2.00): the routine at 00255b50 builds the frontend object's UI sound list
(+0x160) from `audio\UI\uimusic.aud`: `UIMusic`, `NumTracks`, then each `Track%d`
added as a looping sample (flags 0xc1) from `audio\audio`; the routine at 00254ae0 frees the
list when the frontend closes. The routine that picks which tune plays, and when it
changes, is not recovered, so the Rust frontend follows the browser game and loops
the first. Evidence: `tools/test_engine_menu_music.mjs` (host Chromium: silent before
a gesture, then the tune loaded and the output peaking at 0.24; Music off: silent);
`test_engine_front` passes (ride and Quit to Main Menu).


## Crisp menus, turntable and wordmark sway (2026-10-29)

**Blur cause.** The whole surface (3D scene and UI) was capped at the ride's
adaptive render ratio, 1.5 backing pixels per CSS pixel (`RENDER_RATIO`,
game.js renderScale), including the menus, which draw no 3D scene. On a DPR 3
phone every menu pixel was upscaled 2x by the compositor; the 3D previews were
also capped at 2x. Text was already rasterized at the surface scale, so it
inherited the softness. Desktop Metal/DX12 and Android Vulkan builds applied
the same cap.

**Change.** While the frontend owns the screen (`App::ready` is None) the
surface uses the display's full density up to 3x (`MENU_RATIO`); a ride
switches back to `RENDER_RATIO` before its first frame (`sync_surface_density`).
Layout stays in CSS pixels; text, icons, the wordmark and the previews
rasterize at the new scale. The frontend's device no longer keeps the 3D
scene's depth and 4x multisample targets (`Gpu::set_scene_targets`): at
1170x2532 they would be ~95 MB the menus never use. The ride's HUD and Pause
menu still draw at the ride's render ratio (one surface; a separate native UI
target over a reduced scene would cost a full-screen upscale each ride frame).

**Turntable.** The menu vehicle preview (every stage page, the garage, the
Profile Store pages) turns at 0.45 rad/s (one turn in ~14 s). Touching it stops
it; a drag turns it directly and lets go with its speed (up to 12 rad/s),
which eases back to the turntable rate (0.6 s time constant). Left/Right on a
focused preview hold the turntable for 2.5 s. The garage rider keeps its idle
animation and turns only when dragged (with the same fling).

**Wordmark sway.** The Fury Remastered wordmark on the title and the stage
pages tilts in 3D: yaw ±7° over 7 s and pitch ±3° over 5.3 s, through a 900 px
perspective, easing in over the first second. `El::tilt` emits
`Item::PushTilt`/`PopTilt`; the UI shader projects tilted images (and their
drop shadows) with a perspective-correct w; the angles live in the UI globals
uniform (`Renderer::set_tilt`), so the display list does not change while it
sways. prefers-reduced-motion keeps both still and the idle menu draws nothing
again (a drag still turns the preview, without a fling).

**Layer cache.** At 3x a stage page's boxes, gradients and shadows cost too
much to redraw every frame (host Chromium: main menu 21 fps with the
turntable). The UI renderer now marks instances that change between prepares
(tilted images, `add_target` images such as previews, and anything drawn
later that overlaps them), renders the rest once into a layer texture when
the list holds still, and each frame draws the layer plus the moving
instances. A changed list draws directly (slides do not pay for the cache).

Measured (host Chromium/ANGLE Metal, Apple M4, 390x844 DPR 3, 4x CPU
throttle, rAF intervals over 3 s; physical phones not measured):

| Page | Before (1.5x surface) | After (3x surface) |
| --- | --- | --- |
| Main menu, idle | static (0 frames drawn) | turntable + sway, median 15.1 ms, p95 29.9 ms, ~60 fps |
| Main menu, dragging the preview | median 16.8 ms, p95 35.3 ms | median 14.0 ms, p95 33.2 ms |
| Title (Press Start pulse) | median 15.9 ms, p95 27.3 ms | median 15.8 ms, p95 37.6 ms (with sway) |

Without the layer cache the 3x main menu ran at median 46.8 ms. Frontend CPU
prepare per animated frame is ~1.4-3 ms unthrottled. GPU memory: surface
1170x2532 (12 MB) plus the layer (12 MB) instead of 585x1266 plus ~24 MB of
unused scene targets. Test: `tools/test_engine_menu_motion.mjs` (Chromium and
WebKit at 390x844 DPR 3: canvas and surface 1170x2532, preview at 3x, a menu
row's text edges stepping >150 luminance in one device pixel, turntable
rate, fling and ease-back, sway on the title and main menu, reduced motion
still and idle).

## Phone menu layout (2026-10-29)

Driven by real-iPhone reports, an iOS Simulator pass (Safari's toolbars leave
about 390x664 in portrait and 844x330 in landscape) and a WebKit audit at DPR 3.

- **Single Race.** The setup squeezed a course list to one card (portrait) or
  none (landscape), the event tabs wrapped over two lines, and the Copy ride
  link/Back footer covered Difficulty. The setup is now four stepper rows in
  the scrolling rows column (Course, Laps, CPU riders, Difficulty) under a
  lower 2.4:1 photo. Course's arrows step the course; a tap on it, Enter or
  a controller's A opens the **course picker**: the event tabs over a
  full-height card list (portrait drops the photo for it), titled Choose
  Course. A card picks and closes it; the arrows walk the courses, Q/E or
  LB/RB the tabs, and Enter, Escape, B or the Done button close it on the
  page (`Frontend::course_sheet`; no history entry, as a sheet).
- **Scroll cue.** A rows column or list with more than 12px past an edge
  fades that edge and shows a small gold down triangle (`Scroller::paint_edges`),
  so rows below the fold are discoverable.
- **Portrait stages** give rows more room: stage pages take 36% of the
  height below the status bar on phones up to 820px (28% up to 640px), so
  Single Player's six modes fit without scrolling; Select Track's photo is
  2.4:1 too.
- **Landscape** rows columns run to the bottom edge; Back (and Copy ride
  link) sit under the stage at the bottom right. The status bar gets a dark
  band so the profile chip (on red) and the others (on silver) read as one
  bar. The title's wordmark yields height so Press Start and its links stay
  on screen at 330px.
- **Stage text.** Pages with a text table on the portrait stage (Pass Key,
  profile pages) keep 44px under it, clear of the silver's curved lower
  edge (about 40px high over the gutter). Status headings are 800, sentences
  600 at 0.94x with 1.3 line height; values may take 72% of the width so a
  Pass Key reads whole. The Pass Key page says one short sentence
  ("Enter it on another device to keep playing. Keep it private."); the
  save chip's popover keeps the details.
- **Visible viewport.** The canvas is `100dvh` tall (100vh fallback), so
  Safari's toolbars no longer cover the bottom of the menus.

Test: `tools/test_engine_menu_phone_layout.mjs` (Chromium; 390x664, 390x844,
844x330, 844x390, iPad, desktop): Back, Start Race and Copy ride link on
screen and clear of the scroll panes; rows clear of them where nothing
scrolls; Press Start on screen; Single Player's modes fit on portrait
phones; Select Track shows two whole cards; Single Race has no squeezed list,
its picker shows three whole cards (two on a landscape phone) and opens and
closes by touch, keyboard and a stand-in controller; stage text clear of the
rim. Physical iPhones are not tested here.

### Garage vehicle picker (2026-10-29)

The garage squeezed its vehicle list under the vehicle and rider rows: two
names on a portrait phone, one in landscape. The list is now a picker like
Single Race's: the vehicle row's arrows step through owned vehicles, and a tap
on it, Enter or A opens **Choose Vehicle** (the full ATV and motorcycle list
under a smaller stage that still shows the model and rider); a pick, Enter,
Escape, B or Done closes it (`Frontend::vehicle_sheet`). The garage stage
takes about half the portrait height again. Stepper labels ellipsize (giving
way after the value) instead of running under the value ("ATV 1/24 ·
Unlocked" on the store pages), and vehicle names fall back to the store's
list, so a locked ATV's plate no longer reads "Vehicle 22".
`test_engine_menu_phone_layout.mjs` covers the picker (touch and keyboard) and
the store label at every size.

### Audio page and Pass Key copy (2026-10-29)

Options → Audio opened on the song pager, put Back in the middle and left
Music, Volume and SFX as tap-to-cycle buttons under it. Music, the music and
sound-effects volumes (now sliders: drag, or Left/Right in 5% steps, as the
steering slider) and Sound lead the page and its choices, then the songs, the
pager and Back (`atv_core::ui_menu` reorders the music page's choices;
`Preferences::set_volume`). Pass Key with no key says one sentence ("Save
progress to get one, then use it on another device."); with a key, the key,
its status and one line. `test_engine_audio_page.mjs` scrolls the taller panel
to its targets.

### Selector input (2026-10-29)

Reported on an iPhone: cycling Single Race's courses got slow, and fast taps
often did not change the course. Host emulation (Playwright, 390x844 DPR 3
touch; Chromium at x4 CPU throttle, WebKit unthrottled; `work/` probe, not a
physical phone) on the live build:

- **Overlapping taps were dropped.** The frontend kept one press: a second
  finger down before the first lifted replaced it, and the first release
  (`take().filter(pointer)`) then discarded both. Two thumbs alternating at
  4-10 taps a second stepped once in ten taps (9 of 10 dropped); one finger
  at 1-10 taps a second and keys dropped none. Options/Jukebox rows (the
  pause menu) lost one of each overlapping pair the same way.
- **No auto-repeat.** Held keys stepped once (the key's repeats were
  ignored), a held controller direction once, a held arrow never repeated.
- **Each new course built its artwork on the input's frame**: the blurred
  backdrop and the graded photo, 15-40 ms at x4 (frame prepare 30-67 ms, 3-10
  long tasks per 10 steps), per course id although a venue's race, enduro and
  stadium courses share a photo. Single Race also shrank all 51 thumbnails
  on open for a list it shows only in the picker.
- The Choose Course picker opened only from the Course row (tap or Enter);
  nothing on the page said so.

Changes: presses are tracked per pointer (`Frontend::pressed`,
`PauseMenu::pressed`), one finger at a time scrolls a pane, and a finger
down while another drags neither scrolls nor taps. A finger held on a step
arrow (photo arrows, garage vehicle arrows, stepper rows) repeats after
0.4 s every 110 ms and its release adds no step; held D-pad/stick directions
repeat at the same rate; held arrow keys step at the system's key-repeat
rate (front pages and the Options menu). Course artwork is lazy
(`Front::course_image`): the frame that changes the course draws its name
with the last course's images, later frames build one image each, the
current course first, then the neighbours in the stepping direction (+1,
-1, +2), keyed per venue; stale courses skipped in a burst are never built.
Single Race's photo and course name open Choose Course, and an **All
courses** button sits on the photo (`front::COURSES`).

Host Chromium x4, Single Race (live `d5d8e3e` -> candidate): two-finger taps
at 10/s dropped 9/10 -> 0/10; held key 1 step -> one per key event; input to
new name p95 84 -> 64 ms (taps), 55 -> 19 ms (keys); long tasks per 10 taps
3-10 -> 0. `tools/test_engine_selector_latency.mjs` (Chromium x4 and WebKit):
10 rapid taps, 10 overlapping two-finger taps (Chromium) and 10 rapid keys
on Single Race and the garage each step exactly ten times, every input shows
in the next frame published after it, a held finger and a held key repeat,
the photo catches up with the course, and the photo opens the picker.
