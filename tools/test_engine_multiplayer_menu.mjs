// Host-only gate: the Rust engine's Multiplayer menu. A Rust browser hosts
// from Main Menu → Multiplayer → Host Game (name and custom code typed into
// the form's browser inputs) → Select Track → Ride; a JavaScript player joins
// by the code; a second Rust browser joins from Join Game. Host and Join show
// the Free Ride Lobby page while the room connects and then its riders (each
// held 1.5 s with ?joinDelayMs); the guest first leaves it with Back, then
// joins.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_menu.mjs
// ATV_MP_SCREENSHOT=out.png saves the Host and Join forms and the lobby page.
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const errors = [];
const code = `RM${String(Date.now()).slice(-6)}`;
const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const onPage = (page, name) =>
  page.waitForFunction(
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    { timeout: 60000 }
  );
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
// Main Menu → the row for `action`, then Enter.
async function choose(page, action) {
  const f = (await read(page)).front;
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = 0; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
}
async function open(page) {
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
    Object.defineProperty(document, 'hasFocus', { value: () => true });
  });
  await page.goto(url('/engine.html?uiDiagnostics=1&joinDelayMs=1500'));
  await onPage(page, 'title');
  await page.mouse.click(8, 8);
  await key(page, 'Enter');
  await onPage(page, 'main');
  await choose(page, 'multiplayer');
  await onPage(page, 'multiplayer');
}
const browser = await launch();
try {
  // 1. Rust host from the menus.
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await open(host);
  await choose(host, 'create-lobby');
  await onPage(host, 'create-lobby');
  await host.getByLabel('Rider name', { exact: true }).fill('RustHost');
  await host.getByLabel('Lobby code (8 letters/numbers, optional)').fill(code.toLowerCase());
  await host.keyboard.press('Escape');
  await nextFrames(host, 3);
  if (shots) await host.screenshot({ path: shots.replace('.png', '-host-form.png') });
  await host.mouse.click(8, 8);
  await choose(host, 'hosttrack');
  await onPage(host, 'tracks');
  await key(host, 'Enter');
  // The Free Ride Lobby page while the room connects, then the loader.
  await onPage(host, 'lobby');
  const lobby = (await read(host)).front;
  assert.equal(lobby.lobby?.code, code);
  assert.deepEqual(lobby.choices, ['share', 'leave']);
  // Once connected the page lists the room's riders (multiplayer.js
  // lobby_status), held 1.5 s by ?joinDelayMs, then the loader follows.
  await host.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.lobby?.status,
    null,
    { timeout: 60000 }
  );
  const connected = (await read(host)).front.lobby;
  assert.equal(connected.status, '1/4 riders / RustHost');
  report.lobby = { code: lobby.lobby.code, connecting: lobby.lobby.status, connected: connected.status, choices: lobby.choices };
  if (shots) await host.screenshot({ path: shots.replace('.png', '-lobby.png') });
  await host.waitForFunction(c => JSON.parse(window.furyMultiplayer ?? 'null')?.code === c, code, { timeout: 180000 });
  let e = await engine(host);
  assert.equal(e.host, true);
  report.host = { code: e.code, environment: e.environment };

  // 2. A JavaScript player joins by the code.
  const js = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await js.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await js.goto(`${base}/play.html?room=${code}&player=JsGuest`);
  await js.waitForFunction(() => window.atvTerrainStatus?.multiplayer?.remote?.some(r => !r.stale), null, {
    timeout: 180000
  });
  await host.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, { timeout: 60000 });

  // 3. A second Rust browser joins from Join Game.
  const guest = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await open(guest);
  await choose(guest, 'join-lobby');
  await onPage(guest, 'join-lobby');
  await guest.getByLabel('Rider name', { exact: true }).fill('RustGuest');
  await guest.getByLabel('Lobby code', { exact: true }).fill(code);
  await guest.keyboard.press('Escape');
  await nextFrames(guest, 3);
  if (shots) await guest.screenshot({ path: shots.replace('.png', '-join-form.png') });
  await guest.mouse.click(8, 8);
  // Join, then Back on the lobby page: Leave Game before the room connects.
  await choose(guest, 'join');
  await onPage(guest, 'lobby');
  await key(guest, 'Escape');
  await onPage(guest, 'multiplayer');
  await guest.waitForTimeout(2500); // Time-based on purpose: the held join would have finished.
  assert.equal((await engine(guest))?.code, undefined, 'Leave Game abandoned the join');
  assert.equal((await read(guest)).front.page, 'multiplayer');
  report.leftLobby = true;
  await choose(guest, 'join-lobby');
  await onPage(guest, 'join-lobby');
  await guest.mouse.click(8, 8);
  await choose(guest, 'join');
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 2, null, {
    timeout: 180000
  });
  e = await engine(guest);
  report.guest = { code: e.code, peer: e.peer, drawn: e.drawn, players: e.players };
  assert.equal(e.code, code);
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
