// Host-only gate: Arlington Free Ride from the Rust engine's main menu, in the
// same shared room as the JavaScript game, on a local relay with content.
//   1. The Rust main menu lists Arlington Free Ride with its live rider count.
//   2. Choosing it claims a shared-room slot and rides Arlington (env08).
//   3. A JavaScript player taps Arlington Free Ride; both see each other.
//      A shared room's invite link (FREERDnn) opens Free Ride too.
//   4. Pause → Leave Game leaves the room; the JavaScript player sees it go.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_freeride.mjs
// ATV_MP_SCREENSHOT=out.png saves the menu and both rides.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, launch, newPage, nextFrames, url, watchErrors } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 60000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));

const browser = await launch();
const errors = [];
try {
  // 1. Rust main menu with the live count.
  const rust = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await rust.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await rust.goto(url('/engine.html?uiDiagnostics=1'));
  await onPage(rust, 'title');
  await rust.mouse.click(8, 8);
  await key(rust, 'Enter');
  await onPage(rust, 'main');
  await wait(rust, () =>
    Number.isInteger(JSON.parse(document.querySelector('#ui-state').textContent).front.freerideRiders)
  );
  let f = (await read(rust)).front;
  report.menu = { choices: f.choices, riders: f.freerideRiders };
  assert.equal(f.choices[2], 'freeride');
  if (shots) await rust.screenshot({ path: shots.replace('.png', '-menu.png') });

  // 2. Choose Arlington Free Ride.
  await key(rust, 'ArrowDown');
  await key(rust, 'ArrowDown');
  await key(rust, 'Enter');
  await wait(rust, () => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, 180000);
  let e = await engine(rust);
  assert.match(e.code, /^FREERD\d\d$/);
  report.rust = { code: e.code, peer: e.peer, players: e.players };

  // 3. A JavaScript player joins Free Ride from its main menu.
  const js = await (await browser.newContext({ viewport: { width: 1280, height: 720 }, hasTouch: true })).newPage();
  watchErrors(js, errors);
  await js.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await js.goto(base + '/play.html');
  await button(js, 'Press START').click({ timeout: 90000 });
  await button(js, 'Arlington Free Ride').click();
  await js.waitForFunction(
    () => {
      const s = window.atvTerrainStatus;
      return (
        s?.multiplayerError || (s?.simulation && s.multiplayer?.remote?.some(r => !r.stale) && s.multiplayerDraws > 0)
      );
    },
    null,
    { timeout: 180000 }
  );
  let s = await js.evaluate(() => window.atvTerrainStatus);
  assert.equal(s.multiplayerError, undefined, s.multiplayerError);
  assert.equal(s.multiplayer.state.code, e.code, 'both ride the same shared room');
  await wait(rust, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, 60000);
  e = await engine(rust);
  report.together = { jsPeer: s.multiplayer.state.peer, rustDrawn: e.drawn, met: e.met, jsDraws: s.multiplayerDraws };
  if (shots) {
    await rust.screenshot({ path: shots.replace('.png', '-rust.png') });
    await js.screenshot({ path: shots.replace('.png', '-js.png') });
  }

  // 3b. A shared room's invite (FREERDnn) opens Free Ride on Arlington, as
  // multiplayer.js `host` does, instead of a refused room claim.
  const invited = await newPage(browser, { viewport: { width: 960, height: 540 } }, errors);
  await invited.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await invited.goto(url(`/engine.html?uiDiagnostics=1&mode=multiplayer&room=${e.code}&map=env01&play=1`));
  await wait(invited, () => JSON.parse(window.furyMultiplayer ?? 'null')?.environment === 'env08', null, 180000);
  const invite = await engine(invited);
  assert.match(invite.code, /^FREERD\d\d$/);
  assert.equal(invite.closed, null);
  report.invite = { code: invite.code, peer: invite.peer };
  await invited.close();

  // 4. Leave Game leaves the room.
  const rustPeer = e.peer;
  await rust.bringToFront();
  await rust.mouse.click(640, 400);
  await key(rust, 'Escape');
  await wait(rust, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  const quit = (await read(rust)).toolbar.menu.hits.find(h => h.action === 'leave');
  assert.ok(quit, 'Leave Game');
  await rust.mouse.click(centre(quit).x, centre(quit).y);
  await onPage(rust, 'main');
  await js.waitForFunction(
    peer => !(window.atvTerrainStatus.multiplayer?.state?.lobby?.players ?? []).some(p => p.id === peer),
    rustPeer,
    { timeout: 60000 }
  );
  report.left = true;
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
