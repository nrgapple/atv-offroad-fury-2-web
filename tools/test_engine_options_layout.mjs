// Pause and Options in the Rust engine, laid out like the main menu: a
// column of rows on the left with the paused ride in view beside it (the
// whole width on a portrait phone), Back bottom right, and one page at a
// time. Up and Down follow the rows as drawn. The menu status bar's gear
// opens Options from the main menu, beside the frontend's stage.
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
const right = h => h.x + h.width;

async function ride(browser, viewport, errors, options = {}) {
  const page = await newPage(browser, { viewport, ...options }, errors);
  await page.addInitScript(() => localStorage.setItem('atv2-keyboard-help-seen-v1', '1'));
  await page.goto(url('/play.html?venue=env01&uiDiagnostics=1'));
  await page.waitForSelector('#ui-state', { state: 'attached', timeout: 120000 });
  await until(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar);
  return page;
}

// Rows keep to the column on the left of a landscape screen (a scrolled
// row may sit below it); Back, or Pause's sharing chips, sit bottom right,
// clear of every row.
const corner = ['back', 'copyride', 'devlog'];
function column(m, view) {
  const back = row(m, 'back');
  for (const h of m.hits.filter(h => !corner.includes(h.action))) {
    assert.ok(h.x >= -0.5 && right(h) <= view.width + 0.5, `${m.page} ${h.action} on screen: ${JSON.stringify(h)}`);
    if (view.width > view.height) assert.ok(right(h) <= view.width * 0.46, `${m.page} ${h.action} in the column: ${JSON.stringify(h)}`);
    if (back && h.y < view.height) assert.ok(h.y + h.height <= back.y + 0.5 || right(h) <= back.x + 0.5, `${h.action} clear of Back`);
  }
  if (back) assert.ok(inside(back, view) && right(back) >= view.width * 0.85, `Back bottom right: ${JSON.stringify(back)}`);
}

const browser = await launch();
try {
  const errors = [];
  const wide = { width: 1280, height: 720 };
  let page = await ride(browser, wide, errors);
  await page.mouse.click(640, 400);
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  let m = await menu(page);
  // Pause lists each action once (Photo mode used to appear twice).
  assert.equal(new Set(m.hits.map(h => h.action)).size, m.hits.length, `Pause rows once: ${m.hits.map(h => h.action)}`);
  assert.deepEqual(m.choices.slice(0, 2), ['resume', 'reset']);
  assert.ok(m.hits.every(h => inside(h, wide)), 'Pause fits without scrolling');
  column(m, wide);
  // Down walks the rows top to bottom as drawn, then wraps.
  const drawn = [];
  for (let i = 0; i < m.choices.length; i++) {
    const now = await menu(page);
    drawn.push(row(now, now.choices[now.selection]));
    await key(page, 'ArrowDown');
  }
  for (let i = 1; i < drawn.length; i++) {
    const [a, b] = [drawn[i - 1], drawn[i]];
    assert.ok(a.y < b.y || (a.y === b.y && a.x < b.x), `${a.action} then ${b.action}`);
  }
  assert.equal((await menu(page)).selection, 0, 'Down wraps to Resume');

  // Options: its categories alone; each opens a page of its own.
  await tap(page, row(await menu(page), 'settings'), 'Options on Pause');
  await onMenu(page, 'settings');
  m = await menu(page);
  column(m, wide);
  assert.equal(m.choices[0], 'game-settings');
  await key(page, 'ArrowRight');
  assert.equal((await menu(page)).page, 'settings', 'Right does not open a category');
  await tap(page, row(m, 'game-settings'), 'Game Settings');
  await onMenu(page, 'game-settings');
  const before = await page.evaluate(() => localStorage.getItem('atv-measurement'));
  await tap(page, row(await menu(page), 'measurement'), 'measurement');
  assert.notEqual(await page.evaluate(() => localStorage.getItem('atv-measurement')), before, 'the press changed the units');
  column(await menu(page), wide);
  // Back returns to Options on the row it left by, then to Pause.
  await tap(page, row(await menu(page), 'back'), 'Back');
  await onMenu(page, 'settings');
  m = await menu(page);
  assert.equal(m.choices[m.selection], 'game-settings');
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  await page.close();

  // A landscape phone: the same column.
  const land = { width: 844, height: 390 };
  page = await ride(browser, land, errors, { deviceScaleFactor: 3, hasTouch: true, isMobile: true });
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  column(await menu(page), land);
  await tap(page, row(await menu(page), 'settings'), 'Options on Pause');
  await onMenu(page, 'settings');
  await tap(page, row(await menu(page), 'controls'), 'Controls');
  await onMenu(page, 'controls');
  column(await menu(page), land);
  await page.close();

  // A portrait phone: the column spans the screen.
  const tall = { width: 390, height: 844 };
  page = await ride(browser, tall, errors, { deviceScaleFactor: 3, hasTouch: true, isMobile: true });
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  column(await menu(page), tall);
  await tap(page, row(await menu(page), 'settings'), 'Options on Pause');
  await onMenu(page, 'settings');
  column(await menu(page), tall);
  await tap(page, row(await menu(page), 'controls'), 'Controls');
  await onMenu(page, 'controls');
  column(await menu(page), tall);
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
  console.log('PASS: Pause and Options as a column beside the ride, rows in drawn order, one page at a time; the main menu gear');
} finally {
  await browser.close();
}
