// Host-only gate (issue #710, research/FREERIDE_MENU.md): the live Free Ride world
// behind the frontend's menus, on a local game server with content.
//   1. Press Start shows the frontend's Main Menu at once and never the course
//      loader; Free Ride (on whatever venue the shared room is on in the server's
//      rotation) loads behind it and crossfades in on the stage, a roaming CPU
//      riding the player's rider. The menus stay the frontend's (no pause overlay);
//      Hop in is a pulsing target on the world under the rider (no button beside
//      Back), and the last keyboard choice once the world shows.
//   2. Single Player keeps the world on its stage; Back returns to the main menu.
//   3. Hop in hands the same ride to the player where it stands and the ride's HUD
//      takes over; Pause → Quit to Main Menu brings the menus back over the world,
//      the autopilot riding again; a click on the world hops in.
//   4. A phone held upright, and one held sideways: the world in the stage, the Hop
//      in target on it.
//   5. With the game server unreachable, or Game Settings → Start in: Menu, the
//      menus show as before with no world and no Hop in.
//   While the world loads (and offline) the stage shows the player's ATV turning
//   on its turntable without its name; it crossfades to the world. Under the
//   menus the backdrop camera frames the rider higher and wider (horizon in
//   view, clear of the ground) and moves without snapping; Hop in blends back to
//   the chase camera.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride_menu.mjs
// ATV_SCREENSHOTS=dir saves freeride-menu-desktop.png, freeride-menu-loading.png,
// freeride-menu-phone.png, freeride-menu-phone-loading.png,
// freeride-menu-landscape.png and freeride-menu-landscape-loading.png.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_SCREENSHOTS;
const report = {};
// Frontend hit ids (atv_ui::front): Back, Hop in, the live world on the stage.
const BACK = 2401;
const HOP_IN = 2418;
const WORLD = 2419;
const MODEL_QUAD = 2409;
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
// While the world loads: the player's ATV turns on the stage without its name.
async function turntable(page, label) {
  await wait(page, () => {
    const ui = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
    return ui.home?.loading && ui.front?.models?.[0];
  });
  const before = (await read(page)).front;
  await nextFrames(page, 20);
  const ui = await read(page);
  if (ui.home.loading) {
    assert.ok(ui.front.worldStage, `${label}: the ATV without its name`);
    assert.ok(ui.front.hits.some(h => h.id === MODEL_QUAD), `${label}: the ATV on the stage`);
    assert.notEqual(ui.front.modelYaw[0], before.modelYaw[0], `${label}: the turntable turns`);
  }
  return { yaw: [before.modelYaw[0], ui.front.modelYaw[0]], loading: ui.home.loading };
}
// The camera's eye and yaw sampled every frame for `ms` (in the page): the
// fastest yaw turn (rad/s) and eye climb or drop (ft/s) while the backdrop
// has a share, and the lowest clearance over the ground under it.
const sampleCamera = (page, ms) =>
  page.evaluate(
    ms =>
      new Promise(done => {
        const out = { yawRate: 0, rise: 0, clearance: Infinity, blend: [], pitch: [], resets: 0 };
        let last = null;
        const end = performance.now() + ms;
        const step = now => {
          try {
            const ui = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}');
            const b = ui.home?.backdrop;
            const yaw = b?.yaw;
            const at = JSON.parse(window.furyMultiplayer ?? 'null')?.origin;
            if (b && yaw !== undefined) {
              // A reset (a wreck's recovery or the autopilot's stuck rules)
              // cuts the ride's own camera, as in any ride: not a backdrop snap.
              const reset =
                (last && ui.reset_count !== last.resetCount) ||
                (last?.at && at && Math.hypot(at[0] - last.at[0], at[2] - last.at[2]) > 20);
              if (reset) out.resets++;
              // Only while the backdrop has a share (the chase is the original's).
              if (last && !reset && now > last.now && (b.blend > 0 || last.blend > 0)) {
                const dt = (now - last.now) / 1000;
                let turn = yaw - last.yaw;
                turn = Math.atan2(Math.sin(turn), Math.cos(turn));
                const yawRate = Math.abs(turn) / Math.max(dt, 1 / 60);
                const rise = Math.abs(b.eye[1] - last.y) / Math.max(dt, 1 / 60);
                if (yawRate > out.yawRate || rise > out.rise)
                  out.worst = { yawRate, rise, dt, blend: [last.blend, b.blend], y: [last.y, b.eye[1]], yaw: [last.yaw, yaw] };
                out.yawRate = Math.max(out.yawRate, yawRate);
                out.rise = Math.max(out.rise, rise);
              }
              out.clearance = Math.min(out.clearance, b.clearance);
              out.blend.push(b.blend);
              out.pitch.push(b.pitch);
              last = { now, yaw, y: b.eye[1], blend: b.blend, at, resetCount: ui.reset_count };
            }
          } catch {}
          if (now < end) requestAnimationFrame(step);
          else done(out);
        };
        requestAnimationFrame(step);
      }),
    ms
  );
// The backdrop under the menus: full share, the horizon in view (looking down
// a little), clear of the ground, no snaps.
function assertBackdrop(camera, label) {
  assert.ok(camera.blend.length > 10 && camera.blend.every(b => b === 1), `${label}: backdrop framing`);
  assert.ok(camera.pitch.every(p => p < -0.05 && p > -0.45), `${label}: horizon in view: ${camera.pitch.at(-1)}`);
  assert.ok(camera.clearance > 6, `${label}: clear of the ground: ${camera.clearance} ft`);
  assert.ok(camera.yawRate < 3.5, `${label}: no yaw snap: ${camera.yawRate} rad/s`);
  assert.ok(camera.rise < 45, `${label}: no height snap: ${camera.rise} ft/s`);
}

const browser = await launch();
const errors = [];
try {
  // 1. Press Start: the Main Menu at once, the world loading behind it.
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await setup(page, 'freeride');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').home?.loading);
  let ui = await read(page);
  assert.ok(!ui.front.choices.includes('hopin'), 'no Hop in choice before the world shows');
  report.loading = await turntable(page, 'desktop');
  if (shots && (await read(page)).home.loading) await page.screenshot({ path: `${shots}/freeride-menu-loading.png` });
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
  // The target sits on the world, under the rider and above Back.
  assert.ok(
    hop.rect.x >= world.rect.x && hop.rect.x + hop.rect.width <= 1280 && hop.rect.y > 720 * 0.55,
    `Hop in target on the world: ${JSON.stringify(hop.rect)}`
  );
  assert.ok(hop.rect.y + hop.rect.height <= back.rect.y, 'Hop in target above Back');
  assert.ok(hop.rect.height >= 48, 'Hop in touch target');
  assert.ok(world.rect.x > 1280 * 0.4, `the world on the stage: ${JSON.stringify(world.rect)}`);
  // The autopilot rides under the menus.
  let start = await origin(page);
  await nextFrames(page, 240);
  ui = await read(page);
  assert.equal(ui.home.piloting, true, 'the autopilot rides under the menus');
  report.piloted = flat(start, await origin(page));
  assert.ok(report.piloted > 10, `autopilot moved the rider: ${report.piloted} ft`);
  const camera = await sampleCamera(page, 4000);
  assertBackdrop(camera, 'desktop');
  report.backdrop = {
    yawRate: camera.yawRate,
    rise: camera.rise,
    clearance: camera.clearance,
    pitch: camera.pitch.at(-1),
    resets: camera.resets
  };
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-desktop.png` });

  // 2. Another stage page keeps the world; Back returns.
  await key(page, 'ArrowDown');
  ui = await read(page);
  const single = ui.front.choices.indexOf('single');
  while ((await read(page)).front.selection !== single) await key(page, 'ArrowDown');
  await key(page, 'Enter');
  await onFront(page, 'single');
  // The target follows once the page has slid in.
  await wait(
    page,
    id => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.hits?.some(h => h.id === id),
    HOP_IN,
    10000
  );
  ui = await read(page);
  assert.ok(ui.home.live && ui.home.world, 'the world stays behind Single Player');
  assert.ok(ui.front.hits.some(h => h.id === WORLD) && ui.front.hits.some(h => h.id === HOP_IN));
  if (shots) await page.screenshot({ path: `${shots}/freeride-menu-single.png` });
  await key(page, 'Escape');
  await onFront(page, 'main');

  // 3. Hop in: the same ride, the player's, with the ride's HUD.
  start = await origin(page);
  await page.mouse.click(hop.x, hop.y);
  // The chase camera blends back in, without a snap.
  const back2chase = await sampleCamera(page, 1800);
  report.hopCamera = {
    rise: back2chase.rise,
    yawRate: back2chase.yawRate,
    blend: back2chase.blend.at(-1),
    worst: back2chase.worst,
    resets: back2chase.resets
  };
  assert.equal(back2chase.blend.at(-1), 0, 'the chase camera after Hop in');
  assert.ok(back2chase.blend.some(b => b > 0 && b < 1), 'Hop in blends the camera');
  assert.ok(back2chase.rise < 45 && back2chase.yawRate < 3.5, `Hop in camera without a snap: ${JSON.stringify(report.hopCamera)}`);
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
  report.phoneLoading = await turntable(phone, 'phone');
  if (shots && (await read(phone)).home.loading) await phone.screenshot({ path: `${shots}/freeride-menu-phone-loading.png` });
  await live(phone);
  assert.ok(!(await pages(phone)).includes('loading'), 'no course loader on the phone');
  const tap = await frontHit(phone, HOP_IN);
  const top = await frontHit(phone, WORLD);
  report.phone = { hop: tap.rect, world: top.rect };
  assert.ok(
    tap.rect.y >= Math.max(0, top.rect.y) && tap.rect.y + tap.rect.height <= top.rect.y + top.rect.height,
    `Hop in target in the top stage: ${JSON.stringify(tap.rect)}`
  );
  assert.ok(tap.rect.height >= 48, 'Hop in touch target');
  assert.ok(top.rect.y + top.rect.height < 844 * 0.5, 'the world in the top stage');
  await nextFrames(phone, 60);
  if (shots) await phone.screenshot({ path: `${shots}/freeride-menu-phone.png` });
  await phone.touchscreen.tap(tap.x, tap.y);
  await riding(phone);
  await phone.close();
  // A phone held sideways.
  const side = await newPage(
    browser,
    { viewport: { width: 844, height: 390 }, deviceScaleFactor: 2, hasTouch: true, isMobile: true },
    errors
  );
  await setup(side, 'freeride');
  report.landscapeLoading = await turntable(side, 'landscape');
  if (shots && (await read(side)).home.loading) await side.screenshot({ path: `${shots}/freeride-menu-landscape-loading.png` });
  await live(side);
  const sideWorld = await frontHit(side, WORLD);
  const sideHop = await frontHit(side, HOP_IN);
  assert.ok(sideWorld.rect.x > 844 * 0.35, `the world on the sideways stage: ${JSON.stringify(sideWorld.rect)}`);
  assert.ok(sideHop.rect.height >= 48, 'Hop in touch target sideways');
  await nextFrames(side, 120);
  const sideCamera = await sampleCamera(side, 3000);
  assertBackdrop(sideCamera, 'landscape');
  report.landscape = { world: sideWorld.rect, pitch: sideCamera.pitch.at(-1), clearance: sideCamera.clearance };
  if (shots) await side.screenshot({ path: `${shots}/freeride-menu-landscape.png` });
  await side.close();

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
  // The fallback: the ATV keeps turning on the stage, without its name.
  assert.ok(ui.front.worldStage && ui.front.models[0], 'offline: the ATV on the stage');
  await nextFrames(offline, 20);
  assert.notEqual((await read(offline)).front.modelYaw[0], ui.front.modelYaw[0], 'offline: the turntable turns');
  if (shots) await offline.screenshot({ path: `${shots}/freeride-menu-offline.png` });
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
  assert.equal(ui.front.worldStage, false, 'Start in: Menu keeps the vehicle name');
  report.startMenu = ui.home;
  await menu.close();

  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
  console.log('PASS freeride main menu');
} finally {
  await browser.close();
}
