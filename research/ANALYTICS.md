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
| `ride_start` | A ride finishes loading | `mode`, `activity`, `map`, `vehicle`, `quad`, `seconds` (load time); races add `laps` and `cpu`; multiplayer adds `players` |
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
