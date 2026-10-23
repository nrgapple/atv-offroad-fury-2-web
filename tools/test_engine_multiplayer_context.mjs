// WebGL context loss in a multiplayer room (game.js keeps its room through
// webglcontextlost / webglcontextrestored): a Rust host loses its context,
// stays in the room through the loss and, rebuilt in place, keeps its slot
// and position. The guest sees it throughout, and it drives on.
//   PLAYWRIGHT_MODULE=... node tools/run_browser_tests.mjs --only test_engine_multiplayer_context.mjs --url http://127.0.0.1:4187
import assert from 'node:assert/strict';
import { assertNoErrors, base, launch, newPage, nextFrames } from './browser/harness.mjs';

const errors = [];
const room = page => page.evaluate(() => JSON.parse(window.furyMultiplayer ?? 'null'));
const ui = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const distance = (a, b) => Math.hypot(a[0] - b[0], a[2] - b[2]);

const browser = await launch();
try {
  const open = async () => {
    // Separate contexts: two players, each with focus.
    const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
    await page.addInitScript(() => {
      Object.defineProperty(document, 'hasFocus', { value: () => true });
      localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    });
    return page;
  };
  const host = await open();
  await host.goto(`${base}/play.html?venue=env01&host=1&name=LostHost&uiDiagnostics=1`);
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.code);
  const { code, peer } = await room(host);
  const guest = await open();
  await guest.goto(`${base}/play.html?room=${code}&name=Guest&uiDiagnostics=1`);
  await wait(guest, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, 180000);
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1, null, 180000);

  await host.bringToFront();
  await host.mouse.click(640, 400);
  await host.keyboard.down('KeyW'); await host.waitForTimeout(2000); await host.keyboard.up('KeyW');
  await host.evaluate(() => { window.__samePage = true; });
  const before = await ui(host);
  await host.evaluate(() => {
    window.__context = document.querySelector('#game').getContext('webgl2').getExtension('WEBGL_lose_context');
    window.__context.loseContext();
  });
  await wait(host, () => document.querySelector('#status').textContent === 'Restoring graphics…');
  // Several presence periods without a context: the room keeps the host.
  await guest.waitForTimeout(6000);
  const during = await room(guest);
  assert.ok(during.remote.some(r => r.peer === peer), `the guest still has the host: ${JSON.stringify(during.remote)}`);
  assert.equal((await room(host)).closed, null, 'the host is still connected');
  await host.evaluate(() => window.__context.restoreContext());
  await wait(host, () => document.querySelector('#status').textContent !== 'Restoring graphics…');
  await nextFrames(host, 20);
  const after = await ui(host);
  const hostRoom = await room(host);
  assert.equal(await host.evaluate(() => window.__samePage), true, 'rebuilt in place (no reload)');
  assert.equal(hostRoom.code, code);
  assert.equal(hostRoom.peer, peer, 'the same slot');
  assert.equal(hostRoom.closed, null);
  assert.ok(distance(after.origin, before.origin) < 30, 'the ride stays where it was');
  // The guest's model comes back on the new device, and the host drives on.
  await wait(host, () => JSON.parse(window.furyMultiplayer ?? 'null')?.drawn >= 1);
  const seen = (await room(guest)).remote.find(r => r.peer === peer).position;
  await host.keyboard.down('KeyW'); await host.waitForTimeout(2500); await host.keyboard.up('KeyW');
  await wait(guest, ([peer, seen]) => {
    const r = JSON.parse(window.furyMultiplayer ?? 'null')?.remote?.find(r => r.peer === peer);
    return r && Math.hypot(r.position[0] - seen[0], r.position[2] - seen[2]) > 5;
  }, [peer, seen], 30000);
  assertNoErrors(errors);
  console.log(`PASS Rust WebGL context loss in a room: slot ${peer} of ${code} kept, rebuilt in place`);
} finally {
  await browser.close();
}
