// Host-only gate (issue #710, research/FREERIDE_MENU.md): the main menu over the
// live Free Ride world, on a local game server with content.
//   1. Past the title the page joins Free Ride (on the venue the shared room is on in
//      the server's rotation) and opens the Main Menu overlay over the ride (Hop in
//      selected) instead of the static main menu.
//   2. While the menu is open a roaming CPU rides the player's rider (the
//      autopilot); Hop in hands the same ride back where it stands and the player
//      rides; Pause brings the Main Menu and the autopilot back; a click on the
//      world beside the panel closes it too.
//   3. Single Player leaves the world for the frontend's Single Player page;
//      Back to the main menu joins the world again.
//   4. A phone held upright gets the same menu with Hop in bottom right.
//   5. With the game server unreachable the static main menu shows as before;
//      so it does with Game Settings → Start in: Menu.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_menu.mjs
// ATV_SCREENSHOTS=dir saves freeride-menu-desktop.png and freeride-menu-phone.png.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_SCREENSHOTS;
const report = {};
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
// Predicates are functions: the page's CSP refuses string evaluation.
const onFront = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
const onMenu = (page, name, timeout) =>
  wait(
    page,
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === name,
    name,
    timeout
  );
const menuClosed = page =>
  wait(page, () => {
    const ui = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return Boolean(ui.toolbar) && !ui.toolbar.menu;
  });
const probed = (page, answered) =>
  wait(
    page,
    answered => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').home?.probed === answered,
    answered
  );
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
// In a shared Free Ride room on whatever venue the rotation has it on.
const inRoom = async page => {
  await wait(
    page,
    () => {
      const mp = JSON.parse(window.furyMultiplayer ?? 'null');
      return Boolean(mp?.environment) && /^FREERD/.test(mp?.code ?? '');
    },
    null,
    180000
  );
  return (await page.evaluate(() => JSON.parse(window.furyMultiplayer))).environment;
};

const browser = await launch();
const errors = [];
try {
  // 1. Press Start: the world loads and the Main Menu opens over it.
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await setup(page, 'freeride');
  report.venue = await inRoom(page);
  await onMenu(page, 'home', 120000);
  let ui = await read(page);
  report.home = { choices: ui.toolbar.menu.choices, labels: ui.toolbar.menu.labels, home: ui.home };
  assert.equal(ui.home.riding, true);
  assert.equal(ui.toolbar.menu.choices[0], 'resume');
  assert.equal(ui.toolbar.menu.labels.resume, 'Hop in');
  assert.equal(ui.toolbar.menu.selection, 0, 'Hop in is selected first');
  for (const row of ['single', 'multiplayer', 'settings']) assert.ok(ui.toolbar.menu.choices.includes(row), row);
  assert.ok(!ui.toolbar.menu.choices.includes('leave'), 'no Free Ride pause rows');
  // The autopilot rides under the menu.
  const origin = () => page.evaluate(() => JSON.parse(window.furyMultiplayer).origin);
  const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);
  let start = await origin();
  await nextFrames(page, 240);
  ui = await read(page);
  assert.equal(ui.home.piloting, true, 'the autopilot rides under the menu');
  report.piloted = flat(start, await origin());
  assert.ok(report.piloted > 10, `autopilot moved the rider: ${report.piloted} ft`);
  const hop = await hit(page, 'resume');
  assert.ok(hop.rect.x > 1280 * 0.6, `Hop in on the right: ${JSON.stringify(hop.rect)}`);
  assert.ok(hop.rect.height >= 48, 'Hop in touch target');
  await nextFrames(page, 30);
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-desktop.png` });

  // 2. Hop in rides; Pause reopens the Main Menu; a click beside it rides.
  start = await origin();
  await page.mouse.click(hop.x, hop.y);
  await menuClosed(page);
  const before = await origin();
  // Handed over where it stands: no respawn or teleport.
  report.handover = flat(start, before);
  assert.ok(report.handover < 30, `no jump at Hop in: ${report.handover} ft`);
  assert.equal((await read(page)).home.piloting, false);
  // The autopilot lets go: with no input the rider coasts down; the
  // player's throttle then drives it.
  await nextFrames(page, 300);
  let coast = await origin();
  await nextFrames(page, 60);
  report.coasting = flat(coast, await origin());
  coast = await origin();
  await page.keyboard.down('KeyW');
  await nextFrames(page, 120);
  await page.keyboard.up('KeyW');
  const after = await origin();
  report.rode = flat(coast, after);
  assert.ok(report.coasting < 15, `no autopilot after Hop in: ${report.coasting} ft a second`);
  assert.ok(report.rode > report.coasting + 5, `the player rides after Hop in: ${report.rode} ft`);
  await key(page, 'Escape');
  await onMenu(page, 'home');
  // Pause: the autopilot takes over again.
  start = await origin();
  await nextFrames(page, 180);
  assert.equal((await read(page)).home.piloting, true);
  report.repiloted = flat(start, await origin());
  assert.ok(report.repiloted > 5, `autopilot again after Pause: ${report.repiloted} ft`);
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
  await probed(offline, false);
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
  await probed(menu, true);
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
