// Host-only gate: the Rust engine's online pause rows in a room with a
// JavaScript host. Pause shows Players, Copy invite link and Leave Game;
// Players → Spawn beside the host places the guest next to it; Copy invite
// link copies the room's invite; Leave Game leaves the room.
//   PLAYWRIGHT_MODULE=... ATV_TEST_URL=http://127.0.0.1:4187 node tools/test_engine_multiplayer_pause.mjs
// ATV_MP_SCREENSHOT=out.png saves the pause and Players pages.
import assert from 'node:assert/strict';
import { assertNoErrors, base, button, holdKey, launch, newPage, nextFrames } from './browser/harness.mjs';

const shots = process.env.ATV_MP_SCREENSHOT;
const report = {};
const errors = [];
const browser = await launch();
const engine = p => p.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const ui = p => p.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const centre = hit => ({ x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 });
const distance = (a, b) => Math.hypot(...a.map((v, i) => v - b[i]));
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function click(page, action, settle = true) {
  const menu = (await ui(page)).toolbar?.menu;
  const hit = menu?.hits?.find(h => h.action === action);
  assert.ok(hit, `${action} in ${JSON.stringify(menu?.choices)}`);
  await page.mouse.click(centre(hit).x, centre(hit).y);
  if (settle) await nextFrames(page, 3);
}
try {
  const host = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await host.addInitScript(() => Object.defineProperty(document, 'hasFocus', { value: () => true }));
  await host.goto(base + '/play.html');
  await button(host, 'Press START').click({ timeout: 90000 });
  await button(host, 'Multiplayer').click();
  await button(host, 'Host Game').click();
  await host.getByLabel('Rider name').fill('JsHost');
  await button(host, 'Select Track').click();
  await host.locator('button.ms-ride').click({ timeout: 60000 });
  await host.waitForFunction(() => window.atvTerrainStatus?.multiplayer?.state?.code, null, { timeout: 120000 });
  const code = (await host.evaluate(() => window.atvTerrainStatus)).multiplayer.state.code;

  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  await context.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: base });
  const guest = await context.newPage();
  guest.on('pageerror', e => errors.push(e.message));
  await guest.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
  });
  await guest.goto(`${base}/engine.html?room=${code}&name=RustGuest&uiDiagnostics=1`);
  await guest.waitForFunction(() => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, {
    timeout: 180000
  });
  // The host rides away first.
  await host.bringToFront();
  await holdKey(host, 'KeyW', 240);
  await guest.bringToFront();
  let e = await engine(guest);
  const hostAt = (await host.evaluate(() => window.atvTerrainStatus.frame.origin)).slice();
  report.before = distance(e.origin, hostAt);

  // Pause: the online rows.
  await guest.mouse.click(640, 400);
  await key(guest, 'Escape');
  await guest.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause'
  );
  const choices = (await ui(guest)).toolbar.menu.choices;
  report.pause = choices;
  for (const row of ['players', 'copyride', 'leave']) assert.ok(choices.includes(row), `${row} in ${choices}`);
  if (shots) await guest.screenshot({ path: shots.replace('.png', '-pause.png') });

  // Copy invite link.
  await click(guest, 'copyride');
  await guest.waitForTimeout(300);
  const copied = await guest.evaluate(() => navigator.clipboard.readText());
  assert.match(copied, new RegExp(`room=${code}`));
  report.invite = copied;

  // Players → Spawn beside the host.
  await click(guest, 'players');
  await guest.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'players'
  );
  if (shots) await guest.screenshot({ path: shots.replace('.png', '-players.png') });
  await click(guest, 'spawn:0');
  await guest.waitForFunction(() => !JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu, null, {
    timeout: 10000
  });
  await nextFrames(guest, 10);
  e = await engine(guest);
  const hostNow = await host.evaluate(() => window.atvTerrainStatus.frame.origin);
  report.afterSpawn = distance(e.origin, hostNow);
  assert.ok(report.afterSpawn < 40, `spawned beside the host: ${report.afterSpawn} ft (was ${report.before})`);

  // Leave Game.
  const rustPeer = e.peer;
  await key(guest, 'Escape');
  await guest.waitForFunction(
    () => JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu?.page === 'pause'
  );
  await click(guest, 'leave', false); // Leave Game returns to the main menu.
  await host.waitForFunction(
    peer => !(window.atvTerrainStatus.multiplayer?.state?.lobby?.players ?? []).some(p => p.id === peer),
    rustPeer,
    { timeout: 30000 }
  );
  report.left = true;
  assertNoErrors(errors);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
