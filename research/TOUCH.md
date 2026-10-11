# Touch riding controls

The left half remains a floating steering/driving pad. Push forward to accelerate; pull back to brake, then keep holding back to reverse once nearly stopped. Switching from reverse to forward also brakes first. The browser direction adapter uses a one-foot/second standstill tolerance; this is a browser convenience, not a recovered original controller mapping.

Anywhere on the right half of the gameplay canvas is a hold/lean/release surface. Press and hold to prepare preload, drag up or down to lean forward/back while holding, and release near the ramp lip. There are no separate preload, brake or reverse buttons. A small marker appears at the touch origin and shows lean movement; it says **RELEASE** only when Rust reports native preload arming, and briefly **PRELOADED** when the native force is active. The small Pause and Camera HUD buttons remain separate hit areas.

## Native preload and the touch adaptation

The exact USA v2.00 build has a recovered lean-flick measurement at `001eb0d0` and rider-force routine at `001f0ae0`. Existing instruction comparisons in the private `drivetrain-rider-oracle.json` cover 700 grounded and 500 airborne cases with no mismatched compared fields. This is component instruction evidence, not a hardware or full-game match.

The original routine records time while lean exceeds 0.8. Crossing below −0.7 emits the axis rate. The force routine arms while grounded, moving forward above 30 feet/second and rising above 5 feet/second; lean back refreshes a 0.25-second arming window. A sufficiently fast forward flick starts native preload force for up to 0.25 seconds. Holding longer does not accumulate extra power, and pressing on flat ground does not grant a free jump.

The requested touch design separates physical lean from preload preparation. An optional `preload_axis` channel supplies +1 during right hold and −1 for one submitted simulation-input batch on release, while the ordinary lean axis follows vertical dragging. This lets the rider adjust pitch while holding without accidentally releasing the preload. Only the original arming/flick checks read the optional channel; wheelie, body lean and air control retain the physical lean input. Native force magnitude, timing and terrain gates are unchanged. When this optional channel is absent, keyboard and controller input use the original shared lean axis. This input adaptation is authored browser behavior, not a newly discovered original controller scheme.

Release is consumed at the simulation boundary so a visual-frame timer cannot expire while the worker is busy. Unseen taps, pointer cancellation, focus loss, resize, reset and menu transitions do not synthesize release. A second right finger cannot steal the held gesture. Releasing the right finger leaves left-hand throttle/steering active.

## Presentation and remaining controls

Gameplay surfaces and their HUD suppress mobile text selection, selection gestures and the touch callout. This applies only to the game document and its stylesheet; the separate development log remains selectable. The touch marker disappears in menus and on focus loss. Non-error diagnostic copy is hidden during touch driving; errors stay visible.

Normal rider lean/air animation is connected. Original trick modifiers, named stunts, scoring, crash transitions and rider IK remain unimplemented. The workbench's generic swipe display does not execute those original tricks. Original input callbacks/gameplay-state gates, profile preload adjustments and whole-game replay equivalence also remain incomplete.

## Verification

`node --test web/touch.test.mjs` checks five state scenarios: independent lean/preload, exactly-once release, cancellation/reset/unseen taps, pointer ownership, and braking before both forward/reverse direction changes. Native rider tests verify the original ramp gates, reject flat/slow/airborne launches, and demonstrate that physical forward/back lean changes while the optional preload channel is held do not fire the force; releasing that channel does.

`tools/test_touch.mjs` is an opt-in actual-content browser check. It uses actual Chromium touches at several points across the right half, observes submitted WASM inputs, checks continuous lean during hold, release and cancellation, simultaneous left/right operation, braking and automatic reverse/forward transitions, focus/menu pause, narrow layout and game-only selection suppression.

`tools/test_touch_ramp.mjs` drove straight from the original Caribou Peak start using simultaneous touches, held the right half, and released when native arming appeared. The revised gesture produced native active preload, strength 1 and airborne wheel contacts. One captured run measured upward speed of 9.28 feet/second at arming and 25.02 feet/second during active preload. The inspected screenshot shows the airborne quad and transient marker. This is an observed trajectory, not a paired experiment isolating preload from slope and other forces. No vehicle state or forces were injected. The trace and screenshot are ignored artifacts under `work/verification/touch-anywhere-ramp.{json,png}`.

Chromium checks do not establish physical iOS Safari or Bluetooth-controller compatibility. Those hardware checks remain outstanding.
