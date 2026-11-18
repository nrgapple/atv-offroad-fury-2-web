# Motorcycle handling, rider and tricks (authored, 2026-11-18)

**Status: authored gameplay, the default for every motorcycle.** The ground
handling, the airborne assists, the trick pacing, the rider's body movement and
the thrown rider described here are written for this port. They are not
recovered from MX vs. ATV Unleashed. Suspension, engine and clutch, the
wheelie/stoppie controllers, preload, chassis collision and the wreck limits
stay on the recovered controller (research/MOTORCYCLES.md). ATVs are unchanged.

The recovered ground model remains in the code as `Motorcycle::set_classic`
for parity research and for the probe below. No menu selects it.

## Why

Owner feedback after PR #746: the bikes slid around, did not look like a bike
being ridden, the crashed rider stood in place, and tricks felt broken.

Measured on the recovered model (RM125 Race, `atv-tools bike-probe`):

- The source side force is a damper on sideways speed with no friction limit
  (`SideForceScale × SideForceSpeedScale × load` per ft/s, peaking near
  5.4 × load). A turn is made by sliding: 13° of sideslip at 30 mph, 25° at
  45 mph and 45° at 60 mph on full stick, with lateral peaks of 4 to 8 g.
- The bank is steer × a speed ramp, unrelated to the turn being made.
- On ENV01 the same turns reached 60 to 80° of sideslip, fell to 8 to 14 mph
  and wrecked once; the 50cc wrecked in plain turns.
- The fixed flip and spin torques turn at 2.2 to 3.1 rad/s, two to three
  seconds a rotation. Trick clips run 1.3 to 1.8 s and wreck if the bike lands
  before their SafeTime (1.1 to 1.5 s). Few ATV2 jumps last that long.
- A wrecked rider was one rigid upright body playing a flail clip, clamped to
  the terrain: he stood where he landed.

## Ground handling (`motorcycle_handling.rs`)

How shipped games do it was checked first: MX Simulator's steering sets a
target lean and separate helper forces hold it
([FAQ](https://www.mxsimulator.com/faq.html)); GTA exposes a maximum bank
angle, a separate rider lean and in-air steering/levelling multipliers
([Handling.meta](https://gtamods.com/wiki/Handling.meta)); an open-source
controller requests a lean capped at `atan(μ)` and found that commanding lean
faster than yaw builds pushes the bike sideways into a spin
([PR](https://github.com/Noisemaker111/jgengine/pull/1787)). The balanced lean
is `atan(v²/(g·r))`
([bicycle and motorcycle dynamics](https://en.wikipedia.org/wiki/Bicycle_and_motorcycle_dynamics)).

The model here:

1. **The stick asks for a lean.** It is filtered (critically damped, 9.5 rad/s
   slow to 6.5 rad/s fast) into a command. Full command is the tightest turn
   available: the steering lock when slow (37.5°, the source 30° × 1.25), 90%
   of the tire grip when fast.
2. **The front wheel is steered to deliver it**, in the ground plane:
   `δ = atan(a·L/v²)`. At 45 mph that is about 3°; the bars barely move, as on
   a real bike.
3. **Each tire removes its own sideways motion** with an impulse bounded by
   its grip × load. Both wheels are solved together (four passes of 55% each)
   from the velocities the substep's other forces will leave, including the
   pitch rate, which moves the contacts sideways on a leaned bike. Sideways is
   measured on the level, not in the plane of the ground under the wheel, so a
   wheel bouncing on a bump that slopes sideways is not treated as sliding.
   A tire only swings the bike round when the other one is down too; alone it
   holds the line. Under the limit the bike tracks where it points. Asked for
   more, it runs wide instead of turning harder.
4. **Grip** is 1.7 g at 30 ft/s easing to 1.3 g at 110 ft/s on standard dirt
   (Sport 1.9/1.45, Simulation 1.2/1.05), 60% of that below 8 ft/s. It is
   scaled by the map surface: the upper half of the range follows the ATV2
   `SurfaceFriction` (0.1 ice gives 0.5), and MX maps use the ratio of the
   source side-force scales per zone. These are game values. Measured dry-earth
   friction is about 0.68 ([Wong, via
   hpwizard](https://hpwizard.com/tire-friction-coefficient.html)); the game
   corners at about twice that so ATV2 courses stay rideable at speed.
5. **Slides are deliberate.** A locked rear (braking) keeps 62% of its side
   grip and steps out; full drive or braking costs a fifth of the side grip
   (nearly all of it in Simulation). The front steers into the slide, the
   rider holds the turn rate he asked for, and a rear that has come round
   digs in (to 1.4 × grip by 24° of slip) while the front pushes. A slide
   settles at an angle. It cannot spin the bike, whatever the surface or the
   throttle.
6. **The bank is the one that balances the turn**: `atan(k·a/g)` (k 0.74 by
   default so 1.7 g shows about 50°; 1 in Simulation). It is as far as the
   rider asked and no further, so a path bent by a landing, a scrape or a
   cambered slope does not lean the bike on its own; and no more than 0.3 g
   ahead of the path actually ridden, so a bike that is running wide stands
   up. The bike leans about the line its tires stand on, so the wheels stay
   planted and the centre of mass moves in and down. The lean is held about
   the level forward axis: pitched up in a wheelie, the torque that would tip
   a leaned bike over would otherwise turn it.
7. **With the front tire up or only skimming the ground** it cannot steer,
   and on the rear wheel the body-frame yaw axis points backwards. By the
   share the front has unloaded, the bike is turned about the vertical through
   the rear contact at 60% of the rate the lean asks for and leans 45% as far.
   The wheelie and stoppie controllers pitch about the level axis across the
   bike (the source uses the bike's own, which on a leaned bike also turns
   it). The 50cc wheelie command (95° in the source, which loops it) is held
   to 50°.
8. **After a landing** side grip is halved for 0.3 s and the heading swings to
   the travel, instead of the tires snatching the travel round to the heading.

## In the air

- **Whip.** The stick lays the bike over (to 22°) and swings its tail (to
  21°) as angles that come back when it is released, growing in over the first
  0.4 s of air. The source turns at a rate for as long as the stick is held,
  which lands a held stick past the 44° crossup wreck.
- **Pitch.** The bike follows the arc of its jump and, over the last half
  second, matches the slope it will land on, a little rear wheel first. The
  stick trims that by up to 31° either way. The source only damps pitch and
  turns at a rate while the stick is held: every jump lands nose high, and a
  stick still held back from a wheelie loops the bike over. The brake still
  drops the nose.
- **Landing prediction.** While airborne the ballistic path is stepped against
  the terrain thirty times a second for up to four seconds. Tricks and
  rotations use the time; the pitch assist uses the slope.

## Tricks

- **Flips and spins** turn at the rate that completes them before the
  predicted landing (3 to 6.6 rad/s for a flip, 3.2 to 7.2 for a spin), easing
  to a stop. A backflip's last quarter is measured against the landing
  attitude, so it comes round to the ground and not to the angle of the ramp.
  Holding the input with more than 1.4 s of air left goes round again. A jump
  too short for the quick rate still lands badly.
- **Stunt clips** play at 1.5× and faster when the air is short (to 4×), so a
  tap on a short jump is a quick trick and a released hold is back in the seat
  before the predicted landing. They need 0.12 s of air and 0.45 s to the
  ground to start. Landing wrecks only when the rider is still well out of the
  seat: holding a trick to the ground still wrecks, a trick that had barely
  started or was most of the way back does not.
- Clips ease in and out over the riding pose (about 0.15 s) with the hands and
  feet released by the same share, instead of cutting.

## Rider

- The riding clips sweep with the lean (the source sweeps them with the crab
  angle, which this handling no longer has in a corner); a real slide adds to
  it. The inside foot leaves its peg in a hard corner or a slide.
- Motocross form: stand on the pegs on a straight and in the air, sit forward
  and to the outside in a corner, come down into bent knees as the suspension
  compresses, move back under braking. The spine and head stay nearer upright
  than the bike (60% of the lean is taken out through the torso and head) and
  the head turns into the turn. Hands and feet are solved to the grips and
  pegs after all of it.
- **The thrown rider** is the existing articulated body
  (`ragdoll.rs`, previously only in the Crazy/Gorn crash options), now part of
  the motorcycle itself: each limb keeps the motion it had in the riding pose,
  the body keeps most of the speed the bike had before whatever stopped it,
  and the kind of wreck sets which way he leaves. He tumbles and comes to
  rest on the terrain. The camera, sound and triggers follow his pelvis, other
  players receive the same pose, and the respawn waits for him to stop
  tumbling (6 s at most). He does not collide with scenery or the bike.

## Measurements

`atv-tools bike-probe` on a synthetic course (level ground, whoops, a 9 ft
kicker with a landing), RM125 Race, default handling. "Before" is the same
build with `BIKE_PROBE_CLASSIC=1`.

| Scenario | Before | After |
| --- | --- | --- |
| Full stick at 30 mph: peak sideslip, steady lateral | 13°, 2.3 g, falls to 24 mph | 1.6°, 1.5 g, holds speed |
| Full stick at 45 mph | 25°, peaks 5.8 g | 0.9°, 1.4 g |
| Full stick at 60 mph | 45°, peaks 7.7 g | 0.3°, 1.3 g |
| Hard left then hard right at 40 mph | 26° | 0.8° |
| Slalom at 40 mph | 23°, falls to 26 mph | 0.8°, holds speed |
| Brake hard while turning in at 45 mph | 68° | 6°, drives out |
| Hold full steer through a 0.7 s jump | lands 43° sideways | lands 3° |
| Straight over rough ground at 45 mph | 1.5° | 0.8° |
| Full stick over rough ground at 30 mph | 18°, falls to 24 mph | 7° peak, holds speed |
| Steer while pulling a wheelie at 65 mph | does not lift | lifts to 45°, turns, no sideslip |
| Tap a trick on a 0.7 s jump | wreck | completes |
| Backflip, 1.3 s of air | wreck | lands |
| Three-sixty, 1.3 s of air | wreck | lands |

The 50cc and 500cc match the 125 in the cornering rows. On ENV01 terrain with
its own surface table the full-stick turns peak at 3 to 14° (landings while
steering) against 60 to 80° before, keep their speed, and twelve scenarios
including wheelies over its bumps and a 30 s flat-out weave end without a
wreck. From the ENV01 start line (`crates/atv-game/tests/bike_feel.rs`) mean
sideslip through 3 s turns is 1 to 8° and the bank peaks at 29 to 50° across
the 50, 125 and 500; the recovered model measured 3 to 22° and banks to 81°.

## Limits

- Feel is tuned from telemetry and screenshots, not from a controller in hand.
- The side grip is a game value and one number per speed; there is no camber
  thrust, tire relaxation or load sensitivity.
- Pitch, the suspension and whoops behaviour are the recovered controller's;
  the bike still pitches about 20° through whoops at 35 mph.
- The thrown rider collides with terrain only.
- Bike records and ghosts set on the earlier handling were set on a slower
  cornering model.
- CPU racers do not ride motorcycles.
