// Instant second loads (issue #452): a course loaded before opens from the
// persistent decoded-content cache (crates/atv-content/src/decoded.rs).
//
// One browser context loads ENV01 three ways: cold (decodes and stores its
// terrain layers, sky tiles, sounds and expanded archives), warm with the
// cache turned off (`decodedCache=0`: the HTTP cache already holds the
// downloads, so only decoding remains) and warm with the cache. The cached
// load must read every decode from the cache and take measurably less load
// time than the uncached warm load. A damaged entry must be rejected and the
// course still load. Works against production:
//   node tools/run_browser_tests.mjs --only test_engine_decoded_cache.mjs --url https://furyremastered.com
// DECODED_CACHE_SHOT=out.png saves the ride after a cached load.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const MAP = 'env01';
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const cache = async page => (await state(page)).decodedCache;

async function load(page, extra = '') {
  await page.goto(url(`/play.html?venue=${MAP}&uiDiagnostics=1&weather=off${extra}`));
  await page.waitForFunction(map => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.environment === map, MAP, { timeout: 180000 });
  return cache(page);
}

/** Waits until the background expansions and writes of a load finished. */
async function settle(page) {
  await page.waitForFunction(() => {
    const c = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').decodedCache;
    return c && c.background === 0 && c.pending === 0;
  }, null, { timeout: 120000, polling: 250 });
  return cache(page);
}

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  const cold = await load(page);
  assert.ok(cold.enabled, `cache enabled: ${JSON.stringify(cold)}`);
  assert.equal(cold.hits, 0, `a fresh context has no entries: ${JSON.stringify(cold)}`);
  assert.ok(cold.misses > 0, 'the cold load looked entries up');
  const stored = await settle(page);
  assert.ok(stored.stores > 0 && stored.failed === 0, `entries stored: ${JSON.stringify(stored)}`);
  assert.ok(stored.bytes > 0 && stored.bytes <= stored.limit, `bounded: ${JSON.stringify(stored)}`);

  // Alternate uncached and cached warm loads; compare the faster of each.
  const off = [], on = [];
  let hit;
  for (let round = 0; round < 2; round++) {
    const plain = await load(page, '&decodedCache=0');
    assert.ok(!plain.enabled, 'decodedCache=0 turns the cache off');
    off.push(plain.loadMs);
    hit = await load(page);
    assert.ok(hit.hits > 0, `second load hits the cache: ${JSON.stringify(hit)}`);
    assert.equal(hit.misses, 0, `every decode came from the cache: ${JSON.stringify(hit)}`);
    assert.equal(hit.rejected, 0, `no damaged entries: ${JSON.stringify(hit)}`);
    on.push(hit.loadMs);
    await settle(page);
  }
  if (process.env.DECODED_CACHE_SHOT) {
    await page.waitForTimeout(1500); // Deliberately wall-clock: the ride settles on screen.
    await page.screenshot({ path: process.env.DECODED_CACHE_SHOT });
  }
  const [best, base] = [Math.min(...on), Math.min(...off)];
  assert.ok(best < base * 0.9, `cached load ${best} ms is not measurably faster than uncached ${base} ms`);

  // A damaged entry is rejected, decoded again and stored again.
  const damaged = await page.evaluate(async () => {
    const store = await caches.open('atv-decoded-v1');
    const keys = await store.keys();
    const job = keys.find(k => new URL(k.url).pathname.includes('/job/'));
    await store.put(job, new Response(new Uint8Array([65, 84, 86, 68, 1, 2, 3])));
    return job.url;
  });
  const repaired = await load(page);
  assert.ok(repaired.rejected >= 1, `damaged entry rejected: ${JSON.stringify(repaired)} (${damaged})`);
  assert.ok(repaired.misses >= 1 && repaired.hits > 0, `the rest still hit: ${JSON.stringify(repaired)}`);
  assertNoErrors(errors);
  console.log(`PASS: decoded cache ${MAP}: cold ${cold.loadMs} ms (${stored.entries} entries, ${(stored.bytes / 1e6).toFixed(1)} MB); warm uncached ${off.join('/')} ms; cached ${on.join('/')} ms (${hit.hits} hits, ${(hit.hitBytes / 1e6).toFixed(1)} MB); damaged entry rejected`);
} finally {
  await browser.close();
}
