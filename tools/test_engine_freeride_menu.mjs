// Host-only gate (issue #710, research/FREERIDE_MENU.md): the main menu over the
// live Free Ride world, on a local game server with content.
//   1. Past the title the page joins Arlington Free Ride and opens the Main Menu
//      overlay over the ride (Hop in selected) instead of the static main menu.
//   2. Hop in closes it and the rider rides; Pause brings the Main Menu back; a
//      click on the world beside the panel closes it too.
//   3. Single Player leaves the world for the frontend's Single Player page;
//      Back to the main menu joins the world again.
//   4. A phone held upright gets the same menu with Hop in bottom right.
//   5. With the game server unreachable the static main menu shows as before;
//      so it does with Game Settings → Start in: Menu.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_menu.mjs
// ATV_SCREENSHOTS=dir saves freeride-menu-desktop.png and freeride-menu-phone.png.
import assert from 'node:assert/strict';
import { assertNoErrors, holdKey, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_SCREENSHOTS;
const report = {};
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const state = () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
const onFront = (page, name) => wait(page, `(${state})().front?.page === ${JSON.stringify(name)}`);
const onMenu = (page, name, timeout) =>
  wait(page, `(${state})().toolbar?.menu?.page === ${JSON.stringify(name)}`, null, timeout);
const menuClosed = page => wait(page, `(${state})().toolbar && !(${state})().toolbar.menu`);
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function hit(page, action) {
  const m = (await read(page)).toolbar.menu;
  const h = m.hits.find(h => h.action === action);
  assert.ok(h, `${action} on ${m.page}: ${m.choices}`);
  return { x: h.x + h.width / 2, y: h.y + h.height / 2, rect: h };
}
async function setup(page, startIn) {
  await page.addInitScript(startIn => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    if (startIn) localStorage.setItem('atv-start-in', startIn);
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  }, startIn);
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
}
const inRoom = page => wait(page, () => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, 180000);

const browser = await launch();
const errors = [];
try {
  // 1. Press Start: the world loads and the Main Menu opens over it.
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await setup(page, 'freeride');
  await inRoom(page);
  await onMenu(page, 'home', 120000);
  let ui = await read(page);
  report.home = { choices: ui.toolbar.menu.choices, labels: ui.toolbar.menu.labels, home: ui.home };
  assert.equal(ui.home.riding, true);
  assert.equal(ui.toolbar.menu.choices[0], 'resume');
  assert.equal(ui.toolbar.menu.labels.resume, 'Hop in');
  assert.equal(ui.toolbar.menu.selection, 0, 'Hop in is selected first');
  for (const row of ['single', 'multiplayer', 'settings']) assert.ok(ui.toolbar.menu.choices.includes(row), row);
  assert.ok(!ui.toolbar.menu.choices.includes('leave'), 'no Free Ride pause rows');
  const hop = await hit(page, 'resume');
  assert.ok(hop.rect.x > 1280 * 0.6, `Hop in on the right: ${JSON.stringify(hop.rect)}`);
  assert.ok(hop.rect.height >= 48, 'Hop in touch target');
  await nextFrames(page, 30);
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-desktop.png` });

  // 2. Hop in rides; Pause reopens the Main Menu; a click beside it rides.
  await page.mouse.click(hop.x, hop.y);
  await menuClosed(page);
  const before = (await page.evaluate(() => window.atvTerrainStatus.frame.origin)).slice();
  await holdKey(page, 'KeyW', 120);
  const after = await page.evaluate(() => window.atvTerrainStatus.frame.origin);
  report.rode = Math.hypot(after[0] - before[0], after[2] - before[2]);
  assert.ok(report.rode > 3, `rode after Hop in: ${report.rode} ft`);
  await key(page, 'Escape');
  await onMenu(page, 'home');
  await page.mouse.click(760, 140);
  await menuClosed(page);
  await key(page, 'Escape');
  await onMenu(page, 'home');
  // The keyboard's Enter on the selected Hop in rides as well.
  await key(page, 'Enter');
  await menuClosed(page);

  // 3. Single Player leaves for the frontend; Back joins the world again.
  await key(page, 'Escape');
  await onMenu(page, 'home');
  const single = await hit(page, 'single');
  await page.mouse.click(single.x, single.y);
  await onFront(page, 'single');
  assert.equal((await read(page)).home.riding, false);
  await key(page, 'Escape');
  await inRoom(page);
  await onMenu(page, 'home', 120000);
  report.rejoined = true;
  await page.close();

  // 4. A phone held upright.
  const phone = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, hasTouch: true, isMobile: true },
    errors
  );
  await setup(phone, 'freeride');
  await inRoom(phone);
  await onMenu(phone, 'home', 120000);
  const tap = await hit(phone, 'resume');
  report.phoneHop = tap.rect;
  assert.ok(tap.rect.x + tap.rect.width > 390 * 0.75 && tap.rect.y > 844 * 0.6, 'Hop in bottom right');
  assert.ok(tap.rect.height >= 48, 'Hop in touch target');
  await nextFrames(phone, 30);
  if (shots) await phone.screenshot({ path: `${shots}/freeride-menu-phone.png` });
  await phone.touchscreen.tap(tap.x, tap.y);
  await menuClosed(phone);
  await phone.close();

  // 5. Unreachable game server: the static main menu as before.
  // Packaged builds run a service worker, which page.route cannot see.
  const offline = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await offline.route('**/multiplayer/**', route => route.abort());
  await setup(offline, 'freeride');
  await onFront(offline, 'main');
  await wait(offline, `(${state})().home?.probed === false`);
  await offline.waitForTimeout(1500);
  ui = await read(offline);
  assert.equal(ui.front?.page, 'main', 'static main menu');
  assert.equal(ui.home.riding, false);
  report.offline = ui.home;
  await offline.close();
  // Start in: Menu (the harness default for other tests).
  const menu = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await setup(menu, null);
  await onFront(menu, 'main');
  await wait(menu, `(${state})().home?.probed === true`);
  await menu.waitForTimeout(1500);
  ui = await read(menu);
  assert.equal(ui.front?.page, 'main', 'Start in: Menu keeps the static main menu');
  assert.equal(ui.home.riding, false);
  report.startMenu = ui.home;
  await menu.close();

  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
  console.log('PASS freeride main menu');
} finally {
  await browser.close();
}
