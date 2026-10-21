// Rust frontend: title → main → single player → course select (every venue,
// event tabs, photos) → loader → ride → Quit to Main Menu; the garage saves
// the ATV and rider under the browser game's key; ride links start a ride.
// Host browser evidence; parity images come from work capture scripts.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => { localStorage.setItem('atv2-keyboard-help-seen-v1', '1'); localStorage.removeItem('atv-player-selection'); });
  await page.goto(url('/engine.html?uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === 'title', null, 120000);
  let f = await front(page);
  assert.deepEqual(f.choices, ['main', 'devlog']);
  assert.equal(f.tracks, 40, 'every venue is listed');
  // Canvas focus for key events, on an empty part of the title.
  await page.mouse.click(8, 8);
  await key(page, 'Enter'); await onPage(page, 'main');
  f = await front(page);
  assert.ok(f.choices.includes('single') && f.choices.includes('settings'), f.choices.join());
  assert.ok(!f.choices.includes('multiplayer') && !f.choices.includes('jukebox'), 'unported rows are hidden');
  // Settings opens over the frontend and Back closes it.
  await key(page, 'ArrowDown'); await key(page, 'Enter');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.menu?.page === 'settings');
  await key(page, 'Escape');
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).front.menu);
  await key(page, 'ArrowUp'); await key(page, 'Enter'); await onPage(page, 'single');
  assert.deepEqual((await front(page)).choices, ['tracks', 'garage']);
  // Garage: Right steps the ATV, the rider switch toggles; both persist.
  await key(page, 'ArrowDown'); await key(page, 'Enter'); await onPage(page, 'garage');
  await key(page, 'ArrowRight');
  await key(page, 'ArrowDown'); await key(page, 'Enter');
  f = await front(page);
  assert.deepEqual([f.quad, f.rider], [2, 1]);
  const saved = JSON.parse(await page.evaluate(() => localStorage.getItem('atv-player-selection')));
  assert.deepEqual([saved.quadId, saved.riderId], [2, 1]);
  await key(page, 'Escape'); await onPage(page, 'single');
  // Course select: photos load, tabs filter, a click on a card selects it.
  await key(page, 'ArrowUp'); await key(page, 'Enter'); await onPage(page, 'tracks');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.photos >= 30, null, 120000);
  await key(page, 'PageDown');
  f = await front(page);
  assert.equal(f.filter, 'env');
  const card = f.hits.find(h => h.id === 2201);
  assert.ok(card, 'second Enduro card');
  await page.mouse.click(centre(card).x, centre(card).y); await nextFrames(page, 3);
  assert.equal((await front(page)).track, 'env02');
  // Ride: the loader shows, then the ride; Quit to Main Menu returns.
  await key(page, 'Enter');
  await wait(page, () => (document.querySelector('#status')?.textContent ?? '').startsWith('ENV02'), null, 180000);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  const quit = (await read(page)).toolbar.menu.hits.find(h => h.action === 'main');
  assert.ok(quit, 'Quit to Main Menu');
  await page.mouse.click(centre(quit).x, centre(quit).y);
  await onPage(page, 'main');
  assertNoErrors(errors, 'frontend');
  await page.close();

  // A ride link starts its ride with the link's ATV and rider (launch.js).
  const link = await newPage(browser, { viewport: { width: 390, height: 844 } }, []);
  await link.goto(url('/engine.html?map=env03&quad=4&rider=female&uiDiagnostics=1'));
  await wait(link, () => (document.querySelector('#status')?.textContent ?? '').startsWith('ENV03'), null, 180000);
  const linked = JSON.parse(await link.evaluate(() => localStorage.getItem('atv-player-selection')));
  assert.equal(linked.freestyleEnvironment, 'env03');
  await link.close();
  console.log('Rust frontend: navigation, garage persistence, course select, ride and quit, ride links');
} finally {
  await browser.close();
}
