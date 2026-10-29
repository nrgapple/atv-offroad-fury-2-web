# Racing championships

The original's single-player Championship mode runs a series of races per
event type, with championship points, an advance threshold per round,
trophies, Profile Point rewards and unlocks. This note records the recovered
rules and what the port implements. Profile Points per race are in
`PROFILE.md`.

## Exact-build evidence

Private exports of the exact USA v2.00 `SCUS_972.11` (bounded reading; no
runtime traces). "Inferred" marks readings not confirmed by execution.

- **Series.** A championship is a race event type (race settings +8). Course
  lists come from the front end's `Tracks.dat`, one list per type in file
  order (`0021cca0`). A table read by `00322c20` gives the round count on
  Normal and Expert (Expert plays one more course in most series).

  | Type | Series | Pro | Rounds N/E | Laps | Courses (first rounds) |
  | --- | --- | --- | --- | --- | --- |
  | 0 | Amateur Enduro | no | 5/6 | 3 | env05, env02, env08, env06, env04, env01 |
  | 1 | Pro Enduro | yes | 5/6 | 3 | same scenes, long list |
  | 2 | Pro Freestyle | yes | 5/5 | 120 s | free01, free04, free02, free06, free03 |
  | 3 | Amateur Freestyle | no | 6/6 | 120 s | env05, env02, env08, env06, env04, env01 |
  | 4 | Pro Nationals | yes | 6/7 | 5 | nat01 … nat07 |
  | 5 | Amateur Nationals | no | 5/6 | 5 | env05, env02, env08, env06, env04, env01 |
  | 6 | Amateur Supercross | no | 6/7 | 5 | sx08, sx12, sx01, sx13, sx02, sx06, sx05 |
  | 7 | Pro Supercross | yes | 6/7 | 5 | sx03, sx07, sx16, sx04, sx10, sx15, sx14 |
  | 8 | Short Tracks | no | 3/3 | 5 | env06, env01, env07 |

- **Setup (`00289d50`).** Sets the championship flag (+0x36d), the event
  type, laps (3 Enduro, 5 otherwise; Freestyle a 120 s limit), the CPU
  difficulty (+0x610: 3 on Expert, else 2) and the course index to the
  rounds completed (inferred: round N races list entry N). Five riders race:
  the player and four CPUs. Am or Pro is only the chosen event type.
- **Points (`001f1db0`).** 7, 4, 2, 1 and 0 championship points for places
  1–5, the same for every race class. The course TOY Am/Pro × Normal/Hard
  tables are Profile Points (`PROFILE.md`), not championship points.
- **Advancing (`00322c60`, `00322ed0`).** After round r (0-based) the
  player's total must reach 4r + 2 (2, 6, 10, 14, 18, 22). The final round
  always passes; Freestyle always passes. Short of it, the race's points are
  not kept and the round is replayed ("You have not earned enough
  Championship Points to advance to the next course."). Otherwise every
  rider's points are added and the round counter advances (`003222b0`).
- **Trophy (`00322440`).** After the last round the player's rank among the
  five totals (only strictly higher totals rank above; ties go to the
  player) gives gold, silver, bronze or none.
- **Rewards (`00322650`).** Profile Points from a per-series table, paid
  only the first time that trophy is won: Amateur racing 500/250/125 on
  Normal and 1500/750/375 on Expert; Pro racing 1000/500/250 and
  2000/1000/500; Amateur Freestyle 1000/500/250, Pro Freestyle
  2000/1000/500; Short Tracks 0.
- **Unlocks (`003238a0`, Normal only).** Gold opens the series' Expert; any
  trophy in an Amateur series opens the matching Pro series. A new profile
  (`00320db0`) has Amateur Enduro, Nationals and Supercross open on Normal
  and Amateur Freestyle on both difficulties.
- **Loading screen (`002591f0`).** Shows the round, current points, points
  required and the place required to advance.

Short Tracks opens only with the GOLDCUPS or IGIVEUP cheat (`0033eca0`
calls `003238a0` for every type; nothing else found opens type 8, and the
manual counts 14 championships without it; `PROFILE_STORE.md`).

Open: the course
unlock side effects for other modes (`00323640`), runtime confirmation of the
round-to-course mapping, and whether a lesser trophy after a better one pays.

Championships are single player: only Single Player opens the
Championship page; Multiplayer → Split Screen offers Race, Freestyle
(with Objectives) and Games (SPLIT_SCREEN.md "Split objectives and
championships").

## Port

- `atv_core::championship`: the six racing series whose courses the port
  loads (Amateur and Pro Enduro, Amateur and Pro Nationals, Amateur and Pro
  Supercross, and Short Tracks once a cheat opens it), their
  courses, rounds, laps and rewards; championship points, the advance rule,
  trophies, first-trophy rewards and unlocks. Progress per series and
  difficulty lives in the saved profile (`fury-profile`, `PROFILE.md`).
- Single Player → Championship lists each series with its round, points and
  best trophy (or Locked), and a Normal/Expert row; under the rows the
  highlighted series' next course and the five standings. Choosing a series starts
  its next round: five laps, four CPUs, Expert CPUs on Expert. The race
  results add a championship line (points and total, the shortfall and a
  replay prompt, or the trophy, reward and unlocks), and Profile Points use
  the course table for the series and difficulty. Like the original
  ChampionshipRaceOver menu, an advanced round offers Continue Championship
  (the next round's course) and Quit to Championship Menu; a short round
  offers Restart; a finished series only Quit to Championship Menu.
- The loading screen of a round shows the original briefing (`002591f0`):
  the series, "You Are Currently On Track: N of M", the current points, the
  points required and the place required to advance (`00322d80`: 0 missing
  is 5th, 1 is 4th, 2 is 3rd, 3–4 is 2nd, 5–7 is 1st, 8 or more and the
  final round are N/A), above the usual tip.

**Authored or approximate:** a better trophy than the best so far pays its own
reward once; the standings keep four CPU slots whose riders are drawn for
each race, as the original's race creation draws its roster (`00210d20`,
`0021f680`) for every race (a reading, not traced across rounds);
Amateur Nationals races use the port's Environment race courses (the National
mode files); there is no memory-card save or trophy page. Short Tracks (env06st, env01st, env07st, 3 rounds, no reward)
is listed Locked until GOLDCUPS/IGIVEUP. Advancing a round and a gold or
silver trophy unlock courses for Single Race and Freestyle
(`PROFILE_STORE.md`).

**Enduro (2026-10-27).** Amateur Enduro races each venue's short Enduro and
Pro Enduro its long one (`002f65e0` maps the event type to the track
variant, `0021d8b0` builds `<venue>ends|endl`), three laps, with the gate
rules in `RACING.md` "Enduro and Short Track races". Their course lists are
the Environment order above, so round N races `env05ends`, `env02ends`, …
(Pro: `env05endl`, …). Points, advancing, trophies, rewards (Amateur
500/250/125, Pro 1000/500/250 on Normal) and the Amateur → Pro unlock are
the same rules; the course Profile Points come from the Enduro mode TOY's
[ChampionshipInfo]. A new profile has Amateur Enduro open on Normal.
`tools/test_engine_championship_enduro.mjs` races the final Amateur Enduro
round (env04ends) from the menu and checks the trophy, reward and the
Pro Enduro unlock.

Checks: `atv-core` championship tests (lists, short round replay, final
round trophy/reward/unlocks, tie rule); `atv-ui` controller test for the
page and the results choices; `tools/test_engine_championship.mjs` races
Amateur Supercross rounds 5 and 6 from the menu through Continue
Championship and checks the saved profile and the page;
`tools/test_engine_championship_loading.mjs` checks the loading briefing.

## Freestyle objectives (2026-10-27)

Exact-build evidence (private exports; the shipped `<scene>.obj` files of
each course's OB archive, `TRACKS/*/<x>_OB.RES`; bounded reading, no
runtime traces):

- **Objectives.** `[Objectives] NumObjectives`, `[Objective%d] Type`
  (`002fca60`: `CherryPick`, `Stunt`, `ComboMambo`). Every championship
  course ships four: Hit Every Green Target and Hit Every Red Target
  (CherryPick: `NumberOfTargets`, `BonusPointsPerTarget` 500/1000,
  `BonusTimePerTargetInSec` 10/5, a trigger TOY `<scene>gcp|rcp` of
  one-time 5-unit spheres; `002fa840`), Bust 10 Different Combos
  (`CombosToWin`; `002fbe30`, `002fc050`) and Score Stunt Points
  (`NumPointLevels` 3, `Points%d`; 10k/25k/50k, St. Croix Sound
  10k/25k/100k, Wheeler Junction 25k/50k/100k; `002fb720`). Target counts
  per course are in `atv_core::freestyle::course_objectives`; an
  all-course check of the 11 OB archives found each target TOY's `Cherry`
  triggers equal to its `NumberOfTargets`, and every target run loads its
  targets and cherries in the port. Env04's green start ships
  `2428.03,149,4460,47`; the port reads the first three components (assumed
  to match a three-float read in `002fa3c0`, not traced). Tracks.dat's per-course
  `pointObjective%d` repeat the `.obj` levels for those 11 courses; it also
  lists env03, env07 (four levels) and free05, which ship no OB archive.
- **Completion bits.** `00321ff0` returns a per-course, per-difficulty mask
  of six bits; `0025a100` shows "Completed Course Objectives: %d of 6" and
  "Completed Total Objectives: %d of 36/30" (profile +0x1e0 + type*0x3c).
  Six = the two target runs, the combos and the three point levels.
- **Run rules.** The event's limit is 120 s (`00289d50` +0x57c).
  Stunt: each level the run's stunt score passes shows "%d Point Level
  Reached"; the last completes it. ComboMambo: a successful chain of two or
  more stunts whose stunt sequence differs from every counted combo adds one
  ("Repeated Combos Do Not Count"; compared by the recorded stunt ids, up to
  25). Any objective that completes adds its whole seconds left x 100 to
  the score (`2fd980`) before the score / 1000 payout.
- **Target runs (CherryPick).** Class vtable 0x45f550 (base
  `QuadzillaObjective` 0x45f5f0): load `002fa840` (0xa8 NumberOfTargets,
  0xc4 BonusPointsPerTarget default 1000, 0xc8 BonusTimePerTargetInSec
  default 2, 0xe0 red when the `TOYFile` contains "RCP"), start `002fa9c0`
  (the objective's `DefaultStartPosition`/`DefaultStartDirection`, parsed by
  `002fa3c0`, placed through `002fa680`), stop `002faa10` (hides every
  cherry, disables the triggers), reset `002fae60` (shows the cherries,
  re-arms the triggers, registers each player's quad bodies with them),
  hit `002fab40`. Event type 2 reads the `ob` scene suffix
  (`0021ca20`) and `tracks\<course>.obj` (`002fcd98`); the OB archive
  holds the `.obj`, the two target TOYs, the cherry models and motion, the
  explosion emitters and the radar icon. Each target is a TOY trigger
  (`Radius=5`, `OneTimeOnly=T`) whose actions hide its cherry
  (`HideAnimation`), run `Cherry` (action 0xe, `00208a90` → `002fc610` →
  `002fda20` → the objective's hit) and start its explosion (`Particles`).
  A hit adds the bonus points and the bonus seconds to the time left, plays
  `iconhit.vag` and shows "Target Collected" for 1 s (Overlays.pb
  `[Single-Sign-8/9]`); the last shows "All Targets Collected" for 5 s
  (`[Single-Sign-10/11]`) and ends the run without waiting for the clock.
  Airborne or not does not matter. The cherry (`cherryg.psg`/`cherryr.psg`,
  a 10 ft flat target) loops `cherry.MOT` (1.67 s: one turn about +Y and a
  2 ft bob); the cherries show only during a target run. The Overlay Set 5
  HUD shows TARGETS n/total, TIME and SCORE and a Compass with one
  `RadarIcon_Target` dot per untouched target (`002e68e0`, `002e77f0`,
  `002e6e80`). `002edf00` builds the compass per player with the
  mode's icon (event type 2 → `RadarIcon_Target`, sized to the scene's
  model count); `002ed910` shows it only for a target run (objective
  mode 0) and refills it with `002e77f0`: every visible scene model whose
  name starts `cherry`. A hit removes that cherry's icon (`002fab40` →
  `002e78d0`). Icons are clamped to the 33 px rim like the riders
  (`002e6e80`; only an Enduro gate is hidden outside it). The port draws
  this compass at Overlay Set 5's 70,375 from the OB archive's
  `compassbkg`/`compassarrow`/`radaricon_target`, one icon per untouched
  target, heading-up as in Enduro (RACING.md).
- **Menus.** In-game Overlays.pb `[Objectives Menu]` ("Select Objective",
  six objective rows with check icons and point-level sub-rows),
  `[Objective Results Menu]` and `[Objective Championship Results Menu]`
  (`002ec310`, mode 2): "Congratulations, %s" / "Sorry, %s"
  (`002ddc50`), "You have completed the objective!" / "You have failed the
  objective." (`002dde00`), and the championship line (`002ddee0`): the
  trophy, else "You have completed %d/36 objectives." (Pro /30).
- **Championship.** Freestyle rounds always advance (`00322c60`). The
  trophy counts completed objectives: Amateur 24/30/36, Pro 18/24/30 for
  bronze/silver/gold (`003230f0` awards each when a new bit brings the
  count to it; `00322440` evaluates the same thresholds). Rewards (`00322650`)
  Amateur 1000/500/250 and Pro 2000/1000/500 on both difficulties; the
  unlocks are the racing ones (gold opens Expert, a trophy in Amateur
  Freestyle opens Pro Freestyle); a new profile has Amateur Freestyle open
  on Normal and Expert.

Port: `atv_core::freestyle` (objective parsing, the course table, `Run`
with the timer, levels, combos and targets, completion bits, the
round/trophy/reward/unlock rule; completion masks in
`Championships::objectives`). The Championship page lists Amateur and Pro
Freestyle (round, objectives completed, trophy); choosing one opens Select
Objective for the round's course (completed objectives marked, the point
levels counted) with Next Course / Finish Championship. An objective rides
the course in Freestyle with a top-centre panel (objective, time left,
progress, score, the latest level/combo notice); when it completes or time
runs out the Objectives results show the original lines, the course and
total counts, Restart Objective and Continue (back to Select Objective).

**Authored or approximate:** the original picks objectives in-game from the
`[Objectives Menu]` overlay (Select / Quit; Continue on the results returns
to it, `002ec310` mode 2); the port's Select Objective is a front-end page,
and its round ends with Next Course. The original has no such row: a round
ends when the rider quits the event (an inference from `00322c60`, which
always advances; the QuitEvent path was not traced). `003230f0` awards the
trophy, its reward (`00323440`) and unlocks the moment a championship
objective's new bit brings the series' count to a threshold; the port does
the same when a championship run is recorded (the results add the
Objective Championship Results line) and Finish Championship then pays
only a better trophy not yet awarded (a profile that already sat past a
threshold, as the original's counter could not). The completion bits of a course persist across
championship runs (the mask is per profile, course and difficulty, as
`00321ff0` reads it). Saving (`00322080` type 2) requires single player and
the 120 s limit, and does not check the championship flag. Point levels
count when reached even if the run then times out (a reading of
`002fb720`). An objective's end pays score / 1000 plus its combo points
(`002fce00`, see PROFILE.md "Freestyle records and Hall of Fame").

Target runs ride the course's OB archive (the hosted package carries the 11
archives' differing members, about 17-34 KB each, read over the Freeride
archive). **Trigger bodies.** `002fae60` registers, for each player in the
event, nine bodies of the quad entity with every target trigger through
`00172440` (which clears the pair's mutual collision exclusions,
`001201c0`; stop `002fb0a0` puts them back with `001723e0`): the `f.col`
passive actor (+0x984, listed twice), the `ar.col` actor rays (+0x980), the
`ao.col` chassis hull (+0x97c), the chassis rigid body (+0x18c) and the four
wheel collision bodies (+0x168..+0x174); offsets identified from the quad
setup `001e1fd0` (`ar.col`/`ao.col`/`f.col` suffixes at 0x427c18/20/28) and
the wheel pairing in `001e4c90`. The rider (+0x884, attached by
`001e4c90`) is not registered, so a thrown rider does not hit a target. A
`Radius` trigger is a sphere collision object (`00171f90`, class
`0032d840`, else a box); the generic collision pass reports the contact
(`00172bc0`). The port tests the quad's four wheel spheres (rest centres,
wheel radius), its `ao.col` hull points and its `ar.col` rays as segments
against the trigger sphere (`TriggerBody`, `Vehicle::trigger_bodies`); a
motorcycle (port extra) tests its origin. Not modelled: the chassis rigid
body's and `f.col`'s own shapes (the hull covers the chassis; `f.col`'s
extent was not traced), wheel suspension travel (centres sit at rest), and
the exact sphere-versus-ray-tree contact rule of the collision pass (the
port takes any point of a ray inside the sphere). The panel's notice line shows the
signs for their original 1 s / 5 s. A hit starts its target's cherry
explosion from the appended target TOY (AMBIENT_EFFECTS.md "Triggered
bursts"). The `RadarIcon_Target` compass is drawn as described above. Settings
word 0 == 1 skips the bonus seconds (mode not identified; the port always
adds them).

**Standalone mode.** The original offers Objectives as an event type on
Freestyle Prep (`0026a720` mode 2, cycled by `0026d010`/`0026d150`, offline
only; Tracks.dat `SponsoredStuntRaceTypeMask=88`). The port's Single Player
→ Freestyle Objectives lists the 11 courses with an OB archive (env03,
env07 and free05 ship none), then the same Select Objective page without a
round row. Standalone runs save to the course's mask under its series
(Amateur for the Environment venues, Pro for the Freestyle venues) on
Normal: the original keys `00321ff0` by the event category (settings +8),
believed but not proven to be the same series. The menu's check marks show
the saved bits; the original marks them only in a championship (`002fc840`,
settings +0x36d).

Checks: `atv-core` freestyle tests (parser and objective starts, target
TOYs and one-time hits, levels and time-out, combos, the seconds-left
bonus, rounds/trophy/unlock); `atv-ui` controller tests for the
championship and standalone page flows;
`tools/test_engine_freestyle_targets.mjs` rides Amateur round 1's green
target run and a standalone red run on Tioga Valley to completion
(`objectiveTest=targets` sets the ATV down 8 ft short of the next target
and rolls it through, so the real sphere test takes each hit); `tools/test_engine_freestyle_objectives.mjs` fails a Combos run on a
shortened clock (`objectiveTest=timeout`, five seconds), restarts it from Pause, completes Score Stunt Points
(`objectiveTest=score` credits the top level) and finishes Amateur
Freestyle for silver, its reward and the Pro Freestyle unlock.
