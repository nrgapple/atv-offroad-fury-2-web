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

Open: what opens Short Tracks, the Freestyle objective counter, the course
unlock side effects for other modes (`00323640`), runtime confirmation of the
round-to-course mapping, and whether a lesser trophy after a better one pays.

## Port

- `atv_core::championship`: the four racing series whose courses the port
  loads (Amateur and Pro Nationals, Amateur and Pro Supercross), their
  courses, rounds, laps and rewards; championship points, the advance rule,
  trophies, first-trophy rewards and unlocks. Progress per series and
  difficulty lives in the saved profile (`fury-profile`, `PROFILE.md`).
- Single Player → Championship lists each series with its round, points and
  best trophy (or Locked), and a Normal/Expert row. Choosing a series starts
  its next round: five laps, four CPUs, Expert CPUs on Expert. The race
  results add a championship line (points and total, the shortfall and a
  replay prompt, or the trophy, reward and unlocks), and Profile Points use
  the course table for the series and difficulty. Quit Event returns to the
  Championship page.

**Authored or approximate:** a better trophy than the best so far pays its own
reward once; the CPU slots of the standings are the race's CPU slots, whose
roster is drawn per race (the original keeps its championship roster);
Amateur Nationals races use the port's Environment race courses (the National
mode files); there is no championship loading screen, Continue Championship
item, memory-card save or trophy page. Enduro, Freestyle and Short Tracks
championships are not offered: their course variants or objectives are not
ported.

Checks: `atv-core` championship tests (lists, short round replay, final
round trophy/reward/unlocks, tie rule); `atv-ui` controller test for the
page; `tools/test_engine_championship.mjs` races the final Amateur
Supercross round from the menu and checks the saved profile and the page.
