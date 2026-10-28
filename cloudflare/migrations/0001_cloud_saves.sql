-- Pass Key cloud saves (research/CLOUD_SAVE.md). Row ids are the lowercase hex
-- SHA-256 of a normalized Pass Key or share code; raw codes are never stored.
CREATE TABLE IF NOT EXISTS keys (
  id TEXT PRIMARY KEY NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 1),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  save TEXT NOT NULL
);

-- The last 10 revisions of each key, including the current one.
CREATE TABLE IF NOT EXISTS revisions (
  key_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  updated_at TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  save TEXT NOT NULL,
  PRIMARY KEY (key_id, revision)
);

-- Immutable snapshots named by server-generated share codes.
CREATE TABLE IF NOT EXISTS shares (
  id TEXT PRIMARY KEY NOT NULL,
  created_at TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  save TEXT NOT NULL
);
