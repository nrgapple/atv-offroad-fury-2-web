-- Game server health (research/GAME_SERVER.md): the site Worker's five-minute
-- cron fetches the game server's /healthz and records one row per check; rows
-- older than 30 days are pruned. `checked_at` is milliseconds since the epoch.
-- `ok` is 1 when /healthz answered 200 with {"ok":true}; `error` says why not.
-- No visitor data is stored.
CREATE TABLE IF NOT EXISTS game_server_checks (
  checked_at INTEGER PRIMARY KEY,
  ok INTEGER NOT NULL CHECK (ok IN (0, 1)),
  status INTEGER,
  latency_ms INTEGER,
  build TEXT,
  rooms INTEGER,
  riders INTEGER,
  connections INTEGER,
  error TEXT
);

-- The one open outage issue the optional GitHub alert keeps (at most one row).
CREATE TABLE IF NOT EXISTS game_server_alert (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  issue INTEGER NOT NULL,
  opened_at INTEGER NOT NULL
);
