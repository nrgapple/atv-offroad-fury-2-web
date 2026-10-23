# Switch-over readiness: JavaScript game → Rust engine

Audit of 2026-10-21 against `origin/main`. It covers every JavaScript game module that
will be deleted when `play.html` starts the Rust engine (`engine.html` today).

Each row is a user-visible behaviour or URL parameter of the JavaScript game, with
its status in the Rust engine:

- **Ported**: present and wired.
- **Partial**: present but incomplete, with what is missing.
- **Missing**: absent.
- **Not needed**: replaced by another Rust mechanism, or JS-only plumbing.
- **In progress**: owned by another agent; not listed as a gap.

Statuses were checked in the Rust code, not only in RUST_ENGINE.md. "Unverified"
marks rows that were not confirmed either way.

Nothing has been switched or deleted. `play.html`'s PWA, offline and analytics
modules (`install.js`, `offline.js`, `sw.js`, `analytics.js`, `landing.js`) are not
on the deletion list. The gaps below are about wiring them to the Rust page.

## Blocking gaps

These would regress players at switch-over. Highest impact first.

1. ~~**No race from the menus.**~~ Closed: Single Player → Race opens the race
   setup (event tabs and course list, laps, CPU riders, difficulty, preview arrows,
   Start Race, Copy ride link), and the last started race is remembered.
   `?menu=race` opens it, and a race link with `play=0` shows its setup
   (`tools/test_engine_race_gear.mjs`).
2. ~~**iPhone silent switch.**~~ Closed: `sound::sync_audio_session` sets
   `navigator.audioSession.type` (`playback`, or `ambient` for Silent switch → mute)
   and Settings → Extras shows the row where the session exists
   (`tools/test_engine_session_settings.mjs`).
3. ~~**PWA shell and offline.**~~ Closed by the PWA/analytics/render-scale batch.
   engine.html now carries:
   - the manifest, icons, apple meta tags and theme-color;
   - the install banner and iOS help (install.js);
   - the standalone viewport fix (`#game` canvas sized by `--app-width/height`);
   - launch screens;
   - offline.js.

   The frontend mirrors its page to `#frontend` for the banner. sw.js precaches
   and serves `/engine.html` with its module graph, including `atv_app_bg.wasm`.
   Remaining at switch-over: the manifest's `start_url` and shortcuts still name
   play.html.
4. ~~**Multiplayer links.**~~ Closed: `mode=multiplayer`, `lobby=` and `action=create|host`
   links open Create/Join Lobby with the link's code and name (the saved name unless
   `player=` is given) and host or join with `play=1`, on the link's map; motorcycles get
   the JavaScript refusal. Invites carry the rider's own ATV and rider and this page
   (`multiplayer::invite_url`). A bare `?room=CODE` link joins through Join Lobby as
   launch.js reads it (player, vehicle aliases, saved gear, `play=0`, motorcycle refusal);
   only the test hooks `?host=1` and `room=` with `dropAfter=`/`trainPlace=` ride at once.
   Not verified: an ended room reopening on the link's map (needs a relay).
5. ~~**WebGL context loss.**~~ Closed on the web (atv-app `context.rs`): like game.js,
   the engine stops drawing and simulating and shows "Restoring graphics…". Once
   the context is restored, it rebuilds in place on a new device: the scene, the ride's
   and race CPUs' models, the HUD and the menus. The ride keeps its position, step
   count, race and room slot, and loss during a load or under a menu recovers too
   (`tools/test_engine_context_loss.mjs`; relay: `test_engine_multiplayer_context.mjs`).
   Differences: the line is the `#status` strip, not game.js's loading panel. Old
   objects are released on the restored context, so Chromium logs "object does
   not belong to this context" warnings. If no device opens, the page reopens
   the ride's link (`fury-resume`).
   Desktop: a lost GPU device (driver reset) opens a new device and reopens the
   current ride from its start; a room keeps its connection and slot. A direct
   ride with no frontend exits with the reason instead of panicking.
6. ~~**Rider Gear page.**~~ Closed: Garage → Rider gear steps the Profile Store
   catalog and the quad paint. The garage and gear previews show the saved paint and
   the menu rider in the chosen gear.
7. ~~**Sound mode at ride start.**~~ Closed: played rides use the saved mode,
   Enhanced by default; `?sound=` wins and scripted rides keep Original.
8. ~~**No adaptive render resolution.**~~ Closed: atv-app `render_scale.rs`.
   The web surface renders at most 1.5 backing pixels per CSS pixel and steps by
   0.25 to 1.0 after 1.5 s of slow frames while riding, with the browser's
   recovery and backoff. The UI stays in CSS pixels.
9. ~~**Analytics.**~~ Closed: atv-app `usage.rs` sends all five events through
   analytics.js (`window.furyUsage`, set up in engine.js): `session_start`,
   `ride_start` (with load seconds and, online, players), `ride_failed`,
   `ride_time` (riding time stops in menus) and `race_finish`. Before deleting
   launch.js, move the `ENVIRONMENTS` list that analytics.js imports from it.
10. ~~**Missing sounds.**~~ Closed: ENV08 train spatial sounds (atv-core
    `spatial_audio` scene commands driven from the ride's car transforms), the
    `rain.vag` loop while Rain is active, and the Crazy/Gorn crash boom
    (audio/synth.rs `crash_boom`) all play. A scripted JS/Rust audit gives equal
    event counts per category (ENV08 train 11, ENV04 rain 1, ENV03 Gorn crash
    boom 1). Frontend menu music landed in #267.
11. ~~**Audio page song picker.**~~ Closed: Settings → Audio lists the soundtrack
    four songs a page, with the playing song marked, the status line and the page
    count. Previous/Next page through it and wrap (`tools/test_engine_audio_page.mjs`).
12. ~~**Race links.**~~ Closed: `cheats=eatdirt,bringit` reach the race settings and
    motorcycle race links start (`tools/test_engine_links.mjs`).
13. ~~**Race intro skip**~~ Closed: a click or tap off the toolbar and a new pad
    press skip it too.
14. **Smaller regressions.**
    - ~~Controller pinning (`padselect` does nothing).~~ Closed: Controller
      cycles Auto and each connected pad; a pinned pad drives while connected.
    - ~~The Fullscreen row shows even where the Fullscreen API is missing (iPhone).~~
      Closed: shown only where `document.fullscreenEnabled`.
    - ~~CPU racers and remote riders get no contact shadows and cast no High
      shadow-map shadows.~~ Closed: contact blobs within 240 ft and the shadow map
      within 40 ft, as game.js.
    - ~~No buzz when a touch flip arms.~~ Closed: `vibrate(12)` once per armed
      zone where the binding exists.
    - ~~No menu page transitions.~~ Closed: each page shown slides in (240 ms,
      36 px, `cubic-bezier(.16,1,.3,1)`) on the frontend, over the main menu and in
      pause menus, at rest under reduced motion (`tools/test_engine_menu_transitions.mjs`).
    - ~~Non-standard pads get standard glyphs.~~ Closed: an empty browser
      `mapping` (desktop: no gilrs layout) shows "Button N / Axis N".
    - ~~Ride prefetch is serial.~~ Closed: `Content::prefetch` (game.js
      `prefetchRide`). With 150 ms request latency, env01 loaded in 4.6-5.2 s
      against 5.8-6.4 s serial (host-only).
    - ~~Web soundtrack falls back to silence (not VAG decoding) when a stream is
      missing.~~ Closed: a missing or failed stream decodes the original VAGs on
      the mixer's music bus for the rest of the session.

In progress elsewhere, not counted as gaps: the diagnostics overlay (F3).

## Can be dropped

- **workbench.html with app.js, controls.js, input.js and style.css.** This is the
  developer disc explorer and controls tester, not part of the game.
  tools/test_browser.mjs and tools/test_music.mjs open workbench.html and need
  retargeting or retiring. tools/package_release.py lists these files.
- **worker.js and atv_web.wasm** (crates/atv-web, built by tools/build_web.py).
  This is the JS host's decoder and simulation worker; Rust runs everything in
  atv_app. Retire it together with workbench.
- **`?source=disc` and the hidden `#disc` input.** This local ISO/.tr2 path is
  unused, and hosted content replaces it.
- **Hidden play.html `#fullscreen`/`#reset` buttons and `#help` text.** Replaced by
  the Settings Fullscreen row, the Reset binding and the keyboard card.
- **`window.atvTerrainStatus`.** Replaced by `?uiDiagnostics=1` → `#ui-state` and
  `window.furyMultiplayer`. Drop it only after migrating the browser tests that
  read it (about 113 tool files).
- **`window.atvParityView` and the JS side of tools/compare_engine.mjs.** This is
  JS-versus-Rust parity tooling.
- **race.js DOM fallback strip** (direction, WRONG WAY, mini radar, digit
  countdown). The JS hides it whenever the native race HUD exists, and Rust always
  draws that HUD.
- **WebGL/Canvas2D render code** (ModelView, HUD draw lists, minimap Canvas2D,
  `trimDark`, `shadowGlState`, `observedSize`). Replaced by wgpu.
- **The water grid's Uint16 vertex cap.** Rust deliberately keeps the native row
  count (#219).
- **audio.js `updateStadium`.** Dead code: `loadStadium` always empties the list.
- **`snapshot()` hooks** in audio.js, music.js and quad-sounds.js. Test-only; Rust
  has `report()`.
- **content.js lazy music-chunk retention.** Superseded by the Rust 40 MiB LRU.
- **`atv-last-disconnect` session record.** Debug-only; can come with the
  diagnostics overlay.
- **Pre-v3 soundtrack `enabled:false` migration.** A one-off fix for old saves.
  Rust reads `enabled` without the version check; optional.
- **Keep:** index.html and landing.js forward old bookmarks to play.html, and stay.

## Per file

### Entry points: play.html, index.html, launch.js

| Behaviour / parameter | Rust | Where | Notes |
|---|---|---|---|
| index.html → landing.js forwards query/hash to play.html | Not needed | — | Keep as is |
| `parseLaunch` aliases: `map`/`environment`, `quad`/`vehicle`/`atv`, `rider`, `player`/`name`, `room`/`lobby`, `play`/`drive`, `cheats`/`cheat`, `mode`, `activity`, `action`, `laps`, `cpu`, `difficulty` | Ported | atv-game/src/launch.rs `parse_launch` (#210) | Every link except the test hooks (`venue=`, `host=1`, room `dropAfter=`/`trainPlace=`). A bad link's reason shows in the main menu notice |
| `activity=freestyle` link → course select, rides with `play=1` | Ported | atv-app/src/lib.rs `open_launch` | |
| `activity=race` link | Ported | lib.rs `open_launch` | `play=0` opens the race setup; cheats last through Start Race and Copy ride link (not saved) |
| `mode=multiplayer` + `action=create`/`host`, `lobby=` | Missing | lib.rs:2821 | Stale error |
| `room=CODE` invite | Ported | lib.rs `open_launch` | Join Lobby, as `mode=multiplayer` links (`tools/test_engine_menu_parity.mjs`) |
| `?menu=freeride` / `?menu=race` (manifest shortcuts) | Partial | launch.rs `menu_shortcut` | `race` → Single Player |
| `?source=disc` | Missing | — | Can be dropped |
| Copy ride link (`buildRideUrl`/`copyRideUrl`) | Ported | launch.rs `ride_url`, lib.rs `copy_ride_link` (#231) | The current page path; desktop links and invites open `https://furyremastered.com/play.html` and copy through the system clipboard (arboard) |
| Invite link | Partial | multiplayer.rs `invite_url` (#224, #268) | `quad=1&rider=0`, `/play.html` |
| Manifest, icons, apple meta, theme-color | Ported | engine.html | |
| Install banner, iOS help dialog, standalone viewport, launch screens (install.js) | Ported | engine.html, lib.rs `sync_shell` | |
| Service worker and offline shell (offline.js, sw.js) | Ported | sw.js `PRECACHE_PAGES` | `start_url` still play.html |
| Rust-only dev params | — | lib.rs `start()` | `venue, front, camera, weather, steps, graphics, crash, crashcam, chase, free, gear, paint, uibackdrop, sound, music, profile, autodrive, race=1, seed, host=1, name, dropAfter, trainPlace, script, pingMs, joinDelayMs, uiDiagnostics, ui=0, autopilot, presentTrace, traceAll, debug` |

### analytics.js (kept) and its callers in game.js

| Event / behaviour | Rust | Notes |
|---|---|---|
| `session_start` (entry: link or menu) | Ported | atv-app usage.rs |
| `ride_start` (mode, activity, map, vehicle, quad, laps, cpu, players, load seconds) | Ported | |
| `ride_failed` | Ported | |
| `ride_time` (visible unpaused seconds, flushed on hide/pagehide/menu) | Ported | Visibility and pagehide in engine.js |
| `race_finish` (place, field, seconds) | Ported | Unit-tested; not browser-tested |
| Opt-outs (webdriver, DNT, GPC, non-https/local) and device context | Ported | analytics.js itself |

### game.js

| Behaviour / parameter | Rust | Where | Notes |
|---|---|---|---|
| Hosted-content ride load | Ported | lib.rs `prepare`, atv-game/src/ride/load.rs | |
| Toolbar Pause/Camera (+ Keys) | Ported | atv-ui/src/hud/toolbar.rs (#191, #196) | |
| Reset ATV; race reset to last track point | Ported | ride/mod.rs `reset`, riding.rs | Pause → Restart Race in a race reloads the event, also after the finish |
| Fullscreen | Ported | lib.rs `toggle_fullscreen` (#202) | Settings row. The web fullscreens the document element, as frontend.js does, so the Host/Join inputs, install banner and status line stay visible; desktop goes borderless |
| Keyboard driving, bindings, mouse-drag orbit | Ported | ride/bindings.rs, riding.rs | |
| Race intro skip | Partial | ride/race.rs `skip_intro` | Keyboard only (blocking 13) |
| Pause on blur/hidden/menu; rooms keep simulating | Ported | lib.rs `sync_activity` | Web focus is the window's `focus`/`blur` (`Message::PageFocus`), not the canvas's: a Host/Join input or the install banner taking focus pauses nothing. Leaving a field (Enter, Escape, Tab) returns the keyboard to the canvas |
| bfcache pagehide/pageshow | Ported | lib.rs `resumed`, sound.rs (#191, #262) | |
| WebGL context-loss rebuild | Ported | atv-app context.rs, `SceneRenderer::rebuild`, `Overlay::rebuild_gpu` | In place on WebGL2 (wgpu has no WebGPU backend in this build); desktop device loss reopens the ride (blocking 5) |
| Adaptive render resolution | Ported | atv-app render_scale.rs | Also desktop on Metal and DX12 (PERFORMANCE.md) |
| Wake lock while riding | Ported | atv-app/src/wake.rs (#239) | |
| Impact and wreck rumble | Ported | gamepad.rs, ride/impact.rs (#250) | |
| Graphics presets (anisotropy, sky, interpolation, shadows, tracks, dust, fresnel) | Ported | lib.rs `graphics_preset` (#183, #197, #200, #217) | |
| Wheel re-projection | Ported | ride/present.rs (#245) | |
| Dirt buildup and water wash | Ported | atv-game/src/dirt.rs (#248) | |
| Keyboard help card | Ported | overlay.rs, hud/help.rs (#196) | |
| Status/loading line | Ported | `#status`, front `loading_stage` | |
| Boot loader and startup failure | Ported | engine.html `#loading-screen`, lib.rs `boot_loaded`/`fatal` | play.html's "Now Loading..." panel shows until the frontend draws; a startup failure closes it and shows the error on `#menu-notice` with `#frontend[data-error=true]` (`tools/test_engine_boot.mjs`). The meter sweeps, as play.html's, without stage progress |
| Static free camera | Ported (dev) | lib.rs `?free` | |
| `?source=disc`, `#disc` | Missing | — | Can be dropped |
| Diagnostics overlay (F3) | In progress | — | engine.js uses F3 for the debug strip; reconcile |
| `window.atvTerrainStatus`, `window.atvParityView` | Not needed | `#ui-state`, `furyMultiplayer` | Test hooks |

### app.js, worker.js, controls.js, input.js

| Behaviour | Rust | Notes |
|---|---|---|
| Workbench disc explorer and controls tester (app.js, controls.js, input.js) | Not needed | Dev tool |
| atv_web.wasm worker (worker.js) | Not needed | Rust runs in atv_app |

### content.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Manifest version/build-hash check | Ported | atv-content | |
| Per-chunk SHA-256, chunked records | Ported | atv-content `chunk` (#252) | Yields every 512 KiB |
| Archive directory reads, block inflate | Ported | atv-game/src/archive.rs | |
| Parallel ride prefetch with fetch priority | Ported | atv-content `prefetch`, atv-game `ride_prefetch` | |
| Soundtrack `.m4a` streams | Ported | atv-content `MediaStream`, sound.rs (#191) | `Manifest::parse` rejects the package unless each stream is on an `/AUDIO/MUSIC/*.VAG` record, `audio/mp4`, 1 byte to 25 MiB, a lowercase SHA-256 and `/content/<sha256>.m4a`, with positive seconds (content.js). Jukebox seek clamps to the manifest's length − 0.25 s |
| Lazy music-chunk retention | Not needed | Rust LRU | |

### wake-lock.js, graphics.js, alpha.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Wake lock: hold while riding, release in menus, re-acquire | Ported | wake.rs (#239) | Desktop: a display-sleep inhibitor (`keepawake`) under the same policy |
| Presets Auto/Original/Enhanced/High (`atv-graphics`) | Ported | lib.rs, menu/prefs.rs (#183, #202) | |
| `detectGraphicsTier` | Ported | lib.rs `detect_graphics_tier` (#228) | With `userAgentData.mobile` and `saveData`; main-menu Settings and rides share it (`device_preferences`). Desktop: from the GPU adapter (`desktop_tier`, PERFORMANCE.md) |
| Vibration setting and rumble (pad, else `navigator.vibrate`) | Ported | prefs.rs, gamepad.rs (#202, #250) | Rumbles the pad in use: the pinned pad, else Auto's |
| Dirt zones | Ported | dirt.rs (#248) | |
| Settings → Alpha toggles (`atv-alpha`) | Ported | prefs.rs `ALPHA_FEATURES` (#237) | |

### hype.js, hype-gl.js, speed-rush.js, motorcycle-presentation.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Big-air detection, apex slow-mo, big-air camera, landing dip | Ported | ride/hype.rs (#237) | Slow-mo is Freestyle only: off in races and rooms |
| Speed/air muffle, wind | Ported | audio/hype.rs (#255) | |
| Heavy mud zones | Ported | dirt.rs (#251) | |
| Landing burst; air dark blur | Ported | atv-render burst.rs, air_blur.rs (#251, #253) | |
| Speed rush: curve, FOV kick, taps, stencil, lines, vignette; reduced-motion default | Ported | atv-render speed_rush.rs (#204, #228) | The main menu's Settings use the same default. Only the player's vehicle writes the stencil; race CPUs and other riders blur |
| Motorcycle and Enhanced/High ATV interpolation | Ported | ride/present.rs (#217) | |
| Interpolation stretch under Alpha slow-mo | Unverified | — | |

### frontend.js and menu-screens.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Title, main menu, Free Ride "N riding" count, Dev log (web) | Ported | atv-ui/src/front (#210) | Idle menus poll the pad at display rate (buttons and stick past ±0.55, as frontend.js every frame) and draw only when its input changes (`paused_tick`, `tools/test_engine_menu_flows.mjs`). Tab/Shift+Tab walk the fields and every choice; Back opens a page on its first row; Start also takes the rebound controller Pause; a held button acts only after a release once focus returns (`padBlocked`). Deliberate difference: W/A/S/D also navigate the menus |
| Main-menu Jukebox | Ported | front.rs, soundtrack.rs | |
| Single Player → Race setup page | Ported | front/controller.rs, front/mod.rs | |
| Freestyle course select (tabs, photos, hero, keys, LB/RB) | Ported | front/mod.rs, controller.rs | |
| Garage (46 vehicles, rider, stats, 3D previews, ATV-only online) | Ported | front, app/front.rs (#254) | Previews wear the saved gear and paint, show "Loading model...", "Model unavailable" or "3D preview unavailable", and a pressed preview turns with Left/Right. Start leaves a room's garage |
| Rider Gear page | Ported | front/controller.rs `step_gear`, preview.wgsl paint | |
| Host/Join forms, validation, motorcycle refusal | Ported | front/controller.rs (#241, #254) | Escape in a field goes back a page; desktop pastes with Cmd/Ctrl+V. Free Ride rides the name as typed; a blank or "Rider" name becomes "Rider NNNN" for the session and the next save keeps it. Only a Freestyle ride changes the saved last Freestyle course |
| Free Ride Lobby page while connecting, then its riders | Ported | #268, this PR | |
| Loading screen (shots, tips, staged bar) | Ported | controller.rs `loading_stage` | A failed load returns to its page with the error on the notice line (frontend.js `fail`): Freestyle → course select, Race → race setup, Free Ride → main menu, a room → Multiplayer (main for Free Ride rooms), which also forgets the slot key. The tip is a uniform pick of all 14 |
| Pause menu (Resume, Reset, Players/Spawn, Change ATV, Select Track, Settings, Jukebox, Copy link, Quit/Leave) | Ported | menu/controller.rs (#202, #238, #242, #256) | Restart Race in races. Start and the rebindable Pause action resume; Start accepts in Settings over the main menu. Left/Right turn the Bindings and Audio pages and change Jukebox tracks. Only the Pause binding and Escape resume (a P no longer bound does nothing); Shift+Tab walks back. Jukebox always shows (the page waits for the catalog); desktop keeps Copy ride link (the site's play.html link) |
| Settings pages and toggles (Display, Extras, Alpha, Race overlays, Minimap, Controls, Controller, Bindings) | Ported | menu/controller.rs, prefs.rs (#202) | Applied live |
| Extras → Diagnostics | In progress | — | |
| Display → Fullscreen | Ported | lib.rs `toggle_fullscreen` | Shown where `document.fullscreenEnabled`; the whole page goes fullscreen |
| Extras → Silent switch | Missing | — | Blocking 2 |
| Audio page song list and paging | Ported | menu/controller.rs | |
| Controller Settings → pin a controller | Ported | menu/controller.rs, app/gamepad.rs | |
| Binding capture, presets, reset | Ported | menu/controller.rs | Clear binding follows the highlighted row; a capture keeps its row; blur cancels it |
| Page slide transitions | Ported | atv-ui transition.rs, `front::element`, `menu::slide` | animatePage's entry: the stage fades in from translateX(36px) over 240 ms, buttons move with it, input stays live; a resize cancels it; reduced motion shows pages at rest. Its 180 ms exit snapshot is of the cleared `#menu-canvas` (0 opaque pixels measured on play.html), so no exit is drawn. A stalled frame advances the slide by at most 33 ms; the loader opens at rest (it paints between load stages) |
| `atv-player-selection` persistence | Ported | controller.rs `save` | |

### hud.js, minimap.js, keyboard-help.js, race.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Speed/preload meter, stunt text | Ported | hud/meter.rs (#196) | |
| Start lamps; race HUD (lap time, lap, score, place, best/total/lead, history, signs, radar) | Ported | hud/race.rs (#223) | |
| Race overlay settings | Ported | prefs.rs | |
| GPS minimap and all its settings; tap toggles orientation; peer dots | Ported | hud/minimap.rs, overlay.rs (#196, #236) | |
| Keyboard card (live bindings, H, first ride 15 s, never on touch) | Ported | hud/help.rs, overlay.rs | Menus hide it; Ctrl/Alt/Meta+H do not toggle it |
| Race fade-in; results panel; Restart/Quit/Hide | Ported | overlay.rs, hud/results.rs (#225) | Quit Event always shows (a page without the engine's menus reopens them); Tab/arrows/D-pad move the focus, Enter/A activate it |
| DOM fallback race strip | Not needed | — | Hidden whenever the native HUD exists |

### touch.js, bindings.js, gamepads.js, pad-glyphs.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| One-finger compass, trick wheel, two-finger layout, flips, arcade spin | Ported | atv-ui/src/touch (#191) | The armed flip cue, flip-zone arcs with their 220 ms dwell fill and the Arcade airborne spin arcs and cues (touch.js `oneHandedFeedback`, touch.css) |
| Coaching, idle hints, sensitivity, one-handed | Ported | touch/mod.rs | Coaching counts one ride per page load |
| Flip-armed `vibrate(12)` | Ported | touch/mod.rs `flip_buzz` | |
| 17 actions, defaults, chords, migration, presets, trick grace (`atv2-gameplay-bindings-v1`) | Ported | ride/bindings.rs (#182) | |
| Storage-failure notice | Ported | settings.rs `STORAGE_ERROR`, menu/controller.rs | After a binding capture that could not be saved |
| Pad Auto selection | Ported | app/gamepad.rs (gilrs) | Resting-axis baselines, then the current, a standard-mapped or the first pad |
| Pin a pad by name | Ported | gamepad.rs `pinned` | Matched by name, then gilrs index (not the browser index) |
| Tuning sliders, deadzone (`atv2-controller-v1`) | Ported | bindings.rs `Tuning` | |
| Pads with more than 17 buttons or 4 axes | Ported | gamepad.rs `extra_inputs` | Up to 32 buttons and 16 axes; desktop orders a pad's unmapped inputs by event code |
| Glyphs and controller summary | Ported | menu/glyphs.rs (#230) | |
| Non-standard pads as "Button N / Axis N" | Ported | gamepad.rs `standard` | |

### multiplayer.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Host (custom code, 409/429 messages), join, reopen ended room, "Game is full!" | Ported | atv-app/src/multiplayer.rs `claim` (#224, #241) | |
| Arlington Free Ride (429/503) | Ported | multiplayer.rs (#229) | |
| Reserved FREERDnn codes: refusal / redirect to Free Ride | Ported | multiplayer.rs `claim`, front/controller.rs | Hosting one is refused; an invite, lobby link or Join code opens Free Ride (`tools/test_engine_freeride.mjs`) |
| Slot-key reconnect, 25 s window, backoff | Ported | multiplayer.rs (#234) | A closed socket drops the other riders, the room state and their sounds at once, also while reconnecting (multiplayer.js onclose). A room that closes for good ends the ride on Multiplayer (main for Free Ride) with the reason; Leave Game goes to the same page (`tools/test_engine_multiplayer_end.mjs`) |
| Meet a rider on arrival; Spawn beside (car velocity on trains) | Ported | multiplayer.rs, spawn.rs (#227, #238, #264) | Rider contacts sample each peer per step after the train step (atv-web op 50). Authored: a rider alone in a room sends a 1 Hz presence pose so an arrival can meet a host whose tab is in the background, and an arrival waits up to 15 s for a first pose (`tools/test_engine_multiplayer_idle_host.mjs`) |
| Change ATV in place (kept present behind the garage), host Select Track | Ported | lib.rs (#242, #256) | Like game.js, the held ride keeps simulating behind the garage or course picker with neutral input and sends unpaused poses at 50 ms (`Net::ride_held`); Change ATV reloads where it came to rest. The scene and ride sounds are not drawn or played behind the frontend |
| Copy invite link | Partial | `invite_url` | Blocking 4 |
| Pose send, pings, remote models (frame-spread), remote engine audio | Ported | multiplayer.rs, sound.rs (#224, #233, #252) | Models evicted past 20 cached; engine voices ranked by distance to the player's vehicle; inbox guard (192 poses, 256 closes 1008) |
| Name tags, peer dots | Ported | hud/peers.rs (#236) | Riders draw at any distance, as their tags do (drawRemotePlayers); only race CPUs stop at 864 ft |
| Shared train clock | Ported | ride/mod.rs (#232) | |
| `atv-last-disconnect` record | Missing | — | Debug-only |

### audio.js, music.js, jukebox.js, quad-sounds.js, audio-enhance.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Sample bank, VAG decode, loops; engine selection, idle | Ported | atv-game/src/audio/bank.rs, mixer.rs, ride.rs (#191) | |
| Enhanced crossfade and echo / Original | Ported | ride.rs, mixer.rs | |
| Sound mode setting | Partial | lib.rs | Blocking 7 |
| SFX/music volume, music on/off | Ported | sound.rs `set_volumes` | |
| Silent switch (`audioSession`) | Missing | — | Blocking 2 |
| Gesture unlock | Ported | sound.rs | |
| Motorcycle bank; rider vocals; body and chassis impacts; terrain loops; water splash | Ported | ride.rs, bank.rs | |
| Venue ambient, Soultree loops, crowd cue | Ported | ride.rs, audio/race.rs (#222) | |
| Race lamp/GO/gate one-shots, IntOut stingers, soundtrack hold/fade | Ported | race.rs, sound.rs (#222) | The race-over fade shortens to 80% of a song's remainder near its end. Stingers play on the mixer's stinger bus at the music volume, past the Alpha muffle and the Jukebox analyser (audio.js `playIntOut`); INT001-INT004 is a uniform pick |
| ENV08 train sounds | Ported | audio/ride.rs `train_step` | |
| Rain loop | Ported | audio/ride.rs | |
| Crash boom (Crazy/Gorn) | Ported | audio/synth.rs, sound.rs | 0.3 s cooldown |
| Alpha landing thump, muffle, wind | Ported | audio/synth.rs, hype.rs (#251, #255) | |
| Remote engines and impacts | Ported | audio/remote.rs (#233) | Heard from the drawn camera with the player's vehicle velocity (game.js multiplayer listener); the player's, venue and train sounds keep the follow camera |
| Frontend menu music (UIMUSIC) | Ported | audio/menu.rs, lib.rs (#267) | |
| Suspend on blur/hidden; no Media Session | Ported | sound.rs (#262) | |
| Soundtrack: catalog, shuffle, history (`atv-soundtrack-v1`), AAC stream, park/resume, muffle, spectrum | Ported | audio/music.rs, sound.rs | Desktop plays the same `.m4a` songs from its content pack, decoded with Symphonia (audio/stream.rs) |
| Soundtrack VAG fallback on the web | Ported | soundtrack.rs `vag_fallback` | Desktop: a song whose file is missing plays its VAG; a decode error switches the session to the VAGs |
| Pause → Jukebox (list, transport, seek, volume, PAGE14 art, spectrum) | Ported | menu/jukebox.rs (#235) | Desktop seeks the bundled remaster (the VAG fallback by block) |
| Main-menu Jukebox; one soundtrack player for menus and rides | Ported | soundtrack.rs, lib.rs `sync_soundtrack` | `tools/test_engine_menu_jukebox.mjs` |

### render-support.js, water.js, weather.js, fog.js, scenery-materials.js, train.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Frustum culling | Ported | atv-render/src/cull.rs (#183) | |
| Portrait chase, assisted chase, cinematic pre-race, tight crash camera | Ported | ride/view.rs, riding.rs (#203, #249) | |
| Gorn severing, quad paint shader | Ported | crash_effects.rs, vehicle.wgsl (#207, #220) | |
| `RenderScale` adaptive DPR | Ported | atv-app render_scale.rs | Desktop on Metal and DX12 too |
| Water grid, normals, reflection, fog; Fresnel on Enhanced/High | Ported | atv-render/src/water.rs (#183, #219, #228) | Invalid water data fails the ride load with its error, as game.js `new WaterRenderer` does; a restored graphics context that refuses it rides on without the surface |
| Weather selection; snow ring | Ported | venue.rs, snow.rs (#187) | The per-load sample is uniform (atv-app `random.rs`: `Math.random()` on the web, an OS-seeded SplitMix64 on desktop), so NAT06 rains 90% and SX13 80% of loads. The clock digits it replaced kept the sample in [0.5, 1) |
| Rain visuals | Unverified | — | Drawn as a scenery model by game.js |
| GS fog | Ported | shaders `native_fog_coverage` (#179) | |
| Multi-pass scenery materials | Ported | atv-game/src/materials.rs | |
| ENV08 train cars and wheels | Ported | atv-render/src/train.rs (#183); bike rides #259 | |

### wheel-effects.js, tire-tracks.js, shadow-map.js, vehicle-shadows.js, ambient-effects.js, ambient-streams.js, lingering-dust.js

| Behaviour | Rust | Where | Notes |
|---|---|---|---|
| Dust/chunk/spray and wet births; motorcycle MX atlas | Ported | atv-render wheel_effects.rs (#197) | |
| Gorn blood | Ported | blood.rs (#207) | |
| Tire tracks Enhanced/High, mud zones | Ported | tire_tracks.rs (#201) | |
| High sun shadow map | Ported | shadows.rs (#200) | Player, CPUs and riders within 40 ft |
| Contact shadow blobs | Ported | shadows.rs | Player, CPUs within 240 ft, other riders; resting lift measured every step |
| TOY effects (smoke, fire); type-11 fountains | Ported | atv-render ambient.rs (#186) | Far distance: fog end, or 768 ft without fog |
| Lingering dust Enhanced/High | Ported | lingering_dust.rs (#197) | |
