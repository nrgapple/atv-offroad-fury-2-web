// The championship loading screen (research/CHAMPIONSHIPS.md, 002591f0 and
// 00322d80): starting a saved Amateur Supercross round 3 with 7 points shows
// the round, the points, the 3 points still needed and the place that earns
// them (2nd) while the course loads.
import assert from 'node:assert/strict';
import { UNLOCKED_PROFILE, assertNoErrors, launch, newPage, nextFrames, optionalScreenshot, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name) => page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout: 60000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(store => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('fury-profile', JSON.stringify({ store, championships: { progress: { 'amsx-normal': { round: 2, points: [7, 9, 4, 2, 0] } } } }));
  }, UNLOCKED_PROFILE.store);
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  await key(page, 'Enter'); await onPage(page, 'single');
  let front = (await read(page)).front;
  for (let i = front.selection; i < front.choices.indexOf('championship'); i++) await key(page, 'ArrowDown');
  await key(page, 'Enter'); await onPage(page, 'championship');
  front = (await read(page)).front;
  for (let i = front.selection; i < front.choices.indexOf('champ:amsx'); i++) await key(page, 'ArrowDown');
  // The highlighted series' next course and standings.
  assert.equal((await read(page)).front.profile, 'Next: Sausalito · Standings: You 7, CPUs 9 / 4 / 2 / 0');
  await key(page, 'Enter');
  await onPage(page, 'loading');
  const loading = (await read(page)).front.loading;
  assert.equal(loading.environment, 'sx01', 'round 3 races the third course');
  assert.deepEqual(loading.championship, [
    'Amateur Supercross (Normal)',
    'You Are Currently On Track: 3 of 6',
    'Current Championship Points: 7',
    'Points Required to Advance: 3',
    'Place Required to Advance: 2nd',
  ]);
  await optionalScreenshot(page, 'ATV_CHAMPIONSHIP_LOADING_SCREENSHOT', 'loading');
  assertNoErrors(errors);
  console.log(`PASS: Rust engine championship loading screen: ${loading.championship.join(' / ')}`);
} finally {
  await browser.close();
}
