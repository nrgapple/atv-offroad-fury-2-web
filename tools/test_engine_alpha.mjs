// Settings → Alpha in the Rust engine (web/alpha.js): the seven authored
// experiments listed Off by default, toggled from the menu, saved under
// `atv-alpha`, and the ones the Rust ride runs (air slow-mo, big-air camera,
// arcade flips) applied to the ride. Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const until = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function click(page, action) {
  const m = (await read(page)).toolbar.menu, hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => { if (!sessionStorage.getItem('seeded')) { sessionStorage.setItem('seeded', '1'); localStorage.setItem('atv2-keyboard-help-seen-v1', '1'); localStorage.setItem('atv-alpha', '["arcade-flips"]'); } });
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).settings?.arcadeFlips === true);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  await click(page, 'settings');
  await click(page, 'alpha');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.page === 'alpha');
  const m = (await read(page)).toolbar.menu;
  assert.deepEqual(m.choices.filter(c => c.startsWith('alpha:')), ['alpha:air-slowmo', 'alpha:big-air-camera', 'alpha:music-muffle', 'alpha:air-blur', 'alpha:landing-burst', 'alpha:heavy-mud', 'alpha:arcade-flips']);
  await click(page, 'alpha:air-slowmo');
  await click(page, 'alpha:big-air-camera');
  await click(page, 'alpha:arcade-flips');
  assert.deepEqual(JSON.parse(await page.evaluate(() => localStorage.getItem('atv-alpha'))).sort(), ['air-slowmo', 'big-air-camera']);
  const s = (await read(page)).settings;
  assert.deepEqual([s.alphaSlowmo, s.alphaBigAirCamera, s.arcadeFlips, s.timeScale], [true, true, false, 1]);
  assertNoErrors(errors);
  console.log('PASS: Rust Alpha rows, saved toggles and applied ride options');
} finally {
  await browser.close();
}
