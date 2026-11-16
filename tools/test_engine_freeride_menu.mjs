// Host-only gate (issue #710, research/FREERIDE_MENU.md): the live Free Ride world
// behind the frontend's menus, on a local game server with content.
//   1. Press Start shows the frontend's Main Menu at once and never the course
//      loader; Free Ride (on whatever venue the shared room is on in the server's
//      rotation) loads behind it and crossfades in on the stage, a roaming CPU
//      riding the player's rider. The menus stay the frontend's (no pause overlay);
//      Hop in sits bottom right beside Back.
//   2. Single Player keeps the world on its stage; Back returns to the main menu.
//   3. Hop in hands the same ride to the player where it stands and the ride's HUD
//      takes over; Pause → Quit to Main Menu brings the menus back over the world,
//      the autopilot riding again; a click on the world hops in.
//   4. A phone held upright: the world in the top stage, Hop in bottom right.
//   5. With the game server unreachable, or Game Settings → Start in: Menu, the
//      menus show as before with no world and no Hop in.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_menu.mjs
// ATV_SCREENSHOTS=dir saves freeride-menu-desktop.png, freeride-menu-loading.png
// and freeride-menu-phone.png.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_SCREENSHOTS;
const report = {};
// Frontend hit ids (atv_ui::front): Back, Hop in, the live world on the stage.
const BACK = 2401;
const HOP_IN = 2418;
const WORLD = 2419;
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
// Predicates are functions: the page's CSP refuses string evaluation.
const onFront = (page, name, timeout) =>
  wait(
    page,
    name => {
      const ui = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return ui.front?.page === name && !ui.toolbar;
    },
    name,
    timeout
  );
const onMenu = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === name, name);
const riding = page =>
  wait(page, () => {
    const ui = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return Boolean(ui.toolbar) && !ui.toolbar.menu && ui.home?.live === false;
  });
// The world crossfaded in under the menus.
const live = (page, timeout = 180000) =>
  wait(
    page,
    () => {
      const ui = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
      return ui.home?.live && ui.home.fade >= 1 && Boolean(ui.front);
    },
    null,
    timeout
  );
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
async function frontHit(page, id) {
  const f = (await read(page)).front;
  const h = f.hits.find(h => h.id === id);
  assert.ok(h, `hit ${id} on ${f.page}`);
  return { x: h.x + h.width / 2, y: h.y + h.height / 2, rect: h };
}
async function menuHit(page, action) {
  const m = (await read(page)).toolbar.menu;
  const h = m.hits.find(h => h.action === action);
  assert.ok(h, `${action} on ${m.page}: ${m.choices}`);
  return { x: h.x + h.width / 2, y: h.y + h.height / 2, label: m.labels[action] };
}
async function setup(page, startIn) {
  await page.addInitScript(startIn => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    if (startIn) localStorage.setItem('atv-start-in', startIn);
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    // Every frontend page the boot shows (the course loader must not be one).
    window.__pages = new Set();
    setInterval(() => {
      try {
        const page = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page;
        if (page) window.__pages.add(page);
      } catch {}
    }, 25);
  }, startIn);
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onFront(page, 'main');
}
const pages = page => page.evaluate(() => [...window.__pages]);
const origin = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null')?.origin);
const flat = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

const browser = await launch();
const errors = [];
try {
  // 1. Press Start: the Main Menu at once, the world loading behind it.
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await setup(page, 'freeride');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').home?.loading);
  let ui = await read(page);
  assert.ok(ui.front.choices.includes('hopin'), 'Hop in while the world loads');
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-loading.png` });
  await live(page);
  ui = await read(page);
  report.boot = { pages: await pages(page), home: ui.home };
  assert.ok(!report.boot.pages.includes('loading'), `no course loader at boot: ${report.boot.pages}`);
  assert.equal(ui.front.page, 'main', 'the frontend Main Menu');
  assert.ok(!ui.toolbar, 'no ride overlay while the menus show');
  assert.equal(ui.front.choices.at(-1), 'hopin');
  const venue = await page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null')?.environment);
  report.venue = venue;
  assert.ok(venue, 'in a shared Free Ride room');
  const hop = await frontHit(page, HOP_IN);
  const back = await frontHit(page, BACK);
  const world = await frontHit(page, WORLD);
  report.hits = { hop: hop.rect, back: back.rect, world: world.rect };
  assert.ok(hop.rect.x > 1280 * 0.6 && hop.rect.y > 720 * 0.75, `Hop in bottom right: ${JSON.stringify(hop.rect)}`);
  assert.ok(hop.rect.height >= 48, 'Hop in touch target');
  assert.ok(hop.rect.x > back.rect.x, 'Hop in beside Back');
  assert.ok(world.rect.x > 1280 * 0.4, `the world on the stage: ${JSON.stringify(world.rect)}`);
  // The autopilot rides under the menus.
  let start = await origin(page);
  await nextFrames(page, 240);
  ui = await read(page);
  assert.equal(ui.home.piloting, true, 'the autopilot rides under the menus');
  report.piloted = flat(start, await origin(page));
  assert.ok(report.piloted > 10, `autopilot moved the rider: ${report.piloted} ft`);
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-desktop.png` });

  // 2. Another stage page keeps the world; Back returns.
  await key(page, 'ArrowDown');
  ui = await read(page);
  const single = ui.front.choices.indexOf('single');
  while ((await read(page)).front.selection !== single) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onFront(page, 'single');
  ui = await read(page);
  assert.ok(ui.home.live && ui.home.world, 'the world stays behind Single Player');
  assert.ok(ui.front.hits.some(h => h.id === WORLD) && ui.front.hits.some(h => h.id === HOP_IN));
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-single.png` });
  await key(page, 'Escape');
  await onFront(page, 'main');

  // 3. Hop in: the same ride, the player's, with the ride's HUD.
  start = await origin(page);
  await page.mouse.click(hop.x, hop.y);
  await riding(page);
  const before = await origin(page);
  report.handover = flat(start, before);
  assert.ok(report.handover < 30, `no jump at Hop in: ${report.handover} ft`);
  assert.equal((await read(page)).home.piloting, false);
  // The autopilot lets go: with no input the rider coasts down; the
  // player's throttle then drives it.
  await nextFrames(page, 300);
  let coast = await origin(page);
  await nextFrames(page, 60);
  report.coasting = flat(coast, await origin(page));
  coast = await origin(page);
  await page.keyboard.down('KeyW');
  await nextFrames(page, 120);
  await page.keyboard.up('KeyW');
  report.rode = flat(coast, await origin(page));
  assert.ok(report.coasting < 15, `no autopilot after Hop in: ${report.coasting} ft a second`);
  assert.ok(report.rode > report.coasting + 5, `the player rides after Hop in: ${report.rode} ft`);
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-riding.png` });
  // Pause is the ride's own; Quit to Main Menu brings the menus back.
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  const quit = await menuHit(page, 'leave');
  report.quitLabel = quit.label;
  assert.equal(quit.label, 'Quit to Main Menu');
  await page.mouse.click(quit.x, quit.y);
  await onFront(page, 'main');
  ui = await read(page);
  assert.ok(ui.home.live && ui.home.piloting, 'the menus over the world again, the autopilot riding');
  // Left against an obstacle or mid-wreck, the roaming CPU's stuck rules may
  // take a few seconds before it rides off.
  start = await origin(page);
  const since = Date.now();
  await wait(
    page,
    ([x, z]) => {
      const o = JSON.parse(window.furyMultiplayer ?? 'null')?.origin;
      return o && Math.hypot(o[0] - x, o[2] - z) > 5;
    },
    [start[0], start[2]],
    12000
  );
  report.repiloted = { seconds: (Date.now() - since) / 1000, feet: flat(start, await origin(page)) };
  // A click on the world hops in.
  const area = await frontHit(page, WORLD);
  await page.mouse.click(area.x, area.rect.y + area.rect.height * 0.45);
  await riding(page);
  report.worldClick = true;
  await page.close();

  // 4. A phone held upright.
  const phone = await newPage(
    browser,
    { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, hasTouch: true, isMobile: true },
    errors
  );
  await setup(phone, 'freeride');
  await live(phone);
  assert.ok(!(await pages(phone)).includes('loading'), 'no course loader on the phone');
  const tap = await frontHit(phone, HOP_IN);
  const top = await frontHit(phone, WORLD);
  report.phone = { hop: tap.rect, world: top.rect };
  assert.ok(tap.rect.x + tap.rect.width > 390 * 0.75 && tap.rect.y > 844 * 0.75, 'Hop in bottom right');
  assert.ok(tap.rect.height >= 48, 'Hop in touch target');
  assert.ok(top.rect.y + top.rect.height < 844 * 0.5, 'the world in the top stage');
  await nextFrames(phone, 60);
  if (shots) await phone.screenshot({ path: `${shots}/freeride-menu-phone.png` });
  await phone.touchscreen.tap(tap.x, tap.y);
  await riding(phone);
  await phone.close();

  // 5. Unreachable game server: the menus as before.
  // Packaged builds run a service worker, which page.route cannot see.
  const offline = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  await offline.route('**/multiplayer/**', route => route.abort());
  await setup(offline, 'freeride');
  await probed(offline, false);
  await offline.waitForTimeout(1500);
  ui = await read(offline);
  assert.equal(ui.front?.page, 'main');
  assert.ok(!ui.home.loading && !ui.home.world, 'no world offline');
  assert.ok(!ui.front.choices.includes('hopin'), 'no Hop in offline');
  report.offline = ui.home;
  await offline.close();
  // Start in: Menu (the harness default for other tests).
  const menu = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await setup(menu, null);
  await probed(menu, true);
  await menu.waitForTimeout(1500);
  ui = await read(menu);
  assert.equal(ui.front?.page, 'main');
  assert.ok(!ui.home.loading && !ui.home.world, 'Start in: Menu loads no world');
  report.startMenu = ui.home;
  await menu.close();

  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
  console.log('PASS freeride main menu');
} finally {
  await browser.close();
}
