# Ghost challenges: medals, rival ghosts and the weekly challenge (port addition)

Issue #637, slice F of the modern multiplayer epic #631
(MULTIPLAYER_EXPERIENCE.md). Authored for the port: nothing here is recovered
behavior. It builds on the world boards, ghosts and house riders of
WORLD_BOARDS.md and keeps their rules (plausibility, not proof; no IP, device
id or trace stored). [J] marks judgment, [M] a measurement, [S] a source in
`private/reports/multiplayer-product-research-2026-11-14.md` (§6 async
competition, §8 light anti-cheat).

## Medals

- **Author time.** Each measured race course's author time is its fastest
  measured flying lap by the game's own race path driver (`MEASURED_LAP` in
  `cloudflare/board_seeds.mjs`, the same number the lap floors use; 51
  courses; "Measured times" in WORLD_BOARDS.md) [M]. The house leader of a
  Normal lap board is 1 to 2.2 % quicker than it, the tenth house rider 5 to
  16 % slower. The game carries the same table (`atv_core::medals::AUTHOR`);
  a unit test reads `board_seeds.mjs` and fails when the two differ.
- **Medals.** Bronze at 150 %, Silver at 120 %, Gold at 106 % and Author at
  100 % of the author time, rounded down to the millisecond; a lap at or
  under a medal's time earns it [J: Trackmania's shape, scaled so Gold is
  about a mid-board house rider and Author asks a rider to match the game's
  best]. A course without a measurement has no medals. Normal and Expert
  share them: the difficulty changes the CPUs, not the course.
- **Shown.** The race results reveal the best lap's medal coin with a quick
  pop (0.45 s, once the Hall of Fame name is entered) and NEW when it beats
  the course's medal before; Lap Attack says "Gold Medal!" on a lap that
  earns a better medal; Single Race's course list shows each course's coin;
  Profile Overview shows a strip of coins and weekly badges with counts.
  The rider's best lap per course is kept on this device (`fury-medals`,
  `atv_core::medals::MedalBook`); it is not part of the Pass Key cloud save.
- **Where a medal is earned.** Any single-player lap the world boards could
  take or that Lap Attack judges for its best lap. Assisted runs, Kid Mode,
  Practice, split screen and online races earn none (the world boards'
  rule).

## Ghosts

### Retention (Worker, `cloudflare/ghosts.mjs`)

A lap board keeps the ghosts of its ten best rider rows, as before, and now
one per band of five ranks below them down to rank 40 (11-15, 16-20, …
36-40): the band's best-ranked row that has one. At most 16 a board
(`GHOST_LIMIT`). Rank 40 is the last band's end because a served board is the
top 50 of riders and house riders together and a board has ten house riders,
so every kept ghost's row is on the served board. A ghost is dropped when its
row leaves the kept ranks, changes its time, or a better-ranked row of its
band gets one; uploads and placed laps settle the board in the same request
(`keptGhosts`, one read of the board's 40 best rows and its ghosts).

### The picker (game)

Lap Attack's setup offers a **Ghost** row: PB, a medal ghost, Rival and #1
(only the choices that exist are offered).

- **Off** (the default): the original Lap Attack, whose ghost is the
  session's own best lap.
- **PB**: the rider's own best Lap Attack or race lap on the course, kept on
  this device (`fury-pb-ghost:<course>`, the world ghost format as base64;
  replaced only by a quicker lap).
- **Medal**: the next medal the rider has not earned. Its ghost is the
  stored ghost nearest at or under that medal's time; when none is within
  1 % under it, the nearest ghost either side stands in and the choice reads
  "~Gold" [J].
- **Rival**: the stored ghost just above the rider's rank: the slowest
  ghost quicker than the rider's best (their board row, or their PB when
  the board does not have them). Without a time, the slowest stored ghost.
- **#1**: the board's best stored ghost.

### Friend ghosts (hook only; slice D)

Not implemented. When slice D's friend codes land (ONLINE_IDENTITY.md), a
friend's ghost is found by friend code, not by name:

- `GET /api/ghosts/<lap board>/friends?codes=<code>,<code>…` (at most 20
  public friend codes, never a Pass Key) answers
  `{board, ghosts: [{code, name, value, vehicle}]}` for the friends whose
  current row on the board has a kept ghost; cached like a board read.
- The ghost itself downloads through the existing
  `GET /api/ghosts/<board>/<name>/<ms>`.
- The game adds a **Friend** choice to the picker, stepping through the
  answer's rows. A ghost upload would need to carry the friend code with the
  name; D owns how a code is bound to a board name.

## Weekly challenge

- **Pick.** `weeklyPick` (`cloudflare/weekly.mjs`) and
  `atv_core::medals::weekly` hash the ISO week's index (UTC, Monday to
  Sunday) into the daily event's pool of championship race courses and a
  vehicle class (one week in four motorcycles). Both are pinned to the same
  weeks in their tests. The game computes the pick offline.
- **Event.** A full-length race at Normal with no CPUs, any vehicle of the
  week's class: a time trial whose times compare [J]. Board
  `weekly:<YYYY-Www>`, value the race time.
- **Menu.** Single Player shows **Weekly Challenge** beside Daily Event; its
  page shows the course, the time left, the rider's best and live percentile
  band, and the board's top ten.
- **Submissions.** As the daily: the race's submission carries
  `weekly: "<YYYY-Www>"`, accepted for this week (last week's for 30 minutes
  after it ends), the pick's course and class, Normal, the full race. It
  passes the race checks (floors, trace, marks) and, from the trace, every
  lap's (WORLD_BOARDS.md checks only the claimed lap and the whole race):
  each lap covers at least 500 ft at most 150 ft/s, each lap after the first
  closes within 400 ft of where it began, and each flying lap's ridden ground
  is within 0.6 to 1.6 times the flying laps' median (a cut or a jump the
  reset marks missed) [J]. The race clock against a wall clock needs a
  server-issued start that the client cannot forge (a signed ticket, which
  needs a Worker secret); that is not done, so a slowed simulation is not
  detected. The trace's one point a second of race clock already ties the
  claimed time to the run's length.
- **Keeps every finisher.** A weekly board keeps each name's best without
  the top-50 cut, so percentiles count everyone. The answer to a weekly
  submission adds `percentile` (1-100: the riders ahead plus one, over the
  rider count) and `riders`.
- **Cuts.** The five-minute cron (`refreshWeekly`) stores the week's rider
  count and the times at the top 1, 10, 25 and 50 % (`board_stats`,
  migration `0006_board_stats.sql`), at most every 15 minutes, then once more
  after the week's grace with `final: 1`. A board read adds `pick`, `ends`,
  `riders`, `cuts` and `final` from that row. Weekly rows older than five
  weeks are deleted; their final cuts stay.
- **Badges.** The game keeps the rider's best time per week. Once a past
  week's board reads `final`, its best earns **Top 1%**, **Top 10%**,
  **Top 25%**, **Top 50%** or **Finished** (the best band its time is inside;
  rank = ceil(band × riders), at least 1, so the week's winner is always
  Top 1 %) [J]. Badges are cosmetic and only add: a missed week costs
  nothing, and nothing counts a streak (MULTIPLAYER_EXPERIENCE.md UI rules).

## Budget (free plan)

Cloudflare's published D1 free limits (assumed current): 5 M rows read and
100 k rows written a day, 500 MB a database. Not measured on real D1.

- **Ghost storage.** At most 16 ghosts × 128 lap boards: 31 MB at the
  measured 15.2 kB a ghost (Supercross `sx01`), 92 MB if every ghost were as
  long as the longest measured lap (210.6 s, about 45 kB), 179 MB at the
  64 KiB cap (a lap of 6 min 49 s, which no course needs). It was 10 a board
  before.
- **Ghost reads.** A lap board read marks its ghosts with one query of at
  most 16 small rows (was 10), cached with the board for a minute per
  location. A placed lap or an upload reads at most 40 rows and 16 ghost
  keys (not their data) and writes at most one ghost plus what it deletes.
- **Weekly writes.** One row and its index per weekly submission, beside the
  race board's; the cron writes one stats row per refresh (at most 2 a run).
- **Weekly reads.** A board read: 50 rows and one stats row, at most once a
  minute per location. A refresh reads about 1.86 × the week's riders (a
  count and the cuts' offsets); at 96 refreshes a day that is 179 rows a
  rider a day: 1.8 M rows a day at 10,000 riders in a week, so past about
  20,000 the interval must grow. A submission's rank counts the riders
  ahead (at most the week's riders). At 2,000 riders and 1,000 weekly
  submissions a day that is at most 2 M rows; past that the percentile
  should come from the cuts instead of a count. [J: the server's peak so far
  was 150 riders online, mostly in Free Ride.]

## Evidence

- `atv_core::medals` unit tests: the author table equals `MEASURED_LAP`
  (read from `board_seeds.mjs`), medal times and the earned/next rules, the
  book, ISO weeks and the weekly pick pinned to the Worker's, badges, and the
  medal/rival/#1 ghost picks. `atv_core::world_board`: the weekly answer's
  line and board fields. `atv_ui` `world_page`: the Ghost row's choices and
  the events they start, the Weekly Challenge row, page, band and start, the
  profile strip's counts; `medal`: the reveal curve.
- `cloudflare/weekly.test.mjs` and `ghosts.test.mjs`: weeks, the pick, the
  weekly checks, every finisher kept, the percentile, the cron's cuts,
  freezing and deletion; the ghost bands.
- `tools/test_engine_ghost_challenges.mjs` (the Worker in the test process
  over SQLite D1, as `test_engine_world_ghosts.mjs`): a two-lap race by the
  race path driver reveals its medal and keeps the PB ghost, Lap Attack's
  Ghost row offers Off, PB, the medal ghost, Rival and #1 from the board, Rival
  starts Lap Attack against that ghost, and the Weekly Challenge page shows
  the rider's band from the cron's cuts.

## Limits

- No human lap was measured, so how hard the medals are for a person is
  untested; the author times are the game's own driver's.
- Medals, weekly results and the PB ghost live on one device.
- A weekly race's "no CPUs" is the client's: the server cannot tell.
- The browser test runs on host Chromium only (no phone, no desktop app) and
  never against production D1.
