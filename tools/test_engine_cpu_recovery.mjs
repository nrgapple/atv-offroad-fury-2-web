// Idle player reproduces the leader-limited SX15 jump/recovery loop (#545).
// One lap, the original four CPU seats, seed 7, Expert. Headless measurements
// cover full-length races on all four affected courses; this checks Wasm wiring.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 640, height: 360 } }, errors);
  await page.addInitScript(() => localStorage.setItem('furyScript', JSON.stringify([{ frames: 18000, input: {} }])));
  await page.goto(url('/play.html?venue=sx15&race=1&laps=1&cpu=4&difficulty=2&seed=7&weather=off&script=storage'));
  await page.waitForFunction(() => document.querySelector('#trace') || /error|fail/i.test(document.querySelector('#status')?.textContent), null, { timeout: 240000 });
  const trace = JSON.parse(await page.locator('#trace').textContent());
  const race = trace.race;
  const cpus = race.standings.filter(r => !r.player);
  assert.equal(cpus.length, 4);
  assert.ok(cpus.every(r => r.time != null), JSON.stringify(cpus));
  assert.ok(race.opponents.every(r => r.finished), 'every CPU crosses the finish');
  assertNoErrors(errors);
  console.log('PASS: SX15 Expert CPUs finish with the player idle (seed 7)');
} finally {
  await browser.close();
}
