// Transient content reads are retried (issue #520; crates/atv-content `with_retries`).
// A race reads its CPU roster files one after another, so on a flaky phone
// connection one dropped read used to fail the whole load. Now a network
// error, a 5xx answer or a body shorter than the manifest records is read
// again after 400 ms and then 1500 ms; a 4xx answer is final.
//
// 1. An ENV01 race with three CPUs loads while the first request of a run of
//    content blobs fails transiently (503, aborted, cut-off body). The blobs
//    the load reads are requested again and the race reaches the ride.
// 2. A blob answered 404 fails the load fast: it is not retried (at most the
//    prefetch and the one real read request it).
// CONTENT_RETRY_SHOT=out.png saves the ride after the flaky load. Content group.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const RACE = '/play.html?venue=env01&race=1&laps=1&cpu=3&seed=12345&weather=off&uiDiagnostics=1';
const BLOB = /\/content\/[a-f0-9]{64}\.bin$/;
const FAULTS = ['503', 'truncated', 'abort'];
const INJECTED = 12;

const rideOrError = page =>
  page.waitForFunction(
    () => {
      const ride = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride;
      if (ride?.environment === 'env01') return 'ride';
      const notice = document.querySelector('#menu-notice')?.textContent?.trim();
      if (notice) return `error: ${notice}`;
      const status = document.querySelector('#status')?.textContent || '';
      return /could not load|failed|error/i.test(status) ? `error: ${status}` : false;
    },
    null,
    { timeout: 180000, polling: 250 }
  ).then(handle => handle.jsonValue());

const browser = await launch();
try {
  {
    const errors = [];
    // Block the service worker so every content read reaches the route.
    const page = await newPage(browser, { viewport: { width: 640, height: 360 }, serviceWorkers: 'block' }, errors);
    await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
    const requests = new Map();
    const faults = new Map();
    await page.route(BLOB, async route => {
      const blob = new URL(route.request().url()).pathname;
      const count = (requests.get(blob) ?? 0) + 1;
      requests.set(blob, count);
      if (count === 1 && faults.size < INJECTED) faults.set(blob, FAULTS[faults.size % FAULTS.length]);
      const fault = count === 1 ? faults.get(blob) : undefined;
      if (fault === '503') return route.fulfill({ status: 503, body: 'busy' });
      if (fault === 'abort') return route.abort('connectionreset');
      if (fault === 'truncated') {
        const response = await route.fetch();
        const body = await response.body();
        return route.fulfill({ status: 200, contentType: 'application/octet-stream', body: body.subarray(0, body.length >> 1) });
      }
      return route.continue();
    });
    const start = Date.now();
    await page.goto(url(RACE));
    const outcome = await rideOrError(page);
    assert.equal(outcome, 'ride', `the race loads through transient failures: ${outcome}`);
    assert.equal(faults.size, INJECTED, `faults injected: ${faults.size}`);
    // A prefetched blob the load does not read yet (the course's end-of-race
    // files) is only read again when it is needed; every read one recovered,
    // or the race could not have loaded.
    const reread = [...faults].filter(([blob]) => requests.get(blob) >= 2);
    assert.ok(reread.length >= INJECTED - 2, `failed blobs read again: ${reread.length}/${INJECTED}`);
    for (const fault of FAULTS) {
      assert.ok(reread.filter(([, f]) => f === fault).length >= 2, `${fault} failures were read again`);
    }
    const state = JSON.parse(await page.locator('#ui-state').textContent());
    assert.equal(state.ride?.environment, 'env01');
    if (process.env.CONTENT_RETRY_SHOT) {
      await page.waitForTimeout(1500); // Deliberately wall-clock: the ride settles on screen.
      await page.screenshot({ path: process.env.CONTENT_RETRY_SHOT });
    }
    // The aborted requests' own rejections are expected; nothing else may fail.
    assertNoErrors(errors.filter(e => !/Failed to fetch|ERR_CONNECTION_RESET/.test(e)));
    console.log(
      `  flaky race load: ${INJECTED} blobs failed once (${FAULTS.join(', ')}), ${reread.length} read again during the load; ride after ${Date.now() - start} ms`
    );
    await page.close();
  }
  {
    const errors = [];
    const page = await newPage(browser, { viewport: { width: 640, height: 360 }, serviceWorkers: 'block' }, errors);
    const requests = new Map();
    let missing;
    await page.route(BLOB, route => {
      const blob = new URL(route.request().url()).pathname;
      requests.set(blob, (requests.get(blob) ?? 0) + 1);
        // The first blob the race reads (its course archive) is missing for good.
      missing ??= blob;
      if (blob === missing) return route.fulfill({ status: 404, body: 'missing' });
      return route.continue();
    });
    await page.goto(url(RACE));
    const outcome = await rideOrError(page);
    assert.equal(outcome, 'error: Original game content could not load (404).', `a missing blob fails the load: ${outcome}`);
    const tries = requests.get(missing);
    assert.ok(tries >= 1 && tries <= 2, `a 404 is final, not retried: ${tries} requests`);
    assertNoErrors(errors.filter(e => !/404|could not load/.test(e)));
    console.log(`  missing blob: "${outcome.slice(7)}" after ${tries} request(s)`);
    await page.close();
  }
  console.log('PASS: transient content failures are read again and the race loads; a 404 fails fast');
} finally {
  await browser.close();
}
