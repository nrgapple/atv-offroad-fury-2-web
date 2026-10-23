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
export PLAYWRIGHT_MODULE=~/LargeFiles/atv-browser-tests/node_modules/playwright
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
server instead (for example a relay-capable host or the public site), `--bail`
stops at the first failure.

A test can still be run directly: `ATV_TEST_URL=http://127.0.0.1:4173 node tools/test_race.mjs`
against `make serve`.

## Groups

| Group | Needs | Runs in |
| --- | --- | --- |
| `content-free` | `web/dist` only; served with an **empty** content directory | CI build lane, `make browser-smoke` |
| `content` | ignored `private/reports/hosted-content` package | `make browser-tests` |
| `native` | content plus private original-game reports named by `ATV_*_NATIVE` | only when requested |
| `relay` | content plus a same-origin multiplayer relay (`--url`) | only when requested |
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
and the JavaScript guests in the `test_engine_multiplayer*` relay tests) load it from
`ATV_JS_URL` (`jsBase`/`jsUrl` in `tools/browser/harness.mjs`), which defaults to
`ATV_TEST_URL`. For relay tests, serve the reference behind its own relay proxy:
`PORT=4188 ASSET_PORT=4178 node tools/serve_multiplayer.mjs`.

Native comparisons (`test_fog`, `test_reflections`, `test_vegetation_billboards`,
`test_vegetation_materials`) load their report with `nativeData()`. Run directly
without the variable, they fail with a message naming it. The content group sets
`ATV_NATIVE=skip`, which runs only their content checks and prints that the
comparison was skipped. The `native` group never sets it; it fills unset
variables from the manifest's default paths under ignored
`private/reports/map-fidelity/` when those files exist, and otherwise fails.

Relay tests need the game and a multiplayer relay on one origin. Locally:
`make relay`; `wrangler@4.149.0 dev --config relay/wrangler.jsonc --port 8789 --local
--var GAME_ORIGIN:http://127.0.0.1:4187` (with `~/.cargo/bin` on `PATH`);
`python3 tools/serve_web.py --port 4177`; `node tools/serve_multiplayer.mjs`; then
`node tools/run_browser_tests.mjs --group relay --url http://127.0.0.1:4187`
(`ATV_ALLOW_HOST_WIP=1` lets `test_multiplayer_four` accept a host without
`/deployment.json`; `ATV_MP_REOPEN=0` skips the 100 s room-expiry step).

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
