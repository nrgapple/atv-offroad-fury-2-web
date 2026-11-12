// The redesigned Pause and Options in the Rust engine. On a wide screen
// Options keeps its categories in a rail beside the open one's rows: the
// rail's highlight previews a category, a press on a previewed row acts on
// it, a tab turns the category without the panel sliding, and the rail's
// Back leaves Options. On a portrait phone Options stays a list of pages.
// The menu status bar's gear opens Options from the main menu.
// Host browser evidence.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent));
const menu = async page => (await read(page)).toolbar.menu;
const until = (page, predicate, arg) => page.waitForFunction(predicate, arg, { timeout: 30000 });
const onMenu = (page, name) => until(page, name => {
  const m = JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu;
  return m?.page === name && !m.motion && !m.exit;
}, name);
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function tap(page, hit, what) {
  assert.ok(hit, what);
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2); await nextFrames(page, 4);
}
const row = (m, action) => m.hits.find(h => h.action === action);
const inside = (h, { width, height }) => h.x >= -0.5 && h.y >= -0.5 && h.x + h.width <= width + 0.5 && h.y + h.height <= height + 0.5;

async function ride(browser, viewport, errors, options = {}) {
  const page = await newPage(browser, { viewport, ...options }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar);
  return page;
}

const browser = await launch();
try {
  const errors = [];
  // Desktop: the rail beside the rows.
  const wide = { width: 1280, height: 720 };
  let page = await ride(browser, wide, errors);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  let m = await menu(page);
  // Pause lists each action once (Photo mode used to appear twice).
  assert.equal(new Set(m.hits.map(h => h.action)).size, m.hits.length, `Pause rows once: ${m.hits.map(h => h.action)}`);
  assert.deepEqual(m.choices.slice(0, 2), ['resume', 'reset']);
  await tap(page, row(m, 'settings'), 'Options on Pause');
  await onMenu(page, 'settings');
  m = await menu(page);
  assert.equal(m.wide, true);
  const categories = m.choices.filter(c => c !== 'back');
  assert.equal(m.previewPage, categories[0], 'the highlighted category shows beside the rail');
  assert.ok(m.preview.length > 1 && m.preview.every(h => inside(h, wide)), JSON.stringify(m.preview));
  const rail = categories.map(c => row(m, c));
  assert.ok(rail.every(Boolean), 'every category is a rail row');
  assert.ok(m.preview.every(p => rail.every(r => r.x + r.width <= p.x)), 'the rows sit right of the rail');
  await key(page, 'ArrowDown');
  assert.equal((await menu(page)).previewPage, categories[1], 'the preview follows the highlight');
  await key(page, 'ArrowUp');

  // A press on a previewed row acts on it and puts the selection there.
  assert.equal(categories[0], 'game-settings');
  const before = await page.evaluate(() => localStorage.getItem('atv-measurement'));
  m = await menu(page);
  await tap(page, m.preview.find(h => h.action === 'measurement'), `measurement in ${JSON.stringify(m.preview)}`);
  m = await menu(page);
  assert.deepEqual([m.page, m.choices[m.selection]], ['game-settings', 'measurement']);
  assert.notEqual(await page.evaluate(() => localStorage.getItem('atv-measurement')), before, 'the press changed the units');
  assert.equal(m.motion, null, 'entering a category does not slide the panel');

  // Tabs turn the category in place; E and Q do the same from the keyboard.
  assert.deepEqual(m.tabs.map(t => t.action), categories);
  assert.ok(m.tabs.every(t => inside(t, wide) && t.height >= 26));
  await tap(page, m.tabs.find(t => t.action === 'music'), 'the Audio tab');
  m = await menu(page);
  assert.deepEqual([m.page, m.motion], ['music', null]);
  await key(page, 'KeyE');
  assert.equal((await menu(page)).page, categories[categories.indexOf('music') + 1]);
  await key(page, 'KeyQ');
  assert.equal((await menu(page)).page, 'music');
  // Escape steps back to the rail on that category; Right steps in again.
  await key(page, 'Escape');
  m = await menu(page);
  assert.deepEqual([m.page, m.choices[m.selection], m.previewPage], ['settings', 'music', 'music']);
  await key(page, 'ArrowRight');
  assert.equal((await menu(page)).page, 'music');
  // The rail's Back leaves Options from inside a category.
  await tap(page, row(await menu(page), 'back'), 'the rail Back');
  await onMenu(page, 'pause');
  await page.close();

  // A landscape phone keeps the rail, every tab on screen.
  const land = { width: 844, height: 390 };
  page = await ride(browser, land, errors, { deviceScaleFactor: 3, hasTouch: true, isMobile: true });
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  m = await menu(page);
  assert.ok(m.hits.every(h => inside(h, land)), `Pause fits a landscape phone: ${JSON.stringify(m.hits)}`);
  await tap(page, row(m, 'settings'), 'Options on Pause');
  await onMenu(page, 'settings');
  await tap(page, row(await menu(page), 'controls'), 'Controls in the rail');
  m = await menu(page);
  assert.deepEqual([m.page, m.wide], ['controls', true]);
  assert.equal(m.tabs.length, categories.length);
  assert.ok(m.tabs.every(t => inside(t, land) && t.height >= 26), JSON.stringify(m.tabs));
  assert.ok(inside(row(m, 'back'), land), 'Back is on screen');
  await page.close();

  // A portrait phone: a list of categories, each a page of its own.
  const tall = { width: 390, height: 844 };
  page = await ride(browser, tall, errors, { deviceScaleFactor: 3, hasTouch: true, isMobile: true });
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  await tap(page, row(await menu(page), 'settings'), 'Options on Pause');
  await onMenu(page, 'settings');
  m = await menu(page);
  assert.deepEqual([m.wide, m.previewPage, m.preview.length, m.tabs.length], [false, null, 0, 0]);
  assert.ok(m.hits.every(h => inside(h, tall)), `Options fits a portrait phone: ${JSON.stringify(m.hits)}`);
  await tap(page, row(m, 'controls'), 'Controls');
  await onMenu(page, 'controls');
  await tap(page, row(await menu(page), 'back'), 'Back');
  await onMenu(page, 'settings');
  await page.close();

  // The main menu: the status bar's gear opens Options.
  page = await newPage(browser, { viewport: wide }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?uiDiagnostics=1&front=main'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  const front = () => page.evaluate(() => JSON.parse(document.querySelector('#ui-state').textContent).front || {});
  await until(page, () => {
    const f = JSON.parse(document.querySelector('#ui-state').textContent).front;
    return f?.page === 'main' && !f.motion && f.hits?.some(h => h.id === 2467);
  });
  let f = await front();
  assert.equal(f.menu, null);
  await tap(page, f.hits.find(h => h.id === 2467), 'the gear');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front?.menu?.page === 'settings');
  f = await front();
  assert.equal(f.page, 'main', 'Options opens over the menu page');
  assert.ok(f.menu.choices.includes('game-settings') && f.menu.choices.includes('jukebox'), `${f.menu.choices}`);
  await key(page, 'Escape');
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front?.menu == null);
  assertNoErrors(errors);
  console.log('PASS: Options rail, preview, tabs and Back on wide screens; pages on a portrait phone; the main menu gear');
} finally {
  await browser.close();
}
