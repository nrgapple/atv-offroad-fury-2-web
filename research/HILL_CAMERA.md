# Smooth hill chase camera

Strategy chosen 2026-10-13 for the reported lean-back camera jerk: replace
chassis-pitch-driven view changes with terrain look-ahead, and rotate the
presentation rig around a rider-height focus so the ATV stays in frame. Pull
back gradually on sustained steep climbs. Preserve the recovered Rust camera
frame, alternate presets, physics and wreck view.

Research: Unity's [Follow binding modes](https://docs.unity3d.com/Packages/com.unity.cinemachine@3.1/manual/CinemachineFollow.html)
separate yaw from target pitch/roll; [Third Person Follow](https://docs.unity3d.com/Packages/com.unity.cinemachine@3.1/manual/CinemachineThirdPersonFollow.html)
documents an independent tracking target, pivot placement, per-axis damping
and camera clearance. Unreal's [spring arm](https://dev.epicgames.com/documentation/unreal-engine/API/Runtime/Engine/USpringArmComponent)
separates pitch inheritance, rotation lag and collision. These support a stable
tracking rig; the terrain sampling, thresholds and tuning here are authored
choices, not recovered original ATV2 behavior or copied engine code.

Use the horizontal right axis to derive heading without following wheelie pitch.
Sample ground heights across short, medium and long forward spans; use the
median grade to reject a single abrupt sample. Damp climb anticipation and its
release with time-based exponential smoothing. A brief airborne interval holds
the takeoff view, then settles without tracking flips. Reverse settles smoothly;
reset, alternate camera and wreck switches clear the adaptation. Terrain clearance
still applies after the presentation transform. Flat terrain must ignore rider
lean and vehicle pitch completely.

Validation gates: synthetic lean/slope/crest/noise/frame-rate and rig-framing
regressions; actual Caribou keyboard climbs and lean-back in desktop/portrait;
viewport rotation, preset cycling, normal frontend checks; workspace tests,
formatting/Clippy, setup and WASM build; validated-main CI and public HTTPS checks.
This corrects the existing requested browser chase adaptation. Complete original
camera parity, object-aware follow-camera clearance and physical-phone validation
remain open.


## Host checkpoint (2026-10-13)

Flat terrain no longer reacts to wheelie/body pitch; synthetic pitch checks span
0–150 degrees including both sides of vertical. The three median terrain probes
use 8/20/36 feet, with smooth onset from .10 radians of grade. Orbit reaches at
most .52 radians; pullback reaches at most 24% beyond existing viewport framing.
Response time constants are .45 seconds entering and .80 seconds leaving; short
airborne gaps hold for .35 seconds. The entire rig rotates about origin +3 feet
rather than rotating only the sightline away from the vehicle. Rendered terrain
clearance takes precedence over requested orbit. The original Rust camera and
other presets/wreck view remain unchanged.

Host Chromium Metal Caribou climbing passed 390×844 and 1280×720 views, then
2.5 seconds of keyboard lean-back under throttle, release and camera cycling.
The portrait checkpoint measured .20014283 radians of requested orbit and 9.237%
extra pullback; desktop measured .20581202 radians and 9.499% extra pullback.
The traces contain 710/721 samples and 489/497 uninterrupted Riding transitions;
all checked orbit changes stay within the exponential-response bound. Maximum
observed orbit steps were .02396581/.01564768 radians. These are actual normal
input routes, not injected vehicle states; resets/wreck transitions are excluded
from the smooth Riding bound. Both inspected captures keep the ATV visible with
the uphill terrain ahead. No before/after physical-device measurement is claimed.

Eight render-support unit tests cover flat lean/wheelies, terrain/pitch independence,
one-sample noise rejection, downhill, crest/contact gaps, airborne flips, reverse,
reset/preset/wreck, 30/60/120Hz response and framing/terrain clearance. make check
passes 416 Rust tests, 116 JavaScript tests and eight build-cache tests; make verify,
WASM build, 35 release tests (one opt-in skip), 19 Worker checks and repository
hygiene pass. Portrait rotation and idle/focus/context-restoration browser gates
also pass. Default headless Chromium initially lacked WebGL; these driving results
are from explicit Metal reruns.

This is host evidence pending validated-main CI/deployment and public checks.
Physical phones, isolated prop-ramp anticipation and object-aware follow-camera
clearance remain unverified/open. Terrain look-ahead intentionally replaces the
old chassis-pitch fallback; a tilted prop on otherwise flat terrain does not drive
this adjustment. It is an authored adaptation, not recovered original camera parity.
