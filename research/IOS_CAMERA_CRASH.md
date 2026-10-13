# iOS fullscreen, uphill chase and crash presentation

Implementation strategy (2026-10-13): give all fullscreen surfaces one explicit
viewport size, using the physical CSS screen only for Apple standalone mode when
its width matches the app and its height differs by a bounded system-inset gap.
Normal browser/split-screen/zoom layouts retain their visible viewport. Paint the
scene under system insets; keep interactive HUD and menus inside safe areas.
Size the WebGL buffer from the actual canvas rather than window.innerHeight.
WebKit tracks standalone percentage/fixed sizing gaps in
https://bugs.webkit.org/show_bug.cgi?id=254868; Apple's safe-area guidance is
https://webkit.org/blog/7929/designing-websites-for-iphone-x/.

The requested default chase adaptation raises its sightline smoothly on uphill
terrain and supported tilted ramps. Preserve the recovered camera simulation,
other presets and wreck view. Avoid following airborne flips; hold then release
ramp pitch as the vehicle leaves support. Keep the existing portrait framing.

Tune only Funny crashes: retain more launch momentum, add coherent tumble,
strengthen torso/limb braces and preserve terrain rebound through joint solving.
Keep native driving, wreck classification, contacts, scoring and recovery intact.
Verify exact paired enhanced/Off WASM results, actual GPU poses and recovery.
Host WebKit with synthetic standalone/safe-area signals cannot prove physical
Home Screen behavior; the user's device remains the final hardware check.

## Host verification checkpoint (2026-10-13)

Chromium and WebKit automated host checks passed rotation from 390×844 portrait
to 844×390 landscape and back, with the fullscreen canvas and backing buffer
matching each target size. Portrait safe insets were 59px top and 34px bottom;
landscape insets were 59px on each side and 21px at the bottom. HUD/pause
controls stayed safe, and held-touch driving with Pause/Camera secondary taps
worked in both orientations. Existing keyboard/controller and frontend routes
also passed. The WebKit landscape and Chromium portrait captures appear in the
development log.

On the ordinary Caribou throttle-camera route, uphill lift was 0.2208329 radians:
rendered pitch changed from −0.3769404 to −0.0361075 radians. Cycling Camera
reset the lift. This is a default-chase browser adaptation and does not alter
the recovered camera simulation or establish original uphill-camera behavior.

The severe ATV Funny Crashes route completed 1,197 paired enhanced/Off WASM
steps with zero compared gameplay/camera mismatches. Three aerial frames kept
the ATV intact; two actual contact-damage transitions produced seven visible
parts, followed by intact recovery. Host gates passed 416 Rust tests across the
core/net/relay/web/tools/release suites, 100 JavaScript tests, eight build-cache
tests, 35 release tests (one opt-in skip), 19 Worker tests, `make verify`,
formatting, strict Clippy and the WASM build.

The standalone install-flow checks also passed in Chromium and WebKit for the
synthetic Home Screen title/menu, decorative fill, rotation, install prompts and
persistence. The motorcycle touch/crash route completed 302 paired enhanced/Off
WASM steps with zero mismatches across 251 observed frames; a mild crash recovered
without damaging-part contacts.

All results here are host-only automated checks with synthetic viewport/safe-area
signals. They do not prove physical iOS Safari/Home Screen behavior, actual
installation or public deployment. Funny Crashes remains an optional authored
presentation layer, not recovered ATV2 crash behavior; full-game parity is not
implied.
