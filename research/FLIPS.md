# Optional ATV flips (Extras)

**Status: authored enhancement, off by default.** ATV Offroad Fury 2 (USA v2.00)
has no flip tricks. Its recovered airborne wreck angles (ATV01: 85° up, 70° down,
80° roll) eject the rider long before a full rotation ([WRECK.md](WRECK.md)).
Extras → "Flips: Unleashed" adds the backflip and left/right three-sixty that
MX vs. ATV Unleashed gives its ATVs. "Flips: Off (original)" never constructs the
flip state, so the recovered ATV2 step is unchanged.

## Source evidence (MX vs. ATV Unleashed, SLUS_211.04)

- `Rider/Anim/ATV_Rider_Stunts.ini`: non-mappable special stunts Backflip
  (Stunt42, PointValue 2000, HoldPointValue 10000) and Three Sixty Left/Right
  (Stunt45/46, 2000 / 15000). The file has no front flip, and neither do the
  executables or any tuning section.
- All eight `atv_*.phy` and `atv_s_*.phy` files share the bikes' section layout and
  `[Backflip]` / `[Three Sixty]` values: backflip torque 800–1400, decrease 2000;
  three-sixty X 600–1100, Y 200–1100, decrease 1960; hold window 0–60 frames.
  `AirborneDamping` is 450.
- The gesture, torque and termination rules come from the recovered backflip
  004e4f20 and three-sixty 004e53d0 (`motorcycle_drive.rs`). The airborne
  pitch/roll wreck checks are skipped while a trick is active (004e24a0).
- That ATVs run the same routine as the bikes is inferred from identical keys and
  layout. Their dispatch class was not traced.

## Port (`crates/atv-core/src/flip_tricks.rs`)

- **Ramp gesture:** on a ramp face (grounded, rising > 5 ft/s, forward > 30 ft/s,
  no brake), hold L1 alone and push forward, then pull back. Hold time sets the
  torque. The three-sixty uses L1 plus a side-to-side flick.
- **Early-air request:** L1 plus pull-back or a side flick within 0.6 s of takeoff
  starts the trick at minimum strength. This is the same authored adaptation the
  motorcycle uses. Keyboard: L + E, or L + A/D.
- **Touch (authored):** flips come from the driving thumb, not the trick wheel.
  In the default one-handed mode, hold the thumb for throttle into a ramp, pull
  it straight back, left or right (within 30°) past the driving range, hold a
  beat, and lift. The flip zone starts 105 px from where the thumb landed,
  beyond full lean (70 px); sideways it is also at least 1.3× the full-lock
  distance, which grows at low steering sensitivity. Ordinary full steering
  or leaning therefore never flips. The thumb must stay in the zone for
  220 ms before a lift counts, so sliding into it while lifting does nothing;
  a thumb that rolls back out within 100 ms of lifting keeps the armed flip.
  While a lift could flip, faint gold arcs outside the compass rim mark the
  three zones; the entered arc fills over the 220 ms, then the cue turns gold
  and supporting browsers vibrate briefly. The lift is also the preload jump. The request (`flip_request` with
  `flip_launch`) stays armed on the ground for 0.6 s, so it fires at takeoff.
  Lifting in the air counts only within the first 0.6 s, so ordinary releases
  later in a jump never flip. How long the pull was held before lifting sets
  the strength, like the source hold channel: one second gives maximum torque.
  The thumb cue turns gold, "LIFT → BACKFLIP" or "LIFT → 360 LEFT/RIGHT", only
  when lifting would start a flip (preload armed, or the early air window
  reported by Rust). Two-finger mode reads the left pad with the same zone,
  dwell and grace when the right drive thumb lifts on the ground. A long straight swipe (120 px or more)
  down, left or right with the right thumb in the air also flips there. While
  spinning, leaning forward slows a backflip, and steering against a 360 slows
  it.
- **Torque scaling:** rotation is damping-dominated in both games. ATV01 pitch
  inertia is about 44 slug·ft² against `AeroDamping` −800, a time constant near
  0.05 s. Torques are therefore scaled by |AeroDamping| / 450, which preserves
  Unleashed's rotation rates (backflip about 2.2–3.1 rad/s, plus ATV2 lean
  ±1 rad/s).
- **Wreck checks:** only the airborne angle limits are skipped, and only while a
  trick is active with no wheel/body contact. Touchdown ends the trick, so ground
  angles, impacts and the original stunt-landing gate still decide the landing.
- **Scoring:** a clean landing credits 2000 per completed rotation (within 60° of
  a full turn) and shows "Backflip", "Double Backflip", etc. on the HUD. A wreck
  shows the failure pulse. The meaning of HoldPointValue is not recovered.
  Points are display-only and do not enter the recovered ATV2 chain score.
- **Camera:** while a trick is active the follow camera keeps the takeoff
  heading. The flattened body forward reverses past vertical, which otherwise
  swings the camera into the terrain.

## Verification and limits

- Rust tests cover the gesture gates, button exclusivity, bleed-out, rotation
  counting, bail, and the vehicle-level rates. Flips off settles at
  AeroMoment/AeroDamping; flips on adds about 1000/450 rad/s. Flips on with no
  trick still ejects an upside-down rider.
- `tools/test_atv_flips.mjs` (content group) uses real keys on env01. With Flips
  on, the ATV rotated to about 240° while still riding and the HUD showed
  "Backflip". With Flips off there is no flip state and the air pitch rate stays
  at the original 1.00 rad/s.
- The env01 starting preload hop gives about 1.6 s of air, which is not enough for
  a full loop, so that run bails on landing. A full backflip needs about 2 s of
  air (big freestyle jumps). A clean landed flip in the browser is not yet
  demonstrated.
- Touch: `web/touch.test.mjs` covers:
  - the flip zone beyond full lean and full lock, sensitivity, cone and Flips off;
  - dwell (no flip when sliding in while lifting) and the lift grace;
  - single delivery and hold strength;
  - cancellation;
  - cue gating and the two-finger pad.
  `tools/test_atv_flips.mjs` uses real Chromium touches on env01: hold the thumb
  to the Caribou ramp, the gold cue appears, pull back and lift, and a backflip
  launches past the air limit while riding (about 150° on that short hop).
  Physical phones are not yet tested.
- Not included: Unleashed rider/vehicle flip animations (the rider keeps the
  ATV2 air pose), recovery-time yaw suppression, CPU flips,
  multiplayer score sync, and front flips (Unleashed has none).
