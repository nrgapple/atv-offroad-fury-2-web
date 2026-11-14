// Classic menu -> timed event -> landed-chain fixture -> results and records.
// The deterministic fixture exercises the session boundary, not stunt physics.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout: 120000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function choose(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ points: 0 }));
  });
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off&classicTest=score'));
  await onPage(page, 'title'); await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await choose(page, 'single'); await onPage(page, 'single');
  await choose(page, 'classic-courses'); await onPage(page, 'classic-courses');
  let f = (await read(page)).front;
  assert.equal(f.choices.filter(c => c.startsWith('classic:')).length, 14);
  assert.equal(f.labels[0], 'Time Limit: 2:00');
  await choose(page, 'classic-time');
  assert.equal((await read(page)).front.labels[0], 'Time Limit: 4:00');
  for (let i = 0; i < 9; i++) await choose(page, 'classic-time');
  assert.equal((await read(page)).front.labels[0], 'Time Limit: 2:00');
  await optionalScreenshot(page, 'ATV_CLASSIC_SCREENSHOT', 'menu');
  // env03 has no objective archive: Classic must still load.
  await choose(page, 'classic:env03');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.score > 0, null, { timeout: 300000 });
  let ride = (await read(page)).ride;
  assert.equal(ride.environment, 'env03');
  assert.equal(ride.objective.score, 1200);
  await optionalScreenshot(page, 'ATV_CLASSIC_SCREENSHOT', 'hud');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.objective?.recorded, null, { timeout: 60000 });
  ride = (await read(page)).ride;
  assert.equal(ride.objective.timeLeft, 0);
  assert.equal(ride.objective.score, 1200);
  const saved = await page.evaluate(() => ({ hall: JSON.parse(localStorage.getItem('fury-hall-of-fame')), profile: JSON.parse(localStorage.getItem('fury-profile')) }));
  const record = saved.hall.freestyle['unspstunt:env03'];
  assert.equal(record.objectives[0].value, 1200);
  assert.equal(record.stunts[0].value, 1200);
  assert.equal(saved.profile.championships?.objectives?.['unspstunt-normal:env03'], undefined);
  if (ride.results.length === 1) await key(page, 'Enter');
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').ride?.results?.length === 2);
  await optionalScreenshot(page, 'ATV_CLASSIC_SCREENSHOT', 'results');
  await key(page, 'ArrowDown'); await key(page, 'Enter');
  await onPage(page, 'classic-courses');
  assertNoErrors(errors);
  console.log('PASS: Classic Freestyle menu, 14 venues, timer, chain score, results and Hall of Fame');
} finally { await browser.close(); }
