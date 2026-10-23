// Rust engine menu parity with the JavaScript game (research/SWITCHOVER.md):
// - ride links: a bad link explains itself in the main menu notice; a bare
//   `?room=` link opens Join Lobby with its code, player and vehicle alias
//   (launch.js parseLaunch); a race link's cheats survive Copy ride link.
// - Garage previews: a pressed preview turns with Left/Right (0.25 rad) and
//   the garage keeps its vehicle; a changed vehicle shows its status first.
// - Pause in a race: Restart Race; menus hide the keyboard card and Ctrl+H
//   leaves it alone; Bindings → Left/Right turn the page and Clear binding
//   clears the highlighted row.
// Host Chromium evidence (frontend.js, keyboard-help.js, race.js).
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const wait = (page, predicate, arg, timeout = 120000) => page.waitForFunction(predicate, arg, { timeout });
const onFront = (page, name) =>
  wait(page, name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name);
const onMenu = (page, name) =>
  wait(
    page,
    name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === name,
    name,
    30000
  );
async function key(page, code) {
  await page.keyboard.down(code);
  await nextFrames(page, 2);
  await page.keyboard.up(code);
  await nextFrames(page, 3);
}
async function clickHit(page, hit) {
  await page.mouse.click(hit.x + hit.width / 2, hit.y + hit.height / 2);
  await nextFrames(page, 4);
}
async function clickMenu(page, action) {
  const m = (await read(page)).toolbar.menu,
    hit = m.hits.find(h => h.action === action);
  assert.ok(hit, `${action} on ${m.page}: ${m.choices}`);
  await clickHit(page, hit);
}
const SHARE = 2412,
  MODEL_QUAD = 2409;

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(
    browser,
    { viewport: { width: 1280, height: 720 }, permissions: ['clipboard-read', 'clipboard-write'] },
    errors
  );
  await page.addInitScript(() => {
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.setItem('atv-player-selection', JSON.stringify({ quadId: 5, riderId: 0, playerName: 'Saved Sam' }));
  });

  // A bad ride link: the main menu says why (frontend.js openLaunch).
  await page.goto(url('/play.html?uiDiagnostics=1&map=nowhere'));
  await onFront(page, 'main');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.notice);
  assert.match((await front(page)).notice, /^Cannot open ride link\. Unsupported map: nowhere\./);
  // A bare room link joins through Join Lobby: code, player, vehicle alias.
  await page.goto(url('/play.html?uiDiagnostics=1&room=abcd1234&player=Ann&vehicle=4&play=0'));
  await onFront(page, 'join-lobby');
  let f = await front(page);
  assert.deepEqual([f.roomCode, f.playerName, f.quad], ['ABCD1234', 'Ann', 4]);
  // ... and refuses a motorcycle, as launch.js does.
  await page.goto(url('/play.html?uiDiagnostics=1&room=abcd1234&quad=29'));
  await onFront(page, 'main');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.notice);
  assert.match((await front(page)).notice, /motorcycles are Single Player only|Choose an ATV/);
  // A race link with cheats and play=0: Copy ride link keeps them.
  await page.goto(
    url('/play.html?uiDiagnostics=1&activity=race&map=env02&laps=2&cpu=3&cheats=eatdirt,bringit&play=0')
  );
  await onFront(page, 'race');
  await clickHit(
    page,
    (await front(page)).hits.find(h => h.id === SHARE)
  );
  await wait(
    page,
    () => JSON.parse(document.querySelector('#ui-state').textContent).front.notice === 'Ride link copied.',
    null,
    10000
  );
  const link = await page.evaluate(() => navigator.clipboard.readText());
  assert.match(link, /activity=race&map=env02&.*laps=2&cpu=3.*cheats=eatdirt%2Cbringit/);
  console.log(`  links: bad-link notice, bare room link to Join Lobby, motorcycle refusal, race cheats in ${link}`);

  // Garage: a pressed preview turns with the arrows; the vehicle stays.
  await key(page, 'Escape');
  await onFront(page, 'single');
  f = await front(page);
  await clickHit(
    page,
    f.hits.find(h => h.id === 2000 + f.choices.indexOf('garage'))
  );
  await onFront(page, 'garage');
  await wait(page, () => JSON.parse(document.querySelector('#ui-state').textContent).front.models[0]);
  f = await front(page);
  const quad = f.quad,
    yaw = f.modelYaw[0];
  const preview = f.hits.find(h => h.id === MODEL_QUAD);
  await clickHit(page, preview);
  await key(page, 'ArrowRight');
  await key(page, 'ArrowRight');
  f = await front(page);
  assert.equal(f.quad, quad, 'the focused preview took the arrows');
  assert.ok(Math.abs(f.modelYaw[0] - (yaw - 0.5)) < 1e-4, `turned ${yaw} → ${f.modelYaw[0]}`);
  // Another key gives the arrows back to the garage: the next vehicle,
  // which shows "Loading model..." until it is read.
  await key(page, 'ArrowDown');
  await key(page, 'ArrowUp');
  const seen = page.evaluate(
    quad =>
      new Promise(resolve => {
        const states = new Set();
        const poll = () => {
          const f = JSON.parse(document.querySelector('#ui-state').textContent).front;
          if (f.quad !== quad) states.add(String(f.modelStatus[0]));
          if (f.quad !== quad && f.models[0]?.startsWith(`vehicle:${f.quad}:`) && f.modelStatus[0] === null)
            resolve([...states]);
          else requestAnimationFrame(poll);
        };
        poll();
      }),
    quad
  );
  await page.keyboard.press('ArrowRight');
  const statuses = await seen;
  f = await front(page);
  assert.notEqual(f.quad, quad, 'Right steps the garage again');
  assert.ok(statuses.includes('Loading model...'), `statuses ${statuses}`);
  console.log(`  garage: preview turned by 0.5 rad with the arrows; next vehicle statuses ${statuses.join(', ')}`);

  // A race ride: the first-ride keyboard card, Restart Race, bindings.
  await page.evaluate(() => localStorage.removeItem('atv2-keyboard-help-seen-v1'));
  await page.goto(url('/play.html?venue=env01&race=1&laps=1&cpu=0&uiDiagnostics=1'));
  await wait(
    page,
    () => JSON.parse(document.querySelector('#ui-state')?.textContent ?? 'null')?.toolbar?.hud?.helpOpen === true
  );
  await page.mouse.click(640, 400);
  await page.keyboard.down('ControlLeft');
  await key(page, 'KeyH');
  await page.keyboard.up('ControlLeft');
  assert.equal((await read(page)).toolbar.hud.helpOpen, true, 'Ctrl+H leaves the card');
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  let state = await read(page);
  assert.equal(state.toolbar.hud.helpOpen, false, 'the menu hides the card');
  assert.equal(state.toolbar.menu.labels.reset, 'Restart Race');
  // Bindings: Right turns the page from a binding row (no capture).
  await clickMenu(page, 'settings');
  await onMenu(page, 'settings');
  await clickMenu(page, 'controls');
  await onMenu(page, 'controls');
  await clickMenu(page, 'keyboard');
  await onMenu(page, 'bindings');
  await key(page, 'ArrowRight');
  state = await read(page);
  assert.equal(state.toolbar.menu.choices[0], 'bind:leanForward');
  await key(page, 'ArrowLeft');
  await key(page, 'ArrowDown');
  await key(page, 'ArrowDown');
  state = await read(page);
  assert.equal(state.toolbar.menu.choices[state.toolbar.menu.selection], 'bind:steerLeft');
  await clickMenu(page, 'bindclear');
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('atv2-gameplay-bindings-v1')));
  assert.deepEqual(saved.keyboard.steerLeft, []);
  assert.ok(saved.keyboard.throttle.length > 0, 'Gas keeps its keys');
  // Back to Pause; Restart Race reloads the event and resumes.
  await key(page, 'Escape');
  await key(page, 'Escape');
  await key(page, 'Escape');
  await onMenu(page, 'pause');
  await clickMenu(page, 'reset');
  await wait(page, () => !JSON.parse(document.querySelector('#ui-state').textContent).toolbar?.menu, null, 30000);
  assert.equal((await read(page)).paused, false);
  console.log(
    '  ride: first-ride card hidden by the menu, Ctrl+H ignored, Restart Race, binding page turn and Clear binding row'
  );
  assertNoErrors(errors);
  console.log('PASS: Rust menu parity: links, garage previews, pause and bindings');
} finally {
  await browser.close();
}
