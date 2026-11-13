# CPU riders in the mini-games and free roam (authored)

Issue #544. **Port adaptation, not recovered behavior.** The original
USA v2.00 game has CPU riders in races only: the mini-game events create
none and no CPU logic for them was found (research/GAMES.md, "No CPU
riders" and "CPU riders (authored)"), and Freestyle is ridden alone or in
split screen. Everything here is authored and off by default.

## Status

| Slice | Where it is offered | State |
| --- | --- | --- |
| Free roam | Options → Extras → Free Ride CPU riders: Off / 1 / 2 / 3 | this note |
| Tag | Games Prep → CPU Riders: Off / 1 / 2 / 3 | shipped in PR #551 |
| King of the Hills | Games Prep → CPU Riders | shipped in PR #551 |
| Treasure Hunt | Games Prep → CPU Riders | shipped in PR #551 |
| Hockey | Games Prep → CPU Riders | shipped in PR #551 |

The four mini-game brains (`crates/atv-core/src/game_cpu.rs`), their app
glue (`crates/atv-app/src/game_cpu.rs`) and
`tools/test_engine_game_cpus.mjs` are described in research/GAMES.md "CPU
riders (authored)". What that work left open is listed under "Left to do".

## One rider model for every mode

A CPU rider is a whole player ride (`game_cpu::Cpu`: its own `Riding`,
player 1's ATV in another paint), loaded like a split-screen player
without a view and driven through the player's controls. It is a contact
body for the humans and the other CPUs, has the room riders' engine and
impact voices, a name tag ("CPU N") and a map marker.

- **Vehicle control** is the race driver's: its steering law (the sine of
  5 × the bearing to the target, clamped) and its pedal deadband
  (`race_ai::pedals`), at the race driver's 50 ft/s off-line speed
  (`game_cpu::Driver`).
- **Recovery** is the race CPUs' stuck rule (under 3 ft/s while riding):
  back out twice, then turn in place, then the player's Reset.
- **Avoidance** of other riders, in free roam, is the race driver's peer
  correction (`cpu_avoidance::peers`, `00252080`) applied to the wanted
  velocity.
- Only the goal is written per mode.

With the option off no CPU ride is loaded: `Ready::cpus` is empty and
every CPU code path is skipped, so the ride is the one it was before.

## Free roam

**Option.** Options → Extras → "Free Ride CPU riders: Off (original) / 1
/ 2 / 3", saved as `atv-free-ride-cpus`. Extras is where the other
authored ride changes live (crashes, flips, train riding); the Freestyle
course picker is a grid with no rows of its own. The choice applies to
the next free ride, not the one in progress.

**Where it applies.** A plain free ride (Single Player → Freestyle, or a
ride link) alone or in split screen, up to four riders with the humans.
Never a race, a Freestyle objective, Training, the Waypoint Editor, a
mini-game (those have Games Prep's own row) or an online room.

**Records and scoring.** A plain free ride keeps no records and no score
that is saved: Hall of Fame and Freestyle records are kept by objective
runs, races and room Freestyle, none of which take these riders. A CPU's
stunts stay in its own ride and reach no HUD or standings. The CPUs are
contact bodies, so they can bump the player as a split-screen player can.

**Goal selection** (`crates/atv-core/src/roam_cpu.rs`, pure, seeded,
fixed-step, no allocation per step). A roamer keeps one plan at a time:

- **Point of interest** (most plans): ride to a hilltop some way off (150
  to 600 ft preferred, not one of its last three, weighted by a taste of
  its own so riders spread out). The venue's points are found once at
  load (`points_of_interest`): the start, then the most prominent dry
  hilltops on a 28 × 28 grid (a cell higher than its eight neighbours and
  at least 4 ft over their mean), 24 at most; a flat venue gets dry grid
  points instead.
- **Course line** (40 % of plans where the venue has one): join the
  venue's National race line (`<venue>nate.tdf`, read from the race
  archive through its directory, so only that member downloads) and
  follow it 500 to 1400 ft, aiming 60 ft ahead. Environment, National and
  Supercross venues have one; the Freeride-only venues do not.
- **Drift** (12 % of plans, and whenever nothing else is offered): 3 to 7
  seconds on a heading of its own. This and the per-plan pace (70 to
  100 % of the cruise speed) are the roamers' "mistakes": they are not
  efficient, and they are not meant to be.

A plan ends at its point, after its line distance, after 50 s, or after
8 s of being turned away from it.

**Hazards.** Every quarter second the roamer tests the ground 25, 50 and
80 ft along its heading. Ground counts as blocked when it is outside the
rideable area (40 ft inside the boundary launch margin of `0020aec0`: 110
cells, or the heightfield's edge on a stadium venue), off the terrain, or
under more than 2 ft of still water (the deep-water reset needs the ATV
3 ft under for 5 s, `001e5260`). A blocked heading turns to the smallest
of 26°, 52°, 77°, 103°, 132° or 180° to either side that is clear, keeping
the side it chose while it stays blocked, and rides at three quarters of
its pace.

**Facts and assumptions.** Fact: the steering, pedal, stuck and peer
rules are the ones the race CPUs already use, and the boundary and
deep-water thresholds are the recovered ones. Authored: every number in
the goal selection (shares, distances, probe distances, turn steps, the
2 ft wading depth, the 40 ft boundary inset), the choice of hilltops as
points of interest, and using the National race line for a free ride.

### Online rooms: left off

Room races can fill their field with CPUs (research/MULTIPLAYER.md "Room
race CPUs"), but those are race-path `CpuRacer`s inside the host's race
state: each peer draws the same roster from the race serial, the host
sends their motion in `CpuMotion` only within a started race, and guests
present them as race puppets. The riders here are whole local player
rides outside any race, with no roster on the guests and no message that
carries them, so that path cannot be reused as it stands. A room ride
therefore never loads them (`start_ride` clears the request for a room).

## Tests

- `atv-core` unit tests (`roam_cpu::tests`): hilltops found and wet ones
  left out; a roamer visits different points and repeats exactly with its
  seed; five seeds ride five minutes around a lake without entering it or
  leaving the map, with no reset; a roamer follows a circuit (over 90 % of
  its line time within 40 ft of it); it steers round a rider in its way.
- `atv-core` and `atv-ui`: the Extras row and its saved value.
- `tools/test_engine_free_cpus.mjs` (content group, host Chromium): see
  "Evidence".

## Evidence

Filled in below from the host run of `tools/test_engine_free_cpus.mjs`.

## Left to do

- **Mini-game checks not yet made** (PR #551 covered that each mode's
  CPUs pursue the objective): a test that Treasure Hunt, King of the Hills
  and Hockey with CPUs run to their results, an explicit option-off check
  for the games, and host frame times with CPUs in each game.
- **King of the Hills captures** depend on the hill's shape (a real
  airborne exit is needed); only "reaches a hill" is tested.
- A CPU's Stunt Score column does not count its injected hill points; no
  wheel dust for CPUs.
- **Online rooms**: no CPU riders in room games or room free rides.
- Free roam CPUs ride player 1's ATV; they have no roster of their own.
