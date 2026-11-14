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
- Freestyle: at most 425 s, score at most 1,000,000 and 15,000 a second.
  The objective's limit is 120 s, but a target run gains 10 s a green target
  (5 s a red one): St. Croix Sound's 31 green targets allow 120 + 30 × 10 s
  before the last hit ends the run (`atv_core::freestyle::course_objectives`).
  The limit was 125 s until 2026-11-12, which refused long target runs.
- Daily: only today's date (yesterday's for 30 minutes after midnight UTC),
  Normal difficulty, the pick's course, a vehicle in the pick's class, and
  a full-length race.

### Budget (free plan)

D1 free allows 5 M rows read and 100 k rows written a day. A submission is
sent only for a run that earned a local Hall of Fame place or a daily time,
and writes at most three boards (lap, race, daily) of one row plus its index
each. A cached board costs at most one 50-row read per location a minute.
The house riders add no query and no row: a board read is one D1 query
for its rows (plus yesterday's winner on a daily board and the ghost names
on a lap board), and a submission's batch is unchanged but for one bound
number (the house riders ahead of the value). Ghosts have their own budget
("Ghosts" below); house riders never carry one.

### Privacy

A board row is a rider name, a value, a vehicle id and a UTC date. No IP,
Pass Key, device id or trace is stored; IPs key only the edge limiters.

## Freestyle objective scores (2026-11-12)

The game sends a single-player Freestyle objective's score to
`score:<series>:<course>` when the score earned one of the course's three
local Highest Objective Score places, by the race's rule: once its Hall of
Fame name is entered (or the results are left, under the stored name).
`ObjectiveSession` (atv-app) keeps the run's clock and its trace, one point a
second from the run's start with the ride's resets marked, in fixed storage
that a restart clears; `world::ScoreRun::of_objective` builds the submission.
Nothing is sent for split screen (it never comes here), for a run a rider
assist acted in (the race rule; the local Hall of Fame still takes it), or
for a score that placed nowhere locally. Kid Mode and Practice are race
event types and do not exist for objectives. The results summary ends with
the answer ("World #2 Objective Score!", or "World boards offline"); a
refusal shows nothing. The World view's Freestyle groups read one board per
series and course (the Difficulty row does not apply) and show grouped
scores.

Evidence: `atv_core::world_board` and `atv_app::world` unit tests (the body,
the place rule, the assist exclusion), `cloudflare/boards.test.mjs` (the
game's body is accepted; a 380 s target run is accepted), and
`tools/test_engine_world_boards.mjs` (mocked API: the World view lists
Amateur Freestyle's score board; an objective's score is sent after the name
entry and the results show the World rank).

Limits: the browser test's run is ended by `objectiveTest=score` in its
first step, so its body (one second, one or two trace points) is one the
real server refuses (`trace`); the test checks the request and the results
line against a mocked answer, not a real placing. No real objective run was
ridden to the end against the production API. The trace's speed checks are
the race's (200 ft a one-second step); no measured Freestyle top speed backs
them.

## Ghosts (2026-11-12)

Issues #543 ("board entries can carry a downloadable ghost") and #542
("beat my lap"). A lap board's best rows carry the lap's recorded motion;
Hall of Fame → View: World marks such a row "Ghost" and offers **Race This
Ghost**, which starts Lap Attack on the course against it.

### What is recorded, and when

Facts from the code: laps reach the world lap boards only from races that
keep records, and the Lap Attack ghost (`atv_core::race_ghost`) is recorded
only in Lap Attack, which keeps no records (RACING.md "Lap Attack event
type"). Its recording also keeps every drawn node matrix: about 1.7 MB for a
minute's lap. So a separate recorder was added:

- `atv_core::world_ghost::Recorder` runs in every race that keeps records
  (so never in Kid Mode, Practice, Lap Attack, a User Created Enduro, split
  screen's other players, or once an assist acted): one key, the vehicle's
  position and orientation, every sixth simulation step (10 a second), the
  fastest judged lap's keys kept. Fixed storage of 2 × 4094 keys (about
  230 kB), made at the race's first live step; nothing is allocated while
  racing.
- The bytes (version 1): a 32-byte header (`FRGH`, version, key stride,
  vehicle, lap milliseconds, lap steps, key count, course id) and 16 bytes a
  key (three `i32` of 1/32 ft; the orientation's three smaller quaternion
  components in 10 bits each). 160 bytes a second of lap.
- **Cap: 64 KiB** (65,536 bytes = 4094 keys = a lap of 6 min 49 s). A longer
  lap is the best lap without a ghost. **Measured:** Supercross `sx01`'s lap
  of 1:11.21 ridden by the race path driver is 4272 steps, 712 keys and
  11,424 bytes (15,232 characters of base64); a 60 s lap is 9.6 kB by the
  same arithmetic.
- When the submission's answer says the lap placed on its lap board, the
  game uploads the ghost under the same name (`world.rs`). A Freestyle
  score, a race time or a daily time carries none.

### Server (`cloudflare/ghosts.mjs`, migration `0003_board_ghosts.sql`)

- `POST /api/ghosts/<lap board>`, body `{name, ghost}` (the bytes as base64,
  at most 88.4 kB of body): stored only if the board's **ten best rider
  rows** (`GHOST_TOP`) hold a row with that name, exactly the ghost's lap
  time and its vehicle; a ghost already stored for that lap is not replaced.
  Answers `{board, name, kept}`. Checked before storing: size, magic and
  version, the board's course, a key count that fits the steps and steps
  that last the lap time within 0.25 s, positions inside ±1,000,000 ft, at
  most 30 key steps longer than 40 ft (rider resets), at least 500 ft
  ridden. `422 implausible` with a `reason` otherwise.
- `GET /api/ghosts/<lap board>/<name>/<ms>`: `{board, name, value, vehicle,
  ghost}` while the row still has that time; `Cache-Control: public,
  max-age=3600` and stored in the edge cache. The address names the lap
  time, so a rider's better lap is a new address and cached copies never go
  stale.
- A ghost is deleted when its row leaves the board's ten best or changes its
  time: in the same request as the submission that caused it
  (`pruneGhosts`, one `DELETE` per placed lap board) and with every upload.
  House riders' rows have no ghost.
- `GET /api/boards/lap:…` marks rows with `ghost: true` (`markGhosts`, one
  query of at most ten small rows, cached with the board).
- The same limiters as the boards: uploads count against
  `BOARD_SUBMIT_LIMIT` (6 a minute an IP), downloads against
  `BOARD_READ_LIMIT`.
- Like a board row, a ghost is not proof and its upload is not
  authenticated: whoever knows a row's name, time and vehicle can attach a
  ghost to a row that has none. The first one stands.

### Where it is stored, and the budget (free plan)

D1, table `board_ghosts (board, name, value, data)`, `data` the base64 text
(checked to 87,384 characters). Why D1 and not R2: the Worker's only R2
binding is the game's content bucket, which the release tools own and serve
publicly; rider uploads do not belong in it, and an R2 object could not be
deleted in the same transaction as the row it belongs to. Why text and not a
BLOB: the game's request path carries text bodies on the web and on desktop
(`cloud::fetch`, `native_request`), so the ghost is base64 in JSON both ways
and a download hands the stored text on without decoding it in the Worker;
a BLOB would save a quarter of the storage but its D1 round trip could not be
checked without a local workerd (the tests run on SQLite). Not measured:
real D1.

- Storage: the server accepts at most 64 race course ids, so 128 lap boards
  × 10 ghosts × 87.4 kB = 112 MB at the cap; 20 MB at the measured 15.2 kB.
  Cloudflare's published D1 free limits (assumed current): 500 MB a
  database, 5 GB an account.
- Rows written (100 k a day free): an upload writes one row and its index
  entry; a prune writes only what it deletes.
- Rows read (5 M a day free): a download reads the ghost and its board row,
  at most once an hour per edge location per ghost; a board read adds at
  most ten ghost rows a minute per location.
- Requests: one extra request per placed lap, sent whether or not the lap is
  in the ten best (the answer's rank counts house riders, so the game cannot
  tell); the server refuses the others cheaply (`kept: false`).

### In the game

- `BoardEntry.ghost`; the World view shows "· Ghost" on the row and two more
  rows: **Ghost: NAME · time** (Left/Right steps through the board's ghosts)
  and **Race This Ghost**. Locked courses and vehicles are refused as in
  Single Race.
- The host downloads the ghost (`world_ghost::Rival::from_download`), which
  checks the JSON, the base64, every header field and size, that positions
  are inside the world bound, and that it is the lap of the course and time
  asked for. Anything else is reported on the page ("This ghost could not be
  read.", "This ghost needs a newer version of the game.", "This ghost
  belongs to another lap.", "This ghost is no longer on the board.",
  "Offline: a ghost needs the network.") and nothing starts. No path indexes
  unchecked bytes.
- A good ghost starts Lap Attack with the garage's vehicle.
  `race_ghost::Ghost::set_rival` makes it the lap to beat from GO: the HUD
  says "NAME's Ghost · time", the ghost replays from each lap's start, and
  the first faster lap says "You Beat NAME's Ghost!" and replaces it with
  the rider's own recording, as any best lap does. Restart Race races it
  again.

### Evidence

- `atv_core::world_ghost` unit tests: round trip within the format's
  precision, sizes and the cap, the recorder's fixed storage and best-lap
  rule, playback between keys and across a reset, and malformed input
  (every truncation and single-byte change of a ghost's first 400 bytes
  decodes or errors without a panic). `race_ghost`: a rival lap replays in
  the player's first pose until beaten. `atv_ui` `world_page`: the ghost
  rows and the event. `atv_app` `world`: a ghost travels only with its own
  lap.
- `cloudflare/ghosts.test.mjs` (the Worker's fetch handler over SQLite with
  the production migrations): format and plausibility refusals; storing,
  marking and downloading; the edge cache; the ten-row rule; deletion when a
  row leaves the ten best or improves; method, size, origin, limiter and
  storage failures.
- `tools/test_engine_world_ghosts.mjs`: the game, the Worker itself run in
  the test process and a local SQLite D1. (A `wrangler dev` Worker was not
  used: the browser runner serves `web/dist` statically and starts no
  Worker. The handler module and its SQL are the deployed ones; workerd's
  own D1 is not exercised.) A two-lap Supercross race by the race path
  driver places its lap, uploads the ghost, the World view offers it, eleven
  kinds of bad download fail with their message and start nothing, and the
  real one starts Lap Attack with the ghost drawn and moving under its
  rider's name.

### Limits

- Motion only. The ghost is drawn as the downloading rider's own vehicle and
  rider in the pose of their first key on the grid: no wheel spin,
  suspension, steering or rider animation, and not the vehicle the lap was
  ridden on.
- Lap Attack keeps no records, so a lap that beats a world ghost there is
  not sent to the board; only race laps are.
- The ghost starts its lap at GO from wherever its lap began (the line, at
  speed, for any lap but a race's first), while the rider starts on the
  grid. From the second lap on both start at the line together.
- The rider's name shows in HUD messages, not as a label over the ghost.
- Not verified on production or on real D1 (no test rows are written
  there), on the desktop build (same code and API, not run), or on a phone.
  The edge cache was exercised with a stand-in for the Cache API.

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
  house row is never marked as the player's even under the same name. The
  Daily Event page and a Freestyle course show a board's top ten; a race
  course's Hall of Fame view shows the top five of its lap board and of its
  race board (`PAIR_SHOWN`: ten of each ran off the panel, which does not
  scroll), each with the rider's own row below when it ranks lower.
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

Findings left alone: on `env01st` the first CPU of the roster was credited a
lap 0.13 s after GO (it rode four laps and showed a 0.13 s best lap; issue
#584, fixed since: its start slot stood behind the skewed gate line, see
research/RACING.md "Allowance only after an approach"); those races are
excluded here, and its flying laps (line to line) are real laps. On
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
read adds no D1 query).

### End-to-end proof (local, never production)

`tools/test_engine_world_submit.mjs` runs the game against the Worker itself
(`cloudflare/worker.mjs` in the test process, a SQLite D1 with the production
migrations, `cloudflare/testing.mjs`; no response is mocked): Single Player →
Daily Event → Start, the driver rides the day's five-lap race to a real
finish, the name entry names the records, one submission goes out and is
accepted, the results show the World line, and the Daily Event page and Hall
of Fame → View: World show the row among the house riders. The test computes
each expected rank from the submitted value and the house rows. Host run
(Chromium, 2026-11-12, the day's pick `env05` on an ATV): the driver's lap
0:21.40 and race 1:49.29 came back as "World #3 Fastest Lap! · World #4
Daily Event! · World #4 Fastest Race!" among ten house riders a board (an
earlier run: 0:21.35 and 1:49.59, third, fourth and fifth; the browser's
frame timing varies the ride).

Limits: host Chromium only (no phone, no desktop app run); the deployed
Worker and D1 are exercised only by read-only GETs; no human lap, race or
score was measured, so how hard the house leaders are for a person is
untested; Freestyle and motorcycle-day numbers rest on the assumptions above.
