-- World board ghosts (research/WORLD_BOARDS.md "Ghosts"): the recorded lap of
-- a lap board's ten best rows, at most one per rider name per board. `value`
-- is the lap time the ghost was ridden in (the board row's value while the
-- ghost is kept); `data` is the ghost's bytes as base64 text, at most 64 KiB
-- decoded. No IP addresses or device identifiers are stored.
CREATE TABLE IF NOT EXISTS board_ghosts (
  board TEXT NOT NULL,
  name TEXT NOT NULL,
  value INTEGER NOT NULL,
  data TEXT NOT NULL CHECK (length(data) <= 87384),
  PRIMARY KEY (board, name)
);
