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

Open: what opens Short Tracks, the course
unlock side effects for other modes (`00323640`), runtime confirmation of the
round-to-course mapping, and whether a lesser trophy after a better one pays.

## Port

- `atv_core::championship`: the six racing series whose courses the port
  loads (Amateur and Pro Enduro, Amateur and Pro Nationals, Amateur and Pro
  Supercross), their
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
mode files); there is no memory-card save or trophy page. Short Tracks
championships are not offered: nothing found opens Short Tracks.

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
  per course are in `atv_core::freestyle::course_objectives`.
- **Completion bits.** `00321ff0` returns a per-course, per-difficulty mask
  of six bits; `0025a100` shows "Completed Course Objectives: %d of 6" and
  "Completed Total Objectives: %d of 36/30" (profile +0x1e0 + type*0x3c).
  Six = the two target runs, the combos and the three point levels.
- **Run rules.** The event's limit is 120 s (`00289d50` +0x57c).
  Stunt: each level the run's stunt score passes shows "%d Point Level
  Reached"; the last completes it. ComboMambo: a successful chain of two or
  more stunts whose stunt sequence differs from every counted combo adds one
  ("Repeated Combos Do Not Count"; compared by the recorded stunt ids, up to
  25). CherryPick: each target adds its bonus points and seconds.
- **Menus.** In-game Overlays.pb `[Objectives Menu]` ("Select Objective",
  six objective rows with check icons and point-level sub-rows),
  `[Objective Results Menu]` and `[Objective Championship Results Menu]`
  (`002ec310`, mode 2): "Congratulations, %s" / "Sorry, %s"
  (`002ddc50`), "You have completed the objective!" / "You have failed the
  objective." (`002dde00`), and the championship line (`002ddee0`): the
  trophy, else "You have completed %d/36 objectives." (Pro /30).
- **Championship.** Freestyle rounds always advance (`00322c60`). After the
  last round the trophy (`00322440`) counts completed objectives: Amateur
  24/30/36, Pro 18/24/30 for bronze/silver/gold. Rewards (`00322650`)
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

**Authored or approximate:** the original picks objectives in-game from an
overlay; the port's Select Objective is a front-end page, and the round
ends with Next Course rather than a recovered in-game end (the round end
condition was not found). The completion bits of a course persist across
championship runs (the mask is per profile, course and difficulty, as
`00321ff0` reads it; resetting at a new run was not found). Point levels
count when reached even if the run then times out (a reading of
`002fb720`). The target runs are listed but **not playable**: their
trigger TOYs and cherry models ship only in the OB archives, which the
hosted runtime package does not include yet, so 24 of 36 (Amateur) and 18
of 30 (Pro) objectives are reachable today: bronze at most. No Profile
Points are paid per objective (none found); the standalone Single Player
Freestyle objectives mode (`Freestyle Prep` → Objectives) is not offered.

Checks: `atv-core` freestyle tests (parser, levels and time-out, combos,
targets, rounds/trophy/unlock); `atv-ui` controller test for the page
flow; `tools/test_engine_freestyle_objectives.mjs` fails a Combos run on a
shortened clock (`objectiveTest=timeout`), completes Score Stunt Points
(`objectiveTest=score` credits the top level) and finishes Amateur
Freestyle for silver, its reward and the Pro Freestyle unlock.
