# Browser tests

The `tools/test_*.mjs` files are Playwright checks of the real browser build. They
share `tools/browser/harness.mjs`, are classified in `tools/browser/manifest.mjs`,
and run through `tools/run_browser_tests.mjs`. `make check` fails when a test file
is missing from the manifest (`tools/browser/manifest.test.mjs`).

## Running

Install Playwright once outside the repository (no Node dependencies are committed)
and point `PLAYWRIGHT_MODULE` at it. The CI pin is `playwright@1.58.2`:

```sh
npm install --prefix ~/LargeFiles/atv-browser-tests playwright@1.58.2
~/LargeFiles/atv-browser-tests/node_modules/.bin/playwright install chromium
export PLAYWRIGHT_MODULE=~/LargeFiles/atv-browser-tests/node_modules/playwright  # optional: the runner and harness default to this path when it exists
make web
make browser-smoke     # content-free group (also runs in CI)
make browser-tests     # content-free + content groups (needs private hosted content)
make browser-native    # native comparisons (needs ATV_*_NATIVE reports)
make browser-tests BROWSER_ARGS='--only test_race.mjs,test_fog.mjs --verbose'
node tools/run_browser_tests.mjs --list
```

The runner starts `tools/serve_web.py` on a free host-only port, sets
`ATV_TEST_URL`, runs the selected tests one at a time, writes each log to
`work/verification/browser-tests/<test>.log` and prints a per-group summary. It
exits non-zero if any selected test fails. `--url <site>` targets an existing
server instead (for example the local multiplayer proxy or the public site), `--bail`
stops at the first failure.

A test can still be run directly: `ATV_TEST_URL=http://127.0.0.1:4173 node tools/test_race.mjs`
against `make serve`.

## Groups

| Group | Needs | Runs in |
| --- | --- | --- |
| `content-free` | `web/dist` only; served with an **empty** content directory | CI build lane, `make browser-smoke` |
| `content` | ignored `private/reports/hosted-content` package | `make browser-tests` |
| `native` | content plus private original-game reports named by `ATV_*_NATIVE` | only when requested |
| `multiplayer` | content plus a local game server at the same origin (`--url`) | only when requested |
| `manual` | untracked baselines, a second server, an ISO or other tooling | by name, see the manifest reason |
| `broken` | kept for reference; the manifest records why it fails | by name |

Every game page (title screen included) loads the private content manifest, so
only checks that never start the game are content-free: the engine page's start-up
without content, app identity/icons and the dev log.

After the switch-over play.html is the Rust engine. The JavaScript game's own
browser tests (about 100 `tools/test_*.mjs`) were removed with it; they run from the
pinned pre-switch build: `python3 tools/js_reference.py prepare`, then
`python3 tools/js_reference.py test --group content` (its own runner and server).
Tools that drive the JavaScript game next to the Rust engine (`compare_engine.mjs`, `compare_audio.mjs`,
and the JavaScript guests in the `test_engine_multiplayer*` tests) load it from
`ATV_JS_URL` (`jsBase`/`jsUrl` in `tools/browser/harness.mjs`), which defaults to
`ATV_TEST_URL`. For multiplayer tests, serve the reference behind its own proxy:
`PORT=4188 ASSET_PORT=4178 node tools/serve_multiplayer.mjs`.

Native comparisons (`test_fog`, `test_reflections`, `test_vegetation_billboards`,
`test_vegetation_materials`) load their report with `nativeData()`. Run directly
without the variable, they fail with a message naming it. The content group sets
`ATV_NATIVE=skip`, which runs only their content checks and prints that the
comparison was skipped. The `native` group never sets it; it fills unset
variables from the manifest's default paths under ignored
`private/reports/map-fidelity/` when those files exist, and otherwise fails.

Multiplayer tests need the game and the game server (fury-server,
research/GAME_SERVER.md) on one origin. Locally: `make server-run` serves
fury-server on 8789 with the loopback test origins allowed;
`python3 tools/serve_web.py --port 4177`; `node tools/serve_multiplayer.mjs`
(the page on 4187, `/multiplayer/` passed to 8789); then
`node tools/run_browser_tests.mjs --group multiplayer --url http://127.0.0.1:4187`
(`ATV_ALLOW_HOST_WIP=1` lets `test_multiplayer_four` accept a host without
`/deployment.json`; `ATV_MP_REOPEN=0` skips the 100 s room-expiry step). A page
served from a loopback origin uses its own origin for multiplayer; any other
page uses `https://mp.furyremastered.com` unless `?server=<origin>` names
another (desktop: `FURY_SERVER`).

fury-server claims rooms without direct links or batches, so every rider's
poses go through it. `test_engine_multiplayer_links.mjs` (direct WebRTC links)
is `broken` since the Cloudflare relay, the only server that offered links, was
retired on 2026-11-14.

`broken` entries record why they currently fail; every one also failed on unmodified
`main` when the harness was introduced. Fix the test (or the game) and move it back
to `content`. `defaults` lists user-facing defaults a test relies on (crash mode,
crash camera, camera shake, follow camera); `--list` prints them.

To keep tests readable, format edited tests with prettier 3.6.2:
`prettier --no-config --print-width 120 --single-quote --arrow-parens avoid
--trailing-comma none --write tools/test_<name>.mjs`. The manifest check rejects
lines over 500 characters.

## Harness

`tools/browser/harness.mjs` provides:

- `playwright()`, `launch({engine, args})`: `PLAYWRIGHT_MODULE` resolution and a
  headless browser. Chromium gets a WebGL 2 backend from `ATV_BROWSER_GPU`
  (`metal` default on macOS, `swiftshader` elsewhere, `none`); without one headless
  Chromium has no WebGL 2 and the game cannot start.
- `base`, `url(path)`: `ATV_TEST_URL`, default `http://127.0.0.1:4173`.
- `newPage`, `watchErrors`, `assertNoErrors`: page-exception (optionally console
  error) collection that fails the test.
- `unlockStore(page)`, `UNLOCKED_PROFILE`: seeds a saved profile with the
  original unlock cheats (`PROFILE_STORE.md`) so menu tests that pick ATVs,
  gear or courses are not changed by Single Player gating; a test that saves
  its own `fury-profile` keeps it.
- `waitForGame`: waits for `window.atvTerrainStatus.ready` or the loading error,
  failing immediately with the status text. `gameState`, `waitForScreen`,
  `button`, `waitForAnimations`, `nextFrames`.
- `waitForSteps`, `holdKey`: wait on the fixed 60 Hz `simulationSteps` counter
  instead of wall-clock sleeps. Remaining `waitForTimeout` calls are commented as
  deliberately time-based (for example proving nothing changes while paused).
- `optionalScreenshot`, `writeJson`, `nativeData`.

Screenshot and report outputs stay opt-in through each test's existing
`ATV_*_SCREENSHOT(S)`/`ATV_*_REPORT` variables and belong in ignored `work/` or
`private/` paths.

## Failures traced on 2026-11-12 (issue #575)

Four content tests failed on main and against production. None was a game
regression; each test had outlived a deliberate change or relied on timing.
Facts are from reading the code and from runs against production
(`ATV_TEST_URL=https://furyremastered.com`, commit 8dd53ab) and a local
build of the same commit.

- `test_engine_menu_parity.mjs` (garage yaw 0.497 for 0.5). The showroom
  turntable (24f4409, 2026-10-28) turns the garage ATV at 0.45 rad/s. The
  test read the yaw, clicked the preview and then pressed the arrows, so
  the yaw moved on by a few milliradians between the read and the first
  press. A key turn stops the turntable and holds it for 2.5 s
  (`front.rs` `SPIN_HOLD`), so the test now reads the yaw after the first
  press and measures the next two (0.25 rad each, `modelSpin` 0).
- `test_engine_ride_songs.mjs` (history 3 for 4). `data-engine-audio` is
  written only by a ride's sounds, which load after the ride's first frame
  (cc56027), so the previous ride's last report stays on the page until
  the next ride's sounds arrive. The third ride's wait compared the title
  with the Jukebox song, which the stale report (the second ride's song,
  playing) satisfied at once; the saved history was then read before the
  ride picked its song. The hole dates from the test's first version
  (e1ac140); which change moved the timing far enough to lose the race was
  not identified. The test now removes the attribute before each ride and
  waits for the Jukebox title (it trails `playing` by a frame or more).
- `test_engine_soundtrack.mjs` (`musicQueued` about 2 s for 0). The content
  reader tries a refused read twice more, 0.4 s and 1.5 s later (ea7e27f,
  2026-11-10, `atv-content` `RETRY_DELAYS_MS`). The test restored the
  network 1.5 s after the fallback began, so the third try succeeded. It
  now waits for the third refusal of one blob, checks nothing is queued,
  restores the network and checks no song blob is read for 2.5 s. Against
  a release build the page's service worker reads the blobs itself, past
  `page.route`, which is why production failed one assertion earlier; that
  page now blocks service workers.
- `test_engine_user_course.mjs` ("dropped 3 gates", intermittent).
  Straight on from the env05 start stands a tree. The test drove 150
  rendered frames between drops, which is more simulated time on a busy
  host; the quad then reached the tree at speed, wrecked, was put back in
  front of it by wreck recovery and wrecked again on every following
  drive, never 20 ft past its last gate (three runs in three against
  production with the host at load 8-10; the reset timer read 1.23 s after
  every drive). The editor was right to refuse those drops. The test now
  rides a circle beside the start (throttle and left steer held) and drops
  a gate each second.

Limits. Against production the soundtrack test's last block (the race hold)
still fails with a cold cache: the sounds arrive after the 3.5 s countdown,
so there is no hold left to observe. It passes on a local server. The
circle in the user course test was checked on env05 only.

## Use the PR build without compiling locally

After a successful draft PR `Check and build` run, run
`python3 tools/fetch_pr_build.py --run <run-id>`. It verifies that the downloaded
merge-tree artifact matches clean local source, including the PR head parent.
Use the returned static directory with
`make browser-tests BROWSER_ARGS='--root <path> --only <affected-test.mjs>'`.
The harness starts its usual host-only server and reads private resources locally.
This leaves `web/dist` untouched and needs no local Rust compiler slot. Re-fetch
after source changes. Existing `--url` takes precedence over `--root`; multiplayer
still needs its documented game-server setup. These are host checks, not remotely
reachable previews or physical-device measurements.

Packaged builds enable service workers. Tests that simulate network faults with
`page.route` must use `serviceWorkers: 'block'` in those browser contexts so
worker fetches cannot bypass the interception (as in `test_engine_boot.mjs`).
Keep service workers enabled in tests specifically covering offline/update behavior.
