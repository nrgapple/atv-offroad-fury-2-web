-- Weekly challenge percentiles (research/GHOST_CHALLENGES.md "Weekly
-- challenge"): one row per weekly board, written by the five-minute cron at
-- most every 15 minutes. `riders` is the board's rider count, `cuts` a JSON
-- object of the times at the top 1, 10, 25 and 50 % ({"1": ms, ...}), `final`
-- 1 once the week is over and the cuts no longer change. `updated_at` is
-- milliseconds since the epoch. Aggregates only: no names.
CREATE TABLE IF NOT EXISTS board_stats (
  board TEXT PRIMARY KEY,
  riders INTEGER NOT NULL,
  cuts TEXT NOT NULL,
  final INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;
