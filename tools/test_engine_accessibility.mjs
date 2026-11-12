// Options → Accessibility in the Rust engine (#547): a saved field of view
// reaches the ride camera, the page lists its rows, and HUD size, colour
// assistance and the field of view slider save and apply from the menu.
// Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const until = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
const access = () => JSON.parse(document.querySelector('#ui-state').textContent).settings?.access;
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
  await page.addInitScript(() => { if (!sessionStorage.getItem('seeded')) { sessionStorage.setItem('seeded', '1'); localStorage.setItem('atv2-keyboard-help-seen-v1', '1'); localStorage.setItem('atv-fov', '110'); } });
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(page, () => { const a = JSON.parse(document.querySelector('#ui-state').textContent).settings?.access; return a?.fov === 110 && a.hudScale === 1; });
  const before = (await read(page)).settings.access;
  assert.equal(before.colour, 'Off');
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  await click(page, 'settings');
  await click(page, 'accessibility');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar.menu.page === 'accessibility');
  const m = (await read(page)).toolbar.menu;
  assert.deepEqual(m.choices, ['access-fov', 'access:hud', 'access:colour', 'back']);
  await click(page, 'access:hud');
  await click(page, 'access:colour');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-hud-size')), 'large');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-colour-assist')), 'protanopia');
  // The slider is the page's first row: Right widens the view a step.
  for (let i = 0; i < 3 && (await read(page)).toolbar.menu.selection !== 0; i++) await key(page, 'ArrowUp');
  await key(page, 'ArrowRight');
  assert.equal(await page.evaluate(() => localStorage.getItem('atv-fov')), '115');
  // Back to the ride: the HUD behind the menu takes its size and colours.
  for (let i = 0; i < 4 && (await read(page)).toolbar.menu; i++) await key(page, 'Escape');
  await until(page, f => {
    const a = JSON.parse(document.querySelector('#ui-state').textContent).settings.access;
    return a.fov === 115 && a.focal < f && a.hudScale === 1.25 && a.colour === 'Protanopia';
  }, before.focal).catch(async e => { throw new Error(`${e.message}: ${JSON.stringify((await read(page)).settings.access)}`); });
  assertNoErrors(errors);
  console.log('PASS: Rust Accessibility rows, saved choices and the applied field of view, HUD size and colours');
} finally {
  await browser.close();
}
