// Rust engine menu flows against the JavaScript game (research/SWITCHOVER.md):
// - an idle menu answers the controller at once: D-pad and left stick
//   (frontend.js polls every animation frame, axes past ±0.55), while drawing
//   no frames when nothing changes;
// - Host/Join text fields keep the page focused (menu music plays on), and
//   Enter or Tab hands the keyboard back to the canvas without a click;
// - a ride load that fails returns to the page it started from with the error
//   on the notice line (frontend.js `fail`): Freestyle → course select,
//   Race → race setup.
// Host Chromium evidence with a stand-in standard gamepad.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onFront = (page, name, timeout) =>
  wait(
    page,
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name,
    name,
    timeout
  );
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
// Main Menu style pages: Down to the row for `action`, then Enter.
async function choose(page, action) {
  const f = await front(page);
  const at = f.choices.indexOf(action);
  assert.ok(at >= 0, `${action} in ${f.choices}`);
  for (let i = f.selection; i < at; i++) await key(page, 'ArrowDown');
  await key(page, 'Enter');
}
// Time from a pad change until the menu shows `predicate` (ms).
async function padUntil(page, change, predicate, arg) {
  const started = Date.now();
  await page.evaluate(change);
  await wait(page, predicate, arg, 5000);
  return Date.now() - started;
}
const selection = n => JSON.parse(document.querySelector('#ui-state').textContent).front.selection === n;

const browser = await launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 1280, height: 720 }, serviceWorkers: 'block' }, errors);
  // Single Player picks follow the profile's unlocks (PROFILE_STORE.md).
  await unlockStore(page);
  await page.addInitScript(() => {
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
    const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
    const pad = {
      id: 'Menu stand-in (STANDARD GAMEPAD)',
      index: 0,
      connected: true,
      mapping: 'standard',
      timestamp: 1,
      axes: [0, 0, 0, 0],
      buttons
    };
    window.padButton = (index, down) => {
      buttons[index] = { pressed: down, touched: down, value: down ? 1 : 0 };
      pad.timestamp++;
    };
    window.padAxis = (index, value) => {
      pad.axes[index] = value;
      pad.timestamp++;
    };
    navigator.getGamepads = () => [pad];
  });
  await page.goto(url('/play.html?uiDiagnostics=1'));
  await onFront(page, 'title');
  await page.mouse.click(8, 8);
  // A on the title opens the main menu.
  await page.evaluate(() => window.padButton(0, true));
  await onFront(page, 'main', 10000);
  await page.evaluate(() => window.padButton(0, false));
  // Idle: once the slide-in is over, the main menu draws nothing.
  await page.waitForTimeout(1500); // Time-based on purpose: the page settles and sleeps.
  const before = (await read(page)).frontFrames;
  await page.waitForTimeout(1000); // Time-based on purpose: proving no frames are drawn.
  const idle = (await read(page)).frontFrames - before;
  assert.ok(idle <= 1, `idle main menu drew ${idle} frames in 1 s`);
  // The D-pad and the left stick move the selection at once (not on the
  // 15 s Free Ride refresh).
  assert.equal((await front(page)).selection, 0);
  const dpad = await padUntil(page, () => window.padButton(13, true), selection, 1);
  await page.evaluate(() => window.padButton(13, false));
  await nextFrames(page, 3);
  const stick = await padUntil(page, () => window.padAxis(1, 0.9), selection, 2);
  await page.evaluate(() => window.padAxis(1, 0));
  await nextFrames(page, 3);
  const up = await padUntil(page, () => window.padAxis(1, -0.9), selection, 1);
  await page.evaluate(() => window.padAxis(1, 0));
  for (const ms of [dpad, stick, up]) assert.ok(ms < 1500, `pad reached the idle menu after ${ms} ms`);
  console.log(`  idle main menu: ${idle} frames in 1 s; D-pad ${dpad} ms, stick down ${stick} ms, stick up ${up} ms`);

  // Join Game's text fields: the page stays focused and the menu tune plays
  // on while typing; Enter and Tab give the canvas the keyboard back.
  await page.waitForTimeout(300); // Time-based on purpose: let the stick release settle.
  await choose(page, 'multiplayer');
  await onFront(page, 'multiplayer');
  await choose(page, 'join-lobby');
  await onFront(page, 'join-lobby');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).menuMusic?.playing === true);
  await page.getByLabel('Rider name', { exact: true }).fill('Kim');
  await nextFrames(page, 6);
  let state = await read(page);
  assert.equal(state.focused, true, 'a focused text field is not a blur');
  assert.equal(state.menuMusic.playing, true, 'the menu tune plays on while typing');
  assert.equal(state.front.playerName, 'Kim');
  await page.keyboard.press('Enter');
  await nextFrames(page, 3);
  assert.equal(await page.evaluate(() => document.activeElement?.id), 'game', 'Enter gives the canvas the keyboard');
  const row = (await front(page)).selection;
  await key(page, 'ArrowDown');
  assert.notEqual((await front(page)).selection, row, 'the arrows work without a click');
  // Tab out of the code field moves along the menu's focus order.
  await page.getByLabel('Lobby code', { exact: true }).click();
  await nextFrames(page, 3);
  assert.equal((await front(page)).focus, 1);
  await page.keyboard.press('Tab');
  await nextFrames(page, 3);
  let f = await front(page);
  assert.deepEqual([f.focus, f.selection], [null, 0]);
  assert.equal(await page.evaluate(() => document.activeElement?.id), 'game', 'Tab gives the canvas the keyboard');
  // Shift+Tab from the first row goes back to the code field.
  await page.keyboard.down('ShiftLeft');
  await key(page, 'Tab');
  await page.keyboard.up('ShiftLeft');
  assert.equal((await front(page)).focus, 1);
  console.log('  text fields: focus kept, menu tune on, Enter/Tab back to the canvas, Shift+Tab to the code');

  // A failed load returns to its page with the error (frontend.js `fail`).
  // Escape in a field still goes back a page (frontend.js).
  await key(page, 'Escape');
  await onFront(page, 'multiplayer');
  await key(page, 'Escape');
  await onFront(page, 'main');
  await choose(page, 'single');
  await onFront(page, 'single');
  await choose(page, 'tracks');
  await onFront(page, 'tracks');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.photos > 0);
  await page.route('**/*.bin', route => route.abort());
  await key(page, 'Enter');
  const failed = name =>
    wait(
      page,
      name => {
        const f = JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front;
        return f?.page === name && f.notice;
      },
      name,
      90000
    );
  await failed('tracks');
  f = await front(page);
  // frontend.js shows error.message: a refused fetch is the browser's TypeError.
  assert.equal(f.notice, 'Failed to fetch', 'the course select says why the ride failed');
  console.log(`  Freestyle load failure: back on the course select with "${f.notice}"`);
  if (f.race.courses > 0) {
    // An HTTP error reads as content.js words it.
    await page.unroute('**/*.bin');
    await page.route('**/*.bin', route => route.fulfill({ status: 503, body: '' }));
    await key(page, 'Escape');
    await onFront(page, 'single');
    await choose(page, 'race');
    await onFront(page, 'race');
    await choose(page, 'racestart');
    await failed('race');
    f = await front(page);
    assert.equal(f.notice, 'Original game content could not load (503).');
    console.log(`  Race load failure: back on the race setup with "${f.notice}"`);
  }
  await page.unroute('**/*.bin');
  // The refused requests' own rejections are expected; nothing else may fail.
  assertNoErrors(errors.filter(e => e !== 'Failed to fetch'));
  await page.close();
  console.log('PASS: Rust idle menus take the pad, text fields keep focus, failed loads return with the error');
} finally {
  await browser.close();
}
