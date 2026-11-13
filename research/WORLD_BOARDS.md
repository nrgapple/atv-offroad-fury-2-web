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

## Daily event

`dailyPick(date)` (JavaScript) and `atv_core::world_board::daily` (Rust)
hash the UTC day number into the championship race courses (Enduro, Nationals
and Supercross, in event-type order; `DAILY_POOL`) and a vehicle class (one
day in four motorcycles). Both are pinned to the same dates in their tests.
The game computes the pick offline; only its board needs the network.
