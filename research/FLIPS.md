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
- **Touch (authored):** in the default one-handed mode, a second finger in the
  air opens the trick wheel. With Flips on, gold **Backflip** (down), **360 Left**
  and **360 Right** labels sit outside the trick names, past a dashed ring at
  132 px. A short drag still picks the named tricks; dragging past the ring and
  lifting picks the flip, delivered once as `flip_request`. In two-finger mode, a
  long straight swipe (120 px or more) down, left or right flips; short swipes
  and up keep the original tricks. A touch request starts the flip at the
  early-air strength at any point in the air, because a menu pick cannot meet the
  0.6 s takeoff window. While spinning, leaning forward with the steering finger
  slows a backflip, and steering against a 360 slows it.
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
- Touch: `web/touch.test.mjs` covers the flip zones, single release delivery,
  Flips off, and two-finger long swipes. `tools/test_trick_wheel.mjs` checks the
  labels fit and select from center and edge presses at 390×844, 844×390,
  320×568 and 667×375 in Chromium and in WebKit (WebKit uses synthetic pointer
  events). `tools/test_atv_flips.mjs` uses real Chromium touches on env01:
  wheel → past the ring → Backflip starts. Physical phones are not yet tested.
- Not included: Unleashed rider/vehicle flip animations (the rider keeps the
  ATV2 air pose), recovery-time yaw suppression, CPU flips,
  multiplayer score sync, and front flips (Unleashed has none).
