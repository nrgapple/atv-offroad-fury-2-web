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
- Not yet in the Rust menu: Jukebox, Copy ride link and the Alpha experiments
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
stays a gameplay key if bound. Controller rows use the standard-layout names of
pad-glyphs.js as text; the glyph icons are not drawn yet, and pinning a specific
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

The main, single and garage differences are the missing 3D quad and rider
previews (their left columns are 1.8-5.3 / 1.0-8.4, the latter from the hidden
rows). Hidden because the Rust engine cannot run them yet: Multiplayer, Arlington
Free Ride, Jukebox, Race setup, Rider gear and Alpha. Race links on a motorcycle
open the main menu with the reason, as motorcycle races are not ported. Not yet drawn: the 3D model previews, the Free Ride rider count,
backdrop-filter blur on the photo arrows, the menu notice line and the training
venue's photo (a devlog image in the browser). The loader shows one frame per
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
