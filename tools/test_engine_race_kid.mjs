// Single Race's Kid difficulty (research/KID_MODE.md), a port enhancement, on
// a phone: the setup's Difficulty row cycles Normal → Expert → Kid; the race
// starts against CPUs and no touch layout shows until a finger is down. A
// held touch anywhere is GO: the race path driver rides the player along the
// course with no steering input. Released, the driver is off and the quad
// brakes fully. Space is GO too, and the toolbar's Pause still opens Pause.
// ATV_SHOTS=<dir> saves the setup and a held GO frame.
import assert from 'node:assert/strict';
import { assertNoErrors, launch, newPage, nextFrames, unlockStore, url } from './browser/harness.mjs';

const read = page => page.evaluate(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}'));
const front = async page => (await read(page)).front;
const onPage = (page, name, timeout = 60000) =>
  page.waitForFunction(name => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').front?.page === name, name, { timeout });
async function key(page, code) { await page.keyboard.down(code); await nextFrames(page, 2); await page.keyboard.up(code); await nextFrames(page, 3); }
async function pick(page, action) {
  for (let i = 0; i < 12 && (await front(page)).choices[(await front(page)).selection] !== action; i++) await key(page, 'ArrowDown');
  const f = await front(page);
  assert.equal(f.choices[f.selection], action);
}
const label = (f, action) => f.labels[f.choices.indexOf(action)];
async function until(page, predicate, timeout, what) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const s = await read(page);
    if (predicate(s)) return s;
    if (Date.now() > deadline) throw new Error(`timed out: ${what} ${JSON.stringify(s.ride?.raceLap ?? null)}`);
    await page.waitForTimeout(250); // Time-based on purpose: the poll interval.
  }
}
const lap = s => s.ride?.raceLap ?? {};
const shots = process.env.ATV_SHOTS;

const browser = await launch();
try {
  const errors = [];
  const page = await newPage(browser, { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true }, errors);
  await unlockStore(page);
  await page.addInitScript(() => {
    Object.defineProperty(document, 'hasFocus', { value: () => true });
    if (sessionStorage.getItem('seeded')) return;
    sessionStorage.setItem('seeded', '1');
    localStorage.setItem('atv2-keyboard-help-seen-v1', '1');
    localStorage.removeItem('atv-player-selection');
  });
  await page.goto(url('/play.html?uiDiagnostics=1&weather=off'));
  await onPage(page, 'title');
  await key(page, 'Enter'); await onPage(page, 'main');
  await key(page, 'Enter'); await onPage(page, 'single');
  await pick(page, 'race'); await key(page, 'Enter'); await onPage(page, 'race');
  // Supercross: a short course.
  await key(page, 'PageUp');
  let f = await front(page);
  assert.ok(f.race.course.startsWith('sx'), f.race.course);
  await pick(page, 'racedifficulty');
  assert.equal(label(f, 'racedifficulty'), 'Difficulty: Normal');
  await key(page, 'ArrowRight');
  assert.equal(label(await front(page), 'racedifficulty'), 'Difficulty: Expert');
  await key(page, 'ArrowRight');
  f = await front(page);
  assert.equal(label(f, 'racedifficulty'), 'Difficulty: Kid');
  assert.equal(f.race.difficulty, 3);
  if (shots) await page.screenshot({ path: `${shots}/kid-setup.png` });
  await pick(page, 'racestart'); await key(page, 'Enter');
  let s = await until(page, s => lap(s).phase === 'racing', 240000, 'racing');
  assert.equal(lap(s).kid, true);
  assert.ok(lap(s).field > 1, `CPU field: ${lap(s).field}`);
  assert.equal(s.toolbar.kidTouch, true, 'touches are GO');
  // No finger down: the quad holds on full brake, nothing owns a touch.
  await page.waitForTimeout(1500);
  s = await read(page);
  assert.equal(lap(s).kidGo, false);
  assert.equal(lap(s).autopilot, false);
  assert.deepEqual(lap(s).imposed, [0, 1, 0]);
  assert.ok(s.speed < 3, `speed without GO ${s.speed}`);
  assert.equal(s.toolbar.touchOwned, false, 'no D-pad/compass touch');
  // A held touch (left half: the two-finger layout would lean there) is GO.
  const cdp = await page.context().newCDPSession(page);
  const touch = (type, points = []) => cdp.send('Input.dispatchTouchEvent', {
    type, touchPoints: points.map(p => ({ radiusX: 6, radiusY: 6, force: 1, ...p }))
  });
  const finger = { id: 1, x: 90, y: 640 };
  const start = lap(s).distance[0];
  await touch('touchStart', [finger]);
  s = await until(page, s => lap(s).kidGo && lap(s).autopilot, 5000, 'GO held');
  assert.equal(s.toolbar.kidGo, true);
  assert.equal(s.toolbar.touchOwned, false, 'GO is not the riding compass');
  s = await until(page, s => lap(s).distance[0] > start + 60 && s.speed > 15, 30000, 'advance on GO');
  if (shots) await page.screenshot({ path: `${shots}/kid-go-held.png` });
  const held = lap(s).distance[0];
  // Released: the driver is off and the quad brakes to a stop.
  await touch('touchEnd', []);
  s = await until(page, s => !lap(s).kidGo, 5000, 'GO released');
  assert.deepEqual(lap(s).imposed, [0, 1, 0]);
  s = await until(page, s => s.speed < 3, 15000, 'brake to a stop');
  const stopped = lap(s).distance[0];
  await page.waitForTimeout(1000);
  s = await read(page);
  assert.ok(Math.abs(lap(s).distance[0] - stopped) < 3, `held still: ${stopped} -> ${lap(s).distance[0]}`);
  // Space is GO.
  await page.keyboard.down('Space');
  s = await until(page, s => lap(s).kidGo && lap(s).distance[0] > stopped + 20, 20000, 'Space GO');
  await page.keyboard.up('Space');
  // The toolbar's Pause stays reachable: a tap there pauses, not GO.
  const pause = s.toolbar.hits.find(h => h.id === 1);
  assert.ok(pause, 'Pause button');
  const at = { id: 2, x: pause.x + pause.width / 2, y: pause.y + pause.height / 2 };
  await touch('touchStart', [at]);
  await nextFrames(page, 3);
  assert.equal((await read(page)).toolbar.kidGo, false, 'Pause is not GO');
  await touch('touchEnd', []);
  await page.waitForFunction(() => JSON.parse(document.querySelector('#ui-state')?.textContent || '{}').toolbar?.menu?.page === 'pause', null, { timeout: 10000 });
  assertNoErrors(errors);
  console.log(`PASS: Kid on ${f.race.course}: GO rode ${(held - start).toFixed(0)} ft with no steering input, released braked to a stop, Space GO, Pause reachable, ${lap(s).field - 1} CPUs`);
} finally {
  await browser.close();
}
