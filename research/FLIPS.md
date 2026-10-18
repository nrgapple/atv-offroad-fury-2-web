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

## Arcade flips (alpha)

**Status: authored for this port, off by default.** No source game has it: ATV2
has no flips, and Unleashed only has the canned backflip/three-sixty above (no
front flip). It lives in `crates/atv-core/src/arcade_flips.rs`, behind the input
flag `arcade_flips`, switched by Settings → Alpha → "Arcade flips"
(`arcade-flips` in `localStorage['atv-alpha']`, see [ALPHA.md](ALPHA.md)).
Arcade implies flips whatever the Extras row says and replaces the Unleashed
gestures; with it off, Flips Off and Flips Unleashed run exactly the code above.

### Controls

The riding stick sets the air rotation rate directly; no trick button.

| Input | Back/front flip (pitch) | 360 (yaw) | Lip pop |
| --- | --- | --- | --- |
| Keyboard | E/Shift lean back, Q lean forward | A/D | release Space at the lip |
| Controller | left stick down / up | left stick left / right | the original lean flick (back, then forward) at the lip |
| Touch, one-handed | touch again in the air, drag down / up from where the thumb lands (70 px = full) | drag left / right past full lock (see below) | lift the drive thumb at the lip |
| Touch, two-finger | left pad down / up | left pad left / right past full lock | lift the right drive thumb at the lip |

- Hold to keep spinning, let go to bleed it out (time constant 0.25 s, about a
  fifth of a turn from full rate), push the other way to brake and reverse.
- Pitch and yaw together turn about one tilted axis: a cork.
- Rotation engages once the stick passes 0.25 in the air and stays engaged until
  touchdown. Smaller inputs leave the original ATV2 air control untouched.
- A held original stunt modifier (J/K/L, Circle/Triangle/L1, a trick-wheel
  pick) gives the stick back to that stunt; the commanded rate bleeds out.
- One-handed touch: lifting the drive thumb is the preload jump, so rotation
  needs a second touch in the air (the drive thumb's displacement from where it
  lands is the stick). The thumb flip zones, gold arcs and "LIFT →" cues are
  off in Arcade; two-finger flip swipes are off too.
- Touch side lean: a thumb is rarely centred, so a sideways drag within the
  steering range stays the original ATV2 air lean (yaw/roll moment) and does
  not engage Arcade, which lets a flip be lined up for the landing. The 360
  axis (`arcade_spin`) starts at the Unleashed 360 reach, 105 px or 1.3× full
  lock, and is full 50 px further out. While airborne the compass shows only
  the left/right gold arcs, which fill with the spin rate; the cue reads
  "LEAN · FLIP" or "SPIN ← 360" / "SPIN 360 →". Keys and controllers still
  spin with steering.

### Model and tuning (`arcade_flips.rs`)

- A commanded pitch/yaw rate moves toward `stick × max` at `SPIN_UP` 14 rad/s²
  (`BRAKE` 30 rad/s² when the stick opposes the rotation or eases off) and
  decays with `RELEASE_TAU` 0.25 s when released. Full stick is `PITCH_RATE`
  5.5 rad/s and `YAW_RATE` 6 rad/s; past a 0.15 dead zone the response is
  |u|^1.5 so analog sticks get fine control.
- The body tracks that rate: torque = |AeroDamping| × (rate + `TRACK_GAIN` ×
  (rate − ω)), `TRACK_GAIN` 1. The feed-forward cancels the original damping at
  the commanded rate; the original lean moment still adds a little. With
  ATV01 (AeroDamping −800, inertia about 44 on every axis, 1/60 s
  Adams–Bashforth steps) the loop is well inside the integrator's stability
  limit.
- **Lip pop:** the recovered preload pushes for 0.25 s. If that impulse is
  still active at takeoff, quality is 1 up to `POP_PERFECT` 0.05 s after it
  fired, falling linearly to 0 at `POP_WINDOW` 0.25 s. A perfect pop adds
  `POP_LIFT` 4 ft/s takeoff speed and `POP_AUTHORITY` +25% rate and spin-up for
  that jump. Releasing late does nothing: the original preload cannot fire in
  the air. Measured on env01's Caribou ramp (keyboard, 20 steps after the
  preload armed), takeoff came 4 frames after the release, which the port
  reads as 0.10 s and quality 0.75. Releasing at once lifted off 22 frames later
  (no pop) with much less air; the original preload already rewards timing.
- **Wrecks:** only the airborne pitch/roll limits are skipped while the rotation
  is engaged with no wheel or body contact. Touchdown ends it before the
  recovered ground-angle, impact and stunt-landing gates run. There is no
  landing assistance and no rotation snapping, so over-rotation wrecks.
- **Names (display only):** at touchdown the rotation about the combined
  pitch/yaw axis is counted in turns (each within 60° of a full turn).
  Three-quarters or more nose-over is a flip ("Backflip", "Front Flip",
  "Double Backflip"), three-quarters or more spin is "360", "720", …, and the
  rest is a cork ("Cork 360", "Cork 720", "Front Cork 360"). The name shows
  only after the landing survives 0.25 s; a wreck in that window, or while
  spinning, shows the failure pulse. Points use the Unleashed 2000 per turn as
  above and stay out of the recovered ATV2 score.
- The follow camera holds the takeoff heading while the rotation is engaged,
  as for Unleashed tricks.

### Verification

- `cargo test -p atv-core` covers rate tracking, release bleed, braking, front
  flips, spins, corks, analog scaling, the 0.25 engage threshold, stunt
  modifiers, names, landing confirmation and bails, the lip pop (quality, lift
  paid once, authority), a vehicle-level front flip past the air limits, and
  that switching Arcade on and off leaves Unleashed identical to a fresh
  Unleashed vehicle.
- `tools/test_arcade_flips.mjs` (content group) on env01 with real key and
  touch events: held lean-back backflip and held lean-forward front flip, let
  go at about 275°, landed at about 370–385° with the HUD showing "Backflip" /
  "Front Flip" (lip pop 0.67–0.75); holding lean-back through touchdown
  (about 545°) wrecks; one-handed touch (lift for the jump, touch again and
  pull down) landed a backflip; a full-lock sideways drag in the air stayed
  disengaged, and dragging past full lock spun the ATV. `ATV_ARCADE_SCREENSHOT=<prefix>` saves
  mid-flip frames. `tools/test_atv_flips.mjs` still passes unchanged.
- A cork ("Cork 360") and a 360 also landed on that ramp in manual runs.

### Limits

- Authored feel; tuning was checked on env01's Caribou ramp only (about 1.6 s of
  air), not on big freestyle jumps, so multiple rotations are verified only in
  Rust tests. Physical phones and controllers are not yet tested.
- Names come from body-local rate integrals, so sequential rotations (a flip
  then a spin) also count as a cork. Roll is not counted.
- Controller pop uses the original lean flick, so a stick still pushed forward
  at takeoff starts a front flip; let the stick return to centre after the flick.
- The rider keeps the ATV2 air pose; CPU riders and multiplayer peers do not
  flip.
