// Rust engine: race CPUs cast shadows like the player (High: contact blobs
// within 240 ft and the sun shadow map within 40 ft; drawContactShadows and
// renderVehicleShadows in game.js) with the resting lift measured from the
// ride's steps (acceptFrame), and ride archives prefetch in parallel
// (game.js prefetchRide): env01 loads with 150 ms request latency with and
// without `?prefetch=off`.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4177 node tools/test_engine_shadows_prefetch.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const report = {};
const errors = [];
const browser = await launch();
const state = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
try {
  // 1. Shadows on a race grid: player + 3 CPUs.
  for (const graphics of ['high', 'enhanced']) {
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
    await page.goto(url(`/play.html?venue=env01&race=1&laps=1&cpu=3&seed=12345&weather=off&graphics=${graphics}&uiDiagnostics=1`));
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').shadows?.contact > 0, null, { timeout: 180000 });
    const samples = [];
    for (let i = 0; i < 10; i++) {
      samples.push((await state(page)).shadows);
      await page.waitForTimeout(200); // Time-based on purpose: a few frames of the grid.
    }
    if (graphics === 'high' && process.env.ATV_SHADOW_SCREENSHOT)
      await page.screenshot({ path: process.env.ATV_SHADOW_SCREENSHOT });
    report[graphics] = {
      contact: Math.max(...samples.map(s => s.contact)),
      map: Math.max(...samples.map(s => s.map)),
      restLift: samples.at(-1).restLift
    };
    await page.close();
  }
  assert.ok(report.high.contact >= 4, `contact blobs for the player and CPUs: ${JSON.stringify(report)}`);
  assert.ok(report.high.map >= 2, `CPUs near the player cast into the shadow map: ${JSON.stringify(report)}`);
  assert.equal(report.enhanced.map, 0, 'Enhanced has no shadow map');
  assert.ok(report.enhanced.contact >= 4, `Enhanced contact blobs: ${JSON.stringify(report)}`);
  // The contact shadows' resting lift is measured from the ride's own steps
  // (game.js acceptFrame), not left at the 1.5 ft default.
  for (const graphics of ['high', 'enhanced']) {
    const lift = report[graphics].restLift;
    assert.ok(Number.isFinite(lift) && Math.abs(lift) < 5, `${graphics} rest lift: ${JSON.stringify(report)}`);
  }

  // 2. Prefetch: time to a running env01 ride with 150 ms latency per request.
  const load = async prefetch => {
    const context = await browser.newContext({ viewport: { width: 960, height: 540 } });
    const page = await context.newPage();
    await page.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
    const cdp = await context.newCDPSession(page);
    await cdp.send('Network.enable');
    await cdp.send('Network.emulateNetworkConditions', {
      offline: false,
      latency: 150,
      downloadThroughput: -1,
      uploadThroughput: -1
    });
    const requests = [];
    page.on('request', request => /\/content\/[a-f0-9]{64}\.bin$/.test(request.url()) && requests.push(request.url()));
    const started = Date.now();
    await page.goto(url(`/play.html?venue=env01&weather=off&uiDiagnostics=1${prefetch ? '' : '&prefetch=off'}`));
    await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').active === true, null, { timeout: 300000 });
    const seconds = (Date.now() - started) / 1000;
    await context.close();
    return { seconds, blobs: new Set(requests).size };
  };
  report.prefetch = [];
  for (const prefetch of [false, true, false, true]) report.prefetch.push({ prefetch, ...(await load(prefetch)) });
  const mean = on => {
    const runs = report.prefetch.filter(r => r.prefetch === on);
    return runs.reduce((sum, r) => sum + r.seconds, 0) / runs.length;
  };
  report.loadSeconds = { serial: mean(false), prefetch: mean(true) };
  assert.ok(report.loadSeconds.prefetch < report.loadSeconds.serial, `prefetch is faster: ${JSON.stringify(report.loadSeconds)}`);
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
