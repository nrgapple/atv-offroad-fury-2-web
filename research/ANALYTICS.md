# Usage analytics

Fury Remastered has two layers of anonymous, cookie-free analytics on Cloudflare.

| Layer | What it answers | Where to look |
| --- | --- | --- |
| Workers Analytics Engine (`fury_usage`) | How people play: sessions, single player vs multiplayer, Freeride vs Race, venues, vehicles, riding time, race results, load times and load failures | `python3 tools/analytics_report.py`, or SQL in the Cloudflare API |
| Cloudflare Web Analytics (optional) | Page views for the landing page, game and dev log; referrers; countries; browsers; Core Web Vitals | Dashboard → Analytics & Logs → Web Analytics |

Built-in zone traffic analytics for `furyremastered.com` and Worker metrics/logs (`observability` in `wrangler.jsonc`) need no setup.

## Privacy rules

- Events contain only the enumerated fields below. Unknown fields and values are dropped by `normalizeEvent` in `web/analytics.js`. The Worker runs the same validation again.
- Events never contain cookies, client or session identifiers, player names, room codes, invite links, URLs, error messages or free text. The Worker adds a two-letter country from Cloudflare (`request.cf.country`) and the release commit. The IP address keys only the in-memory `EVENT_LIMIT` rate limiter (60 events per minute) and is never written.
- Nothing is sent from automated browsers (`navigator.webdriver`), from local or non-HTTPS hosts, or when the visitor sends Do Not Track or Global Privacy Control.
- Analytics is best effort. Missing bindings, rate limits and write errors return `204` and drop the event, so play is never blocked.

## Events

The client posts one JSON event to `POST /api/events` (via `sendBeacon`, with a `keepalive` fetch fallback). The request must come from a `GAME_ORIGINS` origin, or carry `Sec-Fetch-Site: same-origin` when the browser omits `Origin`. Bodies are limited to 1 KiB.

| Event | Sent when | Fields |
| --- | --- | --- |
| `session_start` | The hosted game page starts | `entry` (`link` = opened with launch parameters, `menu`) |
| `ride_start` | A ride finishes loading | `mode`, `activity`, `map`, `vehicle`, `quad`, `seconds` (load time); races add `laps` and `cpu`; multiplayer and Arlington Free Ride add `players`. `mode` is `single`, `multiplayer` or `freeride` |
| `ride_failed` | A ride fails to load | `mode`, `activity`, `map`, `vehicle`, `quad` |
| `ride_time` | When a menu opens (≥ 10 s), the page is hidden, the page closes, the rider leaves a lobby, or the next ride starts (≥ 1 s) | ride fields + `seconds` of visible riding outside menus since the last report; sum these values for total riding time |
| `race_finish` | The Race Results screen first appears | ride fields + `place`, `field`, `seconds` (race time) |

Every event also includes `device` (`touch` when the primary pointer is coarse, otherwise `desktop`) and `display` (`app` for a Home Screen or standalone launch, otherwise `browser`).

### Column layout

The layout is fixed in `dataPoint()`. Append new columns rather than reordering them.

| Column | Field | Column | Field |
| --- | --- | --- | --- |
| `index1`, `blob1` | event name | `double1` | `quad` (1–46; 27+ are motorcycles) |
| `blob2` | country | `double2` | `seconds` |
| `blob3` | release (first 12 commit characters) | `double3` | `place` |
| `blob4` | `device` | `double4` | `field` |
| `blob5` | `display` | `double5` | `laps` |
| `blob6` | `entry` | `double6` | `cpu` |
| `blob7` | `mode` | `double7` | `players` |
| `blob8` | `activity` | | |
| `blob9` | `map` (launch IDs such as `env01`, `nat03`, `sx12`) | | |
| `blob10` | `vehicle` | | |

Missing fields are `''` or `0`. Analytics Engine may sample at high volume, so weight results with `_sample_interval`: use `SUM(_sample_interval)` for counts and `SUM(_sample_interval * double2)` for totals.

## Reading the data

1. Create a Cloudflare API token with **Account → Account Analytics → Read** for the account in `wrangler.jsonc`.
2. Run `CLOUDFLARE_ANALYTICS_TOKEN=… python3 tools/analytics_report.py --days 7`. Use `--json` for raw rows or `--sql` to print the queries.

The report covers daily activity, sessions by device and entry, rides by mode, riding minutes by venue, top vehicles, race finishes, load failures, online lobby size, countries and releases. Data appears a minute or two after events are written. Analytics Engine keeps data for three months.

## Enabling Cloudflare Web Analytics

1. In the Cloudflare dashboard, open **Analytics & Logs → Web Analytics → Add a site**. Enter `furyremastered.com` and choose the manual JavaScript snippet; do not use automatic injection.
2. Copy the `token` value from the snippet. It is a public site tag, not a secret.
3. Add `"WEB_ANALYTICS_TOKEN": "<token>"` to `vars` in `wrangler.jsonc` and merge the change so CI deploys it.

With a valid 32-character hex token, the Worker adds the beacon to HTML responses and allows `https://static.cloudflareinsights.com` (script) and `https://cloudflareinsights.com` (connect) in the CSP. Without a token, the HTML and CSP stay unchanged. Automatic injection would be blocked by the CSP, so it is not used.

## Public stats page

`/stats.html` shows players anonymous totals: riders online and rooms live right now, the most riders online at once, today's daily event, hours ridden, rides, race finishes and visits (today, this week, all time), rides per day for the last 30 days, the most ridden tracks, the single player / multiplayer / Free Ride, Race / Freestyle, ATV / motorcycle and phone / computer splits, World board totals and the countries riders come from. `web/stats.js` renders `GET /api/stats` (`cloudflare/stats.mjs`).

### Data flow

- **Rollup.** After the game server health check, the five-minute cron runs `refreshStats`. At most every 15 minutes it queries `fury_usage` through the Analytics Engine SQL API for whole UTC days (yesterday and today; a first run backfills the 90 retained days, 14 days a run) and writes one row per day to the D1 table `stats_days` (migration `0005_stats_days.sql`). Each row holds aggregate counts only: sessions, rides, riding seconds, race finishes, rides and seconds by mode, activity, vehicle and venue, sessions by device and country, that day's peak riders and rooms online (from `game_server_checks`) and, on today's row, a count snapshot of the World boards. Rows are never pruned, so all-time totals keep growing after Analytics Engine drops old events.
- **Token.** The Worker reads Analytics Engine with the Worker secret `ANALYTICS_API_TOKEN` (Account → Account Analytics → Read). It is never logged; a failed query logs only its HTTP status. Without the secret the cron still records peaks and board counts.
- **Endpoint.** `GET /api/stats` sums the rows, adds live riders and rooms from the game server's `/healthz` (3 s timeout; falls back to a health check from the last 15 minutes) and answers with `Cache-Control: public, max-age=60`, also kept in the edge cache. One request costs one D1 batch and one `/healthz` fetch a minute per edge location.

### Response

| Field | Content |
| --- | --- |
| `updated` | When the answer was built |
| `live` | `{riders, rooms}` or `null` |
| `peak` | `{riders, day, today}`: the most riders online at once and its UTC day, or `null` |
| `daily` | Today's daily event: `{course, vehicle, entries}` (`entries` is `null` before today's rollup) |
| `boards` | `{times, riders, boards, dailyEntries}`: World board rows and distinct names ranked, or `null` |
| `since` | First UTC day with recorded riding |
| `totals` | `today`, `week` (last 7 UTC days) and `allTime`, each `{rides, hours, sessions, racesFinished}` |
| `modes`, `activities`, `vehicles` | `[{id, rides, hours}]`, or `null` below 10 rides |
| `devices` | `[{id, sessions}]`, or `null` below 10 sessions |
| `venues` | Up to 12 `{id, name, rides, hours, racesFinished}` with at least 10 rides, by hours |
| `countries` | `{count, top}`: how many countries visited and up to 12 `{code, sessions}` with at least 10 sessions |
| `days` | Last 30 UTC days: `[{day, rides, hours}]` |

Everything after `since` is absent until Analytics Engine data has been rolled up; the page then hides those sections and still shows the live numbers, peak, daily event and boards. If the endpoint fails, the page shows a short note.

### Privacy

The answer contains aggregates only: no rider names (board totals are counts), IP addresses, room codes, identifiers, release or build commits, or details about the multiplayer service beyond riders and rooms. Venues and countries below 10 rides or sessions (`MIN_BUCKET`) are not listed; the country count still includes them. `cloudflare/stats.test.mjs` checks the suppression, the missing-token and missing-database paths and that no forbidden field appears. `tools/test_stats.mjs` (content-free browser group) covers the page with full data, without the token and with the endpoint down.
