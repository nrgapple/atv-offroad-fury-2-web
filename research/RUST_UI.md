# Shared Rust touch and toolbar checkpoint

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
