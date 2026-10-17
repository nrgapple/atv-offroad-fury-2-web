# Offline cache and fast relaunch

`web/sw.js` is a classic service worker at scope `/`. It makes a returning
visit start from local storage and lets the installed app open offline into the
menus and any course whose content it has already loaded. Multiplayer, the
Free Ride rider count, usage analytics, the dev log and the streamed soundtrack
stay online-only.

## Registration

`web/offline.js` (loaded last by `play.html`) registers `/sw.js` after `load`
and ignores failures. It only registers in a packaged release: the module's own
URL must carry `?v=<40-hex commit>`. Unversioned development builds and the local
browser-test server (`tools/serve_web.py` serving `web/dist`) therefore never get
a worker, because their app files have no per-deploy cache key and a cache-first
shell would go stale. The existing browser tests are unaffected.

The registration URL stays `/sw.js`. `tools/package_release.py`'s
`versioned_source` rewrites the quoted `'game.js'` probe inside `sw.js` to
`'game.js?v=<commit>'`; that string names the worker's version (`VERSION`) and
makes the worker's bytes differ on every deploy, so browsers install the new
worker on their normal update check. An unversioned `sw.js` intercepts nothing.

When the page runs as an installed app (`display-mode: standalone`/`fullscreen`
or iOS `navigator.standalone`), it calls `navigator.storage.persist()` once
(best effort, no UI). Browser tabs never ask.

## Caches

| Cache | Holds | Lifetime |
| --- | --- | --- |
| `fury-shell-v1-<commit>` | one deploy's versioned JS/CSS/WASM module graph plus manifest, icons and SVG; marker `/__fury-offline/shell-complete` | deleted on activation unless it is this worker's version or the version named by the cached `/play.html` or `/index.html` |
| `fury-pages-v1` | `/play.html`, `/index.html` (and `/devlog.html` once visited) | replaced by every successful network load whose shell is complete |
| `fury-content-v1` | `/content/<sha256>.bin` blobs the player actually loaded | content-addressed; pruned against the current content manifest |
| `fury-manifests-v1` | `/content/manifest.json` and any pinned `/content/manifests/<commit>.json` | pinned manifests of deleted shells are removed on activation |

Other `fury-*` cache names (older schemas) are deleted on activation.

## Strategies

| Request | Strategy |
| --- | --- |
| Navigations to `/`, `/index.html`, `/play.html`, `/devlog.html` | network-first; a network error, a 5xx, or 4 s without an answer falls back to the cached page (query ignored, `/` → `/index.html`); without a cached page it waits for the network |
| `/<name>.js|css|wasm?v=<40 hex>` | cache-first across all caches; only this worker's own version is written from the network |
| `/content/<sha256>.bin` | cache-first; a network response is written only when it is a 200 basic response whose bytes hash to the file name |
| `/content/manifest.json`, `/content/manifests/<commit>.json` | network-first with cache fallback (same 4 s timeout) |
| `/manifest.webmanifest`, `/favicon.ico`, `/fury-remastered.svg`, `/fury-remastered-{home,maskable}-*-v*.png`, `/apple-touch-icon*.png` | network-first with cache fallback |
| everything else | not intercepted: non-GET, any `Range` request, `/content/*.m4a` streams, `/multiplayer/*` (including WebSocket upgrades), `/api/events`, `/devlog.json`, `/devlog-images/*`, `/deployment.json`, cross-origin (Cloudflare beacon) |

A cache miss while offline answers a plain network error (`Response.error()`),
not a rejected `respondWith`. Only 200, non-redirected, same-origin (`basic`) responses are stored. Opaque,
partial (206), error and redirected responses never are. Storage failures
(quota) are swallowed; play continues from the network.

## Install

Install fetches `/play.html` and `/index.html` with `cache: 'reload'`, requires
both to name exactly this worker's version (otherwise install fails and the
browser picks up the newer worker on its next check), then discovers the shell by
following quoted references: versioned `name.js|css|wasm?v=<commit>` strings (the
exact set packaging rewrites, including transitive imports, `new Worker` and the
WASM fetch) and quoted static icon/manifest paths, recursively through JS, CSS and
the web manifest. Discovery keeps the precache list in step with new modules
without a hand-maintained list. A discovered name that 404s is skipped (a quoted
string that is not a file); any other failure fails install. The content manifest
literal in `content.js` is cached best effort (a release's R2 content may be
published after its code). The completion marker and then the pages are written
last, so a cached page always has a complete shell.

Precache uses the browser HTTP cache, which already holds the page's modules
(the Worker marks the current commit's `?v=` files immutable). Content is not
precached: ~454 MB in ~489 blobs is loaded lazily per venue, so only what the
player loads is kept.

## Update semantics

- No `skipWaiting`. A new worker waits until every page of the old worker has
  closed, then activates. An open game is never reloaded or switched mid-run.
- `clients.claim()` on activation takes control of an uncontrolled first-visit
  page. At that point no page of another worker version can be open.
- HTML is network-first, so an online launch runs the newest deploy immediately,
  even while an older worker still controls. The old worker reads the new
  version's shell from `fury-shell-v1-<new>` (installed by the waiting worker) or
  the network.
- A worker never writes another version's `?v=` from the network. The Worker
  serves any `?v=` with the current bytes (the query is only a cache key), so an
  old URL fetched after a deploy can carry new bytes. That mixed-version risk
  exists without a service worker; this design never makes it permanent.
  `game.js` creates its decoder Worker and fetches the WASM at startup, with no
  lazy module imports, so an already-running page does not request its shell
  later.
- A network page is cached only when its version's shell is complete. A waiting
  worker's install writes the new pages after its shell, and activation keeps any
  shell a cached entry page still names. An offline launch therefore always pairs
  a cached page with its own shell, including in the window between deploys.
- Cached blobs survive deploys (they are content-addressed). About 20 s after a
  successful network content manifest, blobs it no longer references are deleted
  (best effort; skipped for an unusable manifest).

## First visit and Home Screen launch

The page is uncontrolled until the first worker activates, so its early content
requests bypass the worker. `offline.js` observes resource timing from startup,
collects same-origin `/content/` blob and manifest requests with `workerStart`
0, and posts them (`{type: 'fury-warm'}`) to the active worker, which re-fetches
(normally from the HTTP cache), hash-checks and stores them. Message URLs are
validated against the same routes; anything else is ignored. iOS Home Screen web
apps keep storage separate from Safari tabs, so the app's first launch fills its
own cache this way.

## Offline behaviour

Host-verified (see below): launching `/` or `/play.html` offline reaches the
title screen and main menu, and a previously ridden course (env01 Free Ride)
loads and simulates. Startup network failures are non-fatal: the Free Ride rider
count is hidden, analytics beacons are dropped, and Cloudflare's beacon script
fails to load. Soundtrack streams are never cached; offline the music falls back
to the original VAG chunks and, when those are not cached either, stays silent.
`web/music.js` suppresses its error status while `navigator.onLine` is false so
an offline ride shows no audio error. Courses never loaded online are
unavailable offline and fail with the normal content error. A lazy file whose
online download was cut short (for example by leaving the page mid-load) is not
cached; the observed case (`ATV01_MT.RES`) was non-fatal offline. Multiplayer requires
connectivity.

## Storage and eviction (iOS and others)

- Safari's seven-day cap on script-writable storage counts days of Safari use; per
  WebKit's [2020 tracking-prevention post](https://webkit.org/blog/10218/full-third-party-cookie-blocking-and-more/),
  Home Screen web apps have their own counter of days of use and are not expected
  to have first-party data deleted.
- WebKit's [Safari 17 storage policy](https://webkit.org/blog/14403/updates-to-storage-policy/)
  gives browser apps and Home Screen web apps an origin quota of up to 60% of
  total disk, evicts least-recently-used whole origins, and grants `persist()` by
  heuristics "like whether the website is opened as a Home Screen Web App".
  Persistent origins may be protected from eviction. The app requests it only
  when installed; the browser may still refuse.
- Under pressure, browsers evict whole origins (least recently used, best-effort
  storage first). The cache is a performance aid; the network path always works.

These notes come from vendor documentation, not device testing.

## Verification (host only)

- `node --test web/sw.test.mjs web/offline.test.mjs` (in `make check`): routing,
  page versions, discovery, install success/failure, activation retention and
  claim, cache-first shell and blob integrity, network-first pages with offline
  and timeout fallback, manifest caching and pruning, warm-up validation and
  pass-through.
- `tools/test_release.py`: packaging versions the `sw.js` probe and `offline.js`
  while `/sw.js` stays unversioned. `cloudflare/worker.test.mjs`: `/sw.js` is not
  marked immutable.
- `tools/test_offline.mjs` (content group; `node tools/run_browser_tests.mjs --only test_offline.mjs`):
  serves a packaged copy of `web/dist` on its own port, checks that the worker
  controls the page and precached the shell (59 files on 2026-10-17), that
  first-visit and ride content is cached and served by the worker on relaunch
  without re-downloading, that streams, ranged reads, `/multiplayer/` and
  `/api/events` bypass it, then stops the server and relaunches `/` → main menu
  and env01 offline. Passes in headless Chromium (with `context.setOffline`) and
  WebKit (`ATV_UI_BROWSER=webkit`): Playwright's WebKit offline emulation also
  fails worker-served navigations, so WebKit gets the stopped server with
  `navigator.onLine` overridden to false, and its page errors for expected
  offline misses ("Response served by service worker is an error") are ignored.
  Offline misses are listed: soundtrack fallback chunks and, in some runs, one
  lazy file cut short online; neither stopped the ride.

Not verified: physical iOS/Android devices, Home Screen installation, real
eviction and `persist()` outcomes, the production Cloudflare Worker serving
`/sw.js`, and update behaviour across real deploys.
