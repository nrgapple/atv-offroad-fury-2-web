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
| `GET /api/save/revisions` + key | 200 `{revisions: [{revision, updatedAt, bytes}]}` | Newest first, at most 10, including the current revision. With `X-Fury-Revision: n` (a positive integer, no leading zero; else 400 `bad_revision`): 200 `{revision, updatedAt, save}` for that kept revision, 404 if it is not kept. Save History uses it to show a revision before restoring it. |
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

D1 was chosen by the user. The CI deployment token has Individual Workers Editor
for this Worker, Account Settings Read, and account-scoped D1 Write (the dashboard's
D1 Edit option). GitHub Actions applies pending migrations before deploying the
Worker, inside the serialized production job and after its stale-build gate.
It uses the root `wrangler.jsonc` so `cloudflare/migrations` resolves correctly;
the generated deployment config lives in a different directory. Migration failure
stops deployment. PR jobs never receive this token or run remote migrations.

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

Add schema changes as new numbered SQL files in `cloudflare/migrations/`; do not
rewrite applied migrations. Merge them with the code that needs them. The trusted
main deployment runs `wrangler d1 migrations apply SAVES --remote` automatically,
using the existing GitHub `CLOUDFLARE_API_TOKEN` secret. Wrangler tracks applied
files, so retries and deployments without new SQL skip them. Pure documentation
changes do not deploy or migrate.

Keep migrations compatible with the currently running Worker: the schema changes
first, and a later deployment failure leaves the previous Worker serving it.
For destructive changes, use separate additive, code-transition and cleanup
releases. A failed migration rolls back that migration, not earlier successful
ones; inspect the failure and fix forward before retrying the deployment.

For manual inspection or recovery on the owner's machine, use a checkout with
Wrangler OAuth (`npx wrangler whoami`). The equivalent commands are:

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

# Pass Key cloud saves (game client)

Authored for the port (the original kept profiles on a memory card). Code:
`crates/atv-core/src/cloud_save.rs` (codes, payload, merge),
`crates/atv-ui/src/cloud.rs` (stored key, sync state machine, apply choices),
`crates/atv-ui/src/front/controller/passkey_page.rs` (pages, badge state),
`crates/atv-app/src/cloud.rs` (HTTP transport, triggers, files, links).

## Keys, storage and payload

- The client makes a Pass Key from 10 bytes of the platform's cryptographic
  random source (`crypto.getRandomValues`; `getrandom` on desktop and Android)
  the first time the local save holds progress: anything beyond a new game's
  single empty PLAYER1 (points, races, unlocks, records, a named profile, Hall
  of Fame entries). Nothing is sent before that.
- Local storage stays the source of truth. New keys: `fury-pass-key` (the key)
  and `fury-cloud` (`{revision, updatedAt, base, shareCode, previousKey}`: the
  server revision and save last agreed on, the last share code made here and the
  key used before entering another). Desktop keeps them in its `settings.json`
  like every other setting.
- `save` payload v1 (JSON): `{"v": 1, "profiles": {"current": n|null,
  "profiles": [Profile…]}, "hallOfFame": {"courses": {…}, "freestyle": {…}}}`.
  `Profile` and `HallOfFame` are the same serde shapes the game already stores
  under `fury-profiles` and `fury-hall-of-fame` (points, earned, races, wins,
  per-course records, Freestyle records, championships and trophies, Profile
  Store purchases and unlocks and cheats). Device settings (graphics, controls,
  audio, minimap, garage choice, multiplayer name) never travel.
- Size: a completed save with a full Hall of Fame is about 90–150 KB as plain
  JSON, over the 64 KiB cap. Saves whose JSON exceeds 48 KiB travel as
  `{"v": 1, "z": "<base64 of raw-DEFLATE of the plain JSON>"}` (the server
  treats `save` as opaque); a synthetic four-profile, 120-course save deflates
  to well under 32 KiB. Readers accept both forms; a save with `v` above 1 is
  refused rather than merged ("update the game"). Export files are always plain,
  pretty JSON.

## Merge rules (never silently drop progress)

`cloud_save::merge(local, remote, base)` has two modes:

- **Rebase** (a 409 conflict or a pull, with `base` = the last agreed save):
  this device's changes since `base` are replayed onto the server save.
  Counters add both sides' gains, `remote + (local − base)`: Profile Points
  (spendable, floored at 0), points earned, points spent in the store, races,
  wins and per-ATV / per-course win and race counts. So points earned on two
  devices both count, and two purchases made from the same balance are both
  paid for (the balance can bottom out at 0; the second item is then kept free
  rather than taken away).
- **Union** (no base: Enter Pass Key → Merge, Import Save → Merge, share codes):
  idempotent. Counters keep the larger side, so merging the same save twice, or
  loading one share code many times, mints no points.

In both modes: best lap and race times keep the minimum; top-three lists
(profile course records and the Hall of Fame's laps, races, objective and stunt
scores) keep the best three distinct entries; jumps, combos and stunt scores
keep the maximum; purchases, gear, course and event unlocks, cheats, opened
series and objective bits are unions; a championship's standing keeps the side
that changed since `base` (else the further round) and the best trophy. Profiles
match by name (case-insensitive). A profile deleted on one side is dropped only
when the other side did not change it since `base`; a merge keeps at most eight
profiles (`MERGE_SLOTS`), so two devices with four profiles each keep all of
them and the Profile Editor refuses new ones until some are deleted. The loaded
profile stays this device's choice.

## Sync

`atv_ui::cloud::Sync` is transport-free (unit-tested with an in-memory contract
server): the app hands it the time and runs the requests it returns.

- Startup and focus/visibility (at most every 30 s) pull `GET /api/save`; a
  newer revision is rebased into local progress and the menus reload.
- Local progress is compared with `base` on every page change and every 10 s;
  a difference is pushed (`PUT`, `baseRevision` = the stored revision). Writes
  are coalesced to one per 5 s; hiding or leaving the app flushes at once.
- 409 with a save: rebase and push again. 409 with `save: null` or a 404 pull
  (the server lost or never had it): push again from revision 0.
- Network failure, 429 or 5xx: exponential backoff from 2 s to 5 min (or
  `Retry-After`); status "Offline · saved on this device". Focus retries at once.
- 413 or 400: that save is not sent again until it changes; the status shows why.
- An answer for a key the player has since replaced is ignored.

## UI

- **Badge**: a dark pill like Back with a status dot (green saved, gold saving
  or no key, orange offline/failed), "PASS KEY · SAVED" and the key (first and
  last groups under 380 px wide; "Enter or create" before a key exists). Title:
  top-right. Main menu, Profile Editor and View Profile: bottom-left, level with
  Back, in landscape; portrait menus run their rows into that corner, so phones
  show it on the title and the Pass Key pages only. Never on the race HUD.
- **Pass Key page** (badge, or Profile Editor → Pass Key & Saves): the key,
  status, last share code and previous key on the panel; rows Copy Pass Key,
  Share Progress, Show QR Code (a `#key=` link drawn as a vector QR code),
  Enter Pass Key, Enter Share Code, Save Files (Export Save / Import Save) and,
  once the device has a key, Save History.
- **Save History**: lists the key's kept revisions (`GET /api/save/revisions`)
  as `#n <date, time> UTC` rows, the newest (current) one on the panel. Choosing
  a row fetches that revision and shows the usual confirmation table with
  Restore This Save / Cancel; Restore calls `POST /api/save/restore`, writes the
  returned save over this device's and records it as the agreed base, so nothing
  is pushed back over it and other devices on the key take it on their next
  pull. The confirmation warns when this device has progress not yet online.
  The save it replaces stays in the history, so a restore can itself be undone.
  Restores are never applied locally without the server (an offline restore
  shows the connection message), so devices cannot disagree about it.
- **Entry**: a real text field (typing, pasting a key or a whole link; O→0,
  I/L→1) plus the controller's Add Symbol row (Left/Right steps the Crockford
  symbols, accept adds), Delete Symbol and Look Up.
- **Confirmation**: the fetched save's profiles, percent complete, points and
  Hall of Fame records. A key or file offers Merge With This Device / Replace
  This Device's Save (just Load This Save when this device has no progress);
  replacing warns when this device has progress not yet saved online. A share
  code offers Copy Into <loaded profile> or Add As New Profile; the player keeps
  their own key. Loading a key keeps the old one as Previous Pass Key.
- **Share Progress** pushes pending progress, makes a share code and offers
  `…/play.html#share=SHARE-…` through the Web Share sheet where the browser has
  one, else the clipboard, else on screen.
- **Links**: `play.html#key=FURY-…` and `#share=SHARE-…` (fragments stay out of
  server logs) look the code up as the menus open and show the confirmation;
  the fragment is removed from the address.
- **Files**: web Export downloads `fury-save-YYYY-MM-DD-HHMMSS.json`, Import uses
  the browser file picker; desktop writes to and imports the newest file from
  `Documents/Fury Remastered` (`FURY_SAVES_DIR` overrides). Android has no save
  files; its Pass Key moves the save.
- Persistent storage: `web/offline.js` asks for it in any tab that holds a
  profile or key (not only installed apps), and the game asks once a key exists.
- Web settings now read saved keys back from `localStorage` rather than a
  per-instance copy, so a merge written by the sync is never overwritten by a
  stale menu or ride instance.

## Client verification

- Unit tests: `cargo test -p atv-core cloud_save` (format, normalization,
  payload and deflate round trip, rebase and union merges, double spend,
  deletions, Hall of Fame top three, new-profile names) and `cargo test -p
  atv-ui cloud passkey` (sync against an in-memory contract server: first key,
  another device, conflict, offline backoff, debounce and flush, share import,
  merge on entering a key, too large, links, export/import; the pages' rows,
  entry, confirmation and links).
- Browser: `tools/test_engine_passkey.mjs` (content group). Against a mocked API
  by default; `ATV_PASSKEY_LIVE=1 ATV_TEST_URL=https://furyremastered.com` runs it
  on the deployed site, and `ATV_PASSKEY_API=https://furyremastered.com` forwards
  a local build's `/api/` calls to the real server (no `Origin`, like the desktop
  app).
- Production (2026-10-28, deployed commit `63b3a3a`, host Chromium): the live run
  passed: a context with progress got a Pass Key and synced it, a fresh context
  typed the key and loaded the same profile, a share code imported as a new
  profile in a third context, an exported file imported on a 390×844 phone
  context, and a `#key=` link opened the confirmation. The test rows it created
  (ACE, 340 points) were left on the server.
- Not verified: physical phones (Web Share sheet, iOS Safari clipboard and file
  picker after an asynchronous share), desktop and Android builds against the
  live API (the native transport compiles in CI but was not run end to end),
  controller-initiated Import on the web (a gamepad press is not a user
  activation, so browsers may refuse the file picker).
