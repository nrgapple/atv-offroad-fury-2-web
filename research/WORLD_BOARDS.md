# World Hall of Fame boards and the daily event (port addition)

Issue #543. The original keeps its Hall of Fame on the memory card; the port
adds optional world boards beside the local one and a date-seeded daily event.
Authored for the port: nothing here is recovered behavior.

## Server (`cloudflare/boards.mjs`)

Same Worker and D1 database (`SAVES`, `fury-saves`) as Pass Key cloud saves
(CLOUD_SAVE.md); additive migration `0002_world_boards.sql` adds one table,
`board_entries (board, name, value, vehicle, day)`, primary key `(board, name)`.
`value` is lower-is-better: milliseconds, or the negated Freestyle score.

| Board id | Value |
| --- | --- |
| `lap:<course>:n` / `:x` | best lap, ms (Normal / Expert) |
| `race:<course>:n` / `:x` | full-length race time, ms (3 laps Enduro, 5 otherwise) |
| `score:<series>:<course>` | Freestyle objective score (`unspstunt`, `spstunt`) |
| `daily:<YYYY-MM-DD>` | the day's event race time, ms |

- `GET /api/boards/<board>`: the top 50 (`rank`, `name`, `value`, `vehicle`,
  `day`, and `house: true` on a house rider's row, below), `Cache-Control:
  public, max-age=60` and stored in the edge cache (Cache API), so a location
  reads D1 at most once a minute per board. Daily reads add `pick` and
  `yesterday` (that board's winner).
- `POST /api/boards/submit`: one finished event, at most 24 KiB. Race:
  `{name, vehicle, course, expert, lapMs?, raceMs?, daily?, trace}`.
  Freestyle: `{name, vehicle, series, course, score, seconds, trace}`.
  Answers `{name, boards: {<board>: {rank, placed}}}`; `422 implausible` with a
  `reason` otherwise. The name keeps only its best per board, and a time
  enters only while it ranks in the top 50 of the riders and the house
  riders together; anything pushed below 50th of the riders is deleted in
  the same batch. `rank` counts the house riders ahead.
- Rate limits (native bindings, per IP, per minute, coarse per location):
  `BOARD_READ_LIMIT` 120, `BOARD_SUBMIT_LIMIT` 6.

### Plausibility (not proof)

The client is open source, so a determined cheat can forge any payload; the
checks stop trivial spoofing and impossible values.

- Names: 1-8 of `A-Z 0-9 space . _ ! -` after trimming and upper-casing,
  never the `@n` placeholder (the local Hall of Fame's `clean_name` rule).
  Vehicles 1-46 (27+ are motorcycles).
- Floors per lap: half the course's fastest measured lap (`MEASURED_LAP` ×
  `LAP_FLOOR_FRACTION` 0.5; "House riders" below has the measurement), 0.4
  of it for Enduro (`ENDURO_LAP_FLOOR_FRACTION`: a rider picks the line
  between the gates, the driver follows its route): from 5.6 s (`env01st`)
  to 84 s (`env04endl`). To be refused on a circuit a rider would have to
  average twice the race path driver's speed over the lap: its laps average
  about 63 ft/s and its fastest second in 126 traced races covered 135 ft,
  so that is over 125 ft/s for a whole lap. Assumption: no strong rider on
  the fastest vehicle laps in half the driver's time (0.4 in Enduro); no
  human laps were measured. The earlier event-type floors
  (Enduro 45 s, Short Tracks 12 s, Supercross 15 s, Nationals 25 s) remain
  for a course id without a measurement; they were wrong where it mattered:
  the CPU riders' own laps on `env05` (21.4 s against 25 s) and `env01st`
  (11.3 s against 12 s) were refused. A race needs the default laps, every
  lap after the first above the floor.
- Trace: one `[x, z]` integer-feet point per second from GO (the count must
  match the run's length within 2), `marks` (each lap's clock ms; the lap
  time must be one consecutive difference and the race time the last mark)
  and `resets` (points a rider reset moved the vehicle to; the step into them
  is skipped). Any ridden step over 200 ft, an average over 150 ft/s, a lap
  under 500 ft of ground, or a lap (after the first) ending over 400 ft from
  where it began is refused.
- Freestyle: at most 125 s, score at most 1,000,000 and 15,000 a second.
- Daily: only today's date (yesterday's for 30 minutes after midnight UTC),
  Normal difficulty, the pick's course, a vehicle in the pick's class, and
  a full-length race.

### Budget (free plan)

D1 free allows 5 M rows read and 100 k rows written a day. A submission is
sent only for a run that earned a local Hall of Fame place or a daily time,
and writes at most three boards (lap, race, daily) of one row plus its index
each. A cached board costs at most one 50-row read per location a minute.
The house riders add no query and no row: a board read is still one D1
query (two for a daily board), and a submission's batch is unchanged but
for one bound number (the house riders ahead of the value).

### Privacy

A board row is a rider name, a value, a vehicle id and a UTC date. No IP,
Pass Key, device id or trace is stored; IPs key only the edge limiters.

## Daily event

`dailyPick(date)` (JavaScript) and `atv_core::world_board::daily` (Rust)
hash the UTC day number into the championship race courses (Enduro, Nationals
and Supercross, in event-type order; `DAILY_POOL`) and a vehicle class (one
day in four motorcycles). Both are pinned to the same dates in their tests.
The game computes the pick offline; only its board needs the network.

## House riders (authored)

The exact build writes no default holders into its Hall of Fame (`00331750`
clears each record; notes in `crates/atv-core/src/hall_of_fame.rs`), so an
empty board is the original behavior on the memory card, and the local Hall
of Fame keeps it. The world boards are a port addition, and an empty world
board gives a new rider nothing to chase; the owner asked for names and times
on them. Every lap, race, score and daily board therefore carries ten house
riders. Authored for the port: nothing in this section is recovered.

- **Where they live.** In the Worker only (`cloudflare/house.mjs`, table
  `cloudflare/board_seeds.mjs`). A board read merges them with the D1 rows
  and serves the top 50, marking each `house: true`; a submission counts the
  house riders quicker than its value into its rank and into the top-50 test.
  Nothing is written to D1 and no migration is needed, so the table can be
  rebuilt and redeployed at any time. A rider may use a house rider's name:
  the two rows are separate (D1 holds only the rider's) and both show. A
  rider's row that the house riders push below 50th stays in D1 unserved (at
  most ten a board) until the riders' own top 50 drops it.
- **In the game.** `BoardEntry::house`; the rows look like any other, and a
  house row is never marked as the player's even under the same name.
- **Names.** 34 authored names (`RIDERS` in `tools/build_board_seeds.mjs`),
  each with a discipline it rides best. The original's own CPU roster names
  were not used: they exist only in the disc's `AIRiders.pb` (content, which
  the repository does not carry), and the sixteen the measurement met read as
  surnames of people. A rider's standing on a board is a fixed skill, a bonus
  on its discipline and the board's own draw, so the leaders change from
  course to course without being random: 21 names lead one or more of the
  215 lap, race and score boards, the most frequent 22 of them.

### Measured times

`cargo run --release -p atv-game --example cpu_times` (headless, original
content, the deterministic 60 Hz simulation) runs full-length races (5 laps,
3 for Enduro) with four CPU riders and records every rider's lap marks, best
lap and finish, the roster and the player's world board trace. 204 races, 51
courses (8 venues' National, long and short Enduro; 3 Short Tracks; 8 Pro
Nationals; 16 Supercross) in four configurations, 22.9 simulated hours:

| Run | Difficulty | Player | Quad | Seed |
| --- | --- | --- | --- | --- |
| A | Expert | waits on the grid, so all four CPUs ride the whole race | 1 | 7 |
| B | Expert | ridden by the race path driver (`autopilot`) | 1 | 11 |
| C | Normal | ridden by the driver | 1 | 7 |
| D | Normal | ridden by the driver | 4 | 11 |

Facts:

- The CPU riders and the driver on the player's vehicle are the same race
  path driver. The difficulty changes only the CPUs' leader limit (Normal
  holds a CPU ahead of the player near the player's speed; `race_ai.rs`),
  not the course or the vehicles. The race ends when the player finishes
  and riders still out get estimates (`race_over`), so on Normal a CPU rode
  the whole race in 125 of 408 cases (on Expert with the player waiting, in
  196 of 204). Estimates are never used.
- Fastest flying lap (every lap after the one from the grid) and fastest
  full race per event type, seconds:

  | Event type | Courses | Fastest lap | Fastest race |
  | --- | --- | --- | --- |
  | Amateur Nationals | 8 | 21.4 (`env05`) to 102.3 (`env01`) | 109 to 543 |
  | Pro Nationals | 8 | 43.1 (`nat01`) to 154.7 (`nat04`) | 217 to 817 |
  | Supercross | 16 | 40.6 (`sx12`) to 77.6 (`sx15`) | 207 to 478 |
  | Short Tracks | 3 | 11.2 (`env01st`) to 27.8 (`env07st`) | 58 to 143 |
  | Enduro short | 8 | 69.0 (`env05ends`) to 155.6 (`env07ends`) | 209 to 486 |
  | Enduro long | 8 | 93.2 (`env03endl`) to 210.6 (`env04endl`) | 281 to 637 |

  The driver on the player's quad set the fastest lap on 38 courses, a CPU
  rider on 13. Per course and difficulty the numbers are in ignored
  `private/reports/world-boards/` (`cpu-times/*.jsonl`, `anchors.json`).
- A race takes about laps × the best lap or more, except where the grid
  stands ahead of the line: the least measured ratio is 0.94 (`sx09`).
- All 126 races the driver rode for the player pass the server's checks as
  the game would submit them (`node tools/build_board_seeds.mjs --verify`:
  floors, trace, lap and race marks).
- A separate sweep (the driver on each of the 46 vehicles, two laps of
  `sx01`, `vehicle-sweep/`) lapped in 66.4 to 86.1 s on the ATVs and 77.5 to
  99.3 s on the motorcycles, where it resets about twice as often. No CPU
  rides a motorcycle, so there is no motorcycle measurement of a rival.

Findings left alone: on `env01st` the first CPU of the roster is credited a
lap 0.13 s after GO (it rides four laps and shows a 0.13 s best lap; issue
#584); those races are excluded here. On
`sx15` every CPU needs the authored recovery (`cpu_recovery`) several times a
race, so its measured times are soft.

### From measurements to rows (`tools/build_board_seeds.mjs`)

- **Anchor.** Per course, the fastest measured flying lap and the fastest
  measured full race, of any rider on either difficulty (Normal and Expert
  share it, as above).
- **Leader.** A little quicker than the anchor, so a rider has to beat the
  game's own best to lead: 1 to 2.2 % on Normal, 2.5 to 4 % on Expert (the
  quicker Expert field is authored).
- **The rest.** Each next time is at least 0.2 % slower, most gaps small, a
  few large, widening down the board (`spread`): the tenth is 5 to 16 %
  behind the first, 10 % at the median. Milliseconds are not rounded.
- **Lap and race agree.** Twelve riders are drawn per course and difficulty;
  the lap board shows the ten quickest laps and the race board the ten
  quickest races, so the boards share most names in a different order. No
  rider's race is quicker than laps × its own lap × 1.002 (× the course's
  measured ratio where the grid stands ahead of the line, never under 0.97),
  and the same holds for the two leaders.
- **Vehicles.** The quads the game's CPU roster rode on that course in the
  measurement (the course's `ChampQuadGroup` or the player's ATV group); a
  rider keeps one vehicle on a course.
- **Freestyle scores.** No CPU rides Freestyle, so these are not measured.
  Basis: the course's own top "Score Stunt Points" level (50,000; 100,000 on
  `free04` and `free06`; `atv_core::freestyle::course_objectives`). A run
  that completes that objective ends just past the level plus 100 a second
  left; a combo or target run is not capped. A board's house scores run from
  about 1.5 times the level (71,475 to 80,820 on the 50,000 courses) down to
  between two thirds of it and just under it (33,055 to 46,395). Assumption:
  that is what a competent run reaches; no human scores were measured.
- **Daily boards.** Computed from the date when served (`houseRows`): ten
  names drawn for the day, the leader within 1.5 % of the pick's race anchor
  (quicker on some days, slower on others, so a daily can be won by matching
  the game's best), the same spread, the course's roster quads, or 125 to
  500 cc motorcycles on a motorcycle day. Assumption: motorcycle days use
  the ATV anchor. Yesterday's winner is the better of that day's riders and
  its house leader.

Rebuild: rerun the example into `private/reports/world-boards/cpu-times/`,
then `node tools/build_board_seeds.mjs` and `node --test cloudflare/*.test.mjs`
(every board has ten rows, names fit the name rule, laps clear the floors,
races agree with laps, ranks and the top 50 count the house riders, a board
read is one D1 query).

### End-to-end proof (local, never production)

`tools/test_engine_world_submit.mjs` runs the game against the Worker itself
(`cloudflare/worker.mjs` in the test process, a SQLite D1 with the production
migrations, `cloudflare/testing.mjs`; no response is mocked): Single Player →
Daily Event → Start, the driver rides the day's five-lap race to a real
finish, the name entry names the records, one submission goes out and is
accepted, the results show the World line, and the Daily Event page and Hall
of Fame → View: World show the row among the house riders. The test computes
each expected rank from the submitted value and the house rows.

Limits: host Chromium only (no phone, no desktop app run); the deployed
Worker and D1 are exercised only by read-only GETs; no human lap, race or
score was measured, so how hard the house leaders are for a person is
untested; Freestyle and motorcycle-day numbers rest on the assumptions above.
