// Switch-over gaps 4, 5 and 12 (research/SWITCHOVER.md) in the Rust engine's
// browser build, against the JavaScript game's link handling (launch.js,
// frontend.js openLaunch) and context-loss recovery:
// - mode=multiplayer links open Create/Join Lobby with the link's code and
//   name (or the saved name); play=1 hosts at once and a failure returns to the
//   form; motorcycles are refused with the JavaScript message.
// - race links keep cheats= and motorcycle races are no longer refused.
// - a lost WebGL context shows "Restoring graphics…" and, once restored, the
//   ride rebuilds in place (tools/test_engine_context_loss.mjs); the ride's
//   link stays in `fury-resume` for a device that cannot reopen. Host
//   Chromium evidence (no relay here).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onPage = (page, name) => wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
const status = page => page.evaluate(() => document.querySelector('#status').textContent);

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 } }, errors);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 5, riderId: 1, playerName: 'Saved Sam' }));
  });
  // Join link without play: the form, the link's code, the saved name.
  await page.goto(url('/play.html?uiDiagnostics=1&mode=multiplayer&action=join&room=abcd1234&map=env02&quad=3&rider=0&play=0'));
  await onPage(page, 'join-lobby');
  let f = (await read(page)).front;
  assert.deepEqual([f.roomCode, f.playerName, f.quad, f.track], ['ABCD1234', 'Saved Sam', 3, 'env02']);
  // The lobby= alias and a named player.
  await page.goto(url('/play.html?uiDiagnostics=1&lobby=WXYZ9876&player=Ann&play=0'));
  await onPage(page, 'join-lobby');
  f = (await read(page)).front;
  assert.deepEqual([f.roomCode, f.playerName], ['WXYZ9876', 'Ann']);
  // action=host: Create Lobby; with play=1 it hosts at once and, with no
  // relay on this server, returns to the form with the error.
  await page.goto(url('/play.html?uiDiagnostics=1&mode=multiplayer&action=host&room=HOST1234&play=0'));
  await onPage(page, 'create-lobby');
  assert.equal((await read(page)).front.createCode, 'HOST1234');
  await page.goto(url('/play.html?uiDiagnostics=1&mode=multiplayer&action=create&play=1'));
  await wait(page, () => { const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front; return f?.page === 'create-lobby' && f.notice; });
  // A motorcycle multiplayer link: the JavaScript refusal on Main.
  await page.goto(url('/play.html?uiDiagnostics=1&debug=1&mode=multiplayer&action=create&quad=29&play=1'));
  await onPage(page, 'main');
  assert.match(await status(page), /Choose an ATV for multiplayer/);
  // A motorcycle race link with cheats starts the race (was refused).
  // engine.html, play.html's pre-switch name, still serves the same page.
  await page.goto(url('/engine.html?uiDiagnostics=1&debug=1&activity=race&map=env01&quad=29&laps=1&cpu=3&cheats=eatdirt,bringit&play=1'));
  await wait(page, () => /race intro/.test(document.querySelector('#status')?.textContent), null, 180000);
  assert.doesNotMatch(await status(page), /Cannot open ride link/);
  console.log('  links: join/lobby/host forms, play=1 host failure back to the form, motorcycle refusal, motorcycle race with cheats');
  // Context loss on a direct ride: the notice, then the same page rides on.
  await page.goto(url('/play.html?venue=env03&quad=2&uiDiagnostics=1'));
  await wait(page, () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.settings != null);
  const resume = await page.evaluate(() => sessionStorage.getItem('fury-resume'));
  assert.match(resume, /map=env03&quad=2&rider=0.*play=1/);
  await page.evaluate(() => {
    const gl = document.querySelector('canvas').getContext('webgl2');
    window.loseContext = gl.getExtension('WEBGL_lose_context');
    window.loseContext.loseContext();
  });
  await wait(page, () => document.querySelector('#status').textContent === 'Restoring graphics…'
    && getComputedStyle(document.querySelector('#status')).display !== 'none', null, 10000);
  await page.evaluate(() => window.loseContext.restoreContext());
  await wait(page, () => /^ENV03 · \d+ fps/.test(document.querySelector('#status')?.textContent), null, 180000);
  assert.match(page.url(), /venue=env03&quad=2/, 'rebuilt in place, not reopened');
  console.log('  context loss: "Restoring graphics…", then the ride rebuilt in place');
  assertNoErrors(errors.filter(e => !/CONTEXT_LOST|context lost|WebGL/i.test(String(e))));
  console.log('PASS: Rust multiplayer and race links, WebGL context-loss recovery');
} finally {
  await browser.close();
}
