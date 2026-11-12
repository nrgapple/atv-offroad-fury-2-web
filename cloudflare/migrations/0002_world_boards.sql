-- World Hall of Fame boards and the daily event (research/WORLD_BOARDS.md).
-- One row per rider name per board, the name's best. `value` is lower-is-
-- better: milliseconds for times, the negated score for Freestyle. No IP
-- addresses or device identifiers are stored.
CREATE TABLE IF NOT EXISTS board_entries (
  board TEXT NOT NULL,
  name TEXT NOT NULL,
  value INTEGER NOT NULL,
  vehicle INTEGER NOT NULL,
  day TEXT NOT NULL,
  PRIMARY KEY (board, name)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS board_rank ON board_entries (board, value);
