# Pass Key cloud saves (server)

Product decision (status.md, 2026-10-28): every player's progress is saved under
an automatically assigned **Pass Key**, a retro save code such as
`FURY-7K3M-9Q2D-XW4H-1BCE`. Entering it on another device or browser continues
that save. A player can also publish a **share code** (`SHARE-…`) that copies a
snapshot of their progress into someone else's save. There are no accounts and no
personal data, and anyone holding a key may use it; that is intended. "Pass Key"
is the product name, not WebAuthn.

This note covers the Worker API and storage. The game client (key generation,
local save, merge, UI) is a separate change.

## Contract

Codes are 16 symbols of Crockford base32 (`0123456789ABCDEFGHJKMNPQRSTVWXYZ`,
80 bits). The client generates Pass Keys with a cryptographic RNG; the server
generates share codes. Both sides normalize the same way: uppercase, strip
whitespace and dashes and the `FURY` (or `SHARE`) prefix, map `O`→`0` and
`I`/`L`→`1`, and reject anything else. A `SHARE` prefix is stripped only when 16
symbols follow it.

All endpoints are same-origin JSON with `Cache-Control: no-store`.

| Request | Success | Notes |
| --- | --- | --- |
| `GET /api/save` + `X-Fury-Pass-Key` | 200 `{revision, updatedAt, save}` | 404 `not_found` |
| `PUT /api/save` + key, `{baseRevision, save}` | 200 `{revision, updatedAt}` | Creates when absent and `baseRevision` is 0. Otherwise `baseRevision` must equal the current revision, or 409 `{error:"conflict", revision, updatedAt, save}` returns the current save so the client merges and retries. An unknown key with a nonzero base gets 409 with `revision: 0, updatedAt: null, save: null`. |
| `POST /api/save/share` + key | 201 `{shareCode, createdAt}` | Immutable snapshot of the current save. 404 if the key has no save. The body is ignored. |
| `GET /api/share` + `X-Fury-Share-Code` | 200 `{createdAt, save}` | Read-only; loading copies it into the loader's own save on the client. |
| `GET /api/save/revisions` + key | 200 `{revisions: [{revision, updatedAt, bytes}]}` | Newest first, at most 10, including the current revision. |
| `POST /api/save/restore` + key, `{revision}` | 200 `{revision, updatedAt, save}` | Copies a kept revision forward as a new revision (undo for a griefed shared key). 404 if that revision is no longer kept. |

Errors: 400 `bad_key` / `bad_body`, 403 `origin_denied`, 404 `not_found`,
405 `method_not_allowed` (with `Allow`), 413 `too_large`, 429
`{error:"rate_limited", retryAfter: 60}` with `Retry-After: 60`, and 503
`unavailable` when storage, a limiter binding or the client IP is missing.
Server-side clarifications of cases the shared contract left open are listed in
ignored `private/reports/cloud-save/contract-changes.md`; the table above is the
authoritative public description.

`save` is opaque to the server. It must be a JSON object with an integer schema
version `v >= 1`, and its UTF-8 `JSON.stringify` form must be at most 64 KiB.
Request bodies over 66 KiB are refused while streaming, before parsing.

Implementation: `cloudflare/cloudsave.mjs` (routes, validation, storage),
`browserOriginAllowed` in `cloudflare/security.mjs`, routing in
`cloudflare/worker.mjs`, tests in `cloudflare/cloudsave.test.mjs`.

## Storage

Cloudflare D1 database `fury-saves` (`d8ee401b-4613-4564-b2b7-309062dee1d2`,
region ENAM), bound as `SAVES` in `wrangler.jsonc`. Schema migrations live in
`cloudflare/migrations/` (`0001_cloud_saves.sql`):

- `keys(id, revision, created_at, updated_at, bytes, save)`: the current save.
- `revisions(key_id, revision, updated_at, bytes, save)`: the last 10 revisions
  of each key, including the current one.
- `shares(id, created_at, bytes, save)`: immutable snapshots.

Row ids are the lowercase hex SHA-256 of the 16 normalized symbols. Raw keys and
share codes are never stored or logged; a share code is shown once at creation
and the client keeps it. Shares do not record which key they came from.

Every write is one D1 batch (a single transaction): the conditional
`INSERT … ON CONFLICT DO NOTHING` (create), `UPDATE … WHERE revision = base`
(write) or `UPDATE … FROM revisions` (restore); a copy of the resulting row into
`revisions`; pruning to the newest 10; and a read of the current row. A change
that matched nothing leaves the table unchanged and is answered as a conflict (or
404 for restore), so concurrent writers cannot both win the same revision.

D1 was chosen by the user. The CI deployment token (Individual Workers Editor for
this Worker plus Account Settings Read) deployed the D1 binding without extra
permissions (PR #385). Schema migrations are **not** applied by CI: that token has
no D1 permission. Apply them from the owner's machine with Wrangler OAuth before
merging code that needs them (see Operations).

## Limits and abuse controls

Native Cloudflare rate-limit bindings, per minute (counters are per Cloudflare
location and eventually consistent, see research/SECURITY.md):

| Binding | Limit | Keyed by | Applies to |
| --- | --- | --- | --- |
| `SAVE_READ_LIMIT` | 120 | IP | GET save, GET share, GET revisions |
| `SAVE_WRITE_LIMIT` | 60 | IP, and separately the key hash | PUT, restore |
| `SAVE_CREATE_LIMIT` | 10 | IP | PUT with `baseRevision: 0` |
| `SAVE_SHARE_LIMIT` | 10 | IP | POST share |

A write every second per key is allowed; the client should still debounce cloud
writes to meaningful progress (D1 bills rows written per account). Saves are
capped at 64 KiB, so one key uses at most about 704 KiB (current row plus 10
revisions). There is no global storage ceiling or expiry yet; shares and keys are
kept indefinitely. Watch the database size in the dashboard (D1 → fury-saves) or
with `wrangler d1 info fury-saves`.

Origins: a request carrying `Origin` must name `https://furyremastered.com` or
`https://atv-offroad-fury-2.champtoday.workers.dev` (`GAME_ORIGINS`); `Origin:
null`, HTTP, lookalike and foreign origins are 403, as are requests with
`Sec-Fetch-Site: cross-site`. Requests without `Origin` (the desktop and Android
apps, which use `ureq`, and curl) are accepted. No CORS headers are sent, so other
sites' pages cannot read responses. The service worker passes `/api/` through
untouched.

## Privacy

A Pass Key is a bearer code, like an old game password: it is not tied to a person,
device, email or IP address. The database holds only code hashes, timestamps,
sizes and the game's save JSON. Client IPs are used only as rate-limit keys at the
edge and are not stored. Worker invocation logs are disabled and the code never
logs request headers or bodies. Anyone with a Pass Key can read and overwrite that
save, and anyone with a share code can copy that snapshot. The public note is in
the dev-log footer (`web/devlog.html`, `#privacy`).

## Operations

All commands run from a checkout with Wrangler OAuth (`npx wrangler whoami`).

Apply new migrations (before merging code that depends on them):

```sh
npx wrangler d1 migrations list fury-saves --remote
npx wrangler d1 migrations apply fury-saves --remote
```

Find a save from a Pass Key (hash the normalized symbols; never paste keys into
tickets or logs):

```sh
KEY='FURY-7K3M-9Q2D-XW4H-1BCE'
ID=$(node -e "const s=process.argv[1].toUpperCase().replace(/[\s-]+/g,'').replace(/^FURY/,'').replace(/O/g,'0').replace(/[IL]/g,'1');console.log(require('crypto').createHash('sha256').update(s).digest('hex'))" "$KEY")
npx wrangler d1 execute fury-saves --remote --command "SELECT revision, updated_at, bytes FROM keys WHERE id = '$ID'"
npx wrangler d1 execute fury-saves --remote --command "SELECT revision, updated_at, bytes FROM revisions WHERE key_id = '$ID' ORDER BY revision DESC"
```

Restore an older revision: the player (or anyone with the key) can call
`POST /api/save/restore` with `{"revision": n}`; an operator can do the same with
curl. To remove a save entirely:

```sh
npx wrangler d1 execute fury-saves --remote --command "DELETE FROM revisions WHERE key_id = '$ID'; DELETE FROM keys WHERE id = '$ID'"
```

Counts and size: `npx wrangler d1 execute fury-saves --remote --command "SELECT
(SELECT COUNT(*) FROM keys) keys, (SELECT COUNT(*) FROM revisions) revisions,
(SELECT COUNT(*) FROM shares) shares"` and `npx wrangler d1 info fury-saves`.
D1 Time Travel can restore the whole database to a point in the last 30 days
(`npx wrangler d1 time-travel info|restore fury-saves`); prefer per-key restore.

## Verification

`node --test cloudflare/*.test.mjs` runs the API against real SQLite with the
production migration (via `node:sqlite`), covering create, read, conflicts,
revision cap, restore, share immutability, normalization, validation, size, rate
limits, storage failure and origin rules.

Live verification (2026-10-28, production commit `7cd4a58`, PR #386) with curl and
a client-style random key against `https://furyremastered.com`: GET before
create 404; create 200 revision 1; write with a lowercase key 200 revision 2; GET
with the bare 16 symbols 200; stale write 409 with the current save; share 201;
a later write did not change the share (GET share returned the revision-2
snapshot); revisions listed 3, 2, 1; restore of revision 1 returned revision 4 with
that save; bad key 400; save without `v` 400; foreign `Origin` 403; the workers.dev
`Origin` 200; a 70 KB save 413. Every response carried `Cache-Control: no-store`.
The SQL was also checked directly on the remote database before the merge.

Rate limits: a keep-alive burst of share requests from one IP was answered 201
twelve times and then 429 with `Retry-After: 60` (limit 10, counters are
eventually consistent). Earlier bursts that opened a new connection per request
(about 40 creates in 25 seconds, limit 10) were not limited at all, so treat the
native limiters as coarse, per-location abuse brakes, not exact quotas. All
verification rows (marked `"test": "server-verification…"`) were deleted
afterwards.
