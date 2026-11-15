-- Friend ghosts (research/GHOST_CHALLENGES.md "Friend ghosts"): each online
-- rider's personal best lap ghost on a lap board, stored under the rider's
-- ghost tag (32 hex digits of SHA-256 of `fury-ghost-tag:` and the player
-- id, which the Worker derives from the uploader's id and never stores).
-- Friends find it by tag through the game server's friend list. At most 32
-- a board, the most recently uploaded. `data` is the ghost's bytes as
-- base64 text, at most 64 KiB decoded. No IP addresses or device
-- identifiers are stored.
CREATE TABLE IF NOT EXISTS friend_ghosts (
  board TEXT NOT NULL,
  tag TEXT NOT NULL,
  name TEXT NOT NULL,
  value INTEGER NOT NULL,
  vehicle INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  data TEXT NOT NULL CHECK (length(data) <= 87384),
  PRIMARY KEY (board, tag)
);
CREATE INDEX IF NOT EXISTS friend_ghosts_age ON friend_ghosts (board, updated_at);
