// Host-only gate: leaving and losing a room in the Rust engine, as
// web/multiplayer.js and game.js do:
// - Pause → Leave Game in a private room returns to the Multiplayer page,
//   in Arlington Free Ride to the main menu (multiplayer.js `leave`);
// - a room that closes for good (here: another tab reclaims this rider's slot
//   key, so the relay closes the socket with "Connection replaced") ends the
//   ride on the Multiplayer page with the reason on the notice line
//   (multiplayer.js onclose → game.js onLeave).
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_end.mjs
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const errors = [];
const stamp = String(Date.now()).slice(-6);
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 180000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, 60000);
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function choose(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  for (let i = f.selection; i > at; i--) await key(page, 'ArrowUp');
  await key(page, 'Enter');
}
// Riding in the room: the pause menu's Leave Game.
async function leaveGame(page) {
  await key(page, 'Escape');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause');
  const hit = (await read(page)).toolbar.menu.hits.find(h => h.action === 'leave');
  assert.ok(hit, 'Leave Game in the pause menu');
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
}
async function host(page, code) {
  await choose(page, 'create-lobby');
  await onPage(page, 'create-lobby');
  await page.getByLabel('Rider name', { exact: true }).fill('EndHost');
  await page.getByLabel('Lobby code (8 letters/numbers, optional)').fill(code);
  await page.keyboard.press('Enter');
  await nextFrames(page, 3);
  await choose(page, 'hosttrack');
  await onPage(page, 'tracks');
  await key(page, 'Enter');
  await wait(page, code => JSON.parse(window.furyMultiplayer ?? 'null')?.code === code, code);
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar);
}

const browser = await launch();
try {
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
  });
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await choose(page, 'multiplayer');
  await onPage(page, 'multiplayer');

  // 1. Leave Game from a private room: the Multiplayer page.
  await host(page, `LV${stamp}`);
  await leaveGame(page);
  await onPage(page, 'multiplayer');
  console.log('  private room: Leave Game → Multiplayer');

  // 2. The room closes for good: the ride ends on Multiplayer with the reason.
  const code = `EN${stamp}`;
  await host(page, code);
  const record = await page.evaluate(() => sessionStorage.getItem('atv-mp-session'));
  assert.ok(record && JSON.parse(record).code === code, `slot key kept: ${record}`);
  const thief = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await thief.addInitScript(record => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    sessionStorage.setItem('atv-mp-session', record);
  }, record);
  await thief.goto(url(`/play.html?uiDiagnostics=1&room=${code}&player=Thief&play=1`));
  await wait(
    page,
    () => {
      const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
      return f?.page === 'multiplayer' && f.notice;
    },
    null
  );
  const notice = (await read(page)).front.notice;
  assert.equal(notice, 'Connection replaced');
  await thief.close();
  console.log(`  room closed: the ride ended on Multiplayer with "${notice}"`);

  // 3. Leave Game from Arlington Free Ride: the main menu.
  await key(page, 'Escape');
  await onPage(page, 'main');
  await choose(page, 'freeride');
  await wait(page, () => /^FREERD/.test(JSON.parse(window.furyMultiplayer ?? 'null')?.code ?? ''));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar);
  await leaveGame(page);
  await onPage(page, 'main');
  console.log('  Free Ride: Leave Game → main menu');
  assertNoErrors(errors);
  console.log('PASS: Rust rooms leave and end on the JavaScript pages');
} finally {
  await browser.close();
}
