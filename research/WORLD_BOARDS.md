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
  `day`), `Cache-Control: public, max-age=60` and stored in the edge cache
  (Cache API), so a location reads D1 at most once a minute per board. Daily
  reads add `pick` and `yesterday` (that board's winner).
- `POST /api/boards/submit`: one finished event, at most 24 KiB. Race:
  `{name, vehicle, course, expert, lapMs?, raceMs?, daily?, trace}`.
  Freestyle: `{name, vehicle, series, course, score, seconds, trace}`.
  Answers `{name, boards: {<board>: {rank, placed}}}`; `422 implausible` with a
  `reason` otherwise. The name keeps only its best per board, and a time
  enters only while it ranks in the top 50; anything pushed below 50th is
  deleted in the same batch.
- Rate limits (native bindings, per IP, per minute, coarse per location):
  `BOARD_READ_LIMIT` 120, `BOARD_SUBMIT_LIMIT` 6.

### Plausibility (not proof)

The client is open source, so a determined cheat can forge any payload; the
checks stop trivial spoofing and impossible values.

- Names: 1-8 of `A-Z 0-9 space . _ ! -` after trimming and upper-casing,
  never the `@n` placeholder (the local Hall of Fame's `clean_name` rule).
  Vehicles 1-46 (27+ are motorcycles).
- Floors per lap (assumed, conservative; tighten from real boards): Enduro
  45 s, Short Tracks 12 s, Supercross 15 s, Nationals 25 s. A race needs the
  default laps, every lap after the first above the floor.
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
Ghosts have their own budget ("Ghosts" below).

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
