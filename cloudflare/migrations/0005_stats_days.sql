-- Public stats page rollups (research/ANALYTICS.md, "Public stats page"): one
-- row per UTC day, written by the five-minute cron at most every 15 minutes.
-- `data` is a JSON object of aggregate counts only (rides, riding seconds,
-- sessions, race finishes, splits by mode, activity, vehicle, device, venue
-- and country, and that day's peak riders online). Analytics Engine keeps
-- three months; these rows keep the all-time totals. `updated_at` is
-- milliseconds since the epoch. No visitor data is stored.
CREATE TABLE IF NOT EXISTS stats_days (
  day TEXT PRIMARY KEY,
  data TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;
