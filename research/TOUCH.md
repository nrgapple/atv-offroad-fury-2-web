# Touch riding controls

## Current enhanced-mobile airborne trick wheel

In enhanced one-handed mode, keep the primary finger anywhere on the game surface to drive and brace. During a jump, place a second finger anywhere on the game surface to open a radial selector. Drag from its center into one of the named sectors, then lift to submit that stunt once. The eight inner sectors match the standard two-finger swipe directions (up Nac Nac, up-right Point Back, right Bar Hop, down-right Cliff Hanger, down No Footed Can Can, down-left Nothing, left Suicide No Hander, up-left Heel Clicker). Cat Nac (up-right) and Cordova (up-left) sit in the outer band, which starts 132px from the center; the band's gaps, including straight up, fall through to the inner sector. The wheel previews selection without starting a trick; returning to the center and lifting cancels it. The wheel stays within the screen at edge presses, and the primary finger keeps its drive, steering and lean axes while the second finger selects. It does not request a held extension. The chosen sector fills gold and its full name shows above the wheel; labels stay plain white outlined text. See [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md).

With the optional Extras **Flips** setting on, flips come from the driving thumb rather than this wheel. Pull it fully back, or fully left or right, and lift as you jump (up to 0.6 s into the air); a gold "LIFT → BACKFLIP" or "LIFT → 360" cue shows when that will work. See [FLIPS.md](FLIPS.md).

These are authored touch-input adaptations that reuse ten existing stunt animations and the original Rust stunt inputs. The existing airborne, speed, reposition-timer and main-motion gates remain in force; the wheel changes neither animation nor physics. Browser checks cover real Chromium multi-touch and a synthetic WebKit pointer/capture fixture across portrait, landscape and small viewports. Physical-phone behavior remains unverified.

Public verification at `e03bc2b89a35cfd7048fbd69841105c61cd10444` passed CI/deployment run `38485063543`. Chromium checks on the public build passed portrait and landscape ground reverse, preserved primary axes, secondary release/cancel, natural-hop Nac Nac/Cat Nac/Cordova preview-and-release, and Pause/Camera utility touches, with no camera orbit, browser errors or file picker. The unchanged native two-second reposition gate correctly declined an early release at simulation step 117; after the test fixture waited 130 simulation steps, the public retry passed. This verifies browser behavior, not physical-phone input.

The swipe and circle mappings below remain active when the explicit **One-handed Off** mode is selected. They are not the enhanced one-handed second-finger control described here.

The left half is a floating steering/lean pad: horizontal motion steers, vertical motion leans forward/back on the ground and in the air. It never supplies throttle or brake. Its origin remains fixed while held, so takeoff cannot discard an existing pullback.

The primary right finger controls drive and prepares preload. Press anywhere on the right to request full throttle immediately; drag backward/down beyond an eight-pixel jitter margin for progressive braking, then reverse near standstill. Forward input while reversing brakes first. Direction changes are resolved in Rust on every fixed step with current signed speed and a one-foot/second standstill tolerance. Direction stays selected while airborne because body rotation can change the sign of body-relative speed; opposite right input brakes instead of switching gear. Releasing the right finger drops touch throttle/brake immediately, including in air, and submits one preload-release input. Left lean stays independent. These are authored browser adaptations, not recovered PS2 control mappings.

Every primary right hold prepares preload, including airborne holds, without bypassing the original ramp gates. A small marker follows vertical drive input and says **BRACED**, **RELEASE** when native arming is active, or **PRELOADED** during force application. There is no accumulating charge bar or separate brake/reverse button. Pause and Camera remain separate hit areas. In the standard touch mode, the primary right thumb switches to trick gestures at takeoff, with throttle/bracing suppressed; landing resumes the held drive input immediately. In enhanced one-handed mode, a second finger opens the radial selector while the primary finger continues driving.

## Native preload and the touch adaptation

The exact USA v2.00 build has a recovered lean-flick measurement at `001eb0d0` and rider-force routine at `001f0ae0`. Existing instruction comparisons in the private `drivetrain-rider-oracle.json` cover 700 grounded and 500 airborne cases with no mismatched compared fields. This is component instruction evidence, not a hardware or full-game match.

The original routine records time while lean exceeds 0.8. Crossing below −0.7 emits the axis rate. The force routine arms while grounded, moving forward above 30 feet/second and rising above 5 feet/second; lean back refreshes a 0.25-second arming window. A sufficiently fast forward flick starts native preload force for up to 0.25 seconds. Holding longer does not accumulate extra power, and pressing on flat ground does not grant a free jump.

The optional `preload_axis` channel supplies +1 during primary right hold and −1 for one submitted simulation-input batch on release. Physical lean comes only from the left touch (plus independent keyboard/controller input). Native force magnitude, timing and terrain gates are unchanged. When this optional channel is absent, keyboard and controller input retain the original shared lean axis.

Release is consumed at the simulation boundary so a visual-frame timer cannot expire while the worker is busy. Unseen taps, pointer cancellation, focus loss, resize, reset and menu transitions do not synthesize release. Exclusive primary ownership prevents another finger stealing drive; the accepted secondary right gesture is deliberately separate. Release drops touch drive even if the secondary stunt remains held.

## Airborne stunts

In standard touch mode, the primary right thumb switches automatically from drive/bracing to tricks when all wheel contacts are absent. Its gesture origin resets to the thumb's current location at takeoff, so the ground throttle drag cannot accidentally select a trick. In enhanced one-handed mode, a second finger opens the radial wheel from anywhere on the surface; only lifting a selected sector submits one stunt. The primary finger's throttle, brace, steering and lean remain active.

| Right-thumb gesture in air | Original stunt |
| --- | --- |
| Up / down / left / right | Nac Nac / No Footed Can Can / Suicide No Hander / Bar Hop |
| Up-left / up-right / down-left / down-right | Heel Clicker / Point Back / Nothing / Cliff Hanger |
| Clockwise circle / counterclockwise circle | Cat Nac / Cordova |

Swipe at least 36 CSS pixels and pause briefly (120ms) to select; this settling window lets a curved path become a circle rather than prematurely selecting a swipe. Circles require a near-closed loop, at least 44px width/height, and at least about 300 degrees of turning. Hold the thumb after a recognized gesture to request the original held extension; the native main-motion minimum still applies. Lift to release. A quick released swipe queues its modifier once, then releases it. Only one right finger owns this control. Cancel/blur/reset discard the gesture and pending input.

Landing clears trick input and immediately restores throttle/bracing for a held right thumb. Holding a trick through actual contact still reaches the original unsafe-landing classifier before browser feedback clears the gesture. These mappings are browser adaptations, not new stunt animations or physics. Ten of the original 24 stunts are exposed through these gestures; keyboard/controller mappings retain all 24. Physical iOS/controller validation and full flip behavior remain incomplete.

The runtime loads all 24 main stunt MOTs and 24 held extensions from the rider MCF catalog. This replaces the earlier secondary-finger Circle-family touch mapping described in historical checks below.

Recovered routines `002d9530` and `002d9870` determine modifier rearming, direction thresholds and held extension. Entry `002da6c0` requires no wheel contact, horizontal world speed above 4 feet/second, and an expired reposition timer; the exact 2-second reset timer is preserved. Per-stunt minimum main-motion times come from exact-build control tuning. The main clock pauses during extension; releasing resumes it. The selector and evaluator `002d8ba0/002d8e50` use the original main/hold motions and snapshot fade. Looping hold motion is sampled using the recovered one-dimensional gradient noise (`003454c0`, initialized by `00345650`), **not a sine wave**. A local original-RNG seed is used, so whole-game shared RNG order is not claimed.

The original manual's printed pages 7–8 describe the jumping-only down/up preload gesture, airborne cross-up and all 24 button/direction stunts. Printed page 11 describes blue preload opportunity and orange achieved preload. The recovered native HUD update `002e2290` agrees: orange records achieved strength and fades after the event, rather than filling while a button is held. Source: [original USA instruction manual scan](https://www.videogamemanual.com/PS2/ATV%20Offroad%20Fury%202%20%28USA%29.pdf).

## Presentation and remaining controls

Gameplay surfaces and their HUD suppress mobile text selection, selection gestures and the touch callout. This applies only to the game document and its stylesheet; the separate development log remains selectable. The touch marker disappears in menus and on focus loss. Non-error diagnostic copy is hidden during touch driving; errors stay visible.

Normal rider and named main/hold stunt animations are connected. Original scoring, profile restrictions, combo landing bookkeeping and rider IK remain incomplete. Native crash/reposition integration now handles unfinished stunt landings. The port currently exposes the recovered basic entry gates in its free riding state; it does not pretend the wider race/profile flags exist. Native `001e8690` crashes on any wheel contact while holding a stunt, or while a released main clip has at least 0.5 seconds remaining (`002d8a20`). `Animator::stunt_landing_safe()` exposes that exact predicate for crash integration. The score system's separate 0.5-second grounded grace is not an animation auto-finish rule. Crash state cancels the stunt through the integrated wreck transition. The workbench's older generic swipe display remains separate from these actual driving controls. Profile preload adjustments and whole-game replay equivalence also remain incomplete.

## Verification

`node --test web/touch.test.mjs` checks six gesture scenarios: primary right drive/preload and release, cancellation/reset/unseen taps, exclusive ownership, left steering/lean only, four independent secondary stunt swipes, and primary release while a stunt stays held. The Rust browser-input tests cover drive/brake/reverse decisions, takeoff/landing, cancellation/recovery and independent controller mixing. Native rider tests verify the original ramp gates, reject flat/slow/airborne launches, and demonstrate that physical forward/back lean changes while the optional preload channel is held do not fire the force; releasing that channel does.

`tools/test_touch.mjs` is an opt-in actual-content browser check. It uses actual Chromium touches at several points across the right half, observes submitted WASM inputs, checks continuous right drive during hold and independent left lean, release and cancellation, simultaneous left/right operation, braking and automatic reverse/forward transitions, focus/menu pause, narrow layout and game-only selection suppression.

The revised `tools/test_touch_ramp.mjs` drives the original Caribou start using right-up throttle, releases right on native arming, observes active preload with throttle zero, lands, and drives again. The normal captured run measured upward speed 9.37 feet/second at arming and 22.25 during active preload. The held-left-pullback variant retained lean +1 with throttle/brake zero at its first observed airborne frame; native pitch rate reached −0.9997 radians/second. Neither check injects poses or forces, and neither establishes a complete flip. Inspected current screenshots are `work/verification/touch-right-drive-held.png` (right marker raised during throttle/bracing) and `touch-right-drive-ramp.png` (actual airborne quad after release).

`tools/test_touch_stunts.mjs` now starts a second right touch after takeoff, selects original Nac Nac, verifies changing original rider matrices and modifier release/main-motion advancement, then expects the native Vertical wreck on unfinished landing. The primary right finger retains its drive/preload role throughout. This short hop does not establish a safe held-stunt completion. Earlier captures that held a stunt beyond wheel contact predate wreck integration and are not current behavior.

Chromium checks do not establish physical iOS Safari or Bluetooth-controller compatibility. Those hardware checks remain outstanding.

A private actual-content animation probe sampled all 24 main clips and 24 held clips through start, hold, release and completion: 8,640 finite original rider poses. This checks the MOT runtime and catalog integration, not complete original stunt scoring or crash equivalence. New core tests cover all 24 modifier/direction selections, native start thresholds, reset cooldown, modifier rearming, held-clock pause/resume and noise initialization/periodicity.

A private 600-case comparison executes original `002d8ba0/002d8e50` instructions for modes 4 and 5 and compares Rust main/hold clocks, snapshot transitions, blend weight and clamped sampling phase: zero mismatches. Pose sampling and noise return are hooked, and arithmetic uses finite host binary32; this is not PS2 hardware or full scoring/input equivalence.

## Historical takeoff continuity and retained-page recovery

The traces below describe the earlier left-drive/right-lean scheme, now superseded by the separate left-lean/right-drive mapping above. They remain component regression evidence, not instructions for current controls.

The takeoff recenter was removed after the user reported ineffective pullback. In the revised actual Caribou test, pullback began at step 52, grounded brake input appeared at steps 55/58/61, and the first observed airborne frame at step 70 already carried backward lean +1 with the touch-generated brake cleared. Native pitch rate subsequently reached −0.998 radians/second; body tilt reached 30.38 degrees during the short observed gesture. The test does not modify forces or pose and does not establish a complete backflip. A separate normal preload run returned to four wheel contacts after touch release and drove again, verifying landing followthrough. Prolonged full pullback can overturn the ATV; complete original crash/reposition behavior remains separate work. Captures: ignored `work/verification/touch-held-pullback-ramp.{json,png}` and `touch-baseline-landing.json`.

`tools/test_lifecycle.mjs` covers a verified browser lifecycle defect: the old unconditional `pagehide` handler terminated the Rust worker and closed audio even when the browser retained the page. Persisted hides now suspend rendering/audio and clear input while retaining resources; persisted shows restart one render loop with fresh timing. The frontend separately resumes menu polling. Actual-content WebKit and Chromium checks drove airborne, dispatched **synthetic** persisted hide/show events, verified suspension then continued body motion with the same worker, checked stale throttle clearing, and exercised pause/resume. Natural Back navigation in the tested installations reloaded the page instead of using BFCache. Thus this regression proves the event-handling fix, not that retained navigation caused the user's hovering report. Ordinary host and public touch driving/release/resume attempts did not reproduce persistent hovering.

`tools/test_touch_pose.mjs` separately verifies the restored pose-only preload feedback: stationary right hold with neutral physical lean and release produce different original rider matrices, while native preload force remains inactive. The inspected `work/verification/touch-preload-pose-held.png` and `touch-preload-pose-released.png` show that pose change. This is a visual preparation cue, not evidence of a free jump or accumulated charge.

## One-handed mobile option

One-finger control is the default. Pause → **One-handed mobile: On/Off** saves the choice in this browser; an existing saved Off continues using two fingers. A single thumb anywhere on the game view holds full throttle and braces immediately. Drag left/right to steer and up/down to lean forward/back, using the same floating-stick axes as the normal left thumb. Vertical movement never brakes or requests reverse. Lift to remove throttle (coast, without braking) and request preload through the original ramp gates.

These inputs stay identical in air and on the ground: no touch mode transition, throttle suppression, origin reset, or landing delay. Holding continues both throttle and bracing; lifting in air removes throttle and releases the brace without bypassing native jump rules. In One-handed mobile mode, a second touch anywhere on the game surface adds a separate action while the first touch remains primary. On the ground, the second touch requests braking and then reverse; when it lifts, the held primary touch resumes forward throttle and brace. In the air, it opens the radial trick wheel: drag into a named sector and release to submit it once; return to the center before releasing to cancel. The primary touch keeps steering/leaning and drive/bracing while the stunt runs. Releasing the primary touch clears both touches. Further gameplay touches are ignored, and Pause/Camera keep their own button hit areas. Cancellation, pause, blur and changing modes clear input without producing a preload release. Storage denial falls back to a session-only setting. The saved Off setting and the standard two-finger controls remain available, as do independent keyboard/controller bindings.

## Historical second-touch swipe and circle checkpoint (2026-10-13)

The saved **One-handed mobile: On/Off** preference remains intact, including a previously saved Off. In On mode, the first touch anywhere on the game surface is always the primary floating control. An optional second touch works anywhere else on the game surface: it requests ground braking/reverse, or selects an airborne stunt through the existing gesture recognizer. A secondary release immediately returns to the primary touch's forward drive/brace; a primary release, cancellation, pause or focus loss clears both. Additional gameplay contacts are ignored. Utility buttons remain separately owned.

Actual-content Chromium checks pass in portrait 390×844 and landscape 844×390. They verify ground reverse, preserved primary steering/lean, secondary release and cancellation, and zero touch camera orbit. A natural Caribou hop selects Nac Nac with an upward swipe, Cat Nac with a clockwise circle, and Cordova with a counterclockwise circle while primary drive/brace continues; releasing the secondary touch releases the stunt independently. Normal one-handed, touch, UI and utility-button regressions pass. These are host browser checks; physical iOS Safari and Bluetooth-controller behavior remain unverified. This touch adaptation does not establish complete stunt, physics or whole-game parity.

## Compact one-finger feedback and default

One-finger mode is the default for new/unset settings and unavailable storage.
An explicitly saved Off is preserved, and Pause still offers the two-finger
option. The TouchInput constructor and both modes' mappings remain unchanged.

The compact floating circle shows a moving vector and FWD/BACK/L/R directions.
A small cue attached to the marker shows drive/brace or the native preload cue;
it disappears immediately on release. There is no persistent card, percentage,
meter or ground/air heading. Diagonals combine steering and lean independently
at the unchanged 70-pixel range. Keeping the visual marker within the viewport
does not move the gesture's input origin.

The display reads intent without calling the input sampler, so it cannot consume
preload releases. Unchanged display state causes no DOM rewrites. Focused checks
cover unset/storage-denied/saved modes, cardinal and diagonal vectors, release
hiding, preload cues, air continuity and clean held-throttle landing. No input
mapping or physics changed; physical phone validation remains outstanding.

## Original main-stunt air-control gate

The vehicle now receives the rider mode selected by the preceding animation update. Exact-build `001e7940` checks mode 4; `002d8ba0/002d8e50` distinguish that main-stunt mode from mode 5, the held extension. While airborne in mode 4, the original stunt axis slew rates replace normal rates, and `001f0ae0` halves only the input-generated angular moment. Angular damping and lateral air force remain unchanged. The original force routine reads dead-zone-adjusted raw axes, so the slower filtered axes affect steering/center-of-mass and pose, not an additional delay before rotational input. Held extensions and ordinary air control use the full input moment. Automatic recovery clears the mode gate.

Previously both gates were hardcoded false despite the recovered force routine supporting them. This correction reduces excess rotation during the main trick phase; it does not increase ordinary jump rotation or remove the original airborne crash angles. No force coefficient or touchscreen sensitivity changed.

Validation: two component tests distinguish input moment from damping and side force, and exercise airborne/grounded filtering transitions. A fixed-step vehicle test compares ordinary, main and held modes: ordinary and held angular-velocity traces are identical, while the main phase has reduced pitch response. A private 60-sample varying-input comparison using ATV01 air coefficients matches the exact `001f0ae0` instructions bit for bit; the old gate disagrees on all 20 main-phase samples and matches the other 40. This is a component comparison, not a PS2 gameplay replay.

The current Caribou short-hop stunt check no longer expects a full held extension: it verifies original Nac Nac entry, changing rider matrices, modifier release/main-time advancement, then the original Vertical wreck for unfinished landing. The historical longer-time held sample above predates crash integration and does not establish a safe stunt completion.

The quad linkage now also receives a separate visual steering angle. Native `001e7940` stores unscaled smoothed-X × 30° at `+440` during ordinary riding and held stunts, but multiplies that stored angle by 0.8 on each main-stunt update. Physical tire steering at `+444` remains speed-scaled and receives no such decay; `001dd830` passes that field to the tire subsystem, whereas `001d65c0` uses `+440` for linkage and handlebars. Both stored angles ease toward zero during wreck updates. Construction initializes the new visual state to zero; automatic reposition preserves the already-decaying steering state, consistent with the recovered reposition routine having no steering-angle assignment. This removes the former conflation of visual and physical steering. A focused high-speed test verifies distinct angles, two successive main-stunt decay steps, and restoration in held mode while physical steering stays unchanged; the wreck test checks decay of both stored angles.

## Fixed-step touch decisions and camera ownership

Primary right-thumb intent crosses the worker boundary as held state, vertical drive axis and cancellation generation. Rust `browser_input::TouchDrive` resolves it before each fixed vehicle step using current wheel contacts and signed speed. Left steer/lean remains independent across ground/air transitions. Grounded opposite input brakes before changing direction near standstill; airborne opposite input brakes without changing the remembered gear. Releasing right stops its throttle/brake in either state. Keyboard/controller mappings and all original force coefficients remain unchanged.

Cancellation, blur/pause clear and manual reset clear touch state; wreck recovery discards remembered direction. Returned frames include resolved input for audio and diagnostics. An old batch cannot overwrite the cleared input display with an older cancellation generation. Left and primary right have exclusive owners, with a separately owned airborne secondary right stunt gesture.

`tools/test_touch_batches.mjs` replays loaded Caribou resources into two fresh WASM instances and compares all returned frame values at native f32 precision, accounting for the extra JSON round trip. The updated 124-step actual-map sequence agrees between four-step batches and individual calls. Replaying identical per-step packets in all three shifted chunk phases places takeoff inside a batch and checks that the following step retains left lean +0.25 with released right throttle/brake zero. The same controls persist through landing; four cancellation-generation steps are neutral. These are authored-port transport comparisons using original content, not console replay equivalence. No pose or force is injected.

A separate confirmed camera fallback occurred when a right touch was accepted by the generic canvas handler before the right gesture handler became active. Its movement then entered mouse orbit. Gameplay touch can no longer enter that fallback, and touch interaction reacquires gameplay focus. A controlled synthetic blur → touch-down → focus → drag comparison changed the committed code's orbit by −0.2 radians with no preload; patched code kept orbit at zero with preload held, both while Riding. The permanent portrait regression additionally requires right touch alone to resume after blur, then drags both thumbs horizontally/vertically while asserting every submitted orbit remains zero. Original chase-camera movement following body motion is separate; the user's exact physical iOS event sequence remains unverified. Inspected private before/after captures are `work/verification/touch-orbit-before.png` and `touch-orbit-after.png`.

## Immediate right-side throttle (2026-10-12)

At the user's request, a primary press anywhere on the right now requests full
forward throttle immediately. A backward/downward drag beyond an eight-pixel
jitter margin requests progressive braking, reaching full brake at seventy
pixels. The existing Rust direction gate switches to reverse after slowing.
Returning to the press position requests forward again; release/cancellation
drops drive. Holding still also braces preload, and the secondary airborne
finger continues to select tricks independently. This is an explicit mobile
input adaptation, with no change to the underlying engine/brake behavior.

Unit and actual-WASM Chromium touch checks cover stationary presses at multiple
right-side positions, forward/reverse direction changes, release, cancellation,
left-thumb isolation, secondary fingers, focus and pause. The controls page now
explains press-to-drive and backward drag.


The immediate-press route passed the public Chromium touch suite on release `0f074b4558f864ea4e4a8103b017b60e102c6690` (CI `38202065109`). This verifies the browser adaptation and cancellation paths, not physical iOS/controller equivalence or original handling parity.

## Mobile steering response

Pause → Controls offers a saved Mobile steering response from 50% to 150% in 5% steps, defaulting to 85%. It applies to both one- and two-finger touch modes. At 100%, steering matches the previous input response. The scale is applied to displacement before the unchanged full-lock clamp; it does not alter lean, brake or preload input. Keyboard and controller bindings are unaffected.

## Mobile Pause and Camera touch ownership (2026-10-13)

A held riding finger suppresses the browser's compatibility click for a second
finger. Actual Chromium touches reproduced Camera staying on the same preset
and Pause staying closed even though both utility buttons received pointer
press/release events. Idle primary taps worked and both buttons remained
uncovered 44-pixel hit areas, so changing their stacking order would not fix
this failure.

Toolbar buttons now handle touch release independently of the captured riding
pointer and invoke their existing action once. Cancelling, dragging away or
losing capture does not activate a button. Native touch is cancelled within the
utility buttons to prevent a duplicate synthesized click; mouse and keyboard
activation retain their usual click path. Camera leaves the riding finger held;
Pause clears riding input, and Resume requires a fresh press.

`tools/test_mobile_utilities.mjs` passes actual-content Chromium touch checks at
390×844 portrait and 844×390 landscape: primary and held-finger utility taps,
one activation per tap, cancellation/drag guards, visible 44px targets, pause
freezing, neutral resume, mouse/keyboard activation and no browser errors.
These are host browser checks; physical iOS Safari remains unverified. Selected
screenshots stay in ignored private storage for the integrated dev-log gate.
